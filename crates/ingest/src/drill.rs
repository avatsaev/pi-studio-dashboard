//! Drill-down buffer reducers (D6, D17, D27). Pure.
//!
//! The buffer is seeded by `Timeline` pages from the client (which does the paging) and kept
//! current by `AgentStream` events. Events that arrive while the timeline loads are held and
//! replayed against the loaded `seqHigh`.

use pistudiodashboard_model::{
    BlockKind, DRILL_HOLD, DrillBuffer, DrillItem, DrillState, DrillTarget, ERROR_TEXT, Effect, HeldEvent, MonitorState, StreamEvent, TOOL_OUTPUT_LINE,
    TOOL_OUTPUT_TAIL_LINES, TimelineItem, TurnKind, cap_string,
};

use crate::activity::label_of;
use crate::apply::Ctx;
use crate::reduce_agent::owner;

/// Text of one assistant / reasoning block.
const BLOCK_TEXT: usize = 64 * 1024;
/// A `system` item (user prompt echo, compaction).
const SYSTEM_TEXT: usize = 500;
const TOOL_FIELD: usize = 120;

/// Why `select` refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrillError {
    /// The agent is unknown or archived.
    NotAvailable,
}

/// Open a drill-down on an agent: an empty `loading` buffer replaces any previous one and the
/// newest timeline page is requested.
///
/// # Errors
/// [`DrillError::NotAvailable`] for an unknown or archived agent; the state is untouched.
pub fn select(state: &mut MonitorState, server_id: &str, agent_id: &str) -> Result<Vec<Effect>, DrillError> {
    let Some(agent) = state.agent(server_id, agent_id).filter(|a| !a.archived) else { return Err(DrillError::NotAvailable) };
    let stale = agent.stale;
    state.drilldown = Some(DrillBuffer {
        target: DrillTarget { server_id: server_id.to_owned(), agent_id: agent_id.to_owned() },
        state: DrillState::Loading,
        stale,
        ..DrillBuffer::default()
    });
    Ok(vec![Effect::FetchTimeline { server_id: server_id.to_owned(), agent_id: agent_id.to_owned() }])
}

/// Close the drill-down, freeing its buffer. `false` when none was open.
pub fn deselect(state: &mut MonitorState) -> bool {
    state.drilldown.take().is_some()
}

fn target_mut<'a>(state: &'a mut MonitorState, sid: &str, agent_id: &str) -> Option<&'a mut DrillBuffer> {
    state.drilldown.as_mut().filter(|d| d.target.server_id == sid && d.target.agent_id == agent_id)
}

fn tail_lines(output: &str) -> Vec<String> {
    let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
    let skip = lines.len().saturating_sub(TOOL_OUTPUT_TAIL_LINES);
    lines.into_iter().skip(skip).map(|l| cap_string(l, TOOL_OUTPUT_LINE)).collect()
}

fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "error" | "canceled")
}

fn push(buf: &mut DrillBuffer, item: DrillItem) {
    if buf.items.push(item) {
        buf.truncated_at_start = true;
    }
}

/// Append a delta to the open block of `kind`, or open a new one.
fn append_text(buf: &mut DrillBuffer, kind: BlockKind, text: &str) {
    if text.is_empty() {
        return;
    }
    let open = buf.open_block == Some(kind);
    if open {
        let last = buf.items.last_mut();
        let slot = match last {
            Some(DrillItem::Assistant { text: t }) if kind == BlockKind::Assistant => Some(t),
            Some(DrillItem::Reasoning { text: t }) if kind == BlockKind::Reasoning => Some(t),
            _ => None,
        };
        if let Some(t) = slot {
            if t.len() < BLOCK_TEXT {
                let room = BLOCK_TEXT - t.len();
                t.push_str(&cap_string(text, room));
            }
            return;
        }
    }
    let first = cap_string(text, BLOCK_TEXT);
    push(buf, if kind == BlockKind::Assistant { DrillItem::Assistant { text: first } } else { DrillItem::Reasoning { text: first } });
    buf.open_block = Some(kind);
}

fn close_running_tools(buf: &mut DrillBuffer, status: &str) {
    for item in buf.items.iter_mut() {
        if let DrillItem::Tool { status: s, .. } = item
            && !is_terminal(s)
        {
            *s = status.to_owned();
        }
    }
}

fn upsert_tool(buf: &mut DrillBuffer, call_id: &str, status: &str, tool: &pistudiodashboard_model::ToolInfo) {
    let tail = tool.output.as_deref().map(tail_lines).filter(|t| !t.is_empty());
    let existing = buf.items.iter_mut().rev().find_map(|i| match i {
        DrillItem::Tool { call_id: c, kind, label, status: s, output_tail } if c == call_id => Some((kind, label, s, output_tail)),
        _ => None,
    });
    if let Some((kind, label, s, output_tail)) = existing {
        *s = cap_string(status, TOOL_FIELD);
        if !tool.kind.is_empty() {
            *kind = cap_string(&tool.kind, TOOL_FIELD);
        }
        if tool.detail.is_some() {
            *label = label_of(tool.detail.as_deref(), &tool.kind);
        }
        if tail.is_some() {
            *output_tail = tail;
        }
        return;
    }
    push(
        buf,
        DrillItem::Tool {
            call_id: cap_string(call_id, TOOL_FIELD),
            kind: cap_string(&tool.kind, TOOL_FIELD),
            label: label_of(tool.detail.as_deref(), &tool.kind),
            status: cap_string(status, TOOL_FIELD),
            output_tail: tail,
        },
    );
}

/// Fold one event into the item list. `false` for an unknown kind (skipped; the caller counts it).
fn fold(buf: &mut DrillBuffer, event: &StreamEvent) -> bool {
    match event {
        StreamEvent::AssistantMessage { text, is_final } => {
            append_text(buf, BlockKind::Assistant, text);
            if *is_final {
                buf.open_block = None;
            }
            return true;
        }
        StreamEvent::Reasoning { text, is_final } => {
            append_text(buf, BlockKind::Reasoning, text.as_deref().unwrap_or(""));
            if *is_final {
                buf.open_block = None;
            }
            return true;
        }
        StreamEvent::Unknown { .. } => return false,
        _ => {}
    }
    buf.open_block = None;
    match event {
        StreamEvent::ToolCall { call_id, status, tool, .. } => upsert_tool(buf, call_id, status, tool),
        StreamEvent::TurnStarted => push(buf, DrillItem::Turn { kind: TurnKind::Started }),
        StreamEvent::TurnCompleted => push(buf, DrillItem::Turn { kind: TurnKind::Completed }),
        StreamEvent::TurnFailed { error } => {
            close_running_tools(buf, "error");
            push(buf, DrillItem::Turn { kind: TurnKind::Failed });
            if let Some(e) = error {
                push(buf, DrillItem::Error { message: cap_string(e, ERROR_TEXT) });
            }
        }
        StreamEvent::TurnCanceled => {
            close_running_tools(buf, "canceled");
            push(buf, DrillItem::Turn { kind: TurnKind::Canceled });
        }
        StreamEvent::Error { message } => push(buf, DrillItem::Error { message: cap_string(message.as_deref().unwrap_or("error"), ERROR_TEXT) }),
        StreamEvent::UserMessage { text } => push(buf, DrillItem::System { text: cap_string(text.as_deref().unwrap_or(""), SYSTEM_TEXT) }),
        StreamEvent::Compaction { phase, .. } => {
            push(buf, DrillItem::System { text: format!("context compaction {}", cap_string(phase.as_deref().unwrap_or("update"), 40)) });
        }
        StreamEvent::AssistantMessage { .. } | StreamEvent::Reasoning { .. } | StreamEvent::Unknown { .. } => {}
    }
    true
}

/// `drill_apply`: a live stream event of the drilled agent. Held while loading; afterwards an
/// event with `seq <= seqHigh` is a replay and ignored; events without `seq` are always applied.
pub(crate) fn apply_stream(state: &mut MonitorState, sid: &str, agent_id: &str, seq: Option<u64>, event: &StreamEvent) {
    let Some(buf) = target_mut(state, sid, agent_id) else { return };
    if matches!(buf.state, DrillState::Loading | DrillState::Refetching) {
        if buf.held.len() >= DRILL_HOLD {
            buf.held.remove(0);
        }
        buf.held.push(HeldEvent { seq, event: event.clone() });
        return;
    }
    apply_live(buf, seq, event);
}

fn apply_live(buf: &mut DrillBuffer, seq: Option<u64>, event: &StreamEvent) {
    if let Some(s) = seq {
        if buf.seq_high.is_some_and(|h| s <= h) {
            return;
        }
        buf.seq_high = Some(buf.seq_high.map_or(s, |h| h.max(s)));
    }
    fold(buf, event);
}

/// `Timeline`: a page for the drilled agent. `reset` replaces the items, otherwise they are
/// appended. Held events newer than the page are replayed and the buffer goes `live`.
pub(crate) fn timeline(state: &mut MonitorState, ctx: Ctx, agent_id: &str, items: &[TimelineItem], reset: bool, seq_end: u64) {
    let Some(sid) = owner(state, ctx) else { return };
    let mut unknown = 0_u64;
    if let Some(buf) = target_mut(state, &sid, agent_id) {
        if reset {
            buf.items.clear();
            buf.open_block = None;
            buf.truncated_at_start = false;
        }
        for item in items {
            if !fold(buf, &item.event) {
                unknown += 1;
            }
        }
        buf.seq_high = Some(buf.seq_high.map_or(seq_end, |h| h.max(seq_end)));
        for held in std::mem::take(&mut buf.held) {
            apply_live(buf, held.seq, &held.event);
        }
        buf.open_block = buf.open_block.filter(|_| !buf.items.is_empty());
        buf.state = DrillState::Live;
        buf.stale = false;
    }
    state.dropped_events = state.dropped_events.saturating_add(unknown);
}

/// `TimelineReset` for the drilled agent: the buffer is emptied and the newest page requested again.
pub(crate) fn timeline_reset(state: &mut MonitorState, ctx: Ctx, agent_id: &str, effects: &mut Vec<Effect>) {
    let Some(sid) = owner(state, ctx) else { return };
    if let Some(buf) = target_mut(state, &sid, agent_id) {
        buf.items.clear();
        buf.held.clear();
        buf.open_block = None;
        buf.seq_high = None;
        buf.truncated_at_start = false;
        buf.state = DrillState::Refetching;
        effects.push(Effect::FetchTimeline { server_id: sid, agent_id: agent_id.to_owned() });
    }
}
