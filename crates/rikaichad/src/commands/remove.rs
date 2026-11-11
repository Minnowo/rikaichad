// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rchad remove`: delete an imported dictionary.

use std::process::ExitCode;

use lexopt::prelude::*;

use super::Context;
use crate::output::{CliError, info, out};

pub const USAGE: &str = "\
Usage: rchad remove <ID|TITLE>

Removes a dictionary and all its entries. ID is the number shown by
`rchad list`; anything that is not a number is matched as an exact title.
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
    let (id, title) = super::resolve_dictionary(&store, &target)?;
    store.remove(id)?;
    info(&format!("removed {title:?} (id {id})"));
    Ok(ExitCode::SUCCESS)
}
