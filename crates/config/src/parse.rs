//! Lenient parsing and stable serialisation.
//!
//! `parse` never panics and never fails: every bad value falls back to its default (or drops its
//! entry) and is reported as a [`ConfigIssue`]. Unparseable input yields defaults plus a
//! `CorruptFile` issue, which `load` uses to decide on recovery.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::model::{
    Config, ConfigIssue, DEFAULT_PORT, DaemonEntry, GroupBy, IssueKind, MAX_DAEMONS, MAX_FILE_BYTES, Notifications, SUPPORTED_VERSION, Settings, ThemeChoice,
    ZOOM_MAX, ZOOM_MIN, is_uuid_v4,
};
use crate::validate::{check_host, check_name, check_port, check_server_id, check_web_ui_url};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseOutcome {
    pub config: Config,
    pub issues: Vec<ConfigIssue>,
}

/// 2-space pretty JSON; known keys in declaration order, unknown keys (sorted) after them.
pub fn serialize(config: &Config) -> Vec<u8> {
    let mut out = serde_json::to_vec_pretty(config).unwrap_or_default();
    out.push(b'\n');
    out
}

type Obj = Map<String, Value>;
type Issues = Vec<ConfigIssue>;

fn join(prefix: &str, key: &str) -> String {
    if prefix.is_empty() { key.to_string() } else { format!("{prefix}.{key}") }
}

fn defaulted(issues: &mut Issues, path: String, detail: &str) {
    issues.push(ConfigIssue::new(IssueKind::DefaultedValue, path, detail));
}

fn corrupt(detail: &str) -> ParseOutcome {
    ParseOutcome { config: Config::default(), issues: vec![ConfigIssue::new(IssueKind::CorruptFile, "", detail)] }
}

fn strip_password(obj: &mut Obj, prefix: &str, issues: &mut Issues) {
    if obj.remove("password").is_some() {
        defaulted(issues, join(prefix, "password"), "plaintext passwords are unsupported; key ignored and removed");
    }
}

fn take_bool(obj: &mut Obj, key: &str, prefix: &str, default: bool, issues: &mut Issues) -> bool {
    match obj.remove(key) {
        None => default,
        Some(Value::Bool(b)) => b,
        Some(_) => {
            defaulted(issues, join(prefix, key), "expected a boolean; default used");
            default
        }
    }
}

fn take_object(obj: &mut Obj, key: &str, prefix: &str, issues: &mut Issues) -> Obj {
    match obj.remove(key) {
        None | Some(Value::Null) => Obj::new(),
        Some(Value::Object(m)) => m,
        Some(_) => {
            defaulted(issues, join(prefix, key), "expected an object; defaults used");
            Obj::new()
        }
    }
}

/// Parses `bytes`. `new_id` supplies fresh UUID-v4 entry ids for entries whose id is missing,
/// invalid or duplicated.
pub fn parse(bytes: &[u8], new_id: &mut dyn FnMut() -> String) -> ParseOutcome {
    if bytes.len() > MAX_FILE_BYTES {
        return corrupt("file larger than 1 MiB");
    }
    let Ok(text) = std::str::from_utf8(bytes) else { return corrupt("not valid UTF-8") };
    let Ok(value) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else {
        return corrupt("not valid JSON");
    };
    let Value::Object(mut root) = value else { return corrupt("top level is not an object") };

    let mut issues = Issues::new();
    strip_password(&mut root, "", &mut issues);
    let version = take_version(&mut root, &mut issues);
    let local_detection_dismissed = take_bool(&mut root, "localDetectionDismissed", "", false, &mut issues);
    let settings = parse_settings(root.remove("settings"), &mut issues);
    let daemons = parse_daemons(root.remove("daemons"), new_id, &mut issues);
    let config = Config { version, daemons, settings, local_detection_dismissed, extra: root };
    ParseOutcome { config, issues }
}

fn take_version(root: &mut Obj, issues: &mut Issues) -> u32 {
    match root.remove("version") {
        None => SUPPORTED_VERSION,
        Some(v) => match v.as_u64().filter(|n| *n >= 1) {
            Some(n) => u32::try_from(n).unwrap_or(u32::MAX),
            None => {
                defaulted(issues, "version".into(), "expected an integer >= 1; 1 assumed");
                SUPPORTED_VERSION
            }
        },
    }
}

fn parse_settings(value: Option<Value>, issues: &mut Issues) -> Settings {
    let mut obj = match value {
        None | Some(Value::Null) => Obj::new(),
        Some(Value::Object(m)) => m,
        Some(_) => {
            defaulted(issues, "settings".into(), "expected an object; defaults used");
            Obj::new()
        }
    };
    let d = Settings::default();
    strip_password(&mut obj, "settings", issues);
    let theme = match obj.remove("theme") {
        None => d.theme,
        Some(Value::String(s)) if s == "dark" => ThemeChoice::Dark,
        Some(Value::String(s)) if s == "light" => ThemeChoice::Light,
        Some(Value::String(s)) if s == "system" => ThemeChoice::System,
        Some(_) => {
            defaulted(issues, "settings.theme".into(), "unknown theme; default used");
            d.theme
        }
    };
    let group_by = match obj.remove("groupBy") {
        None => d.group_by,
        Some(v) => v.as_str().and_then(GroupBy::from_name).unwrap_or_else(|| {
            defaulted(issues, "settings.groupBy".into(), "unknown grouping; default used");
            d.group_by
        }),
    };
    let zoom = match obj.remove("zoom") {
        None => d.zoom,
        Some(v) => v.as_u64().and_then(|z| u16::try_from(z).ok()).filter(|z| (ZOOM_MIN..=ZOOM_MAX).contains(z)).unwrap_or_else(|| {
            defaulted(issues, "settings.zoom".into(), "zoom is a whole percent from 50 to 300; default used");
            d.zoom
        }),
    };
    let mut n = take_object(&mut obj, "notifications", "settings", issues);
    let notifications = Notifications {
        needs_input: take_bool(&mut n, "needsInput", "settings.notifications", true, issues),
        failures: take_bool(&mut n, "failures", "settings.notifications", true, issues),
        extra: n,
    };
    Settings {
        theme,
        notifications,
        close_to_tray: take_bool(&mut obj, "closeToTray", "settings", d.close_to_tray, issues),
        attention_rail_collapsed: take_bool(&mut obj, "attentionRailCollapsed", "settings", false, issues),
        group_by,
        zoom,
        extra: obj,
    }
}

fn parse_daemons(value: Option<Value>, new_id: &mut dyn FnMut() -> String, issues: &mut Issues) -> Vec<DaemonEntry> {
    let items = match value {
        None | Some(Value::Null) => return Vec::new(),
        Some(Value::Array(a)) => a,
        Some(_) => {
            defaulted(issues, "daemons".into(), "expected an array; empty list used");
            return Vec::new();
        }
    };
    if items.len() > MAX_DAEMONS {
        issues.push(ConfigIssue::new(IssueKind::Capped, "daemons", format!("{} entries found; only the first {MAX_DAEMONS} kept", items.len())));
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    let mut out = Vec::new();
    for (i, item) in items.into_iter().take(MAX_DAEMONS).enumerate() {
        if let Some(e) = parse_entry(i, item, &mut ids, &mut names, new_id, issues) {
            out.push(e);
        }
    }
    out
}

fn drop_entry(issues: &mut Issues, path: String, detail: &str) -> Option<DaemonEntry> {
    issues.push(ConfigIssue::new(IssueKind::DroppedEntry, path, detail));
    None
}

fn parse_entry(
    idx: usize,
    item: Value,
    ids: &mut HashSet<String>,
    names: &mut HashSet<String>,
    new_id: &mut dyn FnMut() -> String,
    issues: &mut Issues,
) -> Option<DaemonEntry> {
    let path = format!("daemons[{idx}]");
    let Value::Object(mut obj) = item else { return drop_entry(issues, path, "entry is not an object") };
    strip_password(&mut obj, &path, issues);

    let host = match obj.remove("host") {
        Some(Value::String(s)) => s.trim().to_lowercase().trim_start_matches('[').trim_end_matches(']').to_string(),
        _ => return drop_entry(issues, join(&path, "host"), "missing or non-string host"),
    };
    if let Err(m) = check_host(&host) {
        return drop_entry(issues, join(&path, "host"), &m);
    }
    let port = match obj.remove("port") {
        None => DEFAULT_PORT,
        Some(v) => match v.as_u64().and_then(|n| u16::try_from(n).ok()) {
            Some(p) if check_port(p).is_ok() => p,
            _ => return drop_entry(issues, join(&path, "port"), "port must be an integer 1-65535"),
        },
    };

    let entry_id = take_entry_id(&mut obj, &path, ids, new_id, issues);
    let name = take_name(&mut obj, &path, &host, names, issues);
    let tls = take_bool(&mut obj, "tls", &path, false, issues);
    let auto_detected = take_bool(&mut obj, "autoDetected", &path, false, issues);
    let web_ui_url = take_checked_string(&mut obj, "webUiUrl", &path, check_web_ui_url, issues);
    let last_seen_server_id = take_checked_string(&mut obj, "lastSeenServerId", &path, check_server_id, issues);
    Some(DaemonEntry { entry_id, name, host, port, tls, web_ui_url, last_seen_server_id, auto_detected, extra: obj })
}

fn take_checked_string(obj: &mut Obj, key: &str, path: &str, check: fn(&str) -> Result<(), String>, issues: &mut Issues) -> Option<String> {
    match obj.remove(key) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => match check(&s) {
            Ok(()) => Some(s),
            Err(m) => {
                defaulted(issues, join(path, key), &format!("{m}; value removed"));
                None
            }
        },
        Some(_) => {
            defaulted(issues, join(path, key), "expected a string; value removed");
            None
        }
    }
}

fn take_entry_id(obj: &mut Obj, path: &str, ids: &mut HashSet<String>, new_id: &mut dyn FnMut() -> String, issues: &mut Issues) -> String {
    if let Some(Value::String(s)) = obj.remove("entryId")
        && is_uuid_v4(&s)
        && ids.insert(s.to_lowercase())
    {
        return s;
    }
    let mut id = new_id();
    for _ in 0..8 {
        if ids.insert(id.to_lowercase()) {
            break;
        }
        id = new_id();
    }
    defaulted(issues, join(path, "entryId"), "missing, invalid or duplicate entryId; regenerated");
    id
}

fn take_name(obj: &mut Obj, path: &str, host: &str, names: &mut HashSet<String>, issues: &mut Issues) -> String {
    let raw = match obj.remove("name") {
        Some(Value::String(s)) => s,
        _ => String::new(),
    };
    let cleaned: String = raw.chars().filter(|c| !c.is_control()).collect();
    let mut base: String = cleaned.trim().chars().take(64).collect();
    if base.trim().is_empty() || check_name(&base).is_err() {
        base = host.chars().take(64).collect();
        defaulted(issues, join(path, "name"), "missing or invalid name; host used");
    } else if base != raw {
        defaulted(issues, join(path, "name"), "name was trimmed or cleaned");
    }
    let mut name = base.clone();
    let mut n = 2u32;
    while !names.insert(name.to_lowercase()) {
        let suffix = format!(" ({n})");
        let keep = 64 - suffix.chars().count();
        name = format!("{}{suffix}", base.chars().take(keep).collect::<String>().trim_end());
        n += 1;
    }
    if name != base {
        defaulted(issues, join(path, "name"), "duplicate name; suffix added");
    }
    name
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::model::new_entry_id;

    fn run(input: &str) -> ParseOutcome {
        let mut n = 0u8;
        parse(input.as_bytes(), &mut || {
            n += 1;
            new_entry_id([n; 16])
        })
    }

    #[test]
    fn hostile_inputs_never_panic_and_are_corrupt() {
        for input in ["", "{", "garbage", "[]", "null", "42", "\"x\"", "{\"a\":", "\u{0}\u{1}"] {
            let o = run(input);
            assert_eq!(o.issues.len(), 1, "{input:?}");
            assert_eq!(o.issues[0].kind, IssueKind::CorruptFile, "{input:?}");
            assert_eq!(o.config, Config::default());
        }
        assert_eq!(parse(&[0xff, 0xfe, 0x00], &mut || String::new()).issues[0].kind, IssueKind::CorruptFile);
        assert_eq!(parse(&vec![b' '; MAX_FILE_BYTES + 1], &mut || String::new()).issues[0].kind, IssueKind::CorruptFile);
    }

    #[test]
    fn wrong_types_default_without_corruption() {
        let cases = [
            r#"{"daemons":5}"#,
            r#"{"daemons":[1,"x",null]}"#,
            r#"{"settings":[]}"#,
            r#"{"settings":{"theme":7,"closeToTray":"yes","notifications":3}}"#,
            r#"{"version":"two"}"#,
            r#"{"version":0}"#,
            r#"{"localDetectionDismissed":1}"#,
        ];
        for c in cases {
            let o = run(c);
            assert!(!o.issues.is_empty(), "{c}");
            assert!(o.issues.iter().all(|i| i.kind != IssueKind::CorruptFile), "{c}");
            assert_eq!(o.config.settings, Settings::default(), "{c}");
            assert!(o.config.daemons.is_empty(), "{c}");
            assert_eq!(o.config.version, 1);
        }
    }

    #[test]
    fn group_by_loads_defaults_and_round_trips() {
        assert_eq!(run("{}").config.settings.group_by, GroupBy::Workspace, "grouping by workspace is the default");
        let none = run("{}");
        assert!(none.issues.is_empty());
        let o = run(r#"{"settings":{"groupBy":"none"}}"#);
        assert_eq!((o.config.settings.group_by, o.issues.len()), (GroupBy::None, 0));
        let again = run(&String::from_utf8(serialize(&o.config)).unwrap());
        assert_eq!(again.config.settings.group_by, GroupBy::None, "a flat choice survives a save and reload");
        assert!(again.config.settings.extra.is_empty(), "the key is typed, not left in `extra`");
    }

    #[test]
    fn zoom_loads_within_its_range_and_defaults_with_one_issue_otherwise() {
        assert_eq!(run("{}").config.settings.zoom, 100);
        assert!(run("{}").issues.is_empty());
        for good in ["50", "125", "300"] {
            let o = run(&format!(r#"{{"settings":{{"zoom":{good}}}}}"#));
            assert_eq!((o.config.settings.zoom.to_string(), o.issues.len()), (good.to_owned(), 0));
        }
        let o = run(&String::from_utf8(serialize(&run(r#"{"settings":{"zoom":150}}"#).config)).unwrap());
        assert_eq!(o.config.settings.zoom, 150);
        assert!(o.config.settings.extra.is_empty());
        for bad in ["49", "301", "0", "-5", "1.5", "\"big\"", "null", "70000"] {
            let o = run(&format!(r#"{{"settings":{{"zoom":{bad}}},"daemons":[{{"name":"a","host":"h"}}]}}"#));
            assert_eq!(o.config.settings.zoom, 100, "{bad}");
            assert_eq!(o.issues.iter().filter(|i| i.path == "settings.zoom").count(), 1, "{bad}");
            assert_eq!(o.config.daemons.len(), 1, "{bad}");
        }
    }

    #[test]
    fn a_bad_group_by_defaults_with_one_issue_and_keeps_the_daemons() {
        for bad in [r#""banana""#, "7", "null", r#""Workspace""#] {
            let o = run(&format!(r#"{{"settings":{{"groupBy":{bad}}},"daemons":[{{"name":"a","host":"h"}}]}}"#));
            assert_eq!(o.config.settings.group_by, GroupBy::Workspace, "{bad}");
            assert_eq!(o.issues.iter().filter(|i| i.path == "settings.groupBy" && i.kind == IssueKind::DefaultedValue).count(), 1, "{bad}");
            assert_eq!(o.config.daemons.len(), 1, "{bad}");
        }
    }

    #[test]
    fn bad_entries_dropped_good_kept() {
        let o = run(r#"{"theme":"x","settings":{"theme":"neon"},"daemons":[
              {"name":"a","host":"h1","port":70000},
              {"name":"b","host":"http://h","port":1},
              {"name":"c","host":"h3","port":1.5},
              {"name":"d","host":5},
              {"name":"ok","host":"H2","port":7000,"webUiUrl":"ftp://x","lastSeenServerId":"s"}]}"#);
        assert_eq!(o.config.settings.theme, ThemeChoice::System);
        assert_eq!(o.config.daemons.len(), 1);
        let e = &o.config.daemons[0];
        assert_eq!((e.host.as_str(), e.port, e.web_ui_url.clone()), ("h2", 7000, None));
        assert_eq!(o.issues.iter().filter(|i| i.kind == IssueKind::DroppedEntry).count(), 4);
        assert!(o.issues.iter().any(|i| i.path == "settings.theme"));
        assert!(o.issues.iter().any(|i| i.path == "daemons[4].webUiUrl"));
    }

    #[test]
    fn ids_and_names_are_repaired() {
        let id = "123e4567-e89b-42d3-a456-426614174000";
        let o = run(&format!(
            r#"{{"daemons":[
              {{"entryId":"{id}","name":"Box","host":"a"}},
              {{"entryId":"{id}","name":"box","host":"b"}},
              {{"entryId":"bad","name":"BOX","host":"[::1]"}},
              {{"host":"d"}}]}}"#
        ));
        let d = &o.config.daemons;
        assert_eq!(d.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Box", "box (2)", "BOX (3)", "d"]);
        let unique: HashSet<_> = d.iter().map(|e| e.entry_id.clone()).collect();
        assert_eq!(unique.len(), 4);
        assert!(d.iter().all(|e| is_uuid_v4(&e.entry_id)));
        assert_eq!(d[2].host, "::1");
    }

    #[test]
    fn cap_at_fifty() {
        let items: Vec<String> = (0..51).map(|i| format!(r#"{{"name":"n{i}","host":"h{i}"}}"#)).collect();
        let o = run(&format!(r#"{{"daemons":[{}]}}"#, items.join(",")));
        assert_eq!(o.config.daemons.len(), 50);
        assert_eq!(o.issues.iter().filter(|i| i.kind == IssueKind::Capped).count(), 1);
        assert_eq!(o.config.daemons[49].name, "n49");
    }

    #[test]
    fn unknown_keys_survive_password_does_not() {
        let src = r#"{"version":1,"zeta":{"a":1},"daemons":[{"name":"x","host":"h","password":"hunter2","future":[1,2]}],
          "settings":{"newThing":true,"notifications":{"digest":"daily"}},"password":"top"}"#;
        let o = run(src);
        assert_eq!(o.issues.iter().filter(|i| i.detail.contains("plaintext passwords are unsupported")).count(), 2);
        let out = String::from_utf8(serialize(&o.config)).unwrap();
        assert!(!out.contains("hunter2") && !out.contains("password") && !out.contains("top\""));
        for k in ["zeta", "future", "newThing", "digest"] {
            assert!(out.contains(k), "{k}");
        }
        // stable: reparsing and reserialising is a fixed point
        let again = run(&out);
        assert!(again.issues.iter().all(|i| i.kind == IssueKind::DefaultedValue && i.path.ends_with("entryId")));
        assert_eq!(serialize(&again.config), out.as_bytes());
    }

    #[test]
    fn serialize_format() {
        let out = String::from_utf8(serialize(&Config::default())).unwrap();
        assert!(out.starts_with("{\n  \"version\": 1,\n  \"daemons\": [],\n  \"settings\": {\n    \"theme\": \"system\""));
        assert!(out.ends_with("}\n"));
    }

    #[test]
    fn future_version_is_kept() {
        assert_eq!(run(r#"{"version":2}"#).config.version, 2);
    }
}
