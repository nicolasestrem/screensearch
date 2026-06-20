import type { ReactNode } from 'react'
import { useEmbeddingStatus, useHealth, useServerStatus, useSettings, useVisionStatus } from '../../lib/hooks'
import { pct } from '../../lib/format'
import { StatusDot } from '../../components/ui'

function Stat({
  label,
  children,
  className,
  grow,
}: {
  label: string
  children: ReactNode
  className?: string
  grow?: boolean
}) {
  return (
    <div
      className={
        'flex h-full flex-col justify-center gap-0.5 border-r border-rule px-5 ' +
        (grow ? 'flex-1 border-r-0 ' : '') +
        (className ?? '')
      }
    >
      <span className="eyebrow text-[10.5px] text-faint">{label}</span>
      <span className="flex items-center gap-2 font-mono text-sm text-ink tabular-nums">{children}</span>
    </div>
  )
}

function daysBetween(a?: string | null, b?: string | null): number | null {
  if (!a || !b) return null
  const d = (new Date(b).getTime() - new Date(a).getTime()) / 86400000
  return Math.max(1, Math.round(d))
}

export function StatusRail() {
  const health = useHealth().data
  const settings = useSettings().data
  const server = useServerStatus().data
  const emb = useEmbeddingStatus().data
  const vision = useVisionStatus().data

  const paused = settings?.is_paused === 1
  const interval = settings?.capture_interval
  const span = daysBetween(health?.oldest_frame, health?.newest_frame)

  const accel = server?.acceleration
  const accelView =
    accel === 'gpu'
      ? { label: 'GPU · Vulkan', tone: 'ok' as const }
      : accel === 'cpu'
        ? { label: 'CPU', tone: 'warn' as const }
        : { label: 'idle', tone: 'muted' as const }

  const visionPct =
    vision && vision.total_frames > 0 ? (vision.completed / vision.total_frames) * 100 : null

  return (
    <div className="col-span-2 flex items-stretch border-b border-rule bg-panel2">
      <div className="flex w-[224px] flex-none items-center gap-3 border-r border-rule px-[18px]">
        <span className="relative h-4 w-4 flex-none bg-accent">
          <span className="absolute inset-[3px] bg-void" />
        </span>
        <span className="font-display text-base font-semibold uppercase tracking-[0.13em] text-ink">
          ScreenSearch
        </span>
      </div>
      <div className="flex flex-1 items-stretch overflow-x-auto">
        <Stat label="Capture">
          {paused ? (
            <>
              <span className="text-warn">❚❚ Paused</span>
            </>
          ) : (
            <>
              <StatusDot tone="accent" pulse />
              <span className="text-accent">REC</span>
              {interval != null && <span className="text-faint">· {interval}s</span>}
            </>
          )}
        </Stat>
        <Stat label="Frames">{health ? health.frame_count.toLocaleString() : '—'}</Stat>
        <Stat label="Span">{span != null ? `${span} days` : '—'}</Stat>
        <Stat label="Acceleration">
          <StatusDot tone={accelView.tone} />
          {accelView.label}
        </Stat>
        <Stat label="" grow>
          {''}
        </Stat>
        <Stat label="Index" className="border-l border-rule">
          <span className="text-ink">{emb ? pct(emb.coverage_percent) : '—'}</span>
        </Stat>
        <Stat label="Vision">
          <span className={visionPct != null && visionPct < 80 ? 'text-warn' : 'text-ink'}>
            {visionPct != null ? pct(visionPct) : '—'}
          </span>
        </Stat>
      </div>
    </div>
  )
}
