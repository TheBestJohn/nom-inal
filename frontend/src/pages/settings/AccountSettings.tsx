import { useEffect, useState } from 'react'
import { useMutation } from '@tanstack/react-query'
import { LogOut } from 'lucide-react'

import { api } from '@/api/endpoints'
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
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ErrorNote } from '@/components/shared'
import AboutCard from '@/components/AboutCard'

/**
 * The account itself: who you are to the app, and the way out.
 *
 * Deliberately small. Body facts live under Body, keys under Integrations;
 * this is the name on the header, the email you sign in with, and sign out.
 * The "About" card describing the running build belongs on this page too,
 * below the account card — it is about the instance, not about the diet.
 */
export default function AccountSettings() {
  const { user, setUser, signOut } = useAuth()
  const [name, setName] = useState(user?.display_name ?? '')
  const [saved, setSaved] = useState(false)

  useEffect(() => {
    if (user) setName(user.display_name)
  }, [user])

  const save = useMutation({
    mutationFn: () => api.updateProfile({ display_name: name.trim() }),
    onSuccess: (profile) => {
      setUser(profile)
      setSaved(true)
      setTimeout(() => setSaved(false), 2500)
    },
  })

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Account</CardTitle>
          <CardDescription>
            The name the app greets you by, and the email you sign in with.
          </CardDescription>
          {saved && (
            <CardAction>
              <Badge variant="success">Saved</Badge>
            </CardAction>
          )}
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label htmlFor="a-name">Name</Label>
              <Input id="a-name" value={name} onChange={(e) => setName(e.target.value)} />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="a-email">Email</Label>
              <Input id="a-email" value={user?.email ?? ''} disabled />
            </div>
          </div>
          <ErrorNote error={save.error} />
          <div className="flex flex-wrap items-center gap-3">
            <Button
              disabled={save.isPending || !name.trim() || name.trim() === user?.display_name}
              onClick={() => save.mutate()}
            >
              {save.isPending ? 'Saving…' : 'Save name'}
            </Button>
            <Button variant="outline" onClick={signOut}>
              <LogOut /> Sign out
            </Button>
          </div>
        </CardContent>
      </Card>

      <AboutCard />
    </>
  )
}
