import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { X } from 'lucide-react'

import { api } from '@/api/endpoints'
import { NUTRIENTS } from '@/lib/nutrients'
import type { TargetInput } from '@/api/endpoints'
import type { Nutrient, TargetKind } from '@/api/types'
import { cn } from '@/lib/utils'
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
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { ErrorNote, Spinner } from '@/components/shared'

/**
 * The nutrient vocabulary, mirroring the server's, with the direction each
 * defaults to. Protein and fibre are things you try to reach; the rest are
 * things you try to stay under. Every one can be flipped — carbs are a budget
 * when cutting and a goal when bulking.
 */

/** A row in the editor. A blank amount means "no target for this nutrient". */
interface Row {
  amount: string
  kind: TargetKind
}

type Rows = Record<Nutrient, Row>

const blankRows = (): Rows =>
  Object.fromEntries(NUTRIENTS.map((n) => [n.key, { amount: '', kind: n.defaultKind }])) as Rows

export default function TargetsEditor() {
  const queryClient = useQueryClient()
  const [rows, setRows] = useState<Rows>(blankRows)
  const [saved, setSaved] = useState(false)

  const targets = useQuery({ queryKey: ['targets'], queryFn: () => api.listTargets() })
  // The estimate is made on the server now, from the same inputs and the same
  // arithmetic the focus presets use, so what is suggested here is exactly
  // what the general preset would write.
  const suggestion = useQuery({
    queryKey: ['targets', 'suggestion'],
    queryFn: () => api.targetSuggestion(),
  })

  useEffect(() => {
    if (!targets.data) return
    const next = blankRows()
    for (const t of targets.data) next[t.nutrient] = { amount: String(t.amount), kind: t.kind }
    setRows(next)
  }, [targets.data])

  const save = useMutation({
    mutationFn: () => {
      // PUT replaces the whole set, so a blank row is a cleared target.
      const payload: TargetInput[] = NUTRIENTS.flatMap((n) => {
        const row = rows[n.key]
        const amount = Number(row.amount)
        if (!row.amount.trim() || !Number.isFinite(amount) || amount <= 0) return []
        return [{ nutrient: n.key, amount, kind: row.kind }]
      })
      return api.replaceTargets(payload)
    },
    onSuccess: () => {
      // The diary and dashboard read targets through their day query, so both
      // need refreshing, not just this list.
      queryClient.invalidateQueries({ queryKey: ['targets'] })
      queryClient.invalidateQueries({ queryKey: ['diary'] })
      setSaved(true)
      setTimeout(() => setSaved(false), 2500)
    },
  })

  const set = (key: Nutrient, patch: Partial<Row>) =>
    setRows((r) => ({ ...r, [key]: { ...r[key], ...patch } }))

  const priced = (suggestion.data?.targets ?? []).filter((t) => t.amount !== null)

  const applySuggestion = () => {
    setRows((r) => {
      const next = { ...r }
      for (const t of priced) next[t.nutrient] = { amount: String(t.amount), kind: t.kind }
      return next
    })
  }

  const activeCount = NUTRIENTS.filter((n) => Number(rows[n.key].amount) > 0).length

  if (targets.isLoading) return <Spinner />

  return (
    <Card>
      <CardHeader>
        <CardTitle>Daily goals &amp; budgets</CardTitle>
        <CardDescription>
          A <strong>budget</strong> is a ceiling — stay under it. A <strong>goal</strong> is a floor
          — hit at least that much. Leave a number blank to not track it.
        </CardDescription>
        {saved && (
          <CardAction>
            <Badge variant="success">Saved</Badge>
          </CardAction>
        )}
      </CardHeader>

      <CardContent className="space-y-4">
        {suggestion.data && priced.length > 0 && (
          <Alert variant="success">
            <AlertDescription className="w-full">
              <div className="flex w-full flex-wrap items-center justify-between gap-3">
                <div>
                  <p>
                    <strong>Suggested:</strong>{' '}
                    {priced
                      .map((t) => `${t.amount} ${t.unit} ${t.label.toLowerCase()} ${t.kind}`)
                      .join(' · ')}
                  </p>
                  <p className="text-muted-foreground text-xs">
                    Mifflin-St Jeor resting rate × activity, adjusted for your goal. An estimate —
                    adjust to what the scale actually does.
                  </p>
                </div>
                <Button variant="outline" size="sm" onClick={applySuggestion}>
                  Use these
                </Button>
              </div>
            </AlertDescription>
          </Alert>
        )}
        {suggestion.data && suggestion.data.missing.length > 0 && (
          <p className="text-muted-foreground text-xs">
            No suggestion yet: the estimate needs{' '}
            {suggestion.data.missing
              .map((f) =>
                f === 'weight'
                  ? 'a weigh-in'
                  : f === 'height_cm'
                    ? 'your height'
                    : 'your date of birth',
              )
              .join(', ')}
            .{' '}
            <Link to="/settings/body" className="text-primary underline underline-offset-4">
              Add them under Body
            </Link>
            .
          </p>
        )}

        <div className="divide-y">
          {NUTRIENTS.map((n) => {
            const row = rows[n.key]
            const active = Number(row.amount) > 0
            return (
              <div
                key={n.key}
                className="grid grid-cols-[1fr_auto] items-center gap-x-3 gap-y-2 py-3 sm:grid-cols-[1fr_7rem_auto_auto]"
              >
                <Label
                  htmlFor={`target-${n.key}`}
                  className={cn(
                    'col-span-2 font-medium sm:col-span-1',
                    !active && 'text-muted-foreground',
                  )}
                >
                  {n.label}
                  {n.hint && <span className="text-muted-foreground text-xs">{n.hint}</span>}
                </Label>

                <div className="flex items-center gap-1.5">
                  <Input
                    id={`target-${n.key}`}
                    type="number"
                    min={0}
                    step="any"
                    inputMode="decimal"
                    placeholder="—"
                    className="tabular text-right"
                    value={row.amount}
                    onChange={(e) => set(n.key, { amount: e.target.value })}
                  />
                  <span className="text-muted-foreground w-8 text-xs">{n.unit}</span>
                </div>

                <ToggleGroup
                  type="single"
                  size="sm"
                  value={row.kind}
                  onValueChange={(v) => v && set(n.key, { kind: v as TargetKind })}
                  aria-label={`${n.label} direction`}
                  disabled={!active}
                  className={cn(!active && 'opacity-50')}
                >
                  <ToggleGroupItem value="goal">Goal</ToggleGroupItem>
                  <ToggleGroupItem value="budget">Budget</ToggleGroupItem>
                </ToggleGroup>

                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={`Clear ${n.label} target`}
                  disabled={!active}
                  onClick={() => set(n.key, { amount: '' })}
                >
                  <X />
                </Button>
              </div>
            )
          })}
        </div>

        <ErrorNote error={targets.error} />
        <ErrorNote error={save.error} />

        <div className="flex flex-wrap items-center gap-3">
          <Button disabled={save.isPending} onClick={() => save.mutate()}>
            {save.isPending ? 'Saving…' : 'Save targets'}
          </Button>
          <span className="text-muted-foreground text-xs">
            {activeCount} of {NUTRIENTS.length} tracked
          </span>
        </div>
      </CardContent>
    </Card>
  )
}
