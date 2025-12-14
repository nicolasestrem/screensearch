// API Response Types
export interface PaginationInfo {
  limit: number;
  offset: number;
  total: number;
}

export interface PaginatedResponse<T> {
  data: T[];
  pagination: PaginationInfo;
}

// OCR Text Types
export interface OCRTextData {
  id?: number;
  frame_id?: number;
  text?: string;
  text_json?: string | object;
  x?: number;
  y?: number;
  width?: number;
  height?: number;
  confidence?: number;
  created_at?: string;
}

export type OCRTextContent = string | OCRTextData | Array<OCRTextData>;

// Frame Types
export interface Frame {
  id: number;
  timestamp: string;
  file_path: string;
  app_name: string;
  window_name: string;
  ocr_text: OCRTextContent;
  tags: Tag[];
  thumbnail?: string;
  description?: string;
  confidence?: number;
  analysis_status?: string;
}

export interface FrameResponse {
  id: number;
  timestamp: string;
  file_path: string;
  app_name: string;
  window_name: string;
  ocr_text: string;
  tags: Tag[];
  thumbnail?: string;
  description?: string;
  confidence?: number;
  analysis_status?: string;
}

// Tag Types
export interface Tag {
  id: number;
  name: string;
  color?: string;
  created_at: string;
}

export interface CreateTagRequest {
  name: string;
  color?: string;
}

// Search Types
export interface SearchParams {
  q?: string;
  limit?: number;
  offset?: number;
  content_type?: 'ocr' | 'audio' | 'all';
  start_time?: string;
  end_time?: string;
  app_name?: string;
  window_name?: string;
  include_frames?: boolean;
}

export interface SearchResult {
  type: 'OCR' | 'Audio' | 'UI';
  content: OCRContent | AudioContent | UIContent;
  relevance_score?: number;
}

export interface OCRContent {
  frame_id: number;
  text: string;
  timestamp: string;
  file_path: string;
  app_name: string;
  window_name: string;
  tags: Tag[];
  focused?: boolean;
}

export interface AudioContent {
  chunk_id: number;
  transcription: string;
  timestamp: string;
  device_name: string;
  device_type: string;
}

export interface UIContent {
  element_id: number;
  text: string;
  timestamp: string;
  app_name: string;
}

// Automation Types
export interface AutomationClickRequest {
  x: number;
  y: number;
}

export interface AutomationTypeRequest {
  text: string;
}

export interface AutomationScrollRequest {
  direction: 'up' | 'down' | 'left' | 'right';
  amount?: number;
}

export interface AutomationKeyRequest {
  key: string;
  modifiers?: string[];
}

export interface AutomationFindElementsRequest {
  selector: string;
  timeout?: number;
}

export interface UIElement {
  id: string;
  name: string;
  role: string;
  bounds: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
  value?: string;
}

// Settings Types
export interface Settings {
  id: number;
  capture_interval: number;
  monitors: string; // JSON array
  excluded_apps: string; // JSON array
  is_paused: number; // 0 or 1
  retention_days: number;
  updated_at: string;
  vision_enabled: number;
  vision_provider: string;
  vision_model: string;
  vision_endpoint: string;
  vision_api_key?: string;
}

export interface UpdateSettingsRequest {
  capture_interval: number;
  monitors: string; // JSON array
  excluded_apps: string; // JSON array
  is_paused: number; // 0 or 1
  retention_days: number;
  vision_enabled?: number;
  vision_provider?: string;
  vision_model?: string;
  vision_endpoint?: string;
  vision_api_key?: string;
}

// Legacy type for backwards compatibility
export interface AppSettings {
  captureInterval: number; // seconds
  monitorIds: number[];
  excludedApps: string[];
  isPaused: boolean;
  ocrEnabled: boolean;
  retentionDays: number;
}

// Health Check
export interface HealthStatus {
  status: 'ok' | 'degraded' | 'error';
  version: string;
  uptime: number;
  frame_count: number;
  newest_frame?: string;
  last_capture?: string;
}

// Filter State
export interface FilterState {
  dateRange: {
    start: Date | null;
    end: Date | null;
  };
  applications: string[];
  tags: number[];
  searchQuery: string;
  searchMode: 'fts' | 'semantic' | 'hybrid';
}

export interface GenerateResponse {
  answer: string;
  sources: number[];
}
