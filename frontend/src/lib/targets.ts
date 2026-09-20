import { api } from '@/api/endpoints'
import type { TargetInput } from '@/api/endpoints'
import type { NutritionTarget } from '@/api/types'

/**
 * Write some targets without disturbing the rest.
 *
 * `PUT /targets` replaces the whole set — the settings editor wants that, a
 * nutrient it leaves out is one the person cleared — so a calculator that
 * only knows about protein has to read the current set first and send it
 * back with its own rows swapped in. The read is fresh rather than from a
 * cache, so two "apply" clicks in a row do not race each other's copy.
 */
export function mergeTargets(existing: NutritionTarget[], patch: TargetInput[]): TargetInput[] {
  const replaced = new Set(patch.map((t) => t.nutrient))
  return [
    ...existing
      .filter((t) => !replaced.has(t.nutrient))
      .map((t) => ({ nutrient: t.nutrient, amount: t.amount, kind: t.kind })),
    ...patch,
  ]
}

export async function applyTargets(patch: TargetInput[]): Promise<NutritionTarget[]> {
  const existing = await api.listTargets()
  return api.replaceTargets(mergeTargets(existing, patch))
}
