//! The menu bar as data, generated from the command registry and the shell commands: a command
//! with a `menu` shows up here with its label, shortcut and enabled state, with no frontend
//! code of its own.

use pistudiodashboard_engine::command_specs;
use serde::Serialize;
use serde_json::{Value, json};

use crate::shortcuts::{self, Shortcut};
use crate::state::{Dialog, FormMode, ThemeKind};
use crate::{AppShell, fields, zoom};

pub const MENU_ORDER: &[&str] = &["Daemon", "Agent", "View"];

fn all_ok(_: &AppShell) -> Result<(), String> {
    Ok(())
}

fn detect_enabled(app: &AppShell) -> Result<(), String> {
    if app.services.detect_local_daemon.is_none() {
        return Err("local daemon detection is not available here".into());
    }
    if app.session.config().daemons.iter().any(|d| d.auto_detected) {
        return Err("a detected local daemon is already configured".into());
    }
    Ok(())
}

/// Run the `detect_local_daemon` hook, then `daemon.add` with the result.
fn detect_local(app: &mut AppShell, _: &Value) -> Result<(), String> {
    let found = app.services.detect_local_daemon.as_mut().map(|detect| detect()).ok_or("local daemon detection is not available here")?;
    let params = json!({"name": found.name, "host": found.host, "port": found.port, "tls": found.tls, "webUiUrl": found.web_ui_url, "autoDetected": true});
    app.session.execute("daemon.add", params).map(|_| ()).map_err(|e| e.to_string())
}

fn entry_exists(app: &AppShell) -> Result<(), String> {
    if app.session.config().daemons.is_empty() { Err("no daemon is configured".into()) } else { Ok(()) }
}

/// `daemon.showDiagnostics {entryId}`: open the diagnostics panel of one daemon. An unknown entry is an error and changes nothing.
fn show_diagnostics(app: &mut AppShell, p: &Value) -> Result<(), String> {
    let id = p.get("entryId").and_then(Value::as_str).ok_or("`entryId` is required")?;
    if app.session.snapshot().daemon(id).is_none() {
        let shown: String = id.chars().take(64).collect();
        return Err(format!("unknown_entry: no daemon entry `{shown}`"));
    }
    app.ui.diagnostics_target = Some(id.to_owned());
    Ok(())
}

/// The daemon a menu item or a shortcut acts on (it carries no params): the one daemon selected
/// in the filter, else the only daemon. A chip's own menu names its entry instead.
fn menu_target(app: &AppShell) -> Result<String, String> {
    let snapshot = app.session.snapshot();
    let selected: Vec<&str> =
        snapshot.daemons.iter().filter(|d| d.server_id.as_ref().is_some_and(|s| app.ui.filter_daemons.contains(s))).map(|d| d.entry_id.as_str()).collect();
    match (selected.as_slice(), snapshot.daemons.as_slice()) {
        ([one], _) => Ok((*one).to_owned()),
        ([], [only]) => Ok(only.entry_id.clone()),
        ([], []) => Err("no daemon is configured".into()),
        _ => Err("several daemons: use the menu on a daemon chip".into()),
    }
}

/// One param-bound menu entry of a shell command (`view.setTheme:dark`).
pub struct Bound {
    pub value: &'static str,
    pub label: &'static str,
}

/// A command that changes only view state. These live in the shell, not the engine: they
/// touch no monitor state and mean nothing to the CLI.
pub struct ShellCommand {
    pub id: &'static str,
    pub label: &'static str,
    /// Top-level menu, or `None` for a command without a menu entry.
    pub menu: Option<&'static str>,
    pub shortcut: Option<&'static str>,
    /// What agents and scripts read: the params.
    pub params: &'static str,
    /// `Err(reason)` when the command cannot run now (the menu item's tooltip).
    pub enabled: fn(&AppShell) -> Result<(), String>,
    pub run: fn(&mut AppShell, &Value) -> Result<(), String>,
    /// With `bind_key`: one menu entry per value, running the command with `{bind_key: value}`.
    pub bind_key: &'static str,
    pub bound: &'static [Bound],
    /// Check state of a bound entry (`value`) or of the command itself (`None`); `None` = no check.
    pub checked: fn(&AppShell, Option<&str>) -> Option<bool>,
}

/// Apply `{param: v}` as the `UiState` field `field`.
fn set_field(app: &mut AppShell, p: &Value, param: &str, field: &str) -> Result<(), String> {
    let v = p.get(param).ok_or_else(|| format!("`{param}` is required"))?;
    fields::apply(&mut app.ui, field, v)
}

/// The group commands only mean something while the grid is grouped.
fn grouped(app: &AppShell) -> Result<(), String> {
    if app.ui.group_by == pistudiodashboard_engine::GroupBy::Workspace { Ok(()) } else { Err("the agents are not grouped by workspace".to_owned()) }
}

fn no_check(_: &AppShell, _: Option<&str>) -> Option<bool> {
    None
}

pub const UI_COMMANDS: &[ShellCommand] = &[
    ShellCommand {
        id: "view.setTheme",
        label: "Theme",
        menu: Some("View"),
        shortcut: None,
        params: r#"{"theme": "dark" | "light" | "system"}"#,
        enabled: all_ok,
        run: |app, p| set_field(app, p, "theme", "theme"),
        bind_key: "theme",
        bound: &[
            Bound { value: "dark", label: "Theme: Dark" },
            Bound { value: "light", label: "Theme: Light" },
            Bound { value: "system", label: "Theme: System" },
        ],
        checked: |app, value| value.and_then(ThemeKind::from_name).map(|t| t == app.ui.theme),
    },
    ShellCommand {
        id: "view.filterDaemons",
        label: "Filter Daemons",
        menu: None,
        shortcut: None,
        params: r#"{"serverIds": string[] /* <= 50, empty = all */}"#,
        enabled: all_ok,
        run: |app, p| set_field(app, p, "serverIds", "filterDaemons"),
        bind_key: "",
        bound: &[],
        checked: no_check,
    },
    ShellCommand {
        id: "view.setGroupBy",
        label: "Group Agents",
        menu: Some("View"),
        shortcut: None,
        params: r#"{"groupBy": "none" | "workspace"}"#,
        enabled: all_ok,
        run: |app, p| {
            let value = p.get("groupBy").ok_or("`groupBy` is required")?;
            app.set_group_by(fields::group_by(value)?);
            Ok(())
        },
        bind_key: "groupBy",
        bound: &[Bound { value: "none", label: "Group: None" }, Bound { value: "workspace", label: "Group: By Workspace" }],
        checked: |app, value| value.map(|v| v == app.ui.group_by.name()),
    },
    ShellCommand {
        id: "view.toggleGroup",
        label: "Collapse or Expand Group",
        menu: None,
        shortcut: None,
        params: r#"{"key": string /* a `groups.window[].key` of the view */}"#,
        enabled: grouped,
        run: |app, p| {
            let key = p.get("key").and_then(Value::as_str).filter(|k| !k.is_empty()).ok_or("`key` is a group key (a string)")?;
            let mut next = app.ui.collapsed_groups.clone();
            match next.iter().position(|k| k == key) {
                Some(at) => {
                    next.remove(at);
                }
                None => next.push(key.to_owned()),
            }
            app.ui.collapsed_groups = fields::collapsed_groups(&json!(next))?;
            Ok(())
        },
        bind_key: "",
        bound: &[],
        checked: no_check,
    },
    ShellCommand {
        id: "view.collapseAllGroups",
        label: "Collapse All Groups",
        menu: Some("View"),
        shortcut: None,
        params: "{}",
        enabled: grouped,
        run: |app, _| {
            let mut params = crate::control::inspect_params(app);
            params["includeGroupKeys"] = json!(true);
            let view = app.session.execute("document.inspect", params).map_err(|e| e.to_string())?;
            let keys = view.pointer("/groups/keys").cloned().ok_or("there are no groups to collapse")?;
            app.ui.collapsed_groups = fields::collapsed_groups(&keys).map_err(|_| "too many groups to collapse them all".to_owned())?;
            Ok(())
        },
        bind_key: "",
        bound: &[],
        checked: no_check,
    },
    ShellCommand {
        id: "view.expandAllGroups",
        label: "Expand All Groups",
        menu: Some("View"),
        shortcut: None,
        params: "{}",
        enabled: |app| {
            grouped(app)?;
            if app.ui.collapsed_groups.is_empty() { Err("no group is collapsed".to_owned()) } else { Ok(()) }
        },
        run: |app, _| {
            app.ui.collapsed_groups.clear();
            Ok(())
        },
        bind_key: "",
        bound: &[],
        checked: no_check,
    },
    ShellCommand {
        id: "view.zoomIn",
        label: "Zoom In",
        menu: Some("View"),
        shortcut: Some("Cmd+="),
        params: "{}",
        enabled: |app| zoom::zoom_in(app.ui.zoom).map(|_| ()).ok_or_else(|| "already at the largest size".to_owned()),
        run: |app, _| {
            let next = zoom::zoom_in(app.ui.zoom).ok_or("already at the largest size")?;
            app.set_zoom(next);
            Ok(())
        },
        bind_key: "",
        bound: &[],
        checked: no_check,
    },
    ShellCommand {
        id: "view.zoomOut",
        label: "Zoom Out",
        menu: Some("View"),
        shortcut: Some("Cmd+-"),
        params: "{}",
        enabled: |app| zoom::zoom_out(app.ui.zoom).map(|_| ()).ok_or_else(|| "already at the smallest size".to_owned()),
        run: |app, _| {
            let next = zoom::zoom_out(app.ui.zoom).ok_or("already at the smallest size")?;
            app.set_zoom(next);
            Ok(())
        },
        bind_key: "",
        bound: &[],
        checked: no_check,
    },
    ShellCommand {
        id: "view.zoomReset",
        label: "Actual Size",
        menu: Some("View"),
        shortcut: Some("Cmd+0"),
        params: "{}",
        enabled: |app| if app.ui.zoom == zoom::DEFAULT { Err("already at 100%".to_owned()) } else { Ok(()) },
        run: |app, _| {
            app.set_zoom(zoom::DEFAULT);
            Ok(())
        },
        bind_key: "",
        bound: &[],
        checked: no_check,
    },
    ShellCommand {
        id: "view.filterStatuses",
        label: "Filter Statuses",
        menu: None,
        shortcut: None,
        params: r#"{"statuses": ("initializing"|"idle"|"running"|"error"|"closed"|"unknown")[] /* empty = all */}"#,
        enabled: all_ok,
        run: |app, p| set_field(app, p, "statuses", "filterStatuses"),
        bind_key: "",
        bound: &[],
        checked: no_check,
    },
    ShellCommand {
        id: "view.toggleAttentionRail",
        label: "Toggle Attention Rail",
        menu: Some("View"),
        shortcut: Some("Cmd+Shift+A"),
        params: "{}",
        enabled: all_ok,
        run: |app, _| {
            app.ui.attention_rail_collapsed = !app.ui.attention_rail_collapsed;
            Ok(())
        },
        bind_key: "",
        bound: &[],
        checked: no_check,
    },
    ShellCommand {
        id: "view.setGridWindow",
        label: "Set Grid Window",
        menu: None,
        shortcut: None,
        params: r#"{"start": n, "count": n /* <= 1000 */}"#,
        enabled: all_ok,
        run: |app, p| fields::apply(&mut app.ui, "gridWindow", p),
        bind_key: "",
        bound: &[],
        checked: no_check,
    },
    ShellCommand {
        id: "daemon.detectLocal",
        label: "Detect Local Daemon",
        menu: Some("Daemon"),
        shortcut: None,
        params: "{}",
        enabled: detect_enabled,
        run: detect_local,
        bind_key: "",
        bound: &[],
        checked: no_check,
    },
    ShellCommand {
        id: "daemon.showDiagnostics",
        label: "Daemon Diagnostics",
        menu: Some("Daemon"),
        shortcut: Some("Cmd+I"),
        params: r#"{"entryId": string}"#,
        enabled: entry_exists,
        run: show_diagnostics,
        bind_key: "",
        bound: &[],
        checked: no_check,
    },
];

pub fn shell_command(id: &str) -> Option<&'static ShellCommand> {
    UI_COMMANDS.iter().find(|c| c.id == id)
}

/// What a menu click or a shortcut does. An id `<command>:<value>` runs a param-bound entry of
/// a shell command with `{bind_key: value}`; anything else runs with default params. A value the
/// user types into the frontend travels as params of `engine.execute` instead.
pub fn invoke(app: &mut AppShell, id: &str) -> Result<Value, String> {
    if let Some((command, value)) = id.split_once(':')
        && let Some(spec) = shell_command(command).filter(|s| s.bound.iter().any(|b| b.value == value))
    {
        return app.run(command, json!({ spec.bind_key: value }));
    }
    // These need an entry (or a form) before they can run: the menu opens the dialog or targets one daemon.
    let dialog = match id {
        "daemon.add" => Some(Ok(Dialog::DaemonForm { mode: FormMode::Add, entry_id: None })),
        "daemon.edit" => Some(menu_target(app).map(|entry_id| Dialog::DaemonForm { mode: FormMode::Edit, entry_id: Some(entry_id) })),
        "daemon.remove" => Some(menu_target(app).map(|entry_id| Dialog::RemoveDaemon { entry_id })),
        _ => None,
    };
    if let Some(dialog) = dialog {
        let outcome = dialog.map(|d| app.ui.dialog = Some(d));
        app.ui.status = outcome.as_ref().err().cloned().unwrap_or_default();
        return outcome.map(|()| Value::Null);
    }
    if id == "daemon.showDiagnostics" {
        return menu_target(app).and_then(|entry_id| app.run(id, json!({ "entryId": entry_id })));
    }
    app.run(id, Value::Null)
}

/// One entry of a menu, as the frontend draws it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MenuItem {
    /// A command id, or `<command>:<value>` for a param-bound entry; pass it unchanged to
    /// `invoke_command`.
    pub id: String,
    pub label: String,
    pub shortcut: Option<Shortcut>,
    pub enabled: bool,
    /// Why the item is disabled: its tooltip.
    pub reason: Option<String>,
    /// `Some` for a shell entry that shows a check.
    pub checked: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Menu {
    pub label: &'static str,
    pub items: Vec<MenuItem>,
}

/// The whole menu bar for the current state.
pub fn model(app: &AppShell) -> Vec<Menu> {
    MENU_ORDER.iter().map(|top| Menu { label: top, items: items(app, top) }).collect()
}

fn shell_items(app: &AppShell, c: &ShellCommand) -> Vec<MenuItem> {
    let enabled = (c.enabled)(app);
    let item = |id: String, label: &str, checked: Option<bool>| MenuItem {
        id,
        label: label.to_owned(),
        shortcut: c.shortcut.and_then(shortcuts::parse),
        enabled: enabled.is_ok(),
        reason: enabled.clone().err(),
        checked,
    };
    if c.bound.is_empty() {
        return vec![item(c.id.to_owned(), c.label, (c.checked)(app, None))];
    }
    c.bound.iter().map(|b| item(format!("{}:{}", c.id, b.value), b.label, (c.checked)(app, Some(b.value)))).collect()
}

fn items(app: &AppShell, top: &str) -> Vec<MenuItem> {
    let commands = command_specs().iter().filter(|c| c.menu.first().is_some_and(|m| *m == top)).map(|spec| {
        let enabled = (spec.enabled)(&app.session);
        MenuItem {
            id: spec.id.to_owned(),
            label: spec.label.to_owned(),
            shortcut: spec.shortcut.and_then(shortcuts::parse),
            enabled: enabled.is_ok(),
            reason: enabled.err(),
            checked: None,
        }
    });
    let shell = UI_COMMANDS.iter().filter(|c| c.menu == Some(top)).flat_map(|c| shell_items(app, c));
    commands.chain(shell).collect()
}
