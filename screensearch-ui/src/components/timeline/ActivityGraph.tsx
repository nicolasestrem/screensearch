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
        <div className={cn("w-full h-48 bg-card/40 backdrop-blur-md rounded-xl border border-primary/10 p-4 relative select-none overflow-hidden group/graph", className)}>
            {/* Background Grid */}
            <div className="absolute inset-0 pointer-events-none">
                <div className="absolute top-1/2 left-0 right-0 h-px bg-primary/5" />
                <div className="absolute top-1/4 left-0 right-0 h-px bg-primary/5" />
                <div className="absolute top-3/4 left-0 right-0 h-px bg-primary/5 border-t border-dashed border-primary/10" />
            </div>

            <div className="absolute top-3 left-4 flex items-center gap-2">
                <div className="w-2 h-2 rounded-full bg-primary/50 animate-pulse" />
                <span className="text-xs font-semibold text-primary/80 tracking-wide">
                    ACTIVITY DENSITY • {format(currentDate, 'MMM d').toUpperCase()}
                </span>
            </div>

            {/* Graph Area */}
            <div className="w-full h-full flex items-end gap-[1px] pt-8 pb-4 px-1">
                {buckets.map((count, i) => {
                    const heightPercent = (count / maxCount) * 100;
                    const timeLabel = `${Math.floor(i / 6).toString().padStart(2, '0')}:${((i % 6) * 10).toString().padStart(2, '0')}`;
                    const intensity = count / maxCount; // 0 to 1

                    return (
                        <div
                            key={i}
                            className="flex-1 h-full flex items-end group relative cursor-pointer hover:z-20"
                            onClick={() => {
                                const hour = Math.floor(i / 6);
                                const minute = (i % 6) * 10;
                                const newDate = startOfDay(currentDate);
                                newDate.setHours(hour, minute);
                                onTimeSelect?.(newDate);
                            }}
                        >
                            {/* Bar */}
                            <div
                                className={cn(
                                    "w-full rounded-t-[1px] transition-all duration-300 relative",
                                    count > 0
                                        ? "bg-primary group-hover:bg-cyan-400 group-hover:shadow-[0_0_15px_rgba(34,211,238,0.6)]"
                                        : "bg-primary/5 group-hover:bg-primary/10"
                                )}
                                style={{ 
                                    height: `${Math.max(heightPercent, count > 0 ? 4 : 4)}%`,
                                    opacity: count > 0 ? 0.6 + (intensity * 0.4) : 1
                                }}
                            >
                                {count > 0 && (
                                    <div className="absolute bottom-0 left-0 right-0 h-1/2 bg-gradient-to-t from-black/20 to-transparent" />
                                )}
                            </div>

                            {/* Scrubber Line (appears on hover) */}
                            <div className="absolute bottom-0 left-1/2 -translate-x-1/2 w-px h-full bg-cyan-400/50 hidden group-hover:block pointer-events-none" />

                            {/* Tooltip */}
                            <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 hidden group-hover:block z-50">
                                <div className="bg-popover/90 backdrop-blur-xl text-popover-foreground text-[10px] px-3 py-1.5 rounded-md shadow-2xl whitespace-nowrap border border-primary/20 flex flex-col items-center gap-0.5 pointer-events-none">
                                    <span className="font-bold text-primary">{timeLabel}</span>
                                    <span className="text-muted-foreground">{count} events</span>
                                </div>
                            </div>
                        </div>
                    );
                })}
            </div>

            {/* X-Axis Labels */}
            <div className="absolute bottom-1 left-4 right-4 flex justify-between text-[9px] text-muted-foreground/40 font-mono pointer-events-none tracking-widest uppercase">
                <span>00:00</span>
                <span>06:00</span>
                <span>12:00</span>
                <span>18:00</span>
                <span>23:59</span>
            </div>
        </div>
    );
}
