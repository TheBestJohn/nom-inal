# nom-inal: what the numbers mean

Read this before logging or building anything. The API is precise about a few
things a person would otherwise guess.

## Nutrients are stored per 100 g

Every food stores eight nutrients — `calories_kcal`, `protein_g`, `carbs_g`,
`fat_g`, `fiber_g`, `sugar_g`, `saturated_fat_g`, `sodium_mg` — **per 100 g**.
That is the basis a food's own fields are in when you read it.

A food also carries `serving_size_g` (and sometimes a `serving_label` such as
"1 large"). `nutrient_basis` says how the food was *entered* and should be
*shown* (`per_100g` or `per_serving`); it does not change what is stored. A
food's detail (`foods_get_one`) includes `per_serving`, already converted.

To create a food from a nutrition label, send the figures as the label prints
them with `nutrient_basis: "per_serving"` and the label's `serving_size_g`; the
server converts to per 100 g and range-checks the result. Imports from USDA and
Open Food Facts are per 100 g already.

## Diary entries: a food in grams, or a recipe in servings

An entry is exactly one of:

- `food_id` + `quantity_g` — grams of a food, or
- `recipe_id` + `recipe_servings` — servings of a recipe.

Never both. "2 eggs" means 2 × the egg food's `serving_size_g` grams; "150 g
chicken" means `quantity_g: 150`. `log_food` does this arithmetic and reports
it in its `basis` field before writing anything.

`meal` is `breakfast`, `lunch`, `dinner` or `snack` (lower-case; anything else
is kept as a custom meal name). `logged_on` is a date, `YYYY-MM-DD`, and
defaults to **today in UTC** — pass it explicitly when the person's local day
differs.

A food may carry `portions` — household measures such as "1 cup" with a
`grams` figure. "A cup of oats" is that portion's grams of the oats food;
the entry is still written in `quantity_g`. `foods_recent` lists what the
person logged most recently, with the amount they used last time, and is the
place to resolve "the usual" or "what I had yesterday". `diary_copy` copies a
day, or one meal of it, onto another date; `recipes_from_meal` turns a logged
meal into a recipe without re-entering it.

## Recipes are graphs, and their macros are never stored

A recipe's totals are computed on read from its ingredients, so correcting a
food later corrects every recipe that uses it. An ingredient is exactly one of:

- `food_id` + `quantity_g` — a food, in grams;
- `sub_recipe_id` + `servings` — another recipe, by reference, in servings.
  Linked, not copied: change the base sauce and every dish built on it
  follows. No cycles, at most five levels deep;
- `label` — words only ("salt and pepper to taste"). Contributes **nothing**
  to the macros. The recipe's `untracked_count` says how many such ingredients
  it has, counted through any nesting; its totals are complete only when that
  is 0. Always mention it when reporting a recipe's macros.

`per_serving` is `total / servings`. Recipes are private by default; a public
recipe (`is_public: true`) can be read and logged by every account on the
instance but changed only by its author.

## Foods are shared; recipes are yours

The food database is one global, community-edited record. Anyone can correct
any food, every edit is kept as a revision, and other people can verify a
revision. `foods_get_one` returns `provenance` with the verification status;
an entry nobody has confirmed is `unverified`, which is normal, not wrong.
Deleting a food is refused once anyone else has edited or verified it, and
whenever a recipe or diary entry uses it.

## Targets: goal versus budget

A target is a standing setting, not a daily plan: set once, it applies to
every day. Each target has a direction:

- a **budget** is a ceiling (calories, sugar, sodium by default) — `status`
  is `under` or `over`;
- a **goal** is a floor (protein, fibre by default) — `status` is `short` or
  `met`.

`remaining` is always `amount − consumed`, signed. 120 % of a calorie budget is
a problem; 120 % of a protein goal is a success. Read `kind` before commenting
on a number. `today` returns targets already evaluated, so use those fields
rather than recomputing.

## Days, averages and weight

`progress` and `diary_summary` average over **logged days only**: a day with
no entries is missing data, not a zero-calorie day. `logged_day_count` says how
many days the average stands on.

Weights are one entry per day, in kilograms, with an optional body-fat
percentage. `weights_stats` reports the change over the window and a 7-entry
moving average; the average is what to quote as the trend, since single
weigh-ins swing with water.

## Search

`search_stream_foods` returns the tiers the search found in order —
`exact`, `prefix`, `contains`, then `fuzzy` and `fuzzy_words`. A fuzzy hit
survives typos ("chikn brest") but is a guess; confirm it with the person
before logging. `foods_search_external` looks up USDA and Open Food Facts, and
`foods_import` brings a candidate into the database; `foods_barcode` does the
same for a UPC/EAN.

## Errors and keys

Every error is `{ "error": code, "message": text }` with an HTTP status. A
`403` saying the key is read-only means the key was created without the
write scope; only the account holder can make a new one, in Settings.

`meta_health` reports the running build (`version`, and the git commit and
build time once they are baked into the image) — quote it when something looks
like a bug.
