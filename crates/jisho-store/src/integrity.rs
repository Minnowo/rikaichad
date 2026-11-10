// SPDX-License-Identifier: AGPL-3.0-or-later
//! Consistency checks of the store file.

use crate::error::StoreError;
use crate::store::Store;

/// Checks, each returning a count of offending rows, with a description.
const CHECKS: [(&str, &str); 3] = [
    (
        "SELECT COUNT(*) FROM terms t WHERE NOT EXISTS
             (SELECT 1 FROM term_glossaries g WHERE g.id = t.id)",
        "terms without a glossary",
    ),
    (
        "SELECT COUNT(*) FROM term_glossaries g WHERE NOT EXISTS
             (SELECT 1 FROM terms t WHERE t.id = g.id)",
        "glossaries without a term",
    ),
    (
        "SELECT COUNT(*) FROM terms t WHERE NOT EXISTS
             (SELECT 1 FROM dictionaries d WHERE d.id = t.dictionary)",
        "terms of a missing dictionary",
    ),
];

impl Store {
    /// Runs SQLite's `quick_check` and jisho's own consistency checks.
    /// Returns one message per problem; empty means the store is sound.
    /// Reads the whole store, so it takes a while on large stores.
    pub fn check_integrity(&self) -> Result<Vec<String>, StoreError> {
        let mut problems = Vec::new();
        let mut stmt = self.conn.prepare("PRAGMA quick_check")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            let line = row?;
            if line != "ok" {
                problems.push(format!("sqlite: {line}"));
            }
        }
        for (sql, what) in CHECKS {
            let count: i64 = self.conn.query_row(sql, [], |row| row.get(0))?;
            if count > 0 {
                problems.push(format!("{count} {what}"));
            }
        }
        Ok(problems)
    }
}
