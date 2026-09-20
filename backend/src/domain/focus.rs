//! What each tracking focus puts on screen and which targets it suggests.
//!
//! Defined once, here, and served to the client: a preset that could disagree
//! with the readouts is not shipped, and the readouts are computed on this
//! side. The amounts are conservative, mainstream figures with their basis in
//! the comments beside them. None of it is advice — a preset sets nutrition
//! targets and what is on screen, and everything it writes stays editable.

use std::borrow::Cow;

use chrono::NaiveDate;
use serde::Serialize;
use utoipa::ToSchema;

use super::energy::{EnergyEstimate, EnergyInputs};
use super::nutrients::{KCAL_PER_G_CARBS, KCAL_PER_G_FAT, KCAL_PER_G_PROTEIN};
use super::target::{Nutrient, TargetKind};
use super::user::{ChartMode, TrackingFocus};

/// How a target's amount is derived.
#[derive(Debug, Clone, Copy)]
enum Basis {
    /// The energy estimate's goal-adjusted calories.
    Calories,
    /// Grams per kilogram of body weight.
    PerKg(f64),
    /// A share of the calorie budget, converted to grams.
    PctOfCalories { pct: f64, kcal_per_g: f64 },
    /// Grams per 1000 kcal of the budget.
    Per1000Kcal(f64),
    /// A fixed amount, needing nothing from the profile.
    Fixed(f64),
    /// Whatever calories remain once the targets before this one are
    /// accounted for, converted to grams.
    Remainder { kcal_per_g: f64 },
}

#[derive(Debug, Clone, Copy)]
struct Rule {
    nutrient: Nutrient,
    kind: TargetKind,
    basis: Basis,
    /// Why this figure, in one sentence.
    why: &'static str,
}

/// A whole preset: what to show, what to chart, and the rules for targets.
struct Preset {
    shown: &'static [Nutrient],
    chart: &'static [Nutrient],
    /// Some presets also state the profile goal they imply, so the energy
    /// estimate and the focus cannot pull in opposite directions.
    goal: Option<&'static str>,
    rules: Vec<Rule>,
}

// ---- the figures, with their basis ----------------------------------------

/// Protein per kilogram. 1.6 g/kg is where the dose–response for resistance
/// training plateaus in the Morton et al. 2018 meta-analysis; 2.0 g/kg is the
/// upper end of the ISSN 2017 position stand's 1.4–2.0 range, used when lean
/// mass is under pressure (a deficit) or is the point (a surplus). Keto uses a
/// moderate 1.5 g/kg, the middle of the 1.2–1.7 g/kg most protocols cite.
const PROTEIN_G_PER_KG_MAINTAIN: f64 = 1.6;
const PROTEIN_G_PER_KG_CUT_OR_BULK: f64 = 2.0;
const PROTEIN_G_PER_KG_KETO: f64 = 1.5;

/// Fat as a share of calories where it is a budget: 25% sits in the middle of
/// the 20–35% acceptable macronutrient distribution range (IOM DRIs).
const FAT_PCT_OF_KCAL: f64 = 25.0;

/// Fibre scales with intake at the IOM adequate-intake rate of 14 g per
/// 1000 kcal, which is where the familiar 25 g / 38 g figures come from.
const FIBER_G_PER_1000_KCAL: f64 = 14.0;

/// Net carbohydrate on a ketogenic pattern: 20–30 g/day is the range most
/// protocols use to stay in ketosis; 25 g is the middle of it.
const KETO_NET_CARBS_G: f64 = 25.0;

/// Carbohydrate as a share of calories for carb awareness. There is no single
/// prescribed figure; 45% is the lower bound of the 45–65% distribution range
/// and the usual starting point in consistent-carbohydrate meal planning.
const CARB_AWARE_CARBS_PCT_OF_KCAL: f64 = 45.0;

/// Free sugars under 10% of energy is the WHO 2015 guideline.
const SUGAR_PCT_OF_KCAL: f64 = 10.0;

/// Saturated fat under 10% of calories is the Dietary Guidelines figure.
const SATURATED_FAT_PCT_OF_KCAL: f64 = 10.0;

/// Sodium: 2300 mg is the Dietary Guidelines upper limit; 1500 mg is the AHA's
/// ideal for people watching blood pressure. 2000 mg (the WHO figure) sits
/// between them for the blood-pressure focus, and heart health takes the
/// upper limit.
const SODIUM_MG_BLOOD_PRESSURE: f64 = 2000.0;
const SODIUM_MG_HEART: f64 = 2300.0;

fn calories(kind: TargetKind, why: &'static str) -> Rule {
    Rule {
        nutrient: Nutrient::CaloriesKcal,
        kind,
        basis: Basis::Calories,
        why,
    }
}

fn protein(per_kg: f64, why: &'static str) -> Rule {
    Rule {
        nutrient: Nutrient::ProteinG,
        kind: TargetKind::Goal,
        basis: Basis::PerKg(per_kg),
        why,
    }
}

fn fat_pct(why: &'static str) -> Rule {
    Rule {
        nutrient: Nutrient::FatG,
        kind: TargetKind::Budget,
        basis: Basis::PctOfCalories {
            pct: FAT_PCT_OF_KCAL,
            kcal_per_g: KCAL_PER_G_FAT,
        },
        why,
    }
}

fn fiber(why: &'static str) -> Rule {
    Rule {
        nutrient: Nutrient::FiberG,
        kind: TargetKind::Goal,
        basis: Basis::Per1000Kcal(FIBER_G_PER_1000_KCAL),
        why,
    }
}

fn preset(focus: TrackingFocus, profile_goal: &str) -> Preset {
    use Nutrient::*;
    match focus {
        // The suggestion Settings has always made: the estimate as a budget,
        // protein by weight, fat by share, carbs as what is left.
        TrackingFocus::General => Preset {
            shown: &[CaloriesKcal, ProteinG, CarbsG, FatG],
            chart: &[CaloriesKcal],
            goal: None,
            rules: vec![
                calories(
                    TargetKind::Budget,
                    "Your estimated expenditure, adjusted for your goal.",
                ),
                protein(
                    if profile_goal == "cut" {
                        PROTEIN_G_PER_KG_CUT_OR_BULK
                    } else {
                        PROTEIN_G_PER_KG_MAINTAIN
                    },
                    "Per kilogram of body weight; higher on a cut to protect lean mass.",
                ),
                fat_pct("A quarter of calories, the middle of the usual 20–35% range."),
                Rule {
                    nutrient: CarbsG,
                    kind: TargetKind::Budget,
                    basis: Basis::Remainder {
                        kcal_per_g: KCAL_PER_G_CARBS,
                    },
                    why: "The calories left after protein and fat.",
                },
                fiber("14 g per 1000 kcal, the adequate-intake rate."),
            ],
        },
        TrackingFocus::WeightLoss => Preset {
            shown: &[CaloriesKcal, ProteinG, CarbsG, FatG, FiberG],
            chart: &[CaloriesKcal, ProteinG],
            goal: Some("cut"),
            rules: vec![
                calories(
                    TargetKind::Budget,
                    "500 kcal under your estimated expenditure: about half a kilo a week, and never below 1200.",
                ),
                protein(
                    PROTEIN_G_PER_KG_CUT_OR_BULK,
                    "2 g per kilogram: the upper end of the usual range, to keep lean mass through a deficit.",
                ),
                fat_pct("A quarter of calories, the middle of the usual 20–35% range."),
                Rule {
                    nutrient: CarbsG,
                    kind: TargetKind::Budget,
                    basis: Basis::Remainder {
                        kcal_per_g: KCAL_PER_G_CARBS,
                    },
                    why: "The calories left after protein and fat.",
                },
                fiber("14 g per 1000 kcal; fibre is what keeps a smaller intake filling."),
            ],
        },
        TrackingFocus::MuscleGain => Preset {
            shown: &[CaloriesKcal, ProteinG, CarbsG, FatG],
            chart: &[ProteinG, CaloriesKcal],
            goal: Some("bulk"),
            rules: vec![
                calories(
                    TargetKind::Budget,
                    "300 kcal over your estimated expenditure: a surplus small enough to mostly go to muscle.",
                ),
                protein(
                    PROTEIN_G_PER_KG_CUT_OR_BULK,
                    "2 g per kilogram, the top of the 1.6–2.2 g/kg range for building muscle.",
                ),
                fat_pct("A quarter of calories, the middle of the usual 20–35% range."),
                Rule {
                    nutrient: CarbsG,
                    kind: TargetKind::Goal,
                    basis: Basis::Remainder {
                        kcal_per_g: KCAL_PER_G_CARBS,
                    },
                    why: "The calories left after protein and fat, as a goal: carbohydrate fuels the training.",
                },
                fiber("14 g per 1000 kcal, the adequate-intake rate."),
            ],
        },
        TrackingFocus::Keto => Preset {
            shown: &[CaloriesKcal, NetCarbsG, ProteinG, FatG],
            chart: &[NetCarbsG],
            goal: None,
            rules: vec![
                calories(
                    TargetKind::Budget,
                    "Your estimated expenditure, adjusted for your goal.",
                ),
                Rule {
                    nutrient: NetCarbsG,
                    kind: TargetKind::Budget,
                    basis: Basis::Fixed(KETO_NET_CARBS_G),
                    why: "Carbohydrate minus fibre. 20–30 g a day is the range most ketogenic patterns stay within.",
                },
                protein(
                    PROTEIN_G_PER_KG_KETO,
                    "1.5 g per kilogram: moderate, in the middle of the range ketogenic patterns use.",
                ),
                Rule {
                    nutrient: FatG,
                    kind: TargetKind::Goal,
                    basis: Basis::Remainder {
                        kcal_per_g: KCAL_PER_G_FAT,
                    },
                    why: "The calories left after protein and net carbs; on keto, fat is the energy.",
                },
            ],
        },
        TrackingFocus::Diabetes => Preset {
            shown: &[CaloriesKcal, CarbsG, SugarG, FiberG, ProteinG],
            chart: &[CarbsG],
            goal: None,
            rules: vec![
                calories(
                    TargetKind::Budget,
                    "Your estimated expenditure, adjusted for your goal.",
                ),
                Rule {
                    nutrient: CarbsG,
                    kind: TargetKind::Budget,
                    basis: Basis::PctOfCalories {
                        pct: CARB_AWARE_CARBS_PCT_OF_KCAL,
                        kcal_per_g: KCAL_PER_G_CARBS,
                    },
                    why: "45% of calories, the low end of the usual range. The diary shows each meal's share.",
                },
                Rule {
                    nutrient: SugarG,
                    kind: TargetKind::Budget,
                    basis: Basis::PctOfCalories {
                        pct: SUGAR_PCT_OF_KCAL,
                        kcal_per_g: KCAL_PER_G_CARBS,
                    },
                    why: "Under 10% of calories, the WHO guideline for free sugars.",
                },
                fiber("14 g per 1000 kcal, the adequate-intake rate."),
            ],
        },
        TrackingFocus::BloodPressure => Preset {
            shown: &[CaloriesKcal, SodiumMg, SaturatedFatG, FiberG],
            chart: &[SodiumMg],
            goal: None,
            rules: vec![
                Rule {
                    nutrient: SodiumMg,
                    kind: TargetKind::Budget,
                    basis: Basis::Fixed(SODIUM_MG_BLOOD_PRESSURE),
                    why: "Between the 1500 mg ideal and the 2300 mg upper limit.",
                },
                calories(
                    TargetKind::Budget,
                    "Your estimated expenditure, adjusted for your goal.",
                ),
                Rule {
                    nutrient: SaturatedFatG,
                    kind: TargetKind::Budget,
                    basis: Basis::PctOfCalories {
                        pct: SATURATED_FAT_PCT_OF_KCAL,
                        kcal_per_g: KCAL_PER_G_FAT,
                    },
                    why: "Under 10% of calories, the Dietary Guidelines figure.",
                },
                fiber("14 g per 1000 kcal, the adequate-intake rate."),
            ],
        },
        TrackingFocus::HeartHealth => Preset {
            shown: &[CaloriesKcal, SaturatedFatG, FiberG, SodiumMg],
            chart: &[SaturatedFatG, FiberG],
            goal: None,
            rules: vec![
                Rule {
                    nutrient: SaturatedFatG,
                    kind: TargetKind::Budget,
                    basis: Basis::PctOfCalories {
                        pct: SATURATED_FAT_PCT_OF_KCAL,
                        kcal_per_g: KCAL_PER_G_FAT,
                    },
                    why: "Under 10% of calories, the Dietary Guidelines figure.",
                },
                fiber("14 g per 1000 kcal, the adequate-intake rate."),
                Rule {
                    nutrient: SodiumMg,
                    kind: TargetKind::Budget,
                    basis: Basis::Fixed(SODIUM_MG_HEART),
                    why: "The 2300 mg upper limit.",
                },
                calories(
                    TargetKind::Budget,
                    "Your estimated expenditure, adjusted for your goal.",
                ),
            ],
        },
        // No preset: the account sets things up by hand. Shown and charted
        // nutrients are left exactly as they are.
        TrackingFocus::Custom => Preset {
            shown: &[],
            chart: &[],
            goal: None,
            rules: vec![],
        },
    }
}

// ---- evaluation --------------------------------------------------------------

/// One target as the preset would write it. `amount` is absent when the
/// profile lacks what the rule needs, and `needs` says what.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PreviewTarget {
    pub nutrient: Nutrient,
    pub label: &'static str,
    pub unit: &'static str,
    pub kind: TargetKind,
    pub amount: Option<f64>,
    /// Why this figure, with the arithmetic when there is one.
    pub rationale: String,
    /// Profile fields this rule needs and does not have. Empty when `amount`
    /// is present.
    pub needs: Vec<&'static str>,
}

/// What applying a focus would set.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FocusPreview {
    pub focus: TrackingFocus,
    pub label: &'static str,
    pub summary: &'static str,
    /// Targets the preset writes. Order is display order.
    pub targets: Vec<PreviewTarget>,
    pub shown_nutrients: Vec<Nutrient>,
    pub chart_nutrients: Vec<Nutrient>,
    pub chart_mode: ChartMode,
    /// The profile goal this focus sets, when it implies one.
    pub goal: Option<&'static str>,
    /// Profile fields the estimate needs and does not have. Targets that
    /// depend on them are listed above without an amount, not dropped.
    pub missing: Vec<&'static str>,
    pub estimate: Option<EnergyEstimate>,
    /// True when nothing about the display would change: the focus is Custom.
    pub changes_display: bool,
}

/// Round to the precision a suggested target should read at: kcal to the
/// nearest ten, everything else to whole units. A budget of 2187 kcal claims
/// a precision the estimate does not have.
fn round_for(nutrient: Nutrient, v: f64) -> f64 {
    match nutrient {
        Nutrient::CaloriesKcal => (v / 10.0).round() * 10.0,
        _ => v.round(),
    }
}

fn kcal_per_g_of(nutrient: Nutrient) -> Option<f64> {
    match nutrient {
        Nutrient::ProteinG => Some(KCAL_PER_G_PROTEIN),
        Nutrient::CarbsG | Nutrient::NetCarbsG => Some(KCAL_PER_G_CARBS),
        Nutrient::FatG => Some(KCAL_PER_G_FAT),
        _ => None,
    }
}

fn fmt_kcal(v: f64) -> String {
    format!("{} kcal", v.round() as i64)
}

pub fn preview(focus: TrackingFocus, inputs: &EnergyInputs, today: NaiveDate) -> FocusPreview {
    // A focus that implies a goal is estimated with that goal, so the preview
    // shows the numbers apply would write, not the ones the old goal gives.
    let implied_goal = preset(focus, &inputs.goal).goal;
    let inputs: Cow<'_, EnergyInputs> = match implied_goal {
        Some(g) => Cow::Owned(EnergyInputs {
            goal: g.into(),
            ..inputs.clone()
        }),
        None => Cow::Borrowed(inputs),
    };
    let p = preset(focus, &inputs.goal);
    let estimate = inputs.estimate(today);
    let missing = inputs.missing();

    // Every share and remainder is taken from the budget as it will be
    // written, not from the unrounded estimate, so the arithmetic a preview
    // shows adds up to the figures beside it.
    let calories = estimate
        .as_ref()
        .map(|e| round_for(Nutrient::CaloriesKcal, e.calories_kcal));
    // Energy already committed by targets evaluated so far, for remainders.
    let mut committed_kcal = 0.0;
    let mut targets = Vec::with_capacity(p.rules.len());

    for rule in &p.rules {
        let (amount, detail, needs): (Option<f64>, String, Vec<&'static str>) = match rule.basis {
            Basis::Fixed(v) => (Some(v), String::new(), vec![]),
            Basis::PerKg(per_kg) => match inputs.weight_kg {
                Some(w) => (Some(per_kg * w), format!(" {per_kg} g × {w} kg."), vec![]),
                None => (None, String::new(), vec!["weight"]),
            },
            Basis::Calories => match &estimate {
                Some(e) => {
                    let floored = if e.floored_at_minimum {
                        " Held at the 1200 kcal floor."
                    } else {
                        ""
                    };
                    (
                        calories,
                        format!(
                            " {} expenditure {:+} kcal, to the nearest ten.{}",
                            fmt_kcal(e.tdee_kcal),
                            e.goal_adjustment_kcal as i64,
                            floored
                        ),
                        vec![],
                    )
                }
                None => (None, String::new(), missing.clone()),
            },
            Basis::PctOfCalories { pct, kcal_per_g } => match calories {
                Some(c) => (
                    Some(c * pct / 100.0 / kcal_per_g),
                    format!(" {pct}% of {} ÷ {kcal_per_g} kcal/g.", fmt_kcal(c)),
                    vec![],
                ),
                None => (None, String::new(), missing.clone()),
            },
            Basis::Per1000Kcal(per) => match calories {
                Some(c) => (
                    Some(c / 1000.0 * per),
                    format!(" {per} g × {:.1}.", c / 1000.0),
                    vec![],
                ),
                None => (None, String::new(), missing.clone()),
            },
            Basis::Remainder { kcal_per_g } => match calories {
                Some(c) => {
                    let left = (c - committed_kcal).max(0.0);
                    (
                        Some(left / kcal_per_g),
                        format!(
                            " ({} − {}) ÷ {kcal_per_g} kcal/g.",
                            fmt_kcal(c),
                            fmt_kcal(committed_kcal)
                        ),
                        vec![],
                    )
                }
                None => (None, String::new(), missing.clone()),
            },
        };

        let amount = amount.map(|a| round_for(rule.nutrient, a));
        if let (Some(a), Some(k)) = (amount, kcal_per_g_of(rule.nutrient)) {
            committed_kcal += a * k;
        }

        targets.push(PreviewTarget {
            nutrient: rule.nutrient,
            label: rule.nutrient.label(),
            unit: rule.nutrient.unit(),
            kind: rule.kind,
            amount,
            rationale: format!("{}{}", rule.why, detail),
            needs,
        });
    }

    FocusPreview {
        focus,
        label: focus.label(),
        summary: focus.summary(),
        targets,
        shown_nutrients: p.shown.to_vec(),
        chart_nutrients: p.chart.to_vec(),
        chart_mode: ChartMode::Percent,
        goal: p.goal,
        missing,
        estimate,
        changes_display: focus != TrackingFocus::Custom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::user::ALL_FOCUSES;

    /// 70 kg, 175 cm, 30, male, moderate: BMR 1648.75, TDEE 2555.56.
    fn inputs(goal: &str) -> EnergyInputs {
        EnergyInputs {
            sex: Some("male".into()),
            birth_date: NaiveDate::from_ymd_opt(1996, 1, 1),
            height_cm: Some(175.0),
            weight_kg: Some(70.0),
            weight_source: Some("weigh_in"),
            activity_level: "moderate".into(),
            goal: goal.into(),
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
    }

    fn amount(p: &FocusPreview, n: Nutrient) -> f64 {
        p.targets
            .iter()
            .find(|t| t.nutrient == n)
            .and_then(|t| t.amount)
            .unwrap_or_else(|| panic!("no amount for {}", n.key()))
    }

    fn kind(p: &FocusPreview, n: Nutrient) -> TargetKind {
        p.targets.iter().find(|t| t.nutrient == n).unwrap().kind
    }

    #[test]
    fn general_is_the_suggestion_settings_always_made() {
        let p = preview(TrackingFocus::General, &inputs("maintain"), today());
        // 2555.56 rounded to the nearest ten.
        assert_eq!(amount(&p, Nutrient::CaloriesKcal), 2560.0);
        assert_eq!(amount(&p, Nutrient::ProteinG), 112.0); // 1.6 × 70
        assert_eq!(amount(&p, Nutrient::FatG), 71.0); // 2560 × .25 / 9 = 71.1
                                                      // (2560 − 112×4 − 71×9) / 4 = (2560 − 448 − 639) / 4 = 368.25
        assert_eq!(amount(&p, Nutrient::CarbsG), 368.0);
        assert_eq!(amount(&p, Nutrient::FiberG), 36.0); // 2.56 × 14 = 35.8
        assert_eq!(kind(&p, Nutrient::CarbsG), TargetKind::Budget);
        assert!(p.missing.is_empty());
    }

    #[test]
    fn weight_loss_cuts_regardless_of_the_profile_goal() {
        let p = preview(TrackingFocus::WeightLoss, &inputs("bulk"), today());
        assert_eq!(p.goal, Some("cut"));
        assert_eq!(amount(&p, Nutrient::CaloriesKcal), 2060.0); // 2555.56 − 500
        assert_eq!(amount(&p, Nutrient::ProteinG), 140.0); // 2.0 × 70
    }

    #[test]
    fn muscle_gain_keeps_protein_in_the_evidence_range_and_carbs_as_a_goal() {
        let p = preview(TrackingFocus::MuscleGain, &inputs("cut"), today());
        assert_eq!(p.goal, Some("bulk"));
        assert_eq!(amount(&p, Nutrient::CaloriesKcal), 2860.0); // 2555.56 + 300
        let per_kg = amount(&p, Nutrient::ProteinG) / 70.0;
        assert!((1.6..=2.2).contains(&per_kg), "{per_kg} g/kg");
        assert_eq!(kind(&p, Nutrient::CarbsG), TargetKind::Goal);
    }

    #[test]
    fn keto_budgets_net_carbs_and_fills_with_fat() {
        let p = preview(TrackingFocus::Keto, &inputs("maintain"), today());
        assert_eq!(amount(&p, Nutrient::NetCarbsG), 25.0);
        assert_eq!(kind(&p, Nutrient::NetCarbsG), TargetKind::Budget);
        assert_eq!(amount(&p, Nutrient::ProteinG), 105.0); // 1.5 × 70
                                                           // (2560 − 25×4 − 105×4) / 9 = (2560 − 100 − 420) / 9 = 226.7
        assert_eq!(amount(&p, Nutrient::FatG), 227.0);
        assert_eq!(kind(&p, Nutrient::FatG), TargetKind::Goal);
        assert!(p.chart_nutrients.contains(&Nutrient::NetCarbsG));
    }

    #[test]
    fn carb_awareness_budgets_carbs_and_sugar_by_share() {
        let p = preview(TrackingFocus::Diabetes, &inputs("maintain"), today());
        assert_eq!(amount(&p, Nutrient::CarbsG), 288.0); // 2560 × .45 / 4
        assert_eq!(amount(&p, Nutrient::SugarG), 64.0); // 2560 × .10 / 4
        assert_eq!(kind(&p, Nutrient::CarbsG), TargetKind::Budget);
    }

    #[test]
    fn sodium_and_saturated_fat_follow_the_guideline_figures() {
        let bp = preview(TrackingFocus::BloodPressure, &inputs("maintain"), today());
        assert_eq!(amount(&bp, Nutrient::SodiumMg), 2000.0);
        assert_eq!(amount(&bp, Nutrient::SaturatedFatG), 28.0); // 2560 × .1 / 9

        let heart = preview(TrackingFocus::HeartHealth, &inputs("maintain"), today());
        assert_eq!(amount(&heart, Nutrient::SodiumMg), 2300.0);
        assert_eq!(amount(&heart, Nutrient::SaturatedFatG), 28.0);
        assert_eq!(kind(&heart, Nutrient::FiberG), TargetKind::Goal);
    }

    /// Silence is the bug: a rule that cannot be evaluated is listed without
    /// an amount and says what it needs; the fixed ones still apply.
    #[test]
    fn missing_body_basics_are_named_not_dropped() {
        let bare = EnergyInputs {
            activity_level: "moderate".into(),
            goal: "maintain".into(),
            ..Default::default()
        };
        let p = preview(TrackingFocus::Keto, &bare, today());
        assert_eq!(p.missing, vec!["weight", "height_cm", "birth_date"]);
        let net = p
            .targets
            .iter()
            .find(|t| t.nutrient == Nutrient::NetCarbsG)
            .unwrap();
        assert_eq!(net.amount, Some(25.0));
        let protein = p
            .targets
            .iter()
            .find(|t| t.nutrient == Nutrient::ProteinG)
            .unwrap();
        assert_eq!(protein.amount, None);
        assert_eq!(protein.needs, vec!["weight"]);
        let fat = p
            .targets
            .iter()
            .find(|t| t.nutrient == Nutrient::FatG)
            .unwrap();
        assert_eq!(fat.needs, vec!["weight", "height_cm", "birth_date"]);
    }

    #[test]
    fn custom_changes_nothing() {
        let p = preview(TrackingFocus::Custom, &inputs("maintain"), today());
        assert!(p.targets.is_empty());
        assert!(p.shown_nutrients.is_empty());
        assert!(!p.changes_display);
    }

    /// Every preset only ever shows or charts what it can also total, and
    /// never lists a nutrient twice.
    #[test]
    fn presets_are_internally_consistent() {
        for focus in ALL_FOCUSES {
            let p = preview(focus, &inputs("maintain"), today());
            let mut seen = Vec::new();
            for t in &p.targets {
                assert!(
                    !seen.contains(&t.nutrient),
                    "{focus:?} repeats {:?}",
                    t.nutrient
                );
                seen.push(t.nutrient);
                assert!(
                    t.amount.is_some(),
                    "{focus:?} {:?} has no amount",
                    t.nutrient
                );
                assert!(t.amount.unwrap() > 0.0);
            }
            for n in &p.chart_nutrients {
                assert!(
                    p.shown_nutrients.contains(n),
                    "{focus:?} charts a hidden nutrient"
                );
            }
        }
    }
}
