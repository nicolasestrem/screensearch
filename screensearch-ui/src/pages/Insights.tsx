import { useMemo, useState } from 'react'
import clsx from 'clsx'
import { Panel, PanelBody, PanelHeader } from '../components/Panel'
import { Empty, Spinner } from '../components/ui'
import { useFrames, useSettings } from '../lib/hooks'
import { appLabel, dayBounds, duration, hostOf } from '../lib/format'
import { activityFor, ACTIVITY, ACTIVITY_ORDER } from '../lib/activity'

type Range = 'today' | '7d' | '30d'

function rangeBounds(r: Range): { start: string; end: string } {
  const now = new Date()
  if (r === 'today') return dayBounds(now)
  const days = r === '7d' ? 7 : 30
  return { start: new Date(now.getTime() - days * 86400000).toISOString(), end: now.toISOString() }
}

function StatCell({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <Panel className="flex-1">
      <PanelBody className="flex flex-col gap-1">
        <span className="eyebrow text-[10.5px] text-faint">{label}</span>
        <span className="font-display text-[32px] font-semibold leading-none text-ink">{value}</span>
        {sub && <span className="font-mono text-xs text-muted">{sub}</span>}
      </PanelBody>
    </Panel>
  )
}

export default function Insights() {
  const [range, setRange] = useState<Range>('7d')
  const { start, end } = useMemo(() => rangeBounds(range), [range])
  const framesQ = useFrames({ start_time: start, end_time: end, limit: 5000 })
  const frames = useMemo(() => framesQ.data?.data ?? [], [framesQ.data])
  const total = framesQ.data?.pagination.total ?? 0
  const interval = useSettings().data?.capture_interval ?? 0
  const time = (count: number) => (interval > 0 ? duration(count * interval) : `${count}`)

  const activityMix = useMemo(() => {
    const counts = new Map<string, number>()
    for (const f of frames) {
      const k = activityFor(f.activity_type).key
      counts.set(k, (counts.get(k) ?? 0) + 1)
    }
    const rows = ACTIVITY_ORDER.map((k) => ({ k, n: counts.get(k) ?? 0 })).filter((r) => r.n > 0)
    const max = rows.reduce((m, r) => Math.max(m, r.n), 1)
    return { rows, max }
  }, [frames])

  const apps = useMemo(() => {
    const counts = new Map<string, number>()
    for (const f of frames) {
      const host = hostOf(f.browser_url)
      const key = host ? `${appLabel(f.app_name)} · ${host}` : appLabel(f.app_name)
      counts.set(key, (counts.get(key) ?? 0) + 1)
    }
    const rows = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8)
    const max = rows.length > 0 ? rows[0]![1] : 1
    return { rows, max }
  }, [frames])

  const hourly = useMemo(() => {
    const counts = new Array<number>(24).fill(0)
    for (const f of frames) {
      const h = new Date(f.timestamp).getHours()
      counts[h] = (counts[h] ?? 0) + 1
    }
    const max = counts.reduce((m, n) => Math.max(m, n), 1)
    let peak = 0
    counts.forEach((n, h) => {
      if (n > (counts[peak] ?? 0)) peak = h
    })
    return { counts, max, peak }
  }, [frames])

  const topApp = apps.rows[0]?.[0]
  const topActivity = activityMix.rows.slice().sort((a, b) => b.n - a.n)[0]

  return (
    <div className="flex flex-col gap-5">
      <div className="flex items-center justify-between">
        <h1 className="font-display text-xl font-semibold tracking-wide text-ink">Insights</h1>
        <div className="flex border border-rule">
          {(['today', '7d', '30d'] as Range[]).map((r) => (
            <button
              key={r}
              onClick={() => setRange(r)}
              className={clsx(
                'eyebrow px-3.5 py-2 text-[11px]',
                range === r ? 'bg-accent/15 text-accent' : 'text-muted hover:text-ink'
              )}
            >
              {r === 'today' ? 'Today' : r}
            </button>
          ))}
        </div>
      </div>

      {framesQ.isLoading ? (
        <Spinner label="Crunching your history…" />
      ) : frames.length === 0 ? (
        <Panel>
          <Empty
            title="Not enough captured yet"
            hint="Once ScreenSearch has recorded some activity in this period, your apps, sites, and rhythm will show up here."
          />
        </Panel>
      ) : (
        <>
          <div className="flex flex-wrap gap-5">
            <StatCell label="Active time" value={time(total)} sub={`${total.toLocaleString()} frames`} />
            <StatCell
              label="Top activity"
              value={topActivity ? ACTIVITY[topActivity.k].label : '—'}
              sub={topActivity ? time(topActivity.n) : undefined}
            />
            <StatCell label="Top app / site" value={topApp ?? '—'} sub={topApp ? time(apps.rows[0]![1]) : undefined} />
            <StatCell label="Peak hour" value={`${String(hourly.peak).padStart(2, '0')}:00`} sub="busiest stretch" />
          </div>

          <div className="grid gap-5 lg:grid-cols-2">
            {/* Activity mix */}
            <Panel>
              <PanelHeader num="01" title="Activity mix" />
              <PanelBody className="flex flex-col gap-3.5">
                {activityMix.rows.map(({ k, n }) => (
                  <div key={k} className="flex flex-col gap-1.5">
                    <div className="flex items-baseline justify-between text-sm">
                      <span className="text-ink">{ACTIVITY[k].label}</span>
                      <span className="font-mono text-[13px] text-ink2">{time(n)}</span>
                    </div>
                    <div className="h-2 border border-rule bg-void">
                      <div className="h-full" style={{ width: `${(n / activityMix.max) * 100}%`, background: ACTIVITY[k].color }} />
                    </div>
                  </div>
                ))}
              </PanelBody>
            </Panel>

            {/* Apps & sites */}
            <Panel>
              <PanelHeader num="02" title="Apps & sites" />
              <PanelBody className="flex flex-col gap-3.5">
                {apps.rows.map(([key, n]) => (
                  <div key={key} className="flex flex-col gap-1.5">
                    <div className="flex items-baseline justify-between gap-3 text-sm">
                      <span className="truncate text-ink">{key}</span>
                      <span className="font-mono text-[13px] text-ink2">{time(n)}</span>
                    </div>
                    <div className="h-2 border border-rule bg-void">
                      <div className="h-full bg-accent2" style={{ width: `${(n / apps.max) * 100}%` }} />
                    </div>
                  </div>
                ))}
              </PanelBody>
            </Panel>
          </div>

          {/* Hourly rhythm */}
          <Panel>
            <PanelHeader num="03" title="Daily rhythm" right="by hour" />
            <PanelBody>
              <div className="flex h-40 items-end gap-1">
                {hourly.counts.map((n, h) => (
                  <div key={h} className="group relative flex-1">
                    <div
                      className={clsx('w-full', h === hourly.peak ? 'bg-accent' : 'bg-accent/40')}
                      style={{ height: `${n > 0 ? Math.max(3, (n / hourly.max) * 100) : 0}%` }}
                      title={`${String(h).padStart(2, '0')}:00 — ${time(n)}`}
                    />
                  </div>
                ))}
              </div>
              <div className="mt-2 flex font-mono text-[11px] text-faint">
                {Array.from({ length: 24 }, (_, h) => (
                  <span key={h} className="flex-1 text-center">
                    {h % 3 === 0 ? String(h).padStart(2, '0') : ''}
                  </span>
                ))}
              </div>
            </PanelBody>
          </Panel>

          {total > frames.length && (
            <p className="font-mono text-xs text-faint">
              Based on the most recent {frames.length.toLocaleString()} of {total.toLocaleString()} frames in this range.
            </p>
          )}
        </>
      )}
    </div>
  )
}
