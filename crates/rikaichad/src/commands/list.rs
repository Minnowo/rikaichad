// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rchad list`: show imported dictionaries.

use std::process::ExitCode;

use lexopt::prelude::*;
use rikaichad_api::{DictionaryJson, DictionaryListResponse, LookupSettingsJson};

use super::Context;
use crate::output::{CliError, out, out_json, printable};

pub const USAGE: &str = "\
Usage: rchad list [--json]

Lists imported dictionaries in lookup order.

Options:
  --json    Print the same JSON as GET /v1/dictionaries
";

pub fn run(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let mut json = false;
    while let Some(arg) = parser.next()? {
        match arg {
            Long("json") => json = true,
            Short('h') | Long("help") => {
                out(USAGE)?;
                return Ok(ExitCode::SUCCESS);
            }
            _ => return Err(arg.unexpected().into()),
        }
    }
    let store = super::open_store(ctx)?;
    let dictionaries: Vec<DictionaryJson> = store
        .dictionaries()?
        .into_iter()
        .map(DictionaryJson::from_info)
        .collect();
    let settings = store.lookup_settings()?;
    if json {
        out_json(&DictionaryListResponse {
            dictionaries,
            lookup_settings: LookupSettingsJson::from_settings(settings),
        })?;
        return Ok(ExitCode::SUCCESS);
    }
    let mut text = String::new();
    for d in &dictionaries {
        let mut state = String::new();
        if !d.enabled {
            state.push_str(" (disabled)");
        }
        if settings.main_dictionary.as_deref() == Some(d.title.as_str()) {
            state.push_str(" (main)");
        }
        if settings.sort_frequency_dictionary.as_deref() == Some(d.title.as_str()) {
            state.push_str(" (sort frequency)");
        }
        let mut kinds = Vec::new();
        for (count, label) in [
            (d.counts.terms, "terms"),
            (d.counts.term_meta, "term meta"),
            (d.counts.kanji, "kanji"),
            (d.counts.kanji_meta, "kanji meta"),
        ] {
            if count > 0 {
                kinds.push(format!("{count} {label}"));
            }
        }
        text.push_str(&format!(
            "{:>4}  {}{}  [rev {}]  {}\n",
            d.id,
            d.title,
            state,
            d.revision,
            kinds.join(", ")
        ));
    }
    if dictionaries.is_empty() {
        text.push_str("no dictionaries imported; use `rchad import <zip>`\n");
    }
    out(&printable(&text))?;
    Ok(ExitCode::SUCCESS)
}
