//! The owner thread: builds the shell around the session, answers requests one at a time and
//! emits the `view` through the coalescer.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use pistudiodashboard_client::ClientHandle;
use pistudiodashboard_engine::{EngineServices, Session, redact_request};
use pistudiodashboard_shell::{AppShell, Envelope, LocalDaemon, Request, control};
use serde_json::Value;

use crate::io_worker::IoWorker;
use crate::view_emitter::{ViewEmitter, WINDOW_MS};
use crate::{detect_local, runtime, services, startup};

/// Everything the owner thread needs besides its inbox.
pub struct Wiring {
    pub client: Arc<ClientHandle>,
    pub worker: Arc<IoWorker>,
    /// `None`: nothing is persisted and no config is loaded (no config directory).
    pub config_dir: Option<PathBuf>,
}

/// Engine services with every hook the app provides.
fn engine_services(w: &Wiring) -> EngineServices {
    let mut engine = EngineServices::default();
    runtime::install_hooks(&mut engine, &w.client);
    engine.keychain = Some(Box::new(w.worker.keychain()));
    if let Some(dir) = &w.config_dir {
        engine.config_store = Some(Box::new(w.worker.config_store(dir.clone())));
    }
    engine.random = Some(Box::new(|| {
        let mut bytes = [0u8; 16];
        let _ = getrandom::fill(&mut bytes);
        bytes
    }));
    engine
}

/// The shell with its session, after the startup sequence.
pub fn build(w: &Wiring) -> AppShell {
    build_with(w, &detect_local::detect_from_environment())
}

/// Like [`build`] with the local daemon given instead of read from the environment.
pub fn build_with(w: &Wiring, detected: &LocalDaemon) -> AppShell {
    let session = Session::new().with_services(engine_services(w));
    let mut app = AppShell::new(session, services::native());
    if let Some(dir) = &w.config_dir {
        startup::run(&mut app.session, dir, detected);
        app.adopt_settings();
    }
    app
}

/// Answer requests until every sender is gone, calling `sink` with each emitted view.
pub fn run(inbox: &Receiver<Envelope>, mut app: AppShell, mut sink: impl FnMut(&Value)) {
    let start = Instant::now();
    let clock = || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut emitter = ViewEmitter::new(WINDOW_MS);
    let mut shown = Value::Null;
    let mut emit = |app: &mut AppShell, emitter: &mut ViewEmitter, now: u64| {
        let view = control::view(app);
        if view == shown {
            emitter.unchanged();
        } else {
            sink(&view);
            shown = view;
            emitter.emitted(now);
        }
    };
    emit(&mut app, &mut emitter, clock());
    loop {
        let message = match emitter.deadline() {
            Some(due) => match inbox.recv_timeout(Duration::from_millis(due.saturating_sub(clock()))) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match inbox.recv() {
                Ok(m) => Some(m),
                Err(_) => break,
            },
        };
        match message {
            Some(envelope) => {
                log_request(&envelope.request);
                let reply = app.answer(&envelope.request);
                // The asker may have gone away; nothing to do about it.
                let _ = envelope.reply.send(reply);
                let now = clock();
                if emitter.on_request(now) {
                    emit(&mut app, &mut emitter, now);
                }
            }
            None => {
                let now = clock();
                if emitter.on_tick(now) {
                    emit(&mut app, &mut emitter, now);
                }
            }
        }
    }
    if emitter.deadline().is_some() {
        emit(&mut app, &mut emitter, clock());
    }
}

/// One debug line per request, with passwords redacted (never the feed's event contents).
fn log_request(request: &Request) {
    if !log::log_enabled!(log::Level::Debug) {
        return;
    }
    match request {
        Request::Control { method, params } => log::debug!("control {method} {}", redact_request(method, params)),
        Request::Invoke { command } => log::debug!("invoke {command}"),
        Request::Internal { command, .. } => log::trace!("internal {command}"),
    }
}
