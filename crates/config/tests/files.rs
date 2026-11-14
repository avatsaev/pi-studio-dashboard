use std::fs;
use std::path::Path;

use pistudiodashboard_config::*;

fn ids() -> impl FnMut() -> String {
    let mut n = 0u8;
    move || {
        n = n.wrapping_add(1);
        new_entry_id([n; 16])
    }
}

fn kinds(issues: &[ConfigIssue]) -> Vec<IssueKind> {
    issues.iter().map(|i| i.kind).collect()
}

fn write(dir: &Path, bytes: &[u8]) {
    fs::write(dir.join(CONFIG_FILE), bytes).unwrap();
}

#[test]
fn absent_file_gives_defaults_without_issue() {
    let d = tempfile::tempdir().unwrap();
    let o = load(d.path(), &mut ids());
    assert_eq!(o.config, Config::default());
    assert!(o.issues.is_empty() && !o.recovered_from_corrupt && !o.read_only);
    let s = &o.config.settings;
    assert_eq!(s.theme, ThemeChoice::System);
    assert!(s.notifications.needs_input && s.notifications.failures && s.close_to_tray);
}

#[test]
fn corrupt_files_are_moved_to_bak_replacing_old_bak() {
    let big = vec![b'{'; MAX_FILE_BYTES + 1];
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("truncated", br#"{"version":1,"daemons":[{"name":"a","ho"#.to_vec()),
        ("garbage", vec![0xde, 0xad, 0xbe, 0xef, b'x']),
        ("non-object", b"[1,2]".to_vec()),
        ("huge", big),
    ];
    for (name, bytes) in cases {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join(BACKUP_FILE), b"old backup").unwrap();
        write(d.path(), &bytes);
        let o = load(d.path(), &mut ids());
        assert!(o.recovered_from_corrupt, "{name}");
        assert!(kinds(&o.issues).contains(&IssueKind::CorruptFile), "{name}");
        assert_eq!(o.config, Config::default(), "{name}");
        assert!(!d.path().join(CONFIG_FILE).exists(), "{name}");
        assert_eq!(fs::read(d.path().join(BACKUP_FILE)).unwrap(), bytes, "{name}");
    }
}

#[test]
fn bad_values_do_not_create_bak() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        br#"{"settings":{"theme":"neon"},"daemons":[
            {"name":"bad","host":"h","port":70000},{"name":"good","host":"h2","port":7000}]}"#,
    );
    let o = load(d.path(), &mut ids());
    assert!(!o.recovered_from_corrupt && !o.read_only);
    assert_eq!(o.config.settings.theme, ThemeChoice::System);
    assert_eq!(o.config.daemons.len(), 1);
    assert_eq!(o.config.daemons[0].name, "good");
    assert!(kinds(&o.issues).contains(&IssueKind::DroppedEntry));
    assert!(!d.path().join(BACKUP_FILE).exists());
    assert!(d.path().join(CONFIG_FILE).exists());
}

#[test]
fn fifty_one_entries_cap() {
    let d = tempfile::tempdir().unwrap();
    let items: Vec<String> = (0..51).map(|i| format!(r#"{{"name":"n{i}","host":"h{i}"}}"#)).collect();
    write(d.path(), format!(r#"{{"daemons":[{}]}}"#, items.join(",")).as_bytes());
    let o = load(d.path(), &mut ids());
    assert_eq!(o.config.daemons.len(), 50);
    assert!(kinds(&o.issues).contains(&IssueKind::Capped));
}

#[test]
fn future_version_is_read_only_and_save_refused() {
    let d = tempfile::tempdir().unwrap();
    let src = br#"{"version":2,"settings":{"theme":"dark"},"daemons":[{"name":"a","host":"h"}],"newField":1}"#;
    write(d.path(), src);
    let o = load(d.path(), &mut ids());
    assert!(o.read_only && !o.recovered_from_corrupt);
    assert!(kinds(&o.issues).contains(&IssueKind::FutureVersion));
    assert_eq!(o.config.settings.theme, ThemeChoice::Dark);
    assert_eq!(o.config.daemons.len(), 1);
    assert!(matches!(save(d.path(), &o.config, o.read_only), Err(ConfigError::ReadOnly)));
    assert_eq!(fs::read(d.path().join(CONFIG_FILE)).unwrap(), src);
}

#[test]
fn save_round_trips_atomically_with_mode_0600() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("nested").join("cfg");
    let mut cfg = Config::default();
    cfg.settings.theme = ThemeChoice::Dark;
    cfg.settings.close_to_tray = false;
    save(&dir, &cfg, false).unwrap();
    assert!(!dir.join(TEMP_FILE).exists());
    let o = load(&dir, &mut ids());
    assert_eq!(o.config, cfg);
    assert!(o.issues.is_empty());
    // replace again
    cfg.settings.theme = ThemeChoice::Light;
    save(&dir, &cfg, false).unwrap();
    assert_eq!(load(&dir, &mut ids()).config.settings.theme, ThemeChoice::Light);
    assert!(!dir.join(TEMP_FILE).exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir.join(CONFIG_FILE)), 0o600);
        assert_eq!(mode(&dir), 0o700);
    }
}

#[test]
fn crash_between_tmp_write_and_rename_leaves_old_file() {
    let d = tempfile::tempdir().unwrap();
    save(d.path(), &Config::default(), false).unwrap();
    fs::write(d.path().join(TEMP_FILE), b"{\"version\": 1, \"half").unwrap();
    let o = load(d.path(), &mut ids());
    assert!(o.issues.is_empty() && !o.recovered_from_corrupt);
    // a later save replaces the stale temp file
    save(d.path(), &o.config, false).unwrap();
    assert!(!d.path().join(TEMP_FILE).exists());
}

#[cfg(unix)]
#[test]
fn failed_temp_write_keeps_previous_file() {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    let mut cfg = Config::default();
    save(d.path(), &cfg, false).unwrap();
    let before = fs::read(d.path().join(CONFIG_FILE)).unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let probe = fs::write(d.path().join("probe"), b"x"); // root ignores permissions
    cfg.settings.theme = ThemeChoice::Dark;
    let r = save(d.path(), &cfg, false);
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    if probe.is_ok() {
        return;
    }
    assert!(matches!(r, Err(ConfigError::Io(_))));
    assert_eq!(fs::read(d.path().join(CONFIG_FILE)).unwrap(), before);
    assert!(!d.path().join(TEMP_FILE).exists());
}

#[test]
fn unreadable_path_is_io_issue_and_never_overwritten() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join(CONFIG_FILE)).unwrap(); // reading a directory fails
    let o = load(d.path(), &mut ids());
    assert_eq!(kinds(&o.issues), [IssueKind::Io]);
    assert!(o.read_only && !o.recovered_from_corrupt);
    assert!(matches!(save(d.path(), &o.config, o.read_only), Err(ConfigError::ReadOnly)));
    assert!(!d.path().join(BACKUP_FILE).exists());
}
