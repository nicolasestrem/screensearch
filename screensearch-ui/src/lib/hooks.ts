import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from './api'
import { toast } from './toast'
import type { AiReportRequest, FrameQuery, SearchParams, UpdateSettings } from './types'

function errText(e: unknown): string {
  return e instanceof Error ? e.message : 'Request failed.'
}

// ---- live system status (polled) ----
export const useHealth = () =>
  useQuery({ queryKey: ['health'], queryFn: ({ signal }) => api.health(signal), refetchInterval: 15000 })

export const useEmbeddingStatus = () =>
  useQuery({
    queryKey: ['embeddings', 'status'],
    queryFn: ({ signal }) => api.embeddingStatus(signal),
    refetchInterval: 15000,
  })

export const useVisionStatus = () =>
  useQuery({
    queryKey: ['vision', 'status'],
    queryFn: ({ signal }) => api.visionStatus(signal),
    refetchInterval: 15000,
  })

export const useServerStatus = () =>
  useQuery({
    queryKey: ['ai', 'server', 'status'],
    queryFn: ({ signal }) => api.serverStatus(signal),
    refetchInterval: 20000,
  })

export const useReadiness = (enabled = true) =>
  useQuery({
    queryKey: ['readiness'],
    queryFn: ({ signal }) => api.readiness(signal),
    refetchInterval: enabled ? 2000 : false,
    enabled,
  })

export const useDownloads = (enabled = true) =>
  useQuery({
    queryKey: ['downloads'],
    queryFn: ({ signal }) => api.downloads(signal),
    refetchInterval: enabled ? 1500 : false,
    enabled,
  })

// ---- data ----
export const useFrames = (params: FrameQuery, enabled = true) =>
  useQuery({
    queryKey: ['frames', params],
    queryFn: ({ signal }) => api.frames(params, signal),
    enabled,
    staleTime: 10000,
  })

export const useFrame = (id: number | null) =>
  useQuery({
    queryKey: ['frame', id],
    queryFn: ({ signal }) => api.frame(id as number, signal),
    enabled: id !== null,
    staleTime: 60000,
  })

export const useSearch = (params: SearchParams, enabled: boolean) =>
  useQuery({
    queryKey: ['search', params],
    queryFn: ({ signal }) => api.search(params, signal),
    enabled: enabled && params.q.trim().length > 0,
    staleTime: 20000,
  })

export const useKeywords = (q: string, enabled: boolean) =>
  useQuery({
    queryKey: ['keywords', q],
    queryFn: ({ signal }) => api.keywords(q, 8, signal),
    enabled: enabled && q.trim().length > 0,
    staleTime: 60000,
  })

export const useTags = () => useQuery({ queryKey: ['tags'], queryFn: ({ signal }) => api.tags(signal) })

export const useSettings = () =>
  useQuery({ queryKey: ['settings'], queryFn: ({ signal }) => api.settings(signal) })

export const useMonitors = () =>
  useQuery({ queryKey: ['monitors'], queryFn: ({ signal }) => api.monitors(signal), staleTime: 300000 })

export const useVisionModels = () =>
  useQuery({ queryKey: ['vision', 'models'], queryFn: ({ signal }) => api.visionModels(signal) })

export const useModelStatus = () =>
  useQuery({ queryKey: ['ai', 'model', 'status'], queryFn: ({ signal }) => api.modelStatus(signal) })

// ---- mutations ----
export const useGenerateAnswer = () =>
  useMutation({ mutationFn: (query: string) => api.generate(query) })

export const useGenerateReport = () =>
  useMutation({ mutationFn: (body: AiReportRequest) => api.generateReport(body) })

export const useValidateAi = () =>
  useMutation({
    mutationFn: (v: { provider_url: string; model: string; api_key?: string }) =>
      api.validateAi(v.provider_url, v.model, v.api_key),
  })

export function useUpdateSettings() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (body: UpdateSettings) => api.updateSettings(body),
    onSuccess: (s) => {
      qc.setQueryData(['settings'], s)
      toast.success('Settings saved')
    },
    onError: (e) => toast.error(`Couldn't save settings: ${errText(e)}`),
  })
}

export function useToggleEmbeddings() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (on: boolean) => api.toggleEmbeddings(on),
    onSuccess: (_s, on) => {
      qc.invalidateQueries({ queryKey: ['embeddings', 'status'] })
      qc.invalidateQueries({ queryKey: ['readiness'] })
      toast.success(`Semantic search ${on ? 'enabled' : 'disabled'}`)
    },
    onError: (e) => toast.error(errText(e)),
  })
}

export function useGenerateEmbeddings() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (batch?: number) => api.generateEmbeddings(batch),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['embeddings', 'status'] })
      toast.info('Indexing started — coverage will climb as frames finish.')
    },
    onError: (e) => toast.error(errText(e)),
  })
}

export function usePrepareModels() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => api.prepareModels(),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['embeddings', 'status'] })
      qc.invalidateQueries({ queryKey: ['downloads'] })
    },
    onError: (e) => toast.error(errText(e)),
  })
}

export function useAnalyzeFrame() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (frameId: number) => api.analyzeFrame(frameId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['vision', 'status'] }),
  })
}

export function useServerControl() {
  const qc = useQueryClient()
  const invalidate = () => qc.invalidateQueries({ queryKey: ['ai', 'server', 'status'] })
  return {
    start: useMutation({
      mutationFn: () => api.startServer(),
      onSuccess: () => {
        invalidate()
        toast.info('Starting the local AI server…')
      },
      onError: (e) => toast.error(errText(e)),
    }),
    stop: useMutation({
      mutationFn: () => api.stopServer(),
      onSuccess: () => {
        invalidate()
        toast.success('Local AI server stopped')
      },
      onError: (e) => toast.error(errText(e)),
    }),
    downloadServer: useMutation({
      mutationFn: () => api.downloadServer(),
      onSuccess: () => {
        qc.invalidateQueries({ queryKey: ['downloads'] })
        toast.info('Downloading the local AI server…')
      },
      onError: (e) => toast.error(errText(e)),
    }),
  }
}

export function useDownloadModel() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => api.downloadModel(),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['ai', 'model', 'status'] })
      qc.invalidateQueries({ queryKey: ['downloads'] })
      toast.info('Model download started…')
    },
    onError: (e) => toast.error(errText(e)),
  })
}

export function useSelectModel() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (model: string) => api.selectModel(model),
    onSuccess: (s) => {
      qc.setQueryData(['ai', 'model', 'status'], s)
      // The unified server rebuilds onto the new model on next use.
      qc.invalidateQueries({ queryKey: ['ai', 'server', 'status'] })
      toast.success('Answer model updated')
    },
    onError: (e) => toast.error(errText(e)),
  })
}

export function useFrameTagMutations() {
  const qc = useQueryClient()
  const invalidate = (frameId: number) => {
    qc.invalidateQueries({ queryKey: ['frame', frameId] })
    qc.invalidateQueries({ queryKey: ['tags'] })
  }
  return {
    addTag: useMutation({
      mutationFn: (v: { frameId: number; tagId: number }) => api.addTagToFrame(v.frameId, v.tagId),
      onSuccess: (_d, v) => invalidate(v.frameId),
    }),
    removeTag: useMutation({
      mutationFn: (v: { frameId: number; tagId: number }) => api.removeTagFromFrame(v.frameId, v.tagId),
      onSuccess: (_d, v) => invalidate(v.frameId),
    }),
    createTag: useMutation({
      mutationFn: (v: { name: string; color?: string }) => api.createTag(v.name, v.color),
      onSuccess: () => qc.invalidateQueries({ queryKey: ['tags'] }),
    }),
  }
}
