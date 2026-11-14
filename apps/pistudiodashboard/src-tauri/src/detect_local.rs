//! Find the local pi-studio daemon (D10): read-only look at pi-studio's own config and
//! environment. Any failure falls back to the default address, silently (D29).

use std::io::Read;
use std::path::{Path, PathBuf};

use pistudiodashboard_shell::LocalDaemon;

const DEFAULT_LISTEN: &str = "127.0.0.1:6767";
const WEB_UI_URL: &str = "http://localhost:4173";
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// `host:port` or `[v6]:port`; `None` when it does not parse. Wildcard hosts become loopback.
fn parse_listen(listen: &str) -> Option<(String, u16)> {
    let (host, port) = listen.trim().rsplit_once(':')?;
    let port: u16 = port.parse().ok().filter(|p| *p != 0)?;
    let host = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    if host.contains(':') && host.parse::<std::net::Ipv6Addr>().is_err() {
        return None;
    }
    let wildcard = matches!(host, "" | "0.0.0.0" | "::");
    Some((if wildcard { "127.0.0.1".to_owned() } else { host.to_owned() }, port))
}

fn read_listen(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    json.get("daemon")?.get("listen")?.as_str().map(str::to_owned)
}

/// `env` looks up environment variables; `user_home` is the user's home directory.
pub fn detect(env: &dyn Fn(&str) -> Option<String>, user_home: Option<PathBuf>) -> LocalDaemon {
    let get = |k: &str| env(k).filter(|v| !v.is_empty());
    let home = get("PI_STUDIO_HOME").map(PathBuf::from).or_else(|| user_home.map(|h| h.join(".pi-studio")));
    // The environment wins over the file, as in pi-studio.
    let listen = get("PI_STUDIO_LISTEN").or_else(|| home.and_then(|h| read_listen(&h.join("config.json"))));
    let (host, port) = listen.as_deref().and_then(parse_listen).or_else(|| parse_listen(DEFAULT_LISTEN)).unwrap_or_else(|| ("127.0.0.1".to_owned(), 6767));
    LocalDaemon { name: "Local".into(), host, port, tls: false, web_ui_url: Some(WEB_UI_URL.into()) }
}

/// The real environment.
pub fn detect_from_environment() -> LocalDaemon {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    detect(&|k| std::env::var(k).ok(), home)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        move |k| map.get(k).cloned()
    }

    fn home_with(file: &[u8]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config.json"), file).unwrap();
        dir
    }

    fn found(e: &impl Fn(&str) -> Option<String>) -> (String, u16) {
        let d = detect(e, None);
        (d.host, d.port)
    }

    #[test]
    fn the_file_env_wildcards_and_failures() {
        let dir = home_with(br#"{"daemon": {"listen": "0.0.0.0:7000"}}"#);
        let home = dir.path().to_str().unwrap();
        assert_eq!(found(&env(&[("PI_STUDIO_HOME", home)])), ("127.0.0.1".into(), 7000));
        assert_eq!(found(&env(&[("PI_STUDIO_HOME", home), ("PI_STUDIO_LISTEN", "127.0.0.1:7100")])), ("127.0.0.1".into(), 7100));
        assert_eq!(found(&env(&[("PI_STUDIO_LISTEN", "[::1]:7200")])), ("::1".into(), 7200));
        assert_eq!(found(&env(&[("PI_STUDIO_LISTEN", "[::]:7300")])), ("127.0.0.1".into(), 7300));
        assert_eq!(found(&env(&[("PI_STUDIO_LISTEN", ":7400")])), ("127.0.0.1".into(), 7400));
        assert_eq!(found(&env(&[("PI_STUDIO_LISTEN", "devbox:7500")])), ("devbox".into(), 7500));
        for listen in ["nonsense", "host:0", "host:70000", "a:b:1", "/tmp/sock"] {
            assert_eq!(found(&env(&[("PI_STUDIO_LISTEN", listen)])), ("127.0.0.1".into(), 6767), "{listen}");
        }
        for bad in [&b"not json"[..], br#"{"daemon": {"listen": 5}}"#, br#"{"daemon": "x"}"#, b"[]"] {
            let d = home_with(bad);
            assert_eq!(found(&env(&[("PI_STUDIO_HOME", d.path().to_str().unwrap())])), ("127.0.0.1".into(), 6767));
        }
        assert_eq!(found(&env(&[("PI_STUDIO_HOME", "/definitely/not/here")])), ("127.0.0.1".into(), 6767));
    }

    #[test]
    fn a_file_over_a_mebibyte_is_ignored_and_the_home_default_is_used() {
        let mut big = br#"{"daemon": {"listen": "127.0.0.1:7777"}, "pad": ""#.to_vec();
        big.extend(std::iter::repeat_n(b'x', 1024 * 1024));
        big.extend_from_slice(br#""}"#);
        let dir = home_with(&big);
        assert_eq!(found(&env(&[("PI_STUDIO_HOME", dir.path().to_str().unwrap())])), ("127.0.0.1".into(), 6767));
        let user = tempfile::tempdir().unwrap();
        std::fs::create_dir(user.path().join(".pi-studio")).unwrap();
        std::fs::write(user.path().join(".pi-studio/config.json"), br#"{"daemon": {"listen": "127.0.0.1:6790"}}"#).unwrap();
        let d = detect(&env(&[]), Some(user.path().to_owned()));
        assert_eq!((d.name.as_str(), d.port, d.tls, d.web_ui_url.as_deref()), ("Local", 6790, false, Some("http://localhost:4173")));
    }
}
