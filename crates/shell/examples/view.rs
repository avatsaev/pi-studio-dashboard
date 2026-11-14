//! Print what the frontend would draw, without a window: the `ui.inspect` value of a seeded
//! monitor (one daemon, a few agents) after an optional script.
//!
//! ```sh
//! cargo run -q -p pistudiodashboard-shell --example view -- --script '[["ui.set", {"theme": "light"}]]'
//! ```
//!
//! `--empty` starts from an empty monitor (no daemons) instead of the seeded one.
//! `--script` is a JSON array of `[method, params]` control-protocol calls, applied in order.
//! Each reply goes to standard error, the view to standard output. The frontend's
//! `npm run snapshot` feeds this to the real components in a headless browser and saves a PNG.

use pistudiodashboard_engine::Session;
use pistudiodashboard_model::{ActivitySummary, AgentEntry, AgentStatus, Attention, CurrentTool, DaemonSlice, MonitorState, UsageSnapshot};
use pistudiodashboard_shell::{AppShell, Services, control};
use serde_json::Value;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn seeded() -> MonitorState {
    let mut m = MonitorState::default();
    let mut d = DaemonSlice::new("e1", "workstation");
    d.server_id = Some("srv-1".into());
    m.daemons.push(d);
    let agent = |id: &str, title: &str, status: AgentStatus| AgentEntry {
        server_id: "srv-1".into(),
        agent_id: id.into(),
        title: Some(title.into()),
        model: Some("mock-1".into()),
        status,
        ..AgentEntry::default()
    };
    let mut running = agent("a1", "Refactor parser", AgentStatus::Running);
    running.activity = ActivitySummary {
        current_tool: Some(CurrentTool { call_id: None, kind: "shell".into(), label: "cargo test".into(), status: "running".into() }),
        last_output_line: Some("running 12 tests".into()),
        ..ActivitySummary::default()
    };
    running.usage = UsageSnapshot::Some { tokens: None, cost: Some(0.12), context_usage: None, taken_at: 0 };
    let mut failed = agent("a2", "Fix flaky test", AgentStatus::Error);
    failed.attention = Attention { pending_dialogs: 0, errored: true };
    m.upsert_agent(running);
    m.upsert_agent(failed);
    m.upsert_agent(agent("a3", "Write docs", AgentStatus::Idle));
    m
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let script: Vec<(String, Value)> = match arg(&args, "--script").map(|s| serde_json::from_str(&s)) {
        Some(Ok(script)) => script,
        Some(Err(e)) => {
            eprintln!("--script must be [[method, params], …]: {e}");
            std::process::exit(2);
        }
        None => Vec::new(),
    };
    let monitor = if args.iter().any(|a| a == "--empty") { MonitorState::default() } else { seeded() };
    let mut app = AppShell::new(Session::with_monitor(monitor), Services::default());
    for (method, params) in script {
        eprintln!("{method}: {}", control::handle(&mut app, &method, &params));
    }
    println!("{}", control::view(&mut app));
}
