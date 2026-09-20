import { Link } from 'react-router-dom'
import { useMutation, useQuery } from '@tanstack/react-query'
import {
  CartesianGrid,
  Legend,
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
import { addDays, kcal, shortDate, today, weight, weightChange, weightValue } from '@/lib/format'
import { useUnits } from '@/lib/useUnits'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Separator } from '@/components/ui/separator'
import {
  Empty,
  EnergyShareRow,
  ErrorNote,
  MacroRow,
  Spinner,
  TargetList,
} from '@/components/shared'
import { dashFor, nutrientValue, orderNutrients } from '@/lib/nutrients'
import type { ChartMode, DiarySummary, Nutrient, NutritionTarget } from '@/api/types'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import ReminderBanner from '@/components/ReminderBanner'
import ExpenditureLine from '@/components/ExpenditureLine'

/** Recharts takes colours as values, not classes, so they come from the theme
 *  variables the rest of the UI uses rather than being hard-coded here. */
const TOOLTIP_STYLE = {
  background: 'var(--popover)',
  border: '1px solid var(--border)',
  borderRadius: 'var(--radius)',
  color: 'var(--popover-foreground)',
  fontSize: 12,
}

/**
 * One nutrient over the window: a single series, its own axis, its own unit.
 *
 * Declared here rather than inside the page because a component defined in a
 * render body is a new type on every render, which would remount the chart --
 * and recharts animates from scratch each time it mounts.
 */
function NutrientChart({
  nutrient,
  label,
  unit,
  color,
  summary,
  sole,
}: {
  nutrient: Nutrient
  label: string
  unit: string
  color: string
  summary?: DiarySummary
  sole: boolean
}) {
  const series = (summary?.days ?? []).map((d) => ({
    date: shortDate(d.date),
    value: Math.round(nutrientValue(d.total, nutrient)),
  }))

  return (
    <figure className="space-y-1">
      <figcaption className="text-muted-foreground text-xs">
        {label} <span className="opacity-70">({unit})</span>
      </figcaption>
      <ResponsiveContainer width="100%" height={sole ? 220 : 160}>
        <LineChart data={series} margin={{ top: 8, right: 8, bottom: 0, left: -12 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" />
          <XAxis dataKey="date" stroke="var(--muted-foreground)" fontSize={12} tickMargin={8} />
          <YAxis stroke="var(--muted-foreground)" fontSize={12} width={52} />
          <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v) => [`${v} ${unit}`, label]} />
          {/* No dot on every point: thirty of them is noise, and the hover
              layer is what answers "what was Tuesday". */}
          <Line
            type="monotone"
            dataKey="value"
            name={label}
            stroke={color}
            strokeWidth={2}
            dot={false}
            activeDot={{ r: 4 }}
          />
        </LineChart>
      </ResponsiveContainer>
    </figure>
  )
}

/**
 * Every charted nutrient as a share of its own goal or budget, on one axis.
 *
 * This is the one honest way to put calories and fat on the same chart: they
 * share no scale, but "how far through today's number am I" is the same
 * question for both, so 100% means the same thing on every line.
 *
 * Direction still differs and the chart cannot flatten that — 120% of a protein
 * goal is a good day and 120% of a calorie budget is not — so each series says
 * which it is, and the line at 100% is labelled rather than left to be guessed.
 */
function PercentChart({
  series,
  summary,
}: {
  series: { nutrient: Nutrient; label: string; color: string; target: NutritionTarget }[]
  summary?: DiarySummary
}) {
  const data = (summary?.days ?? []).map((d) => {
    const row: Record<string, string | number> = { date: shortDate(d.date) }
    for (const s of series) {
      row[s.nutrient] = Math.round((nutrientValue(d.total, s.nutrient) / s.target.amount) * 100)
    }
    return row
  })

  return (
    <ResponsiveContainer width="100%" height={260}>
      {/* The right margin is for the target label, which sits outside the plot:
          inside it, it lands on whichever series happens to be near 100%. */}
      <LineChart data={data} margin={{ top: 8, right: 40, bottom: 0, left: -12 }}>
        <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" />
        <XAxis dataKey="date" stroke="var(--muted-foreground)" fontSize={12} tickMargin={8} />
        <YAxis
          stroke="var(--muted-foreground)"
          fontSize={12}
          width={52}
          // 100% is the reference the whole chart is about, so it is always in
          // frame: let recharts fit the data and a light day would put the
          // target line off the top, which is exactly the fact being hidden.
          domain={[0, (max: number) => Math.max(110, Math.ceil(max / 10) * 10)]}
          tickFormatter={(v) => `${v}%`}
        />
        <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v, name) => [`${v}%`, name]} />
        {/* The whole point of the axis: one line everything is measured against. */}
        <ReferenceLine
          y={100}
          stroke="var(--muted-foreground)"
          strokeDasharray="4 4"
          label={{
            value: 'target',
            position: 'right',
            fill: 'var(--muted-foreground)',
            fontSize: 11,
          }}
        />
        <Legend
          verticalAlign="bottom"
          height={36}
          formatter={(value) => <span className="text-muted-foreground text-xs">{value}</span>}
        />
        {series.map((s, i) => (
          <Line
            key={s.nutrient}
            type="monotone"
            dataKey={s.nutrient}
            // The legend entry is the identity; the colour is an accent and the
            // dash pattern is what still separates the lines in greyscale or
            // for a reader who cannot tell these hues apart.
            name={`${s.label} (${s.target.kind})`}
            stroke={s.color}
            strokeDasharray={dashFor(i)}
            strokeWidth={2}
            dot={false}
            activeDot={{ r: 4 }}
          />
        ))}
      </LineChart>
    </ResponsiveContainer>
  )
}

export default function DashboardPage() {
  const { user, setUser } = useAuth()
  const { units } = useUnits()
  const to = today()
  const from = addDays(to, -29)

  const day = useQuery({ queryKey: ['diary', 'day', to], queryFn: () => api.diaryDay(to) })
  const summary = useQuery({
    queryKey: ['diary', 'summary', from, to],
    queryFn: () => api.diarySummary(from, to),
  })
  const weights = useQuery({
    queryKey: ['weights', from, to],
    queryFn: () => api.listWeights({ from, to }),
  })
  const stats = useQuery({
    queryKey: ['weights', 'stats', from, to],
    queryFn: () => api.weightStats({ from, to }),
  })

  const calorieStatus = day.data?.targets.find((t) => t.nutrient === 'calories_kcal')

  // recharts wants oldest-first; list endpoints return newest-first.
  const weightSeries = (weights.data ?? [])
    .slice()
    .reverse()
    .map((w) => ({ date: shortDate(w.recorded_on), value: weightValue(w.weight_kg, units) }))

  const charted = orderNutrients(user?.chart_nutrients ?? (['calories_kcal'] as Nutrient[]))
  const mode: ChartMode = user?.chart_mode ?? 'percent'

  // Percent mode needs the numbers being measured against, and they are the
  // same for every day in the window -- targets are a standing setting, not a
  // per-day record -- so one fetch covers the whole chart.
  const targets = useQuery({
    queryKey: ['targets'],
    queryFn: () => api.listTargets(),
    enabled: mode === 'percent',
  })

  const setMode = useMutation({
    mutationFn: (chart_mode: ChartMode) => api.updateProfile({ chart_mode }),
    onSuccess: (profile) => setUser(profile),
  })

  // A nutrient with no goal or budget has nothing to be a percentage of. It is
  // dropped rather than drawn at zero, and named below rather than silently
  // missing -- a chart quietly short of a line you asked for is worse than one
  // that says why.
  const withTargets = charted.flatMap((meta) => {
    const target = targets.data?.find((t) => t.nutrient === meta.key)
    return target ? [{ nutrient: meta.key, label: meta.label, color: meta.color, target }] : []
  })
  const untargeted = charted.filter((meta) => !withTargets.some((s) => s.nutrient === meta.key))

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold tracking-tight">Today</h1>
        <span className="text-muted-foreground text-sm">
          Hi {user?.display_name.split(' ')[0]} 👋
        </span>
      </div>

      <ReminderBanner />

      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Today&rsquo;s intake</CardTitle>
            <CardAction>
              <Button asChild variant="outline" size="sm">
                <Link to="/diary">Open diary</Link>
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent className="space-y-4">
            {day.isLoading && <Spinner />}
            <ErrorNote error={day.error} />
            {day.data && (
              <>
                <div className="flex flex-wrap items-baseline gap-3">
                  <strong className="tabular text-3xl font-bold tracking-tight">
                    {kcal(day.data.total.calories_kcal)}
                  </strong>
                  {calorieStatus && (
                    <span
                      className={cn(
                        'text-sm',
                        calorieStatus.status === 'over'
                          ? 'text-destructive'
                          : 'text-muted-foreground',
                      )}
                    >
                      {calorieStatus.status === 'over'
                        ? `${kcal(Math.abs(calorieStatus.remaining))} over`
                        : `${kcal(calorieStatus.remaining)} left`}
                    </span>
                  )}
                </div>
                <EnergyShareRow share={day.data.energy_share} />
                <TargetList targets={day.data.targets} />
                {/* What your own complete days say you burn, or what is
                    still needed before that can be said. Never hidden. */}
                <ExpenditureLine />
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Weight</CardTitle>
            <CardAction>
              <Button asChild variant="outline" size="sm">
                <Link to="/weight">Log weight</Link>
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent className="space-y-4">
            {stats.isLoading && <Spinner />}
            <ErrorNote error={stats.error} />
            {stats.data && stats.data.count > 0 ? (
              <>
                <div className="flex flex-wrap items-baseline gap-3">
                  <strong className="tabular text-3xl font-bold tracking-tight">
                    {weight(stats.data.latest_kg, units)}
                  </strong>
                  <span
                    className={cn(
                      'text-sm',
                      (stats.data.change_kg ?? 0) <= 0 ? 'text-success' : 'text-destructive',
                    )}
                  >
                    {weightChange(stats.data.change_kg, units)} in 30 days
                  </span>
                </div>
                <dl className="grid grid-cols-3 gap-3 text-sm">
                  <Stat label="7-entry avg" value={weight(stats.data.moving_average_7_kg, units)} />
                  <Stat
                    label="Range"
                    value={`${weight(stats.data.min_kg, units)} – ${weight(stats.data.max_kg, units)}`}
                  />
                  <Stat label="Target" value={weight(user?.target_weight_kg, units)} />
                </dl>
              </>
            ) : (
              <Empty>
                No weigh-ins in the last 30 days.{' '}
                <Link to="/weight" className="text-primary underline underline-offset-4">
                  Log one
                </Link>
                .
              </Empty>
            )}
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Last 30 days</CardTitle>
          <CardDescription>
            {mode === 'percent'
              ? 'Each nutrient as a share of its own goal or budget, so they share one axis.'
              : 'Real figures, one chart each — they have no shared scale.'}{' '}
            Change which nutrients in{' '}
            <Link to="/settings/display" className="text-primary underline underline-offset-4">
              Settings
            </Link>
            .
          </CardDescription>
          <CardAction>
            <ToggleGroup
              type="single"
              size="sm"
              value={mode}
              onValueChange={(v) => v && setMode.mutate(v as ChartMode)}
              aria-label="Chart style"
            >
              <ToggleGroupItem value="percent">% of target</ToggleGroupItem>
              <ToggleGroupItem value="actual">Actual</ToggleGroupItem>
            </ToggleGroup>
          </CardAction>
        </CardHeader>
        <CardContent className="space-y-3">
          {(summary.isLoading || (mode === 'percent' && targets.isLoading)) && <Spinner />}
          <ErrorNote error={summary.error} />
          <ErrorNote error={targets.error} />
          <ErrorNote error={setMode.error} />

          {charted.length === 0 ? (
            <Empty>No nutrients selected to chart.</Empty>
          ) : (summary.data?.days.length ?? 0) === 0 ? (
            <Empty>Nothing logged in this window yet.</Empty>
          ) : mode === 'percent' && withTargets.length === 0 ? (
            <Empty>
              Nothing here has a goal or budget yet —{' '}
              <Link to="/settings/targets" className="text-primary underline underline-offset-4">
                set one
              </Link>{' '}
              to see progress against it, or switch to actual values.
            </Empty>
          ) : (
            <>
              {mode === 'percent' ? (
                <PercentChart series={withTargets} summary={summary.data} />
              ) : (
                /* Small multiples rather than several lines on one pair of
                   axes. Calories run to a couple of thousand and fat to about
                   seventy, so a shared y-axis would flatten every macro onto
                   the floor. Each nutrient gets its own axis and its own unit;
                   the heading carries the identity, so no legend is needed and
                   colour is never doing the work alone. */
                <div className={cn('grid gap-4', charted.length > 1 && 'sm:grid-cols-2')}>
                  {charted.map((meta) => (
                    <NutrientChart
                      key={meta.key}
                      nutrient={meta.key}
                      label={meta.label}
                      unit={meta.unit}
                      color={meta.color}
                      summary={summary.data}
                      sole={charted.length === 1}
                    />
                  ))}
                </div>
              )}

              {mode === 'percent' && untargeted.length > 0 && (
                <p className="text-muted-foreground text-xs">
                  Not shown: {untargeted.map((m) => m.label).join(', ')} — no goal or budget set.
                </p>
              )}

              <Separator />
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-muted-foreground text-xs">
                  Average over {summary.data?.logged_day_count} logged day
                  {summary.data?.logged_day_count === 1 ? '' : 's'}
                </span>
                {summary.data && (
                  <div className="flex flex-col items-end gap-1">
                    <MacroRow n={summary.data.average} compact />
                    <EnergyShareRow share={summary.data.energy_share} />
                  </div>
                )}
              </div>
            </>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Weight trend, last 30 days</CardTitle>
        </CardHeader>
        <CardContent>
          {weights.isLoading && <Spinner />}
          <ErrorNote error={weights.error} />
          {weightSeries.length === 0 ? (
            <Empty>No weigh-ins yet.</Empty>
          ) : (
            <ResponsiveContainer width="100%" height={220}>
              <LineChart data={weightSeries} margin={{ top: 8, right: 8, bottom: 0, left: -12 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" />
                <XAxis
                  dataKey="date"
                  stroke="var(--muted-foreground)"
                  fontSize={12}
                  tickMargin={8}
                />
                <YAxis
                  stroke="var(--muted-foreground)"
                  fontSize={12}
                  width={52}
                  domain={['dataMin - 1', 'dataMax + 1']}
                />
                <Tooltip contentStyle={TOOLTIP_STYLE} />
                <Line
                  type="monotone"
                  dataKey="value"
                  name={units === 'imperial' ? 'lb' : 'kg'}
                  stroke="var(--chart-2)"
                  strokeWidth={2}
                  dot={{ r: 2 }}
                />
              </LineChart>
            </ResponsiveContainer>
          )}
        </CardContent>
      </Card>
    </div>
  )
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-muted-foreground text-xs">{label}</dt>
      <dd className="tabular font-semibold">{value}</dd>
    </div>
  )
}
