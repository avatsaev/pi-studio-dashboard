//! Ingest reducers (L2): pure, total folds of `DaemonEvent`s into `MonitorState`, plus the
//! view derivations. No I/O, no clock reads (`now` is an argument), wasm-clean.
//!
//! This crate currently reduces the connection-level events (`ConnState`, `Hello`, `Health`,
//! `Snapshot`, `Rtt`, `Diagnostics`); agent, dialog, stats and drill-down reducers arrive with
//! their event variants in later sprints.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod activity;
mod apply;
pub mod derive;
pub mod drill;
mod identity;
mod keys;
mod rebuild;
mod reduce_agent;
mod reduce_stats;
pub mod workspace;

pub use apply::{apply, apply_batch, apply_in_place};
pub use derive::{Filters, UsageTotals, card_order, daemon_status_counts, usage_totals};
pub use drill::{DrillError, deselect as drill_deselect, select as drill_select};
pub use reduce_stats::{needs_stats_tick, stats_tick};
pub use workspace::{NO_WORKSPACE, WorkspaceGroup, grouped_order, label_of, workspace_labels, workspace_of};
