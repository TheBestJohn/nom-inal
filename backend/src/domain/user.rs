use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use super::target::{Nutrient, DEFAULT_CHART_NUTRIENTS, DEFAULT_SHOWN_NUTRIENTS};

/// Every column `UserRow` reads. Shared for the same reason as `FOOD_COLUMNS`.
pub const USER_COLUMNS: &str = r#"
    id, email, password_hash, display_name, sex, birth_date, height_cm,
    activity_level, goal, target_weight_kg, is_admin, disabled_at,
    shown_nutrients, chart_nutrients, created_at
"#;

#[derive(Debug, FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub sex: Option<String>,
    pub birth_date: Option<NaiveDate>,
    pub height_cm: Option<f64>,
    pub activity_level: String,
    pub goal: String,
    pub target_weight_kg: Option<f64>,
    pub is_admin: bool,
    pub disabled_at: Option<DateTime<Utc>>,
    /// NULL until the account expresses a preference; see the migration.
    pub shown_nutrients: Option<Vec<String>>,
    pub chart_nutrients: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
}

/// Turn a stored list into nutrients, falling back to the default when nobody
/// has chosen and dropping anything unrecognised.
///
/// Resolved here rather than left to each client so every one of them agrees on
/// what "unset" means, and so changing the default reaches accounts that never
/// chose. Unknown names are skipped instead of failing: a column written by
/// some future version should cost a missing column in a readout, not a 500 on
/// the profile endpoint.
fn resolve(stored: Option<&Vec<String>>, fallback: &[Nutrient]) -> Vec<Nutrient> {
    match stored {
        None => fallback.to_vec(),
        Some(list) => list.iter().filter_map(|k| Nutrient::from_key(k)).collect(),
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Profile {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub sex: Option<String>,
    pub birth_date: Option<NaiveDate>,
    pub height_cm: Option<f64>,
    pub activity_level: String,
    pub goal: String,
    pub target_weight_kg: Option<f64>,
    /// Drives the admin area in the UI. The server never trusts it — every
    /// admin route re-checks the flag — but the client needs it to know
    /// whether to render the link at all.
    pub is_admin: bool,
    /// Which nutrients the macro readouts should show. Always resolved — a
    /// client never has to know what the default is.
    pub shown_nutrients: Vec<Nutrient>,
    /// Which nutrients the home page plots, one small chart each.
    pub chart_nutrients: Vec<Nutrient>,
    pub created_at: DateTime<Utc>,
}

impl From<UserRow> for Profile {
    fn from(u: UserRow) -> Self {
        Self {
            id: u.id,
            email: u.email,
            display_name: u.display_name,
            sex: u.sex,
            birth_date: u.birth_date,
            height_cm: u.height_cm,
            activity_level: u.activity_level,
            goal: u.goal,
            target_weight_kg: u.target_weight_kg,
            is_admin: u.is_admin,
            shown_nutrients: resolve(u.shown_nutrients.as_ref(), &DEFAULT_SHOWN_NUTRIENTS),
            chart_nutrients: resolve(u.chart_nutrients.as_ref(), &DEFAULT_CHART_NUTRIENTS),
            created_at: u.created_at,
        }
    }
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RegisterRequest {
    #[validate(email(message = "must be a valid email address"))]
    pub email: String,
    #[validate(length(min = 10, message = "must be at least 10 characters"))]
    pub password: String,
    #[validate(length(min = 1, max = 100, message = "must be 1-100 characters"))]
    pub display_name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    pub user: Profile,
}

#[derive(Debug, Default, Deserialize, Validate, ToSchema)]
#[serde(default)]
pub struct UpdateProfileRequest {
    #[validate(length(min = 1, max = 100, message = "must be 1-100 characters"))]
    pub display_name: Option<String>,
    pub sex: Option<String>,
    pub birth_date: Option<NaiveDate>,
    #[validate(range(min = 50.0, max = 280.0, message = "must be between 50 and 280 cm"))]
    pub height_cm: Option<f64>,
    pub activity_level: Option<String>,
    pub goal: Option<String>,
    #[validate(range(min = 20.0, max = 500.0, message = "must be between 20 and 500 kg"))]
    pub target_weight_kg: Option<f64>,
    /// Replaces the list outright. An empty array is a valid choice — "show me
    /// nothing" — and is why this cannot simply treat empty as unset.
    pub shown_nutrients: Option<Vec<Nutrient>>,
    pub chart_nutrients: Option<Vec<Nutrient>>,
}

impl UpdateProfileRequest {
    /// The nutrient lists as they should be stored: de-duplicated and in the
    /// application's canonical order, so two accounts that picked the same set
    /// in a different sequence read back identically.
    pub fn nutrient_keys(list: &[Nutrient]) -> Vec<&'static str> {
        super::target::ALL_NUTRIENTS
            .iter()
            .filter(|n| list.contains(n))
            .map(|n| n.key())
            .collect()
    }
}
