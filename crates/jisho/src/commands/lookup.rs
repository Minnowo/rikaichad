// SPDX-License-Identifier: AGPL-3.0-or-later
//! `jisho lookup`: look up a word.

use std::process::ExitCode;

use jisho_api::{TermEntryJson, lookup_response};
use lexopt::prelude::*;

use super::Context;
use crate::output::{CliError, out, out_json};

pub const USAGE: &str = "\
Usage: jisho lookup [--json] [--limit N] <TEXT>

Prints dictionary entries whose term or reading is exactly TEXT (inflected
forms are found from milestone 2 on). Exits with status 1 when nothing is
found.

Options:
  --json       Print the same JSON as POST /v1/terms/lookup
  --limit N    At most N entries (default 100, max 1000)
";

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 1000;
/// The CLI has no response size cap; usize::MAX disables it.
const NO_BYTE_CAP: usize = usize::MAX;

pub fn run(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let mut json = false;
    let mut limit = DEFAULT_LIMIT;
    let mut text: Option<String> = None;
    while let Some(arg) = parser.next()? {
        match arg {
            Long("json") => json = true,
            Long("limit") => {
                limit = parser.value()?.parse()?;
                if !(1..=MAX_LIMIT).contains(&limit) {
                    return Err(CliError::new(format!("--limit must be 1 to {MAX_LIMIT}")));
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
    let records = store.lookup_exact(&text, limit + 1)?;
    let response = lookup_response(&text, records, limit, NO_BYTE_CAP);
    let found = !response.entries.is_empty();
    if json {
        out_json(&response)?;
    } else {
        let mut rendered = String::new();
        for entry in &response.entries {
            render_entry(entry, &mut rendered);
        }
        if response.truncated {
            rendered.push_str(&format!("(more results; showing {limit})\n"));
        }
        out(&rendered)?;
    }
    Ok(if found {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// Text layout: headword line, tag line, numbered glossary lines with
/// continuation lines indented.
fn render_entry(entry: &TermEntryJson, out: &mut String) {
    if entry.reading == entry.term {
        out.push_str(&entry.term);
    } else {
        out.push_str(&format!("{} [{}]", entry.term, entry.reading));
    }
    out.push_str(&format!("  ({})\n", entry.dictionary.title));
    let mut tags: Vec<&str> = entry.definition_tags.iter().map(String::as_str).collect();
    tags.extend(entry.term_tags.iter().map(String::as_str));
    if !tags.is_empty() {
        out.push_str(&format!("  {}\n", tags.join(" ")));
    }
    for (i, text) in entry.glossary_text.iter().enumerate() {
        let mut lines = text.lines();
        let first = lines.next().unwrap_or_default();
        out.push_str(&format!("  {}. {first}\n", i + 1));
        for line in lines {
            out.push_str(&format!("     {line}\n"));
        }
    }
    out.push('\n');
}
