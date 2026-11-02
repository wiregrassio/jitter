//! The smallest HTTP/1.1 client the sink needs: one POST per connection to a local store,
//! `Connection: close`, status line checked, body kept only for the error message. Plain HTTP
//! only, by design: the store is on this host or this host's private network.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Response bytes read at most; enough for a status line, headers, and an error message.
const MAX_RESPONSE_BYTES: u64 = 16 * 1024;

/// A parsed `http://host:port` endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// Host name or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
}

/// Why a POST did not land.
#[derive(Debug)]
pub enum HttpError {
    /// Resolution, connect, write, read, or timeout.
    Io(std::io::Error),
    /// The response did not start with an HTTP/1.x status line.
    Malformed,
    /// A non-2xx status, with the start of the body.
    Status(u16, String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Io(e) => write!(f, "{e}"),
            HttpError::Malformed => write!(f, "malformed HTTP response"),
            HttpError::Status(code, body) => write!(f, "HTTP {code}: {body}"),
        }
    }
}

impl Endpoint {
    /// Parse `http://host:port`, with no path, query, or credentials.
    pub fn parse_url(url: &str) -> Result<Endpoint, String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| format!("url `{url}` must start with http://"))?;
        let (host, port) = rest
            .rsplit_once(':')
            .ok_or_else(|| format!("url `{url}` must name a port"))?;
        if host.is_empty() || host.contains(['/', '@', '?', '#']) {
            return Err(format!("url `{url}` must be http://host:port with no path"));
        }
        let port = port
            .parse::<u16>()
            .map_err(|_| format!("url `{url}` has an invalid port"))?;
        Ok(Endpoint {
            host: host.to_string(),
            port,
        })
    }

    /// POST `body` to `path_and_query` with the given extra headers. One attempt.
    pub fn post(
        &self,
        path_and_query: &str,
        headers: &[(&str, &str)],
        body: &[u8],
        timeout: Duration,
    ) -> Result<(), HttpError> {
        let addr = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(HttpError::Io)?
            .next()
            .ok_or_else(|| HttpError::Io(std::io::Error::other("host resolved to no address")))?;
        let mut stream = TcpStream::connect_timeout(&addr, timeout).map_err(HttpError::Io)?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(HttpError::Io)?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(HttpError::Io)?;

        let mut head = format!(
            "POST {path_and_query} HTTP/1.1\r\nHost: {}:{}\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.host,
            self.port,
            body.len()
        );
        for (name, value) in headers {
            head.push_str(name);
            head.push_str(": ");
            head.push_str(value);
            head.push_str("\r\n");
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes()).map_err(HttpError::Io)?;
        stream.write_all(body).map_err(HttpError::Io)?;

        let mut response = Vec::new();
        stream
            .take(MAX_RESPONSE_BYTES)
            .read_to_end(&mut response)
            .map_err(HttpError::Io)?;
        let code = status_code(&response).ok_or(HttpError::Malformed)?;
        if (200..300).contains(&code) {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&response);
        let body = text.split_once("\r\n\r\n").map_or("", |(_, b)| b);
        Err(HttpError::Status(code, body.chars().take(512).collect()))
    }
}

/// The status code from `HTTP/1.x NNN reason`.
fn status_code(response: &[u8]) -> Option<u16> {
    let line = response.split(|&b| b == b'\n').next()?;
    let line = std::str::from_utf8(line).ok()?;
    let mut parts = line.split_ascii_whitespace();
    if !parts.next()?.starts_with("HTTP/1.") {
        return None;
    }
    parts.next()?.parse().ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_accepts_only_host_and_port() {
        assert_eq!(
            Endpoint::parse_url("http://127.0.0.1:8086").unwrap(),
            Endpoint {
                host: "127.0.0.1".into(),
                port: 8086
            }
        );
        for bad in [
            "https://h:1",
            "http://h",
            "http://h:1/api",
            "http://u@h:1",
            "http://:1",
            "http://h:99999",
        ] {
            assert!(Endpoint::parse_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn status_code_reads_the_status_line() {
        assert_eq!(status_code(b"HTTP/1.1 204 No Content\r\n\r\n"), Some(204));
        assert_eq!(status_code(b"SSH-2.0-OpenSSH\r\n"), None);
    }
}
