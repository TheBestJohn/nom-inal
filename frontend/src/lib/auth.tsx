import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'

import { api } from '@/api/endpoints'
import { ApiError, tokenStore, UNAUTHORIZED_EVENT } from '@/api/client'
import type { Profile } from '@/api/types'

interface AuthContextValue {
  user: Profile | null
  /** True until the stored token has been checked against the server. */
  loading: boolean
  signIn: (email: string, password: string) => Promise<void>
  signUp: (email: string, password: string, displayName: string) => Promise<void>
  signOut: () => void
  setUser: (user: Profile) => void
}

const AuthContext = createContext<AuthContextValue | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<Profile | null>(null)
  const [loading, setLoading] = useState(true)

  // A token in localStorage is not proof of a valid session — it may have
  // expired while the tab was closed — so verify it against /auth/me on boot.
  useEffect(() => {
    let cancelled = false

    if (!tokenStore.get()) {
      setLoading(false)
      return
    }

    api
      .me()
      .then((profile) => {
        if (!cancelled) setUser(profile)
      })
      .catch((err) => {
        if (cancelled) return
        // Only a server that answered gets to invalidate the token. A
        // network failure — offline, or a proxy hiccup at boot — says
        // nothing about the session, and clearing it would turn every
        // flaky connection into a sign-out.
        if (err instanceof ApiError) tokenStore.clear()
        setUser(null)
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [])

  // Any 401 from anywhere in the app drops us back to the sign-in screen.
  useEffect(() => {
    const onUnauthorized = () => setUser(null)
    window.addEventListener(UNAUTHORIZED_EVENT, onUnauthorized)
    return () => window.removeEventListener(UNAUTHORIZED_EVENT, onUnauthorized)
  }, [])

  const signIn = useCallback(async (email: string, password: string) => {
    const result = await api.login({ email, password })
    tokenStore.set(result.access_token)
    setUser(result.user)
  }, [])

  const signUp = useCallback(async (email: string, password: string, displayName: string) => {
    const result = await api.register({ email, password, display_name: displayName })
    tokenStore.set(result.access_token)
    setUser(result.user)
  }, [])

  const signOut = useCallback(() => {
    tokenStore.clear()
    setUser(null)
  }, [])

  const value = useMemo(
    () => ({ user, loading, signIn, signUp, signOut, setUser }),
    [user, loading, signIn, signUp, signOut],
  )

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth must be used inside <AuthProvider>')
  return ctx
}
