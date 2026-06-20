import { useNavigate } from 'react-router-dom'
import { FrameImage } from './FrameImage'
import { ActivityBadge } from './ui'
import { appLabel, hostOf, timecode } from '../lib/format'
import type { DisplayFrame } from '../lib/display'

export function FrameTile({ frame }: { frame: DisplayFrame }) {
  const navigate = useNavigate()
  const host = hostOf(frame.url)
  return (
    <button
      onClick={() => navigate(`/timeline/${frame.id}`)}
      className="group flex w-full flex-col border border-rule bg-panel text-left transition-colors hover:border-rule2"
    >
      <div className="relative aspect-[16/10] overflow-hidden border-b border-rule bg-void">
        <FrameImage id={frame.id} alt={frame.window || frame.app} className="h-full w-full object-cover object-top opacity-95 transition-transform group-hover:scale-[1.015]" />
        <span className="absolute left-2 top-2 bg-void/85 px-1.5 py-0.5 font-mono text-[11px] text-ink2">
          {timecode(frame.timestamp)}
        </span>
        {frame.activity && (
          <span className="absolute right-2 top-2">
            <ActivityBadge type={frame.activity} />
          </span>
        )}
      </div>
      <div className="flex flex-col gap-1 p-2.5">
        <div className="flex items-center gap-2 font-mono text-xs text-muted">
          <span className="truncate text-ink2">{appLabel(frame.app)}</span>
          {host && <span className="truncate text-faint">· {host}</span>}
        </div>
        <p className="line-clamp-2 min-h-[2.4em] text-[13px] leading-snug text-ink">
          {frame.description || frame.window || '—'}
        </p>
      </div>
    </button>
  )
}

export function FrameRow({ frame }: { frame: DisplayFrame }) {
  const navigate = useNavigate()
  const host = hostOf(frame.url)
  return (
    <button
      onClick={() => navigate(`/timeline/${frame.id}`)}
      className="grid w-full grid-cols-[84px_140px_1fr_50px] items-center gap-4 border-b border-rule px-4 py-3 text-left hover:bg-white/[0.02]"
    >
      <span className="font-mono text-[13px] tabular-nums text-ink2">{timecode(frame.timestamp)}</span>
      <span className="truncate font-mono text-[13px] text-muted">
        {appLabel(frame.app)}
        {host ? <span className="text-faint"> · {host}</span> : null}
      </span>
      <span className="flex items-center gap-2.5 overflow-hidden">
        {frame.activity && <ActivityBadge type={frame.activity} />}
        <span className="truncate text-[13.5px] text-ink">{frame.description || frame.window || frame.ocr?.slice(0, 80) || '—'}</span>
      </span>
      <span className="text-right font-mono text-xs text-faint">
        {frame.confidence != null ? frame.confidence.toFixed(2) : ''}
      </span>
    </button>
  )
}
