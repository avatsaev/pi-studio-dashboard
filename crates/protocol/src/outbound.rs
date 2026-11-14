//! Outbound requests: a closed enum and the only encoder (D2, D36, D37).
//!
//! The daemon has no role gating, so this module is the guard: every request the dashboard can
//! put on the wire is a variant of [`OutboundRequest`], and exactly two wire types are
//! mutations ([`MUTATION_ALLOWLIST`]). There is no way to encode arbitrary JSON.

use serde_json::{Map, Value, json};

/// The only wire types that change daemon state (D2 as revised by D36, D37).
pub const MUTATION_ALLOWLIST: [&str; 2] = ["agent_ui_respond_request", "interrupt_agent"];

/// Every read wire type the dashboard sends.
pub const READ_TYPES: [&str; 5] = ["ping", "list_agents_request", "agent_ui_list_request", "agent_session_stats_request", "fetch_agent_timeline_request"];

/// Page size of the first drill-down page ([`OutboundRequest::FetchTimeline`]).
pub const TIMELINE_PAGE_LIMIT: u32 = 200;

/// Read or mutation; derived from [`MUTATION_ALLOWLIST`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestClass {
    Read,
    Mutation,
}

/// The answer to a dialog (`confirm` or `select`); the wire object has exactly one key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiResponse {
    Confirm { confirmed: bool },
    Select { value: String },
}

/// Paging direction of a timeline page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineDirection {
    Before,
    After,
}

/// A request the dashboard may send to a daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutboundRequest {
    /// Top-level JSON ping; the daemon answers with a `pong`.
    Ping {
        client_sent_at: i64,
    },
    ListAgents,
    ListUi {
        agent_id: Option<String>,
    },
    /// One usage snapshot for an agent.
    SessionStats {
        agent_id: String,
    },
    /// One logical timeline (re)load for an agent (D39): encoded as the first page
    /// (`before`, [`TIMELINE_PAGE_LIMIT`], no cursor). The client does the paging with
    /// [`OutboundRequest::FetchTimelinePage`].
    FetchTimeline {
        agent_id: String,
    },
    /// One explicit timeline page; issued by the client's paging loop.
    FetchTimelinePage {
        agent_id: String,
        cursor: Option<String>,
        direction: TimelineDirection,
        limit: u32,
    },
    /// Mutation 1: answer a Pi dialog.
    RespondUi {
        ui_request_id: String,
        response: UiResponse,
    },
    /// Mutation 2: interrupt a running agent.
    InterruptAgent {
        agent_id: String,
    },
}

impl OutboundRequest {
    /// The wire `type`. Exhaustive: a new variant must be classified here before it compiles.
    #[must_use]
    pub fn wire_type(&self) -> &'static str {
        match self {
            Self::Ping { .. } => "ping",
            Self::ListAgents => "list_agents_request",
            Self::ListUi { .. } => "agent_ui_list_request",
            Self::SessionStats { .. } => "agent_session_stats_request",
            Self::FetchTimeline { .. } | Self::FetchTimelinePage { .. } => "fetch_agent_timeline_request",
            Self::RespondUi { .. } => "agent_ui_respond_request",
            Self::InterruptAgent { .. } => "interrupt_agent",
        }
    }

    /// `Mutation` iff the wire type is in [`MUTATION_ALLOWLIST`].
    #[must_use]
    pub fn class(&self) -> RequestClass {
        if MUTATION_ALLOWLIST.contains(&self.wire_type()) { RequestClass::Mutation } else { RequestClass::Read }
    }

    /// The agent this request is about, when it has one.
    #[must_use]
    pub fn agent_id(&self) -> Option<&str> {
        match self {
            Self::SessionStats { agent_id }
            | Self::FetchTimeline { agent_id }
            | Self::FetchTimelinePage { agent_id, .. }
            | Self::InterruptAgent { agent_id } => Some(agent_id),
            Self::ListUi { agent_id } => agent_id.as_deref(),
            Self::Ping { .. } | Self::ListAgents | Self::RespondUi { .. } => None,
        }
    }
}

/// The one encode function: a text frame for `request` tagged `request_id`. Everything except
/// `Ping` is wrapped `{type:"session", message:{type, requestId, …}}`.
#[must_use]
pub fn encode(request: &OutboundRequest, request_id: &str) -> String {
    let mut m = Map::new();
    m.insert("type".into(), request.wire_type().into());
    m.insert("requestId".into(), request_id.into());
    let mut put = |k: &str, v: Value| {
        m.insert(k.into(), v);
    };
    match request {
        OutboundRequest::Ping { client_sent_at } => {
            put("clientSentAt", (*client_sent_at).into());
            return Value::Object(m).to_string();
        }
        OutboundRequest::ListAgents => {}
        OutboundRequest::ListUi { agent_id } => {
            if let Some(id) = agent_id {
                put("agentId", id.clone().into());
            }
        }
        OutboundRequest::SessionStats { agent_id } | OutboundRequest::InterruptAgent { agent_id } => put("agentId", agent_id.clone().into()),
        OutboundRequest::FetchTimeline { agent_id } => {
            put("agentId", agent_id.clone().into());
            put("direction", "before".into());
            put("limit", TIMELINE_PAGE_LIMIT.into());
        }
        OutboundRequest::FetchTimelinePage { agent_id, cursor, direction, limit } => {
            put("agentId", agent_id.clone().into());
            if let Some(c) = cursor {
                put("cursor", c.clone().into());
            }
            put(
                "direction",
                match direction {
                    TimelineDirection::Before => "before",
                    TimelineDirection::After => "after",
                }
                .into(),
            );
            put("limit", (*limit).into());
        }
        OutboundRequest::RespondUi { ui_request_id, response } => {
            put("uiRequestId", ui_request_id.clone().into());
            put(
                "response",
                match response {
                    UiResponse::Confirm { confirmed } => json!({ "confirmed": confirmed }),
                    UiResponse::Select { value } => json!({ "value": value }),
                },
            );
        }
    }
    json!({ "type": "session", "message": Value::Object(m) }).to_string()
}

/// Every variant once. Test-visible: a unit test with an exhaustive `match` fails to compile
/// when a variant is added without extending this list.
#[must_use]
pub fn all_variants() -> Vec<OutboundRequest> {
    vec![
        OutboundRequest::Ping { client_sent_at: 1 },
        OutboundRequest::ListAgents,
        OutboundRequest::ListUi { agent_id: None },
        OutboundRequest::ListUi { agent_id: Some("a".into()) },
        OutboundRequest::SessionStats { agent_id: "a".into() },
        OutboundRequest::FetchTimeline { agent_id: "a".into() },
        OutboundRequest::FetchTimelinePage { agent_id: "a".into(), cursor: Some("27".into()), direction: TimelineDirection::After, limit: 100 },
        OutboundRequest::FetchTimelinePage { agent_id: "a".into(), cursor: None, direction: TimelineDirection::Before, limit: 100 },
        OutboundRequest::RespondUi { ui_request_id: "u".into(), response: UiResponse::Confirm { confirmed: true } },
        OutboundRequest::RespondUi { ui_request_id: "u".into(), response: UiResponse::Select { value: "a".into() } },
        OutboundRequest::InterruptAgent { agent_id: "a".into() },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time guard: adding a variant breaks this match until `all_variants` is extended.
    fn covered(r: &OutboundRequest) -> usize {
        match r {
            OutboundRequest::Ping { .. } => 0,
            OutboundRequest::ListAgents => 1,
            OutboundRequest::ListUi { .. } => 2,
            OutboundRequest::SessionStats { .. } => 3,
            OutboundRequest::FetchTimeline { .. } => 4,
            OutboundRequest::FetchTimelinePage { .. } => 5,
            OutboundRequest::RespondUi { .. } => 6,
            OutboundRequest::InterruptAgent { .. } => 7,
        }
    }

    #[test]
    fn all_variants_covers_every_variant() {
        let mut seen: Vec<usize> = all_variants().iter().map(covered).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen, (0..8).collect::<Vec<_>>());
    }

    #[test]
    fn wire_types_are_the_documented_names() {
        assert_eq!(OutboundRequest::SessionStats { agent_id: "a".into() }.wire_type(), "agent_session_stats_request");
        assert_eq!(OutboundRequest::FetchTimeline { agent_id: "a".into() }.wire_type(), "fetch_agent_timeline_request");
    }

    #[test]
    fn first_timeline_page_has_no_cursor() {
        let v: Value = serde_json::from_str(&encode(&OutboundRequest::FetchTimeline { agent_id: "a".into() }, "r")).unwrap();
        assert_eq!(v["message"]["direction"], "before");
        assert_eq!(v["message"]["limit"], 200);
        assert!(v["message"].get("cursor").is_none());
    }
}
