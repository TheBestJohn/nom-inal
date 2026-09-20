//! What a reader who has not signed in can see: a shared recipe, its photos,
//! and the card that stands in for it in a link preview.
//!
//! Shared means public. The share switch on a recipe is one switch with one
//! meaning — every account on the instance can read it, and so can anyone
//! holding the link. There is no separate share token: the recipe's own
//! address is the link, and turning the switch off is how it is revoked.
//! These routes take no credentials at all; a token sent anyway is ignored,
//! so a signed-in reader and a stranger get exactly the same answer.
//!
//! "Its own address" now means its slug, with its uuid still accepted:
//! [`resolve`] takes either, so every link ever handed out keeps working.
//! Every public route resolves through that one function, which is what stops
//! the page, the API and the preview image from disagreeing about which
//! recipe `/r/pierogi-ruskie` means.

use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use uuid::Uuid;

use crate::domain::recipe::PublicRecipe;
use crate::error::{ApiError, ApiResult};
use crate::extract::Json;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/recipes/{id}", get(recipe))
        .route("/recipes/{id}/preview.png", get(super::preview::preview))
        .route("/photos/{id}", get(photo))
}

/// A recipe found by whatever was in the URL.
pub struct Resolved {
    pub id: Uuid,
    /// What it is called today, which is the one canonical address.
    pub slug: String,
    pub is_public: bool,
    /// False when the caller arrived by an old slug or by the uuid. The HTML
    /// page redirects; the API routes do not, because a client that asked for
    /// a resource by a name it holds wants the resource, not a lecture.
    pub canonical: bool,
}

/// Find a recipe by slug, by an old slug, or by uuid.
///
/// Nothing here decides who may see it: the caller does that, by the same
/// rule it would apply to any read. This answers only "which recipe", and it
/// answers it through `recipe_slug_history`, so a slug that has been replaced
/// still points at the recipe that held it and never at any other.
pub async fn resolve(state: &AppState, key: &str) -> ApiResult<Resolved> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        slug: String,
        is_public: bool,
    }

    let row: Option<Row> = match Uuid::parse_str(key) {
        Ok(id) => {
            sqlx::query_as("SELECT id, slug, is_public FROM recipes WHERE id = $1")
                .bind(id)
                .fetch_optional(&state.db)
                .await?
        }
        Err(_) => {
            sqlx::query_as(
                "SELECT r.id, r.slug, r.is_public
                 FROM recipe_slug_history h
                 JOIN recipes r ON r.id = h.recipe_id
                 WHERE h.slug = $1",
            )
            .bind(key)
            .fetch_optional(&state.db)
            .await?
        }
    };

    let row = row.ok_or(ApiError::NotFound("recipe"))?;
    Ok(Resolved {
        canonical: row.slug == key,
        id: row.id,
        slug: row.slug,
        is_public: row.is_public,
    })
}

#[utoipa::path(
    get, path = "/api/v1/public/recipes/{id}", tag = "public",
    params(("id" = String, Path, description = "Recipe slug, a slug it used to have, or its uuid")),
    responses(
        (status = 200, description = "A shared recipe, with its photos, readable without signing in", body = PublicRecipe),
        (status = 404, description = "No such recipe, or not shared", body = crate::error::ErrorBody),
    )
)]
pub async fn recipe(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> ApiResult<Json<PublicRecipe>> {
    let resolved = resolve(&state, &key).await?;
    // The same assembly the signed-in page uses, with nobody as the viewer:
    // only a shared recipe comes back, and it comes back the same.
    let recipe = super::recipes::load_recipe(&state, None, resolved.id).await?;
    let photos = super::photos::list_public(&state, resolved.id).await?;
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
