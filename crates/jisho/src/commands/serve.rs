// SPDX-License-Identifier: AGPL-3.0-or-later
//! `jisho serve`: run the HTTP API.

use std::net::SocketAddr;
use std::process::ExitCode;

use jisho_api::{DEFAULT_BIND, ServeConfig};
use lexopt::prelude::*;

use super::Context;
use crate::output::{CliError, info, out, warn};

pub const USAGE: &str = "\
Usage: jisho serve [OPTIONS]

Serves the HTTP/JSON API (docs/api.md) until Ctrl-C. There is no
authentication.

Options:
  --bind ADDR:PORT     Listen address (default 127.0.0.1:47613). Use
                       0.0.0.0:PORT to accept connections from the network.
  --read-only          Refuse every request that would change dictionaries
  --allow-origin URL   Let web pages from this origin call the API (CORS);
                       repeatable, e.g. --allow-origin http://localhost:5173
  --allow-host NAME    Also accept this Host header name; repeatable
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
    jisho_api::run(config, |addr| {
        let mode = if read_only { " (read-only)" } else { "" };
        info(&format!("listening on http://{addr}{mode}"));
    })
    .map_err(|e| CliError::new(e.to_string()))?;
    Ok(ExitCode::SUCCESS)
}
