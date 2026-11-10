// SPDX-License-Identifier: AGPL-3.0-or-later
//! Router construction and the server loop.

use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};

use crate::config::ServeConfig;
use crate::guard::{HostPolicy, guard};
use crate::handlers::{dictionaries_list, fallback, health, terms_lookup};
use crate::state::AppState;
use crate::store_pool::StorePool;

/// Why the server could not start or stopped with an error.
#[derive(Debug)]
pub struct ServerError {
    pub message: String,
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ServerError {}

/// Builds the API router for `config`, opening the store pool. Used by
/// [`run`] and by tests.
pub fn router(config: ServeConfig) -> Result<Router, ServerError> {
    let pool =
        StorePool::new(config.store_path.clone(), config.read_only).map_err(|e| ServerError {
            message: format!("cannot open store {}: {e}", config.store_path.display()),
        })?;
    let state = AppState {
        host_policy: Arc::new(HostPolicy::from_config(&config)),
        config: Arc::new(config),
        pool,
    };
    Ok(Router::new()
        .route("/v1/health", get(health::health))
        .route("/v1/terms/lookup", post(terms_lookup::terms_lookup))
        .route(
            "/v1/dictionaries",
            get(dictionaries_list::dictionaries_list),
        )
        .fallback(fallback::not_found)
        .method_not_allowed_fallback(fallback::method_not_allowed)
        .layer(axum::middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state))
}

/// Serves the API on `config.bind` until Ctrl-C. Calls `on_ready` with the
/// actual listening address (useful with port 0) once the socket is bound.
/// Blocks the calling thread; builds its own async runtime.
pub fn run(config: ServeConfig, on_ready: impl FnOnce(SocketAddr)) -> Result<(), ServerError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_io()
        .enable_time()
        .build()
        .map_err(|e| ServerError {
            message: format!("cannot start async runtime: {e}"),
        })?;
    let bind = config.bind;
    let app = router(config)?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(bind)
            .await
            .map_err(|e| ServerError {
                message: format!("cannot listen on {bind}: {e}"),
            })?;
        let local = listener.local_addr().map_err(|e| ServerError {
            message: format!("cannot read listening address: {e}"),
        })?;
        on_ready(local);
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                // If the signal handler cannot be installed, run until killed.
                if tokio::signal::ctrl_c().await.is_err() {
                    std::future::pending::<()>().await;
                }
            })
            .await
            .map_err(|e| ServerError {
                message: format!("server error: {e}"),
            })
    })
}
