// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared server state and the helper that runs store work off the async
//! threads, with a concurrency cap and the request deadline.

use std::sync::Arc;
use std::time::Instant;

use axum::http::StatusCode;
use rikaichad_store::Store;
use tokio::sync::Semaphore;

use crate::config::ServeConfig;
use crate::error::ApiError;
use crate::guard::HostPolicy;
use crate::store_pool::StorePool;

/// State shared by all handlers. Cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ServeConfig>,
    pub pool: StorePool,
    pub host_policy: Arc<HostPolicy>,
    /// Caps concurrent store tasks (one permit per pooled connection), so
    /// a burst of requests queues instead of opening connections and
    /// blocking threads without bound.
    pub store_permits: Arc<Semaphore>,
    /// Import jobs and the single write slot.
    pub jobs: Arc<crate::jobs::Jobs>,
}

impl AppState {
    /// `403 read_only` when the server runs with `--read-only`.
    pub fn require_writable(&self) -> Result<(), ApiError> {
        if self.config.read_only {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "read_only",
                "server is running with --read-only",
            ));
        }
        Ok(())
    }

    /// Runs `f` with a store connection on the blocking thread pool.
    ///
    /// Waits for a free slot (at most [`crate::store_pool::MAX_IDLE`] tasks
    /// run at once) and gives SQLite a deadline: past the configured request
    /// time limit the query is interrupted, so a timed-out request also
    /// stops its work. Fails with `503 timeout` if the slot or the work is
    /// not done in time.
    pub async fn store_work<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Store) -> Result<T, ApiError> + Send + 'static,
    {
        let limit = self.config.request_timeout;
        let deadline = Instant::now() + limit;
        let permit = tokio::time::timeout(limit, Arc::clone(&self.store_permits).acquire_owned())
            .await
            .map_err(|_| timeout_error())?
            .map_err(|_| ApiError::internal("store task limiter closed"))?;
        let pool = self.pool.clone();
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            pool.with_store(|store| {
                store.set_deadline(Some(deadline))?;
                let result = f(store);
                store.set_deadline(None)?;
                result
            })
        });
        match task.await {
            Ok(result) => result,
            Err(join_error) => Err(ApiError::internal(format!(
                "request handler failed: {join_error}"
            ))),
        }
    }
}

/// The `503 timeout` error.
pub fn timeout_error() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "timeout",
        "request took longer than the server's time limit",
    )
}
