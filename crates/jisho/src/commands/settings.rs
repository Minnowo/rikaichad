// SPDX-License-Identifier: AGPL-3.0-or-later
//! `jisho settings`: store-wide lookup settings.

use std::process::ExitCode;

use jisho_store::FrequencyOrder;
use lexopt::prelude::*;

use super::Context;
use crate::output::{CliError, info, out, printable};

pub const USAGE: &str = "\
Usage: jisho settings [OPTIONS]

Shows or changes the lookup settings kept in the store (Yomitan's general
settings). Without options, prints them.

Options:
  --main-dictionary DICT|none    Dictionary whose entries group results in
                                 merge mode (set at import to the first
                                 dictionary with sequence numbers)
  --sort-frequency DICT|none     Dictionary whose frequencies order results
  --sort-frequency-order ORDER   ascending (ranks) or descending (counts,
                                 the default)

DICT is an id from `jisho list` or an exact title.
";

pub fn run(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let mut main_dictionary: Option<String> = None;
    let mut sort_frequency: Option<String> = None;
    let mut order: Option<FrequencyOrder> = None;
    while let Some(arg) = parser.next()? {
        match arg {
            Long("main-dictionary") => main_dictionary = Some(parser.value()?.string()?),
            Long("sort-frequency") => sort_frequency = Some(parser.value()?.string()?),
            Long("sort-frequency-order") => {
                let value = parser.value()?.string()?;
                order = Some(FrequencyOrder::parse(&value).ok_or_else(|| {
                    CliError::new(format!(
                        "--sort-frequency-order must be ascending or descending, got {value:?}"
                    ))
                })?);
            }
            Short('h') | Long("help") => {
                out(USAGE)?;
                return Ok(ExitCode::SUCCESS);
            }
            _ => return Err(arg.unexpected().into()),
        }
    }
    let mut store = super::open_store(ctx)?;
    let mut settings = store.lookup_settings()?;
    let changing = main_dictionary.is_some() || sort_frequency.is_some() || order.is_some();
    let title = |target: &str| -> Result<Option<String>, CliError> {
        if target == "none" {
            return Ok(None);
        }
        Ok(Some(super::resolve_dictionary(&store, target)?.1))
    };
    if let Some(target) = &main_dictionary {
        settings.main_dictionary = title(target)?;
    }
    if let Some(target) = &sort_frequency {
        settings.sort_frequency_dictionary = title(target)?;
    }
    if let Some(order) = order {
        settings.sort_frequency_order = order;
    }
    if changing {
        store.set_lookup_settings(&settings)?;
        info("settings saved");
    }
    let shown = |value: &Option<String>| value.clone().unwrap_or_else(|| "none".to_string());
    out(&printable(&format!(
        "main dictionary:       {}\nsort frequency:        {}\nsort frequency order:  {}\n",
        shown(&settings.main_dictionary),
        shown(&settings.sort_frequency_dictionary),
        settings.sort_frequency_order.as_str()
    )))?;
    Ok(ExitCode::SUCCESS)
}
