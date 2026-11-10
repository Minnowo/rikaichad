// SPDX-License-Identifier: AGPL-3.0-or-later
//! CLI commands, one per file. Each exposes `USAGE` and `run`.

pub mod import;
pub mod list;
pub mod lookup;
pub mod remove;
pub mod serve;

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
