use axum::extract::{Query, State};
use axum::routing::get;
use axum::Router;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

use crate::auth::CurrentUser;
use crate::domain::focus::{self, FocusPreview};
use crate::domain::user::{Profile, TrackingFocus, UpdateProfileRequest, UserRow, ALL_FOCUSES};
use crate::error::{ApiError, ApiResult};
use crate::extract::Json;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_profile).patch(update_profile))
        .route("/focus", get(focus_options).post(set_focus))
        .route("/focus/preview", get(focus_preview))
}

use crate::domain::user::USER_COLUMNS;

#[utoipa::path(
    get, path = "/api/v1/profile", tag = "profile",
    security(("bearer" = [])),
    responses((status = 200, body = Profile))
)]
pub async fn get_profile(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Profile>> {
    let row: UserRow = sqlx::query_as(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1"))
        .bind(user.id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound("user"))?;
    Ok(Json(row.into()))
}

#[utoipa::path(
    patch, path = "/api/v1/profile", tag = "profile",
    security(("bearer" = [])),
    request_body = UpdateProfileRequest,
    responses((status = 200, body = Profile), (status = 400, body = crate::error::ErrorBody))
)]
pub async fn update_profile(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<UpdateProfileRequest>,
) -> ApiResult<Json<Profile>> {
    body.validate()?;

    // COALESCE-style partial update: a field left out of the JSON body arrives
    // as NULL and the existing column value is kept.
    let row: UserRow = sqlx::query_as(&format!(
        "UPDATE users SET
            display_name = COALESCE($2, display_name),
            sex = COALESCE($3, sex),
            birth_date = COALESCE($4, birth_date),
            height_cm = COALESCE($5, height_cm),
            activity_level = COALESCE($6, activity_level),
            goal = COALESCE($7, goal),
            target_weight_kg = COALESCE($8, target_weight_kg),
            -- An empty array is not NULL, so an explicit empty choice survives
            -- the COALESCE that means the field was omitted entirely.
            shown_nutrients = COALESCE($9, shown_nutrients),
            chart_nutrients = COALESCE($10, chart_nutrients),
            chart_mode = COALESCE($11, chart_mode),
            tracking_focus = COALESCE($12, tracking_focus),
            units = COALESCE($13, units),
            updated_at = now()
         WHERE id = $1
         RETURNING {USER_COLUMNS}"
    ))
    .bind(user.id)
    .bind(body.display_name.as_deref().map(str::trim))
    .bind(body.sex.as_deref())
    .bind(body.birth_date)
    .bind(body.height_cm)
    .bind(body.activity_level.as_deref())
    .bind(body.goal.as_deref())
    .bind(body.target_weight_kg)
    .bind(
        body.shown_nutrients
            .as_deref()
            .map(UpdateProfileRequest::nutrient_keys),
    )
    .bind(
        body.chart_nutrients
            .as_deref()
            .map(UpdateProfileRequest::nutrient_keys),
    )
    .bind(body.chart_mode.map(|m| m.as_str()))
    .bind(body.tracking_focus.map(|f| f.as_str()))
    .bind(body.units.map(|u| u.as_str()))
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound("user"))?;

    Ok(Json(row.into()))
}

/// One focus as the picker lists it.
#[derive(Debug, Serialize, ToSchema)]
pub struct FocusOption {
    pub focus: TrackingFocus,
    pub label: &'static str,
    /// One sentence on what the preset puts on screen.
    pub summary: &'static str,
}

#[utoipa::path(
    get, path = "/api/v1/profile/focus", tag = "profile",
    security(("bearer" = [])),
    responses((status = 200, description = "Every focus, in picker order", body = Vec<FocusOption>))
)]
pub async fn focus_options(_user: CurrentUser) -> ApiResult<Json<Vec<FocusOption>>> {
    Ok(Json(
        ALL_FOCUSES
            .iter()
            .map(|f| FocusOption {
                focus: *f,
                label: f.label(),
                summary: f.summary(),
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct FocusQuery {
    /// e.g. `keto`, `blood_pressure`.
    pub focus: String,
}

fn parse_focus(s: &str) -> ApiResult<TrackingFocus> {
    TrackingFocus::parse(s).ok_or_else(|| ApiError::bad_request(format!("unknown focus '{s}'")))
}

#[utoipa::path(
    get, path = "/api/v1/profile/focus/preview", tag = "profile",
    security(("bearer" = [])),
    params(FocusQuery),
    responses(
        (status = 200, description = "What applying this focus would set", body = FocusPreview),
        (status = 400, body = crate::error::ErrorBody),
    )
)]
pub async fn focus_preview(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<FocusQuery>,
) -> ApiResult<Json<FocusPreview>> {
    let focus = parse_focus(&q.focus)?;
    let inputs = super::targets::energy_inputs(&state, user.id).await?;
    Ok(Json(focus::preview(
        focus,
        &inputs,
        Utc::now().date_naive(),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetFocusRequest {
    pub focus: TrackingFocus,
    /// Also write the preset's targets and display preferences. False records
    /// the answer and changes nothing else.
    #[serde(default)]
    pub apply: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetFocusResponse {
    pub profile: Profile,
    /// What was written, when `apply` was set. A target the profile could not
    /// price is listed here without an amount rather than silently skipped.
    pub applied: Option<FocusPreview>,
}

#[utoipa::path(
    post, path = "/api/v1/profile/focus", tag = "profile",
    security(("bearer" = [])),
    request_body = SetFocusRequest,
    responses(
        (status = 200, description = "The focus, and what applying it wrote", body = SetFocusResponse),
        (status = 400, body = crate::error::ErrorBody),
    )
)]
pub async fn set_focus(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<SetFocusRequest>,
) -> ApiResult<Json<SetFocusResponse>> {
    let inputs = super::targets::energy_inputs(&state, user.id).await?;
    let preview = focus::preview(body.focus, &inputs, Utc::now().date_naive());

    // The focus, the display preferences and the targets land in one
    // transaction: a preset half-applied would be worse than none, because
    // the readouts would show a nutrient the chart has no target for.
    let mut tx = state.db.begin().await?;

    let apply_display = body.apply && preview.changes_display;
    let row: UserRow = sqlx::query_as(&format!(
        "UPDATE users SET
            tracking_focus = $2,
            goal = COALESCE($3, goal),
            shown_nutrients = COALESCE($4, shown_nutrients),
            chart_nutrients = COALESCE($5, chart_nutrients),
            chart_mode = COALESCE($6, chart_mode),
            updated_at = now()
         WHERE id = $1
         RETURNING {USER_COLUMNS}"
    ))
    .bind(user.id)
    .bind(body.focus.as_str())
    .bind(body.apply.then_some(preview.goal).flatten())
    .bind(apply_display.then(|| UpdateProfileRequest::nutrient_keys(&preview.shown_nutrients)))
    .bind(apply_display.then(|| UpdateProfileRequest::nutrient_keys(&preview.chart_nutrients)))
    .bind(apply_display.then(|| preview.chart_mode.as_str()))
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound("user"))?;

    if body.apply && !preview.targets.is_empty() {
        // Applied, not enforced: the preset writes ordinary targets, the same
        // rows PUT /targets writes, so every one of them stays editable.
        let priced: Vec<_> = preview
            .targets
            .iter()
            .filter_map(|t| t.amount.map(|a| (t.nutrient.key(), a, t.kind.as_str())))
            .collect();
        sqlx::query("DELETE FROM nutrition_targets WHERE user_id = $1")
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
        if !priced.is_empty() {
            let nutrients: Vec<&str> = priced.iter().map(|p| p.0).collect();
            let amounts: Vec<f64> = priced.iter().map(|p| p.1).collect();
            let kinds: Vec<&str> = priced.iter().map(|p| p.2).collect();
            sqlx::query(
                "INSERT INTO nutrition_targets (user_id, nutrient, amount, kind)
                 SELECT $1, * FROM UNNEST($2::text[], $3::float8[], $4::text[])",
            )
            .bind(user.id)
            .bind(&nutrients)
            .bind(&amounts)
            .bind(&kinds)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    Ok(Json(SetFocusResponse {
        profile: row.into(),
        applied: body.apply.then_some(preview),
    }))
}
