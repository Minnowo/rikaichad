// SPDX-License-Identifier: AGPL-3.0-or-later
//! `jisho import`: import Yomitan dictionary zips into the store.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use jisho_format::Limits;
use jisho_store::Store;
use lexopt::prelude::*;

use super::Context;
use crate::output::{CliError, info, progress_done, progress_line, warn};

pub const USAGE: &str = "\
Usage: jisho import <ZIP>...

Imports Yomitan dictionary zips (format 3). Each zip is imported in one
transaction: on error nothing from it is kept, and the remaining zips are
still imported. Exits with status 1 if any zip failed.
";

/// Warnings printed per dictionary before summarizing the rest.
const MAX_WARNINGS_SHOWN: usize = 5;

pub fn run(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let mut zips: Vec<PathBuf> = Vec::new();
    while let Some(arg) = parser.next()? {
        match arg {
            Value(path) => zips.push(PathBuf::from(path)),
            Short('h') | Long("help") => {
                crate::output::out(USAGE)?;
                return Ok(ExitCode::SUCCESS);
            }
            _ => return Err(arg.unexpected().into()),
        }
    }
    if zips.is_empty() {
        return Err(CliError::new(format!("no zip given\n\n{USAGE}")));
    }
    let mut store = super::open_store(ctx)?;
    let mut failed = 0usize;
    for zip in &zips {
        if let Err(err) = import_one(&mut store, zip) {
            progress_done();
            warn(&format!("{}: {err}", zip.display()));
            failed += 1;
        }
    }
    if let Some(peak) = crate::peak_memory::peak_rss_bytes() {
        info(&format!("peak memory {:.1} MB", peak as f64 / 1_000_000.0));
    }
    if failed > 0 {
        return Err(CliError::new(format!(
            "{failed} of {} dictionaries failed to import",
            zips.len()
        )));
    }
    Ok(ExitCode::SUCCESS)
}

fn import_one(store: &mut Store, zip: &Path) -> Result<(), CliError> {
    let name = zip
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let started = Instant::now();
    let report = store.import(zip, &Limits::default(), &mut |p| {
        progress_line(&format!(
            "{name}: {}/{} files, {} rows",
            p.files_done, p.files_total, p.rows
        ));
    })?;
    progress_done();
    let c = report.counts;
    info(&format!(
        "imported {:?} (id {}) in {:.1}s: {} terms, {} term meta, {} kanji, {} kanji meta, {} tags",
        report.title,
        report.id,
        started.elapsed().as_secs_f64(),
        c.terms,
        c.term_meta,
        c.kanji,
        c.kanji_meta,
        c.tags
    ));
    for warning in report.warnings.iter().take(MAX_WARNINGS_SHOWN) {
        warn(&format!("{name}: {}: {}", warning.entry, warning.message));
    }
    if report.warnings.len() > MAX_WARNINGS_SHOWN {
        warn(&format!(
            "{name}: {} more warnings",
            report.warnings.len() - MAX_WARNINGS_SHOWN
        ));
    }
    Ok(())
}
