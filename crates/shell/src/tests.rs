use pistudiodashboard_engine::Session;
use pistudiodashboard_model::{AgentEntry, AgentStatus, DaemonSlice, DrillBuffer, DrillTarget, MonitorState, ToastKind};
use serde_json::{Value, json};

use crate::state::Dialog;
use crate::{AppShell, Envelope, Request, Services, ThemeKind, control, menus};

fn app() -> AppShell {
    AppShell::new(Session::new(), Services::default())
}

fn seeded_app() -> AppShell {
    let mut m = MonitorState::default();
    let mut d = DaemonSlice::new("e1", "box");
    d.server_id = Some("srv".into());
    m.daemons.push(d);
    m.upsert_agent(AgentEntry { server_id: "srv".into(), agent_id: "a1".into(), status: AgentStatus::Running, ..AgentEntry::default() });
    AppShell::new(Session::with_monitor(m), Services::default())
}

fn menu_item(app: &mut AppShell, id: &str) -> Value {
    let view = control::view(app);
    let found = view["menus"].as_array().unwrap().iter().flat_map(|m| m["items"].as_array().unwrap().clone()).find(|i| i["id"] == id);
    found.unwrap_or_else(|| panic!("no menu item {id}"))
}

#[test]
fn the_view_is_the_ui_state_the_monitor_and_the_generated_menu_bar() {
    let mut app = seeded_app();
    let view = control::view(&mut app);
    assert_eq!(
        view["ui"],
        json!({
            "theme": "dark", "filterDaemons": [], "filterStatuses": [], "attentionRailCollapsed": false, "groupBy": "workspace", "zoom": 100, "focusItem": null,
            "gridWindow": {"start": 0, "count": 100}, "dialog": null, "diagnosticsTarget": null, "toasts": [], "status": ""
        })
    );
    assert_eq!(view["document"]["cards"]["total"], 1);
    assert_eq!(view["document"]["totals"]["label"], "0 of 1 agents");
    let labels: Vec<&str> = view["menus"].as_array().unwrap().iter().map(|m| m["label"].as_str().unwrap()).collect();
    assert_eq!(labels, menus::MENU_ORDER);
    let themes: Vec<Value> = ["dark", "light", "system"].iter().map(|t| menu_item(&mut app, &format!("view.setTheme:{t}"))).collect();
    assert_eq!(themes.iter().map(|t| t["checked"].clone()).collect::<Vec<_>>(), [json!(true), json!(false), json!(false)], "exactly one checked");
    assert_eq!(themes[2]["label"], "Theme: System");
    let rail = menu_item(&mut app, "view.toggleAttentionRail");
    assert_eq!(rail["shortcut"]["key"], "a");
    assert_eq!((rail["shortcut"]["cmd"].clone(), rail["shortcut"]["shift"].clone()), (json!(true), json!(true)));
    assert!(view["menus"].to_string().find("feed.ingest").is_none(), "internal commands have no menu entry");
}

#[test]
fn a_menu_click_runs_shell_commands() {
    let mut app = app();
    menus::invoke(&mut app, "view.setTheme:light").unwrap();
    assert_eq!(app.ui.theme, ThemeKind::Light);
    assert!(menus::invoke(&mut app, "view.setTheme:neon").is_err(), "an unbound value is not a command");
    menus::invoke(&mut app, "view.toggleAttentionRail").unwrap();
    assert!(app.ui.attention_rail_collapsed);
    assert!(menus::invoke(&mut app, "file.new").is_err(), "the sample document commands are gone");
}

#[test]
fn a_failed_command_reaches_the_status_bar_and_a_success_clears_it() {
    let mut app = app();
    assert!(app.run("feed.ingest", json!({"events": "x"})).is_err());
    assert!(app.ui.status.starts_with("bad params: `events`"), "{}", app.ui.status);
    app.run("command.list", Value::Null).unwrap();
    assert_eq!(app.ui.status, "");
}

#[test]
fn toasts_reach_the_ui_state_bounded_to_five() {
    let mut app = app();
    let skipped =
        app.session.execute_effects((0..7).map(|i| pistudiodashboard_model::Effect::Toast { kind: ToastKind::Info, text: format!("t{i}") }).collect());
    assert!(skipped.is_empty());
    app.run("command.list", Value::Null).unwrap();
    let texts: Vec<&str> = app.ui.toasts.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["t2", "t3", "t4", "t5", "t6"]);
    assert_eq!(app.ui.toasts[0].id, 3);
    assert!(app.session.take_toasts().is_empty());
}

#[test]
fn a_close_drill_effect_from_a_snapshot_becomes_a_toast() {
    let mut m = MonitorState::default();
    m.daemons.push(DaemonSlice::new("e1", "box"));
    m.drilldown = Some(DrillBuffer { target: DrillTarget { server_id: "srv".into(), agent_id: "gone".into() }, ..DrillBuffer::default() });
    let mut app = AppShell::new(Session::with_monitor(m), Services::default());
    let events = json!([
        {"entryId": "e1", "serverId": "srv", "connEpoch": 1, "seq": 0, "at": 1, "kind": {"type": "hello", "info": {}, "caps": {}, "degraded": [], "tlsInUse": false, "isLoopback": true}},
        {"entryId": "e1", "connEpoch": 1, "seq": 1, "at": 2, "kind": {"type": "snapshot", "agents": [], "uiPending": []}}
    ]);
    app.run("feed.ingest", json!({"events": events})).unwrap();
    assert!(app.session.snapshot().drilldown.is_none());
    assert_eq!(app.ui.toasts.last().map(|t| t.text.as_str()), Some("agent gone"));
}

#[test]
fn the_control_protocol_drives_and_inspects_the_app() {
    let mut app = seeded_app();
    let call = |app: &mut AppShell, method: &str, params: Value| control::handle(app, method, &params);
    assert_eq!(call(&mut app, "ui.set", json!({"theme": "light"}))["ok"], true);
    let seen = call(&mut app, "ui.inspect", Value::Null);
    assert_eq!(seen["result"]["ui"]["theme"], "light");
    assert_eq!(seen["result"]["document"]["cards"]["window"][0]["agentId"], "a1");
    assert_eq!(call(&mut app, "engine.execute", json!({"command": "view.setTheme", "params": {"theme": "dark"}}))["ok"], true, "shell commands run by id too");
    assert_eq!(app.ui.theme, ThemeKind::Dark);

    let before = app.ui.clone();
    for (field, error) in
        [("nope", "unknown field `nope`"), ("toasts", "`toasts` is read-only"), ("status", "`status` is read-only"), ("hideDone", "unknown field `hideDone`")]
    {
        let refused = call(&mut app, "ui.set", json!({"theme": "light", field: json!([])}));
        assert_eq!(refused, json!({"ok": false, "error": error}));
    }
    assert_eq!(app.ui, before, "a refused ui.set changes nothing");
    assert_eq!(call(&mut app, "ui.nope", Value::Null)["ok"], false);
    let listed = call(&mut app, "engine.commands", Value::Null);
    let feed = listed["result"].as_array().unwrap().iter().find(|c| c["id"] == "feed.ingest").unwrap().clone();
    assert_eq!(feed["internal"], true);
    assert_eq!(feed["menu"], json!([]));
    let unknown = call(&mut app, "engine.execute", json!({"command": "item.add"}));
    assert_eq!(unknown["error"], "unknown command `item.add`");
}

#[test]
fn requests_from_a_channel_are_answered_in_order() {
    let mut app = app();
    let (tx, rx) = std::sync::mpsc::channel();
    let (first, first_reply) = Envelope::new(Request::Control { method: "ui.set".into(), params: json!({"theme": "light"}) });
    let (second, second_reply) = Envelope::new(Request::Invoke { command: "view.setTheme:dark".into() });
    tx.send(first).unwrap();
    tx.send(second).unwrap();
    drop(tx);
    for envelope in rx {
        let reply = app.answer(&envelope.request);
        envelope.reply.send(reply).unwrap();
    }
    assert_eq!(first_reply.try_recv().unwrap(), json!({"ok": true, "result": null}));
    assert_eq!(second_reply.try_recv().unwrap(), json!({"ok": true, "result": null}));
    assert_eq!(app.ui.theme, ThemeKind::Dark, "the second request ran after the first");
}

#[test]
fn ui_set_is_all_or_nothing_and_validates_every_field() {
    let mut app = seeded_app();
    let set = |app: &mut AppShell, p: Value| control::handle(app, "ui.set", &p);
    for theme in ["system", "light", "dark"] {
        assert_eq!(set(&mut app, json!({"theme": theme}))["ok"], true);
        assert_eq!(control::view(&mut app)["ui"]["theme"], theme, "the choice, never a resolution");
    }
    let before = app.ui.clone();
    for bad in [
        json!({"theme": "neon"}),
        json!({"theme": "light", "bogus": 1}),
        json!({"theme": "light", "filterStatuses": ["nope"]}),
        json!({"theme": "light", "filterStatuses": ["idle", "idle"]}),
        json!({"theme": "light", "filterDaemons": [""]}),
        json!({"theme": "light", "gridWindow": {"start": 0, "count": 1001}}),
        json!({"theme": "light", "focusItem": {"serverId": "s", "itemId": "i"}}),
        json!({"theme": "light", "dialog": {"kind": "nope"}}),
        json!({"theme": "light", "dialog": {"kind": "password", "entryId": "x".repeat(129)}}),
        json!({"theme": "light", "diagnosticsTarget": 5}),
        json!({"theme": "light", "attentionRailCollapsed": "yes"}),
    ] {
        assert_eq!(set(&mut app, bad.clone())["ok"], false, "{bad}");
        assert_eq!(app.ui, before, "{bad}");
    }
    assert_eq!(set(&mut app, json!({"theme": "neon"}))["error"], "`theme` is `dark`, `light` or `system`");
    assert_eq!(set(&mut app, json!({"dialog": {"kind": "password", "entryId": "e1"}}))["ok"], true);
    assert_eq!(app.ui.dialog, Some(crate::state::Dialog::Password { entry_id: "e1".into() }));
    assert_eq!(set(&mut app, json!({"dialog": null, "focusItem": null}))["ok"], true);
    assert!(app.ui.dialog.is_none());
}

#[test]
fn view_commands_change_ui_state_and_failures_reach_the_status_bar() {
    let mut app = seeded_app();
    app.run("view.filterDaemons", json!({"serverIds": ["srv"]})).unwrap();
    app.run("view.filterStatuses", json!({"statuses": ["running"]})).unwrap();
    app.run("view.setGridWindow", json!({"start": 20, "count": 50})).unwrap();
    app.run("view.toggleAttentionRail", Value::Null).unwrap();
    assert_eq!(app.ui.filter_daemons, ["srv"]);
    assert_eq!(app.ui.filter_statuses, [pistudiodashboard_model::AgentStatus::Running]);
    assert_eq!((app.ui.grid_window.start, app.ui.grid_window.count), (20, 50));
    assert!(app.ui.attention_rail_collapsed);
    let view = control::view(&mut app);
    assert_eq!(view["document"]["railCollapsed"], true);
    assert_eq!(view["document"]["cards"]["total"], 1, "the filters reach the monitor view");

    let before = app.ui.clone();
    assert!(app.run("view.setGridWindow", json!({"start": 0, "count": 100_000})).is_err());
    assert!(app.ui.status.contains("gridWindow"), "{}", app.ui.status);
    assert_eq!(app.ui.grid_window, before.grid_window);
    app.run("view.toggleAttentionRail", Value::Null).unwrap();
    assert_eq!(app.ui.status, "");
}

#[test]
fn engine_commands_lists_the_shell_commands_with_unique_ids() {
    let mut app = app();
    let listed = control::handle(&mut app, "engine.commands", &Value::Null);
    let rows = listed["result"].as_array().unwrap();
    let shell: Vec<&str> = rows.iter().filter(|r| r["layer"] == "shell").map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(
        shell,
        [
            "view.setTheme",
            "view.filterDaemons",
            "view.setGroupBy",
            "view.zoomIn",
            "view.zoomOut",
            "view.zoomReset",
            "view.filterStatuses",
            "view.toggleAttentionRail",
            "view.setGridWindow",
            "daemon.detectLocal",
            "daemon.showDiagnostics"
        ]
    );
    let mut ids: Vec<&str> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n);
    assert!(rows.iter().any(|r| r["id"] == "feed.ingest" && r["layer"] == "engine"));
}

fn local() -> crate::LocalDaemon {
    crate::LocalDaemon { name: "Local".into(), host: "127.0.0.1".into(), port: 6767, tls: false, web_ui_url: Some("http://localhost:4173".into()) }
}

#[test]
fn detect_local_adds_the_detected_daemon_once() {
    let mut app = AppShell::new(Session::new(), Services { detect_local_daemon: Some(Box::new(local)) });
    assert_eq!(menu_item(&mut app, "daemon.detectLocal")["enabled"], true);
    app.run("daemon.detectLocal", Value::Null).unwrap();
    let entry = app.session.config().daemons[0].clone();
    assert_eq!((entry.name.as_str(), entry.port, entry.auto_detected), ("Local", 6767, true));
    let item = menu_item(&mut app, "daemon.detectLocal");
    assert_eq!(item["enabled"], false);
    assert!(app.run("daemon.detectLocal", Value::Null).is_err());
    let mut bare = AppShell::new(Session::new(), Services::default());
    assert_eq!(menu_item(&mut bare, "daemon.detectLocal")["reason"], "local daemon detection is not available here");
}

#[test]
fn an_auth_failure_opens_the_password_dialog_and_removing_the_entry_closes_it() {
    let mut app = AppShell::new(Session::new(), Services::default());
    let id = app.run("daemon.add", json!({"name": "Box", "host": "h", "port": 1})).unwrap()["entryId"].as_str().unwrap().to_owned();
    let event = json!({"entryId": id, "connEpoch": 1, "seq": 0, "at": 1, "kind": {"type": "connState", "state": "auth_failed", "since": 1}});
    app.ui.status = "kept".into();
    app.run_internal("feed.ingest", json!({"events": [event]})).unwrap();
    assert_eq!(app.ui.dialog, Some(Dialog::Password { entry_id: id.clone() }));
    assert_eq!(app.ui.status, "kept", "background work leaves the status bar alone");
    app.ui.diagnostics_target = Some(id.clone());
    app.run("daemon.remove", json!({"entryId": id})).unwrap();
    assert!(app.ui.dialog.is_none() && app.ui.diagnostics_target.is_none());
}

fn add_box(app: &mut AppShell, name: &str, password: Option<&str>) -> String {
    let mut params = json!({"name": name, "host": "10.0.0.5", "port": 6790});
    if let Some(pw) = password {
        params["password"] = json!(pw);
    }
    app.run("daemon.add", params).unwrap()["entryId"].as_str().unwrap().to_owned()
}

#[test]
fn show_diagnostics_targets_a_known_daemon_and_ui_set_closes_it() {
    let mut app = AppShell::new(Session::new(), Services::default());
    let id = add_box(&mut app, "Box", None);
    let item = menu_item(&mut app, "daemon.showDiagnostics");
    assert_eq!((item["enabled"].clone(), item["shortcut"]["key"].clone(), item["shortcut"]["cmd"].clone()), (json!(true), json!("i"), json!(true)));
    assert!(control::view(&mut app)["document"].get("diagnostics").is_none(), "no panel, no data");

    let err = app.run("daemon.showDiagnostics", json!({"entryId": "nope"})).unwrap_err();
    assert!(err.contains("unknown_entry"), "{err}");
    assert!(app.ui.diagnostics_target.is_none(), "an unknown entry changes nothing");
    app.ui.diagnostics_target = Some(id.clone());
    assert!(app.run("daemon.showDiagnostics", json!({"entryId": "nope"})).is_err());
    assert_eq!(app.ui.diagnostics_target.as_deref(), Some(id.as_str()), "...and keeps an open panel");

    // A menu click or Cmd+I carries no params: the only daemon is the target.
    app.ui.diagnostics_target = None;
    menus::invoke(&mut app, "daemon.showDiagnostics").unwrap();
    assert_eq!(app.ui.diagnostics_target.as_deref(), Some(id.as_str()));
    let view = control::view(&mut app);
    let diag = &view["document"]["diagnostics"];
    assert_eq!((diag["entryId"].clone(), diag["name"].clone(), diag["conn"].clone()), (json!(id), json!("Box"), json!("idle")));
    assert_eq!(diag["server"]["featuresMissing"], json!([]), "no hello yet: nothing to call missing");
    assert_eq!(diag["degraded"], json!([]));

    assert_eq!(control::handle(&mut app, "ui.set", &json!({"diagnosticsTarget": null}))["ok"], true);
    assert!(app.ui.diagnostics_target.is_none());
}

#[test]
fn the_diagnostics_view_carries_the_counters_and_never_a_secret() {
    let secret = "hunter2-s3cret";
    let mut app = AppShell::new(Session::new(), Services::default());
    let id = add_box(&mut app, "Box", Some(secret));
    let events = json!([
        {"entryId": id, "connEpoch": 1, "seq": 0, "at": 1, "kind": {"type": "hello", "info": {"hostname": "devbox", "version": "0.0.115", "features": ["daemonObservability"]},
            "caps": {"daemonObservability": true}, "degraded": [], "tlsInUse": false, "isLoopback": false}},
        {"entryId": id, "connEpoch": 1, "seq": 1, "at": 2, "kind": {"type": "diagnostics", "reconnectCount": 3, "messagesTotal": 99, "messagesPerSecond": 4,
            "unknownTypes": {"a": 1, "b": 5}, "malformed": 2, "framesTooLarge": 1, "rttLastMs": 7, "rttEwmaMs": 9}},
        {"entryId": id, "connEpoch": 1, "seq": 2, "at": 3, "kind": {"type": "connState", "state": "auth_failed", "since": 3,
            "lastError": {"code": "auth_failed", "message": "Authentication failed"}}},
    ]);
    app.run_internal("feed.ingest", json!({"events": events})).unwrap();
    assert!(matches!(app.ui.dialog, Some(Dialog::Password { .. })), "the auth failure raised the prompt");
    app.run("daemon.showDiagnostics", json!({"entryId": id})).unwrap();
    let view = control::view(&mut app);
    let diag = &view["document"]["diagnostics"];
    assert_eq!(diag["conn"], "auth_failed");
    assert_eq!(diag["lastError"]["kind"], "auth_failed");
    assert_eq!((diag["reconnectCount"].clone(), diag["messagesPerSecond"].clone(), diag["malformed"].clone()), (json!(3), json!(4), json!(2)));
    assert_eq!(diag["unknownTypes"], json!([{"name": "b", "count": 5}, {"name": "a", "count": 1}]));
    assert_eq!((diag["rtt"]["lastMs"].clone(), diag["rtt"]["averageMs"].clone()), (json!(7), json!(9)));
    assert_eq!((diag["server"]["version"].clone(), diag["server"]["hostname"].clone()), (json!("0.0.115"), json!("devbox")));
    assert_eq!(diag["server"]["featuresMissing"], json!([]));
    let chip = &view["document"]["daemonChips"][0];
    assert_eq!((chip["hasPassword"].clone(), chip["unencrypted"].clone()), (json!(true), json!(true)));
    let everything = view.to_string() + &serde_json::to_string(&app.ui).unwrap();
    assert!(!everything.contains(secret), "no password anywhere in the view or ui.inspect");
}

#[test]
fn menu_clicks_open_the_forms_and_target_the_one_daemon() {
    let mut app = AppShell::new(Session::new(), Services::default());
    let add = menu_item(&mut app, "daemon.add");
    assert_eq!((add["label"].clone(), add["shortcut"]["key"].clone()), (json!("Add Daemon…"), json!("n")));
    assert_eq!(menu_item(&mut app, "daemon.edit")["enabled"], false, "an entry must exist");
    menus::invoke(&mut app, "daemon.add").unwrap();
    assert_eq!(app.ui.dialog, Some(Dialog::DaemonForm { mode: crate::state::FormMode::Add, entry_id: None }));
    assert!(menus::invoke(&mut app, "daemon.edit").is_err(), "nothing to edit");
    assert!(app.ui.status.contains("no daemon"), "{}", app.ui.status);

    let first = add_box(&mut app, "One", None);
    menus::invoke(&mut app, "daemon.edit").unwrap();
    assert_eq!(app.ui.dialog, Some(Dialog::DaemonForm { mode: crate::state::FormMode::Edit, entry_id: Some(first.clone()) }));
    menus::invoke(&mut app, "daemon.remove").unwrap();
    assert_eq!(app.ui.dialog, Some(Dialog::RemoveDaemon { entry_id: first }));
    assert_eq!(app.session.config().daemons.len(), 1, "opening the confirm removes nothing");

    add_box(&mut app, "Two", None);
    let err = menus::invoke(&mut app, "daemon.remove").unwrap_err();
    assert!(err.contains("several daemons"), "{err}");
    // Engine params still reach the engine command: the form submit and the CLI.
    assert!(app.run("daemon.add", json!({"name": "Three", "host": "h", "port": 1})).is_ok());
}

mod group_by {
    use std::cell::RefCell;
    use std::rc::Rc;

    use pistudiodashboard_config::Config;
    use pistudiodashboard_engine::{ConfigStore, EngineServices, GroupBy, LoadedConfig};

    use super::*;

    struct Saves(Rc<RefCell<Vec<GroupBy>>>);

    impl ConfigStore for Saves {
        fn load(&mut self) -> LoadedConfig {
            LoadedConfig { config: Config::default(), issues: Vec::new(), read_only: false }
        }
        fn save(&mut self, config: &Config) -> Result<(), String> {
            self.0.borrow_mut().push(config.settings.group_by);
            Ok(())
        }
    }

    fn saving_app() -> (AppShell, Rc<RefCell<Vec<GroupBy>>>) {
        let saves = Rc::default();
        let services = EngineServices { config_store: Some(Box::new(Saves(Rc::clone(&saves)))), ..EngineServices::default() };
        (AppShell::new(Session::new().with_services(services), Services::default()), saves)
    }

    #[test]
    fn the_command_sets_the_state_the_document_and_saves_once() {
        let (mut app, saves) = saving_app();
        app.run("view.setGroupBy", json!({"groupBy": "none"})).unwrap();
        assert_eq!(app.ui.group_by, GroupBy::None);
        let view = control::view(&mut app);
        assert_eq!((view["ui"]["groupBy"].clone(), view["document"]["groupBy"].clone()), (json!("none"), json!("none")));
        assert!(view["document"].get("groups").is_none(), "flat: no groups block");
        assert_eq!(*saves.borrow(), [GroupBy::None]);
        let grouped = {
            app.run("view.setGroupBy", json!({"groupBy": "workspace"})).unwrap();
            control::view(&mut app)["document"].clone()
        };
        assert!(grouped["groups"].is_object());
        assert_eq!(saves.borrow().len(), 2);
        app.run("view.setGroupBy", json!({"groupBy": "workspace"})).unwrap();
        assert_eq!(saves.borrow().len(), 2, "no change, no save");
    }

    #[test]
    fn a_bad_value_is_refused_through_every_door_and_changes_nothing() {
        let (mut app, saves) = saving_app();
        for refused in [
            app.run("view.setGroupBy", json!({"groupBy": "banana"})).unwrap_err(),
            app.run("view.setGroupBy", json!({})).unwrap_err(),
            control::handle(&mut app, "ui.set", &json!({"groupBy": "banana"}))["error"].as_str().unwrap().to_owned(),
        ] {
            assert!(refused.contains("groupBy"), "{refused}");
        }
        assert_eq!((app.ui.group_by, saves.borrow().len()), (GroupBy::Workspace, 0));
    }

    #[test]
    fn the_menu_offers_both_modes_with_one_checked_and_invoking_matches_the_command() {
        let (mut app, _saves) = saving_app();
        let checks = |app: &mut AppShell| ["none", "workspace"].map(|m| menu_item(app, &format!("view.setGroupBy:{m}"))["checked"].clone());
        assert_eq!(checks(&mut app), [json!(false), json!(true)], "grouped by workspace by default");
        assert_eq!(menu_item(&mut app, "view.setGroupBy:none")["label"], "Group: None");
        menus::invoke(&mut app, "view.setGroupBy:none").unwrap();
        assert_eq!((app.ui.group_by, checks(&mut app)), (GroupBy::None, [json!(true), json!(false)]));
    }

    #[test]
    fn ui_set_saves_too_and_a_changed_layout_returns_to_the_top() {
        let (mut app, saves) = saving_app();
        control::handle(&mut app, "ui.set", &json!({"gridWindow": {"start": 300, "count": 40}}));
        assert_eq!(control::handle(&mut app, "ui.set", &json!({"groupBy": "none"}))["ok"], true);
        assert_eq!((app.ui.grid_window.start, saves.borrow().clone()), (0, vec![GroupBy::None]));
        control::handle(&mut app, "ui.set", &json!({"groupBy": "workspace", "gridWindow": {"start": 5, "count": 40}}));
        assert_eq!(app.ui.grid_window.start, 5, "a request that says where to look keeps it");
        control::handle(&mut app, "ui.set", &json!({"theme": "light"}));
        assert_eq!(saves.borrow().len(), 2, "other fields do not touch the saved grouping");
    }

    #[test]
    fn adopt_settings_takes_the_loaded_value() {
        let mut app = app();
        app.session.apply_loaded_config(LoadedConfig {
            config: {
                let mut c = Config::default();
                c.settings.group_by = GroupBy::None;
                c
            },
            issues: Vec::new(),
            read_only: false,
        });
        app.adopt_settings();
        assert_eq!(app.ui.group_by, GroupBy::None);
    }
}

mod zoom {
    use std::cell::RefCell;
    use std::rc::Rc;

    use pistudiodashboard_config::Config;
    use pistudiodashboard_engine::{ConfigStore, EngineServices, LoadedConfig};

    use super::*;

    struct Saves(Rc<RefCell<Vec<u16>>>);

    impl ConfigStore for Saves {
        fn load(&mut self) -> LoadedConfig {
            LoadedConfig { config: Config::default(), issues: Vec::new(), read_only: false }
        }
        fn save(&mut self, config: &Config) -> Result<(), String> {
            self.0.borrow_mut().push(config.settings.zoom);
            Ok(())
        }
    }

    fn saving_app() -> (AppShell, Rc<RefCell<Vec<u16>>>) {
        let saves = Rc::default();
        let services = EngineServices { config_store: Some(Box::new(Saves(Rc::clone(&saves)))), ..EngineServices::default() };
        (AppShell::new(Session::new().with_services(services), Services::default()), saves)
    }

    #[test]
    fn zoom_in_out_and_actual_size_step_through_the_levels_and_save_each_change() {
        let (mut app, saves) = saving_app();
        assert_eq!(app.ui.zoom, 100);
        app.run("view.zoomIn", json!({})).unwrap();
        app.run("view.zoomIn", json!({})).unwrap();
        assert_eq!(app.ui.zoom, 125);
        app.run("view.zoomOut", json!({})).unwrap();
        assert_eq!(app.ui.zoom, 110);
        app.run("view.zoomReset", json!({})).unwrap();
        assert_eq!(app.ui.zoom, 100);
        assert_eq!(*saves.borrow(), [110, 125, 110, 100]);
        assert_eq!(control::view(&mut app)["ui"]["zoom"], 100);
    }

    #[test]
    fn at_the_ends_the_command_is_disabled_with_the_reason_and_refuses() {
        let (mut app, _saves) = saving_app();
        control::handle(&mut app, "ui.set", &json!({"zoom": 300}));
        let item = menu_item(&mut app, "view.zoomIn");
        assert_eq!((item["enabled"].clone(), item["reason"].clone()), (json!(false), json!("already at the largest size")));
        assert!(app.run("view.zoomIn", json!({})).is_err());
        control::handle(&mut app, "ui.set", &json!({"zoom": 50}));
        assert_eq!(menu_item(&mut app, "view.zoomOut")["reason"], "already at the smallest size");
        app.run("view.zoomReset", json!({})).unwrap();
        assert_eq!(menu_item(&mut app, "view.zoomReset")["reason"], "already at 100%");
    }

    #[test]
    fn the_menu_has_the_three_entries_with_their_shortcuts() {
        let (mut app, _saves) = saving_app();
        for (id, label, key) in [("view.zoomIn", "Zoom In", "="), ("view.zoomOut", "Zoom Out", "-"), ("view.zoomReset", "Actual Size", "0")] {
            let item = menu_item(&mut app, id);
            assert_eq!((item["label"].clone(), item["shortcut"]["key"].clone(), item["shortcut"]["cmd"].clone()), (json!(label), json!(key), json!(true)));
        }
        assert_eq!(menus::invoke(&mut app, "view.zoomIn"), Ok(Value::Null));
        assert_eq!(app.ui.zoom, 110);
    }

    #[test]
    fn ui_set_validates_the_range_and_saves() {
        let (mut app, saves) = saving_app();
        for bad in [json!(49), json!(301), json!("big"), json!(1.5), json!(-1), json!(null)] {
            let reply = control::handle(&mut app, "ui.set", &json!({"zoom": bad}));
            assert!(reply["error"].as_str().is_some_and(|e| e.contains("`zoom`")), "{reply}");
        }
        assert_eq!((app.ui.zoom, saves.borrow().len()), (100, 0));
        assert_eq!(control::handle(&mut app, "ui.set", &json!({"zoom": 137}))["ok"], true);
        assert_eq!((app.ui.zoom, saves.borrow().clone()), (137, vec![137]));
        control::handle(&mut app, "ui.set", &json!({"theme": "light"}));
        assert_eq!(saves.borrow().len(), 1, "other fields do not touch the saved zoom");
    }

    #[test]
    fn adopt_settings_takes_the_loaded_zoom() {
        let mut app = app();
        let mut config = Config::default();
        config.settings.zoom = 150;
        app.session.apply_loaded_config(LoadedConfig { config, issues: Vec::new(), read_only: false });
        app.adopt_settings();
        assert_eq!(app.ui.zoom, 150);
    }
}
