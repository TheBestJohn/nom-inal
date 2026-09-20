use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use super::nutrients::Nutrients;

/// Every column `Food` reads, in one place, qualified by a table name.
///
/// This list had been copied into three route modules, and adding the
/// provenance columns broke two of them at runtime — `query_as` is checked
/// against the database, not the compiler, so a stale list is a 500 rather
/// than a build error. Defining it beside the struct means the next column
/// only has to be added once.
///
/// It is a macro rather than a string because the last column is not a
/// column: `portions` is the food's household measures, aggregated from
/// `food_portions` in a correlated subquery so a food arrives whole from a
/// list, a detail read or a `RETURNING` clause alike. That subquery has to
/// name the row it belongs to, and the two statements that alias `foods` as
/// `f` cannot say `foods.id`, so the table name is a parameter.
#[rustfmt::skip]
macro_rules! food_columns {
    ($t:literal) => {
        concat!(
            $t, ".id, ", $t, ".source, ", $t, ".source_id, ", $t, ".name, ", $t, ".brand, ",
            $t, ".upc, ", $t, ".calories_kcal, ", $t, ".protein_g, ", $t, ".carbs_g, ",
            $t, ".fat_g, ", $t, ".fiber_g, ", $t, ".sugar_g, ", $t, ".saturated_fat_g, ",
            $t, ".sodium_mg, ", $t, ".serving_size_g, ", $t, ".serving_label, ",
            $t, ".variant_of, ", $t, ".variant_label, ", $t, ".revision, ",
            $t, ".verified_at, ", $t, ".disputed_at, ", $t, ".nutrient_basis, ",
            $t, ".created_by, ", $t, ".created_at, ", $t, ".updated_at, ",
            "(SELECT coalesce(json_agg(json_build_object(",
            "'id', p.id, 'label', p.label, 'grams', p.grams, 'source', p.source) ",
            "ORDER BY p.sort_order, lower(p.label)), '[]'::json) ",
            "FROM food_portions p WHERE p.food_id = ", $t, ".id) AS portions"
        )
    };
}

pub(crate) use food_columns;

/// The list for any statement that reads `foods` under its own name.
pub const FOOD_COLUMNS: &str = food_columns!("foods");

/// A food as stored: all nutrient figures are **per 100 g**.
#[derive(Debug, Clone, Serialize, FromRow, ToSchema)]
pub struct Food {
    pub id: Uuid,
    /// `custom`, `usda` or `off` (Open Food Facts).
    pub source: String,
    pub source_id: Option<String>,
    pub name: String,
    pub brand: Option<String>,
    pub upc: Option<String>,
    pub calories_kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub fiber_g: Option<f64>,
    pub sugar_g: Option<f64>,
    pub saturated_fat_g: Option<f64>,
    pub sodium_mg: Option<f64>,
    pub serving_size_g: f64,
    pub serving_label: Option<String>,
    /// `per_100g` or `per_serving`: the basis this food's numbers were entered
    /// in and should be shown in. Storage is always per 100 g regardless — this
    /// says how to present it, not how it is kept.
    pub nutrient_basis: String,
    /// Set when this food is a preparation variant of another food, e.g. the
    /// cooked form of a raw ingredient.
    pub variant_of: Option<Uuid>,
    /// What distinguishes this variant from its parent: `cooked`, `raw`,
    /// `drained`. Always present exactly when `variant_of` is.
    pub variant_label: Option<String>,
    /// Bumped by the database on every substantive edit. Verification is
    /// scoped to this number, so an edit resets community agreement.
    pub revision: i32,
    /// When the current revision reached quorum. Cleared by any edit.
    pub verified_at: Option<DateTime<Utc>>,
    /// When someone objected to the current revision and nobody has
    /// out-confirmed them. Cached on the row alongside `verified_at` so a list
    /// can distinguish "nobody has checked" from "somebody says this is wrong"
    /// without aggregating votes per result.
    pub disputed_at: Option<DateTime<Utc>>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Household measures of this food — "1 cup", "1 slice" — each with its
    /// weight, so a picker can offer them beside grams. What gets logged is
    /// still grams; a portion is a way of arriving at the number.
    #[sqlx(json)]
    pub portions: Vec<FoodPortion>,
}

/// One household measure of a food.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FoodPortion {
    pub id: Uuid,
    /// As it reads in the picker: "1 cup", "1 medium", "2 tbsp".
    pub label: String,
    pub grams: f64,
    /// `usda` or `off` for a measure the provider published, `user` for one
    /// somebody typed in. A re-import refreshes the former and never the
    /// latter.
    pub source: String,
}

impl Food {
    /// Per-100g profile with unknown nutrients flattened to 0 for arithmetic.
    pub fn per_100g(&self) -> Nutrients {
        Nutrients {
            calories_kcal: self.calories_kcal,
            protein_g: self.protein_g,
            carbs_g: self.carbs_g,
            fat_g: self.fat_g,
            fiber_g: self.fiber_g.unwrap_or(0.0),
            sugar_g: self.sugar_g.unwrap_or(0.0),
            saturated_fat_g: self.saturated_fat_g.unwrap_or(0.0),
            sodium_mg: self.sodium_mg.unwrap_or(0.0),
        }
    }

    /// Nutrients for an arbitrary gram amount of this food.
    pub fn nutrients_for_grams(&self, grams: f64) -> Nutrients {
        self.per_100g().scaled(grams / 100.0)
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FoodDetail {
    #[serde(flatten)]
    pub food: Food,
    /// Convenience: the same numbers for one serving of `serving_size_g`.
    pub per_serving: Nutrients,
    /// Who has touched this entry and how much the community trusts it.
    pub provenance: FoodProvenance,
    /// Preparation variants of this food. Empty when this row is itself a
    /// variant, since variants are deliberately one level deep.
    #[serde(default)]
    pub variants: Vec<Food>,
    /// The food this one is a variant of, when it is one.
    pub parent: Option<Food>,
}

fn default_serving() -> f64 {
    100.0
}

/// Which quantity a set of nutrient figures describes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NutrientBasis {
    /// 100 g of the food. How USDA and Open Food Facts publish, and how
    /// everything is stored.
    ///
    /// Renamed explicitly: `snake_case` derives `per100g` from `Per100g`,
    /// which would not match the string this enum writes to the database or
    /// the value the column's CHECK constraint allows.
    #[serde(rename = "per_100g")]
    #[default]
    Per100g,
    /// One serving of `serving_size_g`. How a nutrition label reads.
    PerServing,
}

impl NutrientBasis {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Per100g => "per_100g",
            Self::PerServing => "per_serving",
        }
    }
}

/// Nutrients converted to the per-100 g basis everything is stored in.
#[derive(Debug, Clone, Copy)]
pub struct Per100g {
    pub calories_kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub fiber_g: Option<f64>,
    pub sugar_g: Option<f64>,
    pub saturated_fat_g: Option<f64>,
    pub sodium_mg: Option<f64>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpsertFoodRequest {
    #[validate(length(min = 1, max = 200, message = "must be 1-200 characters"))]
    pub name: String,
    #[validate(length(max = 200, message = "must be at most 200 characters"))]
    pub brand: Option<String>,
    #[validate(length(min = 6, max = 20, message = "must be 6-20 digits"))]
    pub upc: Option<String>,
    // These are checked after conversion rather than here: when the figures are
    // per serving, the bound that matters is what they work out to per 100 g,
    // and a limit applied to the typed value would reject a perfectly ordinary
    // label (300 kcal in a 90 g serving) while letting a mistyped one through.
    #[validate(range(min = 0.0, message = "cannot be negative"))]
    pub calories_kcal: f64,
    #[validate(range(min = 0.0, message = "cannot be negative"))]
    pub protein_g: f64,
    #[validate(range(min = 0.0, message = "cannot be negative"))]
    pub carbs_g: f64,
    #[validate(range(min = 0.0, message = "cannot be negative"))]
    pub fat_g: f64,
    pub fiber_g: Option<f64>,
    pub sugar_g: Option<f64>,
    pub saturated_fat_g: Option<f64>,
    pub sodium_mg: Option<f64>,
    #[serde(default = "default_serving")]
    #[validate(range(min = 0.1, max = 5000.0, message = "must be between 0.1 and 5000 g"))]
    pub serving_size_g: f64,
    #[validate(length(max = 100, message = "must be at most 100 characters"))]
    pub serving_label: Option<String>,
    /// Make this food a preparation variant of another food.
    pub variant_of: Option<Uuid>,
    /// Required with `variant_of`, rejected without it.
    #[validate(length(min = 1, max = 60, message = "must be 1-60 characters"))]
    pub variant_label: Option<String>,
    /// What the figures above describe. Defaults to `per_100g`, so a client
    /// that predates this field keeps its existing meaning.
    #[serde(default)]
    pub nutrient_basis: NutrientBasis,
    /// Free-text note stored on the revision this write creates — the edit
    /// summary line of a wiki, not a field on the food itself.
    #[validate(length(max = 300, message = "must be at most 300 characters"))]
    pub edit_summary: Option<String>,
}

/// Per-100 g ceilings. A gram figure cannot exceed the 100 g it describes, and
/// nothing edible reaches 900 kcal per 100 g (pure fat is ~884).
const MAX_PER_100G: [(&str, f64); 7] = [
    ("calories_kcal", 900.0),
    ("protein_g", 100.0),
    ("carbs_g", 100.0),
    ("fat_g", 100.0),
    ("fiber_g", 100.0),
    ("sugar_g", 100.0),
    ("saturated_fat_g", 100.0),
];

/// Sodium is milligrams, so it needs its own bound. Table salt is about
/// 38,750 mg per 100 g and is the saltiest thing anyone logs.
const MAX_SODIUM_MG: f64 = 50_000.0;

impl UpsertFoodRequest {
    /// Convert the submitted figures to the per-100 g basis used for storage,
    /// and range-check the result.
    ///
    /// The conversion lives on the server rather than in the browser so that
    /// every client — the UI, a script posting straight off a label, a future
    /// importer — gets the same arithmetic and the same rounding, and so the
    /// bounds are enforced against the number actually stored.
    pub fn per_100g(&self) -> Result<Per100g, String> {
        let factor = match self.nutrient_basis {
            NutrientBasis::Per100g => 1.0,
            // Guarded by the `serving_size_g` range validator, which runs first
            // and rejects anything at or below 0.1 g.
            NutrientBasis::PerServing => 100.0 / self.serving_size_g,
        };

        // Rounded so the conversion round-trips: a value shown per serving,
        // re-submitted unchanged and converted back lands on the same stored
        // double, instead of drifting in the last bits and manufacturing a
        // revision that says nothing changed.
        let scale = |v: f64| (v * factor * 1e6).round() / 1e6;

        let out = Per100g {
            calories_kcal: scale(self.calories_kcal),
            protein_g: scale(self.protein_g),
            carbs_g: scale(self.carbs_g),
            fat_g: scale(self.fat_g),
            fiber_g: self.fiber_g.map(scale),
            sugar_g: self.sugar_g.map(scale),
            saturated_fat_g: self.saturated_fat_g.map(scale),
            sodium_mg: self.sodium_mg.map(scale),
        };

        let values = [
            out.calories_kcal,
            out.protein_g,
            out.carbs_g,
            out.fat_g,
            out.fiber_g.unwrap_or(0.0),
            out.sugar_g.unwrap_or(0.0),
            out.saturated_fat_g.unwrap_or(0.0),
        ];

        // `contains` rejects NaN for free: it is in no range, so a payload of
        // `NaN` fails here rather than reaching the database.
        for ((field, max), value) in MAX_PER_100G.iter().zip(values) {
            if !(0.0..=*max).contains(&value) {
                return Err(self.out_of_range(field, value, *max));
            }
        }
        if let Some(sodium) = out.sodium_mg {
            if !(0.0..=MAX_SODIUM_MG).contains(&sodium) {
                return Err(self.out_of_range("sodium_mg", sodium, MAX_SODIUM_MG));
            }
        }

        Ok(out)
    }

    /// Say what the number worked out to, not just that it was rejected.
    ///
    /// When the figures came off a label, an over-range value almost always
    /// means the serving size is wrong, and the converted figure is the clue
    /// that points at it.
    fn out_of_range(&self, field: &str, value: f64, max: f64) -> String {
        let rounded = (value * 10.0).round() / 10.0;
        match self.nutrient_basis {
            NutrientBasis::PerServing => format!(
                "{field} works out to {rounded} per 100 g, over the limit of {max} — \
                 check the serving size of {} g",
                self.serving_size_g
            ),
            NutrientBasis::Per100g => {
                format!("{field} is {rounded} per 100 g, over the limit of {max}")
            }
        }
    }

    /// `variant_of` and `variant_label` are meaningless apart: the database
    /// enforces the pairing too, but catching it here yields a field-level
    /// validation error instead of a constraint name.
    pub fn check_variant(&self) -> Result<(), &'static str> {
        match (
            self.variant_of,
            self.variant_label.as_deref().map(str::trim),
        ) {
            (Some(_), None) | (Some(_), Some("")) => {
                Err("variant_label is required when variant_of is set")
            }
            (None, Some(l)) if !l.is_empty() => {
                Err("variant_of is required when variant_label is set")
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct FoodSearchQuery {
    /// Free-text search over name and brand.
    pub q: Option<String>,
    /// Restrict to a source: `custom`, `usda`, `off`.
    pub source: Option<String>,
    /// Only foods this user created.
    #[serde(default)]
    pub mine: bool,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    25
}

/// A hit from an external provider that has not been imported yet. It carries
/// everything needed to import it, so the client can POST it straight back.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExternalFood {
    pub source: String,
    pub source_id: String,
    pub name: String,
    pub brand: Option<String>,
    pub upc: Option<String>,
    pub calories_kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub fiber_g: Option<f64>,
    pub sugar_g: Option<f64>,
    pub saturated_fat_g: Option<f64>,
    pub sodium_mg: Option<f64>,
    pub serving_size_g: f64,
    pub serving_label: Option<String>,
    /// Household measures the provider publishes. USDA's detail record
    /// carries them as `foodPortions`; a search hit does not, and a client
    /// that predates this field sends nothing, so the import fills them in
    /// from the detail record itself when they are missing.
    #[serde(default)]
    pub portions: Vec<ExternalPortion>,
}

/// A provider's household measure, before it has a row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ExternalPortion {
    pub label: String,
    pub grams: f64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ExternalSearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExternalSearchResponse {
    pub results: Vec<ExternalFood>,
    /// Providers that were skipped, with the reason (e.g. no USDA API key).
    pub unavailable: Vec<String>,
}

/// Result of a barcode lookup: either an already-imported local food or a
/// candidate from Open Food Facts the user can choose to import.
#[derive(Debug, Serialize, ToSchema)]
pub struct BarcodeLookup {
    pub upc: String,
    pub local: Option<FoodDetail>,
    pub external: Option<ExternalFood>,
}

/// Something you have logged before, with how you logged it last time.
///
/// Exactly one of `food` and `recipe` is set: a recipe is logged like a food
/// and belongs in the same list, so it is here rather than behind a second
/// endpoint the picker would have to merge by hand.
#[derive(Debug, Serialize, ToSchema)]
pub struct RecentItem {
    pub food: Option<Food>,
    pub recipe: Option<RecentRecipe>,
    /// The grams logged most recently, for a food.
    pub last_quantity_g: Option<f64>,
    /// The servings logged most recently, for a recipe.
    pub last_recipe_servings: Option<f64>,
    pub last_logged_on: NaiveDate,
    /// How many diary entries this item has, ever.
    pub times_logged: i64,
}

/// The part of a recipe the picker needs to log it: its name, and one
/// serving's nutrients for the preview.
#[derive(Debug, Serialize, ToSchema)]
pub struct RecentRecipe {
    pub id: Uuid,
    pub name: String,
    pub servings: f64,
    pub per_serving: Nutrients,
    /// Ingredients, counted through nesting, that carry no nutrition.
    pub untracked_count: i64,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
#[serde(default)]
pub struct RecentQuery {
    /// Page size, 1–50. Defaults to 12.
    pub limit: Option<i64>,
}

// ---------------------------------------------------------------------------
// Provenance: history, contributors and community verification
// ---------------------------------------------------------------------------

/// A vote on a specific revision of a food.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// "I checked this against the label or the source and it is right."
    Confirm,
    /// "These numbers are wrong."
    Dispute,
}

impl Verdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Dispute => "dispute",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "confirm" => Some(Self::Confirm),
            "dispute" => Some(Self::Dispute),
            _ => None,
        }
    }
}

/// How much the community trusts the food's *current* revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum VerificationStatus {
    /// Nobody has weighed in yet, or not enough people have.
    #[default]
    Unverified,
    /// Net confirmations reached the quorum.
    Verified,
    /// At least one person says the numbers are wrong, and nobody has
    /// out-confirmed them. Surfaced loudly: a disputed food is worse than an
    /// unverified one, because someone has actively looked and objected.
    Disputed,
}

impl VerificationStatus {
    /// Disputes are subtracted rather than merely counted so that a single
    /// objection cannot be buried by a couple of casual confirmations: the
    /// quorum has to be cleared *net* of everyone who disagrees.
    pub fn evaluate(confirmations: i64, disputes: i64, quorum: i64) -> Self {
        if disputes > 0 && confirmations <= disputes {
            Self::Disputed
        } else if confirmations - disputes >= quorum {
            Self::Verified
        } else {
            Self::Unverified
        }
    }
}

/// The editorial state of a food: one revision's worth of trust, plus who put
/// the current numbers there.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FoodProvenance {
    pub revision: i32,
    pub status: VerificationStatus,
    pub confirmations: i64,
    pub disputes: i64,
    /// Net confirmations needed for `verified`, so a client can render
    /// "1 of 2" without hard-coding the server's policy.
    pub quorum: i64,
    pub verified_at: Option<DateTime<Utc>>,
    /// Number of distinct people who have ever edited this food.
    pub contributors: i64,
    pub last_change_kind: String,
    pub last_edited_at: DateTime<Utc>,
    pub last_edited_by: Option<Uuid>,
    pub last_edited_by_name: Option<String>,
    pub last_edit_summary: Option<String>,
    /// Your own vote on the current revision, if you cast one.
    pub your_verdict: Option<Verdict>,
    /// False when you wrote the current revision: endorsing your own edit
    /// would make the quorum a formality.
    pub can_verify: bool,
}

/// One historical state of a food. `snapshot` is the full set of editable
/// fields as they stood, so a revision can be rendered or restored without
/// replaying anything.
#[derive(Debug, Clone, Serialize, FromRow, ToSchema)]
pub struct FoodRevision {
    pub id: Uuid,
    pub food_id: Uuid,
    pub revision: i32,
    /// `create`, `edit`, `import`, `revert` or `seed`.
    pub change_kind: String,
    pub edited_by: Option<Uuid>,
    pub edited_by_name: Option<String>,
    pub summary: Option<String>,
    #[schema(value_type = Object)]
    pub snapshot: serde_json::Value,
    pub created_at: DateTime<Utc>,
    /// Field names whose value differs from the revision before this one.
    /// Computed on read so the client can highlight a diff without fetching
    /// two revisions and comparing them itself.
    #[sqlx(skip)]
    #[serde(default)]
    pub changed_fields: Vec<String>,
}

/// One person's vote, for the "who agreed" list on a food.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FoodVerification {
    pub user_id: Uuid,
    pub display_name: String,
    pub revision: i32,
    pub verdict: Verdict,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    /// True when this vote is on the food's current revision. Older votes are
    /// kept and shown, but they no longer count toward the quorum.
    pub current: bool,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct VerifyRequest {
    pub verdict: Verdict,
    /// Why — especially useful on a dispute, where the next editor needs to
    /// know what to fix.
    #[validate(length(max = 300, message = "must be at most 300 characters"))]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RevertRequest {
    /// The revision number to restore. Restoring does not delete history: it
    /// appends a new revision whose content equals the old one, so the bad
    /// edit stays on the record.
    #[validate(range(min = 1, message = "must be a positive revision number"))]
    pub revision: i32,
    #[validate(length(max = 300, message = "must be at most 300 characters"))]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AddPortionRequest {
    /// "1 cup", "1 slice", "1 mug". Unique per food.
    #[validate(length(min = 1, max = 60, message = "must be 1-60 characters"))]
    pub label: String,
    #[validate(range(min = 0.1, max = 50000.0, message = "must be between 0.1 and 50000 g"))]
    pub grams: f64,
}

/// One food in the portable export format. Deliberately not `Food`: internal
/// ids and per-user bookkeeping have no meaning in another deployment, so the
/// export is keyed by natural identity (source + source id, or name + brand)
/// and carries only the facts about the food itself.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct FoodExport {
    pub source: String,
    pub source_id: Option<String>,
    pub name: String,
    pub brand: Option<String>,
    pub upc: Option<String>,
    pub calories_kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub fiber_g: Option<f64>,
    pub sugar_g: Option<f64>,
    pub saturated_fat_g: Option<f64>,
    pub sodium_mg: Option<f64>,
    pub serving_size_g: f64,
    pub serving_label: Option<String>,
    /// Which basis these figures read best in. The values themselves are per
    /// 100 g in the file, as they are in storage.
    pub nutrient_basis: String,
    /// The parent's export key, when this row is a variant. Resolved by name
    /// on import, since ids do not travel.
    pub variant_of_key: Option<String>,
    pub variant_label: Option<String>,
    pub revision: i32,
    pub confirmations: i64,
    pub disputes: i64,
    /// Derived from the counts above and the instance's quorum, so a consumer
    /// need not know what that quorum was.
    #[sqlx(skip)]
    #[serde(default)]
    pub status: VerificationStatus,
    /// Read alongside the counts only to compute `status`. Not exported: a
    /// threshold belonging to the instance that produced the file would mean
    /// nothing to the one reading it.
    #[serde(skip)]
    #[serde(default)]
    pub quorum: i64,
}

/// The whole dataset, shaped to be committed to a git repository: a stable
/// top-level object with a version field, so a future format change is
/// detectable rather than silently mis-parsed.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FoodExportBundle {
    /// Format version of this document, not of the application.
    pub format: u32,
    pub generated_at: DateTime<Utc>,
    pub count: usize,
    pub foods: Vec<FoodExport>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ExportQuery {
    /// Export only foods whose current revision reached quorum — the setting
    /// you want when feeding a curated public dataset.
    #[serde(default)]
    pub verified_only: bool,
}
