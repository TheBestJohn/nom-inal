import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { ExternalLink } from 'lucide-react'

import type { Nutrients, Recipe } from '@/api/types'
import { grams, kcal, round } from '@/lib/format'
import { instructionSteps } from '@/lib/recipeText'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Empty, MacroRow } from '@/components/shared'

/**
 * The totals block, shared by the read view and the editor so the two can
 * never drift apart in what they say or how they say it.
 */
export function NutritionCard({
  total,
  perServing,
  servings,
  weight,
  untracked,
  someNested,
  live,
}: {
  total: Nutrients
  perServing: Nutrients
  servings: number
  weight: number
  untracked: number
  someNested: boolean
  /** Whether the figures are recomputing from a draft, or are the saved ones. */
  live: boolean
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Nutrition</CardTitle>
        {live && (
          <CardDescription>Recomputed as you edit, the same way the server does.</CardDescription>
        )}
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="grid gap-4 sm:grid-cols-2">
          <div className="space-y-1">
            <p className="text-muted-foreground text-xs">Whole recipe · {grams(weight, 0)}</p>
            <MacroRow n={total} />
          </div>
          <div className="space-y-1">
            <p className="text-muted-foreground text-xs">
              Per serving ({round(servings, 2)} servings)
            </p>
            <MacroRow n={perServing} />
          </div>
        </div>

        {/* Said plainly rather than left to be inferred from the ingredient
            list. A total that silently omits three ingredients is worse than
            no total, and the count includes ones inside sub-recipes, which
            you cannot see from this page at all. */}
        {untracked > 0 && (
          <p className="text-muted-foreground text-xs">
            Excludes {untracked} ingredient{untracked === 1 ? '' : 's'} with no nutrition
            information
            {someNested ? ', some inside a sub-recipe' : ''}.
          </p>
        )}
      </CardContent>
    </Card>
  )
}

/**
 * A recipe to read: the name, the description as a paragraph, the
 * ingredients as a list and the method as numbered steps, with the
 * nutrition underneath.
 *
 * One rendering for two pages. The signed-in recipe page wraps it with the
 * edit, share and export controls and an authenticated photo strip; the
 * public page, reachable without signing in, wraps it with a plain header
 * and photos from the public route. What sits between — the recipe itself —
 * is this, so the two can never show the same recipe differently.
 */
export default function RecipeReadout({
  recipe,
  actions,
  note,
  photos,
  subRecipeHref,
}: {
  recipe: Recipe
  /** Controls beside the title: edit, back, copy link. Not printed. */
  actions?: ReactNode
  /** A line under the title: who shared it, or where it came from. */
  note?: ReactNode
  /** The photo strip, whichever route it loads from. */
  photos?: ReactNode
  /**
   * Where a sub-recipe's name links to. Left out, the name is plain text:
   * the public page cannot know whether a sub-recipe is itself shared, and
   * a link that lands on a 404 is worse than no link.
   */
  subRecipeHref?: (id: string) => string
}) {
  const steps = recipe.instructions ? instructionSteps(recipe.instructions) : []

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <h1 className="text-2xl font-semibold tracking-tight">{recipe.name}</h1>
          <p className="text-muted-foreground text-sm">
            {round(recipe.servings, 2)} serving
            {recipe.servings === 1 ? '' : 's'} · {grams(recipe.total_weight_g, 0)}
            {recipe.is_public && recipe.is_owner ? ' · shared' : ''}
          </p>
        </div>
        {actions && <div className="flex flex-wrap items-center gap-2 print:hidden">{actions}</div>}
      </div>

      {note}

      {recipe.description && (
        // Whitespace kept: a description with a line break in it was typed
        // with a line break in it.
        <p className="text-muted-foreground max-w-prose text-sm leading-relaxed whitespace-pre-wrap">
          {recipe.description}
        </p>
      )}

      {photos && <div className="print-photos">{photos}</div>}

      <Card className="print:break-inside-avoid">
        <CardHeader>
          <CardTitle>Ingredients</CardTitle>
        </CardHeader>
        <CardContent>
          {recipe.items.length === 0 ? (
            <Empty>No ingredients.</Empty>
          ) : (
            <ul className="divide-y">
              {recipe.items.map((item) => (
                <li key={item.id} className="flex items-baseline gap-3 py-2">
                  {/* Amount first, as on a recipe card, and in a fixed column
                      so the names line up under each other. */}
                  <span className="text-muted-foreground tabular w-24 shrink-0 text-right text-sm">
                    {item.label
                      ? ''
                      : item.sub_recipe_id
                        ? `${round(item.servings ?? 0, 2)} serving${item.servings === 1 ? '' : 's'}`
                        : grams(item.quantity_g, 0)}
                  </span>
                  <span className="min-w-0 flex-1">
                    {item.sub_recipe_id && subRecipeHref ? (
                      <Link
                        to={subRecipeHref(item.sub_recipe_id)}
                        className="hover:text-primary inline-flex items-center gap-1.5 font-medium underline-offset-4 hover:underline"
                      >
                        {item.name}
                        <ExternalLink className="size-3.5 shrink-0 print:hidden" />
                      </Link>
                    ) : (
                      <span className="font-medium">{item.name}</span>
                    )}
                    {item.brand && (
                      <span className="text-muted-foreground text-xs"> · {item.brand}</span>
                    )}
                    {item.variant_label && (
                      <span className="text-muted-foreground text-xs"> · {item.variant_label}</span>
                    )}
                    {item.sub_recipe_id && (
                      <span className="text-muted-foreground text-xs">
                        {' '}
                        · recipe, {grams(item.weight_g, 0)}
                      </span>
                    )}
                  </span>
                  <span className="text-muted-foreground tabular shrink-0 text-xs">
                    {item.label ? 'not counted' : kcal(item.nutrients.calories_kcal)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>

      {steps.length > 0 && (
        <Card className="print:break-inside-avoid">
          <CardHeader>
            <CardTitle>Method</CardTitle>
          </CardHeader>
          <CardContent>
            {steps.length === 1 ? (
              // One step is a paragraph, not a list with a lone "1." on it.
              <p className="max-w-prose leading-relaxed whitespace-pre-wrap">{steps[0]}</p>
            ) : (
              <ol className="max-w-prose list-decimal space-y-3 pl-6 marker:text-muted-foreground marker:tabular-nums">
                {steps.map((step, i) => (
                  <li key={i} className="pl-1 leading-relaxed">
                    {step}
                  </li>
                ))}
              </ol>
            )}
          </CardContent>
        </Card>
      )}

      <NutritionCard
        total={recipe.total}
        perServing={recipe.per_serving}
        servings={recipe.servings}
        weight={recipe.total_weight_g}
        untracked={recipe.untracked_count}
        someNested={recipe.untracked_count > recipe.items.filter((i) => i.label).length}
        live={false}
      />
    </div>
  )
}
