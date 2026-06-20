import { NavLink } from 'react-router-dom'
import clsx from 'clsx'
import { LayoutGrid, Search, Film, BarChart3, Settings as SettingsIcon } from 'lucide-react'
import { useHealth, useServerStatus } from '../../lib/hooks'
import { duration } from '../../lib/format'

const NAV = [
  { to: '/', label: 'Deck', icon: LayoutGrid, end: true },
  { to: '/recall', label: 'Recall', icon: Search, end: false },
  { to: '/timeline', label: 'Timeline', icon: Film, end: false },
  { to: '/insights', label: 'Insights', icon: BarChart3, end: false },
]

export function NavRail() {
  const health = useHealth().data
  const server = useServerStatus().data

  return (
    <nav className="flex flex-col gap-1 border-r border-rule bg-panel2 py-4">
      {NAV.map(({ to, label, icon: Icon, end }) => (
        <NavLink
          key={to}
          to={to}
          end={end}
          className={({ isActive }) =>
            clsx(
              'flex items-center gap-3 border-l-[3px] px-5 py-2.5 font-display text-[15px] tracking-[0.02em] transition-colors',
              isActive
                ? 'border-accent bg-accent/10 text-ink'
                : 'border-transparent text-muted hover:bg-white/[0.025] hover:text-ink'
            )
          }
        >
          {({ isActive }) => (
            <>
              <Icon size={16} className={isActive ? 'text-accent' : ''} aria-hidden />
              {label}
            </>
          )}
        </NavLink>
      ))}

      <div className="eyebrow px-5 pb-2 pt-4 text-[10.5px] text-faint">System</div>
      <NavLink
        to="/settings"
        className={({ isActive }) =>
          clsx(
            'flex items-center gap-3 border-l-[3px] px-5 py-2.5 font-display text-[15px] tracking-[0.02em] transition-colors',
            isActive
              ? 'border-accent bg-accent/10 text-ink'
              : 'border-transparent text-muted hover:bg-white/[0.025] hover:text-ink'
          )
        }
      >
        {({ isActive }) => (
          <>
            <SettingsIcon size={16} className={isActive ? 'text-accent' : ''} aria-hidden />
            Settings
          </>
        )}
      </NavLink>

      <div className="mt-auto px-5 py-4 font-mono text-[11.5px] leading-[1.9] text-muted">
        {health && (
          <div>
            v{health.version} · <span className="text-ink2">local-only</span>
          </div>
        )}
        {server && (
          <div>
            llama-server{' '}
            <span className="text-ink2">
              {server.status === 'running' ? `idle ${duration(server.idle_seconds)}` : server.status}
            </span>
          </div>
        )}
      </div>
    </nav>
  )
}
