import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Globe, Lock, Plus, X } from 'lucide-react'

import { api } from '@/api/endpoints'
import { grams } from '@/lib/format'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Empty, ErrorNote, MacroRow, Spinner } from '@/components/shared'
import { useAuthedImage } from '@/components/PhotoStrip'

type Scope = 'mine' | 'public'

export default function RecipesPage() {
  const [scope, setScope] = useState<Scope>('mine')
  const queryClient = useQueryClient()

  const recipes = useQuery({
    queryKey: ['recipes', scope],
    queryFn: () => api.listRecipes({ scope }),
  })

  const remove = useMutation({
    mutationFn: (id: string) => api.deleteRecipe(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['recipes'] }),
  })

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold tracking-tight">Recipes</h1>
        <Button asChild size="sm">
          <Link to="/recipes/new">
            <Plus /> New recipe
          </Link>
        </Button>
      </div>

      <Tabs value={scope} onValueChange={(v) => setScope(v as Scope)}>
        <TabsList>
          <TabsTrigger value="mine">
            <Lock /> Mine
          </TabsTrigger>
          <TabsTrigger value="public">
            <Globe /> Shared
          </TabsTrigger>
        </TabsList>
      </Tabs>

      {recipes.isLoading && <Spinner />}
      <ErrorNote error={recipes.error} />
      <ErrorNote error={remove.error} />

      {recipes.data?.length === 0 && (
        <Card>
          <CardContent>
            <Empty>
              {scope === 'mine'
                ? 'No recipes yet. Build one from any food and its macros are computed for you.'
                : 'Nobody has shared a recipe yet. Mark one of yours public to share it.'}
            </Empty>
          </CardContent>
        </Card>
      )}

      <div className="grid gap-4 md:grid-cols-2">
        {recipes.data?.map((recipe) => (
          <Card key={recipe.id}>
            <CardHeader>
              {/* The badge used to sit beside the name and take a bite out of
                  it; a long name then wrapped to four lines against a
                  40-pixel column. The name gets the width, and "Shared" goes
                  with the rest of what the recipe is. */}
              <CardTitle>
                <Link to={`/recipes/${recipe.id}`} className="hover:underline">
                  {recipe.name}
                </Link>
              </CardTitle>
              <CardDescription className="flex flex-wrap items-center gap-x-1.5 gap-y-1">
                {recipe.is_public && (
                  <Badge variant="success" className="gap-1">
                    <Globe className="size-3" /> Shared
                  </Badge>
                )}
                <span>
                  {recipe.author ? `by ${recipe.author} · ` : ''}
                  {recipe.item_count} ingredient{recipe.item_count === 1 ? '' : 's'} ·{' '}
                  {grams(recipe.total_weight_g, 0)} · {recipe.servings} serving
                  {recipe.servings === 1 ? '' : 's'}
                </span>
              </CardDescription>
              {recipe.is_owner && (
                <CardAction>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={`Delete ${recipe.name}`}
                    onClick={() => remove.mutate(recipe.id)}
                  >
                    <X />
                  </Button>
                </CardAction>
              )}
            </CardHeader>
            {/* The macro row sits under the photo rather than beside it.
                With every nutrient switched on it is nine figures, and in the
                column left over by an 80-pixel thumbnail that was five lines
                of two words. The kcal figure is not repeated above it
                either — the row already opens with it. */}
            <CardContent className="space-y-2">
              <div className="flex gap-4">
                {recipe.cover_photo_url && (
                  <Cover url={recipe.cover_photo_url} name={recipe.name} />
                )}
                <div className="min-w-0 flex-1">
                  {recipe.description && (
                    <p className="text-muted-foreground line-clamp-3 text-sm">
                      {recipe.description}
                    </p>
                  )}
                </div>
              </div>
              <div className="space-y-1">
                <p className="text-muted-foreground text-xs">Per serving</p>
                <MacroRow n={recipe.per_serving} />
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  )
}

/**
 * The card's cover: the recipe's first photo. Fetched with the token like
 * every photo, since `<img src>` alone cannot pass one.
 */
function Cover({ url, name }: { url: string; name: string }) {
  const { objectUrl } = useAuthedImage(url)
  return (
    <div className="bg-muted size-24 shrink-0 overflow-hidden rounded-md border sm:size-20">
      {objectUrl && (
        <img src={objectUrl} alt={`Photo of ${name}`} className="size-full object-cover" />
      )}
    </div>
  )
}
