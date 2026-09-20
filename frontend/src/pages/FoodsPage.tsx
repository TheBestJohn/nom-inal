import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, Plus, X } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { ExternalFood, Food } from '@/api/types'
import { useAuth } from '@/lib/auth'
import { grams, kcal, round, sourceLabel } from '@/lib/format'
import { Alert, AlertDescription } from '@/components/ui/alert'
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import {
  Empty,
  ErrorNote,
  SourceBadge,
  Spinner,
  VerificationBadge,
  foodStatus,
} from '@/components/shared'
import FoodForm from '@/components/FoodForm'
import FoodDetailDialog from '@/components/FoodDetailDialog'

export default function FoodsPage() {
  const { user } = useAuth()
  const queryClient = useQueryClient()
  const [term, setTerm] = useState('')
  const [debounced, setDebounced] = useState('')
  const [mineOnly, setMineOnly] = useState(false)
  const [editing, setEditing] = useState<Food | 'new' | null>(null)
  const [openFood, setOpenFood] = useState<string | null>(null)
  // Per 100 g by default, because that is the only basis in which one row can
  // be compared with the next -- a 28 g cracker serving against a 240 g bowl of
  // soup tells you nothing. Flip it to read each row as its own label says.
  const [tableBasis, setTableBasis] = useState<'per_100g' | 'per_serving'>('per_100g')
  const [barcode, setBarcode] = useState('')
  const [submittedBarcode, setSubmittedBarcode] = useState('')
  const [externalTerm, setExternalTerm] = useState('')

  useEffect(() => {
    const id = setTimeout(() => setDebounced(term.trim()), 250)
    return () => clearTimeout(id)
  }, [term])

  const foods = useQuery({
    queryKey: ['foods', debounced, mineOnly],
    queryFn: () => api.listFoods({ q: debounced || undefined, mine: mineOnly, limit: 100 }),
  })

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
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['foods'] }),
  })

  const remove = useMutation({
    mutationFn: (id: string) => api.deleteFood(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['foods'] }),
  })

  // Rows are stored per 100 g, so showing a serving is one multiplication.
  const basisFactor = (food: Food) => (tableBasis === 'per_serving' ? food.serving_size_g / 100 : 1)

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold tracking-tight">Foods</h1>
        <div className="flex gap-2">
          {/* A plain link, not a fetch: the point of the export is to become a
              file you can commit and review, so the browser's own download is
              exactly the right behaviour. */}
          <Button asChild size="sm" variant="outline">
            <a href={api.exportFoodsUrl(false)} download="nom-inal-foods.json">
              <Download /> Export
            </a>
          </Button>
          <Button size="sm" onClick={() => setEditing('new')}>
            <Plus /> Custom food
          </Button>
        </div>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Look up a barcode</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <form
              className="flex gap-2"
              onSubmit={(e) => {
                e.preventDefault()
                setSubmittedBarcode(barcode.replace(/\D/g, ''))
              }}
            >
              <Input
                inputMode="numeric"
                placeholder="UPC / EAN, e.g. 3017624010701"
                value={barcode}
                onChange={(e) => setBarcode(e.target.value)}
              />
              <Button type="submit">Look up</Button>
            </form>
            {lookup.isFetching && <Spinner label="Looking up…" />}
            <ErrorNote error={lookup.error} />
            {lookup.data?.local && (
              <Alert>
                <AlertDescription>
                  Already in the database: <strong>{lookup.data.local.name}</strong>
                </AlertDescription>
              </Alert>
            )}
            {lookup.data?.external && !lookup.data.local && (
              <div className="flex items-center gap-3 rounded-md border px-3 py-2 text-sm">
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-medium">{lookup.data.external.name}</span>
                  <span className="text-muted-foreground block truncate text-xs">
                    {lookup.data.external.brand ? `${lookup.data.external.brand} · ` : ''}
                    {sourceLabel(lookup.data.external.source)} ·{' '}
                    {kcal(lookup.data.external.calories_kcal)} / 100 g
                  </span>
                </span>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={importFood.isPending}
                  onClick={() => importFood.mutate(lookup.data!.external!)}
                >
                  Import
                </Button>
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Search USDA &amp; Open Food Facts</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <Input
              placeholder="e.g. greek yogurt"
              value={externalTerm}
              onChange={(e) => setExternalTerm(e.target.value)}
            />
            {external.isFetching && <Spinner label="Searching…" />}
            <ErrorNote error={external.error} />
            <ErrorNote error={importFood.error} />
            {external.data?.unavailable.map((reason) => (
              <Alert variant="warning" key={reason}>
                <AlertDescription>{reason}</AlertDescription>
              </Alert>
            ))}
            <div className="max-h-64 space-y-1.5 overflow-y-auto">
              {external.data?.results.map((food) => (
                <div
                  key={`${food.source}-${food.source_id}`}
                  className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm"
                >
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">{food.name}</span>
                    <span className="text-muted-foreground block truncate text-xs">
                      {food.brand ? `${food.brand} · ` : ''}
                      {kcal(food.calories_kcal)} / 100 g
                    </span>
                  </span>
                  <SourceBadge source={food.source} />
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={importFood.isPending}
                    onClick={() => importFood.mutate(food)}
                  >
                    Import
                  </Button>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Food database</CardTitle>
          <CardDescription>
            Shared by everyone — a food is a fact about a product, so anyone can correct one. Every
            change is signed, reversible, and unverified until other people agree with it.
          </CardDescription>
          <CardAction className="flex flex-wrap items-center gap-3">
            <ToggleGroup
              type="single"
              size="sm"
              value={tableBasis}
              onValueChange={(v) => v && setTableBasis(v as 'per_100g' | 'per_serving')}
              aria-label="Show figures per"
            >
              <ToggleGroupItem value="per_100g">100 g</ToggleGroupItem>
              <ToggleGroupItem value="per_serving">Serving</ToggleGroupItem>
            </ToggleGroup>
            <Label className="text-muted-foreground text-sm font-normal">
              <Switch checked={mineOnly} onCheckedChange={setMineOnly} />
              Mine only
            </Label>
          </CardAction>
        </CardHeader>
        <CardContent className="space-y-3">
          <Input
            placeholder="Filter foods…"
            value={term}
            onChange={(e) => setTerm(e.target.value)}
          />
          {foods.isLoading && <Spinner />}
          <ErrorNote error={foods.error} />
          <ErrorNote error={remove.error} />
          {foods.data?.length === 0 && <Empty>Nothing matches.</Empty>}

          {foods.data && foods.data.length > 0 && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead className="text-right">kcal</TableHead>
                  <TableHead className="text-right">P</TableHead>
                  <TableHead className="text-right">C</TableHead>
                  <TableHead className="text-right">F</TableHead>
                  <TableHead>Serving</TableHead>
                  <TableHead />
                </TableRow>
              </TableHeader>
              <TableBody>
                {foods.data.map((food) => (
                  <TableRow key={food.id}>
                    <TableCell className="max-w-[16rem] whitespace-normal">
                      <div className="flex flex-wrap items-center gap-2">
                        <button
                          type="button"
                          className="hover:text-primary font-medium underline-offset-4 hover:underline"
                          onClick={() => setOpenFood(food.id)}
                        >
                          {food.name}
                        </button>
                        {food.variant_label && (
                          <Badge variant="secondary" className="text-[10px]">
                            {food.variant_label}
                          </Badge>
                        )}
                        <SourceBadge source={food.source} />
                        <VerificationBadge status={foodStatus(food)} />
                        {food.created_by === user?.id && (
                          <Badge variant="secondary" className="text-[10px]">
                            Yours
                          </Badge>
                        )}
                      </div>
                      {food.brand && (
                        <span className="text-muted-foreground text-xs">{food.brand}</span>
                      )}
                    </TableCell>
                    <TableCell className="tabular text-right">
                      {round(food.calories_kcal * basisFactor(food))}
                    </TableCell>
                    <TableCell className="tabular text-right">
                      {round(food.protein_g * basisFactor(food))}
                    </TableCell>
                    <TableCell className="tabular text-right">
                      {round(food.carbs_g * basisFactor(food))}
                    </TableCell>
                    <TableCell className="tabular text-right">
                      {round(food.fat_g * basisFactor(food))}
                    </TableCell>
                    <TableCell className="text-muted-foreground text-xs">
                      {grams(food.serving_size_g, 0)}
                      {food.serving_label ? ` · ${food.serving_label}` : ''}
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="flex justify-end gap-1">
                        <Button variant="ghost" size="sm" onClick={() => setOpenFood(food.id)}>
                          Details
                        </Button>
                        <Button variant="ghost" size="sm" onClick={() => setEditing(food)}>
                          Edit
                        </Button>
                        {/* Deleting is still narrow: the server refuses once
                            anyone else has edited or vouched for the entry, so
                            the button is only offered to the author. */}
                        {food.created_by === user?.id && food.revision === 1 && (
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            aria-label={`Delete ${food.name}`}
                            onClick={() => remove.mutate(food.id)}
                          >
                            <X />
                          </Button>
                        )}
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
          <p className="text-muted-foreground text-xs">
            {tableBasis === 'per_100g'
              ? 'All figures are per 100 g, so rows can be compared with each other.'
              : "Figures are per each food's own serving, shown in the Serving column."}
          </p>
        </CardContent>
      </Card>

      <Dialog open={editing !== null} onOpenChange={(open) => !open && setEditing(null)}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>{editing === 'new' ? 'New custom food' : 'Correct this food'}</DialogTitle>
            <DialogDescription>
              {editing === 'new'
                ? 'Visible to everyone once saved, and editable by anyone — under your name.'
                : 'Your change is recorded against your name and puts the entry back to unverified.'}
            </DialogDescription>
          </DialogHeader>
          {editing && (
            <FoodForm
              food={editing === 'new' ? null : editing}
              askForSummary={editing !== 'new'}
              onSaved={() => setEditing(null)}
              onCancel={() => setEditing(null)}
            />
          )}
        </DialogContent>
      </Dialog>

      <FoodDetailDialog
        foodId={openFood}
        open={openFood !== null}
        onOpenChange={(open) => !open && setOpenFood(null)}
      />
    </div>
  )
}
