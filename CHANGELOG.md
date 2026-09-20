# Changelog

The API is a contract. A response field is never renamed, removed or made
nullable except at a tagged release; between releases, changes are additive
only. Every wire-level change is listed under **API changes** for its release,
breaking ones first and marked as such, so a client author can read one
section and know what to update.

## Unreleased (v0.2.0)

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
