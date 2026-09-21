import { useState } from 'react'
import { ArrowLeft, Check, Search } from 'lucide-react'

import type { DraftCandidate, DraftLine, Food, RecipeDraft } from '@/api/types'
import { foodItem, textItem, type DraftItem } from '@/lib/recipeDraft'
import { GRAMS } from '@/lib/amounts'
import { round } from '@/lib/format'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import FoodPicker from '@/components/FoodPicker'

/** The tiers tight enough to pick a food without asking — the food picker's rule. */
const TIGHT_TIERS = new Set(['exact', 'prefix'])

/** Units that count things rather than measure them, where "2 of" is "2 servings of". */
const COUNT_UNITS = new Set(['large', 'medium', 'small', 'whole', 'piece', 'slice', 'fillet'])

/** What a food in the review needs: enough to build a draft item from. */
type Chosen = Pick<
  DraftCandidate,
  | 'food_id'
  | 'name'
  | 'brand'
  | 'serving_size_g'
  | 'calories_kcal'
  | 'protein_g'
  | 'carbs_g'
  | 'fat_g'
>

type Choice =
  | { kind: 'food'; food: Chosen; grams: string }
  | { kind: 'text'; text: string }
  | { kind: 'skip' }

/**
 * Grams to suggest for a food chosen on a line. A stated mass is the answer;
 * a bare count or a size word is that many of the food's serving; a cup or a
 * tablespoon is left blank, because converting volume to weight needs a
 * density this app does not have, and a guess dressed as a figure is worse
 * than a box to fill in.
 */
function suggestedGrams(line: DraftLine, food: Chosen): string {
  if (line.grams !== null) return String(round(line.grams, 1))
  if (line.quantity !== null && (line.unit === null || COUNT_UNITS.has(line.unit))) {
    return String(round(line.quantity * food.serving_size_g, 1))
  }
  return ''
}

function initialChoice(line: DraftLine): Choice {
  const top = line.candidates[0]
  if (top && TIGHT_TIERS.has(top.tier)) {
    return { kind: 'food', food: top, grams: suggestedGrams(line, top) }
  }
  return { kind: 'text', text: line.text }
}

function asChosen(food: Food): Chosen {
  return {
    food_id: food.id,
    name: food.name,
    brand: food.brand,
    serving_size_g: food.serving_size_g,
    calories_kcal: food.calories_kcal,
    protein_g: food.protein_g,
    carbs_g: food.carbs_g,
    fat_g: food.fat_g,
  }
}

/** What the line said, in words: "2 cups · flour". */
function reading(line: DraftLine): string {
  const amount = [line.quantity !== null ? round(line.quantity, 3) : null, line.unit]
    .filter((p) => p !== null && p !== undefined)
    .join(' ')
  return amount ? `${amount} · ${line.name}` : line.name
}

/**
 * The step between a page and a recipe: each ingredient line matched to a
 * food, searched for by hand, or kept as words.
 *
 * The server has already read the page and searched for each line; what it
 * cannot do is decide. A line is pre-matched only when the search found the
 * food by name or by prefix — the same bar the food picker sets for its top
 * tiers — and everything looser is offered, not chosen. Nothing here is
 * saved: "Use this recipe" hands the choices to the ordinary recipe form,
 * where they can still be changed before the recipe exists.
 */
export default function RecipeImportReview({
  draft,
  onCancel,
  onUse,
}: {
  draft: RecipeDraft
  onCancel: () => void
  onUse: (items: DraftItem[]) => void
}) {
  const [choices, setChoices] = useState<Choice[]>(() => draft.lines.map(initialChoice))
  const [searching, setSearching] = useState<number | null>(null)

  const setChoice = (index: number, choice: Choice) =>
    setChoices((prev) => prev.map((c, i) => (i === index ? choice : c)))

  const select = (index: number, value: string) => {
    const line = draft.lines[index]
    if (value === 'text') {
      setChoice(index, { kind: 'text', text: line.text })
    } else if (value === 'skip') {
      setChoice(index, { kind: 'skip' })
    } else if (value === 'search') {
      setSearching(index)
    } else {
      const food = line.candidates.find((c) => c.food_id === value)
      if (food) setChoice(index, { kind: 'food', food, grams: suggestedGrams(line, food) })
    }
  }

  const pick = (food: Food) => {
    if (searching === null) return
    const chosen = asChosen(food)
    setChoice(searching, {
      kind: 'food',
      food: chosen,
      grams: suggestedGrams(draft.lines[searching], chosen),
    })
    setSearching(null)
  }

  const missingWeight = choices.filter((c) => c.kind === 'food' && !(Number(c.grams) > 0)).length
  const matched = choices.filter((c) => c.kind === 'food').length
  const asText = choices.filter((c) => c.kind === 'text').length

  const use = () => {
    const items: DraftItem[] = []
    choices.forEach((choice) => {
      if (choice.kind === 'food') {
        items.push(
          foodItem(
            {
              id: choice.food.food_id,
              name: choice.food.name,
              brand: choice.food.brand,
              calories_kcal: choice.food.calories_kcal,
              protein_g: choice.food.protein_g,
              carbs_g: choice.food.carbs_g,
              fat_g: choice.food.fat_g,
            },
            // An imported line already resolved to a weight — "200 g of
            // chicken" — so it arrives in grams. The measures are there in
            // the editor for anyone who would rather count them.
            { measure: GRAMS, count: String(Number(choice.grams)) },
          ),
        )
      } else if (choice.kind === 'text' && choice.text.trim()) {
        items.push(textItem(choice.text.trim()))
      }
    })
    onUse(items)
  }

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <h1 className="text-2xl font-semibold tracking-tight">Review the import</h1>
          <p className="text-muted-foreground truncate text-sm">
            {draft.name}
            {draft.servings ? ` · ${round(draft.servings, 2)} servings` : ''} ·{' '}
            {new URL(draft.source_url).hostname}
          </p>
        </div>
        <Button variant="ghost" size="sm" onClick={onCancel}>
          <ArrowLeft /> Back
        </Button>
      </div>

      <Alert>
        <AlertDescription>
          Each line was searched for in the food database. A line is matched only when the search
          found the food by name; anything looser is offered for you to choose. Lines kept as text
          stay in the recipe as words and are not counted in the nutrition.
        </AlertDescription>
      </Alert>

      <Card>
        <CardHeader>
          <CardTitle>Ingredients</CardTitle>
          <CardDescription>
            {draft.lines.length} line{draft.lines.length === 1 ? '' : 's'} · {matched} matched ·{' '}
            {asText} as text
          </CardDescription>
        </CardHeader>
        <CardContent>
          {draft.lines.length === 0 ? (
            <p className="text-muted-foreground text-sm">The page listed no ingredients.</p>
          ) : (
            <ul className="divide-y">
              {draft.lines.map((line, index) => {
                const choice = choices[index]
                const value =
                  choice.kind === 'food'
                    ? line.candidates.some((c) => c.food_id === choice.food.food_id)
                      ? choice.food.food_id
                      : 'search'
                    : choice.kind
                return (
                  <li
                    key={index}
                    className="grid gap-2 py-3 sm:grid-cols-[1fr_auto] sm:items-start"
                  >
                    <div className="min-w-0 space-y-1">
                      <p className="font-medium">{line.text}</p>
                      <p className="text-muted-foreground text-xs">{reading(line)}</p>
                      {choice.kind === 'text' && (
                        <Input
                          aria-label={`Text for line ${index + 1}`}
                          value={choice.text}
                          onChange={(e) => setChoice(index, { kind: 'text', text: e.target.value })}
                        />
                      )}
                      {choice.kind === 'food' && (
                        <div className="flex flex-wrap items-center gap-2">
                          <p className="text-sm">
                            {choice.food.name}
                            {choice.food.brand && (
                              <span className="text-muted-foreground text-xs">
                                {' '}
                                · {choice.food.brand}
                              </span>
                            )}
                          </p>
                          <div className="flex items-center gap-1.5">
                            <Label htmlFor={`grams-${index}`} className="sr-only">
                              Grams for line {index + 1}
                            </Label>
                            <Input
                              id={`grams-${index}`}
                              type="number"
                              min={0.1}
                              step="any"
                              className="tabular w-24 text-right"
                              placeholder="grams"
                              value={choice.grams}
                              onChange={(e) =>
                                setChoice(index, { ...choice, grams: e.target.value })
                              }
                            />
                            <span className="text-muted-foreground text-xs">g</span>
                          </div>
                          {!(Number(choice.grams) > 0) && (
                            <span className="text-muted-foreground text-xs">
                              {line.unit
                                ? `How much does ${round(line.quantity ?? 1, 3)} ${line.unit} weigh?`
                                : 'Weight needed'}
                            </span>
                          )}
                        </div>
                      )}
                    </div>
                    <Select value={value} onValueChange={(v) => select(index, v)}>
                      <SelectTrigger
                        className="w-full sm:w-64"
                        aria-label={`Match for line ${index + 1}`}
                      >
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {line.candidates.map((c) => (
                          <SelectItem key={c.food_id} value={c.food_id}>
                            {c.name}
                            {c.brand ? ` · ${c.brand}` : ''}
                            {TIGHT_TIERS.has(c.tier) ? '' : ' (maybe)'}
                          </SelectItem>
                        ))}
                        {choice.kind === 'food' &&
                          !line.candidates.some((c) => c.food_id === choice.food.food_id) && (
                            <SelectItem value="search">
                              {choice.food.name}
                              {choice.food.brand ? ` · ${choice.food.brand}` : ''}
                            </SelectItem>
                          )}
                        {!(
                          choice.kind === 'food' &&
                          !line.candidates.some((c) => c.food_id === choice.food.food_id)
                        ) && <SelectItem value="search">Search for a food…</SelectItem>}
                        <SelectItem value="text">Keep as text</SelectItem>
                        <SelectItem value="skip">Leave it out</SelectItem>
                      </SelectContent>
                    </Select>
                  </li>
                )
              })}
            </ul>
          )}
        </CardContent>
      </Card>

      <div className="flex flex-wrap items-center gap-3">
        <Button onClick={use} disabled={missingWeight > 0 || draft.lines.length === 0}>
          <Check /> Use this recipe
        </Button>
        {missingWeight > 0 && (
          <span className="text-muted-foreground text-xs">
            {missingWeight} matched line{missingWeight === 1 ? ' still needs' : 's still need'} a
            weight in grams.
          </span>
        )}
      </div>

      <Dialog open={searching !== null} onOpenChange={(open) => !open && setSearching(null)}>
        <DialogContent className="sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>
              <span className="inline-flex items-center gap-2">
                <Search className="size-4" />
                {searching !== null ? draft.lines[searching].text : 'Find a food'}
              </span>
            </DialogTitle>
          </DialogHeader>
          {searching !== null && (
            <FoodPicker key={searching} initialTerm={draft.lines[searching].name} onPick={pick} />
          )}
        </DialogContent>
      </Dialog>
    </div>
  )
}
