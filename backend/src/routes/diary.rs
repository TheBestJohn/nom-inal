use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::Router;
use chrono::{Duration, NaiveDate, Utc};
use serde::Deserialize;
use utoipa::IntoParams;
use uuid::Uuid;
use validator::Validate;

use crate::auth::CurrentUser;
use crate::domain::diary::{
    CreateDiaryEntryRequest, DailyTotal, DayCompletion, DiaryDay, DiaryEntry, DiaryRow,
    DiarySummary, MealGroup, PatchDiaryEntryRequest, SetDayCompleteRequest,
};
use crate::domain::nutrients::Nutrients;
use crate::domain::target::TargetProgress;
use crate::error::{ApiError, ApiResult};
use crate::extract::Json;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/day", get(day))
        .route("/day/{date}/complete", put(set_complete))
        .route("/summary", get(summary))
        .route("/{id}", get(get_one).patch(patch).delete(delete))
}

const MEALS: [&str; 4] = ["breakfast", "lunch", "dinner", "snack"];

/// Selects one diary entry plus the nutrient basis needed to scale it:
///   * food entries  -> the food's per-100g figures
///   * recipe entries-> the recipe's per-ONE-SERVING figures, aggregated in the
///     lateral join, so both cases reduce to a single multiply in Rust.
const ENTRY_SELECT: &str = r#"
    SELECT d.id, d.logged_on, d.meal, d.food_id, d.recipe_id, d.quantity_g, d.recipe_servings,
           d.created_at, d.updated_at,
           f.name AS food_name, f.brand AS food_brand, r.name AS recipe_name,
           COALESCE(f.calories_kcal,   rt.calories_kcal)   AS calories_kcal,
           COALESCE(f.protein_g,       rt.protein_g)       AS protein_g,
           COALESCE(f.carbs_g,         rt.carbs_g)         AS carbs_g,
           COALESCE(f.fat_g,           rt.fat_g)           AS fat_g,
           COALESCE(f.fiber_g,         rt.fiber_g)         AS fiber_g,
           COALESCE(f.sugar_g,         rt.sugar_g)         AS sugar_g,
           COALESCE(f.saturated_fat_g, rt.saturated_fat_g) AS saturated_fat_g,
           COALESCE(f.sodium_mg,       rt.sodium_mg)       AS sodium_mg
    FROM diary_entries d
    LEFT JOIN foods f   ON f.id = d.food_id
    LEFT JOIN recipes r ON r.id = d.recipe_id
    -- The same `recipe_totals` the recipe pages use, divided by the servings
    -- the recipe makes. This used to be its own copy of the sum, which was
    -- survivable while a recipe was a flat list of foods; once a recipe can
    -- contain another recipe, a second copy is a way for the diary and the
    -- recipe page to quietly report different numbers for the same meal.
    LEFT JOIN LATERAL (
        SELECT t.calories_kcal   / r.servings AS calories_kcal,
               t.protein_g       / r.servings AS protein_g,
               t.carbs_g         / r.servings AS carbs_g,
               t.fat_g           / r.servings AS fat_g,
               t.fiber_g         / r.servings AS fiber_g,
               t.sugar_g         / r.servings AS sugar_g,
               t.saturated_fat_g / r.servings AS saturated_fat_g,
               t.sodium_mg       / r.servings AS sodium_mg
        FROM recipe_totals(d.recipe_id) t
    ) rt ON d.recipe_id IS NOT NULL
"#;

/// Per-day totals for one account over a range: `$1` user, `$2` from, `$3`
/// to. One row per day that has entries, scaled and summed in SQL so a year
/// of history is a single round trip.
///
/// The summary reads this, and so does the adaptive expenditure estimate.
/// It is one query rather than two because the estimate's whole claim is
/// "your intake on these days was X" — X has to be the figure the summary
/// shows for those days, or the estimate is explaining a number nobody can
/// find.
pub(crate) fn day_totals_sql() -> String {
    format!(
        r#"
        WITH entries AS (
            {ENTRY_SELECT}
            WHERE d.user_id = $1 AND d.logged_on BETWEEN $2 AND $3
        ), scaled AS (
            SELECT logged_on,
                   COALESCE(quantity_g / 100.0, recipe_servings, 0) AS factor,
                   COALESCE(calories_kcal, 0)   AS calories_kcal,
                   COALESCE(protein_g, 0)       AS protein_g,
                   COALESCE(carbs_g, 0)         AS carbs_g,
                   COALESCE(fat_g, 0)           AS fat_g,
                   COALESCE(fiber_g, 0)         AS fiber_g,
                   COALESCE(sugar_g, 0)         AS sugar_g,
                   COALESCE(saturated_fat_g, 0) AS saturated_fat_g,
                   COALESCE(sodium_mg, 0)       AS sodium_mg
            FROM entries
        )
        SELECT logged_on,
               count(*)                          AS entry_count,
               sum(calories_kcal   * factor)     AS calories_kcal,
               sum(protein_g       * factor)     AS protein_g,
               sum(carbs_g         * factor)     AS carbs_g,
               sum(fat_g           * factor)     AS fat_g,
               sum(fiber_g         * factor)     AS fiber_g,
               sum(sugar_g         * factor)     AS sugar_g,
               sum(saturated_fat_g * factor)     AS saturated_fat_g,
               sum(sodium_mg       * factor)     AS sodium_mg
        FROM scaled
        GROUP BY logged_on
        "#
    )
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(default)]
pub struct ListQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub meal: Option<String>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(default)]
pub struct DayQuery {
    /// Defaults to today.
    pub date: Option<NaiveDate>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(default)]
pub struct SummaryQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

#[utoipa::path(
    get, path = "/api/v1/diary", tag = "diary",
    security(("bearer" = [])),
    params(ListQuery),
    responses((status = 200, body = Vec<DiaryEntry>))
)]
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<DiaryEntry>>> {
    let rows: Vec<DiaryRow> = sqlx::query_as(&format!(
        "{ENTRY_SELECT}
         WHERE d.user_id = $1
           AND ($2::date IS NULL OR d.logged_on >= $2)
           AND ($3::date IS NULL OR d.logged_on <= $3)
           AND ($4::text IS NULL OR d.meal = $4)
         ORDER BY d.logged_on DESC, d.created_at ASC"
    ))
    .bind(user.id)
    .bind(q.from)
    .bind(q.to)
    .bind(q.meal.as_deref())
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    get, path = "/api/v1/diary/day", tag = "diary",
    security(("bearer" = [])),
    params(DayQuery),
    responses((status = 200, description = "One day, grouped by meal, with targets", body = DiaryDay))
)]
pub async fn day(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<DayQuery>,
) -> ApiResult<Json<DiaryDay>> {
    let date = q.date.unwrap_or_else(|| Utc::now().date_naive());

    let rows: Vec<DiaryRow> = sqlx::query_as(&format!(
        "{ENTRY_SELECT}
         WHERE d.user_id = $1 AND d.logged_on = $2
         ORDER BY d.created_at ASC"
    ))
    .bind(user.id)
    .bind(date)
    .fetch_all(&state.db)
    .await?;

    let entries: Vec<DiaryEntry> = rows.into_iter().map(Into::into).collect();
    let total: Nutrients = entries.iter().map(|e| e.nutrients).sum();

    // Group into the canonical meal order, then append any custom meal names
    // the user has invented so nothing is silently dropped.
    let mut meal_names: Vec<String> = MEALS.iter().map(|m| m.to_string()).collect();
    for e in &entries {
        if !meal_names.contains(&e.meal) {
            meal_names.push(e.meal.clone());
        }
    }

    let meals: Vec<MealGroup> = meal_names
        .into_iter()
        .map(|meal| {
            let group: Vec<DiaryEntry> =
                entries.iter().filter(|e| e.meal == meal).cloned().collect();
            let total: Nutrients = group.iter().map(|e| e.nutrients).sum();
            MealGroup {
                meal,
                entries: group,
                total: total.rounded(),
            }
        })
        .collect();

    let total = total.rounded();

    // Each target is evaluated against the day's total in its own direction:
    // a budget reports what is left before the ceiling, a goal reports what is
    // still needed to reach the floor.
    let targets = super::targets::load_targets(&state, user.id)
        .await?
        .into_iter()
        .map(|t| TargetProgress::evaluate(t.nutrient, t.amount, t.kind, &total))
        .collect();

    // No row is the same as a row saying false: the flag defaults to "not
    // said", and a day nobody has vouched for is not complete.
    let complete: Option<(bool,)> =
        sqlx::query_as("SELECT complete FROM diary_days WHERE user_id = $1 AND day = $2")
            .bind(user.id)
            .bind(date)
            .fetch_optional(&state.db)
            .await?;

    Ok(Json(DiaryDay {
        date,
        meals,
        energy_share: total.energy_share(),
        total,
        targets,
        complete: complete.is_some_and(|c| c.0),
    }))
}

#[utoipa::path(
    put, path = "/api/v1/diary/day/{date}/complete", tag = "diary",
    security(("bearer" = [])),
    params(("date" = NaiveDate, Path, description = "The day, YYYY-MM-DD")),
    request_body = SetDayCompleteRequest,
    responses(
        (status = 200, description = "The flag as stored", body = DayCompletion),
        (status = 400, body = crate::error::ErrorBody),
    )
)]
pub async fn set_complete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(date): Path<NaiveDate>,
    Json(body): Json<SetDayCompleteRequest>,
) -> ApiResult<Json<DayCompletion>> {
    // A day that has not happened cannot have been fully logged. One day of
    // grace past UTC today, because "today" for someone east of Greenwich is
    // tomorrow here for part of every evening.
    if date > Utc::now().date_naive() + Duration::days(1) {
        return Err(ApiError::bad_request(
            "a day in the future cannot be marked as logged",
        ));
    }

    // Upsert: the flag is a fact about the day, and saying it twice is not
    // a conflict. A day with no entries can be complete — a fast day is one
    // of the more informative days an estimate can be given.
    let row: DayCompletion = sqlx::query_as(
        "INSERT INTO diary_days (user_id, day, complete)
         VALUES ($1, $2, $3)
         ON CONFLICT (user_id, day) DO UPDATE SET
            complete = EXCLUDED.complete,
            updated_at = now()
         RETURNING day AS date, complete, updated_at",
    )
    .bind(user.id)
    .bind(date)
    .bind(body.complete)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(row))
}

#[utoipa::path(
    get, path = "/api/v1/diary/summary", tag = "diary",
    security(("bearer" = [])),
    params(SummaryQuery),
    responses((status = 200, description = "Per-day totals over a range", body = DiarySummary))
)]
pub async fn summary(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SummaryQuery>,
) -> ApiResult<Json<DiarySummary>> {
    let to = q.to.unwrap_or_else(|| Utc::now().date_naive());
    let from = q.from.unwrap_or(to - Duration::days(29));
    if from > to {
        return Err(ApiError::bad_request("from must not be after to"));
    }

    #[derive(sqlx::FromRow)]
    struct Row {
        logged_on: NaiveDate,
        entry_count: i64,
        calories_kcal: f64,
        protein_g: f64,
        carbs_g: f64,
        fat_g: f64,
        fiber_g: f64,
        sugar_g: f64,
        saturated_fat_g: f64,
        sodium_mg: f64,
        complete: bool,
    }

    // The per-day totals, joined to the days the account has vouched for. A
    // day marked complete with nothing logged is listed with zero entries:
    // it is a fast day, which is data, and dropping it would be the one way
    // to make the flag invisible. A day marked and then unmarked, with
    // nothing logged, is just an absence and stays out.
    let rows: Vec<Row> = sqlx::query_as(&format!(
        r#"
        WITH totals AS ({totals}),
             marked AS (
                 SELECT day, complete FROM diary_days
                 WHERE user_id = $1 AND day BETWEEN $2 AND $3
             )
        SELECT COALESCE(t.logged_on, m.day)      AS logged_on,
               COALESCE(t.entry_count, 0)        AS entry_count,
               COALESCE(t.calories_kcal, 0)      AS calories_kcal,
               COALESCE(t.protein_g, 0)          AS protein_g,
               COALESCE(t.carbs_g, 0)            AS carbs_g,
               COALESCE(t.fat_g, 0)              AS fat_g,
               COALESCE(t.fiber_g, 0)            AS fiber_g,
               COALESCE(t.sugar_g, 0)            AS sugar_g,
               COALESCE(t.saturated_fat_g, 0)    AS saturated_fat_g,
               COALESCE(t.sodium_mg, 0)          AS sodium_mg,
               COALESCE(m.complete, false)       AS complete
        FROM totals t
        FULL OUTER JOIN marked m ON m.day = t.logged_on
        WHERE t.logged_on IS NOT NULL OR m.complete
        ORDER BY 1 ASC
        "#,
        totals = day_totals_sql()
    ))
    .bind(user.id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?;

    let days: Vec<DailyTotal> = rows
        .into_iter()
        .map(|r| DailyTotal {
            date: r.logged_on,
            entry_count: r.entry_count,
            total: Nutrients {
                calories_kcal: r.calories_kcal,
                protein_g: r.protein_g,
                carbs_g: r.carbs_g,
                fat_g: r.fat_g,
                fiber_g: r.fiber_g,
                sugar_g: r.sugar_g,
                saturated_fat_g: r.saturated_fat_g,
                sodium_mg: r.sodium_mg,
            }
            .rounded(),
            complete: r.complete,
        })
        .collect();

    let logged: Vec<&DailyTotal> = days.iter().filter(|d| d.entry_count > 0).collect();
    let logged_day_count = logged.len() as i64;
    let complete_day_count = days.iter().filter(|d| d.complete).count() as i64;
    // Average over logged days only: days with nothing recorded are missing
    // data, not zero-calorie days, and averaging them in would mislead. A
    // complete fast day is the one honest zero, and the adaptive estimate is
    // where it counts; this average keeps its long-standing meaning.
    let average = if logged_day_count == 0 {
        Nutrients::default()
    } else {
        logged
            .iter()
            .map(|d| d.total)
            .sum::<Nutrients>()
            .scaled(1.0 / logged_day_count as f64)
            .rounded()
    };

    Ok(Json(DiarySummary {
        from,
        to,
        days,
        energy_share: average.energy_share(),
        average,
        logged_day_count,
        complete_day_count,
    }))
}

#[utoipa::path(
    get, path = "/api/v1/diary/{id}", tag = "diary",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Diary entry id")),
    responses((status = 200, body = DiaryEntry), (status = 404, body = crate::error::ErrorBody))
)]
pub async fn get_one(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<DiaryEntry>> {
    Ok(Json(load_entry(&state, user.id, id).await?))
}

#[utoipa::path(
    post, path = "/api/v1/diary", tag = "diary",
    security(("bearer" = [])),
    request_body = CreateDiaryEntryRequest,
    responses((status = 201, body = DiaryEntry), (status = 400, body = crate::error::ErrorBody))
)]
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<CreateDiaryEntryRequest>,
) -> ApiResult<(StatusCode, Json<DiaryEntry>)> {
    body.validate()?;

    let date = body.logged_on.unwrap_or_else(|| Utc::now().date_naive());
    let meal = normalize_meal(body.meal.as_deref());

    // Reject the ambiguous combinations here rather than letting the database
    // CHECK constraint surface as an opaque error.
    let id: Uuid = match (body.food_id, body.recipe_id) {
        (Some(food_id), None) => {
            let grams = body.quantity_g.ok_or_else(|| {
                ApiError::bad_request("quantity_g is required when logging a food")
            })?;

            // Ownership/visibility check before insert.
            super::foods::load_food(&state, food_id).await?;

            sqlx::query_scalar(
                "INSERT INTO diary_entries (user_id, logged_on, meal, food_id, quantity_g)
                 VALUES ($1, $2, $3, $4, $5) RETURNING id",
            )
            .bind(user.id)
            .bind(date)
            .bind(&meal)
            .bind(food_id)
            .bind(grams)
            .fetch_one(&state.db)
            .await?
        }
        (None, Some(recipe_id)) => {
            let servings = body.recipe_servings.unwrap_or(1.0);

            let owned: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM recipes WHERE id = $1 AND user_id = $2")
                    .bind(recipe_id)
                    .bind(user.id)
                    .fetch_optional(&state.db)
                    .await?;
            if owned.is_none() {
                return Err(ApiError::NotFound("recipe"));
            }

            sqlx::query_scalar(
                "INSERT INTO diary_entries (user_id, logged_on, meal, recipe_id, recipe_servings)
                 VALUES ($1, $2, $3, $4, $5) RETURNING id",
            )
            .bind(user.id)
            .bind(date)
            .bind(&meal)
            .bind(recipe_id)
            .bind(servings)
            .fetch_one(&state.db)
            .await?
        }
        _ => {
            return Err(ApiError::bad_request(
                "provide exactly one of food_id or recipe_id",
            ))
        }
    };

    Ok((
        StatusCode::CREATED,
        Json(load_entry(&state, user.id, id).await?),
    ))
}

#[utoipa::path(
    patch, path = "/api/v1/diary/{id}", tag = "diary",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Diary entry id")),
    request_body = PatchDiaryEntryRequest,
    responses((status = 200, body = DiaryEntry), (status = 404, body = crate::error::ErrorBody))
)]
pub async fn patch(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchDiaryEntryRequest>,
) -> ApiResult<Json<DiaryEntry>> {
    body.validate()?;

    let meal = body.meal.as_deref().map(|m| normalize_meal(Some(m)));

    // Quantity fields are only applied to the matching entry kind, so a
    // `quantity_g` sent for a recipe entry cannot break the XOR constraint.
    let result = sqlx::query(
        "UPDATE diary_entries SET
            logged_on = COALESCE($3, logged_on),
            meal = COALESCE($4, meal),
            quantity_g = CASE WHEN food_id IS NOT NULL
                              THEN COALESCE($5, quantity_g) ELSE quantity_g END,
            recipe_servings = CASE WHEN recipe_id IS NOT NULL
                              THEN COALESCE($6, recipe_servings) ELSE recipe_servings END,
            updated_at = now()
         WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .bind(body.logged_on)
    .bind(meal)
    .bind(body.quantity_g)
    .bind(body.recipe_servings)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("diary entry"));
    }

    Ok(Json(load_entry(&state, user.id, id).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/diary/{id}", tag = "diary",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Diary entry id")),
    responses((status = 204, description = "Deleted"), (status = 404, body = crate::error::ErrorBody))
)]
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let result = sqlx::query("DELETE FROM diary_entries WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("diary entry"));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn load_entry(state: &AppState, user_id: Uuid, id: Uuid) -> ApiResult<DiaryEntry> {
    let row: DiaryRow = sqlx::query_as(&format!(
        "{ENTRY_SELECT} WHERE d.user_id = $1 AND d.id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound("diary entry"))?;

    Ok(row.into())
}

fn normalize_meal(meal: Option<&str>) -> String {
    meal.map(str::trim)
        .filter(|m| !m.is_empty())
        .map(|m| m.to_lowercase())
        .unwrap_or_else(|| "snack".to_string())
}
