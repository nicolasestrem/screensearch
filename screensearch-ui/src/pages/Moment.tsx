import { useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import { ArrowLeft, ExternalLink, ScanEye, X } from 'lucide-react'
import { Panel, PanelBody, PanelHeader } from '../components/Panel'
import { ActivityBadge, Button, ErrorNote, Spinner, StatusDot } from '../components/ui'
import { FrameImage } from '../components/FrameImage'
import { useAnalyzeFrame, useFrame, useFrameTagMutations, useTags } from '../lib/hooks'
import { appLabel, dateLabel, hostOf, timecode } from '../lib/format'

function MetaRow({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-4 border-b border-rule py-2 last:border-b-0">
      <span className="eyebrow text-[10.5px] text-faint">{label}</span>
      <span className="min-w-0 truncate text-right font-mono text-[13px] text-ink2">{value}</span>
    </div>
  )
}

function statusTone(s: string | null | undefined): 'ok' | 'warn' | 'alert' | 'muted' {
  if (s === 'completed') return 'ok'
  if (s === 'failed') return 'alert'
  if (s === 'processing' || s === 'pending') return 'warn'
  return 'muted'
}

export default function Moment() {
  const { id: idParam } = useParams()
  const id = Number(idParam)
  const valid = Number.isFinite(id)
  const { data: frame, isLoading, isError } = useFrame(valid ? id : null)
  const allTags = useTags().data ?? []
  const { addTag, removeTag } = useFrameTagMutations()
  const analyze = useAnalyzeFrame()
  const [addId, setAddId] = useState('')

  if (!valid) return <ErrorNote message="Invalid frame id." />
  if (isLoading) return <Spinner label="Loading moment…" />
  if (isError || !frame) return <ErrorNote message="That moment could not be loaded." />

  const host = hostOf(frame.browser_url)
  const analyzed = frame.analysis_status === 'completed'
  const available = allTags.filter((t) => !frame.tags.some((ft) => ft.id === t.id))

  return (
    <div className="flex flex-col gap-5">
      <div className="flex items-center justify-between">
        <Link to="/timeline" className="flex items-center gap-2 font-mono text-sm text-muted hover:text-ink">
          <ArrowLeft size={15} /> Back to timeline
        </Link>
        <span className="font-display text-xl font-semibold tabular-nums text-accent">
          {timecode(frame.timestamp)}
        </span>
      </div>

      <div className="grid gap-5 lg:grid-cols-[1.6fr_1fr]">
        {/* Screenshot */}
        <Panel>
          <PanelHeader num="01" title="Capture" right={dateLabel(frame.timestamp)} />
          <div className="bg-void p-2">
            <FrameImage id={frame.id} alt={frame.window_name || frame.app_name} className="h-auto w-full object-contain" />
          </div>
        </Panel>

        <div className="flex flex-col gap-5">
          {/* Vision */}
          <Panel>
            <PanelHeader
              num="02"
              title="Vision"
              right={
                <span className="flex items-center gap-2">
                  <StatusDot tone={statusTone(frame.analysis_status)} />
                  {frame.analysis_status ?? 'not analyzed'}
                </span>
              }
            />
            <PanelBody className="flex flex-col gap-3">
              {frame.activity_type && (
                <div>
                  <ActivityBadge type={frame.activity_type} />
                </div>
              )}
              <p className="text-[14px] leading-relaxed text-ink">
                {frame.description || (
                  <span className="text-muted">No description yet — this frame hasn't been analyzed.</span>
                )}
              </p>
              <div className="flex flex-wrap gap-x-6 gap-y-1 font-mono text-xs text-muted">
                {frame.app_hint && <span>hint: <span className="text-ink2">{frame.app_hint}</span></span>}
                {frame.confidence != null && (
                  <span>confidence: <span className="text-ink2">{frame.confidence.toFixed(2)}</span></span>
                )}
              </div>
              {!analyzed && (
                <Button
                  variant="ghost"
                  onClick={() => analyze.mutate(frame.id)}
                  disabled={analyze.isPending || frame.analysis_status === 'pending' || frame.analysis_status === 'processing'}
                  className="self-start"
                >
                  <ScanEye size={14} />
                  {analyze.isSuccess ? 'Queued' : 'Analyze now'}
                </Button>
              )}
            </PanelBody>
          </Panel>

          {/* Metadata */}
          <Panel>
            <PanelHeader num="03" title="Metadata" />
            <PanelBody className="py-1">
              <MetaRow label="App" value={appLabel(frame.app_name)} />
              <MetaRow label="Window" value={frame.window_name || '—'} />
              {host && (
                <MetaRow
                  label="Site"
                  value={
                    <a
                      href={frame.browser_url ?? '#'}
                      target="_blank"
                      rel="noreferrer"
                      className="inline-flex items-center gap-1 text-accent hover:underline"
                    >
                      {host} <ExternalLink size={12} />
                    </a>
                  }
                />
              )}
              <MetaRow label="When" value={new Date(frame.timestamp).toLocaleString()} />
              <MetaRow label="Monitor" value={`#${frame.monitor_index + 1}`} />
            </PanelBody>
          </Panel>

          {/* Tags */}
          <Panel>
            <PanelHeader num="04" title="Tags" />
            <PanelBody className="flex flex-col gap-3">
              <div className="flex flex-wrap gap-2">
                {frame.tags.length === 0 && <span className="font-mono text-xs text-faint">No tags</span>}
                {frame.tags.map((t) => (
                  <span
                    key={t.id}
                    className="flex items-center gap-1.5 border border-rule2 px-2 py-1 text-xs text-ink"
                    style={t.color ? { borderColor: t.color } : undefined}
                  >
                    {t.name}
                    <button
                      onClick={() => removeTag.mutate({ frameId: frame.id, tagId: t.id })}
                      aria-label={`Remove ${t.name}`}
                      className="text-faint hover:text-alert"
                    >
                      <X size={12} />
                    </button>
                  </span>
                ))}
              </div>
              {available.length > 0 && (
                <div className="flex gap-2">
                  <select
                    value={addId}
                    onChange={(e) => setAddId(e.target.value)}
                    className="flex-1 border border-rule bg-void px-2.5 py-1.5 font-mono text-xs text-ink2 [color-scheme:dark]"
                  >
                    <option value="">Add a tag…</option>
                    {available.map((t) => (
                      <option key={t.id} value={t.id}>
                        {t.name}
                      </option>
                    ))}
                  </select>
                  <Button
                    variant="ghost"
                    disabled={!addId || addTag.isPending}
                    onClick={() => {
                      addTag.mutate({ frameId: frame.id, tagId: Number(addId) })
                      setAddId('')
                    }}
                  >
                    Add
                  </Button>
                </div>
              )}
            </PanelBody>
          </Panel>
        </div>
      </div>

      {/* OCR */}
      <Panel>
        <PanelHeader num="05" title="Recognized text" right={`${frame.ocr_text.length} chars`} />
        <PanelBody>
          {frame.ocr_text.trim() ? (
            <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words font-mono text-[12.5px] leading-relaxed text-ink2">
              {frame.ocr_text}
            </pre>
          ) : (
            <p className="font-mono text-xs text-faint">No text was recognized in this frame.</p>
          )}
        </PanelBody>
      </Panel>
    </div>
  )
}
