import clsx from 'clsx'
import type { ButtonHTMLAttributes, ReactNode } from 'react'
import { activityFor } from '../lib/activity'

export function StatusDot({
  tone = 'muted',
  pulse = false,
  className,
}: {
  tone?: 'ok' | 'warn' | 'alert' | 'accent' | 'muted'
  pulse?: boolean
  className?: string
}) {
  const bg = {
    ok: 'bg-ok',
    warn: 'bg-warn',
    alert: 'bg-alert',
    accent: 'bg-accent',
    muted: 'bg-faint',
  }[tone]
  return (
    <span
      className={clsx('inline-block h-2 w-2 rounded-full', bg, pulse && 'animate-pulse-rec', className)}
    />
  )
}

export function ActivityBadge({ type }: { type: string | null | undefined }) {
  const a = activityFor(type)
  return (
    <span
      className="eyebrow inline-block px-2 py-0.5 text-[11px] text-void"
      style={{ background: a.color }}
    >
      {a.label}
    </span>
  )
}

export function CoverageBar({
  value,
  tone = 'accent',
}: {
  value: number
  tone?: 'accent' | 'research'
}) {
  const w = Math.max(0, Math.min(100, value))
  const bg = tone === 'accent' ? 'bg-accent' : 'bg-act-research'
  return (
    <div className="h-2.5 border border-rule bg-void">
      <div className={clsx('h-full', bg)} style={{ width: `${w}%` }} />
    </div>
  )
}

export function Button({
  variant = 'ghost',
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: 'primary' | 'ghost' | 'danger'
  children: ReactNode
}) {
  const styles = {
    primary: 'border-accent bg-accent text-void hover:bg-accent2 hover:border-accent2',
    ghost: 'border-rule2 text-ink hover:border-muted hover:bg-white/[0.03]',
    danger: 'border-rule2 text-alert hover:border-alert hover:bg-alert/10',
  }[variant]
  return (
    <button
      className={clsx(
        'eyebrow inline-flex items-center justify-center gap-2 border px-3.5 py-2 text-xs transition-colors disabled:cursor-not-allowed disabled:opacity-40',
        styles,
        className
      )}
      {...rest}
    >
      {children}
    </button>
  )
}

export function Empty({ title, hint, action }: { title: string; hint?: string; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-14 text-center">
      <div className="eyebrow text-sm text-muted">{title}</div>
      {hint && <p className="max-w-md font-mono text-xs leading-relaxed text-faint">{hint}</p>}
      {action && <div className="mt-3">{action}</div>}
    </div>
  )
}

export function Spinner({ label }: { label?: string }) {
  return (
    <div className="flex items-center gap-3 px-1 py-6 font-mono text-xs text-muted">
      <span className="inline-block h-3 w-3 animate-spin border border-rule2 border-t-accent" />
      {label ?? 'Loading…'}
    </div>
  )
}

export function ErrorNote({ message }: { message: string }) {
  return (
    <div className="border border-rule bg-void px-4 py-3 font-mono text-xs text-alert">
      {message}
    </div>
  )
}

/** Large display metric, e.g. a percentage or count. */
export function Metric({ value, unit, tone }: { value: ReactNode; unit?: string; tone?: 'dim' }) {
  return (
    <span
      className={clsx('font-display font-semibold leading-none', tone === 'dim' ? 'text-ink2' : 'text-ink')}
    >
      <span className="text-[40px]">{value}</span>
      {unit && <span className="text-[22px]">{unit}</span>}
    </span>
  )
}
