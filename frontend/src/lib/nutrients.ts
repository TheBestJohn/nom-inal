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
    key: 'net_carbs_g',
    label: 'Net carbs',
    short: 'Net C',
    unit: 'g',
    color: 'var(--netcarbs)',
    defaultKind: 'budget',
    hint: 'carbs − fiber',
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

/**
 * Complete a total the client has assembled itself.
 *
 * The server derives net carbs at the one place a total is serialised, so a
 * food, a recipe and a day cannot disagree about it. The few previews the
 * client computes before anything is saved — grams of a food in the picker,
 * a recipe being edited — derive it here, the same way, rather than each
 * carrying its own subtraction.
 */
export const withNetCarbs = (n: Omit<Nutrients, 'net_carbs_g'>): Nutrients => ({
  ...n,
  net_carbs_g: Math.max(0, n.carbs_g - n.fiber_g),
})

/** Read one nutrient out of a totals object by key. */
export const nutrientValue = (n: Nutrients, key: Nutrient): number => n[key] ?? 0

/**
 * A dash pattern per series, for the combined percent chart.
 *
 * Colour alone cannot carry seven series: run the palette validator over every
 * pair and protein and fat come out at ΔE 0.2 under deuteranopia — one line to
 * a red-green colourblind reader. Hueing around it does not work at this many
 * series, which is why the guidance says to cut, facet, or add a second
 * encoding. This is the second encoding, and it survives greyscale printing
 * and forced-colors mode too.
 */
export const DASH_PATTERNS = [
  undefined, // solid — the first series reads as the primary one
  '6 3',
  '2 3',
  '10 4',
  '6 3 2 3',
  '1 4',
  '12 3 2 3',
  '4 2 1 2',
] as const

export const dashFor = (index: number) => DASH_PATTERNS[index % DASH_PATTERNS.length]
