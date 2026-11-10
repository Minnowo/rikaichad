// SPDX-License-Identifier: AGPL-3.0-or-later
//! `jisho lookup`: look up a word.

use std::process::ExitCode;

use jisho_api::{LookupMode, LookupRequest, TermEntryJson, run_lookup};
use lexopt::prelude::*;

use super::Context;
use crate::output::{CliError, out, out_json, printable};

pub const USAGE: &str = "\
Usage: jisho lookup [--exact] [--json] [--limit N] [--reading R] <TEXT>

Looks up the start of TEXT like Yomitan: the longest prefixes, with
inflections undone and kana/width/romaji variants tried. Exits with status
1 when nothing is found and 2 on errors.

Options:
  --exact      Only entries whose term or reading is exactly TEXT
  --json       Print the same JSON as POST /v1/terms/lookup
  --limit N    At most N entries (default 100, max 1000)
  --reading R  Sort entries with reading R first
";

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 1000;
/// The CLI has no response size cap.
const NO_BYTE_CAP: usize = usize::MAX;

pub fn run(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let mut json = false;
    let mut mode = LookupMode::Scan;
    let mut limit = DEFAULT_LIMIT;
    let mut primary_reading = String::new();
    let mut text: Option<String> = None;
    while let Some(arg) = parser.next()? {
        match arg {
            Long("json") => json = true,
            Long("exact") => mode = LookupMode::Exact,
            Long("reading") => primary_reading = parser.value()?.string()?,
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
    let response = run_lookup(
        &store,
        &LookupRequest {
            text,
            mode,
            limit,
            primary_reading,
            max_bytes: NO_BYTE_CAP,
        },
    )?;
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
        out(&printable(&rendered))?;
    }
    Ok(if found {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// Text layout: headword line, how it was reached, tag line, numbered
/// glossary lines with continuation lines indented.
fn render_entry(entry: &TermEntryJson, out: &mut String) {
    for headword in &entry.headwords {
        if headword.reading == headword.term {
            out.push_str(&headword.term);
        } else {
            out.push_str(&format!("{} [{}]", headword.term, headword.reading));
        }
    }
    let dictionaries: Vec<&str> = entry
        .definitions
        .iter()
        .map(|d| d.dictionary.title.as_str())
        .collect();
    out.push_str(&format!("  ({})\n", dictionaries.join(", ")));
    let source = entry
        .headwords
        .first()
        .and_then(|h| h.sources.first())
        .filter(|s| s.original_text != s.deinflected_text);
    if let Some(source) = source {
        out.push_str(&format!("  from {}", source.original_text));
        if let Some(chain) = entry.inflections.first() {
            let rules: Vec<&str> = chain.rules.iter().map(|r| r.name.as_str()).collect();
            out.push_str(&format!(": {}", rules.join(" < ")));
        }
        out.push('\n');
    }
    for definition in &entry.definitions {
        let mut tags: Vec<&str> = definition
            .definition_tags
            .iter()
            .map(String::as_str)
            .collect();
        if let Some(headword) = entry.headwords.first() {
            tags.extend(headword.term_tags.iter().map(String::as_str));
        }
        if !tags.is_empty() {
            out.push_str(&format!("  {}\n", tags.join(" ")));
        }
        for (i, text) in definition.glossary_text.iter().enumerate() {
            let mut lines = text.lines();
            let first = lines.next().unwrap_or_default();
            out.push_str(&format!("  {}. {first}\n", i + 1));
            for line in lines {
                out.push_str(&format!("     {line}\n"));
            }
        }
    }
    out.push('\n');
}
