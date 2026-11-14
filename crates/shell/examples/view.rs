//! Print what the frontend would draw, without a window: the `ui.inspect` value of a seeded
//! monitor (one daemon, a few agents) after an optional script.
//!
//! ```sh
//! cargo run -q -p pistudiodashboard-shell --example view -- --script '[["ui.set", {"theme": "light"}]]'
//! ```
//!
//! `--empty` starts from an empty monitor (no daemons) instead of the seeded one.
//! `--agents N` adds N idle agents spread over N/2.5 workspaces (to see a virtualised grid).
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
        cwd: Some("/work/a/app".into()),
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
    // Other workspaces, so grouping has something to group: a folder name shared with a/app, one
    // with a question waiting, and agents without a working directory.
    let mut elsewhere = |id: &str, title: &str, cwd: Option<&str>, status: AgentStatus, pending: u32| {
        let mut a = agent(id, title, status);
        a.cwd = cwd.map(str::to_owned);
        a.attention.pending_dialogs = pending;
        m.upsert_agent(a);
    };
    elsewhere("a4", "Port the CLI", Some("/work/b/app/"), AgentStatus::Idle, 1);
    elsewhere("a5", "Bump dependencies", Some("/work/b/app"), AgentStatus::Running, 0);
    elsewhere("a6", "Draft the changelog", Some("/work/docs"), AgentStatus::Idle, 0);
    elsewhere("a7", "Scratch session", None, AgentStatus::Idle, 0);
    elsewhere("a8", "Another scratch", None, AgentStatus::Closed, 0);
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
    let mut monitor = if args.iter().any(|a| a == "--empty") { MonitorState::default() } else { seeded() };
    if let Some(n) = arg(&args, "--agents").and_then(|n| n.parse::<usize>().ok()) {
        for i in 0..n {
            monitor.upsert_agent(AgentEntry {
                server_id: "srv-1".into(),
                agent_id: format!("bulk{i:04}"),
                title: Some(format!("Bulk agent {i}")),
                model: Some("mock-1".into()),
                cwd: Some(format!("/work/project-{:03}", i * 2 / 5)),
                status: AgentStatus::Idle,
                ..AgentEntry::default()
            });
        }
    }
    let mut app = AppShell::new(Session::with_monitor(monitor), Services::default());
    for (method, params) in script {
        eprintln!("{method}: {}", control::handle(&mut app, &method, &params));
    }
    println!("{}", control::view(&mut app));
}
