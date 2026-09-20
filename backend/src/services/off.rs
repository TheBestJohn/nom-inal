//! Open Food Facts client — the barcode (UPC/EAN) source.
//!
//! OFF is the practical choice for barcodes: USDA's branded dataset carries
//! GTINs but has no lookup-by-barcode endpoint, whereas OFF is keyed by the
//! barcode itself and needs no API key.

use serde::Deserialize;

use crate::domain::food::ExternalFood;
use crate::error::ApiError;

#[derive(Clone)]
pub struct OpenFoodFactsClient {
    http: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct ProductResponse {
    status: i64,
    #[serde(default)]
    product: Option<Product>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    products: Vec<Product>,
}

#[derive(Debug, Deserialize)]
struct Product {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    product_name: Option<String>,
    #[serde(default)]
    generic_name: Option<String>,
    #[serde(default)]
    brands: Option<String>,
    #[serde(default)]
    serving_size: Option<String>,
    #[serde(default)]
    serving_quantity: Option<serde_json::Value>,
    #[serde(default)]
    nutriments: Option<serde_json::Value>,
}

const FIELDS: &str =
    "code,product_name,generic_name,brands,serving_size,serving_quantity,nutriments";

impl OpenFoodFactsClient {
    pub fn new(http: reqwest::Client, base_url: String) -> Self {
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Look up a single product by barcode. `Ok(None)` means "not in OFF",
    /// which is a normal outcome, not an error.
    pub async fn by_barcode(&self, barcode: &str) -> Result<Option<ExternalFood>, ApiError> {
        let url = format!("{}/api/v2/product/{}.json", self.base_url, barcode);

        let resp = self
            .http
            .get(&url)
            .query(&[("fields", FIELDS)])
            .send()
            .await
            .map_err(|e| {
                ApiError::UpstreamUnavailable(format!("Open Food Facts request failed: {e}"))
            })?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(ApiError::UpstreamUnavailable(format!(
                "Open Food Facts returned HTTP {}",
                resp.status()
            )));
        }

        let body: ProductResponse = resp.json().await.map_err(|e| {
            ApiError::UpstreamUnavailable(format!("Open Food Facts response unreadable: {e}"))
        })?;

        if body.status != 1 {
            return Ok(None);
        }

        Ok(body.product.map(map_product))
    }

    pub async fn search(&self, query: &str, limit: i64) -> Result<Vec<ExternalFood>, ApiError> {
        let url = format!("{}/cgi/search.pl", self.base_url);
        let limit = limit.clamp(1, 50).to_string();

        let resp = self
            .http
            .get(&url)
            .query(&[
                ("search_terms", query),
                ("search_simple", "1"),
                ("action", "process"),
                ("json", "1"),
                ("page_size", limit.as_str()),
                ("fields", FIELDS),
            ])
            .send()
            .await
            .map_err(|e| {
                ApiError::UpstreamUnavailable(format!("Open Food Facts request failed: {e}"))
            })?;

        if !resp.status().is_success() {
            return Err(ApiError::UpstreamUnavailable(format!(
                "Open Food Facts returned HTTP {}",
                resp.status()
            )));
        }

        let body: SearchResponse = resp.json().await.map_err(|e| {
            ApiError::UpstreamUnavailable(format!("Open Food Facts response unreadable: {e}"))
        })?;

        Ok(body
            .products
            .into_iter()
            .filter(|p| p.code.is_some())
            .map(map_product)
            .collect())
    }
}

/// OFF stores numbers inconsistently — sometimes as JSON numbers, sometimes as
/// strings — so every read goes through this coercion.
fn num(v: Option<&serde_json::Value>) -> Option<f64> {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn nutriment(nutriments: Option<&serde_json::Value>, key: &str) -> Option<f64> {
    nutriments.and_then(|n| num(n.get(key)))
}

fn map_product(p: Product) -> ExternalFood {
    let nutr = p.nutriments.as_ref();

    // Energy: prefer the explicit kcal field, fall back to kJ.
    let calories = nutriment(nutr, "energy-kcal_100g")
        .or_else(|| nutriment(nutr, "energy-kj_100g").map(|kj| kj / 4.184))
        .or_else(|| nutriment(nutr, "energy_100g").map(|kj| kj / 4.184))
        .unwrap_or(0.0);

    // OFF publishes sodium and salt in grams per 100 g; we store milligrams.
    // If sodium is absent, derive it from salt (salt = sodium x 2.5).
    let sodium_mg = nutriment(nutr, "sodium_100g")
        .map(|g| g * 1000.0)
        .or_else(|| nutriment(nutr, "salt_100g").map(|g| g * 1000.0 / 2.5));

    let name = p
        .product_name
        .filter(|s| !s.trim().is_empty())
        .or(p.generic_name)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Unnamed product".into());

    let serving_size_g = num(p.serving_quantity.as_ref())
        .filter(|v| *v > 0.0)
        .unwrap_or(100.0);

    ExternalFood {
        source: "off".into(),
        source_id: p.code.clone().unwrap_or_default(),
        name,
        brand: p
            .brands
            .map(|b| b.split(',').next().unwrap_or(&b).trim().to_string())
            .filter(|b| !b.is_empty()),
        upc: p.code,
        calories_kcal: calories,
        protein_g: nutriment(nutr, "proteins_100g").unwrap_or(0.0),
        carbs_g: nutriment(nutr, "carbohydrates_100g").unwrap_or(0.0),
        fat_g: nutriment(nutr, "fat_100g").unwrap_or(0.0),
        fiber_g: nutriment(nutr, "fiber_100g"),
        sugar_g: nutriment(nutr, "sugars_100g"),
        saturated_fat_g: nutriment(nutr, "saturated-fat_100g"),
        sodium_mg,
        serving_size_g,
        // Deliberately NOT falling back to `quantity`: that is the package
        // size (e.g. "400 g"), which would read as a serving and mislead.
        serving_label: p.serving_size,
        // OFF publishes one serving, which is already `serving_size_g`
        // above; it has no list of household measures to carry over.
        portions: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn coerces_string_numbers() {
        assert_eq!(num(Some(&json!("12.5"))), Some(12.5));
        assert_eq!(num(Some(&json!(3))), Some(3.0));
        assert_eq!(num(Some(&json!("abc"))), None);
    }

    #[test]
    fn derives_sodium_from_salt_and_converts_to_mg() {
        let p = Product {
            code: Some("123".into()),
            product_name: Some("Test".into()),
            generic_name: None,
            brands: Some("Acme, Other".into()),
            serving_size: None,
            serving_quantity: None,
            nutriments: Some(json!({ "salt_100g": 2.5, "energy-kcal_100g": 200.0 })),
        };
        let food = map_product(p);
        assert_eq!(food.sodium_mg, Some(1000.0));
        assert_eq!(food.calories_kcal, 200.0);
        assert_eq!(food.brand.as_deref(), Some("Acme"));
        assert_eq!(food.serving_size_g, 100.0);
    }
}
