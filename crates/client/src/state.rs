//! Connection error vocabulary shared by the state machine and the emitted events.

use pistudiodashboard_model::ConnErrorView;

/// Why a connection attempt or a live connection ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnErrorKind {
    Unreachable,
    AuthFailed,
    HostRejected,
    BadHandshake,
    /// Three consecutive handshake failures in a row.
    HandshakeRejected,
    FrameTooLarge,
    Closed,
    Timeout,
}

impl ConnErrorKind {
    /// The fixed wire/event code.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::AuthFailed => "auth_failed",
            Self::HostRejected => "host_rejected",
            Self::BadHandshake => "bad_handshake",
            Self::HandshakeRejected => "handshake_rejected",
            Self::FrameTooLarge => "frame_too_large",
            Self::Closed => "closed",
            Self::Timeout => "timeout",
        }
    }
}

/// A classified failure with bounded, secret-free text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnError {
    pub kind: ConnErrorKind,
    pub message: String,
}

impl ConnError {
    pub(crate) fn new(kind: ConnErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }

    pub(crate) fn view(&self) -> ConnErrorView {
        ConnErrorView { code: self.kind.code().to_owned(), message: Some(self.message.clone()) }
    }
}
