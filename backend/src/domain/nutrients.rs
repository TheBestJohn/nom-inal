use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A computed nutrient total (a serving, a recipe, a day).
///
/// Foods store nutrients per 100 g with optional fields for values the source
/// did not publish. Totals flatten those unknowns to 0 so that summing is
/// always well-defined — a missing fibre figure must not poison a day's total.
///
/// Serialised through [`NutrientsWire`], which adds the derived figures. They
/// are not fields here because a field has to be maintained by every
/// constructor, and a total built in SQL that forgot to subtract fibre would
/// serve a net-carbs figure that disagreed with its own carbs and fibre.
/// Deriving at the one point of output means there is nothing to forget.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
#[serde(into = "NutrientsWire")]
pub struct Nutrients {
    pub calories_kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub fiber_g: f64,
    pub sugar_g: f64,
    pub saturated_fat_g: f64,
    pub sodium_mg: f64,
}

/// What a `Nutrients` looks like on the wire: the stored eight plus the
/// figures derived from them.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[schema(as = Nutrients)]
pub struct NutrientsWire {
    pub calories_kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub fiber_g: f64,
    pub sugar_g: f64,
    pub saturated_fat_g: f64,
    pub sodium_mg: f64,
    /// Carbohydrate minus fibre, floored at zero. Computed, never stored.
    pub net_carbs_g: f64,
}

impl From<Nutrients> for NutrientsWire {
    fn from(n: Nutrients) -> Self {
        Self {
            calories_kcal: n.calories_kcal,
            protein_g: n.protein_g,
            carbs_g: n.carbs_g,
            fat_g: n.fat_g,
            fiber_g: n.fiber_g,
            sugar_g: n.sugar_g,
            saturated_fat_g: n.saturated_fat_g,
            sodium_mg: n.sodium_mg,
            net_carbs_g: n.net_carbs_g(),
        }
    }
}

// Every response type that carries a `Nutrients` derives `ToSchema`, which
// needs `Nutrients` itself to have a schema. It is the wire shape's schema,
// under the wire shape's name, so the spec documents what is actually sent.
impl utoipa::PartialSchema for Nutrients {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        NutrientsWire::schema()
    }
}

impl ToSchema for Nutrients {
    fn name() -> std::borrow::Cow<'static, str> {
        NutrientsWire::name()
    }
}

/// Share of energy from each macronutrient, in percent.
///
/// Uses the Atwater factors (4 kcal/g protein and carbohydrate, 9 kcal/g fat)
/// and divides by the sum of the three rather than by the stated calories.
/// A label's calorie figure is rounded, sometimes counts fibre, and sometimes
/// includes alcohol, so dividing by it would give three shares that do not
/// add up to anything in particular. Dividing by their own sum makes the
/// three always total 100 — or all read 0 when nothing has been logged.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, ToSchema)]
pub struct EnergyShare {
    pub protein_pct: f64,
    pub carbs_pct: f64,
    pub fat_pct: f64,
}

/// kcal per gram: protein, carbohydrate, fat.
pub const KCAL_PER_G_PROTEIN: f64 = 4.0;
pub const KCAL_PER_G_CARBS: f64 = 4.0;
pub const KCAL_PER_G_FAT: f64 = 9.0;

impl Nutrients {
    /// Carbohydrate minus fibre, floored at zero.
    ///
    /// The floor matters: a source that reports fibre but rounds carbohydrate
    /// down can put fibre a fraction above carbs, and a negative net-carb
    /// figure on a stick of celery is a bug report, not a fact.
    pub fn net_carbs_g(&self) -> f64 {
        (self.carbs_g - self.fiber_g).max(0.0)
    }

    pub fn energy_share(&self) -> EnergyShare {
        let protein = self.protein_g.max(0.0) * KCAL_PER_G_PROTEIN;
        let carbs = self.carbs_g.max(0.0) * KCAL_PER_G_CARBS;
        let fat = self.fat_g.max(0.0) * KCAL_PER_G_FAT;
        let sum = protein + carbs + fat;
        if sum <= 0.0 {
            return EnergyShare::default();
        }
        fn r(v: f64) -> f64 {
            (v * 10.0).round() / 10.0
        }
        EnergyShare {
            protein_pct: r(protein / sum * 100.0),
            carbs_pct: r(carbs / sum * 100.0),
            fat_pct: r(fat / sum * 100.0),
        }
    }

    /// Scale a per-100g profile to an arbitrary gram amount.
    pub fn scaled(&self, factor: f64) -> Self {
        Self {
            calories_kcal: self.calories_kcal * factor,
            protein_g: self.protein_g * factor,
            carbs_g: self.carbs_g * factor,
            fat_g: self.fat_g * factor,
            fiber_g: self.fiber_g * factor,
            sugar_g: self.sugar_g * factor,
            saturated_fat_g: self.saturated_fat_g * factor,
            sodium_mg: self.sodium_mg * factor,
        }
    }

    pub fn rounded(&self) -> Self {
        fn r(v: f64) -> f64 {
            (v * 100.0).round() / 100.0
        }
        Self {
            calories_kcal: r(self.calories_kcal),
            protein_g: r(self.protein_g),
            carbs_g: r(self.carbs_g),
            fat_g: r(self.fat_g),
            fiber_g: r(self.fiber_g),
            sugar_g: r(self.sugar_g),
            saturated_fat_g: r(self.saturated_fat_g),
            sodium_mg: r(self.sodium_mg),
        }
    }
}

impl std::ops::Add for Nutrients {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            calories_kcal: self.calories_kcal + rhs.calories_kcal,
            protein_g: self.protein_g + rhs.protein_g,
            carbs_g: self.carbs_g + rhs.carbs_g,
            fat_g: self.fat_g + rhs.fat_g,
            fiber_g: self.fiber_g + rhs.fiber_g,
            sugar_g: self.sugar_g + rhs.sugar_g,
            saturated_fat_g: self.saturated_fat_g + rhs.saturated_fat_g,
            sodium_mg: self.sodium_mg + rhs.sodium_mg,
        }
    }
}

impl std::iter::Sum for Nutrients {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), |acc, n| acc + n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_carbs_is_carbs_minus_fibre_floored_at_zero() {
        let n = Nutrients {
            carbs_g: 30.0,
            fiber_g: 8.0,
            ..Default::default()
        };
        assert_eq!(n.net_carbs_g(), 22.0);

        let celery = Nutrients {
            carbs_g: 1.0,
            fiber_g: 1.6,
            ..Default::default()
        };
        assert_eq!(celery.net_carbs_g(), 0.0);
    }

    /// The derived figure rides along on the wire without being a field.
    #[test]
    fn net_carbs_is_serialised_from_the_stored_figures() {
        let n = Nutrients {
            carbs_g: 30.0,
            fiber_g: 8.0,
            ..Default::default()
        };
        let v = serde_json::to_value(n).unwrap();
        assert_eq!(v["net_carbs_g"], 22.0);
        assert_eq!(v["carbs_g"], 30.0);
        assert_eq!(v.as_object().unwrap().len(), 9);
    }

    #[test]
    fn energy_share_sums_to_one_hundred() {
        let n = Nutrients {
            calories_kcal: 2000.0,
            protein_g: 150.0, // 600 kcal
            carbs_g: 200.0,   // 800 kcal
            fat_g: 66.7,      // 600.3 kcal
            ..Default::default()
        };
        let s = n.energy_share();
        assert!((s.protein_pct + s.carbs_pct + s.fat_pct - 100.0).abs() < 0.2);
        assert!((s.protein_pct - 30.0).abs() < 0.1);
        assert!((s.carbs_pct - 40.0).abs() < 0.1);
    }

    #[test]
    fn energy_share_of_nothing_is_zero_not_nan() {
        assert_eq!(Nutrients::default().energy_share(), EnergyShare::default());
    }
}
