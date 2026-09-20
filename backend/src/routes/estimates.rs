//! Estimators that use your own data: expenditure by energy balance, and
//! where the weight trend is heading. The arithmetic is in
//! `domain::estimate`; this module only gathers the window and reports what
//! it found, including when what it found is not enough.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::Router;
use chrono::{Duration, NaiveDate, Utc};
use serde::Deserialize;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::domain::estimate::{
    AdaptiveEstimate, ByDatePlan, Evidence, Projection, TdeeEstimate, Trend, TrendEvidence,
    TrendSummary, NEEDED, TREND_NEEDED,
};
use crate::error::{ApiError, ApiResult};
use crate::extract::Json;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tdee", get(tdee))
        .route("/projection", get(projection))
}

const DEFAULT_WINDOW_DAYS: i64 = 28;
const MIN_WINDOW_DAYS: i64 = 7;
const MAX_WINDOW_DAYS: i64 = 365;

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(default)]
pub struct TdeeQuery {
    /// The window, ending today. 7–365; defaults to 28.
    pub days: Option<i64>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(default)]
pub struct ProjectionQuery {
    /// The window the trend is fitted over, ending today. 7–365; defaults
    /// to 28.
    pub days: Option<i64>,
    /// A date to reach the target weight by. The response then carries the
    /// daily energy change that would get there.
    pub by: Option<NaiveDate>,
}

/// Everything in the window the estimators read.
struct Window {
    from: NaiveDate,
    to: NaiveDate,
    days: i64,
    /// Calories on each day marked complete, fast days as zero.
    complete_intakes: Vec<(NaiveDate, f64)>,
    weigh_ins: Vec<(NaiveDate, f64)>,
}

impl Window {
    fn trend(&self) -> Option<Trend> {
        Trend::fit(&self.weigh_ins)
    }

    fn evidence(&self) -> Evidence {
        let trend = self.trend();
        Evidence {
            complete_days: self.complete_intakes.len() as i64,
            weigh_ins: self.weigh_ins.len() as i64,
            span_days: trend.map(|t| t.span_days).unwrap_or(0),
        }
    }
}

fn window_days(days: Option<i64>) -> ApiResult<i64> {
    let days = days.unwrap_or(DEFAULT_WINDOW_DAYS);
    if !(MIN_WINDOW_DAYS..=MAX_WINDOW_DAYS).contains(&days) {
        return Err(ApiError::bad_request(format!(
            "days must be between {MIN_WINDOW_DAYS} and {MAX_WINDOW_DAYS}"
        )));
    }
    Ok(days)
}

async fn load_window(state: &AppState, user_id: Uuid, days: i64) -> ApiResult<Window> {
    let to = Utc::now().date_naive();
    let from = to - Duration::days(days - 1);

    // Only days the account has vouched for, with the same per-day sum the
    // summary shows for them. A complete day with no entries joins as zero:
    // that is what a fast day is, and it is the one zero worth counting.
    let complete_intakes: Vec<(NaiveDate, f64)> = sqlx::query_as(&format!(
        r#"
        WITH totals AS ({totals})
        SELECT dd.day, COALESCE(t.calories_kcal, 0) AS kcal
        FROM diary_days dd
        LEFT JOIN totals t ON t.logged_on = dd.day
        WHERE dd.user_id = $1 AND dd.complete AND dd.day BETWEEN $2 AND $3
        ORDER BY dd.day ASC
        "#,
        totals = super::diary::day_totals_sql()
    ))
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?;

    let weigh_ins: Vec<(NaiveDate, f64)> = sqlx::query_as(
        "SELECT recorded_on, weight_kg FROM weight_entries
         WHERE user_id = $1 AND recorded_on BETWEEN $2 AND $3
         ORDER BY recorded_on ASC",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?;

    Ok(Window {
        from,
        to,
        days,
        complete_intakes,
        weigh_ins,
    })
}

/// The adaptive estimate for a window, or why there is none yet. `goal` is
/// the profile's, for the budget the estimate prices.
fn estimate_for(
    window: &Window,
    goal: &str,
) -> (Evidence, Option<AdaptiveEstimate>, Option<String>) {
    let have = window.evidence();
    match (have.shortfall(), window.trend()) {
        (None, Some(trend)) => {
            let intakes: Vec<f64> = window.complete_intakes.iter().map(|d| d.1).collect();
            (
                have,
                Some(AdaptiveEstimate::from_window(&intakes, &trend, goal)),
                None,
            )
        }
        (Some(reason), _) => (have, None, Some(reason)),
        // Enough weigh-ins on paper but no fittable line: they are all on one
        // day, which the span check already catches. Kept for completeness
        // rather than left as an unreachable unwrap.
        (None, None) => (
            have,
            None,
            Some("Needs weigh-ins on at least two different days.".into()),
        ),
    }
}

#[utoipa::path(
    get, path = "/api/v1/estimates/tdee", tag = "estimates",
    security(("bearer" = [])),
    params(TdeeQuery),
    responses(
        (status = 200, description = "Expenditure by energy balance, or what is still needed", body = TdeeEstimate),
        (status = 400, body = crate::error::ErrorBody),
    )
)]
pub async fn tdee(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<TdeeQuery>,
) -> ApiResult<Json<TdeeEstimate>> {
    let days = window_days(q.days)?;
    let window = load_window(&state, user.id, days).await?;
    // The formula beside it, for comparison: the profile's guess against
    // the measurement, so a person can see how far off their activity level
    // was. Absent, with what it needs named, rather than guessed.
    let inputs = super::targets::energy_inputs(&state, user.id).await?;
    let formula = inputs.estimate(window.to);
    let (have, estimate, reason) = estimate_for(&window, &inputs.goal);

    Ok(Json(TdeeEstimate {
        ready: estimate.is_some(),
        reason,
        from: window.from,
        to: window.to,
        days: window.days,
        have,
        need: NEEDED,
        estimate,
        formula,
        formula_missing: inputs.missing(),
    }))
}

#[utoipa::path(
    get, path = "/api/v1/estimates/projection", tag = "estimates",
    security(("bearer" = [])),
    params(ProjectionQuery),
    responses(
        (status = 200, description = "When the trend meets the target weight, and what a chosen date would take", body = Projection),
        (status = 400, body = crate::error::ErrorBody),
    )
)]
pub async fn projection(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ProjectionQuery>,
) -> ApiResult<Json<Projection>> {
    let days = window_days(q.days)?;
    let window = load_window(&state, user.id, days).await?;

    let target: Option<(Option<f64>,)> =
        sqlx::query_as("SELECT target_weight_kg FROM users WHERE id = $1")
            .bind(user.id)
            .fetch_optional(&state.db)
            .await?;
    let target_weight_kg = target.ok_or(ApiError::NotFound("user"))?.0;

    let trend = window.trend();
    let have = TrendEvidence {
        weigh_ins: window.weigh_ins.len() as i64,
        span_days: trend.map(|t| t.span_days).unwrap_or(0),
    };
    let reason = have.shortfall();
    // Ready means a trend exists; a trend that is too short to trust is
    // not a trend, so the same thresholds apply as for the estimate.
    let trend = match (&reason, trend) {
        (None, Some(t)) => Some(t),
        _ => None,
    };

    let (reached_on, days_to_target, reached_reason) = match (trend, target_weight_kg) {
        (None, _) => (None, None, Some("not_ready")),
        (Some(_), None) => (None, None, Some("no_target_weight")),
        (Some(t), Some(target)) => {
            let (date, days, reach) = Projection::reach_date(&t, target);
            (date, days, reach.reason())
        }
    };

    // The deficit for a chosen date is priced off the adaptive expenditure
    // when there is one, else the profile formula, else stated as a change
    // with no intake beside it — never off a number that was made up.
    let (by, by_reason) = match q.by {
        None => (None, None),
        Some(_) if trend.is_none() => (None, Some("not_ready")),
        Some(_) if target_weight_kg.is_none() => (None, Some("no_target_weight")),
        Some(by) => {
            let t = trend.expect("checked above");
            let target = target_weight_kg.expect("checked above");
            let inputs = super::targets::energy_inputs(&state, user.id).await?;
            let basis = match estimate_for(&window, &inputs.goal).1 {
                Some(adaptive) => Some(("adaptive", adaptive.tdee_kcal)),
                None => inputs.estimate(window.to).map(|e| ("formula", e.tdee_kcal)),
            };
            match ByDatePlan::new(&t, target, by, basis) {
                Some(plan) => (Some(plan), None),
                None => (None, Some("date_not_after_as_of")),
            }
        }
    };

    Ok(Json(Projection {
        ready: trend.is_some(),
        reason,
        from: window.from,
        to: window.to,
        days: window.days,
        have,
        need: TREND_NEEDED,
        target_weight_kg,
        trend: trend.as_ref().map(TrendSummary::from_trend),
        reached_on,
        days_to_target,
        reached_reason,
        by,
        by_reason,
    }))
}
