//! Connection state machine against the scripted fake daemon.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use pistudiodashboard_client::{ClientConfig, ClientHandle, DaemonEvent, DaemonEventKind, Secret};
use pistudiodashboard_model::{ConnState, DegradedReason};
use support::fake_daemon::{After, Behavior, FakeDaemon, Handshake, HealthMode};
use support::{closed_port, collect_until, entry, is_state, state_of};

fn start() -> (ClientHandle, support::Events) {
    let (client, rx) = ClientHandle::start_in(tokio::runtime::Handle::current(), ClientConfig::new("0.1.0"));
    (client, support::Events::new(rx))
}

fn backoff_ms(e: &DaemonEvent) -> Option<u64> {
    match &e.kind {
        DaemonEventKind::ConnState { state: ConnState::Backoff, next_retry_in_ms, .. } => *next_retry_in_ms,
        _ => None,
    }
}

fn last_error_code(e: &DaemonEvent) -> Option<String> {
    match &e.kind {
        DaemonEventKind::ConnState { last_error: Some(err), .. } => Some(err.code.clone()),
        _ => None,
    }
}

#[tokio::test]
async fn happy_path_emits_ordered_events_and_sends_hello_first() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let events = collect_until(&mut rx, |e| is_state(e, ConnState::Live)).await;

    let shape: Vec<String> = events
        .iter()
        .map(|e| match &e.kind {
            DaemonEventKind::ConnState { state, .. } => format!("{state:?}"),
            DaemonEventKind::Health { reachable, .. } => format!("Health({reachable})"),
            DaemonEventKind::Hello { .. } => "Hello".to_owned(),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(shape, ["Probing", "Health(true)", "Connecting", "Handshaking", "Hello", "Syncing", "Live"]);
    assert!(events.iter().all(|e| e.conn_epoch == events[0].conn_epoch && e.entry_id == "e1"));
    assert!(events.windows(2).all(|w| w[0].seq < w[1].seq));

    let DaemonEventKind::Hello { info, caps, degraded, tls_in_use, is_loopback } = &events[4].kind else { panic!() };
    assert_eq!(info.version.as_deref(), Some("0.0.115"));
    assert!(info.features.contains(&"extensionUi".to_owned()));
    assert!(!caps.daemon_observability && degraded.contains(&DegradedReason::NoObservability));
    assert!(!tls_in_use && *is_loopback);
    assert_eq!(events[4].server_id.as_deref(), Some("00000000-0000-4000-8000-0000000000aa"));
    assert_eq!(events[6].server_id, events[4].server_id);

    let first = &daemon.received()[0];
    assert_eq!(first["type"], "hello");
    assert_eq!(first["clientType"], "cli");
    assert_eq!(first["protocolVersion"], 1);
    assert_eq!(first["appVersion"], "pistudiodashboard/0.1.0");
    assert_eq!(first["capabilities"], serde_json::json!({}));
}

#[tokio::test]
async fn daemon_close_then_backoff_then_next_epoch() {
    let daemon = FakeDaemon::start(Behavior { after: After::Close(Duration::from_millis(50)), ..Behavior::default() }).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let events = collect_until(&mut rx, |e| e.conn_epoch == 2 && is_state(e, ConnState::Probing)).await;
    let backoff = events.iter().find(|e| is_state(e, ConnState::Backoff)).expect("backoff event");
    assert_eq!(backoff.conn_epoch, 1);
    assert!(backoff_ms(backoff).is_some_and(|ms| (400..=600).contains(&ms)), "{:?}", backoff_ms(backoff));
    let next = events.last().unwrap();
    assert_eq!((next.conn_epoch, next.seq), (2, 1));
}

#[tokio::test(start_paused = true)]
async fn unreachable_port_backs_off_on_the_documented_schedule() {
    let port = closed_port().await;
    let (client, mut rx) = start();
    client.connect(entry("e1", port), None).unwrap();
    let mut delays = Vec::new();
    while delays.len() < 8 {
        let events = collect_until(&mut rx, |e| backoff_ms(e).is_some()).await;
        let last = events.last().unwrap();
        assert_eq!(last_error_code(last).as_deref(), Some("unreachable"));
        delays.push(backoff_ms(last).unwrap());
    }
    for (got, want) in delays.iter().zip([500_f64, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0, 30000.0, 30000.0]) {
        let got = *got as f64;
        assert!(got >= want * 0.8 - 1.0 && got <= want * 1.2 + 1.0, "{got} vs {want}: {delays:?}");
    }
}

#[tokio::test]
async fn attempt_counter_resets_after_a_long_enough_live_connection() {
    let daemon = FakeDaemon::start(Behavior { upgrade_status: Some(500), ..Behavior::default() }).await;
    let mut config = ClientConfig::new("0.1.0");
    config.reset_after_live = Duration::from_millis(300);
    let (client, rx) = ClientHandle::start_in(tokio::runtime::Handle::current(), config);
    let mut rx = support::Events::new(rx);
    client.connect(entry("e1", daemon.port()), None).unwrap();
    // Two refused upgrades: delays ~0.5 s, ~1 s; without a reset the next one would be ~2 s.
    for _ in 0..2 {
        collect_until(&mut rx, |e| backoff_ms(e).is_some()).await;
    }
    daemon.set(|b| {
        b.upgrade_status = None;
        b.after = After::Close(Duration::from_millis(600));
    });
    let events = collect_until(&mut rx, |e| backoff_ms(e).is_some() && last_error_code(e).as_deref() == Some("closed")).await;
    assert!(events.iter().any(|e| is_state(e, ConnState::Live)));
    let ms = backoff_ms(events.last().unwrap()).unwrap();
    assert!((400..=600).contains(&ms), "delay after a long live connection was {ms} ms");
}

#[tokio::test(start_paused = true)]
async fn disconnect_closes_with_1000_goes_idle_and_stays_quiet() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    collect_until(&mut rx, |e| is_state(e, ConnState::Live)).await;
    client.disconnect("e1");
    let events = collect_until(&mut rx, |e| is_state(e, ConnState::Idle)).await;
    assert!(is_state(events.last().unwrap(), ConnState::Idle));
    tokio::time::sleep(Duration::from_secs(120)).await;
    assert!(rx.try_recv().is_err(), "no event may follow idle");
    assert_eq!(daemon.close_codes(), [Some(1000)]);
    assert_eq!(daemon.upgrades(), 1);
}

#[tokio::test(start_paused = true)]
async fn remove_stops_all_events_immediately() {
    let port = closed_port().await;
    let (client, mut rx) = start();
    client.connect(entry("e1", port), None).unwrap();
    collect_until(&mut rx, |e| backoff_ms(e).is_some()).await;
    while rx.try_recv().is_ok() {}
    client.remove("e1");
    tokio::time::sleep(Duration::from_secs(300)).await;
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn bad_first_frame_and_missing_server_id_are_bad_handshake() {
    for handshake in [
        Handshake::Raw(r#"{"type":"pong","requestId":"r","serverReceivedAt":1,"serverSentAt":2}"#.to_owned()),
        Handshake::ServerInfo { server_id: None, features: None },
        Handshake::ServerInfo { server_id: Some(String::new()), features: None },
        Handshake::Raw("not json".to_owned()),
    ] {
        let daemon = FakeDaemon::start(Behavior { handshake, ..Behavior::default() }).await;
        let (client, mut rx) = start();
        client.connect(entry("e1", daemon.port()), None).unwrap();
        let events = collect_until(&mut rx, |e| is_state(e, ConnState::Backoff)).await;
        assert_eq!(last_error_code(events.last().unwrap()).as_deref(), Some("bad_handshake"));
        assert!(!events.iter().any(|e| is_state(e, ConnState::Live)));
    }
}

#[tokio::test(start_paused = true)]
async fn silent_daemon_times_out_the_handshake() {
    let daemon = FakeDaemon::start(Behavior { handshake: Handshake::Silent, ..Behavior::default() }).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let events = collect_until(&mut rx, |e| is_state(e, ConnState::Backoff)).await;
    assert_eq!(last_error_code(events.last().unwrap()).as_deref(), Some("timeout"));
}

#[tokio::test]
async fn connect_twice_is_one_attempt() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let mut events = collect_until(&mut rx, |e| matches!(e.kind, DaemonEventKind::Snapshot { .. })).await;
    client.connect(entry("e1", daemon.port()), None).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    events.extend(rx.try_recv().unwrap_or_default());
    assert_eq!(daemon.upgrades(), 1);
    assert_eq!(events.iter().filter(|e| is_state(e, ConnState::Probing)).count(), 1);
    assert!(events.iter().all(|e| e.conn_epoch == 1), "no second attempt");
}

#[tokio::test(start_paused = true)]
async fn upgrade_401_and_403_are_terminal_until_the_user_acts() {
    for (status, state, code) in [(401, ConnState::AuthFailed, "auth_failed"), (403, ConnState::HostRejected, "host_rejected")] {
        let daemon = FakeDaemon::start(Behavior { upgrade_status: Some(status), ..Behavior::default() }).await;
        let (client, mut rx) = start();
        client.connect(entry("e1", daemon.port()), None).unwrap();
        let events = collect_until(&mut rx, |e| state_of(e) == Some(state)).await;
        assert_eq!(last_error_code(events.last().unwrap()).as_deref(), Some(code));
        tokio::time::sleep(Duration::from_secs(300)).await;
        assert!(rx.try_recv().is_err(), "no retry without a user action");
        assert_eq!(daemon.health_hits(), 1);

        daemon.set(|b| b.upgrade_status = None);
        client.connect(entry("e1", daemon.port()), None).unwrap();
        let events = collect_until(&mut rx, |e| is_state(e, ConnState::Live)).await;
        assert!(is_state(&events[0], ConnState::Probing));
    }
}

#[tokio::test(start_paused = true)]
async fn failed_probe_gates_the_socket_once_per_cycle() {
    let daemon = FakeDaemon::start(Behavior { health: HealthMode::NotFound, ..Behavior::default() }).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    // Cycle 1: probe fails -> backoff without touching the socket.
    let first = collect_until(&mut rx, |e| is_state(e, ConnState::Backoff)).await;
    assert!(first.iter().any(|e| matches!(e.kind, DaemonEventKind::Health { reachable: false, .. })));
    assert_eq!(daemon.upgrades(), 0);
    // Cycle 2: probe fails again but the socket is attempted anyway and goes live.
    let second = collect_until(&mut rx, |e| is_state(e, ConnState::Live)).await;
    assert!(second.iter().any(|e| is_state(e, ConnState::Connecting)));
    assert_eq!(daemon.upgrades(), 1);
}

#[tokio::test]
async fn password_is_offered_as_bearer_subprotocol_and_never_leaks() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = start();
    let pw = Secret::new("s3cr3tpw");
    assert!(!format!("{pw:?}").contains("s3cr3tpw"));
    client.connect(entry("e1", daemon.port()), Some(pw)).unwrap();
    let events = collect_until(&mut rx, |e| is_state(e, ConnState::Live)).await;
    assert_eq!(daemon.subprotocols(), ["pi-studio.bearer.s3cr3tpw"]);
    assert!(!serde_json::to_string(&events).unwrap().contains("s3cr3tpw"));
}

#[tokio::test]
async fn oversize_frame_closes_with_frame_too_large() {
    let daemon = FakeDaemon::start(Behavior { after: After::Oversize(17 * 1024 * 1024), ..Behavior::default() }).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let events = collect_until(&mut rx, |e| is_state(e, ConnState::Backoff)).await;
    // The oversize frame arrives while the sync is still waiting for its answers.
    assert!(!events.iter().any(|e| is_state(e, ConnState::Live)));
    assert_eq!(last_error_code(events.last().unwrap()).as_deref(), Some("frame_too_large"));
}

#[tokio::test]
async fn restart_begins_a_new_epoch_at_probing() {
    let a = FakeDaemon::start(Behavior::default()).await;
    let b = FakeDaemon::start(Behavior { handshake: Handshake::ServerInfo { server_id: Some("other".into()), features: None }, ..Behavior::default() }).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", a.port()), None).unwrap();
    let first = collect_until(&mut rx, |e| matches!(e.kind, DaemonEventKind::Snapshot { .. })).await;
    client.restart("e1", entry("e1", b.port()), None).unwrap();
    let second = collect_until(&mut rx, |e| matches!(e.kind, DaemonEventKind::Snapshot { .. })).await;
    let next_epoch: Vec<_> = second.iter().filter(|e| e.conn_epoch == first[0].conn_epoch + 1).collect();
    assert!(is_state(next_epoch[0], ConnState::Probing));
    assert_eq!(second.last().unwrap().server_id.as_deref(), Some("other"));
}
