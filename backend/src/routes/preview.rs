//! The picture a shared recipe shows when its link is pasted somewhere.
//!
//! Plain bytes at a plain URL: no token, no `Accept` negotiation, nothing to
//! configure. A crawler fetches it with an ordinary GET or it does not fetch
//! it at all, and half of them will not follow a redirect to get it either.
//!
//! Generating a PNG costs tens of milliseconds, and a link posted in a busy
//! channel is fetched by every client that renders it. So the response
//! carries a strong `ETag` built from what the picture is made of — when the
//! recipe last changed, and which photos it has — and answers
//! `If-None-Match` with a 304. The image only has to be drawn when it would
//! actually come out different.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::services::preview::{self, Card};
use crate::state::AppState;

/// A day. The address has no content hash in it — it is the recipe's slug,
/// which is the point — so this cannot be `immutable`; the ETag is what makes
/// a repeat fetch cheap, and a day is how long a stale card may survive in a
/// crawler's cache.
const CACHE: &str = "public, max-age=86400";

#[utoipa::path(
    get, path = "/api/v1/public/recipes/{id}/preview.png", tag = "public",
    params(
        ("id" = String, Path, description = "Recipe slug, a slug it used to have, or its uuid"),
    ),
    responses(
        (status = 200, description = "A 1200×630 card for this shared recipe: its first photo, cropped, or one drawn from its name and figures", content_type = "image/png"),
        (status = 304, description = "Unchanged since the ETag the caller sent"),
        (status = 404, description = "No such recipe, or not shared", body = crate::error::ErrorBody),
    )
)]
pub async fn preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> ApiResult<Response> {
    let resolved = super::public::resolve(&state, &key).await?;
    if !resolved.is_public {
        return Err(ApiError::NotFound("recipe"));
    }

    #[derive(sqlx::FromRow)]
    struct Row {
        name: String,
        servings: f64,
        updated_at: chrono::DateTime<chrono::Utc>,
        cover_id: Option<Uuid>,
        cover_path: Option<String>,
        photo_count: i64,
    }

    // The cover is the first photo uploaded, the same one the recipe card in
    // the app uses, so the preview and the list agree on what the dish looks
    // like.
    let row: Row = sqlx::query_as(
        "SELECT r.name, r.servings, r.updated_at,
                cover.id AS cover_id, cover.relative_path AS cover_path,
                (SELECT count(*) FROM photos p WHERE p.recipe_id = r.id) AS photo_count
         FROM recipes r
         LEFT JOIN LATERAL (
             SELECT p.id, p.relative_path FROM photos p
             WHERE p.recipe_id = r.id ORDER BY p.created_at ASC LIMIT 1
         ) cover ON TRUE
         WHERE r.id = $1",
    )
    .bind(resolved.id)
    .fetch_one(&state.db)
    .await?;

    // Uploading a photo does not touch the recipe's `updated_at`, so the
    // photo set is part of the tag in its own right: otherwise the first
    // photo added to a recipe would never reach anybody's cache.
    let etag = etag(&format!(
        "{}|{}|{}|{}",
        resolved.id,
        row.updated_at.timestamp_micros(),
        row.cover_id.map(|id| id.to_string()).unwrap_or_default(),
        row.photo_count,
    ));

    if unchanged(&headers, &etag) {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag.as_str()),
                (header::CACHE_CONTROL, CACHE),
            ],
        )
            .into_response());
    }

    let photo = match row.cover_path {
        Some(path) => Some(state.photos.read(&path).await?),
        None => None,
    };

    // The card states a serving, so the totals are divided by the servings
    // here and nothing downstream has to remember to.
    let totals: (f64, f64, f64, f64) =
        sqlx::query_as("SELECT calories_kcal, protein_g, carbs_g, fat_g FROM recipe_totals($1)")
            .bind(resolved.id)
            .fetch_one(&state.db)
            .await?;
    let share = if row.servings > 0.0 {
        row.servings
    } else {
        1.0
    };
    let per = Serving {
        calories: totals.0 / share,
        protein: totals.1 / share,
        carbs: totals.2 / share,
        fat: totals.3 / share,
    };

    // Decoding, scaling and encoding are CPU-bound and would otherwise stall
    // every other request sharing this worker thread — the same reason photo
    // uploads are handed off.
    let bytes = tokio::task::spawn_blocking(move || match photo {
        // A photo that will not decode is not a reason to serve no card: the
        // drawn one still says what the recipe is.
        Some(bytes) => {
            preview::from_photo(&bytes, &card(&row.name, row.servings, &per)).or_else(|e| {
                tracing::warn!(error = ?e, "recipe photo could not be used as a preview");
                draw(&row.name, row.servings, &per)
            })
        }
        None => draw(&row.name, row.servings, &per),
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!("preview task failed: {e}")))??;

    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("image/png")),
            (
                header::ETAG,
                HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("\"preview\"")),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static(CACHE)),
        ],
        Body::from(bytes),
    )
        .into_response())
}

/// One serving's figures, so the four numbers travel together rather than as
/// four arguments in an order nobody can check.
struct Serving {
    calories: f64,
    protein: f64,
    carbs: f64,
    fat: f64,
}

fn card<'a>(name: &'a str, servings: f64, per: &Serving) -> Card<'a> {
    Card {
        name,
        servings,
        calories_per_serving: per.calories,
        protein_per_serving: per.protein,
        carbs_per_serving: per.carbs,
        fat_per_serving: per.fat,
    }
}

fn draw(name: &str, servings: f64, per: &Serving) -> Result<Vec<u8>, ApiError> {
    preview::generated(&card(name, servings, per))
}

/// A strong validator: the same bytes always produce the same tag, and any
/// change to what the card is made of produces a different one.
fn etag(material: &str) -> String {
    let digest = Sha256::digest(material.as_bytes());
    format!("\"{:x}\"", digest)
}

/// Whether the caller already holds this exact image.
///
/// `If-None-Match` is a list, and a cache is allowed to weaken a tag on the
/// way past, so both forms of the tag count.
fn unchanged(headers: &HeaderMap, etag: &str) -> bool {
    let Some(sent) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    sent.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate == etag || candidate.trim_start_matches("W/") == etag
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_changes_when_anything_the_card_is_made_of_changes() {
        let base = etag("recipe|1|cover|1");
        assert_eq!(base, etag("recipe|1|cover|1"));
        assert_ne!(base, etag("recipe|2|cover|1"), "a rename must retag");
        assert_ne!(base, etag("recipe|1|other|1"), "a new cover must retag");
        assert_ne!(base, etag("recipe|1|cover|2"), "a new photo must retag");
        assert!(base.starts_with('"') && base.ends_with('"'));
    }

    #[test]
    fn a_caller_holding_the_tag_is_told_nothing_changed() {
        let tag = etag("recipe|1|cover|1");
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, tag.parse().unwrap());
        assert!(unchanged(&headers, &tag));

        // Weakened on the way past a cache, and sent in a list, both count.
        let mut weak = HeaderMap::new();
        weak.insert(
            header::IF_NONE_MATCH,
            format!("\"something-else\", W/{tag}").parse().unwrap(),
        );
        assert!(unchanged(&weak, &tag));
    }

    #[test]
    fn a_caller_holding_an_old_tag_is_not() {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, etag("old").parse().unwrap());
        assert!(!unchanged(&headers, &etag("new")));
        assert!(!unchanged(&HeaderMap::new(), &etag("new")));
    }
}
