import { useQuery } from '@tanstack/react-query'
import { Check } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { TrackingFocus } from '@/api/types'
import { cn } from '@/lib/utils'
import { ErrorNote, Spinner } from '@/components/shared'

/**
 * The eight answers to "why are you tracking?", one sentence each.
 *
 * The list and the sentences come from the server, which is also where the
 * presets live, so a focus cannot be offered here that the API does not
 * know how to apply. Rendered as a radio group: it is one choice, and the
 * cards are large because on a phone this is the first thing a new account
 * touches.
 */
export default function FocusPicker({
  value,
  onChange,
  disabled = false,
}: {
  value: TrackingFocus | null
  onChange: (focus: TrackingFocus) => void
  disabled?: boolean
}) {
  const options = useQuery({ queryKey: ['focus', 'options'], queryFn: () => api.focusOptions() })

  if (options.isLoading) return <Spinner />
  if (options.error) return <ErrorNote error={options.error} />

  return (
    <div role="radiogroup" aria-label="Tracking focus" className="grid gap-2 sm:grid-cols-2">
      {(options.data ?? []).map((o) => {
        const active = o.focus === value
        return (
          <button
            key={o.focus}
            type="button"
            role="radio"
            aria-checked={active}
            disabled={disabled}
            onClick={() => onChange(o.focus)}
            className={cn(
              'flex items-start gap-3 rounded-lg border p-3 text-left transition-colors',
              'focus-visible:ring-ring/50 outline-none focus-visible:ring-[3px]',
              'disabled:opacity-60',
              active ? 'border-primary bg-primary/5' : 'hover:bg-accent',
            )}
          >
            <span
              aria-hidden="true"
              className={cn(
                'mt-0.5 grid size-5 shrink-0 place-items-center rounded-full border',
                active ? 'border-primary bg-primary text-primary-foreground' : 'border-input',
              )}
            >
              {active && <Check className="size-3.5" />}
            </span>
            <span className="min-w-0">
              <span className="block font-medium">{o.label}</span>
              <span className="text-muted-foreground block text-sm">{o.summary}</span>
            </span>
          </button>
        )
      })}
    </div>
  )
}
