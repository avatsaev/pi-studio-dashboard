//! Typed inbound decode entry point built on the envelope in [`crate::frame`].

use serde_json::Value;

use crate::error::RpcError;
use crate::frame::{Frame, FrameError, decode_frame};
use crate::handshake::{HandshakeError, ServerInfo};
use crate::liveness::Pong;

/// A decoded server-to-client text frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Inbound {
    /// `status` with `payload.status == "server_info"`.
    ServerInfo(ServerInfo),
    /// A `server_info` that lacks a usable `serverId`.
    BadHandshake(HandshakeError),
    /// Any other `status` payload (typed in later tasks).
    Status {
        payload: Value,
    },
    Pong(Pong),
    RpcError(RpcError),
    /// Any other session message, still raw (typed in later tasks).
    Session {
        message: Value,
    },
    /// A known frame type missing a required field; the caller counts `malformed`.
    Malformed {
        type_: &'static str,
    },
    /// Any other top-level `type`, including a (never sent) server `hello` or `ping`.
    Unknown {
        type_: String,
    },
}

/// Decode one text frame into a typed [`Inbound`].
///
/// # Errors
/// [`FrameError`] for oversize, non-JSON, non-object or type-less input.
pub fn decode_inbound(text: &str) -> Result<Inbound, FrameError> {
    Ok(match decode_frame(text)? {
        Frame::Status { payload } => {
            if payload.get("status").and_then(Value::as_str) == Some("server_info") {
                match ServerInfo::from_payload(&payload) {
                    Ok(info) => Inbound::ServerInfo(info),
                    Err(e) => Inbound::BadHandshake(e),
                }
            } else {
                Inbound::Status { payload }
            }
        }
        Frame::Pong { request_id, client_sent_at, server_received_at, server_sent_at } => {
            match Pong::from_parts(request_id, server_received_at, server_sent_at, client_sent_at) {
                Some(p) => Inbound::Pong(p),
                None => Inbound::Malformed { type_: "pong" },
            }
        }
        Frame::Session { message } => {
            if message.get("type").and_then(Value::as_str) == Some("rpc_error") {
                match RpcError::from_message(&message) {
                    Some(e) => Inbound::RpcError(e),
                    None => Inbound::Malformed { type_: "rpc_error" },
                }
            } else {
                Inbound::Session { message }
            }
        }
        Frame::Hello(_) => Inbound::Unknown { type_: "hello".into() },
        Frame::Ping { .. } => Inbound::Unknown { type_: "ping".into() },
        Frame::Unknown { type_ } => Inbound::Unknown { type_ },
    })
}
