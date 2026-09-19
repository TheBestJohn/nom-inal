# nom-inal

Self-hosted nutrition tracking: weight, calories, macros, recipes, and a food
database that looks foods up from government and open data sources — including
by UPC/EAN barcode.

Rust (Axum) API + Postgres + React SPA, all behind one `docker compose up`.

---

## Features

| | |
|---|---|
| **Weight tracking** | One weigh-in per day with optional body-fat %, trend chart, 7-entry moving average, kg/lb toggle, target line |
| **Calorie & macro diary** | Log foods by weight or recipes by serving, grouped into breakfast/lunch/dinner/snack, with progress against your daily targets |
| **Recipes** | Build from any food in your library; totals and per-serving macros are computed for you and recalculate live as you edit |
| **Recipes inside recipes** | Add a serving of one recipe as an ingredient of another. Linked, not copied — correct the base sauce once and every dish built on it follows |
| **Ingredients that are just words** | "Salt and pepper to taste" needs no database entry. It contributes nothing to the macros, and the recipe says how many such ingredients it has so the totals are never quietly short |
| **Food database** | Global and shared: custom foods plus anything imported from USDA or Open Food Facts. Anyone can correct any entry — see **Foods are a shared record** below |
| **Food history & verification** | Every edit is kept, attributed and reversible. Entries stay unverified until other people confirm the numbers, and an edit resets that |
| **Food variants** | Cooked, raw, drained — separate entries pointing at a parent, because they are the same ingredient and different numbers |
| **Enter what the label says** | A manually added food is typed per serving, the way the packet prints it, and converted server-side. Imports stay per 100 g, because that is how USDA and Open Food Facts publish |
| **Add a food anywhere** | Create one from the Foods page, or inline while logging a meal or building a recipe — a search that found nothing offers to create what you typed |
| **Instant search** | Streams results over SSE as you type, tier by tier, and tolerates typos — "chikn brest" finds chicken breast |
| **Recipe sharing** | Private by default; mark one public and everyone can read and log it, while only you can change it. Unlike foods, a recipe is yours |
| **Barcode lookup** | Type or scan a UPC/EAN and import the product in one click |
| **Progress photos** | Attach photos to a weigh-in. Downscaled and re-encoded on upload, which strips EXIF — phone photos carry GPS |
| **Reminders** | "It's been three weeks since your last weigh-in", at a cadence you set |
| **Choose what you see** | Every food stores eight nutrients; pick which reach the screen, and which get a chart on the home page. Kept on your account, so it follows you between devices |
| **Dark mode** | Follows your OS by default, with a toggle that overrides it. Applied before first paint, so there is no flash of the wrong theme |
| **Goals & budgets** | Per-nutrient daily targets that point in a direction: a **budget** is a ceiling to stay under, a **goal** is a floor to reach. Covers calories, the three macros, fibre, sugar, saturated fat and sodium |
| **Accounts** | Email + password sign-up, Argon2id hashing, closable once your accounts exist |
| **Administration** | The first account owns the instance: promote, suspend and survey accounts, and remove a food outright |
| **API keys** | Per-user, scoped read or read-write, revocable, for scripts and dashboards |
| **Export** | `GET /foods/export` dumps the whole food database keyed by natural identity, ready to commit to a repository |
| **OpenAPI 3.1** | Generated from the handlers, served at `/api/v1/openapi.json` |

### Where the food data comes from

- **[USDA FoodData Central](https://fdc.nal.usda.gov/)** — the US government's
  food composition database (Foundation Foods, SR Legacy, and Branded).
  Needs a [free API key](https://fdc.nal.usda.gov/api-key-signup). Optional:
  without one, USDA search is skipped and everything else still works.
- **[Open Food Facts](https://world.openfoodfacts.org/)** — open, crowd-sourced
  product data, keyed by barcode. No API key. This is the barcode source, since
  USDA carries GTINs but has no lookup-by-barcode endpoint.

Neither provider is required at runtime — if one is down or rate-limited, the
search returns what the other found plus a note saying which was unavailable.

---

## Quick start

Published images live in the GitHub Container Registry, so you can run it
without a toolchain or a checkout of the source:

```bash
curl -O https://raw.githubusercontent.com/TheBestJohn/nom-inal/main/docker-compose.yml
curl -o .env https://raw.githubusercontent.com/TheBestJohn/nom-inal/main/.env.example
# edit .env: POSTGRES_PASSWORD and JWT_SECRET are required

docker compose pull
docker compose up -d
```

Images are `linux/amd64` and `linux/arm64`, so a Pi or an ARM VPS works the
same as an x86 box.

| Tag | What it is |
|---|---|
| `latest` | The most recent released version. |
| `1.4.2`, `1.4`, `1` | A specific release, and the moving majors above it. |
| `edge` | The tip of `main`. Current, not vetted. |

Pin `NOM_INAL_VERSION` in `.env` if you would rather upgrades be a decision you
make than something a `pull` does for you.

### From source

```bash
git clone https://github.com/TheBestJohn/nom-inal.git
cd nom-inal

cp .env.example .env
# Required: set POSTGRES_PASSWORD and JWT_SECRET.
#   openssl rand -base64 48   # good source for JWT_SECRET
# Optional: set USDA_API_KEY to enable USDA search.

docker compose up -d --build
```

`--build` is what makes it compile locally rather than pull. Both use the same
compose file.

Open <http://localhost:8088> and create your account.

Once your accounts exist, set `ALLOW_REGISTRATION=false` in `.env` and
`docker compose up -d` again to stop further sign-ups.

### Services

| Service | Image | Purpose |
|---|---|---|
| `db` | `postgres:16-alpine` | Data. Persisted in the `db-data` volume. |
| `api` | built from `./backend` | Rust API. Not published to the host. |
| `web` | built from `./frontend` | nginx serving the SPA and proxying `/api` to `api`. The only published port. |

The API runs its migrations at startup, so there is no separate migration step,
and `db` has a healthcheck that `api` waits on — it never starts against a
Postgres that isn't accepting connections yet.

### Backups

Two things to keep: the database, and the photo volume.

```bash
# database
docker compose exec -T db pg_dump -U nominal nominal | gzip > backup.sql.gz
gunzip -c backup.sql.gz | docker compose exec -T db psql -U nominal nominal

# photos
docker compose cp api:/data/photos ./photo-backup
docker compose cp ./photo-backup/. api:/data/photos
```

The photo volume is mounted at `/data/photos`, a directory the image creates
and owns as the runtime user. That ownership matters: Docker seeds an empty
named volume from the image's directory at the mount path, ownership included,
so without it the volume is created root-owned and the non-root process cannot
write to it.

Photo bytes live on a volume rather than in Postgres. A few hundred kilobytes a
row would work, but it would make every `pg_dump` carry every photo and every
restore rewrite them. The cost of that choice is this second command — if you
back up only the database, the photos are gone.

---

## Configuration

Set in `.env` (see `.env.example`).

| Variable | Default | Notes |
|---|---|---|
| `POSTGRES_PASSWORD` | — | **Required.** |
| `JWT_SECRET` | — | **Required.** Anyone holding this can mint a token for any account. |
| `POSTGRES_USER` / `POSTGRES_DB` | `nominal` | |
| `WEB_PORT` | `8088` | Host port for the UI. |
| `NOM_INAL_VERSION` | `latest` | Which published image tag to run. |
| `JWT_TTL_HOURS` | `168` | Session length. |
| `USDA_API_KEY` | _empty_ | Enables USDA search. |
| `ALLOW_REGISTRATION` | `true` | Set `false` to close sign-ups. |
| `MAX_UPLOAD_MB` | `15` | Largest accepted photo, before downscaling. |
| `FOOD_QUORUM` | `2` | **Seeds** the verification quorum on a fresh install; after an administrator saves one in the admin area, this is ignored. Set to `1` on a single-user instance — a second opinion that can never arrive means nothing is ever verified. |
| `TRGM_WORD_THRESHOLD` | `0.4` | Fuzzy-search strictness, 0–1. Lower matches more typos and more noise. |
| `RUST_LOG` | `nom_inal=info,…` | `tracing-subscriber` filter. |

The API also reads `BIND_ADDR` and `CORS_ORIGINS`; both only matter outside
Docker, where the SPA is served from a different origin than the API.

---

## Design notes

**Foods are a shared record, not personal notes.** A food is a claim about the
world, so anyone signed in can correct any entry, including one imported from a
provider — the person holding the packet is usually not whoever typed it in
first. Three things make that safe rather than reckless:

- *Everything is kept.* A database trigger writes a JSONB snapshot of the row on
  insert and on any substantive update. The trigger owns the write, so no code
  path can skip it; the worst a caller can do by forgetting to set the
  transaction-local actor is leave one revision unattributed. The snapshot is
  JSONB rather than a typed mirror table because the nutrient list is the part
  of this schema most likely to keep growing, and a mirror would need two
  migrations per column.
- *Agreement is per revision.* Verifications are keyed on
  `(food, revision, user)`, so an edit produces a revision with no votes rather
  than inheriting confidence that was given to the numbers it replaced. Nothing
  has to be reset — the old votes simply stop being selected, and stay visible
  as superseded. You cannot vouch for your own edit, disputes are subtracted
  from confirmations, and the quorum — how many net confirmations an entry needs
  — is set in the admin area. Changing it re-evaluates every food in the same
  transaction: lowering it promotes entries that already had the support,
  raising it demotes the ones that no longer clear the bar, and nobody's votes
  are touched either way.
- *Undoing is additive.* A revert restores an old snapshot as a **new** revision,
  so the edit being undone stays on the record. Deletion closes once anyone else
  has edited or verified an entry; past that the answer is an edit or a revert,
  and an administrator is the escape hatch for an entry that should not exist.

A re-import from USDA or Open Food Facts refreshes only rows still at revision 1
— untouched copies of what the provider sent. Past that, somebody has
deliberately disagreed with upstream, and a refresh must not quietly undo them.

**Policy lives in the database, deployment config lives in the environment.**
The quorum decides how this community works and is visible to exactly the people
allowed to change it, so it is a row in `instance_settings` rather than a
variable that needs shell access and a restart. `FOOD_QUORUM` still seeds a fresh
install — `updated_at IS NULL` marks an instance nobody has configured yet — and
stops applying the moment an administrator saves a value, because otherwise every
restart would silently undo them.

Two columns on `foods`, `verified_at` and `disputed_at`, cache the answer that
the vote counts imply. They exist so a list — and especially the tiered streaming
search — can show the state without aggregating votes per result. They are
recomputed by the same SQL function wherever they can change (a vote, a
withdrawal, the quorum moving) and cleared by the edit trigger, and the snapshot
function excludes them so settling a vote never looks like an edit.

**The first account administers the instance.** A self-hosted deployment has no
outside authority to appoint an owner, so installing it is the authority.
Administrators cannot demote or suspend themselves, and the change is rolled
back if it would leave no active administrator at all.

**API keys are digested, not password-hashed.** Argon2 exists to make guessing a
low-entropy human-chosen secret expensive. These tokens are 256 bits from the OS
random source, so there is nothing to guess, and a slow hash would instead add
its cost to every authenticated request — and force a scan of every key row to
find which one a token belongs to. SHA-256 keeps the lookup an indexed equality
check. A read-only key is refused any unsafe method at the extractor rather than
in each handler, and no key can manage credentials or administer the instance,
so a leaked key cannot mint its own replacements.

**Nutrients are stored per 100 g, but entered the way they are printed.**
Storage stays per 100 g: both upstream sources publish that basis, so importing
is lossless, and every derived figure — a serving, a recipe row, a diary entry,
a day — is one multiplication by `grams / 100`. Storing per-serving values
instead would mean a conversion on every import and another on every read. It is
not how the numbers arrive, though. USDA and Open Food Facts publish per 100 g, so an import needs
no thought; a person adding a food is reading a label, and a label states one
serving. Asking them to divide by 0.28 in their head does not fail loudly when
they get it wrong — it stores a food that is 3.5x too rich, and in a database
anyone can edit, the next person sees 500 kcal and "fixes" it back.

So a food carries the basis it was entered in, the form offers both and converts
between them live, and the conversion happens on the server: one implementation,
one rounding, for the UI and for a script posting straight off a packet. The
range checks apply to the converted figure, because that is the number being
stored — which also means a mistyped serving size is caught by what it works out
to ("works out to 7000 kcal per 100 g — check the serving size of 2 g") rather
than sailing through.

**A recipe can be an ingredient of another recipe, by reference.** A sub-recipe
item stores a link and a number of servings, and the parent's macros come from
whatever the sub-recipe says today. Copying its ingredients in would freeze
them, and the first correction to a base sauce would leave every dish built on
it quietly wrong.

That makes recipes a graph, so two things are enforced on write: no cycles, and
no more than five levels of nesting. The depth cap is not really about people —
nobody builds five levels — it bounds the read. Totals are computed by walking
the graph path by path, and a chain of recipes each containing the next twice
has 2^depth paths, so an uncapped depth would be a way for one account to make
everyone else's diary slow.

The sum itself lives in one SQL function, `recipe_totals`. It had been written
out three times — a lateral in the recipe list, a lateral in the diary, a fold
in Rust for the detail view — which was survivable while a recipe was a flat
list of foods. Teaching only some of them about nesting would have made a diary
entry and the recipe page report different numbers for the same meal, which is
the kind of bug nobody reports because they assume they misread it.

**An ingredient with no nutrition still has to be visible.** Some ingredients
are only words — a pinch of salt, a squeeze of lemon. Inventing a food row for
them would put fake entries in a shared, community-edited database to record
something that rounds to nothing, so they are stored as a label and contribute
nothing. The risk is the obvious one: a recipe whose macros silently exclude
three ingredients is worse than one with no macros at all. So the count travels
with the numbers, out of the same walk that produced them, and counts the ones
inside sub-recipes too — those are the ones you cannot see from the page you are
reading.

**The home page uses small multiples, never a shared axis.** Calories run to a
couple of thousand and fat to about seventy, so plotting them together would
flatten every macro onto the floor, and a second y-axis would invite comparing
two scales that have nothing to do with each other. Each charted nutrient gets
its own chart, its own axis and its own unit; the heading carries the identity,
so no legend is needed and colour is never doing the work alone.

The nutrient colours are checked with a palette validator rather than by eye.
The first attempt reused the existing `--chart-*` tokens, which hold the same
values as the macro colours — protein and sodium came out identical side by
side. The second put sodium and saturated fat at ΔE 0.7 under deuteranopia:
indistinguishable to a red-green colourblind reader, and fine to me. Both modes
now pass, stepped separately against their own surface rather than flipped.

**Diary entries are a strict XOR.** An entry is either *a food, in grams* or *a
recipe, in servings*, enforced by a database `CHECK` as well as by the handler,
so the two quantity columns can never both be set. Recipe ingredients now use
the same shape for the same reason — a row carrying both grams and servings is
not a slightly-wrong ingredient, it is an unanswerable one. A third arm covers
free-text ingredients, which carry no quantity at all: a gram figure next to
something the totals deliberately ignore would be worse than no figure.

**A recipe's macros are never stored.** They are derived from its ingredients on
read, which means correcting a food's nutrition retroactively fixes every recipe
using it, instead of leaving stale copies behind.

**Foods are global; recipes are private by default.** These pull in opposite
directions deliberately. A food is a fact about a product — "oats are 379 kcal
per 100 g" is true for everyone, so making each account re-import the same
barcode is pure duplication. A recipe is authorship: yours until you share it.
Editing follows creation in both cases: anyone can *use* a food, only its author
can change it.

**Search streams instead of ranking once.** Logging a meal is the hottest path,
and a single ranked query is slow twice over — an exact hit waits behind a
trigram scan, and a typo returns nothing. So the search runs as progressively
looser tiers (`exact` → `prefix` → `contains` → `fuzzy` → `fuzzy_words`), each
flushed over Server-Sent Events the moment it returns. The first lands in single
-digit milliseconds, so the list is populated while the fuzzy scan is still
running.

**Fuzzy matching uses word similarity, not whole-string similarity.** "chikn"
against "Chicken breast, raw" scores 0.14 by `similarity()` — diluted by the
length of the whole name — but 0.50 by `word_similarity()`, which compares the
query against the best-matching word. Both use the GIN trigram indexes. Postgres
defaults the threshold to 0.6, tuned for whole documents and too strict for
autocomplete, so it is lowered to 0.4 (see `TRGM_WORD_THRESHOLD`).

**Identical foods are collapsed in search, before the limit.** A global table
accumulates the same product entered by several people. Rows matching on name,
brand and macros are collapsed to the oldest; two foods that merely share a name
but differ nutritionally are different things and both stay. Collapsing after
the limit would let five copies of one food consume the entire result budget.

**Uploads are re-encoded, not stored as received.** That costs CPU per upload
and buys three things: EXIF is dropped (phone photos routinely carry GPS, and a
progress photo is usually taken at home), a 12 MP upload becomes a few hundred
kilobytes so the volume grows predictably, and anything that does not decode is
rejected rather than stored and served back later. Photos are served by the API
with an ownership check, not as static files, so a guessed URL is not enough to
read one.

**Reminders have no scheduler.** A reminder stores only a cadence; whether you
are overdue is derived, on read, from the records you already keep. So there is
no job queue, nothing to catch up after the container has been down for a week,
and no stored "next due" date that can drift out of step with reality. The
trade-off is that nothing can reach out to you — these appear in the app, not in
your inbox.

**Targets are standing settings, never set up daily.** A target belongs to you,
not to a date: set it once and it is evaluated against every day, including past
days and days with nothing logged.

**A target carries a direction, not just a number.** A budget and a goal are
the same arithmetic read in opposite directions: 120% of a calorie budget is a
problem, 120% of a protein goal is a success. Storing the direction is what lets
the server say `over` for one and `met` for the other, so every client agrees
instead of each re-deriving it. Protein and fibre default to goals and the rest
to budgets, and any of them can be flipped — carbs are a budget when cutting and
a goal when bulking.

**Targets live in their own table rather than as columns on `users`.** With a
direction, each nutrient needs an amount and a kind; eight nutrients would mean
sixteen nullable columns, and adding a ninth would mean another `ALTER TABLE`.
One row per target makes "which targets are set" a query, and adding a nutrient
a data concern rather than a schema change.

**Days with nothing logged are excluded from averages.** An unlogged day is
missing data, not a zero-calorie day; averaging it in would drag every average
down and misrepresent the week.

**Imported foods are read-only; your own are editable.** An imported row mirrors
an upstream record, so letting it be edited would silently diverge from the
source it claims to come from. Imported foods are shared across accounts —
reference data is useful to everyone — while your custom foods stay yours.

**Imports are idempotent** on `(source, source_id)`, so scanning the same
barcode twice refreshes the existing food rather than creating a duplicate.

---

## API

Base path `/api/v1`. All endpoints except `/health`, `/auth/register` and
`/auth/login` require `Authorization: Bearer <token>`.

The full spec is generated from the handlers and served at
<http://localhost:8088/api/v1/openapi.json> — load it into Swagger UI, Insomnia,
Bruno or Postman for a browsable reference.

<details>
<summary>Endpoint summary</summary>

```
GET    /health

POST   /auth/register            POST   /auth/login             GET  /auth/me
GET    /profile                  PATCH  /profile
GET    /search/foods                     # SSE: tiered, fuzzy, streams as it finds
GET    /targets                  PUT    /targets          # replaces the whole set
GET    /targets/{nutrient}       DELETE /targets/{nutrient}

GET    /weights                  POST   /weights                GET  /weights/stats
GET    /weights/{id}             PATCH  /weights/{id}           DELETE /weights/{id}
GET    /weights/{id}/photos      POST   /weights/{id}/photos    # multipart
GET    /photos/{id}              DELETE /photos/{id}            PATCH /photos/{id}/caption

GET    /reminders                PUT    /reminders
GET    /reminders/status                 # what you are overdue for, right now

GET    /foods                    POST   /foods
GET    /foods/{id}               PUT    /foods/{id}             DELETE /foods/{id}
GET    /foods/search/external            # USDA + Open Food Facts
GET    /foods/barcode/{upc}              # local hit and/or importable candidate
GET    /foods/external/{source}/{id}     # full upstream record
POST   /foods/import                     # idempotent; will not overwrite a local edit
GET    /foods/export                     # whole dataset, keyed by natural identity
GET    /foods/{id}/revisions             # full history, with a per-revision diff
POST   /foods/{id}/revert                # restores an old revision as a new one
GET    /foods/{id}/verify        POST   /foods/{id}/verify      DELETE /foods/{id}/verify

GET    /keys                     POST   /keys                   DELETE /keys/{id}
GET    /admin/stats              GET    /admin/users            PATCH  /admin/users/{id}

GET    /recipes                  POST   /recipes    # items are a food in grams
                                                     # or a recipe in servings
GET    /recipes/{id}             PUT    /recipes/{id}           DELETE /recipes/{id}

GET    /diary                    POST   /diary
GET    /diary/day                        # one day, grouped by meal, vs targets
GET    /diary/summary                    # per-day totals over a range
GET    /diary/{id}               PATCH  /diary/{id}             DELETE /diary/{id}
```

</details>

Errors are uniform — including malformed request bodies, which are routed
through the same error type rather than Axum's plain-text rejection:

```json
{ "error": "not_found", "message": "food not found" }
{ "error": "bad_request", "message": "targets[0].amount must be between 0.1 and 100000" }
```

A day's targets come back already evaluated, so clients render rather than
compute:

```json
{ "nutrient": "protein_g", "label": "Protein", "unit": "g", "kind": "goal",
  "amount": 160, "consumed": 190, "remaining": -30, "percent": 118.75,
  "status": "met" }
```

`remaining` is always `amount - consumed`, signed. `status` is `under`/`over`
for a budget and `short`/`met` for a goal.

---

## Releases

Pushing a `v*` tag builds both images for amd64 and arm64 and publishes them to
`ghcr.io/thebestjohn/nom-inal-{api,web}`:

```bash
git tag -a v0.1.0 -m "v0.1.0"
git push origin v0.1.0
```

Every push to `main` publishes `edge` as well, so the pipeline is exercised
continuously rather than only when a release is cut.

The images cross-compile rather than build under emulation. A Rust release
build through QEMU takes the better part of an hour and sometimes runs out of
memory on a hosted runner; compiling natively for a foreign target costs about
as much as a native build. The frontend goes further — its output is static
files, identical on every architecture, so only the nginx layer varies.

## Development

Requires Rust (stable), Node 22+, and a Postgres you can point at.

```bash
# database
createdb nominal

# API — migrations run on startup
cd backend
cp .env.example .env          # set DATABASE_URL and JWT_SECRET
cargo run

# SPA — proxies /api to localhost:8080, so the browser sees one origin
cd frontend
npm install
npm run dev                   # http://localhost:5173
```

Checks:

```bash
cd backend  && cargo test && cargo clippy --all-targets && cargo fmt --check
cd frontend && npm run build   # tsc -b && vite build
```

### Layout

```
backend/
  migrations/         SQL migrations, embedded into the binary
  src/
    domain/           request/response types and the nutrient arithmetic
    routes/           one module per resource
    services/         USDA and Open Food Facts clients
    auth.rs           password hashing, JWT, the CurrentUser extractor
    error.rs          the one place an error becomes an HTTP response
    openapi.rs        the generated spec
frontend/
  src/
    api/              typed client mirroring the API
    components/ui/    shadcn/ui primitives (Radix + Tailwind), owned in-tree
    components/       app components, including the three-source food picker
    lib/useFoodSearch parses the SSE search stream
    pages/            one per route
    index.css         the Tailwind v4 theme — every colour token lives here
```

The UI is **React 19 + Tailwind v4 + shadcn/ui** on Radix primitives. shadcn
components are copied into the repo rather than installed, so they are ordinary
source files you can edit. Theme tokens live in `src/index.css`; light and dark
are the same variables with different values, so retheming is one file.

Dark mode is a `.dark` class on `<html>`, set by a blocking inline script in
`index.html` before React mounts. That placement is deliberate: applying it from
a component paints the wrong theme first, and it would never reach the
signed-out page at all, since the toggle only exists inside the signed-in
shell.

`useFoodSearch` reads the SSE stream with `fetch` rather than `EventSource`,
because `EventSource` cannot send an `Authorization` header and the alternative
— putting the token in the query string — writes it into every proxy and access
log on the way.

Queries are runtime-checked rather than `sqlx::query!`-checked, so neither
`cargo build` nor the Docker build needs a live database.

---

## Security

- Passwords are hashed with Argon2id; API keys are SHA-256 digests of
  server-generated 256-bit tokens, shown once and never recoverable.
- A read-only API key is rejected for any unsafe HTTP method in the extractor,
  before a handler runs. No API key can manage keys or administer the instance.
- Suspending an account takes effect on the next request, for its session tokens
  and its API keys alike, rather than whenever a token happens to expire.
- Sign-in returns the same error whether the account is unknown or the password
  is wrong.
- Every user-owned query filters on the authenticated user id; `CurrentUser` is
  an extractor, so a handler that forgets authorization doesn't compile.
- The API is not published to the host by Docker Compose — only nginx is.
- There is no TLS here. Put it behind a reverse proxy (Caddy, Traefik, nginx)
  before exposing it to the internet.
