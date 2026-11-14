//! The startup sequence, on the owner thread before the first view: load the config (recovering
//! from a corrupt file), adopt it, add or refresh the auto-detected local daemon, and connect
//! every entry.

use std::path::Path;

use pistudiodashboard_config::{IssueKind, load, new_entry_id};
use pistudiodashboard_engine::{LoadedConfig, Session};
use pistudiodashboard_model::ToastKind;
use pistudiodashboard_shell::LocalDaemon;
use serde_json::json;

/// Fresh entry ids from the OS random source (for entries a loaded file lacks ids for).
pub fn id_source() -> impl FnMut() -> String {
    || {
        let mut bytes = [0u8; 16];
        // A failure leaves zeros: ids stay valid v4 strings, and the loader de-duplicates ids.
        let _ = getrandom::fill(&mut bytes);
        new_entry_id(bytes)
    }
}

/// Run the sequence against the config in `dir`. `detected` is the local daemon as found now.
pub fn run(session: &mut Session, dir: &Path, detected: &LocalDaemon) {
    let mut ids = id_source();
    let outcome = load(dir, &mut ids);
    if outcome.recovered_from_corrupt {
        session.push_toast(ToastKind::Warning, "Your settings file was unreadable; it was saved as config.json.bak and the dashboard started empty");
    }
    if outcome.issues.iter().any(|i| i.kind == IssueKind::FutureVersion) {
        session.push_toast(ToastKind::Warning, "Settings are from a newer version; changes won't be saved");
    }
    let repaired = outcome.issues.iter().filter(|i| matches!(i.kind, IssueKind::DroppedEntry | IssueKind::DefaultedValue | IssueKind::Capped)).count();
    if repaired > 0 {
        log::warn!("{repaired} settings values were repaired or dropped while loading");
        session.push_toast(ToastKind::Info, format!("{repaired} settings values were reset or dropped while loading"));
    }
    session.apply_loaded_config(LoadedConfig { config: outcome.config, issues: outcome.issues, read_only: outcome.read_only });
    auto_detect(session, detected);
    session.connect_all();
}

/// Add the local daemon once, or refresh an untouched auto-detected entry whose address changed.
fn auto_detect(session: &mut Session, found: &LocalDaemon) {
    let config = session.config();
    let same_address = |host: &str, port: u16| host == found.host && port == found.port;
    if let Some(existing) = config.daemons.iter().find(|d| d.auto_detected) {
        if !same_address(&existing.host, existing.port) {
            let params = json!({"entryId": existing.entry_id, "host": found.host, "port": found.port, "tls": found.tls, "autoDetected": true});
            if let Err(e) = session.execute("daemon.edit", params) {
                log::warn!("could not refresh the detected local daemon: {e}");
            }
        }
        return;
    }
    if config.local_detection_dismissed || config.daemons.iter().any(|d| same_address(&d.host, d.port)) {
        return;
    }
    let params = json!({"name": found.name, "host": found.host, "port": found.port, "tls": found.tls, "webUiUrl": found.web_ui_url, "autoDetected": true});
    if let Err(e) = session.execute("daemon.add", params) {
        log::warn!("could not add the detected local daemon: {e}");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use pistudiodashboard_config::{BACKUP_FILE, CONFIG_FILE};
    use pistudiodashboard_engine::EngineServices;

    use super::*;

    fn local(port: u16) -> LocalDaemon {
        LocalDaemon { name: "Local".into(), host: "127.0.0.1".into(), port, tls: false, web_ui_url: Some("http://localhost:4173".into()) }
    }

    fn session(log: &Rc<RefCell<Vec<String>>>) -> Session {
        let sink = Rc::clone(log);
        let services = EngineServices {
            daemon_link: Some(Box::new(move |id, cmd| {
                let what = match cmd {
                    pistudiodashboard_engine::LinkCommand::Connect { entry, .. } => format!("connect {}:{}", entry.host, entry.port),
                    pistudiodashboard_engine::LinkCommand::Restart { entry, .. } => format!("restart {}:{}", entry.host, entry.port),
                    _ => "other".to_owned(),
                };
                sink.borrow_mut().push(format!("{what} ({})", id.len()));
                Ok(())
            })),
            ..EngineServices::default()
        };
        Session::new().with_services(services)
    }

    fn start(dir: &Path, found: &LocalDaemon) -> (Session, Rc<RefCell<Vec<String>>>) {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut s = session(&log);
        run(&mut s, dir, found);
        (s, log)
    }

    #[test]
    fn an_empty_config_gets_the_local_daemon_once_and_connects_it() {
        let dir = tempfile::tempdir().unwrap();
        let (s, log) = start(dir.path(), &local(6767));
        let d = &s.config().daemons;
        assert_eq!((d.len(), d[0].name.as_str(), d[0].port, d[0].auto_detected, d[0].tls), (1, "Local", 6767, true, false));
        assert_eq!(d[0].web_ui_url.as_deref(), Some("http://localhost:4173"));
        assert_eq!(*log.borrow(), ["connect 127.0.0.1:6767 (36)"], "connected exactly once");
    }

    #[test]
    fn a_corrupt_file_is_set_aside_and_the_local_daemon_is_detected_again() {
        for garbage in [&b"{\"version\": 1, \"daemons\": [{\"entryId\""[..], b"\x00\x01garbage"] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join(CONFIG_FILE), garbage).unwrap();
            let (mut s, _) = start(dir.path(), &local(6767));
            assert!(dir.path().join(BACKUP_FILE).exists());
            assert_eq!(s.config().daemons.len(), 1);
            assert!(s.take_toasts().iter().any(|t| t.text.contains("config.json.bak")));
        }
    }

    #[test]
    fn a_dismissed_detection_is_not_repeated() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CONFIG_FILE), br#"{"version": 1, "daemons": [], "localDetectionDismissed": true}"#).unwrap();
        let (s, log) = start(dir.path(), &local(6767));
        assert!(s.config().daemons.is_empty() && log.borrow().is_empty());
    }

    fn write_entry(dir: &Path, auto: bool, port: u16) {
        let id = "11111111-1111-4111-8111-111111111111";
        let text = format!(
            r#"{{"version": 1, "daemons": [{{"entryId": "{id}", "name": "Local", "host": "127.0.0.1", "port": {port}, "tls": false, "autoDetected": {auto}}}]}}"#
        );
        std::fs::write(dir.join(CONFIG_FILE), text).unwrap();
    }

    #[test]
    fn an_untouched_detected_entry_follows_the_address_and_an_edited_one_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        write_entry(dir.path(), true, 6767);
        let (s, log) = start(dir.path(), &local(7000));
        assert_eq!((s.config().daemons[0].port, s.config().daemons[0].auto_detected), (7000, true));
        assert_eq!(log.borrow().len(), 1, "refreshed and connected once: {:?}", log.borrow());
        let dir = tempfile::tempdir().unwrap();
        write_entry(dir.path(), false, 6767);
        let (s, _) = start(dir.path(), &local(7000));
        assert_eq!((s.config().daemons.len(), s.config().daemons[0].port), (1, 6767));
    }

    #[test]
    fn a_newer_version_file_starts_read_only_and_is_never_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let text = br#"{"version": 99, "daemons": [], "future": true}"#;
        std::fs::write(dir.path().join(CONFIG_FILE), text).unwrap();
        let (mut s, _) = start(dir.path(), &local(6767));
        assert!(s.take_toasts().iter().any(|t| t.text.contains("newer version")));
        assert_eq!(std::fs::read(dir.path().join(CONFIG_FILE)).unwrap(), text);
    }

    #[test]
    fn two_entries_both_connect_at_startup() {
        let dir = tempfile::tempdir().unwrap();
        let text = r#"{"version": 1, "daemons": [
            {"entryId": "11111111-1111-4111-8111-111111111111", "name": "A", "host": "a.example", "port": 1},
            {"entryId": "22222222-2222-4222-8222-222222222222", "name": "B", "host": "b.example", "port": 2}]}"#;
        std::fs::write(dir.path().join(CONFIG_FILE), text).unwrap();
        let (s, log) = start(dir.path(), &local(6767));
        assert_eq!(s.config().daemons.len(), 3, "the local daemon is added next to them");
        let mut seen = log.borrow().clone();
        seen.sort();
        assert_eq!(seen, ["connect 127.0.0.1:6767 (36)", "connect a.example:1 (36)", "connect b.example:2 (36)"]);
    }
}
