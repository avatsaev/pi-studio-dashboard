//! A password that never prints and is wiped on drop.

use std::fmt;

/// A daemon password. `Debug`/`Display` print `***`; the value is only readable inside this
/// crate; the bytes are overwritten on drop (best effort, no `unsafe`).
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(Vec<u8>);

/// The password is not a valid HTTP token, so it cannot ride in `Sec-WebSocket-Protocol`.
/// Until pi-studio's own encoding is verified nothing is percent-encoded by guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidPassword;

impl fmt::Display for InvalidPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("password contains characters that are not allowed in a WebSocket subprotocol token")
    }
}

impl std::error::Error for InvalidPassword {}

/// RFC 9110 `tchar`.
fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

impl Secret {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().into_bytes())
    }

    /// Non-empty and token characters only.
    ///
    /// # Errors
    /// [`InvalidPassword`] otherwise.
    pub fn validate(&self) -> Result<(), InvalidPassword> {
        if !self.0.is_empty() && self.0.iter().copied().all(is_token_char) { Ok(()) } else { Err(InvalidPassword) }
    }

    #[must_use]
    pub(crate) fn expose(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_and_redaction() {
        assert!(Secret::new("s3cret-._~!#$%&'*+^`|").validate().is_ok());
        for bad in ["", "has space", "semi;colon", "quo\"te", "ünï", "a,b", "a/b", "tab\t"] {
            assert_eq!(Secret::new(bad).validate(), Err(InvalidPassword), "{bad:?}");
        }
        let s = Secret::new("hunter2");
        assert_eq!(format!("{s:?} {s}"), "Secret(***) ***");
        assert_eq!(s.expose(), "hunter2");
    }
}
