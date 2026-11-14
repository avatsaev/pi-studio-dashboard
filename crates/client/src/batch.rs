//! Per-entry batching: events flush as one `Vec` after 50 ms or 256 events, whichever first.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use pistudiodashboard_model::DaemonEvent;
use tokio::sync::mpsc;
use tokio::time::{Instant, timeout_at};

pub(crate) const FLUSH_AFTER: Duration = Duration::from_millis(50);
pub(crate) const MAX_BATCH: usize = 256;

/// Closed by `remove`: once closed, nothing of that entry reaches the consumer any more.
#[derive(Debug, Default)]
pub(crate) struct Gate(Mutex<bool>);

impl Gate {
    pub(crate) fn close(&self) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = true;
    }

    /// Send through `permit` unless closed. The lock makes `close` a barrier: a send either
    /// completed before `close` returned or never happens.
    fn send(&self, permit: mpsc::Permit<'_, Vec<DaemonEvent>>, batch: Vec<DaemonEvent>) -> bool {
        let closed = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if *closed {
            return false;
        }
        permit.send(batch);
        true
    }
}

/// Drain `rx` into batches on `out`. Ends when `rx` closes (after a final flush), `out` closes
/// or the gate closes. When `out` is full this blocks, `rx` fills, and the producer stalls.
pub(crate) async fn run_batcher(mut rx: mpsc::Receiver<DaemonEvent>, out: mpsc::Sender<Vec<DaemonEvent>>, gate: std::sync::Arc<Gate>) {
    let mut open = true;
    while open {
        let Some(first) = rx.recv().await else { return };
        let mut batch = Vec::with_capacity(MAX_BATCH.min(64));
        batch.push(first);
        let deadline = Instant::now() + FLUSH_AFTER;
        while batch.len() < MAX_BATCH {
            match timeout_at(deadline, rx.recv()).await {
                Ok(Some(event)) => batch.push(event),
                Ok(None) => {
                    open = false;
                    break;
                }
                Err(_) => break,
            }
        }
        let Ok(permit) = out.reserve().await else { return };
        if !gate.send(permit, batch) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pistudiodashboard_model::DaemonEventKind;

    use super::*;

    fn ev(seq: u64) -> DaemonEvent {
        DaemonEvent { entry_id: "e".into(), server_id: None, conn_epoch: 1, seq, at: 0, kind: DaemonEventKind::Rtt { ms: 1 } }
    }

    #[tokio::test(start_paused = true)]
    async fn thousand_events_leave_in_ordered_batches_of_at_most_256() {
        let (tx, rx) = mpsc::channel(2048);
        let (out_tx, mut out_rx) = mpsc::channel(1024);
        tokio::spawn(run_batcher(rx, out_tx, Arc::new(Gate::default())));
        for i in 0..1000 {
            tx.send(ev(i)).await.unwrap();
        }
        drop(tx);
        let mut seen = Vec::new();
        while let Some(batch) = out_rx.recv().await {
            assert!(!batch.is_empty() && batch.len() <= MAX_BATCH);
            seen.extend(batch.into_iter().map(|e| e.seq));
        }
        assert_eq!(seen, (0..1000).collect::<Vec<_>>());
    }

    #[tokio::test(start_paused = true)]
    async fn a_lone_event_flushes_after_50ms() {
        let (tx, rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::channel(8);
        tokio::spawn(run_batcher(rx, out_tx, Arc::new(Gate::default())));
        let start = Instant::now();
        tx.send(ev(1)).await.unwrap();
        let batch = out_rx.recv().await.unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(start.elapsed(), FLUSH_AFTER);
    }

    #[tokio::test(start_paused = true)]
    async fn a_full_consumer_channel_stalls_the_producer() {
        let (tx, rx) = mpsc::channel(4);
        let (out_tx, mut out_rx) = mpsc::channel(1);
        tokio::spawn(run_batcher(rx, out_tx, Arc::new(Gate::default())));
        // Nothing reads `out_rx`: only a bounded number of events can ever be accepted.
        let mut accepted = 0;
        for i in 0..100_000 {
            if tokio::time::timeout(Duration::from_secs(1), tx.send(ev(i))).await.is_err() {
                break;
            }
            accepted += 1;
        }
        assert!(accepted < 4 + 1 + MAX_BATCH * 2 + 8, "accepted {accepted}");
        assert!(out_rx.recv().await.is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn a_closed_gate_drops_everything() {
        let (tx, rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let gate = Arc::new(Gate::default());
        tokio::spawn(run_batcher(rx, out_tx, gate.clone()));
        tx.send(ev(1)).await.unwrap();
        gate.close();
        assert!(out_rx.recv().await.is_none());
    }
}
