import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { api } from '@/api/endpoints'
import type { TrackingFocus } from '@/api/types'
import { useAuth } from '@/lib/auth'
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { ErrorNote } from '@/components/shared'
import FocusPicker from '@/components/FocusPicker'
import FocusPreviewCard from '@/components/FocusPreviewCard'

/**
 * Change the focus after the fact.
 *
 * The picker shows the preview for whatever is selected, and applying it
 * overwrites the targets and display preferences — so when targets already
 * exist, it asks first. Recording the focus without applying is always
 * available; that is how someone who set everything up by hand says what
 * they are doing without losing it.
 */
export default function FocusSettings() {
  const { user, setUser } = useAuth()
  const queryClient = useQueryClient()
  const [selected, setSelected] = useState<TrackingFocus | null>(user?.tracking_focus ?? null)
  const [confirming, setConfirming] = useState(false)
  const [saved, setSaved] = useState<'applied' | 'recorded' | null>(null)

  const targets = useQuery({ queryKey: ['targets'], queryFn: () => api.listTargets() })

  const save = useMutation({
    mutationFn: (body: { focus: TrackingFocus; apply: boolean }) => api.setFocus(body),
    onSuccess: (result, body) => {
      setUser(result.profile)
      // Targets feed the diary and the home page through their day query, so
      // both need refreshing, not just the list.
      queryClient.invalidateQueries({ queryKey: ['targets'] })
      queryClient.invalidateQueries({ queryKey: ['diary'] })
      setConfirming(false)
      setSaved(body.apply ? 'applied' : 'recorded')
      setTimeout(() => setSaved(null), 2500)
    },
  })

  const current = user?.tracking_focus ?? null
  const changed = selected !== null && selected !== current
  const hasTargets = (targets.data?.length ?? 0) > 0

  const apply = () => {
    if (!selected) return
    if (hasTargets) setConfirming(true)
    else save.mutate({ focus: selected, apply: true })
  }

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Why you are tracking</CardTitle>
          <CardDescription>
            A focus is a preset: it fills in which nutrients are on screen and a first set of
            targets. Nothing here is enforced — every target stays yours to edit.
          </CardDescription>
          {saved && (
            <CardAction>
              <Badge variant="success">{saved === 'applied' ? 'Applied' : 'Saved'}</Badge>
            </CardAction>
          )}
        </CardHeader>
        <CardContent className="space-y-4">
          <FocusPicker value={selected} onChange={setSelected} disabled={save.isPending} />
          <ErrorNote error={save.error} />
        </CardContent>
      </Card>

      {selected && (
        <Card>
          <CardHeader>
            <CardTitle>{changed ? 'What applying would set' : 'What this focus sets'}</CardTitle>
            <CardDescription>
              {selected === 'custom'
                ? 'No preset. Your targets and display stay exactly as they are.'
                : 'Priced from your body basics and the latest weigh-in.'}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <FocusPreviewCard focus={selected} />
            <div className="flex flex-wrap items-center gap-2">
              <Button
                onClick={apply}
                disabled={save.isPending || (selected === 'custom' && !changed)}
              >
                {selected === 'custom' ? 'Set focus' : changed ? 'Apply' : 'Apply again'}
              </Button>
              {selected !== 'custom' && (
                <Button
                  variant="outline"
                  onClick={() => save.mutate({ focus: selected, apply: false })}
                  disabled={save.isPending || !changed}
                >
                  Set focus without applying
                </Button>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      <Dialog open={confirming} onOpenChange={setConfirming}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Replace your targets?</DialogTitle>
            <DialogDescription>
              You have {targets.data?.length} target{targets.data?.length === 1 ? '' : 's'} set.
              Applying this focus replaces all of them with the ones above, and changes which
              nutrients are shown and charted. Your diary and weigh-ins are not touched.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirming(false)}>
              Keep mine
            </Button>
            <Button
              onClick={() => selected && save.mutate({ focus: selected, apply: true })}
              disabled={save.isPending}
            >
              {save.isPending ? 'Applying…' : 'Replace them'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
