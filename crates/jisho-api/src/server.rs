// SPDX-License-Identifier: AGPL-3.0-or-later
//! Router construction and the server loop.

use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::config::ServeConfig;
use crate::guard::{HostPolicy, guard, request_timeout};
use crate::handlers::{
    dictionaries_list, dictionaries_manage, fallback, health, jobs, kanji_lookup, settings, source,
    terms_lookup, text_parse,
};
use crate::listener::GuardedListener;
use crate::state::AppState;
use crate::store_pool::{MAX_IDLE, StorePool};

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
    router_with_jobs(config, Arc::new(crate::jobs::Jobs::default()))
}

fn router_with_jobs(
    config: ServeConfig,
    jobs: Arc<crate::jobs::Jobs>,
) -> Result<Router, ServerError> {
    let pool =
        StorePool::new(config.store_path.clone(), config.read_only).map_err(|e| ServerError {
            message: format!("cannot open store {}: {e}", config.store_path.display()),
        })?;
    let state = AppState {
        host_policy: Arc::new(HostPolicy::from_config(&config)),
        config: Arc::new(config),
        pool,
        store_permits: Arc::new(tokio::sync::Semaphore::new(MAX_IDLE)),
        jobs,
    };
    Ok(Router::new()
        .route("/v1/health", get(health::health))
        .route("/v1/terms/lookup", post(terms_lookup::terms_lookup))
        .route("/v1/kanji/lookup", post(kanji_lookup::kanji_lookup))
        .route("/v1/text/parse", post(text_parse::text_parse))
        .route(
            "/v1/dictionaries",
            get(dictionaries_list::dictionaries_list).post(dictionaries_manage::upload),
        )
        .route("/v1/dictionaries/order", put(dictionaries_manage::order))
        .route(
            "/v1/dictionaries/{id}",
            delete(dictionaries_manage::remove).patch(dictionaries_manage::patch),
        )
        .route(
            "/v1/settings",
            get(settings::get_settings).patch(settings::patch_settings),
        )
        .route("/v1/jobs/{id}", get(jobs::get_job).delete(jobs::delete_job))
        .route("/v1/source", get(source::source))
        .fallback(fallback::not_found)
        .method_not_allowed_fallback(fallback::method_not_allowed)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            request_timeout,
        ))
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
    let max_connections = config.max_connections;
    let idle_timeout = config.idle_timeout;
    let jobs = Arc::new(crate::jobs::Jobs::default());
    let app = router_with_jobs(config, Arc::clone(&jobs))?;
    let shutdown_jobs = Arc::clone(&jobs);
    let served = runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(bind)
            .await
            .map_err(|e| ServerError {
                message: format!("cannot listen on {bind}: {e}"),
            })?;
        let local = listener.local_addr().map_err(|e| ServerError {
            message: format!("cannot read listening address: {e}"),
        })?;
        on_ready(local);
        let listener = GuardedListener::new(listener, max_connections, idle_timeout);
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                // If the signal handler cannot be installed, run until killed.
                if tokio::signal::ctrl_c().await.is_err() {
                    std::future::pending::<()>().await;
                }
                // A running import stops within CANCEL_CHECK_ROWS rows and
                // rolls back.
                shutdown_jobs.cancel_all();
            })
            .await
            .map_err(|e| ServerError {
                message: format!("server error: {e}"),
            })
    });
    // Imports run on blocking threads, which dropping the runtime would wait
    // for without limit; they were cancelled above, so this is short. An
    // import still running after it is rolled back when the process exits.
    runtime.shutdown_timeout(SHUTDOWN_GRACE);
    served
}

/// How long shutdown waits for cancelled imports to stop.
const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(10);
