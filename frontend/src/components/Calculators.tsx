import { useState } from 'react'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { TriangleAlert } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { TargetInput } from '@/api/endpoints'
import type { Nutrient, NutritionTarget, Projection } from '@/api/types'
import { useAuth } from '@/lib/auth'
import { kcal, kg, prettyDate, round, today } from '@/lib/format'
import { applyTargets } from '@/lib/targets'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Separator } from '@/components/ui/separator'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { ErrorNote, Spinner } from '@/components/shared'

/** kcal per gram, the Atwater factors the server prices presets with. */
const KCAL_PER_G = { protein: 4, carbs: 4, fat: 9 }

/**
 * Protein per kilogram, by the profile's goal.
 *
 * 1.2–1.6 g/kg covers general health — the upper end is where the
 * dose–response for resistance training plateaus (Morton et al. 2018).
 * Cutting or gaining moves to 1.6–2.2 g/kg: lean mass is under pressure in
 * a deficit, and is the point of a surplus. The server's presets pick one
 * figure inside the band; the default here is that figure.
 */
const PROTEIN_BANDS: Record<string, { lo: number; hi: number; why: string }> = {
  maintain: { lo: 1.2, hi: 1.6, why: 'general health and maintenance' },
  cut: { lo: 1.6, hi: 2.2, why: 'cutting, to keep lean mass through a deficit' },
  bulk: { lo: 1.6, hi: 2.2, why: 'gaining, where muscle is the point' },
}

/** Protein / carbs / fat as shares of calories. */
interface Split {
  p: number
  c: number
  f: number
}

const SPLITS: { key: string; label: string; split: Split; why: string }[] = [
  {
    key: 'balanced',
    label: 'Balanced',
    split: { p: 30, c: 40, f: 30 },
    why: 'Inside the 10–35 / 45–65 / 20–35 acceptable ranges, weighted towards protein.',
  },
  {
    key: 'protein',
    label: 'Higher protein',
    split: { p: 40, c: 30, f: 30 },
    why: 'The usual cutting split: protein high enough to be filling and protect lean mass.',
  },
  {
    key: 'lowcarb',
    label: 'Lower carb',
    split: { p: 35, c: 25, f: 40 },
    why: 'Carbohydrate under the usual range without being ketogenic.',
  },
  {
    key: 'keto',
    label: 'Keto',
    split: { p: 25, c: 5, f: 70 },
    why: 'Carbohydrate near nothing; check the ratio below rather than the percentages.',
  },
]

/** The four WHO bands. A screening figure, not a verdict — see the caveat. */
function bmiBand(bmi: number): string {
  if (bmi < 18.5) return 'under the 18.5 line'
  if (bmi < 25) return 'in the 18.5–25 band'
  if (bmi < 30) return 'in the 25–30 band'
  return 'over 30'
}

/** fat : (protein + carbs) by grams, the classic ketogenic ratio. */
function ketoRatio(fat: number, protein: number, carbs: number): number | null {
  const rest = protein + carbs
  if (rest <= 0 || fat <= 0) return null
  return fat / rest
}

function Section({
  title,
  children,
  className,
}: {
  title: string
  children: ReactNode
  className?: string
}) {
  return (
    <section className={cn('space-y-2', className)}>
      <h3 className="text-sm font-semibold">{title}</h3>
      {children}
    </section>
  )
}

function Note({ children }: { children: ReactNode }) {
  return <p className="text-muted-foreground text-xs">{children}</p>
}

function Caution({ children }: { children: ReactNode }) {
  return (
    <p className="text-carbs flex items-center gap-1.5 text-xs">
      <TriangleAlert className="size-3.5 shrink-0" />
      <span>{children}</span>
    </p>
  )
}

/**
 * The calculators, where the numbers they produce are used: beside the
 * targets they write. Each one says what it read and what it would set, and
 * writes ordinary targets through the same endpoint the editor uses, so
 * nothing here is a second source of truth — the weight is the latest
 * weigh-in (or the target weight, and it says so), the calorie budget is the
 * one set above (or the server's suggestion, and it says so), and the
 * protein default is the figure the focus preset would have written.
 */
export default function Calculators() {
  const { user } = useAuth()
  const queryClient = useQueryClient()

  const targets = useQuery({ queryKey: ['targets'], queryFn: () => api.listTargets() })
  const suggestion = useQuery({
    queryKey: ['targets', 'suggestion'],
    queryFn: () => api.targetSuggestion(),
  })
  const latest = useQuery({
    queryKey: ['weights', 'latest'],
    queryFn: () => api.listWeights({ limit: 1 }),
  })
  const day = useQuery({
    queryKey: ['diary', 'day', today()],
    queryFn: () => api.diaryDay(today()),
  })

  const apply = useMutation({
    mutationFn: (patch: TargetInput[]) => applyTargets(patch),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['targets'] })
      queryClient.invalidateQueries({ queryKey: ['diary'] })
    },
  })

  const [perKgInput, setPerKgInput] = useState<string | null>(null)
  const [splitKey, setSplitKey] = useState('balanced')
  const [custom, setCustom] = useState<Split>({ p: 30, c: 40, f: 30 })

  if (targets.isLoading || suggestion.isLoading || latest.isLoading) return <Spinner />

  const target = (n: Nutrient) => targets.data?.find((t) => t.nutrient === n)
  const goal = user?.goal ?? 'maintain'

  // Body weight, by the server's own rule: the scale first, the target
  // weight when there is no weigh-in yet. Said, not assumed.
  const weighIn = latest.data?.[0]?.weight_kg ?? null
  const weightKg = weighIn ?? user?.target_weight_kg ?? null
  const weightSource =
    weighIn !== null
      ? 'your latest weigh-in'
      : weightKg !== null
        ? 'your target weight, since there is no weigh-in yet'
        : null

  // ---- protein per kg ----
  const band = PROTEIN_BANDS[goal] ?? PROTEIN_BANDS.maintain
  const presetProtein = suggestion.data?.targets.find((t) => t.nutrient === 'protein_g')
  const presetPerKg =
    presetProtein?.amount != null && weightKg ? round(presetProtein.amount / weightKg, 1) : band.hi
  const perKg = perKgInput === null ? presetPerKg : Number(perKgInput)
  const proteinGrams = weightKg && perKg > 0 ? Math.round(perKg * weightKg) : null
  const outsideBand = perKg > 0 && (perKg < band.lo || perKg > band.hi)

  // ---- macro split ----
  const calorieTarget = target('calories_kcal')
  const suggestedCalories = suggestion.data?.targets.find((t) => t.nutrient === 'calories_kcal')
  const budget = calorieTarget?.amount ?? suggestedCalories?.amount ?? null
  const budgetSource = calorieTarget
    ? 'your calorie budget'
    : budget
      ? 'the suggested budget, since no calorie budget is set'
      : null
  const split = splitKey === 'custom' ? custom : SPLITS.find((s) => s.key === splitKey)!.split
  const splitSum = split.p + split.c + split.f
  const grams =
    budget && splitSum === 100
      ? {
          protein: Math.round((budget * split.p) / 100 / KCAL_PER_G.protein),
          carbs: Math.round((budget * split.c) / 100 / KCAL_PER_G.carbs),
          fat: Math.round((budget * split.f) / 100 / KCAL_PER_G.fat),
        }
      : null

  // ---- keto ratio ----
  const fatT = target('fat_g')
  const proteinT = target('protein_g')
  const carbsT = target('carbs_g') ?? target('net_carbs_g')
  const plannedRatio =
    fatT && proteinT && carbsT ? ketoRatio(fatT.amount, proteinT.amount, carbsT.amount) : null
  const todayRatio = day.data
    ? ketoRatio(day.data.total.fat_g, day.data.total.protein_g, day.data.total.carbs_g)
    : null

  // ---- BMI ----
  const bmi = weightKg && user?.height_cm ? round(weightKg / (user.height_cm / 100) ** 2, 1) : null

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Calculators</CardTitle>
          <CardDescription>
            Arithmetic on your own figures, with what each one read. Applying writes an ordinary
            target above; nothing here is enforced.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-5">
          <Section title="Protein per kilogram">
            {weightKg === null ? (
              <Note>
                Needs a body weight —{' '}
                <Link to="/weight" className="text-primary underline underline-offset-4">
                  log a weigh-in
                </Link>{' '}
                or set a target weight under Body.
              </Note>
            ) : (
              <>
                <div className="flex flex-wrap items-end gap-3">
                  <div className="space-y-1.5">
                    <Label htmlFor="protein-per-kg">g per kg</Label>
                    <Input
                      id="protein-per-kg"
                      type="number"
                      step="0.1"
                      min={0}
                      inputMode="decimal"
                      className="tabular w-24"
                      value={perKgInput ?? String(presetPerKg)}
                      onChange={(e) => setPerKgInput(e.target.value)}
                    />
                  </div>
                  <p className="tabular pb-2 text-sm">
                    × {kg(weightKg)} ={' '}
                    <strong>{proteinGrams === null ? '—' : `${proteinGrams} g`}</strong> a day
                  </p>
                  <Button
                    variant="outline"
                    size="sm"
                    className="mb-1"
                    disabled={proteinGrams === null || apply.isPending}
                    onClick={() =>
                      apply.mutate([{ nutrient: 'protein_g', amount: proteinGrams!, kind: 'goal' }])
                    }
                  >
                    Apply as protein goal
                  </Button>
                </div>
                <Note>
                  {band.lo}–{band.hi} g/kg is the usual band for {band.why}, by your goal ({goal}).
                  Weight is {weightSource}
                  {presetProtein?.amount != null
                    ? `; ${presetPerKg} g/kg is what your preset uses`
                    : ''}
                  .
                </Note>
                {outsideBand && (
                  <Caution>
                    {perKg} g/kg is outside the {band.lo}–{band.hi} band for your goal.
                  </Caution>
                )}
              </>
            )}
          </Section>

          <Separator />

          <Section title="Macro split">
            {budget === null ? (
              <Note>
                Needs a calorie budget. Set one above, or fill in Body so a suggestion can be made.
              </Note>
            ) : (
              <>
                <ToggleGroup
                  type="single"
                  size="sm"
                  value={splitKey}
                  onValueChange={(v) => v && setSplitKey(v)}
                  aria-label="Ratio"
                  className="flex-wrap justify-start"
                >
                  {SPLITS.map((s) => (
                    <ToggleGroupItem key={s.key} value={s.key}>
                      {s.label} {s.split.p}/{s.split.c}/{s.split.f}
                    </ToggleGroupItem>
                  ))}
                  <ToggleGroupItem value="custom">Custom</ToggleGroupItem>
                </ToggleGroup>
                {splitKey === 'custom' && (
                  <div className="flex flex-wrap gap-3">
                    {(['p', 'c', 'f'] as const).map((k) => (
                      <div key={k} className="space-y-1.5">
                        <Label htmlFor={`split-${k}`}>
                          {k === 'p' ? 'Protein' : k === 'c' ? 'Carbs' : 'Fat'} %
                        </Label>
                        <Input
                          id={`split-${k}`}
                          type="number"
                          min={0}
                          max={100}
                          inputMode="numeric"
                          className="tabular w-20"
                          value={custom[k]}
                          onChange={(e) => setCustom({ ...custom, [k]: Number(e.target.value) })}
                        />
                      </div>
                    ))}
                  </div>
                )}
                {splitSum !== 100 ? (
                  <Caution>The shares add up to {splitSum}%, not 100%.</Caution>
                ) : (
                  <div className="flex flex-wrap items-center gap-3">
                    <p className="tabular text-sm">
                      Of {kcal(budget)}: <strong>{grams!.protein} g</strong> protein ·{' '}
                      <strong>{grams!.carbs} g</strong> carbs · <strong>{grams!.fat} g</strong> fat
                    </p>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={apply.isPending}
                      onClick={() =>
                        apply.mutate([
                          { nutrient: 'protein_g', amount: grams!.protein, kind: 'goal' },
                          // Carbs and fat keep the direction already chosen
                          // for them: a bulk's carbs goal stays a goal.
                          {
                            nutrient: 'carbs_g',
                            amount: grams!.carbs,
                            kind: target('carbs_g')?.kind ?? 'budget',
                          },
                          { nutrient: 'fat_g', amount: grams!.fat, kind: fatT?.kind ?? 'budget' },
                        ])
                      }
                    >
                      Apply to targets
                    </Button>
                  </div>
                )}
                <Note>
                  {SPLITS.find((s) => s.key === splitKey)?.why ?? 'Your own shares.'} Grams are the
                  share of {budgetSource} at 4 / 4 / 9 kcal per gram.
                </Note>
              </>
            )}
          </Section>

          <Separator />

          <Section title="Keto ratio check">
            <p className="tabular text-sm">
              Your targets:{' '}
              <strong>{plannedRatio === null ? '—' : `${round(plannedRatio, 1)} : 1`}</strong>
              {' · '}
              Today so far:{' '}
              <strong>{todayRatio === null ? '—' : `${round(todayRatio, 1)} : 1`}</strong>
            </p>
            {plannedRatio === null && (
              <Note>
                The targets figure needs fat, protein and carbs (or net carbs) targets set above.
              </Note>
            )}
            {todayRatio === null && day.data && <Note>Nothing with fat logged today yet.</Note>}
            <Note>
              Fat to protein-plus-carbohydrate, by grams. 4 : 1 is the classic therapeutic ketogenic
              diet, 3 : 1 and 2 : 1 its relaxed forms, and about 1 : 1 — fat matching everything
              else — is the modified pattern most people mean by keto. The ratio is what keeps
              ketosis, not the percentages: protein counts against it.
            </Note>
          </Section>

          <Separator />

          <Section title="BMI">
            {bmi === null ? (
              <Note>
                Needs a height and a body weight — fill in{' '}
                <Link to="/settings/body" className="text-primary underline underline-offset-4">
                  Body
                </Link>
                .
              </Note>
            ) : (
              <p className="tabular text-sm">
                <strong>{bmi}</strong>, {bmiBand(bmi)} — {kg(weightKg)} at {user!.height_cm} cm,
                weight from {weightSource}.
              </p>
            )}
            <Note>
              BMI is a population screening ratio: it cannot tell muscle from fat or say where the
              weight sits, so the trend on the scale says more about you than the band does.
            </Note>
          </Section>

          <ErrorNote error={apply.error} />
          <ErrorNote error={targets.error} />
          <ErrorNote error={suggestion.error} />
        </CardContent>
      </Card>

      <ProjectionCard targets={targets.data ?? []} />
    </>
  )
}

function describeReached(p: Projection): string {
  switch (p.reached_reason) {
    case 'no_target_weight':
      return 'No target weight is set — add one under Body to see when the trend reaches it.'
    case 'trend_is_flat':
      return 'The trend is flat, so there is no date to project; that is the answer, not a gap.'
    case 'trend_points_away':
      return 'The trend is heading away from your target weight.'
    default:
      return ''
  }
}

/**
 * Where the weight trend is going, and what a chosen date would take. Reads
 * the same trend the adaptive estimate uses, so the two never disagree about
 * which way the scale is moving.
 */
function ProjectionCard({ targets }: { targets: NutritionTarget[] }) {
  const queryClient = useQueryClient()
  const [by, setBy] = useState('')
  const projection = useQuery({
    queryKey: ['estimates', 'projection', by || null],
    queryFn: () => api.projection(by ? { by } : {}),
  })
  const useAsBudget = useMutation({
    mutationFn: (amount: number) =>
      applyTargets([{ nutrient: 'calories_kcal', amount, kind: 'budget' }]),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['targets'] })
      queryClient.invalidateQueries({ queryKey: ['diary'] })
    },
  })

  const p = projection.data
  const currentBudget = targets.find((t) => t.nutrient === 'calories_kcal')?.amount

  return (
    <Card>
      <CardHeader>
        <CardTitle>Goal projection</CardTitle>
        <CardDescription>
          A straight line through your last 28 days of weigh-ins, extended to your target weight.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {projection.isLoading && <Spinner />}
        <ErrorNote error={projection.error} />
        {p && !p.ready && (
          <Note>
            Not enough weigh-ins for a trend yet: {p.have.weigh_ins} in the window
            {p.have.weigh_ins >= 2 ? `, spanning ${p.have.span_days} days` : ''}. It{' '}
            {p.reason ? p.reason.charAt(0).toLowerCase() + p.reason.slice(1) : ''}{' '}
            <Link to="/weight" className="text-primary underline underline-offset-4">
              Log a weigh-in
            </Link>
            .
          </Note>
        )}
        {p?.trend && (
          <>
            <p className="tabular text-sm">
              <strong>
                {p.trend.rate_kg_per_week > 0 ? '+' : ''}
                {p.trend.rate_kg_per_week} kg/week
              </strong>{' '}
              since {prettyDate(p.trend.first_weigh_in)}: {kg(p.trend.start_kg)} →{' '}
              {kg(p.trend.current_kg)} on {prettyDate(p.trend.as_of)}.
              {p.reached_on && p.days_to_target !== null && (
                <>
                  {' '}
                  At this rate you reach {kg(p.target_weight_kg)} on{' '}
                  <strong>{prettyDate(p.reached_on)}</strong> ({p.days_to_target} days).
                </>
              )}
            </p>
            {p.trend.caution && (
              <Caution>
                Faster than {p.trend.caution_threshold_kg_per_week} kg/week, which is 1% of your
                body weight — the usual line past which a change is not mostly fat.
              </Caution>
            )}
            {!p.reached_on && <Note>{describeReached(p)}</Note>}

            <div className="flex flex-wrap items-end gap-3">
              <div className="space-y-1.5">
                <Label htmlFor="by-date">Reach it by</Label>
                <Input
                  id="by-date"
                  type="date"
                  className="w-auto"
                  value={by}
                  min={today()}
                  onChange={(e) => setBy(e.target.value)}
                />
              </div>
              {by && (
                <Button variant="ghost" size="sm" className="mb-1" onClick={() => setBy('')}>
                  Clear
                </Button>
              )}
            </div>

            {by && p.by_reason && (
              <Note>
                {p.by_reason === 'no_target_weight'
                  ? 'Needs a target weight, set under Body.'
                  : p.by_reason === 'date_not_after_as_of'
                    ? `The date has to be after your last weigh-in, ${prettyDate(p.trend.as_of)}.`
                    : 'Needs a trend first.'}
              </Note>
            )}
            {p.by && (
              <div className="space-y-2">
                <p className="tabular text-sm">
                  To reach {kg(p.target_weight_kg)} by {prettyDate(p.by.date)}:{' '}
                  <strong>
                    {p.by.daily_energy_change_kcal > 0 ? '+' : '−'}
                    {kcal(Math.abs(p.by.daily_energy_change_kcal))}
                  </strong>{' '}
                  a day {p.by.daily_energy_change_kcal > 0 ? 'over' : 'under'} your expenditure,{' '}
                  {Math.abs(p.by.required_rate_kg_per_week)} kg/week for {p.by.days} days from your
                  last weigh-in.
                </p>
                {p.by.caution && (
                  <Caution>
                    That is past 1% of your body weight a week
                    {p.by.floored_at_minimum ? ', and under the 1200 kcal floor' : ''}.
                  </Caution>
                )}
                {p.by.suggested_intake_kcal !== null ? (
                  <div className="flex flex-wrap items-center gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={useAsBudget.isPending}
                      onClick={() => useAsBudget.mutate(p.by!.suggested_intake_kcal!)}
                    >
                      Use {kcal(p.by.suggested_intake_kcal)} as budget
                    </Button>
                    <Note>
                      {kcal(p.by.basis_tdee_kcal)}{' '}
                      {p.by.basis === 'adaptive' ? 'measured' : 'formula'} expenditure{' '}
                      {p.by.daily_energy_change_kcal > 0 ? '+' : '−'}{' '}
                      {Math.abs(p.by.daily_energy_change_kcal)}
                      {p.by.floored_at_minimum ? ', held at the 1200 kcal floor' : ''}
                      {currentBudget ? `; your budget is ${kcal(currentBudget)}` : ''}.
                    </Note>
                  </div>
                ) : (
                  <Note>
                    No intake to suggest: neither the adaptive estimate nor the formula is available
                    yet. The change above still holds against whatever you burn.
                  </Note>
                )}
              </div>
            )}
            <ErrorNote error={useAsBudget.error} />
          </>
        )}
      </CardContent>
    </Card>
  )
}
