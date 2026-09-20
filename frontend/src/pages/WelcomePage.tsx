import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, ArrowRight } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { TrackingFocus } from '@/api/types'
import { useAuth } from '@/lib/auth'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { ErrorNote } from '@/components/shared'
import BodyBasicsForm from '@/components/BodyBasicsForm'
import FocusPicker from '@/components/FocusPicker'
import FocusPreviewCard from '@/components/FocusPreviewCard'

type Step = 'focus' | 'body' | 'preview'

/**
 * The one question asked at first sign-in: why are you tracking?
 *
 * Three steps at most — pick a focus, fill in the body basics the estimate
 * needs if they are missing, see exactly what applying would set — and every
 * one of them can be skipped. Skipping records the answer as "custom", so the
 * question is never asked again; there is no way to leave it unanswered, and
 * so no way to be dropped back here every sign-in.
 */
export default function WelcomePage() {
  const { user, setUser } = useAuth()
  const queryClient = useQueryClient()
  const navigate = useNavigate()
  const [focus, setFocus] = useState<TrackingFocus | null>(null)
  const [step, setStep] = useState<Step>('focus')

  const bodyKnown = Boolean(user?.birth_date && user?.height_cm)

  const finish = useMutation({
    mutationFn: (body: { focus: TrackingFocus; apply: boolean }) => api.setFocus(body),
    onSuccess: (result) => {
      setUser(result.profile)
      queryClient.invalidateQueries({ queryKey: ['targets'] })
      queryClient.invalidateQueries({ queryKey: ['diary'] })
      navigate('/', { replace: true })
    },
  })

  const skip = () => finish.mutate({ focus: 'custom', apply: false })

  const next = () => {
    if (!focus) return
    if (focus === 'custom') {
      finish.mutate({ focus, apply: false })
      return
    }
    // The body step only exists while the estimate has nothing to work from;
    // once height and birth date are on file it is skipped, not shown empty.
    setStep(bodyKnown ? 'preview' : 'body')
  }

  return (
    <div className="mx-auto max-w-2xl space-y-4">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">
          Welcome{user ? `, ${user.display_name.split(' ')[0]}` : ''}
        </h1>
        <p className="text-muted-foreground text-sm">
          One question to set things up. It fills in which nutrients you see and a first set of
          targets; all of it stays editable, and you can change your mind in Settings.
        </p>
      </div>

      {step === 'focus' && (
        <Card>
          <CardHeader>
            <CardTitle>Why are you tracking?</CardTitle>
            <CardDescription>
              Pick the closest fit. This sets up the screen, not a diet.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <FocusPicker value={focus} onChange={setFocus} disabled={finish.isPending} />
            <ErrorNote error={finish.error} />
            <div className="flex flex-wrap items-center justify-between gap-3">
              <Button variant="ghost" onClick={skip} disabled={finish.isPending}>
                Skip for now
              </Button>
              <Button onClick={next} disabled={!focus || finish.isPending}>
                {focus === 'custom' ? 'Finish' : 'Continue'} <ArrowRight />
              </Button>
            </div>
          </CardContent>
        </Card>
      )}

      {step === 'body' && focus && (
        <>
          <BodyBasicsForm
            compact
            submitLabel="Save and continue"
            onSaved={() => setStep('preview')}
          />
          <div className="flex flex-wrap items-center justify-between gap-3">
            <Button variant="ghost" onClick={() => setStep('focus')}>
              <ArrowLeft /> Back
            </Button>
            <Button variant="outline" onClick={() => setStep('preview')}>
              Continue without these
            </Button>
          </div>
        </>
      )}

      {step === 'preview' && focus && (
        <Card>
          <CardHeader>
            <CardTitle>What this sets</CardTitle>
            <CardDescription>
              Applied, not enforced: these become ordinary targets and display settings you can
              change any time.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <FocusPreviewCard focus={focus} />
            <ErrorNote error={finish.error} />
            <div className="flex flex-wrap items-center justify-between gap-3">
              <Button
                variant="ghost"
                onClick={() => setStep(bodyKnown ? 'focus' : 'body')}
                disabled={finish.isPending}
              >
                <ArrowLeft /> Back
              </Button>
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  onClick={() => finish.mutate({ focus, apply: false })}
                  disabled={finish.isPending}
                >
                  Keep the focus, skip the targets
                </Button>
                <Button
                  onClick={() => finish.mutate({ focus, apply: true })}
                  disabled={finish.isPending}
                >
                  {finish.isPending ? 'Applying…' : 'Apply'}
                </Button>
              </div>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  )
}
