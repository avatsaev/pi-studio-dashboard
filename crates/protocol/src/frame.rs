//! Top-level frame envelope: `hello`, `status`, `ping`, `pong`, `session`, plus a fallback.

use serde_json::{Map, Value, json};

use crate::lenient::wire_time_ms;

/// Maximum accepted frame size in bytes (16 MiB). Larger frames are rejected before parsing.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// A decoded top-level frame. Payloads stay as [`Value`]; typed messages live in later modules.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// The whole `hello` frame object (including its `type`).
    Hello(Value),
    Status {
        payload: Value,
    },
    Ping {
        request_id: Option<String>,
        client_sent_at: Option<i64>,
    },
    Pong {
        request_id: Option<String>,
        client_sent_at: Option<i64>,
        server_received_at: Option<i64>,
        server_sent_at: Option<i64>,
    },
    Session {
        message: Value,
    },
    /// Any other top-level `type`; never an error.
    Unknown {
        type_: String,
    },
}

/// Why a frame could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("frame of {bytes} bytes exceeds the {MAX_FRAME_BYTES}-byte cap")]
    TooLarge { bytes: usize },
    #[error("frame is not valid JSON")]
    NotJson,
    #[error("frame is not a JSON object")]
    NotObject,
    #[error("frame has no string `type`")]
    MissingType,
}

fn opt_string(o: &Map<String, Value>, k: &str) -> Option<String> {
    o.get(k).and_then(Value::as_str).map(str::to_owned)
}

fn opt_time(o: &Map<String, Value>, k: &str) -> Option<i64> {
    o.get(k).and_then(wire_time_ms)
}

/// Decode one text frame. Size is checked before parsing.
///
/// # Errors
/// [`FrameError`] for oversize, non-JSON, non-object or type-less input.
pub fn decode_frame(text: &str) -> Result<Frame, FrameError> {
    if text.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge { bytes: text.len() });
    }
    let value: Value = serde_json::from_str(text).map_err(|_| FrameError::NotJson)?;
    let Value::Object(mut o) = value else {
        return Err(FrameError::NotObject);
    };
    let Some(ty) = o.get("type").and_then(Value::as_str).map(str::to_owned) else {
        return Err(FrameError::MissingType);
    };
    Ok(match ty.as_str() {
        "hello" => Frame::Hello(Value::Object(o)),
        "status" => Frame::Status { payload: o.remove("payload").unwrap_or(Value::Null) },
        "session" => Frame::Session { message: o.remove("message").unwrap_or(Value::Null) },
        "ping" => Frame::Ping { request_id: opt_string(&o, "requestId"), client_sent_at: opt_time(&o, "clientSentAt") },
        "pong" => Frame::Pong {
            request_id: opt_string(&o, "requestId"),
            client_sent_at: opt_time(&o, "clientSentAt"),
            server_received_at: opt_time(&o, "serverReceivedAt"),
            server_sent_at: opt_time(&o, "serverSentAt"),
        },
        _ => Frame::Unknown { type_: ty },
    })
}

fn put<T: Into<Value>>(o: &mut Map<String, Value>, k: &str, v: Option<T>) {
    if let Some(v) = v {
        o.insert(k.to_owned(), v.into());
    }
}

/// Encode a frame as JSON text (round-trip support; not the outbound allowlist encoder).
#[must_use]
pub fn encode_frame(frame: &Frame) -> String {
    let mut o = Map::new();
    let ty = match frame {
        Frame::Hello(v) => {
            if let Value::Object(m) = v {
                o = m.clone();
            }
            "hello"
        }
        Frame::Status { payload } => {
            o.insert("payload".into(), payload.clone());
            "status"
        }
        Frame::Session { message } => {
            o.insert("message".into(), message.clone());
            "session"
        }
        Frame::Ping { request_id, client_sent_at } => {
            put(&mut o, "requestId", request_id.clone());
            put(&mut o, "clientSentAt", *client_sent_at);
            "ping"
        }
        Frame::Pong { request_id, client_sent_at, server_received_at, server_sent_at } => {
            put(&mut o, "requestId", request_id.clone());
            put(&mut o, "clientSentAt", *client_sent_at);
            put(&mut o, "serverReceivedAt", *server_received_at);
            put(&mut o, "serverSentAt", *server_sent_at);
            "pong"
        }
        Frame::Unknown { type_ } => {
            o.insert("type".into(), json!(type_));
            return Value::Object(o).to_string();
        }
    };
    o.insert("type".into(), json!(ty));
    Value::Object(o).to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn rt(text: &str) -> Frame {
        let f = decode_frame(text).unwrap();
        assert_eq!(decode_frame(&encode_frame(&f)).unwrap(), f);
        f
    }

    #[test]
    fn known_frames_round_trip() {
        assert_eq!(rt(r#"{"type":"ping"}"#), Frame::Ping { request_id: None, client_sent_at: None });
        assert_eq!(
            rt(r#"{"type":"pong","requestId":"r","serverReceivedAt":1,"serverSentAt":"1970-01-01T00:00:00.002Z"}"#),
            Frame::Pong { request_id: Some("r".into()), client_sent_at: None, server_received_at: Some(1), server_sent_at: Some(2) }
        );
        assert_eq!(rt(r#"{"type":"status","payload":{"a":1}}"#), Frame::Status { payload: json!({"a":1}) });
        assert_eq!(rt(r#"{"type":"session","message":{"type":"x"}}"#), Frame::Session { message: json!({"type":"x"}) });
        assert!(matches!(rt(r#"{"type":"hello","protocolVersion":1}"#), Frame::Hello(_)));
    }

    #[test]
    fn unknown_fields_tolerated() {
        let f = rt(r#"{"type":"pong","extra":{"deep":[1]},"requestId":"r","zzz":null}"#);
        assert!(matches!(f, Frame::Pong { .. }));
        rt(r#"{"type":"session","message":{"type":"x","new":true},"more":1}"#);
    }

    #[test]
    fn unknown_type_is_fallback() {
        assert_eq!(rt(r#"{"type":"terminals_update"}"#), Frame::Unknown { type_: "terminals_update".into() });
    }

    #[test]
    fn hostile_inputs() {
        assert_eq!(decode_frame("{nope"), Err(FrameError::NotJson));
        assert_eq!(decode_frame(""), Err(FrameError::NotJson));
        assert_eq!(decode_frame("[1]"), Err(FrameError::NotObject));
        assert_eq!(decode_frame("5"), Err(FrameError::NotObject));
        assert_eq!(decode_frame("{}"), Err(FrameError::MissingType));
        assert_eq!(decode_frame(r#"{"type":3}"#), Err(FrameError::MissingType));
    }

    #[test]
    fn size_cap_boundary() {
        let prefix = r#"{"type":"ping","pad":""#;
        let suffix = r#""}"#;
        let fill = MAX_FRAME_BYTES - prefix.len() - suffix.len();
        let mut s = String::with_capacity(MAX_FRAME_BYTES + 1);
        s.push_str(prefix);
        s.extend(std::iter::repeat_n('a', fill));
        s.push_str(suffix);
        assert_eq!(s.len(), MAX_FRAME_BYTES);
        assert!(decode_frame(&s).is_ok());
        s.push(' ');
        assert_eq!(decode_frame(&s), Err(FrameError::TooLarge { bytes: MAX_FRAME_BYTES + 1 }));
        assert!(FrameError::TooLarge { bytes: 1 }.to_string().contains("cap"));
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..1000 {
            let mut bytes = Vec::new();
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            for _ in 0..(x % 64) {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                bytes.push(x.to_le_bytes()[0]);
            }
            let _ = decode_frame(&String::from_utf8_lossy(&bytes));
        }
    }
}
