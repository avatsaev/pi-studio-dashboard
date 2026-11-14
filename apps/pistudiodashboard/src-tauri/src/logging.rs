//! `log` records to standard error. Pi Studio Dashboard's own crates log at `info`, everything else
//! at `warn`; `PISTUDIODASHBOARD_LOG=debug` or `trace` raises Pi Studio Dashboard's level only.
//!
//! Third-party targets never go above `warn`, whatever the setting: `tungstenite` logs the whole
//! upgrade request, bearer subprotocol (the daemon password) included, at TRACE.

use log::{Level, LevelFilter, Log, Metadata, Record};

/// The most verbose level a record of `target` may have, given the level chosen for our own crates.
fn ceiling(target: &str, own: Level) -> Level {
    if target.starts_with("pistudiodashboard") { own } else { Level::Warn }
}

struct StderrLogger {
    own: Level,
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= ceiling(metadata.target(), self.own)
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!("{:<5} {}: {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

pub fn init() {
    let own = match std::env::var("PISTUDIODASHBOARD_LOG").as_deref() {
        Ok("trace") => Level::Trace,
        Ok("debug") => Level::Debug,
        Ok("warn") => Level::Warn,
        Ok("error") => Level::Error,
        _ => Level::Info,
    };
    // Fails only if a logger is already set; then that one stays.
    if log::set_boxed_logger(Box::new(StderrLogger { own })).is_ok() {
        log::set_max_level(LevelFilter::Trace);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(target: &str, level: Level, own: Level) -> bool {
        StderrLogger { own }.enabled(&Metadata::builder().target(target).level(level).build())
    }

    #[test]
    fn websocket_crates_never_log_below_warn_even_with_trace_requested() {
        for target in ["tungstenite", "tungstenite::handshake::client", "tokio_tungstenite", "tokio_tungstenite::connect", "rustls", "zbus"] {
            assert!(!allowed(target, Level::Trace, Level::Trace), "{target} at TRACE");
            assert!(!allowed(target, Level::Debug, Level::Trace), "{target} at DEBUG");
            assert!(!allowed(target, Level::Info, Level::Trace), "{target} at INFO");
            assert!(allowed(target, Level::Warn, Level::Info), "{target} at WARN");
        }
    }

    #[test]
    fn our_own_crates_follow_the_requested_level() {
        assert!(allowed("pistudiodashboard_client::conn", Level::Trace, Level::Trace));
        assert!(!allowed("pistudiodashboard_client::conn", Level::Debug, Level::Info));
        assert!(allowed("pistudiodashboard", Level::Info, Level::Info));
    }
}
