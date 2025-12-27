import { cn } from '../../lib/utils';

interface CircularGaugeProps {
  value: number; // 0-100
  size?: number; // diameter in pixels
  strokeWidth?: number;
  className?: string;
  label?: string;
  sublabel?: string;
  showValue?: boolean;
  animated?: boolean;
}

export function CircularGauge({
  value,
  size = 160,
  strokeWidth = 12,
  className,
  label,
  sublabel,
  showValue = true,
  animated = true,
}: CircularGaugeProps) {
  // Ensure value is between 0-100
  const clampedValue = Math.min(100, Math.max(0, value));

  // Calculate SVG parameters
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const strokeDashoffset = circumference - (clampedValue / 100) * circumference;

  // Center point
  const center = size / 2;

  // Color based on value
  const getStrokeColor = () => {
    if (clampedValue >= 80) return 'stroke-green-500';
    if (clampedValue >= 50) return 'stroke-primary';
    if (clampedValue >= 25) return 'stroke-yellow-500';
    return 'stroke-muted-foreground';
  };

  return (
    <div className={cn('relative inline-flex flex-col items-center', className)}>
      <svg
        width={size}
        height={size}
        className="transform -rotate-90"
      >
        {/* Background circle */}
        <circle
          cx={center}
          cy={center}
          r={radius}
          fill="none"
          stroke="currentColor"
          strokeWidth={strokeWidth}
          className="text-muted/30"
        />

        {/* Glow effect (behind progress) */}
        <circle
          cx={center}
          cy={center}
          r={radius}
          fill="none"
          stroke="currentColor"
          strokeWidth={strokeWidth + 8}
          strokeDasharray={circumference}
          strokeDashoffset={strokeDashoffset}
          strokeLinecap="round"
          className="text-primary/20 blur-sm"
          style={{
            transition: animated ? 'stroke-dashoffset 1s ease-out' : 'none',
          }}
        />

        {/* Progress circle */}
        <circle
          cx={center}
          cy={center}
          r={radius}
          fill="none"
          strokeWidth={strokeWidth}
          strokeDasharray={circumference}
          strokeDashoffset={strokeDashoffset}
          strokeLinecap="round"
          className={cn(
            getStrokeColor(),
            'drop-shadow-[0_0_8px_rgba(37,99,235,0.5)]'
          )}
          style={{
            transition: animated ? 'stroke-dashoffset 1s ease-out' : 'none',
          }}
        />
      </svg>

      {/* Center content */}
      <div className="absolute inset-0 flex flex-col items-center justify-center">
        {showValue && (
          <span className="text-3xl font-bold text-foreground">
            {Math.round(clampedValue)}%
          </span>
        )}
        {label && (
          <span className="text-sm font-medium text-muted-foreground mt-1">
            {label}
          </span>
        )}
        {sublabel && (
          <span className="text-xs text-muted-foreground/70">
            {sublabel}
          </span>
        )}
      </div>
    </div>
  );
}
