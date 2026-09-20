import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { api } from '@/api/endpoints'
import type { Reminder, ReminderKind } from '@/api/types'
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
import { Switch } from '@/components/ui/switch'
import { ErrorNote, Spinner } from '@/components/shared'

const HINT: Record<ReminderKind, string> = {
  weigh_in: 'Nudges you when your last weigh-in is older than this.',
  food_log: 'Nudges you when you have not logged any food for this long.',
  progress_photo: 'Nudges you when your last weigh-in photo is older than this.',
}

export default function RemindersEditor() {
  const queryClient = useQueryClient()
  const [rows, setRows] = useState<Reminder[]>([])
  const [saved, setSaved] = useState(false)

  const reminders = useQuery({ queryKey: ['reminders'], queryFn: () => api.listReminders() })

  useEffect(() => {
    if (reminders.data) setRows(reminders.data)
  }, [reminders.data])

  const save = useMutation({
    mutationFn: () =>
      api.replaceReminders(
        rows.map((r) => ({ kind: r.kind, every_days: r.every_days, enabled: r.enabled })),
      ),
    onSuccess: () => {
      // The banner reads a different query, so both have to be refreshed.
      queryClient.invalidateQueries({ queryKey: ['reminders'] })
      setSaved(true)
      setTimeout(() => setSaved(false), 2500)
    },
  })

  const set = (kind: ReminderKind, patch: Partial<Reminder>) =>
    setRows((rs) => rs.map((r) => (r.kind === kind ? { ...r, ...patch } : r)))

  if (reminders.isLoading) return <Spinner />

  return (
    <Card>
      <CardHeader>
        <CardTitle>Reminders</CardTitle>
        <CardDescription>
          Shown in the app when you are overdue. Nothing is emailed or pushed, and nothing is
          scheduled — being overdue is worked out from what you have actually recorded.
        </CardDescription>
        {saved && (
          <CardAction>
            <Badge variant="success">Saved</Badge>
          </CardAction>
        )}
      </CardHeader>

      <CardContent className="space-y-4">
        <div className="divide-y">
          {rows.map((r) => (
            <div key={r.kind} className="flex flex-wrap items-center gap-x-4 gap-y-2 py-3">
              <div className="min-w-0 flex-1">
                <Label
                  htmlFor={`rem-${r.kind}`}
                  className={r.enabled ? '' : 'text-muted-foreground'}
                >
                  {r.label}
                </Label>
                <p className="text-muted-foreground text-xs">{HINT[r.kind]}</p>
              </div>

              <div className="flex items-center gap-2">
                <span className="text-muted-foreground text-xs">every</span>
                <Input
                  id={`rem-${r.kind}`}
                  type="number"
                  min={1}
                  max={365}
                  disabled={!r.enabled}
                  className="tabular w-20 text-right"
                  value={r.every_days}
                  onChange={(e) => set(r.kind, { every_days: Number(e.target.value) })}
                />
                <span className="text-muted-foreground text-xs">days</span>
              </div>

              <Switch
                checked={r.enabled}
                onCheckedChange={(enabled) => set(r.kind, { enabled })}
                aria-label={`Enable ${r.label} reminder`}
              />
            </div>
          ))}
        </div>

        <ErrorNote error={reminders.error} />
        <ErrorNote error={save.error} />

        <Button disabled={save.isPending} onClick={() => save.mutate()}>
          {save.isPending ? 'Saving…' : 'Save reminders'}
        </Button>
      </CardContent>
    </Card>
  )
}
