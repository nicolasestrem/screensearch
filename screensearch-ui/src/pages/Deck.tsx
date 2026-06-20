import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { ArrowRight, Sparkles } from 'lucide-react'
import { Panel, PanelBody, PanelHeader } from '../components/Panel'
import { CoverageBar, Empty, Metric, Spinner } from '../components/ui'
import { ScanlineTimeline } from '../components/ScanlineTimeline'
import { FrameRow } from '../components/FrameTile'
import { useEmbeddingStatus, useFrames, useSettings, useVisionStatus } from '../lib/hooks'
import { fromFrame } from '../lib/display'
import { useUi } from '../lib/store'
import { appLabel, dayBounds, duration, hostOf } from '../lib/format'

export default function Deck() {
  const navigate = useNavigate()
  const setRecallSeed = useUi((s) => s.setRecallSeed)
  const [ask, setAsk] = useState('')

  const today = useMemo(() => new Date(), [])
  const { start, end } = useMemo(() => dayBounds(today), [today])
  const framesQ = useFrames({ start_time: start, end_time: end, limit: 2000 })
  const frames = useMemo(() => framesQ.data?.data ?? [], [framesQ.data])

  const emb = useEmbeddingStatus().data
  const vision = useVisionStatus().data
  const interval = useSettings().data?.capture_interval ?? 0

  const sortedDesc = useMemo(
    () => [...frames].sort((a, b) => b.timestamp.localeCompare(a.timestamp)),
    [frames]
  )
  const feed = sortedDesc.slice(0, 7)

  const appsSites = useMemo(() => {
    const counts = new Map<string, number>()
    for (const f of frames) {
      const host = hostOf(f.browser_url)
      const key = host ? `${appLabel(f.app_name)} · ${host}` : appLabel(f.app_name)
      counts.set(key, (counts.get(key) ?? 0) + 1)
    }
    const rows = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5)
    const max = rows.length > 0 ? rows[0]![1] : 1
    return { rows, max }
  }, [frames])

  const submitAsk = () => {
    const q = ask.trim()
    if (!q) return
    setRecallSeed(q)
    navigate('/recall')
  }

  const visionPct = vision && vision.total_frames > 0 ? (vision.completed / vision.total_frames) * 100 : 0

  return (
    <div className="flex flex-col gap-5">
      <div className="grid gap-5 lg:grid-cols-[1.4fr_1fr]">
        {/* RECALL */}
        <Panel>
          <PanelHeader num="01" title="Recall" right="⌘K" />
          <PanelBody>
            <div className="flex items-center gap-3 border border-rule2 bg-void px-4 py-3.5">
              <span className="font-mono text-base text-accent">&gt;</span>
              <input
                value={ask}
                onChange={(e) => setAsk(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && submitAsk()}
                placeholder="Ask your screen — what did I work on this morning?"
                className="w-full bg-transparent font-sans text-[15px] text-ink placeholder:text-faint focus:outline-none"
              />
              <button
                onClick={submitAsk}
                disabled={!ask.trim()}
                className="text-faint transition-colors hover:text-accent disabled:opacity-40"
                aria-label="Ask"
              >
                <ArrowRight size={18} />
              </button>
            </div>
            <div className="mt-3.5 flex flex-wrap gap-2">
              {['what did I work on this morning', 'where did I read about embeddings', 'find that Figma frame'].map(
                (s) => (
                  <button
                    key={s}
                    onClick={() => {
                      setRecallSeed(s)
                      navigate('/recall')
                    }}
                    className="flex items-center gap-1.5 border border-rule px-3 py-1.5 text-[13px] text-muted hover:border-rule2 hover:text-ink"
                  >
                    <Sparkles size={12} className="text-accent" />
                    {s}
                  </button>
                )
              )}
            </div>
          </PanelBody>
        </Panel>

        {/* INDEX */}
        <Panel>
          <PanelHeader num="02" title="Index" />
          <PanelBody className="flex flex-col gap-5">
            <div>
              <div className="flex items-baseline justify-between">
                <span className="font-display text-[15px] tracking-wide text-ink">Semantic embeddings</span>
                {emb?.enabled === false ? (
                  <span className="eyebrow text-xs text-warn">Off</span>
                ) : (
                  <Metric value={emb ? Math.round(emb.coverage_percent) : '—'} unit="%" />
                )}
              </div>
              {emb?.enabled === false ? (
                <button
                  onClick={() => navigate('/settings')}
                  className="mt-2 border border-rule2 px-3 py-1.5 font-mono text-xs text-accent hover:border-accent"
                >
                  Enable semantic search →
                </button>
              ) : (
                <>
                  <div className="mt-2.5">
                    <CoverageBar value={emb?.coverage_percent ?? 0} />
                  </div>
                  <div className="mt-1.5 flex justify-between font-mono text-xs text-muted">
                    <span>
                      {emb ? `${emb.frames_with_embeddings.toLocaleString()} / ${emb.total_frames.toLocaleString()} frames` : '—'}
                    </span>
                    <span>{emb?.model ?? ''}</span>
                  </div>
                </>
              )}
            </div>

            <div>
              <div className="flex items-baseline justify-between">
                <span className="font-display text-[15px] tracking-wide text-ink">Vision analysis</span>
                <Metric value={Math.round(visionPct)} unit="%" tone="dim" />
              </div>
              <div className="mt-2.5">
                <CoverageBar value={visionPct} tone="research" />
              </div>
              <div className="mt-1.5 flex justify-between font-mono text-xs text-muted">
                <span>
                  {vision ? `${vision.completed.toLocaleString()} done · ${vision.queue_depth} queued` : '—'}
                </span>
                <span>{vision?.model ?? ''}</span>
              </div>
            </div>
          </PanelBody>
        </Panel>
      </div>

      {/* TIMELINE */}
      <Panel>
        <PanelHeader num="03" title="Timeline" right="Today" />
        <PanelBody>
          {framesQ.isLoading ? (
            <Spinner label="Loading today…" />
          ) : (
            <button onClick={() => navigate('/timeline')} className="block w-full text-left">
              <ScanlineTimeline frames={frames} day={today} height={104} />
            </button>
          )}
        </PanelBody>
      </Panel>

      <div className="grid gap-5 lg:grid-cols-[1.55fr_1fr]">
        {/* ACTIVITY FEED */}
        <Panel>
          <PanelHeader num="04" title="Activity feed" right="latest" />
          {framesQ.isLoading ? (
            <Spinner />
          ) : feed.length === 0 ? (
            <Empty title="Nothing captured today yet" hint="As you work, recent moments appear here with their detected activity." />
          ) : (
            <div className="flex flex-col">
              {feed.map((f) => (
                <FrameRow key={f.id} frame={fromFrame(f)} />
              ))}
            </div>
          )}
        </Panel>

        {/* APPS & SITES */}
        <Panel>
          <PanelHeader num="05" title="Apps & sites" right="today" />
          <PanelBody>
            {appsSites.rows.length === 0 ? (
              <Empty title="No activity yet" />
            ) : (
              <div className="flex flex-col gap-4">
                {appsSites.rows.map(([key, count]) => (
                  <div key={key} className="flex flex-col gap-1.5">
                    <div className="flex items-baseline justify-between gap-3">
                      <span className="truncate text-sm text-ink">{key}</span>
                      <span className="font-mono text-[13px] text-ink2">
                        {interval > 0 ? duration(count * interval) : `${count}`}
                      </span>
                    </div>
                    <div className="h-1.5 border border-rule bg-void">
                      <div className="h-full bg-accent2" style={{ width: `${(count / appsSites.max) * 100}%` }} />
                    </div>
                  </div>
                ))}
              </div>
            )}
          </PanelBody>
        </Panel>
      </div>

      {framesQ.isError && (
        <p className="font-mono text-xs text-alert">Could not load today’s frames — is the service running?</p>
      )}
    </div>
  )
}
