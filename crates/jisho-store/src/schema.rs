// SPDX-License-Identifier: AGPL-3.0-or-later
//! Store schema (version 1) and creation.
//!
//! The store is private to jisho (brief section 5): when the version
//! changes, users rebuild it with `jisho reimport --all` from their zips.
//! Tables mirror Yomitan's IndexedDB stores (terms, termMeta, kanji,
//! kanjiMeta, tagMeta, dictionaries); media is not stored (text only).
//! Row ids are assigned in import order, which Yomitan's merge mode sorts
//! by. Dictionary ids use AUTOINCREMENT so an id is never reused after a
//! removal (an API client holding a stale id must not hit another
//! dictionary).
//!
//! `form_of` holds the glossary's deinflection references (`[term,
//! [rules]]` items) as JSON, NULL if none, and `content_count` the number of
//! other glossary items: lookups need both for every match, and keeping
//! them here means a scan never reads glossaries.
//!
//! Glossaries live in `term_glossaries` (same id as the term) rather than in
//! `terms`: a scan matches hundreds of rows, ranks them using the small
//! columns only, and loads glossaries just for the entries it returns.
//! Keeping `terms` rows small also keeps them in the page cache
//! (measured in milestone 1, docs/brief.md section 5).

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

use crate::error::StoreError;

/// Schema version written by this build.
pub const STORE_VERSION: i64 = 1;

const CREATE: &str = "
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE dictionaries (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    title            TEXT NOT NULL UNIQUE,
    priority         INTEGER NOT NULL,
    enabled          INTEGER NOT NULL,
    index_json       TEXT NOT NULL,
    styles           TEXT,
    source_name      TEXT NOT NULL,
    source_sha256    TEXT NOT NULL,
    imported_at      INTEGER NOT NULL,
    term_count       INTEGER NOT NULL,
    term_meta_count  INTEGER NOT NULL,
    kanji_count      INTEGER NOT NULL,
    kanji_meta_count INTEGER NOT NULL,
    tag_count        INTEGER NOT NULL
);
CREATE TABLE terms (
    id              INTEGER PRIMARY KEY,
    dictionary      INTEGER NOT NULL,
    expression      TEXT NOT NULL,
    reading         TEXT NOT NULL,
    definition_tags TEXT NOT NULL,
    rules           TEXT NOT NULL,
    score           REAL NOT NULL,
    sequence        INTEGER NOT NULL,
    term_tags       TEXT NOT NULL,
    form_of         TEXT,
    content_count   INTEGER NOT NULL
);
CREATE TABLE term_glossaries (
    id       INTEGER PRIMARY KEY,
    glossary TEXT NOT NULL
);
CREATE INDEX terms_expression ON terms (expression);
CREATE INDEX terms_reading ON terms (reading);
CREATE INDEX terms_sequence ON terms (dictionary, sequence);
CREATE TABLE term_meta (
    id         INTEGER PRIMARY KEY,
    dictionary INTEGER NOT NULL,
    expression TEXT NOT NULL,
    mode       TEXT NOT NULL,
    data       TEXT NOT NULL
);
CREATE INDEX term_meta_expression ON term_meta (expression);
CREATE INDEX term_meta_dictionary ON term_meta (dictionary);
CREATE TABLE kanji (
    id         INTEGER PRIMARY KEY,
    dictionary INTEGER NOT NULL,
    character  TEXT NOT NULL,
    onyomi     TEXT NOT NULL,
    kunyomi    TEXT NOT NULL,
    tags       TEXT NOT NULL,
    meanings   TEXT NOT NULL,
    stats      TEXT NOT NULL
);
CREATE INDEX kanji_character ON kanji (character);
CREATE INDEX kanji_dictionary ON kanji (dictionary);
CREATE TABLE kanji_meta (
    id         INTEGER PRIMARY KEY,
    dictionary INTEGER NOT NULL,
    character  TEXT NOT NULL,
    mode       TEXT NOT NULL,
    data       TEXT NOT NULL
);
CREATE INDEX kanji_meta_character ON kanji_meta (character);
CREATE INDEX kanji_meta_dictionary ON kanji_meta (dictionary);
CREATE TABLE tags (
    id         INTEGER PRIMARY KEY,
    dictionary INTEGER NOT NULL,
    name       TEXT NOT NULL,
    category   TEXT NOT NULL,
    sort_order REAL NOT NULL,
    notes      TEXT NOT NULL,
    score      REAL NOT NULL
);
CREATE INDEX tags_dictionary_name ON tags (dictionary, name);
";

/// Creates the schema in an empty database, or checks the version of an
/// existing one. Fails with [`StoreError::UnsupportedStoreVersion`] for a
/// store from another schema version.
///
/// Safe when several processes open a fresh store at once: the check runs
/// first without a lock (so opening never waits for a long import), and
/// creation happens in an IMMEDIATE transaction that checks again.
pub fn ensure(conn: &mut Connection) -> Result<(), StoreError> {
    if !has_schema(conn)? {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !has_schema(&tx)? {
            tx.execute_batch(CREATE)?;
            tx.execute(
                "INSERT INTO meta (key, value) VALUES ('store_version', ?1)",
                [STORE_VERSION.to_string()],
            )?;
        }
        tx.commit()?;
    }
    let found: String = conn.query_row(
        "SELECT value FROM meta WHERE key = 'store_version'",
        [],
        |row| row.get(0),
    )?;
    let found: i64 = found.parse().unwrap_or(i64::MAX);
    if found != STORE_VERSION {
        return Err(StoreError::UnsupportedStoreVersion {
            found,
            supported: STORE_VERSION,
        });
    }
    Ok(())
}

fn has_schema(conn: &Connection) -> Result<bool, StoreError> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'meta'",
            [],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false))
}
