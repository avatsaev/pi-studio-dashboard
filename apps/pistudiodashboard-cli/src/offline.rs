//! Offline access to the config file through `pistudiodashboard-config`: the same parsing,
//! validation and atomic write as the app, and no connection, keychain or password anywhere.

use std::io::Read;
use std::path::{Path, PathBuf};

use pistudiodashboard_config::{
    CONFIG_FILE, Config, DaemonEntry, MAX_DAEMONS, MAX_FILE_BYTES, SUPPORTED_VERSION, config_dir, new_entry_id, normalise_host, parse, save, validate_entry,
};
use serde_json::{Value, json};

/// The file to use: `--config`, else `config.json` in the app's config directory.
pub fn config_file(given: Option<&Path>) -> Result<PathBuf, String> {
    match given {
        Some(p) => Ok(p.to_owned()),
        None => config_dir(&|k| std::env::var(k).ok())
            .map(|d| d.join(CONFIG_FILE))
            .ok_or_else(|| "cannot determine the config directory; pass --config FILE".to_owned()),
    }
}

fn ids() -> impl FnMut() -> String {
    || {
        let mut bytes = [0u8; 16];
        let _ = getrandom::fill(&mut bytes);
        new_entry_id(bytes)
    }
}

struct Loaded {
    config: Config,
    issues: Vec<pistudiodashboard_config::ConfigIssue>,
    /// A newer-version file: it must not be rewritten.
    read_only: bool,
}

/// Read and parse `path` without side effects; a missing file is an empty config.
fn read(path: &Path) -> Result<Loaded, String> {
    let mut bytes = Vec::new();
    match std::fs::File::open(path) {
        Ok(f) => {
            f.take(MAX_FILE_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Loaded { config: Config::default(), issues: Vec::new(), read_only: false }),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!("{} is larger than {MAX_FILE_BYTES} bytes", path.display()));
    }
    let outcome = parse(&bytes, &mut ids());
    let read_only = outcome.config.version > SUPPORTED_VERSION;
    Ok(Loaded { config: outcome.config, issues: outcome.issues, read_only })
}

fn entry_json(d: &DaemonEntry) -> Value {
    json!({
        "entryId": d.entry_id,
        "name": d.name,
        "host": d.host,
        "port": d.port,
        "tls": d.tls,
        "webUiUrl": d.web_ui_url,
        "autoDetected": d.auto_detected,
    })
}

pub fn show(path: &Path) -> Result<(), String> {
    let loaded = read(path)?;
    let config = serde_json::to_value(&loaded.config.settings).map_err(|e| e.to_string())?;
    println!("{}", json!({"path": path, "settings": config, "localDetectionDismissed": loaded.config.local_detection_dismissed}));
    for d in &loaded.config.daemons {
        println!("{}", json!({"daemon": entry_json(d)}));
    }
    Ok(())
}

pub fn validate(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("{} does not exist", path.display()));
    }
    let loaded = read(path)?;
    for issue in &loaded.issues {
        println!("{}", json!({"path": issue.path, "problem": issue.detail, "kind": issue.kind}));
    }
    if loaded.issues.is_empty() { Ok(()) } else { Err(format!("{} issue(s) in {}", loaded.issues.len(), path.display())) }
}

pub fn list(path: &Path) -> Result<(), String> {
    for d in &read(path)?.config.daemons {
        println!("{}", entry_json(d));
    }
    Ok(())
}

/// Writes need the file to be called `config.json` (the atomic write owns its directory).
fn writable(path: &Path, loaded: &Loaded) -> Result<PathBuf, String> {
    if loaded.read_only {
        return Err(format!("{} was written by a newer version and is not modified", path.display()));
    }
    if path.file_name().and_then(|n| n.to_str()) != Some(CONFIG_FILE) {
        return Err(format!("only a file named {CONFIG_FILE} can be written (got {})", path.display()));
    }
    Ok(path.parent().map_or_else(|| PathBuf::from("."), Path::to_owned))
}

pub fn add(path: &Path, name: &str, host: &str, port: u16, tls: bool, web_ui_url: Option<String>) -> Result<(), String> {
    let mut loaded = read(path)?;
    let dir = writable(path, &loaded)?;
    let entry = DaemonEntry {
        entry_id: ids()(),
        name: name.trim().to_owned(),
        host: normalise_host(host),
        port,
        tls,
        web_ui_url,
        last_seen_server_id: None,
        auto_detected: false,
        extra: Default::default(),
    };
    let mut problems: Vec<String> = Vec::new();
    if loaded.config.daemons.len() >= MAX_DAEMONS {
        problems.push(format!("daemons: at most {MAX_DAEMONS} daemons"));
    }
    if let Err(list) = validate_entry(&entry, &loaded.config) {
        problems.extend(list.iter().map(|e| format!("{}: {}", e.field, e.message)));
    }
    if !problems.is_empty() {
        return Err(problems.join("; "));
    }
    loaded.config.daemons.push(entry.clone());
    save(&dir, &loaded.config, false).map_err(|e| e.to_string())?;
    println!("{}", entry_json(&entry));
    Ok(())
}

pub fn remove(path: &Path, entry_id: &str) -> Result<(), String> {
    let mut loaded = read(path)?;
    let dir = writable(path, &loaded)?;
    let index = loaded.config.daemons.iter().position(|d| d.entry_id == entry_id).ok_or_else(|| format!("no daemon entry `{entry_id}`"))?;
    let removed = loaded.config.daemons.remove(index);
    if removed.auto_detected {
        loaded.config.local_detection_dismissed = true;
    }
    save(&dir, &loaded.config, false).map_err(|e| e.to_string())?;
    println!("{}", entry_json(&removed));
    eprintln!("note: a password saved for this daemon in the OS keychain is removed by the app, not by the CLI");
    Ok(())
}
