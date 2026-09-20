import type { Nutrients, RecipeSummary } from '@/api/types'

/** The four figures the live totals in the editor need. */
export type Macros = Pick<Nutrients, 'calories_kcal' | 'protein_g' | 'carbs_g' | 'fat_g'>

/**
 * An ingredient being edited.
 *
 * Both kinds are held as an amount times a per-unit figure — grams of a food,
 * or servings of a recipe — so the running totals are one multiplication and
 * never have to ask which kind a row is.
 */
export interface DraftItem {
  key: string
  kind: 'food' | 'recipe' | 'text'
  /** The food id or the sub-recipe id. Empty for a free-text ingredient,
   *  which points at nothing — that is what makes it free text. */
  refId: string
  name: string
  brand: string | null
  /** Grams for a food, servings for a recipe. */
  amount: number
  /** Nutrients in one gram of the food, or one serving of the recipe. */
  perUnit: Macros
  /** Grams in one unit: 1 for a food, and a serving's weight for a recipe. */
  gramsPerUnit: number
}

export const ZERO: Nutrients = {
  calories_kcal: 0,
  protein_g: 0,
  carbs_g: 0,
  fat_g: 0,
  fiber_g: 0,
  sugar_g: 0,
  saturated_fat_g: 0,
  sodium_mg: 0,
  net_carbs_g: 0,
}

/** What a food row needs to become a draft item: its identity and per-100 g figures. */
export interface FoodLike {
  id: string
  name: string
  brand: string | null
  calories_kcal: number
  protein_g: number
  carbs_g: number
  fat_g: number
}

let counter = 0
const fresh = (prefix: string) => `${prefix}-${Date.now()}-${counter++}`

export function foodItem(food: FoodLike, grams: number): DraftItem {
  return {
    key: fresh(food.id),
    kind: 'food',
    refId: food.id,
    name: food.name,
    brand: food.brand,
    amount: grams,
    // Foods are stored per 100 g; the draft works in per-gram so both kinds
    // of row share one multiplication.
    perUnit: {
      calories_kcal: food.calories_kcal / 100,
      protein_g: food.protein_g / 100,
      carbs_g: food.carbs_g / 100,
      fat_g: food.fat_g / 100,
    },
    gramsPerUnit: 1,
  }
}

export function textItem(label: string): DraftItem {
  return {
    key: fresh('text'),
    kind: 'text',
    refId: '',
    name: label,
    brand: null,
    // Nothing to scale and nothing to contribute. Carried as zeroes rather
    // than as a special case so the totals stay one multiplication.
    amount: 0,
    perUnit: { calories_kcal: 0, protein_g: 0, carbs_g: 0, fat_g: 0 },
    gramsPerUnit: 0,
  }
}

export function recipeItem(recipe: RecipeSummary): DraftItem {
  return {
    key: fresh(recipe.id),
    kind: 'recipe',
    refId: recipe.id,
    name: recipe.name,
    brand: null,
    amount: 1,
    perUnit: recipe.per_serving,
    gramsPerUnit: recipe.total_weight_g / recipe.servings,
  }
}
