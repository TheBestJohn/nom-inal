use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete as delete_route, get, post};
use axum::Router;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
use validator::Validate;

use crate::auth::CurrentUser;
use crate::domain::food::food_columns;
use crate::domain::food::{
    AddPortionRequest, BarcodeLookup, ExportQuery, ExternalFood, ExternalPortion,
    ExternalSearchQuery, ExternalSearchResponse, Food, FoodDetail, FoodExport, FoodExportBundle,
    FoodProvenance, FoodRevision, FoodSearchQuery, FoodVerification, RecentItem, RecentQuery,
    RecentRecipe, RevertRequest, UpsertFoodRequest, Verdict, VerificationStatus, VerifyRequest,
};
use crate::domain::nutrients::Nutrients;
use crate::error::{ApiError, ApiResult};
use crate::extract::Json;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/recent", get(recent))
        .route("/search/external", get(search_external))
        .route("/external/{source}/{source_id}", get(external_detail))
        .route("/barcode/{upc}", get(barcode))
        .route("/import", post(import))
        .route("/export", get(export))
        .route("/{id}", get(get_one).put(update).delete(delete))
        .route("/{id}/revisions", get(revisions))
        .route("/{id}/revert", post(revert))
        .route("/{id}/portions", post(add_portion))
        .route("/{id}/portions/{portion_id}", delete_route(remove_portion))
        .route(
            "/{id}/verify",
            get(verifications).post(verify).delete(unverify),
        )
}

use crate::domain::food::FOOD_COLUMNS as COLUMNS;

/// The same list for the two statements that alias `foods` as `f`.
const F_COLUMNS: &str = food_columns!("f");

/// Open a transaction that the history triggers can attribute.
///
/// The settings are transaction-local, so they have to be set inside an
/// explicit transaction: a bare statement on a pooled connection is its own
/// transaction and the value would be discarded before the trigger ran.
async fn authored_tx(
    state: &AppState,
    actor: Uuid,
    change_kind: &str,
    summary: Option<&str>,
) -> ApiResult<Transaction<'static, Postgres>> {
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "SELECT set_config('nom_inal.actor', $1, true),
                set_config('nom_inal.change_kind', $2, true),
                set_config('nom_inal.edit_summary', coalesce($3, ''), true)",
    )
    .bind(actor.to_string())
    .bind(change_kind)
    .bind(summary.map(str::trim).filter(|s| !s.is_empty()))
    .execute(&mut *tx)
    .await?;
    Ok(tx)
}

#[utoipa::path(
    get, path = "/api/v1/foods", tag = "foods",
    security(("bearer" = [])),
    params(
        ("q" = Option<String>, Query, description = "Free-text search over name and brand"),
        ("source" = Option<String>, Query, description = "custom | usda | off"),
        ("mine" = Option<bool>, Query, description = "Only foods you created"),
        ("limit" = Option<i64>, Query, description = "Page size, max 100"),
        ("offset" = Option<i64>, Query, description = "Page offset"),
    ),
    responses((status = 200, body = Vec<Food>))
)]
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<FoodSearchQuery>,
) -> ApiResult<Json<Vec<Food>>> {
    let limit = q.limit.clamp(1, 100);
    let offset = q.offset.max(0);
    let term = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty());

    // The food database is global: a food is a fact about a product, so making
    // every account re-import the same barcode would be pure duplication.
    // `created_by` still decides who may edit a row, and drives `mine`.
    let rows: Vec<Food> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM foods
         WHERE ($2::text IS NULL OR name ILIKE '%' || $2 || '%' OR brand ILIKE '%' || $2 || '%')
           AND ($3::text IS NULL OR source = $3)
           AND ($4::bool IS FALSE OR created_by = $1)
         ORDER BY
           -- exact prefix matches first, then alphabetically
           CASE WHEN $2::text IS NOT NULL AND name ILIKE $2 || '%' THEN 0 ELSE 1 END,
           name ASC
         LIMIT $5 OFFSET $6"
    ))
    .bind(user.id)
    .bind(term)
    .bind(q.source.as_deref())
    .bind(q.mine)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

#[utoipa::path(
    get, path = "/api/v1/foods/{id}", tag = "foods",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Food id")),
    responses((status = 200, body = FoodDetail), (status = 404, body = crate::error::ErrorBody))
)]
pub async fn get_one(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<FoodDetail>> {
    let food = load_food(&state, id).await?;
    Ok(Json(detail(&state, food, user.id).await?))
}

#[utoipa::path(
    post, path = "/api/v1/foods", tag = "foods",
    security(("bearer" = [])),
    request_body = UpsertFoodRequest,
    responses((status = 201, body = FoodDetail), (status = 400, body = crate::error::ErrorBody))
)]
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<UpsertFoodRequest>,
) -> ApiResult<(StatusCode, Json<FoodDetail>)> {
    body.validate()?;
    body.check_variant().map_err(ApiError::bad_request)?;
    // Whatever basis the caller typed in, what gets stored is per 100 g.
    let n = body.per_100g().map_err(ApiError::bad_request)?;

    let mut tx = authored_tx(&state, user.id, "create", body.edit_summary.as_deref()).await?;

    let row: Food = sqlx::query_as(&format!(
        "INSERT INTO foods (source, name, brand, upc, calories_kcal, protein_g, carbs_g, fat_g,
                            fiber_g, sugar_g, saturated_fat_g, sodium_mg, serving_size_g,
                            serving_label, variant_of, variant_label, nutrient_basis, created_by)
         VALUES ('custom', $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
         RETURNING {COLUMNS}"
    ))
    .bind(body.name.trim())
    .bind(body.brand.as_deref())
    .bind(body.upc.as_deref())
    .bind(n.calories_kcal)
    .bind(n.protein_g)
    .bind(n.carbs_g)
    .bind(n.fat_g)
    .bind(n.fiber_g)
    .bind(n.sugar_g)
    .bind(n.saturated_fat_g)
    .bind(n.sodium_mg)
    .bind(body.serving_size_g)
    .bind(body.serving_label.as_deref())
    .bind(body.variant_of)
    .bind(body.variant_label.as_deref().map(str::trim))
    .bind(body.nutrient_basis.as_str())
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(variant_error)?;

    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(detail(&state, row, user.id).await?),
    ))
}

#[utoipa::path(
    put, path = "/api/v1/foods/{id}", tag = "foods",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Food id")),
    request_body = UpsertFoodRequest,
    responses(
        (status = 200, body = FoodDetail),
        (status = 400, description = "Invalid variant relationship", body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody),
    )
)]
pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpsertFoodRequest>,
) -> ApiResult<Json<FoodDetail>> {
    body.validate()?;
    body.check_variant().map_err(ApiError::bad_request)?;
    let n = body.per_100g().map_err(ApiError::bad_request)?;

    // Anyone signed in may edit any food. That is the point of the model: a
    // food is a claim about the world, not the property of whoever typed it in
    // first, and the person holding the label in their hand is usually not the
    // original author. Openness is made safe by what surrounds it rather than
    // by locking the row — every edit is attributed, every prior state is
    // recoverable, and the edit drops the entry back to unverified until other
    // people agree with it.
    load_food(&state, id).await?;

    let mut tx = authored_tx(&state, user.id, "edit", body.edit_summary.as_deref()).await?;

    let row: Food = sqlx::query_as(&format!(
        "UPDATE foods SET
            name = $2, brand = $3, upc = $4, calories_kcal = $5, protein_g = $6,
            carbs_g = $7, fat_g = $8, fiber_g = $9, sugar_g = $10, saturated_fat_g = $11,
            sodium_mg = $12, serving_size_g = $13, serving_label = $14,
            variant_of = $15, variant_label = $16, nutrient_basis = $17
         WHERE id = $1
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(body.name.trim())
    .bind(body.brand.as_deref())
    .bind(body.upc.as_deref())
    .bind(n.calories_kcal)
    .bind(n.protein_g)
    .bind(n.carbs_g)
    .bind(n.fat_g)
    .bind(n.fiber_g)
    .bind(n.sugar_g)
    .bind(n.saturated_fat_g)
    .bind(n.sodium_mg)
    .bind(body.serving_size_g)
    .bind(body.serving_label.as_deref())
    .bind(body.variant_of)
    .bind(body.variant_label.as_deref().map(str::trim))
    .bind(body.nutrient_basis.as_str())
    .fetch_one(&mut *tx)
    .await
    .map_err(variant_error)?;

    tx.commit().await?;

    Ok(Json(detail(&state, row, user.id).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/foods/{id}", tag = "foods",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Food id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 400, description = "Still referenced by a recipe or diary entry", body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody),
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let existing = load_food(&state, id).await?;

    // Editing is open; deleting is not. Once other people have edited or
    // vouched for an entry it is shared work, and the right response to a bad
    // entry at that point is an edit or a revert, both of which keep the
    // record. Deletion stays available only for the case it is actually for:
    // taking back something you just added that nobody has touched.
    let untouched: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (
             SELECT 1 FROM food_revisions WHERE food_id = $1 AND revision > 1
         ) AND NOT EXISTS (
             SELECT 1 FROM food_verifications WHERE food_id = $1
         )",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let own = existing.created_by == Some(user.id);
    if !(user.is_admin || (own && untouched)) {
        return Err(ApiError::forbidden(if own {
            "this food has been edited or verified by others; edit or revert it instead of deleting it"
        } else {
            "only the author of an untouched food, or an administrator, can delete it"
        }));
    }

    // ON DELETE RESTRICT on recipe_items/diary_entries turns this into a
    // foreign-key violation, which the error mapper renders as a 400.
    sqlx::query("DELETE FROM foods WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db) if db.is_foreign_key_violation() => {
                ApiError::bad_request(
                    "this food is used by a recipe or diary entry and cannot be deleted",
                )
            }
            other => other.into(),
        })?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get, path = "/api/v1/foods/search/external", tag = "foods",
    security(("bearer" = [])),
    params(
        ("q" = String, Query, description = "Search terms"),
        ("limit" = Option<i64>, Query, description = "Max results per provider"),
    ),
    responses(
        (status = 200, description = "Candidates from USDA and Open Food Facts", body = ExternalSearchResponse),
        (status = 502, description = "A provider was unreachable", body = crate::error::ErrorBody),
    )
)]
pub async fn search_external(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(q): Query<ExternalSearchQuery>,
) -> ApiResult<Json<ExternalSearchResponse>> {
    let term = q.q.trim();
    if term.is_empty() {
        return Err(ApiError::bad_request("q must not be empty"));
    }

    // Query both providers concurrently: a slow OFF response should not add to
    // the USDA latency. A provider that fails degrades to a note in
    // `unavailable` rather than failing the whole search.
    let (usda_res, off_res) = tokio::join!(
        state.usda.search(term, q.limit),
        state.off.search(term, q.limit)
    );

    let mut results = Vec::new();
    let mut unavailable = Vec::new();

    match usda_res {
        Ok(Some(mut foods)) => results.append(&mut foods),
        Ok(None) => unavailable.push("usda: no USDA_API_KEY configured".to_string()),
        Err(e) => unavailable.push(format!("usda: {e}")),
    }
    match off_res {
        Ok(mut foods) => results.append(&mut foods),
        Err(e) => unavailable.push(format!("off: {e}")),
    }

    Ok(Json(ExternalSearchResponse {
        results,
        unavailable,
    }))
}

#[utoipa::path(
    get, path = "/api/v1/foods/barcode/{upc}", tag = "foods",
    security(("bearer" = [])),
    params(("upc" = String, Path, description = "UPC/EAN barcode digits")),
    responses(
        (status = 200, description = "Local match and/or an importable candidate", body = BarcodeLookup),
        (status = 404, description = "Barcode unknown locally and upstream", body = crate::error::ErrorBody),
    )
)]
pub async fn barcode(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(upc): Path<String>,
) -> ApiResult<Json<BarcodeLookup>> {
    let upc = upc.trim().to_string();
    if upc.is_empty() || !upc.chars().all(|c| c.is_ascii_digit()) {
        return Err(ApiError::bad_request("barcode must be digits only"));
    }

    // Local hit first — once anyone has imported a barcode there is no reason
    // to call out to the network again.
    let local: Option<Food> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM foods
         WHERE upc = $1
         ORDER BY created_by NULLS LAST
         LIMIT 1"
    ))
    .bind(&upc)
    .fetch_optional(&state.db)
    .await?;

    let external = match state.off.by_barcode(&upc).await {
        Ok(found) => found,
        // If we already have it locally, an upstream hiccup is not fatal.
        Err(e) if local.is_some() => {
            tracing::warn!(error = %e, "barcode lookup upstream failed, serving local match");
            None
        }
        Err(e) => return Err(e),
    };

    if local.is_none() && external.is_none() {
        return Err(ApiError::NotFound("barcode"));
    }

    let local = match local {
        Some(food) => Some(detail(&state, food, user.id).await?),
        None => None,
    };

    Ok(Json(BarcodeLookup {
        upc,
        local,
        external,
    }))
}

#[utoipa::path(
    post, path = "/api/v1/foods/import", tag = "foods",
    security(("bearer" = [])),
    request_body = ExternalFood,
    responses(
        (status = 200, description = "Imported, or returned the existing copy", body = FoodDetail),
        (status = 400, body = crate::error::ErrorBody),
    )
)]
pub async fn import(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<ExternalFood>,
) -> ApiResult<Json<FoodDetail>> {
    if !matches!(body.source.as_str(), "usda" | "off") {
        return Err(ApiError::bad_request("source must be 'usda' or 'off'"));
    }
    if body.source_id.trim().is_empty() {
        return Err(ApiError::bad_request("source_id is required"));
    }

    // Household portions come from USDA's detail record, not its search hits,
    // and the picker imports straight from a search hit. Rather than make
    // every client know to fetch the detail first — the UI, a script, an
    // assistant calling the tool — the import fetches it here when nothing
    // was sent. Best effort: a food without its portions is still the food,
    // so an upstream failure is logged and the import goes ahead.
    let portions: Vec<ExternalPortion> = if body.portions.is_empty() && body.source == "usda" {
        match state.usda.get(body.source_id.trim()).await {
            Ok(Some(detail)) => detail.portions,
            Ok(None) => Vec::new(),
            Err(e) => {
                tracing::warn!(error = %e, "USDA detail unavailable; importing without portions");
                Vec::new()
            }
        }
    } else {
        body.portions.clone()
    };

    let mut tx = authored_tx(&state, user.id, "import", None).await?;

    // Idempotent by (source, source_id): importing the same upstream food twice
    // refreshes it in place instead of creating a duplicate.
    let row: Option<Food> = sqlx::query_as(&format!(
        "INSERT INTO foods (source, source_id, name, brand, upc, calories_kcal, protein_g,
                            carbs_g, fat_g, fiber_g, sugar_g, saturated_fat_g, sodium_mg,
                            serving_size_g, serving_label)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
         ON CONFLICT (source, source_id) WHERE source_id IS NOT NULL DO UPDATE SET
            name = EXCLUDED.name,
            brand = EXCLUDED.brand,
            upc = EXCLUDED.upc,
            calories_kcal = EXCLUDED.calories_kcal,
            protein_g = EXCLUDED.protein_g,
            carbs_g = EXCLUDED.carbs_g,
            fat_g = EXCLUDED.fat_g,
            fiber_g = EXCLUDED.fiber_g,
            sugar_g = EXCLUDED.sugar_g,
            saturated_fat_g = EXCLUDED.saturated_fat_g,
            sodium_mg = EXCLUDED.sodium_mg,
            serving_size_g = EXCLUDED.serving_size_g,
            serving_label = EXCLUDED.serving_label,
            updated_at = now()
         -- Refresh only rows nobody has corrected. `revision = 1` means the
         -- entry is still exactly what the provider sent, so overwriting it
         -- loses nothing; past that, a person has deliberately disagreed with
         -- upstream and a re-import must not quietly undo them.
         WHERE foods.revision = 1
         RETURNING {COLUMNS}"
    ))
    .bind(&body.source)
    .bind(body.source_id.trim())
    .bind(body.name.trim())
    .bind(body.brand.as_deref())
    .bind(body.upc.as_deref())
    .bind(body.calories_kcal.max(0.0))
    .bind(body.protein_g.max(0.0))
    .bind(body.carbs_g.max(0.0))
    .bind(body.fat_g.max(0.0))
    .bind(body.fiber_g)
    .bind(body.sugar_g)
    .bind(body.saturated_fat_g)
    .bind(body.sodium_mg)
    .bind(if body.serving_size_g > 0.0 {
        body.serving_size_g
    } else {
        100.0
    })
    .bind(body.serving_label.as_deref())
    .fetch_optional(&mut *tx)
    .await?;

    // The `WHERE foods.revision = 1` guard above means a conflict with a
    // locally-edited row updates nothing and RETURNING yields no row. That is
    // success, not failure: the caller wanted this food to exist, and it does.
    let food_id: Uuid = match &row {
        Some(row) => row.id,
        None => {
            sqlx::query_scalar("SELECT id FROM foods WHERE source = $1 AND source_id = $2")
                .bind(&body.source)
                .bind(body.source_id.trim())
                .fetch_one(&mut *tx)
                .await?
        }
    };

    // Portions are refreshed on every import, edited food or not: they sit
    // outside the revision model, so there is no correction to undo. What a
    // person typed in is theirs — a provider's row never overwrites a label
    // somebody else already gave a weight to.
    if !portions.is_empty() {
        let labels: Vec<&str> = portions.iter().map(|p| p.label.as_str()).collect();
        let grams: Vec<f64> = portions.iter().map(|p| p.grams).collect();
        let orders: Vec<i32> = (0..portions.len() as i32).collect();
        sqlx::query(
            "INSERT INTO food_portions (food_id, label, grams, source, sort_order)
             SELECT $1, label, grams, $2, sort_order
             FROM UNNEST($3::text[], $4::float8[], $5::int[]) AS u(label, grams, sort_order)
             ON CONFLICT (food_id, label) DO UPDATE
                SET grams = EXCLUDED.grams, sort_order = EXCLUDED.sort_order
                WHERE food_portions.source <> 'user'",
        )
        .bind(food_id)
        .bind(&body.source)
        .bind(&labels)
        .bind(&grams)
        .bind(&orders)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // Read back after the commit so the portions just written are on it.
    let row = load_food(&state, food_id).await?;

    Ok(Json(detail(&state, row, user.id).await?))
}

#[utoipa::path(
    get, path = "/api/v1/foods/recent", tag = "foods",
    security(("bearer" = [])),
    params(("limit" = Option<i64>, Query, description = "How many, 1–50, default 12")),
    responses((status = 200, description = "What you logged most recently, most often first among ties", body = Vec<RecentItem>))
)]
pub async fn recent(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<RecentQuery>,
) -> ApiResult<Json<Vec<RecentItem>>> {
    let limit = q.limit.unwrap_or(12).clamp(1, 50);

    // One pass over the diary: each food or recipe once, with when it was
    // last logged, how often, and the amount from that last time. Recency
    // leads and frequency breaks the tie, because "what did I have
    // yesterday" is the question the picker is usually answering, and among
    // several things from the same day the habitual one belongs first.
    #[derive(sqlx::FromRow)]
    struct Row {
        food_id: Option<Uuid>,
        recipe_id: Option<Uuid>,
        times_logged: i64,
        last_logged_on: chrono::NaiveDate,
        last_quantity_g: Option<f64>,
        last_recipe_servings: Option<f64>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT d.food_id, d.recipe_id,
                count(*) AS times_logged,
                max(d.logged_on) AS last_logged_on,
                (array_agg(d.quantity_g ORDER BY d.logged_on DESC, d.created_at DESC))[1]
                    AS last_quantity_g,
                (array_agg(d.recipe_servings ORDER BY d.logged_on DESC, d.created_at DESC))[1]
                    AS last_recipe_servings
         FROM diary_entries d
         WHERE d.user_id = $1
         GROUP BY d.food_id, d.recipe_id
         ORDER BY last_logged_on DESC, times_logged DESC, max(d.created_at) DESC
         LIMIT $2",
    )
    .bind(user.id)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    let food_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.food_id).collect();
    let recipe_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.recipe_id).collect();

    let foods: Vec<Food> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM foods WHERE foods.id = ANY($1)"
    ))
    .bind(&food_ids)
    .fetch_all(&state.db)
    .await?;

    #[derive(sqlx::FromRow)]
    struct RecipeRow {
        id: Uuid,
        name: String,
        servings: f64,
        calories_kcal: f64,
        protein_g: f64,
        carbs_g: f64,
        fat_g: f64,
        fiber_g: f64,
        sugar_g: f64,
        saturated_fat_g: f64,
        sodium_mg: f64,
        untracked_count: i64,
    }

    // `recipe_totals` again, so the preview here says what the recipe page
    // and the diary say.
    let recipes: Vec<RecipeRow> = sqlx::query_as(
        "SELECT r.id, r.name, r.servings,
                t.calories_kcal, t.protein_g, t.carbs_g, t.fat_g, t.fiber_g, t.sugar_g,
                t.saturated_fat_g, t.sodium_mg, t.untracked_count
         FROM recipes r
         LEFT JOIN LATERAL (SELECT * FROM recipe_totals(r.id)) t ON TRUE
         WHERE r.id = ANY($1)",
    )
    .bind(&recipe_ids)
    .fetch_all(&state.db)
    .await?;

    let items = rows
        .into_iter()
        .filter_map(|r| {
            let food = r
                .food_id
                .and_then(|id| foods.iter().find(|f| f.id == id).cloned());
            let recipe = r.recipe_id.and_then(|id| {
                recipes.iter().find(|x| x.id == id).map(|x| RecentRecipe {
                    id: x.id,
                    name: x.name.clone(),
                    servings: x.servings,
                    per_serving: Nutrients {
                        calories_kcal: x.calories_kcal,
                        protein_g: x.protein_g,
                        carbs_g: x.carbs_g,
                        fat_g: x.fat_g,
                        fiber_g: x.fiber_g,
                        sugar_g: x.sugar_g,
                        saturated_fat_g: x.saturated_fat_g,
                        sodium_mg: x.sodium_mg,
                    }
                    .scaled(1.0 / x.servings)
                    .rounded(),
                    untracked_count: x.untracked_count,
                })
            });
            // Both foreign keys are ON DELETE RESTRICT, so a diary entry
            // whose target is gone cannot exist; the filter is belt and
            // braces rather than a case anyone should see.
            if food.is_none() && recipe.is_none() {
                return None;
            }
            Some(RecentItem {
                food,
                recipe,
                last_quantity_g: r.last_quantity_g,
                last_recipe_servings: r.last_recipe_servings,
                last_logged_on: r.last_logged_on,
                times_logged: r.times_logged,
            })
        })
        .collect();

    Ok(Json(items))
}

#[utoipa::path(
    post, path = "/api/v1/foods/{id}/portions", tag = "foods",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Food id")),
    request_body = AddPortionRequest,
    responses(
        (status = 201, description = "The food, portions included", body = FoodDetail),
        (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody),
        (status = 409, description = "This food already has a portion with that label", body = crate::error::ErrorBody),
    )
)]
pub async fn add_portion(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AddPortionRequest>,
) -> ApiResult<(StatusCode, Json<FoodDetail>)> {
    body.validate()?;
    let label = body.label.trim();
    if label.is_empty() {
        return Err(ApiError::bad_request("label must not be blank"));
    }
    load_food(&state, id).await?;

    // Open to anyone signed in, like the food itself. A portion is a measure
    // of the food, not a claim about its nutrition, so it is not versioned:
    // a wrong one is a wrong gram figure the person sees as they pick it,
    // and the fix is to remove it and add the right one.
    sqlx::query(
        "INSERT INTO food_portions (food_id, label, grams, source, sort_order)
         VALUES ($1, $2, $3, 'user',
                 (SELECT coalesce(max(sort_order), -1) + 1 FROM food_portions WHERE food_id = $1))",
    )
    .bind(id)
    .bind(label)
    .bind(body.grams)
    .execute(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db) if db.is_unique_violation() => {
            ApiError::Conflict("this food already has a portion with that label".into())
        }
        other => other.into(),
    })?;

    let food = load_food(&state, id).await?;
    Ok((
        StatusCode::CREATED,
        Json(detail(&state, food, user.id).await?),
    ))
}

#[utoipa::path(
    delete, path = "/api/v1/foods/{id}/portions/{portion_id}", tag = "foods",
    security(("bearer" = [])),
    params(
        ("id" = Uuid, Path, description = "Food id"),
        ("portion_id" = Uuid, Path, description = "Portion id"),
    ),
    responses(
        (status = 200, description = "The food, without that portion", body = FoodDetail),
        (status = 404, body = crate::error::ErrorBody),
    )
)]
pub async fn remove_portion(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, portion_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<FoodDetail>> {
    let result = sqlx::query("DELETE FROM food_portions WHERE id = $1 AND food_id = $2")
        .bind(portion_id)
        .bind(id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("portion"));
    }

    let food = load_food(&state, id).await?;
    Ok(Json(detail(&state, food, user.id).await?))
}

#[utoipa::path(
    get, path = "/api/v1/foods/external/{source}/{source_id}", tag = "foods",
    security(("bearer" = [])),
    params(
        ("source" = String, Path, description = "usda | off"),
        ("source_id" = String, Path, description = "FDC id, or barcode for Open Food Facts"),
    ),
    responses(
        (status = 200, description = "Full upstream record, ready to POST to /foods/import", body = ExternalFood),
        (status = 404, body = crate::error::ErrorBody),
    )
)]
pub async fn external_detail(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path((source, source_id)): Path<(String, String)>,
) -> ApiResult<Json<ExternalFood>> {
    // This id is interpolated into an upstream URL path, so anything other
    // than a plain identifier is rejected: a value containing `/` or `..`
    // could otherwise redirect the request to a different upstream endpoint.
    if source_id.is_empty()
        || source_id.len() > 64
        || !source_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::bad_request("source_id must be alphanumeric"));
    }

    // Search results from FDC are abridged; this fetches the full record so an
    // import carries every nutrient the source actually publishes.
    let found = match source.as_str() {
        "usda" => state
            .usda
            .get(&source_id)
            .await?
            .ok_or(ApiError::NotFound("food"))?,
        "off" => state
            .off
            .by_barcode(&source_id)
            .await?
            .ok_or(ApiError::NotFound("food"))?,
        _ => return Err(ApiError::bad_request("source must be 'usda' or 'off'")),
    };

    Ok(Json(found))
}

/// Load a food. Every food is visible to every account; only editing is scoped.
pub async fn load_food(state: &AppState, id: Uuid) -> ApiResult<Food> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM foods WHERE id = $1"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound("food"))
}

// ---------------------------------------------------------------------------
// Provenance: variants, history and verification
// ---------------------------------------------------------------------------

/// Turn the variant guard trigger's exception into a 400 with its own message.
///
/// The trigger raises `check_violation`, which the generic mapper would render
/// as "that conflicts with an existing record" — true but useless. The three
/// things it guards against are all things a person can fix, so they get told
/// what they were.
fn variant_error(e: sqlx::Error) -> ApiError {
    let sqlx::Error::Database(db) = &e else {
        return e.into();
    };

    // Two different failures, two different answers. The trigger's
    // check_violation means the relationship itself is impossible — a 400,
    // with the trigger's own wording, which already says what was wrong. The
    // unique index means the relationship is fine but taken, which is a 409.
    if db.is_unique_violation() && db.message().contains("variant") {
        return ApiError::Conflict("that food already has a variant with this label".into());
    }
    if db.message().contains("variant") {
        return ApiError::bad_request(db.message().to_string());
    }
    e.into()
}

#[derive(sqlx::FromRow)]
struct VerificationRow {
    user_id: Uuid,
    display_name: String,
    revision: i32,
    verdict: String,
    note: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct ProvenanceRow {
    confirmations: i64,
    disputes: i64,
    contributors: i64,
    last_change_kind: String,
    last_edited_at: DateTime<Utc>,
    last_edited_by: Option<Uuid>,
    last_edited_by_name: Option<String>,
    last_edit_summary: Option<String>,
    your_verdict: Option<String>,
    authored_current: bool,
    quorum: i64,
}

/// Read the editorial state of one food's current revision.
///
/// Everything is scoped to `f.revision`: the vote counts, your own vote and
/// whether you are allowed to vote at all. That single join condition is what
/// implements "an edit resets verification" — there is no reset step anywhere,
/// the old votes simply stop being selected.
async fn load_provenance(state: &AppState, food: &Food, viewer: Uuid) -> ApiResult<FoodProvenance> {
    let row: ProvenanceRow = sqlx::query_as(
        "SELECT
             coalesce(v.confirmations, 0) AS confirmations,
             coalesce(v.disputes, 0)      AS disputes,
             (SELECT count(DISTINCT edited_by) FROM food_revisions
               WHERE food_id = f.id AND edited_by IS NOT NULL) AS contributors,
             coalesce(r.change_kind, 'create')      AS last_change_kind,
             coalesce(r.created_at, f.updated_at)   AS last_edited_at,
             r.edited_by                            AS last_edited_by,
             eu.display_name                        AS last_edited_by_name,
             r.summary                              AS last_edit_summary,
             mv.verdict                             AS your_verdict,
             (r.edited_by IS NOT DISTINCT FROM $2)  AS authored_current,
             food_quorum()                          AS quorum
         FROM foods f
         LEFT JOIN LATERAL (
             SELECT count(*) FILTER (WHERE verdict = 'confirm') AS confirmations,
                    count(*) FILTER (WHERE verdict = 'dispute') AS disputes
             FROM food_verifications
             WHERE food_id = f.id AND revision = f.revision
         ) v ON TRUE
         LEFT JOIN food_revisions r ON r.food_id = f.id AND r.revision = f.revision
         LEFT JOIN users eu ON eu.id = r.edited_by
         LEFT JOIN food_verifications mv
                ON mv.food_id = f.id AND mv.revision = f.revision AND mv.user_id = $2
         WHERE f.id = $1",
    )
    .bind(food.id)
    .bind(viewer)
    .fetch_one(&state.db)
    .await?;

    Ok(FoodProvenance {
        revision: food.revision,
        status: VerificationStatus::evaluate(row.confirmations, row.disputes, row.quorum),
        confirmations: row.confirmations,
        disputes: row.disputes,
        quorum: row.quorum,
        verified_at: food.verified_at,
        contributors: row.contributors,
        last_change_kind: row.last_change_kind,
        last_edited_at: row.last_edited_at,
        last_edited_by: row.last_edited_by,
        last_edited_by_name: row.last_edited_by_name,
        last_edit_summary: row.last_edit_summary,
        your_verdict: row.your_verdict.as_deref().and_then(Verdict::parse),
        can_verify: !row.authored_current,
    })
}

/// A food plus everything that only the detail view needs: its family and its
/// editorial state.
pub async fn detail(state: &AppState, food: Food, viewer: Uuid) -> ApiResult<FoodDetail> {
    let provenance = load_provenance(state, &food, viewer).await?;

    // Exactly one of these runs: a food is either a parent with variants or a
    // variant with a parent, never both, because variants are one level deep.
    let (variants, parent) = match food.variant_of {
        None => {
            let variants: Vec<Food> = sqlx::query_as(&format!(
                "SELECT {COLUMNS} FROM foods WHERE variant_of = $1 ORDER BY lower(variant_label)"
            ))
            .bind(food.id)
            .fetch_all(&state.db)
            .await?;
            (variants, None)
        }
        Some(parent_id) => {
            let parent: Option<Food> =
                sqlx::query_as(&format!("SELECT {COLUMNS} FROM foods WHERE id = $1"))
                    .bind(parent_id)
                    .fetch_optional(&state.db)
                    .await?;
            (Vec::new(), parent)
        }
    };

    let per_serving = food.nutrients_for_grams(food.serving_size_g).rounded();

    Ok(FoodDetail {
        food,
        per_serving,
        provenance,
        variants,
        parent,
    })
}

#[utoipa::path(
    get, path = "/api/v1/foods/{id}/revisions", tag = "foods",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Food id")),
    responses((status = 200, body = Vec<FoodRevision>), (status = 404, body = crate::error::ErrorBody))
)]
pub async fn revisions(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<FoodRevision>>> {
    load_food(&state, id).await?;

    let mut rows: Vec<FoodRevision> = sqlx::query_as(
        "SELECT r.id, r.food_id, r.revision, r.change_kind, r.edited_by,
                u.display_name AS edited_by_name, r.summary, r.snapshot, r.created_at
         FROM food_revisions r
         LEFT JOIN users u ON u.id = r.edited_by
         WHERE r.food_id = $1
         ORDER BY r.revision DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    // Diff each revision against the one below it. Doing this server-side keeps
    // the two sides from disagreeing about what counts as a change — the same
    // snapshot the database compared to decide whether to bump the revision is
    // the one being compared here.
    for i in 0..rows.len() {
        let previous = rows.get(i + 1).map(|r| r.snapshot.clone());
        rows[i].changed_fields = match previous {
            Some(prev) => changed_fields(&prev, &rows[i].snapshot),
            // The first revision introduced everything, so listing every field
            // as "changed" would be noise.
            None => Vec::new(),
        };
    }

    Ok(Json(rows))
}

/// Field names whose value differs between two snapshots.
fn changed_fields(before: &serde_json::Value, after: &serde_json::Value) -> Vec<String> {
    let (Some(before), Some(after)) = (before.as_object(), after.as_object()) else {
        return Vec::new();
    };
    let mut out: Vec<String> = after
        .iter()
        .filter(|(k, v)| before.get(*k) != Some(*v))
        .map(|(k, _)| k.clone())
        // A field dropped from the schema between revisions also counts.
        .chain(
            before
                .keys()
                .filter(|k| !after.contains_key(*k))
                .map(|k| k.to_string()),
        )
        .collect();
    out.sort();
    out.dedup();
    out
}

#[utoipa::path(
    post, path = "/api/v1/foods/{id}/revert", tag = "foods",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Food id")),
    request_body = RevertRequest,
    responses(
        (status = 200, body = FoodDetail),
        (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody),
    )
)]
pub async fn revert(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RevertRequest>,
) -> ApiResult<Json<FoodDetail>> {
    body.validate()?;
    load_food(&state, id).await?;

    let snapshot: serde_json::Value = sqlx::query_scalar(
        "SELECT snapshot FROM food_revisions WHERE food_id = $1 AND revision = $2",
    )
    .bind(id)
    .bind(body.revision)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound("revision"))?;

    let summary = body.reason.clone().unwrap_or_else(|| {
        format!(
            "restored the numbers as they stood at revision {}",
            body.revision
        )
    });
    let mut tx = authored_tx(&state, user.id, "revert", Some(&summary)).await?;

    // Writing the snapshot back through `jsonb_populate_record` means the
    // restore covers whatever columns the snapshot actually holds, so a
    // revision taken before a nutrient existed still restores cleanly instead
    // of needing this statement updated every time the schema grows.
    let row: Food = sqlx::query_as(&format!(
        "UPDATE foods AS f SET
            name = s.name, brand = s.brand, upc = s.upc,
            calories_kcal = s.calories_kcal, protein_g = s.protein_g,
            carbs_g = s.carbs_g, fat_g = s.fat_g, fiber_g = s.fiber_g,
            sugar_g = s.sugar_g, saturated_fat_g = s.saturated_fat_g,
            sodium_mg = s.sodium_mg, serving_size_g = s.serving_size_g,
            serving_label = s.serving_label, variant_of = s.variant_of,
            variant_label = s.variant_label, nutrient_basis = s.nutrient_basis
         FROM (SELECT (jsonb_populate_record(NULL::foods, $2::jsonb)).*) AS s
         WHERE f.id = $1
         RETURNING {F_COLUMNS}"
    ))
    .bind(id)
    .bind(&snapshot)
    .fetch_one(&mut *tx)
    .await
    .map_err(variant_error)?;

    tx.commit().await?;

    Ok(Json(detail(&state, row, user.id).await?))
}

#[utoipa::path(
    get, path = "/api/v1/foods/{id}/verify", tag = "foods",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Food id")),
    responses((status = 200, body = Vec<FoodVerification>), (status = 404, body = crate::error::ErrorBody))
)]
pub async fn verifications(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<FoodVerification>>> {
    let food = load_food(&state, id).await?;

    let rows: Vec<VerificationRow> = sqlx::query_as(
        "SELECT v.user_id, u.display_name, v.revision, v.verdict, v.note, v.created_at
         FROM food_verifications v
         JOIN users u ON u.id = v.user_id
         WHERE v.food_id = $1
         ORDER BY v.revision DESC, v.created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| FoodVerification {
                user_id: r.user_id,
                display_name: r.display_name,
                revision: r.revision,
                verdict: Verdict::parse(&r.verdict).unwrap_or(Verdict::Confirm),
                note: r.note,
                created_at: r.created_at,
                current: r.revision == food.revision,
            })
            .collect(),
    ))
}

#[utoipa::path(
    post, path = "/api/v1/foods/{id}/verify", tag = "foods",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Food id")),
    request_body = VerifyRequest,
    responses(
        (status = 200, body = FoodDetail),
        (status = 403, description = "You wrote this revision", body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody),
    )
)]
pub async fn verify(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<VerifyRequest>,
) -> ApiResult<Json<FoodDetail>> {
    body.validate()?;
    let food = load_food(&state, id).await?;

    // Endorsing your own edit would make the quorum a count of one person
    // agreeing with themselves. Administrators are not exempt: the check is
    // about independence, and an admin confirming their own numbers is exactly
    // as uninformative as anyone else doing it.
    let authored: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM food_revisions
                        WHERE food_id = $1 AND revision = $2 AND edited_by = $3)",
    )
    .bind(id)
    .bind(food.revision)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    if authored {
        return Err(ApiError::forbidden(
            "you wrote this revision, so someone else has to vouch for it",
        ));
    }

    let mut tx = state.db.begin().await?;

    // The primary key is (food, revision, user), so changing your mind updates
    // your vote rather than stacking a second one.
    sqlx::query(
        "INSERT INTO food_verifications (food_id, revision, user_id, verdict, note)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (food_id, revision, user_id) DO UPDATE
            SET verdict = EXCLUDED.verdict, note = EXCLUDED.note, created_at = now()",
    )
    .bind(id)
    .bind(food.revision)
    .bind(user.id)
    .bind(body.verdict.as_str())
    .bind(
        body.note
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .execute(&mut *tx)
    .await?;

    let food = settle_verification(&mut tx, id).await?;
    tx.commit().await?;

    Ok(Json(detail(&state, food, user.id).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/foods/{id}/verify", tag = "foods",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Food id")),
    responses((status = 200, body = FoodDetail), (status = 404, body = crate::error::ErrorBody))
)]
pub async fn unverify(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<FoodDetail>> {
    let food = load_food(&state, id).await?;

    let mut tx = state.db.begin().await?;
    sqlx::query(
        "DELETE FROM food_verifications WHERE food_id = $1 AND revision = $2 AND user_id = $3",
    )
    .bind(id)
    .bind(food.revision)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;

    let food = settle_verification(&mut tx, id).await?;
    tx.commit().await?;

    Ok(Json(detail(&state, food, user.id).await?))
}

/// Recompute `verified_at` from the votes on the current revision.
///
/// `verified_at` is a cache of something derivable, kept as a column so that
/// listing and exporting verified foods is an index scan rather than an
/// aggregate over every vote. It is recomputed on the events that can change
/// it — a vote cast, a vote withdrawn, and the quorum itself moving — and
/// cleared by the edit trigger, so it cannot drift.
async fn settle_verification(tx: &mut Transaction<'static, Postgres>, id: Uuid) -> ApiResult<Food> {
    let row: Food = sqlx::query_as(&format!(
        "UPDATE foods AS f SET
            verified_at = CASE
              WHEN food_is_verified(f.id, f.revision) THEN coalesce(f.verified_at, now())
              ELSE NULL END,
            disputed_at = CASE
              WHEN food_is_disputed(f.id, f.revision) THEN coalesce(f.disputed_at, now())
              ELSE NULL END
         WHERE f.id = $1
         RETURNING {F_COLUMNS}"
    ))
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;

    Ok(row)
}

#[utoipa::path(
    get, path = "/api/v1/foods/export", tag = "foods",
    security(("bearer" = [])),
    params(("verified_only" = Option<bool>, Query, description = "Only foods that reached quorum")),
    responses((status = 200, body = FoodExportBundle))
)]
pub async fn export(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(q): Query<ExportQuery>,
) -> ApiResult<Json<FoodExportBundle>> {
    // The point of this endpoint is that the dataset can leave the instance:
    // dump it, commit it to a repository, review it in public, and seed another
    // deployment from it. So it deliberately carries no internal ids and no
    // per-user data — a variant points at its parent by the same natural key
    // any other instance would compute.
    let rows: Vec<FoodExport> = sqlx::query_as(
        "SELECT
             f.source, f.source_id, f.name, f.brand, f.upc,
             f.calories_kcal, f.protein_g, f.carbs_g, f.fat_g, f.fiber_g, f.sugar_g,
             f.saturated_fat_g, f.sodium_mg, f.serving_size_g, f.serving_label,
             f.nutrient_basis,
             CASE WHEN f.variant_of IS NULL THEN NULL
                  ELSE lower(btrim(p.name)) || '|' || lower(btrim(coalesce(p.brand, '')))
             END AS variant_of_key,
             f.variant_label,
             f.revision,
             coalesce(v.confirmations, 0) AS confirmations,
             coalesce(v.disputes, 0)      AS disputes,
             food_quorum()                AS quorum
         FROM foods f
         LEFT JOIN foods p ON p.id = f.variant_of
         LEFT JOIN LATERAL (
             SELECT count(*) FILTER (WHERE verdict = 'confirm') AS confirmations,
                    count(*) FILTER (WHERE verdict = 'dispute') AS disputes
             FROM food_verifications
             WHERE food_id = f.id AND revision = f.revision
         ) v ON TRUE
         WHERE ($1::bool IS FALSE OR f.verified_at IS NOT NULL)
         -- Parents before their variants, so an importer reading the file in
         -- order can always resolve `variant_of_key`.
         ORDER BY (f.variant_of IS NOT NULL), lower(f.name), lower(coalesce(f.brand, ''))",
    )
    .bind(q.verified_only)
    .fetch_all(&state.db)
    .await?;

    let foods: Vec<FoodExport> = rows
        .into_iter()
        .map(|mut f| {
            f.status = VerificationStatus::evaluate(f.confirmations, f.disputes, f.quorum);
            f
        })
        .collect();

    Ok(Json(FoodExportBundle {
        format: 1,
        generated_at: Utc::now(),
        count: foods.len(),
        foods,
    }))
}
