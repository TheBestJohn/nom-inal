//! What a reader who has not signed in can see: a shared recipe, and its
//! photos.
//!
//! Shared means public. The share switch on a recipe is one switch with one
//! meaning — every account on the instance can read it, and so can anyone
//! holding the link. There is no separate share token: the recipe's own id
//! is the link, and turning the switch off is how it is revoked. These two
//! routes take no credentials at all; a token sent anyway is ignored, so a
//! signed-in reader and a stranger get exactly the same answer.

use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use uuid::Uuid;

use crate::domain::recipe::PublicRecipe;
use crate::error::ApiResult;
use crate::extract::Json;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/recipes/{id}", get(recipe))
        .route("/photos/{id}", get(photo))
}

#[utoipa::path(
    get, path = "/api/v1/public/recipes/{id}", tag = "public",
    params(("id" = Uuid, Path, description = "Recipe id")),
    responses(
        (status = 200, description = "A shared recipe, with its photos, readable without signing in", body = PublicRecipe),
        (status = 404, description = "No such recipe, or not shared", body = crate::error::ErrorBody),
    )
)]
pub async fn recipe(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PublicRecipe>> {
    // The same assembly the signed-in page uses, with nobody as the viewer:
    // only a shared recipe comes back, and it comes back the same.
    let recipe = super::recipes::load_recipe(&state, None, id).await?;
    let photos = super::photos::list_public(&state, id).await?;
    Ok(Json(PublicRecipe { recipe, photos }))
}

#[utoipa::path(
    get, path = "/api/v1/public/photos/{id}", tag = "public",
    params(("id" = Uuid, Path, description = "Photo id")),
    responses(
        (status = 200, description = "The image bytes, for a photo of a shared recipe", content_type = "image/jpeg"),
        (status = 404, description = "No such photo, or not one of a shared recipe", body = crate::error::ErrorBody),
    )
)]
pub async fn photo(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Response> {
    // The one visibility rule, with no viewer: a weigh-in photo can never
    // satisfy it, and a recipe photo only while its recipe is shared.
    super::photos::serve_visible(&state, None, id).await
}
