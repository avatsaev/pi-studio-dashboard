//! The control protocol: what a frontend or automation can ask of a running app. Transport-free:
//! the desktop app feeds it from the webview and from a loopback socket, tests and the `view`
//! example call [`handle`] directly.
//!
//! - `engine.execute {command, params?}`: run any command by id
//! - `engine.commands`: the registry, with what is enabled now
//! - `ui.inspect`: the UI state, the monitor view (`document`) and the menu bar: exactly what the
//!   frontend draws
//! - `ui.set {theme?, filterDaemons?, filterStatuses?, attentionRailCollapsed?, focusItem?,
//!   gridWindow?, dialog?, diagnosticsTarget?, groupBy?, zoom?}`: validate every field, then apply all; an
//!   unknown field, a bad value or a read-only field (`toasts`, `status`) is an error naming it
//!   and nothing changes

use std::sync::mpsc::{Receiver, Sender, channel};

use serde_json::{Value, json};

use crate::state::GridWindow;
use crate::{AppShell, fields, menus};

pub const METHODS: &[&str] = &["engine.commands", "engine.execute", "ui.inspect", "ui.set"];

/// What can be asked of the shell.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// A control-protocol call. The only kind automation can send.
    Control { method: String, params: Value },
    /// A menu click or a shortcut in the frontend: see [`menus::invoke`]. It may open a native
    /// dialog, so it is not part of the control protocol.
    Invoke { command: String },
    /// The app's own plumbing (the feed pump, the I/O worker): an engine command run without
    /// touching `ui.status`. Not reachable from the webview or the control socket.
    Internal { command: String, params: Value },
}

/// One request on its way to the thread that owns the shell, with the channel its reply goes
/// back on.
pub struct Envelope {
    pub request: Request,
    pub reply: Sender<Value>,
}

impl Envelope {
    pub fn new(request: Request) -> (Self, Receiver<Value>) {
        let (reply, rx) = channel();
        (Self { request, reply }, rx)
    }
}

/// `{"ok": true, "result": …}` or `{"ok": false, "error": "…"}`.
pub fn reply(result: Result<Value, String>) -> Value {
    match result {
        Ok(result) => json!({"ok": true, "result": result}),
        Err(error) => json!({"ok": false, "error": error}),
    }
}

/// Answer one control-protocol call.
pub fn handle(app: &mut AppShell, method: &str, params: &Value) -> Value {
    reply(match method {
        "engine.execute" => execute(app, params),
        "engine.commands" => commands(app),
        "ui.inspect" => Ok(view(app)),
        "ui.set" => set(app, params),
        other => Err(format!("unknown method `{other}`; methods: {}", METHODS.join(", "))),
    })
}

fn execute(app: &mut AppShell, params: &Value) -> Result<Value, String> {
    let id = params.get("command").and_then(Value::as_str).ok_or("missing `command`")?;
    app.run(id, params.get("params").cloned().unwrap_or(Value::Null))
}

/// Everything the frontend draws, and everything `ui.inspect` returns: what a person sees is
/// what an agent reads.
pub fn view(app: &mut AppShell) -> Value {
    let shell: Vec<Value> = menus::UI_COMMANDS.iter().map(|c| shell_row(app, c)).collect();
    let inspect = json!({
        "filterDaemons": app.ui.filter_daemons,
        "filterStatuses": app.ui.filter_statuses,
        "gridWindow": app.ui.grid_window,
        "railCollapsed": app.ui.attention_rail_collapsed,
        "groupBy": app.ui.group_by,
    });
    let mut document = app.session.execute("document.inspect", inspect).ok();
    // The diagnostics of the open panel: `null` when none is open or its daemon is gone.
    if let (Some(doc), Some(target)) = (document.as_mut().and_then(Value::as_object_mut), app.ui.diagnostics_target.clone()) {
        let data = app.session.execute("daemon.diagnostics", json!({"entryId": target})).unwrap_or(Value::Null);
        doc.insert("diagnostics".into(), data);
    }
    json!({
        "ui": app.ui,
        "document": document,
        "menus": menus::model(app),
        "shellCommands": shell,
    })
}

fn shell_row(app: &AppShell, c: &menus::ShellCommand) -> Value {
    let enabled = (c.enabled)(app);
    json!({
        "id": c.id,
        "label": c.label,
        "menu": c.menu.map(|m| vec![m]).unwrap_or_default(),
        "shortcut": c.shortcut,
        "internal": false,
        "params": c.params,
        "enabled": enabled.is_ok(),
        "reason": enabled.err(),
        "layer": "shell",
    })
}

/// The engine registry plus the shell commands, each row flagged with its `layer`.
fn commands(app: &mut AppShell) -> Result<Value, String> {
    let mut rows = app.run("command.list", Value::Null)?;
    if let Some(list) = rows.as_array_mut() {
        for row in list.iter_mut() {
            if let Some(o) = row.as_object_mut() {
                o.insert("layer".into(), json!("engine"));
            }
        }
        list.extend(menus::UI_COMMANDS.iter().map(|c| shell_row(app, c)));
    }
    Ok(rows)
}

/// Validate every field, then apply them all: a bad request changes nothing.
fn set(app: &mut AppShell, params: &Value) -> Result<Value, String> {
    let requested = params.as_object().ok_or("`ui.set` takes a JSON object")?;
    let mut next = app.ui.clone();
    for (key, value) in requested {
        fields::apply(&mut next, key, value)?;
    }
    let group_by = next.group_by;
    // A changed layout starts at the top, unless the request also says where to look.
    if group_by != app.ui.group_by && !requested.contains_key("gridWindow") {
        next.grid_window = GridWindow::default();
    }
    app.ui = next;
    // `groupBy` is a saved setting: a change through here is stored like one from the menu.
    if requested.contains_key("groupBy") {
        app.session.set_group_by(group_by);
    }
    // `zoom` is saved too (and applied to the window by the app).
    if requested.contains_key("zoom") {
        app.session.set_zoom(app.ui.zoom);
    }
    Ok(Value::Null)
}
