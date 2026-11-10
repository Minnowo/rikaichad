// SPDX-License-Identifier: AGPL-3.0-or-later
//! `jisho remove`: delete an imported dictionary.

use std::process::ExitCode;

use lexopt::prelude::*;

use super::Context;
use crate::output::{CliError, info, out};

pub const USAGE: &str = "\
Usage: jisho remove <ID|TITLE>

Removes a dictionary and all its entries. ID is the number shown by
`jisho list`; anything that is not a number is matched as an exact title.
";

pub fn run(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let mut target: Option<String> = None;
    while let Some(arg) = parser.next()? {
        match arg {
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
    let (id, title) = match target.parse::<i64>() {
        Ok(id) => {
            let info = store
                .dictionaries()?
                .into_iter()
                .find(|d| d.id == id)
                .ok_or_else(|| CliError::new(format!("no dictionary with id {id}")))?;
            (id, info.index.title)
        }
        Err(_) => (store.dictionary_by_title(&target)?.id, target),
    };
    store.remove(id)?;
    info(&format!("removed {title:?} (id {id})"));
    Ok(ExitCode::SUCCESS)
}
