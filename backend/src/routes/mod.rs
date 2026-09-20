pub mod admin;
pub mod auth;
pub mod diary;
pub mod estimates;
pub mod foods;
pub mod keys;
pub mod photos;
pub mod profile;
pub mod public;
pub mod recipes;
pub mod reminders;
pub mod search;
pub mod targets;
pub mod weights;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct Health {
    pub status: &'static str,
    pub version: &'static str,
    /// The commit this binary was built from, when the build said. Null for a
    /// source build that did not: an unknown provenance is reported as
    /// unknown rather than guessed from a working tree.
    pub git_sha: Option<&'static str>,
    /// When the binary was built, RFC 3339 UTC, on the same terms.
    pub built_at: Option<&'static str>,
    pub database: &'static str,
    /// Whether a USDA FoodData Central API key is configured.
    pub usda_configured: bool,
}

/// Build provenance is read at compile time from `NOM_GIT_SHA` and
/// `NOM_BUILT_AT`, which the Dockerfile sets from build args and the release
/// workflow supplies. An empty value counts as unset so a Dockerfile `ENV`
/// with no arg behind it does not report a build from commit "".
fn build_stamp(value: Option<&'static str>) -> Option<&'static str> {
    value.filter(|v| !v.trim().is_empty())
}

#[utoipa::path(
    get, path = "/api/v1/health", tag = "meta",
    responses((status = 200, body = Health), (status = 500, body = crate::error::ErrorBody))
)]
pub async fn health(State(state): State<AppState>) -> ApiResult<Json<Health>> {
    // Touch the pool so the check fails when the database is unreachable —
    // a health endpoint that only proves the process is alive is not much use
    // to a container orchestrator.
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await?;

    Ok(Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        git_sha: build_stamp(option_env!("NOM_GIT_SHA")),
        built_at: build_stamp(option_env!("NOM_BUILT_AT")),
        database: "ok",
        usda_configured: state.usda.is_configured(),
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .nest("/auth", auth::router())
        // Readable without a token: a shared recipe and its photos. Shared
        // means public, and this is the public half of it.
        .nest("/public", public::router())
        .nest("/admin", admin::router())
        .nest("/keys", keys::router())
        .nest("/profile", profile::router())
        .nest("/weights", weights::router())
        .nest("/weights", photos::weight_photo_router())
        .nest("/photos", photos::router())
        .nest("/reminders", reminders::router())
        .nest("/foods", foods::router())
        .nest("/recipes", recipes::router())
        .nest("/recipes", photos::recipe_photo_router())
        .nest("/targets", targets::router())
        .nest("/search", search::router())
        .nest("/diary", diary::router())
        .nest("/estimates", estimates::router())
}
