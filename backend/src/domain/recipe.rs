use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use super::nutrients::Nutrients;

/// The write paths only need the id back; everything a client reads is
/// assembled by `load_recipe`, which joins the author in.
#[derive(Debug, FromRow)]
pub struct RecipeRow {
    pub id: Uuid,
}

/// One ingredient row, with its contribution already scaled by the query.
///
/// Both kinds of ingredient arrive in the same shape: the SQL resolves a food
/// by grams and a sub-recipe by servings, so nothing downstream has to branch
/// on which it is except to label it.
#[derive(Debug, FromRow)]
pub struct RecipeItemRow {
    pub id: Uuid,
    pub food_id: Option<Uuid>,
    pub sub_recipe_id: Option<Uuid>,
    pub label: Option<String>,
    pub quantity_g: Option<f64>,
    pub servings: Option<f64>,
    pub note: Option<String>,
    pub sort_order: i32,
    /// The food's name, the sub-recipe's, or the free-text label.
    pub name: String,
    /// Only a food has one.
    pub brand: Option<String>,
    pub calories_kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub fiber_g: f64,
    pub sugar_g: f64,
    pub saturated_fat_g: f64,
    pub sodium_mg: f64,
    pub weight_g: f64,
}

impl RecipeItemRow {
    pub fn nutrients(&self) -> Nutrients {
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

#[derive(Debug, Serialize, ToSchema)]
pub struct RecipeItem {
    pub id: Uuid,
    /// Set when this ingredient is a food. Exactly one of this and
    /// `sub_recipe_id` is present.
    pub food_id: Option<Uuid>,
    /// Set when this ingredient is another recipe, taken in servings.
    pub sub_recipe_id: Option<Uuid>,
    /// Set when this ingredient is just words — no nutrition, no database
    /// entry. Its `nutrients` are all zero, by definition rather than by
    /// accident.
    pub label: Option<String>,
    pub name: String,
    pub brand: Option<String>,
    pub quantity_g: Option<f64>,
    pub servings: Option<f64>,
    /// What this ingredient weighs: its grams for a food, and for a
    /// sub-recipe the weight of the servings taken from it.
    pub weight_g: f64,
    pub note: Option<String>,
    pub sort_order: i32,
    pub nutrients: Nutrients,
}

/// A recipe summary (list view) — no ingredient rows, but macros included so
/// the list can be sorted/filtered without an N+1 round trip.
#[derive(Debug, Serialize, ToSchema)]
pub struct RecipeSummary {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub servings: f64,
    /// Shared with every account when true.
    pub is_public: bool,
    /// Whether the caller owns it — only an owner may edit or delete.
    pub is_owner: bool,
    /// Who wrote it, shown on recipes you do not own.
    pub author: Option<String>,
    pub total_weight_g: f64,
    pub item_count: i64,
    /// How many ingredients, counted through any nesting, carry no nutrition.
    /// The macros here are complete only when this is zero.
    pub untracked_count: i64,
    /// The first photo uploaded, for the card. Fetch it with the bearer
    /// token like any photo; it is visible to whoever can see the recipe.
    pub cover_photo_url: Option<String>,
    pub per_serving: Nutrients,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Recipe {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub servings: f64,
    pub is_public: bool,
    pub is_owner: bool,
    pub author: Option<String>,
    pub total_weight_g: f64,
    /// How many ingredients, counted through any nesting, carry no nutrition.
    pub untracked_count: i64,
    pub items: Vec<RecipeItem>,
    pub total: Nutrients,
    pub per_serving: Nutrients,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One ingredient to write: a food in grams, or another recipe in servings.
///
/// The same XOR the diary uses for its entries, and for the same reason — the
/// two quantities mean different things and a row carrying both is not a
/// slightly-wrong recipe, it is an unanswerable one.
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct RecipeItemInput {
    pub food_id: Option<Uuid>,
    /// A one-off ingredient that is just words: "salt and pepper to taste".
    /// Contributes nothing to the macros, and says so on the recipe.
    #[validate(length(min = 1, max = 200, message = "must be 1-200 characters"))]
    pub label: Option<String>,
    /// Include another recipe as an ingredient. It is linked, not copied: its
    /// ingredients stay its own, and correcting it later updates every recipe
    /// built on it.
    pub sub_recipe_id: Option<Uuid>,
    #[validate(range(
        min = 0.1,
        max = 100000.0,
        message = "must be between 0.1 and 100000 g"
    ))]
    pub quantity_g: Option<f64>,
    #[validate(range(
        min = 0.01,
        max = 1000.0,
        message = "must be between 0.01 and 1000 servings"
    ))]
    pub servings: Option<f64>,
    #[validate(length(max = 200, message = "must be at most 200 characters"))]
    pub note: Option<String>,
}

impl RecipeItemInput {
    /// Check the XOR here as well as in the database, so a malformed item comes
    /// back naming what is wrong rather than as a constraint name.
    pub fn check_target(&self) -> Result<(), &'static str> {
        let label = self
            .label
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty());
        let targets = [
            self.food_id.is_some(),
            self.sub_recipe_id.is_some(),
            label.is_some(),
        ];

        match targets.iter().filter(|set| **set).count() {
            0 => return Err("an ingredient needs a food_id, a sub_recipe_id or a label"),
            1 => {}
            _ => return Err("an ingredient is a food, a recipe or a label — only one"),
        }

        if label.is_some() {
            // A free-text ingredient contributes nothing, so a quantity beside
            // it would be a number the totals deliberately ignore.
            if self.quantity_g.is_some() || self.servings.is_some() {
                return Err("a free-text ingredient carries no quantity");
            }
            return Ok(());
        }

        if self.food_id.is_some() {
            if self.quantity_g.is_none() {
                return Err("quantity_g is required for a food ingredient");
            }
            if self.servings.is_some() {
                return Err("a food ingredient is measured in grams, not servings");
            }
        } else {
            if self.servings.is_none() {
                return Err("servings is required for a recipe ingredient");
            }
            if self.quantity_g.is_some() {
                return Err("a recipe ingredient is measured in servings, not grams");
            }
        }

        Ok(())
    }
}

/// Turn one meal of one day into a recipe.
///
/// The entries become the ingredient list as they were logged: a food in its
/// grams, and a logged recipe as a sub-recipe in its servings — linked, not
/// flattened, for the same reason any sub-recipe is.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct FromMealRequest {
    pub date: chrono::NaiveDate,
    /// `breakfast` | `lunch` | `dinner` | `snack`, or a custom meal name.
    pub meal: String,
    #[validate(length(min = 1, max = 200, message = "must be 1-200 characters"))]
    pub name: String,
    /// How many servings the meal was. Defaults to 1: what you ate was one
    /// serving of it.
    #[validate(range(min = 0.1, max = 1000.0, message = "must be between 0.1 and 1000"))]
    pub servings: Option<f64>,
    #[serde(default)]
    pub is_public: bool,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpsertRecipeRequest {
    #[validate(length(min = 1, max = 200, message = "must be 1-200 characters"))]
    pub name: String,
    #[validate(length(max = 2000, message = "must be at most 2000 characters"))]
    pub description: Option<String>,
    #[validate(length(max = 20000, message = "must be at most 20000 characters"))]
    pub instructions: Option<String>,
    #[validate(range(min = 0.1, max = 1000.0, message = "must be between 0.1 and 1000"))]
    pub servings: f64,
    /// Share this recipe with every account. Private by default.
    #[serde(default)]
    pub is_public: bool,
    #[validate(nested)]
    #[validate(length(min = 1, message = "must contain at least one ingredient"))]
    pub items: Vec<RecipeItemInput>,
}
