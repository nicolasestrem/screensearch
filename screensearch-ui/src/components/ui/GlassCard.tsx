import { ReactNode } from 'react';
import { cn } from '../../lib/utils';

type GlowColor = 'cyan' | 'green' | 'purple' | 'none';

interface GlassCardProps {
  children: ReactNode;
  className?: string;
  hover?: boolean;
  glow?: boolean | GlowColor;
  padding?: 'none' | 'sm' | 'md' | 'lg';
}

const paddingClasses = {
  none: '',
  sm: 'p-3',
  md: 'p-4',
  lg: 'p-6',
};

export function GlassCard({
  children,
  className,
  hover = false,
  padding = 'md',
}: GlassCardProps) {

  return (
    <div
      className={cn(
        'bg-paper border border-rule',
        paddingClasses[padding],
        hover && 'hover:bg-paper-2 hover:border-ink transition-colors cursor-pointer',
        className
      )}
    >
      {children}
    </div>
  );
}

interface GlassCardHeaderProps {
  children: ReactNode;
  className?: string;
  icon?: ReactNode;
  badge?: ReactNode;
}

export function GlassCardHeader({
  children,
  className,
  icon,
  badge,
}: GlassCardHeaderProps) {
  return (
    <div className={cn('flex items-center justify-between mb-4 pb-4 border-b border-rule', className)}>
      <div className="flex items-center gap-2">
        {icon && (
          <span className="text-ink">{icon}</span>
        )}
        <h3 className="font-serif text-[18px] text-ink">{children}</h3>
      </div>
      {badge}
    </div>
  );
}

interface GlassCardContentProps {
  children: ReactNode;
  className?: string;
}

export function GlassCardContent({
  children,
  className,
}: GlassCardContentProps) {
  return (
    <div className={cn('text-ink-2', className)}>
      {children}
    </div>
  );
}
