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
| **Recipe sharing** | Private by default; share one and everyone can read and log it — every account, and anyone holding its link at `/r/{id}` without signing in. Only you can change it, and un-sharing takes the link away. Unlike foods, a recipe is yours |
| **Recipe export and import** | Save a recipe as JSON with no internal ids in it, or as a Markdown card; print the read view. Import one from a URL: the page's schema.org recipe is read, each ingredient line is parsed and matched against the food database for you to confirm, and what cannot be matched stays as words |
| **Account export and import** | Everything you own as one JSON file — no password, no keys — or a zip of CSVs for a spreadsheet. Importing merges by name and date rather than restoring, so nothing is duplicated and the same file twice changes nothing |
| **Barcode lookup** | Type or scan a UPC/EAN and import the product in one click |
| **Photos** | Attach photos to a weigh-in (private) or a recipe (shared with the recipe). Downscaled and re-encoded on upload, which strips EXIF — phone photos carry GPS |
| **Reminders** | "It's been three weeks since your last weigh-in", at a cadence you set |
| **Choose what you see** | Every food stores eight nutrients; pick which reach the screen, and which get a chart on the home page. Kept on your account, so it follows you between devices |
| **Dark mode** | Follows your OS by default, with a toggle that overrides it. Applied before first paint, so there is no flash of the wrong theme |
| **Goals & budgets** | Per-nutrient daily targets that point in a direction: a **budget** is a ceiling to stay under, a **goal** is a floor to reach. Covers calories, the three macros, fibre, sugar, saturated fat and sodium |
| **Accounts** | Email + password sign-up, Argon2id hashing, closable once your accounts exist |
| **Administration** | The first account owns the instance: promote, suspend and survey accounts, close sign-ups once everyone is in, and remove a food outright |
| **API keys** | Per-user, scoped read or read-write, revocable, for scripts and dashboards |
| **Food export** | `GET /foods/export` dumps the whole food database keyed by natural identity, ready to commit to a repository |
| **OpenAPI 3.1** | Generated from the handlers, served at `/api/v1/openapi.json` |
| **MCP** | Served by the API at `/mcp`, behind the same keys. Every endpoint is a tool, generated from the OpenAPI document, plus `log_food`, `today`, `progress` and `add_recipe_from_text` — see **MCP** under API |

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

Once your accounts exist, close sign-ups from the admin area (Admin →
Sign-ups). It takes effect at once, and an empty instance always admits its
first account, so closing them can never lock you out.

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
| `ALLOW_REGISTRATION` | `true` | **Seeds** whether sign-ups are open on a fresh install; after an administrator saves the toggle in the admin area, this is ignored. The first account on an empty instance is always allowed. |
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
The quorum decides how this community works, and whether sign-ups are open
decides who it is for; both are visible to exactly the people allowed to change
them, so they are columns of the one-row `instance_settings` rather than
variables that need shell access and a restart. `FOOD_QUORUM` and
`ALLOW_REGISTRATION` still seed a fresh install and stop applying the moment an
administrator saves that setting, because otherwise every restart would
silently undo them. Each seeded setting keeps its own "saved from the admin
area" marker rather than sharing one for the row: with a shared marker, an
instance whose administrator had set the quorum would refuse to seed a
registration setting that arrived in a later upgrade, and a host that closed
sign-ups in `.env` would find them quietly open again.

Closing sign-ups never applies to an empty instance: the first account is
always admitted, because an instance nobody can sign in to cannot be reopened
from inside.

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

**Household portions are per food, and a diary entry is still grams.** Nobody
weighs a cup of oats; they measure a cup, and the question they actually have
is how many grams that is *for this food* — a cup of oats and a cup of milk
weigh nothing alike. So a portion ("1 cup · 80 g") is a row on the food,
imported from USDA's `foodPortions` where the record has them and typed in
otherwise, and the picker offers it beside grams. What is written to the diary
is the grams: the portion is a way of arriving at the number, not a second unit
of storage, so nothing downstream has to know it existed.

Portions sit outside the revision and verification model. They are measures of
the food rather than claims about its nutrition — adding "1 mug · 300 g" says
nothing about how many calories are in it — so adding one does not bump the
revision or unsettle anyone's confirmation of the numbers, and a re-import
refreshes the provider's portions without touching the ones a person typed in,
on an edited food as much as an untouched one, because there is no correction
to undo. The trade is that a portion has no history: a wrong one is a wrong
gram figure the person sees as they pick it, and the fix is to remove it.

The column list that reads a food is a small macro rather than a string, since
a food now arrives with a JSON aggregate of its portions that has to name the
row it belongs to, and the two statements that alias `foods` as `f` cannot
say `foods.id`. One definition parameterised by the table name is what keeps it
from becoming two lists again — which is how the provenance columns once broke
two modules at runtime.

**Units are a display preference; storage is metric, and food is always grams.**
The profile carries `units`, `metric` or `imperial`, and nothing else changes:
kilograms and centimetres go over the wire, the client converts at the edge,
and the API never sees a pound. Body weight and height are the whole of it.
Food amounts are deliberately not covered — "3 oz of chicken" is not how anyone
measures food at home, and the cases people do mean, a cup or a slice, are
household portions on the food rather than a unit switch.

**Recent foods rank by recency, with frequency as the tie-break.** The list the
picker shows before anything is typed answers "what did I have yesterday" first,
because that is the question it is usually asked; among several things from the
same day, the habitual one belongs first. Recipes are in the same list as foods
because they are logged like foods, and a second endpoint the picker merged by
hand would only invite the two to disagree. Copying a day is one
`INSERT … SELECT`, atomic by construction, and each copy is a fresh entry rather
than a link, since it is a new fact about a different day. Saving a meal as a
recipe keeps a logged recipe as a sub-recipe rather than unpacking it, for the
same reason any sub-recipe is linked and not copied.

**The service worker is network-first, not stale-while-revalidate.** It keeps
the built shell and one API response, today's diary (and the profile the app
boots from), and answers from that cache only when the network fails. The diary
is written to constantly and every write is followed by a re-read of the day; a
worker that answered that re-read from cache and refreshed in the background
would show the day as it stood before the entry just logged, every time. Writes
are never cached, photos are never cached, and the cache key carries a digest of
the bearer token so a second account on the same browser never sees the first
one's day. The worker is hand-written; the only thing it cannot know until build
time is the list of hashed files, which a dozen-line Vite plugin fills in along
with a version derived from it, so the worker's bytes change exactly when the
shell does.

**Foods are global; recipes are private by default.** These pull in opposite
directions deliberately. A food is a fact about a product — "oats are 379 kcal
per 100 g" is true for everyone, so making each account re-import the same
barcode is pure duplication. A recipe is authorship: yours until you share it.
Editing follows creation in both cases: anyone can *use* a food, only its author
can change it.

**Shared means public.** One switch on a recipe, with one meaning: every
account on the instance can read it, and so can anyone holding its link without
signing in. There is no separate share token to mint, list or revoke — the
recipe's own id is the link, and the switch is the revocation. The public route
and the signed-in one are the same assembly function called with no viewer, and
a photo's visibility is one SQL rule (`owner = viewer OR recipe.is_public`)
applied to the bytes as well as the listings, so with no viewer a weigh-in photo
can never satisfy it and a recipe photo satisfies it exactly while its recipe is
shared. One rule in one place, rather than a public copy that could drift.

**Exports carry no internal ids.** A recipe or account export names a food by
`lower(name)|lower(brand)` — the key `GET /foods/export` already uses — a
sub-recipe by inlining it, and a diary entry by its date, meal and what was
eaten. That is what makes the account import a *merge*: the same file can be
read into the account it came from, into a fresh account on another instance,
or twice, and each record is created once, updated when the file differs, and
otherwise counted as skipped. Foods are the exception in one direction: an
import adds a food the instance lacks and never overwrites one it has, because
the instance may have corrected it since and a personal file is not the place
to undo that. A food that is missing keeps its recipe line as text and is named
in the report, so a recipe is never silently short an ingredient.

**Importing a page is request forgery territory.** `POST /recipes/import` makes
the server fetch a URL the caller chose, and the server sits inside the
deployment's network. So the host is resolved first and every address it
resolves to has to be globally routable — loopback, private ranges, link-local
(where cloud metadata services live), carrier-grade NAT, IPv4 embedded in IPv6
and the rest are refused before a connection is opened — and the request is
pinned to those addresses so a name that resolves differently a moment later
cannot reach a different one. Redirects are not followed by the client; each
hop comes back through the same checks, at most four. Only http and https, no
credentials in the URL, only a page or JSON in reply, read against a size cap
under one timeout.

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

**A focus is a preset, applied and never enforced.** The app stored the right
things without knowing what they were for: someone counting sodium for their
blood pressure and someone counting protein for the gym were given the same
four readouts, the same calorie chart and no targets. The tracking focus is the
one fact the rest can be derived from — which nutrients are on screen, which are
charted, and which way each target points. Choosing one writes ordinary targets
and display preferences in a single transaction, and every one of them stays
editable; nothing anywhere reads the focus back to decide what a number means.
The presets live on the server, once, as rules priced from the energy estimate
(protein per kilogram, fat as a share of calories, carbs as what is left, sodium
as a fixed ceiling), each with the basis for its figure in a comment beside it.
A rule the profile cannot price is listed without an amount and says what it
needs, rather than being dropped. The column is NULL until the question has been
asked, which is what sends an account through the welcome flow exactly once;
"custom" is the answer "none of these" and is never asked again.

**The energy estimate is made on the server.** Mifflin–St Jeor, the activity
multipliers and the goal adjustment lived in the Settings page as a suggestion.
The presets derive their amounts from the same estimate, and two copies of a
formula with a rounding step each is how a preset and the suggestion beside it
come to disagree by a few kcal that nobody can explain. `GET /targets/suggestion`
is that estimate, and it reads the latest weigh-in rather than the target weight,
which the page and the presets had been answering differently.

**Net carbs is derived where a total is serialised, never stored.** It is a
nutrient a target can be set on and a chart can plot, but it is not a column: a
food, a recipe, an entry, a meal and a day each compute it from the carbs and
fibre they already carry at the one point they are written out, so none of them
can disagree about it, and a target on it is evaluated against exactly the
figure the readouts show. The day and the summary also carry the share of
energy from each macro, divided by the Atwater sum of the three rather than by
the stated calories — a label's calorie figure is rounded and sometimes counts
fibre or alcohol — so the three shares always total 100.

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

**A day is complete when its owner says so, and only complete days feed the
estimator.** Energy balance is the one identity in this domain that is
actually true — expenditure is what went in less what was stored — and the
only thing that can make it lie is a half-logged day counted as a small one.
The diary cannot tell that day from a fast day; both are a day with little in
it. So the distinction is stated, once per day, by the one person who knows,
and the adaptive estimate reads nothing else: mean intake on the days marked
"I logged everything", less what a least-squares line through the weigh-ins
stored at 7700 kcal/kg. A complete day with no entries is a real zero and is
listed in the summary with zero entries rather than dropped, because dropping
it would be the one way to make the flag invisible; the summary's average
keeps its meaning and stays over days with entries.

The estimate refuses to say a number on thin data — seven complete days and two
weigh-ins a week apart, or it reports what is still needed, as counts and as a
sentence, with `ready: false` so a client never has to guess from a missing
field. The same trend, extended, is the goal projection; it says when the
target is reached, or that the trend is flat or heading away, rather than
projecting a date from a slope that is the scale's own noise. A rate past 1 % of
body weight a week carries a caution on the number, which is all it is: none of
this is advice, and everything it prices lands as an ordinary, editable target.

**Imported foods are read-only; your own are editable.** An imported row mirrors
an upstream record, so letting it be edited would silently diverge from the
source it claims to come from. Imported foods are shared across accounts —
reference data is useful to everyone — while your custom foods stay yours.

**Imports are idempotent** on `(source, source_id)`, so scanning the same
barcode twice refreshes the existing food rather than creating a duplicate.

---

## API

Base path `/api/v1`. All endpoints except `/health`, `/auth/register`,
`/auth/registration` and `/auth/login` require `Authorization: Bearer <token>`.

The API is a contract. A response field is never renamed, removed or made
nullable between tagged releases; between releases changes are additive only,
and every wire-level change is listed under "API changes" for its release in
[`CHANGELOG.md`](CHANGELOG.md). `/health` reports the `version`, `git_sha` and
`built_at` of the running build, so a client can say exactly what it was
talking to.

The full spec is generated from the handlers and served at
<http://localhost:8088/api/v1/openapi.json> — load it into Swagger UI, Insomnia,
Bruno or Postman for a browsable reference.

<details>
<summary>Endpoint summary</summary>

```
GET    /health

POST   /auth/register            POST   /auth/login             GET  /auth/me
GET    /auth/registration                # public: are sign-ups open right now
GET    /profile                  PATCH  /profile
GET    /profile/focus                    # the eight focuses, one sentence each
GET    /profile/focus/preview            # what applying ?focus= would set
POST   /profile/focus                    # { focus, apply }: one transaction
GET    /search/foods                     # SSE: tiered, fuzzy, streams as it finds
GET    /targets                  PUT    /targets          # replaces the whole set
GET    /targets/suggestion               # the energy estimate and what it suggests
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
GET    /admin/settings           PUT    /admin/settings         # quorum, sign-ups

GET    /recipes                  POST   /recipes    # items are a food in grams
                                                     # or a recipe in servings
GET    /recipes/{id}             PUT    /recipes/{id}           DELETE /recipes/{id}
GET    /recipes/{id}/export              # ?format=json|markdown; no internal ids
POST   /recipes/import                   # { url }: a draft read off the page, nothing saved
GET    /public/recipes/{id}              # a shared recipe, no token; its photos at
GET    /public/photos/{id}               # this route, and nowhere else without one

GET    /account/export                   # everything the account owns; ?format=csv zips diary and weights
POST   /account/import                   # merge an export back in by natural identity

GET    /diary                    POST   /diary
GET    /diary/day                        # one day, grouped by meal, vs targets
GET    /diary/summary                    # per-day totals over a range
PUT    /diary/day/{date}/complete        # "I logged everything": { complete }
GET    /estimates/tdee                   # expenditure by energy balance, or what is still needed
GET    /estimates/projection             # when the trend meets the target; ?by= prices a deadline
GET    /diary/{id}               PATCH  /diary/{id}             DELETE /diary/{id}

POST   /mcp                              # MCP over Streamable HTTP, API keys only
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

### MCP

The API serves the [Model Context Protocol](https://modelcontextprotocol.io/)
itself, at `/mcp`, over Streamable HTTP. There is no separate server to run:
point an assistant at your instance's URL plus `/mcp` and authenticate with an
API key from Settings, as `Authorization: Bearer <key>` or `X-API-Key`. A
read-only key sees only the tools that read; a key that can make changes sees
them all. Session tokens are refused there — a key can be scoped and revoked
on its own, which is what you want for something that acts on your behalf.

**Every endpoint is a tool, generated from the OpenAPI document at startup.**
The same `#[utoipa::path]` annotations that produce `/api/v1/openapi.json`
produce the tool list, one per operation, named `{tag}_{operation}`
(`diary_day`, `foods_search_external`, `weights_upsert`), with the input
schema composed from the path and query parameters and the request body,
`$ref`s resolved so a client sees the whole shape. Calling one builds an HTTP
request and dispatches it to the router in-process — the same extractor
authenticates the key, the same validators check the body, and the same
`{error, message}` comes back when something is refused. A new endpoint is a
new tool with nothing to install or update, and a tool cannot describe a
request the API no longer accepts, because there is no second copy to go
stale. Credentials, administration, photo uploads and image bytes are the
only operations left out.

Four tools are written by hand, for the sentences people actually say:

| Tool | What it does |
|---|---|
| `log_food(text, meal, date?, confirm?)` | "2 eggs", "150 g chicken breast", "a banana and 30 g oats". Finds each food, works out the grams (a count × the food's serving size, or the weight as written) and returns what it *would* log with the calories. Nothing is written until `confirm: true`, and a match that was only fuzzy is never written without the `food_id` from the confirmation. |
| `today(date?)` | The day's entries by meal, its totals, and every target already evaluated. |
| `progress(days?)` | Per-day totals and the average over logged days, plus the weight entries and trend for the same window. |
| `add_recipe_from_text(name, servings, ingredient_lines[])` | Resolves each line to a food; a line with no confident match becomes a free-text ingredient, and the response says which were resolved and which were not. |

A resource, `nom-inal://guide`, explains the domain — nutrients are per 100 g,
an entry is a food in grams or a recipe in servings, free-text ingredients
count for nothing and say so, a goal is a floor and a budget a ceiling — so a
model reads the rules rather than inferring them from field names.

Clients that speak Streamable HTTP connect directly:

```json
{ "type": "http", "url": "https://nom.example.com/mcp",
  "headers": { "Authorization": "Bearer nomi_…" } }
```

Clients that only speak stdio go through [`mcp-remote`](https://www.npmjs.com/package/mcp-remote):

```bash
npx -y mcp-remote https://nom.example.com/mcp --header "Authorization: Bearer nomi_…"
```

Settings → Integrations shows both, filled in with this instance's URL. The
transport is stateless — one JSON-RPC exchange per POST, each carrying its own
key — so there is no session to lose or to steal, and `curl` is enough to try
it (see the `== mcp` section of `scripts/smoke.sh`).

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

The workflow passes the commit and a UTC timestamp into the API image as build
args, and the binary reports them from `/api/v1/health` as `git_sha` and
`built_at` (and Settings shows them), so an instance can say what it is running
even on `edge` or `latest`. A source build that was not given them reports
`null` rather than guessing.

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
                      (energy.rs is the estimate, focus.rs the presets)
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
    pages/            one per route; settings/ has one per Settings section
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
