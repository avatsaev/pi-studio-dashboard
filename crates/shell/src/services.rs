//! Platform services, injected by each app binary. The shell asks for what it needs through
//! these hooks, so this crate has no dialogs, clipboard, tray or OS code of its own, and the
//! same shell runs in the desktop app and in headless tests. The tray and notification hooks
//! arrive with their sprints.

/// The local pi-studio daemon as found on this machine (`daemon.detectLocal`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDaemon {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub web_ui_url: Option<String>,
}

pub type DetectLocalDaemon = Box<dyn FnMut() -> LocalDaemon>;

/// Every hook is optional: where one is missing (tests, a platform without it), what needs it
/// is disabled instead of failing.
#[derive(Default)]
pub struct Services {
    /// Reads the local pi-studio config and environment (file I/O belongs to the app binary).
    pub detect_local_daemon: Option<DetectLocalDaemon>,
}
