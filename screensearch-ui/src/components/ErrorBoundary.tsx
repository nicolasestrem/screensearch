import { Component, type ErrorInfo, type ReactNode } from 'react'
import { AlertTriangle } from 'lucide-react'

interface Props {
  children: ReactNode
}

interface State {
  error: Error | null
}

/**
 * Catches render-time errors in the routed page tree so a single page fault
 * shows a recovery panel instead of a blank screen. Styled in the Command Deck
 * idiom (warm-graphite panel, mono telemetry, signal-orange accent).
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('ErrorBoundary caught a render error:', error, info)
  }

  private reset = () => this.setState({ error: null })

  render() {
    const { error } = this.state
    if (!error) return this.props.children

    return (
      <div className="flex min-h-[60vh] items-center justify-center px-4">
        <div className="w-full max-w-md border border-rule bg-panel">
          <div className="flex items-center gap-3 border-b border-rule px-5 py-4">
            <AlertTriangle size={18} className="text-alert" aria-hidden />
            <div className="flex flex-col">
              <span className="eyebrow text-sm text-ink">Something went wrong</span>
              <span className="font-mono text-[11px] text-faint">an unexpected render error occurred</span>
            </div>
          </div>
          <div className="flex flex-col gap-4 px-5 py-4">
            <p className="border border-rule bg-void px-3 py-2.5 font-mono text-xs leading-relaxed text-alert">
              {error.message || 'Unknown error'}
            </p>
            <div className="flex gap-2">
              <button
                onClick={this.reset}
                className="eyebrow border border-rule2 px-3.5 py-2 text-xs text-ink hover:border-accent hover:text-accent"
              >
                Try again
              </button>
              <button
                onClick={() => window.location.reload()}
                className="eyebrow border border-accent bg-accent px-3.5 py-2 text-xs text-void hover:bg-accent2 hover:border-accent2"
              >
                Reload app
              </button>
            </div>
          </div>
        </div>
      </div>
    )
  }
}
