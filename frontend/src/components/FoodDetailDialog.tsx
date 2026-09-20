import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, History, Pencil, Plus, ThumbsDown, Undo2, X } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { Food, FoodRevision, Nutrients, Verdict } from '@/api/types'
import { fieldLabel, grams, kcal, relativeTime, round } from '@/lib/format'
import { cn } from '@/lib/utils'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Separator } from '@/components/ui/separator'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import {
  Empty,
  ErrorNote,
  MacroRow,
  SourceBadge,
  Spinner,
  VerificationBadge,
  foodStatus,
} from '@/components/shared'
import FoodForm from '@/components/FoodForm'

/**
 * A food's per-100g figures as a `Nutrients`.
 *
 * `Food` leaves the optional nutrients nullable — "nobody recorded the fiber"
 * is not the same claim as "there is no fiber" — but a readout has to print
 * something, and the server flattens the same way when it does the arithmetic.
 */
const per100g = (food: Food): Nutrients => ({
  calories_kcal: food.calories_kcal,
  protein_g: food.protein_g,
  carbs_g: food.carbs_g,
  fat_g: food.fat_g,
  fiber_g: food.fiber_g ?? 0,
  sugar_g: food.sugar_g ?? 0,
  saturated_fat_g: food.saturated_fat_g ?? 0,
  sodium_mg: food.sodium_mg ?? 0,
})

/** How a revision came about, in words rather than a database enum. */
const CHANGE_KIND: Record<string, string> = {
  create: 'created',
  edit: 'edited',
  import: 'imported',
  revert: 'restored an earlier version',
  seed: 'seeded',
}

/**
 * Everything about one food that is not a number: who wrote it, who agrees,
 * what it used to say, and how to disagree.
 *
 * This is the screen that makes open editing tenable. Anyone can change any
 * food, which is only reasonable if changing one is visible, attributable and
 * undoable — so the history, the vote and the revert all live together rather
 * than being tucked away behind an admin tool.
 */
export default function FoodDetailDialog({
  foodId,
  open,
  onOpenChange,
}: {
  foodId: string | null
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const queryClient = useQueryClient()
  const [mode, setMode] = useState<'view' | 'edit' | 'variant'>('view')
  const [disputeNote, setDisputeNote] = useState('')

  const detail = useQuery({
    queryKey: ['foods', foodId],
    queryFn: () => api.getFood(foodId!),
    enabled: open && !!foodId,
  })

  const revisions = useQuery({
    queryKey: ['foods', foodId, 'revisions'],
    queryFn: () => api.foodRevisions(foodId!),
    enabled: open && !!foodId,
  })

  const verifications = useQuery({
    queryKey: ['foods', foodId, 'verifications'],
    queryFn: () => api.foodVerifications(foodId!),
    enabled: open && !!foodId,
  })

  // Every mutation here changes the same three things, so they share one
  // invalidation rather than each guessing which queries it affected.
  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ['foods'] })
  }

  const vote = useMutation({
    mutationFn: ({ verdict, note }: { verdict: Verdict; note?: string }) =>
      api.verifyFood(foodId!, verdict, note),
    onSuccess: () => {
      setDisputeNote('')
      refresh()
    },
  })

  const withdraw = useMutation({
    mutationFn: () => api.withdrawVerification(foodId!),
    onSuccess: refresh,
  })

  const revert = useMutation({
    mutationFn: (revision: number) => api.revertFood(foodId!, revision),
    onSuccess: refresh,
  })

  const food = detail.data
  const provenance = food?.provenance

  const close = () => {
    setMode('view')
    onOpenChange(false)
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) close()
      }}
    >
      <DialogContent className="sm:max-w-2xl">
        {detail.isLoading && <Spinner />}
        <ErrorNote error={detail.error} />

        {food && provenance && (
          <>
            <DialogHeader>
              <DialogTitle className="flex flex-wrap items-center gap-2">
                <span>{food.name}</span>
                {food.variant_label && <Badge variant="secondary">{food.variant_label}</Badge>}
                <SourceBadge source={food.source} />
                <VerificationBadge
                  status={provenance.status}
                  confirmations={provenance.confirmations}
                  quorum={provenance.quorum}
                />
              </DialogTitle>
              <DialogDescription>
                {food.brand ? `${food.brand} · ` : ''}
                Revision {provenance.revision} · {provenance.last_edited_by_name ?? 'someone'}{' '}
                {CHANGE_KIND[provenance.last_change_kind] ?? provenance.last_change_kind}{' '}
                {relativeTime(provenance.last_edited_at)}
                {provenance.contributors > 1 ? ` · ${provenance.contributors} contributors` : ''}
              </DialogDescription>
            </DialogHeader>

            {mode === 'edit' && (
              <FoodForm
                food={food}
                askForSummary
                onSaved={() => {
                  setMode('view')
                  refresh()
                }}
                onCancel={() => setMode('view')}
              />
            )}

            {mode === 'variant' && (
              <FoodForm
                variantOf={food}
                onSaved={() => {
                  setMode('view')
                  refresh()
                }}
                onCancel={() => setMode('view')}
              />
            )}

            {mode === 'view' && (
              <Tabs defaultValue="facts">
                <TabsList>
                  <TabsTrigger value="facts">Facts</TabsTrigger>
                  <TabsTrigger value="history">
                    <History /> History
                  </TabsTrigger>
                  <TabsTrigger value="trust">Verification</TabsTrigger>
                </TabsList>

                <TabsContent value="facts" className="space-y-4 pt-3">
                  {/* Both bases, but the food's own first and larger. For a
                      packaged product that means the panel reads like the label
                      it was copied from, which is how you check it against the
                      box in your hand. */}
                  {(food.nutrient_basis === 'per_serving'
                    ? (['serving', 'hundred'] as const)
                    : (['hundred', 'serving'] as const)
                  ).map((which, index) => (
                    <div key={which} className="space-y-1">
                      <p className="text-muted-foreground text-xs">
                        {which === 'hundred'
                          ? 'Per 100 g'
                          : `Per serving · ${grams(food.serving_size_g, 0)}${
                              food.serving_label ? ` · ${food.serving_label}` : ''
                            }`}
                        {index === 0 && food.source === 'custom' && ' · as entered'}
                      </p>
                      <MacroRow
                        n={which === 'hundred' ? per100g(food) : food.per_serving}
                        compact={index === 1}
                      />
                    </div>
                  ))}

                  <Separator />

                  {/* Variants are the reason a single row is not enough: raw
                      and cooked are the same ingredient and different numbers,
                      and both have to be loggable. */}
                  {food.parent ? (
                    <p className="text-sm">
                      A <strong>{food.variant_label}</strong> variant of{' '}
                      <strong>{food.parent.name}</strong> ({kcal(food.parent.calories_kcal)} / 100
                      g).
                    </p>
                  ) : (
                    <div className="space-y-2">
                      <div className="flex items-center justify-between gap-2">
                        <p className="text-sm font-medium">Variants</p>
                        <Button size="sm" variant="outline" onClick={() => setMode('variant')}>
                          <Plus /> Add variant
                        </Button>
                      </div>
                      {food.variants.length === 0 ? (
                        <Empty>
                          None yet. Add one for a different preparation — cooked, drained, dried —
                          rather than editing these numbers.
                        </Empty>
                      ) : (
                        <ul className="space-y-1 text-sm">
                          {food.variants.map((v: Food) => (
                            <li
                              key={v.id}
                              className="flex items-center justify-between gap-2 rounded-md border px-3 py-1.5"
                            >
                              <span className="flex items-center gap-2">
                                <Badge variant="secondary">{v.variant_label}</Badge>
                                <span className="text-muted-foreground tabular text-xs">
                                  {kcal(v.calories_kcal)} / 100 g
                                </span>
                              </span>
                              <VerificationBadge status={foodStatus(v)} />
                            </li>
                          ))}
                        </ul>
                      )}
                    </div>
                  )}

                  <div className="flex justify-end gap-2">
                    <Button variant="outline" size="sm" onClick={() => setMode('edit')}>
                      <Pencil /> Correct this
                    </Button>
                  </div>
                  <p className="text-muted-foreground text-xs">
                    Anyone can correct a food. Every change is signed and reversible, and a change
                    puts the entry back to unverified until other people agree with it.
                  </p>
                </TabsContent>

                <TabsContent value="history" className="space-y-3 pt-3">
                  {revisions.isLoading && <Spinner />}
                  <ErrorNote error={revisions.error} />
                  <ErrorNote error={revert.error} />
                  <ol className="space-y-2">
                    {revisions.data?.map((rev: FoodRevision) => (
                      <li
                        key={rev.id}
                        className={cn(
                          'rounded-md border px-3 py-2 text-sm',
                          rev.revision === provenance.revision && 'border-primary/60 bg-muted/40',
                        )}
                      >
                        <div className="flex flex-wrap items-baseline justify-between gap-2">
                          <span>
                            <strong>{rev.edited_by_name ?? 'Someone'}</strong>{' '}
                            {CHANGE_KIND[rev.change_kind] ?? rev.change_kind}
                            <span className="text-muted-foreground">
                              {' '}
                              · {relativeTime(rev.created_at)}
                            </span>
                          </span>
                          <span className="flex items-center gap-2">
                            <span className="text-muted-foreground text-xs">r{rev.revision}</span>
                            {rev.revision !== provenance.revision && (
                              <Button
                                variant="ghost"
                                size="sm"
                                disabled={revert.isPending}
                                onClick={() => revert.mutate(rev.revision)}
                              >
                                <Undo2 /> Restore
                              </Button>
                            )}
                          </span>
                        </div>
                        {rev.summary && (
                          <p className="text-muted-foreground mt-0.5 text-xs italic">
                            “{rev.summary}”
                          </p>
                        )}
                        {rev.changed_fields.length > 0 && (
                          <p className="mt-1 flex flex-wrap gap-1">
                            {rev.changed_fields.map((f) => (
                              <Badge key={f} variant="outline" className="text-[10px]">
                                {fieldLabel(f)}
                                {typeof rev.snapshot[f] === 'number'
                                  ? ` → ${round(rev.snapshot[f] as number, 2)}`
                                  : ''}
                              </Badge>
                            ))}
                          </p>
                        )}
                      </li>
                    ))}
                  </ol>
                  <p className="text-muted-foreground text-xs">
                    Restoring appends a new revision rather than deleting one, so the change you are
                    undoing stays on the record.
                  </p>
                </TabsContent>

                <TabsContent value="trust" className="space-y-3 pt-3">
                  <p className="text-sm">
                    {provenance.confirmations} confirmed, {provenance.disputes} disputed ·{' '}
                    {provenance.quorum} net confirmation{provenance.quorum === 1 ? '' : 's'} needed.
                  </p>

                  <ErrorNote error={vote.error} />
                  <ErrorNote error={withdraw.error} />

                  {!provenance.can_verify ? (
                    <p className="text-muted-foreground text-sm">
                      You wrote the current revision, so someone else has to vouch for it.
                    </p>
                  ) : provenance.your_verdict ? (
                    <div className="flex items-center gap-2 text-sm">
                      <span>
                        You {provenance.your_verdict === 'confirm' ? 'confirmed' : 'disputed'} this
                        revision.
                      </span>
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={withdraw.isPending}
                        onClick={() => withdraw.mutate()}
                      >
                        <X /> Withdraw
                      </Button>
                    </div>
                  ) : (
                    <div className="space-y-2">
                      <Input
                        placeholder="Optional note — what did you check against?"
                        value={disputeNote}
                        onChange={(e) => setDisputeNote(e.target.value)}
                      />
                      <div className="flex gap-2">
                        <Button
                          size="sm"
                          disabled={vote.isPending}
                          onClick={() =>
                            vote.mutate({ verdict: 'confirm', note: disputeNote || undefined })
                          }
                        >
                          <Check /> These are right
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={vote.isPending}
                          onClick={() =>
                            vote.mutate({ verdict: 'dispute', note: disputeNote || undefined })
                          }
                        >
                          <ThumbsDown /> Something is wrong
                        </Button>
                      </div>
                    </div>
                  )}

                  <Separator />

                  {verifications.isLoading && <Spinner />}
                  {verifications.data?.length === 0 && <Empty>Nobody has weighed in yet.</Empty>}
                  <ul className="space-y-1 text-sm">
                    {verifications.data?.map((v) => (
                      <li
                        key={`${v.user_id}-${v.revision}`}
                        className={cn(
                          'flex flex-wrap items-baseline gap-2 rounded-md border px-3 py-1.5',
                          // A vote on an older revision is kept for the record
                          // but no longer counts, so it is shown dimmed rather
                          // than hidden -- disappearing votes look like a bug.
                          !v.current && 'opacity-55',
                        )}
                      >
                        <strong>{v.display_name}</strong>
                        <Badge
                          variant={v.verdict === 'confirm' ? 'success' : 'destructive'}
                          className="text-[10px]"
                        >
                          {v.verdict === 'confirm' ? 'Confirmed' : 'Disputed'}
                        </Badge>
                        <span className="text-muted-foreground text-xs">
                          r{v.revision}
                          {v.current ? '' : ' · superseded'}
                        </span>
                        {v.note && (
                          <span className="text-muted-foreground w-full text-xs italic">
                            “{v.note}”
                          </span>
                        )}
                      </li>
                    ))}
                  </ul>
                </TabsContent>
              </Tabs>
            )}
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}
