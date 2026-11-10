// SPDX-License-Identifier: AGPL-3.0-or-later
//! CLI commands, one per file. Each exposes `USAGE` and `run`.

pub mod bench;
pub mod dictionary_settings;
pub mod import;
pub mod kanji;
pub mod list;
pub mod lookup;
pub mod parse;
pub mod remove;
pub mod serve;
pub mod settings;

use std::path::PathBuf;

use jisho_store::Store;

use crate::output::CliError;

/// Settings shared by all commands (from global options).
pub struct Context {
    pub store_path: PathBuf,
}

/// Opens (creating if needed) the store, including its parent directory.
pub fn open_store(ctx: &Context) -> Result<Store, CliError> {
    if let Some(parent) = ctx.store_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::new(format!("cannot create {}: {e}", parent.display())))?;
    }
    Store::open(&ctx.store_path).map_err(|e| {
        CliError::new(format!(
            "cannot open store {}: {e}",
            ctx.store_path.display()
        ))
    })
}

/// Finds a dictionary by id (a number, as `jisho list` shows) or exact
/// title; returns its id and title.
pub fn resolve_dictionary(store: &Store, target: &str) -> Result<(i64, String), CliError> {
    let dictionaries = store.lookup_dictionaries()?;
    let found = match target.parse::<i64>() {
        Ok(id) => dictionaries.into_iter().find(|d| d.id == id),
        Err(_) => dictionaries.into_iter().find(|d| d.title == target),
    };
    found
        .map(|d| (d.id, d.title))
        .ok_or_else(|| CliError::new(format!("no dictionary {target:?} (see `jisho list`)")))
}
