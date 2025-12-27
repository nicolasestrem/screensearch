import { ReactNode } from 'react';
import { cn } from '../../lib/utils';

interface GlassCardProps {
  children: ReactNode;
  className?: string;
  hover?: boolean;
  glow?: boolean;
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
  glow = false,
  padding = 'md',
}: GlassCardProps) {
  return (
    <div
      className={cn(
        'glass-card',
        paddingClasses[padding],
        hover && 'glass-card-hover cursor-pointer',
        glow && 'animate-border-glow',
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
