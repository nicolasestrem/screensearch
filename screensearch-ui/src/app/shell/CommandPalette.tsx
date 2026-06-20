import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import clsx from 'clsx'
import { Search, MessageSquareText, Film, BarChart3, LayoutGrid, Settings as SettingsIcon } from 'lucide-react'
import { useUi } from '../../lib/store'
import { useKeywords } from '../../lib/hooks'

interface Action {
  id: string
  label: string
  hint?: string
  icon: typeof Search
  run: () => void
}

export function CommandPalette() {
  const open = useUi((s) => s.paletteOpen)
  const setOpen = useUi((s) => s.setPaletteOpen)
  const setRecallSeed = useUi((s) => s.setRecallSeed)
  const navigate = useNavigate()
  const [q, setQ] = useState('')
  const [sel, setSel] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)

  const keywordsQ = useKeywords(q, open && q.trim().length > 0)
  const keywords = useMemo(() => keywordsQ.data ?? [], [keywordsQ.data])

  useEffect(() => {
    if (open) {
      setQ('')
      setSel(0)
      setTimeout(() => inputRef.current?.focus(), 0)
    }
  }, [open])

  const close = useCallback(() => setOpen(false), [setOpen])

  const actions = useMemo<Action[]>(() => {
    const trimmed = q.trim()
    const list: Action[] = []
    if (trimmed) {
      list.push({
        id: 'ask',
        label: `Ask your screen: “${trimmed}”`,
        hint: 'Grounded answer',
        icon: MessageSquareText,
        run: () => {
          setRecallSeed(trimmed)
          navigate('/recall')
          close()
        },
      })
      list.push({
        id: 'search',
        label: `Search timeline: “${trimmed}”`,
        hint: 'Find frames',
        icon: Search,
        run: () => {
          navigate(`/timeline?q=${encodeURIComponent(trimmed)}`)
          close()
        },
      })
      for (const kw of keywords.slice(0, 5)) {
        list.push({
          id: `kw-${kw}`,
          label: kw,
          hint: 'keyword',
          icon: Search,
          run: () => {
            navigate(`/timeline?q=${encodeURIComponent(kw)}`)
            close()
          },
        })
      }
    } else {
      list.push(
        { id: 'nav-deck', label: 'Go to Deck', icon: LayoutGrid, run: () => { navigate('/'); close() } },
        { id: 'nav-recall', label: 'Go to Recall', icon: MessageSquareText, run: () => { navigate('/recall'); close() } },
        { id: 'nav-timeline', label: 'Go to Timeline', icon: Film, run: () => { navigate('/timeline'); close() } },
        { id: 'nav-insights', label: 'Go to Insights', icon: BarChart3, run: () => { navigate('/insights'); close() } },
        { id: 'nav-settings', label: 'Go to Settings', icon: SettingsIcon, run: () => { navigate('/settings'); close() } }
      )
    }
    return list
  }, [q, keywords, navigate, setRecallSeed, close])

  if (!open) return null

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      close()
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      setSel((s) => Math.min(actions.length - 1, s + 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setSel((s) => Math.max(0, s - 1))
    } else if (e.key === 'Enter') {
      e.preventDefault()
      actions[Math.min(sel, actions.length - 1)]?.run()
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/60 px-4 pt-[12vh]"
      onClick={close}
    >
      <div
        className="w-full max-w-xl border border-rule2 bg-panel shadow-2xl"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <div className="flex items-center gap-3 border-b border-rule px-4 py-3">
          <Search size={16} className="text-accent" aria-hidden />
          <input
            ref={inputRef}
            value={q}
            onChange={(e) => {
              setQ(e.target.value)
              setSel(0)
            }}
            placeholder="Ask your screen, or search your memory…"
            className="w-full bg-transparent font-sans text-[15px] text-ink placeholder:text-faint focus:outline-none"
          />
          <kbd className="font-mono text-[11px] text-faint">esc</kbd>
        </div>
        <ul className="max-h-[50vh] overflow-y-auto py-1">
          {actions.map((a, i) => {
            const Icon = a.icon
            return (
              <li key={a.id}>
                <button
                  onMouseEnter={() => setSel(i)}
                  onClick={a.run}
                  className={clsx(
                    'flex w-full items-center gap-3 px-4 py-2.5 text-left',
                    i === sel ? 'bg-accent/10 text-ink' : 'text-muted'
                  )}
                >
                  <Icon size={15} className={i === sel ? 'text-accent' : ''} aria-hidden />
                  <span className="flex-1 truncate text-sm">{a.label}</span>
                  {a.hint && <span className="eyebrow text-[10px] text-faint">{a.hint}</span>}
                </button>
              </li>
            )
          })}
        </ul>
      </div>
    </div>
  )
}
