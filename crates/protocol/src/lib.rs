//! pi-studio wire protocol (L0): top-level frame envelope, handshake/liveness/error/health
//! types and lenient decoding primitives.
//!
//! The daemon has no role gating and sends whatever a newer version sends, so decoding is
//! lenient (unknown fields and types are tolerated) and hostile-input safe (16 MiB cap, no
//! panics). Typed messages build on [`lenient`] in modules next to [`frame`].
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod agent;
pub mod error;
pub mod frame;
pub mod handshake;
pub mod health;
pub mod inbound;
pub mod lenient;
pub mod liveness;
pub mod outbound;
pub mod push;
pub mod reply;
pub mod stats;
pub mod stream;
pub mod sync;
pub mod timeline;

pub use agent::{AgentArchived, AgentDeleted, AgentStatusMsg, AgentUpdate, TimelineReset};
pub use error::RpcError;
pub use frame::{Frame, FrameError, MAX_FRAME_BYTES, decode_frame, encode_frame};
pub use handshake::{ClientType, DegradedReason, HandshakeError, Hello, ServerCaps, ServerInfo};
pub use health::HealthBody;
pub use inbound::{Inbound, decode_inbound};
pub use lenient::{WireTime, cap_str, lenient_opt, wire_time_ms};
pub use liveness::{Ping, Pong};
pub use outbound::{MUTATION_ALLOWLIST, OutboundRequest, RequestClass, TimelineDirection, UiResponse, all_variants, encode};
pub use push::UiResolved;
pub use reply::{InterruptReply, RespondUiReply};
pub use stats::{ContextUsageWire, StatsResponse, Tokens};
pub use stream::{AgentStream, StreamEvent, ToolInfo};
pub use sync::{AgentStatus, ListAgentsResponse, ListedAgent, UiListResponse, WireUiRequest};
pub use timeline::{TimelineItem, TimelinePage};
