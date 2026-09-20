# Plan: making nom-inal as useful as possible

The app stores the right things. What it does not yet know is *why* you are
tracking, and it makes you work harder than necessary to get the day logged.
This plan is ordered by usefulness per unit of correctness risk, and every
phase ends in a tagged release.

## Decisions already made

1. **No `food_name`/`food_brand` aliases on recipe items.** The rename to
   `name`/`brand` was right: an ingredient is not always a food. The client
   that broke had a hand-written schema that went stale, which the MCP phase
   removes at the root. Rule from here on: **breaking response changes only
   at a tagged release; between releases, additive only.** `CHANGELOG.md`
   carries an "API changes" section per release.
2. **Build provenance.** `/api/v1/health` reports `git_sha` and `built_at`,
   baked in at image build and shown in Settings.
3. **Tag `v0.2.0`** once phase 0 lands; every later phase ends in a tag so
   `latest` never trails `edge` by more than one phase.
4. **The home chart stays percent-of-target.** No dual axis.

## Principles

- **Correct by construction.** One definition per fact (`recipe_totals`, the
  column lists, `lib/nutrients.ts`), enforced in the database where it can
  be. A preset or estimator that could disagree with the readouts is not
  shipped.
- **Silence is the bug.** Untracked ingredients, missing targets, incomplete
  days: always named, never dropped.
- **Storage is metric; units are a display preference.**
- **No medical advice.** Presets set nutrition targets and what is on screen.
  They never dose anything.

## Phase 0 — The API is a contract

- `CHANGELOG.md`, with an API changes section per release.
- `git_sha` and `built_at` in `/health`, from build args in the Dockerfile
  and the release workflow; shown in Settings.
- `ALLOW_REGISTRATION` becomes an instance setting an admin can toggle, with
  the env var seeding it — the same pattern as the food quorum. A
  self-hosted instance needs "close signups once my household has joined"
  without a redeploy.
- Tag `v0.2.0`.

## Phase 1 — "Why are you tracking?"

- A **tracking focus** on the profile, chosen at first sign-in and changeable
  in Settings: general health · weight loss · muscle gain / protein · keto /
  low-carb · diabetes / carb awareness · blood pressure / sodium · heart
  health · custom.
- **A preset is applied, not enforced.** Choosing one fills in shown
  nutrients, chart nutrients, target kinds and suggested amounts (built on
  the existing TDEE suggestion with per-focus ratios). Everything stays
  editable; changing focus later asks before overwriting.
- **Derived nutrients, computed server-side** so every readout agrees: net
  carbs (carbs − fiber) as a first-class nutrient; share of calories from
  each macro; per-meal carbs for the diabetes focus. Nothing new is stored.
- **Settings reorganised** around the focus: Focus & goal / Body & units /
  Targets / Display / Reminders / Integrations / Account / Admin, each its
  own route so it can be linked to.

## Phase 2 — Estimators that use your own data

- **Adaptive TDEE.** From at least 14 complete days of intake plus the
  weight trend, estimate actual expenditure by energy balance, show it beside
  the formula estimate, and offer to re-base the calorie budget. The
  correctness key is a per-day **"I logged everything"** flag so half-logged
  days cannot poison it.
- **Goal projection**: at the current trend, the target weight is reached on
  a date; what deficit hits a chosen date; a warning past about 1 % of body
  weight per week.
- Protein per kg by focus, macro split from a ratio, keto ratio check, BMI
  with its caveat stated.
- They live where the number is used: the Targets settings, and as one line
  of context on Today.

## Phase 3 — Logging speed

- Recent and frequent foods first in the picker; copy yesterday / copy a
  meal; save a meal as a recipe in one tap.
- **Barcode scanning with the camera** (`BarcodeDetector`, with a ZXing
  fallback). The Open Food Facts lookup already exists server-side.
- **Household portions** from USDA portion data ("1 cup", "1 slice"), stored
  per food, offered in the picker beside grams.
- **Units preference** (lb / oz / ft-in), display only.
- **PWA manifest**, so it installs on a phone.

## Phase 4 — Sharing, export, import

- **Share link per recipe**: tokenised, read-only, no login, photos
  included, revocable. Separate from `is_public`, which means "everyone on
  this instance".
- **Recipe export** as JSON (the seed-repo format), Markdown, and a print
  stylesheet.
- **Import a recipe from a URL** via schema.org JSON-LD; ingredient lines
  parsed and resolved against foods with a review step; anything unresolved
  becomes a free-text ingredient.
- **Full account export and import** (JSON plus CSV).

## Phase 5 — MCP, served by the API itself

- The backend serves MCP at `/mcp` (Streamable HTTP), authenticated with the
  existing API keys: the read scope gets read tools, write gets all.
- **Tools are generated from the same route registry as the OpenAPI doc**,
  so a new endpoint is a new tool with nothing to install or update, and it
  cannot drift from the API.
- A few hand-written composite tools shaped like how people talk:
  `log_food("2 eggs and toast", meal, date)` doing search → resolve →
  confirm → create; `today()`; `progress(range)`; `add_recipe_from_text`.
- A resource explaining the domain (nutrient bases, recipes as ingredients,
  free text, goal vs budget) so a model does not guess.
- `npx mcp-remote` covers stdio-only clients. An MCP section joins the
  smoke suite.

## Order

0 → 1 → 5 → 3 → 2 → 4. Phase 0 is small and unblocks trust. Phase 1
reshapes Settings, so it goes before anything else touches Settings. Phase 5
comes early because the app is used through AI. Phase 2 needs data to
accumulate, so its flag ships in phase 1 and the estimators arrive once
there is something to estimate.
