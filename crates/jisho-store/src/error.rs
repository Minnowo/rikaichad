// SPDX-License-Identifier: AGPL-3.0-or-later
//! Error type of the dictionary store.

use std::fmt;

use jisho_format::FormatError;

/// Why a store operation failed.
#[derive(Debug)]
pub enum StoreError {
    /// SQLite reported an error.
    Sqlite(rusqlite::Error),
    /// File system error outside SQLite (e.g. hashing the source zip).
    Io {
        context: String,
        source: std::io::Error,
    },
    /// The dictionary archive is invalid.
    Format(FormatError),
    /// A row of a bank file is invalid; `row` is its 0-based index.
    InvalidRow {
        entry: String,
        row: u64,
        message: String,
    },
    /// A dictionary with this title is already imported.
    Conflict { title: String },
    /// Another write (import, removal) holds the store.
    Busy,
    /// No dictionary with the given id or title.
    NotFound { what: String },
    /// The store was written by a different jisho schema version.
    UnsupportedStoreVersion { found: i64, supported: i64 },
    /// The store is opened read-only and the operation would write.
    ReadOnly,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Sqlite(e) => write!(f, "database error: {e}"),
            StoreError::Io { context, source } => write!(f, "{context}: {source}"),
            StoreError::Format(e) => write!(f, "{e}"),
            StoreError::InvalidRow {
                entry,
                row,
                message,
            } => write!(f, "{entry}: row {row}: {message}"),
            StoreError::Conflict { title } => {
                write!(f, "a dictionary titled {title:?} is already imported")
            }
            StoreError::Busy => write!(
                f,
                "the store is busy with another import or change; try again when it finishes"
            ),
            StoreError::NotFound { what } => write!(f, "no dictionary {what}"),
            StoreError::UnsupportedStoreVersion { found, supported } if found > supported => {
                write!(
                    f,
                    "store version {found} is newer than this jisho supports ({supported}); upgrade jisho"
                )
            }
            StoreError::UnsupportedStoreVersion { found, supported } => write!(
                f,
                "store version {found} is older than this jisho uses ({supported}); rebuild it with `jisho reimport --all`"
            ),
            StoreError::ReadOnly => write!(f, "the store is open read-only"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Sqlite(e) => Some(e),
            StoreError::Io { source, .. } => Some(source),
            StoreError::Format(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(err: rusqlite::Error) -> Self {
        if is_busy(&err) {
            return StoreError::Busy;
        }
        if is_read_only(&err) {
            return StoreError::ReadOnly;
        }
        StoreError::Sqlite(err)
    }
}

impl From<FormatError> for StoreError {
    fn from(err: FormatError) -> Self {
        StoreError::Format(err)
    }
}

fn is_busy(err: &rusqlite::Error) -> bool {
    matches!(
        err.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
    )
}

fn is_read_only(err: &rusqlite::Error) -> bool {
    matches!(err.sqlite_error_code(), Some(rusqlite::ErrorCode::ReadOnly))
}
