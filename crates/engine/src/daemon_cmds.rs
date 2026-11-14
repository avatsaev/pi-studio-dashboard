//! `daemon.*` commands: the dashboard's daemon entries, their persistence, passwords and the
//! connection lifecycle.
//!
//! The engine owns the in-memory [`Config`]; every mutation asks the `config_store` hook to save
//! it (the app debounces and writes on its I/O worker) and the `daemon_link` hook to (re)connect.
//! Nothing here opens a socket. Passwords live in a process-lifetime map of [`Secret`]s and in
//! the keychain hook; they never reach `Config`, `MonitorState`, the view or a log.

use std::collections::{BTreeMap, BTreeSet};

use pistudiodashboard_config::{Config, DaemonEntry, FieldError, GroupBy, MAX_DAEMONS, new_entry_id, normalise_host, validate_entry};
use pistudiodashboard_model::{ConnState, DaemonEvent, DaemonEventKind, DaemonSlice, ToastKind};
use serde_json::{Value, json};

use crate::commands::{CommandSpec, Run, always};
use crate::services::{Hook, KeychainError, LinkCommand, Secret};
use crate::{EngineError, LoadedConfig, Result, Session, params};

/// Longest accepted password, in bytes.
pub const MAX_PASSWORD_BYTES: usize = 1024;

/// What the shell should open after an authentication failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswordPrompt {
    pub entry_id: String,
    /// The saved password could not be read: the prompt says the new one will not be saved.
    pub keychain_unavailable: bool,
}

/// Everything the daemon commands keep between calls (a field of `Session`).
#[derive(Default)]
pub(crate) struct DaemonState {
    config: Config,
    read_only: bool,
    read_only_noted: bool,
    passwords: BTreeMap<String, Secret>,
    remembered: BTreeSet<String>,
    keychain_unavailable: BTreeSet<String>,
    prompts: Vec<PasswordPrompt>,
    id_counter: u64,
    /// Entries the link was told to connect (cleared by disconnect and remove), so startup
    /// does not connect an entry it just added a second time.
    linked: BTreeSet<String>,
}

fn spec(id: &'static str, label: &'static str, params: &'static str, run: Run) -> CommandSpec {
    CommandSpec { id, label, menu: &[], shortcut: None, internal: false, params, enabled: always, run }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        // A menu click or Cmd+N runs this without params: the shell opens the form instead
        // (`menus::invoke`); the form submit calls it with the params.
        CommandSpec {
            menu: &["Daemon"],
            shortcut: Some("Cmd+N"),
            ..spec(
                "daemon.add",
                "Add Daemon…",
                r#"{"name": string, "host": string, "port": 1-65535, "tls": bool = false, "webUiUrl"?: string, "password"?: string}"#,
                daemon_add,
            )
        },
        // The menu items act on the one selected or only daemon (`menus::invoke`); a chip's menu names its entry.
        CommandSpec {
            menu: &["Daemon"],
            enabled: entry_exists,
            ..spec(
                "daemon.edit",
                "Edit Daemon…",
                r#"{"entryId": string, "name"?: string, "host"?: string, "port"?: n, "tls"?: bool, "webUiUrl"?: string|null}"#,
                daemon_edit,
            )
        },
        CommandSpec { menu: &["Daemon"], enabled: entry_exists, ..spec("daemon.remove", "Remove Daemon", r#"{"entryId": string}"#, daemon_remove) },
        CommandSpec { enabled: link_available, ..spec("daemon.connect", "Connect", r#"{"entryId": string}"#, daemon_connect) },
        CommandSpec { enabled: link_available, ..spec("daemon.disconnect", "Disconnect", r#"{"entryId": string}"#, daemon_disconnect) },
        spec("daemon.setPassword", "Set Password…", r#"{"entryId": string, "password": string, "remember": bool}"#, daemon_set_password),
        CommandSpec { internal: true, ..spec("config.saveFailed", "Config Save Failed", r#"{"error": string}"#, config_save_failed) },
        // Read-only: the shell asks for it while `UiState.diagnosticsTarget` is set and adds it to the view as `diagnostics`.
        CommandSpec { internal: true, ..spec("daemon.diagnostics", "Daemon Diagnostics Data", r#"{"entryId": string}"#, daemon_diagnostics) },
    ]
}

fn link_available(s: &Session) -> std::result::Result<(), String> {
    s.services.require(Hook::DaemonLink)
}

fn entry_exists(s: &Session) -> std::result::Result<(), String> {
    if s.config().daemons.is_empty() { Err("no daemon is configured".into()) } else { Ok(()) }
}

// ---------------------------------------------------------------------------------------------
// Redaction

/// A copy of a command's params that is safe to log: `password` of `daemon.add` and
/// `daemon.setPassword` becomes `***`.
#[must_use]
pub fn redact_params(command: &str, params: &Value) -> Value {
    let mut out = params.clone();
    if matches!(command, "daemon.add" | "daemon.setPassword")
        && let Some(o) = out.as_object_mut()
        && o.contains_key("password")
    {
        o.insert("password".into(), json!("***"));
    }
    out
}

/// Like [`redact_params`] for a control-protocol call (`engine.execute` wraps the command).
#[must_use]
pub fn redact_request(method: &str, params: &Value) -> Value {
    if method == "engine.execute"
        && let Some(command) = params.get("command").and_then(Value::as_str)
    {
        let mut out = params.clone();
        if let Some(o) = out.as_object_mut()
            && let Some(inner) = params.get("params")
        {
            o.insert("params".into(), redact_params(command, inner));
        }
        return out;
    }
    params.clone()
}

// ---------------------------------------------------------------------------------------------
// Validation

struct Failure {
    field: &'static str,
    code: &'static str,
    message: String,
}

fn failure(field: &'static str, code: &'static str, message: impl Into<String>) -> Failure {
    Failure { field, code, message: message.into() }
}

fn failures(list: Vec<Failure>) -> EngineError {
    let errors: Vec<Value> = list.iter().map(|f| json!({"field": f.field, "code": f.code, "message": f.message})).collect();
    EngineError::BadParams(json!({"errors": errors}).to_string())
}

fn from_field_error(e: &FieldError) -> Failure {
    match e.field {
        "name" if e.message == "name already used" => failure("name", "duplicate_name", e.message.clone()),
        "name" => failure("name", "invalid_name", e.message.clone()),
        "host" => failure("host", "invalid_host", e.message.clone()),
        "port" => failure("port", "invalid_port", e.message.clone()),
        "webUiUrl" => failure("webUiUrl", "invalid_url", e.message.clone()),
        other => failure(other, "invalid_name", e.message.clone()),
    }
}

fn token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// A password that can be sent as a WebSocket subprotocol token (provisional rule, see the
/// client's `Secret::validate`) and fits the keychain.
fn password_problem(password: &str) -> Option<&'static str> {
    if password.len() > MAX_PASSWORD_BYTES {
        Some("password is longer than 1,024 bytes")
    } else if !password.bytes().all(token_char) {
        Some("password contains characters that cannot be sent to the daemon")
    } else {
        None
    }
}

fn port_param(p: &Value) -> Result<Option<u16>> {
    Ok(params::u64_param(p, "port")?.and_then(|n| u16::try_from(n).ok()))
}

fn url_param(p: &Value) -> Result<Option<Option<String>>> {
    match p.get("webUiUrl") {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(Some(None)),
        Some(Value::String(s)) => Ok(Some(Some(s.clone()))),
        Some(other) => Err(EngineError::BadParams(format!("`webUiUrl` must be a string or null, got {}", params::kind(other)))),
    }
}

// ---------------------------------------------------------------------------------------------
// Session API

impl Session {
    /// The engine's copy of the config (what `config_store.save` receives).
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.daemon.config
    }

    /// Set the saved grouping mode. Saves only when the value changes (the store, read-only mode
    /// and failures are handled by `persist`, like every other config change).
    pub fn set_group_by(&mut self, group_by: GroupBy) {
        if self.daemon.config.settings.group_by == group_by {
            return;
        }
        self.daemon.config.settings.group_by = group_by;
        self.persist();
    }

    /// Set the saved zoom (percent). Saved like the grouping mode: only on a change.
    pub fn set_zoom(&mut self, zoom: u16) {
        if self.daemon.config.settings.zoom == zoom {
            return;
        }
        self.daemon.config.settings.zoom = zoom;
        self.persist();
    }

    /// Adopt a config loaded at startup: it becomes the engine's config and one slice per entry
    /// appears in the monitor, in config order. `read_only` (a newer-version file) refuses saves.
    pub fn apply_loaded_config(&mut self, loaded: LoadedConfig) {
        self.daemon.config = loaded.config;
        self.daemon.read_only = loaded.read_only;
        let slices: Vec<DaemonSlice> = self.daemon.config.daemons.iter().map(|d| DaemonSlice::new(d.entry_id.clone(), d.name.clone())).collect();
        let _ = self.update(|m| {
            m.daemons = slices;
            Ok(())
        });
    }

    /// Queue a user-visible toast (startup problems, background failures).
    pub fn push_toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.pending_toasts.push(crate::Toast { kind, text: text.into() });
    }

    /// Password prompts raised by authentication failures since the last call.
    pub fn take_prompts(&mut self) -> Vec<PasswordPrompt> {
        std::mem::take(&mut self.daemon.prompts)
    }

    /// Start connecting every configured entry that is not already linked, each with its own
    /// password lookup. Returns the number of entries asked to connect.
    pub fn connect_all(&mut self) -> usize {
        let todo: Vec<DaemonEntry> = self.daemon.config.daemons.iter().filter(|e| !self.daemon.linked.contains(&e.entry_id)).cloned().collect();
        todo.iter().map(|entry| usize::from(self.link_connect(entry, false))).sum()
    }

    /// `(has_password, in_memory_only, keychain_unavailable)` for a daemon chip.
    pub(crate) fn password_flags(&self, entry_id: &str) -> (bool, bool, bool) {
        let d = &self.daemon;
        let in_memory = d.passwords.contains_key(entry_id);
        (in_memory || d.remembered.contains(entry_id), in_memory && !d.remembered.contains(entry_id), d.keychain_unavailable.contains(entry_id))
    }

    /// Called after a `feed.ingest` batch is published: raise password prompts for
    /// authentication failures and remember each daemon's `serverId`.
    pub(crate) fn note_events(&mut self, events: &[DaemonEvent]) {
        let mut changed = false;
        for e in events {
            match &e.kind {
                DaemonEventKind::ConnState { state: ConnState::AuthFailed, .. } => self.raise_prompt(&e.entry_id),
                DaemonEventKind::Hello { .. } => changed |= self.remember_server_id(&e.entry_id, e.server_id.as_deref()),
                _ => {}
            }
        }
        if changed {
            self.persist();
        }
    }

    fn raise_prompt(&mut self, entry_id: &str) {
        let live = self.snapshot().daemon(entry_id).is_some_and(|d| d.conn == ConnState::AuthFailed);
        if live && !self.daemon.prompts.iter().any(|p| p.entry_id == entry_id) {
            let keychain_unavailable = self.daemon.keychain_unavailable.contains(entry_id);
            self.daemon.prompts.push(PasswordPrompt { entry_id: entry_id.to_owned(), keychain_unavailable });
        }
    }

    fn remember_server_id(&mut self, entry_id: &str, server_id: Option<&str>) -> bool {
        let Some(sid) = server_id else { return false };
        match self.daemon.config.daemons.iter_mut().find(|d| d.entry_id == entry_id) {
            Some(entry) if entry.last_seen_server_id.as_deref() != Some(sid) => {
                entry.last_seen_server_id = Some(sid.to_owned());
                true
            }
            _ => false,
        }
    }

    fn entry_index(&self, entry_id: &str) -> Result<usize> {
        self.daemon.config.daemons.iter().position(|d| d.entry_id == entry_id).ok_or_else(|| EngineError::BadParams(unknown_entry(entry_id)))
    }

    fn next_entry_id(&mut self) -> String {
        if let Some(random) = self.services.random.as_mut() {
            return new_entry_id(random());
        }
        // splitmix64 over a counter: deterministic, no clock, unique within this session.
        let mut bytes = [0u8; 16];
        for half in bytes.chunks_mut(8) {
            self.daemon.id_counter = self.daemon.id_counter.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.daemon.id_counter;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            half.copy_from_slice(&(z ^ (z >> 31)).to_le_bytes());
        }
        new_entry_id(bytes)
    }

    /// Ask the store to save the config; failures and read-only mode become toasts.
    fn persist(&mut self) {
        let Some(store) = self.services.config_store.as_mut() else { return };
        if self.daemon.read_only {
            if !self.daemon.read_only_noted {
                self.daemon.read_only_noted = true;
                self.push_toast(ToastKind::Warning, "Settings are from a newer version; changes won't be saved");
            }
            return;
        }
        if let Err(e) = store.save(&self.daemon.config) {
            self.push_toast(ToastKind::Warning, format!("Couldn't save settings: {e}"));
        }
    }

    /// Memory first, then the keychain. A locked or missing keychain yields `None` and is
    /// remembered so the prompt can say the password will not be saved.
    fn password_for(&mut self, entry_id: &str) -> Option<Secret> {
        if let Some(s) = self.daemon.passwords.get(entry_id) {
            return Some(s.clone());
        }
        let keychain = self.services.keychain.as_mut()?;
        match keychain.get(entry_id) {
            Ok(s) => Some(s),
            Err(KeychainError::NotFound) => None,
            Err(_) => {
                self.daemon.keychain_unavailable.insert(entry_id.to_owned());
                None
            }
        }
    }

    /// Keep `secret` for this process and, when `remember`, in the keychain. A warning code comes
    /// back when it could not be saved.
    fn store_password(&mut self, entry_id: &str, secret: Secret, remember: bool) -> Option<&'static str> {
        self.daemon.passwords.insert(entry_id.to_owned(), secret.clone());
        self.daemon.remembered.remove(entry_id);
        if !remember {
            return None;
        }
        let saved = self.services.keychain.as_mut().map(|k| k.set(entry_id, &secret));
        if matches!(saved, Some(Ok(()))) {
            self.daemon.remembered.insert(entry_id.to_owned());
            self.daemon.keychain_unavailable.remove(entry_id);
            None
        } else {
            self.daemon.keychain_unavailable.insert(entry_id.to_owned());
            Some("keychain_unavailable")
        }
    }

    fn drop_password(&mut self, entry_id: &str) -> Option<&'static str> {
        self.daemon.passwords.remove(entry_id);
        self.daemon.remembered.remove(entry_id);
        let deleted = self.services.keychain.as_mut().map(|k| k.delete(entry_id));
        match deleted {
            Some(Err(e)) if e != KeychainError::NotFound => Some("keychain_unavailable"),
            _ => None,
        }
    }

    fn send_link(&mut self, entry_id: &str, command: LinkCommand) {
        match &command {
            LinkCommand::Connect { .. } | LinkCommand::Restart { .. } => self.daemon.linked.insert(entry_id.to_owned()),
            LinkCommand::Disconnect | LinkCommand::Remove => self.daemon.linked.remove(entry_id),
        };
        let Some(link) = self.services.daemon_link.as_mut() else { return };
        if let Err(e) = link(entry_id, command) {
            self.push_toast(ToastKind::Warning, format!("Couldn't update the connection: {e}"));
        }
    }

    /// Connect (or retry) `entry` unless it is already connecting or live. `force` restarts.
    fn link_connect(&mut self, entry: &DaemonEntry, force: bool) -> bool {
        let state = self.snapshot().daemon(&entry.entry_id).map(|d| d.conn);
        let active = matches!(state, Some(ConnState::Probing | ConnState::Connecting | ConnState::Handshaking | ConnState::Syncing | ConnState::Live));
        if active && !force {
            return false;
        }
        let password = self.password_for(&entry.entry_id);
        let restart = force || matches!(state, Some(ConnState::Backoff | ConnState::AuthFailed | ConnState::HostRejected));
        let entry = entry.clone();
        let command = if restart { LinkCommand::Restart { entry: entry.clone(), password } } else { LinkCommand::Connect { entry: entry.clone(), password } };
        self.send_link(&entry.entry_id, command);
        true
    }
}

fn unknown_entry(entry_id: &str) -> String {
    json!({"errors": [{"field": "entryId", "code": "unknown_entry", "message": format!("no daemon entry `{}`", entry_id.chars().take(64).collect::<String>())}]}).to_string()
}

// ---------------------------------------------------------------------------------------------
// Commands

fn warnings(list: &[&str]) -> Value {
    json!(list)
}

fn daemon_add(s: &mut Session, p: &Value) -> Result<Value> {
    let name = params::str_param(p, "name")?.unwrap_or("").trim().to_owned();
    let host = normalise_host(params::str_param(p, "host")?.unwrap_or(""));
    let port = port_param(p)?.unwrap_or(0);
    let tls = params::bool_param(p, "tls")?.unwrap_or(false);
    let web_ui_url = url_param(p)?.flatten();
    let password = params::str_param(p, "password")?.filter(|x| !x.is_empty()).map(str::to_owned);
    let auto_detected = params::bool_param(p, "autoDetected")?.unwrap_or(false);

    let entry =
        DaemonEntry { entry_id: s.next_entry_id(), name, host, port, tls, web_ui_url, last_seen_server_id: None, auto_detected, extra: Default::default() };
    let mut problems: Vec<Failure> = Vec::new();
    if s.daemon.config.daemons.len() >= MAX_DAEMONS {
        problems.push(failure("daemons", "too_many_daemons", format!("at most {MAX_DAEMONS} daemons")));
    }
    if let Err(list) = validate_entry(&entry, &s.daemon.config) {
        problems.extend(list.iter().map(from_field_error));
    }
    if let Some(msg) = password.as_deref().and_then(password_problem) {
        problems.push(failure("password", "invalid_password", msg));
    }
    if !problems.is_empty() {
        return Err(failures(problems));
    }

    let slice = DaemonSlice::new(entry.entry_id.clone(), entry.name.clone());
    s.update(|m| {
        m.daemons.push(slice);
        Ok(())
    })?;
    s.daemon.config.daemons.push(entry.clone());
    if auto_detected {
        s.daemon.config.local_detection_dismissed = false;
    }
    s.persist();
    let mut warn = Vec::new();
    if let Some(pw) = password {
        warn.extend(s.store_password(&entry.entry_id, Secret::new(pw), true));
    }
    s.link_connect(&entry, false);
    Ok(json!({"entryId": entry.entry_id, "warnings": warnings(&warn)}))
}

fn daemon_edit(s: &mut Session, p: &Value) -> Result<Value> {
    let entry_id = params::req_str(p, "entryId")?;
    let index = s.entry_index(entry_id)?;
    let before = s.daemon.config.daemons.get(index).cloned().ok_or_else(|| EngineError::BadParams(unknown_entry(entry_id)))?;
    let mut entry = before.clone();
    if let Some(name) = params::str_param(p, "name")? {
        entry.name = name.trim().to_owned();
    }
    if let Some(host) = params::str_param(p, "host")? {
        entry.host = normalise_host(host);
    }
    if p.get("port").is_some_and(|v| !v.is_null()) {
        entry.port = port_param(p)?.unwrap_or(0);
    }
    if let Some(tls) = params::bool_param(p, "tls")? {
        entry.tls = tls;
    }
    if let Some(url) = url_param(p)? {
        entry.web_ui_url = url;
    }
    // Editing marks an auto-detected entry as the user's; startup re-detection passes `true`
    // to refresh an untouched one.
    entry.auto_detected = params::bool_param(p, "autoDetected")?.unwrap_or(false) && before.auto_detected;
    if let Err(list) = validate_entry(&entry, &s.daemon.config) {
        return Err(failures(list.iter().map(from_field_error).collect()));
    }
    let reconnect = (entry.host.as_str(), entry.port, entry.tls) != (before.host.as_str(), before.port, before.tls);
    let renamed = entry.name != before.name;
    if let Some(slot) = s.daemon.config.daemons.get_mut(index) {
        *slot = entry.clone();
    }
    if renamed {
        let (id, name) = (entry.entry_id.clone(), entry.name.clone());
        s.update(|m| {
            if let Some(d) = m.daemons.iter_mut().find(|d| d.entry_id == id) {
                d.name = name;
            }
            Ok(())
        })?;
    }
    s.persist();
    if reconnect {
        let password = s.password_for(&entry.entry_id);
        s.send_link(&entry.entry_id.clone(), LinkCommand::Restart { entry, password });
    }
    Ok(json!({"entryId": entry_id, "reconnected": reconnect}))
}

fn daemon_remove(s: &mut Session, p: &Value) -> Result<Value> {
    let entry_id = params::req_str(p, "entryId")?.to_owned();
    let index = s.entry_index(&entry_id)?;
    // The connection goes first: once `Remove` returns, no event of this entry is applied.
    s.send_link(&entry_id, LinkCommand::Remove);
    let server_id = s.snapshot().daemon(&entry_id).and_then(|d| d.server_id.clone());
    let id = entry_id.clone();
    s.update(|m| {
        let shared = server_id.as_deref().is_some_and(|sid| m.daemons.iter().any(|d| d.entry_id != id && d.server_id.as_deref() == Some(sid)));
        let owner_gone = m.daemons.iter().find(|d| d.entry_id == id).is_some_and(|d| d.duplicate_of.is_none());
        m.daemons.retain(|d| d.entry_id != id);
        if shared {
            // Another entry reaches the same daemon: keep its agents and let it take over.
            if owner_gone {
                for d in m.daemons.iter_mut().filter(|d| d.duplicate_of.as_deref() == Some(id.as_str())) {
                    d.duplicate_of = None;
                }
            }
        } else if let Some(sid) = server_id.as_deref() {
            m.agents.retain(|_, a| a.server_id != sid);
            m.ui_requests.retain(|_, u| u.server_id != sid);
            if m.drilldown.as_ref().is_some_and(|d| d.target.server_id == sid) {
                m.drilldown = None;
            }
        }
        Ok(())
    })?;
    let removed = s.daemon.config.daemons.remove(index);
    if removed.auto_detected {
        s.daemon.config.local_detection_dismissed = true;
    }
    s.persist();
    s.daemon.prompts.retain(|x| x.entry_id != entry_id);
    s.daemon.keychain_unavailable.remove(&entry_id);
    let mut warn = Vec::new();
    if let Some(w) = s.drop_password(&entry_id) {
        warn.push(w);
        s.push_toast(ToastKind::Warning, "Couldn't remove the saved password from the keychain");
    }
    Ok(json!({"entryId": entry_id, "warnings": warnings(&warn)}))
}

fn daemon_connect(s: &mut Session, p: &Value) -> Result<Value> {
    let entry_id = params::req_str(p, "entryId")?;
    let index = s.entry_index(entry_id)?;
    let entry = s.daemon.config.daemons.get(index).cloned().ok_or_else(|| EngineError::BadParams(unknown_entry(entry_id)))?;
    if s.link_connect(&entry, false) {
        Ok(json!({"entryId": entry_id, "connecting": true}))
    } else {
        Ok(json!({"entryId": entry_id, "connecting": false, "reason": "already connected"}))
    }
}

fn daemon_disconnect(s: &mut Session, p: &Value) -> Result<Value> {
    let entry_id = params::req_str(p, "entryId")?;
    s.entry_index(entry_id)?;
    let idle = s.snapshot().daemon(entry_id).is_some_and(|d| matches!(d.conn, ConnState::Idle | ConnState::Disabled));
    if idle {
        return Ok(json!({"entryId": entry_id, "disconnected": false, "reason": "already disconnected"}));
    }
    s.send_link(entry_id, LinkCommand::Disconnect);
    Ok(json!({"entryId": entry_id, "disconnected": true}))
}

fn daemon_diagnostics(s: &mut Session, p: &Value) -> Result<Value> {
    let entry_id = params::req_str(p, "entryId")?;
    s.entry_index(entry_id)?;
    let m = s.snapshot();
    let slice = m.daemon(entry_id).ok_or_else(|| EngineError::BadParams(unknown_entry(entry_id)))?;
    Ok(crate::daemon_view::diagnostics(s, slice))
}

fn daemon_set_password(s: &mut Session, p: &Value) -> Result<Value> {
    let entry_id = params::req_str(p, "entryId")?;
    let password = params::req_str(p, "password")?;
    let remember = params::bool_param(p, "remember")?.unwrap_or(false);
    let index = s.entry_index(entry_id)?;
    if let Some(msg) = password_problem(password) {
        return Err(failures(vec![failure("password", "invalid_password", msg)]));
    }
    let mut warn = Vec::new();
    if password.is_empty() {
        if remember {
            warn.extend(s.drop_password(entry_id));
        } else {
            s.daemon.passwords.remove(entry_id);
        }
    } else {
        warn.extend(s.store_password(entry_id, Secret::new(password), remember));
    }
    if let Some(entry) = s.daemon.config.daemons.get(index).cloned() {
        let password = s.password_for(entry_id);
        s.send_link(entry_id, LinkCommand::Restart { entry, password });
    }
    Ok(json!({"entryId": entry_id, "warnings": warnings(&warn)}))
}

fn config_save_failed(s: &mut Session, p: &Value) -> Result<Value> {
    let error: String = params::str_param(p, "error")?.unwrap_or("unknown error").chars().take(200).collect();
    s.push_toast(ToastKind::Warning, format!("Couldn't save settings: {error}"));
    Ok(Value::Null)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    use super::*;
    use crate::services::{ConfigStore, EngineServices, Keychain};

    #[derive(Default)]
    struct Log {
        links: Vec<String>,
        saves: Vec<Config>,
        keychain: Vec<String>,
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Ok,
        Locked,
    }

    struct FakeKeychain {
        log: Rc<RefCell<Log>>,
        store: Rc<RefCell<HashMap<String, String>>>,
        mode: Mode,
        fail_delete: bool,
    }

    impl Keychain for FakeKeychain {
        fn get(&mut self, id: &str) -> std::result::Result<Secret, KeychainError> {
            if self.mode == Mode::Locked {
                return Err(KeychainError::Unavailable);
            }
            self.store.borrow().get(id).map(|s| Secret::new(s.clone())).ok_or(KeychainError::NotFound)
        }

        fn set(&mut self, id: &str, secret: &Secret) -> std::result::Result<(), KeychainError> {
            if self.mode == Mode::Locked {
                return Err(KeychainError::Unavailable);
            }
            self.log.borrow_mut().keychain.push(format!("set:{id}"));
            self.store.borrow_mut().insert(id.to_owned(), secret.expose().to_owned());
            Ok(())
        }

        fn delete(&mut self, id: &str) -> std::result::Result<(), KeychainError> {
            self.log.borrow_mut().keychain.push(format!("delete:{id}"));
            if self.fail_delete {
                return Err(KeychainError::Denied);
            }
            self.store.borrow_mut().remove(id);
            Ok(())
        }
    }

    struct FakeStore {
        log: Rc<RefCell<Log>>,
        fail: bool,
    }

    impl ConfigStore for FakeStore {
        fn load(&mut self) -> LoadedConfig {
            LoadedConfig { config: Config::default(), issues: Vec::new(), read_only: false }
        }

        fn save(&mut self, config: &Config) -> std::result::Result<(), String> {
            if self.fail {
                return Err("disk full".into());
            }
            self.log.borrow_mut().saves.push(config.clone());
            Ok(())
        }
    }

    struct Rig {
        s: Session,
        log: Rc<RefCell<Log>>,
        store: Rc<RefCell<HashMap<String, String>>>,
    }

    fn rig(mode: Mode, fail_save: bool, fail_delete: bool, link: bool) -> Rig {
        let log: Rc<RefCell<Log>> = Rc::default();
        let store: Rc<RefCell<HashMap<String, String>>> = Rc::default();
        let sink = Rc::clone(&log);
        let mut services = EngineServices {
            keychain: Some(Box::new(FakeKeychain { log: Rc::clone(&log), store: Rc::clone(&store), mode, fail_delete })),
            config_store: Some(Box::new(FakeStore { log: Rc::clone(&log), fail: fail_save })),
            ..EngineServices::default()
        };
        if link {
            services.daemon_link = Some(Box::new(move |id, cmd| {
                let text = match &cmd {
                    LinkCommand::Connect { password, .. } => format!("connect:{id}:pw={}", password.is_some()),
                    LinkCommand::Restart { password, .. } => format!("restart:{id}:pw={}", password.is_some()),
                    LinkCommand::Disconnect => format!("disconnect:{id}"),
                    LinkCommand::Remove => format!("remove:{id}"),
                };
                sink.borrow_mut().links.push(text);
                Ok(())
            }));
        }
        Rig { s: Session::new().with_services(services), log, store }
    }

    fn add(s: &mut Session, name: &str, port: u64) -> Result<Value> {
        s.execute("daemon.add", json!({"name": name, "host": "127.0.0.1", "port": port, "tls": false}))
    }

    fn id_of(r: &Value) -> String {
        r["entryId"].as_str().unwrap().to_owned()
    }

    #[test]
    fn invalid_fields_come_back_together_and_nothing_happens() {
        let mut r = rig(Mode::Ok, false, false, true);
        for (params, codes) in [
            (json!({"name": " ", "host": "", "port": 0, "webUiUrl": "ftp://x"}), vec!["invalid_name", "invalid_host", "invalid_port", "invalid_url"]),
            (json!({"name": "a", "host": "h", "port": 65536}), vec!["invalid_port"]),
            (json!({"name": "a", "host": "a/b", "port": 1, "password": "has space"}), vec!["invalid_host", "invalid_password"]),
        ] {
            let EngineError::BadParams(m) = r.s.execute("daemon.add", params).unwrap_err() else { panic!("not bad params") };
            let v: Value = serde_json::from_str(&m).unwrap();
            let got: Vec<&str> = v["errors"].as_array().unwrap().iter().map(|e| e["code"].as_str().unwrap()).collect();
            assert_eq!(got, codes);
        }
        add(&mut r.s, "Box", 1).unwrap();
        let EngineError::BadParams(m) = add(&mut r.s, "box", 2).unwrap_err() else { panic!("not bad params") };
        assert!(m.contains("duplicate_name"), "{m}");
        assert_eq!(r.s.config().daemons.len(), 1);
        assert_eq!(r.log.borrow().links.len(), 1, "only the valid add connected");
    }

    #[test]
    fn a_valid_add_persists_connects_and_leaks_no_password() {
        let mut r = rig(Mode::Ok, false, false, true);
        let out = r.s.execute("daemon.add", json!({"name": " Box ", "host": " [::1] ", "port": 6790, "password": "s3cret"})).unwrap();
        let id = id_of(&out);
        let cfg = r.s.config().daemons[0].clone();
        assert_eq!((cfg.name.as_str(), cfg.host.as_str()), ("Box", "::1"));
        assert_eq!(id.len(), 36);
        assert_eq!(r.log.borrow().saves.len(), 1);
        assert_eq!(r.log.borrow().links, [format!("connect:{id}:pw=true")]);
        assert_eq!(r.store.borrow().get(&id).map(String::as_str), Some("s3cret"));
        let view = r.s.execute("document.inspect", Value::Null).unwrap();
        let shown = format!("{:?}{}{}", r.s.config(), view, serde_json::to_string(&*r.s.snapshot()).unwrap());
        assert!(!shown.contains("s3cret"));
        assert_eq!(r.s.snapshot().daemons[0].name, "Box");
    }

    #[test]
    fn the_fifty_first_entry_is_refused() {
        let mut r = rig(Mode::Ok, false, false, false);
        for i in 0..50 {
            add(&mut r.s, &format!("d{i}"), 1000 + i).unwrap();
        }
        let EngineError::BadParams(m) = add(&mut r.s, "one-more", 5).unwrap_err() else { panic!("not bad params") };
        assert!(m.contains("too_many_daemons"));
    }

    #[test]
    fn edit_reconnects_only_when_the_address_changes_and_claims_auto_detected_entries() {
        let mut r = rig(Mode::Ok, false, false, true);
        let id = id_of(&r.s.execute("daemon.add", json!({"name": "Local", "host": "127.0.0.1", "port": 6767, "autoDetected": true})).unwrap());
        assert!(r.s.config().daemons[0].auto_detected);
        r.log.borrow_mut().links.clear();
        r.s.execute("daemon.edit", json!({"entryId": id, "name": "Renamed"})).unwrap();
        assert!(r.log.borrow().links.is_empty());
        assert!(!r.s.config().daemons[0].auto_detected);
        assert_eq!(r.s.snapshot().daemons[0].name, "Renamed");
        r.s.execute("daemon.edit", json!({"entryId": id, "port": 7000})).unwrap();
        assert_eq!(r.log.borrow().links, [format!("restart:{id}:pw=false")]);
        let e = r.s.execute("daemon.edit", json!({"entryId": "nope", "name": "x"})).unwrap_err();
        assert!(e.to_string().contains("unknown_entry"));
    }

    #[test]
    fn remove_disconnects_first_cleans_state_and_dismisses_auto_detection() {
        let mut r = rig(Mode::Ok, false, false, true);
        let id = id_of(&r.s.execute("daemon.add", json!({"name": "Local", "host": "h", "port": 1, "autoDetected": true, "password": "pw"})).unwrap());
        let events = json!({"events": [
            {"entryId": id, "serverId": "srv", "connEpoch": 1, "seq": 0, "at": 1, "kind": {"type": "hello", "info": {}, "caps": {}, "degraded": [], "tlsInUse": false, "isLoopback": true}},
            {"entryId": id, "connEpoch": 1, "seq": 1, "at": 2, "kind": {"type": "snapshot", "agents": [{"agentId": "a", "status": "idle"}], "uiPending": []}}
        ]});
        r.s.execute("feed.ingest", events).unwrap();
        assert_eq!(r.s.snapshot().agents.len(), 1);
        r.log.borrow_mut().links.clear();
        r.s.execute("daemon.remove", json!({"entryId": id})).unwrap();
        assert_eq!(r.log.borrow().links, [format!("remove:{id}")]);
        assert!(r.s.snapshot().agents.is_empty() && r.s.snapshot().daemons.is_empty());
        let last = r.log.borrow().saves.last().unwrap().clone();
        assert!(last.daemons.is_empty() && last.local_detection_dismissed);
        assert!(r.log.borrow().keychain.contains(&format!("delete:{id}")));
        assert!(r.store.borrow().is_empty());
        assert_eq!(r.s.password_flags(&id), (false, false, false));
    }

    #[test]
    fn a_failing_keychain_delete_still_removes_the_entry_with_a_toast() {
        let mut r = rig(Mode::Ok, false, true, true);
        let id = id_of(&add(&mut r.s, "Box", 1).unwrap());
        r.s.execute("daemon.remove", json!({"entryId": id})).unwrap();
        assert!(r.s.config().daemons.is_empty());
        assert!(r.s.take_toasts().iter().any(|t| t.text.contains("keychain")));
    }

    #[test]
    fn connect_is_a_no_op_when_live_and_restarts_after_an_auth_failure() {
        let mut r = rig(Mode::Ok, false, false, true);
        let id = id_of(&add(&mut r.s, "Box", 1).unwrap());
        r.log.borrow_mut().links.clear();
        let set_state = |s: &mut Session, state: &str| {
            let ev = json!({"events": [{"entryId": id, "connEpoch": 1, "seq": 0, "at": 1, "kind": {"type": "connState", "state": state, "since": 1}}]});
            s.execute("feed.ingest", ev).unwrap();
        };
        set_state(&mut r.s, "live");
        assert_eq!(r.s.execute("daemon.connect", json!({"entryId": id})).unwrap()["connecting"], false);
        assert!(r.log.borrow().links.is_empty());
        set_state(&mut r.s, "auth_failed");
        r.s.execute("daemon.connect", json!({"entryId": id})).unwrap();
        assert_eq!(r.log.borrow().links, [format!("restart:{id}:pw=false")]);
        let listed = r.s.execute("command.list", json!({"filter": "daemon.connect"})).unwrap();
        assert_eq!(listed[0]["enabled"], true);
        let mut cli = Session::new();
        let listed = cli.execute("command.list", json!({"filter": "daemon.connect"})).unwrap();
        assert_eq!((listed[0]["enabled"].clone(), listed[0]["reason"].clone()), (json!(false), json!("no live connection in the CLI")));
        assert!(add(&mut cli, "offline", 1).is_ok(), "add stays enabled without the link hook");
    }

    #[test]
    fn a_failing_save_keeps_memory_and_toasts_and_read_only_refuses() {
        let mut r = rig(Mode::Ok, true, false, false);
        add(&mut r.s, "Box", 1).unwrap();
        assert_eq!(r.s.config().daemons.len(), 1);
        assert!(r.s.take_toasts().iter().any(|t| t.text.contains("disk full")));
        let mut ro = rig(Mode::Ok, false, false, false);
        ro.s.apply_loaded_config(LoadedConfig { config: Config::default(), issues: Vec::new(), read_only: true });
        add(&mut ro.s, "Box", 1).unwrap();
        assert!(ro.log.borrow().saves.is_empty());
        assert!(ro.s.take_toasts().iter().any(|t| t.text.contains("newer version")));
    }

    #[test]
    fn group_by_saves_once_per_change_and_respects_read_only() {
        let mut r = rig(Mode::Ok, false, false, false);
        assert_eq!(r.s.config().settings.group_by, GroupBy::Workspace, "the default");
        r.s.set_group_by(GroupBy::Workspace);
        assert!(r.log.borrow().saves.is_empty(), "the default again is no change");
        r.s.set_group_by(GroupBy::None);
        r.s.set_group_by(GroupBy::None);
        assert_eq!(r.log.borrow().saves.len(), 1);
        assert_eq!(r.log.borrow().saves[0].settings.group_by, GroupBy::None);
        assert_eq!(r.s.config().settings.group_by, GroupBy::None);
        let mut ro = rig(Mode::Ok, false, false, false);
        ro.s.apply_loaded_config(LoadedConfig { config: Config::default(), issues: Vec::new(), read_only: true });
        ro.s.set_group_by(GroupBy::None);
        ro.s.set_group_by(GroupBy::Workspace);
        assert!(ro.log.borrow().saves.is_empty());
        assert_eq!(ro.s.config().settings.group_by, GroupBy::Workspace);
        assert_eq!(ro.s.take_toasts().iter().filter(|t| t.text.contains("newer version")).count(), 1);
    }

    #[test]
    fn zoom_saves_once_per_change() {
        let mut r = rig(Mode::Ok, false, false, false);
        r.s.set_zoom(100);
        assert!(r.log.borrow().saves.is_empty(), "the default again is no change");
        r.s.set_zoom(125);
        r.s.set_zoom(125);
        assert_eq!(r.log.borrow().saves.len(), 1);
        assert_eq!((r.log.borrow().saves[0].settings.zoom, r.s.config().settings.zoom), (125, 125));
    }

    #[test]
    fn set_password_remembers_in_the_keychain_or_falls_back_to_memory() {
        let mut r = rig(Mode::Ok, false, false, true);
        let id = id_of(&add(&mut r.s, "Box", 1).unwrap());
        let out = r.s.execute("daemon.setPassword", json!({"entryId": id, "password": "pw1", "remember": true})).unwrap();
        assert_eq!(out["warnings"], json!([]));
        assert_eq!(r.store.borrow().get(&id).map(String::as_str), Some("pw1"));
        assert_eq!(r.s.password_flags(&id), (true, false, false));
        assert_eq!(r.log.borrow().links.last().unwrap(), &format!("restart:{id}:pw=true"));
        let mut locked = rig(Mode::Locked, false, false, true);
        let id = id_of(&add(&mut locked.s, "Box", 1).unwrap());
        let out = locked.s.execute("daemon.setPassword", json!({"entryId": id, "password": "pw2", "remember": true})).unwrap();
        assert_eq!(out["warnings"], json!(["keychain_unavailable"]));
        assert_eq!(locked.s.password_flags(&id), (true, true, true));
        assert!(locked.log.borrow().saves.iter().all(|c| !format!("{c:?}").contains("pw2")));
        let first = r.s.config().daemons[0].entry_id.clone();
        let empty = r.s.execute("daemon.setPassword", json!({"entryId": first, "password": "", "remember": true})).unwrap();
        assert_eq!(empty["warnings"], json!([]));
        assert!(r.store.borrow().is_empty());
    }

    #[test]
    fn bad_passwords_are_refused_without_effect() {
        let mut r = rig(Mode::Ok, false, false, true);
        let id = id_of(&add(&mut r.s, "Box", 1).unwrap());
        r.log.borrow_mut().links.clear();
        for pw in ["x".repeat(1025), "no space".to_owned(), "é".to_owned()] {
            let e = r.s.execute("daemon.setPassword", json!({"entryId": id, "password": pw, "remember": true})).unwrap_err();
            assert!(e.to_string().contains("invalid_password"));
        }
        assert!(r.store.borrow().is_empty() && r.log.borrow().links.is_empty());
    }

    #[test]
    fn a_locked_keychain_connects_without_a_password_and_an_auth_failure_raises_the_prompt() {
        let mut r = rig(Mode::Locked, false, false, true);
        let id = id_of(&add(&mut r.s, "Box", 1).unwrap());
        assert_eq!(r.log.borrow().links, [format!("connect:{id}:pw=false")]);
        let ev = json!({"events": [{"entryId": id, "connEpoch": 1, "seq": 0, "at": 1, "kind": {"type": "connState", "state": "auth_failed", "since": 1}}]});
        r.s.execute("feed.ingest", ev).unwrap();
        assert_eq!(r.s.take_prompts(), [PasswordPrompt { entry_id: id.clone(), keychain_unavailable: true }]);
        assert!(r.s.take_prompts().is_empty());
        r.s.execute("daemon.setPassword", json!({"entryId": id, "password": "ok", "remember": false})).unwrap();
        assert_eq!(r.log.borrow().links.last().unwrap(), &format!("restart:{id}:pw=true"));
        assert!(r.log.borrow().saves.iter().all(|c| !format!("{c:?}").contains("ok\"")));
    }

    #[test]
    fn hello_remembers_the_server_id_in_the_config() {
        let mut r = rig(Mode::Ok, false, false, false);
        let id = id_of(&add(&mut r.s, "Box", 1).unwrap());
        let ev = json!({"events": [{"entryId": id, "serverId": "srv-9", "connEpoch": 1, "seq": 0, "at": 1,
            "kind": {"type": "hello", "info": {}, "caps": {}, "degraded": [], "tlsInUse": false, "isLoopback": true}}]});
        r.s.execute("feed.ingest", ev).unwrap();
        assert_eq!(r.s.config().daemons[0].last_seen_server_id.as_deref(), Some("srv-9"));
        assert_eq!(r.log.borrow().saves.len(), 2);
    }

    #[test]
    fn secrets_never_print_and_params_are_redacted() {
        let s = Secret::new("hunter2");
        assert_eq!((format!("{s}"), format!("{s:?}")), ("***".to_owned(), "***".to_owned()));
        let call = json!({"command": "daemon.add", "params": {"name": "x", "password": "hunter2"}});
        let shown = redact_request("engine.execute", &call).to_string();
        assert!(!shown.contains("hunter2") && shown.contains("***") && shown.contains("\"name\":\"x\""));
        let other = json!({"command": "view.setTheme", "params": {"password": "kept"}});
        assert!(redact_request("engine.execute", &other).to_string().contains("kept"));
        let set = redact_params("daemon.setPassword", &json!({"entryId": "e", "password": "p", "remember": true}));
        assert_eq!(set["password"], "***");
    }

    #[test]
    fn connect_all_looks_up_each_password_separately_and_skips_linked_entries() {
        let mut r = rig(Mode::Ok, false, false, true);
        let entry = |id: &str, port: u16| DaemonEntry {
            entry_id: id.into(),
            name: id.into(),
            host: "h".into(),
            port,
            tls: false,
            web_ui_url: None,
            last_seen_server_id: None,
            auto_detected: false,
            extra: Default::default(),
        };
        let config = Config { daemons: vec![entry("a", 1), entry("b", 2)], ..Config::default() };
        r.s.apply_loaded_config(LoadedConfig { config, issues: Vec::new(), read_only: false });
        r.store.borrow_mut().insert("b".into(), "pwb".into());
        assert_eq!(r.s.connect_all(), 2);
        assert_eq!(r.log.borrow().links, ["connect:a:pw=false", "connect:b:pw=true"]);
        assert_eq!(r.s.connect_all(), 0, "already linked");
    }
}
