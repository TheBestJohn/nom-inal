import { useEffect, useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Barcode, BookOpen, Globe, History, Plus, Search } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { ExternalFood, Food, RecentItem, RecentRecipe } from '@/api/types'
import { grams, kcal, prettyDate, round, sourceLabel } from '@/lib/format'
import { useFoodSearch, type SearchTier } from '@/lib/useFoodSearch'
import { cn } from '@/lib/utils'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { ErrorNote, SourceBadge, Spinner } from '@/components/shared'
import BarcodeScanner from '@/components/BarcodeScanner'
import FoodForm from '@/components/FoodForm'

/** How each tier is labelled in the list. */
const TIER_LABEL: Record<SearchTier, string> = {
  exact: 'Exact match',
  prefix: 'Starts with',
  contains: 'Contains',
  fuzzy: 'Did you mean',
}

/**
 * One surface over all three food sources.
 *
 * The library tab streams from the SSE endpoint, so results appear tier by
 * tier as the server finds them rather than all at once when the slowest query
 * finishes. External hits are not foods yet — picking one imports it first
 * (idempotent on source + id) and hands back a real local food, so callers
 * never care where it came from.
 *
 * Before anything is typed, the list is what you logged recently, each with
 * the amount you used last time: most meals are the same things as last
 * time, and the search box is for the rest. Recipes are in that list too,
 * since they are logged like foods; a caller that can log one passes
 * `onPickRecipe`, and one that cannot simply does not see them.
 */
export default function FoodPicker({
  onPick,
  onPickRecipe,
  autoFocus = true,
  initialTerm = '',
}: {
  /** `grams` is a suggestion — the amount used last time, when there was one. */
  onPick: (food: Food, grams?: number) => void
  onPickRecipe?: (recipe: RecentRecipe, servings: number) => void
  autoFocus?: boolean
  /** What the search box opens with: the recipe importer passes the
   *  ingredient it is trying to place, so the search is already running. */
  initialTerm?: string
}) {
  const [term, setTerm] = useState(initialTerm)
  const [debounced, setDebounced] = useState(initialTerm.trim())
  const [barcode, setBarcode] = useState('')
  const [submittedBarcode, setSubmittedBarcode] = useState('')
  const [externalTerm, setExternalTerm] = useState('')
  // When set, the picker swaps to the create form rather than opening a second
  // dialog on top of the one it is already inside.
  const [creating, setCreating] = useState<string | null>(null)
  const queryClient = useQueryClient()

  // Short debounce: the stream is fast enough that this is about not opening a
  // connection per keystroke, not about hiding latency.
  useEffect(() => {
    const id = setTimeout(() => setDebounced(term.trim()), 180)
    return () => clearTimeout(id)
  }, [term])

  const search = useFoodSearch(debounced, { limit: 8 })

  const recent = useQuery({
    queryKey: ['foods', 'recent'],
    queryFn: () => api.recentFoods(12),
  })
  // Recipes only when the caller can log one.
  const recentItems = (recent.data ?? []).filter((item) => item.food || onPickRecipe)

  const external = useQuery({
    queryKey: ['foods', 'external', externalTerm],
    queryFn: () => api.searchExternal(externalTerm),
    enabled: externalTerm.trim().length >= 2,
  })

  const lookup = useQuery({
    queryKey: ['foods', 'barcode', submittedBarcode],
    queryFn: () => api.lookupBarcode(submittedBarcode),
    enabled: submittedBarcode.length >= 6,
    retry: false,
  })

  const importFood = useMutation({
    mutationFn: (food: ExternalFood) => api.importFood(food),
    onSuccess: (food) => {
      queryClient.invalidateQueries({ queryKey: ['foods'] })
      onPick(food)
    },
  })

  // Group by tier so the list can show why each block matched.
  const grouped = useMemo(() => {
    const out: { tier: SearchTier; foods: typeof search.results }[] = []
    for (const food of search.results) {
      const last = out[out.length - 1]
      if (last && last.tier === food.tier) last.foods.push(food)
      else out.push({ tier: food.tier, foods: [food] })
    }
    return out
  }, [search.results])

  if (creating !== null) {
    return (
      <div className="space-y-3">
        <Button variant="ghost" size="sm" className="px-0" onClick={() => setCreating(null)}>
          <ArrowLeft /> Back to search
        </Button>
        <FoodForm
          initialName={creating}
          submitLabel="Create and use"
          onCancel={() => setCreating(null)}
          // Straight into the amount step: creating the food was a detour from
          // logging it, not the goal.
          onSaved={(food) => onPick(food)}
        />
      </div>
    )
  }

  return (
    <Tabs defaultValue="library" className="gap-3">
      <TabsList className="grid w-full grid-cols-3">
        <TabsTrigger value="library">
          <Search /> Search
        </TabsTrigger>
        <TabsTrigger value="external">
          <Globe /> Databases
        </TabsTrigger>
        <TabsTrigger value="barcode">
          <Barcode /> Barcode
        </TabsTrigger>
      </TabsList>

      <TabsContent value="library" className="space-y-3">
        <Input
          placeholder="Search foods — typos are fine…"
          value={term}
          onChange={(e) => setTerm(e.target.value)}
          autoFocus={autoFocus}
        />

        {search.error && <ErrorNote error={search.error} />}

        <div className="max-h-[46vh] space-y-3 overflow-y-auto">
          {grouped.map((group) => (
            <div key={group.tier} className="space-y-1.5">
              <p className="text-muted-foreground px-1 text-[11px] font-medium tracking-wide uppercase">
                {TIER_LABEL[group.tier]}
              </p>
              {group.foods.map((food) => (
                <FoodRow key={food.id} food={food} onPick={onPick} />
              ))}
            </div>
          ))}

          {search.searching && <Spinner label="Searching…" />}

          {!search.searching && debounced && search.results.length === 0 && (
            <Alert>
              <AlertDescription className="w-full space-y-2">
                <p>
                  Nothing matched “{debounced}”. Try the <strong>Databases</strong> or{' '}
                  <strong>Barcode</strong> tab, or add it yourself.
                </p>
                <Button variant="outline" size="sm" onClick={() => setCreating(debounced)}>
                  <Plus /> Create “{debounced}”
                </Button>
              </AlertDescription>
            </Alert>
          )}

          {!debounced && (
            <div className="space-y-1.5">
              {recentItems.length > 0 && (
                <p className="text-muted-foreground flex items-center gap-1 px-1 text-[11px] font-medium tracking-wide uppercase">
                  <History className="size-3" /> Recent
                </p>
              )}
              {recent.isLoading && <Spinner label="Loading what you logged recently…" />}
              <ErrorNote error={recent.error} />
              {recentItems.map((item) => (
                <RecentRow
                  key={item.food?.id ?? item.recipe!.id}
                  item={item}
                  onPick={onPick}
                  onPickRecipe={onPickRecipe}
                />
              ))}
              {recent.data && recentItems.length === 0 && (
                <p className="text-muted-foreground py-2 text-sm">
                  Start typing to search every food. What you log will show up here for next time.
                </p>
              )}
            </div>
          )}
        </div>

        <div className="flex items-center justify-between gap-3">
          <Button variant="ghost" size="sm" className="px-0" onClick={() => setCreating(debounced)}>
            <Plus /> New food
          </Button>
          {search.elapsedMs !== null && search.results.length > 0 && (
            <p className="text-muted-foreground text-[11px]">
              {search.results.length} result{search.results.length === 1 ? '' : 's'} in{' '}
              {search.elapsedMs}ms
            </p>
          )}
        </div>
      </TabsContent>

      <TabsContent value="external" className="space-y-3">
        <Input
          placeholder="Search USDA and Open Food Facts…"
          value={externalTerm}
          onChange={(e) => setExternalTerm(e.target.value)}
        />
        {external.isFetching && <Spinner label="Searching food databases…" />}
        <ErrorNote error={external.error} />
        <ErrorNote error={importFood.error} />
        {external.data?.unavailable.map((reason) => (
          <Alert variant="warning" key={reason}>
            <AlertDescription>{reason}</AlertDescription>
          </Alert>
        ))}
        <div className="max-h-[46vh] space-y-1.5 overflow-y-auto">
          {external.data?.results.map((food) => (
            <button
              key={`${food.source}-${food.source_id}`}
              type="button"
              disabled={importFood.isPending}
              onClick={() => importFood.mutate(food)}
              className={rowClass}
            >
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">{food.name}</span>
                <span className="text-muted-foreground block truncate text-xs">
                  {food.brand ? `${food.brand} · ` : ''}
                  {kcal(food.calories_kcal)} / 100 g · P {round(food.protein_g)} C{' '}
                  {round(food.carbs_g)} F {round(food.fat_g)}
                </span>
              </span>
              <SourceBadge source={food.source} />
            </button>
          ))}
          {external.data?.results.length === 0 && !external.isFetching && (
            <p className="text-muted-foreground py-2 text-sm">No matches.</p>
          )}
        </div>
      </TabsContent>

      <TabsContent value="barcode" className="space-y-3">
        {/* A code read off the packet lands in the same box as one typed, so
            what happens next is the same lookup either way. */}
        <BarcodeScanner
          onDetected={(code) => {
            const digits = code.replace(/\D/g, '')
            setBarcode(digits)
            setSubmittedBarcode(digits)
          }}
        />
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault()
            setSubmittedBarcode(barcode.replace(/\D/g, ''))
          }}
        >
          <Input
            inputMode="numeric"
            placeholder="Or type a UPC / EAN"
            aria-label="Barcode"
            value={barcode}
            onChange={(e) => setBarcode(e.target.value)}
          />
          <Button type="submit">Look up</Button>
        </form>

        {lookup.isFetching && <Spinner label="Looking up barcode…" />}
        <ErrorNote error={lookup.error} />
        <ErrorNote error={importFood.error} />

        {lookup.data?.local && (
          <>
            <Alert>
              <AlertDescription>Already in the food database.</AlertDescription>
            </Alert>
            <FoodRow food={lookup.data.local} onPick={onPick} />
          </>
        )}

        {lookup.data?.external && !lookup.data.local && (
          <button
            type="button"
            disabled={importFood.isPending}
            onClick={() => importFood.mutate(lookup.data!.external!)}
            className={rowClass}
          >
            <span className="min-w-0 flex-1">
              <span className="block truncate font-medium">{lookup.data.external.name}</span>
              <span className="text-muted-foreground block truncate text-xs">
                {lookup.data.external.brand ? `${lookup.data.external.brand} · ` : ''}
                {sourceLabel(lookup.data.external.source)} ·{' '}
                {kcal(lookup.data.external.calories_kcal)} / 100 g
              </span>
            </span>
            <Badge>Import</Badge>
          </button>
        )}
      </TabsContent>
    </Tabs>
  )
}

const rowClass = cn(
  'flex w-full items-center gap-3 rounded-md border bg-card px-3 py-2 text-left text-sm',
  'hover:border-primary hover:bg-accent transition-colors',
  'focus-visible:ring-ring/50 focus-visible:ring-[3px] outline-none',
  'disabled:cursor-wait disabled:opacity-60',
)

function FoodRow({ food, onPick }: { food: Food; onPick: (food: Food) => void }) {
  return (
    <button type="button" onClick={() => onPick(food)} className={rowClass}>
      <span className="min-w-0 flex-1">
        <span className="block truncate font-medium">{food.name}</span>
        <span className="text-muted-foreground block truncate text-xs">
          {food.brand ? `${food.brand} · ` : ''}
          {kcal(food.calories_kcal)} / 100 g
        </span>
      </span>
      <SourceBadge source={food.source} />
    </button>
  )
}

/**
 * One thing logged before. The second line says how it was logged last time
 * and what that came to, because that is what makes it one tap rather than
 * a search followed by an amount.
 */
function RecentRow({
  item,
  onPick,
  onPickRecipe,
}: {
  item: RecentItem
  onPick: (food: Food, grams?: number) => void
  onPickRecipe?: (recipe: RecentRecipe, servings: number) => void
}) {
  const when = prettyDate(item.last_logged_on)
  const times = item.times_logged > 1 ? ` · ${item.times_logged}×` : ''

  if (item.food) {
    const food = item.food
    const last = item.last_quantity_g ?? food.serving_size_g
    return (
      <button
        type="button"
        onClick={() => onPick(food, last)}
        className={rowClass}
        aria-label={`${food.name}, ${grams(last, 0)} as last time`}
      >
        <span className="min-w-0 flex-1">
          <span className="block truncate font-medium">{food.name}</span>
          <span className="text-muted-foreground block truncate text-xs">
            {food.brand ? `${food.brand} · ` : ''}
            {grams(last, 0)} · {kcal((food.calories_kcal * last) / 100)} · {when}
            {times}
          </span>
        </span>
        <SourceBadge source={food.source} />
      </button>
    )
  }

  const recipe = item.recipe!
  const servings = item.last_recipe_servings ?? 1
  return (
    <button
      type="button"
      onClick={() => onPickRecipe?.(recipe, servings)}
      className={rowClass}
      aria-label={`${recipe.name}, ${round(servings, 2)} serving${servings === 1 ? '' : 's'} as last time`}
    >
      <span className="min-w-0 flex-1">
        <span className="block truncate font-medium">{recipe.name}</span>
        <span className="text-muted-foreground block truncate text-xs">
          {round(servings, 2)} serving{servings === 1 ? '' : 's'} ·{' '}
          {kcal(recipe.per_serving.calories_kcal * servings)} · {when}
          {times}
        </span>
      </span>
      <Badge variant="outline" className="text-[10px] tracking-wide uppercase">
        <BookOpen className="size-3" /> Recipe
      </Badge>
    </button>
  )
}
