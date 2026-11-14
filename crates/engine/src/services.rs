//! Hooks the app binary injects. All are optional, called on the owner thread and must not
//! block. Only the types and plumbing live here; the commands that call them arrive later.

use pistudiodashboard_config::{Config, ConfigIssue, DaemonEntry};
use pistudiodashboard_protocol::OutboundRequest;

/// Why a request could not be handed to a daemon connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SendError {
    #[error("not connected")]
    NotConnected,
    #[error("the send queue is full")]
    QueueFull,
    #[error("not allowed")]
    NotAllowed,
}

/// Lifecycle commands for one daemon connection. `password` is the one looked up by the engine
/// (memory, then keychain) and never printed.
#[derive(Clone, Debug)]
pub enum LinkCommand {
    Connect { entry: DaemonEntry, password: Option<Secret> },
    Disconnect,
    Restart { entry: DaemonEntry, password: Option<Secret> },
    Remove,
}

/// A password that never prints (`Debug` and `Display` write `***`) and is overwritten on drop.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(Vec<u8>);

impl Secret {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().into_bytes())
    }

    /// The plain text, for the one place that hands it to the keychain or the client.
    #[must_use]
    pub fn expose(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("")
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

impl std::fmt::Display for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum KeychainError {
    #[error("the OS keychain is unavailable")]
    Unavailable,
    #[error("access to the OS keychain was denied")]
    Denied,
    #[error("no secret stored")]
    NotFound,
}

pub trait Keychain {
    fn get(&mut self, entry_id: &str) -> Result<Secret, KeychainError>;
    fn set(&mut self, entry_id: &str, secret: &Secret) -> Result<(), KeychainError>;
    fn delete(&mut self, entry_id: &str) -> Result<(), KeychainError>;
}

/// What `ConfigStore::load` returns (the `config` crate's own `LoadOutcome` is not available on
/// wasm32, and the engine must build there).
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedConfig {
    pub config: Config,
    pub issues: Vec<ConfigIssue>,
    /// Never save: the file is from a newer version or could not be read.
    pub read_only: bool,
}

pub trait ConfigStore {
    fn load(&mut self) -> LoadedConfig;
    fn save(&mut self, config: &Config) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyAction {
    Raise,
    Resolve,
}

/// Which item a notification is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotifyTarget {
    pub server_id: String,
    pub item_id: String,
}

/// Fixed-vocabulary notification: never dialog text, commands or paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub action: NotifyAction,
    pub title: String,
    pub body: String,
    pub target: NotifyTarget,
}

pub type SendToDaemon = Box<dyn FnMut(&str, OutboundRequest) -> Result<(), SendError>>;
pub type DaemonLink = Box<dyn FnMut(&str, LinkCommand) -> Result<(), String>>;
pub type OpenUrl = Box<dyn FnMut(&str) -> Result<(), String>>;
pub type NotifyHook = Box<dyn FnMut(Notification)>;
pub type RandomBytes = Box<dyn FnMut() -> [u8; 16]>;

/// Every hook is optional: a missing one disables what needs it instead of failing.
#[derive(Default)]
pub struct EngineServices {
    pub send_to_daemon: Option<SendToDaemon>,
    pub daemon_link: Option<DaemonLink>,
    pub keychain: Option<Box<dyn Keychain>>,
    pub config_store: Option<Box<dyn ConfigStore>>,
    pub open_url: Option<OpenUrl>,
    pub notify: Option<NotifyHook>,
    /// Random bytes for new entry ids (`getrandom` in the app). Without it ids come from a
    /// deterministic counter: fine for tests and the offline CLI, unique within a session only.
    pub random: Option<RandomBytes>,
}

/// The hooks, for [`EngineServices::require`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    SendToDaemon,
    DaemonLink,
    Keychain,
    ConfigStore,
    OpenUrl,
    Notify,
}

impl EngineServices {
    /// `Err(reason)` when `hook` is not configured: the text a disabled menu item shows.
    ///
    /// # Errors
    /// The "disabled" reason for the missing hook.
    pub fn require(&self, hook: Hook) -> Result<(), String> {
        let (present, reason) = match hook {
            Hook::SendToDaemon => (self.send_to_daemon.is_some(), "no live connection in this build"),
            Hook::DaemonLink => (self.daemon_link.is_some(), "no live connection in the CLI"),
            Hook::Keychain => (self.keychain.is_some(), "no keychain in this build"),
            Hook::ConfigStore => (self.config_store.is_some(), "no settings storage in this build"),
            Hook::OpenUrl => (self.open_url.is_some(), "cannot open links in this build"),
            Hook::Notify => (self.notify.is_some(), "notifications are not available in this build"),
        };
        if present { Ok(()) } else { Err(reason.to_owned()) }
    }
}
