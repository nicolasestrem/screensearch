import { useMemo, useRef } from 'react'
import clsx from 'clsx'
import { activityFor, ACTIVITY, ACTIVITY_ORDER } from '../lib/activity'
import { hourOfDay } from '../lib/format'

export interface TimelineFrame {
  timestamp: string
  activity_type?: string | null
}

const BUCKETS = 96 // 15-minute resolution across 24h

function sameDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate()
}

/**
 * The Scanline Timeline — a 24h track showing a frame-density ribbon, vision
 * activity-type colour bands, a live "now" line, and (when interactive) a
 * draggable playhead that reports the scrubbed hour.
 */
export function ScanlineTimeline({
  frames,
  day,
  value,
  onScrub,
  interactive = false,
  showLegend = true,
  height = 108,
  className,
}: {
  frames: TimelineFrame[]
  day: Date
  value?: number | null
  onScrub?: (hour: number | null) => void
  interactive?: boolean
  showLegend?: boolean
  height?: number
  className?: string
}) {
  const trackRef = useRef<HTMLDivElement>(null)

  const { counts, colors, max } = useMemo(() => {
    const c = new Array<number>(BUCKETS).fill(0)
    const tally: Array<Record<string, number>> = Array.from({ length: BUCKETS }, () => ({}))
    for (const f of frames) {
      const h = hourOfDay(f.timestamp)
      const idx = Math.min(BUCKETS - 1, Math.max(0, Math.floor((h / 24) * BUCKETS)))
      c[idx] = (c[idx] ?? 0) + 1
      const color = activityFor(f.activity_type).color
      const bucket = tally[idx]!
      bucket[color] = (bucket[color] ?? 0) + 1
    }
    const col = tally.map((b) => {
      let best: string | null = null
      let bestN = 0
      for (const [color, n] of Object.entries(b)) {
        if (n > bestN) {
          bestN = n
          best = color
        }
      }
      return best
    })
    return { counts: c, colors: col, max: Math.max(1, ...c) }
  }, [frames])

  const isToday = sameDay(day, new Date())
  const nowHour = isToday ? hourOfDay(new Date().toISOString()) : null

  function hourFromEvent(clientX: number): number {
    const el = trackRef.current
    if (!el) return 0
    const rect = el.getBoundingClientRect()
    const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width))
    return ratio * 24
  }

  function handlePointer(e: React.PointerEvent) {
    if (!interactive || !onScrub) return
    e.preventDefault()
    ;(e.target as HTMLElement).setPointerCapture?.(e.pointerId)
    onScrub(hourFromEvent(e.clientX))
  }
  function handleMove(e: React.PointerEvent) {
    if (!interactive || !onScrub) return
    if (e.buttons !== 1) return
    onScrub(hourFromEvent(e.clientX))
  }

  const ribbonTop = 18

  return (
    <div className={className}>
      <div
        ref={trackRef}
        onPointerDown={handlePointer}
        onPointerMove={handleMove}
        className={clsx(
          'relative border border-rule bg-void',
          interactive && 'cursor-ew-resize touch-none'
        )}
        style={{ height }}
        role={interactive ? 'slider' : undefined}
        aria-label={interactive ? 'Scrub the day' : undefined}
        aria-valuemin={interactive ? 0 : undefined}
        aria-valuemax={interactive ? 24 : undefined}
        aria-valuenow={interactive && value != null ? Math.round(value) : undefined}
      >
        {/* hour gridlines (every 3h) */}
        <div className="pointer-events-none absolute inset-0 flex">
          {Array.from({ length: 8 }, (_, i) => (
            <span key={i} className="flex-1 border-r border-[#26262E] last:border-r-0" />
          ))}
        </div>

        {/* activity bands */}
        <div
          className="pointer-events-none absolute left-0 right-0 top-0 border-b border-rule"
          style={{ height: 14 }}
        >
          {colors.map((color, i) =>
            color ? (
              <div
                key={i}
                className="absolute top-0"
                style={{
                  left: `${(i / BUCKETS) * 100}%`,
                  width: `${(1 / BUCKETS) * 100 + 0.4}%`,
                  height: 14,
                  background: color,
                }}
              />
            ) : null
          )}
        </div>

        {/* density ribbon */}
        <div
          className="pointer-events-none absolute left-0 right-0 flex items-end gap-px px-px"
          style={{ top: ribbonTop, bottom: 0 }}
        >
          {counts.map((n, i) => (
            <div
              key={i}
              className="flex-1 bg-accent"
              style={{
                height: n > 0 ? `${Math.max(4, (n / max) * 100)}%` : '0%',
                opacity: n / max > 0.66 ? 0.7 : 0.34,
              }}
            />
          ))}
        </div>

        {/* now line */}
        {nowHour != null && (
          <div
            className="pointer-events-none absolute top-0 bottom-0 w-px bg-faint"
            style={{ left: `${(nowHour / 24) * 100}%` }}
          />
        )}

        {/* selection playhead */}
        {value != null && (
          <div
            className="pointer-events-none absolute top-0 bottom-0 w-0.5 bg-accent"
            style={{ left: `${(value / 24) * 100}%` }}
          >
            <span
              className="absolute -left-[4px] top-0 h-2 w-2.5 bg-accent"
              style={{ clipPath: 'polygon(0 0,100% 0,50% 100%)' }}
            />
          </div>
        )}
      </div>

      {/* hour ticks */}
      <div className="mt-2 flex font-mono text-[11.5px] text-faint">
        {['00', '03', '06', '09', '12', '15', '18', '21', '24'].map((t, i) => (
          <span key={t} className={i === 8 ? 'flex-none' : 'flex-1'}>
            {t}
          </span>
        ))}
      </div>

      {showLegend && (
        <div className="mt-3 flex flex-wrap gap-x-5 gap-y-1.5 font-display text-[13px] tracking-wide text-muted">
          {ACTIVITY_ORDER.filter((k) => k !== 'other').map((k) => (
            <span key={k} className="inline-flex items-center">
              <i className="mr-1.5 inline-block h-2.5 w-2.5 align-[-1px]" style={{ background: ACTIVITY[k].color }} />
              {ACTIVITY[k].label}
            </span>
          ))}
        </div>
      )}
    </div>
  )
}
