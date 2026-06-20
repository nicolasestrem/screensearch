import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export type ViewMode = 'grid' | 'list'

export interface AiConfig {
  providerUrl: string
  model: string
  apiKey: string
}

interface UiState {
  // Command palette (⌘K)
  paletteOpen: boolean
  setPaletteOpen: (open: boolean) => void

  // Carries a query from the Deck ask box / palette into the Recall page.
  recallSeed: string | null
  setRecallSeed: (q: string | null) => void

  // Timeline contact-sheet layout (persisted).
  viewMode: ViewMode
  setViewMode: (m: ViewMode) => void

  // AI provider used for the Recall "Report" mode (persisted).
  aiConfig: AiConfig
  setAiConfig: (c: AiConfig) => void
}

export const useUi = create<UiState>()(
  persist(
    (set) => ({
      paletteOpen: false,
      setPaletteOpen: (open) => set({ paletteOpen: open }),
      recallSeed: null,
      setRecallSeed: (q) => set({ recallSeed: q }),
      viewMode: 'grid',
      setViewMode: (m) => set({ viewMode: m }),
      aiConfig: { providerUrl: '', model: '', apiKey: '' },
      setAiConfig: (c) => set({ aiConfig: c }),
    }),
    {
      name: 'screensearch-ui',
      partialize: (s) => ({ viewMode: s.viewMode, aiConfig: s.aiConfig }),
    }
  )
)
