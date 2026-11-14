//! Auth, handshake errors, RTT and diagnostics counters against the scripted fake daemon.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use pistudiodashboard_client::{ClientConfig, ClientHandle, DaemonEvent, DaemonEventKind, Secret};
use pistudiodashboard_model::{ConnState, CountersSnapshot};
use support::fake_daemon::{After, Behavior, FakeDaemon, Handshake, Outgoing};
use support::{collect_until, entry, is_state, state_of};
use tokio::time::Instant;

const BEARER: &str = "pi-studio.bearer.s3cret";

fn start() -> (ClientHandle, support::Events) {
    let (client, rx) = ClientHandle::start_in(tokio::runtime::Handle::current(), ClientConfig::new("0.1.0"));
    (client, support::Events::new(rx))
}

fn error_of(e: &DaemonEvent) -> Option<(String, String)> {
    match &e.kind {
        DaemonEventKind::ConnState { last_error: Some(err), .. } => Some((err.code.clone(), err.message.clone().unwrap_or_default())),
        _ => None,
    }
}

fn backoff_ms(e: &DaemonEvent) -> Option<u64> {
    match &e.kind {
        DaemonEventKind::ConnState { state: ConnState::Backoff, next_retry_in_ms, .. } => *next_retry_in_ms,
        _ => None,
    }
}

fn diagnostics(e: &DaemonEvent) -> Option<&CountersSnapshot> {
    match &e.kind {
        DaemonEventKind::Diagnostics(c) => Some(c),
        _ => None,
    }
}

#[tokio::test]
async fn bearer_subprotocol_is_offered_only_with_a_password() {
    let daemon = FakeDaemon::start(Behavior { require_subprotocol: Some(BEARER.to_owned()), ..Behavior::default() }).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), Some(Secret::new("s3cret"))).unwrap();
    collect_until(&mut rx, |e| is_state(e, ConnState::Live)).await;
    assert_eq!(daemon.subprotocols(), [BEARER]);
    assert_eq!(daemon.received()[0]["type"], "hello");

    let open = FakeDaemon::start(Behavior::default()).await;
    client.connect(entry("e2", open.port()), None).unwrap();
    let events = collect_until(&mut rx, |e| e.entry_id == "e2" && is_state(e, ConnState::Live)).await;
    assert!(events.iter().any(|e| e.entry_id == "e2"));
    assert!(open.subprotocols().is_empty(), "no password, no Sec-WebSocket-Protocol");
}

#[tokio::test(start_paused = true)]
async fn wrong_password_is_terminal_and_a_new_password_starts_a_new_epoch() {
    let daemon = FakeDaemon::start(Behavior { require_subprotocol: Some(BEARER.to_owned()), ..Behavior::default() }).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), Some(Secret::new("wrong"))).unwrap();
    let first = collect_until(&mut rx, |e| is_state(e, ConnState::AuthFailed)).await;
    assert_eq!(error_of(first.last().unwrap()).unwrap().0, "auth_failed");
    tokio::time::sleep(Duration::from_secs(600)).await;
    assert!(rx.try_recv().is_err(), "no retry without a user action");
    assert_eq!(daemon.upgrades(), 0);
    assert_eq!(daemon.health_hits(), 1);

    client.connect(entry("e1", daemon.port()), Some(Secret::new("s3cret"))).unwrap();
    let second = collect_until(&mut rx, |e| is_state(e, ConnState::Live)).await;
    assert!(is_state(&second[0], ConnState::Probing));
    assert_eq!(second[0].conn_epoch, first[0].conn_epoch + 1);
}

#[tokio::test(start_paused = true)]
async fn host_rejected_names_the_host_and_does_not_retry() {
    let daemon = FakeDaemon::start(Behavior { upgrade_status: Some(403), ..Behavior::default() }).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let events = collect_until(&mut rx, |e| is_state(e, ConnState::HostRejected)).await;
    let (code, message) = error_of(events.last().unwrap()).unwrap();
    assert_eq!(code, "host_rejected");
    assert!(message.contains("127.0.0.1") && message.contains("PI_STUDIO_HOSTNAMES"), "{message}");
    tokio::time::sleep(Duration::from_secs(600)).await;
    assert!(rx.try_recv().is_err());
    assert_eq!(daemon.health_hits(), 1);
}

#[tokio::test]
async fn invalid_password_is_rejected_before_anything_is_sent() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = start();
    for bad in ["has space", "semi;colon", "ünï", ""] {
        assert!(client.connect(entry("e1", daemon.port()), Some(Secret::new(bad))).is_err(), "{bad:?}");
        assert!(client.restart("e1", entry("e1", daemon.port()), Some(Secret::new(bad))).is_err());
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!((daemon.health_hits(), daemon.upgrades()), (0, 0));
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn oversize_frame_counts_and_reconnects_with_backoff() {
    let daemon = FakeDaemon::start(Behavior { after: After::Oversize(17 * 1024 * 1024), ..Behavior::default() }).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let events = collect_until(&mut rx, |e| diagnostics(e).is_some_and(|c| c.frames_too_large == 1)).await;
    let backoff = events.iter().find(|e| is_state(e, ConnState::Backoff)).expect("backoff after the oversize frame");
    assert_eq!(error_of(backoff).unwrap().0, "frame_too_large");
    assert!(backoff_ms(backoff).is_some());
    let more = collect_until(&mut rx, |e| e.conn_epoch == 2 && is_state(e, ConnState::Probing)).await;
    assert!(!more.is_empty(), "client reconnects after backing off");
}

#[tokio::test]
async fn malformed_binary_and_unknown_frames_are_counted_and_the_connection_stays_live() {
    let frames = vec![
        Outgoing::Text("{not json".into()),
        Outgoing::Binary(vec![1, 2, 3]),
        Outgoing::Text(r#"{"type":"terminals_update","x":1}"#.into()),
        Outgoing::Text(r#"{"type":"session","message":{"type":"weird_session_type"}}"#.into()),
        Outgoing::Text(r#"{"type":"session","message":{"type":"agent_update","agentId":"a"}}"#.into()),
    ];
    let daemon = FakeDaemon::start(Behavior { after: After::Send(frames), ..Behavior::default() }).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let events = collect_until(&mut rx, |e| diagnostics(e).is_some_and(|c| c.messages_total == 7)).await;
    let c = diagnostics(events.last().unwrap()).unwrap();
    assert_eq!((c.malformed, c.binary_frames_ignored, c.frames_too_large), (1, 1, 0));
    assert_eq!(c.unknown_types.len(), 2);
    assert_eq!(c.unknown_types["terminals_update"], 1);
    assert_eq!(c.unknown_types["weird_session_type"], 1);
    assert!(c.connected_since.is_some());
    assert!(!events.iter().any(|e| is_state(e, ConnState::Backoff)), "stays live");
    assert_eq!(events.iter().filter_map(state_of).next_back(), Some(ConnState::Live));
}

const PING: Duration = Duration::from_millis(150);

fn start_pinging() -> (ClientHandle, support::Events) {
    let mut config = ClientConfig::new("0.1.0");
    config.ping_every = PING;
    let (client, rx) = ClientHandle::start_in(tokio::runtime::Handle::current(), config);
    (client, support::Events::new(rx))
}

#[tokio::test]
async fn pongs_produce_rtt_events_and_counters() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = start_pinging();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    collect_until(&mut rx, |e| is_state(e, ConnState::Live)).await;
    let live = Instant::now();
    collect_until(&mut rx, |e| matches!(e.kind, DaemonEventKind::Rtt { .. })).await;
    assert!(live.elapsed() >= PING, "first ping after one period");
    let ping = daemon.received().into_iter().find(|f| f["type"] == "ping").expect("ping sent");
    assert!(ping["requestId"].is_string() && ping["clientSentAt"].is_number());
    let events = collect_until(&mut rx, |e| diagnostics(e).is_some_and(|c| c.rtt_last_ms.is_some())).await;
    let c = diagnostics(events.last().unwrap()).unwrap();
    assert!(c.rtt_ewma_ms.is_some() && c.rtt_last_ms.is_some());
}

#[tokio::test]
async fn two_unanswered_pings_close_the_connection_and_back_off() {
    let daemon = FakeDaemon::start(Behavior { pong: false, ..Behavior::default() }).await;
    let (client, mut rx) = start_pinging();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    collect_until(&mut rx, |e| is_state(e, ConnState::Live)).await;
    let live = Instant::now();
    let events = collect_until(&mut rx, |e| is_state(e, ConnState::Backoff)).await;
    let waited = live.elapsed();
    // Ping at 1 period, ping at 2, the third tick finds two unanswered: dead at 3 periods
    // (45 s with the default 15 s), 30 s after the first ping.
    assert!(waited >= PING * 3 - Duration::from_millis(20) && waited <= PING * 6, "{waited:?}");
    assert_eq!(error_of(events.last().unwrap()).unwrap().0, "timeout");
    assert!(backoff_ms(events.last().unwrap()).is_some());
    assert_eq!(daemon.received().iter().filter(|f| f["type"] == "ping").count(), 2);
}

#[tokio::test(start_paused = true)]
async fn three_handshake_failures_become_handshake_rejected_at_the_cap() {
    let daemon = FakeDaemon::start(Behavior { handshake: Handshake::Close, ..Behavior::default() }).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let mut seen = Vec::new();
    for _ in 0..5 {
        let events = collect_until(&mut rx, |e| backoff_ms(e).is_some()).await;
        let last = events.last().unwrap();
        seen.push((error_of(last).unwrap().0, backoff_ms(last).unwrap()));
    }
    let codes: Vec<&str> = seen.iter().map(|(c, _)| c.as_str()).collect();
    assert_eq!(codes, ["closed", "closed", "handshake_rejected", "handshake_rejected", "handshake_rejected"]);
    for (_, ms) in &seen[2..] {
        assert!((24_000..=36_000).contains(ms), "{seen:?}");
    }
}
