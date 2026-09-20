# Changelog

The API is a contract. A response field is never renamed, removed or made
nullable except at a tagged release; between releases, changes are additive
only. Every wire-level change is listed under **API changes** for its release,
breaking ones first and marked as such, so a client author can read one
section and know what to update.

## Unreleased

### Shared links that look like the recipe

- A shared recipe's link is now a page the server renders: `GET /r/{slug}`
  returns a complete HTML document with the recipe's name in its title, Open
  Graph and Twitter tags, a canonical link and schema.org `Recipe` JSON-LD.
  Slack, iMessage, Discord, Facebook, Twitter and Google read a page's head
  and never run its scripts, so every shared recipe used to preview as the
  same generic card; now each previews as itself. The page carries the recipe
  as a document — photos, ingredients, numbered method, nutrition per serving
  and whole — with its CSS inline, no JavaScript at all, a dark mode and a
  print stylesheet. nginx proxies `/r/` to the API instead of serving the app
  shell. The in-app public recipe page is gone, replaced by it.
- Recipes have a **slug**, made from the name: `/r/pierogi-ruskie` rather than
  `/r/3f7c…`. Renaming a recipe issues a new slug and keeps the old one
  pointing at it, so a link already sent still resolves and redirects once to
  the current address; a slug is never reused by another recipe. Links built
  from a recipe's uuid keep working and redirect the same way.
- **A preview image for every shared recipe**, at
  `GET /api/v1/public/recipes/{slug}/preview.png`: the first photo, re-encoded
  to 1200×630 and cropped to cover, or — for a recipe with no photos — a card
  drawn from its name, servings and calories. Cached hard and revalidated with
  an `ETag`, so a link posted in a busy channel does not redraw it per reader.
- `PUBLIC_ORIGIN` configures the address this instance is reached at, for the
  absolute URLs those tags need. Unset, it is taken from the request, which is
  right behind the bundled nginx.

### API changes

- `Recipe` and `RecipeSummary` gain **`slug`**, the recipe's address in a
  shared link. Additive.
- `GET /api/v1/public/recipes/{id}` now accepts a slug, a slug the recipe used
  to have, or its uuid, where before it took only a uuid. Every existing
  request is still valid.
- New: `GET /api/v1/public/recipes/{id}/preview.png`, the 1200×630 card for a
  shared recipe. 404 unless the recipe is shared, like the routes beside it.
- New, outside the versioned API because it is a page and not a call:
  `GET /r/{slug-or-uuid}`, the shared recipe as HTML. 301 to the canonical
  slug when reached by any other name; 404, as a small HTML page, when the
  recipe is not shared.

## v0.3.0 — 2026-09-20

### Why are you tracking?

- A tracking focus on the profile — general health, weight loss, muscle gain,
  keto, diabetes, blood pressure, heart health, or custom — asked once at first
  sign-in and changeable in Settings. Choosing one fills in targets, readouts
  and the home chart from a preset; everything stays editable, and changing
  focus later asks before replacing targets you already have.
- Net carbs (carbs − fiber) is a nutrient in its own right: shown, charted and
  targeted like any other, computed on the server so every readout agrees.
- Energy share: the percentage of calories from protein, carbs and fat on the
  day and on the 30-day average.
- The calorie estimate behind suggested targets moved to the server
  (Mifflin–St Jeor), so the suggestion, the presets and the API cannot
  disagree.
- Settings is one route per section — focus, body, targets, display,
  reminders, integrations, account, admin — so each can be linked to.

### Estimators that use your own data

- A day can be marked "I logged everything", from the diary's day header. A
  complete day with nothing in it is a fast day, a real zero; a day nobody has
  vouched for is missing data. The summary lists complete days and counts
  them, and the average keeps its meaning: over days with entries.
- Adaptive expenditure: over the last 28 days (or any window from a week to a
  year), the mean intake on complete days less what the weight trend stored,
  at 7700 kcal per kilogram. A least-squares line through the weigh-ins is
  the trend, so one weigh-in after a salty dinner does not decide the month.
  It needs seven complete days and two weigh-ins a week apart before it says
  a number; short of that it says exactly what is still needed. Confidence is
  `low` under fourteen complete days or three weigh-ins, `moderate` under
  twenty-eight, `good` from there. The profile formula is shown beside it.
- Goal projection: the date the trend meets the target weight, or why it
  never will (flat, or heading the other way); the weekly rate, with a
  caution past about 1 % of body weight a week; and, for a chosen date, the
  daily change from expenditure that would get there, priced off the adaptive
  estimate when it is ready and the formula otherwise, never under 1200 kcal.
- Calculators live where the number is used, under Settings → Targets:
  protein per kilogram in the band the profile's goal puts you in, a macro
  split from a chosen ratio into gram targets for the current calorie budget,
  a keto ratio check, and BMI with its caveat. Each writes ordinary targets.
  Today's intake card carries one line of context: the adaptive estimate with
  a "Use as budget" action, or what is still needed to make one.

### Logging speed

- The food picker opens on what you logged recently — foods and recipes alike,
  most recent day first and most often first within it — each with the amount
  you used last time, so logging the usual is two taps.
- Copy yesterday onto an empty day, or copy one meal from any date, with the
  same things in the same amounts. Save a logged meal as a recipe in one step;
  a recipe you logged stays a recipe inside it.
- Scan a barcode with the camera, using the browser's own detector where it has
  one and a bundled decoder everywhere else, with a torch toggle when the camera
  offers one. A photo of the barcode works too, and is what you get when camera
  access is refused. The result goes through the same Open Food Facts lookup as
  a typed code.
- Household portions — "1 cup", "1 slice", "1 mug" — kept per food, imported
  from USDA's portion data and addable in the food form, and offered beside
  grams when logging. The diary still stores grams.
- A units preference, metric or imperial, for body weight and height: shown in
  pounds and feet-and-inches, typed in the same, stored in kilograms and
  centimetres. Food stays in grams.
- The app installs on a phone: a web manifest with icons, and a hand-written
  service worker that keeps the built shell and today's diary for an offline
  open. Nothing is cached that was written, and no photo is. A network failure
  at start-up no longer signs you out.

### Sharing, export, import

- Shared means public. The share switch on a recipe has one meaning: every
  account on the instance can read and log it, and so can anyone holding its
  link, signed in or not, at `/r/{id}`, photos included. The recipe's own id
  is the link; turning the switch off takes the page and its photos away, and
  turning it back on restores the same address. A "Copy link" button appears
  wherever a shared recipe is shown, and the public page is the same read
  view the signed-in page uses, with a print stylesheet.
- A recipe exports as JSON in the seed-repository shape — no internal ids,
  foods by the same name-and-brand key `GET /foods/export` uses, sub-recipes
  inlined by name, free text kept as text — or as a Markdown recipe card with
  the method numbered exactly as the page numbers it and the nutrition per
  serving.
- Import a recipe from a URL. The page's schema.org `Recipe` block is read
  (bare, in a `@graph`, or behind a page node; instructions as text, steps
  or sections); each ingredient line is parsed for its amount, unit and name
  — fractions, ranges, "1 1/2 cups", "½", "1 can (400 g)" — with mass units
  turned into grams and household measures left as they are, since a cup of
  flour and a cup of oil do not weigh the same. Each line is searched for in
  the food database and the review pre-selects a match only when the search
  found the food by name; looser matches are offered, and anything
  unresolved stays as a free-text ingredient. The result lands in the
  ordinary recipe form, still unsaved. Only public web addresses are
  fetched: private, loopback and link-local addresses are refused after DNS
  resolution and before a connection is opened, and a redirect to one is
  refused the same way.
- Account export and import, under Settings → Account. One JSON document
  with everything the account owns — profile without secrets, targets,
  reminders, own foods and the foods its recipes and diary use, recipes,
  diary, weigh-ins, photo metadata — or a zip of `diary.csv` and
  `weights.csv` for a spreadsheet. Importing merges by natural identity:
  foods by name and brand, recipes by name, diary entries by date, meal and
  what was eaten, weigh-ins by date. Nothing is duplicated, a food the
  instance already has is left as it is, a food it lacks keeps its line as
  text and is named in the report, and the same file imported twice changes
  nothing.

### API changes

Additive:

- `GET /public/recipes/{id}` and `GET /public/photos/{id}`, both without
  authentication: a shared recipe (the `Recipe` shape plus `photos`, whose
  URLs point at the public photo route) and a photo of one. 404 for anything
  not shared, including for its owner — the routes take no token.
- `GET /recipes/{id}/export?format=json|markdown` for a recipe you own or one
  that is shared. JSON is `{ format, generated_at, recipe }` with foods named
  by `{ key, name, brand, variant_label }` and sub-recipes inlined; Markdown
  is served as `text/markdown`.
- `POST /recipes/import { url }`, returning a `RecipeDraft` and writing
  nothing: `name`, `description`, `servings`, `instructions`, `source_url`,
  `image_url`, `author`, and `lines[]` each with the line as written, its
  parsed `quantity`, `unit`, `name` and `grams`, and `candidates[]` from the
  food database with the search `tier` that found each. A private or
  non-http address is a 400; a page that could not be fetched is a 502.
- `GET /account/export` (`?format=csv` for the zip) and
  `POST /account/import`, which takes the same document, merges it and
  returns `{ profile_updated, targets, reminders, foods, recipes, diary,
  weights, notes }` with `{ created, updated, skipped }` per kind. A
  `format` newer than the server reads is a 400 before anything is touched.
- `RecipeItem.variant_label` on every recipe item: the preparation variant
  of the food, when it is one, and null otherwise.
- `Food.portions` (`[{ id, label, grams, source }]`) on every food a response
  carries — lists, search results, details, variants and parents.
  `POST /foods/{id}/portions { label, grams }` and
  `DELETE /foods/{id}/portions/{portion_id}`, both returning the food.
  `ExternalFood.portions` is accepted on `POST /foods/import` and returned by
  `GET /foods/external/usda/{id}`; an import that sends none fetches USDA's own.
- `GET /foods/recent?limit=`: what the caller logged, one row per food or
  recipe, with the last amount and a count.
- `POST /diary/copy { from_date, to_date, meal? }`, returning
  `{ from_date, to_date, meal, copied, entries }`.
- `POST /recipes/from-meal { date, meal, name, servings?, is_public? }`,
  returning the new `Recipe`.
- `Profile.units` (`metric` | `imperial`, never null), settable through
  `PATCH /profile`. The figures on the profile stay in kg and cm.
- `Profile.tracking_focus` (nullable enum), settable through `PATCH /profile`.
- `GET /profile/focus`, `GET /profile/focus/preview?focus=`,
  `POST /profile/focus { focus, apply }`.
- `GET /targets/suggestion`.
- `net_carbs_g` on every `Nutrients` payload; accepted as a nutrient for
  targets and display preferences. Targets maximum rises from 8 to 9.
- `energy_share` on the diary day and summary responses.
- `PUT /diary/day/{date}/complete { complete }`, an upsert, returning
  `{ date, complete, updated_at }`. A date more than a day past UTC today is a
  400.
- `complete` on `DiaryDay` and on each `DiarySummary.days[]` entry;
  `complete_day_count` on `DiarySummary`. A day marked complete with no
  entries now appears in `days` with `entry_count: 0`; `logged_day_count` and
  `average` still cover days with entries only.
- `GET /estimates/tdee?days=` and `GET /estimates/projection?days=&by=`.
  Both carry `ready`, a `reason` when not ready, and `have`/`need` counts;
  `estimate` (and `trend`, `reached_on`, `by`) are present exactly when
  there is something to report, each absence with its own `*_reason`.

## v0.2.0 — 2026-09-20

### Foods are a shared record

- Anyone signed in can correct any food, including one imported from USDA or
  Open Food Facts. The person holding the packet is usually not whoever typed
  it in first.
- Every edit is kept. A food's detail dialog shows its full history, who made
  each change and what moved, and any revision can be restored — as a new
  revision, so the edit being undone stays on the record.
- Foods are verified by other people, not by a checkbox. Confirmations and
  disputes are counted per revision against an instance-wide quorum, so an edit
  resets agreement rather than inheriting it. A disputed food is shown as
  disputed in lists, not merely as "unverified".
- Preparation variants: cooked, raw, drained — separate entries pointing at a
  parent, seeded from the parent's numbers.
- Foods are entered the way the label prints them: per serving, converted to
  per 100 g on the server. Range checks apply to the stored figure and the
  error names the serving size when that is the likely culprit. Also fixes the
  numeric fields in the food form losing focus after every keystroke.
- `GET /foods/export` dumps the whole dataset keyed by natural identity rather
  than internal ids, for a shared seed repository.

### Recipes

- A recipe can be an ingredient of another recipe, by reference and in
  servings. Correct the base sauce once and every dish built on it follows.
  Cycles are refused and nesting is capped at five levels.
- An ingredient can be only words — "salt and pepper to taste" — with no
  nutrition attached. The recipe says how many such ingredients it has, counted
  through nesting, so its macros are never quietly short.
- A recipe opens as a page to read, with the method as numbered steps, and the
  editor behind an Edit button for its owner.
- Recipes carry photos, shown as a cover on the list. A recipe photo is visible
  to whoever can see the recipe.

### Display

- Each account chooses which nutrients its readouts show and which the home
  page charts; all eight stored nutrients are available, not four.
- The home chart plots every nutrient as a percentage of its own goal or
  budget, on one axis, with the actual-figure small multiples one toggle away.

### Administration and keys

- The first account to exist administers the instance. Administrators can
  promote, suspend and survey accounts, remove a food outright, and cannot lock
  the instance out of itself.
- Personal API keys with `read` or `write` scope, shown once and stored as a
  digest. A read key is refused any unsafe method; no key can manage keys or
  administer the instance.
- The verification quorum is an instance setting changed from the admin area.
  `FOOD_QUORUM` seeds a fresh install and is ignored once an administrator has
  saved a value.
- Sign-ups are an instance setting too. `ALLOW_REGISTRATION` seeds it the same
  way; closing them from the admin area takes effect on the next request. The
  first account on an empty instance is always admitted, so a closed instance
  can never lock a fresh install out. The sign-in page says when sign-ups are
  closed instead of offering a form the server will refuse.
- `/api/v1/health` reports the commit and build time the image was built from,
  and Settings shows them, so a bug report can say exactly what it is running.
  A source build that was not stamped reports `null` rather than a guess.

### Housekeeping

- The frontend's formatting is pinned by `.prettierrc` and checked in CI.
- `docs/PLAN.md` records the decisions and the phases ahead.

### API changes

**Breaking** — response fields that were renamed, removed or made nullable:

- Recipe items (`Recipe.items[]`): `food_name` is now `name` and `food_brand`
  is now `brand`, because an ingredient is no longer always a food. `food_id`
  and `quantity_g` are nullable. Added `sub_recipe_id`, `servings`, `label`
  and `weight_g`; exactly one of `food_id`, `sub_recipe_id` and `label` is set.
- `Photo.weight_entry_id` is nullable; `recipe_id` was added. Exactly one is
  set.
- `PUT /foods/{id}` no longer returns 403 for imported foods: every food is
  editable by anyone signed in. `DELETE /foods/{id}` is refused (403) once
  anyone else has edited or verified the entry; administrators may still
  delete.
- `DELETE /recipes/{id}` is also refused (400) while another recipe uses this
  one, not only while a diary entry does.
- A 403 always carries a reason in `message`; the bare `"forbidden"` message
  is gone.

**Additive** — new endpoints and fields, safe for existing clients:

- `GET /health`: `git_sha` and `built_at`, both nullable strings.
- `GET /auth/registration` (public): `{ "open": bool }`. `POST /auth/register`
  returns 403 with `"sign-ups are closed on this instance"` when they are.
- `Profile` (`GET /auth/me`, `GET/PATCH /profile`): `is_admin`,
  `shown_nutrients`, `chart_nutrients`, `chart_mode` (`percent` | `actual`).
  `PATCH /profile` accepts the last three.
- `Food`: `nutrient_basis` (`per_100g` | `per_serving`), `variant_of`,
  `variant_label`, `revision`, `verified_at`, `disputed_at`. `FoodDetail`
  (`GET /foods/{id}`, and the body of create, update, import, verify and
  revert): `provenance`, `variants`, `parent`. `POST /foods` and
  `PUT /foods/{id}` accept `nutrient_basis` (default `per_100g`, so an older
  client keeps its meaning), `variant_of`, `variant_label` and `edit_summary`.
- `GET /foods/{id}/revisions`, `POST /foods/{id}/revert`,
  `GET|POST|DELETE /foods/{id}/verify`, `GET /foods/export?verified_only=`.
- `GET|POST /keys`, `DELETE /keys/{id}`. An API key is accepted as
  `Authorization: Bearer nomi_…` or `x-api-key: nomi_…`.
- `GET /admin/stats`, `GET /admin/users`, `PATCH /admin/users/{id}`,
  `GET|PUT /admin/settings`. `InstanceSettings` carries `food_quorum`,
  `food_quorum_updated_at`, `allow_registration`,
  `allow_registration_updated_at`, `updated_at`, `updated_by_name`. In
  `PUT /admin/settings` both `food_quorum` and `allow_registration` are
  optional and an omitted one is left alone; an empty body is a 400.
- `POST /recipes` and `PUT /recipes/{id}` items accept `sub_recipe_id` with
  `servings`, or `label` alone, beside the existing `food_id` with
  `quantity_g`.
- `RecipeSummary` (`GET /recipes`): `untracked_count`, `cover_photo_url`.
  `Recipe`: `untracked_count`.
- `GET|POST /recipes/{id}/photos`, the same shape as weigh-in photos.

## v0.1.0

First release: weight tracking, the diary, recipes, a food database fed from
USDA and Open Food Facts including by barcode, targets with a direction,
progress photos and derived reminders.
