// Typed fetch client for the ScreenSearch REST API (localhost:3131, proxied at /api).
import type {
  AiConnectionResponse,
  AiReportRequest,
  AiReportResponse,
  AllDownloads,
  EmbeddingStatus,
  Frame,
  FrameQuery,
  GenerateAnswer,
  Health,
  ModelStatus,
  MonitorInfo,
  PaginatedFrames,
  Readiness,
  SearchParams,
  SearchResult,
  ServerStatus,
  Settings,
  Tag,
  UpdateSettings,
  VisionModelsResponse,
  VisionStatus,
} from './types'

const BASE = '/api'

export class ApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

function qs(params: Record<string, string | number | boolean | undefined | null>): string {
  const sp = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== null && v !== '') sp.set(k, String(v))
  }
  const s = sp.toString()
  return s ? `?${s}` : ''
}

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response
  try {
    res = await fetch(`${BASE}${path}`, {
      headers: { 'Content-Type': 'application/json', ...(init?.headers ?? {}) },
      ...init,
    })
  } catch {
    throw new ApiError(0, 'Cannot reach the ScreenSearch service. Is it running?')
  }
  if (!res.ok) {
    let detail = res.statusText
    try {
      const body = await res.text()
      if (body) detail = body
    } catch {
      /* ignore */
    }
    throw new ApiError(res.status, detail)
  }
  if (res.status === 204) return undefined as T
  return (await res.json()) as T
}

const jget = <T>(path: string, signal?: AbortSignal) => req<T>(path, { method: 'GET', signal })
const jpost = <T>(path: string, body?: unknown) =>
  req<T>(path, { method: 'POST', body: body === undefined ? undefined : JSON.stringify(body) })

// ---- blob image cache (object URLs), revoked on demand to avoid leaks ----
const blobCache = new Map<number, string>()

export async function getFrameImage(id: number): Promise<string> {
  const existing = blobCache.get(id)
  if (existing) return existing
  const res = await fetch(`${BASE}/frames/${id}/image`)
  if (!res.ok) throw new ApiError(res.status, `Failed to load image for frame ${id}`)
  const url = URL.createObjectURL(await res.blob())
  blobCache.set(id, url)
  return url
}

export function revokeFrameImage(id: number): void {
  const url = blobCache.get(id)
  if (url) {
    URL.revokeObjectURL(url)
    blobCache.delete(id)
  }
}

export const api = {
  health: (s?: AbortSignal) => jget<Health>('/health', s),
  readiness: (s?: AbortSignal) => jget<Readiness>('/system/readiness', s),
  monitors: (s?: AbortSignal) => jget<MonitorInfo[]>('/monitors', s),

  frames: (p: FrameQuery, s?: AbortSignal) => jget<PaginatedFrames>(`/frames${qs({ ...p })}`, s),
  frame: (id: number, s?: AbortSignal) => jget<Frame>(`/frames/${id}`, s),

  search: (p: SearchParams, s?: AbortSignal) => jget<SearchResult[]>(`/search${qs({ ...p })}`, s),
  keywords: (keywords: string, limit = 8, s?: AbortSignal) =>
    jget<string[]>(`/search/keywords${qs({ keywords, limit })}`, s),

  tags: (s?: AbortSignal) => jget<Tag[]>('/tags', s),
  createTag: (tag_name: string, color?: string) => jpost<Tag>('/tags', { tag_name, color }),
  addTagToFrame: (frameId: number, tag_id: number) =>
    jpost<unknown>(`/frames/${frameId}/tags`, { tag_id }),
  removeTagFromFrame: (frameId: number, tagId: number) =>
    req<unknown>(`/frames/${frameId}/tags/${tagId}`, { method: 'DELETE' }),

  settings: (s?: AbortSignal) => jget<Settings>('/settings', s),
  updateSettings: (body: UpdateSettings) => jpost<Settings>('/settings', body),

  embeddingStatus: (s?: AbortSignal) => jget<EmbeddingStatus>('/embeddings/status', s),
  toggleEmbeddings: (on: boolean) => jpost<EmbeddingStatus>('/embeddings/enable', on),
  generateEmbeddings: (batch_size?: number) =>
    jpost<{ success: boolean; message: string; frames_processed: number }>(
      '/embeddings/generate',
      { batch_size }
    ),
  prepareModels: () => jpost<EmbeddingStatus>('/embeddings/models/prepare', undefined),

  visionStatus: (s?: AbortSignal) => jget<VisionStatus>('/vision/status', s),
  visionModels: (s?: AbortSignal) => jget<VisionModelsResponse>('/vision/models', s),
  analyzeFrame: (frameId: number) =>
    jpost<{ success: boolean; frame_id: number; queue_id: number; already_queued: boolean }>(
      `/vision/analyze/${frameId}`,
      undefined
    ),

  serverStatus: (s?: AbortSignal) => jget<ServerStatus>('/ai/server/status', s),
  startServer: () => jpost<{ success: boolean; message: string; status: string }>('/ai/server/start', undefined),
  stopServer: () => jpost<{ success: boolean; message: string; status: string }>('/ai/server/stop', undefined),
  downloadServer: () => jpost<{ success: boolean; message: string }>('/ai/server/download', undefined),
  modelStatus: (s?: AbortSignal) => jget<ModelStatus>('/ai/model/status', s),
  downloadModel: () => jpost<{ success: boolean; message: string }>('/ai/model/download', undefined),

  downloads: (s?: AbortSignal) => jget<AllDownloads>('/downloads/status', s),

  generate: (query: string) => jpost<GenerateAnswer>('/generate', { query }),
  validateAi: (provider_url: string, model: string, api_key?: string) =>
    jpost<AiConnectionResponse>('/ai/validate', { provider_url, model, api_key }),
  generateReport: (body: AiReportRequest) => jpost<AiReportResponse>('/ai/generate', body),
}
