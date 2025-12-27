import { ReactNode } from 'react';
import { Lock } from 'lucide-react';
import { cn } from '../../lib/utils';
import { GlassCard } from './GlassCard';

interface ComingSoonCardProps {
  title: string;
  description: string;
  icon?: ReactNode;
  className?: string;
}

export function ComingSoonCard({
  title,
  description,
  icon,
  className,
}: ComingSoonCardProps) {
  return (
    <GlassCard
      className={cn(
        'relative overflow-hidden opacity-75 hover:opacity-90 transition-opacity',
        className
      )}
      glow
    >
      {/* Subtle gradient overlay */}
      <div className="absolute inset-0 bg-gradient-to-br from-primary/5 to-transparent pointer-events-none" />

      <div className="relative z-10">
        {/* Header with icon and badge */}
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            {icon ? (
              <span className="text-primary/70">{icon}</span>
            ) : (
              <Lock className="h-5 w-5 text-primary/70" />
            )}
            <h3 className="font-semibold text-foreground/80">{title}</h3>
          </div>
          <span className="badge-coming-soon">Coming Soon</span>
        </div>

        {/* Description */}
        <p className="text-sm text-muted-foreground/70">
          {description}
        </p>

        {/* Decorative elements */}
        <div className="mt-4 flex gap-2">
          <div className="h-1 w-8 rounded-full bg-primary/20" />
          <div className="h-1 w-4 rounded-full bg-primary/10" />
          <div className="h-1 w-2 rounded-full bg-primary/5" />
        </div>
      </div>
    </GlassCard>
  );
}
