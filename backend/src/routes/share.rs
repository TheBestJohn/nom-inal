//! The page a shared recipe's link opens.
//!
//! `/r/{slug}` is served by the API as a finished HTML document, not by the
//! SPA. That is the whole point of this module, and it is worth saying why,
//! because "inject a few meta tags into index.html" is the smaller change.
//!
//!   * **Crawlers do not run JavaScript.** Slack, iMessage, Discord, Facebook
//!     and Twitter fetch the URL, read the `<head>`, and stop. When the head
//!     is a static shell, every shared recipe previews identically — which is
//!     the bug this fixes. Nothing the React app does to `document.title`
//!     after it boots can be seen by any of them.
//!   * **The reader usually has no account and is on a phone.** Making them
//!     download an application bundle to read a list of ingredients is the
//!     wrong trade. This page is a few kilobytes of HTML with its CSS inline
//!     and no script at all, so it paints on the first response.
//!   * **It prints, and it works with JavaScript off.** Both fall out of
//!     being a document rather than an application.
//!
//! The template is hand-rolled string building. A templating crate would buy
//! auto-escaping and cost a dependency plus a build step; instead there is
//! one [`escape_html`] and a rule with no exceptions — every interpolated
//! value goes through it. A recipe called `Mum's "<best>" pie` has to render
//! as a recipe name, in the body and in the meta tags alike, rather than as
//! markup.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde_json::{json, Value};

use crate::domain::photo::Photo;
use crate::domain::recipe::{r1, trim_float, Recipe};
use crate::domain::recipe_text::instruction_steps;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/{key}", get(page))
}

/// `GET /r/{slug-or-uuid}`.
///
/// Deliberately outside `/api/v1`: this is a page, and versioning the address
/// of a page that people paste into messages would defeat the purpose of
/// giving it a stable one.
pub async fn page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    let origin = public_origin(&state, &headers);
    match assemble(&state, &key, &origin).await {
        Ok(response) => response,
        Err(ApiError::NotFound(_)) => not_shared(&origin),
        Err(other) => {
            tracing::error!(error = ?other, "shared recipe page failed");
            html(
                StatusCode::INTERNAL_SERVER_ERROR,
                &message_page(
                    &origin,
                    "Something went wrong",
                    "Please try that link again.",
                ),
                "no-store",
            )
        }
    }
}

async fn assemble(state: &AppState, key: &str, origin: &str) -> Result<Response, ApiError> {
    let resolved = super::public::resolve(state, key).await?;
    if !resolved.is_public {
        return Err(ApiError::NotFound("recipe"));
    }

    // A link that arrived by an old slug, or by the raw uuid, still works —
    // and then says once, permanently, where the recipe lives now. One
    // canonical address per recipe is what keeps a crawler from indexing the
    // same dish three times.
    if !resolved.canonical {
        return Ok((
            StatusCode::MOVED_PERMANENTLY,
            [(header::LOCATION, canonical_url(origin, &resolved.slug))],
        )
            .into_response());
    }

    let recipe = super::recipes::load_recipe(state, None, resolved.id).await?;
    let photos = super::photos::list_public(state, resolved.id).await?;

    let body = render_page(&Page {
        recipe: &recipe,
        photos: &photos,
        origin,
    });
    // Short, not immutable: the author may correct the recipe at any moment,
    // and a crawler re-reading it an hour later should see the correction.
    Ok(html(StatusCode::OK, &body, "public, max-age=300"))
}

fn html(status: StatusCode, body: &str, cache: &'static str) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, cache),
        ],
        body.to_string(),
    )
        .into_response()
}

/// What a link to a recipe that is not shared (any more) looks like.
///
/// A small styled page rather than a bare 404 body: the person following the
/// link did nothing wrong, and "this is not shared" is a different fact from
/// "this address is broken".
fn not_shared(origin: &str) -> Response {
    html(
        StatusCode::NOT_FOUND,
        &message_page(
            origin,
            "This recipe is not shared",
            "Its author may have made it private again, or the link may be wrong.",
        ),
        "no-store",
    )
}

// ---------------------------------------------------------------------------
// Where "here" is
// ---------------------------------------------------------------------------

/// The origin to build absolute URLs from, with no trailing slash.
///
/// Open Graph and canonical links have to be absolute, and the server behind
/// a reverse proxy cannot know its public address from its own socket. nginx
/// forwards both halves of it — `X-Forwarded-Proto` and `Host` — so that is
/// the default, and `PUBLIC_ORIGIN` overrides it for a deployment whose proxy
/// does not, or which is reached by a name the proxy does not pass on.
///
/// The host is checked against the characters a host name can contain before
/// it is used. It arrives in a request header, which means a caller chose it,
/// and a URL assembled out of one is exactly the sort of thing that ends up
/// in someone else's timeline.
pub fn public_origin(state: &AppState, headers: &HeaderMap) -> String {
    match state.config.public_origin.as_deref() {
        Some(configured) => configured.trim_end_matches('/').to_string(),
        None => origin_from_headers(headers),
    }
}

/// The half of [`public_origin`] that reads the request, kept separate so it
/// can be tested without a database pool behind it.
fn origin_from_headers(headers: &HeaderMap) -> String {
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| *s == "http" || *s == "https")
        .unwrap_or("http");

    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|h| !h.is_empty() && h.len() <= 255 && h.chars().all(is_host_char))
        .unwrap_or("localhost");

    format!("{scheme}://{host}")
}

fn is_host_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']')
}

pub fn canonical_url(origin: &str, slug: &str) -> String {
    format!("{origin}/r/{slug}")
}

/// The preview image's address. Under `/api/v1/public/` with the other things
/// a stranger may fetch, so one rule covers all of them, and with a `.png`
/// suffix because some crawlers will not look at an `og:image` without one.
pub fn preview_url(origin: &str, slug: &str) -> String {
    format!("{origin}/api/v1/public/recipes/{slug}/preview.png")
}

// ---------------------------------------------------------------------------
// Escaping
// ---------------------------------------------------------------------------

/// The one escaper. Everything interpolated into the document goes through
/// it — body text, attribute values, meta tag contents — with no exceptions,
/// because the exception is always the one that gets a recipe name with a
/// quotation mark in it.
pub fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// JSON-LD sits inside a `<script>` element, where the HTML parser is looking
/// for `</script` and nothing else. Escaping the three characters that could
/// start one keeps the block valid JSON — `\u003c` parses as `<` — while
/// making it impossible for a recipe's text to close the element early.
fn escape_json_ld(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

// ---------------------------------------------------------------------------
// The document
// ---------------------------------------------------------------------------

pub struct Page<'a> {
    pub recipe: &'a Recipe,
    pub photos: &'a [Photo],
    /// Absolute, no trailing slash.
    pub origin: &'a str,
}

impl Page<'_> {
    fn canonical(&self) -> String {
        canonical_url(self.origin, &self.recipe.slug)
    }

    fn preview(&self) -> String {
        preview_url(self.origin, &self.recipe.slug)
    }

    /// What the link preview says under the title.
    ///
    /// The author's own description when there is one; otherwise a sentence
    /// assembled from what the recipe knows about itself, because "nom-inal —
    /// track what you eat" under every shared recipe is the generic preview
    /// this page exists to replace.
    fn description(&self) -> String {
        if let Some(text) = self
            .recipe
            .description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            return text.chars().take(300).collect();
        }
        let count = self.recipe.items.len();
        format!(
            "A recipe with {count} ingredient{}, {} kcal per serving.",
            if count == 1 { "" } else { "s" },
            self.recipe.per_serving.calories_kcal.round()
        )
    }
}

pub fn render_page(page: &Page) -> String {
    let recipe = page.recipe;
    let name = escape_html(&recipe.name);
    let description = escape_html(&page.description());
    let canonical = escape_html(&page.canonical());
    let preview = escape_html(&page.preview());

    let mut out = String::with_capacity(8 * 1024);
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str(&format!("<title>{name} — nom-inal</title>\n"));
    out.push_str(&format!(
        "<meta name=\"description\" content=\"{description}\">\n"
    ));
    out.push_str(&format!("<link rel=\"canonical\" href=\"{canonical}\">\n"));

    // Open Graph is what Slack, iMessage, Discord, WhatsApp and Facebook read.
    out.push_str("<meta property=\"og:type\" content=\"article\">\n");
    out.push_str("<meta property=\"og:site_name\" content=\"nom-inal\">\n");
    out.push_str(&format!(
        "<meta property=\"og:title\" content=\"{name}\">\n"
    ));
    out.push_str(&format!(
        "<meta property=\"og:description\" content=\"{description}\">\n"
    ));
    out.push_str(&format!(
        "<meta property=\"og:url\" content=\"{canonical}\">\n"
    ));
    out.push_str(&format!(
        "<meta property=\"og:image\" content=\"{preview}\">\n"
    ));
    out.push_str("<meta property=\"og:image:width\" content=\"1200\">\n");
    out.push_str("<meta property=\"og:image:height\" content=\"630\">\n");
    out.push_str(&format!(
        "<meta property=\"og:image:alt\" content=\"{name}\">\n"
    ));

    // Twitter reads its own names, and falls back to Open Graph for the rest.
    out.push_str("<meta name=\"twitter:card\" content=\"summary_large_image\">\n");
    out.push_str(&format!(
        "<meta name=\"twitter:title\" content=\"{name}\">\n"
    ));
    out.push_str(&format!(
        "<meta name=\"twitter:description\" content=\"{description}\">\n"
    ));
    out.push_str(&format!(
        "<meta name=\"twitter:image\" content=\"{preview}\">\n"
    ));
    out.push_str(&format!(
        "<meta name=\"twitter:image:alt\" content=\"{name}\">\n"
    ));

    out.push_str("<style>\n");
    out.push_str(STYLE);
    out.push_str("</style>\n");

    out.push_str("<script type=\"application/ld+json\">");
    out.push_str(&escape_json_ld(&json_ld(page).to_string()));
    out.push_str("</script>\n");
    out.push_str("</head>\n<body>\n");

    out.push_str(&format!(
        "<header class=\"bar\"><a class=\"mark\" href=\"{origin}/\"><span aria-hidden=\"true\">🥗</span> nom-inal</a></header>\n",
        origin = escape_html(page.origin)
    ));

    out.push_str("<main>\n<article>\n");
    out.push_str(&format!("<h1>{name}</h1>\n"));
    out.push_str(&format!("<p class=\"meta\">{}</p>\n", byline(recipe)));

    if let Some(text) = recipe
        .description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        out.push_str(&format!("<p class=\"lede\">{}</p>\n", escape_html(text)));
    }

    if !page.photos.is_empty() {
        out.push_str("<div class=\"photos\">\n");
        for photo in page.photos {
            let caption = photo.caption.as_deref().unwrap_or(&recipe.name);
            out.push_str(&format!(
                "<figure><img src=\"{src}\" alt=\"{alt}\" width=\"{w}\" height=\"{h}\" loading=\"lazy\">",
                src = escape_html(&photo.url),
                alt = escape_html(caption),
                w = photo.width,
                h = photo.height,
            ));
            if let Some(text) = photo
                .caption
                .as_deref()
                .map(str::trim)
                .filter(|c| !c.is_empty())
            {
                out.push_str(&format!("<figcaption>{}</figcaption>", escape_html(text)));
            }
            out.push_str("</figure>\n");
        }
        out.push_str("</div>\n");
    }

    out.push_str("<h2>Ingredients</h2>\n<ul class=\"ingredients\">\n");
    for item in ingredient_rows(recipe) {
        out.push_str("<li>");
        if item.amount.is_empty() {
            out.push_str(&format!(
                "<span class=\"what\">{}</span>",
                escape_html(&item.name)
            ));
        } else {
            out.push_str(&format!(
                "<span class=\"amount\">{}</span><span class=\"dot\" aria-hidden=\"true\">·</span><span class=\"what\">{}</span>",
                escape_html(&item.amount),
                escape_html(&item.name),
            ));
        }
        if let Some(note) = &item.note {
            out.push_str(&format!(
                "<span class=\"note\">{}</span>",
                escape_html(note)
            ));
        }
        out.push_str("</li>\n");
    }
    out.push_str("</ul>\n");

    if recipe.untracked_count > 0 {
        out.push_str(&format!(
            "<p class=\"caution\">The figures below exclude {} ingredient{} with no nutrition information.</p>\n",
            recipe.untracked_count,
            if recipe.untracked_count == 1 { "" } else { "s" }
        ));
    }

    let steps = recipe
        .instructions
        .as_deref()
        .map(instruction_steps)
        .unwrap_or_default();
    if !steps.is_empty() {
        out.push_str("<h2>Method</h2>\n<ol class=\"method\">\n");
        for step in &steps {
            out.push_str(&format!("<li>{}</li>\n", escape_html(step)));
        }
        out.push_str("</ol>\n");
    }

    out.push_str("<h2>Nutrition</h2>\n");
    out.push_str("<table class=\"nutrition\">\n<thead><tr><th scope=\"col\">Nutrient</th>");
    out.push_str("<th scope=\"col\">Per serving</th><th scope=\"col\">Whole recipe</th></tr></thead>\n<tbody>\n");
    for (label, per, whole) in nutrition_rows(recipe) {
        out.push_str(&format!(
            "<tr><th scope=\"row\">{}</th><td>{}</td><td>{}</td></tr>\n",
            escape_html(&label),
            escape_html(&per),
            escape_html(&whole),
        ));
    }
    out.push_str("</tbody>\n</table>\n");
    out.push_str("</article>\n");

    out.push_str(&format!(
        "<footer><p>Shared from <a href=\"{origin}/\">nom-inal</a>, an open-source nutrition tracker.</p></footer>\n",
        origin = escape_html(page.origin)
    ));
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

/// "Serves 4 · 1.2 kg · by Ada" — everything above the fold that is not the
/// name, as one line.
fn byline(recipe: &Recipe) -> String {
    let mut parts = vec![format!("Serves {}", trim_float(recipe.servings))];
    if recipe.total_weight_g > 0.0 {
        parts.push(if recipe.total_weight_g >= 1000.0 {
            format!("{} kg", r1(recipe.total_weight_g / 1000.0))
        } else {
            format!("{} g", trim_float(recipe.total_weight_g))
        });
    }
    parts.push(format!(
        "{} kcal per serving",
        recipe.per_serving.calories_kcal.round()
    ));
    let mut line = parts
        .iter()
        .map(|p| escape_html(p))
        .collect::<Vec<_>>()
        .join(" <span class=\"dot\" aria-hidden=\"true\">·</span> ");
    if let Some(author) = recipe.author.as_deref().filter(|a| !a.trim().is_empty()) {
        line.push_str(&format!(
            " <span class=\"dot\" aria-hidden=\"true\">·</span> by {}",
            escape_html(author)
        ));
    }
    line
}

struct Row {
    amount: String,
    name: String,
    note: Option<String>,
}

/// An ingredient as the page and the JSON-LD both need it: a measure and a
/// thing, from the one list, so the two cannot describe different recipes.
///
/// The measure is the server's own `amount_label`, which is what the app and
/// the Markdown card print too — "2 chicken breasts" here reads the way the
/// person wrote it, and `recipeIngredient` is exactly the line another site's
/// importer expects to parse.
fn ingredient_rows(recipe: &Recipe) -> Vec<Row> {
    recipe
        .items
        .iter()
        .map(|item| {
            // The measure and the name are decided together: a portion the
            // name already carries belongs to the name, not to the amount.
            let (amount, mut name) = crate::domain::measure::ingredient_columns(
                &item.amount_label,
                item.portion_label.as_deref(),
                item.portion_count,
                &item.name,
            );
            if let Some(variant) = item.variant_label.as_deref().filter(|v| !v.is_empty()) {
                name = format!("{name}, {variant}");
            }
            if let Some(brand) = item.brand.as_deref().filter(|b| !b.is_empty()) {
                name = format!("{name} ({brand})");
            }
            Row {
                amount,
                name,
                note: item
                    .note
                    .as_deref()
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(String::from),
            }
        })
        .collect()
}

/// The same rows as one string each, which is the shape `recipeIngredient`
/// takes and the shape the app's own importer reads back.
fn ingredient_lines(recipe: &Recipe) -> Vec<String> {
    ingredient_rows(recipe)
        .into_iter()
        .map(|row| {
            if row.amount.is_empty() {
                row.name
            } else {
                format!("{} {}", row.amount, row.name)
            }
        })
        .collect()
}

fn nutrition_rows(recipe: &Recipe) -> Vec<(String, String, String)> {
    let per = &recipe.per_serving;
    let all = &recipe.total;
    vec![
        (
            "Calories".into(),
            format!("{} kcal", per.calories_kcal.round()),
            format!("{} kcal", all.calories_kcal.round()),
        ),
        (
            "Protein".into(),
            format!("{} g", r1(per.protein_g)),
            format!("{} g", r1(all.protein_g)),
        ),
        (
            "Carbohydrate".into(),
            format!("{} g", r1(per.carbs_g)),
            format!("{} g", r1(all.carbs_g)),
        ),
        (
            "Net carbs".into(),
            format!("{} g", r1(per.net_carbs_g())),
            format!("{} g", r1(all.net_carbs_g())),
        ),
        (
            "Fat".into(),
            format!("{} g", r1(per.fat_g)),
            format!("{} g", r1(all.fat_g)),
        ),
        (
            "Fibre".into(),
            format!("{} g", r1(per.fiber_g)),
            format!("{} g", r1(all.fiber_g)),
        ),
        (
            "Sugar".into(),
            format!("{} g", r1(per.sugar_g)),
            format!("{} g", r1(all.sugar_g)),
        ),
        (
            "Saturated fat".into(),
            format!("{} g", r1(per.saturated_fat_g)),
            format!("{} g", r1(all.saturated_fat_g)),
        ),
        (
            "Sodium".into(),
            format!("{} mg", per.sodium_mg.round()),
            format!("{} mg", all.sodium_mg.round()),
        ),
    ]
}

// ---------------------------------------------------------------------------
// Structured data
// ---------------------------------------------------------------------------

/// A schema.org `Recipe`, which is what Google reads and what every recipe
/// site publishes.
///
/// Pleasingly, it is also what this application's *own* importer reads off
/// other people's pages (`domain::jsonld`), so a recipe shared from one
/// instance can be imported into another with the feature that already
/// exists. The round-trip is asserted in the tests below rather than assumed.
fn json_ld(page: &Page) -> Value {
    let recipe = page.recipe;

    let mut images = vec![Value::String(page.preview())];
    images.extend(
        page.photos
            .iter()
            .map(|p| Value::String(format!("{}{}", page.origin, p.url))),
    );

    let steps: Vec<Value> = recipe
        .instructions
        .as_deref()
        .map(instruction_steps)
        .unwrap_or_default()
        .into_iter()
        .map(|text| json!({"@type": "HowToStep", "text": text}))
        .collect();

    let per = &recipe.per_serving;
    let mut doc = json!({
        "@context": "https://schema.org",
        "@type": "Recipe",
        "name": recipe.name,
        "description": page.description(),
        "url": page.canonical(),
        "image": images,
        // `recipeYield` opens with the number, which is what a reader — ours
        // included — takes off the front of it.
        "recipeYield": format!(
            "{} serving{}",
            trim_float(recipe.servings),
            if recipe.servings == 1.0 { "" } else { "s" }
        ),
        "recipeIngredient": ingredient_lines(recipe),
        "datePublished": recipe.created_at.date_naive().to_string(),
        "nutrition": {
            "@type": "NutritionInformation",
            // schema.org wants these as text with their unit, per serving.
            "servingSize": "1 serving",
            "calories": format!("{} kcal", per.calories_kcal.round()),
            "proteinContent": format!("{} g", r1(per.protein_g)),
            "carbohydrateContent": format!("{} g", r1(per.carbs_g)),
            "fatContent": format!("{} g", r1(per.fat_g)),
            "saturatedFatContent": format!("{} g", r1(per.saturated_fat_g)),
            "sugarContent": format!("{} g", r1(per.sugar_g)),
            "fiberContent": format!("{} g", r1(per.fiber_g)),
            "sodiumContent": format!("{} mg", per.sodium_mg.round()),
        },
    });

    if !steps.is_empty() {
        doc["recipeInstructions"] = Value::Array(steps);
    }
    if let Some(author) = recipe.author.as_deref().filter(|a| !a.trim().is_empty()) {
        doc["author"] = json!({"@type": "Person", "name": author});
    }
    doc
}

// ---------------------------------------------------------------------------
// Presentation
// ---------------------------------------------------------------------------

/// A small page with one sentence on it, in the same skin as the recipe.
fn message_page(origin: &str, heading: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{heading} — nom-inal</title>\n<meta name=\"robots\" content=\"noindex\">\n\
         <style>\n{STYLE}</style>\n</head>\n<body>\n\
         <header class=\"bar\"><a class=\"mark\" href=\"{origin}/\"><span aria-hidden=\"true\">🥗</span> nom-inal</a></header>\n\
         <main><article><h1>{heading}</h1><p class=\"lede\">{body}</p>\n\
         <p><a class=\"button\" href=\"{origin}/\">Open nom-inal</a></p></article></main>\n\
         </body>\n</html>\n",
        heading = escape_html(heading),
        body = escape_html(body),
        origin = escape_html(origin),
    )
}

/// The page's whole stylesheet, inline.
///
/// Inline because a second request for a stylesheet is a second round trip
/// before anything is readable, and this is a few hundred bytes. The colours
/// are the application's own tokens, converted from the oklch they are
/// authored in to hex so that a WebView in a chat app renders them rather
/// than falling back to black on white. Mobile first throughout: the layout
/// is one column with a 16 px gutter, and the widths only widen from there.
const STYLE: &str = r#"
:root {
  color-scheme: light dark;
  --bg: #f9fafb;        /* oklch(0.985 0.002 250) */
  --fg: #15191d;        /* oklch(0.21 0.01 250) */
  --card: #ffffff;
  --muted: #666d74;     /* oklch(0.53 0.014 250) */
  --border: #dfe1e4;    /* oklch(0.91 0.005 250) */
  --primary: #357855;   /* oklch(0.52 0.09 158) */
  --accent: #ecf2ee;    /* oklch(0.955 0.008 158) */
  --on-primary: #f7fbf9;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #0b0d10;
    --fg: #e2e5e8;
    --card: #14171b;
    --muted: #9399a0;
    --border: #2b2e32;
    --primary: #54b581;
    --accent: #26312a;
    --on-primary: #0b0d10;
  }
}
* { box-sizing: border-box; }
html { -webkit-text-size-adjust: 100%; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--fg);
  font: 16px/1.6 ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
}
a { color: var(--primary); }
.bar {
  border-bottom: 1px solid var(--border);
  background: var(--card);
}
.mark {
  display: inline-block;
  padding: 14px 16px;
  font-weight: 600;
  text-decoration: none;
  color: inherit;
}
main { max-width: 44rem; margin: 0 auto; padding: 24px 16px 56px; }
h1 { font-size: 1.75rem; line-height: 1.2; margin: 0 0 8px; overflow-wrap: anywhere; }
h2 {
  font-size: 1.15rem;
  margin: 32px 0 8px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--border);
}
.meta { color: var(--muted); margin: 0 0 16px; font-size: 0.95rem; }
.lede { margin: 0 0 16px; }
.dot { color: var(--border); padding: 0 2px; }
.photos { display: grid; gap: 12px; margin: 16px 0; }
.photos figure { margin: 0; }
.photos img {
  width: 100%;
  height: auto;
  border-radius: 12px;
  border: 1px solid var(--border);
  background: var(--accent);
}
figcaption { color: var(--muted); font-size: 0.9rem; padding-top: 6px; }
.ingredients { list-style: none; margin: 0; padding: 0; }
.ingredients li {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 6px;
  padding: 8px 0;
  border-bottom: 1px solid var(--border);
}
.amount { font-variant-numeric: tabular-nums; color: var(--muted); min-width: 4.5rem; }
.what { font-weight: 500; overflow-wrap: anywhere; }
.note { color: var(--muted); font-size: 0.9rem; width: 100%; }
.method { margin: 0; padding-left: 1.3rem; }
.method li { padding: 6px 0; }
.caution {
  background: var(--accent);
  border-radius: 10px;
  padding: 10px 12px;
  margin: 16px 0;
  font-size: 0.95rem;
}
.nutrition { width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; }
.nutrition th, .nutrition td { text-align: right; padding: 8px 0; border-bottom: 1px solid var(--border); }
.nutrition thead th { color: var(--muted); font-weight: 500; font-size: 0.85rem; }
.nutrition thead th:first-child { text-align: left; }
.nutrition th[scope="row"] { text-align: left; font-weight: 500; }
footer { margin-top: 40px; color: var(--muted); font-size: 0.9rem; }
.button {
  display: inline-block;
  background: var(--primary);
  color: var(--on-primary);
  text-decoration: none;
  border-radius: 999px;
  padding: 10px 18px;
  font-weight: 600;
}
@media (min-width: 40rem) {
  h1 { font-size: 2.25rem; }
  .photos { grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr)); }
}
@media print {
  .bar, footer { display: none; }
  body { background: #fff; color: #000; font-size: 11pt; }
  main { max-width: none; padding: 0; }
  .photos { grid-template-columns: repeat(2, 1fr); }
  a { color: #000; text-decoration: none; }
  h2 { break-after: avoid; }
  li, tr { break-inside: avoid; }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::jsonld;
    use crate::domain::nutrients::Nutrients;
    use crate::domain::recipe::RecipeItem;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn nutrients() -> Nutrients {
        Nutrients {
            calories_kcal: 320.0,
            protein_g: 12.0,
            carbs_g: 40.0,
            fat_g: 9.0,
            fiber_g: 5.0,
            sugar_g: 3.0,
            saturated_fat_g: 1.5,
            sodium_mg: 120.0,
        }
    }

    fn item(name: &str, grams: Option<f64>, servings: Option<f64>) -> RecipeItem {
        RecipeItem {
            id: Uuid::nil(),
            food_id: grams.map(|_| Uuid::nil()),
            sub_recipe_id: servings.map(|_| Uuid::nil()),
            label: None,
            name: name.to_string(),
            brand: None,
            variant_label: None,
            quantity_g: grams,
            servings,
            portion_label: None,
            portion_count: None,
            amount_label: crate::domain::measure::amount_label(None, None, grams, servings),
            weight_g: grams.unwrap_or(100.0),
            note: None,
            sort_order: 0,
            nutrients: nutrients(),
        }
    }

    /// The same ingredient, written the way somebody would say it.
    fn item_in_portions(name: &str, grams: f64, label: &str, count: f64) -> RecipeItem {
        let mut item = item(name, Some(grams), None);
        item.amount_label =
            crate::domain::measure::amount_label(Some(label), Some(count), Some(grams), None);
        item.portion_label = Some(label.to_string());
        item.portion_count = Some(count);
        item
    }

    fn recipe(name: &str) -> Recipe {
        Recipe {
            id: Uuid::nil(),
            slug: "pierogi-ruskie".into(),
            name: name.to_string(),
            description: None,
            instructions: Some("Mix the lot.\nBoil for 4 minutes.".into()),
            servings: 4.0,
            is_public: true,
            is_owner: false,
            author: Some("Ada".into()),
            total_weight_g: 1200.0,
            untracked_count: 0,
            items: vec![
                item("Flour", Some(200.0), None),
                item("Sauce", None, Some(1.0)),
            ],
            total: nutrients(),
            per_serving: nutrients(),
            created_at: Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap(),
            updated_at: Utc.with_ymd_and_hms(2026, 3, 2, 12, 0, 0).unwrap(),
        }
    }

    fn render(recipe: &Recipe) -> String {
        render_page(&Page {
            recipe,
            photos: &[],
            origin: "https://nom.example",
        })
    }

    #[test]
    fn escaping_covers_every_character_that_could_start_markup() {
        assert_eq!(
            escape_html(r#"<script>alert("x" & 'y')</script>"#),
            "&lt;script&gt;alert(&quot;x&quot; &amp; &#39;y&#39;)&lt;/script&gt;"
        );
    }

    #[test]
    fn escaping_leaves_ordinary_text_alone() {
        assert_eq!(
            escape_html("Crème brûlée — 4 servings"),
            "Crème brûlée — 4 servings"
        );
    }

    /// The case this exists for: a name with a quotation mark in it must not
    /// end the `content="…"` of a meta tag, and a name with a tag in it must
    /// not become one.
    #[test]
    fn a_hostile_name_cannot_break_out_of_the_head() {
        let html = render(&recipe(r#"Mum's "<script>" pie"#));
        assert!(
            html.contains("<title>Mum&#39;s &quot;&lt;script&gt;&quot; pie — nom-inal</title>"),
            "title was not escaped"
        );
        assert!(html.contains(
            r#"<meta property="og:title" content="Mum&#39;s &quot;&lt;script&gt;&quot; pie">"#
        ));
        // The only script element on the page is the JSON-LD block.
        assert_eq!(html.matches("<script").count(), 1);
        assert_eq!(html.matches("</script>").count(), 1);
    }

    #[test]
    fn the_json_ld_block_cannot_be_closed_by_a_recipe_name() {
        let html = render(&recipe("Pie </script><img src=x>"));
        assert!(
            html.contains("\\u003c/script\\u003e"),
            "not escaped: {html}"
        );
        assert_eq!(html.matches("</script>").count(), 1);
    }

    #[test]
    fn the_head_names_the_recipe_and_its_canonical_address() {
        let html = render(&recipe("Pierogi Ruskie"));
        assert!(html.contains("<title>Pierogi Ruskie — nom-inal</title>"));
        assert!(
            html.contains(r#"<link rel="canonical" href="https://nom.example/r/pierogi-ruskie">"#)
        );
        assert!(html.contains(
            r#"<meta property="og:url" content="https://nom.example/r/pierogi-ruskie">"#
        ));
        assert!(html.contains(
            r#"<meta property="og:image" content="https://nom.example/api/v1/public/recipes/pierogi-ruskie/preview.png">"#
        ));
        assert!(html.contains(r#"<meta property="og:image:width" content="1200">"#));
        assert!(html.contains(r#"<meta name="twitter:card" content="summary_large_image">"#));
    }

    #[test]
    fn a_recipe_with_no_description_gets_a_sentence_made_of_what_it_knows() {
        let html = render(&recipe("Pierogi"));
        assert!(
            html.contains(
                r#"<meta name="description" content="A recipe with 2 ingredients, 320 kcal per serving.">"#
            ),
            "{html}"
        );
    }

    #[test]
    fn the_author_s_own_description_wins() {
        let mut r = recipe("Pierogi");
        r.description = Some("Grandmother's, with too much butter.".into());
        let html = render(&r);
        assert!(html.contains(r#"content="Grandmother&#39;s, with too much butter.""#));
    }

    /// The symmetry worth having: this page is readable by the same extractor
    /// the application points at other people's recipe sites, so a recipe
    /// shared from one instance imports into another.
    #[test]
    fn the_rendered_page_is_readable_by_our_own_importer() {
        let html = render(&recipe("Pierogi Ruskie"));
        let scraped = jsonld::recipe_from_html(&html).expect("no Recipe found in the page");

        assert_eq!(scraped.name, "Pierogi Ruskie");
        assert_eq!(scraped.servings, Some(4.0));
        assert_eq!(scraped.ingredients, vec!["200 g Flour", "1 serving Sauce"]);
        assert_eq!(
            scraped.instructions,
            vec!["Mix the lot.", "Boil for 4 minutes."]
        );
        assert_eq!(scraped.author.as_deref(), Some("Ada"));
    }

    #[test]
    fn the_structured_data_carries_nutrition_with_the_units_schema_org_expects() {
        let r = recipe("Pierogi");
        let doc = json_ld(&Page {
            recipe: &r,
            photos: &[],
            origin: "https://nom.example",
        });
        let nutrition = &doc["nutrition"];
        assert_eq!(nutrition["@type"], "NutritionInformation");
        assert_eq!(nutrition["calories"], "320 kcal");
        assert_eq!(nutrition["proteinContent"], "12 g");
        assert_eq!(nutrition["sodiumContent"], "120 mg");
        assert_eq!(doc["datePublished"], "2026-03-01");
    }

    #[test]
    fn the_body_is_the_recipe_as_a_document() {
        let html = render(&recipe("Pierogi"));
        assert!(html.contains("<h1>Pierogi</h1>"));
        assert!(html.contains("Serves 4"));
        assert!(html.contains("by Ada"));
        assert!(html.contains(r#"<span class="amount">200 g</span>"#));
        assert!(html.contains("<li>Mix the lot.</li>"));
        assert!(html.contains("320 kcal"));
        // No script beyond the structured data: nothing here needs running.
        assert!(!html.contains("<script src"));
    }

    /// The point of rendering the phrase on the server: the page a stranger
    /// opens says what the cook said, and so does the line an importer reads
    /// out of the structured data.
    #[test]
    fn an_ingredient_measured_in_portions_reads_that_way_here_too() {
        let mut r = recipe("Chicken traybake");
        r.items = vec![
            item_in_portions("Chicken breast", 348.0, "1 chicken breast", 2.0),
            item_in_portions("Bread", 28.0, "1 slice", 1.0),
            item("Flour", Some(200.0), None),
        ];
        let html = render(&r);
        assert!(
            html.contains(r#"<span class="amount">2 chicken breasts</span>"#),
            "{html}"
        );
        assert!(
            html.contains(r#"<span class="amount">1 slice</span>"#),
            "{html}"
        );

        let scraped = jsonld::recipe_from_html(&html).expect("no Recipe found in the page");
        assert_eq!(
            scraped.ingredients,
            vec![
                "2 chicken breasts Chicken breast",
                "1 slice Bread",
                "200 g Flour"
            ]
        );
    }

    #[test]
    fn an_untracked_ingredient_is_named_rather_than_dropped() {
        let mut r = recipe("Pierogi");
        r.untracked_count = 2;
        let html = render(&r);
        assert!(html.contains("exclude 2 ingredients with no nutrition information"));
    }

    #[test]
    fn the_origin_comes_from_what_the_proxy_forwarded() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "nom.example".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        assert_eq!(origin_from_headers(&headers), "https://nom.example");

        // No proxy in front: plain http and whatever host was asked for.
        let mut bare = HeaderMap::new();
        bare.insert(header::HOST, "192.168.1.4:8088".parse().unwrap());
        assert_eq!(origin_from_headers(&bare), "http://192.168.1.4:8088");
    }

    /// The Host header is chosen by whoever sent the request, and it ends up
    /// inside `og:url`. Anything that is not a host name is not used.
    #[test]
    fn a_host_that_is_not_a_host_is_refused() {
        let mut spoofed = HeaderMap::new();
        spoofed.insert(header::HOST, "evil.test/path".parse().unwrap());
        assert_eq!(origin_from_headers(&spoofed), "http://localhost");

        let mut scheme = HeaderMap::new();
        scheme.insert(header::HOST, "nom.example".parse().unwrap());
        scheme.insert("x-forwarded-proto", "javascript".parse().unwrap());
        assert_eq!(origin_from_headers(&scheme), "http://nom.example");
    }
}
