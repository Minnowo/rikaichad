// SPDX-License-Identifier: AGPL-3.0-or-later
//! Server configuration.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Default address: loopback only (docs/api.md).
pub const DEFAULT_BIND: &str = "127.0.0.1:47613";

/// Everything `rchad serve` needs to run.
#[derive(Debug, Clone)]
pub struct ServeConfig {
    /// Path of the store file.
    pub store_path: PathBuf,
    /// Address to listen on.
    pub bind: SocketAddr,
    /// Refuse every mutating request and open the store query-only.
    pub read_only: bool,
    /// Browser origins allowed to call the API (exact match, e.g.
    /// `http://localhost:5173`).
    pub allowed_origins: Vec<String>,
    /// Extra `Host` header names accepted (see docs/api.md "Browser access").
    pub allowed_hosts: Vec<String>,
    /// Time limit per request, including reading its body.
    pub request_timeout: Duration,
    /// Most connections open at once.
    pub max_connections: usize,
    /// A connection without reads or writes for this long is closed.
    pub idle_timeout: Duration,
    /// Cap on JSON request bodies, in bytes.
    pub max_json_body: usize,
    /// Cap on serialized result entries per response, in bytes.
    pub max_response_bytes: usize,
    /// Cap on dictionary uploads, in bytes.
    pub max_upload_bytes: u64,
    /// Where the source of this build is (`GET /v1/source`); defaults to
    /// the RIKAICHAD_SOURCE_URL given at build time.
    pub source_url: Option<String>,
}

impl ServeConfig {
    /// Defaults from docs/api.md for the given store and address.
    pub fn new(store_path: PathBuf, bind: SocketAddr) -> ServeConfig {
        ServeConfig {
            store_path,
            bind,
            read_only: false,
            allowed_origins: Vec::new(),
            allowed_hosts: Vec::new(),
            request_timeout: Duration::from_secs(10),
            max_connections: 64,
            idle_timeout: Duration::from_secs(30),
            max_json_body: 64 * 1024,
            max_response_bytes: 16 * 1024 * 1024,
            max_upload_bytes: 4 * 1024 * 1024 * 1024,
            source_url: Some(env!("RIKAICHAD_SOURCE_URL").to_string()).filter(|u| !u.is_empty()),
        }
    }

    /// A warning to show at startup when the server is reachable from the
    /// network and accepts writes (brief section 3, "Bind default").
    pub fn exposure_warning(&self) -> Option<String> {
        if self.bind.ip().is_loopback() || self.read_only {
            return None;
        }
        Some(format!(
            "listening on {} without --read-only: anyone who can reach this address can \
             change your dictionaries (there is no authentication)",
            self.bind
        ))
    }
}
