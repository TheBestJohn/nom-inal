export interface Nutrients {
  calories_kcal: number
  protein_g: number
  carbs_g: number
  fat_g: number
  fiber_g: number
  sugar_g: number
  saturated_fat_g: number
  sodium_mg: number
  /** Carbs minus fibre, floored at zero. Derived by the server, never stored. */
  net_carbs_g: number
}

/** Share of energy from each macro, in percent. Always sums to 100, or all 0. */
export interface EnergyShare {
  protein_pct: number
  carbs_pct: number
  fat_pct: number
}

/**
 * Why the account is tracking. Null until the welcome flow has asked; `custom`
 * is the answer "none of these" and is never asked again.
 */
export type TrackingFocus =
  | 'general'
  | 'weight_loss'
  | 'muscle_gain'
  | 'keto'
  | 'diabetes'
  | 'blood_pressure'
  | 'heart_health'
  | 'custom'

export interface Profile {
  id: string
  email: string
  display_name: string
  sex: string | null
  birth_date: string | null
  height_cm: number | null
  activity_level: string
  goal: string
  target_weight_kg: number | null
  /** Whether to show the admin area. The server re-checks on every call. */
  is_admin: boolean
  /** Nutrients the macro readouts should show. Always resolved by the server. */
  shown_nutrients: Nutrient[]
  /** Nutrients the home page plots. */
  chart_nutrients: Nutrient[]
  /**
   * `percent` indexes each series to its own goal or budget and draws them on
   * one axis; `actual` keeps the real figures and gives each its own chart.
   */
  chart_mode: ChartMode
  tracking_focus: TrackingFocus | null
  /**
   * How body measurements are shown and typed. The API stays metric either
   * way: kg and cm go over the wire, and the client converts at the edge.
   * Food amounts are grams whatever this says.
   */
  units: Units
  created_at: string
}

/** Display preference for body weight and height. Storage is metric. */
export type Units = 'metric' | 'imperial'

/** Nutrients a target can be set on. Mirrors the server's vocabulary. */
export type Nutrient =
  | 'calories_kcal'
  | 'protein_g'
  | 'carbs_g'
  | 'net_carbs_g'
  | 'fat_g'
  | 'fiber_g'
  | 'sugar_g'
  | 'saturated_fat_g'
  | 'sodium_mg'

/**
 * Which way a target points.
 * - `goal`   — a floor. Hit at least this. Exceeding it is fine.
 * - `budget` — a ceiling. Stay under this. Exceeding it is over-budget.
 */
export type TargetKind = 'goal' | 'budget'

export interface NutritionTarget {
  nutrient: Nutrient
  amount: number
  kind: TargetKind
  label: string
  unit: string
}

export interface TargetProgress {
  nutrient: Nutrient
  label: string
  unit: string
  kind: TargetKind
  amount: number
  consumed: number
  /** Always `amount - consumed`, signed: negative means past the number. */
  remaining: number
  percent: number
  /** `under` | `over` for a budget, `short` | `met` for a goal. */
  status: 'under' | 'over' | 'short' | 'met'
}

export interface AuthResponse {
  access_token: string
  token_type: string
  expires_in: number
  user: Profile
}

export interface WeightEntry {
  id: string
  recorded_on: string
  weight_kg: number
  body_fat_pct: number | null
  note: string | null
  created_at: string
  updated_at: string
}

export interface WeightStats {
  count: number
  latest_kg: number | null
  earliest_kg: number | null
  change_kg: number | null
  min_kg: number | null
  max_kg: number | null
  moving_average_7_kg: number | null
}

export interface Food {
  id: string
  source: 'custom' | 'usda' | 'off' | string
  source_id: string | null
  name: string
  brand: string | null
  upc: string | null
  calories_kcal: number
  protein_g: number
  carbs_g: number
  fat_g: number
  fiber_g: number | null
  sugar_g: number | null
  saturated_fat_g: number | null
  sodium_mg: number | null
  serving_size_g: number
  serving_label: string | null
  /**
   * Which basis these numbers read best in. Storage is always per 100 g — this
   * says how the food was entered and how it should be shown.
   */
  nutrient_basis: NutrientBasis
  /** Set when this row is a preparation variant of another food. */
  variant_of: string | null
  /** `cooked`, `raw`, `drained` — present exactly when `variant_of` is. */
  variant_label: string | null
  /** Bumped by the server on every substantive edit. */
  revision: number
  verified_at: string | null
  /** Set when someone objected to the current numbers and nobody outvoted them. */
  disputed_at: string | null
  created_by: string | null
  created_at: string
  updated_at: string
  /**
   * Household measures — "1 cup", "1 slice" — each with its weight, offered
   * beside grams when logging. What gets logged is still grams.
   */
  portions: FoodPortion[]
}

export interface FoodPortion {
  id: string
  label: string
  grams: number
  /** `usda` | `off` for a provider's measure, `user` for one typed in. */
  source: 'usda' | 'off' | 'user' | string
}

/** How the home page plots the nutrients you follow. */
export type ChartMode = 'percent' | 'actual'

/** Which quantity a set of nutrient figures describes. */
export type NutrientBasis = 'per_100g' | 'per_serving'

/** How much the community trusts a food's *current* revision. */
export type VerificationStatus = 'unverified' | 'verified' | 'disputed'

export type Verdict = 'confirm' | 'dispute'

export interface FoodProvenance {
  revision: number
  status: VerificationStatus
  confirmations: number
  disputes: number
  /** Net confirmations needed for `verified`, so "1 of 2" needs no guessing. */
  quorum: number
  verified_at: string | null
  contributors: number
  last_change_kind: string
  last_edited_at: string
  last_edited_by: string | null
  last_edited_by_name: string | null
  last_edit_summary: string | null
  your_verdict: Verdict | null
  /** False when you wrote the current revision. */
  can_verify: boolean
}

export interface FoodRevision {
  id: string
  food_id: string
  revision: number
  change_kind: 'create' | 'edit' | 'import' | 'revert' | 'seed' | string
  edited_by: string | null
  edited_by_name: string | null
  summary: string | null
  snapshot: Record<string, unknown>
  created_at: string
  /** Fields whose value differs from the revision before this one. */
  changed_fields: string[]
}

export interface FoodVerification {
  user_id: string
  display_name: string
  revision: number
  verdict: Verdict
  note: string | null
  /** False for a vote on an older revision: kept, but no longer counted. */
  current: boolean
}

/**
 * `GET /foods/{id}` flattens the food and adds the computed per-serving block,
 * its editorial state, and whichever side of the variant relationship applies.
 */
export type FoodDetail = Food & {
  per_serving: Nutrients
  provenance: FoodProvenance
  variants: Food[]
  parent: Food | null
}

export interface ApiKey {
  id: string
  name: string
  prefix: string
  scopes: ('read' | 'write')[]
  last_used_at: string | null
  expires_at: string | null
  revoked_at: string | null
  created_at: string
}

/** The only shape that ever carries the token, returned once at creation. */
export type CreatedApiKey = ApiKey & { token: string }

export interface AdminUserRow {
  id: string
  email: string
  display_name: string
  is_admin: boolean
  disabled_at: string | null
  created_at: string
  diary_entries: number
  weigh_ins: number
  foods_created: number
  food_edits: number
  active_api_keys: number
  last_activity_at: string | null
}

export interface InstanceSettings {
  food_quorum: number
  /** Null while the quorum is still at its installation default. */
  food_quorum_updated_at: string | null
  /** Whether new accounts may be created. An empty instance admits its first regardless. */
  allow_registration: boolean
  /** Null while sign-ups are still at their installation default. */
  allow_registration_updated_at: string | null
  /** When anything here was last saved from the admin area, and by whom. */
  updated_at: string | null
  updated_by_name: string | null
}

/** Whether `POST /auth/register` would accept a new account right now. */
export interface RegistrationStatus {
  open: boolean
}

export interface AdminStats {
  users: number
  admins: number
  disabled_users: number
  foods: number
  food_variants: number
  foods_verified: number
  foods_disputed: number
  food_revisions: number
  recipes: number
  public_recipes: number
  diary_entries: number
  weigh_ins: number
  photos: number
  active_api_keys: number
  food_quorum: number
}

export interface ExternalFood {
  source: string
  source_id: string
  name: string
  brand: string | null
  upc: string | null
  calories_kcal: number
  protein_g: number
  carbs_g: number
  fat_g: number
  fiber_g: number | null
  sugar_g: number | null
  saturated_fat_g: number | null
  sodium_mg: number | null
  serving_size_g: number
  serving_label: string | null
  /** Provider portions, when the record carried them. Optional on the way in. */
  portions?: { label: string; grams: number }[]
}

export interface ExternalSearchResponse {
  results: ExternalFood[]
  unavailable: string[]
}

export interface BarcodeLookup {
  upc: string
  local: FoodDetail | null
  external: ExternalFood | null
}

/** The part of a recipe the picker needs to log it. */
export interface RecentRecipe {
  id: string
  name: string
  servings: number
  per_serving: Nutrients
  untracked_count: number
}

/**
 * Something logged before, with how it was logged last time. Exactly one of
 * `food` and `recipe` is set.
 */
export interface RecentItem {
  food: Food | null
  recipe: RecentRecipe | null
  last_quantity_g: number | null
  last_recipe_servings: number | null
  last_logged_on: string
  times_logged: number
}

export interface CopyDiaryResult {
  from_date: string
  to_date: string
  meal: string | null
  /** Zero means the source had nothing to copy. */
  copied: number
  entries: DiaryEntry[]
}

export interface RecipeItem {
  id: string
  /** Set when this ingredient is a food. Exactly one of these two is set. */
  food_id: string | null
  /** Set when this ingredient is another recipe, taken in servings. */
  sub_recipe_id: string | null
  /** Set when this ingredient is just words, with no nutrition attached. */
  label: string | null
  name: string
  brand: string | null
  /** For a food that is a preparation variant: "cooked", "drained". */
  variant_label: string | null
  quantity_g: number | null
  servings: number | null
  /** Grams for a food; for a sub-recipe, the weight of the servings taken. */
  weight_g: number
  note: string | null
  sort_order: number
  nutrients: Nutrients
}

export interface RecipeSummary {
  id: string
  /**
   * The word this recipe's shared link is spelled with, from its name.
   * Unique across the instance; a rename issues a new one and the old one
   * keeps resolving, so a link already sent never rots.
   */
  slug: string
  name: string
  description: string | null
  servings: number
  /** Shared with every account when true. */
  is_public: boolean
  /** Only an owner may edit or delete. */
  is_owner: boolean
  /** Set on recipes you do not own. */
  author: string | null
  total_weight_g: number
  item_count: number
  /** Ingredients, counted through nesting, that carry no nutrition. */
  untracked_count: number
  /** The first photo uploaded, for the card. Needs the auth header like any photo. */
  cover_photo_url: string | null
  per_serving: Nutrients
  created_at: string
  updated_at: string
}

export interface Recipe {
  id: string
  /** See `RecipeSummary.slug`. */
  slug: string
  name: string
  description: string | null
  instructions: string | null
  servings: number
  is_public: boolean
  is_owner: boolean
  author: string | null
  total_weight_g: number
  /** Ingredients, counted through nesting, that carry no nutrition. */
  untracked_count: number
  items: RecipeItem[]
  total: Nutrients
  per_serving: Nutrients
  created_at: string
  updated_at: string
}

/**
 * A shared recipe as anyone holding the link sees it, no token needed. The
 * photo URLs point at the public photo route, which serves a photo only
 * while its recipe is shared.
 */
export type PublicRecipe = Recipe & { photos: Photo[] }

/** A food the importer thinks an ingredient line might mean. */
export interface DraftCandidate {
  food_id: string
  name: string
  brand: string | null
  serving_size_g: number
  /** Per 100 g. */
  calories_kcal: number
  protein_g: number
  carbs_g: number
  fat_g: number
  /** How it was found. Only `exact` and `prefix` are tight enough to pre-select. */
  tier: 'exact' | 'prefix' | 'contains' | 'fuzzy' | string
}

/** One ingredient line off an imported page, as written and as read. */
export interface DraftLine {
  text: string
  quantity: number | null
  unit: string | null
  name: string
  /** Set when the line said a mass; null for a cup, a clove, a large. */
  grams: number | null
  candidates: DraftCandidate[]
}

/** What `POST /recipes/import` hands back. Nothing is saved yet. */
export interface RecipeDraft {
  name: string
  description: string | null
  servings: number | null
  instructions: string | null
  lines: DraftLine[]
  source_url: string
  image_url: string | null
  author: string | null
}

export interface MergeCount {
  created: number
  updated: number
  skipped: number
}

/** What an account import did, and everything it could not do as asked. */
export interface ImportReport {
  profile_updated: boolean
  targets: MergeCount
  reminders: MergeCount
  foods: MergeCount
  recipes: MergeCount
  diary: MergeCount
  weights: MergeCount
  notes: string[]
}

export interface DiaryEntry {
  id: string
  logged_on: string
  meal: string
  food_id: string | null
  recipe_id: string | null
  name: string
  brand: string | null
  quantity_g: number | null
  recipe_servings: number | null
  nutrients: Nutrients
  created_at: string
  updated_at: string
}

export interface MealGroup {
  meal: string
  entries: DiaryEntry[]
  total: Nutrients
}

export interface DiaryDay {
  date: string
  meals: MealGroup[]
  total: Nutrients
  energy_share: EnergyShare
  /** Progress against each target that is set, in display order. */
  targets: TargetProgress[]
  /** "I logged everything": only complete days feed the adaptive estimate. */
  complete: boolean
}

/** The flag as stored, from `PUT /diary/day/{date}/complete`. */
export interface DayCompletion {
  date: string
  complete: boolean
  updated_at: string
}

export interface DailyTotal {
  date: string
  total: Nutrients
  /** Zero for a complete day with nothing logged — a fast day, listed because it is data. */
  entry_count: number
  complete: boolean
}

export interface DiarySummary {
  from: string
  to: string
  days: DailyTotal[]
  average: Nutrients
  energy_share: EnergyShare
  /** Days with at least one entry; the average is over these. */
  logged_day_count: number
  /** Days marked "I logged everything", with or without entries. */
  complete_day_count: number
}

/** What a window held against what an estimate needs; the same shape for both. */
export interface Evidence {
  complete_days: number
  weigh_ins: number
  span_days: number
}

export type Confidence = 'low' | 'moderate' | 'good'

/** Expenditure by energy balance, with everything it was made from. */
export interface AdaptiveEstimate {
  tdee_kcal: number
  mean_intake_kcal: number
  /** `mean_intake_kcal − tdee_kcal`: negative in a deficit. */
  energy_balance_kcal_per_day: number
  /** Negative when losing. */
  weight_change_kg_per_week: number
  slope_kg_per_day: number
  trend_start_kg: number
  trend_end_kg: number
  first_weigh_in: string
  last_weigh_in: string
  confidence: Confidence
  /** The profile goal `budget_kcal` is adjusted for, and by how much. */
  goal: string
  goal_adjustment_kcal: number
  /** `tdee_kcal + goal_adjustment_kcal`, to the nearest ten, never under 1200. */
  budget_kcal: number
  floored_at_minimum: boolean
}

/**
 * `GET /estimates/tdee`. `ready` is the field to branch on: `estimate` is
 * present exactly when it is true, `reason` exactly when it is not.
 */
export interface TdeeEstimate {
  ready: boolean
  reason: string | null
  from: string
  to: string
  days: number
  have: Evidence
  need: Evidence
  estimate: AdaptiveEstimate | null
  /** The profile formula, for comparison. */
  formula: EnergyEstimate | null
  formula_missing: string[]
}

export interface TrendEvidence {
  weigh_ins: number
  span_days: number
}

export interface TrendSummary {
  /** The last weigh-in in the window; every projection counts from here. */
  as_of: string
  /** The fitted weight on `as_of`. */
  current_kg: number
  first_weigh_in: string
  start_kg: number
  rate_kg_per_week: number
  slope_kg_per_day: number
  /** Past about 1 % of body weight a week. */
  caution: boolean
  caution_threshold_kg_per_week: number
}

export interface ByDatePlan {
  date: string
  from: string
  days: number
  required_rate_kg_per_week: number
  /** Change from expenditure, per day: negative is a deficit. */
  daily_energy_change_kcal: number
  /** `adaptive` or `formula`; null when neither estimate could be made. */
  basis: 'adaptive' | 'formula' | null
  basis_tdee_kcal: number | null
  /** `basis_tdee_kcal + daily_energy_change_kcal`, to the nearest ten, never under 1200. */
  suggested_intake_kcal: number | null
  floored_at_minimum: boolean
  caution: boolean
}

export type ReachedReason = 'not_ready' | 'no_target_weight' | 'trend_is_flat' | 'trend_points_away'
export type ByReason = 'not_ready' | 'no_target_weight' | 'date_not_after_as_of'

/** `GET /estimates/projection`. Each absent part carries its own reason. */
export interface Projection {
  ready: boolean
  reason: string | null
  from: string
  to: string
  days: number
  have: TrendEvidence
  need: TrendEvidence
  target_weight_kg: number | null
  trend: TrendSummary | null
  reached_on: string | null
  days_to_target: number | null
  reached_reason: ReachedReason | null
  by: ByDatePlan | null
  by_reason: ByReason | null
}

/** The server's energy estimate, with its inputs echoed. */
export interface EnergyEstimate {
  sex_assumed_male: boolean
  age_years: number
  height_cm: number
  weight_kg: number
  weight_source: 'weigh_in' | 'target_weight' | string
  activity_level: string
  activity_factor: number
  goal: string
  goal_adjustment_kcal: number
  bmr_kcal: number
  tdee_kcal: number
  calories_kcal: number
  floored_at_minimum: boolean
}

/** One target as a preset would write it. `amount` is null when `needs` is not empty. */
export interface PreviewTarget {
  nutrient: Nutrient
  label: string
  unit: string
  kind: TargetKind
  amount: number | null
  rationale: string
  /** Profile fields this rule needs and does not have. */
  needs: string[]
}

export interface FocusPreview {
  focus: TrackingFocus
  label: string
  summary: string
  targets: PreviewTarget[]
  shown_nutrients: Nutrient[]
  chart_nutrients: Nutrient[]
  chart_mode: ChartMode
  /** The profile goal this focus sets, when it implies one. */
  goal: string | null
  missing: string[]
  estimate: EnergyEstimate | null
  changes_display: boolean
}

export interface FocusOption {
  focus: TrackingFocus
  label: string
  summary: string
}

export interface SetFocusResponse {
  profile: Profile
  applied: FocusPreview | null
}

export interface TargetSuggestion {
  estimate: EnergyEstimate | null
  missing: string[]
  targets: PreviewTarget[]
}

export interface Health {
  status: string
  version: string
  /** The commit the image was built from. Null for a source build nobody stamped. */
  git_sha: string | null
  /** RFC 3339 UTC, on the same terms. */
  built_at: string | null
  database: string
  usda_configured: boolean
}

export interface Photo {
  id: string
  /** Set on a progress photo. Exactly one of this and `recipe_id` is set. */
  weight_entry_id: string | null
  /** Set on a recipe photo. */
  recipe_id: string | null
  content_type: string
  byte_size: number
  width: number
  height: number
  caption: string | null
  created_at: string
  /** Served by the API, not as a static file — needs the auth header. */
  url: string
}

export type ReminderKind = 'weigh_in' | 'food_log' | 'progress_photo'

export interface Reminder {
  kind: ReminderKind
  label: string
  every_days: number
  enabled: boolean
}

export interface ReminderStatus {
  kind: ReminderKind
  label: string
  every_days: number
  enabled: boolean
  last_on: string | null
  days_since: number | null
  due: boolean
  overdue_days: number
  /** Pre-worded by the server so every client says the same thing. */
  message: string
}
