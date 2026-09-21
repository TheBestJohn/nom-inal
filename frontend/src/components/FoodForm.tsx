import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { ChevronDown, ChevronRight, Plus, X } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { FoodInput } from '@/api/endpoints'
import type { Food, FoodDetail, NutrientBasis } from '@/api/types'
import { grams } from '@/lib/format'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { ErrorNote } from '@/components/shared'

/**
 * A household portion as the form holds it. `id` is set for one already on
 * the food; a new one has none until it is saved.
 */
interface PortionDraft {
  id?: string
  label: string
  grams: number
  source: string
}

/**
 * The portions list: what is on the food, plus what has been typed since.
 *
 * Module scope, like `NumField` below and for the same reason. The rows are
 * edited in place and written when the food is saved, so a new food and a
 * correction go through one form with one Save.
 */
function PortionsEditor({
  portions,
  onChange,
}: {
  portions: PortionDraft[]
  onChange: (next: PortionDraft[]) => void
}) {
  const [label, setLabel] = useState('')
  const [weight, setWeight] = useState('')
  // A duplicate used to be dropped in silence: the button was enabled, it was
  // pressed, and nothing happened. It says so now.
  const [note, setNote] = useState<string | null>(null)

  const add = () => {
    const name = label.trim()
    const value = Number(weight)
    if (!name || !(value > 0)) return
    if (portions.some((p) => p.label.toLowerCase() === name.toLowerCase())) {
      setNote(`“${name}” is already on this food.`)
      return
    }
    onChange([...portions, { label: name, grams: value, source: 'user' }])
    setLabel('')
    setWeight('')
    setNote(null)
  }

  return (
    <div className="space-y-2">
      <Label>Household portions</Label>
      {portions.length > 0 && (
        <ul className="space-y-1">
          {portions.map((p) => (
            <li
              key={p.id ?? p.label}
              className="flex items-center gap-2 rounded-md border px-3 py-1.5 text-sm"
            >
              <span className="min-w-0 flex-1 truncate">
                {p.label} · {grams(p.grams, 0)}
              </span>
              {p.source !== 'user' && (
                <Badge variant="outline" className="text-[10px] tracking-wide uppercase">
                  {p.source}
                </Badge>
              )}
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label={`Remove portion ${p.label}`}
                onClick={() => onChange(portions.filter((x) => x !== p))}
              >
                <X />
              </Button>
            </li>
          ))}
        </ul>
      )}
      <div className="grid grid-cols-[1fr_6rem_auto] gap-2">
        <Input
          // Without the one in front: the amount box puts a count beside
          // this, where "2  1 cup" reads badly.
          placeholder="cup, slice, mug…"
          aria-label="Portion label"
          value={label}
          onChange={(e) => {
            setLabel(e.target.value)
            setNote(null)
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              add()
            }
          }}
        />
        <Input
          type="number"
          min={0}
          step="any"
          placeholder="grams"
          aria-label="Portion grams"
          value={weight}
          onChange={(e) => setWeight(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              add()
            }
          }}
        />
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={add}
          disabled={!label.trim() || !(Number(weight) > 0)}
          aria-label="Add portion"
        >
          <Plus /> Add
        </Button>
      </div>
      {note && <p className="text-destructive text-xs">{note}</p>}
      <p className="text-muted-foreground text-xs">
        How many grams a cup, a slice or a mug of this is. These are the units the amount box offers
        when this food is logged — “2 chicken breasts”, with the weight worked out beside it. One
        can also be added while logging, without coming back here.
      </p>
    </div>
  )
}

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

  // Portions travel with the form but not with the food's own request: they
  // are rows of their own, added and removed one at a time, so the save
  // writes the food first and then settles the difference.
  const [portions, setPortions] = useState<PortionDraft[]>(() =>
    (food?.portions ?? []).map((p) => ({ ...p })),
  )

  const save = useMutation({
    mutationFn: async () => {
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
      let saved = food ? await api.updateFood(food.id, payload) : await api.createFood(payload)

      const removed = (food?.portions ?? []).filter((p) => !portions.some((d) => d.id === p.id))
      const added = portions.filter((p) => !p.id)
      for (const p of removed) saved = await api.removePortion(saved.id, p.id)
      for (const p of added)
        saved = await api.addPortion(saved.id, { label: p.label, grams: p.grams })
      return saved
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

      {/* Not behind "More nutrients": a portion is not a nutrient, and it is
          the one thing a person adding a food from their own kitchen knows
          better than any database. A variant inherits none, since a cup of
          cooked rice does not weigh what a cup of raw rice does. */}
      {!variantOf && <PortionsEditor portions={portions} onChange={setPortions} />}

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
