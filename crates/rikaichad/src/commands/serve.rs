// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rchad serve`: run the HTTP API.

use std::net::SocketAddr;
use std::process::ExitCode;

use lexopt::prelude::*;
use rikaichad_api::{DEFAULT_BIND, ServeConfig};

use super::Context;
use crate::output::{CliError, info, out, warn};

pub const USAGE: &str = "\
Usage: rchad serve [OPTIONS]

Serves the HTTP/JSON API (docs/api.md) until Ctrl-C. There is no
authentication.

Options:
  --bind ADDR:PORT     Listen address (default 127.0.0.1:47613). Use
                       0.0.0.0:PORT to accept connections from the network.
  --read-only          Refuse every request that would change dictionaries
  --allow-origin URL   Let web pages from this origin call the API (CORS);
                       repeatable, e.g. --allow-origin http://localhost:5173
  --allow-host NAME    Also accept this Host header name; repeatable
  --max-upload SIZE    Largest dictionary upload, e.g. 500M or 4G (default
                       4G)
  --source-url URL     Where this server's source code is published, shown
                       by GET /v1/source (AGPL-3.0: set this if you serve
                       a modified version to others)
";

pub fn run(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let default_bind: SocketAddr = DEFAULT_BIND
        .parse()
        .map_err(|e| CliError::new(format!("bad default address: {e}")))?;
    let mut config = ServeConfig::new(ctx.store_path.clone(), default_bind);
    while let Some(arg) = parser.next()? {
        match arg {
            Long("bind") => {
                let value = parser.value()?.string()?;
                config.bind = value.parse().map_err(|_| {
                    CliError::new(format!(
                        "--bind: {value:?} is not ADDR:PORT (e.g. 127.0.0.1:47613 or [::1]:47613)"
                    ))
                })?;
            }
            Long("read-only") => config.read_only = true,
            Long("allow-origin") => config.allowed_origins.push(parser.value()?.string()?),
            Long("allow-host") => config.allowed_hosts.push(parser.value()?.string()?),
            Long("max-upload") => {
                let value = parser.value()?.string()?;
                config.max_upload_bytes = parse_size(&value).ok_or_else(|| {
                    CliError::new(format!(
                        "--max-upload: {value:?} is not a size like 500M or 4G"
                    ))
                })?;
            }
            Long("source-url") => config.source_url = Some(parser.value()?.string()?),
            Short('h') | Long("help") => {
                out(USAGE)?;
                return Ok(ExitCode::SUCCESS);
            }
            _ => return Err(arg.unexpected().into()),
        }
    }
    // Create the store (and its directory) before serving, so a fresh
    // install serves an empty store instead of failing.
    drop(super::open_store(ctx)?);
    if let Some(message) = config.exposure_warning() {
        warn(&message);
    }
    let read_only = config.read_only;
    rikaichad_api::run(config, |addr| {
        let mode = if read_only { " (read-only)" } else { "" };
        info(&format!(
            "listening on http://{addr}{mode}; web UI at http://{addr}/"
        ));
    })
    .map_err(|e| CliError::new(e.to_string()))?;
    Ok(ExitCode::SUCCESS)
}

/// `123`, `500K`, `500M`, `4G` (powers of 1024) as bytes.
fn parse_size(text: &str) -> Option<u64> {
    let text = text.trim();
    let (digits, factor) = match text.chars().last()?.to_ascii_uppercase() {
        'K' => (text.get(..text.len() - 1)?, 1u64 << 10),
        'M' => (text.get(..text.len() - 1)?, 1 << 20),
        'G' => (text.get(..text.len() - 1)?, 1 << 30),
        _ => (text, 1),
    };
    digits
        .parse::<u64>()
        .ok()?
        .checked_mul(factor)
        .filter(|n| *n > 0)
}

#[cfg(test)]
mod tests {
    use super::parse_size;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("500M"), Some(500 << 20));
        assert_eq!(parse_size("4g"), Some(4 << 30));
        assert_eq!(parse_size("123"), Some(123));
        assert_eq!(parse_size("0"), None);
        assert_eq!(parse_size("x"), None);
        assert_eq!(parse_size(""), None);
    }
}
