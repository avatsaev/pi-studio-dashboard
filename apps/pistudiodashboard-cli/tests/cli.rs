//! Runs the built binary: what a script or an agent sees.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn cli(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_pistudiodashboard-cli")).args(args).output().unwrap();
    (out.status.code(), String::from_utf8(out.stdout).unwrap(), String::from_utf8(out.stderr).unwrap())
}

fn lines(out: &str) -> Vec<Value> {
    out.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn with_config(dir: &Path, rest: &[&str]) -> (Option<i32>, String, String) {
    let file = dir.join("config.json");
    let mut args = vec!["--config", file.to_str().unwrap()];
    args.extend_from_slice(rest);
    cli(&args)
}

#[test]
fn commands_lists_the_registry_with_the_internal_flag() {
    let (code, out, _) = cli(&["commands", "--filter", "feed.ingest"]);
    assert_eq!(code, Some(0));
    let rows = lines(&out);
    assert_eq!(rows.len(), 1, "{out}");
    assert_eq!((rows[0]["id"].clone(), rows[0]["internal"].clone(), rows[0]["enabled"].clone()), ("feed.ingest".into(), true.into(), true.into()));
}

#[test]
fn live_commands_are_disabled_with_the_reason() {
    let (_, out, _) = cli(&["commands", "--filter", "daemon.connect"]);
    let rows = lines(&out);
    assert_eq!((rows[0]["enabled"].clone(), rows[0]["reason"].clone()), (false.into(), "no live connection in the CLI".into()));
    let (_, out, _) = cli(&["commands", "--filter", "daemon.add"]);
    assert_eq!(lines(&out)[0]["enabled"], true, "offline editing stays enabled");
}

#[test]
fn the_sample_subcommands_are_usage_errors() {
    for args in [&["info", "x.pistudiodashboard"][..], &["run", "--new", "x"], &["daemon", "add", "--name", "x"]] {
        let (code, out, err) = cli(args);
        assert_eq!(code, Some(2), "{args:?}: {err}");
        assert!(out.is_empty());
    }
}

#[test]
fn validate_prints_one_line_per_issue_exits_1_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let text = r#"{"version": 1, "daemons": [
        {"entryId": "11111111-1111-4111-8111-111111111111", "name": "ok", "host": "h", "port": 1},
        {"entryId": "22222222-2222-4222-8222-222222222222", "name": "bad port", "host": "h", "port": 0},
        {"entryId": "33333333-3333-4333-8333-333333333333", "name": "NAME_OF_65_CHARACTERS_NAME_OF_65_CHARACTERS_NAME_OF_65_CHARACTERS_", "host": "h", "port": 5}]}"#;
    let file = dir.path().join("config.json");
    std::fs::write(&file, text).unwrap();
    let (code, out, err) = cli(&["config", "validate", file.to_str().unwrap()]);
    assert_eq!(code, Some(1), "{err}");
    let issues = lines(&out);
    assert!(issues.len() >= 2 && issues.iter().all(|i| i["path"].is_string() && i["problem"].is_string()), "{out}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text, "the file is unchanged");
    assert!(!dir.path().join("config.json.bak").exists());
    // a corrupt file is reported, not renamed
    std::fs::write(&file, b"{not json").unwrap();
    let (code, out, _) = cli(&["config", "validate", file.to_str().unwrap()]);
    assert_eq!(code, Some(1));
    assert!(!out.is_empty() && !dir.path().join("config.json.bak").exists());
    let (code, _, err) = cli(&["config", "validate", dir.path().join("missing.json").to_str().unwrap()]);
    assert_eq!(code, Some(1));
    assert!(err.contains("does not exist"));
}

#[test]
fn show_prints_settings_and_entries_without_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let text =
        r#"{"version": 1, "daemons": [{"entryId": "11111111-1111-4111-8111-111111111111", "name": "Box", "host": "h", "port": 1, "password": "leaked"}]}"#;
    std::fs::write(dir.path().join("config.json"), text).unwrap();
    let (code, out, _) = with_config(dir.path(), &["config", "show"]);
    assert_eq!(code, Some(0));
    let rows = lines(&out);
    assert_eq!(rows[0]["settings"]["theme"], "system");
    assert_eq!(rows[1]["daemon"]["name"], "Box");
    assert!(!out.contains("leaked") && !out.contains("\"password\""), "{out}");
    let (_, path, _) = with_config(dir.path(), &["config", "path"]);
    assert!(path.trim().ends_with("config.json"));
}

#[test]
fn add_list_remove_round_trip_through_the_atomic_write() {
    let dir = tempfile::tempdir().unwrap();
    let (code, out, err) =
        with_config(dir.path(), &["daemon", "add", "--name", "Dev", "--host", " 127.0.0.1 ", "--port", "6790", "--web-ui-url", "http://localhost:4173"]);
    assert_eq!(code, Some(0), "{err}");
    let id = lines(&out)[0]["entryId"].as_str().unwrap().to_owned();
    let (_, out, _) = with_config(dir.path(), &["daemon", "list"]);
    let rows = lines(&out);
    assert_eq!((rows.len(), rows[0]["name"].clone(), rows[0]["port"].clone(), rows[0]["host"].clone()), (1, "Dev".into(), 6790.into(), "127.0.0.1".into()));
    let (code, _, err) = with_config(dir.path(), &["daemon", "add", "--name", "dev", "--host", "h", "--port", "1"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("name already used"), "{err}");
    let (code, _, err) = with_config(dir.path(), &["daemon", "add", "--name", "x", "--host", "h", "--port", "0"]);
    assert_eq!(code, Some(1), "{err}");
    let (code, _, _) = with_config(dir.path(), &["daemon", "add", "--name", "x", "--host", "h", "--port", "70000"]);
    assert_eq!(code, Some(2), "70000 is not a port: usage error");
    assert_eq!(lines(&with_config(dir.path(), &["daemon", "list"]).1).len(), 1, "nothing was written by the failures");
    let (code, _, err) = with_config(dir.path(), &["daemon", "remove", &id]);
    assert_eq!(code, Some(0));
    assert!(err.contains("keychain"));
    assert!(with_config(dir.path(), &["daemon", "list"]).1.is_empty());
    assert_eq!(with_config(dir.path(), &["daemon", "remove", &id]).0, Some(1));
}

#[test]
fn a_newer_version_file_is_never_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let text = r#"{"version": 99, "daemons": []}"#;
    std::fs::write(dir.path().join("config.json"), text).unwrap();
    let (code, _, err) = with_config(dir.path(), &["daemon", "add", "--name", "x", "--host", "h", "--port", "1"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("newer version"));
    assert_eq!(std::fs::read_to_string(dir.path().join("config.json")).unwrap(), text);
}

#[test]
fn the_cli_links_no_client_crate() {
    let manifest = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(!manifest.contains("pistudiodashboard-client") && !manifest.contains("tokio"));
}
