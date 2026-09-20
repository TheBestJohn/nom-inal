import { useEffect, useState } from 'react'
import { Navigate } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ShieldCheck } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { AdminUserRow, InstanceSettings } from '@/api/types'
import { useAuth } from '@/lib/auth'
import { relativeTime } from '@/lib/format'
import { cn } from '@/lib/utils'
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Empty, ErrorNote, Spinner } from '@/components/shared'

/**
 * The instance's own policy settings.
 *
 * The quorum lived in an environment variable, which put it out of reach of
 * exactly the people allowed to see it — changing it meant shell access and a
 * restart. It is a decision about how this community works, not about how the
 * container is wired, so it belongs here.
 */
function SettingsCard() {
  const queryClient = useQueryClient()
  const settings = useQuery({ queryKey: ['admin', 'settings'], queryFn: () => api.adminSettings() })
  const [draft, setDraft] = useState<string>('')

  // Seeded from the server rather than held in state from the start, so the
  // field shows the live value on first paint instead of flashing a guess.
  const current = settings.data?.food_quorum
  useEffect(() => {
    if (current !== undefined) setDraft(String(current))
  }, [current])

  const save = useMutation({
    mutationFn: () => api.updateAdminSettings({ food_quorum: Number(draft) }),
    // Every food's verified/unverified state is measured against this number,
    // so a change re-evaluates all of them server-side; drop the cached food
    // queries too or the list would keep showing the old badges.
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['admin'] })
      queryClient.invalidateQueries({ queryKey: ['foods'] })
    },
  })

  const dirty = draft !== '' && Number(draft) !== current

  return (
    <Card>
      <CardHeader>
        <CardTitle>Verification quorum</CardTitle>
        <CardDescription>
          How many net confirmations a food&rsquo;s current numbers need before it counts as
          verified. Disputes are subtracted, and nobody can confirm their own edit — so 1 is the
          right answer on a single-user instance, where a second opinion is never coming.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {settings.isLoading && <Spinner />}
        <ErrorNote error={settings.error} />
        <ErrorNote error={save.error} />

        <form
          className="flex flex-wrap items-end gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            save.mutate()
          }}
        >
          <div className="space-y-1.5">
            <Label htmlFor="quorum">Confirmations needed</Label>
            <Input
              id="quorum"
              type="number"
              min={1}
              max={50}
              className="w-28"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
            />
          </div>
          <Button type="submit" disabled={!dirty || save.isPending}>
            {save.isPending ? 'Saving…' : 'Save'}
          </Button>
          <p className="text-muted-foreground text-xs">
            Applies to every food at once: lowering it promotes entries that already had the
            support, raising it demotes the ones that no longer clear the bar. Nobody&rsquo;s votes
            are lost either way.
          </p>
        </form>

        <p className="text-muted-foreground text-xs">
          {settings.data &&
            seededNote(settings.data, settings.data.food_quorum_updated_at, 'FOOD_QUORUM')}
        </p>
      </CardContent>
    </Card>
  )
}

/**
 * Whether the environment still steers a seeded setting, or who took it over.
 *
 * The attribution is the row's last editor, so it is only claimed for the
 * setting saved by that edit: both stamps come from the same `now()` in one
 * statement, which is what makes the equality safe.
 */
function seededNote(settings: InstanceSettings, savedAt: string | null, variable: string) {
  if (!savedAt) {
    return `Still at its installation default, so ${variable} in the environment can still set it at startup. Saving here takes it over for good.`
  }
  const by =
    savedAt === settings.updated_at
      ? (settings.updated_by_name ?? 'a former administrator')
      : 'an administrator'
  return `Saved by ${by} ${relativeTime(savedAt)}; ${variable} in the environment no longer applies.`
}

/**
 * Whether new accounts may be created.
 *
 * The usual life of a self-hosted instance: sign up, invite the household,
 * then close the door. That used to mean an edit to .env and a restart, which
 * is exactly the wrong amount of ceremony for a toggle. The server reads it
 * on every registration, so flipping it here takes effect at once, and an
 * empty instance always admits its first account so this cannot lock a fresh
 * install out of itself.
 */
function RegistrationCard() {
  const queryClient = useQueryClient()
  const settings = useQuery({ queryKey: ['admin', 'settings'], queryFn: () => api.adminSettings() })

  const save = useMutation({
    mutationFn: (open: boolean) => api.updateAdminSettings({ allow_registration: open }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['admin', 'settings'] })
      queryClient.invalidateQueries({ queryKey: ['registration'] })
    },
  })

  const open = settings.data?.allow_registration ?? true

  return (
    <Card>
      <CardHeader>
        <CardTitle>Sign-ups</CardTitle>
        <CardDescription>
          Whether anyone who reaches this instance can create an account. Existing accounts keep
          signing in either way, and administrators can still restore a suspended one.
        </CardDescription>
        <CardAction>
          <Label className="text-muted-foreground text-sm font-normal">
            <Switch
              checked={open}
              disabled={settings.isLoading || save.isPending}
              onCheckedChange={(v) => save.mutate(v)}
              aria-label="Allow sign-ups"
            />
            {open ? 'Open' : 'Closed'}
          </Label>
        </CardAction>
      </CardHeader>
      <CardContent className="space-y-3">
        <ErrorNote error={settings.error} />
        <ErrorNote error={save.error} />
        <p className="text-muted-foreground text-xs">
          {settings.data &&
            seededNote(
              settings.data,
              settings.data.allow_registration_updated_at,
              'ALLOW_REGISTRATION',
            )}
        </p>
      </CardContent>
    </Card>
  )
}

/** One number and its caption. The stats grid is a dozen of these. */
function Stat({ label, value, hint }: { label: string; value: number; hint?: string }) {
  return (
    <div className="rounded-lg border px-3 py-2">
      <div className="tabular text-xl font-semibold">{value.toLocaleString()}</div>
      <div className="text-muted-foreground text-xs">{label}</div>
      {hint && <div className="text-muted-foreground text-[11px]">{hint}</div>}
    </div>
  )
}

/**
 * Instance administration.
 *
 * Guarded twice on purpose. The route hides itself when `is_admin` is false,
 * which is a convenience; every endpoint behind it re-checks the flag against
 * the database on each request, which is the actual control. A client-side
 * check alone would be decoration.
 */
export default function AdminPage() {
  const { user } = useAuth()
  const queryClient = useQueryClient()
  const [term, setTerm] = useState('')
  const [debounced, setDebounced] = useState('')
  const [includeDisabled, setIncludeDisabled] = useState(false)

  useEffect(() => {
    const id = setTimeout(() => setDebounced(term.trim()), 250)
    return () => clearTimeout(id)
  }, [term])

  const stats = useQuery({ queryKey: ['admin', 'stats'], queryFn: () => api.adminStats() })
  const users = useQuery({
    queryKey: ['admin', 'users', debounced, includeDisabled],
    queryFn: () => api.adminUsers({ q: debounced || undefined, include_disabled: includeDisabled }),
  })

  const patch = useMutation({
    mutationFn: ({ id, body }: { id: string; body: { is_admin?: boolean; disabled?: boolean } }) =>
      api.adminPatchUser(id, body),
    onSuccess: (_row, { body }) => {
      // Suspending someone otherwise makes them vanish from the default list,
      // which reads as "deleted" rather than "suspended" and gives you nothing
      // to undo it with. Reveal the suspended rows instead.
      if (body.disabled) setIncludeDisabled(true)
      queryClient.invalidateQueries({ queryKey: ['admin'] })
    },
  })

  if (user && !user.is_admin) return <Navigate to="/" replace />

  const unchecked = stats.data ? stats.data.foods - stats.data.foods_verified : 0

  return (
    <div className="space-y-4">
      <h1 className="flex items-center gap-2 text-2xl font-semibold tracking-tight">
        <ShieldCheck className="size-5" /> Administration
      </h1>

      <Card>
        <CardHeader>
          <CardTitle>This instance</CardTitle>
          <CardDescription>
            The food database is the part worth watching: it is open to everyone, so the useful
            number is how much of it anyone has actually checked.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {stats.isLoading && <Spinner />}
          <ErrorNote error={stats.error} />
          {stats.data && (
            <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
              <Stat label="Accounts" value={stats.data.users} hint={`${stats.data.admins} admin`} />
              <Stat label="Suspended" value={stats.data.disabled_users} />
              <Stat label="Active API keys" value={stats.data.active_api_keys} />
              <Stat
                label="Foods"
                value={stats.data.foods}
                hint={`${stats.data.food_variants} variants`}
              />
              <Stat
                label="Verified foods"
                value={stats.data.foods_verified}
                hint={`quorum of ${stats.data.food_quorum}`}
              />
              <Stat label="Unverified" value={unchecked} />
              <Stat label="Disputed" value={stats.data.foods_disputed} />
              <Stat label="Food revisions" value={stats.data.food_revisions} />
              <Stat
                label="Recipes"
                value={stats.data.recipes}
                hint={`${stats.data.public_recipes} shared`}
              />
              <Stat label="Diary entries" value={stats.data.diary_entries} />
              <Stat label="Weigh-ins" value={stats.data.weigh_ins} />
              <Stat label="Photos" value={stats.data.photos} />
            </div>
          )}
        </CardContent>
      </Card>

      <RegistrationCard />

      <SettingsCard />

      <Card>
        <CardHeader>
          <CardTitle>Accounts</CardTitle>
          <CardDescription>
            Suspending keeps everything the account has recorded and simply stops it signing in —
            including its API keys. Deleting would take the diary, weights and photos with it.
          </CardDescription>
          <CardAction>
            <Label className="text-muted-foreground text-sm font-normal">
              <Switch checked={includeDisabled} onCheckedChange={setIncludeDisabled} />
              Show suspended
            </Label>
          </CardAction>
        </CardHeader>
        <CardContent className="space-y-3">
          <Input
            placeholder="Filter by name or email…"
            value={term}
            onChange={(e) => setTerm(e.target.value)}
          />
          {users.isLoading && <Spinner />}
          <ErrorNote error={users.error} />
          <ErrorNote error={patch.error} />
          {users.data?.length === 0 && <Empty>Nobody matches.</Empty>}

          {users.data && users.data.length > 0 && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Account</TableHead>
                  <TableHead className="text-right">Logged</TableHead>
                  <TableHead className="text-right">Foods</TableHead>
                  <TableHead className="text-right">Keys</TableHead>
                  <TableHead>Last seen</TableHead>
                  <TableHead />
                </TableRow>
              </TableHeader>
              <TableBody>
                {users.data.map((row: AdminUserRow) => {
                  const self = row.id === user?.id
                  return (
                    <TableRow key={row.id} className={cn(row.disabled_at && 'opacity-55')}>
                      <TableCell className="max-w-[16rem] whitespace-normal">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="font-medium">{row.display_name}</span>
                          {row.is_admin && (
                            <Badge variant="secondary" className="text-[10px]">
                              Admin
                            </Badge>
                          )}
                          {row.disabled_at && (
                            <Badge variant="destructive" className="text-[10px]">
                              Suspended
                            </Badge>
                          )}
                          {self && (
                            <Badge variant="outline" className="text-[10px]">
                              You
                            </Badge>
                          )}
                        </div>
                        <span className="text-muted-foreground text-xs">{row.email}</span>
                      </TableCell>
                      <TableCell className="tabular text-right">
                        {row.diary_entries + row.weigh_ins}
                      </TableCell>
                      <TableCell className="tabular text-right">
                        {row.foods_created}
                        <span className="text-muted-foreground text-xs"> / {row.food_edits}</span>
                      </TableCell>
                      <TableCell className="tabular text-right">{row.active_api_keys}</TableCell>
                      <TableCell className="text-muted-foreground text-xs">
                        {row.last_activity_at ? relativeTime(row.last_activity_at) : 'never'}
                      </TableCell>
                      <TableCell className="text-right">
                        {/* No actions on yourself: the server refuses a
                            self-demotion or self-suspension outright, since
                            that is the one mistake here nothing inside the
                            application can undo. */}
                        {!self && (
                          <div className="flex justify-end gap-1">
                            <Button
                              variant="ghost"
                              size="sm"
                              disabled={patch.isPending}
                              onClick={() =>
                                patch.mutate({ id: row.id, body: { is_admin: !row.is_admin } })
                              }
                            >
                              {row.is_admin ? 'Demote' : 'Make admin'}
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              disabled={patch.isPending}
                              onClick={() =>
                                patch.mutate({ id: row.id, body: { disabled: !row.disabled_at } })
                              }
                            >
                              {row.disabled_at ? 'Restore' : 'Suspend'}
                            </Button>
                          </div>
                        )}
                      </TableCell>
                    </TableRow>
                  )
                })}
              </TableBody>
            </Table>
          )}
          <p className="text-muted-foreground text-xs">Foods column reads created / edited.</p>
        </CardContent>
      </Card>
    </div>
  )
}
