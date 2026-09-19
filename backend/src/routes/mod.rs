pub mod admin;
pub mod auth;
pub mod diary;
pub mod foods;
pub mod keys;
pub mod photos;
pub mod profile;
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
    pub database: &'static str,
    /// Whether a USDA FoodData Central API key is configured.
    pub usda_configured: bool,
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
        database: "ok",
        usda_configured: state.usda.is_configured(),
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .nest("/auth", auth::router())
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
}
