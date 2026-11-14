//! Stamps events with entry id, server id, connection epoch and per-attempt `seq`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use pistudiodashboard_model::{DaemonEvent, DaemonEventKind};
use tokio::sync::mpsc;

/// Epoch milliseconds now.
pub(crate) fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

pub(crate) struct Emitter {
    entry_id: String,
    server_id: Option<String>,
    epochs: Arc<AtomicU64>,
    epoch: u64,
    seq: u64,
    tx: mpsc::Sender<DaemonEvent>,
}

impl Emitter {
    pub(crate) fn new(entry_id: String, epochs: Arc<AtomicU64>, tx: mpsc::Sender<DaemonEvent>) -> Self {
        let epoch = epochs.load(Ordering::SeqCst);
        Self { entry_id, server_id: None, epochs, epoch, seq: 0, tx }
    }

    /// A new connection attempt: `connEpoch` + 1 (monotonic per entry, across restarts) and
    /// `seq` restarts.
    pub(crate) fn begin_attempt(&mut self) {
        self.epoch = self.epochs.fetch_add(1, Ordering::SeqCst) + 1;
        self.seq = 0;
    }

    pub(crate) fn entry_id(&self) -> &str {
        &self.entry_id
    }

    pub(crate) fn set_server_id(&mut self, id: String) {
        self.server_id = Some(id);
    }

    /// Queue one event without waiting: `false` when the queue is full (the caller sheds it).
    pub(crate) fn try_emit(&mut self, kind: DaemonEventKind) -> bool {
        let event =
            DaemonEvent { entry_id: self.entry_id.clone(), server_id: self.server_id.clone(), conn_epoch: self.epoch, seq: self.seq + 1, at: now_ms(), kind };
        let sent = self.tx.try_send(event).is_ok();
        if sent {
            self.seq += 1;
        }
        sent
    }

    /// Queue one event; waits while the consumer is behind (backpressure).
    pub(crate) async fn emit(&mut self, kind: DaemonEventKind) {
        self.seq += 1;
        let event =
            DaemonEvent { entry_id: self.entry_id.clone(), server_id: self.server_id.clone(), conn_epoch: self.epoch, seq: self.seq, at: now_ms(), kind };
        // A closed channel means the entry was removed or the client shut down.
        let _ = self.tx.send(event).await;
    }
}
