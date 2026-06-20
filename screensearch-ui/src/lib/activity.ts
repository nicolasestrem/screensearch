// Maps free-form vision `activity_type` strings to a small set of canonical
// buckets, each with a label and a stable colour used in the timeline, badges,
// and insights charts.

export type ActivityKey = 'code' | 'research' | 'comms' | 'media' | 'reading' | 'idle' | 'other'

interface ActivityMeta {
  key: ActivityKey
  label: string
  color: string
}

export const ACTIVITY: Record<ActivityKey, ActivityMeta> = {
  code: { key: 'code', label: 'Coding', color: '#E8853D' },
  research: { key: 'research', label: 'Research', color: '#5E8BA8' },
  comms: { key: 'comms', label: 'Comms', color: '#C9A24B' },
  media: { key: 'media', label: 'Media', color: '#9B7CB8' },
  reading: { key: 'reading', label: 'Reading', color: '#6FA8A0' },
  idle: { key: 'idle', label: 'Idle', color: '#4A4A52' },
  other: { key: 'other', label: 'Other', color: '#A39E94' },
}

export const ACTIVITY_ORDER: ActivityKey[] = ['code', 'research', 'comms', 'reading', 'media', 'other', 'idle']

const MATCHERS: Array<[RegExp, ActivityKey]> = [
  [/cod|develop|program|terminal|debug|ide|engineer/i, 'code'],
  [/research|search|brows|docs?|documentation|stack ?overflow|learn/i, 'research'],
  [/comm|chat|messag|email|mail|slack|teams|meeting|call|social/i, 'comms'],
  [/read|article|news|paper|pdf|book/i, 'reading'],
  [/media|video|music|watch|stream|game|play|youtube|spotify/i, 'media'],
  [/idle|away|lock|inactive|screensaver/i, 'idle'],
]

export function activityFor(type: string | null | undefined): ActivityMeta {
  if (!type) return ACTIVITY.other
  for (const [re, key] of MATCHERS) {
    if (re.test(type)) return ACTIVITY[key]
  }
  return ACTIVITY.other
}
