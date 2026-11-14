//! `GET /api/health` body.

use serde::Deserialize;

use crate::lenient::lenient_opt;

/// `{status:"ok"}`; extra fields are tolerated.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HealthBody {
    #[serde(default, deserialize_with = "lenient_opt")]
    pub status: Option<String>,
}

impl HealthBody {
    /// Decode a response body.
    ///
    /// # Errors
    /// When the body is not a JSON object.
    pub fn decode(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.status.as_deref() == Some("ok")
    }
}
