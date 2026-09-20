import { Link } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { api } from '@/api/endpoints'
import type { TdeeEstimate } from '@/api/types'
import { kcal } from '@/lib/format'
import { applyTargets } from '@/lib/targets'
import { Button } from '@/components/ui/button'
import { ErrorNote } from '@/components/shared'

const WINDOW_DAYS = 28

/** "lost 0.4 kg/week", "gained 0.2 kg/week", or that the scale held. */
function trendPhrase(rate: number): string {
  if (Math.abs(rate) < 0.05) return 'your weight has held steady'
  return `you've ${rate < 0 ? 'lost' : 'gained'} ${Math.abs(rate)} kg/week`
}

/**
 * One line under today's intake: what your own complete days say you
 * actually burn, or what is still needed before that can be said.
 *
 * Both states are rendered on purpose. An estimator that appears only once
 * it has enough data teaches nobody that the flag on the diary exists, and
 * the person who most needs the number is the one who has not yet earned
 * it. Silence is the bug.
 */
export default function ExpenditureLine() {
  const queryClient = useQueryClient()
  const tdee = useQuery({
    queryKey: ['estimates', 'tdee', WINDOW_DAYS],
    queryFn: () => api.tdeeEstimate(WINDOW_DAYS),
  })

  const useAsBudget = useMutation({
    mutationFn: (amount: number) =>
      applyTargets([{ nutrient: 'calories_kcal', amount, kind: 'budget' }]),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['targets'] })
      queryClient.invalidateQueries({ queryKey: ['diary'] })
    },
  })

  if (tdee.isLoading) return null
  if (tdee.error) return <ErrorNote error={tdee.error} />
  if (!tdee.data) return null

  return (
    <div className="space-y-1">
      <p className="text-muted-foreground text-xs">
        <Sentence data={tdee.data} />
      </p>
      {tdee.data.estimate && (
        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={useAsBudget.isPending}
            onClick={() => useAsBudget.mutate(tdee.data.estimate!.budget_kcal)}
          >
            {useAsBudget.isPending
              ? 'Saving…'
              : `Use ${kcal(tdee.data.estimate.budget_kcal)} as budget`}
          </Button>
          <span className="text-muted-foreground text-xs">
            {tdee.data.estimate.goal_adjustment_kcal === 0
              ? 'your expenditure, to the nearest ten'
              : `${tdee.data.estimate.goal_adjustment_kcal > 0 ? '+' : '−'}${Math.abs(
                  tdee.data.estimate.goal_adjustment_kcal,
                )} kcal for your goal`}
            {tdee.data.estimate.floored_at_minimum ? ', held at the 1200 kcal floor' : ''}
            {tdee.data.estimate.confidence !== 'good'
              ? ` · ${tdee.data.estimate.confidence} confidence`
              : ''}
          </span>
        </div>
      )}
      <ErrorNote error={useAsBudget.error} />
    </div>
  )
}

function Sentence({ data }: { data: TdeeEstimate }) {
  if (data.estimate) {
    const e = data.estimate
    return (
      <>
        Your last {data.have.complete_days} complete day
        {data.have.complete_days === 1 ? '' : 's'} average {kcal(e.mean_intake_kcal)} and{' '}
        {trendPhrase(e.weight_change_kg_per_week)} — estimated expenditure{' '}
        <strong className="text-foreground">{kcal(e.tdee_kcal)}</strong>
        {data.formula ? ` (the formula says ${kcal(data.formula.tdee_kcal)})` : ''}.
      </>
    )
  }
  // The server's sentence starts "Needs …"; it reads on from here.
  const reason = data.reason ? data.reason.charAt(0).toLowerCase() + data.reason.slice(1) : ''
  return (
    <>
      To estimate your real expenditure from your own data, it {reason} Mark a day with{' '}
      <Link to="/diary" className="text-primary underline underline-offset-4">
        “I logged everything”
      </Link>{' '}
      in the diary.
    </>
  )
}
