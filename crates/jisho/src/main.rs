// SPDX-License-Identifier: AGPL-3.0-or-later
//! jisho command-line entry point: parses global arguments and dispatches
//! to a command. Output goes through explicit writes so a closed pipe is an
//! error to report, not a panic.

use std::io::Write;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
jisho - Japanese dictionary engine for Yomitan dictionaries

Usage: jisho [OPTIONS]

Options:
  -h, --help       Print this help
  -V, --version    Print the version
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let mut stderr = std::io::stderr().lock();
            // Nothing sensible is left to do if stderr itself is gone.
            let _ = writeln!(stderr, "jisho: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Parses the command line and runs the requested action.
///
/// Returns a user-facing error message on failure.
fn run() -> Result<(), String> {
    let mut parser = lexopt::Parser::from_env();
    let mut stdout = std::io::stdout().lock();
    let arg = parser.next().map_err(|e| e.to_string())?;
    match arg {
        Some(lexopt::Arg::Short('V')) | Some(lexopt::Arg::Long("version")) => {
            write_out(&mut stdout, &format!("jisho {VERSION}\n"))
        }
        Some(lexopt::Arg::Short('h')) | Some(lexopt::Arg::Long("help")) | None => {
            write_out(&mut stdout, USAGE)
        }
        Some(other) => Err(format!(
            "{}\nRun 'jisho --help' for usage.",
            other.unexpected()
        )),
    }
}

/// Writes `text` to `out`, mapping I/O failures to a message.
fn write_out(out: &mut impl Write, text: &str) -> Result<(), String> {
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| format!("cannot write output: {e}"))
}
