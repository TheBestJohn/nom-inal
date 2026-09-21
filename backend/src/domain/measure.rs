//! How an amount reads: "2 chicken breasts", "1.5 cups", "348 g".
//!
//! Grams are what the database stores and what everything adds up. A
//! household measure is how the amount was *arrived at* — a portion's label
//! and a count of it — and this module is the one place that turns the two
//! into a phrase.
//!
//! One place because the same amount is shown in five: the diary, the recipe
//! page, the public share page, its JSON-LD, and the Markdown export. Four
//! copies of "how do I pluralise a portion label" would be four subtly
//! different answers, and the one people would notice first is the shared
//! page, where "2 chicken breast" reads as a typo in someone else's kitchen.
//!
//! The pluralisation is deliberately timid. A label a person typed — "chicken
//! breast", "slice", "patty" — is an ordinary noun phrase and gets an
//! ordinary English plural. A label USDA published — "cup, chopped",
//! "medium (2-1/2\" dia)" — is not a noun phrase at all, and no amount of
//! cleverness turns it into one, so it is multiplied instead: "3 × cup,
//! chopped". Being obviously mechanical is better than being confidently
//! wrong.

/// "2" rather than "2.0", "1.5" rather than "1.500000". The same rule for a
/// portion count as for a weight, so an amount never mixes two number
/// formats in one line.
pub fn number(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Abbreviations that are already their own plural. "2 oz" is right; "2 ozs"
/// is not English, and neither is "2 gs".
///
/// The size words are here for the same reason: `food_portions` is full of
/// "1 medium" and "1 large", which are adjectives standing in for the food
/// itself. "2 medium" is what a person says.
const INVARIANT: &[&str] = &[
    "g",
    "gm",
    "kg",
    "mg",
    "ml",
    "cl",
    "dl",
    "l",
    "oz",
    "fl oz",
    "fl. oz",
    "lb",
    "lbs",
    "tbsp",
    "tbs",
    "tsp",
    "cc",
    "pt",
    "qt",
    "gal",
    "kcal",
    "medium",
    "large",
    "small",
    "extra large",
    "extra small",
    "x-large",
    "jumbo",
    "mini",
];

/// The handful of endings the -s/-es/-ies rules get wrong.
const IRREGULAR: &[(&str, &str)] = &[
    ("half", "halves"),
    ("loaf", "loaves"),
    ("leaf", "leaves"),
    ("knife", "knives"),
    ("potato", "potatoes"),
    ("tomato", "tomatoes"),
    ("mango", "mangoes"),
    ("foot", "feet"),
];

/// A label that reads as a noun phrase: letters, spaces and hyphens.
///
/// A comma, a bracket or a digit means the label is a description rather than
/// a name — "cup, chopped", "medium (2-1/2\" dia)", "2 tbsp" — and nothing
/// good comes of appending an "s" to one.
fn is_noun_phrase(label: &str) -> bool {
    !label.is_empty()
        && label
            .chars()
            .all(|c| c.is_alphabetic() || c == ' ' || c == '-' || c == '\'')
}

/// The plural of one word, by the ordinary English endings.
fn plural_word(word: &str) -> String {
    let lower = word.to_lowercase();
    if let Some((_, plural)) = IRREGULAR.iter().find(|(s, _)| *s == lower) {
        // Keep the label's own capitalisation: "Half" stays "Halves".
        if word.chars().next().is_some_and(char::is_uppercase) {
            let mut chars = plural.chars();
            let mut out: String = chars
                .next()
                .into_iter()
                .flat_map(char::to_uppercase)
                .collect();
            out.push_str(chars.as_str());
            return out;
        }
        return plural.to_string();
    }

    let ends_with = |suffix: &str| lower.ends_with(suffix);
    if ends_with("s") || ends_with("x") || ends_with("z") || ends_with("ch") || ends_with("sh") {
        format!("{word}es")
    } else if ends_with("y")
        && lower
            .chars()
            .rev()
            .nth(1)
            .is_some_and(|c| !"aeiou".contains(c))
    {
        format!("{}ies", &word[..word.len() - 1])
    } else {
        format!("{word}s")
    }
}

/// The plural of a label, pluralising the word that carries the count: the
/// last one, or the one before "of" in "slice of bread".
fn pluralise(label: &str) -> String {
    if INVARIANT.contains(&label.to_lowercase().as_str()) {
        return label.to_string();
    }
    if let Some(at) = label.find(" of ") {
        return format!("{}{}", pluralise(&label[..at]), &label[at..]);
    }
    match label.rsplit_once(' ') {
        Some((head, last)) => format!("{head} {}", plural_word(last)),
        None => plural_word(label),
    }
}

/// The measure itself, without the "1" the labels conventionally carry.
///
/// Portions are written "1 cup", "1 medium", "1 breast" — the label answers
/// "one what?". A count of two needs the "what" on its own, and a label with
/// any other number in it ("2 tbsp") is left exactly as written, because a
/// count of it means something the label does not say.
pub fn measure_of(label: &str) -> &str {
    let label = label.trim();
    for prefix in ["1 x ", "1x ", "1 ", "one ", "One "] {
        if let Some(rest) = label.strip_prefix(prefix) {
            let rest = rest.trim_start();
            if !rest.is_empty() {
                return rest;
            }
        }
    }
    label
}

/// The singular of the last word of a phrase, when English gives one:
/// "eggs" → "egg", "berries" → "berry", "tomatoes" → "tomato". `None` when
/// nothing changes.
///
/// The inverse of [`pluralise`], and here beside it so the two rules are read
/// together. The food search uses it to retry "2 eggs" as "egg", and the
/// portion seeds to recognise a food whose name is written in the plural.
pub fn singularise(phrase: &str) -> Option<String> {
    let mut words: Vec<&str> = phrase.split_whitespace().collect();
    let last = words.pop()?;
    let lower = last.to_ascii_lowercase();
    let singular = if let Some(stem) = lower.strip_suffix("ies") {
        format!("{stem}y")
    } else if lower.ends_with("oes")
        || lower.ends_with("ses")
        || lower.ends_with("xes")
        || lower.ends_with("ches")
        || lower.ends_with("shes")
    {
        lower[..lower.len() - 2].to_string()
    } else if lower.ends_with('s') && !lower.ends_with("ss") && lower.len() > 2 {
        lower[..lower.len() - 1].to_string()
    } else {
        return None;
    };
    words.push(&singular);
    Some(words.join(" "))
}

/// "1 chicken breast", "2 chicken breasts", "3 × cup, chopped".
pub fn portion_phrase(label: &str, count: f64) -> String {
    let measure = measure_of(label);
    let n = number(count);
    if count == 1.0 {
        format!("{n} {measure}")
    } else if is_noun_phrase(measure) {
        format!("{n} {}", pluralise(measure))
    } else {
        format!("{n} × {measure}")
    }
}

/// A weight, as the diary has always shown one.
pub fn grams_phrase(grams: f64) -> String {
    format!("{} g", number(grams))
}

/// Servings of a recipe: what a recipe entry and a sub-recipe ingredient are
/// measured in.
pub fn servings_phrase(servings: f64) -> String {
    format!(
        "{} serving{}",
        number(servings),
        if servings == 1.0 { "" } else { "s" }
    )
}

/// The amount to show, however it was entered.
///
/// The portion wins when there is one — it is what the person said — and the
/// weight is the fallback, which is what every amount read as before this
/// existed. An ingredient that is only words has no amount at all, and gets
/// an empty string rather than a "0 g" that would be a lie.
pub fn amount_label(
    portion_label: Option<&str>,
    portion_count: Option<f64>,
    quantity_g: Option<f64>,
    servings: Option<f64>,
) -> String {
    match (portion_label, portion_count) {
        (Some(label), Some(count)) if !label.trim().is_empty() && count > 0.0 => {
            portion_phrase(label, count)
        }
        _ => match (quantity_g, servings) {
            (Some(grams), _) => grams_phrase(grams),
            (None, Some(servings)) => servings_phrase(servings),
            (None, None) => String::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_of_something_is_the_label_as_written() {
        assert_eq!(portion_phrase("chicken breast", 1.0), "1 chicken breast");
        assert_eq!(portion_phrase("1 chicken breast", 1.0), "1 chicken breast");
        assert_eq!(portion_phrase("1 cup, chopped", 1.0), "1 cup, chopped");
    }

    #[test]
    fn a_count_pluralises_an_ordinary_label() {
        assert_eq!(portion_phrase("1 chicken breast", 2.0), "2 chicken breasts");
        assert_eq!(portion_phrase("1 slice", 3.0), "3 slices");
        assert_eq!(portion_phrase("1 patty", 2.0), "2 patties");
        assert_eq!(portion_phrase("1 sandwich", 2.0), "2 sandwiches");
        assert_eq!(portion_phrase("1 box", 2.0), "2 boxes");
        assert_eq!(portion_phrase("1 glass", 2.0), "2 glasses");
        assert_eq!(portion_phrase("1 half", 2.0), "2 halves");
        assert_eq!(portion_phrase("1 potato", 3.0), "3 potatoes");
        assert_eq!(portion_phrase("1 day-old roll", 2.0), "2 day-old rolls");
        assert_eq!(portion_phrase("1 slice of bread", 2.0), "2 slices of bread");
    }

    #[test]
    fn a_unit_abbreviation_is_never_given_an_s() {
        assert_eq!(portion_phrase("1 oz", 4.0), "4 oz");
        assert_eq!(portion_phrase("1 tbsp", 2.0), "2 tbsp");
        assert_eq!(portion_phrase("1 fl oz", 8.0), "8 fl oz");
        assert_eq!(portion_phrase("1 g", 20.0), "20 g");
        // The size words `food_portions` is full of are adjectives, not nouns.
        assert_eq!(portion_phrase("1 medium", 2.0), "2 medium");
        assert_eq!(portion_phrase("1 large", 3.0), "3 large");
    }

    #[test]
    fn a_label_that_is_not_a_noun_phrase_is_multiplied_instead() {
        assert_eq!(portion_phrase("1 cup, chopped", 3.0), "3 × cup, chopped");
        assert_eq!(
            portion_phrase("1 medium (2-1/2\" dia)", 2.0),
            "2 × medium (2-1/2\" dia)"
        );
        // A label with its own number says something a count cannot restate.
        assert_eq!(portion_phrase("2 tbsp", 3.0), "3 × 2 tbsp");
    }

    #[test]
    fn counts_read_the_way_people_write_them() {
        assert_eq!(number(1.0), "1");
        assert_eq!(number(2.0), "2");
        assert_eq!(number(1.5), "1.5");
        assert_eq!(number(0.5), "0.5");
        assert_eq!(number(348.0), "348");
        assert_eq!(portion_phrase("1 cup", 1.5), "1.5 cups");
        assert_eq!(portion_phrase("1 cup", 0.5), "0.5 cups");
    }

    #[test]
    fn without_a_portion_an_amount_is_its_weight() {
        assert_eq!(amount_label(None, None, Some(348.0), None), "348 g");
        assert_eq!(amount_label(None, None, Some(174.5), None), "174.5 g");
        assert_eq!(amount_label(None, None, None, Some(2.0)), "2 servings");
        assert_eq!(amount_label(None, None, None, Some(1.0)), "1 serving");
        assert_eq!(amount_label(None, None, None, None), "");
    }

    #[test]
    fn a_portion_beats_the_weight_it_worked_out_to() {
        assert_eq!(
            amount_label(Some("1 chicken breast"), Some(2.0), Some(348.0), None),
            "2 chicken breasts"
        );
        // Half a snapshot is no snapshot: the database forbids it, and if one
        // ever arrives the weight is still true.
        assert_eq!(
            amount_label(Some("1 chicken breast"), None, Some(348.0), None),
            "348 g"
        );
        assert_eq!(
            amount_label(Some("  "), Some(2.0), Some(348.0), None),
            "348 g"
        );
    }
}
