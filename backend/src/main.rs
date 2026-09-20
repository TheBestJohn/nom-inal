mod auth;
mod config;
mod domain;
mod error;
mod extract;
mod mcp;
mod openapi;
mod routes;
mod services;
mod state;

use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderName, HeaderValue, Method};
use axum::routing::get;
use axum::{Json, Router};
use sqlx::postgres::PgPoolOptions;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nom_inal=info,tower_http=info,sqlx=warn".into()),
        )
        .init();

    let config = Config::from_env()?;

    // `<%` reads its threshold from a session GUC, so it has to be set on each
    // pooled connection rather than passed per query.
    let trgm_threshold = config.trgm_word_threshold;
    let db = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                sqlx::query("SELECT set_config('pg_trgm.word_similarity_threshold', $1, false)")
                    .bind(trgm_threshold.to_string())
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&config.database_url)
        .await?;

    // Migrations are embedded in the binary and run at startup, so a fresh
    // container comes up with a correct schema without a separate deploy step.
    sqlx::migrate!("./migrations").run(&db).await?;
    tracing::info!("migrations applied");

    // Seed the environment-backed settings, but only while each is still at
    // its installation default. A null `*_updated_at` is what marks that: once
    // an administrator has saved a value, a restart must not quietly undo them.
    // The markers are per setting, so closing sign-ups from the admin area
    // does not stop FOOD_QUORUM seeding a quorum nobody has touched.
    if let Some(quorum) = config.initial_food_quorum {
        let seeded = sqlx::query(
            "UPDATE instance_settings SET food_quorum = $1
             WHERE food_quorum_updated_at IS NULL AND food_quorum <> $1",
        )
        .bind(quorum as i32)
        .execute(&db)
        .await?;
        if seeded.rows_affected() > 0 {
            tracing::info!(quorum, "food quorum seeded from FOOD_QUORUM");
        }
    }
    if let Some(open) = config.initial_allow_registration {
        let seeded = sqlx::query(
            "UPDATE instance_settings SET allow_registration = $1
             WHERE allow_registration_updated_at IS NULL AND allow_registration <> $1",
        )
        .bind(open)
        .execute(&db)
        .await?;
        if seeded.rows_affected() > 0 {
            tracing::info!(open, "registration seeded from ALLOW_REGISTRATION");
        }
    }

    let http = reqwest::Client::builder()
        // Open Food Facts asks API clients to identify themselves.
        .user_agent(concat!(
            "nom-inal/",
            env!("CARGO_PKG_VERSION"),
            " (nutrition tracker)"
        ))
        .timeout(Duration::from_secs(15))
        .build()?;

    let bind_addr = config.bind_addr.clone();
    let max_upload = config.max_upload_bytes;
    let cors = build_cors(&config);
    let state = AppState::new(db, config, http);

    // Fail at boot rather than on someone's first upload.
    state.photos.ensure_ready().await?;
    tracing::info!(dir = %state.config.photo_dir, "photo storage ready");

    let api = Router::new()
        .nest("/api/v1", routes::api_router())
        .route(
            "/api/v1/openapi.json",
            get(|| async { Json(openapi::ApiDoc::openapi()) }),
        )
        // Axum caps request bodies at 2 MB by default, which a photo exceeds
        // immediately. The store enforces the real limit after decoding.
        .layer(DefaultBodyLimit::max(max_upload + 1024 * 1024))
        .with_state(state.clone());

    // The MCP server calls the API by dispatching requests to this same
    // router in-process, so it is built from `api` before the transport
    // layers go on: a tool call and an HTTP call reach the handlers by the
    // same path, and the trace and compression layers see only the outside.
    let app = api
        .clone()
        .merge(mcp::router(state, api))
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(cors);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(%bind_addr, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// In production the SPA is served from the same origin by nginx, so CORS is
/// only needed for local development (`vite` on :5173). An explicit origin list
/// keeps credentials-bearing requests from arbitrary origins out.
fn build_cors(config: &Config) -> CorsLayer {
    let base = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            // Browser-based MCP clients and scripts send these.
            HeaderName::from_static("x-api-key"),
            HeaderName::from_static("mcp-protocol-version"),
        ])
        .max_age(Duration::from_secs(3600));

    if config.cors_origins.is_empty() {
        return base.allow_origin(Any);
    }

    let origins: Vec<HeaderValue> = config
        .cors_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    base.allow_origin(origins)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}
