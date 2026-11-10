// SPDX-License-Identifier: AGPL-3.0-or-later
//! Opening the store file and connection-level settings.

use std::path::Path;
use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags};

use crate::error::StoreError;
use crate::schema;

/// How long a connection waits for another writer before giving up with
/// [`StoreError::Busy`].
const BUSY_TIMEOUT: Duration = Duration::from_secs(2);

/// Page cache per connection, in KiB (negative `cache_size` means KiB).
const CACHE_KIB: i64 = 64 * 1024;

/// SQLite virtual machine steps between deadline checks.
const DEADLINE_CHECK_STEPS: i32 = 1000;

/// Pause between attempts to switch a fresh store to WAL mode.
const WAL_RETRY_PAUSE: Duration = Duration::from_millis(10);

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
        enable_wal(&conn)?;
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

    /// Makes queries on this connection fail with
    /// [`StoreError::Interrupted`] once `deadline` has passed (checked every
    /// few thousand SQLite steps); `None` removes the deadline. Used to stop
    /// work for HTTP requests that timed out.
    pub fn set_deadline(&mut self, deadline: Option<Instant>) -> Result<(), StoreError> {
        match deadline {
            Some(deadline) => self.conn.progress_handler(
                DEADLINE_CHECK_STEPS,
                Some(move || Instant::now() >= deadline),
            )?,
            None => self
                .conn
                .progress_handler(DEADLINE_CHECK_STEPS, None::<fn() -> bool>)?,
        }
        Ok(())
    }

    /// Moves everything from the WAL file into the main file and truncates
    /// the WAL, so the store file alone is complete (e.g. for copying).
    ///
    /// Returns `Ok(true)` when the WAL was fully checkpointed and `Ok(false)`
    /// when another connection was reading and kept part of it alive (SQLite
    /// finishes the job later, at the latest when the last connection
    /// closes). Waits up to the busy timeout for readers.
    pub fn checkpoint(&self) -> Result<bool, StoreError> {
        let busy: i64 = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        Ok(busy == 0)
    }
}

/// Switches the database to WAL mode. On a fresh file this needs an
/// exclusive lock, and SQLite's busy handler does not retry it, so two
/// processes creating the store at once retry here for up to the busy
/// timeout. On a store already in WAL mode it returns at once.
fn enable_wal(conn: &Connection) -> Result<(), StoreError> {
    let started = Instant::now();
    loop {
        // journal_mode returns the resulting mode as a row.
        let result: rusqlite::Result<String> =
            conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0));
        match result {
            Ok(_) => return Ok(()),
            Err(err) => {
                let error = StoreError::from(err);
                if !matches!(error, StoreError::Busy) || started.elapsed() >= BUSY_TIMEOUT {
                    return Err(error);
                }
                std::thread::sleep(WAL_RETRY_PAUSE);
            }
        }
    }
}
