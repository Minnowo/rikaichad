// SPDX-License-Identifier: AGPL-3.0-or-later
//! A small pool of store connections for blocking request handlers.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use jisho_store::{Store, StoreError};

/// Most idle connections kept; more are opened on demand and closed after.
const MAX_IDLE: usize = 8;

/// Hands out [`Store`] connections to closures and keeps them for reuse.
/// Cheap to clone (shared inner state).
#[derive(Clone)]
pub struct StorePool {
    inner: Arc<Inner>,
}

struct Inner {
    path: PathBuf,
    query_only: bool,
    idle: Mutex<Vec<Store>>,
}

impl StorePool {
    /// A pool for the store at `path`. With `query_only`, every connection
    /// refuses writes. Opens one connection now so a bad path fails early.
    pub fn new(path: PathBuf, query_only: bool) -> Result<StorePool, StoreError> {
        let pool = StorePool {
            inner: Arc::new(Inner {
                path,
                query_only,
                idle: Mutex::new(Vec::new()),
            }),
        };
        let first = pool.open()?;
        pool.put(first);
        Ok(pool)
    }

    /// Runs `f` with a connection: an idle one if available, else a new
    /// one. The connection goes back to the pool afterwards. Blocking (call
    /// from a blocking thread).
    pub fn with_store<T, E: From<StoreError>>(
        &self,
        f: impl FnOnce(&Store) -> Result<T, E>,
    ) -> Result<T, E> {
        let idle = self.lock().pop();
        let store = match idle {
            Some(store) => store,
            None => self.open()?,
        };
        let result = f(&store);
        self.put(store);
        result
    }

    fn open(&self) -> Result<Store, StoreError> {
        let store = Store::open(&self.inner.path)?;
        if self.inner.query_only {
            store.set_query_only()?;
        }
        Ok(store)
    }

    fn put(&self, store: Store) {
        let mut idle = self.lock();
        if idle.len() < MAX_IDLE {
            idle.push(store);
        }
    }

    /// A poisoned lock only means another thread panicked while holding it;
    /// the Vec inside is still valid, so keep using it.
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Store>> {
        self.inner
            .idle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
