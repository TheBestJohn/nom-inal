//! Reading an ingredient line the way a cook wrote it.
//!
//! "1 1/2 cups flour", "2-3 cloves garlic, minced", "1 can (400 g) chopped
//! tomatoes", "½ tsp salt", "200g butter". The line is split into a quantity,
//! a unit and the thing itself, and when the unit is a mass the quantity is
//! converted to grams — the only unit this application stores. A cup or a
//! clove stays a cup or a clove: converting volume to weight needs a density
//! per food, and a number that looks precise and is not would be worse than
//! asking.

use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;
use utoipa::ToSchema;

/// One ingredient line, taken apart.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ParsedIngredient {
    /// The line as it was written, after whitespace cleanup.
    pub text: String,
    /// The amount, when the line opens with one. A range ("2-3") is its
    /// midpoint, and unicode and vulgar fractions are resolved.
    pub quantity: Option<f64>,
    /// The measure as written, normalised to a singular short form ("cup",
    /// "tbsp", "g"). Absent when the line has a bare count ("2 eggs") or no
    /// amount at all.
    pub unit: Option<String>,
    /// What is being measured, with the amount, unit and any parenthetical
    /// removed.
    pub name: String,
    /// The amount in grams, when it can be known: a mass unit, or a mass
    /// stated in brackets ("1 can (400 g)"). Null for household measures.
    pub grams: Option<f64>,
}

/// Mass units and their gram factor.
const MASS_UNITS: &[(&str, f64)] = &[
    ("g", 1.0),
    ("gram", 1.0),
    ("gr", 1.0),
    ("gm", 1.0),
    ("kg", 1000.0),
    ("kilogram", 1000.0),
    ("kilo", 1000.0),
    ("mg", 0.001),
    ("milligram", 0.001),
    ("oz", 28.349523125),
    ("ounce", 28.349523125),
    ("lb", 453.59237),
    ("lbs", 453.59237),
    ("pound", 453.59237),
];

/// Household and volume units, with the singular form each is normalised to.
/// Kept rather than converted, see the module note.
const OTHER_UNITS: &[(&str, &str)] = &[
    ("ml", "ml"),
    ("milliliter", "ml"),
    ("millilitre", "ml"),
    ("l", "l"),
    ("liter", "l"),
    ("litre", "l"),
    ("dl", "dl"),
    ("cl", "cl"),
    ("cup", "cup"),
    ("c", "cup"),
    ("tbsp", "tbsp"),
    ("tbs", "tbsp"),
    ("tb", "tbsp"),
    ("tablespoon", "tbsp"),
    ("tsp", "tsp"),
    ("teaspoon", "tsp"),
    ("t", "tsp"),
    ("pint", "pint"),
    ("pt", "pint"),
    ("quart", "quart"),
    ("qt", "quart"),
    ("gallon", "gallon"),
    ("gal", "gallon"),
    ("pinch", "pinch"),
    ("dash", "dash"),
    ("drop", "drop"),
    ("handful", "handful"),
    ("bunch", "bunch"),
    ("sprig", "sprig"),
    ("clove", "clove"),
    ("head", "head"),
    ("stalk", "stalk"),
    ("stick", "stick"),
    ("slice", "slice"),
    ("piece", "piece"),
    ("can", "can"),
    ("tin", "tin"),
    ("jar", "jar"),
    ("packet", "packet"),
    ("package", "package"),
    ("pkg", "package"),
    ("bag", "bag"),
    ("box", "box"),
    ("bottle", "bottle"),
    ("large", "large"),
    ("medium", "medium"),
    ("small", "small"),
    ("whole", "whole"),
    ("scoop", "scoop"),
    ("knob", "knob"),
    ("pat", "pat"),
    ("sheet", "sheet"),
    ("fillet", "fillet"),
    ("rasher", "rasher"),
    ("ear", "ear"),
    ("leaf", "leaf"),
    ("leaves", "leaf"),
];

/// Number words that open a line ("a pinch", "two eggs", "half an onion").
const NUMBER_WORDS: &[(&str, f64)] = &[
    ("a", 1.0),
    ("an", 1.0),
    ("one", 1.0),
    ("two", 2.0),
    ("three", 3.0),
    ("four", 4.0),
    ("five", 5.0),
    ("six", 6.0),
    ("seven", 7.0),
    ("eight", 8.0),
    ("nine", 9.0),
    ("ten", 10.0),
    ("eleven", 11.0),
    ("twelve", 12.0),
    ("dozen", 12.0),
    ("half", 0.5),
    ("quarter", 0.25),
];

const VULGAR_FRACTIONS: &[(char, &str)] = &[
    ('½', "1/2"),
    ('⅓', "1/3"),
    ('⅔', "2/3"),
    ('¼', "1/4"),
    ('¾', "3/4"),
    ('⅕', "1/5"),
    ('⅖', "2/5"),
    ('⅗', "3/5"),
    ('⅘', "4/5"),
    ('⅙', "1/6"),
    ('⅚', "5/6"),
    ('⅛', "1/8"),
    ('⅜', "3/8"),
    ('⅝', "5/8"),
    ('⅞', "7/8"),
];

/// A plain number, a decimal, a fraction, or a mixed number: "2", "2.5",
/// "1/2", "1 1/2", "1,5" (a decimal comma between digits). The bare fraction
/// is tried first: alternation is leftmost-first, and the plain-number arm
/// would otherwise accept the "1" of "1/2" and stop.
fn number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\d+)/(\d+)|^(\d+(?:[.,]\d+)?)(?:\s+(\d+)/(\d+))?").unwrap())
}

/// "(400 g)", "(7 oz)", "(14 oz each)", "(about 2 lb)" — a mass stated in
/// brackets, whatever else is said around it.
fn bracketed_mass_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\([^)]*?(\d+(?:[.,]\d+)?)\s*([a-z]+)\b[^)]*\)").unwrap())
}

fn parenthetical_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s*\([^)]*\)").unwrap())
}

/// A number glued to its unit: "200g", "1.5kg", "2oz".
fn glued_unit_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\d+(?:[.,]\d+)?)([a-zA-Z]+)$").unwrap())
}

fn parse_number(s: &str) -> Option<f64> {
    s.replace(',', ".").parse().ok()
}

/// Read a number off the front of `s`. Returns the value and how many bytes
/// it consumed.
fn take_number(s: &str) -> Option<(f64, usize)> {
    let caps = number_re().captures(s)?;
    let whole = caps.get(0)?;
    let value = if let (Some(num), Some(den)) = (caps.get(1), caps.get(2)) {
        // A bare fraction.
        let den: f64 = den.as_str().parse().ok()?;
        if den == 0.0 {
            return None;
        }
        num.as_str().parse::<f64>().ok()? / den
    } else {
        let mut value = parse_number(caps.get(3)?.as_str())?;
        if let (Some(num), Some(den)) = (caps.get(4), caps.get(5)) {
            let den: f64 = den.as_str().parse().ok()?;
            if den == 0.0 {
                return None;
            }
            value += num.as_str().parse::<f64>().ok()? / den;
        }
        value
    };
    Some((value, whole.end()))
}

/// Read a quantity: a number, optionally followed by a range ("2-3",
/// "2 to 3", "2–3") whose midpoint is taken, or a number word.
fn take_quantity(s: &str) -> Option<(f64, usize)> {
    if let Some((first, used)) = take_number(s) {
        let rest = &s[used..];
        let trimmed = rest.trim_start();
        let lead = rest.len() - trimmed.len();
        for sep in ["-", "–", "—", "to", "or"] {
            if let Some(after) = trimmed.strip_prefix(sep) {
                // "2 to 3" needs a space after the word; "2-3" does not.
                if sep.chars().all(char::is_alphabetic) && !after.starts_with(char::is_whitespace) {
                    continue;
                }
                let after_trim = after.trim_start();
                if let Some((second, used2)) = take_number(after_trim) {
                    let consumed =
                        used + lead + sep.len() + (after.len() - after_trim.len()) + used2;
                    return Some(((first + second) / 2.0, consumed));
                }
            }
        }
        return Some((first, used));
    }

    let word: String = s
        .chars()
        .take_while(|c| c.is_alphabetic())
        .collect::<String>()
        .to_lowercase();
    if word.is_empty() {
        return None;
    }
    let (_, value) = NUMBER_WORDS.iter().find(|(w, _)| *w == word)?;
    // "a" only counts as a quantity before a unit or a noun, never on its own.
    Some((*value, word.len()))
}

/// Normalise a unit token: lowercase, no trailing dot, singular.
fn unit_of(token: &str) -> Option<Unit> {
    let t = token.trim_end_matches('.').to_lowercase();
    let singular = t
        .strip_suffix("es")
        .filter(|s| s.ends_with("ch") || s.ends_with("sh") || s.ends_with('x'))
        .map(String::from)
        .or_else(|| t.strip_suffix('s').map(String::from))
        .unwrap_or_else(|| t.clone());

    for candidate in [t.as_str(), singular.as_str()] {
        if let Some((_, factor)) = MASS_UNITS.iter().find(|(u, _)| *u == candidate) {
            let short = match candidate {
                "kg" | "kilogram" | "kilo" => "kg",
                "mg" | "milligram" => "mg",
                "oz" | "ounce" => "oz",
                "lb" | "lbs" | "pound" => "lb",
                _ => "g",
            };
            return Some(Unit::Mass(short, *factor));
        }
        if let Some((_, canonical)) = OTHER_UNITS.iter().find(|(u, _)| *u == candidate) {
            return Some(Unit::Other(canonical));
        }
    }
    None
}

#[derive(Debug, Clone, Copy)]
enum Unit {
    Mass(&'static str, f64),
    Other(&'static str),
}

impl Unit {
    fn name(&self) -> &'static str {
        match self {
            Unit::Mass(n, _) => n,
            Unit::Other(n) => n,
        }
    }
}

fn normalise(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + 8);
    for c in line.chars() {
        if let Some((_, frac)) = VULGAR_FRACTIONS.iter().find(|(f, _)| *f == c) {
            // "1½" is a mixed number, so the fraction needs a space before
            // it; "½" alone must not gain one.
            if out.chars().last().is_some_and(|p| p.is_ascii_digit()) {
                out.push(' ');
            }
            out.push_str(frac);
        } else if c == '\u{a0}' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse_ingredient(line: &str) -> ParsedIngredient {
    let text = normalise(line);

    // A mass in brackets is the most useful thing on the line: "1 can (400 g)
    // tomatoes" says exactly what a can weighs. Taken before the brackets are
    // dropped from the name.
    let bracketed_grams = bracketed_mass_re().captures(&text).and_then(|caps| {
        let value = parse_number(caps.get(1)?.as_str())?;
        match unit_of(caps.get(2)?.as_str())? {
            Unit::Mass(_, factor) => Some(value * factor),
            Unit::Other(_) => None,
        }
    });

    let stripped = parenthetical_re().replace_all(&text, "").trim().to_string();
    let mut rest = stripped.as_str();

    let mut quantity = None;
    let mut unit: Option<Unit> = None;

    // "200g": the number and the unit arrive as one token.
    let first_token = rest.split_whitespace().next().unwrap_or_default();
    if let Some(caps) = glued_unit_re().captures(first_token) {
        if let (Some(value), Some(u)) = (
            parse_number(&caps[1]),
            caps.get(2).and_then(|m| unit_of(m.as_str())),
        ) {
            quantity = Some(value);
            unit = Some(u);
            rest = rest[first_token.len()..].trim_start();
        }
    }

    if quantity.is_none() {
        if let Some((value, used)) = take_quantity(rest) {
            quantity = Some(value);
            rest = rest[used..].trim_start();

            // "2 x 400g tins": a count of a measured thing. The measure wins
            // and the count multiplies it.
            let mut words = rest.splitn(2, ' ');
            let first = words.next().unwrap_or_default();
            if matches!(first, "x" | "X" | "×") {
                let after = words.next().unwrap_or_default().trim_start();
                let token = after.split_whitespace().next().unwrap_or_default();
                if let Some(caps) = glued_unit_re().captures(token) {
                    if let (Some(v), Some(u)) = (
                        parse_number(&caps[1]),
                        caps.get(2).and_then(|m| unit_of(m.as_str())),
                    ) {
                        quantity = Some(value * v);
                        unit = Some(u);
                        rest = after[token.len()..].trim_start();
                    }
                } else if let Some((v, used2)) = take_number(after) {
                    let tail = after[used2..].trim_start();
                    let token = tail.split_whitespace().next().unwrap_or_default();
                    if let Some(u) = unit_of(token) {
                        quantity = Some(value * v);
                        unit = Some(u);
                        rest = tail[token.len()..].trim_start();
                    }
                }
            }
        }
    }

    // The unit, when the quantity did not bring its own. "fl oz" is two
    // tokens for one unit; "fl" alone is nothing.
    if quantity.is_some() && unit.is_none() {
        let mut tokens = rest.splitn(2, ' ');
        let first = tokens.next().unwrap_or_default();
        let after = tokens.next().unwrap_or_default();
        if first.eq_ignore_ascii_case("fl") || first.eq_ignore_ascii_case("fl.") {
            let mut more = after.splitn(2, ' ');
            let second = more.next().unwrap_or_default();
            if second.eq_ignore_ascii_case("oz") || second.eq_ignore_ascii_case("oz.") {
                unit = Some(Unit::Other("fl oz"));
                rest = more.next().unwrap_or_default().trim_start();
            }
        } else if let Some(u) = unit_of(first) {
            // A unit needs something after it to measure: "2 large" is a
            // size, "2 large eggs" is two eggs. Either way the token is the
            // unit; only a lone trailing size word is kept as the name.
            if !after.trim().is_empty() {
                unit = Some(u);
                rest = after.trim_start();
            }
        }
    }

    // "of" after a measure is grammar, not an ingredient.
    let mut name = rest.trim().to_string();
    if let Some(after) = name
        .strip_prefix("of ")
        .or_else(|| name.strip_prefix("Of "))
    {
        name = after.trim_start().to_string();
    }
    name = name
        .trim_matches(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .to_string();
    if name.is_empty() {
        // A line that was only a measure ("2 large") keeps its words as the
        // name rather than vanishing.
        name = stripped.clone();
        quantity = None;
        unit = None;
    }

    let grams = match (quantity, unit) {
        (Some(q), Some(Unit::Mass(_, factor))) => Some(round2(q * factor)),
        _ => {
            bracketed_grams.map(|g| round2(g * quantity.filter(|_| unit.is_some()).unwrap_or(1.0)))
        }
    };

    ParsedIngredient {
        text,
        quantity: quantity.map(round3),
        unit: unit.map(|u| u.name().to_string()),
        name,
        grams,
    }
}

/// Words that describe preparation rather than identity, dropped from the
/// search term so "onion, finely chopped" finds onions.
const PREPARATION_WORDS: &[&str] = &[
    "fresh",
    "freshly",
    "chopped",
    "finely",
    "roughly",
    "diced",
    "minced",
    "sliced",
    "thinly",
    "thickly",
    "grated",
    "crushed",
    "peeled",
    "cored",
    "seeded",
    "deseeded",
    "halved",
    "quartered",
    "cubed",
    "shredded",
    "melted",
    "softened",
    "beaten",
    "sifted",
    "packed",
    "heaped",
    "heaping",
    "level",
    "ripe",
    "large",
    "medium",
    "small",
    "extra",
    "optional",
    "plus",
    "more",
    "for",
    "serving",
    "to",
    "taste",
    "cooked",
    "raw",
    "boneless",
    "skinless",
    "trimmed",
    "washed",
    "drained",
    "rinsed",
    "at",
    "room",
    "temperature",
    "divided",
    "or",
    "and",
    "about",
    "approximately",
    "roughly",
    "the",
    "of",
    "a",
    "an",
    "some",
    "few",
    "good",
    "handful",
    "pinch",
    "dash",
    "juice",
    "zest",
    "warm",
    "cold",
    "hot",
    "boiling",
    "lukewarm",
    "chilled",
    "frozen",
    "thawed",
    "defrosted",
    "toasted",
    "roasted",
    "ground",
    "whole",
    "unsalted",
    "salted",
];

impl ParsedIngredient {
    /// What to search the food database for: the name up to its first comma,
    /// with preparation words dropped and at most four words kept.
    pub fn search_term(&self) -> String {
        let base = self.name.split([',', ';']).next().unwrap_or_default();
        let words: Vec<&str> = base
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| !w.is_empty())
            .filter(|w| !PREPARATION_WORDS.contains(&w.to_lowercase().as_str()))
            .take(4)
            .collect();
        if words.is_empty() {
            base.trim().to_string()
        } else {
            words.join(" ")
        }
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(line: &str) -> ParsedIngredient {
        parse_ingredient(line)
    }

    #[test]
    fn grams_stay_grams() {
        let i = p("200 g flour");
        assert_eq!(i.quantity, Some(200.0));
        assert_eq!(i.unit.as_deref(), Some("g"));
        assert_eq!(i.name, "flour");
        assert_eq!(i.grams, Some(200.0));
    }

    #[test]
    fn a_number_glued_to_its_unit() {
        let i = p("200g butter, softened");
        assert_eq!(i.quantity, Some(200.0));
        assert_eq!(i.unit.as_deref(), Some("g"));
        assert_eq!(i.name, "butter, softened");
        assert_eq!(i.grams, Some(200.0));
        assert_eq!(i.search_term(), "butter");

        let i = p("1.5kg potatoes");
        assert_eq!(i.grams, Some(1500.0));
        assert_eq!(i.unit.as_deref(), Some("kg"));
    }

    #[test]
    fn mass_units_convert() {
        assert_eq!(p("2 oz cheddar").grams, Some(56.7));
        assert_eq!(p("1 lb ground beef").grams, Some(453.59));
        assert_eq!(p("1 pound ground beef").unit.as_deref(), Some("lb"));
        assert_eq!(p("500 mg salt").grams, Some(0.5));
    }

    #[test]
    fn household_measures_are_kept_not_guessed() {
        let i = p("1 1/2 cups all-purpose flour");
        assert_eq!(i.quantity, Some(1.5));
        assert_eq!(i.unit.as_deref(), Some("cup"));
        assert_eq!(i.name, "all-purpose flour");
        assert_eq!(i.grams, None);

        let i = p("2 tbsp olive oil");
        assert_eq!(i.unit.as_deref(), Some("tbsp"));
        assert_eq!(i.grams, None);

        let i = p("3 Tablespoons sugar");
        assert_eq!(i.unit.as_deref(), Some("tbsp"));
        assert_eq!(i.quantity, Some(3.0));
    }

    #[test]
    fn unicode_fractions() {
        assert_eq!(p("½ tsp salt").quantity, Some(0.5));
        assert_eq!(p("1½ cups milk").quantity, Some(1.5));
        assert_eq!(p("1 ½ cups milk").quantity, Some(1.5));
        assert_eq!(p("¾ cup sugar").unit.as_deref(), Some("cup"));
    }

    #[test]
    fn ranges_take_the_midpoint() {
        let i = p("2-3 cloves garlic, minced");
        assert_eq!(i.quantity, Some(2.5));
        assert_eq!(i.unit.as_deref(), Some("clove"));
        assert_eq!(i.name, "garlic, minced");
        assert_eq!(i.search_term(), "garlic");
        assert_eq!(p("2 to 3 eggs").quantity, Some(2.5));
        assert_eq!(p("2–3 eggs").quantity, Some(2.5));
    }

    #[test]
    fn a_bare_count_has_no_unit() {
        let i = p("2 eggs");
        assert_eq!(i.quantity, Some(2.0));
        assert_eq!(i.unit, None);
        assert_eq!(i.name, "eggs");
        assert_eq!(i.grams, None);
    }

    #[test]
    fn a_size_word_is_the_unit() {
        let i = p("2 large eggs");
        assert_eq!(i.quantity, Some(2.0));
        assert_eq!(i.unit.as_deref(), Some("large"));
        assert_eq!(i.name, "eggs");
    }

    #[test]
    fn brackets_carry_the_weight() {
        let i = p("1 can (400 g) chopped tomatoes");
        assert_eq!(i.quantity, Some(1.0));
        assert_eq!(i.unit.as_deref(), Some("can"));
        assert_eq!(i.name, "chopped tomatoes");
        assert_eq!(i.grams, Some(400.0));

        let i = p("2 cans (14 oz each) black beans, drained");
        assert_eq!(i.grams, Some(793.79));
        assert_eq!(i.name, "black beans, drained");

        // A non-mass bracket is dropped from the name and gives no grams.
        let i = p("1 cup (packed) brown sugar");
        assert_eq!(i.name, "brown sugar");
        assert_eq!(i.grams, None);
    }

    #[test]
    fn multiplied_measures() {
        let i = p("2 x 400g tins tomatoes");
        assert_eq!(i.quantity, Some(800.0));
        assert_eq!(i.unit.as_deref(), Some("g"));
        assert_eq!(i.grams, Some(800.0));
        assert_eq!(i.name, "tins tomatoes");
    }

    #[test]
    fn number_words_and_of() {
        let i = p("a pinch of salt");
        assert_eq!(i.quantity, Some(1.0));
        assert_eq!(i.unit.as_deref(), Some("pinch"));
        assert_eq!(i.name, "salt");

        let i = p("Two cloves of garlic");
        assert_eq!(i.quantity, Some(2.0));
        assert_eq!(i.name, "garlic");

        let i = p("half an onion");
        assert_eq!(i.quantity, Some(0.5));
        assert_eq!(i.name, "an onion");
    }

    #[test]
    fn no_amount_at_all() {
        let i = p("Salt and pepper to taste");
        assert_eq!(i.quantity, None);
        assert_eq!(i.unit, None);
        assert_eq!(i.name, "Salt and pepper to taste");
        assert_eq!(i.search_term(), "Salt pepper");
    }

    #[test]
    fn fluid_ounces_are_two_tokens() {
        let i = p("8 fl oz milk");
        assert_eq!(i.unit.as_deref(), Some("fl oz"));
        assert_eq!(i.name, "milk");
        assert_eq!(i.grams, None);
    }

    #[test]
    fn decimal_comma() {
        assert_eq!(p("1,5 kg flour").grams, Some(1500.0));
    }

    #[test]
    fn whitespace_is_tidied_but_the_text_is_kept() {
        let i = p("  200 g   flour  ");
        assert_eq!(i.text, "200 g flour");
    }

    #[test]
    fn search_term_drops_preparation_words() {
        assert_eq!(p("1 large onion, finely chopped").search_term(), "onion");
        assert_eq!(
            p("2 boneless skinless chicken breasts").search_term(),
            "chicken breasts"
        );
        assert_eq!(p("juice of 1 lemon").search_term(), "1 lemon");
    }
}
