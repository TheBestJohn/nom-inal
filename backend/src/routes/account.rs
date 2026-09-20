//! Everything an account owns, out and back in.
//!
//! The export is one JSON document with no internal ids in it: foods by
//! natural key, recipes by name with sub-recipes inlined, diary entries by
//! date and what was eaten, weigh-ins by date. That is what makes the import
//! a merge rather than a restore — the same file can be read into the account
//! it came from, into a fresh account on another instance, or twice, and each
//! record is created once, updated when the file says something newer, and
//! otherwise left alone. The counts say which happened to what, and anything
//! the import could not place is named in `notes` rather than dropped.
//!
//! Photo metadata travels; photo bytes do not. Passwords and API keys never
//! do.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Write};

use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::domain::diary::DiaryRow;
use crate::domain::food::{FoodExport, VerificationStatus, FOOD_KEY_SQL};
use crate::domain::recipe::{
    FoodRef, RecipeExport, RecipeExportItem, RecipeItemInput, UpsertRecipeRequest,
};
use crate::domain::reminder::ReminderKind;
use crate::domain::target::{Nutrient, TargetKind};
use crate::domain::user::{
    ChartMode, Profile, TrackingFocus, UpdateProfileRequest, UserRow, USER_COLUMNS,
};
use crate::error::{ApiError, ApiResult};
use crate::extract::Json;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/export", get(export))
        .route("/import", axum::routing::post(import))
}

// ---------------------------------------------------------------------------
// The document
// ---------------------------------------------------------------------------

/// The profile without anything secret: no password hash, no keys, and the
/// email only so a reader knows whose file it is.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct ExportedProfile {
    pub display_name: String,
    pub email: String,
    pub sex: Option<String>,
    pub birth_date: Option<NaiveDate>,
    pub height_cm: Option<f64>,
    pub activity_level: Option<String>,
    pub goal: Option<String>,
    pub target_weight_kg: Option<f64>,
    pub shown_nutrients: Option<Vec<Nutrient>>,
    pub chart_nutrients: Option<Vec<Nutrient>>,
    pub chart_mode: Option<ChartMode>,
    pub tracking_focus: Option<TrackingFocus>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportedTarget {
    pub nutrient: Nutrient,
    pub amount: f64,
    pub kind: TargetKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportedReminder {
    pub kind: ReminderKind,
    pub every_days: i32,
    pub enabled: bool,
}

/// A diary entry by what it was, not by id: the date, the meal, the food by
/// key or the recipe by name, and the amount. `created_at` is kept so two
/// genuinely separate helpings of the same thing at the same meal stay two
/// entries through an export and back.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct ExportedDiaryEntry {
    pub logged_on: NaiveDate,
    pub meal: String,
    pub food: Option<FoodRef>,
    pub recipe_name: Option<String>,
    pub quantity_g: Option<f64>,
    pub recipe_servings: Option<f64>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportedWeight {
    pub recorded_on: NaiveDate,
    pub weight_kg: f64,
    pub body_fat_pct: Option<f64>,
    pub note: Option<String>,
}

/// A photo's metadata. The bytes are not in the file; `url` is where the
/// account that owns them can still fetch them, with its token.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportedPhoto {
    /// `weigh_in` or `recipe`.
    pub subject: String,
    pub recorded_on: Option<NaiveDate>,
    pub recipe_name: Option<String>,
    pub content_type: String,
    pub byte_size: i64,
    pub width: i32,
    pub height: i32,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
    pub url: String,
}

/// The whole account. Every list defaults to empty on the way in, so a
/// hand-trimmed file with only `weights` in it is a valid import.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct AccountExport {
    /// Format version of this document, not of the application.
    pub format: u32,
    pub generated_at: Option<DateTime<Utc>>,
    pub profile: Option<ExportedProfile>,
    pub targets: Vec<ExportedTarget>,
    pub reminders: Vec<ExportedReminder>,
    /// Foods this account created, plus any its recipes or diary refer to,
    /// so the import can recreate a missing one rather than lose the line
    /// that used it. In the same shape as `GET /foods/export`.
    pub foods: Vec<FoodExport>,
    pub recipes: Vec<RecipeExport>,
    pub diary: Vec<ExportedDiaryEntry>,
    pub weights: Vec<ExportedWeight>,
    pub photos: Vec<ExportedPhoto>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(default)]
pub struct ExportFormat {
    /// `json` (default), or `csv` for a zip holding `diary.csv` and
    /// `weights.csv`.
    pub format: Option<String>,
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

#[utoipa::path(
    get, path = "/api/v1/account/export", tag = "account",
    security(("bearer" = [])),
    params(ExportFormat),
    responses(
        (status = 200, description = "Everything the account owns, without secrets or photo bytes; with `format=csv`, a zip of `diary.csv` and `weights.csv`",
         content((AccountExport = "application/json"), (String = "application/zip"))),
        (status = 400, description = "Unknown format", body = crate::error::ErrorBody),
    )
)]
pub async fn export(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ExportFormat>,
) -> ApiResult<Response> {
    match q.format.as_deref().map(str::trim).unwrap_or("json") {
        "json" => Ok(Json(build_export(&state, user.id).await?).into_response()),
        "csv" => {
            let bytes = build_csv_zip(&state, user.id).await?;
            Ok((
                [
                    (header::CONTENT_TYPE, "application/zip"),
                    (
                        header::CONTENT_DISPOSITION,
                        "attachment; filename=\"nom-inal-export.zip\"",
                    ),
                ],
                bytes,
            )
                .into_response())
        }
        other => Err(ApiError::bad_request(format!(
            "unknown format '{other}': use json or csv"
        ))),
    }
}

async fn build_export(state: &AppState, user_id: Uuid) -> ApiResult<AccountExport> {
    let user: UserRow = sqlx::query_as(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1"))
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    let profile = Profile::from(user);

    let targets: Vec<ExportedTarget> = sqlx::query_as::<_, (String, f64, String)>(
        "SELECT nutrient, amount, kind FROM nutrition_targets WHERE user_id = $1 ORDER BY nutrient",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .filter_map(|(nutrient, amount, kind)| {
        Some(ExportedTarget {
            nutrient: Nutrient::from_key(&nutrient)?,
            amount,
            kind: TargetKind::from_str(&kind)?,
        })
    })
    .collect();

    let reminders: Vec<ExportedReminder> = sqlx::query_as::<_, (String, i32, bool)>(
        "SELECT kind, every_days, enabled FROM reminders WHERE user_id = $1 ORDER BY kind",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .filter_map(|(kind, every_days, enabled)| {
        Some(ExportedReminder {
            kind: crate::domain::reminder::ALL_KINDS
                .into_iter()
                .find(|k| k.key() == kind)?,
            every_days,
            enabled,
        })
    })
    .collect();

    // Foods this account created, and every food its recipes (through any
    // nesting of its own recipes) or diary point at. Parents before variants,
    // as the foods export orders them, so a reader can resolve
    // `variant_of_key` in one pass.
    let mut foods: Vec<FoodExport> = sqlx::query_as(&format!(
        "WITH RECURSIVE mine AS (
             SELECT id FROM recipes WHERE user_id = $1
             UNION
             SELECT ri.sub_recipe_id FROM recipe_items ri JOIN mine ON ri.recipe_id = mine.id
             WHERE ri.sub_recipe_id IS NOT NULL
         ),
         wanted AS (
             SELECT id FROM foods WHERE created_by = $1
             UNION
             SELECT ri.food_id FROM recipe_items ri JOIN mine ON ri.recipe_id = mine.id
             WHERE ri.food_id IS NOT NULL
             UNION
             SELECT d.food_id FROM diary_entries d WHERE d.user_id = $1 AND d.food_id IS NOT NULL
         ),
         wanted_with_parents AS (
             SELECT id FROM wanted
             UNION
             SELECT f.variant_of FROM foods f JOIN wanted ON f.id = wanted.id
             WHERE f.variant_of IS NOT NULL
         )
         SELECT
             f.source, f.source_id, f.name, f.brand, f.upc,
             f.calories_kcal, f.protein_g, f.carbs_g, f.fat_g, f.fiber_g, f.sugar_g,
             f.saturated_fat_g, f.sodium_mg, f.serving_size_g, f.serving_label,
             f.nutrient_basis,
             CASE WHEN f.variant_of IS NULL THEN NULL ELSE {parent_key} END AS variant_of_key,
             f.variant_label,
             f.revision,
             coalesce(v.confirmations, 0) AS confirmations,
             coalesce(v.disputes, 0)      AS disputes,
             food_quorum()                AS quorum
         FROM foods f
         JOIN wanted_with_parents w ON w.id = f.id
         LEFT JOIN foods p ON p.id = f.variant_of
         LEFT JOIN LATERAL (
             SELECT count(*) FILTER (WHERE verdict = 'confirm') AS confirmations,
                    count(*) FILTER (WHERE verdict = 'dispute') AS disputes
             FROM food_verifications
             WHERE food_id = f.id AND revision = f.revision
         ) v ON TRUE
         ORDER BY (f.variant_of IS NOT NULL), lower(f.name), lower(coalesce(f.brand, ''))",
        parent_key = FOOD_KEY_SQL.replace("f.", "p."),
    ))
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    for f in &mut foods {
        f.status = VerificationStatus::evaluate(f.confirmations, f.disputes, f.quorum);
    }

    let recipe_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM recipes WHERE user_id = $1 ORDER BY lower(name)")
            .bind(user_id)
            .fetch_all(&state.db)
            .await?;
    let mut recipes = Vec::with_capacity(recipe_ids.len());
    for id in recipe_ids {
        let recipe = super::recipes::load_recipe(state, Some(user_id), id).await?;
        recipes.push(super::recipes::export_recipe(state, Some(user_id), &recipe).await?);
    }

    #[derive(sqlx::FromRow)]
    struct DiaryLine {
        logged_on: NaiveDate,
        meal: String,
        food_key: Option<String>,
        food_name: Option<String>,
        food_brand: Option<String>,
        food_variant: Option<String>,
        recipe_name: Option<String>,
        quantity_g: Option<f64>,
        recipe_servings: Option<f64>,
        created_at: DateTime<Utc>,
    }
    let diary: Vec<ExportedDiaryEntry> = sqlx::query_as::<_, DiaryLine>(&format!(
        "SELECT d.logged_on, d.meal, d.quantity_g, d.recipe_servings, d.created_at,
                CASE WHEN f.id IS NULL THEN NULL ELSE {FOOD_KEY_SQL} END AS food_key,
                f.name AS food_name, f.brand AS food_brand, f.variant_label AS food_variant,
                r.name AS recipe_name
         FROM diary_entries d
         LEFT JOIN foods f ON f.id = d.food_id
         LEFT JOIN recipes r ON r.id = d.recipe_id
         WHERE d.user_id = $1
         ORDER BY d.logged_on, d.created_at"
    ))
    .bind(user_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|d| ExportedDiaryEntry {
        logged_on: d.logged_on,
        meal: d.meal,
        food: match (d.food_key, d.food_name) {
            (Some(key), Some(name)) => Some(FoodRef {
                key,
                name,
                brand: d.food_brand,
                variant_label: d.food_variant,
            }),
            _ => None,
        },
        recipe_name: d.recipe_name,
        quantity_g: d.quantity_g,
        recipe_servings: d.recipe_servings,
        created_at: Some(d.created_at),
    })
    .collect();

    let weights: Vec<ExportedWeight> =
        sqlx::query_as::<_, (NaiveDate, f64, Option<f64>, Option<String>)>(
            "SELECT recorded_on, weight_kg, body_fat_pct, note FROM weight_entries
         WHERE user_id = $1 ORDER BY recorded_on",
        )
        .bind(user_id)
        .fetch_all(&state.db)
        .await?
        .into_iter()
        .map(
            |(recorded_on, weight_kg, body_fat_pct, note)| ExportedWeight {
                recorded_on,
                weight_kg,
                body_fat_pct,
                note,
            },
        )
        .collect();

    #[derive(sqlx::FromRow)]
    struct PhotoLine {
        id: Uuid,
        recorded_on: Option<NaiveDate>,
        recipe_name: Option<String>,
        content_type: String,
        byte_size: i64,
        width: i32,
        height: i32,
        caption: Option<String>,
        created_at: DateTime<Utc>,
    }
    let photos: Vec<ExportedPhoto> = sqlx::query_as::<_, PhotoLine>(
        "SELECT p.id, w.recorded_on, r.name AS recipe_name, p.content_type, p.byte_size,
                p.width, p.height, p.caption, p.created_at
         FROM photos p
         LEFT JOIN weight_entries w ON w.id = p.weight_entry_id
         LEFT JOIN recipes r ON r.id = p.recipe_id
         WHERE p.user_id = $1
         ORDER BY p.created_at",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|p| ExportedPhoto {
        subject: if p.recipe_name.is_some() {
            "recipe".into()
        } else {
            "weigh_in".into()
        },
        recorded_on: p.recorded_on,
        recipe_name: p.recipe_name,
        content_type: p.content_type,
        byte_size: p.byte_size,
        width: p.width,
        height: p.height,
        caption: p.caption,
        created_at: p.created_at,
        url: crate::domain::photo::photo_url(p.id),
    })
    .collect();

    Ok(AccountExport {
        format: 1,
        generated_at: Some(Utc::now()),
        profile: Some(ExportedProfile {
            display_name: profile.display_name,
            email: profile.email,
            sex: profile.sex,
            birth_date: profile.birth_date,
            height_cm: profile.height_cm,
            activity_level: Some(profile.activity_level),
            goal: Some(profile.goal),
            target_weight_kg: profile.target_weight_kg,
            shown_nutrients: Some(profile.shown_nutrients),
            chart_nutrients: Some(profile.chart_nutrients),
            chart_mode: Some(profile.chart_mode),
            tracking_focus: profile.tracking_focus,
            created_at: Some(profile.created_at),
        }),
        targets,
        reminders,
        foods,
        recipes,
        diary,
        weights,
        photos,
    })
}

/// `diary.csv` and `weights.csv` in one zip: the two tables people actually
/// put in a spreadsheet, with the diary's nutrients computed the same way
/// the diary page computes them.
async fn build_csv_zip(state: &AppState, user_id: Uuid) -> ApiResult<Vec<u8>> {
    let rows: Vec<DiaryRow> = sqlx::query_as(&format!(
        "{} WHERE d.user_id = $1 ORDER BY d.logged_on, d.created_at",
        super::diary::ENTRY_SELECT
    ))
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    let mut diary = String::new();
    csv_line(
        &mut diary,
        &[
            "date",
            "meal",
            "kind",
            "name",
            "brand",
            "quantity_g",
            "recipe_servings",
            "calories_kcal",
            "protein_g",
            "carbs_g",
            "net_carbs_g",
            "fat_g",
            "fiber_g",
            "sugar_g",
            "saturated_fat_g",
            "sodium_mg",
            "logged_at",
        ],
    );
    for row in rows {
        let n = row.nutrients().rounded();
        let kind = if row.recipe_id.is_some() {
            "recipe"
        } else {
            "food"
        };
        let name = row
            .food_name
            .clone()
            .or_else(|| row.recipe_name.clone())
            .unwrap_or_default();
        csv_line(
            &mut diary,
            &[
                &row.logged_on.to_string(),
                &row.meal,
                kind,
                &name,
                row.food_brand.as_deref().unwrap_or(""),
                &opt_num(row.quantity_g),
                &opt_num(row.recipe_servings),
                &num(n.calories_kcal),
                &num(n.protein_g),
                &num(n.carbs_g),
                &num(n.net_carbs_g()),
                &num(n.fat_g),
                &num(n.fiber_g),
                &num(n.sugar_g),
                &num(n.saturated_fat_g),
                &num(n.sodium_mg),
                &row.created_at.to_rfc3339(),
            ],
        );
    }

    let weights: Vec<(NaiveDate, f64, Option<f64>, Option<String>)> = sqlx::query_as(
        "SELECT recorded_on, weight_kg, body_fat_pct, note FROM weight_entries
         WHERE user_id = $1 ORDER BY recorded_on",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    let mut weight_csv = String::new();
    csv_line(
        &mut weight_csv,
        &["date", "weight_kg", "body_fat_pct", "note"],
    );
    for (date, kg, fat, note) in weights {
        csv_line(
            &mut weight_csv,
            &[
                &date.to_string(),
                &num(kg),
                &opt_num(fat),
                note.as_deref().unwrap_or(""),
            ],
        );
    }

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in [("diary.csv", diary), ("weights.csv", weight_csv)] {
            zip.start_file(name, options)
                .and_then(|_| zip.write_all(body.as_bytes()).map_err(Into::into))
                .map_err(|e| ApiError::Internal(anyhow::anyhow!("writing {name}: {e}")))?;
        }
        zip.finish()
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("closing the zip: {e}")))?;
    }
    Ok(cursor.into_inner())
}

/// One CSV record, RFC 4180: fields quoted when they hold a comma, a quote
/// or a line break, quotes doubled inside.
fn csv_line(out: &mut String, fields: &[&str]) {
    for (i, field) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        if field.contains([',', '"', '\n', '\r']) {
            out.push('"');
            out.push_str(&field.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(field);
        }
    }
    out.push_str("\r\n");
}

fn num(v: f64) -> String {
    let s = format!("{:.3}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn opt_num(v: Option<f64>) -> String {
    v.map(num).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// What happened to one kind of record.
#[derive(Debug, Default, Serialize, ToSchema)]
pub struct MergeCount {
    pub created: i64,
    pub updated: i64,
    /// Already present and identical, or could not be placed — the latter is
    /// always also named in `notes`.
    pub skipped: i64,
}

#[derive(Debug, Default, Serialize, ToSchema)]
pub struct ImportReport {
    /// Whether any profile field changed. The email is never touched.
    pub profile_updated: bool,
    pub targets: MergeCount,
    pub reminders: MergeCount,
    pub foods: MergeCount,
    pub recipes: MergeCount,
    pub diary: MergeCount,
    pub weights: MergeCount,
    /// Everything the import could not do as asked, one line each.
    pub notes: Vec<String>,
}

/// How many notes to keep. Past this the file has a systematic problem and
/// one more line will not explain it better.
const MAX_NOTES: usize = 100;

fn note(report: &mut ImportReport, text: String) {
    if report.notes.len() < MAX_NOTES {
        report.notes.push(text);
    } else if report.notes.len() == MAX_NOTES {
        report.notes.push("… and more".into());
    }
}

#[utoipa::path(
    post, path = "/api/v1/account/import", tag = "account",
    security(("bearer" = [])),
    request_body = AccountExport,
    responses(
        (status = 200, description = "What was created, updated and skipped. All or nothing: a file that cannot be applied changes nothing", body = ImportReport),
        (status = 400, body = crate::error::ErrorBody),
    )
)]
pub async fn import(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(file): Json<AccountExport>,
) -> ApiResult<Json<ImportReport>> {
    if file.format > 1 {
        return Err(ApiError::bad_request(format!(
            "this file is format {}, and this version reads up to format 1",
            file.format
        )));
    }

    let mut report = ImportReport::default();

    // One transaction for the whole file: a merge that stops halfway would
    // leave the account in a state neither the file nor the account was in.
    // Opened as an authored transaction so any food it creates is attributed
    // in the food's history.
    let mut tx = super::foods::authored_tx(
        &state,
        user.id,
        "import",
        Some("restored from an account export"),
    )
    .await?;

    if let Some(profile) = &file.profile {
        report.profile_updated = merge_profile(&mut tx, user.id, profile).await?;
    }
    merge_targets(&mut tx, user.id, &file.targets, &mut report).await?;
    merge_reminders(&mut tx, user.id, &file.reminders, &mut report).await?;
    merge_foods(&mut tx, user.id, &file.foods, &mut report).await?;
    let recipe_ids = merge_recipes(&mut tx, user.id, &file.recipes, &mut report).await?;
    merge_diary(&mut tx, user.id, &file.diary, &recipe_ids, &mut report).await?;
    merge_weights(&mut tx, user.id, &file.weights, &mut report).await?;

    if !file.photos.is_empty() {
        note(
            &mut report,
            format!(
                "{} photo{} listed in the file were not imported: photo bytes do not travel in an export",
                file.photos.len(),
                if file.photos.len() == 1 { "" } else { "s" }
            ),
        );
    }

    tx.commit().await?;
    Ok(Json(report))
}

/// Fill in whatever the file says and the account has not set differently.
/// Nothing is blanked: a null in the file leaves the field alone, so a
/// trimmed file cannot erase a profile.
async fn merge_profile(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    p: &ExportedProfile,
) -> ApiResult<bool> {
    let before: UserRow =
        sqlx::query_as(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1"))
            .bind(user_id)
            .fetch_one(&mut **tx)
            .await?;
    let before = serde_json::to_value(Profile::from(before)).unwrap_or_default();

    let after: UserRow = sqlx::query_as(&format!(
        "UPDATE users SET
            display_name = COALESCE($2, display_name),
            sex = COALESCE($3, sex),
            birth_date = COALESCE($4, birth_date),
            height_cm = COALESCE($5, height_cm),
            activity_level = COALESCE($6, activity_level),
            goal = COALESCE($7, goal),
            target_weight_kg = COALESCE($8, target_weight_kg),
            shown_nutrients = COALESCE($9, shown_nutrients),
            chart_nutrients = COALESCE($10, chart_nutrients),
            chart_mode = COALESCE($11, chart_mode),
            tracking_focus = COALESCE($12, tracking_focus),
            updated_at = now()
         WHERE id = $1
         RETURNING {USER_COLUMNS}"
    ))
    .bind(user_id)
    .bind(
        Some(p.display_name.trim())
            .filter(|n| !n.is_empty())
            .map(|n| n.chars().take(100).collect::<String>()),
    )
    .bind(p.sex.as_deref())
    .bind(p.birth_date)
    .bind(p.height_cm.filter(|h| (50.0..=280.0).contains(h)))
    .bind(p.activity_level.as_deref())
    .bind(p.goal.as_deref())
    .bind(p.target_weight_kg.filter(|w| (20.0..=500.0).contains(w)))
    .bind(
        p.shown_nutrients
            .as_deref()
            .map(UpdateProfileRequest::nutrient_keys),
    )
    .bind(
        p.chart_nutrients
            .as_deref()
            .map(UpdateProfileRequest::nutrient_keys),
    )
    .bind(p.chart_mode.map(|m| m.as_str()))
    .bind(p.tracking_focus.map(|f| f.as_str()))
    .fetch_one(&mut **tx)
    .await?;
    let after = serde_json::to_value(Profile::from(after)).unwrap_or_default();

    Ok(before != after)
}

async fn merge_targets(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    targets: &[ExportedTarget],
    report: &mut ImportReport,
) -> ApiResult<()> {
    let existing: HashMap<String, (f64, String)> = sqlx::query_as::<_, (String, f64, String)>(
        "SELECT nutrient, amount, kind FROM nutrition_targets WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|(n, a, k)| (n, (a, k)))
    .collect();

    let mut seen = HashSet::new();
    for t in targets {
        let key = t.nutrient.key();
        if !seen.insert(key) {
            continue;
        }
        if !(0.1..=100000.0).contains(&t.amount) {
            report.targets.skipped += 1;
            note(
                report,
                format!("target for {key}: amount {} is out of range", t.amount),
            );
            continue;
        }
        match existing.get(key) {
            Some((amount, kind)) if *amount == t.amount && kind == t.kind.as_str() => {
                report.targets.skipped += 1;
            }
            Some(_) => {
                sqlx::query(
                    "UPDATE nutrition_targets SET amount = $3, kind = $4, updated_at = now()
                     WHERE user_id = $1 AND nutrient = $2",
                )
                .bind(user_id)
                .bind(key)
                .bind(t.amount)
                .bind(t.kind.as_str())
                .execute(&mut **tx)
                .await?;
                report.targets.updated += 1;
            }
            None => {
                sqlx::query(
                    "INSERT INTO nutrition_targets (user_id, nutrient, amount, kind)
                     VALUES ($1, $2, $3, $4)",
                )
                .bind(user_id)
                .bind(key)
                .bind(t.amount)
                .bind(t.kind.as_str())
                .execute(&mut **tx)
                .await?;
                report.targets.created += 1;
            }
        }
    }
    Ok(())
}

async fn merge_reminders(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    reminders: &[ExportedReminder],
    report: &mut ImportReport,
) -> ApiResult<()> {
    let existing: HashMap<String, (i32, bool)> = sqlx::query_as::<_, (String, i32, bool)>(
        "SELECT kind, every_days, enabled FROM reminders WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|(k, d, e)| (k, (d, e)))
    .collect();

    let mut seen = HashSet::new();
    for r in reminders {
        let key = r.kind.key();
        if !seen.insert(key) {
            continue;
        }
        if !(1..=365).contains(&r.every_days) {
            report.reminders.skipped += 1;
            note(
                report,
                format!(
                    "reminder {key}: every {} days is out of range",
                    r.every_days
                ),
            );
            continue;
        }
        match existing.get(key) {
            Some((days, enabled)) if *days == r.every_days && *enabled == r.enabled => {
                report.reminders.skipped += 1;
            }
            Some(_) => {
                sqlx::query(
                    "UPDATE reminders SET every_days = $3, enabled = $4, updated_at = now()
                     WHERE user_id = $1 AND kind = $2",
                )
                .bind(user_id)
                .bind(key)
                .bind(r.every_days)
                .bind(r.enabled)
                .execute(&mut **tx)
                .await?;
                report.reminders.updated += 1;
            }
            None => {
                sqlx::query(
                    "INSERT INTO reminders (user_id, kind, every_days, enabled) VALUES ($1, $2, $3, $4)",
                )
                .bind(user_id)
                .bind(key)
                .bind(r.every_days)
                .bind(r.enabled)
                .execute(&mut **tx)
                .await?;
                report.reminders.created += 1;
            }
        }
    }
    Ok(())
}

/// The id of the food with this natural key and variant label, if the
/// instance has one. A variant shares its parent's name and brand, so the
/// label is part of the identity, as it is in the foods export. When several
/// rows still share a key — the food table is global and duplicates happen —
/// the oldest wins, which is also what the search collapses to.
async fn food_by_key(
    tx: &mut Transaction<'_, Postgres>,
    key: &str,
    variant_label: Option<&str>,
) -> ApiResult<Option<Uuid>> {
    Ok(sqlx::query_scalar(&format!(
        "SELECT f.id FROM foods f
         WHERE {FOOD_KEY_SQL} = $1
           AND lower(btrim(coalesce(f.variant_label, ''))) = lower(btrim(coalesce($2, '')))
         ORDER BY f.created_at LIMIT 1"
    ))
    .bind(key)
    .bind(variant_label)
    .fetch_optional(&mut **tx)
    .await?)
}

/// Foods are a shared record, so an import only ever adds one the instance
/// does not have. One it does have is left exactly as it is — the instance
/// may have corrected it since, and a personal file is not the place to
/// undo that — and reported as skipped.
async fn merge_foods(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    foods: &[FoodExport],
    report: &mut ImportReport,
) -> ApiResult<()> {
    for f in foods {
        let key = crate::domain::food::food_key(&f.name, f.brand.as_deref());
        if f.name.trim().is_empty() {
            report.foods.skipped += 1;
            note(report, "a food with no name was skipped".into());
            continue;
        }
        if food_by_key(tx, &key, f.variant_label.as_deref())
            .await?
            .is_some()
        {
            report.foods.skipped += 1;
            continue;
        }

        // The same bounds `POST /foods` applies to the stored per-100 g
        // figures; the file is per 100 g already.
        let over = [
            ("calories_kcal", f.calories_kcal, 900.0),
            ("protein_g", f.protein_g, 100.0),
            ("carbs_g", f.carbs_g, 100.0),
            ("fat_g", f.fat_g, 100.0),
            ("fiber_g", f.fiber_g.unwrap_or_default(), 100.0),
            ("sugar_g", f.sugar_g.unwrap_or_default(), 100.0),
            (
                "saturated_fat_g",
                f.saturated_fat_g.unwrap_or_default(),
                100.0,
            ),
            ("sodium_mg", f.sodium_mg.unwrap_or_default(), 50_000.0),
        ]
        .into_iter()
        .find(|(_, v, max)| *v < 0.0 || *v > *max || !v.is_finite());
        if let Some((field, v, _)) = over {
            report.foods.skipped += 1;
            note(
                report,
                format!("food '{}': {field} = {v} per 100 g is out of range", f.name),
            );
            continue;
        }
        if !(0.1..=5000.0).contains(&f.serving_size_g) {
            report.foods.skipped += 1;
            note(
                report,
                format!(
                    "food '{}': serving size {} g is out of range",
                    f.name, f.serving_size_g
                ),
            );
            continue;
        }

        let variant_of = match &f.variant_of_key {
            None => None,
            Some(parent_key) => match food_by_key(tx, parent_key, None).await? {
                Some(id) => Some(id),
                None => {
                    // Kept as a plain food rather than lost: the numbers are
                    // still right, only the link to its parent is missing.
                    note(report, format!(
                        "food '{}' is a variant of '{}', which is not on this instance; imported as a food of its own",
                        f.name, parent_key
                    ));
                    None
                }
            },
        };
        let variant_label = variant_of.and(f.variant_label.as_deref());
        let source = match f.source.as_str() {
            "usda" | "off" if f.source_id.is_some() => f.source.as_str(),
            _ => "custom",
        };
        let source_id = if source == "custom" {
            None
        } else {
            f.source_id.as_deref()
        };
        let basis = match f.nutrient_basis.as_str() {
            "per_serving" => "per_serving",
            _ => "per_100g",
        };

        // A savepoint around the insert: a unique violation on (source,
        // source id) means the instance already has this provider's food
        // under another name, and that must not abort the whole import.
        sqlx::query("SAVEPOINT food_insert")
            .execute(&mut **tx)
            .await?;
        let inserted = sqlx::query(
            "INSERT INTO foods (source, source_id, name, brand, upc, calories_kcal, protein_g,
                                carbs_g, fat_g, fiber_g, sugar_g, saturated_fat_g, sodium_mg,
                                serving_size_g, serving_label, variant_of, variant_label,
                                nutrient_basis, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                     $18, $19)",
        )
        .bind(source)
        .bind(source_id)
        .bind(f.name.trim())
        .bind(f.brand.as_deref().map(str::trim).filter(|b| !b.is_empty()))
        .bind(f.upc.as_deref())
        .bind(f.calories_kcal)
        .bind(f.protein_g)
        .bind(f.carbs_g)
        .bind(f.fat_g)
        .bind(f.fiber_g)
        .bind(f.sugar_g)
        .bind(f.saturated_fat_g)
        .bind(f.sodium_mg)
        .bind(f.serving_size_g)
        .bind(f.serving_label.as_deref())
        .bind(variant_of)
        .bind(variant_label)
        .bind(basis)
        .bind(user_id)
        .execute(&mut **tx)
        .await;
        match inserted {
            Ok(_) => {
                sqlx::query("RELEASE SAVEPOINT food_insert")
                    .execute(&mut **tx)
                    .await?;
                report.foods.created += 1;
            }
            Err(sqlx::Error::Database(db))
                if db.is_unique_violation() || db.is_check_violation() =>
            {
                sqlx::query("ROLLBACK TO SAVEPOINT food_insert")
                    .execute(&mut **tx)
                    .await?;
                report.foods.skipped += 1;
                note(
                    report,
                    format!(
                        "food '{}' was not created: {}",
                        f.name,
                        if db.is_unique_violation() {
                            "this instance already has that provider entry under another name"
                                .to_string()
                        } else {
                            db.message().to_string()
                        }
                    ),
                );
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// The recipes in the file in the order they can be written: every
/// sub-recipe before the recipe that uses it, each name once.
fn flatten_recipes<'a>(
    recipes: &'a [RecipeExport],
    out: &mut Vec<&'a RecipeExport>,
    seen: &mut HashSet<String>,
    depth: usize,
) -> Result<(), String> {
    if depth > 6 {
        return Err("recipes in the file nest more than 6 levels deep".into());
    }
    for r in recipes {
        let subs: Vec<&RecipeExport> = r.items.iter().filter_map(|i| i.recipe.as_deref()).collect();
        for sub in subs {
            flatten_recipes(std::slice::from_ref(sub), out, seen, depth + 1)?;
        }
        if seen.insert(r.name.trim().to_lowercase()) {
            out.push(r);
        }
    }
    Ok(())
}

/// What a recipe write would put in the database, as one comparable value:
/// the header and each ingredient as the ids it resolved to. Compared with
/// the same shape read back for the existing recipe, so "unchanged" means
/// the rows would be identical after the write — a food the instance does
/// not have compares as the text line it becomes, and stays skipped on the
/// second import rather than rewritten every time.
fn shape_of(body: &UpsertRecipeRequest) -> serde_json::Value {
    let items: Vec<serde_json::Value> = body
        .items
        .iter()
        .map(|i| {
            serde_json::json!({
                "food_id": i.food_id,
                "sub_recipe_id": i.sub_recipe_id,
                "label": i.label.as_deref().map(str::trim),
                "quantity_g": i.quantity_g,
                "servings": i.servings,
                "note": i.note.as_deref().map(str::trim).filter(|n| !n.is_empty()),
            })
        })
        .collect();
    serde_json::json!({
        "name": body.name.trim(),
        "description": body.description.as_deref().map(str::trim).filter(|d| !d.is_empty()),
        "instructions": body.instructions.as_deref().map(str::trim).filter(|d| !d.is_empty()),
        "servings": body.servings,
        "is_public": body.is_public,
        "items": items,
    })
}

/// Merge recipes by name. Returns the account's recipe ids by lowercase
/// name, for the diary to point at.
async fn merge_recipes(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    recipes: &[RecipeExport],
    report: &mut ImportReport,
) -> ApiResult<HashMap<String, Uuid>> {
    let mut by_name: HashMap<String, Uuid> = sqlx::query_as::<_, (String, Uuid)>(
        "SELECT lower(btrim(name)), id FROM recipes WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    // Two of the same name (possible: nothing forbids it) resolve to the
    // oldest, consistently.
    .rev()
    .collect();

    let mut ordered = Vec::new();
    flatten_recipes(recipes, &mut ordered, &mut HashSet::new(), 0)
        .map_err(ApiError::bad_request)?;

    for r in ordered {
        let name = r.name.trim();
        if name.is_empty() {
            report.recipes.skipped += 1;
            note(report, "a recipe with no name was skipped".into());
            continue;
        }
        if !(0.1..=1000.0).contains(&r.servings) {
            report.recipes.skipped += 1;
            note(
                report,
                format!("recipe '{name}': {} servings is out of range", r.servings),
            );
            continue;
        }

        // Resolve each ingredient to what this instance has. A food that is
        // missing keeps its line as free text so the recipe is still whole,
        // and the note says the macros are short by it.
        let mut items = Vec::with_capacity(r.items.len());
        for (i, item) in r.items.iter().enumerate() {
            let resolved = resolve_item(tx, &by_name, name, i, item, report).await?;
            if let Some(input) = resolved {
                items.push(input);
            }
        }
        if items.is_empty() {
            report.recipes.skipped += 1;
            note(
                report,
                format!("recipe '{name}' has no ingredients and was skipped"),
            );
            continue;
        }

        let body = UpsertRecipeRequest {
            name: name.chars().take(200).collect(),
            description: r
                .description
                .as_deref()
                .map(|d| d.chars().take(2000).collect())
                .filter(|d: &String| !d.trim().is_empty()),
            instructions: r
                .instructions
                .as_deref()
                .map(|d| d.chars().take(20000).collect())
                .filter(|d: &String| !d.trim().is_empty()),
            servings: r.servings,
            is_public: r.is_public,
            items,
        };

        let lower = name.to_lowercase();
        match by_name.get(&lower).copied() {
            Some(existing_id) => {
                // Compare what the write would store with what is there,
                // before rewriting anything.
                let current = current_shape(tx, user_id, existing_id).await?;
                if current == Some(shape_of(&body)) {
                    report.recipes.skipped += 1;
                    continue;
                }
                match super::recipes::replace_recipe(tx, user_id, existing_id, &body).await {
                    Ok(true) => report.recipes.updated += 1,
                    Ok(false) => {
                        report.recipes.skipped += 1;
                        note(report, format!("recipe '{name}' could not be updated"));
                    }
                    Err(ApiError::BadRequest(m)) => {
                        return Err(ApiError::bad_request(format!("recipe '{name}': {m}")))
                    }
                    Err(e) => return Err(e),
                }
            }
            None => {
                let id = super::recipes::insert_recipe(tx, user_id, &body)
                    .await
                    .map_err(|e| match e {
                        ApiError::BadRequest(m) => {
                            ApiError::bad_request(format!("recipe '{name}': {m}"))
                        }
                        other => other,
                    })?;
                by_name.insert(lower, id);
                report.recipes.created += 1;
            }
        }
    }

    Ok(by_name)
}

/// The existing recipe in the same shape `shape_of` gives a write, read
/// inside the transaction so a sub-recipe written moments ago is seen.
async fn current_shape(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: Uuid,
) -> ApiResult<Option<serde_json::Value>> {
    #[derive(sqlx::FromRow)]
    struct Head {
        name: String,
        description: Option<String>,
        instructions: Option<String>,
        servings: f64,
        is_public: bool,
    }
    let Some(head) = sqlx::query_as::<_, Head>(
        "SELECT name, description, instructions, servings, is_public
         FROM recipes WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(None);
    };

    let lines: Vec<RecipeItemInput> = sqlx::query_as::<
        _,
        (
            Option<Uuid>,
            Option<Uuid>,
            Option<String>,
            Option<f64>,
            Option<f64>,
            Option<String>,
        ),
    >(
        "SELECT food_id, sub_recipe_id, label, quantity_g, servings, note
         FROM recipe_items WHERE recipe_id = $1 ORDER BY sort_order",
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(
        |(food_id, sub_recipe_id, label, quantity_g, servings, note)| RecipeItemInput {
            food_id,
            label,
            sub_recipe_id,
            quantity_g,
            servings,
            note,
        },
    )
    .collect();

    Ok(Some(shape_of(&UpsertRecipeRequest {
        name: head.name,
        description: head.description,
        instructions: head.instructions,
        servings: head.servings,
        is_public: head.is_public,
        items: lines,
    })))
}

async fn resolve_item(
    tx: &mut Transaction<'_, Postgres>,
    recipes_by_name: &HashMap<String, Uuid>,
    recipe_name: &str,
    index: usize,
    item: &RecipeExportItem,
    report: &mut ImportReport,
) -> ApiResult<Option<RecipeItemInput>> {
    let note_text = item
        .note
        .as_deref()
        .map(|n| n.chars().take(200).collect::<String>())
        .filter(|n| !n.trim().is_empty());

    if let Some(food) = &item.food {
        let key = if food.key.trim().is_empty() {
            crate::domain::food::food_key(&food.name, food.brand.as_deref())
        } else {
            food.key.clone()
        };
        let grams = item.quantity_g.unwrap_or_default();
        if !(0.1..=100000.0).contains(&grams) {
            note(report, format!(
                "recipe '{recipe_name}', ingredient {}: {grams} g of '{}' is out of range; kept as text",
                index + 1, food.name
            ));
            return Ok(Some(text_item(
                format!("{grams} g {}", food.name),
                note_text,
            )));
        }
        return match food_by_key(tx, &key, food.variant_label.as_deref()).await? {
            Some(id) => Ok(Some(RecipeItemInput {
                food_id: Some(id),
                label: None,
                sub_recipe_id: None,
                quantity_g: Some(grams),
                servings: None,
                note: note_text,
            })),
            None => {
                note(report, format!(
                    "recipe '{recipe_name}': food '{}' is not on this instance; its line was kept as text and the macros exclude it",
                    food.name
                ));
                Ok(Some(text_item(
                    format!("{} g {}", trim_num(grams), food.name),
                    note_text,
                )))
            }
        };
    }

    if let Some(sub) = &item.recipe {
        let servings = item.servings.unwrap_or(1.0);
        let lower = sub.name.trim().to_lowercase();
        return match recipes_by_name.get(&lower) {
            Some(id) if (0.01..=1000.0).contains(&servings) => Ok(Some(RecipeItemInput {
                food_id: None,
                label: None,
                sub_recipe_id: Some(*id),
                quantity_g: None,
                servings: Some(servings),
                note: note_text,
            })),
            _ => {
                note(
                    report,
                    format!(
                        "recipe '{recipe_name}': sub-recipe '{}' could not be linked; kept as text",
                        sub.name
                    ),
                );
                Ok(Some(text_item(
                    format!(
                        "{} serving{} of {}",
                        trim_num(servings),
                        if servings == 1.0 { "" } else { "s" },
                        sub.name
                    ),
                    note_text,
                )))
            }
        };
    }

    if let Some(label) = item
        .label
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        return Ok(Some(text_item(label.to_string(), note_text)));
    }

    note(
        report,
        format!(
            "recipe '{recipe_name}', ingredient {}: neither a food, a recipe nor text; skipped",
            index + 1
        ),
    );
    Ok(None)
}

fn text_item(label: String, note: Option<String>) -> RecipeItemInput {
    RecipeItemInput {
        food_id: None,
        label: Some(label.chars().take(200).collect()),
        sub_recipe_id: None,
        quantity_g: None,
        servings: None,
        note,
    }
}

fn trim_num(v: f64) -> String {
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

async fn merge_diary(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    entries: &[ExportedDiaryEntry],
    recipes_by_name: &HashMap<String, Uuid>,
    report: &mut ImportReport,
) -> ApiResult<()> {
    for (i, e) in entries.iter().enumerate() {
        let meal = e.meal.trim().to_lowercase();
        let meal = if meal.is_empty() {
            "snack".to_string()
        } else {
            meal
        };

        let (food_id, recipe_id) = match (&e.food, &e.recipe_name) {
            (Some(food), None) => {
                let key = if food.key.trim().is_empty() {
                    crate::domain::food::food_key(&food.name, food.brand.as_deref())
                } else {
                    food.key.clone()
                };
                match food_by_key(tx, &key, food.variant_label.as_deref()).await? {
                    Some(id) => (Some(id), None),
                    None => {
                        report.diary.skipped += 1;
                        note(
                            report,
                            format!(
                                "diary {} {}: food '{}' is not on this instance; entry skipped",
                                e.logged_on, meal, food.name
                            ),
                        );
                        continue;
                    }
                }
            }
            (None, Some(name)) => match recipes_by_name.get(&name.trim().to_lowercase()) {
                Some(id) => (None, Some(*id)),
                None => {
                    report.diary.skipped += 1;
                    note(
                        report,
                        format!(
                            "diary {} {}: recipe '{name}' is not in this account; entry skipped",
                            e.logged_on, meal
                        ),
                    );
                    continue;
                }
            },
            _ => {
                report.diary.skipped += 1;
                note(
                    report,
                    format!(
                        "diary entry {}: needs exactly one of a food or a recipe",
                        i + 1
                    ),
                );
                continue;
            }
        };

        let quantity_g = food_id.and(e.quantity_g);
        let recipe_servings = recipe_id.and(e.recipe_servings.or(Some(1.0)));
        if let Some(q) = quantity_g {
            if !(0.1..=100000.0).contains(&q) {
                report.diary.skipped += 1;
                note(
                    report,
                    format!("diary {} {}: {q} g is out of range", e.logged_on, meal),
                );
                continue;
            }
        } else if food_id.is_some() {
            report.diary.skipped += 1;
            note(
                report,
                format!(
                    "diary {} {}: a food entry needs quantity_g",
                    e.logged_on, meal
                ),
            );
            continue;
        }
        if let Some(s) = recipe_servings {
            if !(0.01..=1000.0).contains(&s) {
                report.diary.skipped += 1;
                note(
                    report,
                    format!(
                        "diary {} {}: {s} servings is out of range",
                        e.logged_on, meal
                    ),
                );
                continue;
            }
        }

        let exists: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM diary_entries
             WHERE user_id = $1 AND logged_on = $2 AND meal = $3
               AND food_id IS NOT DISTINCT FROM $4
               AND recipe_id IS NOT DISTINCT FROM $5
               AND quantity_g IS NOT DISTINCT FROM $6
               AND recipe_servings IS NOT DISTINCT FROM $7
               AND ($8::timestamptz IS NULL OR created_at = $8)
             LIMIT 1",
        )
        .bind(user_id)
        .bind(e.logged_on)
        .bind(&meal)
        .bind(food_id)
        .bind(recipe_id)
        .bind(quantity_g)
        .bind(recipe_servings)
        .bind(e.created_at)
        .fetch_optional(&mut **tx)
        .await?;
        if exists.is_some() {
            report.diary.skipped += 1;
            continue;
        }

        sqlx::query(
            "INSERT INTO diary_entries
                 (user_id, logged_on, meal, food_id, recipe_id, quantity_g, recipe_servings, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, coalesce($8, now()))",
        )
        .bind(user_id)
        .bind(e.logged_on)
        .bind(&meal)
        .bind(food_id)
        .bind(recipe_id)
        .bind(quantity_g)
        .bind(recipe_servings)
        .bind(e.created_at)
        .execute(&mut **tx)
        .await?;
        report.diary.created += 1;
    }
    Ok(())
}

async fn merge_weights(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    weights: &[ExportedWeight],
    report: &mut ImportReport,
) -> ApiResult<()> {
    let existing: HashMap<NaiveDate, (f64, Option<f64>, Option<String>)> = sqlx::query_as::<
        _,
        (NaiveDate, f64, Option<f64>, Option<String>),
    >(
        "SELECT recorded_on, weight_kg, body_fat_pct, note FROM weight_entries WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|(d, w, f, n)| (d, (w, f, n)))
    .collect();

    let mut seen = HashSet::new();
    for w in weights {
        if !seen.insert(w.recorded_on) {
            continue;
        }
        if !(1.0..=699.0).contains(&w.weight_kg)
            || w.body_fat_pct.is_some_and(|f| !(0.0..=100.0).contains(&f))
        {
            report.weights.skipped += 1;
            note(
                report,
                format!("weigh-in {}: figures out of range", w.recorded_on),
            );
            continue;
        }
        let note_text = w
            .note
            .as_deref()
            .map(|n| n.chars().take(500).collect::<String>())
            .filter(|n| !n.trim().is_empty());
        match existing.get(&w.recorded_on) {
            Some((kg, fat, n))
                if *kg == w.weight_kg && *fat == w.body_fat_pct && *n == note_text =>
            {
                report.weights.skipped += 1;
            }
            Some(_) => {
                sqlx::query(
                    "UPDATE weight_entries SET weight_kg = $3, body_fat_pct = $4, note = $5,
                                               updated_at = now()
                     WHERE user_id = $1 AND recorded_on = $2",
                )
                .bind(user_id)
                .bind(w.recorded_on)
                .bind(w.weight_kg)
                .bind(w.body_fat_pct)
                .bind(&note_text)
                .execute(&mut **tx)
                .await?;
                report.weights.updated += 1;
            }
            None => {
                sqlx::query(
                    "INSERT INTO weight_entries (user_id, recorded_on, weight_kg, body_fat_pct, note)
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(user_id)
                .bind(w.recorded_on)
                .bind(w.weight_kg)
                .bind(w.body_fat_pct)
                .bind(&note_text)
                .execute(&mut **tx)
                .await?;
                report.weights.created += 1;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quoting() {
        let mut out = String::new();
        csv_line(&mut out, &["a", "b,c", "say \"hi\"", "two\nlines", ""]);
        assert_eq!(out, "a,\"b,c\",\"say \"\"hi\"\"\",\"two\nlines\",\r\n");
        assert_eq!(num(1.5), "1.5");
        assert_eq!(num(2.0), "2");
        assert_eq!(num(0.0), "0");
        assert_eq!(opt_num(None), "");
    }

    #[test]
    fn recipes_flatten_children_first_and_once() {
        let sauce = RecipeExport {
            name: "Sauce".into(),
            servings: 1.0,
            ..Default::default()
        };
        let pasta = RecipeExport {
            name: "Pasta".into(),
            servings: 2.0,
            items: vec![RecipeExportItem {
                recipe: Some(Box::new(sauce.clone())),
                servings: Some(1.0),
                ..Default::default()
            }],
            ..Default::default()
        };
        let file = [pasta.clone(), sauce.clone()];
        let mut out = Vec::new();
        flatten_recipes(&file, &mut out, &mut HashSet::new(), 0).unwrap();
        let names: Vec<&str> = out.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Sauce", "Pasta"]);
    }

    #[test]
    fn a_trimmed_file_still_parses() {
        let file: AccountExport =
            serde_json::from_str(r#"{"weights":[{"recorded_on":"2026-01-01","weight_kg":80}]}"#)
                .unwrap();
        assert_eq!(file.format, 0);
        assert!(file.profile.is_none());
        assert_eq!(file.weights.len(), 1);
        assert!(file.recipes.is_empty());
    }
}
