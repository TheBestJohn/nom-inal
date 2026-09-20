import { useEffect, useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'

import { api } from '@/api/endpoints'
import type { Profile } from '@/api/types'
import { useAuth } from '@/lib/auth'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardAction, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import TargetsEditor from '@/components/TargetsEditor'
import RemindersEditor from '@/components/RemindersEditor'
import DisplayEditor from '@/components/DisplayEditor'
import ApiKeysCard from '@/components/ApiKeysCard'
import AboutCard from '@/components/AboutCard'
import { ErrorNote, Spinner } from '@/components/shared'

const ACTIVITY = [
  { value: 'sedentary', label: 'Sedentary (desk job, little exercise)', factor: 1.2 },
  { value: 'light', label: 'Light (1-3 sessions a week)', factor: 1.375 },
  { value: 'moderate', label: 'Moderate (3-5 sessions a week)', factor: 1.55 },
  { value: 'active', label: 'Active (6-7 sessions a week)', factor: 1.725 },
  { value: 'very_active', label: 'Very active (physical job or 2x/day)', factor: 1.9 },
]

const GOALS = [
  { value: 'cut', label: 'Lose weight', adjust: -500 },
  { value: 'maintain', label: 'Maintain', adjust: 0 },
  { value: 'bulk', label: 'Gain weight', adjust: 300 },
]

export default function SettingsPage() {
  const { user, setUser } = useAuth()
  const [form, setForm] = useState<Partial<Profile>>({})
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
    mutationFn: () => api.updateProfile(form),
    onSuccess: (updated) => {
      setUser(updated)
      setForm(updated)
      setSaved(true)
      setTimeout(() => setSaved(false), 2500)
    },
  })

  const set = <K extends keyof Profile>(key: K, value: Profile[K]) =>
    setForm((f) => ({ ...f, [key]: value }))

  const num = (key: keyof Profile) => (e: React.ChangeEvent<HTMLInputElement>) =>
    set(key as never, (e.target.value === '' ? null : Number(e.target.value)) as never)

  /**
   * Mifflin-St Jeor BMR, scaled by activity, adjusted for the stated goal.
   * An estimate to seed the targets — every one can be overwritten.
   */
  const suggestion = (() => {
    const weight = latestWeight.data?.[0]?.weight_kg ?? form.target_weight_kg
    const height = form.height_cm
    const birth = form.birth_date
    if (!weight || !height || !birth) return null

    const age = Math.floor((Date.now() - new Date(birth).getTime()) / (365.25 * 24 * 3600 * 1000))
    const sexOffset = form.sex === 'female' ? -161 : 5
    const bmr = 10 * weight + 6.25 * height - 5 * age + sexOffset
    const factor = ACTIVITY.find((a) => a.value === form.activity_level)?.factor ?? 1.55
    const adjust = GOALS.find((g) => g.value === form.goal)?.adjust ?? 0
    const calories = Math.round(bmr * factor + adjust)

    // 2 g protein/kg on a cut (protects lean mass), 1.6 otherwise; 25% of
    // calories from fat; carbohydrate fills the remainder. Fibre scales with
    // intake at the usual ~14 g per 1000 kcal.
    const protein = Math.round(weight * (form.goal === 'cut' ? 2.0 : 1.6))
    const fat = Math.round((calories * 0.25) / 9)
    const carbs = Math.round((calories - protein * 4 - fat * 9) / 4)
    const fiber = Math.round((calories / 1000) * 14)

    return { calories, protein, fat, carbs: Math.max(carbs, 0), fiber }
  })()

  if (profile.isLoading) return <Spinner />

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>

      <Card>
        <CardHeader>
          <CardTitle>Profile</CardTitle>
          {saved && (
            <CardAction>
              <Badge variant="success">Saved</Badge>
            </CardAction>
          )}
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-3 sm:grid-cols-2">
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
          </div>

          <ErrorNote error={save.error} />

          <div className="flex flex-wrap items-center gap-3">
            <Button disabled={save.isPending} onClick={() => save.mutate()}>
              {save.isPending ? 'Saving…' : 'Save profile'}
            </Button>
            <span className="text-muted-foreground text-xs">
              Goals and budgets have their own Save below.
            </span>
          </div>
        </CardContent>
      </Card>

      <TargetsEditor suggestion={suggestion} />

      <DisplayEditor />

      <RemindersEditor />

      <ApiKeysCard />

      <AboutCard />
    </div>
  )
}
