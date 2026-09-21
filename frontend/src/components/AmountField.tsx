import { useMemo, useRef, useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Ruler, X } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { FoodDetail, FoodPortion } from '@/api/types'
import { grams } from '@/lib/format'
import {
  GRAMS,
  amountGrams,
  cleanCount,
  measureOf,
  measuresFor,
  sameLabel,
  type Amount,
  type AmountFood,
} from '@/lib/amounts'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { ErrorNote } from '@/components/shared'

/** The option that opens the "how much is one of these" form. */
const NEW_MEASURE = '__new__'

/** Digits and the point, one character at a time. */
const isDigit = (key: string) => key.length === 1 && /[0-9.]/.test(key)

/**
 * How much of a food, in the units people actually say it in.
 *
 * A count beside a unit, and the weight it comes to underneath: "2" and
 * "chicken breast" and, in smaller type, "348 g". Both halves are wanted —
 * the count is how the amount was decided, the weight is what the nutrition
 * is computed from, and hiding either one makes the other unverifiable.
 *
 * Module scope, and so is everything it is made of. A component declared in a
 * render body is a new component type on every render, which remounts its
 * inputs and loses the caret mid-word; this codebase has fixed that twice.
 */
export default function AmountField({
  food,
  value,
  onChange,
  idPrefix = 'amount',
  label = 'Amount',
  autoFocus = false,
  compact = false,
  className,
}: {
  food: AmountFood
  value: Amount
  onChange: (next: Amount) => void
  /** Ids have to be unique on a page that shows one of these per ingredient. */
  idPrefix?: string
  /** Left out, the field is unlabelled — for a list row, where the row says it. */
  label?: string | null
  autoFocus?: boolean
  /** A list row rather than a form: no label, tighter, weight on the same line. */
  compact?: boolean
  className?: string
}) {
  const queryClient = useQueryClient()
  // Portions added from here, kept locally as well as pushed to the caches:
  // the food object this control was handed is the caller's, and it may not
  // be re-fetched before the next keystroke.
  const [added, setAdded] = useState<FoodPortion[]>([])
  const [adding, setAdding] = useState(false)
  // Whether the count box still holds the figure it was given, rather than
  // one somebody has started editing. See the key handler below.
  const pristine = useRef(true)
  const [newLabel, setNewLabel] = useState('')
  const [newGrams, setNewGrams] = useState('')

  const measures = useMemo(() => {
    const merged = [...(food.portions ?? [])]
    for (const p of added) if (!merged.some((m) => m.id === p.id)) merged.push(p)
    const out = measuresFor({ ...food, portions: merged })
    // A measure the caller is holding that this food does not list — an
    // amount saved against a portion whose food has not been fetched yet —
    // stays selectable. Dropping it would turn "2 chicken breasts" into two
    // grams the moment the row was drawn.
    if (!out.some((m) => m.key === value.measure.key)) out.unshift(value.measure)
    return out
  }, [food, added, value.measure])

  // Looked up by key rather than taken as given: the caller may be holding a
  // measure built before a portion was added here, or before the food it came
  // from had been fetched in full.
  const measure = measureOf(measures, value.measure.key)
  const weight = amountGrams({ ...value, measure })

  const addPortion = useMutation({
    mutationFn: () => api.addPortion(food.id, { label: newLabel.trim(), grams: Number(newGrams) }),
    onSuccess: (saved: FoodDetail) => {
      const known = new Set((food.portions ?? []).map((p) => p.id))
      for (const p of added) known.add(p.id)
      const fresh = saved.portions.filter((p) => !known.has(p.id))
      setAdded((prev) => [...prev, ...fresh])
      // Pick it: adding a measure is something you do in order to use it.
      const picked = fresh[0] ?? saved.portions.find((p) => sameLabel(p.label, newLabel))
      if (picked) {
        onChange({
          measure: {
            key: picked.id,
            label: picked.label,
            grams: picked.grams,
            portionId: picked.id,
          },
          count: '1',
        })
      }
      pristine.current = true
      setAdding(false)
      setNewLabel('')
      setNewGrams('')
      queryClient.invalidateQueries({ queryKey: ['foods'] })
    },
  })

  const canAdd = newLabel.trim().length > 0 && Number(newGrams) > 0

  if (adding) {
    return (
      <div className={cn('space-y-2 rounded-md border p-3', className)} data-amount-new>
        {/* The question the user asked us to ask, filled in as it is
            answered: "How much is one …?" becomes "How much is one chicken
            breast?" the moment there is a chicken breast to ask about. The
            food's own name is no good here — one basmati rice, dry. */}
        <p className="text-sm font-medium">How much is one {newLabel.trim() || 'of these'}?</p>
        <div className="grid grid-cols-[minmax(0,1fr)_5.5rem] gap-2">
          <Input
            id={`${idPrefix}-new-label`}
            autoFocus
            placeholder="breast, slice, cup…"
            aria-label="What one of these is called"
            value={newLabel}
            onChange={(e) => setNewLabel(e.target.value)}
            className="pointer-coarse:h-11"
          />
          <Input
            id={`${idPrefix}-new-grams`}
            inputMode="decimal"
            placeholder="grams"
            aria-label="Grams in one"
            value={newGrams}
            onChange={(e) => {
              const next = cleanCount(e.target.value)
              if (next !== null) setNewGrams(next)
            }}
            className="tabular pointer-coarse:h-11 text-right"
          />
        </div>
        <ErrorNote error={addPortion.error} />
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            size="sm"
            disabled={!canAdd || addPortion.isPending}
            onClick={() => addPortion.mutate()}
          >
            <Plus /> {addPortion.isPending ? 'Saving…' : 'Save and use it'}
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => {
              setAdding(false)
              addPortion.reset()
            }}
          >
            <X /> Cancel
          </Button>
        </div>
        <p className="text-muted-foreground text-xs">
          A measure for {food.name}, kept on the food: it is there the next time you log it, and for
          anyone else logging the same thing.
        </p>
      </div>
    )
  }

  return (
    <div className={cn(compact ? 'min-w-0 flex-1 space-y-1' : 'space-y-1.5', className)}>
      {label && !compact && <Label htmlFor={`${idPrefix}-count`}>{label}</Label>}
      <div className="flex items-center gap-2">
        <Input
          id={`${idPrefix}-count`}
          inputMode="decimal"
          autoFocus={autoFocus}
          aria-label={measure.key === GRAMS.key ? 'Grams' : 'How many'}
          className={cn('tabular pointer-coarse:h-11 text-right', compact ? 'w-16' : 'w-20')}
          value={value.count}
          // A count is one or two characters and it arrives already holding
          // one, so the first digit typed into it replaces what is there
          // rather than joining it: otherwise the box that helpfully says 2
          // turns "12" into "122". Only the first, and only a digit — a
          // second keystroke, or a backspace, is somebody editing what they
          // can see, and that has to behave like any other box.
          //
          // Done here rather than by selecting the text on focus, which is
          // the usual trick and is not reliable: the browser places the caret
          // from a click of its own accord, after the event has been
          // dispatched, so the selection is collapsed again a moment later.
          // Selecting on focus is still worth doing for the keyboard, where
          // there is no click to undo it.
          onFocus={(e) => e.currentTarget.select()}
          onKeyDown={(e) => {
            const first = pristine.current
            pristine.current = false
            if (!first || !isDigit(e.key)) return
            e.preventDefault()
            const next = cleanCount(e.key)
            if (next !== null) onChange({ ...value, count: next })
          }}
          onBlur={() => {
            pristine.current = true
          }}
          onChange={(e) => {
            const next = cleanCount(e.target.value)
            if (next !== null) onChange({ ...value, count: next })
          }}
        />
        <Select
          value={measure.key}
          onValueChange={(next) => {
            if (next === NEW_MEASURE) {
              setAdding(true)
              return
            }
            const picked = measureOf(measures, next)
            // Switching to grams from "2 chicken breasts" keeps the weight,
            // which is the number you were looking at; switching the other
            // way starts at one of them, not at 348 of them.
            const current = amountGrams({ ...value, measure })
            const count =
              picked.key === GRAMS.key && current !== null
                ? String(current)
                : measure.key === GRAMS.key
                  ? '1'
                  : value.count
            pristine.current = true
            onChange({ measure: picked, count })
          }}
        >
          <SelectTrigger
            aria-label="Unit"
            // Marked important because the primitive sets its height off a
            // data attribute, which outranks a plain utility however late it
            // is declared. Same 44 pixels every other control gets.
            className="pointer-coarse:h-11! min-w-0 flex-1 justify-between"
          >
            <SelectValue>
              <span className="truncate">{measure.label}</span>
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              {measures.map((m) => (
                <SelectItem key={m.key} value={m.key}>
                  {m.key === GRAMS.key ? 'grams' : `${m.label} · ${grams(m.grams, 0)}`}
                </SelectItem>
              ))}
              <SelectItem value={NEW_MEASURE}>
                {/* One span, because the item puts its children inside its
                    own text node — two of them stack, and the icon landed on
                    a line above the words it belongs to. */}
                <span className="flex items-center gap-2">
                  <Ruler className="size-4 shrink-0" /> Add a measure…
                </span>
              </SelectItem>
            </SelectGroup>
          </SelectContent>
        </Select>
      </div>

      {/* The exact number, always visible. It is the whole reason grams are
          still here: "two chicken breasts" is how the amount was decided,
          "348 g" is what the nutrition below it was computed from. */}
      <p className="text-muted-foreground tabular text-xs" data-amount-weight>
        {measure.key === GRAMS.key
          ? weight === null
            ? 'Type an amount.'
            : // The box is the weight here, so this line only repeats it —
              // and is kept anyway, so that switching units does not shuffle
              // everything under it up and down by a line.
              grams(weight, 0)
          : weight === null
            ? `One is ${grams(measure.grams, 0)}.`
            : `${value.count} × ${grams(measure.grams, 0)} = ${grams(weight, 0)}`}
      </p>
    </div>
  )
}
