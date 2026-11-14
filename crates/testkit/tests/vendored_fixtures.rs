//! Contract harness over the vendored pi-studio fixtures and the dashboard's `local/` set.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pistudiodashboard_protocol::{Frame, decode_frame};
use pistudiodashboard_testkit::fixtures::{Fixture, FixtureError, Manifest, SUPPORTED_FORMAT_VERSION, load_fixture, load_manifest, local_dir, pi_studio_dir};
use std::collections::BTreeSet;
use std::path::Path;

fn json_files(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| !n.starts_with('.') && n.ends_with(".json") && n != "manifest.json")
        .collect()
}

fn check_consistent(dir: &Path, manifest: &Manifest) {
    let listed: BTreeSet<String> = manifest.messages.iter().map(|m| m.file.clone()).collect();
    assert_eq!(listed.len(), manifest.messages.len(), "duplicate manifest entries");
    for file in &listed {
        assert!(dir.join(file).is_file(), "manifest lists missing file {file}");
    }
    let on_disk = json_files(dir);
    let unlisted: Vec<_> = on_disk.difference(&listed).collect();
    assert!(unlisted.is_empty(), "files not in manifest: {unlisted:?}");
    for entry in &manifest.messages {
        if let Some(response) = &entry.response {
            assert!(listed.contains(response), "{}: response {response} is not a listed file", entry.file);
        }
    }
}

fn assert_decodes(name: &str, fixture: &Fixture) {
    let text = serde_json::to_string(&fixture.frame).unwrap();
    let frame = decode_frame(&text).unwrap_or_else(|e| panic!("{name}: decode failed: {e}"));
    match fixture.frame.get("type").and_then(|t| t.as_str()) {
        Some("session") => assert!(matches!(frame, Frame::Session { .. }), "{name}: expected Frame::Session, got {frame:?}"),
        Some("status") => assert!(matches!(frame, Frame::Status { .. }), "{name}: expected Frame::Status"),
        Some("ping") => assert!(matches!(frame, Frame::Ping { .. }), "{name}: expected Frame::Ping"),
        Some("pong") => assert!(matches!(frame, Frame::Pong { .. }), "{name}: expected Frame::Pong"),
        Some("hello") => assert!(matches!(frame, Frame::Hello(_)), "{name}: expected Frame::Hello"),
        other => panic!("{name}: unexpected top-level frame type {other:?}"),
    }
}

fn check_decodes(dir: &Path, manifest: &Manifest) -> usize {
    let mut decoded = 0;
    for entry in &manifest.messages {
        let fixture = load_fixture(dir, entry).unwrap();
        assert_eq!(fixture.type_, entry.type_, "{}: type differs from manifest", entry.file);
        assert_eq!(fixture.direction, entry.direction, "{}: direction differs from manifest", entry.file);
        match entry.direction.as_str() {
            "server_to_client" | "client_to_server" => {
                assert_decodes(&entry.file, &fixture);
                decoded += 1;
            }
            "http" => {
                assert_eq!(entry.file, "health.json");
                let obj = fixture.frame.as_object().unwrap();
                for key in ["status", "contentType", "body"] {
                    assert!(obj.contains_key(key), "health.json frame lacks `{key}`");
                }
            }
            other => panic!("{}: unknown direction {other}", entry.file),
        }
    }
    decoded
}

#[test]
fn vendored_manifest_matches_directory() {
    let dir = pi_studio_dir();
    let manifest = load_manifest(&dir).unwrap();
    assert_eq!(manifest.format_version, SUPPORTED_FORMAT_VERSION);
    assert!(!manifest.messages.is_empty());
    check_consistent(&dir, &manifest);
}

#[test]
fn vendored_frames_decode() {
    let dir = pi_studio_dir();
    let manifest = load_manifest(&dir).unwrap();
    let decoded = check_decodes(&dir, &manifest);
    assert_eq!(decoded, manifest.messages.len() - 1, "every entry except health is a socket frame");
}

#[test]
fn local_fixtures_load_and_decode() {
    let dir = local_dir();
    let manifest = load_manifest(&dir).unwrap();
    check_consistent(&dir, &manifest);
    let files: BTreeSet<&str> = manifest.messages.iter().map(|m| m.file.as_str()).collect();
    for expected in [
        "status.server_info.no_observability.json",
        "agent_stream.unknown_kind.json",
        "agent_update.unknown_status.json",
        "agent_ui_request.unknown_method.json",
    ] {
        assert!(files.contains(expected), "missing local fixture {expected}");
    }
    assert_eq!(check_decodes(&dir, &manifest), manifest.messages.len());

    let no_obs = load_fixture(&dir, manifest.messages.iter().find(|m| m.file.starts_with("status.")).unwrap()).unwrap();
    let features = &no_obs.frame["payload"]["features"];
    assert!(features.get("daemonObservability").is_none());
    assert_eq!(features["daemonStatusRpc"], true);
}

#[test]
fn source_json_records_provenance() {
    let path = pi_studio_dir().join("../SOURCE.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for key in ["repo", "path", "commit", "version", "vendoredOn"] {
        assert!(v[key].as_str().is_some_and(|s| !s.is_empty()), "SOURCE.json lacks `{key}`");
    }
    assert_eq!(v["commit"].as_str().unwrap().len(), 40);
}

#[test]
fn newer_format_version_fails_loudly() {
    let dir = std::env::temp_dir().join(format!("pistudiodashboard-fixtures-v2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("manifest.json"), r#"{"formatVersion": 2, "messages": []}"#).unwrap();
    let result = load_manifest(&dir);
    std::fs::remove_dir_all(&dir).unwrap();
    let err = result.unwrap_err();
    assert!(matches!(err, FixtureError::UnsupportedVersion { found: 2, supported: 1 }));
    let msg = err.to_string();
    assert!(msg.contains('2') && msg.contains("supports formatVersion 1"), "{msg}");
}
