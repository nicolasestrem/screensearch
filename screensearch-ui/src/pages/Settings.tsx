import { useEffect, useMemo, useRef, useState } from 'react'
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
import { toast } from '../lib/toast'
import type { UpdateSettings } from '../lib/types'
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

/** A download progress row (percentage, speed, ETA) for the active downloads. */
function DownloadRow({
  name,
  percentage,
  speed_bps,
  eta_seconds,
  error,
}: {
  name: string
  percentage: number
  speed_bps: number
  eta_seconds: number
  error?: string | null
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-baseline justify-between font-mono text-xs">
        <span className="text-ink2">{name}</span>
        <span className="text-muted">
          {error ? (
            <span className="text-alert">{error}</span>
          ) : (
            `${Math.round(percentage)}% · ${bytes(speed_bps)}/s · ${duration(eta_seconds)} left`
          )}
        </span>
      </div>
      <CoverageBar value={percentage} />
    </div>
  )
}

export default function Settings() {
  const settingsQ = useSettings()
  const monitors = useMonitors().data ?? []
  const updateSettings = useUpdateSettings()

  // Persisted settings form (capture + vision + AI provider — one backend record).
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
  // AI report provider (now persisted server-side, not browser-only). The API
  // key is write-only: GET /settings never returns it (only `ai_has_api_key`), so
  // the form sends a value ONLY when the user edits the field (`aiApiKeyDirty`).
  // Untouched → omitted → backend keeps the stored key; edited to "" → clears it.
  const [aiProviderUrl, setAiProviderUrl] = useState('local')
  const [aiModel, setAiModel] = useState('')
  const [aiApiKey, setAiApiKey] = useState('')
  const [aiApiKeyDirty, setAiApiKeyDirty] = useState(false)

  // Populate the form once on first load; later background refetches must not
  // clobber the user's unsaved edits. `lastSaved` holds the serialized body that
  // is in sync with the backend so the auto-save effect can no-op when nothing
  // actually changed (including right after the initial populate).
  const [initialized, setInitialized] = useState(false)
  const lastSaved = useRef('')

  const buildBody = useMemo(
    () => (): UpdateSettings => {
      const body: UpdateSettings = {
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
        ai_provider_url: aiProviderUrl,
        ai_model: aiModel,
      }
      // Write-only key: include it only after the user edits the field, so an
      // unrelated edit can't blank a stored key (and "" explicitly clears it).
      if (aiApiKeyDirty) body.ai_api_key = aiApiKey
      return body
    },
    [
      captureInterval,
      monSel,
      excluded,
      paused,
      retention,
      visionEnabled,
      visionProvider,
      visionModel,
      visionEndpoint,
      visionApiKey,
      aiProviderUrl,
      aiModel,
      aiApiKey,
      aiApiKeyDirty,
    ]
  )

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
    setAiProviderUrl(s.ai_provider_url || 'local')
    setAiModel(s.ai_model || '')
    // ai_api_key is write-only: the backend never returns it, so the field starts
    // blank and is omitted from the baseline below (matching `buildBody` while
    // `aiApiKeyDirty` is false).
    // Mark the just-loaded values as the saved baseline so the auto-save effect
    // doesn't immediately POST them back. Key order MUST match `buildBody`.
    lastSaved.current = JSON.stringify({
      capture_interval: s.capture_interval,
      monitors: JSON.stringify(parseArr(s.monitors).map(Number).filter((n) => !Number.isNaN(n))),
      excluded_apps: JSON.stringify(parseArr(s.excluded_apps)),
      is_paused: s.is_paused === 1 ? 1 : 0,
      retention_days: s.retention_days,
      vision_enabled: s.vision_enabled === 1 ? 1 : 0,
      vision_provider: s.vision_provider || 'local',
      vision_model: s.vision_model || '',
      vision_endpoint: s.vision_endpoint || '',
      vision_api_key: s.vision_api_key || null,
      ai_provider_url: s.ai_provider_url || 'local',
      ai_model: s.ai_model || '',
    } satisfies UpdateSettings)
    setInitialized(true)
  }, [settingsQ.data, initialized])

  // Debounced auto-save: whenever the form differs from the last-saved snapshot,
  // persist it 600 ms after the user stops editing. No explicit Save button.
  const mutateRef = useRef(updateSettings.mutate)
  mutateRef.current = updateSettings.mutate
  useEffect(() => {
    if (!initialized) return
    const body = buildBody()
    const snap = JSON.stringify(body)
    if (snap === lastSaved.current) return
    const t = setTimeout(() => {
      // Stamp the baseline only after the save succeeds — otherwise a failed save
      // would mark the failed values as "in sync" and never retry them.
      mutateRef.current(body, { onSuccess: () => { lastSaved.current = snap } })
    }, 600)
    return () => clearTimeout(t)
  }, [initialized, buildBody])

  // embeddings
  const emb = useEmbeddingStatus().data
  const toggleEmb = useToggleEmbeddings()
  const genEmb = useGenerateEmbeddings()
  const prepEmb = usePrepareModels()
  // Tracks a user-triggered "Download model" so we can poll progress and toast
  // on completion even when semantic search is still toggled off.
  const [preparing, setPreparing] = useState(false)
  const wasReady = useRef<boolean>(false)
  useEffect(() => {
    const ready = emb?.engine_ready ?? false
    if (ready && !wasReady.current && preparing) {
      toast.success('Search model ready')
      setPreparing(false)
    }
    wasReady.current = ready
  }, [emb?.engine_ready, preparing])

  // vision status + models
  const vision = useVisionStatus().data
  const visionModels = useVisionModels().data?.models ?? []

  // AI report provider validation (Test connection)
  const validate = useValidateAi()
  const aiIsLocal = aiProviderUrl === 'local'

  // local model + server
  const modelStatus = useModelStatus().data
  const downloadModel = useDownloadModel()
  const selectModel = useSelectModel()
  const server = useServerStatus().data
  const serverCtl = useServerControl()

  // Poll downloads while anything is actively fetching: the embedding model
  // (boot auto-load or manual prepare), the answer model, or the server binary.
  const embModelDownloading = preparing || (!!emb?.enabled && !(emb?.engine_ready ?? false))
  const downloadsActive =
    embModelDownloading || (modelStatus?.downloading ?? false) || server?.status === 'starting'
  const downloads = useDownloads(downloadsActive).data?.downloads ?? []
  const embDownload = downloads.find((d) => d.key === 'embedding_model')
  const otherDownloads = downloads.filter((d) => d.key !== 'embedding_model')

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
      <div className="flex items-center justify-between">
        <h1 className="font-display text-xl font-semibold tracking-wide text-ink">Settings</h1>
        <span className="flex items-center gap-1.5 font-mono text-xs">
          {updateSettings.isPending ? (
            <span className="flex items-center gap-1.5 text-muted">
              <span className="inline-block h-3 w-3 animate-spin border border-rule2 border-t-accent" />
              Saving…
            </span>
          ) : updateSettings.isError ? (
            <span className="text-alert">Save failed</span>
          ) : updateSettings.isSuccess ? (
            <span className="flex items-center gap-1.5 text-ok">
              <Check size={13} /> Saved
            </span>
          ) : (
            <span className="text-faint">Changes save automatically</span>
          )}
        </span>
      </div>

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
        </PanelBody>
      </Panel>

      {/* SEMANTIC SEARCH */}
      <Panel>
        <PanelHeader num="02" title="Semantic search" right={emb ? pct(emb.coverage_percent) : ''} />
        <PanelBody className="flex flex-col gap-4">
          <p className="font-mono text-xs leading-relaxed text-muted">
            Embeddings power meaning-based search and the “Ask” answers. The {emb?.model ?? 'embedding'} model
            (~300 MB) downloads once from HuggingFace on first use and is cached locally.
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
                  {emb.engine_ready ? 'engine ready' : embModelDownloading ? 'loading…' : 'engine idle'}
                </span>
              </div>
            </>
          )}
          {emb?.error && <ErrorNote message={emb.error} />}

          {embDownload && (
            <div className="border-t border-rule pt-4">
              <DownloadRow
                name={embDownload.name}
                percentage={embDownload.percentage}
                speed_bps={embDownload.speed_bps}
                eta_seconds={embDownload.eta_seconds}
                error={embDownload.error}
              />
            </div>
          )}

          <div className="flex flex-wrap gap-2">
            <Button variant="ghost" onClick={() => genEmb.mutate(undefined)} disabled={genEmb.isPending || !emb?.enabled}>
              {genEmb.isPending ? 'Indexing…' : 'Index now'}
            </Button>
            <Button
              variant="ghost"
              onClick={() => {
                setPreparing(true)
                prepEmb.mutate()
              }}
              disabled={(emb?.engine_ready ?? false) || !!embDownload || preparing}
            >
              {emb?.engine_ready
                ? 'Model ready'
                : embDownload || preparing
                  ? 'Downloading…'
                  : 'Download model'}
            </Button>
          </div>
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
            On-device vision describes each frame and tags its activity. Local models are auto-discovered from your{' '}
            <span className="text-ink2">.models/</span> folder.
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
          {visionProvider === 'local' && visionModels.length > 0 && (
            <p className="font-mono text-[11px] leading-relaxed text-faint">
              {visionModels.length} model{visionModels.length > 1 ? 's' : ''} found in{' '}
              <span className="text-ink2">.models/</span> · vision shares the local answer engine (section 04).
            </p>
          )}
          {visionProvider === 'local' && visionModels.length === 0 && (
            <p className="font-mono text-xs leading-relaxed text-muted">
              No vision-capable model detected. Drop a vision GGUF <span className="text-ink2">and</span> its matching{' '}
              <span className="text-ink2">*mmproj*.gguf</span> projector into <span className="text-ink2">.models/</span> (the
              two must be the same family — a projector is only paired with its own model).
            </p>
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

      {/* AI & MODELS (answer engine + report provider) */}
      <Panel>
        <PanelHeader num="04" title="AI & models" right={aiIsLocal ? 'local engine' : 'remote provider'} />
        <PanelBody className="flex flex-col gap-4">
          <p className="font-mono text-xs leading-relaxed text-muted">
            Powers “Ask” answers and Reports. “Ask” always uses the local engine; Reports use whichever
            engine you pick here.
          </p>

          {/* Answer engine: local vs remote */}
          <div className="flex flex-wrap gap-2">
            <button
              onClick={() => setAiProviderUrl('local')}
              className={clsx(
                'border px-3 py-1.5 text-xs',
                aiIsLocal ? 'border-accent bg-accent/15 text-accent' : 'border-rule text-muted hover:text-ink'
              )}
            >
              Local engine
            </button>
            <button
              onClick={() => setAiProviderUrl((u) => (u === 'local' ? '' : u))}
              className={clsx(
                'border px-3 py-1.5 text-xs',
                !aiIsLocal ? 'border-accent bg-accent/15 text-accent' : 'border-rule text-muted hover:text-ink'
              )}
            >
              Remote provider
            </button>
          </div>

          {aiIsLocal ? (
            <>
              <div className="grid gap-4 sm:grid-cols-2">
                <Field label="Local model">
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

              {/* Provenance: where the active model comes from. */}
              <p className="font-mono text-[11px] leading-relaxed text-faint">
                {modelStatus?.model_path ? (
                  <>
                    active: <span className="text-ink2">{baseName(modelStatus.model_path)}</span> · from{' '}
                    <span className="text-ink2">.models/</span> · auto-discovered
                    {modelStatus.downloaded ? ` · ${bytes(modelStatus.model_size_bytes)}` : ''}
                    {selectModel.isPending ? ' · applying…' : ''}
                  </>
                ) : modelStatus && !modelStatus.downloaded ? (
                  <>No local model yet. Download the default below, or drop a GGUF into <span className="text-ink2">.models/</span>.</>
                ) : (
                  'Resolving local model…'
                )}
              </p>

              <div className="flex flex-wrap gap-2">
                {/* Guard on the query being loaded so the buttons don't flash on
                    first paint (when `modelStatus`/`server` are still undefined). */}
                {modelStatus && !modelStatus.downloaded && (
                  <Button variant="ghost" onClick={() => downloadModel.mutate()} disabled={downloadModel.isPending}>
                    Download default model
                  </Button>
                )}
                {server && !server.server_binary_available && (
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
            </>
          ) : (
            <>
              <div className="grid gap-4 sm:grid-cols-3">
                <Field label="Provider URL">
                  <input value={aiProviderUrl} onChange={(e) => setAiProviderUrl(e.target.value)} placeholder="http://localhost:11434/v1" className={inputCls} />
                </Field>
                <Field label="Model">
                  <input value={aiModel} onChange={(e) => setAiModel(e.target.value)} placeholder="e.g. llama3.1" className={inputCls} />
                </Field>
                <Field label="API key (optional)">
                  <input
                    value={aiApiKey}
                    onChange={(e) => {
                      setAiApiKey(e.target.value)
                      setAiApiKeyDirty(true)
                    }}
                    type="password"
                    placeholder={
                      settingsQ.data?.ai_has_api_key && !aiApiKeyDirty ? '•••••••• saved — type to replace' : 'sk-…'
                    }
                    className={inputCls}
                  />
                </Field>
              </div>
              <div className="flex flex-wrap items-center gap-3">
                <Button
                  variant="ghost"
                  disabled={!aiProviderUrl || !aiModel || validate.isPending}
                  onClick={() => validate.mutate({ provider_url: aiProviderUrl, model: aiModel, api_key: aiApiKey || undefined })}
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
            </>
          )}

          {otherDownloads.length > 0 && (
            <div className="flex flex-col gap-3 border-t border-rule pt-4">
              {otherDownloads.map((d) => (
                <DownloadRow
                  key={d.key}
                  name={d.name}
                  percentage={d.percentage}
                  speed_bps={d.speed_bps}
                  eta_seconds={d.eta_seconds}
                  error={d.error}
                />
              ))}
            </div>
          )}
        </PanelBody>
      </Panel>
    </div>
  )
}
