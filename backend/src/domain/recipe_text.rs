//! Turning free-text instructions into steps.
//!
//! Instructions are stored as one string because that is what people type: a
//! line per step, usually, sometimes with their own "1." in front, sometimes
//! as a single paragraph with the numbers inline. The recipe page numbers the
//! steps, and the Markdown export does too, so both need the same split — the
//! frontend's `lib/recipeText.ts` is this same rule in JavaScript, and the
//! two are kept in step by the test cases below, which are the same cases.

use std::sync::OnceLock;

use regex::Regex;

/// "1.", "1)", "Step 1:", "- ", "• " — the markers people write themselves.
fn leading_marker() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(?:(?:step\s*)?\d{1,3}\s*[.):\-–]|[-*•])\s*").unwrap())
}

/// A number-dot-space in the middle of a paragraph: "…then 2. Add the…".
/// The whitespace before the number is consumed; the number itself is kept
/// for the leading-marker pass to strip, exactly as the lookahead split in
/// the TypeScript does.
fn inline_marker() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+(\d{1,3}[.)]\s)").unwrap())
}

fn opens_with_number() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d{1,3}[.)]\s").unwrap())
}

pub fn instruction_steps(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();

    // One long line reading "1. … 2. … 3. …" is a list that lost its line
    // breaks, not a paragraph. It has to open with a number to count: a real
    // paragraph that happens to contain "2. " partway through is left alone.
    if lines.len() == 1 && opens_with_number().is_match(&lines[0]) {
        let line = lines.pop().unwrap_or_default();
        let mut start = 0;
        for cap in inline_marker().captures_iter(&line) {
            let marker = cap.get(1).expect("group 1 is not optional");
            lines.push(line[start..marker.start()].to_string());
            start = marker.start();
        }
        lines.push(line[start..].to_string());
    }

    lines
        .iter()
        .map(|l| leading_marker().replace(l, "").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every case here has a twin in the frontend's `recipeText.ts`; a change
    // to the rule has to land in both.

    #[test]
    fn one_step_per_line() {
        assert_eq!(
            instruction_steps("Preheat the oven.\nToss the veg.\n\nRoast.\n"),
            vec!["Preheat the oven.", "Toss the veg.", "Roast."]
        );
    }

    #[test]
    fn strips_the_numbers_people_add_themselves() {
        assert_eq!(
            instruction_steps("1. Preheat\n2) Toss\n3 - Roast\nStep 4: Serve\nstep5. Eat"),
            vec!["Preheat", "Toss", "Roast", "Serve", "Eat"]
        );
    }

    #[test]
    fn strips_bullets() {
        assert_eq!(
            instruction_steps("- Preheat\n* Toss\n• Roast"),
            vec!["Preheat", "Toss", "Roast"]
        );
    }

    #[test]
    fn splits_a_numbered_list_that_lost_its_line_breaks() {
        assert_eq!(
            instruction_steps(
                "1. Preheat the oven to 200°C. 2. Toss the veg. 3. Roast for 25 min."
            ),
            vec![
                "Preheat the oven to 200°C.",
                "Toss the veg.",
                "Roast for 25 min."
            ]
        );
    }

    #[test]
    fn leaves_a_real_paragraph_alone() {
        // Contains "2. " but does not open with a number, so it is one step.
        let text = "Mix everything, then after 2. hours check on it.";
        assert_eq!(instruction_steps(text), vec![text]);
    }

    #[test]
    fn windows_line_endings_and_blank_lines() {
        assert_eq!(
            instruction_steps("Preheat\r\n\r\nToss\r\n"),
            vec!["Preheat", "Toss"]
        );
    }

    #[test]
    fn empty_in_empty_out() {
        assert!(instruction_steps("").is_empty());
        assert!(instruction_steps("  \n \n").is_empty());
        assert!(instruction_steps("1.\n2.").is_empty());
    }

    #[test]
    fn a_number_that_is_content_is_kept() {
        // "200 g" is not a step marker: no dot or bracket follows the digits.
        assert_eq!(
            instruction_steps("200 g of flour goes in first"),
            vec!["200 g of flour goes in first"]
        );
    }
}
