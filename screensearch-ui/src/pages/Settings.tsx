import { useEffect, useMemo, useState } from 'react'
import clsx from 'clsx'
import { Check, Plus, X } from 'lucide-react'
import { Panel, PanelBody, PanelHeader } from '../components/Panel'
import { Button, CoverageBar, ErrorNote, Spinner, StatusDot } from '../components/ui'
import {
  useDownloadModel,
  useDownloads,
  useEmbeddingStatus,
  useGenerateEmbeddings,
  useModelStatus,
  useMonitors,
  usePrepareModels,
  useSelectModel,
  useServerControl,
  useServerStatus,
  useSettings,
  useToggleEmbeddings,
  useUpdateSettings,
  useValidateAi,
  useVisionModels,
  useVisionStatus,
} from '../lib/hooks'
import { useUi } from '../lib/store'
import { bytes, duration, pct } from '../lib/format'
import { ApiError } from '../lib/api'

function parseArr(s: string | undefined): string[] {
  if (!s) return []
  try {
    const v = JSON.parse(s)
    return Array.isArray(v) ? v.map(String) : []
  } catch {
    return []
  }
}

/** Filename (with extension) from an absolute model path, for compact display. */
function baseName(p: string): string {
  return p.split(/[\\/]/).pop() || p
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="eyebrow text-[10.5px] text-faint">{label}</span>
      {children}
    </label>
  )
}

const inputCls =
  'border border-rule bg-void px-3 py-2 font-mono text-sm text-ink placeholder:text-faint focus:outline-none'

function Toggle({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      onClick={() => onChange(!on)}
      role="switch"
      aria-checked={on}
      className="flex items-center gap-3"
    >
      <span className={clsx('relative h-5 w-9 border transition-colors', on ? 'border-accent bg-accent/30' : 'border-rule2 bg-void')}>
        <span className={clsx('absolute top-0.5 h-3.5 w-3.5 transition-all', on ? 'left-[18px] bg-accent' : 'left-0.5 bg-muted')} />
      </span>
      <span className="text-sm text-ink">{label}</span>
    </button>
  )
}

function errMsg(e: unknown): string {
  if (e instanceof ApiError) return e.message
  if (e instanceof Error) return e.message
  return 'Request failed.'
}

export default function Settings() {
  const settingsQ = useSettings()
  const monitors = useMonitors().data ?? []
  const updateSettings = useUpdateSettings()

  // capture + vision form (single backend settings record)
  const [captureInterval, setCaptureInterval] = useState(3)
  const [retention, setRetention] = useState(30)
  const [paused, setPaused] = useState(false)
  const [monSel, setMonSel] = useState<number[]>([]) // empty = all
  const [excluded, setExcluded] = useState<string[]>([])
  const [excludeInput, setExcludeInput] = useState('')
  const [visionEnabled, setVisionEnabled] = useState(false)
  const [visionProvider, setVisionProvider] = useState('local')
  const [visionModel, setVisionModel] = useState('')
  const [visionEndpoint, setVisionEndpoint] = useState('')
  const [visionApiKey, setVisionApiKey] = useState('')
  // Populate the form once on first load; later background refetches must not
  // clobber the user's unsaved edits.
  const [initialized, setInitialized] = useState(false)

  useEffect(() => {
    const s = settingsQ.data
    if (!s || initialized) return
    setCaptureInterval(s.capture_interval)
    setRetention(s.retention_days)
    setPaused(s.is_paused === 1)
    setMonSel(parseArr(s.monitors).map(Number).filter((n) => !Number.isNaN(n)))
    setExcluded(parseArr(s.excluded_apps))
    setVisionEnabled(s.vision_enabled === 1)
    setVisionProvider(s.vision_provider || 'local')
    setVisionModel(s.vision_model || '')
    setVisionEndpoint(s.vision_endpoint || '')
    setVisionApiKey(s.vision_api_key || '')
    setInitialized(true)
  }, [settingsQ.data, initialized])

  const save = () =>
    updateSettings.mutate({
      capture_interval: captureInterval,
      monitors: JSON.stringify(monSel),
      excluded_apps: JSON.stringify(excluded),
      is_paused: paused ? 1 : 0,
      retention_days: retention,
      vision_enabled: visionEnabled ? 1 : 0,
      vision_provider: visionProvider,
      vision_model: visionModel,
      vision_endpoint: visionEndpoint,
      vision_api_key: visionApiKey || null,
    })

  // embeddings
  const emb = useEmbeddingStatus().data
  const toggleEmb = useToggleEmbeddings()
  const genEmb = useGenerateEmbeddings()
  const prepEmb = usePrepareModels()

  // vision status + models
  const vision = useVisionStatus().data
  const visionModels = useVisionModels().data?.models ?? []

  // AI provider (Recall reports) — client-side config
  const aiConfig = useUi((s) => s.aiConfig)
  const setAiConfig = useUi((s) => s.setAiConfig)
  const [ai, setAi] = useState(aiConfig)
  useEffect(() => setAi(aiConfig), [aiConfig])
  const validate = useValidateAi()
  const aiIsLocal = ai.providerUrl === 'local'

  // local model + server
  const modelStatus = useModelStatus().data
  const downloadModel = useDownloadModel()
  const selectModel = useSelectModel()
  const server = useServerStatus().data
  const serverCtl = useServerControl()
  const downloadsActive = (modelStatus?.downloading ?? false) || (server?.status === 'starting')
  const downloads = useDownloads(downloadsActive).data?.downloads ?? []

  const monitorsAll = monSel.length === 0
  const toggleMonitor = (idx: number) => {
    setMonSel((cur) => (cur.includes(idx) ? cur.filter((i) => i !== idx) : [...cur, idx]))
  }

  const accel = useMemo(() => {
    if (server?.acceleration === 'gpu') return { label: 'GPU · Vulkan', tone: 'ok' as const }
    if (server?.acceleration === 'cpu') return { label: 'CPU', tone: 'warn' as const }
    return { label: 'not running', tone: 'muted' as const }
  }, [server])

  if (settingsQ.isLoading) return <Spinner label="Loading settings…" />
  if (settingsQ.isError) return <ErrorNote message="Could not load settings." />

  return (
    <div className="flex max-w-4xl flex-col gap-5">
      <h1 className="font-display text-xl font-semibold tracking-wide text-ink">Settings</h1>

      {/* CAPTURE */}
      <Panel>
        <PanelHeader num="01" title="Capture" />
        <PanelBody className="flex flex-col gap-5">
          <div className="grid gap-5 sm:grid-cols-2">
            <Field label="Capture interval (seconds)">
              <input
                type="number"
                min={1}
                value={captureInterval}
                onChange={(e) => setCaptureInterval(Math.max(1, Number(e.target.value)))}
                className={inputCls}
              />
            </Field>
            <Field label="Retention (days)">
              <input
                type="number"
                min={1}
                value={retention}
                onChange={(e) => setRetention(Math.max(1, Number(e.target.value)))}
                className={inputCls}
              />
            </Field>
          </div>

          <Toggle on={paused} onChange={setPaused} label="Pause capture" />

          <div className="flex flex-col gap-2">
            <span className="eyebrow text-[10.5px] text-faint">Monitors</span>
            <div className="flex flex-wrap gap-2">
              <button
                onClick={() => setMonSel([])}
                className={clsx(
                  'border px-3 py-1.5 text-xs',
                  monitorsAll ? 'border-accent bg-accent/15 text-accent' : 'border-rule text-muted hover:text-ink'
                )}
              >
                All
              </button>
              {monitors.map((m) => {
                const active = monSel.includes(m.index)
                return (
                  <button
                    key={m.index}
                    onClick={() => toggleMonitor(m.index)}
                    className={clsx(
                      'border px-3 py-1.5 text-xs',
                      active ? 'border-accent bg-accent/15 text-accent' : 'border-rule text-muted hover:text-ink'
                    )}
                  >
                    {m.label}
                  </button>
                )
              })}
              {monitors.length === 0 && <span className="font-mono text-xs text-faint">No monitors detected</span>}
            </div>
          </div>

          <div className="flex flex-col gap-2">
            <span className="eyebrow text-[10.5px] text-faint">Excluded apps (never captured)</span>
            <div className="flex flex-wrap gap-2">
              {excluded.map((a) => (
                <span key={a} className="flex items-center gap-1.5 border border-rule2 px-2 py-1 text-xs text-ink">
                  {a}
                  <button onClick={() => setExcluded((c) => c.filter((x) => x !== a))} aria-label={`Remove ${a}`} className="text-faint hover:text-alert">
                    <X size={12} />
                  </button>
                </span>
              ))}
            </div>
            <div className="flex gap-2">
              <input
                value={excludeInput}
                onChange={(e) => setExcludeInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && excludeInput.trim()) {
                    setExcluded((c) => Array.from(new Set([...c, excludeInput.trim()])))
                    setExcludeInput('')
                  }
                }}
                placeholder="App name, e.g. 1Password"
                className={clsx(inputCls, 'flex-1')}
              />
              <Button
                variant="ghost"
                onClick={() => {
                  if (excludeInput.trim()) {
                    setExcluded((c) => Array.from(new Set([...c, excludeInput.trim()])))
                    setExcludeInput('')
                  }
                }}
              >
                <Plus size={14} /> Add
              </Button>
            </div>
          </div>

          <div className="flex items-center gap-3">
            <Button variant="primary" onClick={save} disabled={updateSettings.isPending}>
              {updateSettings.isPending ? 'Saving…' : 'Save capture & vision'}
            </Button>
            {updateSettings.isSuccess && (
              <span className="flex items-center gap-1.5 font-mono text-xs text-ok">
                <Check size={13} /> Saved
              </span>
            )}
            {updateSettings.isError && <span className="font-mono text-xs text-alert">{errMsg(updateSettings.error)}</span>}
          </div>
        </PanelBody>
      </Panel>

      {/* SEMANTIC SEARCH */}
      <Panel>
        <PanelHeader num="02" title="Semantic search" right={emb ? pct(emb.coverage_percent) : ''} />
        <PanelBody className="flex flex-col gap-4">
          <p className="font-mono text-xs leading-relaxed text-muted">
            Embeddings power meaning-based search and the “Ask” answers. The {emb?.model ?? 'embedding'} model
            downloads on first use.
          </p>
          <Toggle on={emb?.enabled ?? false} onChange={(v) => toggleEmb.mutate(v)} label="Enable semantic search" />
          {emb && (
            <>
              <CoverageBar value={emb.coverage_percent} />
              <div className="flex justify-between font-mono text-xs text-muted">
                <span>
                  {emb.frames_with_embeddings.toLocaleString()} / {emb.total_frames.toLocaleString()} frames indexed
                </span>
                <span className="flex items-center gap-1.5">
                  <StatusDot tone={emb.engine_ready ? 'ok' : 'muted'} />
                  {emb.engine_ready ? 'engine ready' : 'engine idle'}
                </span>
              </div>
            </>
          )}
          {emb?.error && <ErrorNote message={emb.error} />}
          <div className="flex flex-wrap gap-2">
            <Button variant="ghost" onClick={() => genEmb.mutate(undefined)} disabled={genEmb.isPending || !emb?.enabled}>
              {genEmb.isPending ? 'Indexing…' : 'Index now'}
            </Button>
            <Button variant="ghost" onClick={() => prepEmb.mutate()} disabled={prepEmb.isPending}>
              {prepEmb.isPending ? 'Preparing…' : 'Download model'}
            </Button>
          </div>
          {genEmb.data && <span className="font-mono text-xs text-ok">Processed {genEmb.data.frames_processed} frames.</span>}
        </PanelBody>
      </Panel>

      {/* VISION */}
      <Panel>
        <PanelHeader
          num="03"
          title="Vision"
          right={vision ? `${vision.completed.toLocaleString()} / ${vision.total_frames.toLocaleString()}` : ''}
        />
        <PanelBody className="flex flex-col gap-4">
          <p className="font-mono text-xs leading-relaxed text-muted">
            On-device vision describes each frame and tags its activity. Changes here are saved with the capture
            settings above.
          </p>
          <Toggle on={visionEnabled} onChange={setVisionEnabled} label="Enable vision analysis" />
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Provider">
              <select value={visionProvider} onChange={(e) => setVisionProvider(e.target.value)} className={clsx(inputCls, '[color-scheme:dark]')}>
                <option value="local">Local (llama.cpp)</option>
                <option value="ollama">Ollama</option>
                <option value="openai">OpenAI-compatible</option>
              </select>
            </Field>
            <Field label="Model">
              {visionProvider === 'local' && visionModels.length > 0 ? (
                <select value={visionModel} onChange={(e) => setVisionModel(e.target.value)} className={clsx(inputCls, '[color-scheme:dark]')}>
                  <option value="">Auto-select</option>
                  {visionModels.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.id}
                    </option>
                  ))}
                </select>
              ) : visionProvider === 'local' ? (
                <div className="flex items-center gap-2 font-mono text-sm text-muted">
                  <StatusDot tone="muted" />
                  <span>no vision model found</span>
                </div>
              ) : (
                <input value={visionModel} onChange={(e) => setVisionModel(e.target.value)} placeholder="e.g. qwen2.5vl" className={inputCls} />
              )}
            </Field>
          </div>
          {visionProvider === 'local' && visionModels.length === 0 && (
            <p className="font-mono text-xs leading-relaxed text-muted">
              No vision-capable model detected. Drop a vision GGUF <span className="text-ink2">and</span> its matching{' '}
              <span className="text-ink2">*mmproj*.gguf</span> projector into <span className="text-ink2">.models/</span> (the
              two must be the same family — a projector is only paired with its own model).
            </p>
          )}
          {visionProvider === 'local' && (
            <div className="flex flex-wrap items-center gap-3">
              {!server?.server_binary_available ? (
                <>
                  <Button variant="ghost" onClick={() => serverCtl.downloadServer.mutate()} disabled={serverCtl.downloadServer.isPending}>
                    {serverCtl.downloadServer.isPending ? 'Downloading…' : 'Download llama-server'}
                  </Button>
                  <span className="font-mono text-xs text-faint">Vision shares the local server (section 05).</span>
                </>
              ) : (
                <span className="font-mono text-xs text-faint">
                  Uses the local llama-server (section 05) · {accel.label}
                </span>
              )}
            </div>
          )}
          {visionProvider !== 'local' && (
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label="Endpoint">
                <input value={visionEndpoint} onChange={(e) => setVisionEndpoint(e.target.value)} placeholder="http://localhost:11434/v1" className={inputCls} />
              </Field>
              <Field label="API key (optional)">
                <input value={visionApiKey} onChange={(e) => setVisionApiKey(e.target.value)} type="password" placeholder="sk-…" className={inputCls} />
              </Field>
            </div>
          )}
          {vision && (
            <div className="flex flex-wrap gap-x-6 gap-y-1 font-mono text-xs text-muted">
              <span>done: <span className="text-ink2">{vision.completed.toLocaleString()}</span></span>
              <span>pending: <span className="text-ink2">{vision.pending.toLocaleString()}</span></span>
              <span>processing: <span className="text-ink2">{vision.processing.toLocaleString()}</span></span>
              <span>failed: <span className={vision.failed > 0 ? 'text-alert' : 'text-ink2'}>{vision.failed.toLocaleString()}</span></span>
            </div>
          )}
        </PanelBody>
      </Panel>

      {/* AI PROVIDER (reports) */}
      <Panel>
        <PanelHeader num="04" title="AI provider" right="for reports" />
        <PanelBody className="flex flex-col gap-4">
          <p className="font-mono text-xs leading-relaxed text-muted">
            Used by Recall → Report. Use the local engine below, or point at a remote
            OpenAI-compatible provider. (“Ask” answers always use the local engine.)
          </p>
          <div className="flex flex-wrap gap-2">
            <button
              onClick={() => setAi({ ...ai, providerUrl: 'local' })}
              className={clsx(
                'border px-3 py-1.5 text-xs',
                aiIsLocal ? 'border-accent bg-accent/15 text-accent' : 'border-rule text-muted hover:text-ink'
              )}
            >
              Local engine
            </button>
            <button
              onClick={() => setAi({ ...ai, providerUrl: ai.providerUrl === 'local' ? '' : ai.providerUrl })}
              className={clsx(
                'border px-3 py-1.5 text-xs',
                !aiIsLocal ? 'border-accent bg-accent/15 text-accent' : 'border-rule text-muted hover:text-ink'
              )}
            >
              Remote
            </button>
          </div>
          {aiIsLocal ? (
            <p className="font-mono text-xs leading-relaxed text-muted">
              Reports will run on the local answer engine
              {modelStatus?.model_path ? <> (<span className="text-ink2">{baseName(modelStatus.model_path)}</span>)</> : null}. Choose the
              model in “Local answer engine” below.
            </p>
          ) : (
            <div className="grid gap-4 sm:grid-cols-3">
              <Field label="Provider URL">
                <input value={ai.providerUrl} onChange={(e) => setAi({ ...ai, providerUrl: e.target.value })} placeholder="http://localhost:11434/v1" className={inputCls} />
              </Field>
              <Field label="Model">
                <input value={ai.model} onChange={(e) => setAi({ ...ai, model: e.target.value })} placeholder="e.g. llama3.1" className={inputCls} />
              </Field>
              <Field label="API key (optional)">
                <input value={ai.apiKey} onChange={(e) => setAi({ ...ai, apiKey: e.target.value })} type="password" placeholder="sk-…" className={inputCls} />
              </Field>
            </div>
          )}
          <div className="flex flex-wrap items-center gap-3">
            <Button variant="primary" onClick={() => setAiConfig(ai)}>
              Save
            </Button>
            <Button
              variant="ghost"
              disabled={(!aiIsLocal && (!ai.providerUrl || !ai.model)) || validate.isPending}
              onClick={() => validate.mutate({ provider_url: ai.providerUrl, model: aiIsLocal ? 'local' : ai.model, api_key: ai.apiKey || undefined })}
            >
              {validate.isPending ? 'Testing…' : 'Test connection'}
            </Button>
            {validate.data && (
              <span className={clsx('font-mono text-xs', validate.data.success ? 'text-ok' : 'text-alert')}>
                {validate.data.message}
              </span>
            )}
            {validate.isError && <span className="font-mono text-xs text-alert">{errMsg(validate.error)}</span>}
          </div>
        </PanelBody>
      </Panel>

      {/* LOCAL MODEL & SERVER */}
      <Panel>
        <PanelHeader num="05" title="Local answer engine" />
        <PanelBody className="flex flex-col gap-4">
          <p className="font-mono text-xs leading-relaxed text-muted">
            Runs your local GGUF for “Ask” answers (and for Reports when the AI
            provider above is set to Local). Pick which model to use; when vision is
            enabled it shares this server, so the vision model answers instead.
          </p>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Model">
              {modelStatus && modelStatus.available_models.length > 0 ? (
                <select
                  value={modelStatus.selected}
                  onChange={(e) => selectModel.mutate(e.target.value)}
                  disabled={selectModel.isPending}
                  className={clsx(inputCls, '[color-scheme:dark]')}
                >
                  <option value="">Auto-select</option>
                  {modelStatus.available_models.map((m) => (
                    <option key={m} value={m}>
                      {baseName(m)}
                    </option>
                  ))}
                </select>
              ) : (
                <div className="flex items-center gap-2 font-mono text-sm text-ink2">
                  <StatusDot tone={modelStatus?.downloaded ? 'ok' : 'muted'} />
                  <span>{modelStatus ? `${modelStatus.model_name}${modelStatus.downloaded ? '' : ' · not downloaded'}` : '—'}</span>
                </div>
              )}
            </Field>
            <div className="flex flex-col gap-1.5">
              <span className="eyebrow text-[10.5px] text-faint">Server</span>
              <div className="flex items-center gap-2 font-mono text-sm text-ink2">
                <StatusDot tone={accel.tone} />
                <span>
                  {server?.status ?? '—'} · {accel.label}
                </span>
              </div>
            </div>
          </div>
          {modelStatus?.model_path && (
            <p className="font-mono text-xs text-faint">
              active: {baseName(modelStatus.model_path)}
              {modelStatus.downloaded ? ` · ${bytes(modelStatus.model_size_bytes)}` : ''}
              {selectModel.isPending ? ' · applying…' : ''}
            </p>
          )}

          <div className="flex flex-wrap gap-2">
            {!modelStatus?.downloaded && (
              <Button variant="ghost" onClick={() => downloadModel.mutate()} disabled={downloadModel.isPending}>
                Download model
              </Button>
            )}
            {!server?.server_binary_available && (
              <Button variant="ghost" onClick={() => serverCtl.downloadServer.mutate()} disabled={serverCtl.downloadServer.isPending}>
                Download server binary
              </Button>
            )}
            {server?.status === 'running' ? (
              <Button variant="danger" onClick={() => serverCtl.stop.mutate()} disabled={serverCtl.stop.isPending}>
                Stop server
              </Button>
            ) : (
              <Button
                variant="ghost"
                onClick={() => serverCtl.start.mutate()}
                disabled={serverCtl.start.isPending || !modelStatus?.downloaded || !server?.server_binary_available}
              >
                Start server
              </Button>
            )}
          </div>

          {downloads.length > 0 && (
            <div className="flex flex-col gap-3 border-t border-rule pt-4">
              {downloads.map((d) => (
                <div key={d.name} className="flex flex-col gap-1.5">
                  <div className="flex items-baseline justify-between font-mono text-xs">
                    <span className="text-ink2">{d.name}</span>
                    <span className="text-muted">
                      {d.error ? (
                        <span className="text-alert">{d.error}</span>
                      ) : (
                        `${Math.round(d.percentage)}% · ${bytes(d.speed_bps)}/s · ${duration(d.eta_seconds)} left`
                      )}
                    </span>
                  </div>
                  <CoverageBar value={d.percentage} />
                </div>
              ))}
            </div>
          )}
        </PanelBody>
      </Panel>
    </div>
  )
}
