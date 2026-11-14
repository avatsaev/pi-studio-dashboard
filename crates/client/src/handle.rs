//! [`ClientHandle`]: owns the runtime and one connection task per entry.

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use pistudiodashboard_config::DaemonEntry;
use pistudiodashboard_model::DaemonEvent;
use pistudiodashboard_protocol::OutboundRequest;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::backoff::RESET_AFTER_LIVE;
use crate::batch::{Gate, run_batcher};
use crate::conn::{Ctl, Target, Task};
use crate::emitter::Emitter;
use crate::requests::{SendError, Shared};
use crate::rtt::PING_EVERY;
use crate::secret::{InvalidPassword, Secret};

/// Events one entry may have queued towards its batcher before the connection task stalls.
const ENTRY_QUEUE: usize = 1024;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Bare semver; sent as `appVersion: "pistudiodashboard/<app_version>"`.
    pub app_version: String,
    /// Capacity of the batch channel returned by [`ClientHandle::start`] (default 1024).
    pub event_channel_capacity: usize,
    /// Liveness ping period while live (default 15 s; two unanswered pings end the connection).
    pub ping_every: Duration,
    /// How long a connection must stay live before the backoff attempt counter resets (default 10 s).
    pub reset_after_live: Duration,
}

impl ClientConfig {
    #[must_use]
    pub fn new(app_version: impl Into<String>) -> Self {
        Self { app_version: app_version.into(), event_channel_capacity: 1024, ping_every: PING_EVERY, reset_after_live: RESET_AFTER_LIVE }
    }
}

struct Slot {
    ctl: mpsc::UnboundedSender<Ctl>,
    gate: Arc<Gate>,
    shared: Arc<Shared>,
    tasks: [JoinHandle<()>; 2],
}

struct Inner {
    rt: Option<Handle>,
    app_version: String,
    ping_every: Duration,
    reset_after_live: Duration,
    out: mpsc::Sender<Vec<DaemonEvent>>,
    slots: Mutex<HashMap<String, Slot>>,
    /// Per-entry `connEpoch` counters; kept after `remove` so a re-added entry never reuses an epoch.
    epochs: Mutex<HashMap<String, Arc<AtomicU64>>>,
}

fn validate(password: Option<&Secret>) -> Result<(), InvalidPassword> {
    password.map_or(Ok(()), Secret::validate)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Control surface of the connection layer. Dropping it (or [`shutdown`](Self::shutdown))
/// stops every task; no event is emitted afterwards.
pub struct ClientHandle {
    inner: Arc<Inner>,
    /// Present when this handle owns its runtime ([`start`](Self::start)).
    runtime: Option<Runtime>,
}

impl ClientHandle {
    /// Start on a private 2-worker multi-thread runtime. If the OS refuses to create it, the
    /// error is logged and the handle is inert: every command is a no-op and the receiver is closed.
    #[must_use]
    pub fn start(config: ClientConfig) -> (Self, mpsc::Receiver<Vec<DaemonEvent>>) {
        match Self::try_start(config.clone()) {
            Ok(pair) => pair,
            Err(e) => {
                log::error!("client runtime failed to start: {e}");
                let (tx, rx) = mpsc::channel(1);
                (Self::with_runtime(None, None, config, tx), rx)
            }
        }
    }

    /// Like [`start`](Self::start) but reports a runtime start failure.
    ///
    /// # Errors
    /// The OS error from building the tokio runtime.
    pub fn try_start(config: ClientConfig) -> std::io::Result<(Self, mpsc::Receiver<Vec<DaemonEvent>>)> {
        let rt = Builder::new_multi_thread().worker_threads(2).thread_name("pistudiodashboard-client").enable_all().build()?;
        let (tx, rx) = mpsc::channel(config.event_channel_capacity.max(1));
        Ok((Self::with_runtime(Some(rt.handle().clone()), Some(rt), config, tx), rx))
    }

    /// Start on a runtime the caller owns (tests with simulated time, embedding).
    #[must_use]
    pub fn start_in(rt: Handle, config: ClientConfig) -> (Self, mpsc::Receiver<Vec<DaemonEvent>>) {
        let (tx, rx) = mpsc::channel(config.event_channel_capacity.max(1));
        (Self::with_runtime(Some(rt), None, config, tx), rx)
    }

    fn with_runtime(rt: Option<Handle>, owned: Option<Runtime>, config: ClientConfig, out: mpsc::Sender<Vec<DaemonEvent>>) -> Self {
        let inner = Arc::new(Inner {
            rt,
            app_version: config.app_version,
            ping_every: config.ping_every,
            reset_after_live: config.reset_after_live,
            out,
            slots: Mutex::new(HashMap::new()),
            epochs: Mutex::new(HashMap::new()),
        });
        Self { inner, runtime: owned }
    }

    /// Start connecting `entry`. Idempotent while connecting or live.
    ///
    /// # Errors
    /// [`InvalidPassword`] if the password cannot be a subprotocol token; nothing is started or sent.
    pub fn connect(&self, entry: DaemonEntry, password: Option<Secret>) -> Result<(), InvalidPassword> {
        validate(password.as_ref())?;
        self.send(&entry.entry_id.clone(), Ctl::Connect(Target { entry, password }));
        Ok(())
    }

    /// Send `request` to a daemon, addressed by entry id or by `serverId`. Never blocks.
    ///
    /// Only a `live` connection accepts requests; otherwise this fails at once and nothing is
    /// queued, so a mutation is never replayed after a reconnect (D27). `token` is an opaque
    /// caller key echoed in the `RequestResult` event of a mutation. Reads get no result event.
    ///
    /// # Errors
    /// [`SendError::NotConnected`], [`SendError::Busy`] (256 reads outstanding) or
    /// [`SendError::QueueFull`].
    pub fn request(&self, daemon: &str, request: OutboundRequest, token: impl Into<String>) -> Result<(), SendError> {
        let shared = {
            let slots = lock(&self.inner.slots);
            slots.get(daemon).or_else(|| slots.values().find(|s| s.shared.server_id().as_deref() == Some(daemon))).map(|s| s.shared.clone())
        };
        shared.ok_or(SendError::NotConnected)?.try_enqueue(request, token.into())
    }

    /// Close with code 1000 and go `idle`; no timer fires afterwards.
    pub fn disconnect(&self, entry_id: &str) {
        if let Some(slot) = lock(&self.inner.slots).get(entry_id) {
            let _ = slot.ctl.send(Ctl::Disconnect);
        }
    }

    /// Stop the entry's task for good. When this returns, no further event of the entry is emitted.
    pub fn remove(&self, entry_id: &str) {
        let slot = lock(&self.inner.slots).remove(entry_id);
        if let Some(slot) = slot {
            slot.gate.close();
            for t in &slot.tasks {
                t.abort();
            }
        }
    }

    /// Tear down and restart at `probing` (entry edit or new password).
    ///
    /// # Errors
    /// [`InvalidPassword`] as for [`connect`](Self::connect); the running connection is left alone.
    pub fn restart(&self, entry_id: &str, entry: DaemonEntry, password: Option<Secret>) -> Result<(), InvalidPassword> {
        validate(password.as_ref())?;
        self.send(entry_id, Ctl::Restart(Target { entry, password }));
        Ok(())
    }

    /// Stop everything; equivalent to dropping the handle.
    pub fn shutdown(self) {}

    fn send(&self, entry_id: &str, msg: Ctl) {
        let Some(rt) = &self.inner.rt else { return };
        let mut slots = lock(&self.inner.slots);
        let slot = slots.entry(entry_id.to_owned()).or_insert_with(|| self.spawn_slot(rt, entry_id));
        let _ = slot.ctl.send(msg);
    }

    fn spawn_slot(&self, rt: &Handle, entry_id: &str) -> Slot {
        let epochs = lock(&self.inner.epochs).entry(entry_id.to_owned()).or_default().clone();
        let (ctl_tx, ctl_rx) = mpsc::unbounded_channel();
        let (ev_tx, ev_rx) = mpsc::channel(ENTRY_QUEUE);
        let gate = Arc::new(Gate::default());
        let batcher = rt.spawn(run_batcher(ev_rx, self.inner.out.clone(), gate.clone()));
        let shared = Arc::new(Shared::default());
        let task = Task::new(
            ctl_rx,
            Emitter::new(entry_id.to_owned(), epochs, ev_tx),
            self.inner.app_version.clone(),
            self.inner.ping_every,
            self.inner.reset_after_live,
            shared.clone(),
        );
        let conn = rt.spawn(task.run());
        Slot { ctl: ctl_tx, gate, shared, tasks: [conn, batcher] }
    }
}

impl Drop for ClientHandle {
    fn drop(&mut self) {
        let slots = std::mem::take(&mut *lock(&self.inner.slots));
        for (_, slot) in slots {
            slot.gate.close();
            for t in &slot.tasks {
                t.abort();
            }
        }
        if let Some(rt) = self.runtime.take() {
            rt.shutdown_background();
        }
    }
}
