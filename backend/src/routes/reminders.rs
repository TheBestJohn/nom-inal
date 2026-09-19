//! Cadence reminders.
//!
//! There is no scheduler here on purpose. A reminder stores only "how often",
//! and whether you are due is derived, on read, from the data you already
//! record. So there is no job queue to run, nothing to catch up after the
//! container has been down for a week, and no way for a stored "next due" date
//! to drift out of step with what actually happened.
//!
//! The trade is that nothing can reach out to you — these surface in the app,
//! not in your inbox.

use axum::extract::State;
use axum::routing::get;
use axum::Router;
use chrono::{NaiveDate, Utc};
use uuid::Uuid;
use validator::Validate;

use crate::auth::CurrentUser;
use crate::domain::reminder::{
    Reminder, ReminderKind, ReminderStatus, ReplaceRemindersRequest, ALL_KINDS,
};
use crate::error::{ApiError, ApiResult};
use crate::extract::Json;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).put(replace))
        .route("/status", get(status))
}

#[utoipa::path(
    get, path = "/api/v1/reminders", tag = "reminders",
    security(("bearer" = [])),
    responses((status = 200, description = "Every kind, with your cadence or its default", body = Vec<Reminder>))
)]
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Reminder>>> {
    Ok(Json(load(&state, user.id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/reminders", tag = "reminders",
    security(("bearer" = [])),
    request_body = ReplaceRemindersRequest,
    responses((status = 200, body = Vec<Reminder>), (status = 400, body = crate::error::ErrorBody))
)]
pub async fn replace(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<ReplaceRemindersRequest>,
) -> ApiResult<Json<Vec<Reminder>>> {
    body.validate()?;

    let mut seen = Vec::new();
    for r in &body.reminders {
        if seen.contains(&r.kind) {
            return Err(ApiError::bad_request(format!(
                "duplicate reminder for '{}'",
                r.kind.key()
            )));
        }
        seen.push(r.kind);
    }

    let kinds: Vec<String> = body.reminders.iter().map(|r| r.kind.key().into()).collect();
    let days: Vec<i32> = body.reminders.iter().map(|r| r.every_days).collect();
    let enabled: Vec<bool> = body.reminders.iter().map(|r| r.enabled).collect();

    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM reminders WHERE user_id = $1")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;

    if !kinds.is_empty() {
        sqlx::query(
            "INSERT INTO reminders (user_id, kind, every_days, enabled)
             SELECT $1, * FROM UNNEST($2::text[], $3::int[], $4::bool[])",
        )
        .bind(user.id)
        .bind(&kinds)
        .bind(&days)
        .bind(&enabled)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(Json(load(&state, user.id).await?))
}

#[utoipa::path(
    get, path = "/api/v1/reminders/status", tag = "reminders",
    security(("bearer" = [])),
    responses((status = 200, description = "Where each enabled reminder stands today", body = Vec<ReminderStatus>))
)]
pub async fn status(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<ReminderStatus>>> {
    let reminders = load(&state, user.id).await?;
    let today = Utc::now().date_naive();

    // One query for all three "when did this last happen" dates rather than
    // three round trips.
    let last: (Option<NaiveDate>, Option<NaiveDate>, Option<NaiveDate>) = sqlx::query_as(
        "SELECT
            (SELECT max(recorded_on) FROM weight_entries WHERE user_id = $1),
            (SELECT max(logged_on)   FROM diary_entries  WHERE user_id = $1),
            (SELECT max(w.recorded_on)
               FROM photos p
               -- The join is the filter: a recipe photo has no weigh-in and
               -- must not count as a progress photo.
               JOIN weight_entries w ON w.id = p.weight_entry_id
              WHERE p.user_id = $1)",
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    let statuses = reminders
        .into_iter()
        .filter(|r| r.enabled)
        .map(|r| {
            let last_on = match r.kind {
                ReminderKind::WeighIn => last.0,
                ReminderKind::FoodLog => last.1,
                ReminderKind::ProgressPhoto => last.2,
            };
            ReminderStatus::evaluate(r.kind, r.every_days, r.enabled, last_on, today)
        })
        .collect();

    Ok(Json(statuses))
}

/// Every kind is returned, whether or not the user has saved one, so a client
/// can render the full settings form without knowing the defaults itself.
async fn load(state: &AppState, user_id: Uuid) -> ApiResult<Vec<Reminder>> {
    let rows: Vec<(String, i32, bool)> =
        sqlx::query_as("SELECT kind, every_days, enabled FROM reminders WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&state.db)
            .await?;

    Ok(ALL_KINDS
        .into_iter()
        .map(|kind| {
            let saved = rows.iter().find(|(k, _, _)| k == kind.key());
            Reminder {
                kind,
                label: kind.label(),
                every_days: saved
                    .map(|(_, d, _)| *d)
                    .unwrap_or(kind.default_every_days()),
                // A kind with no saved row is off: reminders are opt-in, not
                // something a new account is immediately nagged by.
                enabled: saved.map(|(_, _, e)| *e).unwrap_or(false),
            }
        })
        .collect())
}
