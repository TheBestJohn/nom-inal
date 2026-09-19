use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use super::nutrients::Nutrients;

/// Which way a target points.
///
/// This is the whole reason targets are more than a number: a protein target
/// and a calorie target are read in opposite directions, and showing both as
/// "percentage consumed, red when exceeded" is wrong for one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TargetKind {
    /// A floor: hit at least this much. Exceeding it is fine.
    Goal,
    /// A ceiling: stay under this much. Exceeding it is over-budget.
    Budget,
}

impl TargetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Goal => "goal",
            Self::Budget => "budget",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "goal" => Some(Self::Goal),
            "budget" => Some(Self::Budget),
            _ => None,
        }
    }
}

/// The nutrients a target can be set on.
///
/// Label, unit, default direction and how to read the value out of a computed
/// total all live here, so adding a nutrient is one variant plus four match
/// arms rather than a hunt through the codebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Nutrient {
    CaloriesKcal,
    ProteinG,
    CarbsG,
    FatG,
    FiberG,
    SugarG,
    SaturatedFatG,
    SodiumMg,
}

/// Display order: calories, then the three macros, then the rest.
/// What the macro readouts show when nobody has said otherwise: the figure
/// people actually budget, and the three macros that make it up.
pub const DEFAULT_SHOWN_NUTRIENTS: [Nutrient; 4] = [
    Nutrient::CaloriesKcal,
    Nutrient::ProteinG,
    Nutrient::CarbsG,
    Nutrient::FatG,
];

/// What the home page plots by default — one line, as it always has.
pub const DEFAULT_CHART_NUTRIENTS: [Nutrient; 1] = [Nutrient::CaloriesKcal];

pub const ALL_NUTRIENTS: [Nutrient; 8] = [
    Nutrient::CaloriesKcal,
    Nutrient::ProteinG,
    Nutrient::CarbsG,
    Nutrient::FatG,
    Nutrient::FiberG,
    Nutrient::SugarG,
    Nutrient::SaturatedFatG,
    Nutrient::SodiumMg,
];

impl Nutrient {
    pub fn key(&self) -> &'static str {
        match self {
            Self::CaloriesKcal => "calories_kcal",
            Self::ProteinG => "protein_g",
            Self::CarbsG => "carbs_g",
            Self::FatG => "fat_g",
            Self::FiberG => "fiber_g",
            Self::SugarG => "sugar_g",
            Self::SaturatedFatG => "saturated_fat_g",
            Self::SodiumMg => "sodium_mg",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        ALL_NUTRIENTS.into_iter().find(|n| n.key() == key)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::CaloriesKcal => "Calories",
            Self::ProteinG => "Protein",
            Self::CarbsG => "Carbs",
            Self::FatG => "Fat",
            Self::FiberG => "Fiber",
            Self::SugarG => "Sugar",
            Self::SaturatedFatG => "Saturated fat",
            Self::SodiumMg => "Sodium",
        }
    }

    pub fn unit(&self) -> &'static str {
        match self {
            Self::CaloriesKcal => "kcal",
            Self::SodiumMg => "mg",
            _ => "g",
        }
    }

    /// How this nutrient is normally tracked, used when the client does not say.
    ///
    /// Protein and fibre are things people try to reach; everything else here
    /// is something people try to stay under. Any of them can be flipped —
    /// carbs are a budget when cutting and a goal when bulking.
    pub fn default_kind(&self) -> TargetKind {
        match self {
            Self::ProteinG | Self::FiberG => TargetKind::Goal,
            _ => TargetKind::Budget,
        }
    }

    pub fn value_in(&self, n: &Nutrients) -> f64 {
        match self {
            Self::CaloriesKcal => n.calories_kcal,
            Self::ProteinG => n.protein_g,
            Self::CarbsG => n.carbs_g,
            Self::FatG => n.fat_g,
            Self::FiberG => n.fiber_g,
            Self::SugarG => n.sugar_g,
            Self::SaturatedFatG => n.saturated_fat_g,
            Self::SodiumMg => n.sodium_mg,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NutritionTarget {
    pub nutrient: Nutrient,
    pub amount: f64,
    pub kind: TargetKind,
    /// Human-readable name, so clients need no nutrient lookup table.
    pub label: &'static str,
    pub unit: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct TargetInput {
    pub nutrient: Nutrient,
    #[validate(range(min = 0.1, max = 100000.0, message = "must be between 0.1 and 100000"))]
    pub amount: f64,
    /// Defaults to the nutrient's usual direction when omitted.
    pub kind: Option<TargetKind>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ReplaceTargetsRequest {
    #[validate(nested)]
    #[validate(length(max = 8, message = "may contain at most one entry per nutrient"))]
    pub targets: Vec<TargetInput>,
}

/// Where the day stands against one target.
///
/// `remaining` is always `amount - consumed`, signed, so a budget that has been
/// blown reads negative and a goal not yet reached reads positive. `status`
/// interprets that sign for the direction, so clients don't re-derive it and
/// risk disagreeing with each other.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TargetProgress {
    pub nutrient: Nutrient,
    pub label: &'static str,
    pub unit: &'static str,
    pub kind: TargetKind,
    pub amount: f64,
    pub consumed: f64,
    pub remaining: f64,
    pub percent: f64,
    /// `under` / `over` for a budget, `short` / `met` for a goal.
    pub status: &'static str,
}

impl TargetProgress {
    pub fn evaluate(nutrient: Nutrient, amount: f64, kind: TargetKind, total: &Nutrients) -> Self {
        let consumed = nutrient.value_in(total);
        let remaining = amount - consumed;

        let status = match kind {
            TargetKind::Budget if remaining < 0.0 => "over",
            TargetKind::Budget => "under",
            TargetKind::Goal if remaining <= 0.0 => "met",
            TargetKind::Goal => "short",
        };

        Self {
            nutrient,
            label: nutrient.label(),
            unit: nutrient.unit(),
            kind,
            amount: round2(amount),
            consumed: round2(consumed),
            remaining: round2(remaining),
            percent: round2(if amount > 0.0 {
                consumed / amount * 100.0
            } else {
                0.0
            }),
            status,
        }
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn totals(calories: f64, protein: f64) -> Nutrients {
        Nutrients {
            calories_kcal: calories,
            protein_g: protein,
            ..Nutrients::default()
        }
    }

    #[test]
    fn a_budget_goes_over_when_exceeded() {
        let t = TargetProgress::evaluate(
            Nutrient::CaloriesKcal,
            2200.0,
            TargetKind::Budget,
            &totals(2500.0, 0.0),
        );
        assert_eq!(t.status, "over");
        assert_eq!(t.remaining, -300.0);
    }

    #[test]
    fn a_budget_stays_under_at_exactly_the_limit() {
        let t = TargetProgress::evaluate(
            Nutrient::CaloriesKcal,
            2200.0,
            TargetKind::Budget,
            &totals(2200.0, 0.0),
        );
        assert_eq!(t.status, "under");
        assert_eq!(t.remaining, 0.0);
    }

    /// The point of the distinction: passing a goal is success, not a warning.
    #[test]
    fn a_goal_is_met_when_exceeded_not_flagged() {
        let t = TargetProgress::evaluate(
            Nutrient::ProteinG,
            160.0,
            TargetKind::Goal,
            &totals(0.0, 190.0),
        );
        assert_eq!(t.status, "met");
        assert_eq!(t.remaining, -30.0);
        assert_eq!(t.percent, 118.75);
    }

    #[test]
    fn a_goal_is_short_until_reached() {
        let t = TargetProgress::evaluate(
            Nutrient::ProteinG,
            160.0,
            TargetKind::Goal,
            &totals(0.0, 120.0),
        );
        assert_eq!(t.status, "short");
        assert_eq!(t.remaining, 40.0);
    }

    #[test]
    fn default_directions_match_how_each_nutrient_is_tracked() {
        assert_eq!(Nutrient::ProteinG.default_kind(), TargetKind::Goal);
        assert_eq!(Nutrient::FiberG.default_kind(), TargetKind::Goal);
        assert_eq!(Nutrient::CaloriesKcal.default_kind(), TargetKind::Budget);
        assert_eq!(Nutrient::SodiumMg.default_kind(), TargetKind::Budget);
    }

    #[test]
    fn nutrient_keys_round_trip() {
        for n in ALL_NUTRIENTS {
            assert_eq!(Nutrient::from_key(n.key()), Some(n));
        }
        assert_eq!(Nutrient::from_key("not_a_nutrient"), None);
    }
}
