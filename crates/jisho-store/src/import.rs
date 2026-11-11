// SPDX-License-Identifier: AGPL-3.0-or-later
//! Streaming import of a Yomitan dictionary zip into the store.
//!
//! Follows Yomitan's importer (`ext/js/dictionary/dictionary-importer.js`,
//! see docs/plan/milestone-1.md): bank files in zip order, terms first,
//! then term meta, kanji, kanji meta and tags; empty readings replaced by
//! the term; `{type: "text"}` glossaries flattened; a duplicate title
//! rejected. Everything happens in one write transaction, so a failure at
//! any point leaves the store unchanged.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use jisho_format::{
    Archive, EntryInfo, EntryKind, FormatError, Index, KanjiMetaRow, KanjiRow, Limits, TagRow,
    TermMetaRow, TermRow, Warning, glossary_item_text, normalize_glossary, stream_rows,
};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use crate::error::StoreError;
use crate::glossary_codec::{self, GlossaryWriter};
use crate::records::Counts;
use crate::sha256::sha256_hex;
use crate::store::Store;

/// Bank kinds in the order Yomitan imports them.
const BANK_ORDER: [EntryKind; 5] = [
    EntryKind::TermBank,
    EntryKind::TermMetaBank,
    EntryKind::KanjiBank,
    EntryKind::KanjiMetaBank,
    EntryKind::TagBank,
];

/// Result of a successful import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportReport {
    /// Id of the new dictionary.
    pub id: i64,
    pub title: String,
    pub counts: Counts,
    /// Non-fatal problems (e.g. CRC mismatches), in the order found.
    pub warnings: Vec<Warning>,
}

/// Progress after each bank file, for user feedback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportProgress {
    /// Bank files finished so far.
    pub files_done: usize,
    /// Bank files in the archive.
    pub files_total: usize,
    /// Rows imported so far, all kinds.
    pub rows: u64,
}

impl Store {
    /// Imports the dictionary zip at `path`.
    ///
    /// Blocks until done; large dictionaries take minutes. `progress` is
    /// called after each bank file. Other connections can keep reading
    /// during the import and see the dictionary only once it is complete.
    ///
    /// Errors: invalid archive or rows ([`StoreError::Format`],
    /// [`StoreError::InvalidRow`]), a dictionary with the same title already
    /// imported ([`StoreError::Conflict`]), another write in progress
    /// ([`StoreError::Busy`]), and I/O or database failures. On error
    /// nothing is written.
    pub fn import(
        &mut self,
        path: &Path,
        limits: &Limits,
        progress: &mut dyn FnMut(&ImportProgress),
    ) -> Result<ImportReport, StoreError> {
        self.import_cancellable(path, limits, progress, &AtomicBool::new(false))
    }

    /// Like [`Store::import`], stopping with [`StoreError::Cancelled`] (and
    /// writing nothing) soon after `cancel` is set: it is checked every
    /// [`CANCEL_CHECK_ROWS`] rows.
    pub fn import_cancellable(
        &mut self,
        path: &Path,
        limits: &Limits,
        progress: &mut dyn FnMut(&ImportProgress),
        cancel: &AtomicBool,
    ) -> Result<ImportReport, StoreError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(StoreError::Cancelled);
        }
        let source_sha256 = hash_file(path, cancel)?;
        let source_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        let mut archive = Archive::open(path, limits.clone())?;
        let mut warnings = Vec::new();
        let index_entry = archive.index_entry()?;
        let (index_text, warning) = archive.read_to_string(&index_entry, limits.max_index_bytes)?;
        warnings.extend(warning);
        let index = Index::parse(&index_text)?;
        for (reader, minimum) in [
            ("Yomitan", &index.minimum_yomitan_version),
            ("Rikaitan", &index.minimum_rikaitan_version),
        ] {
            if let Some(minimum) = minimum {
                warnings.push(Warning {
                    entry: index_entry.name.clone(),
                    message: format!(
                        "dictionary asks for {reader} {minimum} or newer; imported anyway"
                    ),
                });
            }
        }
        check_total_size(archive.entries(), limits)?;

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists: Option<i64> = tx
            .query_row(
                "SELECT id FROM dictionaries WHERE title = ?1",
                [&index.title],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(StoreError::Conflict { title: index.title });
        }
        let priority: i64 = tx.query_row(
            "SELECT COALESCE(MAX(priority) + 1, 0) FROM dictionaries",
            [],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO dictionaries (title, priority, enabled, index_json, styles, source_name,
                 source_sha256, imported_at, term_count, term_meta_count, kanji_count,
                 kanji_meta_count, tag_count)
             VALUES (?1, ?2, 1, ?3, NULL, ?4, ?5, ?6, 0, 0, 0, 0, 0)",
            params![
                index.title,
                priority,
                index_text,
                source_name,
                source_sha256,
                unix_now()
            ],
        )?;
        let id = tx.last_insert_rowid();
        if index.sequenced {
            // Yomitan's settings page makes the first sequenced dictionary
            // the main dictionary (merge mode) when none is set.
            tx.execute(
                "INSERT OR IGNORE INTO meta (key, value) VALUES ('main_dictionary', ?1)",
                [&index.title],
            )?;
        }

        let banks: Vec<EntryInfo> = BANK_ORDER
            .iter()
            .flat_map(|kind| archive.entries().iter().filter(move |e| e.kind == *kind))
            .cloned()
            .collect();
        let mut state = ImportState {
            id,
            limits,
            counts: Counts::default(),
            progress: ImportProgress {
                files_done: 0,
                files_total: banks.len(),
                rows: 0,
            },
            glossaries: GlossaryWriter::new(),
            cancel,
        };
        for entry in &banks {
            if cancel.load(Ordering::Relaxed) {
                return Err(StoreError::Cancelled);
            }
            let warning = import_bank(&tx, &mut archive, entry, &mut state)?;
            warnings.extend(warning);
            state.progress.files_done += 1;
            progress(&state.progress);
        }
        let rest = state.glossaries.finish()?;
        insert_glossaries(&mut tx.prepare(INSERT_GLOSSARY)?, id, rest)?;
        tx.execute(
            "INSERT INTO glossary_dictionaries (dictionary, trained) VALUES (?1, ?2)",
            params![id, state.glossaries.dictionary()],
        )?;
        // Yomitan adds index.json tagMeta while processing tag files.
        if banks.iter().any(|e| e.kind == EntryKind::TagBank) {
            insert_index_tags(&tx, &index, &mut state)?;
        }

        let styles = read_styles(&mut archive, limits, &mut warnings)?;
        let counts = state.counts;
        tx.execute(
            "UPDATE dictionaries SET styles = ?2, term_count = ?3, term_meta_count = ?4,
                 kanji_count = ?5, kanji_meta_count = ?6, tag_count = ?7
             WHERE id = ?1",
            params![
                id,
                styles,
                to_i64(counts.terms),
                to_i64(counts.term_meta),
                to_i64(counts.kanji),
                to_i64(counts.kanji_meta),
                to_i64(counts.tags)
            ],
        )?;
        // Last chance to cancel: after this the import is committed.
        if cancel.load(Ordering::Relaxed) {
            return Err(StoreError::Cancelled);
        }
        tx.commit()?;
        // The import is committed; a checkpoint problem only means the WAL
        // file still holds part of it, which SQLite resolves later.
        match self.checkpoint() {
            Ok(true) => {}
            Ok(false) => warnings.push(Warning {
                entry: String::new(),
                message: "another process is reading the store; part of the import stays in \
                          the -wal file until it finishes"
                    .to_string(),
            }),
            Err(e) => warnings.push(Warning {
                entry: String::new(),
                message: format!("checkpoint after import failed: {e}"),
            }),
        }
        Ok(ImportReport {
            id,
            title: index.title,
            counts,
            warnings,
        })
    }
}

/// Mutable state threaded through the bank imports.
struct ImportState<'a> {
    id: i64,
    limits: &'a Limits,
    counts: Counts,
    progress: ImportProgress,
    glossaries: GlossaryWriter,
    cancel: &'a AtomicBool,
}

/// Rows between checks of an import's cancel flag.
pub const CANCEL_CHECK_ROWS: u64 = 10_000;

impl ImportState<'_> {
    /// Fails with [`StoreError::Cancelled`] every [`CANCEL_CHECK_ROWS`] rows
    /// once the cancel flag is set.
    fn check_cancel(&self, row_index: u64) -> Result<(), StoreError> {
        if row_index.is_multiple_of(CANCEL_CHECK_ROWS) && self.cancel.load(Ordering::Relaxed) {
            return Err(StoreError::Cancelled);
        }
        Ok(())
    }
}

fn import_bank(
    tx: &Transaction<'_>,
    archive: &mut Archive<BufReader<File>>,
    entry: &EntryInfo,
    state: &mut ImportState<'_>,
) -> Result<Option<Warning>, StoreError> {
    let max = state.limits.max_bank_bytes;
    let (rows, warning) = match entry.kind {
        EntryKind::TermBank => {
            archive.read_entry(entry, max, |r| import_terms(tx, r, entry, state))?
        }
        EntryKind::TermMetaBank => {
            archive.read_entry(entry, max, |r| import_term_meta(tx, r, entry, state))?
        }
        EntryKind::KanjiBank => {
            archive.read_entry(entry, max, |r| import_kanji(tx, r, entry, state))?
        }
        EntryKind::KanjiMetaBank => {
            archive.read_entry(entry, max, |r| import_kanji_meta(tx, r, entry, state))?
        }
        EntryKind::TagBank => {
            archive.read_entry(entry, max, |r| import_tags(tx, r, entry, state))?
        }
        EntryKind::Index | EntryKind::Styles | EntryKind::Other => (0, None),
    };
    state.progress.rows += rows;
    Ok(warning)
}

fn import_terms(
    tx: &Transaction<'_>,
    reader: &mut dyn std::io::Read,
    entry: &EntryInfo,
    state: &mut ImportState<'_>,
) -> Result<u64, StoreError> {
    let mut insert = tx.prepare(
        "INSERT INTO terms (dictionary, expression, reading, definition_tags, rules, score,
             sequence, term_tags, form_of, content_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    let mut insert_glossary = tx.prepare(INSERT_GLOSSARY)?;
    let mut row_index: u64 = 0;
    let rows = stream_rows(
        reader,
        &entry.name,
        state.limits.max_row_bytes,
        |mut row: TermRow| -> Result<(), StoreError> {
            let invalid = |message: String| invalid_row(entry, row_index, message);
            row.check(state.limits).map_err(invalid)?;
            normalize_glossary(&mut row.glossary).map_err(invalid)?;
            if row.reading.is_empty() {
                row.reading.clone_from(&row.term);
            }
            let glossary = serde_json::to_string(&row.glossary)
                .map_err(|e| invalid(format!("glossary: {e}")))?;
            let texts: Vec<String> = row.glossary.iter().map(glossary_item_text).collect();
            let text =
                serde_json::to_string(&texts).map_err(|e| invalid(format!("glossary: {e}")))?;
            let references: Vec<&serde_json::Value> =
                row.glossary.iter().filter(|item| item.is_array()).collect();
            let form_of = if references.is_empty() {
                None
            } else {
                Some(
                    serde_json::to_string(&references)
                        .map_err(|e| invalid(format!("glossary: {e}")))?,
                )
            };
            let content_count =
                i64::try_from(row.glossary.len() - references.len()).unwrap_or(i64::MAX);
            let term_id = insert.insert(params![
                state.id,
                row.term,
                row.reading,
                row.definition_tags,
                row.rules,
                row.score,
                row.sequence,
                row.term_tags,
                form_of,
                content_count
            ])?;
            let ready = state
                .glossaries
                .push(term_id, glossary_codec::encode(&glossary, &text))?;
            insert_glossaries(&mut insert_glossary, state.id, ready)?;
            row_index += 1;
            state.check_cancel(row_index)?;
            Ok(())
        },
    )?;
    state.counts.terms += rows;
    Ok(rows)
}

const INSERT_GLOSSARY: &str =
    "INSERT INTO term_glossaries (id, dictionary, size, payload) VALUES (?1, ?2, ?3, ?4)";

/// Inserts compressed glossaries `(term id, uncompressed size, payload)`
/// with an [`INSERT_GLOSSARY`] statement.
fn insert_glossaries(
    insert: &mut rusqlite::Statement<'_>,
    dictionary: i64,
    rows: Vec<(i64, usize, Vec<u8>)>,
) -> Result<(), StoreError> {
    for (id, size, payload) in rows {
        insert.execute(params![id, dictionary, to_i64(size as u64), payload])?;
    }
    Ok(())
}

fn import_term_meta(
    tx: &Transaction<'_>,
    reader: &mut dyn std::io::Read,
    entry: &EntryInfo,
    state: &mut ImportState<'_>,
) -> Result<u64, StoreError> {
    let mut insert = tx.prepare(
        "INSERT INTO term_meta (dictionary, expression, mode, data) VALUES (?1, ?2, ?3, ?4)",
    )?;
    let mut row_index: u64 = 0;
    let rows = stream_rows(
        reader,
        &entry.name,
        state.limits.max_row_bytes,
        |row: TermMetaRow| -> Result<(), StoreError> {
            row.check(state.limits)
                .map_err(|m| invalid_row(entry, row_index, m))?;
            insert.execute(params![state.id, row.term, row.mode, row.data.to_string()])?;
            row_index += 1;
            state.check_cancel(row_index)?;
            Ok(())
        },
    )?;
    state.counts.term_meta += rows;
    Ok(rows)
}

fn import_kanji(
    tx: &Transaction<'_>,
    reader: &mut dyn std::io::Read,
    entry: &EntryInfo,
    state: &mut ImportState<'_>,
) -> Result<u64, StoreError> {
    let mut insert = tx.prepare(
        "INSERT INTO kanji (dictionary, character, onyomi, kunyomi, tags, meanings, stats)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let mut row_index: u64 = 0;
    let rows = stream_rows(
        reader,
        &entry.name,
        state.limits.max_row_bytes,
        |row: KanjiRow| -> Result<(), StoreError> {
            let invalid = |message: String| invalid_row(entry, row_index, message);
            row.check(state.limits).map_err(invalid)?;
            let meanings = serde_json::to_string(&row.meanings)
                .map_err(|e| invalid(format!("meanings: {e}")))?;
            // Pairs, to keep the source order (see KanjiStats).
            let stats =
                serde_json::to_string(&row.stats.0).map_err(|e| invalid(format!("stats: {e}")))?;
            insert.execute(params![
                state.id,
                row.character,
                row.onyomi,
                row.kunyomi,
                row.tags,
                meanings,
                stats
            ])?;
            row_index += 1;
            state.check_cancel(row_index)?;
            Ok(())
        },
    )?;
    state.counts.kanji += rows;
    Ok(rows)
}

fn import_kanji_meta(
    tx: &Transaction<'_>,
    reader: &mut dyn std::io::Read,
    entry: &EntryInfo,
    state: &mut ImportState<'_>,
) -> Result<u64, StoreError> {
    let mut insert = tx.prepare(
        "INSERT INTO kanji_meta (dictionary, character, mode, data) VALUES (?1, ?2, ?3, ?4)",
    )?;
    let mut row_index: u64 = 0;
    let rows = stream_rows(
        reader,
        &entry.name,
        state.limits.max_row_bytes,
        |row: KanjiMetaRow| -> Result<(), StoreError> {
            row.check(state.limits)
                .map_err(|m| invalid_row(entry, row_index, m))?;
            insert.execute(params![
                state.id,
                row.character,
                row.mode,
                row.data.to_string()
            ])?;
            row_index += 1;
            state.check_cancel(row_index)?;
            Ok(())
        },
    )?;
    state.counts.kanji_meta += rows;
    Ok(rows)
}

fn import_tags(
    tx: &Transaction<'_>,
    reader: &mut dyn std::io::Read,
    entry: &EntryInfo,
    state: &mut ImportState<'_>,
) -> Result<u64, StoreError> {
    let mut row_index: u64 = 0;
    let rows = stream_rows(
        reader,
        &entry.name,
        state.limits.max_row_bytes,
        |row: TagRow| -> Result<(), StoreError> {
            row.check(state.limits)
                .map_err(|m| invalid_row(entry, row_index, m))?;
            insert_tag(tx, state.id, &row)?;
            row_index += 1;
            state.check_cancel(row_index)?;
            Ok(())
        },
    )?;
    state.counts.tags += rows;
    Ok(rows)
}

fn insert_index_tags(
    tx: &Transaction<'_>,
    index: &Index,
    state: &mut ImportState<'_>,
) -> Result<(), StoreError> {
    for tag in &index.tag_meta {
        let row = TagRow {
            name: tag.name.clone(),
            category: tag.category.clone(),
            order: tag.order,
            notes: tag.notes.clone(),
            score: tag.score,
        };
        insert_tag(tx, state.id, &row)?;
        state.counts.tags += 1;
    }
    Ok(())
}

fn insert_tag(tx: &Transaction<'_>, id: i64, row: &TagRow) -> Result<(), StoreError> {
    let mut insert = tx.prepare(
        "INSERT INTO tags (dictionary, name, category, sort_order, notes, score)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    insert.execute(params![
        id,
        row.name,
        row.category,
        row.order,
        row.notes,
        row.score
    ])?;
    Ok(())
}

/// Reads `styles.css`. An empty file counts as no styles (Yomitan rejects
/// the whole dictionary instead; see docs/plan/milestone-1.md).
fn read_styles(
    archive: &mut Archive<BufReader<File>>,
    limits: &Limits,
    warnings: &mut Vec<Warning>,
) -> Result<Option<String>, StoreError> {
    let Some(entry) = archive
        .entries()
        .iter()
        .find(|e| e.kind == EntryKind::Styles)
        .cloned()
    else {
        return Ok(None);
    };
    let (styles, warning) = archive.read_to_string(&entry, limits.max_styles_bytes)?;
    warnings.extend(warning);
    Ok(if styles.trim().is_empty() {
        None
    } else {
        Some(styles)
    })
}

fn check_total_size(entries: &[EntryInfo], limits: &Limits) -> Result<(), StoreError> {
    let total: u64 = entries
        .iter()
        .filter(|e| BANK_ORDER.contains(&e.kind))
        .map(|e| e.size)
        .fold(0u64, u64::saturating_add);
    if total > limits.max_total_bank_bytes {
        return Err(FormatError::Limit {
            what: "total uncompressed size of bank files".to_string(),
            limit: limits.max_total_bank_bytes,
            actual: total,
        }
        .into());
    }
    Ok(())
}

/// The file's SHA-256, stopping with [`StoreError::Cancelled`] when
/// `cancel` is set (hashing a large upload takes a while).
fn hash_file(path: &Path, cancel: &AtomicBool) -> Result<String, StoreError> {
    let io_error = |source| StoreError::Io {
        context: format!("cannot read {}", path.display()),
        source,
    };
    let file = File::open(path).map_err(io_error)?;
    let mut reader = CancellableReader {
        inner: BufReader::new(file),
        cancel,
    };
    match sha256_hex(&mut reader) {
        Ok(hash) => Ok(hash),
        Err(_) if cancel.load(Ordering::Relaxed) => Err(StoreError::Cancelled),
        Err(e) => Err(io_error(e)),
    }
}

/// A reader that fails once the cancel flag is set.
struct CancellableReader<'a, R> {
    inner: R,
    cancel: &'a AtomicBool,
}

impl<R: std::io::Read> std::io::Read for CancellableReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("cancelled"));
        }
        self.inner.read(buf)
    }
}

fn invalid_row(entry: &EntryInfo, row: u64, message: String) -> StoreError {
    StoreError::InvalidRow {
        entry: entry.name.clone(),
        row,
        message,
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Row counts fit in i64 for any real store; saturate rather than wrap.
fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
