//! Between the webview and the shell.
//!
//! One thread owns the [`AppShell`](pistudiodashboard_shell::AppShell) (and so the session): see
//! `owner.rs`. The webview's two commands (`commands.rs`) and the control socket only forward
//! requests to it over a channel, so a click and an automation request can never race.

use std::sync::mpsc::Sender;
use std::time::Duration;

use pistudiodashboard_shell::{Envelope, Request};
use serde_json::Value;

/// The event that carries the view (what `ui.inspect` returns) to the webview.
pub const VIEW_EVENT: &str = "view";
const REPLY_DEADLINE: Duration = Duration::from_secs(60);

/// The webview's way to the owner thread. Managed by Tauri.
pub struct Bridge {
    requests: Sender<Envelope>,
}

impl Bridge {
    pub fn new(requests: Sender<Envelope>) -> Self {
        Self { requests }
    }

    pub async fn ask(&self, request: Request) -> Result<Value, String> {
        let (envelope, reply) = Envelope::new(request);
        self.requests.send(envelope).map_err(|_| "the app is shutting down".to_string())?;
        // Waiting blocks, so it happens off the async runtime's worker threads.
        let waited = tauri::async_runtime::spawn_blocking(move || reply.recv_timeout(REPLY_DEADLINE)).await;
        match waited {
            Ok(Ok(reply)) => Ok(reply),
            Ok(Err(_)) => Err("timeout: no reply in 60s".to_string()),
            Err(e) => Err(format!("internal error: {e}")),
        }
    }
}
