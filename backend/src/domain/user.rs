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
    shown_nutrients, chart_nutrients, chart_mode, tracking_focus, created_at
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
    pub chart_mode: Option<String>,
    /// NULL until the welcome flow has asked; see migration 0015.
    pub tracking_focus: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Why this account is tracking.
///
/// The one fact the rest of the setup can be derived from: which nutrients go
/// on screen, which get charted, and which way each target points. A focus is
/// applied as a preset and never enforced — nothing reads it back to decide
/// what a number means.
///
/// Every variant carries an explicit wire name. `snake_case` would produce the
/// same strings today, but the names are also what the database CHECK allows,
/// and `per_100g` showed that trusting a derive to match a constraint is how
/// a valid choice turns into a 500.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum TrackingFocus {
    #[serde(rename = "general")]
    General,
    #[serde(rename = "weight_loss")]
    WeightLoss,
    #[serde(rename = "muscle_gain")]
    MuscleGain,
    #[serde(rename = "keto")]
    Keto,
    #[serde(rename = "diabetes")]
    Diabetes,
    #[serde(rename = "blood_pressure")]
    BloodPressure,
    #[serde(rename = "heart_health")]
    HeartHealth,
    /// "None of these": the account chose to set things up by hand. A real
    /// answer, which is why it is stored rather than left NULL.
    #[serde(rename = "custom")]
    Custom,
}

pub const ALL_FOCUSES: [TrackingFocus; 8] = [
    TrackingFocus::General,
    TrackingFocus::WeightLoss,
    TrackingFocus::MuscleGain,
    TrackingFocus::Keto,
    TrackingFocus::Diabetes,
    TrackingFocus::BloodPressure,
    TrackingFocus::HeartHealth,
    TrackingFocus::Custom,
];

impl TrackingFocus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::General => "general",
            Self::WeightLoss => "weight_loss",
            Self::MuscleGain => "muscle_gain",
            Self::Keto => "keto",
            Self::Diabetes => "diabetes",
            Self::BloodPressure => "blood_pressure",
            Self::HeartHealth => "heart_health",
            Self::Custom => "custom",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        ALL_FOCUSES.into_iter().find(|f| f.as_str() == s)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::General => "General health",
            Self::WeightLoss => "Weight loss",
            Self::MuscleGain => "Muscle gain",
            Self::Keto => "Keto / low-carb",
            Self::Diabetes => "Carb awareness",
            Self::BloodPressure => "Blood pressure",
            Self::HeartHealth => "Heart health",
            Self::Custom => "Custom",
        }
    }

    /// One sentence, for the picker. What the preset puts on screen — never
    /// what anyone should eat.
    pub fn summary(&self) -> &'static str {
        match self {
            Self::General => "Calories and the three macros, with a budget from your estimate.",
            Self::WeightLoss => "A calorie budget below your estimate, with protein kept up.",
            Self::MuscleGain => "A protein goal by body weight and a small calorie surplus.",
            Self::Keto => "A net-carbs budget, with fat as the rest of your energy.",
            Self::Diabetes => "Carbohydrate, sugar and fibre, with per-meal totals in the diary.",
            Self::BloodPressure => "Sodium front and centre, with a budget for it.",
            Self::HeartHealth => "Saturated fat and sodium budgets, and a fibre goal.",
            Self::Custom => "No preset. Choose your own nutrients and targets.",
        }
    }
}

/// How the home page draws the nutrients you follow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChartMode {
    /// Each series as a share of its own goal or budget, all on one axis.
    /// 100% means the same thing on every line, which is what makes them
    /// comparable at all.
    #[default]
    Percent,
    /// The real figures, one small chart per nutrient — they have no shared
    /// scale, so they do not share an axis.
    Actual,
}

impl ChartMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Percent => "percent",
            Self::Actual => "actual",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "percent" => Some(Self::Percent),
            "actual" => Some(Self::Actual),
            _ => None,
        }
    }
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
    /// Which nutrients the home page plots.
    pub chart_nutrients: Vec<Nutrient>,
    pub chart_mode: ChartMode,
    /// Why this account is tracking. `null` until the welcome flow has asked,
    /// which is what the client uses to decide whether to show it.
    pub tracking_focus: Option<TrackingFocus>,
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
            chart_mode: u
                .chart_mode
                .as_deref()
                .and_then(ChartMode::parse)
                .unwrap_or_default(),
            tracking_focus: u.tracking_focus.as_deref().and_then(TrackingFocus::parse),
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
    pub chart_mode: Option<ChartMode>,
    /// Sets the focus alone. `POST /profile/focus` is the way to also apply
    /// what it implies.
    pub tracking_focus: Option<TrackingFocus>,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire names are also the database's CHECK list, so every variant's
    /// serde name, `as_str` and `parse` have to agree exactly.
    #[test]
    fn focus_wire_names_match_the_column_check() {
        let allowed = [
            "general",
            "weight_loss",
            "muscle_gain",
            "keto",
            "diabetes",
            "blood_pressure",
            "heart_health",
            "custom",
        ];
        for (focus, expected) in ALL_FOCUSES.iter().zip(allowed) {
            assert_eq!(focus.as_str(), expected);
            assert_eq!(
                serde_json::to_value(focus).unwrap(),
                serde_json::Value::String(expected.into())
            );
            assert_eq!(TrackingFocus::parse(expected), Some(*focus));
            let parsed: TrackingFocus =
                serde_json::from_value(serde_json::Value::String(expected.into())).unwrap();
            assert_eq!(parsed, *focus);
        }
        assert_eq!(TrackingFocus::parse("Keto"), None);
    }
}
