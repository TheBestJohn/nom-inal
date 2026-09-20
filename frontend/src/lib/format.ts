/** Formatting helpers kept in one place so units read the same on every screen. */

import type { Units } from '@/api/types'

export const kcal = (v: number | null | undefined) =>
  v === null || v === undefined ? '—' : `${Math.round(v).toLocaleString()} kcal`

export const grams = (v: number | null | undefined, digits = 1) =>
  v === null || v === undefined ? '—' : `${round(v, digits)} g`

export const kg = (v: number | null | undefined, digits = 1) =>
  v === null || v === undefined ? '—' : `${round(v, digits)} kg`

export const round = (v: number, digits = 1) => {
  const factor = 10 ** digits
  return Math.round(v * factor) / factor
}

export const signed = (v: number | null | undefined, digits = 1) => {
  if (v === null || v === undefined) return '—'
  const r = round(v, digits)
  return r > 0 ? `+${r}` : `${r}`
}

export const today = () => toISODate(new Date())

export function toISODate(date: Date): string {
  // Local calendar date, not UTC: logging dinner at 9pm should not land on
  // tomorrow for anyone east of Greenwich.
  const y = date.getFullYear()
  const m = String(date.getMonth() + 1).padStart(2, '0')
  const d = String(date.getDate()).padStart(2, '0')
  return `${y}-${m}-${d}`
}

export function addDays(iso: string, days: number): string {
  const [y, m, d] = iso.split('-').map(Number)
  const date = new Date(y, m - 1, d)
  date.setDate(date.getDate() + days)
  return toISODate(date)
}

export function prettyDate(iso: string): string {
  const [y, m, d] = iso.split('-').map(Number)
  const date = new Date(y, m - 1, d)
  const t = today()
  if (iso === t) return 'Today'
  if (iso === addDays(t, -1)) return 'Yesterday'
  if (iso === addDays(t, 1)) return 'Tomorrow'
  return date.toLocaleDateString(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    year: date.getFullYear() === new Date().getFullYear() ? undefined : 'numeric',
  })
}

export const shortDate = (iso: string) => {
  const [, m, d] = iso.split('-').map(Number)
  return `${m}/${d}`
}

export const titleCase = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

export const sourceLabel = (source: string) =>
  source === 'usda' ? 'USDA' : source === 'off' ? 'Open Food Facts' : 'Custom'

/**
 * Units are a display preference; storage is metric.
 *
 * Everything below converts at the edge: kilograms and centimetres come off
 * the wire, are shown in the preferred unit, and whatever is typed goes back
 * as kilograms and centimetres. The API never sees a pound. Food amounts are
 * deliberately not here — a diary entry is grams whatever the preference says,
 * and the cases people mean ("a cup", "a slice") are household portions kept
 * on the food, not a unit switch.
 */
export const LB_PER_KG = 2.2046226218
export const CM_PER_IN = 2.54
export const kgToLb = (v: number) => v * LB_PER_KG
export const lbToKg = (v: number) => v / LB_PER_KG
export const cmToIn = (v: number) => v / CM_PER_IN
export const inToCm = (v: number) => v * CM_PER_IN

/** Body weight in the preferred unit: "82.5 kg" or "181.9 lb". */
export const weight = (kg: number | null | undefined, units: Units, digits = 1) =>
  kg === null || kg === undefined
    ? '—'
    : units === 'imperial'
      ? `${round(kgToLb(kg), digits)} lb`
      : `${round(kg, digits)} kg`

/** A signed weight change in the preferred unit: "-1.5 kg", "+2.2 lb". */
export const weightChange = (kg: number | null | undefined, units: Units, digits = 1) =>
  kg === null || kg === undefined
    ? '—'
    : units === 'imperial'
      ? `${signed(kgToLb(kg), digits)} lb`
      : `${signed(kg, digits)} kg`

/** A bare weight figure in the preferred unit, for inputs and chart series. */
export const weightValue = (kg: number, units: Units, digits = 2) =>
  round(units === 'imperial' ? kgToLb(kg) : kg, digits)

/** What was typed in the preferred unit, as the kilograms the API stores. */
export const weightToKg = (value: number, units: Units) =>
  units === 'imperial' ? lbToKg(value) : value

/** Height in the preferred unit: "180 cm" or "5 ft 11 in". */
export const height = (cm: number | null | undefined, units: Units) => {
  if (cm === null || cm === undefined) return '—'
  if (units !== 'imperial') return `${round(cm, 0)} cm`
  const { feet, inches } = cmToFtIn(cm)
  return `${feet} ft ${inches} in`
}

/** Centimetres as whole feet and the remaining inches, the way a height is said. */
export function cmToFtIn(cm: number): { feet: number; inches: number } {
  const totalInches = Math.round(cmToIn(cm))
  return { feet: Math.floor(totalInches / 12), inches: totalInches % 12 }
}

export const ftInToCm = (feet: number, inches: number) => inToCm(feet * 12 + inches)

/** The unit word alone, for labels: "kg" or "lb". */
export const weightUnit = (units: Units) => (units === 'imperial' ? 'lb' : 'kg')

/**
 * "3 weeks ago", from a timestamp.
 *
 * Revision histories are read as a sequence of events, and an absolute
 * timestamp makes the reader do the subtraction themselves. `Intl` handles the
 * pluralisation and the locale, so this only has to pick a unit.
 */
const RELATIVE_UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['week', 7 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
]

export function relativeTime(iso: string): string {
  const seconds = (Date.parse(iso) - Date.now()) / 1000
  const magnitude = Math.abs(seconds)
  if (magnitude < 45) return 'just now'

  const formatter = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
  for (const [unit, size] of RELATIVE_UNITS) {
    if (magnitude >= size) return formatter.format(Math.round(seconds / size), unit)
  }
  return formatter.format(Math.round(seconds), 'second')
}

/** Column name to the label the food form uses, for revision diffs. */
const FIELD_LABELS: Record<string, string> = {
  name: 'Name',
  brand: 'Brand',
  upc: 'UPC',
  calories_kcal: 'Calories',
  protein_g: 'Protein',
  carbs_g: 'Carbs',
  fat_g: 'Fat',
  fiber_g: 'Fiber',
  sugar_g: 'Sugar',
  saturated_fat_g: 'Sat. fat',
  sodium_mg: 'Sodium',
  serving_size_g: 'Serving size',
  serving_label: 'Serving label',
  variant_of: 'Parent food',
  variant_label: 'Variant',
  source: 'Source',
  source_id: 'Source id',
}

export const fieldLabel = (field: string) => FIELD_LABELS[field] ?? field
