import { Link } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { Globe, LogIn, Printer } from 'lucide-react'

import { api } from '@/api/endpoints'
import { ApiError } from '@/api/client'
import { useAuth } from '@/lib/auth'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { ThemeToggle } from '@/components/ThemeToggle'
import PhotoStrip from '@/components/PhotoStrip'
import RecipeReadout from '@/components/RecipeReadout'
import { CopyLinkButton } from '@/components/CopyLinkButton'
import { ErrorNote, Spinner } from '@/components/shared'

/**
 * A shared recipe, for anyone holding the link.
 *
 * Lives outside the signed-in shell: no navigation, no session, just the
 * recipe with a small header saying whose app this is and where to sign in.
 * The recipe itself is the same `RecipeReadout` the signed-in page renders,
 * fed from the public route, so the two can never disagree. Photos come from
 * the public photo route, which answers only while the recipe is shared —
 * un-sharing takes this page and its photos away together.
 *
 * The id arrives as a prop rather than from `useParams`: the page is rendered
 * from a `useMatch` in `App`, outside any `<Route>`, where the route params
 * are empty.
 */
export default function PublicRecipePage({ id }: { id: string }) {
  const { user } = useAuth()

  const recipe = useQuery({
    queryKey: ['public', 'recipes', id],
    queryFn: () => api.publicRecipe(id),
    enabled: id.length > 0,
  })

  const notShared = recipe.error instanceof ApiError && recipe.error.status === 404
  const data = recipe.data

  return (
    <div className="flex min-h-dvh flex-col">
      <header className="bg-background/85 sticky top-0 z-30 border-b backdrop-blur-sm print:hidden">
        <div className="mx-auto flex h-14 w-full max-w-5xl items-center justify-between gap-3 px-4">
          <Link to="/" className="flex items-center gap-2 font-semibold tracking-tight">
            <span aria-hidden="true">🥗</span>
            <span>nom-inal</span>
          </Link>
          <div className="flex items-center gap-1">
            <ThemeToggle />
            <Button asChild variant="ghost" size="sm">
              <Link to="/">
                <LogIn />
                {user ? 'Open the app' : 'Sign in'}
              </Link>
            </Button>
          </div>
        </div>
      </header>

      <main className="mx-auto w-full max-w-5xl flex-1 px-4 py-6 pb-16">
        {recipe.isLoading && <Spinner />}

        {notShared ? (
          <Alert>
            <Globe />
            <AlertDescription>
              This recipe is not shared. Its author may have made it private again, or the link may
              be wrong.
            </AlertDescription>
          </Alert>
        ) : (
          <ErrorNote error={recipe.error} />
        )}

        {data && (
          <RecipeReadout
            recipe={data}
            actions={
              <>
                <CopyLinkButton url={api.publicRecipeUrl(data.id)} />
                <Button variant="ghost" size="sm" onClick={() => window.print()}>
                  <Printer /> Print
                </Button>
              </>
            }
            note={
              <p className="text-muted-foreground flex items-center gap-1.5 text-sm">
                <Globe className="size-4" />
                Shared by {data.author ?? 'its author'} on nom-inal.
              </p>
            }
            photos={
              data.photos.length > 0 && (
                <PhotoStrip
                  queryKey={['public', 'photos', id]}
                  // Already in hand: the public recipe carries its photo list,
                  // so there is no second request to make.
                  list={() => Promise.resolve(data.photos)}
                  upload={() => Promise.reject(new Error('Sign in to add a photo.'))}
                  canEdit={false}
                  size="lg"
                  label="Recipe photo"
                />
              )
            }
          />
        )}
      </main>
    </div>
  )
}
