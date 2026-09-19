import type { Nutrient, Nutrients, TargetKind } from '@/api/types'

/**
 * Everything the UI needs to know about a nutrient, in one place.
 *
 * This list existed inside the targets editor, and the labels a second time in
 * the revision-diff helper. Three screens now need the same vocabulary — the
 * readouts, the home charts and the display settings — and a nutrient the
 * server knows about but one of them has forgotten would show up as a blank
 * column rather than an error. One array, in the order everything displays in.
 */
export interface NutrientMeta {
  key: Nutrient
  label: string
  /** For the tight readout row, where "Saturated fat 12 g" will not fit. */
  short: string
  unit: string
  /**
   * A theme variable, not a hex value: charts take colours as values rather
   * than classes, so they have to come from the same tokens the rest of the UI
   * uses or dark mode silently stops matching.
   */
  color: string
  defaultKind: TargetKind
  hint?: string
}

export const NUTRIENTS: NutrientMeta[] = [
  {
    key: 'calories_kcal',
    label: 'Calories',
    short: 'kcal',
    unit: 'kcal',
    color: 'var(--kcal)',
    defaultKind: 'budget',
  },
  {
    key: 'protein_g',
    label: 'Protein',
    short: 'P',
    unit: 'g',
    color: 'var(--protein)',
    defaultKind: 'goal',
  },
  {
    key: 'carbs_g',
    label: 'Carbs',
    short: 'C',
    unit: 'g',
    color: 'var(--carbs)',
    defaultKind: 'budget',
  },
  {
    key: 'fat_g',
    label: 'Fat',
    short: 'F',
    unit: 'g',
    color: 'var(--fat)',
    defaultKind: 'budget',
  },
  {
    key: 'fiber_g',
    label: 'Fiber',
    short: 'Fib',
    unit: 'g',
    color: 'var(--fiber)',
    defaultKind: 'goal',
    hint: '25–38 g is typical',
  },
  {
    key: 'sugar_g',
    label: 'Sugar',
    short: 'Sug',
    unit: 'g',
    color: 'var(--sugar)',
    defaultKind: 'budget',
  },
  {
    key: 'saturated_fat_g',
    label: 'Saturated fat',
    short: 'Sat',
    unit: 'g',
    color: 'var(--satfat)',
    defaultKind: 'budget',
  },
  {
    key: 'sodium_mg',
    label: 'Sodium',
    short: 'Na',
    unit: 'mg',
    color: 'var(--sodium)',
    defaultKind: 'budget',
    hint: '2300 mg is the usual cap',
  },
]

const BY_KEY = new Map(NUTRIENTS.map((n) => [n.key, n]))

export const nutrientMeta = (key: Nutrient) => BY_KEY.get(key)

/**
 * The chosen nutrients, in display order, ignoring anything unrecognised.
 *
 * The server already normalises what it stores, but a client that is a version
 * behind its API would otherwise render an empty slot for a nutrient it has
 * never heard of.
 */
export const orderNutrients = (chosen: Nutrient[]): NutrientMeta[] =>
  NUTRIENTS.filter((n) => chosen.includes(n.key))

/** Read one nutrient out of a totals object by key. */
export const nutrientValue = (n: Nutrients, key: Nutrient): number => n[key] ?? 0
