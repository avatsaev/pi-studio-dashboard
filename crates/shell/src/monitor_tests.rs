//! The monitor view as the shell serves it: filters and the grid window set through the same
//! commands the webview sends, and the Esc shortcut of the drill-down.

use pistudiodashboard_engine::Session;
use pistudiodashboard_model::{AgentEntry, AgentStatus, DaemonSlice, MonitorState};
use serde_json::{Value, json};

use crate::{AppShell, Services, control};

fn app_with(n: usize) -> AppShell {
    let mut m = MonitorState::default();
    for (entry, sid) in [("e1", "s1"), ("e2", "s2")] {
        let mut d = DaemonSlice::new(entry, entry);
        d.server_id = Some(sid.into());
        m.daemons.push(d);
    }
    for i in 0..n {
        let status = if i % 3 == 0 { AgentStatus::Running } else { AgentStatus::Idle };
        m.upsert_agent(AgentEntry { server_id: if i % 2 == 0 { "s1" } else { "s2" }.into(), agent_id: format!("a{i:04}"), status, ..AgentEntry::default() });
    }
    AppShell::new(Session::with_monitor(m), Services::default())
}

fn cards(view: &Value) -> &Vec<Value> {
    view["document"]["cards"]["window"].as_array().unwrap()
}

#[test]
fn the_reported_grid_window_decides_which_cards_the_view_carries() {
    let mut app = app_with(500);
    let view = control::view(&mut app);
    assert_eq!((view["document"]["cards"]["total"].clone(), view["document"]["cards"]["virtualised"].clone()), (json!(500), json!(true)));
    assert_eq!(cards(&view).len(), 120, "the default window 0..100 plus 20 overscan");
    app.run("view.setGridWindow", json!({"start": 300, "count": 30})).unwrap();
    let view = control::view(&mut app);
    assert_eq!((cards(&view).len(), view["document"]["cards"]["start"].clone()), (70, json!(280)));
    app.run("view.setGridWindow", json!({"start": 0, "count": 1000})).unwrap();
    assert_eq!(cards(&control::view(&mut app)).len(), 200, "never more than 200 cards in one view");
    assert!(app.run("view.setGridWindow", json!({"start": 0, "count": 1001})).is_err());
}

#[test]
fn both_filter_rows_narrow_cards_totals_and_counts() {
    let mut app = app_with(12);
    app.run("view.filterDaemons", json!({"serverIds": ["s1"]})).unwrap();
    app.run("view.filterStatuses", json!({"statuses": ["running"]})).unwrap();
    let view = control::view(&mut app);
    let doc = &view["document"];
    // s1 holds the even agents; running ones are multiples of 3: 0, 6 -> 2 cards.
    assert_eq!(doc["cards"]["total"], 2);
    assert_eq!(doc["totals"]["label"], "0 of 2 agents");
    assert_eq!(doc["statusChips"], json!([{"bucket": "running", "count": 2, "selected": true}, {"bucket": "idle", "count": 4, "selected": false}]));
    assert_eq!(doc["daemonChips"][0]["selected"], true);
    assert_eq!(doc["daemonChips"][1]["selected"], false);
    app.run("view.filterDaemons", json!({"serverIds": []})).unwrap();
    app.run("view.filterStatuses", json!({"statuses": []})).unwrap();
    let all = control::view(&mut app);
    assert_eq!((all["document"]["cards"]["total"].clone(), all["document"]["totals"]["label"].clone()), (json!(12), json!("0 of 12 agents")));
}

#[test]
fn esc_is_the_close_shortcut_of_the_drill_and_enabled_only_while_one_is_open() {
    let mut app = app_with(2);
    let row = |app: &mut AppShell| {
        let view = control::view(app);
        let found = view["menus"].as_array().unwrap().iter().flat_map(|m| m["items"].as_array().unwrap().clone()).find(|i| i["id"] == "agent.deselect");
        found.unwrap()
    };
    let closed = row(&mut app);
    assert_eq!((closed["shortcut"]["key"].clone(), closed["enabled"].clone()), (json!("escape"), json!(false)));
    assert_eq!(closed["reason"], "no agent details are open");
    app.run("agent.select", json!({"serverId": "s1", "agentId": "a0000"})).unwrap();
    let view = control::view(&mut app);
    assert_eq!(row(&mut app)["enabled"], true);
    assert_eq!(view["document"]["drill"]["key"], json!({"serverId": "s1", "agentId": "a0000"}));
    assert_eq!(view["document"]["drill"]["state"], "loading");
    assert_eq!(view["document"]["drill"]["header"]["selected"], true);
    app.run("agent.deselect", Value::Null).unwrap();
    assert_eq!(control::view(&mut app)["document"]["drill"], Value::Null);
}
