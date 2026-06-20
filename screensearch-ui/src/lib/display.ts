import type { Frame, SearchResult } from './types'

/** Common shape for rendering a captured moment across pages. */
export interface DisplayFrame {
  id: number
  timestamp: string
  app: string
  window: string
  activity?: string | null
  description?: string | null
  confidence?: number | null
  url?: string | null
  ocr?: string
  monitor?: number
}

export function fromFrame(f: Frame): DisplayFrame {
  return {
    id: f.id,
    timestamp: f.timestamp,
    app: f.app_name,
    window: f.window_name,
    activity: f.activity_type,
    description: f.description,
    confidence: f.confidence,
    url: f.browser_url,
    ocr: f.ocr_text,
    monitor: f.monitor_index,
  }
}

export function fromSearch(r: SearchResult): DisplayFrame {
  return {
    id: r.frame.id,
    timestamp: r.frame.timestamp,
    app: r.frame.active_process ?? '',
    window: r.frame.active_window ?? '',
    activity: r.frame.activity_type,
    description: r.frame.description,
    confidence: r.frame.confidence,
    url: r.frame.browser_url,
    ocr: r.ocr_matches.map((m) => m.text).join(' '),
    monitor: r.frame.monitor_index,
  }
}
