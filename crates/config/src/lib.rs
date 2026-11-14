//! The dashboard config file (`config.json`): model, lenient parsing, validation, stable
//! serialisation, and (native only) load-with-recovery and atomic save.
//!
//! Everything except [`load`] / [`save`] is pure and builds for `wasm32`. No password is ever a
//! field: a `password` key in a hand-edited file is dropped and reported.
//!
//! Design choices (spec leaves them open):
//! - Entry ids need randomness, which this crate does not own: [`parse`] and [`load`] take an id
//!   source `&mut dyn FnMut() -> String` (see [`new_entry_id`] to build one from 16 random bytes).
//! - [`save`] takes the `read_only` flag returned by [`load`] and refuses while it is set.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod model;
mod parse;
mod validate;

#[cfg(not(target_arch = "wasm32"))]
mod io;
#[cfg(not(target_arch = "wasm32"))]
mod location;

pub use model::{
    Config, ConfigIssue, DEFAULT_PORT, DaemonEntry, GroupBy, IssueKind, MAX_DAEMONS, MAX_FILE_BYTES, Notifications, SUPPORTED_VERSION, Settings, ThemeChoice,
    ZOOM_DEFAULT, ZOOM_MAX, ZOOM_MIN, new_entry_id,
};
pub use parse::{ParseOutcome, parse, serialize};
pub use validate::{FieldError, normalise_host, validate_entry};

#[cfg(not(target_arch = "wasm32"))]
pub use io::{BACKUP_FILE, CONFIG_FILE, ConfigError, LoadOutcome, TEMP_FILE, load, save};

#[cfg(not(target_arch = "wasm32"))]
pub use location::{APP_IDENTIFIER, config_dir};
