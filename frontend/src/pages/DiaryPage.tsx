import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { BookmarkPlus, ChevronLeft, ChevronRight, Copy, Pencil, Plus, X } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { DiaryEntry, Food, RecentRecipe, RecipeSummary } from '@/api/types'
import { addDays, amountParts, kcal, prettyDate, round, titleCase, today } from '@/lib/format'
import { cn } from '@/lib/utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardAction, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Separator } from '@/components/ui/separator'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import FoodPicker from '@/components/FoodPicker'
import AmountField from '@/components/AmountField'
import { GRAMS, amountGrams, amountRequest, defaultAmount, type Amount } from '@/lib/amounts'
import {
  Empty,
  EnergyShareRow,
  ErrorNote,
  MacroRow,
  Spinner,
  TargetList,
} from '@/components/shared'
import { useAuth } from '@/lib/auth'
import { withNetCarbs } from '@/lib/nutrients'

const MEALS = ['breakfast', 'lunch', 'dinner', 'snack']

/** A per-meal action that needs a dialog: copy it from another day, or keep it as a recipe. */
type MealAction = { kind: 'copy' | 'recipe'; meal: string }

/**
 * How much of it, as it was entered: "2 chicken breasts · 348 g".
 *
 * Both halves, because both were asked for — the phrase is how the amount was
 * decided and the weight is what the figures beside it were computed from.
 * The phrase comes from the server already pluralised; a row logged in plain
 * grams has nothing to add and says the weight once.
 *
 * The food's name is deliberately not passed to `amountParts` here. A portion
 * is often the food again — "chicken breast" of "Chicken breast fillet" — and
 * where the two sit side by side in one line that reads as a stutter; but
 * this line sits under the name rather than beside it, and has to make sense
 * read on its own.
 */
function entryAmount(entry: DiaryEntry) {
  if (entry.quantity_g === null) {
    const servings = entry.recipe_servings ?? 0
    return `${round(servings, 2)} serving${servings === 1 ? '' : 's'}`
  }
  const { phrase, weight } = amountParts(entry)
  return weight ? `${phrase} · ${weight}` : phrase
}

export default function DiaryPage() {
  const { user } = useAuth()
  const [date, setDate] = useState(today())
  const [adding, setAdding] = useState<string | null>(null)
  const [editing, setEditing] = useState<DiaryEntry | null>(null)
  const [action, setAction] = useState<MealAction | null>(null)
  const [copyNote, setCopyNote] = useState<string | null>(null)
  const queryClient = useQueryClient()

  const day = useQuery({ queryKey: ['diary', 'day', date], queryFn: () => api.diaryDay(date) })

  const remove = useMutation({
    mutationFn: (id: string) => api.deleteDiaryEntry(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['diary'] }),
  })

  // "I logged everything": the one fact the diary cannot infer. A day with
  // nothing in it and the flag on is a fast day, which is why the toggle is
  // offered on every day rather than only once something is logged.
  const setComplete = useMutation({
    mutationFn: (complete: boolean) => api.setDayComplete(date, complete),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['diary'] })
      // The estimate on the home page reads only complete days.
      queryClient.invalidateQueries({ queryKey: ['estimates'] })
    },
  })

  // The whole of yesterday onto an empty day. Offered only when the day is
  // empty, because on a day with entries it would double up rather than fill
  // in, and a per-meal copy covers the rest.
  const copyYesterday = useMutation({
    mutationFn: () => api.copyDiary({ from_date: addDays(date, -1), to_date: date }),
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['diary'] })
      queryClient.invalidateQueries({ queryKey: ['foods', 'recent'] })
      setCopyNote(
        result.copied === 0
          ? `Nothing was logged on ${prettyDate(result.from_date).toLowerCase()} to copy.`
          : null,
      )
    },
  })

  const calorieStatus = day.data?.targets.find((t) => t.nutrient === 'calories_kcal')
  const meals = day.data?.meals ?? MEALS.map((meal) => ({ meal, entries: [], total: null }))
  const dayIsEmpty = day.data !== undefined && day.data.meals.every((m) => m.entries.length === 0)

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold tracking-tight">Diary</h1>
        <div className="flex items-center gap-1">
          <Button
            variant="outline"
            size="icon"
            onClick={() => setDate(addDays(date, -1))}
            aria-label="Previous day"
          >
            <ChevronLeft />
          </Button>
          <Input
            type="date"
            className="w-auto"
            value={date}
            onChange={(e) => setDate(e.target.value || today())}
          />
          <Button
            variant="outline"
            size="icon"
            onClick={() => setDate(addDays(date, 1))}
            aria-label="Next day"
          >
            <ChevronRight />
          </Button>
          {date !== today() && (
            <Button variant="ghost" size="sm" onClick={() => setDate(today())}>
              Today
            </Button>
          )}
        </div>
      </div>

      {day.isLoading && <Spinner />}
      <ErrorNote error={day.error} />

      {day.data && (
        <Card>
          <CardHeader>
            <CardTitle>{prettyDate(date)}</CardTitle>
            <CardAction>
              <Label
                htmlFor="day-complete"
                className="text-muted-foreground flex cursor-pointer items-center gap-2 text-xs font-normal"
              >
                <Switch
                  id="day-complete"
                  checked={day.data.complete}
                  disabled={setComplete.isPending}
                  onCheckedChange={(checked) => setComplete.mutate(checked)}
                />
                I logged everything {date === today() ? 'today' : 'this day'}
              </Label>
            </CardAction>
          </CardHeader>
          <CardContent className="space-y-4">
            <ErrorNote error={setComplete.error} />
            <div className="flex flex-wrap items-baseline gap-3">
              <strong className="tabular text-3xl font-bold tracking-tight">
                {kcal(day.data.total.calories_kcal)}
              </strong>
              {calorieStatus && (
                <span
                  className={cn(
                    'text-sm',
                    calorieStatus.status === 'over' ? 'text-destructive' : 'text-muted-foreground',
                  )}
                >
                  {calorieStatus.status === 'over'
                    ? `${kcal(Math.abs(calorieStatus.remaining))} over budget`
                    : `${kcal(calorieStatus.remaining)} left`}
                </span>
              )}
            </div>
            <EnergyShareRow share={day.data.energy_share} />
            <TargetList targets={day.data.targets} />
            {dayIsEmpty && (
              <Alert>
                <AlertDescription className="w-full space-y-2">
                  <p>Nothing logged yet. Most days look like the one before.</p>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={copyYesterday.isPending}
                    onClick={() => copyYesterday.mutate()}
                  >
                    <Copy /> {copyYesterday.isPending ? 'Copying…' : 'Copy yesterday'}
                  </Button>
                  {copyNote && <p className="text-muted-foreground text-xs">{copyNote}</p>}
                </AlertDescription>
              </Alert>
            )}
            <ErrorNote error={copyYesterday.error} />
          </CardContent>
        </Card>
      )}

      {meals.map((group) => (
        <Card key={group.meal}>
          <CardHeader>
            <CardTitle>{titleCase(group.meal)}</CardTitle>
            <CardAction className="flex items-center gap-1">
              {/* A meal with entries can be kept as a recipe; any meal can be
                  filled from another day. Icons with names, so the row stays
                  one line on a phone. */}
              {group.entries.length > 0 && (
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={`Save ${group.meal} as a recipe`}
                  title="Save as recipe"
                  onClick={() => setAction({ kind: 'recipe', meal: group.meal })}
                >
                  <BookmarkPlus />
                </Button>
              )}
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={`Copy ${group.meal} from another day`}
                title="Copy from…"
                onClick={() => setAction({ kind: 'copy', meal: group.meal })}
              >
                <Copy />
              </Button>
              <Button variant="outline" size="sm" onClick={() => setAdding(group.meal)}>
                <Plus /> Add
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent>
            {group.entries.length === 0 ? (
              <Empty>Nothing logged.</Empty>
            ) : (
              <ul className="divide-y">
                {group.entries.map((entry) => (
                  <li
                    key={entry.id}
                    className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2.5 sm:flex-nowrap"
                  >
                    {/* The whole left side opens the amount again. A row you
                        can only delete meant re-picking the food to correct a
                        number, and an edit button beside the remove one is two
                        icons where the phone has room for one. */}
                    <button
                      type="button"
                      className="focus-visible:ring-ring/50 min-w-0 flex-1 basis-full rounded-sm text-left outline-none focus-visible:ring-[3px] sm:basis-auto"
                      aria-label={`Edit ${entry.name}`}
                      onClick={() => setEditing(entry)}
                    >
                      <p className="truncate font-medium">{entry.name}</p>
                      {/* Three parts on one line, and only one of them may be
                          cut: at 360 pixels a long brand in front of the
                          amount ate the weight off the end of it — "2 chicken
                          breasts · 3…" — which is the figure the amount is
                          shown with in the first place. So the brand is the
                          part that gives way, and the pencil is a sibling of
                          the text rather than its last word: inside a line
                          that truncates it laid out past the edge of the
                          card, clipped but still there as far as a layout
                          check, or a screen reader, is concerned. */}
                      <p className="text-muted-foreground flex flex-wrap items-center gap-x-1.5 text-xs">
                        {entry.brand && (
                          <span className="min-w-0 flex-1 truncate">{entry.brand} ·</span>
                        )}
                        <span className="shrink-0" data-entry-amount>
                          {entryAmount(entry)}
                        </span>
                        <Pencil className="size-3 shrink-0" />
                      </p>
                    </button>
                    <MacroRow n={entry.nutrients} compact />
                    {/* Pushed to the end of whatever line it lands on: when
                        the row wraps on a phone the remove button was ending
                        up alone under the left edge, next to nothing. */}
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      className="ml-auto"
                      aria-label={`Remove ${entry.name}`}
                      onClick={() => remove.mutate(entry.id)}
                    >
                      <X />
                    </Button>
                  </li>
                ))}
              </ul>
            )}

            {/* A subtotal only earns its space once a meal has more than one
                entry — otherwise it repeats the row above it. The exception
                is carb awareness, where the per-meal figure is the point,
                so it is always there to be found in the same place. */}
            {group.total && (group.entries.length > 1 || user?.tracking_focus === 'diabetes') && (
              <>
                <Separator className="my-2" />
                <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
                  {/* Nine nutrients leave nothing for a two-word label, which
                      then broke across two lines. It keeps its line and the
                      figures take the next one. */}
                  <span className="text-muted-foreground shrink-0 text-xs">Meal total</span>
                  <MacroRow n={group.total} compact />
                </div>
              </>
            )}
          </CardContent>
        </Card>
      ))}

      <Dialog open={adding !== null} onOpenChange={(open) => !open && setAdding(null)}>
        <DialogContent className="sm:max-w-xl">
          {adding && <AddEntry meal={adding} date={date} onDone={() => setAdding(null)} />}
        </DialogContent>
      </Dialog>

      <Dialog open={editing !== null} onOpenChange={(open) => !open && setEditing(null)}>
        <DialogContent>
          {editing && <EditEntry entry={editing} onDone={() => setEditing(null)} />}
        </DialogContent>
      </Dialog>

      <Dialog open={action !== null} onOpenChange={(open) => !open && setAction(null)}>
        <DialogContent>
          {action?.kind === 'copy' && (
            <CopyMeal meal={action.meal} date={date} onDone={() => setAction(null)} />
          )}
          {action?.kind === 'recipe' && (
            <SaveMealAsRecipe meal={action.meal} date={date} onDone={() => setAction(null)} />
          )}
        </DialogContent>
      </Dialog>
    </div>
  )
}

/** Fill one meal from the same meal on another day. */
function CopyMeal({ meal, date, onDone }: { meal: string; date: string; onDone: () => void }) {
  const [from, setFrom] = useState(addDays(date, -1))
  const [note, setNote] = useState<string | null>(null)
  const queryClient = useQueryClient()

  const copy = useMutation({
    mutationFn: () => api.copyDiary({ from_date: from, to_date: date, meal }),
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['diary'] })
      queryClient.invalidateQueries({ queryKey: ['foods', 'recent'] })
      if (result.copied === 0) {
        setNote(`No ${meal} was logged on ${prettyDate(from).toLowerCase()}.`)
      } else {
        onDone()
      }
    },
  })

  return (
    <>
      <DialogHeader>
        <DialogTitle>Copy {meal} from another day</DialogTitle>
        <DialogDescription>
          The same things in the same amounts, added to {meal} on {prettyDate(date).toLowerCase()}.
          Nothing already there is touched.
        </DialogDescription>
      </DialogHeader>
      <div className="space-y-1.5">
        <Label htmlFor="copy-from">Copy from</Label>
        <Input
          id="copy-from"
          type="date"
          value={from}
          max={today()}
          onChange={(e) => {
            setFrom(e.target.value || addDays(date, -1))
            setNote(null)
          }}
        />
      </div>
      {note && <p className="text-muted-foreground text-sm">{note}</p>}
      <ErrorNote error={copy.error} />
      <DialogFooter>
        <Button variant="outline" onClick={onDone}>
          Cancel
        </Button>
        <Button disabled={copy.isPending || from === date} onClick={() => copy.mutate()}>
          <Copy /> {copy.isPending ? 'Copying…' : 'Copy'}
        </Button>
      </DialogFooter>
    </>
  )
}

/** Keep a logged meal as a recipe, entries as they were logged. */
function SaveMealAsRecipe({
  meal,
  date,
  onDone,
}: {
  meal: string
  date: string
  onDone: () => void
}) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [name, setName] = useState(`${titleCase(meal)}, ${prettyDate(date)}`)
  const [servings, setServings] = useState('1')

  const save = useMutation({
    mutationFn: () =>
      api.recipeFromMeal({ date, meal, name: name.trim(), servings: Number(servings) || 1 }),
    onSuccess: (recipe) => {
      queryClient.invalidateQueries({ queryKey: ['recipes'] })
      onDone()
      navigate(`/recipes/${recipe.id}`)
    },
  })

  return (
    <>
      <DialogHeader>
        <DialogTitle>Save {meal} as a recipe</DialogTitle>
        <DialogDescription>
          Everything logged for {meal} on {prettyDate(date).toLowerCase()} becomes the ingredient
          list, in the amounts you logged. A recipe you logged stays a recipe inside it.
        </DialogDescription>
      </DialogHeader>
      <div className="grid gap-3 sm:grid-cols-[1fr_7rem]">
        <div className="space-y-1.5">
          <Label htmlFor="meal-recipe-name">Name</Label>
          <Input
            id="meal-recipe-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoFocus
          />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="meal-recipe-servings">Servings</Label>
          <Input
            id="meal-recipe-servings"
            type="number"
            min={0.1}
            step="any"
            value={servings}
            onChange={(e) => setServings(e.target.value)}
          />
        </div>
      </div>
      <p className="text-muted-foreground text-xs">
        Servings is how many the meal was: 1 if you ate all of it.
      </p>
      <ErrorNote error={save.error} />
      <DialogFooter>
        <Button variant="outline" onClick={onDone}>
          Cancel
        </Button>
        <Button disabled={save.isPending || !name.trim()} onClick={() => save.mutate()}>
          <BookmarkPlus /> {save.isPending ? 'Saving…' : 'Save recipe'}
        </Button>
      </DialogFooter>
    </>
  )
}

/**
 * Change how much of something was logged, without logging it again.
 *
 * The same control the add dialog uses, so correcting an amount and entering
 * one are the same gesture. The food is fetched rather than taken from the
 * entry: an entry carries its name and its weight, not the measures the food
 * can be counted in, and those are the whole point of being here.
 */
function EditEntry({ entry, onDone }: { entry: DiaryEntry; onDone: () => void }) {
  const queryClient = useQueryClient()
  const [amount, setAmount] = useState<Amount>({
    measure: GRAMS,
    count: entry.quantity_g === null ? '' : String(round(entry.quantity_g, 1)),
  })
  const [servings, setServings] = useState(String(round(entry.recipe_servings ?? 1, 2)))
  // Set once the food arrives, so re-fetching it cannot overwrite a measure
  // that has since been picked.
  const [ready, setReady] = useState(false)

  const food = useQuery({
    queryKey: ['foods', entry.food_id],
    queryFn: () => api.getFood(entry.food_id!),
    enabled: entry.food_id !== null,
  })

  useEffect(() => {
    if (!food.data || ready) return
    setAmount(defaultAmount(food.data, entry.quantity_g))
    setReady(true)
  }, [food.data, ready, entry.quantity_g])

  const save = useMutation({
    mutationFn: () =>
      api.updateDiaryEntry(entry.id, {
        ...(entry.food_id ? amountRequest(amount) : { recipe_servings: Number(servings) }),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['diary'] })
      queryClient.invalidateQueries({ queryKey: ['foods', 'recent'] })
      onDone()
    },
  })

  const usable = entry.food_id ? amountGrams(amount) !== null : Number(servings) > 0

  return (
    <>
      <DialogHeader>
        <DialogTitle className="truncate">{entry.name}</DialogTitle>
        <DialogDescription>
          {titleCase(entry.meal)}, {prettyDate(entry.logged_on).toLowerCase()}
        </DialogDescription>
      </DialogHeader>

      {entry.food_id ? (
        <>
          {food.isLoading && <Spinner label="Loading the measures for this food…" />}
          <ErrorNote error={food.error} />
          {food.data && (
            <AmountField
              food={food.data}
              value={amount}
              onChange={setAmount}
              idPrefix="edit-amount"
              autoFocus
            />
          )}
        </>
      ) : (
        <div className="space-y-1.5">
          <Label htmlFor="edit-servings">Servings</Label>
          <Input
            id="edit-servings"
            inputMode="decimal"
            autoFocus
            className="tabular pointer-coarse:h-11"
            value={servings}
            onFocus={(e) => e.currentTarget.select()}
            onChange={(e) => setServings(e.target.value)}
          />
        </div>
      )}

      <ErrorNote error={save.error} />
      <DialogFooter>
        <Button variant="outline" onClick={onDone}>
          Cancel
        </Button>
        <Button disabled={save.isPending || !usable} onClick={() => save.mutate()}>
          {save.isPending ? 'Saving…' : 'Save'}
        </Button>
      </DialogFooter>
    </>
  )
}

/** What the recipe tab needs of a recipe, met by a summary and by a recent item alike. */
type PickedRecipe = Pick<RecipeSummary, 'id' | 'name' | 'per_serving'>

function AddEntry({ meal, date, onDone }: { meal: string; date: string; onDone: () => void }) {
  const [mode, setMode] = useState<'food' | 'recipe'>('food')
  const [picked, setPicked] = useState<Food | null>(null)
  const [pickedRecipe, setPickedRecipe] = useState<PickedRecipe | null>(null)
  const [amount, setAmount] = useState<Amount>({ measure: GRAMS, count: '100' })
  const [servings, setServings] = useState('1')
  const queryClient = useQueryClient()

  // From the picker's recent list: the amount used last time comes with the
  // pick, so logging the usual is two taps rather than a search and a number.
  // What it lands on when there is no last time is `defaultAmount`'s business.
  const pickFood = (food: Food, lastGrams?: number) => {
    setPicked(food)
    setAmount(defaultAmount(food, lastGrams))
  }
  const pickRecipe = (recipe: RecentRecipe, lastServings: number) => {
    setMode('recipe')
    setPickedRecipe(recipe)
    setServings(String(round(lastServings, 2)))
  }

  const recipes = useQuery({
    queryKey: ['recipes', 'all'],
    queryFn: () => api.listRecipes({ scope: 'all' }),
    enabled: mode === 'recipe',
  })

  const log = useMutation({
    mutationFn: () =>
      picked
        ? api.logDiaryEntry({
            logged_on: date,
            meal,
            food_id: picked.id,
            ...amountRequest(amount),
          })
        : api.logDiaryEntry({
            logged_on: date,
            meal,
            recipe_id: pickedRecipe!.id,
            recipe_servings: Number(servings),
          }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['diary'] })
      queryClient.invalidateQueries({ queryKey: ['foods', 'recent'] })
      onDone()
    },
  })

  // Preview using the same grams/100 scaling the server applies, so what you
  // see before committing is what gets stored.
  const weight = amountGrams(amount)
  const preview =
    picked && weight !== null
      ? withNetCarbs({
          calories_kcal: (picked.calories_kcal * weight) / 100,
          protein_g: (picked.protein_g * weight) / 100,
          carbs_g: (picked.carbs_g * weight) / 100,
          fat_g: (picked.fat_g * weight) / 100,
          fiber_g: ((picked.fiber_g ?? 0) * weight) / 100,
          sugar_g: ((picked.sugar_g ?? 0) * weight) / 100,
          saturated_fat_g: ((picked.saturated_fat_g ?? 0) * weight) / 100,
          sodium_mg: ((picked.sodium_mg ?? 0) * weight) / 100,
        })
      : pickedRecipe && Number(servings) > 0
        ? withNetCarbs({
            calories_kcal: pickedRecipe.per_serving.calories_kcal * Number(servings),
            protein_g: pickedRecipe.per_serving.protein_g * Number(servings),
            carbs_g: pickedRecipe.per_serving.carbs_g * Number(servings),
            fat_g: pickedRecipe.per_serving.fat_g * Number(servings),
            fiber_g: pickedRecipe.per_serving.fiber_g * Number(servings),
            sugar_g: pickedRecipe.per_serving.sugar_g * Number(servings),
            saturated_fat_g: pickedRecipe.per_serving.saturated_fat_g * Number(servings),
            sodium_mg: pickedRecipe.per_serving.sodium_mg * Number(servings),
          })
        : null

  return (
    <>
      <DialogHeader>
        <DialogTitle>Add to {titleCase(meal)}</DialogTitle>
        <DialogDescription>{prettyDate(date)}</DialogDescription>
      </DialogHeader>

      <Tabs
        value={mode}
        onValueChange={(v) => {
          setMode(v as typeof mode)
          setPicked(null)
          setPickedRecipe(null)
        }}
      >
        <TabsList className="grid w-full grid-cols-2">
          <TabsTrigger value="food">Food</TabsTrigger>
          <TabsTrigger value="recipe">Recipe</TabsTrigger>
        </TabsList>

        <TabsContent value="food" className="pt-3">
          {picked ? (
            <div className="space-y-4">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="truncate font-semibold">{picked.name}</p>
                  {picked.brand && (
                    <p className="text-muted-foreground truncate text-xs">{picked.brand}</p>
                  )}
                </div>
                <Button variant="ghost" size="sm" onClick={() => setPicked(null)}>
                  Change
                </Button>
              </div>

              {/* How many, of what. A count beside a unit rather than a row
                  of buttons that each set a gram figure: "two chicken
                  breasts" is the thing being said, and a button that
                  silently wrote 174 into a box could only ever say one.
                  What is stored is still grams, and the weight is right
                  there under the count so it is never a mystery. */}
              <AmountField food={picked} value={amount} onChange={setAmount} autoFocus />

              {preview && <MacroRow n={preview} />}
              <ErrorNote error={log.error} />
              <Button
                className="w-full"
                disabled={log.isPending || weight === null}
                onClick={() => log.mutate()}
              >
                {log.isPending ? 'Saving…' : 'Log it'}
              </Button>
            </div>
          ) : (
            <FoodPicker onPick={pickFood} onPickRecipe={pickRecipe} />
          )}
        </TabsContent>

        <TabsContent value="recipe" className="pt-3">
          {pickedRecipe ? (
            <div className="space-y-4">
              <div className="flex items-start justify-between gap-3">
                <p className="truncate font-semibold">{pickedRecipe.name}</p>
                <Button variant="ghost" size="sm" onClick={() => setPickedRecipe(null)}>
                  Change
                </Button>
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="servings">Servings</Label>
                <Input
                  id="servings"
                  type="number"
                  min={0.1}
                  step="any"
                  value={servings}
                  onChange={(e) => setServings(e.target.value)}
                  autoFocus
                />
              </div>
              {preview && <MacroRow n={preview} />}
              <ErrorNote error={log.error} />
              <Button
                className="w-full"
                disabled={log.isPending || !(Number(servings) > 0)}
                onClick={() => log.mutate()}
              >
                {log.isPending ? 'Saving…' : 'Log it'}
              </Button>
            </div>
          ) : (
            <div className="max-h-[46vh] space-y-1.5 overflow-y-auto">
              {recipes.isLoading && <Spinner />}
              <ErrorNote error={recipes.error} />
              {recipes.data?.length === 0 && <Empty>No recipes yet.</Empty>}
              {recipes.data?.map((recipe) => (
                <button
                  key={recipe.id}
                  type="button"
                  onClick={() => setPickedRecipe(recipe)}
                  className="hover:border-primary hover:bg-accent focus-visible:ring-ring/50 flex w-full items-center gap-3 rounded-md border px-3 py-2 text-left text-sm outline-none transition-colors focus-visible:ring-[3px]"
                >
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">{recipe.name}</span>
                    <span className="text-muted-foreground block truncate text-xs">
                      {kcal(recipe.per_serving.calories_kcal)} per serving
                      {recipe.author ? ` · by ${recipe.author}` : ''}
                    </span>
                  </span>
                </button>
              ))}
            </div>
          )}
        </TabsContent>
      </Tabs>
    </>
  )
}
