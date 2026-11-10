// SPDX-License-Identifier: AGPL-3.0-or-later
//! jisho command-line entry point: parses global options and dispatches
//! to a command in `commands/`.

mod commands;
mod data_dir;
mod output;
mod peak_memory;

use std::path::PathBuf;
use std::process::ExitCode;

use lexopt::prelude::*;

use crate::commands::Context;
use crate::output::{CliError, log, out};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
jisho - Japanese dictionary engine for Yomitan dictionaries

Usage: jisho [OPTIONS] <COMMAND> [ARGS]

Commands:
  import <ZIP>...   Import Yomitan dictionary zips
  list              List imported dictionaries
  remove <ID>       Remove a dictionary
  lookup <TEXT>     Look up a word
  serve             Run the HTTP API
  bench             Measure lookup latency on the current store

Exit status: 0 success, 1 nothing found (lookup), 2 error.

Options:
  --store PATH      Store file (default: $JISHO_STORE, else the platform
                    data directory, e.g. ~/.local/share/jisho/jisho.sqlite)
  -h, --help        Print help (also: jisho <COMMAND> --help)
  -V, --version     Print the version
";

/// Exit status for errors. Status 1 is reserved for "nothing found" from
/// `lookup` (like grep), so scripts can tell the two apart.
const ERROR_EXIT: u8 = 2;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            log("", &err.message);
            ExitCode::from(ERROR_EXIT)
        }
    }
}

fn run() -> Result<ExitCode, CliError> {
    let mut parser = lexopt::Parser::from_env();
    let mut store_path: Option<PathBuf> = None;
    loop {
        let Some(arg) = parser.next()? else {
            out(USAGE)?;
            return Ok(ExitCode::SUCCESS);
        };
        match arg {
            Short('V') | Long("version") => {
                out(&format!("jisho {VERSION}\n"))?;
                return Ok(ExitCode::SUCCESS);
            }
            Short('h') | Long("help") => {
                out(USAGE)?;
                return Ok(ExitCode::SUCCESS);
            }
            Long("store") => store_path = Some(PathBuf::from(parser.value()?)),
            Value(command) => {
                let command = command.string()?;
                let store_path = match store_path {
                    Some(path) => path,
                    None => data_dir::default_store_path().ok_or_else(|| {
                        CliError::new(format!(
                            "cannot find a data directory; pass --store or set {}",
                            data_dir::STORE_ENV
                        ))
                    })?,
                };
                let ctx = Context { store_path };
                return match command.as_str() {
                    "import" => commands::import::run(&mut parser, &ctx),
                    "list" => commands::list::run(&mut parser, &ctx),
                    "remove" => commands::remove::run(&mut parser, &ctx),
                    "lookup" => commands::lookup::run(&mut parser, &ctx),
                    "serve" => commands::serve::run(&mut parser, &ctx),
                    "bench" => commands::bench::run(&mut parser, &ctx),
                    other => Err(CliError::new(format!(
                        "unknown command {other:?}\nRun 'jisho --help' for usage."
                    ))),
                };
            }
            _ => {
                return Err(CliError::new(format!(
                    "{}\nRun 'jisho --help' for usage.",
                    arg.unexpected()
                )));
            }
        }
    }
}
