//! Finding a schema.org `Recipe` in a web page.
//!
//! Recipe sites almost universally embed one as JSON-LD in a
//! `<script type="application/ld+json">` block, because that is what search
//! engines read. The shape varies more than the standard suggests — a bare
//! object, an array, a `@graph`, a type given as a list, instructions as one
//! string or as `HowToStep`s inside `HowToSection`s — so this reads all of
//! those and hands back one flat structure. No HTML parser: the blocks are
//! located by scanning for the script tag, which is enough and keeps a page
//! with broken markup elsewhere from failing the import.

use serde_json::Value;

/// A recipe as a page describes it, before any resolving against foods.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScrapedRecipe {
    pub name: String,
    pub description: Option<String>,
    /// From `recipeYield`: the first number in it, if any.
    pub servings: Option<f64>,
    /// One line per ingredient, as written.
    pub ingredients: Vec<String>,
    /// One line per step, ready for `instruction_steps`.
    pub instructions: Vec<String>,
    pub image: Option<String>,
    pub author: Option<String>,
}

/// Every JSON-LD block in the page, parsed. Blocks that fail to parse are
/// skipped: one broken block must not hide a good one further down.
pub fn jsonld_blocks(html: &str) -> Vec<Value> {
    let lower = html.to_ascii_lowercase();
    let mut blocks = Vec::new();
    let mut from = 0;

    while let Some(rel) = lower[from..].find("<script") {
        let open = from + rel;
        let Some(rel_end) = lower[open..].find('>') else {
            break;
        };
        let tag_end = open + rel_end + 1;
        let attributes = &lower[open..tag_end];
        let Some(rel_close) = lower[tag_end..].find("</script") else {
            break;
        };
        let close = tag_end + rel_close;

        if attributes.contains("application/ld+json") {
            let body = html[tag_end..close].trim();
            // Some sites wrap the JSON in an HTML comment or CDATA marker.
            let body = body
                .trim_start_matches("<!--")
                .trim_end_matches("-->")
                .trim_start_matches("//<![CDATA[")
                .trim_end_matches("//]]>")
                .trim();
            if let Ok(value) = serde_json::from_str::<Value>(body) {
                blocks.push(value);
            }
        }
        from = close;
    }

    blocks
}

/// The first schema.org `Recipe` in a page.
pub fn recipe_from_html(html: &str) -> Option<ScrapedRecipe> {
    jsonld_blocks(html).iter().find_map(recipe_from_json)
}

/// The first `Recipe` in a JSON-LD document, looking through arrays,
/// `@graph` and the `mainEntity` of a page node.
pub fn recipe_from_json(value: &Value) -> Option<ScrapedRecipe> {
    find_recipe(value, 0).map(read_recipe)
}

fn find_recipe(value: &Value, depth: usize) -> Option<&Value> {
    if depth > 8 {
        return None;
    }
    match value {
        Value::Array(items) => items.iter().find_map(|v| find_recipe(v, depth + 1)),
        Value::Object(map) => {
            if is_type(value, "Recipe") {
                return Some(value);
            }
            for key in [
                "@graph",
                "mainEntity",
                "mainEntityOfPage",
                "itemListElement",
            ] {
                if let Some(found) = map.get(key).and_then(|v| find_recipe(v, depth + 1)) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

/// `@type` may be a string or a list of strings; the match is case-insensitive
/// because both "Recipe" and "recipe" are seen in the wild.
fn is_type(value: &Value, wanted: &str) -> bool {
    match value.get("@type") {
        Some(Value::String(t)) => t.eq_ignore_ascii_case(wanted),
        Some(Value::Array(ts)) => ts
            .iter()
            .filter_map(Value::as_str)
            .any(|t| t.eq_ignore_ascii_case(wanted)),
        _ => false,
    }
}

fn read_recipe(recipe: &Value) -> ScrapedRecipe {
    ScrapedRecipe {
        name: text_of(recipe.get("name")).unwrap_or_default(),
        description: text_of(recipe.get("description")),
        servings: recipe.get("recipeYield").and_then(first_number),
        ingredients: recipe
            .get("recipeIngredient")
            .or_else(|| recipe.get("ingredients"))
            .map(string_list)
            .unwrap_or_default(),
        instructions: recipe
            .get("recipeInstructions")
            .map(|v| {
                let mut steps = Vec::new();
                collect_steps(v, &mut steps, 0);
                steps
            })
            .unwrap_or_default(),
        image: recipe.get("image").and_then(first_url),
        author: recipe.get("author").and_then(name_of),
    }
}

/// Instructions arrive as a string, a list of strings, `HowToStep` objects,
/// `HowToSection`s holding steps, or an `ItemList` wrapping any of those.
/// Section names are dropped: the steps are stored one per line and the
/// page numbers them, so a heading in the middle would become a step.
fn collect_steps(value: &Value, out: &mut Vec<String>, depth: usize) {
    if depth > 6 {
        return;
    }
    match value {
        Value::String(s) => {
            // One string may hold every step separated by newlines.
            for line in s.split('\n') {
                if let Some(line) = clean_text(line) {
                    out.push(line);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_steps(item, out, depth + 1);
            }
        }
        Value::Object(map) => {
            if let Some(list) = map.get("itemListElement") {
                collect_steps(list, out, depth + 1);
            } else if let Some(text) = map.get("text").and_then(|t| text_of(Some(t))) {
                out.push(text);
            } else if let Some(name) = map.get("name").and_then(|t| text_of(Some(t))) {
                out.push(name);
            }
        }
        _ => {}
    }
}

fn string_list(value: &Value) -> Vec<String> {
    match value {
        Value::String(s) => s.split('\n').filter_map(clean_text).collect(),
        Value::Array(items) => items
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => clean_text(s),
                Value::Object(_) => text_of(v.get("name")).or_else(|| text_of(v.get("text"))),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A text field, whichever of the ways it can be wrapped: a string, a list
/// whose first entry is a string, or an object with `@value`.
fn text_of(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => clean_text(s),
        Value::Array(items) => items.iter().find_map(|v| text_of(Some(v))),
        Value::Object(map) => text_of(map.get("@value")),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn name_of(value: &Value) -> Option<String> {
    match value {
        Value::String(_) => text_of(Some(value)),
        Value::Array(items) => items.iter().find_map(name_of),
        Value::Object(map) => text_of(map.get("name")),
        _ => None,
    }
}

fn first_url(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Value::Array(items) => items.iter().find_map(first_url),
        Value::Object(map) => map
            .get("url")
            .or_else(|| map.get("contentUrl"))
            .and_then(first_url),
        _ => None,
    }
}

/// `recipeYield` is "4", 4, "4 servings", "Makes 12 muffins", or a list
/// like ["4", "4 servings"]. The first number in the first usable entry.
fn first_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64().filter(|v| *v > 0.0),
        Value::String(s) => {
            let mut digits = String::new();
            for c in s.chars() {
                if c.is_ascii_digit() || (c == '.' && !digits.is_empty()) {
                    digits.push(c);
                } else if !digits.is_empty() {
                    break;
                }
            }
            digits.parse().ok().filter(|v: &f64| *v > 0.0)
        }
        Value::Array(items) => items.iter().find_map(first_number),
        _ => None,
    }
}

/// Strip tags, decode the entities that turn up in recipe text, and collapse
/// whitespace. None for a line that was nothing but markup.
pub fn clean_text(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for c in raw.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            _ => out.push(c),
        }
    }
    let decoded = decode_entities(&out);
    let collapsed = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(collapsed).filter(|s| !s.is_empty())
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "deg" => Some('°'),
            "frac12" => Some('½'),
            "frac14" => Some('¼'),
            "frac34" => Some('¾'),
            _ => entity
                .strip_prefix('#')
                .and_then(|n| {
                    n.strip_prefix(['x', 'X'])
                        .and_then(|h| u32::from_str_radix(h, 16).ok())
                        .or_else(|| n.parse().ok())
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &str = r#"<html><head>
        <script type="application/ld+json">
        {"@context":"https://schema.org","@type":"Recipe","name":"Pancakes",
         "description":"Fluffy &amp; quick","recipeYield":"4 servings",
         "recipeIngredient":["200 g flour","2 eggs","300 ml milk"],
         "recipeInstructions":"Mix.\nFry.","image":"https://x.test/p.jpg",
         "author":{"@type":"Person","name":"Ada"}}
        </script></head><body></body></html>"#;

    #[test]
    fn a_plain_recipe_block() {
        let r = recipe_from_html(PLAIN).expect("recipe");
        assert_eq!(r.name, "Pancakes");
        assert_eq!(r.description.as_deref(), Some("Fluffy & quick"));
        assert_eq!(r.servings, Some(4.0));
        assert_eq!(r.ingredients, vec!["200 g flour", "2 eggs", "300 ml milk"]);
        assert_eq!(r.instructions, vec!["Mix.", "Fry."]);
        assert_eq!(r.image.as_deref(), Some("https://x.test/p.jpg"));
        assert_eq!(r.author.as_deref(), Some("Ada"));
    }

    const GRAPH: &str = r#"<html><head>
        <SCRIPT type='application/ld+json' class="yoast">
        {"@context":"https://schema.org","@graph":[
          {"@type":"WebPage","name":"A page"},
          {"@type":["Recipe","NewsArticle"],"name":"Curry","recipeYield":["6","6 bowls"],
           "recipeIngredient":"1 onion\n2 cloves garlic",
           "recipeInstructions":[
             {"@type":"HowToSection","name":"Sauce","itemListElement":[
               {"@type":"HowToStep","text":"Fry the <b>onion</b>."},
               {"@type":"HowToStep","text":"Add garlic &#233;."}]},
             {"@type":"HowToStep","name":"Serve with rice"}],
           "image":{"@type":"ImageObject","url":"https://x.test/c.jpg"},
           "author":[{"@type":"Person","name":"Bo"}]}]}
        </SCRIPT></head></html>"#;

    #[test]
    fn a_graph_with_sections_and_a_type_list() {
        let r = recipe_from_html(GRAPH).expect("recipe");
        assert_eq!(r.name, "Curry");
        assert_eq!(r.servings, Some(6.0));
        assert_eq!(r.ingredients, vec!["1 onion", "2 cloves garlic"]);
        assert_eq!(
            r.instructions,
            vec!["Fry the onion.", "Add garlic é.", "Serve with rice"]
        );
        assert_eq!(r.image.as_deref(), Some("https://x.test/c.jpg"));
        assert_eq!(r.author.as_deref(), Some("Bo"));
    }

    #[test]
    fn a_broken_block_does_not_hide_a_good_one() {
        let html = r#"<script type="application/ld+json">{not json</script>
            <script type="text/javascript">var x = 1;</script>
            <script type="application/ld+json">[{"@type":"Recipe","name":"Soup",
              "recipeIngredient":["1 leek"],"recipeInstructions":[]}]</script>"#;
        let r = recipe_from_html(html).expect("recipe");
        assert_eq!(r.name, "Soup");
        assert_eq!(r.ingredients, vec!["1 leek"]);
        assert!(r.instructions.is_empty());
        assert_eq!(r.servings, None);
    }

    #[test]
    fn no_recipe_is_none() {
        assert!(recipe_from_html("<html><body>hello</body></html>").is_none());
        let html = r#"<script type="application/ld+json">{"@type":"Article","name":"x"}</script>"#;
        assert!(recipe_from_html(html).is_none());
    }

    #[test]
    fn a_page_node_pointing_at_its_recipe() {
        let v: Value = serde_json::from_str(
            r#"{"@type":"WebPage","mainEntity":{"@type":"recipe","name":"Toast",
                "recipeYield":2,"recipeInstructions":{"@type":"ItemList",
                "itemListElement":[{"@type":"HowToStep","text":"Toast it"}]}}}"#,
        )
        .unwrap();
        let r = recipe_from_json(&v).expect("recipe");
        assert_eq!(r.name, "Toast");
        assert_eq!(r.servings, Some(2.0));
        assert_eq!(r.instructions, vec!["Toast it"]);
    }

    #[test]
    fn yields_are_read_for_their_number() {
        assert_eq!(
            first_number(&Value::String("Makes 12 muffins".into())),
            Some(12.0)
        );
        assert_eq!(first_number(&Value::String("4-6".into())), Some(4.0));
        assert_eq!(first_number(&Value::String("serves a crowd".into())), None);
        assert_eq!(
            first_number(&serde_json::json!(["", "8 servings"])),
            Some(8.0)
        );
    }

    #[test]
    fn entities_and_tags() {
        assert_eq!(
            clean_text("  Heat to 200&deg;C &amp; <em>rest</em>&nbsp;10 min &#x2014; done "),
            Some("Heat to 200°C & rest 10 min — done".into())
        );
        assert_eq!(clean_text("<br/>"), None);
        assert_eq!(clean_text("Tom & Jerry"), Some("Tom & Jerry".into()));
    }
}
