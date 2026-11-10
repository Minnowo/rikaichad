// SPDX-License-Identifier: AGPL-3.0-or-later
//! `jisho lookup`: look up a word.

use std::process::ExitCode;

use jisho_api::{
    DEFAULT_LIMIT, DEFAULT_SCAN_LENGTH, LookupMode, LookupRequest, MAX_SCAN_LENGTH,
    PronunciationItemJson, TagJson, TermEntryJson, run_lookup,
};
use jisho_lookup::ResultMode;
use lexopt::prelude::*;

use super::Context;
use crate::output::{CliError, out, out_json, printable};

pub const USAGE: &str = "\
Usage: jisho lookup [OPTIONS] <TEXT>

Looks up the start of TEXT like Yomitan: the longest prefixes of its first
16 characters, with inflections undone and kana/width/romaji variants
tried. Exits with status 1 when nothing is found and 2 on errors.

Options:
  --exact              Only entries whose term or reading is exactly TEXT
  --json               Print the same JSON as POST /v1/terms/lookup
  --limit N            At most N entries (default 32, max 1000)
  --reading R          Sort entries with reading R first
  --result-mode MODE   group (default): one entry per term, reading and
                       inflection; merge: by the main dictionary's entries;
                       term: one entry per term; split: one entry per
                       definition; simple: split without tags or metadata
                       (except the sort-frequency dictionary's)
  --scan-length N      Characters of TEXT to scan (default 16, max 32)
";

const MAX_LIMIT: usize = 1000;
/// The CLI has no response size cap.
const NO_BYTE_CAP: usize = usize::MAX;

pub fn run(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let mut json = false;
    let mut mode = LookupMode::Scan;
    let mut result_mode = ResultMode::default();
    let mut limit = DEFAULT_LIMIT;
    let mut primary_reading = String::new();
    let mut scan_length = DEFAULT_SCAN_LENGTH;
    let mut text: Option<String> = None;
    while let Some(arg) = parser.next()? {
        match arg {
            Long("json") => json = true,
            Long("exact") => mode = LookupMode::Exact,
            Long("reading") => primary_reading = parser.value()?.string()?,
            Long("result-mode") => {
                let value = parser.value()?.string()?;
                result_mode = ResultMode::parse(&value).ok_or_else(|| {
                    CliError::new(format!(
                        "--result-mode must be group, merge, term, split or simple, got {value:?}"
                    ))
                })?;
            }
            Long("scan-length") => {
                scan_length = parser.value()?.parse()?;
                if !(1..=MAX_SCAN_LENGTH).contains(&scan_length) {
                    return Err(CliError::new(format!(
                        "--scan-length must be 1 to {MAX_SCAN_LENGTH}"
                    )));
                }
            }
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
            result_mode,
            limit,
            primary_reading,
            scan_length,
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

/// Text layout: headwords line, how they were reached, frequencies and
/// pitch accents, then numbered definitions with their tags and
/// glossary lines (continuation lines indented).
fn render_entry(entry: &TermEntryJson, out: &mut String) {
    let headwords: Vec<String> = entry
        .headwords
        .iter()
        .map(|h| {
            let tags = tag_names(&h.tags);
            let word = if h.reading == h.term {
                h.term.clone()
            } else {
                format!("{} [{}]", h.term, h.reading)
            };
            if tags.is_empty() {
                word
            } else {
                format!("{word} ({tags})")
            }
        })
        .collect();
    out.push_str(&headwords.join(", "));
    out.push('\n');
    let source = entry
        .headwords
        .iter()
        .flat_map(|h| &h.sources)
        .find(|s| s.is_primary && s.original_text != s.deinflected_text);
    if let Some(source) = source {
        out.push_str(&format!("  from {}", source.original_text));
        if let Some(chain) = entry.inflections.iter().find(|c| !c.rules.is_empty()) {
            let rules: Vec<&str> = chain.rules.iter().map(|r| r.name.as_str()).collect();
            out.push_str(&format!(": {}", rules.join(" < ")));
        }
        out.push('\n');
    }
    let several = entry.headwords.len() > 1;
    let headword_label = |index: usize| match entry.headwords.get(index) {
        Some(h) if several => format!("{}: ", h.term),
        _ => String::new(),
    };
    for frequency in &entry.frequencies {
        let value = frequency
            .display_value
            .clone()
            .unwrap_or_else(|| format_number(frequency.frequency));
        out.push_str(&format!(
            "  frequency: {}{} ({})\n",
            headword_label(frequency.headword_index),
            value,
            frequency.dictionary.title
        ));
    }
    for pronunciation in &entry.pronunciations {
        let items: Vec<String> = pronunciation
            .items
            .iter()
            .map(|item| match item {
                PronunciationItemJson::PitchAccent { position, tags, .. } => {
                    let text = match position {
                        serde_json::Value::String(pattern) => pattern.clone(),
                        other => format!("[{other}]"),
                    };
                    with_tags(text, tags)
                }
                PronunciationItemJson::PhoneticTranscription { ipa, tags } => {
                    with_tags(ipa.clone(), tags)
                }
            })
            .collect();
        out.push_str(&format!(
            "  pronunciation: {}{} ({})\n",
            headword_label(pronunciation.headword_index),
            items.join(", "),
            pronunciation.dictionary.title
        ));
    }
    let mut last_dictionary: Option<i64> = None;
    for (number, definition) in entry.definitions.iter().enumerate() {
        if last_dictionary != Some(definition.dictionary.id) {
            out.push_str(&format!("  [{}]\n", definition.dictionary.title));
            last_dictionary = Some(definition.dictionary.id);
        }
        let mut prefix = String::new();
        if several && definition.headword_indices.len() < entry.headwords.len() {
            let only: Vec<&str> = definition
                .headword_indices
                .iter()
                .filter_map(|&i| entry.headwords.get(i))
                .map(|h| h.term.as_str())
                .collect();
            prefix.push_str(&format!("({} only) ", only.join(", ")));
        }
        let tags = tag_names(&definition.tags);
        if !tags.is_empty() {
            prefix.push_str(&format!("({tags}) "));
        }
        out.push_str(&format!("  {}. {prefix}", number + 1));
        let indent = " ".repeat(format!("  {}. ", number + 1).len());
        let items = &definition.glossary_text;
        if items.iter().all(|item| !item.contains('\n')) {
            // Short items on one line, as Yomitan's compact glossary.
            out.push_str(&items.join("; "));
            out.push('\n');
        } else {
            for (i, line) in items.iter().flat_map(|item| item.lines()).enumerate() {
                if i > 0 {
                    out.push_str(&indent);
                }
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out.push('\n');
}

/// Names of the tags that are not redundant, space-separated.
fn tag_names(tags: &[TagJson]) -> String {
    tags.iter()
        .filter(|t| !t.redundant)
        .map(|t| t.name.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn with_tags(text: String, tags: &[TagJson]) -> String {
    let names = tag_names(tags);
    if names.is_empty() {
        text
    } else {
        format!("{text} ({names})")
    }
}

/// A frequency as written: integers without a fraction.
pub(crate) fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}
