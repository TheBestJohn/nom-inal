import { Fragment, useEffect, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  ArrowLeft,
  ExternalLink,
  FileJson,
  FileText,
  Globe,
  Link2,
  MoreHorizontal,
  Pencil,
  Plus,
  Printer,
  X,
} from 'lucide-react'

import { api } from '@/api/endpoints'
import type { RecipeInput } from '@/api/endpoints'
import type { Food, Nutrients, Recipe, RecipeDraft, RecipeSummary } from '@/api/types'
import { withNetCarbs } from '@/lib/nutrients'
import { grams, kcal } from '@/lib/format'
import { cn } from '@/lib/utils'
import { ZERO, foodItem, recipeItem, textItem, type DraftItem } from '@/lib/recipeDraft'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardAction, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import FoodPicker from '@/components/FoodPicker'
import RecipePicker from '@/components/RecipePicker'
import PhotoStrip from '@/components/PhotoStrip'
import RecipeReadout, { NutritionCard } from '@/components/RecipeReadout'
import RecipeImportReview from '@/components/RecipeImportReview'
import { CopyLinkButton } from '@/components/CopyLinkButton'
import { Empty, ErrorNote, Spinner } from '@/components/shared'

/**
 * Add an ingredient that is only words.
 *
 * Its own component so the input has its own state: typing here must not
 * re-render the ingredient list on every keystroke, and a component declared
 * inside the page body would be a new type on every render and lose focus.
 */
function TextIngredientForm({ onAdd }: { onAdd: (label: string) => void }) {
  const [text, setText] = useState('')
  return (
    <form
      className="space-y-3"
      onSubmit={(e) => {
        e.preventDefault()
        if (text.trim()) onAdd(text.trim())
        setText('')
      }}
    >
      <div className="space-y-1.5">
        <Label htmlFor="r-freetext">Ingredient</Label>
        <Input
          id="r-freetext"
          autoFocus
          placeholder="e.g. salt and pepper to taste"
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      </div>
      <Button type="submit" disabled={!text.trim()}>
        <Plus /> Add
      </Button>
      <p className="text-muted-foreground text-xs">
        For the things not worth a database entry — a pinch of salt, a squeeze of lemon. It
        contributes nothing to the macros, and the recipe says how many of these it has so the
        totals are never quietly short.
      </p>
    </form>
  )
}

/**
 * Import a recipe from a page.
 *
 * Its own component so the address box has its own state, and so the
 * request lives with the box: the page only hears about it once the server
 * has read the page and handed back a draft.
 */
function ImportFromUrlForm({ onDraft }: { onDraft: (draft: RecipeDraft) => void }) {
  const [url, setUrl] = useState('')
  const fetchDraft = useMutation({
    mutationFn: () => api.importRecipeFromUrl(url.trim()),
    onSuccess: onDraft,
  })
  return (
    <form
      className="space-y-3"
      onSubmit={(e) => {
        e.preventDefault()
        if (url.trim()) fetchDraft.mutate()
      }}
    >
      <div className="space-y-1.5">
        <Label htmlFor="r-import-url">Address of the recipe</Label>
        <Input
          id="r-import-url"
          type="url"
          autoFocus
          inputMode="url"
          placeholder="https://…"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
        />
      </div>
      <ErrorNote error={fetchDraft.error} />
      <Button type="submit" disabled={!url.trim() || fetchDraft.isPending}>
        {fetchDraft.isPending ? 'Reading the page…' : 'Read the page'}
      </Button>
      <p className="text-muted-foreground text-xs">
        Most recipe sites describe the recipe in a form search engines read, and that is what is
        used here: the name, the servings, the ingredient lines and the method. Each ingredient line
        is then looked up in the food database for you to confirm. Nothing is saved until you create
        the recipe.
      </p>
    </form>
  )
}

/**
 * A recipe to read, not a form with its inputs switched off.
 *
 * The recipe itself is `RecipeReadout`, which the public page renders too;
 * what this adds is everything a signed-in reader can do with it — edit it
 * if it is theirs, copy its link if it is shared, save it as a file, print
 * it — and the photo strip through the authenticated route, with uploads
 * for the author.
 */
function RecipeView({ recipe, onEdit }: { recipe: Recipe; onEdit: () => void }) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const download = useMutation({
    mutationFn: (format: 'json' | 'markdown') =>
      api.downloadRecipeExport(recipe.id, recipe.name, format),
  })

  // Six controls is a comfortable row on a laptop and three stacked lines on
  // a phone, which pushed the recipe itself below the fold. Editing and
  // copying the link stay in the open because they are what people came for;
  // the rest are named once, here, and rendered twice — as buttons where
  // there is room and as a menu where there is not — so the two can never
  // come to disagree about what the page can do.
  const secondary = [
    {
      key: 'markdown',
      short: 'Markdown',
      label: 'Save as a Markdown recipe card',
      icon: FileText,
      variant: 'outline' as const,
      run: () => download.mutate('markdown'),
    },
    {
      key: 'json',
      short: 'JSON',
      label: 'Save as JSON, with foods named rather than numbered',
      icon: FileJson,
      variant: 'outline' as const,
      run: () => download.mutate('json'),
    },
    {
      key: 'print',
      short: 'Print',
      label: 'Print this recipe',
      icon: Printer,
      variant: 'ghost' as const,
      run: () => window.print(),
    },
    {
      key: 'back',
      short: 'Back',
      label: 'Back to recipes',
      icon: ArrowLeft,
      variant: 'ghost' as const,
      run: () => navigate('/recipes'),
    },
  ]

  return (
    <RecipeReadout
      recipe={recipe}
      actions={
        <>
          {recipe.is_owner && (
            <Button variant="outline" size="sm" onClick={onEdit}>
              <Pencil /> Edit
            </Button>
          )}
          {recipe.is_public && <CopyLinkButton url={api.publicRecipeUrl(recipe.id)} />}

          <div className="hidden items-center gap-2 sm:flex">
            {secondary.map(({ key, short, label, icon: Icon, variant, run }) => (
              <Button
                key={key}
                variant={variant}
                size="sm"
                disabled={download.isPending}
                onClick={run}
                title={label}
              >
                <Icon /> {short}
              </Button>
            ))}
          </div>

          <div className="sm:hidden">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon" aria-label="More recipe actions">
                  <MoreHorizontal />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                {secondary.map(({ key, short, icon: Icon, run }) => (
                  <Fragment key={key}>
                    {key === 'back' && <DropdownMenuSeparator />}
                    <DropdownMenuItem onSelect={run} disabled={download.isPending}>
                      <Icon /> {key === 'back' ? 'Back to recipes' : short}
                    </DropdownMenuItem>
                  </Fragment>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </>
      }
      note={
        <>
          {!recipe.is_owner && (
            <Alert className="print:hidden">
              <Globe />
              <AlertDescription>
                Shared by {recipe.author}. You can view it and log it, but only its author can
                change it.
              </AlertDescription>
            </Alert>
          )}
          {recipe.is_owner && recipe.is_public && (
            <p className="text-muted-foreground flex items-center gap-1.5 text-sm print:hidden">
              <Link2 className="size-4" />
              Public: anyone holding the link can read this recipe, signed in or not.
            </p>
          )}
          <ErrorNote error={download.error} />
        </>
      }
      photos={
        // Photos live on the read view, not in the form: an upload is saved
        // the moment it lands, so there is nothing for a Save button to do
        // with it, and the author wants to add one to a recipe they are
        // looking at, not one they are editing. Everyone else just sees them.
        <PhotoStrip
          queryKey={['photos', 'recipe', recipe.id]}
          list={() => api.listRecipePhotos(recipe.id)}
          upload={(file) => api.uploadRecipePhoto(recipe.id, file)}
          canEdit={recipe.is_owner}
          size="lg"
          label="Recipe photo"
          // The list cards carry a cover photo taken from the same set.
          onChange={() => queryClient.invalidateQueries({ queryKey: ['recipes'] })}
        />
      }
      subRecipeHref={(subId) => `/recipes/${subId}`}
    />
  )
}

export default function RecipeEditorPage() {
  const { id } = useParams()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const isNew = !id

  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [instructions, setInstructions] = useState('')
  const [servings, setServings] = useState('1')
  const [isPublic, setIsPublic] = useState(false)
  const [items, setItems] = useState<DraftItem[]>([])
  const [picking, setPicking] = useState(false)
  // Import from a URL: the address dialog, then the draft under review. The
  // review replaces the form until its lines are chosen or it is abandoned.
  const [importing, setImporting] = useState(false)
  const [draft, setDraft] = useState<RecipeDraft | null>(null)
  // An existing recipe opens as a page to read; the form is a step away.
  const [editing, setEditing] = useState(isNew)

  const existing = useQuery({
    queryKey: ['recipes', id],
    queryFn: () => api.getRecipe(id!),
    enabled: !isNew,
  })

  // Hydrate once the recipe arrives, and again on Cancel, which is the same
  // thing: throw the draft away and start from what is saved. Per-unit
  // figures are recovered from the stored per-item totals, so the live
  // arithmetic below stays accurate.
  const hydrate = (recipe: Recipe) => {
    setName(recipe.name)
    setDescription(recipe.description ?? '')
    setInstructions(recipe.instructions ?? '')
    setServings(String(recipe.servings))
    setIsPublic(recipe.is_public)
    setItems(
      recipe.items.map((item) => {
        // The server sends each item's contribution already scaled, so dividing
        // by the amount recovers the per-unit figure whichever kind it is.
        const amount = item.quantity_g ?? item.servings ?? 1
        return {
          key: item.id,
          kind: item.label
            ? ('text' as const)
            : item.sub_recipe_id
              ? ('recipe' as const)
              : ('food' as const),
          refId: item.sub_recipe_id ?? item.food_id ?? '',
          name: item.name,
          brand: item.brand,
          amount,
          perUnit: {
            calories_kcal: item.nutrients.calories_kcal / amount,
            protein_g: item.nutrients.protein_g / amount,
            carbs_g: item.nutrients.carbs_g / amount,
            fat_g: item.nutrients.fat_g / amount,
          },
          gramsPerUnit: item.weight_g / amount,
        }
      }),
    )
  }

  useEffect(() => {
    if (existing.data) hydrate(existing.data)
  }, [existing.data])

  const save = useMutation({
    mutationFn: () => {
      const payload: RecipeInput = {
        name,
        description: description || null,
        instructions: instructions || null,
        servings: Number(servings),
        is_public: isPublic,
        items: items.map((i) => {
          if (i.kind === 'food') return { food_id: i.refId, quantity_g: i.amount }
          if (i.kind === 'recipe') return { sub_recipe_id: i.refId, servings: i.amount }
          return { label: i.name }
        }),
      }
      return isNew ? api.createRecipe(payload) : api.updateRecipe(id!, payload)
    },
    onSuccess: (recipe) => {
      queryClient.invalidateQueries({ queryKey: ['recipes'] })
      // Back to reading it. The route change alone is not enough: the new and
      // existing routes share this component, so state can survive it.
      setEditing(false)
      navigate(`/recipes/${recipe.id}`, { replace: true })
    },
  })

  const servingCount = Number(servings) > 0 ? Number(servings) : 1

  // Totals recompute as you type, using the same grams/100 scaling the server
  // applies on save — so what is shown is what gets stored.
  // The live total follows the four macros only; the server is the reference
  // once saved. Net carbs is still derived from it rather than left at zero,
  // so the row never shows carbs beside a net-carbs figure that ignores them.
  const total: Nutrients = withNetCarbs(
    items.reduce<Nutrients>(
      (acc, item) => ({
        ...acc,
        calories_kcal: acc.calories_kcal + item.perUnit.calories_kcal * item.amount,
        protein_g: acc.protein_g + item.perUnit.protein_g * item.amount,
        carbs_g: acc.carbs_g + item.perUnit.carbs_g * item.amount,
        fat_g: acc.fat_g + item.perUnit.fat_g * item.amount,
      }),
      ZERO,
    ),
  )

  const perServing: Nutrients = withNetCarbs({
    ...total,
    calories_kcal: total.calories_kcal / servingCount,
    protein_g: total.protein_g / servingCount,
    carbs_g: total.carbs_g / servingCount,
    fat_g: total.fat_g / servingCount,
  })

  const totalWeight = items.reduce((sum, i) => sum + i.gramsPerUnit * i.amount, 0)

  // What this page can see for itself, and what the server counted through any
  // nesting. The saved figure is the honest one; the local count is what keeps
  // the warning truthful while you are still editing.
  const untrackedHere = items.filter((i) => i.kind === 'text').length
  const untracked = Math.max(untrackedHere, existing.data?.untracked_count ?? 0)

  const addFood = (food: Food, suggested?: number) => {
    setItems((prev) => [...prev, foodItem(food, suggested ?? food.serving_size_g)])
    setPicking(false)
  }

  const addText = (label: string) => {
    setItems((prev) => [...prev, textItem(label)])
    setPicking(false)
  }

  const addRecipe = (recipe: RecipeSummary) => {
    setItems((prev) => [...prev, recipeItem(recipe)])
    setPicking(false)
  }

  // The reviewed import lands in the form as if it had been typed: the
  // header fields filled from the page, the chosen lines appended to the
  // ingredients, and the page's address kept in the description so the
  // recipe says where it came from. Still nothing saved.
  const useDraft = (chosen: DraftItem[]) => {
    if (!draft) return
    setName(draft.name)
    setDescription(
      [
        draft.description,
        draft.author ? `By ${draft.author}, from ${draft.source_url}` : `From ${draft.source_url}`,
      ]
        .filter(Boolean)
        .join('\n\n'),
    )
    setInstructions(draft.instructions ?? '')
    if (draft.servings) setServings(String(draft.servings))
    setItems((prev) => [...prev, ...chosen])
    setDraft(null)
  }

  if (!isNew && existing.isLoading) return <Spinner />
  if (existing.error) return <ErrorNote error={existing.error} />

  // Anyone who cannot edit gets the page to read, and so does the owner until
  // they ask for the form.
  if (existing.data && (!existing.data.is_owner || !editing)) {
    return <RecipeView recipe={existing.data} onEdit={() => setEditing(true)} />
  }

  const cancel = () => {
    if (isNew) {
      navigate('/recipes')
      return
    }
    if (existing.data) hydrate(existing.data)
    setEditing(false)
  }

  if (draft) {
    return <RecipeImportReview draft={draft} onCancel={() => setDraft(null)} onUse={useDraft} />
  }

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold tracking-tight">
          {isNew ? 'New recipe' : 'Edit recipe'}
        </h1>
        <div className="flex flex-wrap items-center gap-2">
          {isNew && (
            <Button variant="outline" size="sm" onClick={() => setImporting(true)}>
              <Globe /> Import from a URL
            </Button>
          )}
          <Button variant="ghost" size="sm" onClick={cancel}>
            <ArrowLeft /> {isNew ? 'Back' : 'Cancel'}
          </Button>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Details</CardTitle>
        </CardHeader>
        {/* Three columns at every width, not from `sm` up: the name and the
            servings are a pair, and stacking them on a phone spent a whole
            line on a box that holds the digit 3. */}
        <CardContent className="grid grid-cols-3 gap-3">
          <div className="col-span-2 space-y-1.5">
            <Label htmlFor="r-name">Name</Label>
            <Input
              id="r-name"
              required
              autoFocus={isNew}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="r-servings">Servings</Label>
            <Input
              id="r-servings"
              type="number"
              min={0.1}
              step="any"
              value={servings}
              onChange={(e) => setServings(e.target.value)}
            />
          </div>
          <div className="col-span-3 space-y-1.5">
            <Label htmlFor="r-desc">Description</Label>
            {/* A textarea, not an input: a description is allowed to be two
                lines, and a single-line box silently ate the second one. */}
            <Textarea
              id="r-desc"
              rows={2}
              placeholder="A line or two about it"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>
          <div className="col-span-3 space-y-1.5">
            <Label htmlFor="r-inst">Method</Label>
            <Textarea
              id="r-inst"
              rows={8}
              placeholder={
                'Preheat the oven to 200°C.\nToss the vegetables in oil and salt.\nRoast for 25 minutes.'
              }
              value={instructions}
              onChange={(e) => setInstructions(e.target.value)}
            />
            <p className="text-muted-foreground text-xs">
              One step per line. They are numbered on the recipe page, so there is no need to number
              them here.
            </p>
          </div>

          <div className="col-span-3 flex items-start gap-3 rounded-md border p-3">
            <Switch
              id="r-public"
              checked={isPublic}
              onCheckedChange={setIsPublic}
              className="mt-0.5"
            />
            <div className="min-w-0 flex-1 space-y-2">
              <Label htmlFor="r-public">Share this recipe</Label>
              <p className="text-muted-foreground text-xs">
                Recipes are private by default. Sharing makes this one public: every account can
                read and log it, and so can anyone holding its link, signed in or not. Only you can
                edit it, and turning this off takes the link away again.
              </p>
              {isPublic &&
                (isNew ? (
                  <p className="text-muted-foreground text-xs">
                    The link appears once the recipe is created.
                  </p>
                ) : (
                  <CopyLinkButton url={api.publicRecipeUrl(id!)} />
                ))}
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Ingredients</CardTitle>
          <CardAction>
            <Button variant="outline" size="sm" onClick={() => setPicking(true)}>
              <Plus /> Add ingredient
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          {items.length === 0 ? (
            <Empty>No ingredients yet.</Empty>
          ) : (
            <ul className="divide-y">
              {items.map((item, index) => (
                // An input, a unit, a figure and a remove button beside a name
                // is five things competing for 280 pixels, and the name lost:
                // on a phone the list showed four amount boxes and no
                // ingredients. So the name takes the first line with the
                // button that removes it, and the amount sits under it where
                // it has room to be typed into.
                <li
                  key={item.key}
                  className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-2 py-2.5 sm:grid-cols-[minmax(0,1fr)_auto_auto]"
                >
                  <div className="col-start-1 row-start-1 min-w-0">
                    {item.kind === 'text' ? (
                      // Free text stays editable in place: there is no record
                      // behind it to open, and re-picking it to fix a typo
                      // would be silly.
                      <Input
                        aria-label={`Ingredient ${index + 1}`}
                        value={item.name}
                        onChange={(e) =>
                          setItems((prev) =>
                            prev.map((it, i) =>
                              i === index ? { ...it, name: e.target.value } : it,
                            ),
                          )
                        }
                      />
                    ) : item.kind === 'recipe' ? (
                      // A sub-recipe is a real thing elsewhere in the app, so
                      // its name goes where its name belongs: to it. Same tab,
                      // like the Back button — unsaved edits are lost either
                      // way, and a link that opens somewhere unexpected is
                      // worse than one that behaves like every other link.
                      <Link
                        to={`/recipes/${item.refId}`}
                        className="hover:text-primary flex items-center gap-1.5 truncate font-medium underline-offset-4 hover:underline"
                      >
                        {item.name}
                        <ExternalLink className="size-3.5 shrink-0" />
                      </Link>
                    ) : (
                      <p className="truncate font-medium">{item.name}</p>
                    )}
                    {item.kind === 'recipe' ? (
                      <p className="text-muted-foreground truncate text-xs">
                        recipe · {grams(item.gramsPerUnit * item.amount, 0)}
                      </p>
                    ) : item.kind === 'text' ? (
                      <p className="text-muted-foreground truncate text-xs">
                        no nutrition information
                      </p>
                    ) : (
                      item.brand && (
                        <p className="text-muted-foreground truncate text-xs">{item.brand}</p>
                      )
                    )}
                  </div>
                  <div
                    className={cn(
                      'col-span-2 col-start-1 row-start-2 flex items-center gap-2 sm:col-span-1 sm:col-start-2 sm:row-start-1',
                      // A free-text line has nothing to put on the second
                      // row, and the sub-line under its name already says it
                      // is not counted. Wide enough for a column, the words
                      // keep the amounts of the rows above them lined up.
                      item.kind === 'text' && 'max-sm:hidden',
                    )}
                  >
                    {item.kind === 'text' ? (
                      // No amount and no calories: both would be numbers the
                      // totals deliberately ignore.
                      <span className="text-muted-foreground w-[10.5rem] text-right text-xs">
                        not counted
                      </span>
                    ) : (
                      <>
                        <Input
                          type="number"
                          min={item.kind === 'recipe' ? 0.01 : 0.1}
                          step="any"
                          inputMode="decimal"
                          className="tabular w-24 text-right"
                          aria-label={`${item.name} ${item.kind === 'recipe' ? 'servings' : 'grams'}`}
                          value={item.amount}
                          onChange={(e) =>
                            setItems((prev) =>
                              prev.map((it, i) =>
                                i === index ? { ...it, amount: Number(e.target.value) } : it,
                              ),
                            )
                          }
                        />
                        <span className="text-muted-foreground w-12 text-xs">
                          {item.kind === 'recipe' ? 'servings' : 'g'}
                        </span>
                        <span className="text-muted-foreground tabular ml-auto w-20 text-right text-xs sm:ml-0">
                          {kcal(item.perUnit.calories_kcal * item.amount)}
                        </span>
                      </>
                    )}
                  </div>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    className="col-start-2 row-start-1 sm:col-start-3"
                    aria-label={`Remove ${item.name}`}
                    onClick={() => setItems((prev) => prev.filter((_, i) => i !== index))}
                  >
                    <X />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>

      <NutritionCard
        total={total}
        perServing={perServing}
        servings={servingCount}
        weight={totalWeight}
        untracked={untracked}
        someNested={!!existing.data && existing.data.untracked_count > untrackedHere}
        live
      />

      <ErrorNote error={save.error} />
      {/* On a phone, saving stays within reach of a thumb: a recipe form is
          several screens tall by the time it has a method and six
          ingredients, and a Save button at the bottom of it meant scrolling
          back past everything you had just typed to commit it. It clears the
          home indicator on the way. Above `sm` the form fits a screen and the
          button stays where it was, at the end of the page. */}
      <div className="max-sm:bg-background/90 flex flex-wrap items-center gap-3 max-sm:sticky max-sm:bottom-0 max-sm:z-20 max-sm:-mx-4 max-sm:border-t max-sm:px-4 max-sm:py-3 max-sm:pb-[max(0.75rem,env(safe-area-inset-bottom))] max-sm:backdrop-blur-sm">
        <Button
          disabled={
            save.isPending ||
            !name ||
            items.length === 0 ||
            items.some((i) => i.kind === 'text' && !i.name.trim())
          }
          onClick={() => save.mutate()}
        >
          {save.isPending ? 'Saving…' : isNew ? 'Create recipe' : 'Save changes'}
        </Button>
        {items.length === 0 && (
          <span className="text-muted-foreground text-xs">Add at least one ingredient.</span>
        )}
      </div>

      <Dialog open={importing} onOpenChange={setImporting}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>Import from a URL</DialogTitle>
          </DialogHeader>
          <ImportFromUrlForm
            onDraft={(d) => {
              setImporting(false)
              setDraft(d)
            }}
          />
        </DialogContent>
      </Dialog>

      <Dialog open={picking} onOpenChange={setPicking}>
        <DialogContent className="sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>Add ingredient</DialogTitle>
          </DialogHeader>
          <Tabs defaultValue="food">
            <TabsList className="grid w-full grid-cols-3">
              <TabsTrigger value="food">Food</TabsTrigger>
              <TabsTrigger value="recipe">Recipe</TabsTrigger>
              <TabsTrigger value="text">Just text</TabsTrigger>
            </TabsList>
            <TabsContent value="food" className="pt-3">
              <FoodPicker onPick={addFood} />
            </TabsContent>
            <TabsContent value="recipe" className="pt-3">
              <RecipePicker excludeId={id} onPick={addRecipe} />
            </TabsContent>
            <TabsContent value="text" className="pt-3">
              <TextIngredientForm onAdd={addText} />
            </TabsContent>
          </Tabs>
        </DialogContent>
      </Dialog>
    </div>
  )
}
