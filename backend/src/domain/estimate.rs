//! Estimators that use your own data.
//!
//! The formula in `energy.rs` guesses expenditure from a profile. This module
//! measures it: over a window, the calories that went in on the days the
//! account says were fully logged, against what the scale did. Energy balance
//! is the one identity in this whole domain that is actually true —
//! expenditure is intake minus whatever was stored — and the only thing that
//! can make it lie is a half-logged day counted as a small one. So the
//! estimate reads only days marked complete, and refuses to say anything at
//! all on thin data rather than saying something confidently wrong.
//!
//! Every number here comes with its inputs, and every "not yet" says what is
//! still needed. Silence is the bug.

use chrono::{Duration, NaiveDate};
use serde::Serialize;
use utoipa::ToSchema;

use super::energy::{goal_adjustment_kcal, EnergyEstimate, MIN_CALORIES_KCAL};
use super::focus::round_for;
use super::target::Nutrient;

/// Energy in a kilogram of body weight. Wishnofsky's 3500 kcal per pound
/// (1958) — the figure behind "500 kcal a day is a pound a week" — is
/// 7700 kcal/kg. It is a convention rather than a constant of nature: the
/// true value moves with body composition and drifts over a diet. It is the
/// one every calculator uses, though, which means a person can check the
/// arithmetic against any other source and get the same answer.
pub const KCAL_PER_KG: f64 = 7700.0;

/// What the adaptive estimate needs before it will say a number.
///
/// Seven complete days is one of each weekday, so a weekend does not stand in
/// for the week; two weigh-ins seven days apart is the least a slope can be
/// fitted to without the scale's own day-to-day noise being the whole
/// signal. Below these the answer is "not yet", never an estimate.
pub const MIN_COMPLETE_DAYS: i64 = 7;
pub const MIN_WEIGH_INS: i64 = 2;
pub const MIN_SPAN_DAYS: i64 = 7;

/// Under this weekly rate the trend counts as flat. A fitted slope of a few
/// grams a week is noise around a bathroom scale's own resolution, and
/// projecting a date from it would say "in eleven years" with a straight
/// face.
pub const FLAT_KG_PER_WEEK: f64 = 0.05;

/// Past this share of body weight per week a projection carries a caution.
/// About 1 % a week is the usual upper bound for loss that is mostly fat and
/// keeps lean mass; gain any faster is mostly not muscle. A caution is a
/// flag on a number, not advice about what to do with it.
pub const CAUTION_FRACTION_PER_WEEK: f64 = 0.01;

// ---- the weight trend -------------------------------------------------------

/// A straight line through the weigh-ins in a window, by least squares.
///
/// A line rather than "last minus first" because a single weigh-in after a
/// salty dinner would otherwise decide the whole month; least squares lets
/// every point vote. Days are counted from the first weigh-in so the
/// intercept is a weight on a real date rather than at some epoch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trend {
    pub first_day: NaiveDate,
    pub last_day: NaiveDate,
    pub weigh_ins: usize,
    pub span_days: i64,
    pub slope_kg_per_day: f64,
    /// Fitted weight on `first_day`.
    pub intercept_kg: f64,
}

impl Trend {
    /// Fit a line to `(date, kg)` points, in any order. `None` with fewer
    /// than two points or when every point is on one day: a slope needs two
    /// different days to exist at all.
    pub fn fit(points: &[(NaiveDate, f64)]) -> Option<Trend> {
        if points.len() < 2 {
            return None;
        }
        let first_day = points.iter().map(|p| p.0).min()?;
        let last_day = points.iter().map(|p| p.0).max()?;
        let span_days = (last_day - first_day).num_days();
        if span_days < 1 {
            return None;
        }

        let n = points.len() as f64;
        let xs: Vec<f64> = points
            .iter()
            .map(|p| (p.0 - first_day).num_days() as f64)
            .collect();
        let mean_x = xs.iter().sum::<f64>() / n;
        let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
        let sxx: f64 = xs.iter().map(|x| (x - mean_x).powi(2)).sum();
        let sxy: f64 = xs
            .iter()
            .zip(points)
            .map(|(x, p)| (x - mean_x) * (p.1 - mean_y))
            .sum();
        let slope = sxy / sxx;

        Some(Trend {
            first_day,
            last_day,
            weigh_ins: points.len(),
            span_days,
            slope_kg_per_day: slope,
            intercept_kg: mean_y - slope * mean_x,
        })
    }

    /// The fitted weight on a date, on or off the window.
    pub fn at(&self, day: NaiveDate) -> f64 {
        self.intercept_kg + self.slope_kg_per_day * (day - self.first_day).num_days() as f64
    }

    pub fn rate_kg_per_week(&self) -> f64 {
        self.slope_kg_per_day * 7.0
    }
}

// ---- energy balance -----------------------------------------------------------

/// Expenditure by energy balance: what went in, less what was stored. A
/// falling trend means intake understated expenditure, so the slope's sign
/// does the right thing on its own — losing 0.4 kg a week at 1850 kcal a day
/// is 1850 + 0.4 / 7 × 7700 = 2290 kcal a day out.
pub fn expenditure_kcal(mean_intake_kcal: f64, slope_kg_per_day: f64) -> f64 {
    mean_intake_kcal - slope_kg_per_day * KCAL_PER_KG
}

/// How much to trust the adaptive figure, from how much fed it.
///
/// Fourteen complete days is two of every weekday and three weigh-ins is the
/// first point that can disagree with a line; under either the estimate is
/// mostly noise and says so. Four full weeks is where the scale's water
/// swings have had a chance to cancel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Moderate,
    Good,
}

pub const CONFIDENCE_MODERATE_DAYS: i64 = 14;
pub const CONFIDENCE_MODERATE_WEIGH_INS: i64 = 3;
pub const CONFIDENCE_GOOD_DAYS: i64 = 28;

pub fn confidence(complete_days: i64, weigh_ins: i64) -> Confidence {
    if complete_days < CONFIDENCE_MODERATE_DAYS || weigh_ins < CONFIDENCE_MODERATE_WEIGH_INS {
        Confidence::Low
    } else if complete_days < CONFIDENCE_GOOD_DAYS {
        Confidence::Moderate
    } else {
        Confidence::Good
    }
}

/// What the window held, against what the estimate needs. The same shape for
/// both so a client compares field by field rather than parsing a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct Evidence {
    /// Days in the window marked "I logged everything".
    pub complete_days: i64,
    /// Weigh-ins in the window.
    pub weigh_ins: i64,
    /// Days between the first and last weigh-in in the window.
    pub span_days: i64,
}

pub const NEEDED: Evidence = Evidence {
    complete_days: MIN_COMPLETE_DAYS,
    weigh_ins: MIN_WEIGH_INS,
    span_days: MIN_SPAN_DAYS,
};

impl Evidence {
    /// Why the estimate is not ready, naming every shortfall, or `None`
    /// when it is. The sentence is for people; `have` and `need` are for
    /// clients.
    pub fn shortfall(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.complete_days < NEEDED.complete_days {
            parts.push(format!(
                "{} more day{} marked as fully logged",
                NEEDED.complete_days - self.complete_days,
                plural(NEEDED.complete_days - self.complete_days)
            ));
        }
        if self.weigh_ins < NEEDED.weigh_ins {
            parts.push(format!(
                "{} more weigh-in{}",
                NEEDED.weigh_ins - self.weigh_ins,
                plural(NEEDED.weigh_ins - self.weigh_ins)
            ));
        } else if self.span_days < NEEDED.span_days {
            parts.push(format!(
                "weigh-ins at least {} days apart (yours span {})",
                NEEDED.span_days, self.span_days
            ));
        }
        if parts.is_empty() {
            None
        } else {
            Some(format!("Needs {}.", parts.join(" and ")))
        }
    }
}

fn plural(n: i64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// The adaptive estimate, with everything it was made from.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdaptiveEstimate {
    /// Expenditure by energy balance, to the nearest kcal.
    pub tdee_kcal: f64,
    /// Mean intake over the complete days, to the nearest kcal.
    pub mean_intake_kcal: f64,
    /// `mean_intake_kcal − tdee_kcal`: negative in a deficit.
    pub energy_balance_kcal_per_day: f64,
    /// The fitted trend's rate; negative when losing.
    pub weight_change_kg_per_week: f64,
    pub slope_kg_per_day: f64,
    /// The fitted line's ends, not the raw weigh-ins.
    pub trend_start_kg: f64,
    pub trend_end_kg: f64,
    pub first_weigh_in: NaiveDate,
    pub last_weigh_in: NaiveDate,
    pub confidence: Confidence,
    /// The profile goal the budget below is adjusted for.
    pub goal: String,
    pub goal_adjustment_kcal: f64,
    /// `tdee_kcal + goal_adjustment_kcal`, to the nearest ten and never
    /// under the 1200 kcal floor: the budget "use as budget" writes, so the
    /// same re-basing the formula does happens here and nowhere else.
    pub budget_kcal: f64,
    pub floored_at_minimum: bool,
}

impl AdaptiveEstimate {
    pub fn from_window(intakes_kcal: &[f64], trend: &Trend, goal: &str) -> Self {
        let mean = intakes_kcal.iter().sum::<f64>() / intakes_kcal.len() as f64;
        let tdee = expenditure_kcal(mean, trend.slope_kg_per_day);
        let adjustment = goal_adjustment_kcal(goal);
        Self {
            tdee_kcal: tdee.round(),
            mean_intake_kcal: mean.round(),
            energy_balance_kcal_per_day: (mean - tdee).round(),
            weight_change_kg_per_week: round(trend.rate_kg_per_week(), 2),
            slope_kg_per_day: round(trend.slope_kg_per_day, 4),
            trend_start_kg: round(trend.at(trend.first_day), 2),
            trend_end_kg: round(trend.at(trend.last_day), 2),
            first_weigh_in: trend.first_day,
            last_weigh_in: trend.last_day,
            confidence: confidence(intakes_kcal.len() as i64, trend.weigh_ins as i64),
            goal: goal.to_string(),
            goal_adjustment_kcal: adjustment,
            budget_kcal: budget_from(tdee + adjustment),
            floored_at_minimum: tdee + adjustment < MIN_CALORIES_KCAL,
        }
    }
}

/// A calorie budget from an expenditure figure: floored, then to the nearest
/// ten, the same way a preset prices one. 2287 kcal claims a precision no
/// estimate has.
fn budget_from(kcal: f64) -> f64 {
    round_for(Nutrient::CaloriesKcal, kcal.max(MIN_CALORIES_KCAL))
}

/// `GET /estimates/tdee`. `ready` is the one field to branch on: `estimate`
/// is present exactly when it is true, and `reason` exactly when it is not.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TdeeEstimate {
    pub ready: bool,
    /// What is still needed, when not ready.
    pub reason: Option<String>,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub days: i64,
    pub have: Evidence,
    pub need: Evidence,
    pub estimate: Option<AdaptiveEstimate>,
    /// The profile formula (Mifflin–St Jeor × activity), for comparison.
    pub formula: Option<EnergyEstimate>,
    /// Profile fields the formula needs and does not have.
    pub formula_missing: Vec<&'static str>,
}

// ---- projection ---------------------------------------------------------------

/// When the trend line crosses the target, if it does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reach {
    /// Days from the trend's last day until the target, never negative.
    Days(f64),
    Flat,
    PointsAway,
}

impl Reach {
    pub fn reason(&self) -> Option<&'static str> {
        match self {
            Self::Days(_) => None,
            Self::Flat => Some("trend_is_flat"),
            Self::PointsAway => Some("trend_points_away"),
        }
    }
}

/// Days until `target_kg` at `slope_kg_per_day` from `current_kg`. Flat is
/// checked first: a slope of zero would divide by zero, and a slope near it
/// would give a date nobody should be told.
pub fn days_to_reach(current_kg: f64, target_kg: f64, slope_kg_per_day: f64) -> Reach {
    if (slope_kg_per_day * 7.0).abs() < FLAT_KG_PER_WEEK {
        return Reach::Flat;
    }
    let diff = target_kg - current_kg;
    let days = diff / slope_kg_per_day;
    if days < 0.0 {
        Reach::PointsAway
    } else {
        Reach::Days(days)
    }
}

/// Whether a weekly rate is past the caution line for this body weight.
pub fn caution(rate_kg_per_week: f64, body_kg: f64) -> bool {
    rate_kg_per_week.abs() > CAUTION_FRACTION_PER_WEEK * body_kg
}

/// The change from expenditure that reaches `target_kg` from `current_kg`
/// in `days`. `None` when there are no days to do it in.
pub fn change_to_reach_by(current_kg: f64, target_kg: f64, days: i64) -> Option<(f64, f64)> /* (kcal per day, kg per week) */
{
    if days <= 0 {
        return None;
    }
    let diff = target_kg - current_kg;
    Some((diff * KCAL_PER_KG / days as f64, diff / days as f64 * 7.0))
}

/// The weigh-in evidence behind a projection. Fewer fields than the TDEE's,
/// because a trend needs no diary at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct TrendEvidence {
    pub weigh_ins: i64,
    pub span_days: i64,
}

pub const TREND_NEEDED: TrendEvidence = TrendEvidence {
    weigh_ins: MIN_WEIGH_INS,
    span_days: MIN_SPAN_DAYS,
};

impl TrendEvidence {
    pub fn shortfall(&self) -> Option<String> {
        Evidence {
            complete_days: NEEDED.complete_days,
            weigh_ins: self.weigh_ins,
            span_days: self.span_days,
        }
        .shortfall()
    }
}

/// The trend as a projection reads it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrendSummary {
    /// The last weigh-in in the window; every projection counts from here.
    pub as_of: NaiveDate,
    /// The fitted weight on `as_of`.
    pub current_kg: f64,
    pub first_weigh_in: NaiveDate,
    pub start_kg: f64,
    pub rate_kg_per_week: f64,
    pub slope_kg_per_day: f64,
    /// True when the rate is past 1 % of body weight a week.
    pub caution: bool,
    pub caution_threshold_kg_per_week: f64,
}

impl TrendSummary {
    pub fn from_trend(trend: &Trend) -> Self {
        let current = trend.at(trend.last_day);
        Self {
            as_of: trend.last_day,
            current_kg: round(current, 2),
            first_weigh_in: trend.first_day,
            start_kg: round(trend.at(trend.first_day), 2),
            rate_kg_per_week: round(trend.rate_kg_per_week(), 2),
            slope_kg_per_day: round(trend.slope_kg_per_day, 4),
            caution: caution(trend.rate_kg_per_week(), current),
            caution_threshold_kg_per_week: round(CAUTION_FRACTION_PER_WEEK * current, 2),
        }
    }
}

/// What reaching the target by a chosen date would take.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ByDatePlan {
    pub date: NaiveDate,
    /// Counted from the trend's `as_of`, where the weight is known.
    pub from: NaiveDate,
    pub days: i64,
    pub required_rate_kg_per_week: f64,
    /// Change from expenditure, per day: negative is a deficit.
    pub daily_energy_change_kcal: f64,
    /// `adaptive` when the energy-balance estimate is ready, else `formula`;
    /// absent when neither could be made, and so is the intake below.
    pub basis: Option<&'static str>,
    pub basis_tdee_kcal: Option<f64>,
    /// `basis_tdee_kcal + daily_energy_change_kcal`, to the nearest ten and
    /// never under the 1200 kcal floor.
    pub suggested_intake_kcal: Option<f64>,
    pub floored_at_minimum: bool,
    pub caution: bool,
}

impl ByDatePlan {
    pub fn new(
        trend: &Trend,
        target_kg: f64,
        by: NaiveDate,
        basis: Option<(&'static str, f64)>,
    ) -> Option<Self> {
        let current = trend.at(trend.last_day);
        let days = (by - trend.last_day).num_days();
        let (change, rate) = change_to_reach_by(current, target_kg, days)?;
        let intake = basis.map(|(_, tdee)| tdee + change);
        Some(Self {
            date: by,
            from: trend.last_day,
            days,
            required_rate_kg_per_week: round(rate, 2),
            daily_energy_change_kcal: change.round(),
            basis: basis.map(|b| b.0),
            basis_tdee_kcal: basis.map(|b| b.1),
            suggested_intake_kcal: intake.map(budget_from),
            floored_at_minimum: intake.is_some_and(|i| i < MIN_CALORIES_KCAL),
            caution: caution(rate, current),
        })
    }
}

/// `GET /estimates/projection`. `ready` says whether there is a trend at
/// all; `reached_on` and `by` each carry their own reason when absent, since
/// a trend can exist and still not cross the target.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Projection {
    pub ready: bool,
    pub reason: Option<String>,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub days: i64,
    pub have: TrendEvidence,
    pub need: TrendEvidence,
    pub target_weight_kg: Option<f64>,
    pub trend: Option<TrendSummary>,
    /// The date the trend line meets the target weight.
    pub reached_on: Option<NaiveDate>,
    pub days_to_target: Option<i64>,
    /// `not_ready`, `no_target_weight`, `trend_is_flat` or
    /// `trend_points_away` when `reached_on` is absent.
    pub reached_reason: Option<&'static str>,
    pub by: Option<ByDatePlan>,
    /// `not_ready`, `no_target_weight` or `date_not_after_as_of` when a
    /// `by` date was asked for and no plan could be made; absent when none
    /// was asked for.
    pub by_reason: Option<&'static str>,
}

impl Projection {
    /// The date and whole days at which the trend meets the target, rounded
    /// up: a target reached halfway through a day is reached that day.
    pub fn reach_date(trend: &Trend, target_kg: f64) -> (Option<NaiveDate>, Option<i64>, Reach) {
        let reach = days_to_reach(trend.at(trend.last_day), target_kg, trend.slope_kg_per_day);
        match reach {
            Reach::Days(d) => {
                let days = d.ceil() as i64;
                (
                    Some(trend.last_day + Duration::days(days)),
                    Some(days),
                    reach,
                )
            }
            _ => (None, None, reach),
        }
    }
}

fn round(v: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (v * f).round() / f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn a_line_through_exact_points_is_recovered() {
        // 0.1 kg a day, for three weeks.
        let t = Trend::fit(&[
            (d("2026-03-15"), 83.6),
            (d("2026-03-01"), 85.0),
            (d("2026-03-08"), 84.3),
        ])
        .unwrap();
        assert_eq!(t.first_day, d("2026-03-01"));
        assert_eq!(t.last_day, d("2026-03-15"));
        assert_eq!(t.span_days, 14);
        assert!((t.slope_kg_per_day + 0.1).abs() < 1e-9);
        assert!((t.intercept_kg - 85.0).abs() < 1e-9);
        assert!((t.at(d("2026-03-15")) - 83.6).abs() < 1e-9);
        assert!((t.rate_kg_per_week() + 0.7).abs() < 1e-9);
    }

    #[test]
    fn least_squares_lets_every_point_vote() {
        // Symmetric noise around a flat 80 kg: the line is flat, not the
        // last-minus-first +1.
        let t = Trend::fit(&[
            (d("2026-01-01"), 79.0),
            (d("2026-01-02"), 81.0),
            (d("2026-01-03"), 79.0),
            (d("2026-01-04"), 81.0),
            (d("2026-01-05"), 79.0),
            (d("2026-01-06"), 81.0),
            (d("2026-01-07"), 79.0),
            (d("2026-01-08"), 81.0),
        ])
        .unwrap();
        // Slope of the alternating series: Σ(x−3.5)(y−80) / Σ(x−3.5)² = 4/42
        // ≈ 0.095 kg a day — a residue of the phase, nothing like the +2 kg
        // that last-minus-first would report.
        assert!((t.slope_kg_per_day - 4.0 / 42.0).abs() < 1e-9);
    }

    #[test]
    fn a_trend_needs_two_different_days() {
        assert!(Trend::fit(&[]).is_none());
        assert!(Trend::fit(&[(d("2026-01-01"), 80.0)]).is_none());
        assert!(Trend::fit(&[(d("2026-01-01"), 80.0), (d("2026-01-01"), 81.0)]).is_none());
    }

    /// The worked example: 1850 kcal a day in, 0.4 kg a week off, is
    /// 2290 kcal a day out.
    #[test]
    fn energy_balance_is_intake_minus_what_was_stored() {
        let slope = -0.4 / 7.0;
        assert!((expenditure_kcal(1850.0, slope) - 2290.0).abs() < 1e-9);
        // Gaining 0.2 kg a week on 3000 kcal: 3000 − 220 = 2780 out.
        assert!((expenditure_kcal(3000.0, 0.2 / 7.0) - 2780.0).abs() < 1e-9);
        // Flat scale: intake is expenditure.
        assert_eq!(expenditure_kcal(2100.0, 0.0), 2100.0);
    }

    #[test]
    fn the_estimate_composes_and_rounds() {
        let trend = Trend::fit(&[
            (d("2026-02-01"), 85.0),
            (d("2026-02-08"), 84.6),
            (d("2026-02-15"), 84.2),
            (d("2026-02-22"), 83.8),
        ])
        .unwrap();
        let intakes = vec![1850.0; 28];
        let e = AdaptiveEstimate::from_window(&intakes, &trend, "cut");
        assert_eq!(e.tdee_kcal, 2290.0);
        assert_eq!(e.mean_intake_kcal, 1850.0);
        assert_eq!(e.energy_balance_kcal_per_day, -440.0);
        assert_eq!(e.weight_change_kg_per_week, -0.4);
        assert_eq!(e.trend_start_kg, 85.0);
        assert_eq!(e.trend_end_kg, 83.8);
        assert_eq!(e.confidence, Confidence::Good);
        // The budget re-bases the goal's deficit on the measured figure.
        assert_eq!(e.goal_adjustment_kcal, -500.0);
        assert_eq!(e.budget_kcal, 1790.0);
        assert!(!e.floored_at_minimum);

        // A maintainer's budget is the expenditure, to the nearest ten.
        let m = AdaptiveEstimate::from_window(&[2287.0; 7], &trend, "maintain");
        assert_eq!(m.tdee_kcal, 2727.0);
        assert_eq!(m.budget_kcal, 2730.0);

        // A small person cutting hard is held at the floor, and told.
        let flat = Trend::fit(&[(d("2026-02-01"), 50.0), (d("2026-02-08"), 50.0)]).unwrap();
        let s = AdaptiveEstimate::from_window(&[1500.0; 7], &flat, "cut");
        assert_eq!(s.budget_kcal, 1200.0);
        assert!(s.floored_at_minimum);
    }

    #[test]
    fn confidence_follows_the_thresholds() {
        assert_eq!(confidence(7, 2), Confidence::Low);
        assert_eq!(confidence(13, 5), Confidence::Low);
        assert_eq!(confidence(20, 2), Confidence::Low);
        assert_eq!(confidence(14, 3), Confidence::Moderate);
        assert_eq!(confidence(27, 10), Confidence::Moderate);
        assert_eq!(confidence(28, 3), Confidence::Good);
    }

    #[test]
    fn the_shortfall_names_everything_missing() {
        let none = Evidence {
            complete_days: 0,
            weigh_ins: 0,
            span_days: 0,
        };
        assert_eq!(
            none.shortfall().unwrap(),
            "Needs 7 more days marked as fully logged and 2 more weigh-ins."
        );
        let close = Evidence {
            complete_days: 6,
            weigh_ins: 3,
            span_days: 4,
        };
        assert_eq!(
            close.shortfall().unwrap(),
            "Needs 1 more day marked as fully logged and weigh-ins at least 7 days apart (yours span 4)."
        );
        assert!(Evidence {
            complete_days: 7,
            weigh_ins: 2,
            span_days: 7,
        }
        .shortfall()
        .is_none());
    }

    #[test]
    fn the_target_is_reached_by_dividing_the_gap_by_the_slope() {
        // 84 → 78 at 0.1 kg/day is 60 days.
        assert_eq!(days_to_reach(84.0, 78.0, -0.1), Reach::Days(60.0));
        // Already there.
        assert_eq!(days_to_reach(78.0, 78.0, -0.1), Reach::Days(0.0));
        // Gaining while the target is below: never.
        assert_eq!(days_to_reach(84.0, 78.0, 0.1), Reach::PointsAway);
        // 20 g a week is the scale's noise, not a trend.
        assert_eq!(days_to_reach(84.0, 78.0, -0.02 / 7.0), Reach::Flat);
        assert_eq!(days_to_reach(84.0, 78.0, 0.0), Reach::Flat);
    }

    #[test]
    fn the_reach_date_rounds_a_part_day_up() {
        // 83.8 → 78 at 0.4 kg a week is 101.5 days: reached on day 102.
        let trend = Trend::fit(&[(d("2026-02-01"), 85.0), (d("2026-02-22"), 83.8)]).unwrap();
        let (date, days, reach) = Projection::reach_date(&trend, 78.0);
        assert_eq!(days, Some(102));
        assert_eq!(date, Some(d("2026-06-04")));
        assert!(matches!(reach, Reach::Days(_)));
    }

    #[test]
    fn caution_is_one_percent_of_body_weight_a_week() {
        assert!(!caution(-0.7, 84.0)); // threshold 0.84
        assert!(caution(-0.9, 84.0));
        assert!(caution(1.0, 84.0)); // gaining that fast is flagged too
        assert!(!caution(-0.84, 84.0)); // on the line is not past it
    }

    #[test]
    fn a_deadline_prices_its_deficit() {
        // 6 kg in 60 days: 770 kcal a day, 0.7 kg a week.
        let (kcal, rate) = change_to_reach_by(84.0, 78.0, 60).unwrap();
        assert!((kcal + 770.0).abs() < 1e-9);
        assert!((rate + 0.7).abs() < 1e-9);
        assert!(change_to_reach_by(84.0, 78.0, 0).is_none());

        let trend = Trend::fit(&[(d("2026-02-01"), 84.0), (d("2026-02-08"), 84.0)]).unwrap();
        // 30 days: 1540 kcal a day off a 2300 expenditure lands under the
        // floor, and the rate (1.4 kg/wk on 84 kg) earns a caution.
        let plan =
            ByDatePlan::new(&trend, 78.0, d("2026-03-10"), Some(("adaptive", 2300.0))).unwrap();
        assert_eq!(plan.days, 30);
        assert_eq!(plan.daily_energy_change_kcal, -1540.0);
        assert_eq!(plan.required_rate_kg_per_week, -1.4);
        assert_eq!(plan.suggested_intake_kcal, Some(1200.0));
        assert!(plan.floored_at_minimum);
        assert!(plan.caution);
        // 300 days: 154 kcal a day, and the intake reads to the nearest ten.
        let gentle =
            ByDatePlan::new(&trend, 78.0, d("2026-12-05"), Some(("formula", 2300.0))).unwrap();
        assert_eq!(gentle.days, 300);
        assert_eq!(gentle.daily_energy_change_kcal, -154.0);
        assert_eq!(gentle.suggested_intake_kcal, Some(2150.0));
        assert!(!gentle.floored_at_minimum);
        assert!(!gentle.caution);
        // No basis: the change is still stated, the intake is not invented.
        let bare = ByDatePlan::new(&trend, 78.0, d("2026-03-10"), None).unwrap();
        assert_eq!(bare.basis, None);
        assert_eq!(bare.suggested_intake_kcal, None);
        assert!(!bare.floored_at_minimum);
        // A date on or before the last weigh-in has no days to work with.
        assert!(ByDatePlan::new(&trend, 78.0, d("2026-02-08"), None).is_none());
    }
}
