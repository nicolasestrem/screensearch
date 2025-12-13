import { useMemo } from 'react';
import { FrameResponse } from '../../types';
import { getHours, getMinutes, startOfDay, format } from 'date-fns';
import { cn } from '../../lib/utils';

/**
 * Props for the ActivityGraph component.
 */
interface ActivityGraphProps {
    /** List of frames to visualize density for */
    frames: FrameResponse[];
    /** The date currently being viewed */
    currentDate: Date;
    /** Callback when a specific time bucket is clicked */
    onTimeSelect?: (date: Date) => void;
    /** Optional class name override */
    className?: string;
}

/**
 * Visualizes daily activity density using a bar graph.
 * Divides the day into 10-minute buckets (144 total) and plots frame counts.
 */
export function ActivityGraph({ frames, currentDate, onTimeSelect, className }: ActivityGraphProps) {
    // Buckets: 144 buckets (10 minutes each) for 24 hours
    const BUCKETS = 144;

    const buckets = useMemo(() => {
        const counts = new Array(BUCKETS).fill(0);
        frames.forEach(frame => {
            const date = new Date(frame.timestamp);
            const minutes = getHours(date) * 60 + getMinutes(date);
            const bucketIndex = Math.floor(minutes / 10); // 10 minute buckets
            if (bucketIndex >= 0 && bucketIndex < BUCKETS) {
                counts[bucketIndex]++;
            }
        });
        return counts;
    }, [frames]);

    const maxCount = Math.max(...buckets, 1);

    return (
        <div className={cn("w-full h-32 bg-card/50 backdrop-blur-sm rounded-xl border border-border/50 p-4 relative select-none", className)}>
            <div className="absolute top-2 left-4 text-xs font-semibold text-muted-foreground">
                Activity - {format(currentDate, 'MMMM d')}
            </div>

            {/* Graph Area */}
            <div className="w-full h-full flex items-end gap-[2px] pt-6">
                {buckets.map((count, i) => {
                    const heightPercent = (count / maxCount) * 100;
                    const timeLabel = `${Math.floor(i / 6).toString().padStart(2, '0')}:${((i % 6) * 10).toString().padStart(2, '0')}`;

                    return (
                        <div
                            key={i}
                            className="flex-1 h-full flex items-end group relative cursor-pointer"
                            onClick={() => {
                                const hour = Math.floor(i / 6);
                                const minute = (i % 6) * 10;
                                const newDate = startOfDay(currentDate);
                                newDate.setHours(hour, minute);
                                onTimeSelect?.(newDate);
                            }}
                        >
                            <div
                                className={cn(
                                    "w-full rounded-t-sm transition-all duration-300",
                                    count > 0
                                        ? "bg-primary/80 group-hover:bg-primary"
                                        : "bg-muted/20 group-hover:bg-muted/40"
                                )}
                                style={{ height: `${Math.max(heightPercent, count > 0 ? 5 : 0)}%` }}
                            />

                            {/* Tooltip */}
                            <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 hidden group-hover:block z-50">
                                <div className="bg-popover text-popover-foreground text-[10px] px-2 py-1 rounded shadow-xl whitespace-nowrap border border-border">
                                    {timeLabel} • {count} frames
                                </div>
                            </div>
                        </div>
                    );
                })}
            </div>

            {/* X-Axis Labels */}
            <div className="absolute bottom-1 left-4 right-4 flex justify-between text-[10px] text-muted-foreground/50 font-mono pointer-events-none">
                <span>00:00</span>
                <span>06:00</span>
                <span>12:00</span>
                <span>18:00</span>
                <span>23:59</span>
            </div>
        </div>
    );
}
