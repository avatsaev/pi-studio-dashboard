//! File access: load with recovery, atomic save. Native only.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use crate::model::{Config, ConfigIssue, IssueKind, MAX_FILE_BYTES, SUPPORTED_VERSION};
use crate::parse::{parse, serialize};

pub const CONFIG_FILE: &str = "config.json";
pub const BACKUP_FILE: &str = "config.json.bak";
pub const TEMP_FILE: &str = "config.json.tmp";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config is read-only (written by a newer version, or unreadable at load)")]
    ReadOnly,
    #[error("config I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadOutcome {
    pub config: Config,
    pub issues: Vec<ConfigIssue>,
    /// The old file was unreadable JSON and was moved to `config.json.bak`.
    pub recovered_from_corrupt: bool,
    /// Never save: the file is from a newer version, or could not be read (so it must not be
    /// overwritten). Pass this to [`save`].
    pub read_only: bool,
}

fn defaults(issues: Vec<ConfigIssue>, recovered: bool, read_only: bool) -> LoadOutcome {
    LoadOutcome { config: Config::default(), issues, recovered_from_corrupt: recovered, read_only }
}

/// Loads `dir/config.json`. `new_id` supplies fresh entry ids (see [`parse`]).
pub fn load(dir: &Path, new_id: &mut dyn FnMut() -> String) -> LoadOutcome {
    let path = dir.join(CONFIG_FILE);
    let mut bytes = Vec::new();
    let read = File::open(&path).and_then(|f| f.take(MAX_FILE_BYTES as u64 + 1).read_to_end(&mut bytes));
    match read {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return defaults(Vec::new(), false, false),
        Err(e) => {
            let issue = ConfigIssue::new(IssueKind::Io, CONFIG_FILE, format!("could not read config: {e}"));
            return defaults(vec![issue], false, true);
        }
    }
    let mut outcome = parse(&bytes, new_id);
    if outcome.issues.iter().any(|i| i.kind == IssueKind::CorruptFile) {
        let mut issues = outcome.issues;
        if let Err(e) = fs::rename(&path, dir.join(BACKUP_FILE)) {
            issues.push(ConfigIssue::new(IssueKind::Io, BACKUP_FILE, format!("could not save backup: {e}")));
        }
        return defaults(issues, true, false);
    }
    let read_only = outcome.config.version > SUPPORTED_VERSION;
    if read_only {
        outcome.issues.push(ConfigIssue::new(
            IssueKind::FutureVersion,
            "version",
            format!("file version {} is newer than supported {SUPPORTED_VERSION}; changes will not be saved", outcome.config.version),
        ));
    }
    LoadOutcome { config: outcome.config, issues: outcome.issues, recovered_from_corrupt: false, read_only }
}

/// Atomically replaces `dir/config.json`; on any error the old file is untouched.
/// Refuses with [`ConfigError::ReadOnly`] when `read_only` (from [`load`]) is set.
pub fn save(dir: &Path, config: &Config, read_only: bool) -> Result<(), ConfigError> {
    if read_only {
        return Err(ConfigError::ReadOnly);
    }
    create_dir(dir)?;
    let tmp = dir.join(TEMP_FILE);
    if let Err(e) = write_tmp(&tmp, &serialize(config)).and_then(|()| fs::rename(&tmp, dir.join(CONFIG_FILE))) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

fn create_dir(dir: &Path) -> std::io::Result<()> {
    let mut b = fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
    b.create(dir)
}

fn write_tmp(tmp: &Path, bytes: &[u8]) -> std::io::Result<()> {
    // A leftover temp file may have other permissions; mode only applies on creation.
    match fs::remove_file(tmp) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    let mut f = o.open(tmp)?;
    f.write_all(bytes)?;
    f.sync_all()
}
