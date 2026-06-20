import { useReadiness } from '../../lib/hooks'
import { duration } from '../../lib/format'

const TRANSITIONAL = new Set(['initializing', 'loading', 'downloading'])

/**
 * Replaces the old mock StartupStatus / DownloadProgress modals with a real
 * surface driven by GET /api/system/readiness. Stays out of the way once
 * everything is ready.
 */
export function ReadinessBanner() {
  const { data } = useReadiness(true)
  if (!data || data.all_ready) return null

  const active = data.stages.filter((s) => TRANSITIONAL.has(s.state))
  if (active.length === 0) return null

  return (
    <div className="mb-5 border border-rule bg-panel">
      <div className="flex items-center gap-3 border-b border-rule px-4 py-2.5">
        <span className="inline-block h-3 w-3 animate-spin border border-rule2 border-t-accent" />
        <span className="eyebrow text-xs text-ink2">Warming up</span>
      </div>
      <div className="flex flex-col">
        {active.map((s) => (
          <div key={s.id} className="border-b border-rule px-4 py-2.5 last:border-b-0">
            <div className="flex items-baseline justify-between gap-4">
              <span className="font-display text-sm text-ink">{s.label}</span>
              <span className="font-mono text-xs text-muted">
                {s.progress != null ? `${Math.round(s.progress)}%` : s.state}
                {s.eta_seconds != null ? ` · ${duration(s.eta_seconds)} left` : ''}
              </span>
            </div>
            <p className="mt-1 font-mono text-[11.5px] leading-relaxed text-faint">{s.detail}</p>
            {s.progress != null && (
              <div className="mt-2 h-1.5 border border-rule bg-void">
                <div className="h-full bg-accent" style={{ width: `${Math.max(0, Math.min(100, s.progress))}%` }} />
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  )
}
