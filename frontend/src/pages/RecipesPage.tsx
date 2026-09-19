import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Globe, Lock, Plus, X } from 'lucide-react'

import { api } from '@/api/endpoints'
import { grams, kcal } from '@/lib/format'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
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
              <CardTitle className="flex items-center gap-2">
                <Link to={`/recipes/${recipe.id}`} className="hover:underline">
                  {recipe.name}
                </Link>
                {recipe.is_public && (
                  <Badge variant="success" className="gap-1">
                    <Globe className="size-3" /> Shared
                  </Badge>
                )}
              </CardTitle>
              <CardDescription>
                {recipe.author ? `by ${recipe.author} · ` : ''}
                {recipe.item_count} ingredient{recipe.item_count === 1 ? '' : 's'} ·{' '}
                {grams(recipe.total_weight_g, 0)} · {recipe.servings} serving
                {recipe.servings === 1 ? '' : 's'}
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
            <CardContent className="flex gap-4">
              {recipe.cover_photo_url && <Cover url={recipe.cover_photo_url} name={recipe.name} />}
              <div className="min-w-0 flex-1 space-y-1">
                {recipe.description && (
                  <p className="text-muted-foreground line-clamp-2 text-sm">{recipe.description}</p>
                )}
                <p className="text-muted-foreground text-xs">
                  Per serving · {kcal(recipe.per_serving.calories_kcal)}
                </p>
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
    <div className="bg-muted size-20 shrink-0 overflow-hidden rounded-md border">
      {objectUrl && <img src={objectUrl} alt={`Photo of ${name}`} className="size-full object-cover" />}
    </div>
  )
}
