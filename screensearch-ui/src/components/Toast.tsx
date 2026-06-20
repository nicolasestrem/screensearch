import { useEffect } from 'react'
import clsx from 'clsx'
import { AlertTriangle, Check, Info, X } from 'lucide-react'
import { useUi, type Toast as ToastT } from '../lib/store'

const AUTO_DISMISS_MS = 2800

function ToastRow({ toast }: { toast: ToastT }) {
  const dismiss = useUi((s) => s.dismissToast)

  useEffect(() => {
    const t = setTimeout(() => dismiss(toast.id), AUTO_DISMISS_MS)
    return () => clearTimeout(t)
  }, [toast.id, dismiss])

  const tone =
    toast.kind === 'success'
      ? 'text-ok'
      : toast.kind === 'error'
        ? 'text-alert'
        : 'text-accent'
  const Icon = toast.kind === 'success' ? Check : toast.kind === 'error' ? AlertTriangle : Info

  return (
    <div className="flex items-center gap-2.5 border border-rule bg-panel px-3.5 py-2.5 shadow-lg">
      <Icon size={14} className={tone} />
      <span className="flex-1 font-mono text-xs text-ink">{toast.message}</span>
      <button
        onClick={() => dismiss(toast.id)}
        aria-label="Dismiss"
        className="text-faint hover:text-ink"
      >
        <X size={13} />
      </button>
    </div>
  )
}

/**
 * Bottom-right stack of transient notifications, driven by the `toasts` slice in
 * the UI store. Used for save confirmations and mutation feedback so the user
 * always knows whether an action took effect.
 */
export function ToastHost() {
  const toasts = useUi((s) => s.toasts)
  if (toasts.length === 0) return null
  return (
    <div className={clsx('pointer-events-none fixed bottom-5 right-5 z-50 flex w-80 flex-col gap-2')}>
      {toasts.map((t) => (
        <div key={t.id} className="pointer-events-auto">
          <ToastRow toast={t} />
        </div>
      ))}
    </div>
  )
}
