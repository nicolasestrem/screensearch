import { useState, useEffect } from 'react';
import { Download, Check } from 'lucide-react';

interface DownloadProgressInfo {
    name: string;
    bytes_downloaded: number;
    total_bytes: number;
    speed_bps: number;
    eta_seconds: number;
    percentage: number;
}

interface AllDownloadsResponse {
    downloads: DownloadProgressInfo[];
}

// Format bytes to human readable
function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

// Format speed
function formatSpeed(bps: number): string {
    return `${formatBytes(bps)}/s`;
}

// Format time
function formatTime(seconds: number): string {
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    const secs = seconds % 60;
    if (minutes < 60) return secs > 0 ? `${minutes}m ${secs}s` : `${minutes}m`;
    const hours = Math.floor(minutes / 60);
    const mins = minutes % 60;
    return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`;
}

export function DownloadProgress() {
    const [downloads, setDownloads] = useState<DownloadProgressInfo[]>([]);

    useEffect(() => {
        const fetchProgress = async () => {
            try {
                const response = await fetch('/api/downloads/status');
                if (response.ok) {
                    const data: AllDownloadsResponse = await response.json();
                    setDownloads(data.downloads);
                }
            } catch (error) {
                console.error('Failed to fetch download progress:', error);
            }
        };

        // Poll every second while there are active downloads
        const interval = setInterval(fetchProgress, 1000);
        fetchProgress(); // Initial fetch

        return () => clearInterval(interval);
    }, []);

    if (downloads.length === 0) {
        return null;
    }

    return (
        <div className="fixed bottom-4 right-4 z-50 space-y-2">
            {downloads.map((download) => (
                <div
                    key={download.name}
                    className="bg-card border border-border rounded-lg p-4 shadow-lg min-w-[320px] max-w-[400px]"
                >
                    <div className="flex items-center gap-2 mb-2">
                        <Download className="h-4 w-4 text-primary animate-pulse" />
                        <span className="font-medium text-sm">{download.name}</span>
                    </div>

                    {/* Progress bar */}
                    <div className="w-full bg-secondary rounded-full h-2 overflow-hidden mb-2">
                        <div
                            className="h-full bg-primary transition-all duration-300"
                            style={{ width: `${Math.min(download.percentage, 100)}%` }}
                        />
                    </div>

                    {/* Stats */}
                    <div className="flex justify-between text-xs text-muted-foreground">
                        <span>
                            {formatBytes(download.bytes_downloaded)} / {formatBytes(download.total_bytes)}
                        </span>
                        <span>{download.percentage.toFixed(1)}%</span>
                    </div>

                    <div className="flex justify-between text-xs text-muted-foreground mt-1">
                        <span>{formatSpeed(download.speed_bps)}</span>
                        <span>ETA: {formatTime(download.eta_seconds)}</span>
                    </div>
                </div>
            ))}
        </div>
    );
}
