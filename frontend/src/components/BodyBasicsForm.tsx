import { useEffect, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { api } from '@/api/endpoints'
import type { Profile, Units } from '@/api/types'
import { useAuth } from '@/lib/auth'
import { cmToFtIn, ftInToCm, round, today, weightToKg, weightUnit, weightValue } from '@/lib/format'
import { useUnits } from '@/lib/useUnits'
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
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { ErrorNote, Spinner } from '@/components/shared'

/**
 * A weight in the preferred unit, converted to kilograms on the way in.
 *
 * Module scope, since a component declared inside a render body is a new
 * type on every render and the input would lose focus on each keystroke.
 * The field holds text so a half-typed number is not rounded under you.
 */
function WeightField({
  id,
  label,
  kg,
  units,
  placeholder,
  onChange,
}: {
  id: string
  label: string
  kg: number | null | undefined
  units: Units
  placeholder?: string
  onChange: (kg: number | null) => void
}) {
  const [text, setText] = useState(kg == null ? '' : String(weightValue(kg, units)))
  const emitted = useRef<number | null | undefined>(undefined)
  // Re-derive the text when the unit changes, or when the value changes from
  // outside — the profile arriving, say — but never from the field's own
  // keystrokes: rewriting "18." as "18" under someone's cursor is how a
  // controlled number input eats the decimal point.
  const [shown, setShown] = useState({ kg, units })
  if (shown.units !== units || (shown.kg !== kg && kg !== emitted.current)) {
    setShown({ kg, units })
    setText(kg == null ? '' : String(weightValue(kg, units)))
  } else if (shown.kg !== kg) {
    setShown({ kg, units })
  }
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>
        {label} ({weightUnit(units)})
      </Label>
      <Input
        id={id}
        type="number"
        step="any"
        placeholder={placeholder}
        value={text}
        onChange={(e) => {
          setText(e.target.value)
          const value = Number(e.target.value)
          const next =
            e.target.value === '' || !Number.isFinite(value) ? null : weightToKg(value, units)
          emitted.current = next
          onChange(next)
        }}
      />
    </div>
  )
}

/**
 * Height as centimetres, or as feet and inches that become centimetres.
 * Storage is metric; the two boxes are how a height is said aloud.
 */
function HeightField({
  cm,
  units,
  onChange,
}: {
  cm: number | null | undefined
  units: Units
  onChange: (cm: number | null) => void
}) {
  if (units !== 'imperial') {
    return (
      <div className="space-y-1.5">
        <Label htmlFor="s-height">Height (cm)</Label>
        <Input
          id="s-height"
          type="number"
          step="any"
          value={cm ?? ''}
          onChange={(e) => onChange(e.target.value === '' ? null : Number(e.target.value))}
        />
      </div>
    )
  }
  const { feet, inches } = cm == null ? { feet: NaN, inches: NaN } : cmToFtIn(cm)
  const set = (f: number, i: number) =>
    onChange(Number.isFinite(f) || Number.isFinite(i) ? round(ftInToCm(f || 0, i || 0), 1) : null)
  return (
    <div className="space-y-1.5">
      <Label htmlFor="s-height-ft">Height (ft, in)</Label>
      <div className="grid grid-cols-2 gap-2">
        <Input
          id="s-height-ft"
          type="number"
          min={0}
          step={1}
          placeholder="ft"
          aria-label="Height, feet"
          value={Number.isFinite(feet) ? feet : ''}
          onChange={(e) => set(Number(e.target.value), inches)}
        />
        <Input
          id="s-height-in"
          type="number"
          min={0}
          max={11}
          step={1}
          placeholder="in"
          aria-label="Height, inches"
          value={Number.isFinite(inches) ? inches : ''}
          onChange={(e) => set(feet, Number(e.target.value))}
        />
      </div>
    </div>
  )
}

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
  const { units, setUnits, saving: savingUnits } = useUnits()
  const queryClient = useQueryClient()
  const [form, setForm] = useState<Partial<Profile>>({})
  // Kilograms, whatever unit the box shows: converted at the field.
  const [weightKg, setWeightKg] = useState<number | null>(null)
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
      if (weightKg !== null && weightKg > 0) {
        await api.logWeight({ recorded_on: today(), weight_kg: round(weightKg, 2) })
      }
      return api.updateProfile({
        display_name: form.display_name,
        sex: form.sex,
        birth_date: form.birth_date,
        height_cm: form.height_cm,
        activity_level: form.activity_level,
        goal: form.goal,
        // Converted from whatever unit was typed, so trimmed to what a
        // scale could show rather than stored to sixteen places.
        target_weight_kg:
          form.target_weight_kg == null ? form.target_weight_kg : round(form.target_weight_kg, 2),
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
        {/* Units are a display preference and saved as soon as they are
            chosen, separately from the figures: storage stays metric, so
            switching converts what is shown and never what is kept. Food
            amounts are not affected — they are grams, and "a cup" is a
            portion on the food rather than a unit. */}
        <div className="space-y-1.5">
          <Label>Units</Label>
          <ToggleGroup
            type="single"
            value={units}
            onValueChange={(v) => v && setUnits(v as Units)}
            disabled={savingUnits}
            aria-label="Units for body measurements"
          >
            <ToggleGroupItem value="metric">kg, cm</ToggleGroupItem>
            <ToggleGroupItem value="imperial">lb, ft/in</ToggleGroupItem>
          </ToggleGroup>
          <p className="text-muted-foreground text-xs">
            How weight and height are shown and typed, everywhere. Food is always in grams.
          </p>
        </div>

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
          <HeightField cm={form.height_cm} units={units} onChange={(cm) => set('height_cm', cm)} />
          {compact ? (
            <WeightField
              id="s-weight"
              label="Current weight"
              kg={weightKg}
              units={units}
              placeholder={lastKg ? `last weigh-in ${weightValue(lastKg, units, 1)}` : ''}
              onChange={setWeightKg}
            />
          ) : (
            <WeightField
              id="s-target"
              label="Target weight"
              kg={form.target_weight_kg}
              units={units}
              onChange={(kg) => set('target_weight_kg', kg)}
            />
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
