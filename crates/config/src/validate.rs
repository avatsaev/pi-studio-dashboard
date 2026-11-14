//! Field validation shared by `daemon.add/edit` and by `parse`.

use std::net::Ipv6Addr;

use crate::model::{Config, DaemonEntry};

/// A validation failure keyed by the (camelCase) field name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: &'static str,
    pub message: String,
}

fn err(field: &'static str, message: impl Into<String>) -> FieldError {
    FieldError { field, message: message.into() }
}

pub(crate) fn check_name(name: &str) -> Result<(), String> {
    if name.chars().any(char::is_control) {
        return Err("name must not contain control characters".into());
    }
    let n = name.trim().chars().count();
    if n == 0 || n > 64 {
        return Err("name must be 1-64 characters".into());
    }
    Ok(())
}

/// Host grammar: hostname, IPv4 or bare IPv6 literal; no scheme, port, path, spaces; <= 253 chars.
pub(crate) fn check_host(host: &str) -> Result<(), String> {
    if host.is_empty() || host.chars().count() > 253 {
        return Err("host must be 1-253 characters".into());
    }
    if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("host must not contain spaces".into());
    }
    if host.contains("://") {
        return Err("host must not contain a scheme".into());
    }
    if host.contains(':') {
        return if host.parse::<Ipv6Addr>().is_ok() { Ok(()) } else { Err("host must be a hostname, IPv4 or IPv6 address without port".into()) };
    }
    if host.contains(['/', '?', '#', '@', '[', ']', '\\']) {
        return Err("host must not contain / ? # @ or brackets".into());
    }
    for label in host.split('.') {
        let ok = !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !ok {
            return Err("host is not a valid hostname".into());
        }
    }
    Ok(())
}

/// Trim, lowercase and strip the brackets of an IPv6 literal: the form entries are stored in.
pub fn normalise_host(raw: &str) -> String {
    let t = raw.trim().to_lowercase();
    t.strip_prefix('[').and_then(|r| r.strip_suffix(']')).map_or_else(|| t.clone(), str::to_owned)
}

pub(crate) fn check_port(port: u16) -> Result<(), String> {
    if port == 0 { Err("port must be 1-65535".into()) } else { Ok(()) }
}

pub(crate) fn check_web_ui_url(url: &str) -> Result<(), String> {
    if url.chars().count() > 2048 {
        return Err("URL must be at most 2048 characters".into());
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("URL must not contain spaces".into());
    }
    let lower = url.to_ascii_lowercase();
    let rest = lower.strip_prefix("http://").or_else(|| lower.strip_prefix("https://")).ok_or_else(|| "URL must start with http:// or https://".to_string())?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return Err("URL must not contain credentials".into());
    }
    if authority.is_empty() {
        return Err("URL has no host".into());
    }
    Ok(())
}

pub(crate) fn check_server_id(id: &str) -> Result<(), String> {
    if id.chars().count() > 128 { Err("lastSeenServerId must be at most 128 characters".into()) } else { Ok(()) }
}

/// Validates `entry` against the rules and against `config` (name uniqueness, ignoring the entry
/// itself by `entry_id`). Errors come in the order name, host, port, URL, server id.
pub fn validate_entry(entry: &DaemonEntry, config: &Config) -> Result<(), Vec<FieldError>> {
    let mut errors = Vec::new();
    if let Err(m) = check_name(&entry.name) {
        errors.push(err("name", m));
    } else {
        let wanted = entry.name.trim().to_lowercase();
        let taken = config.daemons.iter().any(|d| d.entry_id != entry.entry_id && d.name.trim().to_lowercase() == wanted);
        if taken {
            errors.push(err("name", "name already used"));
        }
    }
    if let Err(m) = check_host(&entry.host) {
        errors.push(err("host", m));
    }
    if let Err(m) = check_port(entry.port) {
        errors.push(err("port", m));
    }
    if let Some(url) = &entry.web_ui_url
        && let Err(m) = check_web_ui_url(url)
    {
        errors.push(err("webUiUrl", m));
    }
    if let Some(id) = &entry.last_seen_server_id
        && let Err(m) = check_server_id(id)
    {
        errors.push(err("lastSeenServerId", m));
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::model::Config;

    fn entry() -> DaemonEntry {
        DaemonEntry {
            entry_id: "a".into(),
            name: "Local".into(),
            host: "127.0.0.1".into(),
            port: 6767,
            tls: false,
            web_ui_url: Some("http://localhost:4173".into()),
            last_seen_server_id: None,
            auto_detected: false,
            extra: Default::default(),
        }
    }

    fn fields(e: &DaemonEntry, c: &Config) -> Vec<&'static str> {
        validate_entry(e, c).unwrap_err().into_iter().map(|f| f.field).collect()
    }

    #[test]
    fn valid_passes() {
        assert!(validate_entry(&entry(), &Config::default()).is_ok());
        for h in ["::1", "2001:db8::1", "my-host.example.com", "localhost", "10.0.0.5", "host_1"] {
            let mut e = entry();
            e.host = h.into();
            assert!(validate_entry(&e, &Config::default()).is_ok(), "{h}");
        }
    }

    #[test]
    fn bad_names() {
        let c = Config::default();
        for n in ["", "   ", &"x".repeat(65), "a\nb"] {
            let mut e = entry();
            e.name = n.into();
            assert_eq!(fields(&e, &c), ["name"], "{n:?}");
        }
        let mut e = entry();
        e.name = "x".repeat(64);
        assert!(validate_entry(&e, &c).is_ok());
    }

    #[test]
    fn duplicate_name_case_insensitive() {
        let mut cfg = Config::default();
        cfg.daemons.push(entry());
        let mut other = entry();
        other.entry_id = "b".into();
        other.name = "LOCAL".into();
        assert_eq!(fields(&other, &cfg), ["name"]);
        // editing the same entry is fine
        assert!(validate_entry(&entry(), &cfg).is_ok());
    }

    #[test]
    fn bad_hosts() {
        let c = Config::default();
        for h in ["", "http://x", "my host", "x/y", "x:6767", "user@x", "[::1]", "x?a", "x#a", "-a.com", "a..b", "ünï", &"a".repeat(254)] {
            let mut e = entry();
            e.host = h.into();
            assert_eq!(fields(&e, &c), ["host"], "{h:?}");
        }
    }

    #[test]
    fn bad_port_and_urls() {
        let c = Config::default();
        let mut e = entry();
        e.port = 0;
        assert_eq!(fields(&e, &c), ["port"]);
        let long = format!("http://a.com/{}", "x".repeat(2048));
        for u in ["ftp://x.com", "http://user:pw@x.com", "http://x.com@evil.com", "https://", "x.com", long.as_str()] {
            let mut e = entry();
            e.web_ui_url = Some(u.into());
            assert_eq!(fields(&e, &c), ["webUiUrl"], "{u}");
        }
        let mut e = entry();
        e.web_ui_url = Some("HTTPS://x.com/a@b".into());
        assert!(validate_entry(&e, &c).is_ok());
        e.last_seen_server_id = Some("s".repeat(129));
        assert_eq!(fields(&e, &c), ["lastSeenServerId"]);
    }

    #[test]
    fn all_errors_together() {
        let mut e = entry();
        e.name = "".into();
        e.host = "".into();
        e.port = 0;
        assert_eq!(fields(&e, &Config::default()), ["name", "host", "port"]);
    }
}
