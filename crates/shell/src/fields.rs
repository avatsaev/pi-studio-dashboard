//! One validator per `UiState` field, shared by `ui.set` and the `view.*` commands, so a value
//! is accepted or refused identically whichever door it comes through.

use pistudiodashboard_engine::{GroupBy, ZOOM_MAX, ZOOM_MIN};
use pistudiodashboard_model::AgentStatus;
use serde_json::Value;

use crate::state::{Dialog, GridWindow, ThemeKind, UiState};

const MAX_FILTER_DAEMONS: usize = 50;
const MAX_ID: usize = 128;
const MAX_GRID_COUNT: u64 = 1000;
const STATUS_WORDS: [(&str, AgentStatus); 6] = [
    ("initializing", AgentStatus::Initializing),
    ("idle", AgentStatus::Idle),
    ("running", AgentStatus::Running),
    ("error", AgentStatus::Error),
    ("closed", AgentStatus::Closed),
    ("unknown", AgentStatus::Unknown),
];

fn id_ok(s: &str) -> bool {
    !s.is_empty() && s.chars().count() <= MAX_ID
}

fn theme(v: &Value) -> Result<ThemeKind, String> {
    v.as_str().and_then(ThemeKind::from_name).ok_or_else(|| "`theme` is `dark`, `light` or `system`".to_owned())
}

fn filter_daemons(v: &Value) -> Result<Vec<String>, String> {
    let bad = || format!("`filterDaemons` is an array of at most {MAX_FILTER_DAEMONS} serverIds of 1-{MAX_ID} characters");
    let items = v.as_array().filter(|a| a.len() <= MAX_FILTER_DAEMONS).ok_or_else(bad)?;
    items.iter().map(|i| i.as_str().filter(|s| id_ok(s)).map(str::to_owned).ok_or_else(bad)).collect()
}

fn filter_statuses(v: &Value) -> Result<Vec<AgentStatus>, String> {
    let words: Vec<&str> = STATUS_WORDS.iter().map(|(w, _)| *w).collect();
    let bad = || format!("`filterStatuses` is an array of distinct status words: {}", words.join(", "));
    let items = v.as_array().ok_or_else(bad)?;
    let mut out: Vec<AgentStatus> = Vec::new();
    for item in items {
        let status = item.as_str().and_then(|w| STATUS_WORDS.iter().find(|(n, _)| *n == w)).map(|(_, s)| *s).ok_or_else(bad)?;
        if out.contains(&status) {
            return Err(bad());
        }
        out.push(status);
    }
    Ok(out)
}

fn grid_window(v: &Value) -> Result<GridWindow, String> {
    let bad = || format!("`gridWindow` is {{start, count}} of non-negative integers with count <= {MAX_GRID_COUNT}");
    let field = |k: &str| v.get(k).and_then(Value::as_u64).ok_or_else(bad);
    let (start, count) = (field("start")?, field("count")?);
    if !v.is_object() || count > MAX_GRID_COUNT {
        return Err(bad());
    }
    Ok(GridWindow { start: u32::try_from(start).map_err(|_| bad())?, count: u32::try_from(count).map_err(|_| bad())? })
}

fn dialog(v: &Value) -> Result<Option<Dialog>, String> {
    if v.is_null() {
        return Ok(None);
    }
    let d: Dialog = serde_json::from_value(v.clone()).map_err(|e| format!("`dialog` is null or a dialog object: {e}"))?;
    let ids: Vec<&str> = match &d {
        Dialog::DaemonForm { entry_id, .. } => entry_id.iter().map(String::as_str).collect(),
        Dialog::Password { entry_id } | Dialog::RemoveDaemon { entry_id } => vec![entry_id],
        Dialog::ConfirmInterrupt { server_id, agent_id } => vec![server_id, agent_id],
    };
    if ids.iter().all(|i| id_ok(i)) { Ok(Some(d)) } else { Err(format!("`dialog` ids are 1-{MAX_ID} characters")) }
}

fn diagnostics_target(v: &Value) -> Result<Option<String>, String> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) if id_ok(s) => Ok(Some(s.clone())),
        _ => Err(format!("`diagnosticsTarget` is null or a string of 1-{MAX_ID} characters")),
    }
}

/// `groupBy`: `none` or `workspace`.
pub(crate) fn group_by(v: &Value) -> Result<GroupBy, String> {
    v.as_str().and_then(GroupBy::from_name).ok_or_else(|| "`groupBy` is one of: none, workspace".to_owned())
}

/// `zoom`: a whole percent from the smallest to the largest allowed.
pub(crate) fn zoom(v: &Value) -> Result<u16, String> {
    v.as_u64()
        .and_then(|z| u16::try_from(z).ok())
        .filter(|z| (ZOOM_MIN..=ZOOM_MAX).contains(z))
        .ok_or_else(|| format!("`zoom` is a whole percent from {ZOOM_MIN} to {ZOOM_MAX}"))
}

/// Validate and apply one `UiState` field by its wire name. On `Err` the state is unchanged.
pub(crate) fn apply(ui: &mut UiState, key: &str, v: &Value) -> Result<(), String> {
    match key {
        "theme" => ui.theme = theme(v)?,
        "filterDaemons" => ui.filter_daemons = filter_daemons(v)?,
        "filterStatuses" => ui.filter_statuses = filter_statuses(v)?,
        "attentionRailCollapsed" => ui.attention_rail_collapsed = v.as_bool().ok_or("`attentionRailCollapsed` must be a boolean")?,
        "groupBy" => ui.group_by = group_by(v)?,
        "zoom" => ui.zoom = zoom(v)?,
        "focusItem" if v.is_null() => ui.focus_item = None,
        "focusItem" => return Err("`focusItem` can only be cleared here (null); use `window.focusItem` to set it".into()),
        "gridWindow" => ui.grid_window = grid_window(v)?,
        "dialog" => ui.dialog = dialog(v)?,
        "diagnosticsTarget" => ui.diagnostics_target = diagnostics_target(v)?,
        "toasts" | "status" => return Err(format!("`{key}` is read-only")),
        other => return Err(format!("unknown field `{other}`")),
    }
    Ok(())
}
