//! Agent pushes → `DaemonEvent`s, the stats executor and the timeline pager.
//!
//! The engine decides *when* to fetch stats and timelines (D39); this module only executes:
//! stats requests are deduplicated per agent (in `Shared`) and limited to 4 in flight, a timeline
//! load pages `before` then `after` until `!hasNewer` or 2,000 items, and a newer load cancels
//! the one in progress.

use std::collections::VecDeque;

use futures_util::SinkExt;
use pistudiodashboard_model::{ContextUsage, DaemonEventKind, StatsResult, TokenUsage};
use pistudiodashboard_protocol::{
    AgentArchived, AgentDeleted, AgentStatus, AgentStatusMsg, AgentStream, AgentUpdate, ListAgentsResponse, OutboundRequest, StatsResponse, StreamEvent,
    TimelineDirection, TimelinePage, TimelineReset, encode,
};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

use crate::conn::{Task, Ws};
use crate::requests::{Kind, timeout_for};
use crate::state::{ConnError, ConnErrorKind};
use crate::sync::agent_item;

/// Stats requests in flight per daemon.
pub(crate) const MAX_STATS_IN_FLIGHT: usize = 4;
/// Items fetched per timeline load (gap protection, D17).
const TIMELINE_ITEM_CAP: usize = 2000;
const TIMELINE_PAGE: u32 = 200;

/// Stats requests waiting for one of the 4 slots.
#[derive(Default)]
pub(crate) struct StatsExec {
    waiting: VecDeque<String>,
    in_flight: usize,
}

/// The timeline load in progress (a newer `FetchTimeline` replaces it).
#[derive(Default)]
pub(crate) struct TimelineExec {
    job: u64,
    agent_id: Option<String>,
    fetched: usize,
    next_cursor: Option<String>,
}

/// Mid-stream pushes the dashboard may shed when its consumer is stalled.
pub(crate) fn is_foldable(kind: &DaemonEventKind) -> bool {
    matches!(kind, DaemonEventKind::AgentStream { event, .. } if event.is_foldable())
}

fn labels(map: &std::collections::BTreeMap<String, String>) -> Vec<String> {
    map.iter().map(|(k, v)| format!("{k}={v}")).collect()
}

impl Task {
    /// Map a known agent push to its event. A malformed message is counted and dropped.
    pub(crate) fn agent_push(&mut self, ty: &str, message: &Value) {
        let kind = match ty {
            "agent_update" => AgentUpdate::from_message(message).map(|u| {
                if u.has_unknown_status() {
                    self.counters.unknown_enum_value();
                }
                DaemonEventKind::AgentUpdate {
                    agent_id: u.agent_id,
                    status: u.status,
                    title: u.title,
                    labels: u.labels.as_ref().map(labels),
                    model: u.model,
                    model_provider: u.model_provider,
                }
            }),
            "agent_status" => AgentStatusMsg::from_message(message).map(|s| {
                if AgentStatus::from_wire(&s.status) == AgentStatus::Unknown {
                    self.counters.unknown_enum_value();
                }
                DaemonEventKind::AgentStatus { agent_id: s.agent_id, status: s.status }
            }),
            "agent_archived" => {
                AgentArchived::from_message(message).map(|a| DaemonEventKind::AgentArchived { agent_id: a.agent_id, archived_at: a.archived_at })
            }
            "agent_deleted" => AgentDeleted::from_message(message).map(|d| DaemonEventKind::AgentDeleted { agent_id: d.agent_id }),
            "agent_timeline_reset" => TimelineReset::from_message(message).map(|r| DaemonEventKind::TimelineReset { agent_id: r.agent_id, reason: r.reason }),
            "agent_stream" => AgentStream::from_message(message).map(|s| {
                if matches!(s.event, StreamEvent::Unknown { .. }) {
                    self.counters.unknown_enum_value();
                }
                DaemonEventKind::AgentStream { agent_id: s.agent_id, seq: s.seq, timestamp: s.timestamp, event: s.event }
            }),
            _ => return,
        };
        match kind {
            Some(k) => self.out.push(k),
            None => self.counters.malformed(),
        }
    }

    /// Emit what frame handling produced; foldable stream events are shed (and counted) when the
    /// consumer is behind, everything else waits for room (TCP backpressure).
    pub(crate) async fn flush_out(&mut self) {
        for kind in std::mem::take(&mut self.out) {
            if is_foldable(&kind) {
                if !self.em.try_emit(kind) {
                    self.counters.dropped_event();
                }
            } else {
                self.em.emit(kind).await;
            }
        }
    }

    /// Start the executor-side handling of `SessionStats` / `FetchTimeline`. `None` when the
    /// request is not one of those (the caller handles it generically).
    pub(crate) async fn start_special(&mut self, ws: &mut Ws, request: &OutboundRequest) -> Option<Result<(), ConnError>> {
        match request {
            OutboundRequest::SessionStats { agent_id } => {
                self.stats.waiting.push_back(agent_id.clone());
                Some(self.pump_stats(ws).await)
            }
            OutboundRequest::FetchTimeline { agent_id } => {
                self.timeline.job += 1;
                self.timeline.agent_id = Some(agent_id.clone());
                self.timeline.fetched = 0;
                self.timeline.next_cursor = None;
                let job = self.timeline.job;
                Some(self.send_timeline(ws, request, Kind::Timeline { job, first: true }).await)
            }
            _ => None,
        }
    }

    async fn send_timeline(&mut self, ws: &mut Ws, request: &OutboundRequest, kind: Kind) -> Result<(), ConnError> {
        let id = self.rpc.start(kind, timeout_for(request));
        ws.send(Message::text(encode(request, &id))).await.map_err(|e| ConnError::new(ConnErrorKind::Closed, e.to_string()))
    }

    /// Send queued stats requests while slots are free, and the next timeline page if one is due.
    pub(crate) async fn pump(&mut self, ws: &mut Ws) -> Option<ConnError> {
        if let Err(e) = self.pump_stats(ws).await {
            return Some(e);
        }
        let cursor = self.timeline.next_cursor.take()?;
        let agent_id = self.timeline.agent_id.clone()?;
        let request = OutboundRequest::FetchTimelinePage { agent_id, cursor: Some(cursor), direction: TimelineDirection::After, limit: TIMELINE_PAGE };
        let job = self.timeline.job;
        self.send_timeline(ws, &request, Kind::Timeline { job, first: false }).await.err()
    }

    async fn pump_stats(&mut self, ws: &mut Ws) -> Result<(), ConnError> {
        while self.stats.in_flight < MAX_STATS_IN_FLIGHT {
            let Some(agent_id) = self.stats.waiting.pop_front() else { break };
            let request = OutboundRequest::SessionStats { agent_id: agent_id.clone() };
            self.stats.in_flight += 1;
            self.send_timeline(ws, &request, Kind::Stats { agent_id }).await?;
        }
        Ok(())
    }

    /// A stats request ended (answered, rejected or timed out). Only an answer yields an event;
    /// nothing is retried.
    pub(crate) fn finish_stats(&mut self, agent_id: &str, message: Option<&Value>) {
        self.stats.in_flight = self.stats.in_flight.saturating_sub(1);
        self.shared.stats_done(agent_id);
        let Some(m) = message.filter(|m| m.get("type").and_then(Value::as_str) == Some("agent_session_stats_response")) else { return };
        let Some(r) = StatsResponse::from_message(m) else {
            self.counters.malformed();
            return;
        };
        let result = if r.ok {
            StatsResult::Usage {
                tokens: r.tokens.map(|t| TokenUsage { input: t.input, output: t.output, total: t.total }),
                cost: r.cost,
                context_usage: r.context_usage.map(|c| ContextUsage { tokens: c.tokens, window: c.context_window, percent: c.percent }),
            }
        } else {
            StatsResult::NoSession
        };
        self.out.push(DaemonEventKind::Stats { agent_id: agent_id.to_owned(), result });
    }

    /// A post-sync `list_agents_response` arrived (or the request failed; nothing is retried, the
    /// next new agent asks again). A malformed answer is counted and dropped.
    pub(crate) fn finish_agent_list(&mut self, message: Option<&Value>) {
        let Some(m) = message.filter(|m| m.get("type").and_then(Value::as_str) == Some("list_agents_response")) else { return };
        let Some(list) = ListAgentsResponse::from_message(m) else {
            self.counters.malformed();
            return;
        };
        for _ in 0..list.malformed {
            self.counters.malformed();
        }
        for _ in 0..list.unknown_status {
            self.counters.unknown_enum_value();
        }
        self.out.push(DaemonEventKind::AgentsListed { agents: list.agents.iter().map(agent_item).collect() });
    }

    /// A timeline page arrived (or the request failed). A page of a cancelled load is dropped.
    pub(crate) fn finish_timeline(&mut self, job: u64, first: bool, message: Option<&Value>) {
        if job != self.timeline.job {
            return;
        }
        let Some(m) = message.filter(|m| m.get("type").and_then(Value::as_str) == Some("fetch_agent_timeline_response")) else { return };
        let Some(page) = TimelinePage::from_message(m) else {
            self.counters.malformed();
            return;
        };
        for _ in 0..page.malformed {
            self.counters.malformed();
        }
        for _ in 0..page.unknown_kinds {
            self.counters.unknown_enum_value();
        }
        let Some(agent_id) = self.timeline.agent_id.clone() else { return };
        self.timeline.fetched += page.items.len();
        if page.has_newer && self.timeline.fetched < TIMELINE_ITEM_CAP {
            self.timeline.next_cursor = page.end_cursor.clone();
        }
        self.out.push(DaemonEventKind::Timeline { agent_id, items: page.items, reset: first, seq_end: page.seq_end });
    }

    /// A reconnect or removal ends everything in flight.
    pub(crate) fn reset_executors(&mut self) {
        self.stats = StatsExec::default();
        self.timeline.next_cursor = None;
        self.timeline.job += 1;
    }
}
