//! Turning a recipe's name into the word that stands in its link.
//!
//! A shared recipe is handed to people as a URL, and `/r/pierogi-ruskie` says
//! what it is where `/r/3f7c…` says nothing. The rule here is the usual one —
//! lowercase, accents folded to their ASCII letter, everything else a single
//! hyphen — with two properties that matter more than the spelling:
//!
//!   * **It is generated in one place.** Create and rename both call
//!     `slugify`, so two recipes with the same name cannot be slugged by two
//!     slightly different rules.
//!   * **A slug is never reused by a different recipe.** Every slug a recipe
//!     has ever had is kept in `recipe_slug_history`, whose primary key is the
//!     slug itself, and the next free suffix is chosen against that history
//!     rather than against the recipes currently holding one. A link somebody
//!     sent last year therefore either resolves to the recipe it always meant
//!     or resolves to nothing — never to somebody else's dinner.

use std::collections::HashSet;

use uuid::Uuid;

/// How long a generated slug may get. Long enough for a real recipe name,
/// short enough to survive being pasted into a chat window unwrapped.
pub const MAX_LEN: usize = 60;

/// Latin letters that carry a mark, and the ASCII they fold to.
///
/// Written out rather than pulled from a normalisation crate: this is the
/// alphabet a recipe name is actually written in, the table is checked by the
/// tests below, and a dependency whose job is to know about Han unification
/// is a lot of machinery for "é is an e". Anything outside the table is not
/// mangled into a wrong letter — it becomes a separator, and a name with no
/// ASCII in it at all falls back to `fallback_slug`.
const FOLDED: &[(&str, &str)] = &[
    ("àáâãäåāăą", "a"),
    ("æ", "ae"),
    ("çćĉċč", "c"),
    ("ďđ", "d"),
    ("ð", "d"),
    ("èéêëēĕėęě", "e"),
    ("ĝğġģ", "g"),
    ("ĥħ", "h"),
    ("ìíîïĩīĭįı", "i"),
    ("ĵ", "j"),
    ("ķ", "k"),
    ("ĺļľŀł", "l"),
    ("ñńņňŉ", "n"),
    ("òóôõöøōŏő", "o"),
    ("œ", "oe"),
    ("ŕŗř", "r"),
    ("śŝşš", "s"),
    ("ß", "ss"),
    ("ţťŧ", "t"),
    ("þ", "th"),
    ("ùúûüũūŭůűų", "u"),
    ("ŵ", "w"),
    ("ýÿŷ", "y"),
    ("źżž", "z"),
];

fn fold(ch: char) -> Option<&'static str> {
    FOLDED
        .iter()
        .find(|(from, _)| from.chars().any(|c| c == ch))
        .map(|(_, to)| *to)
}

/// The slug a name wants, before anything is known about collisions.
///
/// Empty when the name has nothing sluggable in it — a name of only emoji, or
/// only punctuation. That is a real answer rather than a failure, and the
/// caller substitutes `fallback_slug`.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for ch in name.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if let Some(folded) = fold(ch) {
            out.push_str(folded);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    truncate(out.trim_matches('-'), MAX_LEN)
}

/// Cut to at most `max` characters, on a word boundary where there is one.
///
/// Cutting mid-word reads like a typo, so the last hyphen inside the budget
/// wins — unless the first word alone is longer than the budget, in which
/// case there is nothing to do but cut it.
fn truncate(slug: &str, max: usize) -> String {
    if slug.len() <= max {
        return slug.to_string();
    }
    let head = &slug[..max];
    match head.rfind('-') {
        Some(cut) if cut > 0 => head[..cut].to_string(),
        _ => head.trim_end_matches('-').to_string(),
    }
}

/// What a recipe whose name yields nothing is called: short, stable, and
/// obviously a fallback rather than a mangled attempt at the name.
pub fn fallback_slug(id: Uuid) -> String {
    format!("recipe-{}", &id.simple().to_string()[..8])
}

/// The first slug built on `base` that nobody has ever held.
///
/// `taken` is every slug ever issued that could clash — the base itself and
/// anything suffixed from it — so the suffix counts through history, not
/// through what happens to be in use today. The cap is respected with the
/// suffix included, so a 60-character name does not grow a 62-character slug.
pub fn free_slug(base: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_string();
    }
    for n in 2..=1000 {
        let suffix = format!("-{n}");
        let head = truncate(base, MAX_LEN.saturating_sub(suffix.len()));
        let candidate = format!("{head}{suffix}");
        if !taken.contains(&candidate) {
            return candidate;
        }
    }
    // A thousand recipes of the same name on one instance is not a case worth
    // a cleverer scheme; a random tail always terminates.
    format!(
        "{}-{}",
        truncate(base, MAX_LEN - 9),
        &Uuid::new_v4().simple().to_string()[..8]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taken(slugs: &[&str]) -> HashSet<String> {
        slugs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_plain_name_becomes_plain_words() {
        assert_eq!(slugify("Roast Chicken"), "roast-chicken");
    }

    #[test]
    fn accents_fold_to_the_letter_underneath() {
        assert_eq!(slugify("Crème Brûlée"), "creme-brulee");
        assert_eq!(slugify("Æbleskiver"), "aebleskiver");
        assert_eq!(slugify("Smørrebrød"), "smorrebrod");
        assert_eq!(
            slugify("Gulasch mit Spätzle und Straße"),
            "gulasch-mit-spatzle-und-strasse"
        );
        assert_eq!(
            slugify("Pierogi ruskie z łososiem"),
            "pierogi-ruskie-z-lososiem"
        );
    }

    #[test]
    fn punctuation_becomes_one_hyphen_and_never_two() {
        assert_eq!(slugify("Mum's  (best!!) — pie"), "mum-s-best-pie");
        assert_eq!(slugify("Chicken & Rice / 2 ways"), "chicken-rice-2-ways");
    }

    #[test]
    fn leading_and_trailing_separators_are_trimmed() {
        assert_eq!(slugify("  ...Soup!  "), "soup");
        assert_eq!(slugify("--- Stew ---"), "stew");
    }

    #[test]
    fn a_name_with_nothing_sluggable_comes_back_empty() {
        assert_eq!(slugify("🍕🍕🍕"), "");
        assert_eq!(slugify("!!!"), "");
        assert_eq!(slugify("   "), "");
        // Not mangled into wrong letters either: a name we cannot romanise is
        // better served by the fallback than by silently losing its meaning.
        assert_eq!(slugify("餃子"), "");
    }

    #[test]
    fn the_fallback_is_short_and_says_what_it_is() {
        let id = Uuid::parse_str("3f7c1a2b-0000-4000-8000-000000000000").unwrap();
        assert_eq!(fallback_slug(id), "recipe-3f7c1a2b");
    }

    #[test]
    fn a_long_name_is_capped_on_a_word_boundary() {
        let slug =
            slugify("Slow roasted tomato and red pepper soup with basil oil and a swirl of cream");
        assert!(slug.len() <= MAX_LEN, "{slug} is {} long", slug.len());
        assert_eq!(
            slug,
            "slow-roasted-tomato-and-red-pepper-soup-with-basil-oil-and"
        );
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn one_very_long_word_is_cut_rather_than_emptied() {
        let slug = slugify(&"a".repeat(200));
        assert_eq!(slug.len(), MAX_LEN);
    }

    #[test]
    fn a_free_slug_is_the_base_when_nobody_holds_it() {
        assert_eq!(free_slug("pasta", &taken(&["soup"])), "pasta");
    }

    #[test]
    fn collisions_count_upwards_through_every_slug_ever_issued() {
        assert_eq!(free_slug("pasta", &taken(&["pasta"])), "pasta-2");
        assert_eq!(free_slug("pasta", &taken(&["pasta", "pasta-2"])), "pasta-3");
        // "pasta-2" belonging to a recipe that has since been renamed still
        // counts: history is what is consulted, not current holders.
        assert_eq!(
            free_slug("pasta", &taken(&["pasta", "pasta-2", "pasta-3", "pasta-4"])),
            "pasta-5"
        );
    }

    #[test]
    fn a_suffix_never_pushes_a_slug_past_the_cap() {
        let base = slugify(&"ragu ".repeat(40));
        assert_eq!(base.len(), 59);
        let mut held = taken(&[]);
        held.insert(base.clone());
        let next = free_slug(&base, &held);
        assert!(next.len() <= MAX_LEN, "{next} is {} long", next.len());
        assert!(next.ends_with("-2"));
        assert!(!next.contains("--"));
    }
}
