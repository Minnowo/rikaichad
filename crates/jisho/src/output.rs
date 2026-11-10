// SPDX-License-Identifier: AGPL-3.0-or-later
//! Writing to stdout and stderr without panicking on closed pipes, and
//! the CLI's error type.

use std::fmt;
use std::io::{IsTerminal, Write};

use serde::Serialize;

/// A failed command: the message shown to the user after "jisho: ".
#[derive(Debug)]
pub struct CliError {
    pub message: String,
}

impl CliError {
    pub fn new(message: impl Into<String>) -> CliError {
        CliError {
            message: message.into(),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<lexopt::Error> for CliError {
    fn from(err: lexopt::Error) -> Self {
        CliError::new(err.to_string())
    }
}

impl From<jisho_store::StoreError> for CliError {
    fn from(err: jisho_store::StoreError) -> Self {
        CliError::new(err.to_string())
    }
}

/// Writes `text` to stdout.
pub fn out(text: &str) -> Result<(), CliError> {
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|e| CliError::new(format!("cannot write output: {e}")))
}

/// Writes `value` as pretty JSON plus a newline to stdout.
pub fn out_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| CliError::new(format!("cannot serialize output: {e}")))?;
    out(&format!("{text}\n"))
}

/// Writes a diagnostic line to stderr. Failures are ignored: if stderr is
/// gone there is nowhere left to report them.
pub fn log(level: &str, message: &str) {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "jisho: {level}{message}");
}

/// Logs an informational line.
pub fn info(message: &str) {
    log("", message);
}

/// Logs a warning line.
pub fn warn(message: &str) {
    log("warning: ", message);
}

/// Rewrites the current stderr line (progress display), only when stderr is
/// a terminal.
pub fn progress_line(message: &str) {
    let mut stderr = std::io::stderr().lock();
    if stderr.is_terminal() {
        let _ = write!(stderr, "\r\x1b[2K{message}");
        let _ = stderr.flush();
    }
}

/// Ends a progress line started with [`progress_line`].
pub fn progress_done() {
    let mut stderr = std::io::stderr().lock();
    if stderr.is_terminal() {
        let _ = write!(stderr, "\r\x1b[2K");
        let _ = stderr.flush();
    }
}
