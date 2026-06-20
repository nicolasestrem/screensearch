import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export type ViewMode = 'grid' | 'list'

export type ToastKind = 'success' | 'error' | 'info'

export interface Toast {
  id: number
  kind: ToastKind
  message: string
}

let toastSeq = 0

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

  // Transient toast notifications (not persisted).
  toasts: Toast[]
  pushToast: (kind: ToastKind, message: string) => void
  dismissToast: (id: number) => void
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
      toasts: [],
      pushToast: (kind, message) =>
        set((s) => ({ toasts: [...s.toasts, { id: ++toastSeq, kind, message }] })),
      dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
    }),
    {
      name: 'screensearch-ui',
      // Only the view layout is durable; toasts and palette state are ephemeral.
      partialize: (s) => ({ viewMode: s.viewMode }),
    }
  )
)
