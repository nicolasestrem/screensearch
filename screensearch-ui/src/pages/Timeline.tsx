import { useEffect, useMemo, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import clsx from 'clsx'
import { ChevronLeft, ChevronRight, LayoutGrid, List, Search, X } from 'lucide-react'
import { Panel, PanelBody, PanelHeader } from '../components/Panel'
import { Empty, Spinner } from '../components/ui'
import { ScanlineTimeline } from '../components/ScanlineTimeline'
import { FrameRow, FrameTile } from '../components/FrameTile'
import { useFrames, useMonitors, useSearch } from '../lib/hooks'
import { fromFrame, fromSearch, type DisplayFrame } from '../lib/display'
import { useUi } from '../lib/store'
import { clock, dayBounds, hourOfDay } from '../lib/format'
import { activityFor, ACTIVITY, ACTIVITY_ORDER, type ActivityKey } from '../lib/activity'
import type { SearchMode } from '../lib/types'

const CAP = 400
const pad = (n: number) => String(n).padStart(2, '0')
const toInputDate = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`

export default function Timeline() {
  const [params, setParams] = useSearchParams()
  const viewMode = useUi((s) => s.viewMode)
  const setViewMode = useUi((s) => s.setViewMode)

  const [day, setDay] = useState(() => new Date())
  const [query, setQuery] = useState(params.get('q') ?? '')
  const [mode, setMode] = useState<SearchMode>('fts')
  const [appFilter, setAppFilter] = useState('')
  const [monitor, setMonitor] = useState<number | 'all'>('all')
  const [activity, setActivity] = useState<ActivityKey | 'all'>('all')
  const [scrub, setScrub] = useState<number | null>(null)

  // keep the URL q in sync when arriving from the palette
  useEffect(() => {
    const urlQ = params.get('q')
    if (urlQ !== null && urlQ !== query) setQuery(urlQ)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params])

  const { start, end } = useMemo(() => dayBounds(day), [day])
  const searching = query.trim().length > 0

  const framesQ = useFrames({ start_time: start, end_time: end, limit: 1500 })
  const searchQ = useSearch(
    { q: query, mode, start_time: start, end_time: end, limit: 200 },
    searching
  )
  const monitors = useMonitors().data ?? []

  const dayFrames = useMemo(() => framesQ.data?.data ?? [], [framesQ.data])

  const items = useMemo<DisplayFrame[]>(() => {
    const base: DisplayFrame[] = searching
      ? (searchQ.data ?? []).map(fromSearch)
      : [...dayFrames].sort((a, b) => a.timestamp.localeCompare(b.timestamp)).map(fromFrame)
    return base.filter((f) => {
      if (appFilter && !f.app.toLowerCase().includes(appFilter.toLowerCase())) return false
      if (monitor !== 'all' && f.monitor !== monitor) return false
      if (activity !== 'all' && activityFor(f.activity).key !== activity) return false
      if (!searching && scrub != null && Math.abs(hourOfDay(f.timestamp) - scrub) > 0.5) return false
      return true
    })
  }, [searching, searchQ.data, dayFrames, appFilter, monitor, activity, scrub])

  const shown = items.slice(0, CAP)
  const loading = searching ? searchQ.isLoading : framesQ.isLoading

  const setDayOffset = (delta: number) => {
    setScrub(null)
    setDay((d) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + delta))
  }

  const onSearchChange = (v: string) => {
    setQuery(v)
    const next = new URLSearchParams(params)
    if (v) next.set('q', v)
    else next.delete('q')
    setParams(next, { replace: true })
  }

  return (
    <div className="flex flex-col gap-5">
      {/* TIMELINE */}
      <Panel>
        <PanelHeader
          num="01"
          title="Timeline"
          right={
            <span className="flex items-center gap-3">
              <button onClick={() => setDayOffset(-1)} aria-label="Previous day" className="hover:text-ink">
                <ChevronLeft size={15} />
              </button>
              <input
                type="date"
                value={toInputDate(day)}
                onChange={(e) => {
                  const [y, m, d] = e.target.value.split('-').map(Number)
                  if (y && m && d) {
                    setScrub(null)
                    setDay(new Date(y, m - 1, d))
                  }
                }}
                className="border border-rule bg-void px-2 py-1 font-mono text-xs text-ink2 [color-scheme:dark]"
              />
              <button onClick={() => setDayOffset(1)} aria-label="Next day" className="hover:text-ink">
                <ChevronRight size={15} />
              </button>
            </span>
          }
        />
        <PanelBody>
          <div className="mb-3 flex items-center justify-between">
            <span className="font-mono text-[13px] text-muted">
              {searching ? 'Scrubbing disabled while searching' : 'Drag the playhead to scan a moment'}
            </span>
            {scrub != null && !searching && (
              <button
                onClick={() => setScrub(null)}
                className="flex items-center gap-1.5 border border-rule2 px-2.5 py-1 font-mono text-xs text-accent"
              >
                around {clock(new Date(day.getFullYear(), day.getMonth(), day.getDate(), Math.floor(scrub), Math.round((scrub % 1) * 60)))}
                <X size={12} />
              </button>
            )}
          </div>
          <ScanlineTimeline
            frames={dayFrames}
            day={day}
            interactive={!searching}
            value={searching ? null : scrub}
            onScrub={(h) => setScrub(h)}
            height={104}
          />
        </PanelBody>
      </Panel>

      {/* FILTER BAR */}
      <div className="flex flex-wrap items-center gap-2.5">
        <div className="flex min-w-[260px] flex-1 items-center gap-2.5 border border-rule2 bg-void px-3 py-2">
          <Search size={15} className="text-faint" />
          <input
            value={query}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder="Search this day's text…"
            className="w-full bg-transparent text-sm text-ink placeholder:text-faint focus:outline-none"
          />
          {query && (
            <button onClick={() => onSearchChange('')} aria-label="Clear search" className="text-faint hover:text-ink">
              <X size={14} />
            </button>
          )}
        </div>

        <div className="flex border border-rule">
          {(['fts', 'semantic', 'hybrid'] as SearchMode[]).map((m) => (
            <button
              key={m}
              onClick={() => setMode(m)}
              className={clsx(
                'eyebrow px-2.5 py-2 text-[11px]',
                mode === m ? 'bg-accent/15 text-accent' : 'text-muted hover:text-ink'
              )}
            >
              {m}
            </button>
          ))}
        </div>

        <input
          value={appFilter}
          onChange={(e) => setAppFilter(e.target.value)}
          placeholder="App…"
          className="w-28 border border-rule bg-void px-2.5 py-2 font-mono text-xs text-ink placeholder:text-faint focus:outline-none"
        />

        <select
          value={activity}
          onChange={(e) => setActivity(e.target.value as ActivityKey | 'all')}
          className="border border-rule bg-void px-2.5 py-2 font-mono text-xs text-ink2 [color-scheme:dark]"
        >
          <option value="all">All activity</option>
          {ACTIVITY_ORDER.map((k) => (
            <option key={k} value={k}>
              {ACTIVITY[k].label}
            </option>
          ))}
        </select>

        {monitors.length > 1 && (
          <select
            value={monitor}
            onChange={(e) => setMonitor(e.target.value === 'all' ? 'all' : Number(e.target.value))}
            className="border border-rule bg-void px-2.5 py-2 font-mono text-xs text-ink2 [color-scheme:dark]"
          >
            <option value="all">All monitors</option>
            {monitors.map((m) => (
              <option key={m.index} value={m.index}>
                Monitor {m.index + 1}
              </option>
            ))}
          </select>
        )}

        <div className="flex border border-rule">
          <button
            onClick={() => setViewMode('grid')}
            aria-label="Grid view"
            className={clsx('px-2.5 py-2', viewMode === 'grid' ? 'bg-accent/15 text-accent' : 'text-muted hover:text-ink')}
          >
            <LayoutGrid size={15} />
          </button>
          <button
            onClick={() => setViewMode('list')}
            aria-label="List view"
            className={clsx('px-2.5 py-2', viewMode === 'list' ? 'bg-accent/15 text-accent' : 'text-muted hover:text-ink')}
          >
            <List size={15} />
          </button>
        </div>
      </div>

      {/* CONTACT SHEET */}
      <Panel>
        <PanelHeader
          num="02"
          title={searching ? 'Search results' : 'Contact sheet'}
          right={`${items.length}${items.length > CAP ? ` · showing ${CAP}` : ''}`}
        />
        {loading ? (
          <Spinner label="Loading frames…" />
        ) : shown.length === 0 ? (
          <Empty
            title={searching ? 'No matches' : 'No frames in this range'}
            hint={
              searching
                ? 'Try a different term, switch search mode, or pick another day.'
                : 'Nothing was captured for the selected day and filters.'
            }
          />
        ) : viewMode === 'grid' ? (
          <PanelBody>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4 2xl:grid-cols-5">
              {shown.map((f) => (
                <FrameTile key={f.id} frame={f} />
              ))}
            </div>
          </PanelBody>
        ) : (
          <div className="flex flex-col">
            {shown.map((f) => (
              <FrameRow key={f.id} frame={f} />
            ))}
          </div>
        )}
      </Panel>

      {items.length > CAP && (
        <p className="font-mono text-xs text-faint">
          Showing the first {CAP} of {items.length}. Narrow the time range, search, or add a filter to see more.
        </p>
      )}
    </div>
  )
}
