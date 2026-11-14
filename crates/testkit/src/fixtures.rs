//! Loader for the vendored pi-studio wire fixtures (`manifest.json` + one file per message).
//!
//! Format: pi-studio `packages/protocol/fixtures/external-clients/README.md`. Parsing is done on
//! `serde_json::Value` by hand so testkit needs no serde derive and no workspace dependency.

use serde_json::Value;
use std::fmt;
use std::path::{Path, PathBuf};

/// Highest manifest `formatVersion` this loader understands.
pub const SUPPORTED_FORMAT_VERSION: u32 = 1;

/// Why a manifest or fixture could not be loaded.
#[derive(Debug)]
pub enum FixtureError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// A required field is absent or has the wrong JSON type.
    Shape {
        path: PathBuf,
        what: String,
    },
    /// The manifest is newer than this loader.
    UnsupportedVersion {
        found: u64,
        supported: u32,
    },
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Json { path, source } => write!(f, "{}: invalid JSON: {source}", path.display()),
            Self::Shape { path, what } => write!(f, "{}: {what}", path.display()),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "unsupported fixture formatVersion {found}; this harness supports formatVersion {supported} (re-vendor procedure: crates/protocol/tests/fixtures/README.md)"
            ),
        }
    }
}

impl std::error::Error for FixtureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// One `messages[]` entry of `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    pub file: String,
    pub type_: String,
    pub direction: String,
    /// Client requests only: the fixture the daemon answers with.
    pub response: Option<String>,
    /// Why no daemon code path emits this fixture (schema-only), as a free-form string.
    pub exception: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub format_version: u32,
    pub messages: Vec<ManifestEntry>,
}

/// One fixture file.
#[derive(Debug, Clone, PartialEq)]
pub struct Fixture {
    pub type_: String,
    pub direction: String,
    pub since: String,
    pub requires_feature: Option<String>,
    /// JSON Pointers into `frame` whose values change per run.
    pub volatile: Vec<String>,
    /// The complete frame as sent on the socket (`{status, contentType, body}` for `http`).
    pub frame: Value,
}

/// `crates/protocol/tests/fixtures/pi-studio`, vendored verbatim.
pub fn pi_studio_dir() -> PathBuf {
    fixtures_root().join("pi-studio")
}

/// `crates/protocol/tests/fixtures/local`, hand-written degraded frames.
pub fn local_dir() -> PathBuf {
    fixtures_root().join("local")
}

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../protocol/tests/fixtures")
}

fn read_json(path: &Path) -> Result<Value, FixtureError> {
    let text = std::fs::read_to_string(path).map_err(|source| FixtureError::Io { path: path.to_path_buf(), source })?;
    serde_json::from_str(&text).map_err(|source| FixtureError::Json { path: path.to_path_buf(), source })
}

fn shape(path: &Path, what: impl Into<String>) -> FixtureError {
    FixtureError::Shape { path: path.to_path_buf(), what: what.into() }
}

fn req_str(path: &Path, obj: &Value, key: &str) -> Result<String, FixtureError> {
    obj.get(key).and_then(Value::as_str).map(str::to_owned).ok_or_else(|| shape(path, format!("`{key}` must be a string")))
}

fn opt_str(path: &Path, obj: &Value, key: &str) -> Result<Option<String>, FixtureError> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(shape(path, format!("`{key}` must be a string or null"))),
    }
}

/// Load `<dir>/manifest.json`; fails loudly on a `formatVersion` newer than
/// [`SUPPORTED_FORMAT_VERSION`].
pub fn load_manifest(dir: &Path) -> Result<Manifest, FixtureError> {
    let path = dir.join("manifest.json");
    let root = read_json(&path)?;
    let version = root.get("formatVersion").and_then(Value::as_u64).ok_or_else(|| shape(&path, "`formatVersion` must be a non-negative integer"))?;
    if version > u64::from(SUPPORTED_FORMAT_VERSION) {
        return Err(FixtureError::UnsupportedVersion { found: version, supported: SUPPORTED_FORMAT_VERSION });
    }
    let list = root.get("messages").and_then(Value::as_array).ok_or_else(|| shape(&path, "`messages` must be an array"))?;
    let mut messages = Vec::with_capacity(list.len());
    for item in list {
        messages.push(ManifestEntry {
            file: req_str(&path, item, "file")?,
            type_: req_str(&path, item, "type")?,
            direction: req_str(&path, item, "direction")?,
            response: opt_str(&path, item, "response")?,
            exception: opt_str(&path, item, "exception")?,
        });
    }
    Ok(Manifest { format_version: version as u32, messages })
}

/// Load the fixture file a manifest entry names.
pub fn load_fixture(dir: &Path, entry: &ManifestEntry) -> Result<Fixture, FixtureError> {
    load_fixture_file(dir, &entry.file)
}

/// Load one fixture by file name (also used for files that are not in a manifest).
pub fn load_fixture_file(dir: &Path, file: &str) -> Result<Fixture, FixtureError> {
    let path = dir.join(file);
    let root = read_json(&path)?;
    let volatile = match root.get("volatile") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => {
            items.iter().map(|v| v.as_str().map(str::to_owned).ok_or_else(|| shape(&path, "`volatile` entries must be strings"))).collect::<Result<_, _>>()?
        }
        Some(_) => return Err(shape(&path, "`volatile` must be an array")),
    };
    let frame = root.get("frame").cloned().ok_or_else(|| shape(&path, "`frame` is missing"))?;
    Ok(Fixture {
        type_: req_str(&path, &root, "type")?,
        direction: req_str(&path, &root, "direction")?,
        since: req_str(&path, &root, "since")?,
        requires_feature: opt_str(&path, &root, "requiresFeature")?,
        volatile,
        frame,
    })
}
