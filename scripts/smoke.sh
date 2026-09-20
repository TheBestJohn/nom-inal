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
expect "database reachable" "$(curl -fsS "$BASE/health" | j "['database']")" "ok"

echo "== auth"
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
if [ "$PATHS" -ge 42 ]; then pass "spec documents $PATHS paths"; else fail "spec only documents $PATHS paths"; fi

echo
if [ "$failures" -eq 0 ]; then
  echo "All checks passed."
else
  echo "$failures check(s) failed."
  exit 1
fi
