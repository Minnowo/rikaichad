// SPDX-License-Identifier: AGPL-3.0-or-later
//! Opening the store file and connection-level settings.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::error::StoreError;
use crate::schema;

/// How long a connection waits for another writer before giving up with
/// [`StoreError::Busy`].
const BUSY_TIMEOUT: Duration = Duration::from_secs(2);

/// Page cache per connection, in KiB (negative `cache_size` means KiB).
const CACHE_KIB: i64 = 64 * 1024;

/// An open dictionary store: one SQLite file in WAL mode.
///
/// A `Store` wraps one connection. It is `Send` but not `Sync`; open one
/// per thread. Several processes may open the same file: readers run
/// concurrently with one writer.
pub struct Store {
    pub(crate) conn: Connection,
}

impl Store {
    /// Opens the store at `path`, creating the file and schema if missing.
    ///
    /// Errors: the file cannot be opened or created, it is not a SQLite
    /// database, or it has an unsupported store version.
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut conn = Connection::open_with_flags(path, flags)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        // journal_mode returns the resulting mode as a row.
        let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        conn.execute_batch(&format!(
            "PRAGMA synchronous = NORMAL; PRAGMA cache_size = -{CACHE_KIB}; PRAGMA temp_store = MEMORY;"
        ))?;
        schema::ensure(&mut conn)?;
        Ok(Store { conn })
    }

    /// Makes this connection refuse writes (`PRAGMA query_only`). Used by
    /// `serve --read-only`; writes then fail with [`StoreError::ReadOnly`].
    pub fn set_query_only(&self) -> Result<(), StoreError> {
        self.conn.execute_batch("PRAGMA query_only = ON")?;
        Ok(())
    }

    /// Moves everything from the WAL file into the main file and truncates
    /// the WAL, so the store file alone is complete (e.g. for copying).
    /// Readers that are still active can keep part of the WAL alive; that
    /// is not an error.
    pub fn checkpoint(&self) -> Result<(), StoreError> {
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }
}
