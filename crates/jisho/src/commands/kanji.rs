// SPDX-License-Identifier: AGPL-3.0-or-later
//! `jisho kanji`: look up kanji.

use std::process::ExitCode;

use jisho_api::{DEFAULT_LIMIT, KanjiEntryJson, run_kanji_lookup};
use lexopt::prelude::*;

use super::Context;
use super::lookup::format_number;
use crate::output::{CliError, out, out_json, printable};

pub const USAGE: &str = "\
Usage: jisho kanji [OPTIONS] <TEXT>

Looks up each distinct character of TEXT in the kanji dictionaries
(readings, meanings, stats, frequencies). Exits with status 1 when
nothing is found and 2 on errors.

Options:
  --json       Print the same JSON as POST /v1/kanji/lookup
  --limit N    At most N entries (default 32, max 1000)
";

const MAX_LIMIT: usize = 1000;

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
    let response = run_kanji_lookup(&store, &text, limit, usize::MAX)?;
    let found = !response.entries.is_empty();
    if json {
        out_json(&response)?;
    } else {
        let mut rendered = String::new();
        for entry in &response.entries {
            render(entry, &mut rendered);
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

fn render(entry: &KanjiEntryJson, out: &mut String) {
    out.push_str(&format!(
        "{}  ({})\n",
        entry.character, entry.dictionary.title
    ));
    let tags: Vec<&str> = entry.tags.iter().map(|t| t.name.as_str()).collect();
    if !tags.is_empty() {
        out.push_str(&format!("  tags: {}\n", tags.join(" ")));
    }
    if !entry.onyomi.is_empty() {
        out.push_str(&format!("  on: {}\n", entry.onyomi.join(" ")));
    }
    if !entry.kunyomi.is_empty() {
        out.push_str(&format!("  kun: {}\n", entry.kunyomi.join(" ")));
    }
    for (i, meaning) in entry.meanings.iter().enumerate() {
        out.push_str(&format!("  {}. {meaning}\n", i + 1));
    }
    for group in &entry.stats {
        let stats: Vec<String> = group
            .stats
            .iter()
            .map(|s| {
                let label = if s.notes.is_empty() {
                    &s.name
                } else {
                    &s.notes
                };
                format!("{label}: {}", s.value)
            })
            .collect();
        out.push_str(&format!("  {}: {}\n", group.category, stats.join("; ")));
    }
    for frequency in &entry.frequencies {
        let value = frequency
            .display_value
            .clone()
            .unwrap_or_else(|| format_number(frequency.frequency));
        out.push_str(&format!(
            "  frequency: {value} ({})\n",
            frequency.dictionary.title
        ));
    }
    out.push('\n');
}
