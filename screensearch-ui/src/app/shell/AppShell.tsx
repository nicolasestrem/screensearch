import { useEffect } from 'react'
import { Outlet, useLocation } from 'react-router-dom'
import { StatusRail } from './StatusRail'
import { NavRail } from './NavRail'
import { CommandPalette } from './CommandPalette'
import { ReadinessBanner } from './ReadinessBanner'
import { ErrorBoundary } from '../../components/ErrorBoundary'
import { ToastHost } from '../../components/Toast'
import { useUi } from '../../lib/store'

export function AppShell() {
  const setPaletteOpen = useUi((s) => s.setPaletteOpen)
  const paletteOpen = useUi((s) => s.paletteOpen)
  const location = useLocation()

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setPaletteOpen(!useUi.getState().paletteOpen)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [setPaletteOpen])

  return (
    <div className="grid h-screen grid-cols-[224px_1fr] grid-rows-[54px_1fr]">
      <StatusRail />
      <NavRail />
      <main className="overflow-y-auto px-7 pb-12 pt-6">
        <div className="mx-auto max-w-[1400px]">
          <ReadinessBanner />
          <ErrorBoundary key={location.pathname}>
            <Outlet />
          </ErrorBoundary>
        </div>
      </main>
      {paletteOpen && <CommandPalette />}
      <ToastHost />
    </div>
  )
}
