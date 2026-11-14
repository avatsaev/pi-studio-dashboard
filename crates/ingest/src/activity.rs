//! Activity-summary derivation from `agent_stream` events (live-state.md table). Pure.

use pistudiodashboard_model::{ASSISTANT_TAIL, AgentEntry, CurrentTool, ERROR_TEXT, LABEL, OUTPUT_LINE, StreamEvent, TurnState, cap_string};

/// What the caller still has to do after an event was folded into the summary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Outcome {
    /// `turn_failed`: the agent is marked errored.
    pub failed_turn: bool,
    /// A mid-stream `error` event.
    pub error_event: bool,
    /// The turn ended: usage is worth a refresh.
    pub refresh_stats: bool,
}

/// The last non-empty line of `s`, trimmed and capped.
fn last_line(s: &str) -> Option<String> {
    s.lines().rev().map(str::trim).find(|l| !l.is_empty()).map(|l| cap_string(l, OUTPUT_LINE))
}

/// Single-line label: whitespace collapsed, capped.
pub(crate) fn label_of(detail: Option<&str>, kind: &str) -> String {
    let text = detail.map(|d| d.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|t| !t.is_empty());
    cap_string(text.as_deref().unwrap_or(kind), LABEL)
}

/// Keep at most the last `ASSISTANT_TAIL` bytes of `s`, cutting on a char boundary.
fn trim_tail(s: &mut String) {
    if s.len() > ASSISTANT_TAIL {
        let mut cut = s.len() - ASSISTANT_TAIL;
        while !s.is_char_boundary(cut) {
            cut += 1;
        }
        s.drain(..cut);
    }
}

fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "error")
}

fn end_turn(a: &mut AgentEntry, state: TurnState) {
    a.activity.current_tool = None;
    a.activity.turn_state = state;
}

/// Fold one event into the card summary.
pub(crate) fn update_activity(a: &mut AgentEntry, event: &StreamEvent) -> Outcome {
    let mut out = Outcome::default();
    match event {
        StreamEvent::TurnStarted => {
            a.activity.current_tool = None;
            a.activity.last_output_line = None;
            a.activity.turn_state = TurnState::Started;
            a.activity.last_error_message = None;
            a.assistant_tail.clear();
        }
        StreamEvent::AssistantMessage { text, is_final } => {
            // The docs say `final` carries no text, but the daemon does send the whole message on
            // it when no deltas preceded: it counts as text, then closes the buffer.
            a.assistant_tail.push_str(text);
            trim_tail(&mut a.assistant_tail);
            if let Some(line) = last_line(&a.assistant_tail) {
                a.activity.last_output_line = Some(line);
            }
            if *is_final {
                a.assistant_tail.clear();
            }
        }
        StreamEvent::ToolCall { call_id, status, partial, tool } => {
            let output_line = tool.output.as_deref().and_then(last_line);
            if is_terminal(status) {
                if a.activity.current_tool.as_ref().is_some_and(|t| t.call_id.as_deref() == Some(call_id)) {
                    a.activity.current_tool = None;
                }
                if output_line.is_some() {
                    a.activity.last_output_line = output_line;
                }
            } else {
                a.activity.current_tool = Some(CurrentTool {
                    call_id: Some(call_id.clone()),
                    kind: cap_string(&tool.kind, LABEL),
                    label: label_of(tool.detail.as_deref(), &tool.kind),
                    status: cap_string(status, LABEL),
                });
                if *partial && output_line.is_some() {
                    a.activity.last_output_line = output_line;
                }
            }
        }
        StreamEvent::TurnCompleted => {
            end_turn(a, TurnState::Completed);
            out.refresh_stats = true;
        }
        StreamEvent::TurnFailed { error } => {
            end_turn(a, TurnState::Failed);
            a.activity.last_error_message = error.as_deref().map(|e| cap_string(e, ERROR_TEXT));
            a.attention.errored = true;
            out.failed_turn = true;
            out.refresh_stats = true;
        }
        StreamEvent::TurnCanceled => end_turn(a, TurnState::Canceled),
        StreamEvent::Error { message } => {
            a.activity.last_error_message = message.as_deref().map(|m| cap_string(m, ERROR_TEXT));
            out.error_event = true;
        }
        StreamEvent::UserMessage { .. } | StreamEvent::Reasoning { .. } | StreamEvent::Compaction { .. } | StreamEvent::Unknown { .. } => {}
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use pistudiodashboard_model::ToolInfo;

    use super::*;

    fn tool(call: &str, status: &str, partial: bool, detail: Option<&str>, output: Option<&str>) -> StreamEvent {
        StreamEvent::ToolCall {
            call_id: call.into(),
            status: status.into(),
            partial,
            tool: ToolInfo { kind: "shell".into(), detail: detail.map(Into::into), output: output.map(Into::into) },
        }
    }

    #[test]
    fn the_documented_tool_sequence() {
        let mut a = AgentEntry::default();
        update_activity(&mut a, &StreamEvent::TurnStarted);
        update_activity(&mut a, &tool("c1", "running", false, Some("npm   test\n--watch"), None));
        assert_eq!(a.activity.current_tool.as_ref().unwrap().label, "npm test --watch");
        update_activity(&mut a, &tool("c1", "running", true, Some("npm test"), Some("a\nb\n\n")));
        assert_eq!(a.activity.last_output_line.as_deref(), Some("b"));
        update_activity(&mut a, &StreamEvent::Reasoning { text: Some("thinking".into()), is_final: false });
        assert_eq!(a.activity.last_output_line.as_deref(), Some("b"), "reasoning is never output");
        update_activity(&mut a, &tool("other", "completed", false, None, None));
        assert!(a.activity.current_tool.is_some(), "another call's end does not clear the current tool");
        update_activity(&mut a, &tool("c1", "completed", false, Some("npm test"), Some("done")));
        assert!(a.activity.current_tool.is_none());
        assert_eq!(a.activity.last_output_line.as_deref(), Some("done"));
    }

    #[test]
    fn unknown_tool_status_counts_as_running_and_latest_call_wins() {
        let mut a = AgentEntry::default();
        update_activity(&mut a, &tool("c1", "started", false, Some("one"), None));
        update_activity(&mut a, &tool("c2", "weird", false, Some("two"), None));
        let t = a.activity.current_tool.unwrap();
        assert_eq!((t.call_id.as_deref(), t.label.as_str(), t.status.as_str()), (Some("c2"), "two", "weird"));
    }

    #[test]
    fn assistant_deltas_feed_the_tail_and_final_closes_it() {
        let mut a = AgentEntry::default();
        let delta = |t: &str| StreamEvent::AssistantMessage { text: t.into(), is_final: false };
        update_activity(&mut a, &delta("hello\nwor"));
        update_activity(&mut a, &delta("ld"));
        assert_eq!(a.activity.last_output_line.as_deref(), Some("world"));
        update_activity(&mut a, &StreamEvent::AssistantMessage { text: String::new(), is_final: true });
        assert!(a.assistant_tail.is_empty());
        assert_eq!(a.activity.last_output_line.as_deref(), Some("world"));
        update_activity(&mut a, &StreamEvent::AssistantMessage { text: "echo: hi".into(), is_final: true });
        assert_eq!(a.activity.last_output_line.as_deref(), Some("echo: hi"), "a final message that carries its whole text");
        assert!(a.assistant_tail.is_empty());
        update_activity(&mut a, &StreamEvent::TurnStarted);
        assert_eq!((a.activity.last_output_line.clone(), a.activity.turn_state), (None, TurnState::Started));
    }

    #[test]
    fn turn_end_kinds() {
        let mut a = AgentEntry::default();
        let o = update_activity(&mut a, &StreamEvent::TurnFailed { error: Some("boom".into()) });
        assert!(o.failed_turn && o.refresh_stats && a.attention.errored);
        assert_eq!((a.activity.turn_state, a.activity.last_error_message.as_deref()), (TurnState::Failed, Some("boom")));
        let mut b = AgentEntry::default();
        let o = update_activity(&mut b, &StreamEvent::Error { message: Some("oops".into()) });
        assert!(o.error_event && !o.failed_turn && !b.attention.errored);
        assert_eq!(b.activity.last_error_message.as_deref(), Some("oops"));
        assert_eq!(update_activity(&mut b, &StreamEvent::TurnCompleted), Outcome { refresh_stats: true, ..Outcome::default() });
        assert_eq!(update_activity(&mut b, &StreamEvent::TurnCanceled), Outcome::default());
        assert_eq!(b.activity.turn_state, TurnState::Canceled);
    }

    #[test]
    fn huge_inputs_stay_inside_their_caps() {
        let mut a = AgentEntry::default();
        let big = "é".repeat(500_000);
        update_activity(&mut a, &StreamEvent::AssistantMessage { text: big.clone(), is_final: false });
        assert!(a.assistant_tail.len() <= ASSISTANT_TAIL && a.activity.last_output_line.as_ref().unwrap().len() <= OUTPUT_LINE);
        update_activity(&mut a, &StreamEvent::TurnFailed { error: Some(big.clone()) });
        assert!(a.activity.last_error_message.as_ref().unwrap().len() <= ERROR_TEXT);
        update_activity(&mut a, &tool("c", "running", false, Some(&big), None));
        assert!(a.activity.current_tool.as_ref().unwrap().label.len() <= LABEL);
    }
}
