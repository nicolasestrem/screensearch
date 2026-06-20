import { useUi } from './store'

/**
 * Fire-and-forget toast helpers usable outside React components (e.g. TanStack
 * Query mutation callbacks). They read the Zustand store imperatively, so they
 * don't need hook context.
 */
export const toast = {
  success: (message: string) => useUi.getState().pushToast('success', message),
  error: (message: string) => useUi.getState().pushToast('error', message),
  info: (message: string) => useUi.getState().pushToast('info', message),
}
