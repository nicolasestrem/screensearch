// API types — mirror the Rust serde structs in screensearch-api / screensearch-db.

export interface Tag {
  id: number
  name: string
  color?: string | null
  created_at: string
}

/** Enriched frame DTO returned by GET /api/frames/ and /api/frames/:id. */
export interface Frame {
  id: number
  timestamp: string
  file_path: string
  app_name: string
  window_name: string
  ocr_text: string
  tags: Tag[]
  thumbnail?: string | null
  description?: string | null
  confidence?: number | null
  analysis_status?: string | null
  activity_type?: string | null
  app_hint?: string | null
  browser_url?: string | null
  monitor_index: number
}

export interface PaginationInfo {
  limit: number
  offset: number
  total: number
}

export interface PaginatedFrames {
  data: Frame[]
  pagination: PaginationInfo
}

/** Raw DB frame record as embedded in SearchResult (different field names). */
export interface FrameRecord {
  id: number
  timestamp: string
  monitor_index: number
  device_name: string
  file_path: string
  active_window?: string | null
  active_process?: string | null
  browser_url?: string | null
  width: number
  height: number
  focused?: boolean | null
  created_at: string
  analysis_status?: string | null
  description?: string | null
  activity_type?: string | null
  app_hint?: string | null
  confidence?: number | null
}

export interface OcrMatch {
  id: number
  frame_id: number
  text: string
  x: number
  y: number
  width: number
  height: number
  confidence: number
}

export interface SearchResult {
  frame: FrameRecord
  ocr_matches: OcrMatch[]
  relevance_score: number
  tags: string[]
}

export type SearchMode = 'fts' | 'semantic' | 'hybrid'

export interface Health {
  status: string
  version: string
  uptime_seconds?: number
  frame_count: number
  ocr_count: number
  tag_count: number
  oldest_frame?: string | null
  newest_frame?: string | null
}

export interface EmbeddingStatus {
  enabled: boolean
  model: string
  provider: string
  model_version: string
  dimension: number
  reindex_required: boolean
  engine_ready: boolean
  error?: string | null
  total_frames: number
  frames_with_embeddings: number
  coverage_percent: number
  last_processed_frame_id: number
}

export interface VisionStatus {
  enabled: number
  provider: string
  model: string
  total_frames: number
  completed: number
  pending: number
  processing: number
  failed: number
  queue_depth: number
}

export interface ServerStatus {
  status: string
  port: number
  idle_seconds: number
  model_loaded: boolean
  server_binary_available: boolean
  acceleration: string
}

export interface ReadinessStage {
  id: string
  label: string
  state: string
  detail: string
  progress?: number | null
  eta_seconds?: number | null
}

export interface Readiness {
  core_ready: boolean
  all_ready: boolean
  stages: ReadinessStage[]
}

export interface DownloadProgress {
  name: string
  bytes_downloaded: number
  total_bytes: number
  speed_bps: number
  eta_seconds: number
  percentage: number
  error?: string | null
}

export interface AllDownloads {
  downloads: DownloadProgress[]
}

export interface Settings {
  id: number
  capture_interval: number
  monitors: string
  excluded_apps: string
  is_paused: number
  retention_days: number
  updated_at: string
  vision_enabled: number
  vision_provider: string
  vision_model: string
  vision_endpoint: string
  vision_api_key?: string | null
}

export interface UpdateSettings {
  capture_interval: number
  monitors: string
  excluded_apps: string
  is_paused: number
  retention_days: number
  vision_enabled: number
  vision_provider: string
  vision_model: string
  vision_endpoint: string
  vision_api_key?: string | null
}

export interface MonitorInfo {
  index: number
  label: string
  width: number
  height: number
  is_primary: boolean
}

export interface VisionModelEntry {
  id: string
  model_file: string
  mmproj_file: string
  selected: boolean
}

export interface VisionModelsResponse {
  models: VisionModelEntry[]
  selected?: string | null
}

export interface ModelStatus {
  downloaded: boolean
  downloading: boolean
  model_name: string
  model_size_bytes: number
  model_path?: string | null
  available_models: string[]
}

export interface GenerateAnswer {
  answer: string
  sources: number[]
}

export interface AiConnectionResponse {
  success: boolean
  message: string
}

export interface AiReportRequest {
  provider_url: string
  api_key?: string | null
  model: string
  start_time?: string | null
  end_time?: string | null
  prompt?: string | null
}

export interface AiReportResponse {
  report: string
  model_used: string
  tokens_used?: number | null
  context_source: string
}

export interface FrameQuery {
  start_time?: string
  end_time?: string
  monitor_index?: number
  limit?: number
  offset?: number
  q?: string
}

export interface SearchParams {
  q: string
  mode?: SearchMode
  limit?: number
  start_time?: string
  end_time?: string
  app?: string
}
