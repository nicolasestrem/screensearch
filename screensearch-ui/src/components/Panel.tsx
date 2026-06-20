import clsx from 'clsx'
import type { ReactNode } from 'react'

export function Panel({
  className,
  children,
}: {
  className?: string
  children: ReactNode
}) {
  return <section className={clsx('border border-rule bg-panel', className)}>{children}</section>
}

export function PanelHeader({
  num,
  title,
  right,
}: {
  num?: string
  title: string
  right?: ReactNode
}) {
  return (
    <div className="flex items-center gap-3 border-b border-rule px-4 py-3">
      {num && <span className="font-mono text-xs text-accent">{num}</span>}
      <h2 className="eyebrow text-sm text-ink2">{title}</h2>
      <div className="h-px flex-1 bg-rule" />
      {right && <div className="font-mono text-xs text-muted">{right}</div>}
    </div>
  )
}

export function PanelBody({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={clsx('p-4', className)}>{children}</div>
}
