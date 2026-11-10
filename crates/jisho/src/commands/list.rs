// SPDX-License-Identifier: AGPL-3.0-or-later
//! `jisho list`: show imported dictionaries.

use std::process::ExitCode;

use jisho_api::{DictionaryJson, DictionaryListResponse};
use lexopt::prelude::*;

use super::Context;
use crate::output::{CliError, out, out_json};

pub const USAGE: &str = "\
Usage: jisho list [--json]

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
    if json {
        out_json(&DictionaryListResponse { dictionaries })?;
        return Ok(ExitCode::SUCCESS);
    }
    let mut text = String::new();
    for d in &dictionaries {
        let state = if d.enabled { "" } else { " (disabled)" };
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
        text.push_str("no dictionaries imported; use `jisho import <zip>`\n");
    }
    out(&text)?;
    Ok(ExitCode::SUCCESS)
}
