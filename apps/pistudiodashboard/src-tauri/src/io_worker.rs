//! The single I/O worker: config-file writes (debounced) and keychain calls, off the owner
//! thread. Results that matter come back as engine commands on the request channel.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pistudiodashboard_config::{Config, load, save};
use pistudiodashboard_engine::{ConfigStore, Keychain, KeychainError, LoadedConfig, Secret};
use pistudiodashboard_shell::{Envelope, Request};
use serde_json::json;

use crate::keychain::{Store, account};

/// A save waits this long for further changes before it is written.
const DEBOUNCE: Duration = Duration::from_millis(250);
/// ... but never longer than this under a steady stream of changes.
const MAX_DELAY: Duration = Duration::from_secs(2);
/// One keychain call may take this long before it counts as unavailable.
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(3);

enum KeychainOp {
    Get(String),
    Set(String, String),
    Delete(String),
}

type KeychainReply = Result<Option<String>, KeychainError>;

enum Job {
    Save(Box<Config>),
    Keychain(KeychainOp, Sender<KeychainReply>),
    Shutdown,
}

/// Handle to the worker thread; clones of its senders live in the engine hooks.
pub struct IoWorker {
    tx: Sender<Job>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl IoWorker {
    /// `dir` is the config directory (`None`: nothing is saved). Failed saves are reported on
    /// `requests` as the internal `config.saveFailed` command.
    pub fn start(dir: Option<PathBuf>, requests: Sender<Envelope>, store: Arc<dyn Store>) -> Self {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::Builder::new().name("pistudiodashboard-io".into()).spawn(move || run(&rx, dir.as_deref(), &requests, &store)).ok();
        Self { tx, thread: Mutex::new(thread) }
    }

    /// The engine's `config_store` hook.
    pub fn config_store(&self, dir: PathBuf) -> impl ConfigStore + use<> {
        WorkerConfigStore { tx: self.tx.clone(), dir }
    }

    /// The engine's `keychain` hook.
    pub fn keychain(&self) -> impl Keychain + use<> {
        WorkerKeychain { tx: self.tx.clone() }
    }

    /// Flush a pending save and stop the thread.
    pub fn shutdown(&self) {
        let _ = self.tx.send(Job::Shutdown);
        let handle = self.thread.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

struct WorkerConfigStore {
    tx: Sender<Job>,
    dir: PathBuf,
}

impl ConfigStore for WorkerConfigStore {
    fn load(&mut self) -> LoadedConfig {
        let mut ids = crate::startup::id_source();
        let out = load(&self.dir, &mut ids);
        LoadedConfig { config: out.config, issues: out.issues, read_only: out.read_only }
    }

    fn save(&mut self, config: &Config) -> Result<(), String> {
        self.tx.send(Job::Save(Box::new(config.clone()))).map_err(|_| "the settings writer has stopped".to_owned())
    }
}

struct WorkerKeychain {
    tx: Sender<Job>,
}

impl WorkerKeychain {
    fn call(&self, op: KeychainOp) -> KeychainReply {
        let (reply, rx) = mpsc::channel();
        self.tx.send(Job::Keychain(op, reply)).map_err(|_| KeychainError::Unavailable)?;
        rx.recv_timeout(KEYCHAIN_TIMEOUT + Duration::from_secs(1)).map_err(|_| KeychainError::Unavailable)?
    }
}

impl Keychain for WorkerKeychain {
    fn get(&mut self, entry_id: &str) -> Result<Secret, KeychainError> {
        self.call(KeychainOp::Get(account(entry_id)))?.map(Secret::new).ok_or(KeychainError::NotFound)
    }

    fn set(&mut self, entry_id: &str, secret: &Secret) -> Result<(), KeychainError> {
        self.call(KeychainOp::Set(account(entry_id), secret.expose().to_owned())).map(|_| ())
    }

    fn delete(&mut self, entry_id: &str) -> Result<(), KeychainError> {
        self.call(KeychainOp::Delete(account(entry_id))).map(|_| ())
    }
}

fn run(rx: &Receiver<Job>, dir: Option<&std::path::Path>, requests: &Sender<Envelope>, store: &Arc<dyn Store>) {
    let mut pending: Option<(Box<Config>, Instant)> = None;
    loop {
        let job = match &pending {
            Some((_, since)) => rx.recv_timeout(DEBOUNCE.min(MAX_DELAY.saturating_sub(since.elapsed()))),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match job {
            Ok(Job::Save(config)) => {
                let since = pending.take().map_or_else(Instant::now, |(_, since)| since);
                pending = Some((config, since));
            }
            Ok(Job::Keychain(op, reply)) => {
                let _ = reply.send(keychain_call(store, op));
            }
            Ok(Job::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                flush(&mut pending, dir, requests);
                return;
            }
            Err(RecvTimeoutError::Timeout) => flush(&mut pending, dir, requests),
        }
    }
}

fn flush(pending: &mut Option<(Box<Config>, Instant)>, dir: Option<&std::path::Path>, requests: &Sender<Envelope>) {
    let (Some((config, _)), Some(dir)) = (pending.take(), dir) else { return };
    if let Err(e) = save(dir, &config, false) {
        log::warn!("settings could not be saved: {e}");
        let (envelope, _reply) = Envelope::new(Request::Internal { command: "config.saveFailed".into(), params: json!({"error": e.to_string()}) });
        let _ = requests.send(envelope);
    }
}

/// Run one keychain call on a helper thread so a wedged backend cannot wedge the worker.
fn keychain_call(store: &Arc<dyn Store>, op: KeychainOp) -> KeychainReply {
    let (tx, rx) = mpsc::channel();
    let store = Arc::clone(store);
    let spawned = std::thread::Builder::new().name("pistudiodashboard-keychain".into()).spawn(move || {
        let reply = match op {
            KeychainOp::Get(a) => store.get(&a),
            KeychainOp::Set(a, p) => store.set(&a, &p).map(|()| None),
            KeychainOp::Delete(a) => store.delete(&a).map(|()| None),
        };
        let _ = tx.send(reply);
    });
    if spawned.is_err() {
        return Err(KeychainError::Unavailable);
    }
    rx.recv_timeout(KEYCHAIN_TIMEOUT).unwrap_or(Err(KeychainError::Unavailable))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct Mem(Mutex<HashMap<String, String>>);

    impl Store for Mem {
        fn get(&self, a: &str) -> Result<Option<String>, KeychainError> {
            Ok(self.0.lock().unwrap().get(a).cloned())
        }
        fn set(&self, a: &str, p: &str) -> Result<(), KeychainError> {
            self.0.lock().unwrap().insert(a.into(), p.into());
            Ok(())
        }
        fn delete(&self, a: &str) -> Result<(), KeychainError> {
            self.0.lock().unwrap().remove(a);
            Ok(())
        }
    }

    struct Hang;

    impl Store for Hang {
        fn get(&self, _: &str) -> Result<Option<String>, KeychainError> {
            std::thread::sleep(Duration::from_secs(30));
            Ok(None)
        }
        fn set(&self, _: &str, _: &str) -> Result<(), KeychainError> {
            Err(KeychainError::Denied)
        }
        fn delete(&self, _: &str) -> Result<(), KeychainError> {
            Err(KeychainError::Unavailable)
        }
    }

    #[test]
    fn saves_are_debounced_and_the_last_one_wins() {
        let dir = tempfile::tempdir().unwrap();
        let (requests, _inbox) = mpsc::channel();
        let worker = IoWorker::start(Some(dir.path().to_owned()), requests, Arc::new(Mem::default()));
        let mut store = worker.config_store(dir.path().to_owned());
        for i in 0..5 {
            let mut c = Config::default();
            c.settings.close_to_tray = i % 2 == 0;
            c.local_detection_dismissed = i == 4;
            store.save(&c).unwrap();
        }
        assert!(!dir.path().join("config.json").exists(), "nothing written inside the debounce window");
        worker.shutdown();
        let text = std::fs::read_to_string(dir.path().join("config.json")).unwrap();
        assert!(text.contains("\"localDetectionDismissed\": true"), "{text}");
    }

    #[test]
    fn the_keychain_round_trips_on_the_worker() {
        let (requests, _inbox) = mpsc::channel();
        let mem = Arc::new(Mem::default());
        let worker = IoWorker::start(None, requests, mem.clone());
        let mut kc = worker.keychain();
        assert_eq!(kc.get("e1").unwrap_err(), KeychainError::NotFound);
        kc.set("e1", &Secret::new("pw")).unwrap();
        assert_eq!(mem.0.lock().unwrap().get("daemon:e1").map(String::as_str), Some("pw"));
        assert_eq!(kc.get("e1").unwrap().expose(), "pw");
        kc.delete("e1").unwrap();
        assert_eq!(kc.get("e1").unwrap_err(), KeychainError::NotFound);
        worker.shutdown();
    }

    #[test]
    fn a_failing_save_is_reported_as_a_command() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the directory should be: the save cannot create it.
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, b"x").unwrap();
        let (requests, inbox) = mpsc::channel();
        let worker = IoWorker::start(Some(blocked.join("cfg")), requests, Arc::new(Mem::default()));
        worker.config_store(blocked.join("cfg")).save(&Config::default()).unwrap();
        worker.shutdown();
        let envelope = inbox.try_recv().unwrap();
        assert!(matches!(&envelope.request, Request::Internal { command, .. } if command == "config.saveFailed"));
    }

    #[test]
    fn a_wedged_keychain_times_out_as_unavailable_and_does_not_block_the_worker() {
        let (requests, _inbox) = mpsc::channel();
        let worker = IoWorker::start(None, requests, Arc::new(Hang));
        let mut kc = worker.keychain();
        let started = Instant::now();
        assert_eq!(kc.get("e1").unwrap_err(), KeychainError::Unavailable);
        assert!(started.elapsed() < Duration::from_secs(8));
        assert_eq!(kc.set("e1", &Secret::new("x")).unwrap_err(), KeychainError::Denied, "the worker still answers");
    }
}
