import { request } from './client'
import type {
  AdminStats,
  AdminUserRow,
  ApiKey,
  AuthResponse,
  InstanceSettings,
  BarcodeLookup,
  DiaryDay,
  DiaryEntry,
  DiarySummary,
  ExternalFood,
  ExternalSearchResponse,
  CreatedApiKey,
  Food,
  FoodDetail,
  FoodRevision,
  FoodVerification,
  Health,
  Nutrient,
  NutrientBasis,
  NutritionTarget,
  Photo,
  Reminder,
  ReminderKind,
  ReminderStatus,
  Profile,
  Recipe,
  RecipeSummary,
  RegistrationStatus,
  WeightEntry,
  TargetKind,
  Verdict,
  WeightStats,
} from './types'

export interface RecipeItemInput {
  food_id?: string | null
  /**
   * Include another recipe as an ingredient. Linked, not copied: correcting
   * that recipe later updates everything built on it.
   */
  sub_recipe_id?: string | null
  /**
   * A one-off ingredient that is just words — "salt and pepper to taste". No
   * nutrition and no database entry; the recipe reports how many it has.
   */
  label?: string | null
  /** For a food. */
  quantity_g?: number | null
  /** For a sub-recipe. */
  servings?: number | null
  note?: string | null
}

export interface RecipeInput {
  name: string
  description?: string | null
  instructions?: string | null
  servings: number
  /** Private by default; true shares it with every account. */
  is_public?: boolean
  items: RecipeItemInput[]
}

export interface TargetInput {
  nutrient: Nutrient
  amount: number
  kind: TargetKind
}

export interface FoodInput {
  name: string
  brand?: string | null
  upc?: string | null
  calories_kcal: number
  protein_g: number
  carbs_g: number
  fat_g: number
  fiber_g?: number | null
  sugar_g?: number | null
  saturated_fat_g?: number | null
  sodium_mg?: number | null
  serving_size_g: number
  serving_label?: string | null
  /**
   * What the figures above describe. The server converts to per 100 g for
   * storage, so a client can post exactly what a label says.
   */
  nutrient_basis?: NutrientBasis
  /** Makes this food a preparation variant of another. */
  variant_of?: string | null
  /** Required with `variant_of`, rejected without it. */
  variant_label?: string | null
  /** Stored on the revision this write creates, not on the food. */
  edit_summary?: string | null
}

function uploadPhotoTo(path: string, file: File, caption?: string) {
  const form = new FormData()
  form.append('file', file)
  if (caption) form.append('caption', caption)
  // No Content-Type header: the browser has to set it itself so the
  // multipart boundary matches the body it generates.
  return request<Photo>(path, { method: 'POST', form })
}

export const api = {
  health: () => request<Health>('/health'),

  register: (body: { email: string; password: string; display_name: string }) =>
    request<AuthResponse>('/auth/register', { method: 'POST', body }),
  /** Public: the sign-in page asks before offering "create an account". */
  registrationStatus: () => request<RegistrationStatus>('/auth/registration'),
  login: (body: { email: string; password: string }) =>
    request<AuthResponse>('/auth/login', { method: 'POST', body }),
  me: () => request<Profile>('/auth/me'),

  getProfile: () => request<Profile>('/profile'),
  updateProfile: (body: Partial<Profile>) =>
    request<Profile>('/profile', { method: 'PATCH', body }),

  listTargets: () => request<NutritionTarget[]>('/targets'),
  replaceTargets: (targets: TargetInput[]) =>
    request<NutritionTarget[]>('/targets', { method: 'PUT', body: { targets } }),
  deleteTarget: (nutrient: Nutrient) => request<void>(`/targets/${nutrient}`, { method: 'DELETE' }),

  listPhotos: (weightEntryId: string) => request<Photo[]>(`/weights/${weightEntryId}/photos`),
  uploadPhoto: (weightEntryId: string, file: File, caption?: string) =>
    uploadPhotoTo(`/weights/${weightEntryId}/photos`, file, caption),
  listRecipePhotos: (recipeId: string) => request<Photo[]>(`/recipes/${recipeId}/photos`),
  uploadRecipePhoto: (recipeId: string, file: File, caption?: string) =>
    uploadPhotoTo(`/recipes/${recipeId}/photos`, file, caption),
  deletePhoto: (id: string) => request<void>(`/photos/${id}`, { method: 'DELETE' }),

  listReminders: () => request<Reminder[]>('/reminders'),
  reminderStatus: () => request<ReminderStatus[]>('/reminders/status'),
  replaceReminders: (reminders: { kind: ReminderKind; every_days: number; enabled: boolean }[]) =>
    request<Reminder[]>('/reminders', { method: 'PUT', body: { reminders } }),

  listWeights: (query: { from?: string; to?: string; limit?: number } = {}) =>
    request<WeightEntry[]>('/weights', { query }),
  weightStats: (query: { from?: string; to?: string } = {}) =>
    request<WeightStats>('/weights/stats', { query }),
  logWeight: (body: {
    recorded_on?: string
    weight_kg: number
    body_fat_pct?: number | null
    note?: string | null
  }) => request<WeightEntry>('/weights', { method: 'POST', body }),
  deleteWeight: (id: string) => request<void>(`/weights/${id}`, { method: 'DELETE' }),

  listFoods: (query: { q?: string; source?: string; mine?: boolean; limit?: number } = {}) =>
    request<Food[]>('/foods', { query }),
  getFood: (id: string) => request<FoodDetail>(`/foods/${id}`),
  createFood: (body: FoodInput) => request<FoodDetail>('/foods', { method: 'POST', body }),
  updateFood: (id: string, body: FoodInput) =>
    request<FoodDetail>(`/foods/${id}`, { method: 'PUT', body }),
  deleteFood: (id: string) => request<void>(`/foods/${id}`, { method: 'DELETE' }),
  searchExternal: (q: string, limit = 15) =>
    request<ExternalSearchResponse>('/foods/search/external', { query: { q, limit } }),
  lookupBarcode: (upc: string) => request<BarcodeLookup>(`/foods/barcode/${upc}`),
  importFood: (body: ExternalFood) =>
    request<FoodDetail>('/foods/import', { method: 'POST', body }),

  foodRevisions: (id: string) => request<FoodRevision[]>(`/foods/${id}/revisions`),
  foodVerifications: (id: string) => request<FoodVerification[]>(`/foods/${id}/verify`),
  verifyFood: (id: string, verdict: Verdict, note?: string) =>
    request<FoodDetail>(`/foods/${id}/verify`, { method: 'POST', body: { verdict, note } }),
  withdrawVerification: (id: string) =>
    request<FoodDetail>(`/foods/${id}/verify`, { method: 'DELETE' }),
  revertFood: (id: string, revision: number, reason?: string) =>
    request<FoodDetail>(`/foods/${id}/revert`, { method: 'POST', body: { revision, reason } }),
  exportFoodsUrl: (verifiedOnly: boolean) =>
    `/api/v1/foods/export${verifiedOnly ? '?verified_only=true' : ''}`,

  listApiKeys: () => request<ApiKey[]>('/keys'),
  createApiKey: (body: { name: string; scopes?: ('read' | 'write')[]; expires_in_days?: number }) =>
    request<CreatedApiKey>('/keys', { method: 'POST', body }),
  revokeApiKey: (id: string) => request<ApiKey>(`/keys/${id}`, { method: 'DELETE' }),

  adminStats: () => request<AdminStats>('/admin/stats'),
  adminUsers: (query: { q?: string; include_disabled?: boolean } = {}) =>
    request<AdminUserRow[]>('/admin/users', { query }),
  adminPatchUser: (id: string, body: { is_admin?: boolean; disabled?: boolean }) =>
    request<AdminUserRow>(`/admin/users/${id}`, { method: 'PATCH', body }),
  adminSettings: () => request<InstanceSettings>('/admin/settings'),
  /** Omitted fields are left alone, so each card saves only its own setting. */
  updateAdminSettings: (body: { food_quorum?: number; allow_registration?: boolean }) =>
    request<InstanceSettings>('/admin/settings', { method: 'PUT', body }),

  listRecipes: (query: { q?: string; scope?: 'mine' | 'public' | 'all' } = {}) =>
    request<RecipeSummary[]>('/recipes', { query }),
  getRecipe: (id: string) => request<Recipe>(`/recipes/${id}`),
  createRecipe: (body: RecipeInput) => request<Recipe>('/recipes', { method: 'POST', body }),
  updateRecipe: (id: string, body: RecipeInput) =>
    request<Recipe>(`/recipes/${id}`, { method: 'PUT', body }),
  deleteRecipe: (id: string) => request<void>(`/recipes/${id}`, { method: 'DELETE' }),

  diaryDay: (date: string) => request<DiaryDay>('/diary/day', { query: { date } }),
  diarySummary: (from: string, to: string) =>
    request<DiarySummary>('/diary/summary', { query: { from, to } }),
  logDiaryEntry: (body: {
    logged_on?: string
    meal?: string
    food_id?: string
    recipe_id?: string
    quantity_g?: number
    recipe_servings?: number
  }) => request<DiaryEntry>('/diary', { method: 'POST', body }),
  updateDiaryEntry: (
    id: string,
    body: { quantity_g?: number; recipe_servings?: number; meal?: string; logged_on?: string },
  ) => request<DiaryEntry>(`/diary/${id}`, { method: 'PATCH', body }),
  deleteDiaryEntry: (id: string) => request<void>(`/diary/${id}`, { method: 'DELETE' }),
}
