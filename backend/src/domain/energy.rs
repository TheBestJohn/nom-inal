//! Energy expenditure, estimated from the profile.
//!
//! This arithmetic lived in the Settings page as a "Suggested" block. Moving it
//! here is not about the browser being unable to multiply: the focus presets
//! derive their amounts from the same estimate, and two copies of a formula
//! with a rounding step each is how a preset and the suggestion beside it end
//! up disagreeing by a few kcal that nobody can explain.

use chrono::NaiveDate;
use serde::Serialize;
use utoipa::ToSchema;

/// Resting energy expenditure by Mifflin–St Jeor (Mifflin et al., Am J Clin
/// Nutr 1990), the equation most validation studies have found closest to
/// measured values in adults. `sex` is `female` or anything else: the male
/// constant is used when sex is unstated, which the estimate says.
pub fn bmr_kcal(sex: &str, age_years: f64, height_cm: f64, weight_kg: f64) -> f64 {
    let offset = if sex == "female" { -161.0 } else { 5.0 };
    10.0 * weight_kg + 6.25 * height_cm - 5.0 * age_years + offset
}

/// Total daily energy expenditure: BMR scaled by the usual activity
/// multipliers (1.2 sedentary through 1.9 very active), which are the
/// Harris–Benedict factors every calculator uses. An unknown level is treated
/// as moderate rather than refused — the profile column has no CHECK, and a
/// stray value should cost an ordinary estimate, not the whole preview.
pub fn tdee_kcal(bmr: f64, activity_level: &str) -> f64 {
    bmr * activity_factor(activity_level)
}

pub fn activity_factor(activity_level: &str) -> f64 {
    match activity_level {
        "sedentary" => 1.2,
        "light" => 1.375,
        "moderate" => 1.55,
        "active" => 1.725,
        "very_active" => 1.9,
        _ => 1.55,
    }
}

/// The profile's stated goal as a kcal offset from expenditure: a 500 kcal
/// deficit is the conventional "about half a kilo a week", and 300 kcal is a
/// surplus small enough that most of it can go to muscle rather than fat.
pub fn goal_adjustment_kcal(goal: &str) -> f64 {
    match goal {
        "cut" => -500.0,
        "bulk" => 300.0,
        _ => 0.0,
    }
}

/// The floor under any calorie budget this app suggests. Below about
/// 1200 kcal it is hard to meet nutrient needs from ordinary food, which is
/// why that figure is the usual lower bound for an unsupervised diet. A
/// budget that would fall below it is clamped, and the preview says so.
pub const MIN_CALORIES_KCAL: f64 = 1200.0;

/// Whole years between two dates, the way a birthday counts them.
pub fn age_years(birth_date: NaiveDate, today: NaiveDate) -> f64 {
    // `years_since` is birthday-aware and returns None for a birth date after
    // today, which is reported as 0 rather than as a negative age.
    today.years_since(birth_date).unwrap_or(0) as f64
}

/// Everything the estimate reads from the account, with what is missing
/// named rather than silently defaulted.
#[derive(Debug, Clone, Default)]
pub struct EnergyInputs {
    pub sex: Option<String>,
    pub birth_date: Option<NaiveDate>,
    pub height_cm: Option<f64>,
    /// The latest weigh-in, or the target weight when there is none — a
    /// person who has not weighed in yet has still said roughly what they
    /// weigh by saying where they want to be.
    pub weight_kg: Option<f64>,
    pub weight_source: Option<&'static str>,
    pub activity_level: String,
    pub goal: String,
}

impl EnergyInputs {
    /// Which profile fields stand between this account and an estimate.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut m = Vec::new();
        if self.weight_kg.is_none() {
            m.push("weight");
        }
        if self.height_cm.is_none() {
            m.push("height_cm");
        }
        if self.birth_date.is_none() {
            m.push("birth_date");
        }
        m
    }

    pub fn estimate(&self, today: NaiveDate) -> Option<EnergyEstimate> {
        let (weight_kg, height_cm, birth_date) =
            (self.weight_kg?, self.height_cm?, self.birth_date?);
        let age = age_years(birth_date, today);
        let sex = self.sex.clone().unwrap_or_else(|| "unspecified".into());
        let bmr = bmr_kcal(&sex, age, height_cm, weight_kg);
        let tdee = tdee_kcal(bmr, &self.activity_level);
        let adjustment = goal_adjustment_kcal(&self.goal);
        Some(EnergyEstimate {
            sex_assumed_male: sex != "female" && sex != "male",
            age_years: age,
            height_cm,
            weight_kg,
            weight_source: self.weight_source.unwrap_or("unknown"),
            activity_level: self.activity_level.clone(),
            activity_factor: activity_factor(&self.activity_level),
            goal: self.goal.clone(),
            goal_adjustment_kcal: adjustment,
            bmr_kcal: bmr.round(),
            tdee_kcal: tdee.round(),
            calories_kcal: (tdee + adjustment).max(MIN_CALORIES_KCAL).round(),
            floored_at_minimum: tdee + adjustment < MIN_CALORIES_KCAL,
        })
    }
}

/// The estimate, with its inputs echoed so a client can show where a number
/// came from instead of presenting it as a verdict.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EnergyEstimate {
    /// True when sex is unstated and the male constant was used.
    pub sex_assumed_male: bool,
    pub age_years: f64,
    pub height_cm: f64,
    pub weight_kg: f64,
    /// `weigh_in` or `target_weight`.
    pub weight_source: &'static str,
    pub activity_level: String,
    pub activity_factor: f64,
    pub goal: String,
    pub goal_adjustment_kcal: f64,
    /// Resting expenditure, Mifflin–St Jeor.
    pub bmr_kcal: f64,
    /// Expenditure including activity.
    pub tdee_kcal: f64,
    /// Expenditure adjusted for the stated goal and floored at
    /// [`MIN_CALORIES_KCAL`]: the figure a calorie budget starts from.
    pub calories_kcal: f64,
    /// True when the goal adjustment would have taken `calories_kcal` under
    /// the floor, so the figure shown is the floor rather than the arithmetic.
    pub floored_at_minimum: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example most references print: 70 kg, 175 cm, 30-year-old
    /// male is 1648.75 kcal; female is 1482.75.
    #[test]
    fn mifflin_st_jeor_matches_the_published_example() {
        assert!((bmr_kcal("male", 30.0, 175.0, 70.0) - 1648.75).abs() < 1e-9);
        assert!((bmr_kcal("female", 30.0, 175.0, 70.0) - 1482.75).abs() < 1e-9);
        // Unstated sex uses the male constant rather than refusing.
        assert_eq!(
            bmr_kcal("unspecified", 30.0, 175.0, 70.0),
            bmr_kcal("male", 30.0, 175.0, 70.0)
        );
    }

    #[test]
    fn activity_scales_bmr_by_the_usual_factors() {
        assert_eq!(tdee_kcal(1000.0, "sedentary"), 1200.0);
        assert_eq!(tdee_kcal(1000.0, "light"), 1375.0);
        assert_eq!(tdee_kcal(1000.0, "moderate"), 1550.0);
        assert_eq!(tdee_kcal(1000.0, "active"), 1725.0);
        assert_eq!(tdee_kcal(1000.0, "very_active"), 1900.0);
        assert_eq!(tdee_kcal(1000.0, "couch"), 1550.0);
    }

    #[test]
    fn age_counts_whole_birthdays() {
        let born = NaiveDate::from_ymd_opt(1990, 6, 15).unwrap();
        assert_eq!(
            age_years(born, NaiveDate::from_ymd_opt(2026, 6, 14).unwrap()),
            35.0
        );
        assert_eq!(
            age_years(born, NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()),
            36.0
        );
        assert_eq!(
            age_years(born, NaiveDate::from_ymd_opt(1980, 1, 1).unwrap()),
            0.0
        );
    }

    #[test]
    fn the_estimate_names_what_it_is_missing() {
        let inputs = EnergyInputs {
            activity_level: "moderate".into(),
            goal: "maintain".into(),
            ..Default::default()
        };
        assert_eq!(inputs.missing(), vec!["weight", "height_cm", "birth_date"]);
        assert!(inputs
            .estimate(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
            .is_none());
    }

    #[test]
    fn a_deficit_never_takes_the_budget_below_the_floor() {
        let inputs = EnergyInputs {
            sex: Some("female".into()),
            birth_date: NaiveDate::from_ymd_opt(1960, 1, 1),
            height_cm: Some(150.0),
            weight_kg: Some(45.0),
            weight_source: Some("weigh_in"),
            activity_level: "sedentary".into(),
            goal: "cut".into(),
        };
        let e = inputs
            .estimate(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
            .unwrap();
        assert!(e.tdee_kcal - 500.0 < MIN_CALORIES_KCAL);
        assert_eq!(e.calories_kcal, MIN_CALORIES_KCAL);
        assert!(e.floored_at_minimum);
    }

    #[test]
    fn the_estimate_composes_the_three_steps() {
        let inputs = EnergyInputs {
            sex: Some("male".into()),
            birth_date: NaiveDate::from_ymd_opt(1996, 1, 1),
            height_cm: Some(175.0),
            weight_kg: Some(70.0),
            weight_source: Some("weigh_in"),
            activity_level: "moderate".into(),
            goal: "cut".into(),
        };
        let e = inputs
            .estimate(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
            .unwrap();
        assert_eq!(e.age_years, 30.0);
        assert_eq!(e.bmr_kcal, 1649.0);
        assert_eq!(e.tdee_kcal, 2556.0); // 1648.75 * 1.55 = 2555.56
        assert_eq!(e.calories_kcal, 2056.0);
        assert!(!e.sex_assumed_male);
    }
}
