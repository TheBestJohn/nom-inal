//! Household measures a fresh instance already knows.
//!
//! Portions are per food, which means that on a new instance every food has
//! none until somebody types one in — and the feature that needs them, "two
//! chicken breasts", is exactly the feature nobody discovers if the first
//! food they add offers nothing to pick. This table is the answer to that:
//! the measures for the few dozen things people log constantly, applied when
//! a food is created whose name is one of them and which has no portions of
//! its own yet.
//!
//! **Source.** Every weight here is USDA FoodData Central's own standard
//! portion for that food — the `foodPortions` of the Foundation Foods and SR
//! Legacy datasets, <https://fdc.nal.usda.gov/> — which is the same source
//! the importer reads when it brings a USDA food in, and why a seeded row is
//! recorded with `source = 'usda'` rather than pretending a person typed it.
//! A provider re-import may refresh one; nothing here ever overwrites a
//! measure somebody added themselves.
//!
//! **Matching is deliberately literal.** A food is seeded only when its name
//! *is* one of these things, not when it mentions one: "Chicken breast" and
//! "Chicken breast, raw" are a chicken breast, and "Chicken breast burrito"
//! is a burrito. Guessing wider would put "1 breast · 174 g" on a burrito,
//! and a wrong portion is worse than no portion — it is a number somebody
//! will log without checking.

use super::measure;

/// One measure, in the house style: the label answers "one what?".
pub struct SeedPortion {
    pub label: &'static str,
    pub grams: f64,
}

/// The measures for one food, keyed by its normalised name.
pub struct SeedFood {
    /// Already normalised: lower case, singular, no brand or preparation.
    pub name: &'static str,
    pub portions: &'static [SeedPortion],
}

macro_rules! seed {
    ($name:literal, $(($label:literal, $grams:expr)),+ $(,)?) => {
        SeedFood { name: $name, portions: &[$(SeedPortion { label: $label, grams: $grams }),+] }
    };
}

/// The bundled table: 67 foods, 95 measures. Deliberately short — the things
/// people log by eye, not a second food database.
pub const SEEDS: &[SeedFood] = &[
    // Meat, fish and eggs
    seed!(
        "chicken breast",
        ("1 breast", 174.0),
        ("1 half breast", 87.0)
    ),
    seed!("chicken thigh", ("1 thigh", 82.0)),
    seed!(
        "egg",
        ("1 large", 50.0),
        ("1 medium", 44.0),
        ("1 extra large", 56.0)
    ),
    seed!("salmon", ("1 fillet", 178.0)),
    seed!("tuna", ("1 can, drained", 142.0)),
    seed!("bacon", ("1 slice", 8.0)),
    seed!("ground beef", ("1 patty", 85.0)),
    // Bread and grains
    seed!("bread", ("1 slice", 28.0)),
    seed!("bagel", ("1 medium", 98.0)),
    seed!("tortilla", ("1 medium", 45.0)),
    seed!("rice", ("1 cup, cooked", 158.0)),
    seed!("white rice", ("1 cup, cooked", 158.0)),
    seed!("brown rice", ("1 cup, cooked", 195.0)),
    seed!("pasta", ("1 cup, cooked", 140.0)),
    seed!("spaghetti", ("1 cup, cooked", 140.0)),
    seed!("quinoa", ("1 cup, cooked", 185.0)),
    seed!("couscous", ("1 cup, cooked", 157.0)),
    seed!("oats", ("1 cup", 81.0), ("1 half cup", 40.5)),
    seed!("rolled oats", ("1 cup", 81.0), ("1 half cup", 40.5)),
    // Fruit
    seed!(
        "banana",
        ("1 medium", 118.0),
        ("1 large", 136.0),
        ("1 small", 101.0)
    ),
    seed!(
        "apple",
        ("1 medium", 182.0),
        ("1 large", 223.0),
        ("1 small", 149.0)
    ),
    seed!("orange", ("1 medium", 131.0)),
    seed!("pear", ("1 medium", 178.0)),
    seed!("peach", ("1 medium", 150.0)),
    seed!("kiwifruit", ("1 fruit", 69.0)),
    seed!("mango", ("1 whole", 336.0), ("1 cup, sliced", 165.0)),
    seed!("avocado", ("1 whole", 201.0), ("1 half", 100.5)),
    seed!("strawberry", ("1 cup, halved", 152.0), ("1 berry", 12.0)),
    seed!("blueberry", ("1 cup", 148.0)),
    seed!("grape", ("1 cup", 151.0)),
    seed!("watermelon", ("1 cup, diced", 152.0)),
    seed!("pineapple", ("1 cup, chunks", 165.0)),
    // Vegetables
    seed!(
        "potato",
        ("1 medium", 213.0),
        ("1 large", 369.0),
        ("1 small", 170.0)
    ),
    seed!("sweet potato", ("1 medium", 130.0)),
    seed!("onion", ("1 medium", 110.0), ("1 cup, chopped", 160.0)),
    seed!("garlic", ("1 clove", 3.0)),
    seed!("carrot", ("1 medium", 61.0), ("1 cup, chopped", 128.0)),
    seed!("tomato", ("1 medium", 123.0), ("1 cup, chopped", 180.0)),
    seed!("cucumber", ("1 medium", 201.0), ("1 cup, sliced", 104.0)),
    seed!("bell pepper", ("1 medium", 119.0)),
    seed!("broccoli", ("1 cup, chopped", 91.0), ("1 spear", 31.0)),
    seed!("spinach", ("1 cup", 30.0)),
    seed!("lettuce", ("1 cup, shredded", 47.0)),
    seed!("mushroom", ("1 cup, sliced", 70.0), ("1 medium", 18.0)),
    seed!("celery", ("1 stalk", 40.0)),
    seed!("zucchini", ("1 medium", 196.0)),
    seed!("green bean", ("1 cup", 100.0)),
    seed!("corn", ("1 ear", 90.0)),
    // Pulses
    seed!("black bean", ("1 cup, cooked", 172.0)),
    seed!("chickpea", ("1 cup, cooked", 164.0)),
    seed!("lentil", ("1 cup, cooked", 198.0)),
    // Dairy and fats
    seed!("milk", ("1 cup", 244.0)),
    seed!("yogurt", ("1 cup", 245.0), ("1 container", 170.0)),
    seed!(
        "cheddar cheese",
        ("1 slice", 28.0),
        ("1 cup, shredded", 113.0)
    ),
    seed!("butter", ("1 tbsp", 14.2), ("1 pat", 5.0)),
    seed!("olive oil", ("1 tbsp", 13.5), ("1 tsp", 4.5)),
    seed!("peanut butter", ("1 tbsp", 16.0), ("1 tbsp, heaped", 32.0)),
    // Nuts, store-cupboard and drinks
    seed!("almond", ("1 oz", 28.4), ("1 cup, whole", 143.0)),
    seed!("walnut", ("1 cup, halved", 117.0), ("1 oz", 28.4)),
    seed!("honey", ("1 tbsp", 21.0)),
    seed!("sugar", ("1 tsp", 4.2), ("1 cup", 200.0)),
    seed!("flour", ("1 cup", 125.0)),
    seed!("pizza", ("1 slice", 107.0)),
    seed!("orange juice", ("1 cup", 248.0)),
    seed!("coffee", ("1 cup", 237.0)),
    seed!("beer", ("1 can", 356.0)),
    seed!("wine", ("1 glass", 147.0)),
];

/// A food's name as the table keys it: lower case, no brand parenthesis and
/// no preparation clause.
///
/// "Bananas, raw" and "Banana (loose)" are both a banana; "banana bread" is
/// not, and keeps every word so it fails to match.
fn normalise(name: &str) -> String {
    let head = name
        .split(['(', ','])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    head.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The measures to give a newly created food, or nothing.
///
/// Matched on the whole normalised name, then on its singular, so "Eggs" and
/// "Bananas, raw" find their entry while "chicken breast burrito" finds
/// none.
pub fn seeds_for(name: &str) -> &'static [SeedPortion] {
    let normalised = normalise(name);
    if normalised.is_empty() {
        return &[];
    }
    let find = |key: &str| {
        SEEDS
            .iter()
            .find(|seed| seed.name == key)
            .map(|seed| seed.portions)
    };
    find(&normalised)
        .or_else(|| measure::singularise(&normalised).as_deref().and_then(find))
        .unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(name: &str) -> Vec<&'static str> {
        seeds_for(name).iter().map(|p| p.label).collect()
    }

    #[test]
    fn a_food_named_after_the_thing_gets_its_measures() {
        assert_eq!(labels("Chicken breast"), ["1 breast", "1 half breast"]);
        assert_eq!(labels("chicken breast"), ["1 breast", "1 half breast"]);
        assert_eq!(labels("Egg"), ["1 large", "1 medium", "1 extra large"]);
        assert_eq!(labels("Garlic"), ["1 clove"]);
    }

    #[test]
    fn a_plural_or_a_qualifier_is_still_the_same_thing() {
        assert_eq!(labels("Eggs"), ["1 large", "1 medium", "1 extra large"]);
        assert_eq!(labels("Bananas, raw"), ["1 medium", "1 large", "1 small"]);
        assert_eq!(labels("Potatoes"), ["1 medium", "1 large", "1 small"]);
        assert_eq!(labels("Chicken breast, raw"), ["1 breast", "1 half breast"]);
        assert_eq!(
            labels("Chicken breast (skinless)"),
            ["1 breast", "1 half breast"]
        );
        assert_eq!(labels("  Bread  "), ["1 slice"]);
    }

    #[test]
    fn a_food_that_merely_mentions_one_gets_nothing() {
        // The whole point of matching the entire name: a burrito weighs what
        // a burrito weighs, and 174 g of it is not "1 breast".
        assert!(seeds_for("Chicken breast burrito").is_empty());
        assert!(seeds_for("Banana bread").is_empty());
        assert!(seeds_for("Egg fried rice").is_empty());
        assert!(seeds_for("Garlic bread").is_empty());
        assert!(seeds_for("Cream of mushroom soup").is_empty());
        assert!(seeds_for("Apple pie").is_empty());
        // Nor does a brand's version of one: the name is not the thing.
        assert!(seeds_for("Birds Eye green beans, frozen").is_empty());
    }

    #[test]
    fn nothing_matches_nothing() {
        assert!(seeds_for("").is_empty());
        assert!(seeds_for("   ").is_empty());
        assert!(seeds_for(", raw").is_empty());
        assert!(seeds_for("Kombucha").is_empty());
    }

    #[test]
    fn the_table_is_well_formed() {
        for seed in SEEDS {
            assert_eq!(
                seed.name,
                normalise(seed.name),
                "a key has to be written the way a name normalises"
            );
            assert!(!seed.portions.is_empty(), "{}", seed.name);
            for portion in seed.portions {
                assert!(portion.grams > 0.0, "{} {}", seed.name, portion.label);
                assert!(
                    portion.label.starts_with("1 "),
                    "a label answers \"one what?\": {}",
                    portion.label
                );
            }
            let mut labels: Vec<&str> = seed.portions.iter().map(|p| p.label).collect();
            labels.sort_unstable();
            let before = labels.len();
            labels.dedup();
            assert_eq!(before, labels.len(), "duplicate label on {}", seed.name);
        }
        let mut names: Vec<&str> = SEEDS.iter().map(|s| s.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "the same food is keyed twice");
    }
}
