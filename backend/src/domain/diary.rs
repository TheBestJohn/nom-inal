use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use super::nutrients::{EnergyShare, Nutrients};
use super::target::TargetProgress;

#[derive(Debug, FromRow)]
pub struct DiaryRow {
    pub id: Uuid,
    pub logged_on: NaiveDate,
    pub meal: String,
    pub food_id: Option<Uuid>,
    pub recipe_id: Option<Uuid>,
    pub quantity_g: Option<f64>,
    pub recipe_servings: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    pub food_name: Option<String>,
    pub food_brand: Option<String>,
    pub recipe_name: Option<String>,

    /// For a food row: nutrients per 100 g.
    /// For a recipe row: nutrients for ONE serving of the recipe (aggregated in
    /// SQL), which is why both cases can share one scaling step below.
    pub calories_kcal: Option<f64>,
    pub protein_g: Option<f64>,
    pub carbs_g: Option<f64>,
    pub fat_g: Option<f64>,
    pub fiber_g: Option<f64>,
    pub sugar_g: Option<f64>,
    pub saturated_fat_g: Option<f64>,
    pub sodium_mg: Option<f64>,
}

impl DiaryRow {
    fn base(&self) -> Nutrients {
        Nutrients {
            calories_kcal: self.calories_kcal.unwrap_or(0.0),
            protein_g: self.protein_g.unwrap_or(0.0),
            carbs_g: self.carbs_g.unwrap_or(0.0),
            fat_g: self.fat_g.unwrap_or(0.0),
            fiber_g: self.fiber_g.unwrap_or(0.0),
            sugar_g: self.sugar_g.unwrap_or(0.0),
            saturated_fat_g: self.saturated_fat_g.unwrap_or(0.0),
            sodium_mg: self.sodium_mg.unwrap_or(0.0),
        }
    }

    pub fn nutrients(&self) -> Nutrients {
        match (self.quantity_g, self.recipe_servings) {
            // food: base is per 100 g
            (Some(grams), _) => self.base().scaled(grams / 100.0),
            // recipe: base is one serving
            (_, Some(servings)) => self.base().scaled(servings),
            _ => Nutrients::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DiaryEntry {
    pub id: Uuid,
    pub logged_on: NaiveDate,
    pub meal: String,
    pub food_id: Option<Uuid>,
    pub recipe_id: Option<Uuid>,
    pub name: String,
    pub brand: Option<String>,
    pub quantity_g: Option<f64>,
    pub recipe_servings: Option<f64>,
    pub nutrients: Nutrients,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<DiaryRow> for DiaryEntry {
    fn from(row: DiaryRow) -> Self {
        let nutrients = row.nutrients().rounded();
        Self {
            id: row.id,
            logged_on: row.logged_on,
            meal: row.meal.clone(),
            food_id: row.food_id,
            recipe_id: row.recipe_id,
            name: row
                .food_name
                .clone()
                .or_else(|| row.recipe_name.clone())
                .unwrap_or_else(|| "(deleted)".into()),
            brand: row.food_brand.clone(),
            quantity_g: row.quantity_g,
            recipe_servings: row.recipe_servings,
            nutrients,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateDiaryEntryRequest {
    /// Defaults to today.
    pub logged_on: Option<NaiveDate>,
    /// `breakfast` | `lunch` | `dinner` | `snack`.
    pub meal: Option<String>,
    pub food_id: Option<Uuid>,
    pub recipe_id: Option<Uuid>,
    #[validate(range(
        min = 0.1,
        max = 100000.0,
        message = "must be between 0.1 and 100000 g"
    ))]
    pub quantity_g: Option<f64>,
    #[validate(range(min = 0.01, max = 1000.0, message = "must be between 0.01 and 1000"))]
    pub recipe_servings: Option<f64>,
}

#[derive(Debug, Default, Deserialize, Validate, ToSchema)]
#[serde(default)]
pub struct PatchDiaryEntryRequest {
    pub logged_on: Option<NaiveDate>,
    pub meal: Option<String>,
    #[validate(range(
        min = 0.1,
        max = 100000.0,
        message = "must be between 0.1 and 100000 g"
    ))]
    pub quantity_g: Option<f64>,
    #[validate(range(min = 0.01, max = 1000.0, message = "must be between 0.01 and 1000"))]
    pub recipe_servings: Option<f64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MealGroup {
    pub meal: String,
    pub entries: Vec<DiaryEntry>,
    pub total: Nutrients,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DiaryDay {
    pub date: NaiveDate,
    /// Grouped by meal, each with its own total, so a client watching one
    /// nutrient per meal reads it rather than re-adding entries.
    pub meals: Vec<MealGroup>,
    pub total: Nutrients,
    /// Share of the day's energy from protein, carbohydrate and fat.
    pub energy_share: EnergyShare,
    /// Where the day stands against each target the user has set, in display
    /// order. Empty when no targets are set.
    pub targets: Vec<TargetProgress>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DailyTotal {
    pub date: NaiveDate,
    pub total: Nutrients,
    pub entry_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DiarySummary {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub days: Vec<DailyTotal>,
    /// Average across days that actually have entries.
    pub average: Nutrients,
    /// Share of the average day's energy from protein, carbohydrate and fat.
    pub energy_share: EnergyShare,
    pub logged_day_count: i64,
}
