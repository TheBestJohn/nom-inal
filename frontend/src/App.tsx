import { NavLink, Navigate, Route, Routes, useLocation } from 'react-router-dom'
import {
  BookOpen,
  CircleDot,
  LayoutList,
  LogOut,
  Search,
  Settings as SettingsIcon,
  ShieldCheck,
  TrendingUp,
} from 'lucide-react'

import { useAuth } from '@/lib/auth'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/shared'
import { ThemeToggle } from '@/components/ThemeToggle'
import SignInPage from '@/pages/SignInPage'
import DashboardPage from '@/pages/DashboardPage'
import DiaryPage from '@/pages/DiaryPage'
import FoodsPage from '@/pages/FoodsPage'
import RecipesPage from '@/pages/RecipesPage'
import RecipeEditorPage from '@/pages/RecipeEditorPage'
import WeightPage from '@/pages/WeightPage'
import WelcomePage from '@/pages/WelcomePage'
import AdminPage from '@/pages/AdminPage'
import SettingsLayout from '@/pages/settings/SettingsLayout'
import FocusSettings from '@/pages/settings/FocusSettings'
import AccountSettings from '@/pages/settings/AccountSettings'
import BodyBasicsForm from '@/components/BodyBasicsForm'
import TargetsEditor from '@/components/TargetsEditor'
import DisplayEditor from '@/components/DisplayEditor'
import RemindersEditor from '@/components/RemindersEditor'
import ApiKeysCard from '@/components/ApiKeysCard'

interface NavItem {
  to: string
  label: string
  icon: typeof CircleDot
  /** Only match this exact path — otherwise "/" stays active everywhere. */
  end?: boolean
}

const NAV: NavItem[] = [
  { to: '/', label: 'Today', icon: CircleDot, end: true },
  { to: '/diary', label: 'Diary', icon: LayoutList },
  { to: '/foods', label: 'Foods', icon: Search },
  { to: '/recipes', label: 'Recipes', icon: BookOpen },
  { to: '/weight', label: 'Weight', icon: TrendingUp },
  { to: '/settings', label: 'Settings', icon: SettingsIcon },
]

/**
 * Shown only to administrators. Hiding it is presentation, not security — the
 * page and every endpoint behind it check the flag against the database.
 */
const ADMIN_NAV: NavItem = { to: '/settings/admin', label: 'Admin', icon: ShieldCheck }

/**
 * Sends an unasked account to the welcome flow from wherever it landed.
 * A component rather than a redirect on every route, so the route table
 * stays a list of pages.
 */
function WelcomeGate() {
  const location = useLocation()
  if (location.pathname === '/welcome') return null
  return <Navigate to="/welcome" replace />
}

export default function App() {
  const { user, loading, signOut } = useAuth()

  if (loading) {
    return (
      <div className="grid min-h-dvh place-items-center">
        <Spinner label="Starting up…" />
      </div>
    )
  }

  if (!user) return <SignInPage />

  // An account that has never been asked why it is tracking is asked once,
  // wherever it was heading. Skipping records "custom", so this never loops.
  const welcome = user.tracking_focus === null

  return (
    <div className="flex min-h-dvh flex-col">
      <header className="bg-background/85 sticky top-0 z-30 border-b backdrop-blur-sm">
        <div className="mx-auto flex h-14 w-full max-w-5xl items-center justify-between gap-3 px-4">
          <div className="flex items-center gap-2 font-semibold tracking-tight">
            <span aria-hidden="true">🥗</span>
            <span>nom-inal</span>
          </div>
          <div className="flex items-center gap-1">
            <span className="text-muted-foreground mr-2 hidden text-sm sm:inline">
              {user.display_name}
            </span>
            <ThemeToggle />
            <Button variant="ghost" size="sm" onClick={signOut}>
              <LogOut />
              <span className="hidden sm:inline">Sign out</span>
            </Button>
          </div>
        </div>

        <nav
          aria-label="Main"
          className="mx-auto flex w-full max-w-5xl gap-1 overflow-x-auto px-3 pb-2"
        >
          {(user.is_admin ? [...NAV, ADMIN_NAV] : NAV).map(({ to, label, icon: Icon, end }) => (
            <NavLink
              key={to}
              to={to}
              end={end}
              className={({ isActive }) =>
                cn(
                  'flex items-center gap-1.5 rounded-full px-3 py-1.5 text-sm font-medium whitespace-nowrap transition-colors',
                  'focus-visible:ring-ring/50 outline-none focus-visible:ring-[3px]',
                  isActive
                    ? 'bg-primary text-primary-foreground'
                    : 'text-muted-foreground hover:bg-accent hover:text-accent-foreground',
                )
              }
            >
              <Icon className="size-4" />
              {label}
            </NavLink>
          ))}
        </nav>
      </header>

      <main className="mx-auto w-full max-w-5xl flex-1 px-4 py-6 pb-16">
        {welcome && <WelcomeGate />}
        <Routes>
          <Route path="/" element={<DashboardPage />} />
          <Route path="/diary" element={<DiaryPage />} />
          <Route path="/foods" element={<FoodsPage />} />
          <Route path="/recipes" element={<RecipesPage />} />
          <Route path="/recipes/new" element={<RecipeEditorPage />} />
          <Route path="/recipes/:id" element={<RecipeEditorPage />} />
          <Route path="/weight" element={<WeightPage />} />
          <Route
            path="/welcome"
            element={welcome ? <WelcomePage /> : <Navigate to="/" replace />}
          />
          <Route path="/settings" element={<SettingsLayout />}>
            <Route index element={<Navigate to="focus" replace />} />
            <Route path="focus" element={<FocusSettings />} />
            <Route path="body" element={<BodyBasicsForm />} />
            <Route path="targets" element={<TargetsEditor />} />
            <Route path="display" element={<DisplayEditor />} />
            <Route path="reminders" element={<RemindersEditor />} />
            <Route path="integrations" element={<ApiKeysCard />} />
            <Route path="account" element={<AccountSettings />} />
            <Route path="admin" element={<AdminPage />} />
          </Route>
          <Route path="/admin" element={<Navigate to="/settings/admin" replace />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
  )
}
