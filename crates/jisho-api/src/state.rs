// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared server state and the helper that runs store work off the async
//! threads with the request time limit.

use std::sync::Arc;

use axum::http::StatusCode;

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
}

impl AppState {
    /// Runs blocking store work `f` on the blocking pool, failing with
    /// `503 timeout` after the configured request time limit. The work
    /// itself is not cancelled (SQLite calls cannot be interrupted here);
    /// only the response is.
    pub async fn blocking<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&StorePool) -> Result<T, ApiError> + Send + 'static,
    {
        let pool = self.pool.clone();
        let task = tokio::task::spawn_blocking(move || f(&pool));
        match tokio::time::timeout(self.config.request_timeout, task).await {
            Ok(Ok(result)) => result,
            Ok(Err(join_error)) => Err(ApiError::internal(format!(
                "request handler failed: {join_error}"
            ))),
            Err(_) => Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "timeout",
                "request took longer than the server's time limit",
            )),
        }
    }
}
