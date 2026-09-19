import { Link } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'

import { api } from '@/api/endpoints'
import { useAuth } from '@/lib/auth'
import { addDays, kcal, kg, shortDate, signed, today } from '@/lib/format'
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
import { Empty, ErrorNote, MacroRow, Spinner, TargetList } from '@/components/shared'
import { nutrientValue, orderNutrients } from '@/lib/nutrients'
import type { DiarySummary, Nutrient } from '@/api/types'
import ReminderBanner from '@/components/ReminderBanner'

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

export default function DashboardPage() {
  const { user } = useAuth()
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
    .map((w) => ({ date: shortDate(w.recorded_on), kg: w.weight_kg }))

  const charted = orderNutrients(user?.chart_nutrients ?? (['calories_kcal'] as Nutrient[]))

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
                <TargetList targets={day.data.targets} />
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
                    {kg(stats.data.latest_kg)}
                  </strong>
                  <span
                    className={cn(
                      'text-sm',
                      (stats.data.change_kg ?? 0) <= 0 ? 'text-success' : 'text-destructive',
                    )}
                  >
                    {signed(stats.data.change_kg)} kg in 30 days
                  </span>
                </div>
                <dl className="grid grid-cols-3 gap-3 text-sm">
                  <Stat label="7-entry avg" value={kg(stats.data.moving_average_7_kg)} />
                  <Stat
                    label="Range"
                    value={`${kg(stats.data.min_kg)} – ${kg(stats.data.max_kg)}`}
                  />
                  <Stat label="Target" value={kg(user?.target_weight_kg)} />
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
            One chart per nutrient you follow. Change which in{' '}
            <Link to="/settings" className="text-primary underline underline-offset-4">
              Settings
            </Link>
            .
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          {summary.isLoading && <Spinner />}
          <ErrorNote error={summary.error} />
          {charted.length === 0 ? (
            <Empty>No nutrients selected to chart.</Empty>
          ) : (summary.data?.days.length ?? 0) === 0 ? (
            <Empty>Nothing logged in this window yet.</Empty>
          ) : (
            <>
              {/* Small multiples rather than several lines on one pair of axes.
                  Calories run to a couple of thousand and fat to about seventy,
                  so a shared y-axis would flatten every macro onto the floor and
                  a second axis would invite comparing two scales that have
                  nothing to do with each other. Each nutrient gets its own axis
                  and its own unit; the heading carries the identity, so no
                  legend is needed and colour is never doing the work alone. */}
              <div
                className={cn(
                  'grid gap-4',
                  charted.length > 1 && 'sm:grid-cols-2',
                )}
              >
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
              <Separator />
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-muted-foreground text-xs">
                  Average over {summary.data?.logged_day_count} logged day
                  {summary.data?.logged_day_count === 1 ? '' : 's'}
                </span>
                {summary.data && <MacroRow n={summary.data.average} compact />}
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
                <XAxis dataKey="date" stroke="var(--muted-foreground)" fontSize={12} tickMargin={8} />
                <YAxis
                  stroke="var(--muted-foreground)"
                  fontSize={12}
                  width={52}
                  domain={['dataMin - 1', 'dataMax + 1']}
                />
                <Tooltip contentStyle={TOOLTIP_STYLE} />
                <Line
                  type="monotone"
                  dataKey="kg"
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
