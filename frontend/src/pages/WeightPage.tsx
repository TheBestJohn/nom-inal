import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { X } from 'lucide-react'
import {
  CartesianGrid,
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'

import { api } from '@/api/endpoints'
import { useAuth } from '@/lib/auth'
import { addDays, kg, kgToLb, lbToKg, prettyDate, round, shortDate, signed, today } from '@/lib/format'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Card, CardAction, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { Empty, ErrorNote, Spinner } from '@/components/shared'
import PhotoStrip from '@/components/PhotoStrip'
import ReminderBanner from '@/components/ReminderBanner'

const RANGES = [
  { label: '30d', days: 30 },
  { label: '90d', days: 90 },
  { label: '1y', days: 365 },
  { label: 'All', days: 3650 },
]

const TOOLTIP_STYLE = {
  background: 'var(--popover)',
  border: '1px solid var(--border)',
  borderRadius: 'var(--radius)',
  color: 'var(--popover-foreground)',
  fontSize: 12,
}

export default function WeightPage() {
  const { user } = useAuth()
  const queryClient = useQueryClient()
  const [rangeDays, setRangeDays] = useState(90)
  const [unit, setUnit] = useState<'kg' | 'lb'>('kg')

  const [date, setDate] = useState(today())
  const [value, setValue] = useState('')
  const [bodyFat, setBodyFat] = useState('')
  const [note, setNote] = useState('')

  const to = today()
  const from = addDays(to, -rangeDays)

  const entries = useQuery({
    queryKey: ['weights', from, to],
    queryFn: () => api.listWeights({ from, to, limit: 2000 }),
  })
  const stats = useQuery({
    queryKey: ['weights', 'stats', from, to],
    queryFn: () => api.weightStats({ from, to }),
  })

  const log = useMutation({
    mutationFn: () =>
      api.logWeight({
        recorded_on: date,
        // The API is metric; converting at this one edge keeps lb out of
        // everything else.
        weight_kg: unit === 'kg' ? Number(value) : lbToKg(Number(value)),
        body_fat_pct: bodyFat ? Number(bodyFat) : null,
        note: note || null,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['weights'] })
      setValue('')
      setBodyFat('')
      setNote('')
    },
  })

  const remove = useMutation({
    mutationFn: (id: string) => api.deleteWeight(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['weights'] }),
  })

  const display = (v: number | null | undefined) =>
    v === null || v === undefined ? '—' : unit === 'kg' ? kg(v) : `${round(kgToLb(v))} lb`

  const series = (entries.data ?? [])
    .slice()
    .reverse()
    .map((w) => ({
      date: shortDate(w.recorded_on),
      value: round(unit === 'kg' ? w.weight_kg : kgToLb(w.weight_kg), 2),
    }))

  const targetLine =
    user?.target_weight_kg != null
      ? round(unit === 'kg' ? user.target_weight_kg : kgToLb(user.target_weight_kg), 2)
      : null

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold tracking-tight">Weight</h1>
        <ToggleGroup
          type="single"
          size="sm"
          value={unit}
          onValueChange={(v) => v && setUnit(v as 'kg' | 'lb')}
        >
          <ToggleGroupItem value="kg">kg</ToggleGroupItem>
          <ToggleGroupItem value="lb">lb</ToggleGroupItem>
        </ToggleGroup>
      </div>

      <ReminderBanner />

      <Card>
        <CardHeader>
          <CardTitle>Log a weigh-in</CardTitle>
        </CardHeader>
        <CardContent>
          <form
            className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4"
            onSubmit={(e) => {
              e.preventDefault()
              log.mutate()
            }}
          >
            <Field label="Date" htmlFor="w-date">
              <Input id="w-date" type="date" value={date} max={today()} onChange={(e) => setDate(e.target.value)} />
            </Field>
            <Field label={`Weight (${unit})`} htmlFor="w-value">
              <Input
                id="w-value"
                type="number"
                step="any"
                min={1}
                required
                value={value}
                onChange={(e) => setValue(e.target.value)}
                placeholder={unit === 'kg' ? '82.5' : '182'}
              />
            </Field>
            <Field label="Body fat % (optional)" htmlFor="w-bf">
              <Input
                id="w-bf"
                type="number"
                step="any"
                min={0}
                max={100}
                value={bodyFat}
                onChange={(e) => setBodyFat(e.target.value)}
              />
            </Field>
            <Field label="Note (optional)" htmlFor="w-note">
              <Input id="w-note" value={note} maxLength={500} onChange={(e) => setNote(e.target.value)} />
            </Field>

            <div className="flex flex-wrap items-center gap-3 sm:col-span-2 lg:col-span-4">
              <Button type="submit" disabled={log.isPending || !value}>
                {log.isPending ? 'Saving…' : 'Save'}
              </Button>
              <span className="text-muted-foreground text-xs">
                One entry per day — saving again replaces it.
              </span>
            </div>
          </form>
          <ErrorNote error={log.error} />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Trend</CardTitle>
          <CardAction>
            <ToggleGroup
              type="single"
              size="sm"
              value={String(rangeDays)}
              onValueChange={(v) => v && setRangeDays(Number(v))}
            >
              {RANGES.map((r) => (
                <ToggleGroupItem key={r.label} value={String(r.days)}>
                  {r.label}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </CardAction>
        </CardHeader>
        <CardContent className="space-y-4">
          {entries.isLoading && <Spinner />}
          <ErrorNote error={entries.error} />
          {series.length === 0 ? (
            <Empty>No entries in this range.</Empty>
          ) : (
            <>
              <ResponsiveContainer width="100%" height={260}>
                <LineChart data={series} margin={{ top: 8, right: 8, bottom: 0, left: -12 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" />
                  <XAxis dataKey="date" stroke="var(--muted-foreground)" fontSize={12} tickMargin={8} />
                  <YAxis
                    stroke="var(--muted-foreground)"
                    fontSize={12}
                    width={56}
                    domain={['dataMin - 1', 'dataMax + 1']}
                  />
                  <Tooltip contentStyle={TOOLTIP_STYLE} />
                  {targetLine != null && (
                    <ReferenceLine
                      y={targetLine}
                      stroke="var(--chart-3)"
                      strokeDasharray="4 4"
                      label={{
                        value: 'Target',
                        fill: 'var(--muted-foreground)',
                        fontSize: 11,
                        position: 'right',
                      }}
                    />
                  )}
                  <Line
                    type="monotone"
                    dataKey="value"
                    name={unit}
                    stroke="var(--chart-2)"
                    strokeWidth={2}
                    dot={{ r: 2 }}
                  />
                </LineChart>
              </ResponsiveContainer>

              {stats.data && (
                <dl className="grid grid-cols-2 gap-3 text-sm sm:grid-cols-4">
                  <Stat label="Latest" value={display(stats.data.latest_kg)} />
                  <Stat
                    label="Change"
                    value={
                      stats.data.change_kg == null
                        ? '—'
                        : `${signed(unit === 'kg' ? stats.data.change_kg : kgToLb(stats.data.change_kg))} ${unit}`
                    }
                    tone={(stats.data.change_kg ?? 0) <= 0 ? 'good' : 'bad'}
                  />
                  <Stat label="7-entry avg" value={display(stats.data.moving_average_7_kg)} />
                  <Stat label="Entries" value={String(stats.data.count)} />
                </dl>
              )}
            </>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>History</CardTitle>
        </CardHeader>
        <CardContent>
          {entries.data?.length === 0 && <Empty>Nothing yet.</Empty>}
          <ul className="divide-y">
            {entries.data?.map((entry) => (
              <li key={entry.id} className="space-y-2 py-3">
                <div className="flex items-center gap-3">
                  <div className="min-w-0 flex-1">
                    <p className="tabular font-medium">{display(entry.weight_kg)}</p>
                    <p className="text-muted-foreground truncate text-xs">
                      {prettyDate(entry.recorded_on)}
                      {entry.body_fat_pct != null ? ` · ${round(entry.body_fat_pct)}% body fat` : ''}
                      {entry.note ? ` · ${entry.note}` : ''}
                    </p>
                  </div>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={`Delete entry for ${entry.recorded_on}`}
                    onClick={() => remove.mutate(entry.id)}
                  >
                    <X />
                  </Button>
                </div>
                <PhotoStrip
                  queryKey={['photos', 'weight', entry.id]}
                  list={() => api.listPhotos(entry.id)}
                  upload={(file) => api.uploadPhoto(entry.id, file)}
                  canEdit
                  label="Progress photo"
                  // A new photo can clear the progress-photo reminder.
                  onChange={() => queryClient.invalidateQueries({ queryKey: ['reminders'] })}
                />
              </li>
            ))}
          </ul>
        </CardContent>
      </Card>
    </div>
  )
}

function Field({
  label,
  htmlFor,
  children,
}: {
  label: string
  htmlFor: string
  children: React.ReactNode
}) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
    </div>
  )
}

function Stat({ label, value, tone }: { label: string; value: string; tone?: 'good' | 'bad' }) {
  return (
    <div>
      <dt className="text-muted-foreground text-xs">{label}</dt>
      <dd
        className={cn(
          'tabular font-semibold',
          tone === 'good' && 'text-success',
          tone === 'bad' && 'text-destructive',
        )}
      >
        {value}
      </dd>
    </div>
  )
}
