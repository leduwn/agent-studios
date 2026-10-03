use std::time::Duration;

use crate::error::{TransportError, sanitize_error_message};

/// Configuration options and safety thresholds for runtime transport execution.
#[derive(Clone, Debug)]
pub struct RuntimeTransportOptions {
    pub connect_timeout: Duration,
    pub request_headers_timeout: Duration,
    pub stream_idle_timeout: Duration,
    pub max_sse_event_bytes: usize,
    pub max_error_body_bytes: usize,
    pub allow_insecure_remote_http: bool,
}

impl Default for RuntimeTransportOptions {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_headers_timeout: Duration::from_secs(30),
            stream_idle_timeout: Duration::from_secs(300),
            max_sse_event_bytes: 4 * 1024 * 1024, // 4 MB default per SSE event
            max_error_body_bytes: 64 * 1024,      // 64 KB default error body limit
            allow_insecure_remote_http: false,
        }
    }
}

impl RuntimeTransportOptions {
    /// Validates an outbound URL string against remote HTTP policy and returns parsed `url::Url`.
    pub fn validate_url(&self, raw_url: &str) -> Result<url::Url, TransportError> {
        let parsed = url::Url::parse(raw_url).map_err(|e| {
            TransportError::InvalidEndpoint(format!(
                "Malformed URL '{}': {e}",
                sanitize_error_message(raw_url)
            ))
        })?;

        match parsed.scheme() {
            "https" => Ok(parsed),
            "http" => {
                let is_loopback = match parsed.host() {
                    Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
                    Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                    Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                    None => false,
                };

                if is_loopback || self.allow_insecure_remote_http {
                    Ok(parsed)
                } else {
                    Err(TransportError::InsecureRemoteHttpRejected {
                        url: sanitize_error_message(raw_url),
                    })
                }
            }
            scheme => Err(TransportError::InvalidEndpoint(format!(
                "Unsupported URL scheme '{scheme}'; expected 'http' or 'https'"
            ))),
        }
    }
}

/// Reads an upstream HTTP error response body up to `max_bytes` without unbounded memory buffering.
pub async fn read_bounded_error_body(
    mut resp: reqwest::Response,
    max_bytes: usize,
) -> Result<String, TransportError> {
    use bytes::BytesMut;

    let mut buf = BytesMut::new();
    while let Some(chunk_res) = resp.chunk().await.map_err(TransportError::from)? {
        if buf.len() + chunk_res.len() > max_bytes {
            let remaining = max_bytes.saturating_sub(buf.len());
            buf.extend_from_slice(&chunk_res[..remaining]);
            let s = String::from_utf8_lossy(&buf).to_string();
            let sanitized = sanitize_error_message(&s);
            return Ok(format!("{sanitized} [TRUNCATED at {max_bytes} bytes]"));
        }
        buf.extend_from_slice(&chunk_res);
    }
    let s = String::from_utf8_lossy(&buf).to_string();
    Ok(sanitize_error_message(&s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url_https() {
        let options = RuntimeTransportOptions::default();
        let url = options
            .validate_url("https://api.openai.com/v1/chat/completions")
            .unwrap();
        assert_eq!(url.scheme(), "https");
    }

    #[test]
    fn test_validate_url_localhost_http() {
        let options = RuntimeTransportOptions::default();
        let url = options
            .validate_url("http://localhost:8080/v1/chat/completions")
            .unwrap();
        assert_eq!(url.scheme(), "http");

        let url_ip = options
            .validate_url("http://127.0.0.1:9090/v1/messages")
            .unwrap();
        assert_eq!(url_ip.scheme(), "http");
    }

    #[test]
    fn test_validate_url_remote_insecure_http_rejected() {
        let options = RuntimeTransportOptions::default();
        let err = options
            .validate_url("http://api.anthropic.com/v1/messages")
            .unwrap_err();
        match err {
            TransportError::InsecureRemoteHttpRejected { url } => {
                assert_eq!(url, "http://api.anthropic.com/v1/messages");
            }
            other => panic!("Unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_validate_url_remote_insecure_http_opt_in() {
        let options = RuntimeTransportOptions {
            allow_insecure_remote_http: true,
            ..Default::default()
        };
        let url = options
            .validate_url("http://remote-custom-proxy.internal:8000/v1")
            .unwrap();
        assert_eq!(url.scheme(), "http");
    }
}
