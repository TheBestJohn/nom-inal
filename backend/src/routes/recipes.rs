use std::future::Future;
use std::pin::Pin;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use serde::Deserialize;
use sqlx::{Postgres, Transaction};
use utoipa::IntoParams;
use uuid::Uuid;
use validator::Validate;

use crate::auth::CurrentUser;
use crate::domain::food::food_key;
use crate::domain::ingredient::parse_ingredient;
use crate::domain::jsonld;
use crate::domain::nutrients::Nutrients;
use crate::domain::photo::photo_url;
use crate::domain::recipe::{
    DraftCandidate, DraftLine, FoodRef, FromMealRequest, ImportRecipeRequest, Recipe, RecipeDraft,
    RecipeExport, RecipeExportBundle, RecipeExportItem, RecipeItem, RecipeItemInput, RecipeItemRow,
    RecipeRow, RecipeSummary, UpsertRecipeRequest,
};
use crate::domain::slug;
use crate::error::{ApiError, ApiResult};
use crate::extract::Json;
use crate::services::fetch::fetch_public_page;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/import", post(import_from_url))
        .route("/from-meal", post(from_meal))
        .route("/{id}", get(get_one).put(update).delete(delete))
        .route("/{id}/export", get(export))
}

const RECIPE_COLUMNS: &str = "id, user_id, name, description, instructions, servings, is_public, \
                              slug, created_at, updated_at";

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(default)]
pub struct ListQuery {
    pub q: Option<String>,
    /// `mine` (default) lists only your recipes; `public` lists everyone's
    /// shared ones; `all` lists both.
    pub scope: Option<String>,
}

#[utoipa::path(
    get, path = "/api/v1/recipes", tag = "recipes",
    security(("bearer" = [])),
    params(ListQuery),
    responses((status = 200, body = Vec<RecipeSummary>))
)]
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<RecipeSummary>>> {
    let term = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let scope = match q.scope.as_deref() {
        Some("public") => "public",
        Some("all") => "all",
        _ => "mine",
    };

    // Aggregate the macros in SQL rather than fetching every ingredient row:
    // the list view only needs totals, and this keeps it to a single query
    // regardless of how many recipes or ingredients exist.
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        user_id: Uuid,
        slug: String,
        name: String,
        description: Option<String>,
        servings: f64,
        is_public: bool,
        author: Option<String>,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
        item_count: i64,
        total_weight_g: f64,
        calories_kcal: f64,
        protein_g: f64,
        carbs_g: f64,
        fat_g: f64,
        fiber_g: f64,
        sugar_g: f64,
        saturated_fat_g: f64,
        sodium_mg: f64,
        untracked_count: i64,
        cover_photo_id: Option<Uuid>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT r.id, r.user_id, r.slug, r.name, r.description, r.servings, r.is_public,
               u.display_name AS author, r.created_at, r.updated_at,
               -- The first photo uploaded is the cover. No ordering column to
               -- get wrong, and the first one is usually the finished dish.
               (SELECT p.id FROM photos p WHERE p.recipe_id = r.id
                 ORDER BY p.created_at ASC LIMIT 1) AS cover_photo_id,
               COALESCE(c.item_count, 0)      AS item_count,
               COALESCE(t.weight_g, 0)        AS total_weight_g,
               COALESCE(t.calories_kcal, 0)   AS calories_kcal,
               COALESCE(t.protein_g, 0)       AS protein_g,
               COALESCE(t.carbs_g, 0)         AS carbs_g,
               COALESCE(t.fat_g, 0)           AS fat_g,
               COALESCE(t.fiber_g, 0)         AS fiber_g,
               COALESCE(t.sugar_g, 0)         AS sugar_g,
               COALESCE(t.saturated_fat_g, 0) AS saturated_fat_g,
               COALESCE(t.sodium_mg, 0)       AS sodium_mg,
               COALESCE(t.untracked_count, 0) AS untracked_count
        FROM recipes r
        JOIN users u ON u.id = r.user_id
        -- `recipe_totals` resolves any nesting to the foods at the leaves.
        -- Using it here rather than a sum written out in place is what keeps
        -- this list, the detail view and the diary from disagreeing about the
        -- same recipe.
        LEFT JOIN LATERAL (SELECT * FROM recipe_totals(r.id)) t ON TRUE
        LEFT JOIN LATERAL (
            SELECT count(*) AS item_count FROM recipe_items ri WHERE ri.recipe_id = r.id
        ) c ON TRUE
        WHERE CASE $3::text
                WHEN 'public' THEN r.is_public
                WHEN 'all'    THEN (r.user_id = $1 OR r.is_public)
                ELSE r.user_id = $1
              END
          AND ($2::text IS NULL OR r.name ILIKE '%' || $2 || '%')
        -- Your own first, then everyone's shared ones, newest first within each.
        ORDER BY (r.user_id = $1) DESC, r.updated_at DESC
        "#,
    )
    .bind(user.id)
    .bind(term)
    .bind(scope)
    .fetch_all(&state.db)
    .await?;

    let summaries = rows
        .into_iter()
        .map(|r| {
            let total = Nutrients {
                calories_kcal: r.calories_kcal,
                protein_g: r.protein_g,
                carbs_g: r.carbs_g,
                fat_g: r.fat_g,
                fiber_g: r.fiber_g,
                sugar_g: r.sugar_g,
                saturated_fat_g: r.saturated_fat_g,
                sodium_mg: r.sodium_mg,
            };
            RecipeSummary {
                id: r.id,
                slug: r.slug,
                is_owner: r.user_id == user.id,
                author: (r.user_id != user.id).then_some(r.author).flatten(),
                is_public: r.is_public,
                name: r.name,
                description: r.description,
                servings: r.servings,
                total_weight_g: round2(r.total_weight_g),
                item_count: r.item_count,
                untracked_count: r.untracked_count,
                cover_photo_url: r.cover_photo_id.map(photo_url),
                per_serving: total.scaled(1.0 / r.servings).rounded(),
                created_at: r.created_at,
                updated_at: r.updated_at,
            }
        })
        .collect();

    Ok(Json(summaries))
}

#[utoipa::path(
    get, path = "/api/v1/recipes/{id}", tag = "recipes",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Recipe id")),
    responses((status = 200, body = Recipe), (status = 404, body = crate::error::ErrorBody))
)]
pub async fn get_one(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Recipe>> {
    Ok(Json(load_recipe(&state, Some(user.id), id).await?))
}

#[utoipa::path(
    post, path = "/api/v1/recipes", tag = "recipes",
    security(("bearer" = [])),
    request_body = UpsertRecipeRequest,
    responses((status = 201, body = Recipe), (status = 400, body = crate::error::ErrorBody))
)]
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<UpsertRecipeRequest>,
) -> ApiResult<(StatusCode, Json<Recipe>)> {
    body.validate()?;
    // Header and ingredients are written in one transaction: a recipe that
    // exists with half its ingredients would silently misreport its macros.
    let mut tx = state.db.begin().await?;
    let id = insert_recipe(&mut tx, user.id, &body).await?;
    tx.commit().await?;

    let full = load_recipe(&state, Some(user.id), id).await?;
    Ok((StatusCode::CREATED, Json(full)))
}

/// Write a new recipe, header and ingredients, inside the caller's
/// transaction. The one insert path: a recipe typed into the form, one built
/// from a logged meal and one restored by the account import all come
/// through here, so each goes through exactly the same checks.
pub async fn insert_recipe(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    body: &UpsertRecipeRequest,
) -> ApiResult<Uuid> {
    // The id is minted here rather than by the column default, because the
    // slug may need it: a name with nothing sluggable in it — only emoji,
    // only punctuation — falls back to a short form of the id.
    let id = Uuid::new_v4();
    let slug = choose_slug(tx, id, &body.name).await?;

    let recipe: RecipeRow = sqlx::query_as(&format!(
        "INSERT INTO recipes (id, user_id, name, description, instructions, servings, is_public, slug)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING {RECIPE_COLUMNS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(body.name.trim())
    .bind(body.description.as_deref())
    .bind(body.instructions.as_deref())
    .bind(body.servings)
    .bind(body.is_public)
    .bind(&slug)
    .fetch_one(&mut **tx)
    .await?;

    record_slug(tx, recipe.id, &slug).await?;
    insert_items(tx, recipe.id, user_id, body).await?;
    Ok(recipe.id)
}

/// The slug `id` should have for `name`.
///
/// Called from create and from rename alike, which is the whole point: one
/// rule in one place. It is idempotent — asked twice for the same recipe and
/// the same name it answers the same, because a slug this recipe already
/// holds is not a collision with itself.
///
/// Choosing and recording are two steps, [`record_slug`] being the second,
/// because the history row references the recipe: the recipe has to exist
/// before its slug can be written down. Both run in the caller's
/// transaction, so a write that is rolled back leaves no slug reserved.
async fn choose_slug(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    name: &str,
) -> ApiResult<String> {
    let base = match slug::slugify(name) {
        empty if empty.is_empty() => slug::fallback_slug(id),
        base => base,
    };

    // Every slug that could stand in the way: the base and anything suffixed
    // from it, taken from the history rather than from the recipes currently
    // holding one, so a slug freed by a rename is still never handed out
    // again. This recipe's own past slugs are excluded, so renaming a recipe
    // back to what it was called returns the address people already have.
    let held: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM recipe_slug_history
         WHERE (slug = $1 OR slug LIKE $1 || '-%') AND recipe_id <> $2",
    )
    .bind(&base)
    .bind(id)
    .fetch_all(&mut **tx)
    .await?;

    Ok(slug::free_slug(&base, &held.into_iter().collect()))
}

/// Write the slug into the history, where it stays for good.
///
/// A slug this recipe has held before is already recorded, and recording it
/// again is not an error — it is the same fact. The `recipes.slug` foreign
/// key into this table is deferred, which is what lets the recipe be written
/// first and its slug a moment later, inside the one transaction.
async fn record_slug(tx: &mut Transaction<'_, Postgres>, id: Uuid, slug: &str) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO recipe_slug_history (slug, recipe_id) VALUES ($1, $2)
         ON CONFLICT (slug) DO NOTHING",
    )
    .bind(slug)
    .bind(id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Replace an existing recipe of `user_id`'s, header and ingredients, inside
/// the caller's transaction. False when there is no such recipe of theirs.
pub async fn replace_recipe(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: Uuid,
    body: &UpsertRecipeRequest,
) -> ApiResult<bool> {
    // Renaming re-slugs. The old slug stays in the history pointing here, so a
    // link somebody has already sent still finds the recipe; the page then
    // says, once and permanently, where it lives now.
    let slug = choose_slug(tx, id, &body.name).await?;

    let updated: Option<RecipeRow> = sqlx::query_as(&format!(
        "UPDATE recipes SET name = $3, description = $4, instructions = $5, servings = $6,
                            is_public = $7, slug = $8, updated_at = now()
         WHERE id = $1 AND user_id = $2
         RETURNING {RECIPE_COLUMNS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(body.name.trim())
    .bind(body.description.as_deref())
    .bind(body.instructions.as_deref())
    .bind(body.servings)
    .bind(body.is_public)
    .bind(&slug)
    .fetch_optional(&mut **tx)
    .await?;

    if updated.is_none() {
        return Ok(false);
    }
    record_slug(tx, id, &slug).await?;

    // Ingredient list is replaced wholesale — simpler and less error-prone than
    // diffing, and the list is small enough that the rewrite cost is irrelevant.
    sqlx::query("DELETE FROM recipe_items WHERE recipe_id = $1")
        .bind(id)
        .execute(&mut **tx)
        .await?;

    insert_items(tx, id, user_id, body).await?;
    Ok(true)
}

#[utoipa::path(
    post, path = "/api/v1/recipes/from-meal", tag = "recipes",
    security(("bearer" = [])),
    request_body = FromMealRequest,
    responses(
        (status = 201, description = "The new recipe, built from that meal's entries", body = Recipe),
        (status = 400, description = "Nothing logged for that meal", body = crate::error::ErrorBody),
    )
)]
pub async fn from_meal(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<FromMealRequest>,
) -> ApiResult<(StatusCode, Json<Recipe>)> {
    body.validate()?;
    let meal = body.meal.trim().to_lowercase();
    if meal.is_empty() {
        return Err(ApiError::bad_request("meal is required"));
    }

    // The entries as logged, in the order they were logged. A food entry is
    // grams of a food and a recipe entry is servings of a recipe, which is
    // exactly the shape an ingredient takes — so the meal maps onto the
    // request the ordinary create path takes, and goes through it. A logged
    // recipe stays a sub-recipe rather than being unpacked into its foods:
    // unpacking would freeze its ingredients, and the point of linking is
    // that correcting the base recipe corrects what was built on it.
    #[derive(sqlx::FromRow)]
    struct Entry {
        food_id: Option<Uuid>,
        recipe_id: Option<Uuid>,
        quantity_g: Option<f64>,
        recipe_servings: Option<f64>,
    }
    let entries: Vec<Entry> = sqlx::query_as(
        "SELECT food_id, recipe_id, quantity_g, recipe_servings
         FROM diary_entries
         WHERE user_id = $1 AND logged_on = $2 AND meal = $3
         ORDER BY created_at ASC",
    )
    .bind(user.id)
    .bind(body.date)
    .bind(&meal)
    .fetch_all(&state.db)
    .await?;

    if entries.is_empty() {
        return Err(ApiError::bad_request(format!(
            "nothing logged for {meal} on {}",
            body.date
        )));
    }

    let request = UpsertRecipeRequest {
        name: body.name.clone(),
        description: None,
        instructions: None,
        servings: body.servings.unwrap_or(1.0),
        is_public: body.is_public,
        items: entries
            .into_iter()
            .map(|e| RecipeItemInput {
                food_id: e.food_id,
                sub_recipe_id: e.recipe_id,
                label: None,
                quantity_g: e.quantity_g,
                servings: e.recipe_servings,
                note: None,
            })
            .collect(),
    };
    request.validate()?;

    let mut tx = state.db.begin().await?;
    let id = insert_recipe(&mut tx, user.id, &request).await?;
    tx.commit().await?;
    let full = load_recipe(&state, Some(user.id), id).await?;
    Ok((StatusCode::CREATED, Json(full)))
}

#[utoipa::path(
    put, path = "/api/v1/recipes/{id}", tag = "recipes",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Recipe id")),
    request_body = UpsertRecipeRequest,
    responses((status = 200, body = Recipe), (status = 404, body = crate::error::ErrorBody))
)]
pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpsertRecipeRequest>,
) -> ApiResult<Json<Recipe>> {
    body.validate()?;

    let mut tx = state.db.begin().await?;
    if !replace_recipe(&mut tx, user.id, id, &body).await? {
        return Err(ApiError::NotFound("recipe"));
    }
    tx.commit().await?;

    Ok(Json(load_recipe(&state, Some(user.id), id).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/recipes/{id}", tag = "recipes",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Recipe id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 400, description = "Still used by a diary entry or another recipe", body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody),
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    // Asked before attempting the delete, because the foreign key cannot say
    // *which* reference blocked it and "logged in your diary" is a confusing
    // answer when the real reason is another recipe — possibly someone else's.
    let used_by: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT recipe_id) FROM recipe_items WHERE sub_recipe_id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    if used_by > 0 {
        return Err(ApiError::bad_request(format!(
            "this recipe is an ingredient of {used_by} other recipe{} and cannot be deleted",
            if used_by == 1 { "" } else { "s" }
        )));
    }

    // The photo rows go in the same transaction as the recipe, and their files
    // only after it commits: the delete below can still be refused by the
    // diary's foreign key, and a refused delete must not have cost the photos.
    let mut tx = state.db.begin().await?;
    let photo_paths = super::photos::delete_for_recipe(&mut tx, user.id, id).await?;

    let result = sqlx::query("DELETE FROM recipes WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db) if db.is_foreign_key_violation() => {
                ApiError::bad_request("this recipe is logged in your diary and cannot be deleted")
            }
            other => other.into(),
        })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("recipe"));
    }
    tx.commit().await?;
    super::photos::remove_files(&state, photo_paths).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn insert_items(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    recipe_id: Uuid,
    user_id: Uuid,
    body: &UpsertRecipeRequest,
) -> ApiResult<()> {
    for item in &body.items {
        item.check_target().map_err(ApiError::bad_request)?;
    }

    let food_ids: Vec<Option<Uuid>> = body.items.iter().map(|i| i.food_id).collect();
    let sub_ids: Vec<Option<Uuid>> = body.items.iter().map(|i| i.sub_recipe_id).collect();
    let labels: Vec<Option<String>> = body
        .items
        .iter()
        .map(|i| {
            i.label
                .as_deref()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
        })
        .collect();
    let quantities: Vec<Option<f64>> = body.items.iter().map(|i| i.quantity_g).collect();
    let servings: Vec<Option<f64>> = body.items.iter().map(|i| i.servings).collect();
    let notes: Vec<Option<String>> = body.items.iter().map(|i| i.note.clone()).collect();
    let orders: Vec<i32> = (0..body.items.len() as i32).collect();

    // Validate every ingredient in one query rather than one per item. Checking
    // explicitly (instead of relying on the foreign key) is what lets an unknown
    // id come back as a clear 400 naming the id rather than an opaque
    // constraint error. Foods are global, so there is no visibility test.
    let wanted_foods: Vec<Uuid> = food_ids.iter().flatten().copied().collect();
    let known: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM foods WHERE id = ANY($1)")
        .bind(&wanted_foods)
        .fetch_all(&mut **tx)
        .await?;
    if let Some(missing) = wanted_foods.iter().find(|id| !known.contains(id)) {
        return Err(ApiError::bad_request(format!("unknown food id {missing}")));
    }

    // Sub-recipes do need a visibility test: recipes are private unless shared,
    // so the set you may build on is your own plus everyone's public ones. A
    // recipe you cannot read is reported as unknown rather than forbidden,
    // which is also what a plain GET would tell you about it.
    let wanted_subs: Vec<Uuid> = sub_ids.iter().flatten().copied().collect();
    let visible: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM recipes WHERE id = ANY($1) AND (user_id = $2 OR is_public)",
    )
    .bind(&wanted_subs)
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?;
    if let Some(missing) = wanted_subs.iter().find(|id| !visible.contains(id)) {
        return Err(ApiError::bad_request(format!(
            "unknown recipe id {missing}"
        )));
    }

    // One INSERT for the whole ingredient list: UNNEST turns the parallel
    // arrays into rows, so a 20-ingredient recipe is a single round trip.
    sqlx::query(
        "INSERT INTO recipe_items
             (recipe_id, food_id, sub_recipe_id, label, quantity_g, servings, note, sort_order)
         SELECT $1, * FROM UNNEST($2::uuid[], $3::uuid[], $4::text[], $5::float8[], $6::float8[],
                                  $7::text[], $8::int[])",
    )
    .bind(recipe_id)
    .bind(&food_ids)
    .bind(&sub_ids)
    .bind(&labels)
    .bind(&quantities)
    .bind(&servings)
    .bind(&notes)
    .bind(&orders)
    .execute(&mut **tx)
    .await
    .map_err(graph_error)?;

    Ok(())
}

/// Surface the cycle and depth guards with the wording the trigger raised.
///
/// Both come back as `check_violation`, which the generic mapper renders as
/// "that conflicts with an existing record" — true, and no help at all to
/// someone who has just tried to put a recipe inside itself.
fn graph_error(e: sqlx::Error) -> ApiError {
    match &e {
        sqlx::Error::Database(db)
            if db.message().contains("contain itself") || db.message().contains("nested") =>
        {
            ApiError::bad_request(db.message().to_string())
        }
        _ => e.into(),
    }
}

/// Assemble a recipe for `viewer`: its items with their contributions, the
/// totals, and who wrote it.
///
/// Visible when you own it or its author shared it. `viewer` is `None` for a
/// reader who has not signed in — the public recipe page — and then only a
/// shared recipe is found, by the same WHERE clause: `r.user_id = NULL` is
/// unknown, so `is_public` alone decides. One assembly path for both, so the
/// public page can never show a different recipe from the signed-in one.
/// Editing stays owner-only and is checked separately by the handlers that
/// write.
pub async fn load_recipe(state: &AppState, viewer: Option<Uuid>, id: Uuid) -> ApiResult<Recipe> {
    // The author's name is only needed here, so it rides along on a local row
    // type rather than widening RecipeRow, which the write paths also use and
    // which never joins `users`.
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        user_id: Uuid,
        slug: String,
        name: String,
        description: Option<String>,
        instructions: Option<String>,
        servings: f64,
        is_public: bool,
        author: String,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
    }

    let recipe: Row = sqlx::query_as(
        "SELECT r.id, r.user_id, r.slug, r.name, r.description, r.instructions, r.servings,
                r.is_public, u.display_name AS author, r.created_at, r.updated_at
         FROM recipes r JOIN users u ON u.id = r.user_id
         WHERE r.id = $1 AND (r.user_id = $2 OR r.is_public)",
    )
    .bind(id)
    .bind(viewer)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound("recipe"))?;

    let author = recipe.author.clone();

    // Each row arrives already scaled: a food by its grams, a sub-recipe by the
    // servings taken of it. `recipe_totals` does the nested part, so this query
    // stays a flat join no matter how deep the recipe goes.
    let item_rows: Vec<RecipeItemRow> = sqlx::query_as(
        r#"
        SELECT ri.id, ri.food_id, ri.sub_recipe_id, ri.label, ri.quantity_g, ri.servings,
               ri.note, ri.sort_order,
               COALESCE(f.name, sub.name, ri.label) AS name,
               f.brand AS brand,
               f.variant_label AS variant_label,
               COALESCE(f.calories_kcal * ri.quantity_g / 100.0,
                        rt.calories_kcal * ri.servings / sub.servings, 0) AS calories_kcal,
               COALESCE(f.protein_g * ri.quantity_g / 100.0,
                        rt.protein_g * ri.servings / sub.servings, 0) AS protein_g,
               COALESCE(f.carbs_g * ri.quantity_g / 100.0,
                        rt.carbs_g * ri.servings / sub.servings, 0) AS carbs_g,
               COALESCE(f.fat_g * ri.quantity_g / 100.0,
                        rt.fat_g * ri.servings / sub.servings, 0) AS fat_g,
               COALESCE(f.fiber_g * ri.quantity_g / 100.0,
                        rt.fiber_g * ri.servings / sub.servings, 0) AS fiber_g,
               COALESCE(f.sugar_g * ri.quantity_g / 100.0,
                        rt.sugar_g * ri.servings / sub.servings, 0) AS sugar_g,
               COALESCE(f.saturated_fat_g * ri.quantity_g / 100.0,
                        rt.saturated_fat_g * ri.servings / sub.servings, 0) AS saturated_fat_g,
               COALESCE(f.sodium_mg * ri.quantity_g / 100.0,
                        rt.sodium_mg * ri.servings / sub.servings, 0) AS sodium_mg,
               COALESCE(ri.quantity_g, rt.weight_g * ri.servings / sub.servings, 0) AS weight_g
        FROM recipe_items ri
        LEFT JOIN foods f   ON f.id = ri.food_id
        LEFT JOIN recipes sub ON sub.id = ri.sub_recipe_id
        LEFT JOIN LATERAL (SELECT * FROM recipe_totals(ri.sub_recipe_id)) rt
               ON ri.sub_recipe_id IS NOT NULL
        WHERE ri.recipe_id = $1
        ORDER BY ri.sort_order ASC
        "#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    // Asked of the same function the list and the diary use, rather than summed
    // from the rows above. The two agree to the last float, and when they ever
    // stop agreeing it will be because the function changed — one place to look.
    let totals: TotalsRow = sqlx::query_as("SELECT * FROM recipe_totals($1)")
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    let total = totals.nutrients();

    let items = item_rows
        .into_iter()
        .map(|r| RecipeItem {
            nutrients: r.nutrients().rounded(),
            id: r.id,
            food_id: r.food_id,
            sub_recipe_id: r.sub_recipe_id,
            label: r.label,
            name: r.name,
            brand: r.brand,
            variant_label: r.variant_label,
            quantity_g: r.quantity_g,
            servings: r.servings,
            weight_g: round2(r.weight_g),
            note: r.note,
            sort_order: r.sort_order,
        })
        .collect();

    let is_owner = viewer == Some(recipe.user_id);

    Ok(Recipe {
        id: recipe.id,
        slug: recipe.slug,
        is_owner,
        is_public: recipe.is_public,
        author: (!is_owner).then_some(author),
        name: recipe.name,
        description: recipe.description,
        instructions: recipe.instructions,
        servings: recipe.servings,
        total_weight_g: round2(totals.weight_g),
        untracked_count: totals.untracked_count,
        items,
        total: total.rounded(),
        per_serving: total.scaled(1.0 / recipe.servings).rounded(),
        created_at: recipe.created_at,
        updated_at: recipe.updated_at,
    })
}

/// The row `recipe_totals` returns, so the three callers that need the whole
/// recipe in one go share a type as well as a query.
#[derive(Debug, sqlx::FromRow)]
struct TotalsRow {
    calories_kcal: f64,
    protein_g: f64,
    carbs_g: f64,
    fat_g: f64,
    fiber_g: f64,
    sugar_g: f64,
    saturated_fat_g: f64,
    sodium_mg: f64,
    weight_g: f64,
    untracked_count: i64,
}

impl TotalsRow {
    fn nutrients(&self) -> Nutrients {
        Nutrients {
            calories_kcal: self.calories_kcal,
            protein_g: self.protein_g,
            carbs_g: self.carbs_g,
            fat_g: self.fat_g,
            fiber_g: self.fiber_g,
            sugar_g: self.sugar_g,
            saturated_fat_g: self.saturated_fat_g,
            sodium_mg: self.sodium_mg,
        }
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(default)]
pub struct ExportQuery {
    /// `json` (default): the seed-repository shape, no internal ids, foods by
    /// natural key, sub-recipes inlined. `markdown`: a recipe card.
    pub format: Option<String>,
}

#[utoipa::path(
    get, path = "/api/v1/recipes/{id}/export", tag = "recipes",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Recipe id"), ExportQuery),
    responses(
        (status = 200, description = "The recipe as a portable document: JSON by default, or Markdown with `format=markdown`",
         content((RecipeExportBundle = "application/json"), (String = "text/markdown"))),
        (status = 400, description = "Unknown format", body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody),
    )
)]
pub async fn export(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<ExportQuery>,
) -> ApiResult<Response> {
    // Your own or a shared one: the same rule as reading it.
    let recipe = load_recipe(&state, Some(user.id), id).await?;
    let export = export_recipe(&state, Some(user.id), &recipe).await?;

    match q.format.as_deref().map(str::trim).unwrap_or("json") {
        "json" => Ok(Json(RecipeExportBundle {
            format: 1,
            generated_at: Utc::now(),
            recipe: export,
        })
        .into_response()),
        "markdown" | "md" => {
            let markdown = export.to_markdown(&recipe.per_serving, recipe.untracked_count);
            Ok((
                [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
                markdown,
            )
                .into_response())
        }
        other => Err(ApiError::bad_request(format!(
            "unknown format '{other}': use json or markdown"
        ))),
    }
}

/// A loaded recipe in the portable shape, with sub-recipes inlined as far as
/// `viewer` may see them. Boxed because it recurses; the depth is bounded by
/// the database's nesting cap, not by anything here.
pub fn export_recipe<'a>(
    state: &'a AppState,
    viewer: Option<Uuid>,
    recipe: &'a Recipe,
) -> Pin<Box<dyn Future<Output = ApiResult<RecipeExport>> + Send + 'a>> {
    Box::pin(async move {
        let mut items = Vec::with_capacity(recipe.items.len());
        for item in &recipe.items {
            let exported = if item.food_id.is_some() {
                RecipeExportItem {
                    food: Some(FoodRef {
                        key: food_key(&item.name, item.brand.as_deref()),
                        name: item.name.clone(),
                        brand: item.brand.clone(),
                        variant_label: item.variant_label.clone(),
                    }),
                    quantity_g: item.quantity_g,
                    note: item.note.clone(),
                    ..Default::default()
                }
            } else if let Some(sub_id) = item.sub_recipe_id {
                match load_recipe(state, viewer, sub_id).await {
                    Ok(sub) => RecipeExportItem {
                        recipe: Some(Box::new(export_recipe(state, viewer, &sub).await?)),
                        servings: item.servings,
                        note: item.note.clone(),
                        ..Default::default()
                    },
                    // A sub-recipe the author built on but has not shared. Its
                    // figures are already in the totals, but its ingredient
                    // list is theirs, so the file says what it is rather than
                    // listing it or dropping it.
                    Err(ApiError::NotFound(_)) => RecipeExportItem {
                        label: Some(format!(
                            "{} serving{} of {} (a recipe not shared with you)",
                            item.servings.unwrap_or(1.0),
                            if item.servings == Some(1.0) { "" } else { "s" },
                            item.name
                        )),
                        note: item.note.clone(),
                        ..Default::default()
                    },
                    Err(e) => return Err(e),
                }
            } else {
                RecipeExportItem {
                    label: item.label.clone(),
                    note: item.note.clone(),
                    ..Default::default()
                }
            };
            items.push(exported);
        }

        Ok(RecipeExport {
            name: recipe.name.clone(),
            description: recipe.description.clone(),
            instructions: recipe.instructions.clone(),
            servings: recipe.servings,
            is_public: recipe.is_public,
            items,
        })
    })
}

// ---------------------------------------------------------------------------
// Import from a URL
// ---------------------------------------------------------------------------

/// How many foods to offer per ingredient line.
const CANDIDATES_PER_LINE: usize = 5;

#[utoipa::path(
    post, path = "/api/v1/recipes/import", tag = "recipes",
    security(("bearer" = [])),
    request_body = ImportRecipeRequest,
    responses(
        (status = 200, description = "A draft read off the page: nothing is saved", body = RecipeDraft),
        (status = 400, description = "Not an importable URL, or no recipe on the page", body = crate::error::ErrorBody),
        (status = 502, description = "The page could not be fetched", body = crate::error::ErrorBody),
    )
)]
pub async fn import_from_url(
    State(state): State<AppState>,
    _user: CurrentUser,
    Json(body): Json<ImportRecipeRequest>,
) -> ApiResult<Json<RecipeDraft>> {
    body.validate()?;

    let page = fetch_public_page(&body.url).await?;

    // A URL may point straight at a JSON-LD document; anything else is a
    // page with the block somewhere inside it.
    let scraped = if page.content_type.starts_with("application/ld+json")
        || page.content_type.starts_with("application/json")
    {
        serde_json::from_str::<serde_json::Value>(&page.body)
            .ok()
            .as_ref()
            .and_then(jsonld::recipe_from_json)
    } else {
        jsonld::recipe_from_html(&page.body)
    };
    let scraped = scraped.ok_or_else(|| {
        ApiError::bad_request("no recipe found on that page: it carries no schema.org Recipe data")
    })?;

    let mut lines = Vec::with_capacity(scraped.ingredients.len());
    for raw in &scraped.ingredients {
        let parsed = parse_ingredient(raw);
        let term = parsed.search_term();
        let candidates = if term.is_empty() {
            Vec::new()
        } else {
            super::search::candidates(&state, &term, CANDIDATES_PER_LINE)
                .await?
                .into_iter()
                .map(|(tier, food)| DraftCandidate {
                    food_id: food.id,
                    name: food.name,
                    brand: food.brand,
                    serving_size_g: food.serving_size_g,
                    calories_kcal: food.calories_kcal,
                    protein_g: food.protein_g,
                    carbs_g: food.carbs_g,
                    fat_g: food.fat_g,
                    tier,
                })
                .collect()
        };
        lines.push(DraftLine {
            text: parsed.text,
            quantity: parsed.quantity,
            unit: parsed.unit,
            name: parsed.name,
            grams: parsed.grams,
            candidates,
        });
    }

    let instructions = if scraped.instructions.is_empty() {
        None
    } else {
        Some(scraped.instructions.join("\n"))
    };

    Ok(Json(RecipeDraft {
        name: if scraped.name.trim().is_empty() {
            "Imported recipe".to_string()
        } else {
            scraped.name.chars().take(200).collect()
        },
        description: scraped.description.map(|d| d.chars().take(2000).collect()),
        servings: scraped.servings,
        instructions: instructions.map(|i| i.chars().take(20000).collect()),
        lines,
        source_url: page.url,
        image_url: scraped.image,
        author: scraped.author,
    }))
}
