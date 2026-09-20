//! Eager, tiered food search over Server-Sent Events.
//!
//! Logging a meal is the app's hottest path, and a single ranked query makes it
//! feel slow twice over: an exact hit waits behind a trigram scan of the whole
//! table, and a typo returns nothing at all.
//!
//! So the search runs as a sequence of progressively looser tiers, each flushed
//! to the client the moment it returns:
//!
//!   1. `exact`    — the name is the query
//!   2. `prefix`   — the name starts with the query
//!   3. `contains` — the name or brand contains the query
//!   4. `fuzzy`    — trigram similarity, which survives typos ("chikn")
//!   5. `fuzzy_words` — every word fuzzy-matches somewhere ("chikn brest")
//!
//! The first tier typically lands in single-digit milliseconds, so the list is
//! populated before the fuzzy scan has finished. SSE is what makes that
//! visible: with one JSON response the client waits for the slowest tier.
//!
//! Tiers are deduplicated as they go, so a food already sent is never repeated
//! in a looser tier.

use std::collections::HashSet;
use std::time::Instant;

use async_stream::stream;
use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use utoipa::IntoParams;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::domain::food::Food;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/foods", get(stream_foods))
}

use crate::domain::food::FOOD_COLUMNS as COLUMNS;

#[derive(Debug, Deserialize, IntoParams)]
pub struct StreamQuery {
    /// Search terms.
    pub q: String,
    /// Maximum foods per tier (1–50, default 10).
    pub limit: Option<i64>,
}

/// One flush of results. `tier` says how the match was found, so the client can
/// label or group them ("exact" hits above "did you mean" hits).
#[derive(Debug, Serialize)]
struct TierPayload {
    tier: &'static str,
    results: Vec<Food>,
}

#[derive(Debug, Serialize)]
struct DonePayload {
    total: usize,
    elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
struct ErrorPayload {
    message: String,
}

#[utoipa::path(
    get, path = "/api/v1/search/foods", tag = "search",
    security(("bearer" = [])),
    params(StreamQuery),
    responses((status = 200, description = "text/event-stream of `tier`, `done` and `error` events", content_type = "text/event-stream"))
)]
pub async fn stream_foods(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(query): Query<StreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let term = query.q.trim().to_string();
    let limit = query.limit.unwrap_or(10).clamp(1, 50);

    let stream = stream! {
        let started = Instant::now();

        if term.is_empty() {
            yield Ok(done_event(0, started));
            return;
        }

        // Ids already sent, so each tier only adds what the tighter ones missed.
        let mut seen: HashSet<Uuid> = HashSet::new();
        // Because the food table is global, the same product can legitimately
        // have been added by several people. Rows that are identical in every
        // way that matters for picking one are collapsed to the first, while
        // two foods that merely share a name but differ nutritionally both
        // stay — they are genuinely different things.
        let mut seen_content: HashSet<String> = HashSet::new();
        let mut total = 0usize;

        for (tier, sql) in tiers(&term) {
            let rows: Result<Vec<Food>, _> = sqlx::query_as(sql)
                .bind(&term)
                .bind(limit)
                .fetch_all(&state.db)
                .await;

            match rows {
                Ok(rows) => {
                    let fresh: Vec<Food> = rows
                        .into_iter()
                        .filter(|f| seen.insert(f.id) && seen_content.insert(content_key(f)))
                        .collect();

                    if fresh.is_empty() {
                        continue;
                    }
                    total += fresh.len();

                    // Flush this tier before running the next one — that is the
                    // whole point of streaming rather than collecting.
                    match Event::default().event("tier").json_data(TierPayload { tier, results: fresh }) {
                        Ok(event) => yield Ok(event),
                        Err(e) => {
                            tracing::error!(error = %e, "failed to encode search tier");
                        }
                    }
                }
                Err(e) => {
                    // A failed tier is reported and the rest still run: partial
                    // results beat an empty list.
                    tracing::error!(error = %e, tier, "search tier failed");
                    if let Ok(event) = Event::default()
                        .event("error")
                        .json_data(ErrorPayload { message: format!("{tier} search failed") })
                    {
                        yield Ok(event);
                    }
                }
            }
        }

        yield Ok(done_event(total, started));
    };

    // A comment every 15s keeps proxies and load balancers from culling an
    // idle connection mid-search.
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// The same tiered search, collected rather than streamed, for a caller that
/// wants the best few matches for a term in one go — the recipe importer
/// resolving an ingredient line. Each food comes with the tier that found
/// it, tightest first, deduplicated exactly as the stream is.
pub async fn candidates(
    state: &AppState,
    term: &str,
    limit: usize,
) -> Result<Vec<(&'static str, Food)>, sqlx::Error> {
    let term = term.trim();
    let mut out: Vec<(&'static str, Food)> = Vec::new();
    if term.is_empty() || limit == 0 {
        return Ok(out);
    }
    let mut seen: HashSet<Uuid> = HashSet::new();
    let mut seen_content: HashSet<String> = HashSet::new();

    for (tier, sql) in tiers(term) {
        let rows: Vec<Food> = sqlx::query_as(sql)
            .bind(term)
            .bind(limit as i64)
            .fetch_all(&state.db)
            .await?;
        for food in rows {
            if seen.insert(food.id) && seen_content.insert(content_key(&food)) {
                out.push((tier, food));
                if out.len() >= limit {
                    return Ok(out);
                }
            }
        }
    }
    Ok(out)
}

/// Identity of a food for duplicate-collapsing: what it is, and what it is
/// made of. Nutrients are rounded so figures that differ only in float noise
/// still collapse.
fn content_key(food: &Food) -> String {
    format!(
        "{}|{}|{:.1}|{:.1}|{:.1}|{:.1}",
        food.name.trim().to_lowercase(),
        food.brand
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase(),
        food.calories_kcal,
        food.protein_g,
        food.carbs_g,
        food.fat_g,
    )
}

fn done_event(total: usize, started: Instant) -> Event {
    Event::default()
        .event("done")
        .json_data(DonePayload {
            total,
            elapsed_ms: started.elapsed().as_millis(),
        })
        .unwrap_or_else(|_| Event::default().event("done").data("{}"))
}

/// The tiers, tightest first. Each takes the search term as `$1` and a row
/// limit as `$2`.
fn tiers(term: &str) -> Vec<(&'static str, &'static str)> {
    let mut tiers = vec![
        ("exact", const_format::exact()),
        ("prefix", const_format::prefix()),
        ("contains", const_format::contains()),
        ("fuzzy", const_format::fuzzy()),
    ];

    // `<%` looks for ONE contiguous matching extent, so it finds "chikn" but
    // not "chikn brest" — two misspelled words never form one extent. The
    // per-word tier covers that, and is gated on a multi-word query because it
    // cannot use the trigram index and so scans. That scan is affordable
    // precisely because of the streaming design: the tiers above have already
    // been flushed, so the user is reading results while this one runs.
    if term.split_whitespace().count() > 1 {
        tiers.push(("fuzzy_words", const_format::fuzzy_words()));
    }

    tiers
}

/// The SQL lives here rather than inline so each tier reads as one idea.
mod const_format {
    use super::COLUMNS;
    use std::sync::OnceLock;

    /// Fields that decide whether two rows are the same food.
    ///
    /// The table is global, so the same product can have been added by several
    /// people. Rows identical in all of these are the same thing; two foods
    /// sharing only a name but differing nutritionally are not, and both stay.
    const IDENTITY: &str = "lower(btrim(name)), lower(btrim(coalesce(brand, ''))), \
         round(calories_kcal::numeric, 1), round(protein_g::numeric, 1), \
         round(carbs_g::numeric, 1), round(fat_g::numeric, 1)";

    /// Every tier has the same shape: match, collapse duplicates, rank, limit.
    ///
    /// The collapse has to happen BEFORE the limit. Deduplicating afterwards
    /// lets five copies of one food consume the whole result budget and hide
    /// everything else that matched.
    fn tier_sql(predicate: &str, rank: &str) -> String {
        format!(
            "SELECT {cols} FROM (
                 SELECT DISTINCT ON ({identity}) *
                 FROM foods
                 WHERE {predicate}
                 -- DISTINCT ON keeps the first row per group, so order by the
                 -- identity first and then oldest-wins within each group.
                 ORDER BY {identity}, created_at ASC
             ) foods
             ORDER BY {rank}
             LIMIT $2",
            cols = COLUMNS,
            identity = IDENTITY,
            predicate = predicate,
            rank = rank,
        )
    }

    macro_rules! tier {
        ($name:ident, $predicate:expr, $rank:expr) => {
            pub fn $name() -> &'static str {
                static SQL: OnceLock<String> = OnceLock::new();
                SQL.get_or_init(|| tier_sql($predicate, $rank))
            }
        };
    }

    tier!(exact, "lower(name) = lower($1)", "name");

    tier!(prefix, "name ILIKE $1 || '%'", "length(name), name");

    tier!(
        contains,
        "name ILIKE '%' || $1 || '%' OR brand ILIKE '%' || $1 || '%'",
        "length(name), name"
    );

    // `<%` is pg_trgm's WORD similarity operator, and the choice matters.
    // Whole-string `similarity()` divides by the length of the whole name, so
    // "chikn" against "Chicken breast, raw" scores 0.14 and is missed, while
    // word similarity compares the query against the best-matching word extent
    // and scores 0.50. Both operators use the GIN trigram indexes; spelling it
    // as `word_similarity(...) > x` instead would force a sequential scan.
    //
    // The threshold lives on the connection (see `Config::trgm_word_threshold`)
    // because `<%` reads it from a GUC rather than taking it inline.
    tier!(
        fuzzy,
        "$1 <% name OR $1 <% COALESCE(brand, '')",
        "GREATEST(word_similarity($1, name), word_similarity($1, COALESCE(brand, ''))) DESC, \
         length(name), name"
    );

    // Every word of the query has to fuzzy-match somewhere in the name or
    // brand, which keeps "chikn brest" precise instead of returning everything
    // that looks a bit like "brest". `bool_and` over no rows is NULL, so a
    // query of only very short words coalesces to no match rather than to all.
    tier!(
        fuzzy_words,
        "COALESCE((
             SELECT bool_and(w <% foods.name OR w <% COALESCE(foods.brand, ''))
             FROM unnest(string_to_array(lower(btrim($1)), ' ')) AS w
             WHERE length(w) >= 3
         ), false)",
        "GREATEST(word_similarity($1, name), word_similarity($1, COALESCE(brand, ''))) DESC, \
         length(name), name"
    );
}
