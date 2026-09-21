//! Tools shaped like how people talk.
//!
//! The generated tools are the API, one call per operation. These few are
//! the sentences a person actually says to an assistant — "log two eggs for
//! breakfast", "how am I doing this week" — each of which is several API
//! calls and a decision in between. They are built on the same in-process
//! dispatch as the generated tools, so they inherit the key's scope and the
//! API's validation rather than re-implementing either.
//!
//! The rule throughout is that nothing is guessed silently. A fuzzy match is
//! reported as one and not logged until the caller confirms it; an ingredient
//! line that matches no food becomes a free-text ingredient, named as such.

use chrono::{Duration, NaiveDate, Utc};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::dispatch::{ApiResponse, Credential, Dispatcher};
use crate::auth::CurrentUser;
use crate::domain::measure;

/// A composite tool's description for `tools/list`.
pub struct CompositeTool {
    pub name: &'static str,
    pub description: &'static str,
    pub writes: bool,
    pub input_schema: fn() -> Value,
}

pub const TOOLS: &[CompositeTool] = &[
    CompositeTool {
        name: "log_food",
        description: "Log what someone ate, in their words: \"2 chicken breasts\", \"2 eggs\", \
            \"150 g chicken breast\", \"a banana and 30g oats\". Finds the food in the database and \
            works out the grams: an explicit weight is used as given, and a count is matched \
            against that food's household measures — \"2 breasts\" is 2 × its \"1 breast\" portion — \
            falling back to its serving size when none of them fits. The entry keeps the words as \
            well as the grams, so the diary says \"2 chicken breasts\". Returns what WOULD be logged \
            with its calories; nothing is written unless `confirm` is true, and a match that was \
            only fuzzy is never written without a `food_id` from an earlier confirmation payload. \
            Needs a key with the write scope.",
        writes: true,
        input_schema: log_food_schema,
    },
    CompositeTool {
        name: "today",
        description: "One day of the diary: entries grouped by meal, the day's nutrient totals, \
            and where the day stands against each target (a budget reports what is left before \
            the ceiling, a goal what is still needed). Defaults to today, UTC.",
        writes: false,
        input_schema: today_schema,
    },
    CompositeTool {
        name: "progress",
        description: "How the last N days went: per-day nutrient totals and the average over \
            logged days, plus the weight entries and trend for the same window. Days with \
            nothing logged are missing data, not zero-calorie days, and are excluded from the \
            average.",
        writes: false,
        input_schema: progress_schema,
    },
    CompositeTool {
        name: "add_recipe_from_text",
        description: "Create a recipe from ingredient lines as a person would write them \
            (\"200 g chicken breast\", \"2 eggs\", \"salt and pepper to taste\"). Each line is \
            matched against the food database; a line with no confident match becomes a free-text \
            ingredient that contributes nothing to the macros, and the response says which lines \
            were resolved and which were not. Needs a key with the write scope.",
        writes: true,
        input_schema: add_recipe_schema,
    },
];

fn log_food_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": {
                "type": "string",
                "description": "What was eaten, e.g. \"2 eggs\", \"150 g chicken\", \"banana\". Several items may be separated by commas or \"and\"."
            },
            "meal": {
                "type": "string",
                "description": "breakfast, lunch, dinner or snack."
            },
            "date": {
                "type": "string", "format": "date",
                "description": "YYYY-MM-DD. Defaults to today, UTC."
            },
            "confirm": {
                "type": "boolean",
                "description": "Write the diary entries. Without it the tool only reports what it would log."
            },
            "food_id": {
                "type": "string", "format": "uuid",
                "description": "Skip the search and use this food, typically taken from a previous confirmation payload. Applies when `text` is a single item."
            },
            "grams": {
                "type": "number",
                "description": "Override the quantity in grams, for use with `food_id`."
            }
        },
        "required": ["text", "meal"]
    })
}

fn today_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "date": {"type": "string", "format": "date", "description": "YYYY-MM-DD. Defaults to today, UTC."}
        }
    })
}

fn progress_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "days": {"type": "integer", "minimum": 1, "maximum": 365, "description": "Window length ending today. Defaults to 30."}
        }
    })
}

fn add_recipe_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "servings": {"type": "number", "description": "How many servings the recipe makes."},
            "ingredient_lines": {
                "type": "array", "items": {"type": "string"},
                "description": "One ingredient per line, as written: \"200 g rice\", \"2 eggs\", \"a pinch of salt\"."
            },
            "description": {"type": "string"},
            "instructions": {"type": "string", "description": "The method, as free text."},
            "is_public": {"type": "boolean", "description": "Share with every account on this instance. Private by default."}
        },
        "required": ["name", "servings", "ingredient_lines"]
    })
}

/// A tool that could not do its job. Carries the API's own error when there
/// was one, so the caller sees the same explanation an HTTP client would.
#[derive(Debug)]
pub struct ToolError(pub Value);

impl ToolError {
    fn message(msg: impl Into<String>) -> Self {
        Self(json!({"error": "bad_request", "message": msg.into()}))
    }

    fn from_response(response: &ApiResponse) -> Self {
        Self(match &response.body {
            Value::Object(_) => response.body.clone(),
            other => json!({
                "error": "api_error",
                "status": response.status.as_u16(),
                "message": other,
            }),
        })
    }
}

impl From<String> for ToolError {
    fn from(msg: String) -> Self {
        Self(json!({"error": "internal_error", "message": msg}))
    }
}

pub type ToolResult = Result<Value, ToolError>;

pub async fn call(
    name: &str,
    dispatcher: &Dispatcher,
    credential: &Credential,
    user: &CurrentUser,
    args: Map<String, Value>,
) -> Option<ToolResult> {
    let ctx = Ctx {
        dispatcher,
        credential,
        user,
    };
    Some(match name {
        "log_food" => log_food(&ctx, args).await,
        "today" => today(&ctx, args).await,
        "progress" => progress(&ctx, args).await,
        "add_recipe_from_text" => add_recipe_from_text(&ctx, args).await,
        _ => return None,
    })
}

struct Ctx<'a> {
    dispatcher: &'a Dispatcher,
    credential: &'a Credential,
    user: &'a CurrentUser,
}

impl Ctx<'_> {
    async fn get(&self, path: &str) -> Result<Value, ToolError> {
        let response = self
            .dispatcher
            .send(self.credential, axum::http::Method::GET, path, None)
            .await?;
        if !response.is_success() {
            return Err(ToolError::from_response(&response));
        }
        Ok(response.body)
    }

    async fn post(&self, path: &str, body: &Value) -> Result<Value, ToolError> {
        let response = self
            .dispatcher
            .send(self.credential, axum::http::Method::POST, path, Some(body))
            .await?;
        if !response.is_success() {
            return Err(ToolError::from_response(&response));
        }
        Ok(response.body)
    }
}

// ---------------------------------------------------------------------------
// Parsing "2 eggs" and "150 g chicken"
// ---------------------------------------------------------------------------

/// How much of a food a line asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Amount {
    /// An explicit weight, already in grams.
    Grams(f64),
    /// A count of servings. `unit` is whatever word followed the number
    /// ("slices", "cup") when there was one — it is reported, not
    /// interpreted, because the food's serving size is the only portion the
    /// database knows.
    Count { n: f64, unit: Option<String> },
}

impl Amount {
    /// How many of whatever it is. A weight is one of itself, which is what
    /// makes it safe to ask this of any amount.
    fn count(&self) -> f64 {
        match self {
            Amount::Grams(_) => 1.0,
            Amount::Count { n, .. } => *n,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLine {
    /// The words left over to search for.
    pub food: String,
    pub amount: Amount,
}

/// Grams per unit for the weights people write.
fn mass_unit(word: &str) -> Option<f64> {
    Some(match word.to_ascii_lowercase().as_str() {
        "g" | "gram" | "grams" | "gr" => 1.0,
        "kg" | "kilo" | "kilos" | "kilogram" | "kilograms" => 1000.0,
        "oz" | "ounce" | "ounces" => 28.349_523_125,
        "lb" | "lbs" | "pound" | "pounds" => 453.592_37,
        // Millilitres are taken as grams: right for water and near enough
        // for most drinks, and the payload says the weight it settled on.
        "ml" | "millilitre" | "millilitres" | "milliliter" | "milliliters" => 1.0,
        _ => return None,
    })
}

fn number_word(word: &str) -> Option<f64> {
    Some(match word.to_ascii_lowercase().as_str() {
        "a" | "an" | "one" => 1.0,
        "two" => 2.0,
        "three" => 3.0,
        "four" => 4.0,
        "five" => 5.0,
        "six" => 6.0,
        "seven" => 7.0,
        "eight" => 8.0,
        "nine" => 9.0,
        "ten" => 10.0,
        "half" => 0.5,
        _ => return None,
    })
}

/// Parse a leading number, with a unit that may be glued on: "150g", "1.5kg",
/// "2". Returns the number and whatever letters followed it.
fn split_number(token: &str) -> Option<(f64, &str)> {
    let end = token
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(token.len());
    if end == 0 {
        return None;
    }
    let n: f64 = token[..end].replace(',', ".").parse().ok()?;
    Some((n, &token[end..]))
}

/// Turn "2 eggs", "150 g chicken breast", "chicken 150g", "a banana" into a
/// food to search for and an amount. A line with no number at all is one
/// serving.
pub fn parse_line(text: &str) -> ParsedLine {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.is_empty() {
        return ParsedLine {
            food: String::new(),
            amount: Amount::Count { n: 1.0, unit: None },
        };
    }

    // Leading quantity: "2 eggs", "150 g chicken", "150g chicken", "a banana".
    let leading = split_number(tokens[0])
        .map(|(n, rest)| (n, rest, 1))
        .or_else(|| number_word(tokens[0]).map(|n| (n, "", 1)));

    if let Some((n, glued_unit, mut next)) = leading {
        let mut amount = None;
        if !glued_unit.is_empty() {
            match mass_unit(glued_unit) {
                Some(per) => amount = Some(Amount::Grams(n * per)),
                None => {
                    amount = Some(Amount::Count {
                        n,
                        unit: Some(glued_unit.to_string()),
                    })
                }
            }
        } else if let Some(word) = tokens.get(1) {
            if let Some(per) = mass_unit(word) {
                amount = Some(Amount::Grams(n * per));
                next = 2;
            } else if let Some(per) = plural_or_unit(word) {
                // "2 slices of bread": a unit the database cannot convert.
                // Counted as servings, and the unit is passed back so the
                // caller can see what was assumed.
                amount = Some(Amount::Count {
                    n,
                    unit: Some(per.to_string()),
                });
                next = 2;
            }
        }
        let amount = amount.unwrap_or(Amount::Count { n, unit: None });
        let rest: Vec<&str> = tokens[next..].to_vec();
        return ParsedLine {
            food: strip_of(&rest),
            amount,
        };
    }

    // Trailing quantity: "chicken breast 150 g", "chicken breast 150g".
    if tokens.len() >= 2 {
        let last = tokens[tokens.len() - 1];
        if let Some(per) = mass_unit(last) {
            if let Some((n, "")) = split_number(tokens[tokens.len() - 2]) {
                return ParsedLine {
                    food: tokens[..tokens.len() - 2].join(" "),
                    amount: Amount::Grams(n * per),
                };
            }
        }
        if let Some((n, unit)) = split_number(last) {
            if let Some(per) = mass_unit(unit) {
                return ParsedLine {
                    food: tokens[..tokens.len() - 1].join(" "),
                    amount: Amount::Grams(n * per),
                };
            }
        }
    }

    ParsedLine {
        food: tokens.join(" "),
        amount: Amount::Count { n: 1.0, unit: None },
    }
}

/// Words that name a portion rather than a food. Recognised so "2 slices
/// bread" searches for bread rather than for "slices bread".
fn plural_or_unit(word: &str) -> Option<&'static str> {
    Some(match word.to_ascii_lowercase().as_str() {
        "slice" | "slices" => "slice",
        "cup" | "cups" => "cup",
        "tbsp" | "tablespoon" | "tablespoons" => "tbsp",
        "tsp" | "teaspoon" | "teaspoons" => "tsp",
        "piece" | "pieces" => "piece",
        "scoop" | "scoops" => "scoop",
        "serving" | "servings" | "portion" | "portions" => "serving",
        "can" | "cans" => "can",
        "bottle" | "bottles" => "bottle",
        "glass" | "glasses" => "glass",
        "bowl" | "bowls" => "bowl",
        "handful" | "handfuls" => "handful",
        _ => return None,
    })
}

fn strip_of(tokens: &[&str]) -> String {
    let tokens = match tokens.first() {
        Some(first) if first.eq_ignore_ascii_case("of") => &tokens[1..],
        _ => tokens,
    };
    tokens.join(" ")
}

/// Split "2 eggs and toast" or "banana, 30 g oats" into items. A food name
/// that itself contains "and" is the reason the whole text is tried as one
/// item first (see `log_food`).
pub fn split_items(text: &str) -> Vec<String> {
    text.split(',')
        .flat_map(|part| {
            part.split(" and ")
                .flat_map(|p| p.split(" & "))
                .flat_map(|p| p.split(" + "))
                .collect::<Vec<_>>()
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Resolving words to a food
// ---------------------------------------------------------------------------

/// Tiers the search reports, from tightest to loosest. A tight match is one
/// the name itself contains; anything looser is a guess that has to be
/// confirmed.
const TIGHT_TIERS: &[&str] = &["exact", "prefix", "contains"];

#[derive(Debug, Clone)]
struct Candidate {
    food: Value,
    tier: String,
    own: bool,
}

impl Candidate {
    fn is_tight(&self) -> bool {
        TIGHT_TIERS.contains(&self.tier.as_str())
    }

    fn summary(&self) -> Value {
        json!({
            "food_id": self.food.get("id"),
            "name": self.food.get("name"),
            "brand": self.food.get("brand"),
            "serving_size_g": self.food.get("serving_size_g"),
            "serving_label": self.food.get("serving_label"),
            "match": self.tier,
            "own": self.own,
        })
    }
}

/// Search the food database the way the app's picker does, and rank what
/// comes back: tight tiers before loose ones, and within the tight tiers the
/// caller's own foods first — a food someone entered themselves is the one
/// they mean.
///
/// People count in plurals ("2 eggs") and the database names in singulars
/// ("Egg"), which the trigram tier bridges only as a guess. When nothing
/// tight comes back, the singular is tried too, and a tight hit on it is
/// taken as tight — "eggs" naming "Egg" is not a guess.
async fn search(ctx: &Ctx<'_>, query: &str) -> Result<Vec<Candidate>, ToolError> {
    let mut candidates = search_once(ctx, query).await?;
    if !candidates.iter().any(Candidate::is_tight) {
        if let Some(singular) = singularise(query) {
            let mut tight: Vec<Candidate> = search_once(ctx, &singular)
                .await?
                .into_iter()
                .filter(Candidate::is_tight)
                .collect();
            if !tight.is_empty() {
                tight.append(&mut candidates);
                candidates = tight;
            }
        }
    }
    // The same food can arrive from both searches; the first (tightest)
    // sighting is the one that counts.
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|c| {
        let id = c
            .food
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        seen.insert(id)
    });
    Ok(candidates)
}

/// The singular of the last word, when English gives one: "eggs" → "egg",
/// "berries" → "berry", "tomatoes" → "tomato". `None` when nothing changes.
///
/// One rule, in `domain::measure` beside the plural it undoes — the search
/// uses it to find "Egg" from "2 eggs", and the same function is what lets a
/// portion labelled "1 breast" be recognised in "2 chicken breasts".
pub fn singularise(query: &str) -> Option<String> {
    measure::singularise(query)
}

async fn search_once(ctx: &Ctx<'_>, query: &str) -> Result<Vec<Candidate>, ToolError> {
    let path = format!(
        "/api/v1/search/foods?q={}&limit=5",
        super::dispatch::percent_encode(query)
    );
    let events = ctx.get(&path).await?;

    let mut candidates = Vec::new();
    for event in events.as_array().into_iter().flatten() {
        if event.get("event").and_then(Value::as_str) != Some("tier") {
            continue;
        }
        let tier = event
            .pointer("/data/tier")
            .and_then(Value::as_str)
            .unwrap_or("fuzzy")
            .to_string();
        for food in event
            .pointer("/data/results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let own = food
                .get("created_by")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<Uuid>().ok())
                .is_some_and(|id| id == ctx.user.id);
            candidates.push(Candidate {
                food: food.clone(),
                tier: tier.clone(),
                own,
            });
        }
    }

    // Stable, so the search's own ranking survives within each group.
    candidates.sort_by_key(|c| {
        let tier_rank = TIGHT_TIERS
            .iter()
            .position(|t| *t == c.tier)
            .unwrap_or(TIGHT_TIERS.len());
        let tight = tier_rank < TIGHT_TIERS.len();
        (!tight, if tight { !c.own } else { false }, tier_rank)
    });

    Ok(candidates)
}

/// Fetch one food by id, as the diary would see it.
async fn food_by_id(ctx: &Ctx<'_>, id: &str) -> Result<Value, ToolError> {
    ctx.get(&format!(
        "/api/v1/foods/{}",
        super::dispatch::percent_encode(id)
    ))
    .await
}

fn num(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

/// Per-100 g figures scaled to a weight, mirroring `Food::nutrients_for_grams`
/// over the JSON the API returned.
fn nutrients_for(food: &Value, grams: f64) -> Value {
    let factor = grams / 100.0;
    let scale = |key: &str| ((num(food, key) * factor) * 100.0).round() / 100.0;
    json!({
        "calories_kcal": scale("calories_kcal"),
        "protein_g": scale("protein_g"),
        "carbs_g": scale("carbs_g"),
        "fat_g": scale("fat_g"),
        "fiber_g": scale("fiber_g"),
        "sugar_g": scale("sugar_g"),
        "saturated_fat_g": scale("saturated_fat_g"),
        "sodium_mg": scale("sodium_mg"),
    })
}

/// The words of a phrase, lower case, stripped of punctuation and reduced to
/// the singular, so "2 Chicken Breasts!" and "breast" can be compared.
fn compare_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .map(|w| measure::singularise(&w).unwrap_or(w))
        .collect()
}

/// Whether `needle` appears in `haystack` as consecutive words.
fn contains_words(haystack: &[String], needle: &[String]) -> bool {
    !needle.is_empty()
        && needle.len() <= haystack.len()
        && haystack.windows(needle.len()).any(|w| w == needle)
}

/// The food's own household measure that a counted amount most likely means.
///
/// Two ways in, because people write it both ways. "2 slices of bread" names
/// the measure outright, so the unit the parser kept is matched against the
/// labels. "2 chicken breasts" names it inside the food, so the words of the
/// line are searched for a label instead — "1 breast" is in "chicken
/// breasts" once both are singular.
///
/// Nothing is matched loosely: a food whose measures are "1 large" and
/// "1 medium" answers "2 eggs" with neither, and the serving size takes over
/// as it always did. A wrong measure would be a wrong weight.
fn matching_portion<'a>(food: &'a Value, amount: &Amount, line: &str) -> Option<&'a Value> {
    let Amount::Count { unit, .. } = amount else {
        return None;
    };
    let portions = food.get("portions")?.as_array()?;
    let wanted = compare_words(unit.as_deref().unwrap_or(line));
    if wanted.is_empty() {
        return None;
    }

    portions.iter().find(|portion| {
        let label = portion
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let words = compare_words(measure::measure_of(label));
        portion
            .get("grams")
            .and_then(Value::as_f64)
            .is_some_and(|g| g > 0.0)
            && match unit {
                // "cup" is a match for "1 cup, chopped".
                Some(_) => contains_words(&words, &wanted),
                None => contains_words(&wanted, &words),
            }
    })
}

/// Work out the grams an amount comes to for a given food, and say how.
///
/// Returns the portion it went through, when it went through one, so the
/// entry can be written as that portion rather than as the grams it came to:
/// the diary then remembers "2 chicken breasts" and not only 348 g.
fn grams_for(food: &Value, amount: &Amount, line: &str) -> (f64, String, Option<Value>) {
    match amount {
        Amount::Grams(g) => (*g, format!("{g} g as written"), None),
        Amount::Count { n, unit } => {
            if let Some(portion) = matching_portion(food, amount, line) {
                let grams = num(portion, "grams");
                let label = portion
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let basis =
                    format!("{n} × \"{label}\" ({grams} g), a household measure of this food");
                return (
                    (n * grams * 1000.0).round() / 1000.0,
                    basis,
                    Some(portion.clone()),
                );
            }

            let serving = food
                .get("serving_size_g")
                .and_then(Value::as_f64)
                .filter(|s| *s > 0.0)
                .unwrap_or(100.0);
            let label = food
                .get("serving_label")
                .and_then(Value::as_str)
                .map(|l| format!(" ({l})"))
                .unwrap_or_default();
            let basis = match unit {
                Some(u) => format!(
                    "{n} × {serving} g serving{label}; \"{u}\" is not a measure this food has, so it was counted as servings"
                ),
                None => format!("{n} × {serving} g serving{label}"),
            };
            (n * serving, basis, None)
        }
    }
}

/// What one resolved item will say its amount is, and how to write it.
///
/// A portion is sent to the diary as a portion — the server multiplies it out
/// and keeps the words — so the assistant, the app and a script all arrive at
/// the same entry.
fn amount_fields(item: &mut Map<String, Value>, grams: f64, portion: Option<Value>, count: f64) {
    match portion {
        Some(portion) => {
            let label = portion
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or_default();
            item.insert(
                "portion_id".into(),
                portion.get("id").cloned().unwrap_or(Value::Null),
            );
            item.insert("portion_count".into(), json!(count));
            item.insert(
                "amount".into(),
                json!(measure::portion_phrase(label, count)),
            );
        }
        None => {
            item.insert("amount".into(), json!(measure::grams_phrase(grams)));
        }
    }
}

// ---------------------------------------------------------------------------
// log_food
// ---------------------------------------------------------------------------

fn string_arg(args: &Map<String, Value>, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

async fn log_food(ctx: &Ctx<'_>, args: Map<String, Value>) -> ToolResult {
    let text = string_arg(&args, "text").ok_or_else(|| ToolError::message("text is required"))?;
    let meal = string_arg(&args, "meal")
        .ok_or_else(|| ToolError::message("meal is required: breakfast, lunch, dinner or snack"))?;
    let date = string_arg(&args, "date");
    let confirm = args
        .get("confirm")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let food_id = string_arg(&args, "food_id");
    let grams_override = args.get("grams").and_then(Value::as_f64);

    // Resolve the whole text as one food before splitting it, so "macaroni
    // and cheese" is one dish and "2 eggs and toast" is two.
    let mut items: Vec<Value> = Vec::new();
    let mut all_tight = true;

    if let Some(id) = food_id {
        let food = food_by_id(ctx, &id).await?;
        let parsed = parse_line(&text);
        let (grams, basis, portion) = match grams_override {
            // Grams given outright are the amount, whatever the words said.
            Some(g) => (g, format!("{g} g as given"), None),
            None => grams_for(&food, &parsed.amount, &parsed.food),
        };
        let mut item = json!({
            "text": text,
            "food_id": food.get("id"),
            "name": food.get("name"),
            "brand": food.get("brand"),
            "match": "food_id",
            "grams": grams,
            "basis": basis,
            "nutrients": nutrients_for(&food, grams),
        });
        amount_fields(
            item.as_object_mut().expect("just built as an object"),
            grams,
            portion,
            parsed.amount.count(),
        );
        items.push(item);
    } else {
        let whole = parse_line(&text);
        let whole_match = if whole.food.is_empty() {
            Vec::new()
        } else {
            search(ctx, &whole.food).await?
        };

        let lines: Vec<ParsedLine> = match whole_match.first() {
            Some(first) if first.is_tight() => vec![whole],
            _ => {
                let parts = split_items(&text);
                if parts.len() <= 1 {
                    vec![whole]
                } else {
                    parts.iter().map(|p| parse_line(p)).collect()
                }
            }
        };

        for line in lines {
            let candidates = if lines_equal(&line, &text) && !whole_match.is_empty() {
                whole_match.clone()
            } else {
                search(ctx, &line.food).await?
            };
            match candidates.first() {
                None => {
                    all_tight = false;
                    items.push(json!({
                        "text": line.food,
                        "match": "none",
                        "note": "no food in the database matches; search USDA or Open Food Facts with foods_search_external and import one with foods_import, or add it with foods_create",
                    }));
                }
                Some(best) => {
                    let (grams, basis, portion) = grams_for(&best.food, &line.amount, &line.food);
                    if !best.is_tight() {
                        all_tight = false;
                    }
                    let alternatives: Vec<Value> = candidates
                        .iter()
                        .skip(1)
                        .take(4)
                        .map(Candidate::summary)
                        .collect();
                    let mut item = json!({
                        "text": line.food,
                        "food_id": best.food.get("id"),
                        "name": best.food.get("name"),
                        "brand": best.food.get("brand"),
                        "match": best.tier,
                        "own": best.own,
                        "grams": grams,
                        "basis": basis,
                        "nutrients": nutrients_for(&best.food, grams),
                        "alternatives": alternatives,
                    });
                    amount_fields(
                        item.as_object_mut().expect("just built as an object"),
                        grams,
                        portion,
                        line.amount.count(),
                    );
                    items.push(item);
                }
            }
        }
    }

    // A fold from 0.0 rather than `sum()`, whose empty result is -0.0.
    let total_kcal = items
        .iter()
        .filter_map(|i| {
            i.pointer("/nutrients/calories_kcal")
                .and_then(Value::as_f64)
        })
        .fold(0.0, |acc, kcal| acc + kcal);
    let date_shown = date
        .clone()
        .unwrap_or_else(|| Utc::now().date_naive().to_string());

    if !confirm {
        return Ok(json!({
            "logged": false,
            "meal": meal,
            "date": date_shown,
            "items": items,
            "total_kcal": (total_kcal * 100.0).round() / 100.0,
            "next": "Call again with confirm: true to write these entries. For a single item, pass its food_id (and grams, to adjust) to log exactly that.",
        }));
    }

    if !all_tight {
        return Ok(json!({
            "logged": false,
            "meal": meal,
            "date": date_shown,
            "items": items,
            "total_kcal": (total_kcal * 100.0).round() / 100.0,
            "reason": "at least one item matched only loosely or not at all, so nothing was written; confirm each with its food_id",
        }));
    }

    let mut entries = Vec::new();
    for item in &items {
        let mut body = json!({
            "meal": meal,
            "food_id": item.get("food_id"),
        });
        // An amount that came from a portion is written as that portion, so
        // the entry remembers the words as well as the weight. The server
        // does the multiplication either way; sending both would be refused.
        match (item.get("portion_id"), item.get("portion_count")) {
            (Some(portion_id), Some(count)) if !portion_id.is_null() => {
                body["portion_id"] = portion_id.clone();
                body["portion_count"] = count.clone();
            }
            _ => body["quantity_g"] = item.get("grams").cloned().unwrap_or(Value::Null),
        }
        if let Some(d) = &date {
            body["logged_on"] = json!(d);
        }
        entries.push(ctx.post("/api/v1/diary", &body).await?);
    }

    Ok(json!({
        "logged": true,
        "meal": meal,
        "date": entries.first().and_then(|e| e.get("logged_on")).cloned(),
        "items": items,
        "entries": entries,
        "total_kcal": (total_kcal * 100.0).round() / 100.0,
    }))
}

fn lines_equal(line: &ParsedLine, text: &str) -> bool {
    parse_line(text) == *line
}

// ---------------------------------------------------------------------------
// today / progress
// ---------------------------------------------------------------------------

async fn today(ctx: &Ctx<'_>, args: Map<String, Value>) -> ToolResult {
    let path = match string_arg(&args, "date") {
        Some(date) => format!(
            "/api/v1/diary/day?date={}",
            super::dispatch::percent_encode(&date)
        ),
        None => "/api/v1/diary/day".to_string(),
    };
    ctx.get(&path).await
}

async fn progress(ctx: &Ctx<'_>, args: Map<String, Value>) -> ToolResult {
    let days = args.get("days").and_then(Value::as_i64).unwrap_or(30);
    if !(1..=365).contains(&days) {
        return Err(ToolError::message("days must be between 1 and 365"));
    }
    let to: NaiveDate = Utc::now().date_naive();
    let from = to - Duration::days(days - 1);
    let range = format!("from={from}&to={to}");

    let diary = ctx.get(&format!("/api/v1/diary/summary?{range}")).await?;
    let stats = ctx.get(&format!("/api/v1/weights/stats?{range}")).await?;
    let entries = ctx
        .get(&format!("/api/v1/weights?{range}&limit=365"))
        .await?;

    Ok(json!({
        "from": from,
        "to": to,
        "days": days,
        "diary": diary,
        "weight": { "stats": stats, "entries": entries },
    }))
}

// ---------------------------------------------------------------------------
// add_recipe_from_text
// ---------------------------------------------------------------------------

async fn add_recipe_from_text(ctx: &Ctx<'_>, args: Map<String, Value>) -> ToolResult {
    let name = string_arg(&args, "name").ok_or_else(|| ToolError::message("name is required"))?;
    let servings = args
        .get("servings")
        .and_then(Value::as_f64)
        .ok_or_else(|| ToolError::message("servings is required"))?;
    let lines: Vec<String> = args
        .get("ingredient_lines")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::message("ingredient_lines must be a list of strings"))?
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    if lines.is_empty() {
        return Err(ToolError::message(
            "ingredient_lines must contain at least one line",
        ));
    }

    let mut items = Vec::new();
    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();

    for line in &lines {
        let parsed = parse_line(line);
        let best = if parsed.food.is_empty() {
            None
        } else {
            search(ctx, &parsed.food).await?.into_iter().next()
        };
        match best {
            // Only a match the name itself contains counts. A fuzzy hit on a
            // recipe would be a guess baked into every serving from now on.
            Some(candidate) if candidate.is_tight() => {
                let (grams, basis, portion) =
                    grams_for(&candidate.food, &parsed.amount, &parsed.food);
                let count = parsed.amount.count();
                // An ingredient written as "2 chicken breasts" is stored as
                // that, so the recipe page and the card read the way the line
                // was typed. Either the portion or the grams, never both.
                let mut item = json!({"food_id": candidate.food.get("id")});
                match &portion {
                    Some(portion) => {
                        item["portion_id"] = portion.get("id").cloned().unwrap_or(Value::Null);
                        item["portion_count"] = json!(count);
                    }
                    None => item["quantity_g"] = json!(grams),
                }
                items.push(item);
                let mut record = json!({
                    "line": line,
                    "food_id": candidate.food.get("id"),
                    "name": candidate.food.get("name"),
                    "brand": candidate.food.get("brand"),
                    "match": candidate.tier,
                    "grams": grams,
                    "basis": basis,
                });
                amount_fields(
                    record.as_object_mut().expect("just built as an object"),
                    grams,
                    portion,
                    count,
                );
                resolved.push(record);
            }
            other => {
                items.push(json!({"label": line}));
                unresolved.push(json!({
                    "line": line,
                    "kept_as": "free-text ingredient (contributes nothing to the macros)",
                    "nearest": other.map(|c| c.summary()),
                }));
            }
        }
    }

    let mut body = json!({
        "name": name,
        "servings": servings,
        "items": items,
        "is_public": args.get("is_public").and_then(Value::as_bool).unwrap_or(false),
    });
    if let Some(d) = string_arg(&args, "description") {
        body["description"] = json!(d);
    }
    if let Some(i) = string_arg(&args, "instructions") {
        body["instructions"] = json!(i);
    }

    let recipe = ctx.post("/api/v1/recipes", &body).await?;

    Ok(json!({
        "recipe": recipe,
        "resolved": resolved,
        "unresolved": unresolved,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(n: f64) -> Amount {
        Amount::Count { n, unit: None }
    }

    #[test]
    fn a_leading_count_multiplies_servings() {
        assert_eq!(
            parse_line("2 eggs"),
            ParsedLine {
                food: "eggs".into(),
                amount: count(2.0)
            }
        );
        assert_eq!(
            parse_line("a banana"),
            ParsedLine {
                food: "banana".into(),
                amount: count(1.0)
            }
        );
        assert_eq!(
            parse_line("half an avocado"),
            ParsedLine {
                food: "an avocado".into(),
                amount: count(0.5)
            }
        );
    }

    #[test]
    fn an_explicit_weight_is_grams() {
        assert_eq!(
            parse_line("150 g chicken"),
            ParsedLine {
                food: "chicken".into(),
                amount: Amount::Grams(150.0)
            }
        );
        assert_eq!(
            parse_line("150g chicken breast"),
            ParsedLine {
                food: "chicken breast".into(),
                amount: Amount::Grams(150.0)
            }
        );
        assert_eq!(
            parse_line("30 grams of oats"),
            ParsedLine {
                food: "oats".into(),
                amount: Amount::Grams(30.0)
            }
        );
        assert_eq!(
            parse_line("chicken breast 150 g"),
            ParsedLine {
                food: "chicken breast".into(),
                amount: Amount::Grams(150.0)
            }
        );
        assert_eq!(
            parse_line("rice 200g"),
            ParsedLine {
                food: "rice".into(),
                amount: Amount::Grams(200.0)
            }
        );
    }

    #[test]
    fn other_mass_units_convert_to_grams() {
        match parse_line("1.5 kg potatoes").amount {
            Amount::Grams(g) => assert!((g - 1500.0).abs() < 1e-9),
            other => panic!("{other:?}"),
        }
        match parse_line("4 oz steak").amount {
            Amount::Grams(g) => assert!((g - 113.398).abs() < 0.01),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_portion_word_is_reported_not_converted() {
        assert_eq!(
            parse_line("2 slices of bread"),
            ParsedLine {
                food: "bread".into(),
                amount: Amount::Count {
                    n: 2.0,
                    unit: Some("slice".into())
                }
            }
        );
    }

    #[test]
    fn no_number_means_one_serving() {
        assert_eq!(
            parse_line("banana"),
            ParsedLine {
                food: "banana".into(),
                amount: count(1.0)
            }
        );
    }

    #[test]
    fn plurals_have_a_singular_to_retry_with() {
        assert_eq!(singularise("eggs").as_deref(), Some("egg"));
        assert_eq!(singularise("2 large eggs").as_deref(), Some("2 large egg"));
        assert_eq!(singularise("berries").as_deref(), Some("berry"));
        assert_eq!(singularise("tomatoes").as_deref(), Some("tomato"));
        assert_eq!(singularise("egg"), None);
        assert_eq!(singularise("swiss"), None, "a double s is not a plural");
    }

    #[test]
    fn items_split_on_commas_and_and() {
        assert_eq!(
            split_items("2 eggs and toast, 200 ml milk & a banana"),
            vec!["2 eggs", "toast", "200 ml milk", "a banana"]
        );
        assert_eq!(split_items("  "), Vec::<String>::new());
    }

    #[test]
    fn grams_come_from_the_serving_size_for_a_count() {
        let egg = json!({"serving_size_g": 50.0, "serving_label": "1 large"});
        let (grams, basis, portion) = grams_for(&egg, &count(2.0), "eggs");
        assert_eq!(grams, 100.0);
        assert!(basis.contains("2 × 50 g serving (1 large)"));
        assert!(portion.is_none());

        let (grams, _, _) = grams_for(&egg, &Amount::Grams(150.0), "eggs");
        assert_eq!(grams, 150.0);

        let unknown = json!({});
        let (grams, _, _) = grams_for(&unknown, &count(1.0), "something");
        assert_eq!(
            grams, 100.0,
            "a food with no serving size is taken per 100 g"
        );
    }

    /// The thing this feature is for: the count goes through the food's own
    /// measure, and comes back as the measure rather than as a weight.
    #[test]
    fn a_count_goes_through_the_foods_own_measures_when_the_words_fit() {
        let chicken = json!({
            "serving_size_g": 100.0,
            "portions": [
                {"id": "11111111-1111-1111-1111-111111111111", "label": "1 breast", "grams": 174.0},
                {"id": "22222222-2222-2222-2222-222222222222", "label": "1 cup, diced", "grams": 140.0}
            ]
        });

        // "2 chicken breasts" — the measure is named inside the food.
        let (grams, basis, portion) = grams_for(&chicken, &count(2.0), "chicken breasts");
        assert_eq!(grams, 348.0);
        assert!(basis.contains("\"1 breast\""), "{basis}");
        assert_eq!(portion.unwrap()["label"], "1 breast");

        // "2 cups of chicken" — the parser kept the unit, so it is matched
        // against the labels directly, and "cup" finds "1 cup, diced".
        let cups = Amount::Count {
            n: 2.0,
            unit: Some("cup".into()),
        };
        let (grams, _, portion) = grams_for(&chicken, &cups, "chicken");
        assert_eq!(grams, 280.0);
        assert_eq!(portion.unwrap()["label"], "1 cup, diced");

        // Nothing fits: the serving size takes over rather than a measure
        // being guessed at.
        let (grams, basis, portion) = grams_for(&chicken, &count(2.0), "chicken");
        assert_eq!(grams, 200.0);
        assert!(basis.contains("serving"), "{basis}");
        assert!(portion.is_none());

        // A food with no measures at all is the behaviour that always was.
        let plain = json!({"serving_size_g": 50.0, "portions": []});
        let (grams, _, portion) = grams_for(&plain, &count(2.0), "eggs");
        assert_eq!(grams, 100.0);
        assert!(portion.is_none());
    }

    #[test]
    fn a_measure_is_matched_on_whole_words_only() {
        let bread = json!({
            "serving_size_g": 30.0,
            "portions": [{"id": "33333333-3333-3333-3333-333333333333", "label": "1 slice", "grams": 28.0}]
        });
        // "slices" is the same measure as "slice"...
        let slices = Amount::Count {
            n: 2.0,
            unit: Some("slice".into()),
        };
        assert_eq!(grams_for(&bread, &slices, "bread").0, 56.0);
        // ...and a food whose name merely ends in one is not counted in it.
        assert!(grams_for(&bread, &count(2.0), "sliceable loaf").2.is_none());
    }

    #[test]
    fn nutrients_scale_from_per_100g() {
        let food =
            json!({"calories_kcal": 155.0, "protein_g": 12.6, "carbs_g": 1.1, "fat_g": 10.6});
        let n = nutrients_for(&food, 100.0);
        assert_eq!(n["calories_kcal"], 155.0);
        let n = nutrients_for(&food, 50.0);
        assert_eq!(n["calories_kcal"], 77.5);
        assert_eq!(n["protein_g"], 6.3);
        assert_eq!(n["fiber_g"], 0.0);
    }

    #[test]
    fn composite_schemas_are_objects() {
        for tool in TOOLS {
            let schema = (tool.input_schema)();
            assert_eq!(schema["type"], "object", "{}", tool.name);
        }
    }
}
