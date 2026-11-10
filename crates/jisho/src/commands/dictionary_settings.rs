// SPDX-License-Identifier: AGPL-3.0-or-later
//! `jisho dictionary` (one dictionary's lookup settings) and `jisho order`
//! (dictionary order).

use std::process::ExitCode;

use lexopt::prelude::*;

use super::Context;
use crate::output::{CliError, info, out, printable};

pub const USAGE: &str = "\
Usage: jisho dictionary <DICT> [OPTIONS]

Shows or changes how lookups use one dictionary (Yomitan's dictionary
settings). Without options, prints them. DICT is an id from `jisho list`
or an exact title.

Options:
  --enable / --disable             Use the dictionary in lookups or not
  --alias NAME                     Name shown in results (--no-alias: title)
  --secondary-searches on|off      Merge mode also searches this dictionary
                                   for the main dictionary's headwords
                                   (default off)
  --pos-filter on|off              Deinflections must fit the entry's part
                                   of speech (default on)
  --deinflections on|off           Follow the dictionary's own deinflection
                                   references (default on)
";

pub const ORDER_USAGE: &str = "\
Usage: jisho order <DICT>...

Puts the given dictionaries first, in the given order; the others keep
their order after them. Results from earlier dictionaries sort first when
other criteria tie. DICT is an id from `jisho list` or an exact title.
";

pub fn run(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let mut target: Option<String> = None;
    let mut enabled: Option<bool> = None;
    let mut alias: Option<Option<String>> = None;
    let mut secondary: Option<bool> = None;
    let mut pos_filter: Option<bool> = None;
    let mut deinflections: Option<bool> = None;
    while let Some(arg) = parser.next()? {
        match arg {
            Long("enable") => enabled = Some(true),
            Long("disable") => enabled = Some(false),
            Long("alias") => alias = Some(Some(parser.value()?.string()?)),
            Long("no-alias") => alias = Some(None),
            Long("secondary-searches") => secondary = Some(on_off(parser, "secondary-searches")?),
            Long("pos-filter") => pos_filter = Some(on_off(parser, "pos-filter")?),
            Long("deinflections") => deinflections = Some(on_off(parser, "deinflections")?),
            Value(value) if target.is_none() => target = Some(value.string()?),
            Short('h') | Long("help") => {
                out(USAGE)?;
                return Ok(ExitCode::SUCCESS);
            }
            _ => return Err(arg.unexpected().into()),
        }
    }
    let Some(target) = target else {
        return Err(CliError::new(format!("no dictionary given\n\n{USAGE}")));
    };
    let mut store = super::open_store(ctx)?;
    let (id, title) = super::resolve_dictionary(&store, &target)?;
    let info_of = |store: &jisho_store::Store| {
        store
            .lookup_dictionaries()
            .map(|all| all.into_iter().find(|d| d.id == id))
    };
    let Some(current) = info_of(&store)? else {
        return Err(CliError::new(format!("no dictionary {target:?}")));
    };
    let mut settings = current.settings.clone();
    if let Some(alias) = alias {
        settings.alias = alias.filter(|a| !a.is_empty());
    }
    if let Some(value) = secondary {
        settings.allow_secondary_searches = value;
    }
    if let Some(value) = pos_filter {
        settings.parts_of_speech_filter = value;
    }
    if let Some(value) = deinflections {
        settings.use_deinflections = value;
    }
    let mut changed = false;
    if settings != current.settings {
        store.set_dictionary_settings(id, &settings)?;
        changed = true;
    }
    if let Some(enabled) = enabled
        && enabled != current.enabled
    {
        store.set_enabled(id, enabled)?;
        changed = true;
    }
    if changed {
        info(&format!("updated {title:?} (id {id})"));
    }
    let Some(now) = info_of(&store)? else {
        return Err(CliError::new(format!("no dictionary {target:?}")));
    };
    let flag = |on: bool| if on { "on" } else { "off" };
    out(&printable(&format!(
        "{} (id {})\n  enabled:             {}\n  alias:               {}\n  secondary searches:  {}\n  pos filter:          {}\n  deinflections:       {}\n",
        now.title,
        now.id,
        if now.enabled { "yes" } else { "no" },
        now.settings.alias.as_deref().unwrap_or("(title)"),
        flag(now.settings.allow_secondary_searches),
        flag(now.settings.parts_of_speech_filter),
        flag(now.settings.use_deinflections),
    )))?;
    Ok(ExitCode::SUCCESS)
}

fn on_off(parser: &mut lexopt::Parser, name: &str) -> Result<bool, CliError> {
    let value = parser.value()?.string()?;
    match value.as_str() {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(CliError::new(format!(
            "--{name} must be on or off, got {value:?}"
        ))),
    }
}

pub fn run_order(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let mut targets: Vec<String> = Vec::new();
    while let Some(arg) = parser.next()? {
        match arg {
            Value(value) => targets.push(value.string()?),
            Short('h') | Long("help") => {
                out(ORDER_USAGE)?;
                return Ok(ExitCode::SUCCESS);
            }
            _ => return Err(arg.unexpected().into()),
        }
    }
    if targets.is_empty() {
        return Err(CliError::new(format!(
            "no dictionary given\n\n{ORDER_USAGE}"
        )));
    }
    let mut store = super::open_store(ctx)?;
    let mut ids = Vec::with_capacity(targets.len());
    for target in &targets {
        ids.push(super::resolve_dictionary(&store, target)?.0);
    }
    store.set_order(&ids)?;
    info("order saved");
    let mut text = String::new();
    for (i, d) in store.lookup_dictionaries()?.iter().enumerate() {
        let state = if d.enabled { "" } else { " (disabled)" };
        text.push_str(&format!("{:>3}. {} (id {}){state}\n", i + 1, d.title, d.id));
    }
    out(&printable(&text))?;
    Ok(ExitCode::SUCCESS)
}
