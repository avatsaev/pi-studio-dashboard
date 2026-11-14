//! The feed pump: batches of daemon events from the client runtime become one `feed.ingest`
//! request each on the owner thread's channel. The owner never waits on the client, and a batch
//! the engine rejects is logged (counts only) and dropped: the pump keeps running.

use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use pistudiodashboard_client::DaemonEvent;
use pistudiodashboard_shell::{Envelope, Request};
use serde_json::{Value, json};

const REPLY_DEADLINE: Duration = Duration::from_secs(60);

/// How often the engine is asked whether usage needs refreshing (D16: every 10 s; the engine
/// decides what, if anything, to fetch: the client never schedules).
const TICK_EVERY: Duration = Duration::from_secs(10);

/// Call `feed.tick {now}` every [`TICK_EVERY`] until the owner thread is gone.
pub fn spawn_ticker(requests: Sender<Envelope>) -> Option<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("pistudiodashboard-tick".into())
        .spawn(move || {
            loop {
                std::thread::sleep(TICK_EVERY);
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
                let now = u64::try_from(now).unwrap_or(0);
                let (envelope, reply) = Envelope::new(Request::Internal { command: "feed.tick".into(), params: json!({ "now": now }) });
                if requests.send(envelope).is_err() {
                    break;
                }
                // The answer carries nothing the ticker needs; waiting keeps ticks from piling up.
                let _ = reply.recv_timeout(REPLY_DEADLINE);
            }
        })
        .ok()
}

/// Forward every batch from `rx` until the client side closes it or the owner is gone.
pub fn spawn(mut rx: tokio::sync::mpsc::Receiver<Vec<DaemonEvent>>, requests: Sender<Envelope>) -> Option<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("pistudiodashboard-feed".into())
        .spawn(move || {
            while let Some(batch) = rx.blocking_recv() {
                if !forward(&requests, &batch) {
                    break;
                }
            }
        })
        .ok()
}

/// `false` when the owner thread has gone and the pump should stop.
fn forward(requests: &Sender<Envelope>, batch: &[DaemonEvent]) -> bool {
    let Ok(events) = serde_json::to_value(batch) else {
        log::warn!("a batch of {} daemon events could not be serialised", batch.len());
        return true;
    };
    let (envelope, reply) = Envelope::new(Request::Internal { command: "feed.ingest".into(), params: json!({ "events": events }) });
    if requests.send(envelope).is_err() {
        return false;
    }
    match reply.recv_timeout(REPLY_DEADLINE) {
        Ok(answer) if answer.get("ok") == Some(&Value::Bool(true)) => {}
        Ok(_) => log::warn!("a batch of {} daemon events was rejected", batch.len()),
        Err(_) => log::warn!("a batch of {} daemon events got no answer", batch.len()),
    }
    true
}
