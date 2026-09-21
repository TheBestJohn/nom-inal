/**
 * Amounts in the units people say them in.
 *
 * A number on its own is not an amount — "two chicken breasts" is, and so is
 * "348 g". This is the arithmetic and the wire format for both, kept apart
 * from the control that draws them so a list row, a request body and a
 * default can all reason about an amount without rendering one.
 */

import type { FoodPortion } from '@/api/types'
import { round } from '@/lib/format'

/**
 * A unit an amount can be given in: grams, the food's serving, or one of its
 * household portions.
 *
 * What goes over the wire is either `quantity_g` or `{portion_id,
 * portion_count}` — never both. `portionId` is what decides which, so a
 * measure with no portion behind it still works: it becomes the grams it is
 * worth, exactly as typing that number would.
 */
export interface Measure {
  /** Stable across renders: `g` for grams, `serving` for a bare serving, else the portion id. */
  key: string
  /** What the selector says: "chicken breast", "cup, chopped", "1/2 cup". */
  label: string
  /** Grams in one of these. 1 for grams, so the arithmetic is the same either way. */
  grams: number
  /** The portion to send. Null means send the weight instead. */
  portionId: string | null
}

export const GRAMS: Measure = { key: 'g', label: 'grams', grams: 1, portionId: null }

/**
 * The value of the control: which unit, and how many.
 *
 * The measure itself, not a key into a list the caller would then have to
 * keep. A measure can appear while the control is open — someone says how
 * much one chicken breast is and there it is — and a caller holding a key
 * into a list assembled before that would silently resolve it to grams.
 */
export interface Amount {
  measure: Measure
  /**
   * The count as a string, not a number. A number would render `0` into an
   * empty box, and this app has already been bitten by that once: typing 140
   * into a box holding a pre-filled zero gives 0140.
   */
  count: string
}

/**
 * What the control needs of a food. Deliberately narrower than `Food`, so a
 * caller can pass a food it only has part of — a picker result, a row of an
 * ingredient list — without inventing the rest.
 */
export interface AmountFood {
  id: string
  name: string
  portions?: FoodPortion[]
  /**
   * The food's own serving in a portion's shape, which the API sends beside
   * `portions` precisely so it can be offered like one — every food has a
   * serving, and most foods have no portions at all. Optional here only
   * because a caller may be holding a food it has not fetched in full.
   */
  serving_portion?: FoodPortion | null
  serving_size_g?: number
  serving_label?: string | null
}

/**
 * The food's own serving, as a measure.
 *
 * Taken as the server sends it where it is there, which is the case that
 * matters: a serving with an id behind it can be sent as a portion, so "1
 * serving" is recorded as those words rather than flattened to its weight.
 * Failing that — an older server, or a food this caller only half has — the
 * serving is still offered, as the grams it is worth. Same figure, arrived at
 * the way the old serving button arrived at it.
 */
function servingMeasure(food: AmountFood): Measure | null {
  const serving = food.serving_portion
  if (serving && serving.grams > 0) {
    return {
      key: serving.id,
      label: measureLabel(serving.label),
      grams: serving.grams,
      portionId: serving.id,
    }
  }
  const weight = food.serving_size_g ?? 0
  if (!(weight > 0)) return null
  return {
    key: 'serving',
    label: measureLabel(food.serving_label ?? '') || 'serving',
    grams: weight,
    portionId: null,
  }
}

/**
 * Everything this food can be measured in, in the order the list offers them:
 * its portions, its serving, and grams last — grams is always available and
 * never the interesting answer, so it does not deserve the top of the list.
 */
export function measuresFor(food: AmountFood): Measure[] {
  const out: Measure[] = (food.portions ?? []).map((p) => ({
    key: p.id,
    label: measureLabel(p.label),
    grams: p.grams,
    portionId: p.id,
  }))
  const serving = servingMeasure(food)
  // A serving spelled the same as a portion is that portion. Offering both
  // would be two rows saying "1/2 cup · 40 g" with no way to tell them apart.
  if (serving && !out.some((m) => m.key === serving.key || sameLabel(m.label, serving.label))) {
    out.push(serving)
  }
  out.push(GRAMS)
  return out
}

/**
 * A portion's label as a unit beside a count.
 *
 * Providers and the serving alike spell a measure with the one in front of
 * it — "1 serving", "1 cup" — which beside a count box reads "2  1 serving".
 * The count is the number here, so the leading one goes. A fraction keeps
 * its: "1/2 cup" has no space after the 1 and is one word.
 */
export const measureLabel = (label: string) => label.trim().replace(/^1\s+/, '')

export const sameLabel = (a: string, b: string) =>
  measureLabel(a).toLowerCase() === measureLabel(b).toLowerCase()

export const measureOf = (measures: Measure[], key: string) =>
  measures.find((m) => m.key === key) ?? measures[measures.length - 1] ?? GRAMS

/** The weight this amount comes to, or null while it is not a usable number. */
export function amountGrams(amount: Amount): number | null {
  const count = Number(amount.count)
  if (!Number.isFinite(count) || count <= 0) return null
  return round(count * amount.measure.grams, 2)
}

/**
 * The amount, as the API takes it. Exactly one of the two forms, because
 * sending both is a 400 — the server will not guess which one you meant.
 */
export function amountRequest(amount: Amount) {
  const count = Number(amount.count)
  if (amount.measure.portionId) {
    return { portion_id: amount.measure.portionId, portion_count: count }
  }
  return { quantity_g: round(count * amount.measure.grams, 2) }
}

/**
 * Where a food with portions should land before anyone touches it.
 *
 * The rule, in order:
 *
 * 1. A weight this food was logged at before — the picker's "as last time" —
 *    keeps its exact value. If a measure divides it into a tidy count it is
 *    shown that way ("2 chicken breasts"), and otherwise as grams. Either way
 *    the weight is the one that was logged, never rounded to fit a portion.
 * 2. Otherwise: a portion someone typed in for this food, because they typed
 *    it in to use it; then the food's own serving when it is named, because a
 *    named serving ("1/2 cup") is a household measure that happens to live in
 *    another column; then a portion the provider supplied; then an unnamed
 *    serving; then grams.
 *
 * The count starts at 1, which is the question the user asked us to answer —
 * how many chicken breasts — and not at a zero anyone has to delete first.
 */
export function defaultAmount(food: AmountFood, lastGrams?: number | null): Amount {
  const measures = measuresFor(food)
  if (lastGrams && lastGrams > 0) {
    for (const measure of measures) {
      if (measure.portionId === null) continue
      const count = lastGrams / measure.grams
      const tidy = round(count, 2)
      if (tidy >= 0.25 && tidy <= 99 && Math.abs(tidy * measure.grams - lastGrams) <= 0.01) {
        return { measure, count: String(tidy) }
      }
    }
    return { measure: GRAMS, count: String(round(lastGrams, 1)) }
  }

  const portions = food.portions ?? []
  const serving = servingMeasure(food)
  // Named, not merely present: every food has a serving and most are "1
  // serving · 100 g", which is a worse answer than "cup, chopped · 120 g".
  const named = food.serving_label?.trim() ? serving : null
  const user = portions.find((p) => p.source === 'user')
  const preferred =
    (user && measures.find((m) => m.key === user.id)) ??
    (named && measures.find((m) => m.key === named.key)) ??
    (portions[0] && measures.find((m) => m.key === portions[0].id)) ??
    (serving && measures.find((m) => m.key === serving.key)) ??
    GRAMS

  // Grams is the only measure where "1" is a silly answer. 100 g is what the
  // dialog has always opened with, and what a per-100 g food reads in.
  return { measure: preferred, count: preferred.key === GRAMS.key ? '100' : '1' }
}

/** Digits and at most one point. Anything else is not a number being typed. */
const NUMERIC = /^\d*\.?\d*$/

/**
 * What a count box should hold after this keystroke.
 *
 * Leading zeroes go, which is the "0140" bug at its source rather than at the
 * one call site that hit it, and a lone "0." is left alone because somebody is
 * halfway through typing 0.5.
 */
export function cleanCount(next: string): string | null {
  const trimmed = next.replace(/\s/g, '')
  if (!NUMERIC.test(trimmed)) return null
  if (trimmed.length > 1 && trimmed.startsWith('0') && !trimmed.startsWith('0.')) {
    return trimmed.replace(/^0+(?=\d)/, '')
  }
  return trimmed
}

/**
 * An amount that is already saved, before the food it belongs to is known.
 *
 * A diary entry or a recipe item says what it was counted in — "chicken
 * breast", twice — but not which portion row that was, and an ingredient list
 * does not carry each food's measures. So the label and the count are
 * believed as sent, with the weight divided back out to give one of them, and
 * `resolveAmount` attaches it to the real portion once the food arrives.
 * Left unresolved it still saves correctly, as the weight it always was.
 */
export interface SavedAmount {
  quantity_g?: number | null
  portion_label?: string | null
  portion_count?: number | null
}

export function savedAmount(row: SavedAmount): Amount {
  const weight = row.quantity_g ?? 0
  const count = row.portion_count ?? 0
  const label = measureLabel(row.portion_label ?? '')
  if (label && count > 0 && weight > 0) {
    return {
      measure: { key: `saved:${label}`, label, grams: round(weight / count, 4), portionId: null },
      count: String(round(count, 2)),
    }
  }
  return { measure: GRAMS, count: String(round(weight, 1)) }
}

/** Re-attach a saved amount to the food's own measures, once the food is known. */
export function resolveAmount(amount: Amount, food: AmountFood | undefined): Amount {
  if (!food) return amount
  if (amount.measure.portionId !== null || amount.measure.key === GRAMS.key) return amount
  const match = measuresFor(food).find((m) => sameLabel(m.label, amount.measure.label))
  return match ? { ...amount, measure: match } : amount
}
