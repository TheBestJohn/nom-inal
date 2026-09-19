import { Link } from 'react-router-dom'
import { Loader2, TriangleAlert } from 'lucide-react'
import type { ReactNode } from 'react'

import type { Food, Nutrient, Nutrients, TargetProgress, VerificationStatus } from '@/api/types'
import { useAuth } from '@/lib/auth'
import { kcal, round } from '@/lib/format'
import { nutrientValue, orderNutrients } from '@/lib/nutrients'
import { cn } from '@/lib/utils'

/** Mirrors the server's default, for the moment before the profile arrives. */
const DEFAULT_SHOWN: Nutrient[] = ['calories_kcal', 'protein_g', 'carbs_g', 'fat_g']
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'

export function Spinner({ label = 'Loading…' }: { label?: string }) {
  return (
    <div className="text-muted-foreground flex items-center gap-2 py-2 text-sm" role="status">
      <Loader2 className="size-4 animate-spin" />
      <span>{label}</span>
    </div>
  )
}

export function ErrorNote({ error }: { error: unknown }) {
  if (!error) return null
  const message = error instanceof Error ? error.message : String(error)
  return (
    <Alert variant="destructive">
      <TriangleAlert />
      <AlertDescription>{message}</AlertDescription>
    </Alert>
  )
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="text-muted-foreground py-2 text-sm">{children}</p>
}

/**
 * The nutrient readout. One component everywhere a total appears, which is what
 * makes a recipe, a diary entry and a whole day read alike.
 *
 * Which nutrients it shows comes from the signed-in account rather than being
 * fixed at four. Someone tracking sodium for blood pressure was previously
 * looking at a number the app stored, computed and never displayed. Calories
 * keep the emphasis when present, because that is the figure most people are
 * budgeting; the rest follow in the canonical order, coloured to match their
 * bars elsewhere.
 */
export function MacroRow({
  n,
  compact = false,
  className,
}: {
  n: Nutrients
  compact?: boolean
  className?: string
}) {
  const { user } = useAuth()
  // Falls back to the old fixed four while the profile is still loading, so
  // the row never flickers from empty to populated on every page.
  const chosen = orderNutrients(user?.shown_nutrients ?? DEFAULT_SHOWN)

  if (chosen.length === 0) return null

  return (
    <div
      className={cn(
        'tabular flex flex-wrap items-baseline gap-x-3 gap-y-1',
        compact ? 'text-xs' : 'text-sm',
        className,
      )}
    >
      {chosen.map((meta) =>
        meta.key === 'calories_kcal' ? (
          <span
            key={meta.key}
            className={cn('text-foreground font-semibold', compact ? 'text-sm' : 'text-base')}
          >
            {kcal(n.calories_kcal)}
          </span>
        ) : (
          // The colour is an accent beside the text, never the only thing
          // carrying identity -- the short label is always there.
          <span key={meta.key} style={{ color: meta.color }}>
            {meta.short} {round(nutrientValue(n, meta.key))}
            {meta.unit === 'kcal' ? '' : meta.unit}
          </span>
        ),
      )}
    </div>
  )
}

/** Nutrient hue for a bar, matching the figure it belongs to. */
const TONE: Record<string, string> = {
  calories_kcal: 'bg-kcal',
  protein_g: 'bg-protein',
  carbs_g: 'bg-carbs',
  fat_g: 'bg-fat',
}

/**
 * Progress against one target, read in that target's own direction.
 *
 * A budget and a goal are the same arithmetic with opposite meanings: 120% of
 * a calorie budget is a problem, 120% of a protein goal is a success. So only
 * a blown budget turns red, a met goal turns green, and the caption reads
 * "left" for a budget and "to go" for a goal.
 */
export function TargetBar({ target }: { target: TargetProgress }) {
  const { kind, status, percent, amount, consumed, remaining, unit, label } = target
  const over = status === 'over'
  const met = status === 'met'

  const caption = over
    ? `${round(Math.abs(remaining))}${unit} over`
    : met
      ? 'goal met'
      : kind === 'budget'
        ? `${round(remaining)}${unit} left`
        : `${round(remaining)}${unit} to go`

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-3 text-sm">
        {/* No kind badge: the caption below already says "left" for a budget
            and "to go" for a goal, and only a budget ever turns red. The badge
            repeated that in a third place. */}
        <span>{label}</span>
        <span className="tabular whitespace-nowrap">
          {round(consumed)}
          <span className="text-muted-foreground">
            {' / '}
            {round(amount)}
            {unit}
          </span>
        </span>
      </div>
      <Progress
        value={Math.min(100, percent)}
        aria-label={`${label} ${kind}`}
        indicatorClassName={cn(over ? 'bg-destructive' : met ? 'bg-success' : TONE[target.nutrient] ?? 'bg-primary')}
      />
      <span
        className={cn(
          'text-xs',
          over ? 'text-destructive' : met ? 'text-success' : 'text-muted-foreground',
        )}
      >
        {caption}
      </span>
    </div>
  )
}

export function TargetList({ targets }: { targets: TargetProgress[] }) {
  if (targets.length === 0) {
    return (
      <Alert>
        <AlertDescription>
          No goals or budgets set yet —{' '}
          <Link to="/settings" className="text-primary font-medium underline underline-offset-4">
            add them in Settings
          </Link>{' '}
          to track progress.
        </AlertDescription>
      </Alert>
    )
  }
  return (
    <div className="space-y-3">
      {targets.map((t) => (
        <TargetBar key={t.nutrient} target={t} />
      ))}
    </div>
  )
}

export function SourceBadge({ source }: { source: string }) {
  const label = source === 'usda' ? 'USDA' : source === 'off' ? 'OFF' : 'Custom'
  return (
    <Badge variant="outline" className="text-[10px] tracking-wide uppercase">
      {label}
    </Badge>
  )
}

/**
 * A food row's editorial state, from the two cached columns it carries.
 *
 * The detail view gets the same answer from vote counts; this exists so a list
 * can render the badge without an aggregate per result, which the streaming
 * search in particular cannot afford.
 */
export function foodStatus(food: Pick<Food, 'verified_at' | 'disputed_at'>): VerificationStatus {
  if (food.disputed_at) return 'disputed'
  if (food.verified_at) return 'verified'
  return 'unverified'
}

/**
 * How much the community trusts a food's current numbers.
 *
 * Three states rather than a checkmark, because "nobody has looked" and
 * "somebody looked and says this is wrong" are opposite situations that a
 * single boolean would flatten into "not verified".
 */
export function VerificationBadge({
  status,
  confirmations,
  quorum,
  className,
}: {
  status: VerificationStatus
  confirmations?: number
  quorum?: number
  className?: string
}) {
  if (status === 'verified') {
    return (
      <Badge variant="success" className={cn('text-[10px]', className)}>
        Verified
      </Badge>
    )
  }
  if (status === 'disputed') {
    return (
      <Badge variant="destructive" className={cn('text-[10px]', className)}>
        Disputed
      </Badge>
    )
  }
  // The counts turn a bare "Unverified" into something actionable: it says how
  // close the entry is and, implicitly, that one more person could finish it.
  const progress =
    confirmations !== undefined && quorum !== undefined ? ` ${confirmations}/${quorum}` : ''
  return (
    <Badge variant="outline" className={cn('text-muted-foreground text-[10px]', className)}>
      Unverified{progress}
    </Badge>
  )
}
