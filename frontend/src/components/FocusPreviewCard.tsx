import { Link } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'

import { api } from '@/api/endpoints'
import type { FocusPreview, TrackingFocus } from '@/api/types'
import { weight } from '@/lib/format'
import { nutrientMeta } from '@/lib/nutrients'
import { useUnits } from '@/lib/useUnits'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { ErrorNote, Spinner } from '@/components/shared'
import { GOALS } from '@/components/BodyBasicsForm'

/** Field names the API reports as missing, as they read on screen. */
const FIELD_LABELS: Record<string, string> = {
  weight: 'a weigh-in',
  height_cm: 'your height',
  birth_date: 'your date of birth',
}

const listMissing = (fields: string[]) => fields.map((f) => FIELD_LABELS[f] ?? f).join(', ')

/**
 * What applying a focus would set, laid out before anything is written.
 *
 * Every figure comes with the sentence the server derived it from, because a
 * preset that just appears as numbers reads as a prescription. A target the
 * profile cannot price is listed with what it needs rather than left out —
 * a preview quietly short of a row is worse than one that says why.
 */
export function FocusPreviewBody({ preview }: { preview: FocusPreview }) {
  const { units } = useUnits()
  const goal = GOALS.find((g) => g.value === preview.goal)

  return (
    <div className="space-y-4">
      {preview.missing.length > 0 && (
        <Alert variant="warning">
          <AlertDescription>
            Some amounts need {listMissing(preview.missing)} —{' '}
            <Link to="/settings/body" className="font-medium underline underline-offset-4">
              add them under Body
            </Link>{' '}
            and the preview fills in.
          </AlertDescription>
        </Alert>
      )}

      {preview.targets.length === 0 ? (
        <p className="text-muted-foreground text-sm">
          No targets. You choose your own under Targets.
        </p>
      ) : (
        <ul className="divide-y">
          {preview.targets.map((t) => (
            <li key={t.nutrient} className="space-y-1 py-2.5">
              <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
                <span className="flex items-center gap-2 font-medium">
                  <span
                    aria-hidden="true"
                    className="size-2.5 shrink-0 rounded-full"
                    style={{ background: nutrientMeta(t.nutrient)?.color }}
                  />
                  {t.label}
                  <Badge variant="outline" className="text-[10px]">
                    {t.kind}
                  </Badge>
                </span>
                <span className="tabular">
                  {t.amount === null ? (
                    <span className="text-muted-foreground">needs {listMissing(t.needs)}</span>
                  ) : (
                    <>
                      {t.amount.toLocaleString()} {t.unit}
                    </>
                  )}
                </span>
              </div>
              <p className="text-muted-foreground text-xs">{t.rationale}</p>
            </li>
          ))}
        </ul>
      )}

      {preview.changes_display && (
        <dl className="grid gap-2 text-sm sm:grid-cols-2">
          <div>
            <dt className="text-muted-foreground text-xs">In readouts</dt>
            <dd>{preview.shown_nutrients.map((n) => nutrientMeta(n)?.label ?? n).join(', ')}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground text-xs">On the home page</dt>
            <dd>
              {preview.chart_nutrients.map((n) => nutrientMeta(n)?.label ?? n).join(', ')}
              <span className="text-muted-foreground"> — as a share of target</span>
            </dd>
          </div>
        </dl>
      )}

      {goal && (
        <p className="text-muted-foreground text-xs">
          Also sets your goal to <strong>{goal.label.toLowerCase()}</strong>, so the estimate and
          the focus agree.
        </p>
      )}

      {preview.estimate && (
        <p className="text-muted-foreground text-xs">
          Estimate: {Math.round(preview.estimate.bmr_kcal)} kcal resting ×{' '}
          {preview.estimate.activity_factor} activity = {Math.round(preview.estimate.tdee_kcal)}{' '}
          kcal a day, from {weight(preview.estimate.weight_kg, units)}
          {preview.estimate.weight_source === 'target_weight'
            ? ' (your target weight — no weigh-in yet)'
            : ''}
          {preview.estimate.sex_assumed_male ? '; sex unstated, so the male constant was used' : ''}
          . An estimate — adjust to what the scale actually does.
        </p>
      )}
    </div>
  )
}

/** Fetches and shows the preview for one focus. */
export default function FocusPreviewCard({ focus }: { focus: TrackingFocus }) {
  const preview = useQuery({
    queryKey: ['focus', 'preview', focus],
    queryFn: () => api.focusPreview(focus),
  })

  if (preview.isLoading) return <Spinner label="Working out the numbers…" />
  if (preview.error) return <ErrorNote error={preview.error} />
  if (!preview.data) return null
  return <FocusPreviewBody preview={preview.data} />
}
