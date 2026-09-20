//! USDA FoodData Central client.
//!
//! FDC reports nutrients per 100 g using stable numeric nutrient ids, which is
//! exactly the basis this app stores, so the mapping is a straight lookup.

use serde::Deserialize;

use crate::domain::food::{ExternalFood, ExternalPortion};
use crate::error::ApiError;

// FoodData Central nutrient ids.
const N_ENERGY_KCAL: i64 = 1008;
const N_ENERGY_KJ: i64 = 1062;
const N_PROTEIN: i64 = 1003;
const N_CARBS: i64 = 1005;
const N_FAT: i64 = 1004;
const N_FIBER: i64 = 1079;
const N_SUGAR: i64 = 2000;
const N_SAT_FAT: i64 = 1258;
const N_SODIUM: i64 = 1093;

const KJ_PER_KCAL: f64 = 4.184;

#[derive(Clone)]
pub struct UsdaClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    foods: Vec<SearchFood>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchFood {
    fdc_id: i64,
    description: String,
    #[serde(default)]
    brand_name: Option<String>,
    #[serde(default)]
    brand_owner: Option<String>,
    #[serde(default)]
    gtin_upc: Option<String>,
    #[serde(default)]
    serving_size: Option<f64>,
    #[serde(default)]
    serving_size_unit: Option<String>,
    #[serde(default)]
    household_serving_full_text: Option<String>,
    #[serde(default)]
    food_nutrients: Vec<SearchNutrient>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchNutrient {
    #[serde(default)]
    nutrient_id: Option<i64>,
    #[serde(default)]
    value: Option<f64>,
}

impl UsdaClient {
    pub fn new(http: reqwest::Client, base_url: String, api_key: Option<String>) -> Self {
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    /// Free-text search. Returns `Ok(None)` when no API key is configured so the
    /// caller can degrade gracefully instead of failing the whole request.
    pub async fn search(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Option<Vec<ExternalFood>>, ApiError> {
        let Some(key) = &self.api_key else {
            return Ok(None);
        };

        let url = format!("{}/foods/search", self.base_url);
        let limit = limit.clamp(1, 50).to_string();

        let resp = self
            .http
            .get(&url)
            .query(&[
                ("api_key", key.as_str()),
                ("query", query),
                ("pageSize", limit.as_str()),
                ("dataType", "Foundation,SR Legacy,Branded"),
            ])
            .send()
            .await
            .map_err(|e| ApiError::UpstreamUnavailable(format!("USDA request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(ApiError::UpstreamUnavailable(format!(
                "USDA returned HTTP {}",
                resp.status()
            )));
        }

        let body: SearchResponse = resp
            .json()
            .await
            .map_err(|e| ApiError::UpstreamUnavailable(format!("USDA response unreadable: {e}")))?;

        Ok(Some(body.foods.into_iter().map(map_food).collect()))
    }

    /// Fetch a single food by its FDC id.
    pub async fn get(&self, fdc_id: &str) -> Result<Option<ExternalFood>, ApiError> {
        let Some(key) = &self.api_key else {
            return Ok(None);
        };

        let url = format!("{}/food/{}", self.base_url, fdc_id);
        let resp = self
            .http
            .get(&url)
            .query(&[("api_key", key.as_str())])
            .send()
            .await
            .map_err(|e| ApiError::UpstreamUnavailable(format!("USDA request failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(ApiError::UpstreamUnavailable(format!(
                "USDA returned HTTP {}",
                resp.status()
            )));
        }

        // The detail endpoint nests the nutrient id one level deeper than search
        // does, so normalise both shapes into the flat form before mapping.
        let raw: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::UpstreamUnavailable(format!("USDA response unreadable: {e}")))?;

        Ok(Some(map_detail(&raw)))
    }
}

fn nutrient(nutrients: &[SearchNutrient], id: i64) -> Option<f64> {
    nutrients
        .iter()
        .find(|n| n.nutrient_id == Some(id))
        .and_then(|n| n.value)
}

fn energy_kcal(nutrients: &[SearchNutrient]) -> f64 {
    if let Some(kcal) = nutrient(nutrients, N_ENERGY_KCAL) {
        return kcal;
    }
    // Some Foundation foods only publish kilojoules.
    nutrient(nutrients, N_ENERGY_KJ)
        .map(|kj| kj / KJ_PER_KCAL)
        .unwrap_or(0.0)
}

/// USDA serving sizes come with a unit; only gram-like units can be trusted as
/// a gram weight, so anything else falls back to the 100 g default.
fn serving_grams(size: Option<f64>, unit: Option<&str>) -> f64 {
    match (size, unit.map(|u| u.to_ascii_lowercase())) {
        (Some(s), Some(u)) if s > 0.0 && (u == "g" || u == "gram" || u == "grm") => s,
        // millilitres: assume ~1 g/ml, which is right for water-like products
        (Some(s), Some(u)) if s > 0.0 && (u == "ml" || u == "mlt") => s,
        _ => 100.0,
    }
}

fn map_food(f: SearchFood) -> ExternalFood {
    let n = &f.food_nutrients;
    ExternalFood {
        source: "usda".into(),
        source_id: f.fdc_id.to_string(),
        name: f.description,
        brand: f.brand_name.or(f.brand_owner),
        upc: f.gtin_upc.filter(|u| !u.trim().is_empty()),
        calories_kcal: energy_kcal(n),
        protein_g: nutrient(n, N_PROTEIN).unwrap_or(0.0),
        carbs_g: nutrient(n, N_CARBS).unwrap_or(0.0),
        fat_g: nutrient(n, N_FAT).unwrap_or(0.0),
        fiber_g: nutrient(n, N_FIBER),
        sugar_g: nutrient(n, N_SUGAR),
        saturated_fat_g: nutrient(n, N_SAT_FAT),
        sodium_mg: nutrient(n, N_SODIUM),
        serving_size_g: serving_grams(f.serving_size, f.serving_size_unit.as_deref()),
        serving_label: f.household_serving_full_text,
        // Search hits are abridged and carry no `foodPortions`; the import
        // fetches the detail record to fill these in.
        portions: Vec::new(),
    }
}

/// USDA's household measures for a food, as `label` + grams.
///
/// The three datasets spell a portion differently. Foundation and Survey
/// foods give a `portionDescription` ("1 cup, chopped"); SR Legacy leaves it
/// blank and puts the unit in `modifier` with `measureUnit` set to
/// "undetermined"; Branded foods have none at all. The label is assembled
/// from whichever parts are present, and anything without a positive
/// `gramWeight` is dropped, because a portion is only useful as a weight.
fn map_portions(raw: &serde_json::Value) -> Vec<ExternalPortion> {
    let Some(list) = raw.get("foodPortions").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let text = |v: Option<&serde_json::Value>| {
        v.and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    let mut out: Vec<ExternalPortion> = Vec::new();
    for item in list {
        let Some(grams) = item.get("gramWeight").and_then(|v| v.as_f64()) else {
            continue;
        };
        if !grams.is_finite() || grams <= 0.0 {
            continue;
        }

        let description =
            text(item.get("portionDescription")).filter(|d| d != "Quantity not specified");
        let label = match description {
            Some(d) => d,
            None => {
                let amount = item.get("amount").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let unit = text(item.get("measureUnit").and_then(|u| u.get("name")))
                    .filter(|u| u != "undetermined");
                let modifier = text(item.get("modifier"));
                let mut label = format_amount(amount);
                match (unit, modifier) {
                    (Some(u), Some(m)) => {
                        label.push(' ');
                        label.push_str(&u);
                        label.push_str(", ");
                        label.push_str(&m);
                    }
                    (Some(u), None) | (None, Some(u)) => {
                        label.push(' ');
                        label.push_str(&u);
                    }
                    (None, None) => continue,
                }
                label
            }
        };

        // The unique key is (food, label), so a duplicate here would fail
        // the whole import over a measure that says nothing new.
        if out.iter().any(|p| p.label.eq_ignore_ascii_case(&label)) {
            continue;
        }
        out.push(ExternalPortion { label, grams });
    }
    out
}

/// "1", "0.5", "1.25": trailing zeros dropped, never more than two decimals.
fn format_amount(amount: f64) -> String {
    let rounded = (amount * 100.0).round() / 100.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

fn map_detail(raw: &serde_json::Value) -> ExternalFood {
    let nutrients: Vec<SearchNutrient> = raw
        .get("foodNutrients")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|item| SearchNutrient {
                    // detail shape: { "nutrient": { "id": 1008 }, "amount": 52 }
                    nutrient_id: item
                        .get("nutrient")
                        .and_then(|n| n.get("id"))
                        .and_then(|v| v.as_i64())
                        .or_else(|| item.get("nutrientId").and_then(|v| v.as_i64())),
                    value: item
                        .get("amount")
                        .and_then(|v| v.as_f64())
                        .or_else(|| item.get("value").and_then(|v| v.as_f64())),
                })
                .collect()
        })
        .unwrap_or_default();

    let str_field = |key: &str| {
        raw.get(key)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .filter(|s| !s.trim().is_empty())
    };

    ExternalFood {
        source: "usda".into(),
        source_id: raw
            .get("fdcId")
            .and_then(|v| v.as_i64())
            .map(|v| v.to_string())
            .unwrap_or_default(),
        name: str_field("description").unwrap_or_else(|| "Unnamed food".into()),
        brand: str_field("brandName").or_else(|| str_field("brandOwner")),
        upc: str_field("gtinUpc"),
        calories_kcal: energy_kcal(&nutrients),
        protein_g: nutrient(&nutrients, N_PROTEIN).unwrap_or(0.0),
        carbs_g: nutrient(&nutrients, N_CARBS).unwrap_or(0.0),
        fat_g: nutrient(&nutrients, N_FAT).unwrap_or(0.0),
        fiber_g: nutrient(&nutrients, N_FIBER),
        sugar_g: nutrient(&nutrients, N_SUGAR),
        saturated_fat_g: nutrient(&nutrients, N_SAT_FAT),
        sodium_mg: nutrient(&nutrients, N_SODIUM),
        serving_size_g: serving_grams(
            raw.get("servingSize").and_then(|v| v.as_f64()),
            raw.get("servingSizeUnit").and_then(|v| v.as_str()),
        ),
        serving_label: str_field("householdServingFullText"),
        portions: map_portions(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_falls_back_to_kilojoules() {
        let n = vec![SearchNutrient {
            nutrient_id: Some(N_ENERGY_KJ),
            value: Some(418.4),
        }];
        assert!((energy_kcal(&n) - 100.0).abs() < 0.001);
    }

    #[test]
    fn maps_a_search_result_to_per_100g_values() {
        // Shape returned by /foods/search: flat `nutrientId` + `value`.
        let raw = serde_json::json!({
            "fdcId": 173944,
            "description": "Bananas, raw",
            "brandOwner": null,
            "servingSize": 118.0,
            "servingSizeUnit": "g",
            "householdServingFullText": "1 medium",
            "foodNutrients": [
                {"nutrientId": 1008, "value": 89.0,  "unitName": "KCAL"},
                {"nutrientId": 1003, "value": 1.09,  "unitName": "G"},
                {"nutrientId": 1005, "value": 22.84, "unitName": "G"},
                {"nutrientId": 1004, "value": 0.33,  "unitName": "G"},
                {"nutrientId": 1079, "value": 2.6,   "unitName": "G"},
                {"nutrientId": 1093, "value": 1.0,   "unitName": "MG"}
            ]
        });
        let parsed: SearchFood = serde_json::from_value(raw).expect("deserializes");
        let food = map_food(parsed);

        assert_eq!(food.source, "usda");
        assert_eq!(food.source_id, "173944");
        assert_eq!(food.calories_kcal, 89.0);
        assert_eq!(food.protein_g, 1.09);
        assert_eq!(food.carbs_g, 22.84);
        assert_eq!(food.fiber_g, Some(2.6));
        assert_eq!(food.sodium_mg, Some(1.0));
        assert_eq!(food.serving_size_g, 118.0);
        assert_eq!(food.serving_label.as_deref(), Some("1 medium"));
    }

    #[test]
    fn maps_the_nested_detail_shape_too() {
        // /food/{id} nests the id under `nutrient` and names the value `amount`.
        let raw = serde_json::json!({
            "fdcId": 173944,
            "description": "Bananas, raw",
            "brandName": "Acme",
            "gtinUpc": "0001112223334",
            "servingSize": 1.0,
            "servingSizeUnit": "cup",
            "foodNutrients": [
                {"nutrient": {"id": 1008}, "amount": 89.0},
                {"nutrient": {"id": 1003}, "amount": 1.09},
                {"nutrient": {"id": 2000}, "amount": 12.23}
            ]
        });
        let food = map_detail(&raw);

        assert_eq!(food.name, "Bananas, raw");
        assert_eq!(food.brand.as_deref(), Some("Acme"));
        assert_eq!(food.upc.as_deref(), Some("0001112223334"));
        assert_eq!(food.calories_kcal, 89.0);
        assert_eq!(food.sugar_g, Some(12.23));
        // "cup" is not a gram unit, so the 100 g default stands.
        assert_eq!(food.serving_size_g, 100.0);
        // Nutrients the payload omits stay absent rather than becoming 0.
        assert_eq!(food.fiber_g, None);
    }

    #[test]
    fn portions_are_labelled_from_whichever_parts_the_dataset_gives() {
        let raw = serde_json::json!({
            "fdcId": 1,
            "description": "Test",
            "foodPortions": [
                // Foundation / Survey: a description ready to use.
                {"gramWeight": 240.0, "amount": 1, "portionDescription": "1 cup",
                 "measureUnit": {"name": "cup"}},
                // SR Legacy: unit undetermined, the measure in `modifier`.
                {"gramWeight": 15.0, "amount": 1, "portionDescription": "",
                 "measureUnit": {"name": "undetermined"}, "modifier": "tbsp"},
                // Unit and modifier both present.
                {"gramWeight": 120.0, "amount": 0.5, "measureUnit": {"name": "cup"},
                 "modifier": "chopped"},
                // The placeholder USDA uses when it does not know.
                {"gramWeight": 100.0, "amount": 1, "portionDescription": "Quantity not specified",
                 "measureUnit": {"name": "undetermined"}},
                // A weightless measure is no use as a portion.
                {"gramWeight": 0.0, "amount": 1, "portionDescription": "1 pinch"},
                // A repeat of a label already taken.
                {"gramWeight": 245.0, "amount": 1, "portionDescription": "1 CUP"}
            ]
        });
        let portions = map_detail(&raw).portions;
        let labels: Vec<(&str, f64)> = portions
            .iter()
            .map(|p| (p.label.as_str(), p.grams))
            .collect();
        assert_eq!(
            labels,
            vec![
                ("1 cup", 240.0),
                ("1 tbsp", 15.0),
                ("0.5 cup, chopped", 120.0)
            ]
        );
    }

    #[test]
    fn a_record_without_portions_imports_with_none() {
        let raw = serde_json::json!({"fdcId": 1, "description": "Plain"});
        assert!(map_detail(&raw).portions.is_empty());
    }

    #[test]
    fn non_gram_serving_units_fall_back_to_100g() {
        assert_eq!(serving_grams(Some(1.0), Some("cup")), 100.0);
        assert_eq!(serving_grams(Some(30.0), Some("g")), 30.0);
        assert_eq!(serving_grams(None, None), 100.0);
    }
}
