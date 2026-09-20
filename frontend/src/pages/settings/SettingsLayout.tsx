import { NavLink, Outlet } from 'react-router-dom'
import {
  Bell,
  Compass,
  Eye,
  KeyRound,
  PersonStanding,
  ShieldCheck,
  Target,
  UserRound,
} from 'lucide-react'

import { useAuth } from '@/lib/auth'
import { cn } from '@/lib/utils'

interface Section {
  to: string
  label: string
  icon: typeof Compass
  adminOnly?: boolean
}

/**
 * Settings, one route per section, so each can be linked to: "set a budget"
 * goes to /settings/targets, not to a page where the right card is somewhere
 * below the fold. Ordered around the focus, since everything after it is
 * derived from or edits what the focus set.
 */
const SECTIONS: Section[] = [
  { to: 'focus', label: 'Focus & goal', icon: Compass },
  { to: 'body', label: 'Body & units', icon: PersonStanding },
  { to: 'targets', label: 'Targets', icon: Target },
  { to: 'display', label: 'Display', icon: Eye },
  { to: 'reminders', label: 'Reminders', icon: Bell },
  { to: 'integrations', label: 'Integrations', icon: KeyRound },
  { to: 'account', label: 'Account', icon: UserRound },
  { to: 'admin', label: 'Admin', icon: ShieldCheck, adminOnly: true },
]

export default function SettingsLayout() {
  const { user } = useAuth()
  const sections = SECTIONS.filter((s) => !s.adminOnly || user?.is_admin)

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>

      {/* A side nav on desktop, a scrollable strip on a phone. Same links,
          same order; only the axis changes. */}
      <div className="grid gap-4 md:grid-cols-[12rem_1fr] md:items-start">
        <nav
          aria-label="Settings sections"
          className="-mx-4 flex gap-1 overflow-x-auto px-4 pb-1 md:sticky md:top-24 md:mx-0 md:flex-col md:overflow-visible md:px-0"
        >
          {sections.map(({ to, label, icon: Icon }) => (
            <NavLink
              key={to}
              to={to}
              className={({ isActive }) =>
                cn(
                  'flex items-center gap-2 rounded-md px-3 py-1.5 text-sm font-medium whitespace-nowrap transition-colors',
                  'focus-visible:ring-ring/50 outline-none focus-visible:ring-[3px]',
                  isActive
                    ? 'bg-accent text-accent-foreground'
                    : 'text-muted-foreground hover:bg-accent/60 hover:text-accent-foreground',
                )
              }
            >
              <Icon className="size-4" />
              {label}
            </NavLink>
          ))}
        </nav>

        <div className="min-w-0 space-y-4">
          <Outlet />
        </div>
      </div>
    </div>
  )
}
