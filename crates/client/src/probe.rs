//! Unauthenticated `GET /api/health` over a bare socket (no HTTP client dependency).
//!
//! HTTP/1.0 is used on purpose: the reply is never chunked and the server closes the socket,
//! so reading to EOF yields the whole body. No `Authorization` or cookies are ever sent.

use std::time::Duration;

use pistudiodashboard_config::DaemonEntry;
use pistudiodashboard_protocol::HealthBody;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::tls;

pub(crate) const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

/// `true` iff the daemon answered 200 with a `{status:"ok"}` body within [`PROBE_TIMEOUT`].
pub(crate) async fn probe(entry: &DaemonEntry) -> bool {
    tokio::time::timeout(PROBE_TIMEOUT, probe_inner(entry)).await.is_ok_and(|r| r.is_some())
}

async fn probe_inner(entry: &DaemonEntry) -> Option<()> {
    let tcp = TcpStream::connect((entry.host.as_str(), entry.port)).await.ok()?;
    let request = format!("GET /api/health HTTP/1.0\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n", crate::conn::authority(entry));
    let response = if entry.tls {
        let config = tls::client_config().ok()?;
        let name = rustls::pki_types::ServerName::try_from(entry.host.clone()).ok()?;
        let stream = tokio_rustls::TlsConnector::from(config).connect(name, tcp).await.ok()?;
        exchange(stream, &request).await?
    } else {
        exchange(tcp, &request).await?
    };
    parse_health(&response)
}

async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S, request: &str) -> Option<Vec<u8>> {
    stream.write_all(request.as_bytes()).await.ok()?;
    let mut out = Vec::new();
    // A truncated read (peer reset after the body) still leaves a usable prefix.
    let _ = (&mut stream).take(MAX_RESPONSE_BYTES).read_to_end(&mut out).await;
    Some(out)
}

fn parse_health(response: &[u8]) -> Option<()> {
    let text = std::str::from_utf8(response).ok()?;
    let (head, body) = text.split_once("\r\n\r\n")?;
    let status_line = head.lines().next()?;
    let mut parts = status_line.split_whitespace();
    if !parts.next()?.starts_with("HTTP/") || parts.next()? != "200" {
        return None;
    }
    HealthBody::decode(body).ok()?.is_ok().then_some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ok_and_rejects_the_rest() {
        assert!(parse_health(b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"status\":\"ok\"}").is_some());
        assert!(parse_health(b"HTTP/1.1 200 OK\r\n\r\n{\"status\":\"ok\",\"x\":1}").is_some());
        assert!(parse_health(b"HTTP/1.1 404 Not Found\r\n\r\n{\"status\":\"ok\"}").is_none());
        assert!(parse_health(b"HTTP/1.1 200 OK\r\n\r\n{\"status\":\"down\"}").is_none());
        assert!(parse_health(b"HTTP/1.1 200 OK\r\n\r\nnot json").is_none());
        assert!(parse_health(b"").is_none());
        assert!(parse_health(&[0xff, 0xfe]).is_none());
    }
}
