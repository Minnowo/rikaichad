// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rchad parse`: split a sentence into words with readings.

use std::process::ExitCode;

use lexopt::prelude::*;
use rikaichad_api::{DEFAULT_SCAN_LENGTH, MAX_SCAN_LENGTH, run_parse};

use super::Context;
use crate::output::{CliError, out, out_json, printable};

pub const USAGE: &str = "\
Usage: rchad parse [OPTIONS] <TEXT>

Splits TEXT into words like Yomitan's scanning parser: from each position
the longest dictionary match is a word. Prints the words separated by
spaces, readings in brackets after the parts that need them (e.g.
食[た]べた).

Options:
  --json             Print the same JSON as POST /v1/text/parse
  --scan-length N    How far each lookup reads, in UTF-16 units (default
                     16, max 32)
";

pub fn run(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let mut json = false;
    let mut scan_length = DEFAULT_SCAN_LENGTH;
    let mut text: Option<String> = None;
    while let Some(arg) = parser.next()? {
        match arg {
            Long("json") => json = true,
            Long("scan-length") => {
                scan_length = parser.value()?.parse()?;
                if !(1..=MAX_SCAN_LENGTH).contains(&scan_length) {
                    return Err(CliError::new(format!(
                        "--scan-length must be 1 to {MAX_SCAN_LENGTH}"
                    )));
                }
            }
            Value(value) if text.is_none() => text = Some(value.string()?),
            Short('h') | Long("help") => {
                out(USAGE)?;
                return Ok(ExitCode::SUCCESS);
            }
            _ => return Err(arg.unexpected().into()),
        }
    }
    let Some(text) = text.filter(|t| !t.is_empty()) else {
        return Err(CliError::new(format!("no text given\n\n{USAGE}")));
    };
    let store = super::open_store(ctx)?;
    let response = run_parse(&store, &text, scan_length)?;
    if json {
        out_json(&response)?;
        return Ok(ExitCode::SUCCESS);
    }
    let words: Vec<String> = response
        .tokens
        .iter()
        .map(|token| {
            token
                .segments
                .iter()
                .map(|s| {
                    if s.reading.is_empty() {
                        s.text.clone()
                    } else {
                        format!("{}[{}]", s.text, s.reading)
                    }
                })
                .collect()
        })
        .collect();
    out(&printable(&format!("{}\n", words.join(" "))))?;
    Ok(ExitCode::SUCCESS)
}
