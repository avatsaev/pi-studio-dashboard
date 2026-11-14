//! Read-only commands: what agents call first to learn what exists and what state it is in.

use serde_json::Value;

use crate::commands::{CommandSpec, always, command_specs, list_entry};
use crate::{Result, Session, params, view};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "command.list",
            label: "List Commands",
            menu: &[],
            shortcut: None,
            internal: false,
            params: r#"{"filter": string = ""}"#,
            enabled: always,
            run: command_list,
        },
        CommandSpec {
            id: "document.inspect",
            label: "Inspect Monitor",
            menu: &[],
            shortcut: None,
            internal: false,
            params: r#"{"filterDaemons": string[] /*serverIds*/ = [], "filterStatuses": string[] = [], "gridWindow": {"start": n, "count": n} = all, "railCollapsed": bool = false}"#,
            enabled: always,
            run: inspect,
        },
    ]
}

fn command_list(s: &mut Session, p: &Value) -> Result<Value> {
    let needle = params::str_param(p, "filter")?.unwrap_or("").to_lowercase();
    let rows = command_specs().iter().filter(|c| c.id.to_lowercase().contains(&needle) || c.label.to_lowercase().contains(&needle));
    Ok(Value::Array(rows.map(|c| list_entry(s, c)).collect()))
}

fn inspect(s: &mut Session, p: &Value) -> Result<Value> {
    view::monitor_view(s, p)
}
