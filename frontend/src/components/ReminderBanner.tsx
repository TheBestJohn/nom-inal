import { Link } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { BellRing } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { ReminderKind } from '@/api/types'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'

/** Where each nudge sends you to act on it. */
const ACTION: Record<ReminderKind, { to: string; label: string }> = {
  weigh_in: { to: '/weight', label: 'Log it' },
  food_log: { to: '/diary', label: 'Open diary' },
  progress_photo: { to: '/weight', label: 'Add one' },
}

/**
 * Overdue nudges.
 *
 * Nothing is scheduled or pushed — the server derives "overdue" from your own
 * records each time this is read, so the banner can never be stale or fire
 * twice for the same thing.
 */
export default function ReminderBanner() {
  const status = useQuery({
    queryKey: ['reminders', 'status'],
    queryFn: () => api.reminderStatus(),
  })
  const due = (status.data ?? []).filter((r) => r.due)

  if (due.length === 0) return null

  return (
    <div className="space-y-2">
      {due.map((r) => (
        <Alert key={r.kind} variant="warning">
          <BellRing />
          <AlertDescription className="w-full">
            <div className="flex w-full flex-wrap items-center justify-between gap-3">
              <span>{r.message}</span>
              <Button asChild variant="outline" size="sm">
                <Link to={ACTION[r.kind].to}>{ACTION[r.kind].label}</Link>
              </Button>
            </div>
          </AlertDescription>
        </Alert>
      ))}
    </div>
  )
}
