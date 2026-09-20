import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { api } from '@/api/endpoints'
import type { Profile } from '@/api/types'
import { useAuth } from '@/lib/auth'
import { today } from '@/lib/format'
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { ErrorNote, Spinner } from '@/components/shared'

/**
 * The labels only. The multipliers used to live beside them; they now live on
 * the server, where the estimate is made, so the two cannot drift.
 */
export const ACTIVITY = [
  { value: 'sedentary', label: 'Sedentary (desk job, little exercise)' },
  { value: 'light', label: 'Light (1-3 sessions a week)' },
  { value: 'moderate', label: 'Moderate (3-5 sessions a week)' },
  { value: 'active', label: 'Active (6-7 sessions a week)' },
  { value: 'very_active', label: 'Very active (physical job or 2x/day)' },
]

export const GOALS = [
  { value: 'cut', label: 'Lose weight' },
  { value: 'maintain', label: 'Maintain' },
  { value: 'bulk', label: 'Gain weight' },
]

/**
 * The body facts the energy estimate is made from, and the goal it is
 * adjusted for.
 *
 * One form for two places: the welcome flow asks for these once, before it
 * can price a preset, and the Body settings page lets them be corrected
 * later. `compact` is the welcome variant — it drops the name and adds a
 * current-weight field, because an account that has never weighed in has
 * nothing for the estimate to read.
 */
export default function BodyBasicsForm({
  compact = false,
  onSaved,
  submitLabel = 'Save',
}: {
  compact?: boolean
  onSaved?: (profile: Profile) => void
  submitLabel?: string
}) {
  const { user, setUser } = useAuth()
  const queryClient = useQueryClient()
  const [form, setForm] = useState<Partial<Profile>>({})
  const [weight, setWeight] = useState('')
  const [saved, setSaved] = useState(false)

  const profile = useQuery({ queryKey: ['profile'], queryFn: () => api.getProfile() })
  const latestWeight = useQuery({
    queryKey: ['weights', 'latest'],
    queryFn: () => api.listWeights({ limit: 1 }),
  })

  useEffect(() => {
    if (profile.data) setForm(profile.data)
  }, [profile.data])

  const save = useMutation({
    mutationFn: async () => {
      // A weight typed here is a weigh-in, not a profile field: the estimate
      // reads the scale, and this is the scale.
      const kg = Number(weight)
      if (weight.trim() && Number.isFinite(kg) && kg > 0) {
        await api.logWeight({ recorded_on: today(), weight_kg: kg })
      }
      return api.updateProfile({
        display_name: form.display_name,
        sex: form.sex,
        birth_date: form.birth_date,
        height_cm: form.height_cm,
        activity_level: form.activity_level,
        goal: form.goal,
        target_weight_kg: form.target_weight_kg,
      })
    },
    onSuccess: (updated) => {
      setUser(updated)
      setForm(updated)
      // The suggestion and every focus preview are priced from these fields.
      queryClient.invalidateQueries({ queryKey: ['targets', 'suggestion'] })
      queryClient.invalidateQueries({ queryKey: ['focus'] })
      queryClient.invalidateQueries({ queryKey: ['weights'] })
      setSaved(true)
      setTimeout(() => setSaved(false), 2500)
      onSaved?.(updated)
    },
  })

  const set = <K extends keyof Profile>(key: K, value: Profile[K]) =>
    setForm((f) => ({ ...f, [key]: value }))

  const num = (key: keyof Profile) => (e: React.ChangeEvent<HTMLInputElement>) =>
    set(key as never, (e.target.value === '' ? null : Number(e.target.value)) as never)

  if (profile.isLoading) return <Spinner />

  const lastKg = latestWeight.data?.[0]?.weight_kg

  return (
    <Card>
      <CardHeader>
        <CardTitle>{compact ? 'Body basics' : 'Body & goal'}</CardTitle>
        <CardDescription>
          {compact
            ? 'What the energy estimate is made from. Nothing here is shown to anyone else.'
            : 'The estimate behind every suggested target starts from these.'}
        </CardDescription>
        {saved && (
          <CardAction>
            <Badge variant="success">Saved</Badge>
          </CardAction>
        )}
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid gap-3 sm:grid-cols-2">
          {!compact && (
            <>
              <div className="space-y-1.5">
                <Label htmlFor="s-name">Name</Label>
                <Input
                  id="s-name"
                  value={form.display_name ?? ''}
                  onChange={(e) => set('display_name', e.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="s-email">Email</Label>
                <Input id="s-email" value={user?.email ?? ''} disabled />
              </div>
            </>
          )}
          <div className="space-y-1.5">
            <Label htmlFor="s-dob">Date of birth</Label>
            <Input
              id="s-dob"
              type="date"
              value={form.birth_date ?? ''}
              onChange={(e) => set('birth_date', e.target.value || null)}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="s-sex">Sex</Label>
            <Select
              value={form.sex ?? 'unspecified'}
              onValueChange={(v) => set('sex', v === 'unspecified' ? null : v)}
            >
              <SelectTrigger id="s-sex" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="unspecified">Prefer not to say</SelectItem>
                <SelectItem value="female">Female</SelectItem>
                <SelectItem value="male">Male</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="s-height">Height (cm)</Label>
            <Input
              id="s-height"
              type="number"
              step="any"
              value={form.height_cm ?? ''}
              onChange={num('height_cm')}
            />
          </div>
          {compact ? (
            <div className="space-y-1.5">
              <Label htmlFor="s-weight">Current weight (kg)</Label>
              <Input
                id="s-weight"
                type="number"
                step="any"
                placeholder={lastKg ? `last weigh-in ${lastKg}` : ''}
                value={weight}
                onChange={(e) => setWeight(e.target.value)}
              />
            </div>
          ) : (
            <div className="space-y-1.5">
              <Label htmlFor="s-target">Target weight (kg)</Label>
              <Input
                id="s-target"
                type="number"
                step="any"
                value={form.target_weight_kg ?? ''}
                onChange={num('target_weight_kg')}
              />
            </div>
          )}
          <div className="space-y-1.5 sm:col-span-2">
            <Label htmlFor="s-activity">Activity level</Label>
            <Select
              value={form.activity_level ?? 'moderate'}
              onValueChange={(v) => set('activity_level', v)}
            >
              <SelectTrigger id="s-activity" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ACTIVITY.map((a) => (
                  <SelectItem key={a.value} value={a.value}>
                    {a.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          {!compact && (
            <div className="space-y-1.5 sm:col-span-2">
              <Label htmlFor="s-goal">Goal</Label>
              <Select value={form.goal ?? 'maintain'} onValueChange={(v) => set('goal', v)}>
                <SelectTrigger id="s-goal" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {GOALS.map((g) => (
                    <SelectItem key={g.value} value={g.value}>
                      {g.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}
        </div>

        <ErrorNote error={save.error} />

        <div className="flex flex-wrap items-center gap-3">
          <Button disabled={save.isPending} onClick={() => save.mutate()}>
            {save.isPending ? 'Saving…' : submitLabel}
          </Button>
          {!compact && (
            <span className="text-muted-foreground text-xs">
              Weigh-ins live on the Weight page; the estimate reads the latest one.
            </span>
          )}
        </div>
      </CardContent>
    </Card>
  )
}
