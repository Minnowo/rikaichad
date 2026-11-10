// SPDX-License-Identifier: AGPL-3.0-or-later
//! Host and Origin checks plus CORS for allowlisted origins
//! (docs/api.md "Browser access"). There is no authentication; these
//! checks stop web pages in the user's browser from driving the API, and
//! DNS rebinding from reaching a loopback server.

use std::net::SocketAddr;

use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::config::ServeConfig;
use crate::error::ApiError;
use crate::state::AppState;

const LOOPBACK_NAMES: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];

/// Which `Host` header values are accepted, derived from the bind address.
#[derive(Debug, Clone)]
pub enum HostPolicy {
    /// Bound to loopback: loopback names plus `extra`.
    Loopback { extra: Vec<String> },
    /// Bound to one address: that address plus `extra`.
    Specific { address: String, extra: Vec<String> },
    /// Bound to all interfaces: anything, unless `extra` is set.
    Wildcard { extra: Vec<String> },
}

impl HostPolicy {
    pub fn from_config(config: &ServeConfig) -> HostPolicy {
        let extra: Vec<String> = config
            .allowed_hosts
            .iter()
            .map(|h| h.to_ascii_lowercase())
            .collect();
        let ip = config.bind.ip();
        if ip.is_loopback() {
            HostPolicy::Loopback { extra }
        } else if ip.is_unspecified() {
            HostPolicy::Wildcard { extra }
        } else {
            let address = match config.bind {
                SocketAddr::V4(v4) => v4.ip().to_string(),
                SocketAddr::V6(v6) => format!("[{}]", v6.ip()),
            };
            HostPolicy::Specific { address, extra }
        }
    }

    /// Whether a request with this `Host` header (if any) is accepted.
    pub fn allows(&self, host: Option<&str>) -> bool {
        let name = host.map(|h| strip_port(h).to_ascii_lowercase());
        match (self, name) {
            (HostPolicy::Wildcard { extra }, _) if extra.is_empty() => true,
            (_, None) => false,
            (HostPolicy::Loopback { extra }, Some(name)) => {
                LOOPBACK_NAMES.contains(&name.as_str()) || extra.contains(&name)
            }
            (HostPolicy::Specific { address, extra }, Some(name)) => {
                *address == name || extra.contains(&name)
            }
            (HostPolicy::Wildcard { extra }, Some(name)) => extra.contains(&name),
        }
    }
}

impl HostPolicy {
    /// Whether `host` (a Host header) names this server in a way DNS
    /// rebinding cannot fake: a loopback name, an IP literal, or a name in
    /// `allowed_hosts`. A wildcard bind without `allowed_hosts` accepts any
    /// Host, so there a matching Origin alone proves nothing.
    pub fn is_trusted_name(&self, host: &str) -> bool {
        let name = strip_port(host).to_ascii_lowercase();
        let extra = match self {
            HostPolicy::Loopback { extra }
            | HostPolicy::Specific { extra, .. }
            | HostPolicy::Wildcard { extra } => extra,
        };
        let literal = name
            .strip_prefix('[')
            .and_then(|n| n.strip_suffix(']'))
            .unwrap_or(&name);
        LOOPBACK_NAMES.contains(&name.as_str())
            || literal.parse::<std::net::IpAddr>().is_ok()
            || extra.contains(&name)
    }
}

/// Middleware: fails requests (including reading their body) that take
/// longer than the configured time limit with `503 timeout`. Dictionary
/// uploads and removals are exempt: they take as long as the dictionary
/// needs (an upload stalls out on the idle timeout instead).
pub async fn request_timeout(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let long_write = (request.method() == Method::POST && path == "/v1/dictionaries")
        || (request.method() == Method::DELETE && path.starts_with("/v1/dictionaries/"));
    if long_write {
        return next.run(request).await;
    }
    match tokio::time::timeout(state.config.request_timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => crate::state::timeout_error().into_response(),
    }
}

/// Middleware: rejects disallowed hosts and origins, answers CORS
/// preflights for allowed origins, and adds CORS headers to their
/// responses.
pub async fn guard(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok());
    if !state.host_policy.allows(host) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "forbidden_host",
            "Host header not allowed for this server",
        )
        .into_response();
    }
    let origin = match request.headers().get(header::ORIGIN) {
        None => return next.run(request).await,
        Some(value) => match value.to_str() {
            Ok(text) => text.to_string(),
            Err(_) => {
                return ApiError::new(
                    StatusCode::FORBIDDEN,
                    "forbidden_origin",
                    "Origin header is not valid text",
                )
                .into_response();
            }
        },
    };
    // The UI's own fetches: same origin as the (already checked) Host, if
    // that Host cannot be a rebound DNS name.
    if host.is_some_and(|h| state.host_policy.is_trusted_name(h) && is_same_origin(&origin, h)) {
        return next.run(request).await;
    }
    let allowed = state
        .config
        .allowed_origins
        .iter()
        .any(|o| o.eq_ignore_ascii_case(&origin));
    if !allowed {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "forbidden_origin",
            format!("origin {origin:?} is not in allowed_origins"),
        )
        .into_response();
    }
    let mut response = if request.method() == Method::OPTIONS {
        let mut preflight = StatusCode::NO_CONTENT.into_response();
        let headers = preflight.headers_mut();
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE"),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("content-type"),
        );
        headers.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("600"),
        );
        preflight
    } else {
        next.run(request).await
    };
    if let Ok(value) = HeaderValue::from_str(&origin) {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
        response
            .headers_mut()
            .insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    response
}

/// Whether `origin` is `http://` or `https://` (behind a TLS proxy)
/// followed by exactly `host`, the request's Host header.
fn is_same_origin(origin: &str, host: &str) -> bool {
    let rest = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"));
    rest.is_some_and(|r| r.eq_ignore_ascii_case(host))
}

/// `example.com:8080` -> `example.com`, `[::1]:80` -> `[::1]`.
fn strip_port(host: &str) -> &str {
    if host.starts_with('[') {
        return match host.find(']') {
            Some(end) => host.get(..=end).unwrap_or(host),
            None => host,
        };
    }
    match host.rsplit_once(':') {
        Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name,
        _ => host,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(bind: &str, extra: &[&str]) -> HostPolicy {
        let mut config = ServeConfig::new("s".into(), bind.parse().unwrap());
        config.allowed_hosts = extra.iter().map(|s| s.to_string()).collect();
        HostPolicy::from_config(&config)
    }

    #[test]
    fn loopback_accepts_only_loopback_names() {
        let p = policy("127.0.0.1:47613", &[]);
        for ok in [
            "localhost",
            "localhost:47613",
            "127.0.0.1:47613",
            "[::1]:47613",
            "LOCALHOST",
        ] {
            assert!(p.allows(Some(ok)), "{ok}");
        }
        for bad in [
            "evil.example",
            "evil.example:47613",
            "127.0.0.1.evil.example",
        ] {
            assert!(!p.allows(Some(bad)), "{bad}");
        }
        assert!(!p.allows(None));
        assert!(policy("127.0.0.1:1", &["jisho.lan"]).allows(Some("jisho.lan:1")));
    }

    #[test]
    fn specific_and_wildcard_binds() {
        let p = policy("192.168.1.5:8000", &["box.lan"]);
        assert!(p.allows(Some("192.168.1.5:8000")));
        assert!(p.allows(Some("box.lan")));
        assert!(!p.allows(Some("localhost")));
        let v6 = policy("[fd00::5]:8000", &[]);
        assert!(v6.allows(Some("[fd00::5]:8000")));
        assert!(policy("0.0.0.0:8000", &[]).allows(Some("anything")));
        assert!(policy("0.0.0.0:8000", &[]).allows(None));
        let named = policy("0.0.0.0:8000", &["box.lan"]);
        assert!(named.allows(Some("box.lan:8000")));
        assert!(!named.allows(Some("other")));
    }
}
