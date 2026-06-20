import { useEffect, useRef, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import ReactMarkdown from 'react-markdown'
import clsx from 'clsx'
import { ArrowRight, Copy, Download, FileText, MessageSquareText } from 'lucide-react'
import { Panel, PanelBody, PanelHeader } from '../components/Panel'
import { Button, ErrorNote, Spinner } from '../components/ui'
import { useFrame, useGenerateAnswer, useGenerateReport, useSettings } from '../lib/hooks'
import { useUi } from '../lib/store'
import { appLabel, dayBounds, timecode } from '../lib/format'
import { ApiError } from '../lib/api'

function SourceChip({ id }: { id: number }) {
  const navigate = useNavigate()
  const f = useFrame(id).data
  return (
    <button
      onClick={() => navigate(`/timeline/${id}`)}
      className="flex items-center gap-2 border border-rule2 bg-void px-2.5 py-1.5 font-mono text-xs text-ink2 hover:border-accent hover:text-ink"
    >
      <span className="text-accent">{f ? timecode(f.timestamp) : `#${id}`}</span>
      {f && <span className="truncate text-muted">{appLabel(f.app_name)}</span>}
    </button>
  )
}

function errMsg(e: unknown): string {
  if (e instanceof ApiError) return e.message
  if (e instanceof Error) return e.message
  return 'Something went wrong.'
}

type Mode = 'ask' | 'report'
type ReportType = 'daily' | 'weekly' | 'custom'

export default function Recall() {
  const [mode, setMode] = useState<Mode>('ask')
  const seed = useUi((s) => s.recallSeed)
  const setSeed = useUi((s) => s.setRecallSeed)

  // AI report provider — now sourced from persisted backend settings (was a
  // browser-only Zustand store). The local engine is always usable; a remote
  // provider needs both a URL and a model. Default to '' (not 'local') while
  // settings are still loading so the Generate button isn't briefly enabled with
  // an unconfirmed provider. The API key is write-only and never returned, so the
  // request omits it and the backend falls back to the stored key.
  const settings = useSettings().data
  const aiProviderUrl = settings?.ai_provider_url ?? ''
  const aiModel = settings?.ai_model ?? ''

  // ---- Ask ----
  const [question, setQuestion] = useState('')
  const ask = useGenerateAnswer()
  // Keep a stable ref to the latest mutate so the seed effect can depend only on
  // the seed itself (the mutation object changes identity on every render).
  const askMutate = useRef(ask.mutate)
  askMutate.current = ask.mutate

  // Run a seeded question handed off from the command palette. setSeed(null)
  // clears the seed so re-renders don't re-fire, and a fresh palette submission
  // sets a new seed that this effect picks up.
  useEffect(() => {
    if (seed) {
      setQuestion(seed)
      setSeed(null)
      askMutate.current(seed)
    }
  }, [seed, setSeed])

  const runAsk = () => {
    const q = question.trim()
    if (q) ask.mutate(q)
  }

  // ---- Report ----
  const [reportType, setReportType] = useState<ReportType>('daily')
  const [customPrompt, setCustomPrompt] = useState('')
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('')
  const report = useGenerateReport()
  const aiReady = aiProviderUrl === 'local' || (aiProviderUrl.trim() !== '' && aiModel.trim() !== '')

  const runReport = () => {
    const now = new Date()
    let start: string | undefined
    let end: string | undefined
    if (reportType === 'daily') {
      const b = dayBounds(now)
      start = b.start
      end = b.end
    } else if (reportType === 'weekly') {
      end = now.toISOString()
      start = new Date(now.getTime() - 7 * 86400000).toISOString()
    } else {
      start = from ? new Date(from).toISOString() : undefined
      end = to ? new Date(to).toISOString() : undefined
    }
    report.mutate({
      provider_url: aiProviderUrl,
      api_key: undefined, // write-only; backend uses the stored key
      model: aiModel,
      start_time: start,
      end_time: end,
      prompt: reportType === 'custom' && customPrompt.trim() ? customPrompt.trim() : undefined,
    })
  }

  const reportText = report.data?.report ?? ''
  const copyReport = () => navigator.clipboard?.writeText(reportText)
  const downloadReport = () => {
    const blob = new Blob([reportText], { type: 'text/markdown' })
    const a = document.createElement('a')
    a.href = URL.createObjectURL(blob)
    a.download = `screensearch-report-${reportType}.md`
    a.click()
    // Defer revocation so the browser has queued the download before the URL is
    // released (avoids timing issues on slower machines).
    const url = a.href
    setTimeout(() => URL.revokeObjectURL(url), 0)
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex border border-rule">
        {(
          [
            { k: 'ask', label: 'Ask', icon: MessageSquareText },
            { k: 'report', label: 'Report', icon: FileText },
          ] as const
        ).map(({ k, label, icon: Icon }) => (
          <button
            key={k}
            onClick={() => setMode(k)}
            className={clsx(
              'eyebrow flex items-center gap-2 px-5 py-2.5 text-xs',
              mode === k ? 'bg-accent/15 text-accent' : 'text-muted hover:text-ink'
            )}
          >
            <Icon size={14} /> {label}
          </button>
        ))}
      </div>

      {mode === 'ask' ? (
        <Panel>
          <PanelHeader num="01" title="Ask your screen" right="grounded answer" />
          <PanelBody className="flex flex-col gap-4">
            <div className="flex items-center gap-3 border border-rule2 bg-void px-4 py-3.5">
              <span className="font-mono text-base text-accent">&gt;</span>
              <input
                value={question}
                onChange={(e) => setQuestion(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && runAsk()}
                placeholder="What did the error in yesterday's build say?"
                className="w-full bg-transparent font-sans text-[15px] text-ink placeholder:text-faint focus:outline-none"
              />
              <Button variant="primary" onClick={runAsk} disabled={!question.trim() || ask.isPending}>
                {ask.isPending ? 'Thinking…' : 'Ask'}
                {!ask.isPending && <ArrowRight size={14} />}
              </Button>
            </div>

            {ask.isPending && <Spinner label="Searching your memory and composing an answer…" />}
            {ask.isError && <ErrorNote message={errMsg(ask.error)} />}
            {ask.data && (
              <div className="border border-rule bg-panel2">
                <div className="prose prose-invert prose-deck max-w-none px-5 py-4">
                  <ReactMarkdown>{ask.data.answer}</ReactMarkdown>
                </div>
                {ask.data.sources.length > 0 && (
                  <div className="border-t border-rule px-5 py-4">
                    <div className="eyebrow mb-3 text-[10.5px] text-faint">
                      Evidence · {ask.data.sources.length} frames
                    </div>
                    <div className="flex flex-wrap gap-2">
                      {ask.data.sources.map((s) => (
                        <SourceChip key={s} id={s} />
                      ))}
                    </div>
                  </div>
                )}
              </div>
            )}
          </PanelBody>
        </Panel>
      ) : (
        <Panel>
          <PanelHeader num="01" title="Generate report" right="analysis over a time range" />
          <PanelBody className="flex flex-col gap-4">
            {!aiReady ? (
              <div className="flex flex-col items-start gap-3 border border-rule bg-void px-4 py-4">
                <p className="font-mono text-xs text-muted">
                  Reports use your configured AI provider. Set a provider URL and model first.
                </p>
                <Link to="/settings" className="eyebrow border border-rule2 px-3 py-1.5 text-xs text-accent hover:border-accent">
                  Configure AI provider →
                </Link>
              </div>
            ) : (
              <>
                <div className="flex flex-wrap items-center gap-2.5">
                  <div className="flex border border-rule">
                    {(['daily', 'weekly', 'custom'] as ReportType[]).map((t) => (
                      <button
                        key={t}
                        onClick={() => setReportType(t)}
                        className={clsx(
                          'eyebrow px-3.5 py-2 text-[11px]',
                          reportType === t ? 'bg-accent/15 text-accent' : 'text-muted hover:text-ink'
                        )}
                      >
                        {t}
                      </button>
                    ))}
                  </div>
                  {reportType === 'custom' && (
                    <>
                      <input
                        type="date"
                        value={from}
                        onChange={(e) => setFrom(e.target.value)}
                        className="border border-rule bg-void px-2.5 py-2 font-mono text-xs text-ink2 [color-scheme:dark]"
                      />
                      <span className="font-mono text-xs text-faint">→</span>
                      <input
                        type="date"
                        value={to}
                        onChange={(e) => setTo(e.target.value)}
                        className="border border-rule bg-void px-2.5 py-2 font-mono text-xs text-ink2 [color-scheme:dark]"
                      />
                    </>
                  )}
                  <Button variant="primary" onClick={runReport} disabled={report.isPending} className="ml-auto">
                    {report.isPending ? 'Generating…' : 'Generate'}
                  </Button>
                </div>

                {reportType === 'custom' && (
                  <textarea
                    value={customPrompt}
                    onChange={(e) => setCustomPrompt(e.target.value)}
                    placeholder="Optional: ask a specific question about this period…"
                    rows={3}
                    className="border border-rule bg-void px-3 py-2.5 font-sans text-sm text-ink placeholder:text-faint focus:outline-none"
                  />
                )}

                {report.isPending && <Spinner label="Building context and writing the report…" />}
                {report.isError && <ErrorNote message={errMsg(report.error)} />}
                {report.data && (
                  <div className="border border-rule bg-panel2">
                    <div className="flex items-center justify-between border-b border-rule px-4 py-2.5">
                      <span className="font-mono text-xs text-muted">
                        {report.data.model_used} · {report.data.context_source}
                        {report.data.tokens_used ? ` · ${report.data.tokens_used} tok` : ''}
                      </span>
                      <div className="flex gap-2">
                        <Button variant="ghost" onClick={copyReport}>
                          <Copy size={13} /> Copy
                        </Button>
                        <Button variant="ghost" onClick={downloadReport}>
                          <Download size={13} /> .md
                        </Button>
                      </div>
                    </div>
                    <div className="prose prose-invert prose-deck max-w-none px-5 py-4">
                      <ReactMarkdown>{report.data.report}</ReactMarkdown>
                    </div>
                  </div>
                )}
              </>
            )}
          </PanelBody>
        </Panel>
      )}
    </div>
  )
}
