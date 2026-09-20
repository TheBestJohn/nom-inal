#!/usr/bin/env bash
#
# End-to-end exercise of every endpoint against a running API.
#
# Queries are runtime-checked rather than compile-time-checked, so a typo in a
# SQL string only shows up when it executes. This runs each one and asserts the
# arithmetic, which a `cargo build` cannot do.
#
# Usage: scripts/smoke.sh [base-url]        (default http://127.0.0.1:8080)

set -euo pipefail

BASE="${1:-http://127.0.0.1:8080}/api/v1"
EMAIL="smoke-$(date +%s)-$RANDOM@example.test"
PASSWORD="smoke-test-password"
failures=0

# Extract a value from a JSON body on stdin, e.g.  j "['id']"
j() { EXPR="$1" python3 -c 'import sys,json,os;print(eval("d"+os.environ["EXPR"],{"d":json.load(sys.stdin)}))'; }

pass() { printf '  \033[32m✓\033[0m %s\n' "$1"; }
fail() { printf '  \033[31m✗\033[0m %s\n' "$1"; failures=$((failures + 1)); }

expect() { # expect <label> <actual> <wanted>
  if [ "$2" = "$3" ]; then pass "$1 ($3)"; else fail "$1: got '$2', wanted '$3'"; fi
}

status() { # status <label> <wanted-code> <curl args...>
  local label="$1" want="$2"; shift 2
  local got; got=$(curl -s -o /dev/null -w '%{http_code}' "$@")
  expect "$label" "$got" "$want"
}

echo "== health"
HEALTH=$(curl -fsS "$BASE/health")
expect "database reachable" "$(echo "$HEALTH" | j "['database']")" "ok"
# Build provenance is stamped by the image build; a source build reports null
# rather than a guess, so only the keys are asserted here.
expect "the build says which commit it is, or admits it does not" \
  "$(echo "$HEALTH" | j " and 'git_sha' in d and 'built_at' in d")" "True"
expect "a stamp is either absent or a string" \
  "$(echo "$HEALTH" | j " and all(v is None or isinstance(v, str) for v in (d['git_sha'], d['built_at']))")" "True"

echo "== auth"
# Public, so the sign-in page can say whether "create an account" would work
# before anyone types a password.
expect "registration status is readable without a token" \
  "$(curl -fsS "$BASE/auth/registration" | j "['open']")" "True"
TOKEN=$(curl -fsS -X POST "$BASE/auth/register" -H 'content-type: application/json' \
  -d "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\",\"display_name\":\"Smoke\"}" | j "['access_token']")
AUTH="Authorization: Bearer $TOKEN"
expect "registered and signed in" "$(curl -fsS "$BASE/auth/me" -H "$AUTH" | j "['email']")" "$EMAIL"

# Verification needs people other than the author, so the run has three
# accounts: the first owns the instance, the other two are ordinary users.
EMAIL2="smoke2-$(date +%s)-$RANDOM@example.test"
EMAIL3="smoke3-$(date +%s)-$RANDOM@example.test"
REG2=$(curl -fsS -X POST "$BASE/auth/register" -H 'content-type: application/json' \
  -d "{\"email\":\"$EMAIL2\",\"password\":\"$PASSWORD\",\"display_name\":\"Smoke Two\"}")
REG3=$(curl -fsS -X POST "$BASE/auth/register" -H 'content-type: application/json' \
  -d "{\"email\":\"$EMAIL3\",\"password\":\"$PASSWORD\",\"display_name\":\"Smoke Three\"}")
AUTH2="Authorization: Bearer $(echo "$REG2" | j "['access_token']")"
AUTH3="Authorization: Bearer $(echo "$REG3" | j "['access_token']")"
UID2=$(echo "$REG2" | j "['user']['id']")

status "reject duplicate email"  409 -X POST "$BASE/auth/register" -H 'content-type: application/json' -d "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\",\"display_name\":\"Dup\"}"
status "reject short password"   400 -X POST "$BASE/auth/register" -H 'content-type: application/json' -d '{"email":"short@example.test","password":"short","display_name":"S"}'
status "reject wrong password"   401 -X POST "$BASE/auth/login"    -H 'content-type: application/json' -d "{\"email\":\"$EMAIL\",\"password\":\"definitely-wrong\"}"
status "reject missing token"    401 "$BASE/weights"

echo "== profile"
expect "update profile" \
  "$(curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"height_cm":180,"target_weight_kg":78}' | j "['height_cm']")" \
  "180.0"

# Which nutrients to show is a fact about your diet, not your device, so it
# lives on the account rather than in one browser's storage.
expect "readouts default to calories and macros" \
  "$(curl -fsS "$BASE/profile" -H "$AUTH" | j "['shown_nutrients']")" \
  "['calories_kcal', 'protein_g', 'carbs_g', 'fat_g']"
expect "and the home chart to calories alone" \
  "$(curl -fsS "$BASE/profile" -H "$AUTH" | j "['chart_nutrients']")" "['calories_kcal']"
# Percent of target is the default: it is the one basis on which nutrients with
# no shared scale can honestly share an axis.
expect "the chart indexes to targets by default" \
  "$(curl -fsS "$BASE/profile" -H "$AUTH" | j "['chart_mode']")" "percent"
expect "and the mode can be changed" \
  "$(curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"chart_mode":"actual"}' | j "['chart_mode']")" "actual"
expect "it survives a reload" "$(curl -fsS "$BASE/profile" -H "$AUTH" | j "['chart_mode']")" "actual"
status "an unknown mode is refused" 400 -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"chart_mode":"pie"}'
curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' -d '{"chart_mode":"percent"}' >/dev/null

PREFS=$(curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"shown_nutrients":["sodium_mg","calories_kcal","fiber_g"],"chart_nutrients":["protein_g","calories_kcal"]}')
# Stored in the application's canonical order, so two accounts that picked the
# same set in a different sequence read back the same.
expect "a chosen set is normalised"  "$(echo "$PREFS" | j "['shown_nutrients']")" "['calories_kcal', 'fiber_g', 'sodium_mg']"
expect "and so is the chart set"     "$(echo "$PREFS" | j "['chart_nutrients']")" "['calories_kcal', 'protein_g']"
expect "it survives a reload"        "$(curl -fsS "$BASE/profile" -H "$AUTH" | j "['shown_nutrients']")" "['calories_kcal', 'fiber_g', 'sodium_mg']"

expect "duplicates collapse" \
  "$(curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"shown_nutrients":["fat_g","fat_g","fat_g"]}' | j "['shown_nutrients']")" \
  "['fat_g']"
expect "showing nothing is a real choice, not 'unset'" \
  "$(curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"shown_nutrients":[]}' | j "['shown_nutrients']")" \
  "[]"
expect "omitting the field leaves it alone" \
  "$(curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"height_cm":181}' | j "['shown_nutrients']")" \
  "[]"
status "an unknown nutrient is refused" 400 -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"shown_nutrients":["vitamin_q"]}'

# Put it back so the rest of the run sees the usual readout.
curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"shown_nutrients":["calories_kcal","protein_g","carbs_g","fat_g"],"chart_nutrients":["calories_kcal"]}' >/dev/null

echo "== units"
# Storage is metric; units are how the client shows and reads body figures.
# The API never sees a pound: the preference is stored, the numbers are not
# converted, and food amounts are grams whatever it says.
expect "metric until chosen"   "$(curl -fsS "$BASE/profile" -H "$AUTH" | j "['units']")" "metric"
expect "imperial can be chosen" \
  "$(curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' -d '{"units":"imperial"}' | j "['units']")" "imperial"
expect "it survives a reload"  "$(curl -fsS "$BASE/profile" -H "$AUTH" | j "['units']")" "imperial"
expect "and the stored height is still centimetres" \
  "$(curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' -d '{"height_cm":181}' | j " and (d['height_cm'], d['units'])")" "(181.0, 'imperial')"
status "an unknown unit system is refused" 400 -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' -d '{"units":"stone"}'
curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' -d '{"units":"metric"}' >/dev/null

echo "== goals and budgets"
# A budget is a ceiling, a goal is a floor. The same arithmetic, read in
# opposite directions -- which is the whole point of storing the direction.
curl -fsS -X PUT "$BASE/targets" -H "$AUTH" -H 'content-type: application/json' -d '{"targets":[
  {"nutrient":"calories_kcal","amount":2200,"kind":"budget"},
  {"nutrient":"protein_g","amount":160,"kind":"goal"},
  {"nutrient":"fiber_g","amount":30}]}' >/dev/null
expect "targets stored"              "$(curl -fsS "$BASE/targets" -H "$AUTH" | j ".__len__()")" "3"
expect "kind defaults per nutrient"  "$(curl -fsS "$BASE/targets/fiber_g" -H "$AUTH" | j "['kind']")" "goal"
expect "calories default to budget"  "$(curl -fsS "$BASE/targets/calories_kcal" -H "$AUTH" | j "['kind']")" "budget"

status "reject a duplicate nutrient" 400 -X PUT "$BASE/targets" -H "$AUTH" -H 'content-type: application/json' -d '{"targets":[{"nutrient":"protein_g","amount":1},{"nutrient":"protein_g","amount":2}]}'
status "reject a negative amount"    400 -X PUT "$BASE/targets" -H "$AUTH" -H 'content-type: application/json' -d '{"targets":[{"nutrient":"protein_g","amount":-5}]}'
status "reject an unknown nutrient"  400 -X PUT "$BASE/targets" -H "$AUTH" -H 'content-type: application/json' -d '{"targets":[{"nutrient":"vitamin_q","amount":5}]}'
status "reject an unknown kind"      400 -X PUT "$BASE/targets" -H "$AUTH" -H 'content-type: application/json' -d '{"targets":[{"nutrient":"protein_g","amount":5,"kind":"wish"}]}'

# Every error, including a body the server could not deserialize, uses the
# same JSON shape. Axum's own rejection would be plain text with a 422.
expect "malformed bodies use the standard error shape" \
  "$(curl -s -X POST "$BASE/weights" -H "$AUTH" -H 'content-type: application/json' -d '{"weight_kg":' | j "['error']")" \
  "bad_request"

echo "== weights"
curl -fsS -X POST "$BASE/weights" -H "$AUTH" -H 'content-type: application/json' -d '{"recorded_on":"2026-01-01","weight_kg":84.2}' >/dev/null
curl -fsS -X POST "$BASE/weights" -H "$AUTH" -H 'content-type: application/json' -d '{"recorded_on":"2026-01-01","weight_kg":84.0}' >/dev/null
curl -fsS -X POST "$BASE/weights" -H "$AUTH" -H 'content-type: application/json' -d '{"recorded_on":"2026-01-15","weight_kg":82.5,"body_fat_pct":18.4}' >/dev/null
expect "same date upserts, not duplicates" "$(curl -fsS "$BASE/weights" -H "$AUTH" | j ".__len__()")" "2"
expect "change over the window"            "$(curl -fsS "$BASE/weights/stats" -H "$AUTH" | j "['change_kg']")" "-1.5"
WID=$(curl -fsS "$BASE/weights" -H "$AUTH" | j "[0]['id']")
expect "patch an entry" "$(curl -fsS -X PATCH "$BASE/weights/$WID" -H "$AUTH" -H 'content-type: application/json' -d '{"weight_kg":82.7}' | j "['weight_kg']")" "82.7"
status "delete an entry" 204 -X DELETE "$BASE/weights/$WID" -H "$AUTH"
status "deleted entry is gone" 404 "$BASE/weights/$WID" -H "$AUTH"

echo "== foods"
OATS=$(curl -fsS -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"name":"Rolled oats","calories_kcal":379,"protein_g":13.2,"carbs_g":67.7,"fat_g":6.5,"fiber_g":10.1,"serving_size_g":40}' | j "['id']")
MILK=$(curl -fsS -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"name":"Whole milk","calories_kcal":61,"protein_g":3.2,"carbs_g":4.8,"fat_g":3.3,"serving_size_g":244}' | j "['id']")
# 379 kcal/100g x 40 g = 151.6
expect "per-serving scaling" "$(curl -fsS "$BASE/foods/$OATS" -H "$AUTH" | j "['per_serving']['calories_kcal']")" "151.6"
expect "search by name"      "$(curl -fsS "$BASE/foods?q=oat" -H "$AUTH" | j " and ('Rolled oats' in [f['name'] for f in d])")" "True"

BAN=$(curl -fsS -X POST "$BASE/foods/import" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"source":"usda","source_id":"173944","name":"Bananas, raw","calories_kcal":89,"protein_g":1.09,"carbs_g":22.84,"fat_g":0.33,"serving_size_g":118}' | j "['id']")
BAN2=$(curl -fsS -X POST "$BASE/foods/import" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"source":"usda","source_id":"173944","name":"Bananas, raw","calories_kcal":89,"protein_g":1.09,"carbs_g":22.84,"fat_g":0.33,"serving_size_g":118}' | j "['id']")
expect "re-import is idempotent" "$BAN2" "$BAN"
status "reject unknown import source" 400 -X POST "$BASE/foods/import" -H "$AUTH" -H 'content-type: application/json' -d '{"source":"nope","source_id":"1","name":"x","calories_kcal":0,"protein_g":0,"carbs_g":0,"fat_g":0,"serving_size_g":100}'
# An imported row is a copy of what a provider published, not scripture: the
# person holding the package is often right and the database wrong. Correcting
# one is allowed, and the correction is what stops the next refresh from
# quietly putting the old value back.
expect "an imported food can be corrected" \
  "$(curl -fsS -X PUT "$BASE/foods/$BAN" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"name":"Bananas, raw (corrected)","calories_kcal":89,"protein_g":1.09,"carbs_g":22.84,"fat_g":0.33,"serving_size_g":118,"edit_summary":"matches the loose-fruit entry"}' | j "['revision']")" \
  "2"
expect "and a re-import will not undo the correction" \
  "$(curl -fsS -X POST "$BASE/foods/import" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"source":"usda","source_id":"173944","name":"Bananas, raw","calories_kcal":89,"protein_g":1.09,"carbs_g":22.84,"fat_g":0.33,"serving_size_g":118}' | j "['name']")" \
  "Bananas, raw (corrected)"

echo "== household portions"
# Nobody weighs a cup of oats; they measure a cup. A portion is the answer to
# "how many grams is that for this food", kept per food, offered beside grams,
# and never what gets stored: the entry is grams whatever route led to them.
CUP=$(curl -fsS -X POST "$BASE/foods/$OATS/portions" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"label":"1 cup","grams":80}')
expect "a portion is added"               "$(echo "$CUP" | j " and [(p['label'], p['grams'], p['source']) for p in d['portions']]")" "[('1 cup', 80.0, 'user')]"
expect "and does not count as an edit"    "$(echo "$CUP" | j "['revision']")" "1"
expect "the list carries it too"          "$(curl -fsS "$BASE/foods?q=Rolled%20oats" -H "$AUTH" | j " and [p['label'] for f in d if f['id']=='$OATS' for p in f['portions']]")" "['1 cup']"
# The picker reads the search stream, so a portion has to arrive there as
# well. A run-scoped name: the table is global and persistent, and the search
# collapses identical foods to the oldest, which on a reused database is a
# previous run's copy with no portions on it.
PORTION_NAME="Portion probe $(date +%s)-$RANDOM"
PORTION_FOOD=$(curl -fsS -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"name\":\"$PORTION_NAME\",\"calories_kcal\":50,\"protein_g\":1,\"carbs_g\":2,\"fat_g\":3}" | j "['id']")
curl -fsS -X POST "$BASE/foods/$PORTION_FOOD/portions" -H "$AUTH" -H 'content-type: application/json' -d '{"label":"1 mug","grams":300}' >/dev/null
expect "and so does the search stream" \
  "$(curl -fsSN "$BASE/search/foods?q=$(python3 -c 'import sys,urllib.parse;print(urllib.parse.quote(sys.argv[1]))' "$PORTION_NAME")&limit=5" -H "$AUTH" \
    | grep '^data:' | sed 's/^data: *//' \
    | NAME="$PORTION_NAME" python3 -c 'import sys,json,os
hits=[f for line in sys.stdin for f in json.loads(line).get("results",[]) if f["name"]==os.environ["NAME"]]
print([p["label"] for f in hits for p in f["portions"]])')" "['1 mug']"
status "the same label twice is refused"  409 -X POST "$BASE/foods/$OATS/portions" -H "$AUTH" -H 'content-type: application/json' -d '{"label":"1 cup","grams":90}'
status "a blank label is refused"         400 -X POST "$BASE/foods/$OATS/portions" -H "$AUTH" -H 'content-type: application/json' -d '{"label":"   ","grams":90}'
status "a weightless portion is refused"  400 -X POST "$BASE/foods/$OATS/portions" -H "$AUTH" -H 'content-type: application/json' -d '{"label":"1 pinch","grams":0}'
status "an unknown food has no portions"  404 -X POST "$BASE/foods/00000000-0000-0000-0000-000000000000/portions" -H "$AUTH" -H 'content-type: application/json' -d '{"label":"1 cup","grams":80}'
CUP_ID=$(echo "$CUP" | j "['portions'][0]['id']")
expect "removing it returns the food without it" \
  "$(curl -fsS -X DELETE "$BASE/foods/$OATS/portions/$CUP_ID" -H "$AUTH" | j "['portions']")" "[]"
status "removing it twice is a 404"       404 -X DELETE "$BASE/foods/$OATS/portions/$CUP_ID" -H "$AUTH"

# A provider's portions arrive with the import. USDA's own detail record is
# not reachable here, so the suite sends them in the body, which is the same
# path the import takes once it has fetched them. $BAN was corrected above and
# sits at revision 2: portions refresh regardless, because they are outside
# the revision model and there is no correction to undo.
#
# The banana is the same global row on every run, so a reused database may
# still carry the portions this section adds; start from none.
for PID in $(curl -fsS "$BASE/foods/$BAN" -H "$AUTH" | j " and ' '.join(p['id'] for p in d['portions'])"); do
  curl -fsS -X DELETE "$BASE/foods/$BAN/portions/$PID" -H "$AUTH" >/dev/null
done
IMPORTED=$(curl -fsS -X POST "$BASE/foods/import" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"source":"usda","source_id":"173944","name":"Bananas, raw","calories_kcal":89,"protein_g":1.09,"carbs_g":22.84,"fat_g":0.33,"serving_size_g":118,"portions":[{"label":"1 medium","grams":118},{"label":"1 cup, sliced","grams":150}]}')
expect "an import carries the provider's portions" \
  "$(echo "$IMPORTED" | j " and [(p['label'], p['grams'], p['source']) for p in d['portions']]")" \
  "[('1 medium', 118.0, 'usda'), ('1 cup, sliced', 150.0, 'usda')]"
expect "on the corrected food, uncorrected" "$(echo "$IMPORTED" | j "['name']")" "Bananas, raw (corrected)"
curl -fsS -X POST "$BASE/foods/$BAN/portions" -H "$AUTH" -H 'content-type: application/json' -d '{"label":"1 large","grams":136}' >/dev/null
REIMPORTED=$(curl -fsS -X POST "$BASE/foods/import" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"source":"usda","source_id":"173944","name":"Bananas, raw","calories_kcal":89,"protein_g":1.09,"carbs_g":22.84,"fat_g":0.33,"serving_size_g":118,"portions":[{"label":"1 medium","grams":120},{"label":"1 large","grams":999}]}')
expect "a re-import refreshes the provider's figure" \
  "$(echo "$REIMPORTED" | j " and [p['grams'] for p in d['portions'] if p['label']=='1 medium']")" "[120.0]"
expect "keeps a provider portion it no longer sends" \
  "$(echo "$REIMPORTED" | j " and [p['grams'] for p in d['portions'] if p['label']=='1 cup, sliced']")" "[150.0]"
expect "and never overwrites what a person typed" \
  "$(echo "$REIMPORTED" | j " and [(p['grams'], p['source']) for p in d['portions'] if p['label']=='1 large']")" "[(136.0, 'user')]"
expect "an import that brings none leaves them alone" \
  "$(curl -fsS -X POST "$BASE/foods/import" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"source":"usda","source_id":"173944","name":"Bananas, raw","calories_kcal":89,"protein_g":1.09,"carbs_g":22.84,"fat_g":0.33,"serving_size_g":118}' | j " and len(d['portions'])")" "3"

echo "== recipes"
# 100g oats (379) + 300g milk (183) + 118g banana (105.02) = 667.02 over 2 servings
REC=$(curl -fsS -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Oatmeal bowl\",\"servings\":2,
  \"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":100},{\"food_id\":\"$MILK\",\"quantity_g\":300},{\"food_id\":\"$BAN\",\"quantity_g\":118}]}")
RID=$(echo "$REC" | j "['id']")
expect "recipe total"       "$(echo "$REC" | j "['total']['calories_kcal']")"       "667.02"
expect "recipe per serving" "$(echo "$REC" | j "['per_serving']['calories_kcal']")" "333.51"
expect "list view agrees"   "$(curl -fsS "$BASE/recipes" -H "$AUTH" | j "[0]['per_serving']['calories_kcal']")" "333.51"
status "reject unknown ingredient" 400 -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"name":"x","servings":1,"items":[{"food_id":"00000000-0000-0000-0000-000000000000","quantity_g":10}]}'
status "reject empty ingredient list" 400 -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' -d '{"name":"x","servings":1,"items":[]}'

echo "== recipes inside recipes"
# A sub-recipe is linked, not copied: the parent asks the child what it says
# today. Numbers chosen so the arithmetic is checkable by eye — the sauce is
# 200 kcal over 4 servings, so one serving is 50.
SAUCE=$(curl -fsS -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Smoke sauce\",\"servings\":4,
  \"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":100}]}" | j "['id']")
# 379 kcal/100g x 100 g = 379, over 4 servings = 94.75 per serving.
expect "the sub-recipe on its own"  "$(curl -fsS "$BASE/recipes/$SAUCE" -H "$AUTH" | j "['per_serving']['calories_kcal']")" "94.75"

DISH=$(curl -fsS -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Smoke dish\",\"servings\":1,
  \"items\":[{\"food_id\":\"$MILK\",\"quantity_g\":200},
            {\"sub_recipe_id\":\"$SAUCE\",\"servings\":2}]}")
DISH_ID=$(echo "$DISH" | j "['id']")
# 61 kcal/100g x 200 g = 122, plus 2 x 94.75 = 189.5 -> 311.5
expect "a recipe can be an ingredient"   "$(echo "$DISH" | j "['total']['calories_kcal']")" "311.5"
expect "the ingredient carries its link" "$(echo "$DISH" | j " and [i['sub_recipe_id'] for i in d['items'] if i['sub_recipe_id']][0]")" "$SAUCE"
expect "and is named after the recipe"   "$(echo "$DISH" | j " and [i['name'] for i in d['items'] if i['sub_recipe_id']][0]")" "Smoke sauce"
expect "its weight is the servings taken" "$(echo "$DISH" | j " and [i['weight_g'] for i in d['items'] if i['sub_recipe_id']][0]")" "50.0"
expect "ingredients are not copied in"   "$(echo "$DISH" | j " and len(d['items'])")" "2"

# The whole point of linking: correcting the sauce corrects everything built on it.
curl -fsS -X PUT "$BASE/recipes/$SAUCE" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Smoke sauce\",\"servings\":4,
  \"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":200}]}" >/dev/null
# 758 over 4 = 189.5 per serving; 122 + 2 x 189.5 = 501
expect "editing the sub-recipe flows through" "$(curl -fsS "$BASE/recipes/$DISH_ID" -H "$AUTH" | j "['total']['calories_kcal']")" "501.0"
expect "the list view agrees"                 "$(curl -fsS "$BASE/recipes?q=Smoke%20dish" -H "$AUTH" | j "[0]['per_serving']['calories_kcal']")" "501.0"

# Logging the parent must use the same figure the recipe page shows.
curl -fsS -X POST "$BASE/diary" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"logged_on\":\"2026-02-02\",\"meal\":\"dinner\",\"recipe_id\":\"$DISH_ID\",\"recipe_servings\":1}" >/dev/null
expect "and the diary agrees with both"       "$(curl -fsS "$BASE/diary/day?date=2026-02-02" -H "$AUTH" | j "['total']['calories_kcal']")" "501.0"

status "a recipe cannot contain itself" 400 -X PUT "$BASE/recipes/$DISH_ID" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"name\":\"Smoke dish\",\"servings\":1,\"items\":[{\"sub_recipe_id\":\"$DISH_ID\",\"servings\":1}]}"
status "nor a cycle through another"    400 -X PUT "$BASE/recipes/$SAUCE" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"name\":\"Smoke sauce\",\"servings\":4,\"items\":[{\"sub_recipe_id\":\"$DISH_ID\",\"servings\":1}]}"
status "an ingredient is not both"      400 -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"name\":\"Both\",\"servings\":1,\"items\":[{\"food_id\":\"$OATS\",\"sub_recipe_id\":\"$SAUCE\",\"quantity_g\":10}]}"
status "nor neither"                    400 -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"name":"Neither","servings":1,"items":[{"quantity_g":10}]}'
status "a recipe wants servings, not grams" 400 -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"name\":\"Wrong unit\",\"servings\":1,\"items\":[{\"sub_recipe_id\":\"$SAUCE\",\"quantity_g\":10}]}"
status "an unknown sub-recipe is refused" 400 -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"name":"Ghost","servings":1,"items":[{"sub_recipe_id":"00000000-0000-0000-0000-000000000000","servings":1}]}'
status "a recipe in use cannot be deleted" 400 -X DELETE "$BASE/recipes/$SAUCE" -H "$AUTH"

# Some ingredients are just words. They carry no nutrition, which is the point --
# but a recipe whose macros silently exclude them would be quietly wrong, so the
# count of them travels with the numbers.
LOOSE=$(curl -fsS -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Loose ends\",\"servings\":1,
  \"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":100},
            {\"label\":\"salt and pepper to taste\"},
            {\"label\":\"a squeeze of lemon\"}]}")
LOOSE_ID=$(echo "$LOOSE" | j "['id']")
expect "a free-text ingredient is kept"      "$(echo "$LOOSE" | j " and [i['label'] for i in d['items'] if i['label']][0]")" "salt and pepper to taste"
expect "and contributes nothing"             "$(echo "$LOOSE" | j "['total']['calories_kcal']")" "379.0"
expect "and weighs nothing"                  "$(echo "$LOOSE" | j " and [i['weight_g'] for i in d['items'] if i['label']][0]")" "0.0"
expect "but the recipe says how many"        "$(echo "$LOOSE" | j "['untracked_count']")" "2"
expect "the list view says so too"           "$(curl -fsS "$BASE/recipes?q=Loose%20ends" -H "$AUTH" | j "[0]['untracked_count']")" "2"

# A sub-recipe's loose ends leave the parent just as incomplete.
WRAP=$(curl -fsS -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Wrapper\",\"servings\":1,
  \"items\":[{\"sub_recipe_id\":\"$LOOSE_ID\",\"servings\":1},{\"label\":\"olive oil\"}]}")
expect "untracked is counted through nesting" "$(echo "$WRAP" | j "['untracked_count']")" "3"

expect "a recipe can be nothing but words" \
  "$(curl -fsS -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"name":"Improvised","servings":1,"items":[{"label":"whatever is in the fridge"}]}' | j "['total']['calories_kcal']")" \
  "0.0"

status "a label carries no quantity"  400 -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"name":"Bad","servings":1,"items":[{"label":"salt","quantity_g":5}]}'
status "a label is not also a food"   400 -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"name\":\"Bad\",\"servings\":1,\"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":10,\"label\":\"salt\"}]}"
status "a blank label is not one"     400 -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"name":"Bad","servings":1,"items":[{"label":"   "}]}'

echo "== diary"
curl -fsS -X POST "$BASE/diary" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"logged_on\":\"2026-01-15\",\"meal\":\"breakfast\",\"recipe_id\":\"$RID\",\"recipe_servings\":1}" >/dev/null
curl -fsS -X POST "$BASE/diary" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"logged_on\":\"2026-01-15\",\"meal\":\"lunch\",\"food_id\":\"$BAN\",\"quantity_g\":118}" >/dev/null

DAY=$(curl -fsS "$BASE/diary/day?date=2026-01-15" -H "$AUTH")
expect "day total (recipe + food)" "$(echo "$DAY" | j "['total']['calories_kcal']")" "438.53"
expect "grouped into four meals"   "$(echo "$DAY" | j " and len(d['meals'])")"        "4"

# 438.53 kcal against a 2200 budget: under, with 1761.47 left.
expect "calorie budget reports what is left" \
  "$(echo "$DAY" | j " and [t for t in d['targets'] if t['nutrient']=='calories_kcal'][0]['remaining']")" "1761.47"
expect "calorie budget is under"   "$(echo "$DAY" | j " and [t for t in d['targets'] if t['nutrient']=='calories_kcal'][0]['status']")" "under"
# 13.33 g protein (12.04 from the recipe serving + 1.29 from the banana)
# against a 160 g goal: short, 146.67 g still needed.
expect "protein goal reports what is still needed" \
  "$(echo "$DAY" | j " and [t for t in d['targets'] if t['nutrient']=='protein_g'][0]['remaining']")" "146.67"
expect "protein goal is short"     "$(echo "$DAY" | j " and [t for t in d['targets'] if t['nutrient']=='protein_g'][0]['status']")" "short"

# The distinction that matters: blow a budget and it is over; pass a goal and
# it is met, not flagged.
curl -fsS -X PUT "$BASE/targets" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"targets":[{"nutrient":"calories_kcal","amount":300,"kind":"budget"},{"nutrient":"protein_g","amount":5,"kind":"goal"}]}' >/dev/null
DAY=$(curl -fsS "$BASE/diary/day?date=2026-01-15" -H "$AUTH")
expect "an exceeded budget is over" "$(echo "$DAY" | j " and [t for t in d['targets'] if t['nutrient']=='calories_kcal'][0]['status']")" "over"
expect "an exceeded goal is met"    "$(echo "$DAY" | j " and [t for t in d['targets'] if t['nutrient']=='protein_g'][0]['status']")"     "met"

# Targets are standing settings: set once, they apply to every day, including
# days in the past and days with nothing logged. Nothing is ever set up daily.
expect "targets apply to an untouched past day" \
  "$(curl -fsS "$BASE/diary/day?date=2025-06-30" -H "$AUTH" | j " and [t['nutrient'] for t in d['targets']]")" \
  "['calories_kcal', 'protein_g']"
expect "and to a day with no entries at all" \
  "$(curl -fsS "$BASE/diary/day?date=2025-06-30" -H "$AUTH" | j " and [t['status'] for t in d['targets']]")" \
  "['under', 'short']"

status "clear one target"        204 -X DELETE "$BASE/targets/protein_g" -H "$AUTH"
status "clearing it twice 404s"  404 -X DELETE "$BASE/targets/protein_g" -H "$AUTH"

SUM=$(curl -fsS "$BASE/diary/summary?from=2026-01-01&to=2026-01-31" -H "$AUTH")
expect "summary counts logged days only" "$(echo "$SUM" | j "['logged_day_count']")" "1"
expect "summary day total matches"       "$(echo "$SUM" | j "['days'][0]['total']['calories_kcal']")" "438.53"

# Picked by what it is, not by position: an open-ended range put whichever
# entry a later section happened to log at the front of this list.
DID=$(curl -fsS "$BASE/diary?from=2026-01-15&to=2026-01-15" -H "$AUTH" \
  | j " and [e['id'] for e in d if e.get('recipe_id')][0]")
expect "patch recipe servings rescales" \
  "$(curl -fsS -X PATCH "$BASE/diary/$DID" -H "$AUTH" -H 'content-type: application/json' -d '{"recipe_servings":2}' | j "['nutrients']['calories_kcal']")" \
  "667.02"
status "reject both food_id and recipe_id" 400 -X POST "$BASE/diary" -H "$AUTH" -H 'content-type: application/json' -d "{\"food_id\":\"$BAN\",\"recipe_id\":\"$RID\",\"quantity_g\":10}"
status "reject neither food_id nor recipe_id" 400 -X POST "$BASE/diary" -H "$AUTH" -H 'content-type: application/json' -d '{"quantity_g":10}'

echo "== copying a day or a meal"
# 2026-01-15 holds the oatmeal bowl at 2 servings (667.02) for breakfast and
# 118 g of banana (105.02) for lunch. A copy is the same things in the same
# amounts on another date, as fresh entries of their own.
COPIED=$(curl -fsS -X POST "$BASE/diary/copy" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"from_date":"2026-01-15","to_date":"2026-03-01"}')
expect "the whole day copies"           "$(echo "$COPIED" | j "['copied']")" "2"
expect "with its meals"                 "$(echo "$COPIED" | j " and sorted(e['meal'] for e in d['entries'])")" "['breakfast', 'lunch']"
expect "and its amounts"                "$(echo "$COPIED" | j " and sorted([(e['quantity_g'], e['recipe_servings']) for e in d['entries']], key=str)")" "[(118.0, None), (None, 2.0)]"
expect "onto the target date"           "$(echo "$COPIED" | j " and {e['logged_on'] for e in d['entries']}")" "{'2026-03-01'}"
expect "so the target day now totals the same" \
  "$(curl -fsS "$BASE/diary/day?date=2026-03-01" -H "$AUTH" | j "['total']['calories_kcal']")" "772.04"
expect "and the source is untouched" \
  "$(curl -fsS "$BASE/diary/day?date=2026-01-15" -H "$AUTH" | j "['total']['calories_kcal']")" "772.04"
MEAL_COPY=$(curl -fsS -X POST "$BASE/diary/copy" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"from_date":"2026-01-15","to_date":"2026-03-02","meal":"Lunch"}')
expect "one meal copies alone"          "$(echo "$MEAL_COPY" | j " and (d['copied'], d['meal'], d['entries'][0]['name'])")" "(1, 'lunch', 'Bananas, raw (corrected)')"
expect "an empty source copies nothing, and says so" \
  "$(curl -fsS -X POST "$BASE/diary/copy" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"from_date":"2025-06-30","to_date":"2026-03-02"}' | j "['copied']")" "0"
status "a day cannot be copied onto itself" 400 -X POST "$BASE/diary/copy" -H "$AUTH" -H 'content-type: application/json' -d '{"from_date":"2026-01-15","to_date":"2026-01-15"}'
status "another account's diary is not a source" 200 -X POST "$BASE/diary/copy" -H "$AUTH2" -H 'content-type: application/json' -d '{"from_date":"2026-01-15","to_date":"2026-03-01"}'
expect "it just has nothing to copy" \
  "$(curl -fsS -X POST "$BASE/diary/copy" -H "$AUTH2" -H 'content-type: application/json' -d '{"from_date":"2026-01-15","to_date":"2026-03-01"}' | j "['copied']")" "0"

echo "== a meal as a recipe"
# What was logged becomes the ingredient list as it was logged: a food in its
# grams, a logged recipe as a sub-recipe in its servings. The sub-recipe is
# linked, not unpacked, for the same reason any sub-recipe is.
curl -fsS -X POST "$BASE/diary" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"logged_on\":\"2026-03-03\",\"meal\":\"dinner\",\"food_id\":\"$BAN\",\"quantity_g\":118}" >/dev/null
curl -fsS -X POST "$BASE/diary" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"logged_on\":\"2026-03-03\",\"meal\":\"dinner\",\"recipe_id\":\"$RID\",\"recipe_servings\":1}" >/dev/null
SAVED=$(curl -fsS -X POST "$BASE/recipes/from-meal" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"date":"2026-03-03","meal":"dinner","name":"Tuesday dinner"}')
SAVED_ID=$(echo "$SAVED" | j "['id']")
expect "the meal becomes a recipe"          "$(echo "$SAVED" | j " and (d['name'], d['servings'], len(d['items']))")" "('Tuesday dinner', 1.0, 2)"
# 105.02 for the banana plus one serving of the bowl at 333.51.
expect "with the meal's own total"          "$(echo "$SAVED" | j "['total']['calories_kcal']")" "438.53"
expect "the food is an ingredient in grams" "$(echo "$SAVED" | j " and [(i['food_id'], i['quantity_g']) for i in d['items'] if i['food_id']]")" "[('$BAN', 118.0)]"
expect "the logged recipe stays a recipe"   "$(echo "$SAVED" | j " and [(i['sub_recipe_id'], i['servings']) for i in d['items'] if i['sub_recipe_id']]")" "[('$RID', 1.0)]"
expect "and reads back like any recipe"     "$(curl -fsS "$BASE/recipes/$SAVED_ID" -H "$AUTH" | j "['total']['calories_kcal']")" "438.53"
status "an empty meal makes no recipe"      400 -X POST "$BASE/recipes/from-meal" -H "$AUTH" -H 'content-type: application/json' -d '{"date":"2025-06-30","meal":"dinner","name":"Nothing"}'
status "and a name is required"             400 -X POST "$BASE/recipes/from-meal" -H "$AUTH" -H 'content-type: application/json' -d '{"date":"2026-03-03","meal":"dinner","name":""}'

echo "== recent foods"
# What the picker shows before anyone types: each thing logged, once, most
# recent first, most often first among things from the same day, with the
# amount used last time. The banana is on 2026-03-03 with four entries in all;
# the bowl is there too with three.
RECENT=$(curl -fsS "$BASE/foods/recent" -H "$AUTH")
expect "the most recent day leads, most-logged first" \
  "$(echo "$RECENT" | j " and (d[0]['food']['id'], d[0]['recipe'], d[0]['times_logged'], d[0]['last_logged_on'])")" "('$BAN', None, 4, '2026-03-03')"
expect "with the grams used last time"      "$(echo "$RECENT" | j "[0]['last_quantity_g']")" "118.0"
expect "a recipe is in the same list"       "$(echo "$RECENT" | j " and (d[1]['food'], d[1]['recipe']['id'], d[1]['times_logged'])")" "(None, '$RID', 3)"
expect "with the servings used last time"   "$(echo "$RECENT" | j "[1]['last_recipe_servings']")" "1.0"
expect "and one serving's figures"          "$(echo "$RECENT" | j "[1]['recipe']['per_serving']['calories_kcal']")" "333.51"
expect "a food arrives with its portions"   "$(echo "$RECENT" | j " and sorted(p['label'] for p in d[0]['food']['portions'])")" "['1 cup, sliced', '1 large', '1 medium']"
expect "each item appears once"             "$(echo "$RECENT" | j " and len(d) == len({(i['food'] or {}).get('id') or i['recipe']['id'] for i in d})")" "True"
expect "limit is honoured"                  "$(curl -fsS "$BASE/foods/recent?limit=1" -H "$AUTH" | j ".__len__()")" "1"
expect "an account with no diary has none"  "$(curl -fsS "$BASE/foods/recent" -H "$AUTH3" | j ".__len__()")" "0"

echo "== tracking focus"
# Why you are tracking. NULL until the welcome flow has asked, which is what
# sends an account through it once; 'custom' is the answer "none of these".
expect "focus starts unanswered" "$(curl -fsS "$BASE/profile" -H "$AUTH" | j "['tracking_focus']")" "None"
# Every wire name is also the column's CHECK list, so each one is written and
# read back: a serde rename that drifted from the constraint would 500 here.
for F in general weight_loss muscle_gain keto diabetes blood_pressure heart_health custom; do
  expect "focus '$F' round-trips" \
    "$(curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' \
        -d "{\"tracking_focus\":\"$F\"}" | j "['tracking_focus']")" "$F"
done
status "an unknown focus is refused" 400 -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' -d '{"tracking_focus":"paleo"}'
status "and on the preview"          400 "$BASE/profile/focus/preview?focus=paleo" -H "$AUTH"
expect "eight focuses are offered, with a sentence each" \
  "$(curl -fsS "$BASE/profile/focus" -H "$AUTH" | j " and [len(d), all(o['summary'] for o in d)]")" "[8, True]"

# The estimate lives on the server now. Nothing is guessed: with no birth date
# it says what it needs, and every target that depends on it is listed without
# an amount rather than dropped.
expect "the suggestion names what it is missing" \
  "$(curl -fsS "$BASE/targets/suggestion" -H "$AUTH" | j "['missing']")" "['birth_date']"
expect "and prices only what it can" \
  "$(curl -fsS "$BASE/profile/focus/preview?focus=keto" -H "$AUTH" \
    | j " and [(t['nutrient'], t['amount'] is not None) for t in d['targets']]")" \
  "[('calories_kcal', False), ('net_carbs_g', True), ('protein_g', True), ('fat_g', False)]"

curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"sex":"male","birth_date":"1990-06-15","activity_level":"moderate","goal":"maintain"}' >/dev/null
SUG=$(curl -fsS "$BASE/targets/suggestion" -H "$AUTH")
# The latest weigh-in (84.0 kg, from the weights section) is the body weight,
# not the 78 kg target: the two used to disagree between the settings page
# and the presets.
expect "the estimate reads the scale, not the target" "$(echo "$SUG" | j "['estimate']['weight_kg']")" "84.0"
expect "Mifflin-St Jeor, scaled by activity" \
  "$(echo "$SUG" | j " and round(d['estimate']['bmr_kcal'] * 1.55) == d['estimate']['tdee_kcal']")" "True"
expect "the calorie budget is the estimate to the nearest ten" \
  "$(echo "$SUG" | j " and [t for t in d['targets'] if t['nutrient']=='calories_kcal'][0]['amount'] == round(d['estimate']['calories_kcal'] / 10) * 10")" "True"
expect "protein by body weight (1.6 g/kg when maintaining)" \
  "$(echo "$SUG" | j " and [t for t in d['targets'] if t['nutrient']=='protein_g'][0]['amount']")" "134.0"

# A preset is applied, not enforced: it writes ordinary targets and display
# preferences, all in one transaction, and every one of them stays editable.
KETO=$(curl -fsS "$BASE/profile/focus/preview?focus=keto" -H "$AUTH")
expect "keto budgets net carbs" \
  "$(echo "$KETO" | j " and [(t['nutrient'], t['kind'], t['amount']) for t in d['targets'] if t['nutrient'] in ('net_carbs_g','protein_g')]")" \
  "[('net_carbs_g', 'budget', 25.0), ('protein_g', 'goal', 126.0)]"
expect "and fat is what remains, as a goal" \
  "$(echo "$KETO" | j " and (lambda t: (t['fat_g']['kind'], t['fat_g']['amount'] == round((t['calories_kcal']['amount'] - 25*4 - 126*4) / 9)))({x['nutrient']: x for x in d['targets']})")" \
  "('goal', True)"
expect "every target says why" "$(echo "$KETO" | j " and all(t['rationale'] for t in d['targets'])")" "True"

APPLIED=$(curl -fsS -X POST "$BASE/profile/focus" -H "$AUTH" -H 'content-type: application/json' -d '{"focus":"keto","apply":true}')
expect "applying sets the focus"     "$(echo "$APPLIED" | j "['profile']['tracking_focus']")" "keto"
expect "and the readouts"            "$(echo "$APPLIED" | j "['profile']['shown_nutrients']")" "['calories_kcal', 'protein_g', 'net_carbs_g', 'fat_g']"
expect "and the chart"               "$(echo "$APPLIED" | j "['profile']['chart_nutrients']")" "['net_carbs_g']"
expect "and writes the targets"      "$(curl -fsS "$BASE/targets" -H "$AUTH" | j " and [(t['nutrient'], t['amount']) for t in d]")" \
  "[('calories_kcal', $(echo "$KETO" | j " and [t for t in d['targets'] if t['nutrient']=='calories_kcal'][0]['amount']")), ('protein_g', 126.0), ('net_carbs_g', 25.0), ('fat_g', $(echo "$KETO" | j " and [t for t in d['targets'] if t['nutrient']=='fat_g'][0]['amount']"))]"
expect "a written target reads back like any other" "$(curl -fsS "$BASE/targets/net_carbs_g" -H "$AUTH" | j "['kind']")" "budget"
expect "weight loss implies a cut" \
  "$(curl -fsS "$BASE/profile/focus/preview?focus=weight_loss" -H "$AUTH" | j " and (d['goal'], d['estimate']['goal_adjustment_kcal'])")" "('cut', -500.0)"
# Recording the answer without applying it changes nothing else.
curl -fsS -X POST "$BASE/profile/focus" -H "$AUTH" -H 'content-type: application/json' -d '{"focus":"blood_pressure","apply":false}' >/dev/null
expect "without apply, only the focus moves" "$(curl -fsS "$BASE/profile" -H "$AUTH" | j " and (d['tracking_focus'], d['chart_nutrients'])")" "('blood_pressure', ['net_carbs_g'])"
expect "and the targets stay"                "$(curl -fsS "$BASE/targets" -H "$AUTH" | j ".__len__()")" "4"
expect "custom applies nothing"              "$(curl -fsS -X POST "$BASE/profile/focus" -H "$AUTH" -H 'content-type: application/json' -d '{"focus":"custom","apply":true}' | j " and (d['profile']['tracking_focus'], d['applied']['targets'], d['profile']['chart_nutrients'])")" "('custom', [], ['net_carbs_g'])"
expect "so the keto targets survive it"      "$(curl -fsS "$BASE/targets" -H "$AUTH" | j ".__len__()")" "4"

# Derived nutrients are computed where the total is serialised, so every
# payload agrees: a food, a recipe, an entry, a meal, a day, an average.
DAY=$(curl -fsS "$BASE/diary/day?date=2026-01-15" -H "$AUTH")
expect "net carbs are carbs minus fibre" \
  "$(echo "$DAY" | j " and round(d['total']['carbs_g'] - d['total']['fiber_g'], 2) == d['total']['net_carbs_g'] and d['total']['net_carbs_g'] > 0")" "True"
expect "on the food too" \
  "$(curl -fsS "$BASE/foods/$OATS" -H "$AUTH" | j " and d['per_serving']['net_carbs_g'] == round(d['per_serving']['carbs_g'] - d['per_serving']['fiber_g'], 2)")" "True"
expect "a net-carbs target reports progress against it" \
  "$(echo "$DAY" | j " and (lambda t: (t['consumed'] == d['total']['net_carbs_g'], t['status']))([t for t in d['targets'] if t['nutrient']=='net_carbs_g'][0])")" "(True, 'over')"
expect "each meal carries its own total" \
  "$(echo "$DAY" | j " and [(m['meal'], m['total']['net_carbs_g'] == round(sum(e['nutrients']['net_carbs_g'] for e in m['entries']), 2)) for m in d['meals'] if m['entries']]")" \
  "[('breakfast', True), ('lunch', True)]"
expect "energy share sums to 100" \
  "$(echo "$DAY" | j " and abs(d['energy_share']['protein_pct'] + d['energy_share']['carbs_pct'] + d['energy_share']['fat_pct'] - 100) < 0.2")" "True"
expect "and so does the average's" \
  "$(curl -fsS "$BASE/diary/summary?from=2026-01-01&to=2026-01-31" -H "$AUTH" | j " and abs(sum(d['energy_share'].values()) - 100) < 0.2")" "True"
expect "an empty day shares nothing, not NaN" \
  "$(curl -fsS "$BASE/diary/day?date=2025-06-30" -H "$AUTH" | j "['energy_share']")" "{'protein_pct': 0.0, 'carbs_pct': 0.0, 'fat_pct': 0.0}"

# Put the display back so the rest of the run sees the usual readout.
curl -fsS -X PATCH "$BASE/profile" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"shown_nutrients":["calories_kcal","protein_g","carbs_g","fat_g"],"chart_nutrients":["calories_kcal"]}' >/dev/null

echo "== fully logged days"
# A diary cannot tell a fast day from a day someone stopped logging after
# breakfast, and the estimator below must not be fed the second kind. The
# flag is the one person who knows saying which it was.
expect "a day starts not complete" \
  "$(curl -fsS "$BASE/diary/day?date=2026-01-15" -H "$AUTH" | j "['complete']")" "False"
expect "marking it is an upsert" \
  "$(curl -fsS -X PUT "$BASE/diary/day/2026-01-15/complete" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"complete":true}' | j "['complete']")" "True"
expect "and the day says so" \
  "$(curl -fsS "$BASE/diary/day?date=2026-01-15" -H "$AUTH" | j "['complete']")" "True"
# Nothing logged and everything logged: a fast day. It is listed, with zero
# entries, because it is data; dropping it would make the flag invisible.
curl -fsS -X PUT "$BASE/diary/day/2026-01-16/complete" -H "$AUTH" -H 'content-type: application/json' -d '{"complete":true}' >/dev/null
SUM=$(curl -fsS "$BASE/diary/summary?from=2026-01-01&to=2026-01-31" -H "$AUTH")
expect "the summary lists the fast day with nothing in it" \
  "$(echo "$SUM" | j " and [(x['date'], x['entry_count'], x['complete']) for x in d['days']]")" \
  "[('2026-01-15', 2, True), ('2026-01-16', 0, True)]"
expect "and counts complete days"             "$(echo "$SUM" | j "['complete_day_count']")" "2"
expect "while the average stays over logged days" "$(echo "$SUM" | j "['logged_day_count']")" "1"
expect "unmarking a day is the same call" \
  "$(curl -fsS -X PUT "$BASE/diary/day/2026-01-16/complete" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"complete":false}' | j "['complete']")" "False"
expect "an unmarked empty day is an absence again" \
  "$(curl -fsS "$BASE/diary/summary?from=2026-01-01&to=2026-01-31" -H "$AUTH" | j " and [x['date'] for x in d['days']]")" "['2026-01-15']"
status "a day in the future cannot have been logged" 400 -X PUT "$BASE/diary/day/2999-01-01/complete" -H "$AUTH" -H 'content-type: application/json' -d '{"complete":true}'

echo "== estimators"
# Expenditure by energy balance: intake on complete days, less what the scale
# stored. A fresh account, so the window holds exactly what this section
# puts in it, and dates relative to today because the window ends today.
EMAIL_EST="smoke-est-$(date +%s)-$RANDOM@example.test"
AUTH_EST="Authorization: Bearer $(curl -fsS -X POST "$BASE/auth/register" -H 'content-type: application/json' \
  -d "{\"email\":\"$EMAIL_EST\",\"password\":\"$PASSWORD\",\"display_name\":\"Estimator\"}" | j "['access_token']")"
curl -fsS -X PATCH "$BASE/profile" -H "$AUTH_EST" -H 'content-type: application/json' \
  -d '{"sex":"male","birth_date":"1990-06-15","height_cm":180,"activity_level":"moderate","goal":"cut"}' >/dev/null
TODAY=$(date -u +%F)
daysago()   { date -u -d "$TODAY -$1 days" +%F; }
daysahead() { date -u -d "$TODAY +$1 days" +%F; }

# Nothing yet: not ready, and every shortfall named. `ready` is the field to
# branch on; `estimate` is present exactly when it is true.
TDEE=$(curl -fsS "$BASE/estimates/tdee" -H "$AUTH_EST")
expect "with nothing logged the estimate is not ready" "$(echo "$TDEE" | j "['ready']")" "False"
expect "and no estimate is offered on thin data"        "$(echo "$TDEE" | j "['estimate']")" "None"
expect "it says what it has" \
  "$(echo "$TDEE" | j "['have']")" "{'complete_days': 0, 'weigh_ins': 0, 'span_days': 0}"
expect "and what it needs" \
  "$(echo "$TDEE" | j "['need']")" "{'complete_days': 7, 'weigh_ins': 2, 'span_days': 7}"
expect "in a sentence" \
  "$(echo "$TDEE" | j "['reason']")" "Needs 7 more days marked as fully logged and 2 more weigh-ins."
expect "the formula names what it is missing too" "$(echo "$TDEE" | j "['formula_missing']")" "['weight']"
status "the window has a floor"   400 "$BASE/estimates/tdee?days=3" -H "$AUTH_EST"
status "and a ceiling"            400 "$BASE/estimates/tdee?days=400" -H "$AUTH_EST"

# Twenty-eight complete days ending today: 26 at 1850 kcal, one fast day at
# nothing, one at 3700 — a mean of exactly 1850. A 29th day, outside the
# window and never marked, at 5000 kcal is the poison the flag exists to
# keep out.
CHOW=$(curl -fsS -X POST "$BASE/foods" -H "$AUTH_EST" -H 'content-type: application/json' \
  -d '{"name":"Estimator chow","calories_kcal":500,"protein_g":25,"carbs_g":50,"fat_g":20,"serving_size_g":100}' | j "['id']")
chow() { # chow <date> <grams>
  curl -fsS -X POST "$BASE/diary" -H "$AUTH_EST" -H 'content-type: application/json' \
    -d "{\"logged_on\":\"$1\",\"meal\":\"lunch\",\"food_id\":\"$CHOW\",\"quantity_g\":$2}" >/dev/null
}
for i in $(seq 0 27); do
  D=$(daysago "$i")
  if   [ "$i" -eq 3 ];  then :                 # a fast day: nothing to log
  elif [ "$i" -eq 10 ]; then chow "$D" 740     # 3700 kcal
  else                       chow "$D" 370     # 1850 kcal
  fi
  curl -fsS -X PUT "$BASE/diary/day/$D/complete" -H "$AUTH_EST" -H 'content-type: application/json' -d '{"complete":true}' >/dev/null
done
chow "$(daysago 28)" 1000                      # 5000 kcal, never marked

TDEE=$(curl -fsS "$BASE/estimates/tdee" -H "$AUTH_EST")
expect "complete days alone are not enough" \
  "$(echo "$TDEE" | j " and (d['ready'], d['have']['complete_days'], d['reason'])")" "(False, 28, 'Needs 2 more weigh-ins.')"
curl -fsS -X POST "$BASE/weights" -H "$AUTH_EST" -H 'content-type: application/json' -d "{\"recorded_on\":\"$(daysago 27)\",\"weight_kg\":85.0}" >/dev/null
expect "one weigh-in is not a trend" \
  "$(curl -fsS "$BASE/estimates/tdee" -H "$AUTH_EST" | j " and (d['ready'], d['have']['weigh_ins'], d['reason'])")" "(False, 1, 'Needs 1 more weigh-in.')"
NEAR=$(curl -fsS -X POST "$BASE/weights" -H "$AUTH_EST" -H 'content-type: application/json' -d "{\"recorded_on\":\"$(daysago 24)\",\"weight_kg\":84.8}" | j "['id']")
expect "two weigh-ins three days apart is not one either" \
  "$(curl -fsS "$BASE/estimates/tdee" -H "$AUTH_EST" | j " and (d['ready'], d['have']['span_days'], d['reason'])")" \
  "(False, 3, 'Needs weigh-ins at least 7 days apart (yours span 3).')"
curl -fsS -X DELETE "$BASE/weights/$NEAR" -H "$AUTH_EST" >/dev/null

# Weigh-ins on an exact line: 0.4 kg a week off, four points a week apart.
for pair in "20 84.6" "13 84.2" "6 83.8"; do
  set -- $pair
  curl -fsS -X POST "$BASE/weights" -H "$AUTH_EST" -H 'content-type: application/json' -d "{\"recorded_on\":\"$(daysago "$1")\",\"weight_kg\":$2}" >/dev/null
done
TDEE=$(curl -fsS "$BASE/estimates/tdee" -H "$AUTH_EST")
expect "the estimate is ready"                     "$(echo "$TDEE" | j "['ready']")" "True"
expect "and says so without a reason"              "$(echo "$TDEE" | j "['reason']")" "None"
expect "the mean is over complete days, fast day included" "$(echo "$TDEE" | j "['estimate']['mean_intake_kcal']")" "1850.0"
expect "the trend is 0.4 kg a week off"            "$(echo "$TDEE" | j "['estimate']['weight_change_kg_per_week']")" "-0.4"
expect "so expenditure is 1850 + 0.4/7 × 7700"     "$(echo "$TDEE" | j "['estimate']['tdee_kcal']")" "2290.0"
expect "a 440 kcal deficit"                        "$(echo "$TDEE" | j "['estimate']['energy_balance_kcal_per_day']")" "-440.0"
expect "the fitted line's ends are reported"       "$(echo "$TDEE" | j " and (d['estimate']['trend_start_kg'], d['estimate']['trend_end_kg'])")" "(85.0, 83.8)"
expect "28 complete days and 4 weigh-ins is good"  "$(echo "$TDEE" | j "['estimate']['confidence']")" "good"
expect "the budget re-bases the goal on the measured figure: 2290 − 500, to the nearest ten" \
  "$(echo "$TDEE" | j " and (d['estimate']['goal'], d['estimate']['goal_adjustment_kcal'], d['estimate']['budget_kcal'], d['estimate']['floored_at_minimum'])")" "('cut', -500.0, 1790.0, False)"
expect "the formula sits beside it, off the latest weigh-in" \
  "$(echo "$TDEE" | j " and (d['formula']['weight_kg'], d['formula']['tdee_kcal'] == round(d['formula']['bmr_kcal'] * 1.55))")" "(83.8, True)"
expect "the unmarked 5000 kcal day is ignored even inside a wider window" \
  "$(curl -fsS "$BASE/estimates/tdee?days=35" -H "$AUTH_EST" | j " and (d['have']['complete_days'], d['estimate']['mean_intake_kcal'])")" "(28, 1850.0)"
expect "a week's window has one weigh-in and says so" \
  "$(curl -fsS "$BASE/estimates/tdee?days=7" -H "$AUTH_EST" | j " and (d['ready'], d['have'], d['reason'])")" \
  "(False, {'complete_days': 7, 'weigh_ins': 1, 'span_days': 0}, 'Needs 1 more weigh-in.')"
expect "a fortnight is ready but low confidence: only two weigh-ins" \
  "$(curl -fsS "$BASE/estimates/tdee?days=14" -H "$AUTH_EST" | j " and (d['ready'], d['estimate']['tdee_kcal'], d['estimate']['confidence'])")" "(True, 2290.0, 'low')"

# Goal projection: the same trend, extended to the target weight.
PROJ=$(curl -fsS "$BASE/estimates/projection" -H "$AUTH_EST")
expect "the trend is ready"                    "$(echo "$PROJ" | j "['ready']")" "True"
expect "but there is no target weight yet"     "$(echo "$PROJ" | j " and (d['reached_on'], d['reached_reason'])")" "(None, 'no_target_weight')"
expect "the trend is reported regardless" \
  "$(echo "$PROJ" | j " and (d['trend']['as_of'], d['trend']['current_kg'], d['trend']['rate_kg_per_week'], d['trend']['caution'])")" \
  "('$(daysago 6)', 83.8, -0.4, False)"
expect "as is the caution line, 1% of body weight a week" "$(echo "$PROJ" | j "['trend']['caution_threshold_kg_per_week']")" "0.84"
expect "a by-date without a target says why" \
  "$(curl -fsS "$BASE/estimates/projection?by=$(daysahead 30)" -H "$AUTH_EST" | j " and (d['by'], d['by_reason'])")" "(None, 'no_target_weight')"

curl -fsS -X PATCH "$BASE/profile" -H "$AUTH_EST" -H 'content-type: application/json' -d '{"target_weight_kg":78}' >/dev/null
PROJ=$(curl -fsS "$BASE/estimates/projection" -H "$AUTH_EST")
# 83.8 → 78 at 0.4 kg a week is 101.5 days from the last weigh-in, six days
# ago: reached on day 102, which is 96 days from today.
expect "5.8 kg at 0.4 kg a week is 102 days"   "$(echo "$PROJ" | j "['days_to_target']")" "102"
expect "counted from the last weigh-in"        "$(echo "$PROJ" | j "['reached_on']")" "$(daysahead 96)"
expect "with no caution at this rate"          "$(echo "$PROJ" | j " and (d['trend']['caution'], d['reached_reason'])")" "(False, None)"

# What a deadline would take: 36 days from the last weigh-in is 1241 kcal a
# day under expenditure, 1.13 kg a week, past the caution line, and an
# intake that lands under the 1200 floor.
BY=$(curl -fsS "$BASE/estimates/projection?by=$(daysahead 30)" -H "$AUTH_EST")
expect "the plan counts from the last weigh-in" "$(echo "$BY" | j " and (d['by']['from'], d['by']['days'])")" "('$(daysago 6)', 36)"
expect "5.8 kg × 7700 over 36 days"            "$(echo "$BY" | j "['by']['daily_energy_change_kcal']")" "-1241.0"
expect "is 1.13 kg a week, which is flagged"   "$(echo "$BY" | j " and (d['by']['required_rate_kg_per_week'], d['by']['caution'])")" "(-1.13, True)"
expect "priced off the adaptive expenditure"   "$(echo "$BY" | j " and (d['by']['basis'], d['by']['basis_tdee_kcal'])")" "('adaptive', 2290.0)"
expect "and held at the 1200 kcal floor"       "$(echo "$BY" | j " and (d['by']['suggested_intake_kcal'], d['by']['floored_at_minimum'])")" "(1200.0, True)"
expect "a gentler deadline is not flagged" \
  "$(curl -fsS "$BASE/estimates/projection?by=$(daysahead 200)" -H "$AUTH_EST" \
    | j " and (d['by']['days'], d['by']['daily_energy_change_kcal'], d['by']['caution'], d['by']['suggested_intake_kcal'])")" "(206, -217.0, False, 2070.0)"
expect "a date already passed has no days to work with" \
  "$(curl -fsS "$BASE/estimates/projection?by=$(daysago 10)" -H "$AUTH_EST" | j " and (d['by'], d['by_reason'])")" "(None, 'date_not_after_as_of')"
curl -fsS -X PATCH "$BASE/profile" -H "$AUTH_EST" -H 'content-type: application/json' -d '{"target_weight_kg":90}' >/dev/null
expect "a target the trend is moving away from is never reached" \
  "$(curl -fsS "$BASE/estimates/projection" -H "$AUTH_EST" | j " and (d['reached_on'], d['reached_reason'])")" "(None, 'trend_points_away')"

# A flat scale projects nothing, and says that rather than "in eleven years".
EMAIL_FLAT="smoke-flat-$(date +%s)-$RANDOM@example.test"
AUTH_FLAT="Authorization: Bearer $(curl -fsS -X POST "$BASE/auth/register" -H 'content-type: application/json' \
  -d "{\"email\":\"$EMAIL_FLAT\",\"password\":\"$PASSWORD\",\"display_name\":\"Flat\"}" | j "['access_token']")"
curl -fsS -X PATCH "$BASE/profile" -H "$AUTH_FLAT" -H 'content-type: application/json' -d '{"target_weight_kg":75}' >/dev/null
curl -fsS -X POST "$BASE/weights" -H "$AUTH_FLAT" -H 'content-type: application/json' -d "{\"recorded_on\":\"$(daysago 7)\",\"weight_kg\":80.0}" >/dev/null
curl -fsS -X POST "$BASE/weights" -H "$AUTH_FLAT" -H 'content-type: application/json' -d "{\"recorded_on\":\"$TODAY\",\"weight_kg\":80.02}" >/dev/null
expect "a flat trend is called flat" \
  "$(curl -fsS "$BASE/estimates/projection" -H "$AUTH_FLAT" | j " and (d['ready'], d['reached_on'], d['reached_reason'])")" "(True, None, 'trend_is_flat')"
expect "a deadline is still priced, as a change with no intake invented for it" \
  "$(curl -fsS "$BASE/estimates/projection?by=$(daysahead 50)" -H "$AUTH_FLAT" | j " and (d['by']['daily_energy_change_kcal'], d['by']['basis'], d['by']['suggested_intake_kcal'])")" "(-773.0, None, None)"

echo "== global foods and recipe visibility"
# A second account, to check what crosses the boundary between users.
OTHER_EMAIL="smoke-other-$(date +%s)-$RANDOM@example.test"
OTHER=$(curl -fsS -X POST "$BASE/auth/register" -H 'content-type: application/json' \
  -d "{\"email\":\"$OTHER_EMAIL\",\"password\":\"$PASSWORD\",\"display_name\":\"Other\"}" | j "['access_token']")
OAUTH="Authorization: Bearer $OTHER"

# Foods are global: a food is a fact about a product, so everyone sees it, and
# everyone may correct it. What is not shared is the ability to make a
# correction anonymous or permanent — see the provenance section below.
expect "another account sees your custom food" \
  "$(curl -fsS "$BASE/foods?q=Rolled%20oats" -H "$OAUTH" | j "[0]['name']")" "Rolled oats"
expect "and may correct it, on the record" \
  "$(curl -fsS -X PUT "$BASE/foods/$MILK" -H "$OAUTH" -H 'content-type: application/json' \
      -d '{"name":"Whole milk","calories_kcal":61,"protein_g":3.2,"carbs_g":4.8,"fat_g":3.3,"serving_size_g":244,"serving_label":"1 cup"}' \
      | j "['provenance']['last_edited_by_name']")" \
  "Other"

# Recipes are the opposite: private until shared.
PUB=$(curl -fsS -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"name\":\"Shared bowl\",\"servings\":1,\"is_public\":true,\"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":50}]}" | j "['id']")

status "a private recipe is invisible to others"  404 "$BASE/recipes/$RID" -H "$OAUTH"
status "a public recipe is visible to others"     200 "$BASE/recipes/$PUB" -H "$OAUTH"
expect "a public recipe names its author"         "$(curl -fsS "$BASE/recipes/$PUB" -H "$OAUTH" | j "['author']")" "Smoke"
expect "and is not owned by the reader"           "$(curl -fsS "$BASE/recipes/$PUB" -H "$OAUTH" | j "['is_owner']")" "False"
status "others cannot edit a public recipe"       404 -X PUT "$BASE/recipes/$PUB" -H "$OAUTH" \
  -H 'content-type: application/json' -d "{\"name\":\"Hijacked\",\"servings\":1,\"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":1}]}"
status "others cannot delete a public recipe"     404 -X DELETE "$BASE/recipes/$PUB" -H "$OAUTH"
expect "scope=mine excludes others' recipes"      "$(curl -fsS "$BASE/recipes?scope=mine" -H "$OAUTH" | j ".__len__()")" "0"
expect "scope=public includes them"               "$(curl -fsS "$BASE/recipes?scope=public" -H "$OAUTH" | j " and ('Shared bowl' in [r['name'] for r in d])")" "True"

echo "== streaming fuzzy search"
# Each tier is flushed as it completes, so the response is a sequence of SSE
# events rather than one JSON body.
sse() { curl -fsSN "$BASE/search/foods?q=$1&limit=5" -H "$AUTH"; }
expect "responds as an event stream" \
  "$(curl -fsSI -o /dev/null -w '%{content_type}' "$BASE/search/foods?q=oats" -H "$AUTH" 2>/dev/null | cut -d';' -f1)" \
  "text/event-stream"
expect "an exact name hits the exact tier"    "$(sse 'Rolled%20oats' | grep -c 'tier":"exact"')"    "1"
expect "a prefix hits the prefix tier"        "$(sse 'Rolled' | grep -c 'tier":"prefix"')"          "1"
# Whole-string similarity scores this pair at 0.14 and would miss it; word
# similarity scores the best-matching word and finds it.
expect "a typo still finds the food"          "$(sse 'Rolld%20oatz' | grep -c '"tier"')"            "1"
# A multi-word typo must still resolve. Which tier catches it is an
# implementation detail — the indexed one often does — so assert that it is
# found, and separately that the scanning per-word tier is never run for a
# single-word query, where the index alone is enough.
expect "a multi-word typo finds it too"       "$(sse 'rolld%20oatts' | grep -c '\"tier\"')"        "1"
expect "per-word tier is skipped when unneeded" "$(sse 'oatz' | grep -c 'fuzzy_words')"             "0"

# A global food table accumulates the same product added by different people.
# The probe name is scoped to this run: the table is global AND persistent, so
# a previous run's rows are legitimately still there and would be counted.
PROBE="Smoke dupe probe $(date +%s)-$RANDOM"
DUPE="{\"name\":\"$PROBE\",\"calories_kcal\":100,\"protein_g\":5,\"carbs_g\":10,\"fat_g\":2,\"serving_size_g\":100}"
PROBE_Q=$(python3 -c 'import sys,urllib.parse;print(urllib.parse.quote(sys.argv[1]))' "$PROBE")
curl -fsS -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' -d "$DUPE" >/dev/null
curl -fsS -X POST "$BASE/foods" -H "$OAUTH" -H 'content-type: application/json' -d "$DUPE" >/dev/null
expect "identical foods collapse to one result" \
  "$(sse "$PROBE_Q" | grep -o "\"name\":\"$PROBE\"" | wc -l | tr -d ' ')" "1"

# ...but two foods that merely share a name are different things, and both stay.
curl -fsS -X POST "$BASE/foods" -H "$OAUTH" -H 'content-type: application/json' \
  -d "{\"name\":\"$PROBE\",\"calories_kcal\":250,\"protein_g\":9,\"carbs_g\":30,\"fat_g\":8,\"serving_size_g\":100}" >/dev/null
# A custom food — created the same way from the Foods page and from inside the
# meal-logging picker — has to be searchable and loggable straight away.
# A date of its own, so this does not disturb the diary totals asserted above.
CUSTOM_NAME="Homemade granola bar $(date +%s)-$RANDOM"
CUSTOM=$(curl -fsS -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"name\":\"$CUSTOM_NAME\",\"calories_kcal\":471,\"protein_g\":9.1,\"carbs_g\":64,\"fat_g\":19,\"serving_size_g\":45}" | j "['id']")
CUSTOM_Q=$(python3 -c 'import sys,urllib.parse;print(urllib.parse.quote(sys.argv[1]))' "$CUSTOM_NAME")

# Count the food itself, not tier events: a near-identical name from an earlier
# run can legitimately surface in the fuzzy tier alongside it.
expect "a newly created food is searchable at once" \
  "$(sse "$CUSTOM_Q" | grep -o "\"name\":\"$CUSTOM_NAME\"" | wc -l | tr -d ' ')" "1"
expect "and is loggable immediately" \
  "$(curl -fsS -X POST "$BASE/diary" -H "$AUTH" -H 'content-type: application/json' \
      -d "{\"logged_on\":\"2026-02-20\",\"meal\":\"snack\",\"food_id\":\"$CUSTOM\",\"quantity_g\":90}" \
      | j "['nutrients']['calories_kcal']")" "423.9"

expect "a nutritionally different namesake is kept" \
  "$(sse "$PROBE_Q" | grep -o "\"name\":\"$PROBE\"" | wc -l | tr -d ' ')" "2"
expect "the stream always ends with done"     "$(sse 'oats' | grep -c 'event: done')"               "1"
expect "an empty query ends cleanly"          "$(sse '' | grep -c 'event: done')"                   "1"
expect "a miss returns no tiers"              "$(sse 'zzzzznotafood' | grep -c '"tier"')"           "0"

echo "== progress photos"
# A real PNG, built without third-party libraries so this script keeps its
# only dependency on python3 itself.
PHOTO=$(mktemp /tmp/smoke-photo-XXXXXX.png)
python3 - "$PHOTO" <<'PYEOF'
import struct, sys, zlib
W = H = 600
raw = b"".join(b"\x00" + bytes([(x * 7) % 256, (y * 5) % 256, 128][c % 3] for x in range(W) for c in range(3)) for y in range(H))
def chunk(tag, data):
    body = tag + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)
png = (b"\x89PNG\r\n\x1a\n"
       + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0))
       + chunk(b"IDAT", zlib.compress(raw, 6))
       + chunk(b"IEND", b""))
open(sys.argv[1], "wb").write(png)
PYEOF

PHOTO_ENTRY=$(curl -fsS -X POST "$BASE/weights" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"recorded_on":"2026-05-05","weight_kg":81.1}' | j "['id']")
UPLOADED=$(curl -fsS -X POST "$BASE/weights/$PHOTO_ENTRY/photos" -H "$AUTH" \
  -F "file=@$PHOTO" -F "caption=Week one")

expect "photo attaches to the weigh-in" "$(echo "$UPLOADED" | j "['weight_entry_id']")" "$PHOTO_ENTRY"
expect "caption is kept"                "$(echo "$UPLOADED" | j "['caption']")"          "Week one"
# Uploads are re-encoded, which is what bounds their size and drops EXIF.
expect "re-encoded as jpeg"             "$(echo "$UPLOADED" | j "['content_type']")"     "image/jpeg"
PHOTO_ID=$(echo "$UPLOADED" | j "['id']")

SERVED=$(mktemp /tmp/smoke-served-XXXXXX.jpg)
curl -fsS "$BASE/photos/$PHOTO_ID" -H "$AUTH" -o "$SERVED"
expect "served bytes are a jpeg" "$(od -An -tx1 -N2 "$SERVED" | tr -d ' \n')" "ffd8"
expect "and carry no EXIF segment" "$(grep -c Exif "$SERVED" || true)" "0"
rm -f "$SERVED"

status "another account cannot read the photo" 404 "$BASE/photos/$PHOTO_ID" -H "$OAUTH"
status "an unauthenticated request cannot"     401 "$BASE/photos/$PHOTO_ID"
status "another account cannot attach one"     404 -X POST "$BASE/weights/$PHOTO_ENTRY/photos" -H "$OAUTH" -F "file=@$PHOTO"

# Anything that does not decode is rejected rather than stored and served back.
NOTAPHOTO=$(mktemp /tmp/smoke-notaphoto-XXXXXX.jpg)
printf '#!/bin/sh\necho not a photo\n' > "$NOTAPHOTO"
status "a non-image is rejected" 400 -X POST "$BASE/weights/$PHOTO_ENTRY/photos" -H "$AUTH" -F "file=@$NOTAPHOTO"

status "deleting the weigh-in succeeds"   204 -X DELETE "$BASE/weights/$PHOTO_ENTRY" -H "$AUTH"
status "and takes its photos with it"     404 "$BASE/photos/$PHOTO_ID" -H "$AUTH"

echo "== recipe photos"
# Same storage and the same serving route as progress photos; the difference
# is who may look. A weigh-in photo is its owner's alone. A recipe photo goes
# with the recipe, so sharing the recipe shares its photos, and un-sharing it
# takes them back.
PREC=$(curl -fsS -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Photographed\",\"servings\":1,\"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":50}]}" | j "['id']")
RUPLOADED=$(curl -fsS -X POST "$BASE/recipes/$PREC/photos" -H "$AUTH" -F "file=@$PHOTO" -F "caption=Plated")
RPHOTO=$(echo "$RUPLOADED" | j "['id']")
expect "photo attaches to the recipe"      "$(echo "$RUPLOADED" | j "['recipe_id']")"       "$PREC"
expect "and to nothing else"               "$(echo "$RUPLOADED" | j "['weight_entry_id']")" "None"
expect "the owner lists it"                "$(curl -fsS "$BASE/recipes/$PREC/photos" -H "$AUTH" | j "[0]['id']")" "$RPHOTO"
expect "the list card carries it as cover" "$(curl -fsS "$BASE/recipes?q=Photographed" -H "$AUTH" | j "[0]['cover_photo_url']")" "/api/v1/photos/$RPHOTO"

status "private: another account cannot list"  404 "$BASE/recipes/$PREC/photos" -H "$OAUTH"
status "private: nor read the bytes"           404 "$BASE/photos/$RPHOTO" -H "$OAUTH"
status "nor attach one of their own"           404 -X POST "$BASE/recipes/$PREC/photos" -H "$OAUTH" -F "file=@$PHOTO"

curl -fsS -X PUT "$BASE/recipes/$PREC" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Photographed\",\"servings\":1,\"is_public\":true,\"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":50}]}" >/dev/null
status "shared: another account can list"      200 "$BASE/recipes/$PREC/photos" -H "$OAUTH"
status "shared: and read the bytes"            200 "$BASE/photos/$RPHOTO" -H "$OAUTH"
status "but still not add to it"               404 -X POST "$BASE/recipes/$PREC/photos" -H "$OAUTH" -F "file=@$PHOTO"
status "nor delete what is there"              404 -X DELETE "$BASE/photos/$RPHOTO" -H "$OAUTH"
status "nor retitle it"                        404 -X PATCH "$BASE/photos/$RPHOTO/caption" -H "$OAUTH" -H 'content-type: application/json' -d '{"caption":"mine now"}'

echo "== shared means public"
# One switch, one meaning. A shared recipe is readable by every account and
# by anyone holding the link, with no token at all; the recipe's own id is
# the link, and turning the switch off is how it is revoked. The public page
# is assembled by the same code as the signed-in one, so the figures agree.
PUBLIC_VIEW=$(curl -fsS "$BASE/public/recipes/$PREC")
status "a shared recipe is readable with no token"   200 "$BASE/public/recipes/$PREC"
expect "and names its author"                        "$(echo "$PUBLIC_VIEW" | j "['author']")"   "Smoke"
expect "and belongs to nobody reading it"            "$(echo "$PUBLIC_VIEW" | j "['is_owner']")" "False"
expect "with the figures the signed-in page shows" \
  "$(echo "$PUBLIC_VIEW" | j "['per_serving']['calories_kcal']")" \
  "$(curl -fsS "$BASE/recipes/$PREC" -H "$AUTH" | j "['per_serving']['calories_kcal']")"
expect "and its photos, at the public photo route"   "$(echo "$PUBLIC_VIEW" | j "['photos'][0]['url']")" "/api/v1/public/photos/$RPHOTO"
status "which serves the bytes with no token"        200 "$BASE/public/photos/$RPHOTO"
expect "as an image" \
  "$(curl -fsS -o /dev/null -w '%{content_type}' "$BASE/public/photos/$RPHOTO")" "image/jpeg"
status "a private recipe is not there"               404 "$BASE/public/recipes/$RID"
status "not even for its owner: the route takes no token" 404 "$BASE/public/recipes/$RID" -H "$AUTH"

# A weigh-in photo is its owner's alone, and the public route can never
# reach one: the visibility rule with no viewer only ever admits a photo of
# a shared recipe.
PUB_ENTRY=$(curl -fsS -X POST "$BASE/weights" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"recorded_on":"2026-05-07","weight_kg":81.0}' | j "['id']")
PUB_WPHOTO=$(curl -fsS -X POST "$BASE/weights/$PUB_ENTRY/photos" -H "$AUTH" -F "file=@$PHOTO" | j "['id']")
status "a weigh-in photo is never public"            404 "$BASE/public/photos/$PUB_WPHOTO"
status "not even with its owner's token"             404 "$BASE/public/photos/$PUB_WPHOTO" -H "$AUTH"
status "though its owner still has it"               200 "$BASE/photos/$PUB_WPHOTO" -H "$AUTH"
# The weigh-in and its photo are kept: the account export below lists them.

# Un-sharing takes the page and the photos away together; sharing again
# brings both back at the same address.
curl -fsS -X PUT "$BASE/recipes/$PREC" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Photographed\",\"servings\":1,\"is_public\":false,\"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":50}]}" >/dev/null
status "un-sharing revokes the page"                 404 "$BASE/public/recipes/$PREC"
status "and the photos with it"                      404 "$BASE/public/photos/$RPHOTO"
curl -fsS -X PUT "$BASE/recipes/$PREC" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Photographed\",\"servings\":1,\"is_public\":true,\"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":50}]}" >/dev/null
status "sharing again restores the same link"        200 "$BASE/public/recipes/$PREC"
status "a nonsense id is a 404, not an error"        404 "$BASE/public/recipes/00000000-0000-0000-0000-000000000000"

echo "== shareable links"
# A shared recipe's link is a page, served by the API at /r/{slug}: the
# clients that matter for it — Slack, iMessage, Discord, search engines —
# read the head and never run the script that would tell them which recipe
# it is. So the name has to be in the markup, and the picture has to exist.
#
# ROOT is the API without /api/v1, because this page deliberately does not
# live under the versioned API: it is an address people paste into messages.
ROOT="${BASE%/api/v1}"
STAMP="$(date +%s)-$RANDOM"

SLUGGED=$(curl -fsS -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Crème Brûlée $STAMP\",\"servings\":4,\"is_public\":true,
  \"instructions\":\"1. Mix it all. 2. Bake for 40 minutes.\",
  \"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":200}]}")
SLUG_ID=$(echo "$SLUGGED" | j "['id']")
SLUG=$(echo "$SLUGGED" | j "['slug']")
expect "a new recipe is slugged from its name" "$SLUG" "creme-brulee-$STAMP"

PAGE=$(curl -fsS "$ROOT/r/$SLUG")
status "the page is served with no token" 200 "$ROOT/r/$SLUG"
expect "as HTML, not as the app shell" \
  "$(curl -fsS -o /dev/null -w '%{content_type}' "$ROOT/r/$SLUG")" "text/html; charset=utf-8"
expect "the title names the recipe" \
  "$(echo "$PAGE" | grep -c "<title>Crème Brûlée $STAMP — nom-inal</title>")" "1"
expect "and so does og:title" \
  "$(echo "$PAGE" | grep -c "property=\"og:title\" content=\"Crème Brûlée $STAMP\"")" "1"
expect "og:image points at this recipe's card" \
  "$(echo "$PAGE" | grep -c "property=\"og:image\" content=\"[^\"]*/public/recipes/$SLUG/preview.png\"")" "1"

# The absolute URLs in those tags are built from the address the page was
# reached at, port included. nginx's $host drops the port, which on the
# default compose port (8088) pointed every preview image at nothing; the
# proxy forwards $http_host for that reason and this is what says so.
PORTED=$(curl -fsS -H "Host: share.example:8443" "$ROOT/r/$SLUG")
expect "a non-default port survives into og:url" \
  "$(echo "$PORTED" | grep -c "property=\"og:url\" content=\"http://share.example:8443/r/$SLUG\"")" "1"
expect "and into the card's address" \
  "$(echo "$PORTED" | grep -c "property=\"og:image\" content=\"http://share.example:8443/")" "1"
# The structured data is the same shape this application's own importer reads
# off other people's recipe sites, so a shared recipe can be imported back.
LD=$(echo "$PAGE" | sed -n 's|.*<script type="application/ld+json">\(.*\)</script>.*|\1|p')
expect "it carries a schema.org Recipe" "$(echo "$LD" | j "['@type']")" "Recipe"
expect "named, with its yield and ingredients" \
  "$(echo "$LD" | j " and (d['name'], d['recipeYield'], len(d['recipeIngredient']))")" \
  "('Crème Brûlée $STAMP', '4 servings', 1)"

# Renaming does not break the link somebody already sent. The old slug still
# finds the recipe and says, permanently, where it lives now.
curl -fsS -X PUT "$BASE/recipes/$SLUG_ID" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Rhubarb Crumble $STAMP\",\"servings\":4,\"is_public\":true,
  \"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":200}]}" >/dev/null
NEW_SLUG=$(curl -fsS "$BASE/recipes/$SLUG_ID" -H "$AUTH" | j "['slug']")
expect "renaming re-slugs"                "$NEW_SLUG" "rhubarb-crumble-$STAMP"
status "the old slug still resolves"      301 "$ROOT/r/$SLUG"
expect "to the recipe's new address" \
  "$(curl -s -o /dev/null -w '%{redirect_url}' "$ROOT/r/$SLUG")" "$ROOT/r/$NEW_SLUG"
expect "and following it lands on the recipe" \
  "$(curl -fsSL "$ROOT/r/$SLUG" | grep -c "<title>Rhubarb Crumble $STAMP — nom-inal</title>")" "1"
# Links handed out before slugs existed are uuids, and keep working the same
# way: one canonical address per recipe, arrived at from either.
status "a uuid link still resolves"       301 "$ROOT/r/$SLUG_ID"
expect "to the same canonical address" \
  "$(curl -s -o /dev/null -w '%{redirect_url}' "$ROOT/r/$SLUG_ID")" "$ROOT/r/$NEW_SLUG"
status "an unknown slug is a 404, not an error" 404 "$ROOT/r/no-such-recipe-at-all"

# The card. 1200x630 is what Open Graph asks for and what every client crops
# to, drawn from the recipe when it has no photo and from its first photo
# when it has one.
png_size() { curl -fsS "$1" | python3 -c 'import sys,struct;d=sys.stdin.buffer.read();print("%dx%d" % struct.unpack(">II", d[16:24]))'; }
expect "a recipe with no photo gets a drawn card" \
  "$(png_size "$BASE/public/recipes/$NEW_SLUG/preview.png")" "1200x630"
expect "served as a PNG" \
  "$(curl -fsS -o /dev/null -w '%{content_type}' "$BASE/public/recipes/$NEW_SLUG/preview.png")" "image/png"
expect "a recipe with a photo gets its photo, cropped" \
  "$(png_size "$BASE/public/recipes/$PREC/preview.png")" "1200x630"

# Generating a PNG per crawler hit would be the whole cost of this feature.
PREVIEW_ETAG=$(curl -fsS -o /dev/null -D - "$BASE/public/recipes/$NEW_SLUG/preview.png" \
  | grep -i '^etag:' | tr -d '\r' | cut -d' ' -f2)
expect "the card is tagged"  "$(printf '%s' "$PREVIEW_ETAG" | head -c 1)" '"'
status "and answers a matching If-None-Match with 304" 304 \
  -H "If-None-Match: $PREVIEW_ETAG" "$BASE/public/recipes/$NEW_SLUG/preview.png"

# Un-sharing takes the page and the card with it, exactly as it takes the
# JSON and the photos.
curl -fsS -X PUT "$BASE/recipes/$SLUG_ID" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Rhubarb Crumble $STAMP\",\"servings\":4,\"is_public\":false,
  \"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":200}]}" >/dev/null
status "a private recipe has no page"           404 "$ROOT/r/$NEW_SLUG"
status "nor one for its owner: no token is read" 404 "$ROOT/r/$NEW_SLUG" -H "$AUTH"
status "and no card"                            404 "$BASE/public/recipes/$NEW_SLUG/preview.png"
expect "the page that is refused is still a page, not JSON" \
  "$(curl -s -o /dev/null -w '%{content_type}' "$ROOT/r/$NEW_SLUG")" "text/html; charset=utf-8"
curl -fsS -X DELETE "$BASE/recipes/$SLUG_ID" -H "$AUTH" >/dev/null

# A recipe that is logged cannot be deleted; its photos must survive the refusal.
PLOG=$(curl -fsS -X POST "$BASE/diary" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"logged_on\":\"2026-05-06\",\"meal\":\"lunch\",\"recipe_id\":\"$PREC\",\"recipe_servings\":1}" | j "['id']")
status "a logged recipe cannot be deleted"     400 -X DELETE "$BASE/recipes/$PREC" -H "$AUTH"
status "and the refusal cost it no photos"     200 "$BASE/photos/$RPHOTO" -H "$AUTH"
curl -fsS -X DELETE "$BASE/diary/$PLOG" -H "$AUTH" >/dev/null
status "deleting the recipe succeeds"          204 -X DELETE "$BASE/recipes/$PREC" -H "$AUTH"
status "and takes its photos with it"          404 "$BASE/photos/$RPHOTO" -H "$AUTH"
rm -f "$PHOTO" "$NOTAPHOTO"

echo "== reminders"
# Reminders store a cadence; being overdue is derived from your own records,
# so there is no scheduler and nothing to catch up.
expect "every kind is offered"     "$(curl -fsS "$BASE/reminders" -H "$AUTH" | j ".__len__()")" "3"
expect "and each is off until set" "$(curl -fsS "$BASE/reminders" -H "$AUTH" | j " and [r['enabled'] for r in d]")" "[False, False, False]"

curl -fsS -X PUT "$BASE/reminders" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"reminders":[{"kind":"weigh_in","every_days":7,"enabled":true}]}' >/dev/null
expect "a disabled kind is absent from status" "$(curl -fsS "$BASE/reminders/status" -H "$AUTH" | j ".__len__()")" "1"

# The account has weigh-ins from earlier in this run, all far in the past.
expect "an old weigh-in reads as due" \
  "$(curl -fsS "$BASE/reminders/status" -H "$AUTH" | j "[0]['due']")" "True"

# Weighing in today clears it, with no separate state to update.
curl -fsS -X POST "$BASE/weights" -H "$AUTH" -H 'content-type: application/json' -d '{"weight_kg":80.0}' >/dev/null
expect "weighing in today clears it"  "$(curl -fsS "$BASE/reminders/status" -H "$AUTH" | j "[0]['due']")"     "False"
expect "and says so"                  "$(curl -fsS "$BASE/reminders/status" -H "$AUTH" | j "[0]['message']")" "Weigh in — done today."

status "reject a zero cadence"     400 -X PUT "$BASE/reminders" -H "$AUTH" -H 'content-type: application/json' -d '{"reminders":[{"kind":"weigh_in","every_days":0}]}'
status "reject an unknown kind"    400 -X PUT "$BASE/reminders" -H "$AUTH" -H 'content-type: application/json' -d '{"reminders":[{"kind":"floss","every_days":1}]}'
status "reject a duplicate kind"   400 -X PUT "$BASE/reminders" -H "$AUTH" -H 'content-type: application/json' -d '{"reminders":[{"kind":"weigh_in","every_days":7},{"kind":"weigh_in","every_days":3}]}'

echo "== nutrient basis"
# A nutrition label states one serving, not 100 g. Posting what the label says
# and letting the server divide is the difference between a correct food and one
# that is wrong by the serving ratio -- silently, with no error to notice.
# Cheez-It: 28 g serving, 140 kcal, 2 g protein, 18 g carbs, 7 g fat.
LABEL=$(curl -fsS -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' -d '{
  "name":"Label crackers","serving_size_g":28,"serving_label":"27 crackers",
  "nutrient_basis":"per_serving",
  "calories_kcal":140,"protein_g":2,"carbs_g":18,"fat_g":7,"sodium_mg":230}')
LABEL_ID=$(echo "$LABEL" | j "['id']")
# 140 / 0.28 = 500
expect "per-serving input is converted to per 100 g" "$(echo "$LABEL" | j "['calories_kcal']")" "500.0"
expect "and the macros with it"                    "$(echo "$LABEL" | j "['carbs_g']")"        "64.285714"
expect "the basis is remembered"                   "$(echo "$LABEL" | j "['nutrient_basis']")" "per_serving"
# ...and what comes back per serving is what was typed in.
expect "per-serving reads back as entered"         "$(echo "$LABEL" | j "['per_serving']['calories_kcal']")" "140.0"
expect "with no drift on the macros"               "$(echo "$LABEL" | j "['per_serving']['carbs_g']")"       "18.0"

# Re-submitting untouched values must not manufacture a revision.
expect "an unchanged re-save is not an edit" \
  "$(curl -fsS -X PUT "$BASE/foods/$LABEL_ID" -H "$AUTH" -H 'content-type: application/json' -d '{
      "name":"Label crackers","serving_size_g":28,"serving_label":"27 crackers",
      "nutrient_basis":"per_serving",
      "calories_kcal":140,"protein_g":2,"carbs_g":18,"fat_g":7,"sodium_mg":230}' | j "['revision']")" \
  "1"

# Sent explicitly, not left to the default: the two paths serialise through
# different code, and only this one would have caught the enum's wire name
# disagreeing with the value stored in the column.
expect "per_100g is accepted as written" \
  "$(curl -fsS -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"name":"Basis explicit","nutrient_basis":"per_100g","calories_kcal":50,"protein_g":1,"carbs_g":2,"fat_g":3}' | j "['calories_kcal']")" \
  "50.0"
status "and an unknown basis is refused" 400 -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"name":"Basis bogus","nutrient_basis":"per100g","calories_kcal":50,"protein_g":1,"carbs_g":2,"fat_g":3}'

expect "the default basis is still per 100 g" \
  "$(curl -fsS -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"name":"Basis default","calories_kcal":50,"protein_g":1,"carbs_g":2,"fat_g":3}' | j "['nutrient_basis']")" \
  "per_100g"

# The bound that matters is the converted figure, so an ordinary label passes...
status "a rich but real label is accepted" 201 -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"name":"Peanut butter scoop","serving_size_g":32,"nutrient_basis":"per_serving","calories_kcal":190,"protein_g":7,"carbs_g":7,"fat_g":16}'
# ...and a mistyped serving size is caught by what it works out to.
status "an impossible per-100g result is refused" 400 -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"name":"Bad serving","serving_size_g":2,"nutrient_basis":"per_serving","calories_kcal":140,"protein_g":2,"carbs_g":18,"fat_g":7}'
expect "and the error names the likely cause" \
  "$(curl -s -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' \
      -d '{"name":"Bad serving","serving_size_g":2,"nutrient_basis":"per_serving","calories_kcal":140,"protein_g":2,"carbs_g":18,"fat_g":7}' \
      | j " and 'check the serving size' in d['message']")" \
  "True"

echo "== food provenance"
# Foods are a shared record rather than personal notes, so the interesting
# assertions are about the paper trail: who changed what, whether anyone else
# agrees, and whether a change can be undone.
VAR=$(curl -fsS -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"name\":\"Rolled oats\",\"calories_kcal\":71,\"protein_g\":2.5,\"carbs_g\":12,\"fat_g\":1.5,\"serving_size_g\":200,\"variant_of\":\"$OATS\",\"variant_label\":\"cooked\"}" | j "['id']")
expect "a variant points at its parent"   "$(curl -fsS "$BASE/foods/$VAR" -H "$AUTH" | j "['parent']['name']")" "Rolled oats"
expect "and the parent lists it"          "$(curl -fsS "$BASE/foods/$OATS" -H "$AUTH" | j " and [v['variant_label'] for v in d['variants']]")" "['cooked']"
status "the same label twice is refused"  409 -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' -d "{\"name\":\"x\",\"calories_kcal\":1,\"protein_g\":1,\"carbs_g\":1,\"fat_g\":1,\"variant_of\":\"$OATS\",\"variant_label\":\"Cooked\"}"
status "a variant of a variant is refused" 400 -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' -d "{\"name\":\"x\",\"calories_kcal\":1,\"protein_g\":1,\"carbs_g\":1,\"fat_g\":1,\"variant_of\":\"$VAR\",\"variant_label\":\"diced\"}"
status "a label with no parent is refused" 400 -X POST "$BASE/foods" -H "$AUTH" -H 'content-type: application/json' -d '{"name":"x","calories_kcal":1,"protein_g":1,"carbs_g":1,"fat_g":1,"variant_label":"raw"}'

# Anyone may edit anyone's food. What makes that safe is the rest of this block.
EDIT=$(curl -fsS -X PUT "$BASE/foods/$VAR" -H "$AUTH2" -H 'content-type: application/json' \
  -d "{\"name\":\"Rolled oats\",\"calories_kcal\":68,\"protein_g\":2.5,\"carbs_g\":12,\"fat_g\":1.5,\"serving_size_g\":200,\"variant_of\":\"$OATS\",\"variant_label\":\"cooked\",\"edit_summary\":\"68 kcal cooked, per the pack\"}")
expect "another account can edit it"      "$(echo "$EDIT" | j "['revision']")" "2"
expect "and the edit is attributed"       "$(echo "$EDIT" | j "['provenance']['last_edited_by_name']")" "Smoke Two"
expect "two people have now touched it"   "$(echo "$EDIT" | j "['provenance']['contributors']")" "2"

REVS=$(curl -fsS "$BASE/foods/$VAR/revisions" -H "$AUTH")
expect "both revisions are kept"          "$(echo "$REVS" | j ".__len__()")" "2"
expect "the diff names the changed field" "$(echo "$REVS" | j "[0]['changed_fields']")" "['calories_kcal']"
expect "the summary is on the revision"   "$(echo "$REVS" | j "[0]['summary']")" "68 kcal cooked, per the pack"

status "you cannot vouch for your own edit" 403 -X POST "$BASE/foods/$VAR/verify" -H "$AUTH2" -H 'content-type: application/json' -d '{"verdict":"confirm"}'
expect "one confirmation is not a quorum" \
  "$(curl -fsS -X POST "$BASE/foods/$VAR/verify" -H "$AUTH" -H 'content-type: application/json' -d '{"verdict":"confirm"}' | j "['provenance']['status']")" \
  "unverified"
expect "a second one is"  \
  "$(curl -fsS -X POST "$BASE/foods/$VAR/verify" -H "$AUTH3" -H 'content-type: application/json' -d '{"verdict":"confirm"}' | j "['provenance']['status']")" \
  "verified"

# The reason verification is keyed to a revision: agreement was given to a
# particular set of numbers and does not transfer to the next set.
AFTER=$(curl -fsS -X PUT "$BASE/foods/$VAR" -H "$AUTH3" -H 'content-type: application/json' \
  -d "{\"name\":\"Rolled oats\",\"calories_kcal\":70,\"protein_g\":2.5,\"carbs_g\":12,\"fat_g\":1.5,\"serving_size_g\":200,\"variant_of\":\"$OATS\",\"variant_label\":\"cooked\"}")
expect "an edit drops it back to unverified" "$(echo "$AFTER" | j "['provenance']['status']")"        "unverified"
expect "and the old votes stop counting"     "$(echo "$AFTER" | j "['provenance']['confirmations']")" "0"
expect "but are still on the record"         "$(curl -fsS "$BASE/foods/$VAR/verify" -H "$AUTH" | j " and [v['current'] for v in d]")" "[False, False]"
expect "a dispute is louder than silence" \
  "$(curl -fsS -X POST "$BASE/foods/$VAR/verify" -H "$AUTH" -H 'content-type: application/json' -d '{"verdict":"dispute","note":"cooked oats are nearer 71"}' | j "['provenance']['status']")" \
  "disputed"
# Cached on the row, so a list can say "somebody objected" without aggregating
# votes per result -- the streaming search in particular cannot afford that.
expect "and is readable from the list without counting votes" \
  "$(curl -fsS "$BASE/foods?q=Rolled%20oats" -H "$AUTH" | j " and any(f['disputed_at'] for f in d)")" "True"

# Undoing a bad edit moves the history forward rather than erasing it.
REVERTED=$(curl -fsS -X POST "$BASE/foods/$VAR/revert" -H "$AUTH2" -H 'content-type: application/json' -d '{"revision":1,"reason":"back to the measured value"}')
expect "a revert restores the value"   "$(echo "$REVERTED" | j "['calories_kcal']")" "71.0"
expect "as a new revision"             "$(echo "$REVERTED" | j "['revision']")"      "4"
expect "leaving the bad edit on record" "$(curl -fsS "$BASE/foods/$VAR/revisions" -H "$AUTH" | j ".__len__()")" "4"

status "a food others have worked on cannot be deleted" 403 -X DELETE "$BASE/foods/$VAR" -H "$AUTH2"

EXPORT=$(curl -fsS "$BASE/foods/export" -H "$AUTH")
expect "the export is versioned"        "$(echo "$EXPORT" | j "['format']")" "1"
expect "and carries no internal ids"    "$(echo "$EXPORT" | j " and any('id' in f for f in d['foods'])")" "False"
expect "a variant exports by parent key" \
  "$(echo "$EXPORT" | j " and [f['variant_of_key'] for f in d['foods'] if f['variant_label']=='cooked'] != [None]")" "True"

echo "== recipe export"
# The seed-repository shape: no internal ids anywhere, foods named by the
# same natural key the foods export uses, sub-recipes inlined by name with
# their own items, free text kept as text.
REXP=$(curl -fsS "$BASE/recipes/$DISH_ID/export" -H "$AUTH")
expect "the export is versioned"            "$(echo "$REXP" | j "['format']")" "1"
expect "and is the recipe by name"          "$(echo "$REXP" | j "['recipe']['name']")" "Smoke dish"
expect "with no internal ids anywhere" \
  "$(echo "$REXP" | j " and (lambda walk: walk(walk, d))(lambda w, v: (isinstance(v, dict) and (any(k.endswith('id') for k in v) or any(w(w, x) for x in v.values()))) or (isinstance(v, list) and any(w(w, x) for x in v)))")" "False"
expect "a food is named by its key"         "$(echo "$REXP" | j "['recipe']['items'][0]['food']['key']")" "whole milk|"
expect "the key the foods export computes" \
  "$(echo "$REXP" | j "['recipe']['items'][1]['recipe']['items'][0]['food']['key']")" \
  "$(echo "$EXPORT" | j " and [f['variant_of_key'] for f in d['foods'] if f['variant_label']=='cooked'][0]")"
expect "grams travel with it"               "$(echo "$REXP" | j "['recipe']['items'][0]['quantity_g']")" "200.0"
expect "a sub-recipe is inlined by name"    "$(echo "$REXP" | j "['recipe']['items'][1]['recipe']['name']")" "Smoke sauce"
expect "with the servings taken of it"      "$(echo "$REXP" | j "['recipe']['items'][1]['servings']")" "2.0"
expect "and its own items"                  "$(echo "$REXP" | j "['recipe']['items'][1]['recipe']['items'][0]['quantity_g']")" "200.0"
WEXP=$(curl -fsS "$BASE/recipes/$LOOSE_ID/export?format=json" -H "$AUTH")
expect "free text is kept as text"          "$(echo "$WEXP" | j "['recipe']['items'][1]['label']")" "salt and pepper to taste"

# Markdown: a recipe card, with the method split into the same steps the
# recipe page numbers and the nutrition per serving.
STEPPED=$(curl -fsS -X POST "$BASE/recipes" -H "$AUTH" -H 'content-type: application/json' -d "{
  \"name\":\"Stepped\",\"servings\":2,\"description\":\"Two lines.\",
  \"instructions\":\"1. Preheat the oven.\\n2. Roast.\",
  \"items\":[{\"food_id\":\"$OATS\",\"quantity_g\":100},{\"label\":\"salt\"}]}" | j "['id']")
MD=$(curl -fsS "$BASE/recipes/$STEPPED/export?format=markdown" -H "$AUTH")
expect "markdown is served as markdown" \
  "$(curl -fsS -o /dev/null -w '%{content_type}' "$BASE/recipes/$STEPPED/export?format=markdown" -H "$AUTH")" "text/markdown; charset=utf-8"
expect "it opens with the title"            "$(echo "$MD" | head -1)" "# Stepped"
expect "says how many it makes"             "$(echo "$MD" | grep -c '^Makes 2 servings\.$')" "1"
expect "lists ingredients as amounts"       "$(echo "$MD" | grep -c '^- 100 g Rolled oats$')" "1"
expect "and free text as itself"            "$(echo "$MD" | grep -c '^- salt$')" "1"
expect "numbers the method"                 "$(echo "$MD" | grep -c '^2\. Roast\.$')" "1"
# 379 kcal over 2 servings.
expect "and gives nutrition per serving"    "$(echo "$MD" | grep -c '^| 190 kcal |')" "1"
expect "naming what it had to leave out"    "$(echo "$MD" | grep -c '^Excludes 1 ingredient with no nutrition information\.$')" "1"
DMD=$(curl -fsS "$BASE/recipes/$DISH_ID/export?format=markdown" -H "$AUTH")
expect "a sub-recipe nests under its line"  "$(echo "$DMD" | grep -c '^  - 200 g Rolled oats$')" "1"
status "an unknown format is refused"       400 "$BASE/recipes/$STEPPED/export?format=xml" -H "$AUTH"
status "a private recipe exports for its owner only" 404 "$BASE/recipes/$STEPPED/export" -H "$OAUTH"
status "a shared one for anyone signed in"  200 "$BASE/recipes/$PUB/export" -H "$OAUTH"
status "but not for nobody"                 401 "$BASE/recipes/$PUB/export"

echo "== recipe import from a url"
# The server fetches a page the caller chose, which is request forgery
# territory: anything private, local or reserved is refused before a
# connection is opened, and so is anything that is not a web page. The page
# reading itself is unit-tested against fixtures (cargo test), since a
# smoke run has no public web to fetch from.
imp() { curl -s -o /dev/null -w '%{http_code}' -X POST "$BASE/recipes/import" -H "$AUTH" -H 'content-type: application/json' -d "{\"url\":\"$1\"}"; }
imp_msg() { curl -s -X POST "$BASE/recipes/import" -H "$AUTH" -H 'content-type: application/json' -d "{\"url\":\"$1\"}" | j "['message']"; }
expect "loopback is refused"                "$(imp "http://127.0.0.1:8080/recipe")" "400"
expect "and says why"                       "$(imp_msg "http://127.0.0.1:8080/recipe" | grep -c 'private, local or reserved')" "1"
expect "so is the API's own address"        "$(imp "${BASE%/api/v1}/api/v1/health")" "400"
expect "and localhost by name"              "$(imp "http://localhost/recipe")" "400"
expect "and an IPv6 loopback"               "$(imp "http://[::1]/recipe")" "400"
expect "and a private network"              "$(imp "http://10.0.0.1/recipe")" "400"
expect "and the cloud metadata address"     "$(imp "http://169.254.169.254/latest/meta-data/")" "400"
expect "and a non-http scheme"              "$(imp "ftp://example.com/recipe")" "400"
expect "and a file path"                    "$(imp "file:///etc/passwd")" "400"
expect "and credentials in the URL"         "$(imp "http://user:secret@example.com/")" "400"
expect "and something that is not a URL"    "$(imp "not a url at all")" "400"
status "a missing url is a 400"             400 -X POST "$BASE/recipes/import" -H "$AUTH" -H 'content-type: application/json' -d '{}'
status "and the import needs a token"       401 -X POST "$BASE/recipes/import" -H 'content-type: application/json' -d '{"url":"https://example.com/"}'

echo "== account export and import"
# One document, everything the account owns and nothing secret. Read back
# into a second account it recreates the lot by natural identity; read a
# second time it changes nothing, and says so.
ACCT=$(curl -fsS "$BASE/account/export" -H "$AUTH")
expect "the export is versioned"            "$(echo "$ACCT" | j "['format']")" "1"
expect "and is this account's"              "$(echo "$ACCT" | j "['profile']['email']")" "$EMAIL"
expect "with nothing secret in it" \
  "$(echo "$ACCT" | python3 -c 'import sys; t=sys.stdin.read().lower(); print(any(k in t for k in ("password","hash","token","nomi_")))')" "False"
expect "the foods it uses"                  "$(echo "$ACCT" | j " and any(f['name'] == 'Rolled oats' and f['variant_label'] is None for f in d['foods'])")" "True"
expect "in the seed shape"                  "$(echo "$ACCT" | j " and all('id' not in f and 'created_by' not in f for f in d['foods'])")" "True"
expect "every recipe, sub-recipes inlined"  "$(echo "$ACCT" | j " and [r['items'][1]['recipe']['name'] for r in d['recipes'] if r['name'] == 'Smoke dish'][0]")" "Smoke sauce"
expect "the diary by what was eaten"        "$(echo "$ACCT" | j " and any(e['recipe_name'] == 'Smoke dish' and e['logged_on'] == '2026-02-02' for e in d['diary'])")" "True"
expect "weigh-ins by date"                  "$(echo "$ACCT" | j " and len(d['weights'])")" "$(curl -fsS "$BASE/weights" -H "$AUTH" | j ".__len__()")"
expect "targets and reminders"              "$(echo "$ACCT" | j " and len(d['targets']) > 0 and len(d['reminders']) > 0")" "True"
expect "photo metadata, with a way to fetch the bytes" \
  "$(echo "$ACCT" | j " and len(d['photos']) > 0 and all(p['url'].startswith('/api/v1/photos/') and p['byte_size'] > 0 for p in d['photos'])")" "True"
expect "a weigh-in photo named by its date" \
  "$(echo "$ACCT" | j " and any(p['subject'] == 'weigh_in' and p['recorded_on'] == '2026-05-07' for p in d['photos'])")" "True"

# The CSV form is the two tables people put in a spreadsheet, zipped together.
CSVZIP=$(mktemp /tmp/smoke-export-XXXXXX.zip)
curl -fsS "$BASE/account/export?format=csv" -H "$AUTH" -o "$CSVZIP"
expect "csv comes as a zip" \
  "$(curl -fsS -o /dev/null -w '%{content_type}' "$BASE/account/export?format=csv" -H "$AUTH")" "application/zip"
expect "holding the diary and the weights" \
  "$(python3 -c 'import sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); print(sorted(z.namelist()))' "$CSVZIP")" "['diary.csv', 'weights.csv']"
expect "with the diary's nutrients computed" \
  "$(python3 -c 'import sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); print(z.read("diary.csv").decode().splitlines()[0])' "$CSVZIP")" \
  "date,meal,kind,name,brand,quantity_g,recipe_servings,calories_kcal,protein_g,carbs_g,net_carbs_g,fat_g,fiber_g,sugar_g,saturated_fat_g,sodium_mg,logged_at"
expect "and the day the recipe was logged agrees with the diary" \
  "$(python3 -c 'import sys,zipfile,csv,io; z=zipfile.ZipFile(sys.argv[1]); rows=list(csv.DictReader(io.StringIO(z.read("diary.csv").decode()))); print(sum(float(r["calories_kcal"]) for r in rows if r["date"]=="2026-02-02"))' "$CSVZIP")" "501.0"
rm -f "$CSVZIP"
status "an unknown format is refused"       400 "$BASE/account/export?format=xml" -H "$AUTH"

# Into a fresh account.
RESTORE_EMAIL="smoke-restore-$(date +%s)-$RANDOM@example.test"
RAUTH="Authorization: Bearer $(curl -fsS -X POST "$BASE/auth/register" -H 'content-type: application/json' \
  -d "{\"email\":\"$RESTORE_EMAIL\",\"password\":\"$PASSWORD\",\"display_name\":\"Restore\"}" | j "['access_token']")"
REPORT=$(curl -fsS -X POST "$BASE/account/import" -H "$RAUTH" -H 'content-type: application/json' -d "$ACCT")
expect "the profile is filled in"           "$(echo "$REPORT" | j "['profile_updated']")" "True"
expect "but the email is not touched"       "$(curl -fsS "$BASE/auth/me" -H "$RAUTH" | j "['email']")" "$RESTORE_EMAIL"
expect "and the name is"                    "$(curl -fsS "$BASE/auth/me" -H "$RAUTH" | j "['display_name']")" "Smoke"
expect "every recipe is created"            "$(echo "$REPORT" | j "['recipes']['created']")" "$(echo "$ACCT" | j " and len(d['recipes'])")"
expect "foods already here are left alone"  "$(echo "$REPORT" | j "['foods']['created']")" "0"
expect "and counted as skipped"             "$(echo "$REPORT" | j "['foods']['skipped']")" "$(echo "$ACCT" | j " and len(d['foods'])")"
expect "every diary entry is created"       "$(echo "$REPORT" | j "['diary']['created']")" "$(echo "$ACCT" | j " and len(d['diary'])")"
expect "every weigh-in is created"          "$(echo "$REPORT" | j "['weights']['created']")" "$(echo "$ACCT" | j " and len(d['weights'])")"
expect "targets and reminders too"          "$(echo "$REPORT" | j "['targets']['created'] > 0 and d['reminders']['created'] > 0")" "True"
expect "photos are named as not restored"   "$(echo "$REPORT" | j " and any('photo' in n for n in d['notes'])")" "True"
expect "and nothing else was left unsaid"   "$(echo "$REPORT" | j " and len(d['notes'])")" "1"
expect "the restored recipe adds up the same" \
  "$(curl -fsS "$BASE/recipes?q=Smoke%20dish" -H "$RAUTH" | j "[0]['per_serving']['calories_kcal']")" "501.0"
expect "linked to its own copy of the sauce" \
  "$(curl -fsS "$BASE/recipes?q=Smoke%20dish" -H "$RAUTH" | j "[0]['item_count']")" "2"
expect "and so does the restored diary" \
  "$(curl -fsS "$BASE/diary/day?date=2026-02-02" -H "$RAUTH" | j "['total']['calories_kcal']")" "501.0"
expect "free text survived the trip" \
  "$(curl -fsS "$BASE/recipes?q=Loose%20ends" -H "$RAUTH" | j "[0]['untracked_count']")" "2"

# The same file again changes nothing.
AGAIN=$(curl -fsS -X POST "$BASE/account/import" -H "$RAUTH" -H 'content-type: application/json' -d "$ACCT")
expect "a second import creates nothing"    "$(echo "$AGAIN" | j " and sum(d[k]['created'] + d[k]['updated'] for k in ('targets','reminders','foods','recipes','diary','weights'))")" "0"
expect "and the profile is already right"   "$(echo "$AGAIN" | j "['profile_updated']")" "False"
expect "recipes are still one each"         "$(curl -fsS "$BASE/recipes" -H "$RAUTH" | j ".__len__()")" "$(echo "$ACCT" | j " and len(d['recipes'])")"

# A trimmed, hand-written file: a changed weigh-in updates in place, a
# recipe naming a food this instance lacks keeps the line as text and says
# so, and an unknown format is refused before anything is touched.
PARTIAL=$(curl -fsS -X POST "$BASE/account/import" -H "$RAUTH" -H 'content-type: application/json' -d '{
  "weights":[{"recorded_on":"2026-01-01","weight_kg":83.0}],
  "recipes":[{"name":"Restored ghost","servings":2,
              "items":[{"food":{"key":"unobtainium|","name":"Unobtainium","brand":null},"quantity_g":100},
                       {"label":"salt"}]}]}')
expect "a changed weigh-in is updated"      "$(echo "$PARTIAL" | j "['weights']['updated']")" "1"
expect "the recipe is still created"        "$(echo "$PARTIAL" | j "['recipes']['created']")" "1"
expect "with the missing food kept as text" "$(curl -fsS "$BASE/recipes?q=Restored%20ghost" -H "$RAUTH" | j "[0]['untracked_count']")" "2"
expect "and the gap named"                  "$(echo "$PARTIAL" | j " and any('Unobtainium' in n for n in d['notes'])")" "True"
PARTIAL2=$(curl -fsS -X POST "$BASE/account/import" -H "$RAUTH" -H 'content-type: application/json' -d '{
  "recipes":[{"name":"Restored ghost","servings":2,
              "items":[{"food":{"key":"unobtainium|","name":"Unobtainium","brand":null},"quantity_g":100},
                       {"label":"salt"}]}]}')
expect "and the same gap twice is not a change" "$(echo "$PARTIAL2" | j "['recipes']['skipped']")" "1"
status "a future format is refused"         400 -X POST "$BASE/account/import" -H "$RAUTH" -H 'content-type: application/json' -d '{"format":2}'
status "and the import needs a token"       401 -X POST "$BASE/account/import" -H 'content-type: application/json' -d '{}'

echo "== api keys"
KEY=$(curl -fsS -X POST "$BASE/keys" -H "$AUTH2" -H 'content-type: application/json' -d '{"name":"smoke dashboard"}')
KEYTOKEN=$(echo "$KEY" | j "['token']")
KEYID=$(echo "$KEY" | j "['id']")
expect "a new key defaults to read-only" "$(echo "$KEY" | j "['scopes']")" "['read']"
expect "the token is returned once"      "$(echo "$KEY" | j " and d['token'].startswith('nomi_')")" "True"
expect "and never again"                 "$(curl -fsS "$BASE/keys" -H "$AUTH2" | j " and any('token' in k for k in d)")" "False"
expect "the prefix identifies the key"   "$(curl -fsS "$BASE/keys" -H "$AUTH2" | j " and d[0]['prefix'] == \"$KEYTOKEN\"[:13]")" "True"

status "a read key can read"             200 "$BASE/foods" -H "Authorization: Bearer $KEYTOKEN"
status "the x-api-key header works too"  200 "$BASE/foods" -H "x-api-key: $KEYTOKEN"
status "a read key cannot write"         403 -X POST "$BASE/foods" -H "Authorization: Bearer $KEYTOKEN" -H 'content-type: application/json' -d '{"name":"nope","calories_kcal":1,"protein_g":1,"carbs_g":1,"fat_g":1}'
status "and cannot mint another key"     403 -X POST "$BASE/keys"  -H "Authorization: Bearer $KEYTOKEN" -H 'content-type: application/json' -d '{"name":"second"}'
status "nor even list them"              403 "$BASE/keys" -H "Authorization: Bearer $KEYTOKEN"

WKEY=$(curl -fsS -X POST "$BASE/keys" -H "$AUTH2" -H 'content-type: application/json' -d '{"name":"smoke logger","scopes":["write"]}' | j "['token']")
status "a write key can write"           201 -X POST "$BASE/foods" -H "Authorization: Bearer $WKEY" -H 'content-type: application/json' -d '{"name":"Logged by a key","calories_kcal":10,"protein_g":1,"carbs_g":1,"fat_g":1}'
status "an unknown token is a 401"       401 "$BASE/foods" -H 'Authorization: Bearer nomi_thisisnotarealtokenatall'
status "names are unique while active"   409 -X POST "$BASE/keys" -H "$AUTH2" -H 'content-type: application/json' -d '{"name":"smoke dashboard"}'
curl -fsS -X DELETE "$BASE/keys/$KEYID" -H "$AUTH2" >/dev/null
status "a revoked key stops working"     401 "$BASE/foods" -H "Authorization: Bearer $KEYTOKEN"
status "and frees its name"              201 -X POST "$BASE/keys" -H "$AUTH2" -H 'content-type: application/json' -d '{"name":"smoke dashboard"}'

echo "== mcp"
# The API serves MCP itself, stateless over Streamable HTTP: one JSON-RPC
# request per POST, authenticated with the same keys as everything else. The
# tools are read from the OpenAPI registry, so the assertions below name one
# generated tool and one hand-written one rather than counting.
MCP="${BASE%/api/v1}/mcp"
mcp() { # mcp <key> <method> [params-json]
  local params="${3:-"{}"}"
  curl -s -X POST "$MCP" -H 'content-type: application/json' \
    -H 'accept: application/json, text/event-stream' -H "Authorization: Bearer $1" \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$2\",\"params\":$params}"
}
MRKEY=$(curl -fsS -X POST "$BASE/keys" -H "$AUTH" -H 'content-type: application/json' -d '{"name":"smoke mcp reader"}' | j "['token']")
MWKEY=$(curl -fsS -X POST "$BASE/keys" -H "$AUTH" -H 'content-type: application/json' -d '{"name":"smoke mcp writer","scopes":["write"]}' | j "['token']")

status "no key is a 401"                 401 -X POST "$MCP" -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}'
status "a session token is not a key"    401 -X POST "$MCP" -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' -H "$AUTH" -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}'
expect "initialize names the server" \
  "$(mcp "$MWKEY" initialize '{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}' | j "['result']['serverInfo']['name']")" "nom-inal"

WTOOLS=$(mcp "$MWKEY" tools/list)
expect "a generated tool is listed"      "$(echo "$WTOOLS" | j " and 'diary_day' in [t['name'] for t in d['result']['tools']]")" "True"
expect "a composite tool is listed"      "$(echo "$WTOOLS" | j " and 'log_food' in [t['name'] for t in d['result']['tools']]")" "True"
expect "its schema was dereferenced"     "$(echo "$WTOOLS" | j " and 'food_id' in [t for t in d['result']['tools'] if t['name']=='recipes_create'][0]['inputSchema']['properties']['items']['items']['properties']")" "True"
RTOOLS=$(mcp "$MRKEY" tools/list)
expect "a read key sees no write tools"  "$(echo "$RTOOLS" | j " and [t['name'] for t in d['result']['tools'] if t['name'] in ('diary_create','log_food','recipes_update')]")" "[]"
expect "but still the reads"             "$(echo "$RTOOLS" | j " and 'today' in [t['name'] for t in d['result']['tools']]")" "True"

# The composite answers with the API's own figures: the same day, fetched
# over HTTP, must agree to the cent.
DAYKCAL=$(curl -fsS "$BASE/diary/day?date=2026-01-15" -H "$AUTH" | j "['total']['calories_kcal']")
expect "tools/call today matches the API" \
  "$(mcp "$MRKEY" tools/call '{"name":"today","arguments":{"date":"2026-01-15"}}' | j "['result']['structuredContent']['total']['calories_kcal']")" "$DAYKCAL"
# A generated tool dispatches in-process to the same handler.
expect "a generated tool answers too" \
  "$(mcp "$MRKEY" tools/call '{"name":"diary_day","arguments":{"date":"2026-01-15"}}' | j "['result']['structuredContent']['total']['calories_kcal']")" "$DAYKCAL"
# "2 bananas": the plural finds "Bananas, raw", and the count is servings.
# 2 x 118 g x 89 kcal/100 g = 210.04, and nothing is written without confirm.
LOG=$(mcp "$MWKEY" tools/call '{"name":"log_food","arguments":{"text":"2 bananas","meal":"snack","date":"2026-01-16"}}')
expect "log_food resolves a count to grams" "$(echo "$LOG" | j "['result']['structuredContent']['items'][0]['grams']")" "236.0"
expect "and reports the calories"           "$(echo "$LOG" | j "['result']['structuredContent']['total_kcal']")" "210.04"
expect "without writing anything"           "$(echo "$LOG" | j "['result']['structuredContent']['logged']")" "False"
expect "a read key is refused a write tool" \
  "$(mcp "$MRKEY" tools/call '{"name":"log_food","arguments":{"text":"banana","meal":"snack"}}' | j "['result']['isError']")" "True"
expect "the guide resource reads back" \
  "$(mcp "$MRKEY" resources/read '{"uri":"nom-inal://guide"}' | j " and 'per 100 g' in d['result']['contents'][0]['text']")" "True"

echo "== administration"
# A self-hosted instance has no outside authority to appoint an owner, so the
# first account to exist becomes one. That rule only has something to say on a
# fresh database; re-running this script against a used one is not a failure.
if [ "$(curl -s -o /dev/null -w '%{http_code}' "$BASE/admin/stats" -H "$AUTH")" = "200" ]; then
  pass "the first account administers the instance"
  status "and later accounts do not"      403 "$BASE/admin/stats" -H "$AUTH2"
  expect "stats report the quorum in use" "$(curl -fsS "$BASE/admin/stats" -H "$AUTH" | j "['food_quorum'] >= 1")" "True"
  expect "the user list carries activity" "$(curl -fsS "$BASE/admin/users" -H "$AUTH" | j " and any(u['food_edits'] > 0 for u in d)")" "True"

  expect "an administrator can promote"   "$(curl -fsS -X PATCH "$BASE/admin/users/$UID2" -H "$AUTH" -H 'content-type: application/json' -d '{"is_admin":true}' | j "['is_admin']")" "True"
  status "but not demote themselves"      400 -X PATCH "$BASE/admin/users/$(curl -fsS "$BASE/auth/me" -H "$AUTH" | j "['id']")" -H "$AUTH" -H 'content-type: application/json' -d '{"is_admin":false}'

  curl -fsS -X PATCH "$BASE/admin/users/$UID2" -H "$AUTH" -H 'content-type: application/json' -d '{"disabled":true}' >/dev/null
  status "a suspended account is locked out"   403 "$BASE/foods" -H "$AUTH2"
  status "its keys are locked out with it"     403 "$BASE/foods" -H "Authorization: Bearer $WKEY"
  status "and it cannot sign back in"          403 -X POST "$BASE/auth/login" -H 'content-type: application/json' -d "{\"email\":\"$EMAIL2\",\"password\":\"$PASSWORD\"}"
  curl -fsS -X PATCH "$BASE/admin/users/$UID2" -H "$AUTH" -H 'content-type: application/json' -d '{"disabled":false}' >/dev/null
  status "restoring it restores access"        200 "$BASE/foods" -H "$AUTH2"

  echo "== instance settings"
  # The quorum is policy, not deployment configuration: it decides how this
  # community works, so it is changed from the admin area rather than by
  # editing a file and restarting a container.
  expect "the quorum is readable"      "$(curl -fsS "$BASE/admin/settings" -H "$AUTH" | j "['food_quorum'] >= 1")" "True"
  expect "and starts unconfigured"     "$(curl -fsS "$BASE/admin/settings" -H "$AUTH" | j "['updated_at'] is None")" "True"
  expect "so does each seeded setting" \
    "$(curl -fsS "$BASE/admin/settings" -H "$AUTH" | j "['food_quorum_updated_at'] is None and d['allow_registration_updated_at'] is None")" "True"
  status "an empty change is refused"  400 -X PUT "$BASE/admin/settings" -H "$AUTH" -H 'content-type: application/json' -d '{}'
  status "a non-admin cannot read it"  403 "$BASE/admin/settings" -H "$AUTH3"
  status "nor change it"               403 -X PUT "$BASE/admin/settings" -H "$AUTH3" -H 'content-type: application/json' -d '{"food_quorum":1}'
  status "and zero is refused"         400 -X PUT "$BASE/admin/settings" -H "$AUTH" -H 'content-type: application/json' -d '{"food_quorum":0}'

  # $VAR sits at one confirmation short of the default quorum of two, so
  # lowering the bar has to promote it without anyone voting again.
  curl -fsS -X POST "$BASE/foods/$VAR/verify" -H "$AUTH" -H 'content-type: application/json' -d '{"verdict":"confirm"}' >/dev/null
  expect "one confirmation is short at a quorum of two" \
    "$(curl -fsS "$BASE/foods/$VAR" -H "$AUTH" | j "['provenance']['status']")" "unverified"

  SETTINGS=$(curl -fsS -X PUT "$BASE/admin/settings" -H "$AUTH" -H 'content-type: application/json' -d '{"food_quorum":1}')
  expect "the change is recorded"      "$(echo "$SETTINGS" | j "['food_quorum']")" "1"
  expect "and attributed"              "$(echo "$SETTINGS" | j "['updated_by_name']")" "Smoke"
  expect "lowering it promotes what already had the support" \
    "$(curl -fsS "$BASE/foods/$VAR" -H "$AUTH" | j "['provenance']['status']")" "verified"
  expect "and the new quorum is what clients are told" \
    "$(curl -fsS "$BASE/foods/$VAR" -H "$AUTH" | j "['provenance']['quorum']")" "1"
  expect "a verified-only export sees it now" \
    "$(curl -fsS "$BASE/foods/export?verified_only=true" -H "$AUTH" | j "['count'] > 0")" "True"

  curl -fsS -X PUT "$BASE/admin/settings" -H "$AUTH" -H 'content-type: application/json' -d '{"food_quorum":3}' >/dev/null
  expect "raising it demotes what no longer clears the bar" \
    "$(curl -fsS "$BASE/foods/$VAR" -H "$AUTH" | j "['provenance']['status']")" "unverified"
  expect "stats report the live value" "$(curl -fsS "$BASE/admin/stats" -H "$AUTH" | j "['food_quorum']")" "3"
  curl -fsS -X PUT "$BASE/admin/settings" -H "$AUTH" -H 'content-type: application/json' -d '{"food_quorum":2}' >/dev/null

  # Deletion is closed to everyone else once a food is shared work, but an
  # administrator is the escape hatch for an entry that should not exist.
  JUNK=$(curl -fsS -X POST "$BASE/foods" -H "$AUTH2" -H 'content-type: application/json' -d '{"name":"Spam entry","calories_kcal":1,"protein_g":1,"carbs_g":1,"fat_g":1}' | j "['id']")
  curl -fsS -X PUT "$BASE/foods/$JUNK" -H "$AUTH3" -H 'content-type: application/json' -d '{"name":"Spam entry edited","calories_kcal":2,"protein_g":1,"carbs_g":1,"fat_g":1}' >/dev/null
  status "an administrator can remove a food outright" 204 -X DELETE "$BASE/foods/$JUNK" -H "$AUTH"

  echo "== registration"
  # "Close sign-ups once my household has joined" is the same kind of decision
  # as the quorum, so it lives in the same row and is read on every request
  # rather than at boot. Each seeded setting keeps its own marker: saving the
  # quorum above must not have taken this one over from the environment.
  expect "sign-ups start open"         "$(curl -fsS "$BASE/admin/settings" -H "$AUTH" | j "['allow_registration']")" "True"
  expect "and still unconfigured"      "$(curl -fsS "$BASE/admin/settings" -H "$AUTH" | j "['allow_registration_updated_at'] is None")" "True"
  CLOSED=$(curl -fsS -X PUT "$BASE/admin/settings" -H "$AUTH" -H 'content-type: application/json' -d '{"allow_registration":false}')
  expect "an administrator can close them" "$(echo "$CLOSED" | j "['allow_registration']")" "False"
  expect "which is recorded"           "$(echo "$CLOSED" | j "['allow_registration_updated_at'] is not None")" "True"
  expect "without touching the quorum" "$(echo "$CLOSED" | j "['food_quorum']")" "2"
  expect "the sign-in page is told"    "$(curl -fsS "$BASE/auth/registration" | j "['open']")" "False"
  status "and a new account is refused" 403 -X POST "$BASE/auth/register" -H 'content-type: application/json' -d "{\"email\":\"late-$RANDOM@example.test\",\"password\":\"$PASSWORD\",\"display_name\":\"Late\"}"
  status "while signing in still works" 200 -X POST "$BASE/auth/login" -H 'content-type: application/json' -d "{\"email\":\"$EMAIL3\",\"password\":\"$PASSWORD\"}"
  curl -fsS -X PUT "$BASE/admin/settings" -H "$AUTH" -H 'content-type: application/json' -d '{"allow_registration":true}' >/dev/null
  expect "reopening takes effect at once" "$(curl -fsS "$BASE/auth/registration" | j "['open']")" "True"
  status "and accounts can be created again" 201 -X POST "$BASE/auth/register" -H 'content-type: application/json' -d "{\"email\":\"late-$RANDOM@example.test\",\"password\":\"$PASSWORD\",\"display_name\":\"Late\"}"
else
  printf '  – skipped: this database already had accounts before the run\n'
fi

echo "== referential integrity"
# $OATS is untouched by anyone else, so the only thing standing between it and
# deletion is the recipe that references it.
status "food in use cannot be deleted"   400 -X DELETE "$BASE/foods/$OATS" -H "$AUTH"
status "recipe in use cannot be deleted" 400 -X DELETE "$BASE/recipes/$RID" -H "$AUTH"

echo "== openapi"
PATHS=$(curl -fsS "${BASE%/api/v1}/api/v1/openapi.json" | j " and len(d['paths'])")
if [ "$PATHS" -ge 57 ]; then pass "spec documents $PATHS paths"; else fail "spec only documents $PATHS paths"; fi

echo
if [ "$failures" -eq 0 ]; then
  echo "All checks passed."
else
  echo "$failures check(s) failed."
  exit 1
fi
