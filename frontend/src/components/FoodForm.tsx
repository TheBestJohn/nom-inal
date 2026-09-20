import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { ChevronDown, ChevronRight } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { FoodInput } from '@/api/endpoints'
import type { Food, FoodDetail, NutrientBasis } from '@/api/types'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { ErrorNote } from '@/components/shared'

/** The nutrient fields, so scaling them between bases is one list, not eight. */
const NUTRIENT_FIELDS = [
  'calories_kcal',
  'protein_g',
  'carbs_g',
  'fat_g',
  'fiber_g',
  'sugar_g',
  'saturated_fat_g',
  'sodium_mg',
] as const

/**
 * Trim floating-point dust without losing real precision.
 *
 * Converting between bases and back is exact in decimal but not in binary, so
 * a value that went out as 5 can come back as 5.000000000000001. Six decimals
 * is well past anything a label prints, and matches what the server rounds to,
 * so the round trip is stable and re-saving an untouched food does not look
 * like an edit.
 */
const tidy = (v: number) => Math.round(v * 1e6) / 1e6

/** Rescale every nutrient between the per-100 g and per-serving bases. */
function rebase(form: FoodDraft, to: NutrientBasis): FoodDraft {
  const serving = form.serving_size_g && form.serving_size_g > 0 ? form.serving_size_g : 100
  const factor = to === 'per_serving' ? serving / 100 : 100 / serving
  const out = { ...form, nutrient_basis: to }
  for (const field of NUTRIENT_FIELDS) {
    const value = out[field]
    if (typeof value === 'number') out[field] = tidy(value * factor)
  }
  return out
}

/**
 * One labelled numeric input.
 *
 * Defined at module scope, not inside `FoodForm`. A component declared in a
 * render body is a *new type* on every render, so React unmounts the old input
 * and mounts a fresh one rather than updating it — which throws away focus and
 * the caret on every keystroke. Same JSX, same props, entirely different
 * behaviour, and nothing about the markup hints at it.
 */
function NumField({
  id,
  label,
  value,
  onChange,
  required,
}: {
  id: string
  label: string
  value: number | null
  onChange: (e: React.ChangeEvent<HTMLInputElement>) => void
  required?: boolean
}) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{label}</Label>
      <Input
        id={id}
        type="number"
        step="any"
        min={0}
        required={required}
        value={value ?? ''}
        onChange={onChange}
      />
    </div>
  )
}

/**
 * The form's own state. The numeric fields can be empty while you type, which
 * `FoodInput` cannot express — and pre-filling them with 0 instead is not a
 * neutral choice: typing 140 into a field showing 0 produces 0140.
 */
type FoodDraft = Omit<
  FoodInput,
  'calories_kcal' | 'protein_g' | 'carbs_g' | 'fat_g' | 'serving_size_g'
> & {
  calories_kcal: number | null
  protein_g: number | null
  carbs_g: number | null
  fat_g: number | null
  serving_size_g: number | null
}

const EMPTY: FoodDraft = {
  name: '',
  brand: '',
  upc: '',
  calories_kcal: null,
  protein_g: null,
  carbs_g: null,
  fat_g: null,
  fiber_g: null,
  sugar_g: null,
  saturated_fat_g: null,
  sodium_mg: null,
  serving_size_g: null,
  serving_label: '',
  nutrient_basis: 'per_serving',
  variant_of: null,
  variant_label: null,
  edit_summary: '',
}

/**
 * Create or edit a food.
 *
 * Shared between the Foods page, the food picker and the detail dialog, so a
 * food can be created or corrected without leaving whatever you were in the
 * middle of. The optional nutrients start collapsed: thirteen fields is a lot
 * to face when you are halfway through logging lunch, and only six of them are
 * required.
 */
export default function FoodForm({
  food,
  variantOf,
  initialName,
  askForSummary,
  onSaved,
  onCancel,
  submitLabel,
}: {
  food?: Food | null
  /**
   * Start a new preparation variant of this food. The nutrients are seeded
   * from the parent because a variant is usually a nudge from it, not a blank
   * form -- cooked chicken is not a different food you have to look up again.
   */
  variantOf?: Food | null
  /** Seeds the name, so a search that found nothing carries straight over. */
  initialName?: string
  /**
   * Ask what changed. Shown when editing something that already exists, where
   * the note is the difference between a history you can read and a list of
   * timestamps.
   */
  askForSummary?: boolean
  onSaved: (food: FoodDetail) => void
  onCancel: () => void
  submitLabel?: string
}) {
  const queryClient = useQueryClient()
  const [showMore, setShowMore] = useState(false)
  // The form holds numbers in whichever basis is on screen. Storage is always
  // per 100 g, so an existing food is rebased on the way in and the basis
  // travels with the payload on the way out — the server does the arithmetic,
  // once, for every client.
  const [form, setForm] = useState<FoodDraft>(() => {
    const source = food ?? variantOf
    if (!source) return { ...EMPTY, name: initialName ?? '' }
    const stored: FoodDraft = {
      name: source.name,
      brand: source.brand ?? '',
      upc: food?.upc ?? '',
      calories_kcal: source.calories_kcal,
      protein_g: source.protein_g,
      carbs_g: source.carbs_g,
      fat_g: source.fat_g,
      fiber_g: source.fiber_g,
      sugar_g: source.sugar_g,
      saturated_fat_g: source.saturated_fat_g,
      sodium_mg: source.sodium_mg,
      serving_size_g: source.serving_size_g,
      serving_label: source.serving_label ?? '',
      nutrient_basis: 'per_100g',
      variant_of: variantOf ? variantOf.id : (food?.variant_of ?? null),
      variant_label: variantOf ? '' : (food?.variant_label ?? null),
      edit_summary: '',
    }
    return source.nutrient_basis === 'per_serving' ? rebase(stored, 'per_serving') : stored
  })

  const perServing = form.nutrient_basis === 'per_serving'

  const save = useMutation({
    mutationFn: () => {
      const payload: FoodInput = {
        ...form,
        name: form.name.trim(),
        calories_kcal: form.calories_kcal ?? 0,
        protein_g: form.protein_g ?? 0,
        carbs_g: form.carbs_g ?? 0,
        fat_g: form.fat_g ?? 0,
        // 100 g is the sensible default only when the figures are already per
        // 100 g; per serving it is a silent wrong answer, so the field is
        // required in that mode instead.
        serving_size_g: form.serving_size_g ?? 100,
        brand: form.brand || null,
        upc: form.upc || null,
        serving_label: form.serving_label || null,
        variant_label: form.variant_label ? form.variant_label.trim() : null,
        edit_summary: form.edit_summary || null,
      }
      return food ? api.updateFood(food.id, payload) : api.createFood(payload)
    },
    onSuccess: (saved) => {
      queryClient.invalidateQueries({ queryKey: ['foods'] })
      onSaved(saved)
    },
  })

  const num = (key: keyof FoodInput) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm((f) => ({ ...f, [key]: e.target.value === '' ? null : Number(e.target.value) }))

  /** Binds one numeric field of this form to the shared `NumField` above. */
  const field = (id: string, label: string, key: keyof FoodDraft, required?: boolean) => (
    <NumField
      id={id}
      label={label}
      required={required}
      value={form[key] as number | null}
      onChange={num(key)}
    />
  )

  return (
    <form
      className="space-y-3"
      onSubmit={(e) => {
        e.preventDefault()
        save.mutate()
      }}
    >
      <div className="space-y-1.5">
        <Label htmlFor="f-name">Name</Label>
        <Input
          id="f-name"
          required
          autoFocus
          value={form.name}
          onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
        />
      </div>

      <div className="space-y-1.5">
        <Label htmlFor="f-brand">Brand</Label>
        <Input
          id="f-brand"
          value={form.brand ?? ''}
          onChange={(e) => setForm((f) => ({ ...f, brand: e.target.value }))}
        />
      </div>

      {form.variant_of && (
        <div className="space-y-1.5">
          <Label htmlFor="f-variant">Variant</Label>
          <Input
            id="f-variant"
            required
            placeholder="cooked, raw, drained…"
            value={form.variant_label ?? ''}
            onChange={(e) => setForm((f) => ({ ...f, variant_label: e.target.value }))}
          />
          <p className="text-muted-foreground text-xs">
            What makes this different from the food it came from. The numbers below started as a
            copy of the parent&rsquo;s — change the ones the preparation changes.
          </p>
        </div>
      )}

      {/* Two bases, because the numbers come from two kinds of place. A
          packaged food is read off a label, which states one serving; a
          reference figure from USDA is per 100 g. Making people convert by
          hand does not fail loudly — it silently stores a food that is wrong
          by the serving ratio, and in a database anyone can edit, the next
          person "fixes" the converted value back. */}
      <div className="space-y-1.5">
        <Label>These numbers are</Label>
        <ToggleGroup
          type="single"
          value={form.nutrient_basis ?? 'per_100g'}
          onValueChange={(next) => {
            if (next) setForm((f) => rebase(f, next as NutrientBasis))
          }}
          aria-label="What the figures below describe"
          className="w-full"
        >
          <ToggleGroupItem value="per_serving" className="flex-1">
            Per serving
          </ToggleGroupItem>
          <ToggleGroupItem value="per_100g" className="flex-1">
            Per 100 g
          </ToggleGroupItem>
        </ToggleGroup>
        <p className="text-muted-foreground text-xs">
          {perServing
            ? 'As a nutrition label reads. Switching converts what you have typed, so you can check it either way.'
            : 'As USDA and Open Food Facts publish. Switching converts what you have typed, so you can check it either way.'}
        </p>
      </div>

      {/* The serving size divides the figures below it when the basis is per
          serving, so it is asked for first rather than buried underneath
          them. */}
      <div className="grid grid-cols-2 gap-3">
        {field('f-serv', 'Serving size (g)', 'serving_size_g', perServing)}
        <div className="space-y-1.5">
          <Label htmlFor="f-servlabel">Serving label</Label>
          <Input
            id="f-servlabel"
            placeholder="e.g. 1 cup, 12 crackers"
            value={form.serving_label ?? ''}
            onChange={(e) => setForm((f) => ({ ...f, serving_label: e.target.value }))}
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {field('f-kcal', 'Calories', 'calories_kcal', true)}
        {field('f-p', 'Protein (g)', 'protein_g', true)}
        {field('f-c', 'Carbs (g)', 'carbs_g', true)}
        {field('f-f', 'Fat (g)', 'fat_g', true)}
      </div>

      {/* The check the person entering cannot easily do in their head, and the
          one that catches a mistyped serving size: 900 kcal per 100 g is pure
          fat, so anything near it is a wrong number rather than a rich food. */}
      {perServing && !!form.serving_size_g && !!form.calories_kcal && (
        <p className="text-muted-foreground text-xs">
          Works out to{' '}
          <strong>{tidy((form.calories_kcal * 100) / form.serving_size_g).toFixed(0)} kcal</strong>{' '}
          per 100 g, which is what gets stored.
        </p>
      )}

      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="px-0"
        onClick={() => setShowMore((v) => !v)}
        aria-expanded={showMore}
      >
        {showMore ? <ChevronDown /> : <ChevronRight />}
        More nutrients
      </Button>

      {showMore && (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          {field('f-fib', 'Fiber (g)', 'fiber_g')}
          {field('f-sug', 'Sugar (g)', 'sugar_g')}
          {field('f-sat', 'Sat. fat (g)', 'saturated_fat_g')}
          {field('f-na', 'Sodium (mg)', 'sodium_mg')}
          <div className="col-span-2 space-y-1.5 sm:col-span-4">
            <Label htmlFor="f-upc">UPC</Label>
            <Input
              id="f-upc"
              inputMode="numeric"
              value={form.upc ?? ''}
              onChange={(e) => setForm((f) => ({ ...f, upc: e.target.value }))}
            />
          </div>
        </div>
      )}

      {askForSummary && (
        <div className="space-y-1.5">
          <Label htmlFor="f-summary">What changed?</Label>
          <Input
            id="f-summary"
            placeholder="e.g. fat is 3.2 g on the current packaging"
            value={form.edit_summary ?? ''}
            onChange={(e) => setForm((f) => ({ ...f, edit_summary: e.target.value }))}
          />
          <p className="text-muted-foreground text-xs">
            Kept on the revision, not on the food. It is what the next person reads before deciding
            whether to trust your numbers.
          </p>
        </div>
      )}

      <ErrorNote error={save.error} />

      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button
          type="submit"
          disabled={
            save.isPending ||
            !form.name.trim() ||
            // Per serving, the serving size is a divisor: leaving it blank
            // would quietly store the label's figures as per-100 g values.
            (perServing && !form.serving_size_g) ||
            // The server refuses a parent without a label too; catching it here
            // saves a round trip to be told something the form already knows.
            (!!form.variant_of && !form.variant_label?.trim())
          }
        >
          {save.isPending ? 'Saving…' : (submitLabel ?? 'Save')}
        </Button>
      </div>
    </form>
  )
}
