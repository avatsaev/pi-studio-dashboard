//! The engine: a [`Session`] that owns the live [`MonitorState`](pistudiodashboard_model::MonitorState),
//! and the command registry.
//!
//! Every user-visible action is a command with a stable id. The window, the CLI, the control
//! channel and tests all call [`Session::execute`] with an id and JSON params, so anything a
//! person can do an agent or a script can do too, and nothing here knows about a UI toolkit.
//! Live data enters through the internal `feed.ingest` command (see `feed_cmds`).
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod agent_cmds;
pub mod commands;
mod daemon_cmds;
mod daemon_view;
mod error;
mod feed_cmds;
pub mod params;
mod query_cmds;
mod services;
mod session;
mod tick_cmds;
mod view;

pub use commands::{CommandSpec, command_specs, find};
pub use daemon_cmds::{MAX_PASSWORD_BYTES, PasswordPrompt, redact_params, redact_request};
pub use error::{EngineError, Result};
pub use feed_cmds::Skipped;
pub use pistudiodashboard_config::{GroupBy, ZOOM_DEFAULT, ZOOM_MAX, ZOOM_MIN};
pub use services::{
    ConfigStore, EngineServices, Hook, Keychain, KeychainError, LinkCommand, LoadedConfig, Notification, NotifyAction, NotifyTarget, Secret, SendError,
};
pub use session::{Session, Toast};
