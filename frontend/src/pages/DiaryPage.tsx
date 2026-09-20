import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ChevronLeft, ChevronRight, Plus, X } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { Food, RecipeSummary } from '@/api/types'
import { addDays, grams, kcal, prettyDate, round, titleCase, today } from '@/lib/format'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Card, CardAction, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Separator } from '@/components/ui/separator'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import FoodPicker from '@/components/FoodPicker'
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

export default function DiaryPage() {
  const { user } = useAuth()
  const [date, setDate] = useState(today())
  const [adding, setAdding] = useState<string | null>(null)
  const queryClient = useQueryClient()

  const day = useQuery({ queryKey: ['diary', 'day', date], queryFn: () => api.diaryDay(date) })

  const remove = useMutation({
    mutationFn: (id: string) => api.deleteDiaryEntry(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['diary'] }),
  })

  const calorieStatus = day.data?.targets.find((t) => t.nutrient === 'calories_kcal')
  const meals = day.data?.meals ?? MEALS.map((meal) => ({ meal, entries: [], total: null }))

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
          </CardHeader>
          <CardContent className="space-y-4">
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
          </CardContent>
        </Card>
      )}

      {meals.map((group) => (
        <Card key={group.meal}>
          <CardHeader>
            <CardTitle>{titleCase(group.meal)}</CardTitle>
            <CardAction>
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
                    <div className="min-w-0 flex-1 basis-full sm:basis-auto">
                      <p className="truncate font-medium">{entry.name}</p>
                      <p className="text-muted-foreground truncate text-xs">
                        {entry.brand ? `${entry.brand} · ` : ''}
                        {entry.quantity_g != null
                          ? grams(entry.quantity_g, 0)
                          : `${round(entry.recipe_servings ?? 0, 2)} serving${
                              (entry.recipe_servings ?? 0) === 1 ? '' : 's'
                            }`}
                      </p>
                    </div>
                    <MacroRow n={entry.nutrients} compact />
                    <Button
                      variant="ghost"
                      size="icon-sm"
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
                <div className="flex items-center justify-between gap-3">
                  <span className="text-muted-foreground text-xs">Meal total</span>
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
    </div>
  )
}

function AddEntry({ meal, date, onDone }: { meal: string; date: string; onDone: () => void }) {
  const [mode, setMode] = useState<'food' | 'recipe'>('food')
  const [picked, setPicked] = useState<Food | null>(null)
  const [pickedRecipe, setPickedRecipe] = useState<RecipeSummary | null>(null)
  const [amount, setAmount] = useState('100')
  const [servings, setServings] = useState('1')
  const queryClient = useQueryClient()

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
            quantity_g: Number(amount),
          })
        : api.logDiaryEntry({
            logged_on: date,
            meal,
            recipe_id: pickedRecipe!.id,
            recipe_servings: Number(servings),
          }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['diary'] })
      onDone()
    },
  })

  // Preview using the same grams/100 scaling the server applies, so what you
  // see before committing is what gets stored.
  const preview =
    picked && Number(amount) > 0
      ? withNetCarbs({
          calories_kcal: (picked.calories_kcal * Number(amount)) / 100,
          protein_g: (picked.protein_g * Number(amount)) / 100,
          carbs_g: (picked.carbs_g * Number(amount)) / 100,
          fat_g: (picked.fat_g * Number(amount)) / 100,
          fiber_g: ((picked.fiber_g ?? 0) * Number(amount)) / 100,
          sugar_g: ((picked.sugar_g ?? 0) * Number(amount)) / 100,
          saturated_fat_g: ((picked.saturated_fat_g ?? 0) * Number(amount)) / 100,
          sodium_mg: ((picked.sodium_mg ?? 0) * Number(amount)) / 100,
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

              <div className="space-y-1.5">
                <Label htmlFor="amount">Amount (grams)</Label>
                <Input
                  id="amount"
                  type="number"
                  min={1}
                  step="any"
                  value={amount}
                  onChange={(e) => setAmount(e.target.value)}
                  autoFocus
                />
              </div>

              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setAmount(String(picked.serving_size_g))}
                >
                  1 serving ({grams(picked.serving_size_g, 0)}
                  {picked.serving_label ? ` · ${picked.serving_label}` : ''})
                </Button>
                <Button variant="outline" size="sm" onClick={() => setAmount('100')}>
                  100 g
                </Button>
              </div>

              {preview && <MacroRow n={preview} />}
              <ErrorNote error={log.error} />
              <Button
                className="w-full"
                disabled={log.isPending || !(Number(amount) > 0)}
                onClick={() => log.mutate()}
              >
                {log.isPending ? 'Saving…' : 'Log it'}
              </Button>
            </div>
          ) : (
            <FoodPicker onPick={setPicked} />
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
