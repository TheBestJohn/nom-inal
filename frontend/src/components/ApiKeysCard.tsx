import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, Copy, KeyRound, Plus } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { ApiKey, CreatedApiKey } from '@/api/types'
import { relativeTime } from '@/lib/format'
import { cn } from '@/lib/utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
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
import { Empty, ErrorNote, Spinner } from '@/components/shared'

/**
 * Personal API keys, for anything that is not this browser.
 *
 * The one piece of real interaction design here is that a freshly minted token
 * is shown once and then gone forever — the server only keeps a digest. So the
 * new key gets its own alert with a copy button and stays on screen until it is
 * dismissed, rather than appearing as another row in a table that the next
 * render would quietly replace with a prefix.
 */
export default function ApiKeysCard() {
  const queryClient = useQueryClient()
  const [name, setName] = useState('')
  const [canWrite, setCanWrite] = useState(false)
  const [expiryDays, setExpiryDays] = useState('')
  const [fresh, setFresh] = useState<CreatedApiKey | null>(null)
  const [copied, setCopied] = useState(false)

  const keys = useQuery({ queryKey: ['keys'], queryFn: () => api.listApiKeys() })

  const create = useMutation({
    mutationFn: () =>
      api.createApiKey({
        name: name.trim(),
        scopes: canWrite ? ['read', 'write'] : ['read'],
        expires_in_days: expiryDays ? Number(expiryDays) : undefined,
      }),
    onSuccess: (key) => {
      setFresh(key)
      setName('')
      setCanWrite(false)
      setExpiryDays('')
      queryClient.invalidateQueries({ queryKey: ['keys'] })
    },
  })

  const revoke = useMutation({
    mutationFn: (id: string) => api.revokeApiKey(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['keys'] }),
  })

  const copy = async () => {
    if (!fresh) return
    try {
      await navigator.clipboard.writeText(fresh.token)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      // Clipboard access is denied outside a secure context; the token is
      // selectable on screen either way, so this is not worth an error state.
    }
  }

  const active = (key: ApiKey) =>
    !key.revoked_at && (!key.expires_at || Date.parse(key.expires_at) > Date.now())

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <KeyRound className="size-4" /> API keys
        </CardTitle>
        <CardDescription>
          For scripts, dashboards and anything else that is not this browser. A key acts as you, and
          only for the data you can already see.
        </CardDescription>
        <CardAction>
          <Badge variant="outline">{keys.data?.filter(active).length ?? 0} active</Badge>
        </CardAction>
      </CardHeader>

      <CardContent className="space-y-4">
        {fresh && (
          <Alert>
            <AlertDescription className="space-y-2">
              <p className="font-medium">
                Copy <strong>{fresh.name}</strong> now — this is the only time it is shown.
              </p>
              <div className="flex gap-2">
                <Input readOnly value={fresh.token} className="font-mono text-xs" />
                <Button type="button" variant="outline" size="sm" onClick={copy}>
                  {copied ? <Check /> : <Copy />}
                  {copied ? 'Copied' : 'Copy'}
                </Button>
                <Button type="button" variant="ghost" size="sm" onClick={() => setFresh(null)}>
                  Done
                </Button>
              </div>
              <p className="text-muted-foreground text-xs">
                Send it as <code>Authorization: Bearer …</code> or <code>X-API-Key: …</code>.
              </p>
            </AlertDescription>
          </Alert>
        )}

        <form
          className="grid gap-3 sm:grid-cols-[1fr_auto_auto_auto] sm:items-end"
          onSubmit={(e) => {
            e.preventDefault()
            create.mutate()
          }}
        >
          <div className="space-y-1.5">
            <Label htmlFor="key-name">Name</Label>
            <Input
              id="key-name"
              placeholder="e.g. kitchen dashboard"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="key-expiry">Expires in (days)</Label>
            <Input
              id="key-expiry"
              type="number"
              min={1}
              max={3650}
              placeholder="never"
              className="sm:w-32"
              value={expiryDays}
              onChange={(e) => setExpiryDays(e.target.value)}
            />
          </div>
          <Label className="text-muted-foreground pb-2 text-sm font-normal">
            <Switch checked={canWrite} onCheckedChange={setCanWrite} />
            Can make changes
          </Label>
          <Button type="submit" disabled={create.isPending || !name.trim()}>
            <Plus /> Create
          </Button>
        </form>
        <p className="text-muted-foreground text-xs">
          A key without &ldquo;can make changes&rdquo; is refused anything but reads. No key can
          create other keys or administer the instance — signing in is required for that, so a
          leaked key cannot replace itself.
        </p>

        <ErrorNote error={create.error} />
        <ErrorNote error={revoke.error} />

        {keys.isLoading && <Spinner />}
        {keys.data?.length === 0 && <Empty>No keys yet.</Empty>}

        <ul className="space-y-1.5">
          {keys.data?.map((key) => (
            <li
              key={key.id}
              className={cn(
                'flex flex-wrap items-center gap-2 rounded-md border px-3 py-2 text-sm',
                !active(key) && 'opacity-55',
              )}
            >
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">{key.name}</span>
                <span className="text-muted-foreground block font-mono text-xs">{key.prefix}…</span>
              </span>
              {key.scopes.includes('write') ? (
                <Badge variant="secondary" className="text-[10px]">
                  Read &amp; write
                </Badge>
              ) : (
                <Badge variant="outline" className="text-[10px]">
                  Read only
                </Badge>
              )}
              <span className="text-muted-foreground text-xs">
                {key.revoked_at
                  ? `revoked ${relativeTime(key.revoked_at)}`
                  : key.expires_at && Date.parse(key.expires_at) <= Date.now()
                    ? `expired ${relativeTime(key.expires_at)}`
                    : key.last_used_at
                      ? `last used ${relativeTime(key.last_used_at)}`
                      : 'never used'}
              </span>
              {active(key) && (
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={revoke.isPending}
                  onClick={() => revoke.mutate(key.id)}
                >
                  Revoke
                </Button>
              )}
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  )
}
