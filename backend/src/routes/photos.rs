use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::domain::photo::{public_photo_url, Photo, PhotoRow};
use crate::error::{ApiError, ApiResult};
use crate::extract::Json;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{id}", get(serve).delete(delete))
        .route("/{id}/caption", axum::routing::patch(set_caption))
}

/// Mounted under `/weights` so a photo is created against the weigh-in it
/// belongs to.
pub fn weight_photo_router() -> Router<AppState> {
    Router::new().route("/{id}/photos", get(list_for_weight).post(upload_for_weight))
}

/// Mounted under `/recipes`, likewise.
pub fn recipe_photo_router() -> Router<AppState> {
    Router::new().route("/{id}/photos", get(list_for_recipe).post(upload_for_recipe))
}

const COLUMNS: &str = "p.id, p.weight_entry_id, p.recipe_id, p.relative_path, p.content_type, \
                       p.byte_size, p.width, p.height, p.caption, p.created_at";

/// Who may see a photo, as a WHERE fragment over `photos p` joined to
/// `recipes r`. One definition, used by every read: a weigh-in photo is its
/// owner's alone; a recipe photo goes with the recipe, so a shared recipe's
/// photos are shared too.
///
/// `$2` is the viewer, and may be NULL for someone who has not signed in.
/// SQL's three-valued logic then does the right thing without a second
/// rule: `NULL = user_id` is unknown, so only `r.is_public` being true can
/// make the row visible, and a weigh-in photo — whose `r` is the empty side
/// of an outer join — never is.
const VISIBLE: &str = "(p.user_id = $2 OR r.is_public)";

/// What a photo is attached to. The upload, the ownership check before it
/// and the insert after it are the same for both; only the table asked and
/// the column written differ.
#[derive(Clone, Copy)]
enum Subject {
    WeightEntry(Uuid),
    Recipe(Uuid),
}

impl Subject {
    /// Confirm the subject is this user's before accepting any bytes. Both
    /// cases are a 404 rather than a 403: a subject you do not own is not
    /// distinguishable from one that does not exist, on purpose.
    async fn require_owned(self, state: &AppState, user_id: Uuid) -> ApiResult<()> {
        let (sql, id, what) = match self {
            Subject::WeightEntry(id) => (
                "SELECT id FROM weight_entries WHERE id = $1 AND user_id = $2",
                id,
                "weight entry",
            ),
            Subject::Recipe(id) => (
                "SELECT id FROM recipes WHERE id = $1 AND user_id = $2",
                id,
                "recipe",
            ),
        };
        let owned: Option<(Uuid,)> = sqlx::query_as(sql)
            .bind(id)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await?;
        owned.map(|_| ()).ok_or(ApiError::NotFound(what))
    }
}

#[utoipa::path(
    get, path = "/api/v1/weights/{id}/photos", tag = "photos",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Weight entry id")),
    responses((status = 200, body = Vec<Photo>), (status = 404, body = crate::error::ErrorBody))
)]
pub async fn list_for_weight(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(entry_id): Path<Uuid>,
) -> ApiResult<Json<Vec<Photo>>> {
    let rows: Vec<PhotoRow> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM photos p
         WHERE p.weight_entry_id = $1 AND p.user_id = $2
         ORDER BY p.created_at ASC"
    ))
    .bind(entry_id)
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    get, path = "/api/v1/recipes/{id}/photos", tag = "photos",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Recipe id")),
    responses(
        (status = 200, body = Vec<Photo>),
        (status = 404, description = "No such recipe, or not one you can see", body = crate::error::ErrorBody),
    )
)]
pub async fn list_for_recipe(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(recipe_id): Path<Uuid>,
) -> ApiResult<Json<Vec<Photo>>> {
    // A recipe you cannot see has no photos you can see, and an empty list
    // would be indistinguishable from a recipe that has none. Say which.
    let visible: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM recipes WHERE id = $1 AND (user_id = $2 OR is_public)")
            .bind(recipe_id)
            .bind(user.id)
            .fetch_optional(&state.db)
            .await?;
    if visible.is_none() {
        return Err(ApiError::NotFound("recipe"));
    }

    let rows: Vec<PhotoRow> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM photos p
         WHERE p.recipe_id = $1
         ORDER BY p.created_at ASC"
    ))
    .bind(recipe_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// The photos of a recipe as a reader with no token gets them: only when the
/// recipe is public, and with each URL pointing at the public photo route.
/// The same `VISIBLE` rule as every other read, with no viewer.
pub async fn list_public(state: &AppState, recipe_id: Uuid) -> ApiResult<Vec<Photo>> {
    let rows: Vec<PhotoRow> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM photos p
         JOIN recipes r ON r.id = p.recipe_id
         WHERE p.recipe_id = $1 AND {VISIBLE}
         ORDER BY p.created_at ASC"
    ))
    .bind(recipe_id)
    .bind(Option::<Uuid>::None)
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let mut photo: Photo = row.into();
            photo.url = public_photo_url(photo.id);
            photo
        })
        .collect())
}

#[utoipa::path(
    post, path = "/api/v1/weights/{id}/photos", tag = "photos",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Weight entry id")),
    request_body(content = String, description = "multipart/form-data with a `file` part and an optional `caption`", content_type = "multipart/form-data"),
    responses(
        (status = 201, body = Photo),
        (status = 400, description = "Not an image, or too large", body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody),
    )
)]
pub async fn upload_for_weight(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(entry_id): Path<Uuid>,
    multipart: Multipart,
) -> ApiResult<(StatusCode, Json<Photo>)> {
    upload(&state, user.id, Subject::WeightEntry(entry_id), multipart).await
}

#[utoipa::path(
    post, path = "/api/v1/recipes/{id}/photos", tag = "photos",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Recipe id")),
    request_body(content = String, description = "multipart/form-data with a `file` part and an optional `caption`", content_type = "multipart/form-data"),
    responses(
        (status = 201, body = Photo),
        (status = 400, description = "Not an image, or too large", body = crate::error::ErrorBody),
        (status = 404, description = "No such recipe, or not yours", body = crate::error::ErrorBody),
    )
)]
pub async fn upload_for_recipe(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(recipe_id): Path<Uuid>,
    multipart: Multipart,
) -> ApiResult<(StatusCode, Json<Photo>)> {
    upload(&state, user.id, Subject::Recipe(recipe_id), multipart).await
}

async fn upload(
    state: &AppState,
    user_id: Uuid,
    subject: Subject,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<Photo>)> {
    subject.require_owned(state, user_id).await?;

    let mut file: Option<Vec<u8>> = None;
    let mut caption: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("malformed upload: {e}")))?
    {
        match field.name() {
            Some("file") => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("could not read the file: {e}")))?;
                file = Some(bytes.to_vec());
            }
            Some("caption") => {
                let text = field.text().await.unwrap_or_default();
                let text = text.trim();
                if !text.is_empty() {
                    caption = Some(text.chars().take(500).collect());
                }
            }
            _ => {}
        }
    }

    let bytes = file.ok_or_else(|| ApiError::bad_request("a `file` part is required"))?;
    let stored = state.photos.store(user_id, bytes).await?;

    let (weight_entry_id, recipe_id) = match subject {
        Subject::WeightEntry(id) => (Some(id), None),
        Subject::Recipe(id) => (None, Some(id)),
    };

    let inserted: Result<PhotoRow, sqlx::Error> = sqlx::query_as(&format!(
        "INSERT INTO photos AS p
            (user_id, weight_entry_id, recipe_id, relative_path, content_type,
             byte_size, width, height, caption)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING {COLUMNS}"
    ))
    .bind(user_id)
    .bind(weight_entry_id)
    .bind(recipe_id)
    .bind(&stored.relative_path)
    .bind(stored.content_type)
    .bind(stored.byte_size)
    .bind(stored.width)
    .bind(stored.height)
    .bind(caption.as_deref())
    .fetch_one(&state.db)
    .await;

    let row: PhotoRow = match inserted {
        Ok(row) => row,
        Err(e) => {
            // The file is already written; without this it would linger with
            // nothing pointing at it.
            state.photos.remove(&stored.relative_path).await;
            return Err(ApiError::from(e));
        }
    };

    Ok((StatusCode::CREATED, Json(row.into())))
}

#[utoipa::path(
    get, path = "/api/v1/photos/{id}", tag = "photos",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Photo id")),
    responses(
        (status = 200, description = "The image bytes", content_type = "image/jpeg"),
        (status = 404, body = crate::error::ErrorBody),
    )
)]
pub async fn serve(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Response> {
    serve_visible(&state, Some(user.id), id).await
}

/// The bytes of a photo, if `viewer` may see it. `None` is a reader who has
/// not signed in, for whom only a public recipe's photos exist.
///
/// The visibility rule is applied here, on the bytes, rather than only on
/// the listings: a guessable URL must not be enough to read a photo that was
/// not meant for you. The public route calls this with no viewer rather than
/// carrying a rule of its own, so the two cannot drift.
pub async fn serve_visible(
    state: &AppState,
    viewer: Option<Uuid>,
    id: Uuid,
) -> ApiResult<Response> {
    let row: PhotoRow = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM photos p
         LEFT JOIN recipes r ON r.id = p.recipe_id
         WHERE p.id = $1 AND {VISIBLE}"
    ))
    .bind(id)
    .bind(viewer)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound("photo"))?;

    let bytes = state.photos.read(&row.relative_path).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&row.content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("image/jpeg")),
    );
    // The bytes at a given id never change, and the response is per-user, so
    // it can be cached hard but only in that user's own browser.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );

    Ok((headers, Body::from(bytes)).into_response())
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct CaptionRequest {
    pub caption: Option<String>,
}

#[utoipa::path(
    patch, path = "/api/v1/photos/{id}/caption", tag = "photos",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Photo id")),
    request_body = CaptionRequest,
    responses((status = 200, body = Photo), (status = 404, body = crate::error::ErrorBody))
)]
pub async fn set_caption(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CaptionRequest>,
) -> ApiResult<Json<Photo>> {
    let caption = body
        .caption
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(|c| c.chars().take(500).collect::<String>());

    // Writes stay with the uploader: being able to see a shared recipe's
    // photo is not being able to retitle it.
    let row: PhotoRow = sqlx::query_as(&format!(
        "UPDATE photos AS p SET caption = $3 WHERE p.id = $1 AND p.user_id = $2 RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(user.id)
    .bind(caption)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound("photo"))?;

    Ok(Json(row.into()))
}

#[utoipa::path(
    delete, path = "/api/v1/photos/{id}", tag = "photos",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Photo id")),
    responses((status = 204, description = "Deleted"), (status = 404, body = crate::error::ErrorBody))
)]
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    // Delete the row first and take the path back with it, so a failure leaves
    // an unreferenced file rather than a row pointing at nothing.
    let path: Option<(String,)> =
        sqlx::query_as("DELETE FROM photos WHERE id = $1 AND user_id = $2 RETURNING relative_path")
            .bind(id)
            .bind(user.id)
            .fetch_optional(&state.db)
            .await?;

    let Some((relative_path,)) = path else {
        return Err(ApiError::NotFound("photo"));
    };

    state.photos.remove(&relative_path).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove every photo belonging to a weigh-in, files included.
///
/// The foreign key would cascade the rows on its own, but nothing in the
/// database knows about the filesystem, so deleting a weigh-in has to reclaim
/// the files explicitly or they are orphaned forever.
pub async fn delete_for_weight_entry(
    state: &AppState,
    user_id: Uuid,
    entry_id: Uuid,
) -> ApiResult<()> {
    let paths: Vec<(String,)> = sqlx::query_as(
        "DELETE FROM photos WHERE weight_entry_id = $1 AND user_id = $2
         RETURNING relative_path",
    )
    .bind(entry_id)
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    remove_files(state, paths.into_iter().map(|(p,)| p)).await;
    Ok(())
}

/// Delete a recipe's photo rows inside the caller's transaction and hand back
/// the file paths to reclaim once it commits.
///
/// Split in two, unlike the weigh-in version, because deleting a recipe can
/// still fail after this point — it may be logged in a diary — and a rolled
/// back transaction must leave the photos exactly as they were. Files are
/// only removed by `remove_files` after the commit.
pub async fn delete_for_recipe(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    recipe_id: Uuid,
) -> ApiResult<Vec<String>> {
    let paths: Vec<(String,)> = sqlx::query_as(
        "DELETE FROM photos WHERE recipe_id = $1 AND user_id = $2 RETURNING relative_path",
    )
    .bind(recipe_id)
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(paths.into_iter().map(|(p,)| p).collect())
}

pub async fn remove_files(state: &AppState, paths: impl IntoIterator<Item = String>) {
    for path in paths {
        state.photos.remove(&path).await;
    }
}
