use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow, ToSchema)]
pub struct Photo {
    pub id: Uuid,
    /// Set when the photo is a progress photo on a weigh-in. Exactly one of
    /// this and `recipe_id` is set.
    pub weight_entry_id: Option<Uuid>,
    /// Set when the photo is of a recipe.
    pub recipe_id: Option<Uuid>,
    pub content_type: String,
    pub byte_size: i64,
    pub width: i32,
    pub height: i32,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Where to fetch the bytes. Served by the API rather than as a static
    /// file so the visibility check cannot be bypassed by guessing a URL —
    /// a progress photo is its owner's alone, and a recipe photo is visible
    /// only to whoever can see the recipe.
    pub url: String,
}

/// The row as stored. `relative_path` never leaves the server: exposing it
/// would invite clients to construct their own file paths.
#[derive(Debug, FromRow)]
pub struct PhotoRow {
    pub id: Uuid,
    pub weight_entry_id: Option<Uuid>,
    pub recipe_id: Option<Uuid>,
    pub relative_path: String,
    pub content_type: String,
    pub byte_size: i64,
    pub width: i32,
    pub height: i32,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// The one place a photo's URL is spelled out, so a list summary that wants
/// to point at a cover photo says the same thing the photo itself does.
pub fn photo_url(id: Uuid) -> String {
    format!("/api/v1/photos/{id}")
}

impl From<PhotoRow> for Photo {
    fn from(r: PhotoRow) -> Self {
        Self {
            url: photo_url(r.id),
            id: r.id,
            weight_entry_id: r.weight_entry_id,
            recipe_id: r.recipe_id,
            content_type: r.content_type,
            byte_size: r.byte_size,
            width: r.width,
            height: r.height,
            caption: r.caption,
            created_at: r.created_at,
        }
    }
}
