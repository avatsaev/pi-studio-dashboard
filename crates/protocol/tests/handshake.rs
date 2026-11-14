//! Handshake/liveness/error/health types against the vendored pi-studio and local fixtures.
//! (Read straight from disk: L0 is `Standalone` and may not depend on the testkit loader.)
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::path::Path;

use pistudiodashboard_protocol::{DegradedReason, HandshakeError, HealthBody, Hello, Inbound, Ping, ServerCaps, ServerInfo, decode_inbound};
use serde_json::{Value, json};

fn fixture(dir: &str, name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(dir).join(name);
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn frame(dir: &str, name: &str) -> Value {
    fixture(dir, name)["frame"].clone()
}

fn inbound(v: &Value) -> Inbound {
    decode_inbound(&v.to_string()).unwrap()
}

#[test]
fn hello_encodes_like_fixture() {
    let want = frame("pi-studio", "hello.json");
    let got = Hello::new("some-uuid", "0.1.0").to_value();
    assert_eq!(got["type"], "hello");
    assert_eq!(got["clientType"], "cli");
    assert_eq!(got["protocolVersion"], 1);
    assert!(got["appVersion"].as_str().unwrap().starts_with("pistudiodashboard/"));
    assert_eq!(got["capabilities"], json!({}));
    // Every field agrees except the volatile clientId and the advertised capabilities.
    let (mut w, mut g) = (want, got);
    for v in [&mut w, &mut g] {
        let o = v.as_object_mut().unwrap();
        o.remove("clientId");
        o.remove("capabilities");
    }
    assert_eq!(w, g);
    assert_eq!(Hello::new("x", "0.1.0").encode(), Hello::new("x", "0.1.0").to_value().to_string());
}

#[test]
fn server_info_fixture() {
    let Inbound::ServerInfo(i) = inbound(&frame("pi-studio", "status.server_info.json")) else { panic!("not server_info") };
    assert_eq!(i.server_id, "00000000-0000-4000-8000-000000000001");
    assert_eq!(i.hostname.as_deref(), Some("daemon-host"));
    assert_eq!(i.version.as_deref(), Some("0.0.115"));
    assert_eq!(i.home_dir.as_deref(), Some("/home/user"));
    assert!(i.capabilities.is_empty());
    assert_eq!(i.features.get("extensionUi"), Some(&json!(true)));
    assert_eq!(i.caps(), ServerCaps { daemon_observability: false });
}

#[test]
fn bad_server_info_is_typed_error() {
    for payload in [
        json!({"status":"server_info"}),
        json!({"status":"server_info","serverId":""}),
        json!({"status":"server_info","serverId":"  "}),
        json!({"status":"server_info","serverId":7}),
    ] {
        let v = json!({"type":"status","payload":payload});
        assert!(matches!(inbound(&v), Inbound::BadHandshake(HandshakeError::BadHandshake(_))), "{v}");
    }
    assert!(ServerInfo::from_payload(&json!([])).is_err());
}

#[test]
fn server_info_is_lenient() {
    let p = json!({"serverId":"s","hostname":5,"version":"v","homeDir":null,"capabilities":3,"features":"x","extra":{}});
    let i = ServerInfo::from_payload(&p).unwrap();
    assert_eq!((i.hostname, i.version.as_deref(), i.home_dir), (None, Some("v"), None));
    assert!(i.capabilities.is_empty() && i.features.is_empty());
    let long = "é".repeat(400);
    let i = ServerInfo::from_payload(&json!({"serverId":"s","hostname":long})).unwrap();
    let h = i.hostname.unwrap();
    assert!(h.len() <= 256 && h.chars().all(|c| c == 'é'));
}

#[test]
fn other_status_payloads_pass_through() {
    let v = json!({"type":"status","payload":{"status":"something_else"}});
    assert!(matches!(inbound(&v), Inbound::Status { .. }));
}

#[test]
fn degraded_table() {
    let none: BTreeSet<DegradedReason> = BTreeSet::new();
    let no_obs: BTreeSet<_> = [DegradedReason::NoObservability].into();
    let cases = [
        (json!({"daemonObservability":true}), true, none),
        (json!({"daemonStatusRpc":true}), false, no_obs.clone()),
        (json!({"daemonObservability":"yes"}), false, no_obs.clone()),
        (json!({"daemonObservability":1}), false, no_obs.clone()),
        (json!({"daemonObservability":false}), false, no_obs.clone()),
        (json!({}), false, no_obs),
    ];
    for (features, want, degraded) in cases {
        let info = ServerInfo::from_payload(&json!({"serverId":"s","features":features})).unwrap();
        assert_eq!(info.caps().daemon_observability, want, "{features}");
        assert_eq!(info.caps().degraded(), degraded, "{features}");
    }
    assert_eq!(serde_json::to_string(&DegradedReason::NoObservability).unwrap(), "\"no_observability\"");
}

#[test]
fn local_no_observability_fixture_is_degraded() {
    let Inbound::ServerInfo(i) = inbound(&frame("local", "status.server_info.no_observability.json")) else { panic!("not server_info") };
    assert_eq!(i.features.get("daemonStatusRpc"), Some(&json!(true)));
    assert!(!i.caps().daemon_observability);
    assert!(i.caps().degraded().contains(&DegradedReason::NoObservability));
}

#[test]
fn ping_encodes_like_fixture() {
    let want = frame("pi-studio", "ping.json");
    let p = Ping { request_id: "req-1".into(), client_sent_at: 1_767_225_600_000 };
    assert_eq!(p.to_value(), want);
    assert_eq!(serde_json::from_str::<Value>(&p.encode()).unwrap(), want);
}

#[test]
fn pong_fixture_and_iso_variant() {
    let Inbound::Pong(p) = inbound(&frame("pi-studio", "pong.json")) else { panic!("not pong") };
    assert_eq!(p.request_id, "req-1");
    assert_eq!((p.server_received_at, p.server_sent_at, p.client_sent_at), (1_767_225_600_000, 1_767_225_600_000, Some(1_767_225_600_000)));

    let iso = json!({"type":"pong","requestId":"r","serverReceivedAt":"2026-01-01T00:00:00Z","serverSentAt":"2026-01-01T00:00:00.250Z"});
    let Inbound::Pong(p) = inbound(&iso) else { panic!("not pong") };
    assert_eq!((p.server_received_at, p.server_sent_at, p.client_sent_at), (1_767_225_600_000, 1_767_225_600_250, None));
}

#[test]
fn pong_missing_required_is_malformed() {
    for v in [
        json!({"type":"pong","serverReceivedAt":1,"serverSentAt":2}),
        json!({"type":"pong","requestId":"r","serverSentAt":2}),
        json!({"type":"pong","requestId":"r","serverReceivedAt":1,"serverSentAt":"garbage"}),
    ] {
        assert_eq!(inbound(&v), Inbound::Malformed { type_: "pong" }, "{v}");
    }
}

#[test]
fn rpc_error_fixture_and_tolerance() {
    let Inbound::RpcError(e) = inbound(&frame("pi-studio", "rpc_error.json")) else { panic!("not rpc_error") };
    assert_eq!(e.request_id, "req-1");
    assert_eq!(e.code.as_deref(), Some("unknown_message_type"));
    assert_eq!(e.message.as_deref(), Some("no handler for scenario_no_such_type"));
    assert_eq!(e.error, None);

    let v = json!({"type":"session","message":{"type":"rpc_error","requestId":"r","code":9,"error":"boom","future":{"a":1}}});
    let Inbound::RpcError(e) = inbound(&v) else { panic!("not rpc_error") };
    assert_eq!((e.code, e.error.as_deref()), (None, Some("boom")));

    let v = json!({"type":"session","message":{"type":"rpc_error","code":"handler_error"}});
    assert_eq!(inbound(&v), Inbound::Malformed { type_: "rpc_error" });
}

#[test]
fn other_session_and_unknown_types() {
    let v = json!({"type":"session","message":{"type":"agent_update","agentId":"a"}});
    assert!(matches!(inbound(&v), Inbound::Session { .. }));
    assert_eq!(inbound(&json!({"type":"terminals_update"})), Inbound::Unknown { type_: "terminals_update".into() });
}

#[test]
fn health_fixture_and_variants() {
    let body = fixture("pi-studio", "health.json")["frame"]["body"].to_string();
    assert!(HealthBody::decode(&body).unwrap().is_ok());
    assert!(HealthBody::decode(r#"{"status":"ok","uptime":3,"x":{"y":1}}"#).unwrap().is_ok());
    assert!(!HealthBody::decode(r#"{"status":"degraded"}"#).unwrap().is_ok());
    assert!(!HealthBody::decode(r#"{"status":true}"#).unwrap().is_ok());
    assert!(!HealthBody::decode("{}").unwrap().is_ok());
    assert!(HealthBody::decode("not json").is_err());
}
