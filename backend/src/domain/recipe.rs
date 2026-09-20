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
    /// Set when the food is a preparation variant: "cooked", "drained".
    pub variant_label: Option<String>,
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
    /// For a food that is a preparation variant of another, what makes it
    /// one: "cooked", "drained". Null otherwise. Part of the food's identity
    /// in an export, since a variant shares its parent's name and brand.
    pub variant_label: Option<String>,
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

// ---------------------------------------------------------------------------
// Sharing, export and import
// ---------------------------------------------------------------------------

/// A shared recipe as a stranger sees it: the recipe, with its photos, whose
/// URLs point at the public photo route since the reader has no token.
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicRecipe {
    #[serde(flatten)]
    pub recipe: Recipe,
    pub photos: Vec<super::photo::Photo>,
}

/// A food named by its natural identity rather than an id, so a recipe
/// export can be read on another instance. `key` is the same string the
/// foods export uses (`FoodExport::variant_of_key`), and is what an import
/// looks the food up by; name and brand are there for a person reading the
/// file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct FoodRef {
    pub key: String,
    pub name: String,
    pub brand: Option<String>,
    /// A variant shares its parent's key; this is what tells them apart,
    /// exactly as `variant_label` does beside `variant_of_key` in the foods
    /// export.
    #[serde(default)]
    pub variant_label: Option<String>,
}

/// One exported ingredient. Exactly one of `food`, `recipe` and `label` is
/// set, mirroring `RecipeItem`: a food by natural key with its grams, a
/// sub-recipe inlined with its own items and the servings taken of it, or
/// a free-text ingredient.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(default)]
pub struct RecipeExportItem {
    pub food: Option<FoodRef>,
    pub quantity_g: Option<f64>,
    /// A sub-recipe, inlined. The schema is recursive here, and utoipa has
    /// to be told so or it collects schemas forever.
    #[schema(no_recursion)]
    pub recipe: Option<Box<RecipeExport>>,
    pub servings: Option<f64>,
    pub label: Option<String>,
    pub note: Option<String>,
}

/// A recipe with no internal ids in it: sub-recipes inlined by name, foods
/// referenced by natural key, free text kept as text. What `GET
/// /recipes/{id}/export?format=json` returns and what the account export
/// carries, and therefore what the account import reads.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(default)]
pub struct RecipeExport {
    pub name: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub servings: f64,
    /// Whether it was shared. Kept in the file so an account restore puts a
    /// shared recipe back as shared; a recipe export alone does not need it,
    /// and an importer that ignores it gets a private recipe.
    pub is_public: bool,
    pub items: Vec<RecipeExportItem>,
}

/// The document `GET /recipes/{id}/export` returns.
#[derive(Debug, Serialize, ToSchema)]
pub struct RecipeExportBundle {
    /// Format version of this document, not of the application.
    pub format: u32,
    pub generated_at: DateTime<Utc>,
    pub recipe: RecipeExport,
}

impl RecipeExport {
    /// The recipe as Markdown: what someone pastes into a note or prints.
    ///
    /// Nutrition is per serving and comes from the caller, since the export
    /// shape deliberately carries no computed figures; a file with totals in
    /// it would go stale the moment a food was corrected.
    pub fn to_markdown(&self, per_serving: &Nutrients, untracked: i64) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", self.name.trim()));
        if let Some(d) = self
            .description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            out.push_str(d);
            out.push_str("\n\n");
        }
        out.push_str(&format!(
            "Makes {} serving{}.\n\n",
            trim_float(self.servings),
            plural(self.servings)
        ));

        out.push_str("## Ingredients\n\n");
        write_items(&mut out, &self.items, 0);
        out.push('\n');

        let steps = self
            .instructions
            .as_deref()
            .map(super::recipe_text::instruction_steps)
            .unwrap_or_default();
        if !steps.is_empty() {
            out.push_str("## Method\n\n");
            for (i, step) in steps.iter().enumerate() {
                out.push_str(&format!("{}. {}\n", i + 1, step));
            }
            out.push('\n');
        }

        out.push_str("## Nutrition per serving\n\n");
        out.push_str(
            "| Calories | Protein | Carbs | Net carbs | Fat | Fiber | Sugar | Sat. fat | Sodium |\n",
        );
        out.push_str("|---|---|---|---|---|---|---|---|---|\n");
        out.push_str(&format!(
            "| {} kcal | {} g | {} g | {} g | {} g | {} g | {} g | {} g | {} mg |\n",
            per_serving.calories_kcal.round(),
            r1(per_serving.protein_g),
            r1(per_serving.carbs_g),
            r1(per_serving.net_carbs_g()),
            r1(per_serving.fat_g),
            r1(per_serving.fiber_g),
            r1(per_serving.sugar_g),
            r1(per_serving.saturated_fat_g),
            per_serving.sodium_mg.round(),
        ));
        if untracked > 0 {
            out.push_str(&format!(
                "\nExcludes {untracked} ingredient{} with no nutrition information.\n",
                if untracked == 1 { "" } else { "s" }
            ));
        }
        out
    }
}

fn write_items(out: &mut String, items: &[RecipeExportItem], indent: usize) {
    let pad = "  ".repeat(indent);
    for item in items {
        if let Some(food) = &item.food {
            let brand = food
                .brand
                .as_deref()
                .map(|b| format!(" ({b})"))
                .unwrap_or_default();
            out.push_str(&format!(
                "{pad}- {} g {}{brand}{}\n",
                trim_float(item.quantity_g.unwrap_or_default()),
                food.name,
                note(item)
            ));
        } else if let Some(sub) = &item.recipe {
            let servings = item.servings.unwrap_or(1.0);
            out.push_str(&format!(
                "{pad}- {} serving{} of {}{}\n",
                trim_float(servings),
                plural(servings),
                sub.name,
                note(item)
            ));
            write_items(out, &sub.items, indent + 1);
        } else if let Some(label) = &item.label {
            out.push_str(&format!("{pad}- {label}{}\n", note(item)));
        }
    }
}

fn note(item: &RecipeExportItem) -> String {
    item.note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| format!(" — {n}"))
        .unwrap_or_default()
}

fn plural(n: f64) -> &'static str {
    if n == 1.0 {
        ""
    } else {
        "s"
    }
}

/// "2" rather than "2.0", "1.5" rather than "1.500000".
fn trim_float(v: f64) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn r1(v: f64) -> String {
    trim_float((v * 10.0).round() / 10.0)
}

/// `POST /recipes/import`.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ImportRecipeRequest {
    /// The page to read. Only http and https, and only public addresses.
    #[validate(length(min = 1, max = 2048, message = "must be 1-2048 characters"))]
    pub url: String,
}

/// A food that might be what an ingredient line means, with enough of its
/// figures for a client to show a running total before anything is saved.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DraftCandidate {
    pub food_id: Uuid,
    pub name: String,
    pub brand: Option<String>,
    pub serving_size_g: f64,
    /// Per 100 g, as stored.
    pub calories_kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    /// How the search found it: `exact`, `prefix`, `contains` or `fuzzy`.
    /// A client should only pre-select a candidate from the first two —
    /// the same rule the food picker applies to its tiers.
    pub tier: &'static str,
}

/// One ingredient line of a draft: the line as written, what was read off
/// it, and the foods it might be. Nothing here is saved.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DraftLine {
    pub text: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    /// The ingredient with its measure removed, as searched for.
    pub name: String,
    /// Grams, when the line said a mass; null for a household measure,
    /// which the client has to ask about rather than guess.
    pub grams: Option<f64>,
    pub candidates: Vec<DraftCandidate>,
}

/// What `POST /recipes/import` returns: a recipe read off a page, resolved
/// as far as it can be without a person, and not yet written anywhere.
#[derive(Debug, Serialize, ToSchema)]
pub struct RecipeDraft {
    pub name: String,
    pub description: Option<String>,
    pub servings: Option<f64>,
    /// One step per line, ready for the recipe form.
    pub instructions: Option<String>,
    pub lines: Vec<DraftLine>,
    /// Where it came from, and the page's cover image if it named one. The
    /// image is not fetched; a client may show it or ignore it.
    pub source_url: String,
    pub image_url: Option<String>,
    pub author: Option<String>,
}

#[cfg(test)]
mod export_tests {
    use super::*;

    #[test]
    fn markdown_reads_like_a_recipe_card() {
        let sub = RecipeExport {
            name: "Sauce".into(),
            servings: 4.0,
            items: vec![RecipeExportItem {
                food: Some(FoodRef {
                    key: "tomato|".into(),
                    name: "Tomato".into(),
                    brand: None,
                    variant_label: None,
                }),
                quantity_g: Some(400.0),
                ..Default::default()
            }],
            ..Default::default()
        };
        let recipe = RecipeExport {
            name: "Pasta".into(),
            description: Some("Quick.".into()),
            instructions: Some("1. Boil\n2. Toss".into()),
            servings: 2.0,
            is_public: false,
            items: vec![
                RecipeExportItem {
                    food: Some(FoodRef {
                        key: "spaghetti|barilla".into(),
                        name: "Spaghetti".into(),
                        brand: Some("Barilla".into()),
                        variant_label: None,
                    }),
                    quantity_g: Some(200.0),
                    ..Default::default()
                },
                RecipeExportItem {
                    recipe: Some(Box::new(sub)),
                    servings: Some(1.0),
                    ..Default::default()
                },
                RecipeExportItem {
                    label: Some("salt to taste".into()),
                    ..Default::default()
                },
            ],
        };
        let per_serving = Nutrients {
            calories_kcal: 412.4,
            protein_g: 14.25,
            carbs_g: 80.0,
            fat_g: 2.0,
            fiber_g: 5.0,
            sugar_g: 3.0,
            saturated_fat_g: 0.5,
            sodium_mg: 120.0,
        };
        let md = recipe.to_markdown(&per_serving, 1);
        assert_eq!(
            md,
            "# Pasta\n\nQuick.\n\nMakes 2 servings.\n\n## Ingredients\n\n\
             - 200 g Spaghetti (Barilla)\n- 1 serving of Sauce\n  - 400 g Tomato\n- salt to taste\n\n\
             ## Method\n\n1. Boil\n2. Toss\n\n## Nutrition per serving\n\n\
             | Calories | Protein | Carbs | Net carbs | Fat | Fiber | Sugar | Sat. fat | Sodium |\n\
             |---|---|---|---|---|---|---|---|---|\n\
             | 412 kcal | 14.3 g | 80 g | 75 g | 2 g | 5 g | 3 g | 0.5 g | 120 mg |\n\
             \nExcludes 1 ingredient with no nutrition information.\n"
        );
    }
}
