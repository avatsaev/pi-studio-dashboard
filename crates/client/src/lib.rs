//! The daemon connection layer (L4): one connection task per configured daemon entry, the
//! connection state machine, backoff, the health probe and batched [`DaemonEvent`] emission.
//!
//! This is the only crate that opens sockets. Events cross to the rest of the app through the
//! bounded channel returned by [`ClientHandle::start`]; nothing here knows about the engine.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod agents;
pub mod backoff;
mod batch;
mod conn;
mod counters;
mod emitter;
mod handle;
mod probe;
mod requests;
mod rtt;
mod secret;
mod state;
mod sync;
mod tls;

pub use handle::{ClientConfig, ClientHandle};
pub use pistudiodashboard_model::{DaemonEvent, DaemonEventKind, MutationOp, MutationResult, StatsResult, StreamEvent};
pub use pistudiodashboard_protocol::{OutboundRequest, UiResponse};
pub use requests::SendError;
pub use secret::{InvalidPassword, Secret};
pub use state::{ConnError, ConnErrorKind};
