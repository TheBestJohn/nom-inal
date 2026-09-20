import { useMutation } from '@tanstack/react-query'

import { api } from '@/api/endpoints'
import type { Units } from '@/api/types'
import { useAuth } from '@/lib/auth'

/**
 * The signed-in account's unit preference, and a way to change it.
 *
 * Read from the profile rather than from a page's own toggle so a choice made
 * on the weight page holds on the home page and in Settings too: which units
 * you think in is a fact about you, not about the screen you happen to be on.
 * Storage stays metric throughout; see `lib/format.ts` for the conversions.
 */
export function useUnits(): {
  units: Units
  setUnits: (units: Units) => void
  saving: boolean
} {
  const { user, setUser } = useAuth()
  const save = useMutation({
    mutationFn: (units: Units) => api.updateProfile({ units }),
    onSuccess: (profile) => setUser(profile),
  })
  return {
    units: user?.units ?? 'metric',
    setUnits: (units) => save.mutate(units),
    saving: save.isPending,
  }
}
