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

const glowClasses: Record<GlowColor, string> = {
  cyan: 'glow-cyan-subtle border-gradient-cyan',
  green: 'shadow-[0_0_20px_-5px_rgba(0,255,136,0.2)]',
  purple: 'shadow-[0_0_20px_-5px_rgba(168,85,247,0.2)]',
  none: '',
};

export function GlassCard({
  children,
  className,
  hover = false,
  glow = false,
  padding = 'md',
}: GlassCardProps) {
  // Handle glow prop - can be boolean or color string
  const glowClass = typeof glow === 'string'
    ? glowClasses[glow]
    : glow
      ? 'animate-border-glow'
      : '';

  return (
    <div
      className={cn(
        'glass-card',
        paddingClasses[padding],
        hover && 'glass-card-hover cursor-pointer',
        glowClass,
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
    <div className={cn('flex items-center justify-between mb-4', className)}>
      <div className="flex items-center gap-2">
        {icon && (
          <span className="text-primary">{icon}</span>
        )}
        <h3 className="font-semibold text-foreground">{children}</h3>
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
    <div className={cn('text-muted-foreground', className)}>
      {children}
    </div>
  );
}
