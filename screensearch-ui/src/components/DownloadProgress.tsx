import { useState, useEffect } from 'react';
import { Download, RotateCw, X } from 'lucide-react';

interface DownloadProgressInfo {
    name: string;
    bytes_downloaded: number;
    total_bytes: number;
    speed_bps: number;
    eta_seconds: number;
    percentage: number;
    error?: string;
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
    const [dismissedDownloads, setDismissedDownloads] = useState<Set<string>>(new Set());

    useEffect(() => {
        let cancelled = false;

        const pollProgress = async () => {
            while (!cancelled) {
                try {
                    const response = await fetch('/api/downloads/status');
                    if (response.ok) {
                        const data: AllDownloadsResponse = await response.json();
                        setDownloads(data.downloads);

                        // Stop polling if no downloads are active
                        if (data.downloads.length === 0) {
                            break;
                        }
                    }
                } catch (error) {
                    console.error('Failed to fetch download progress:', error);
                }

                // Wait 1 second before next poll
                await new Promise(resolve => setTimeout(resolve, 1000));
            }
        };

        pollProgress();

        return () => {
            cancelled = true;
        };
    }, []); // Empty dependency array - only run once on mount

    const handleRetry = async (downloadName: string) => {
        // Remove from dismissed list
        setDismissedDownloads(prev => {
            const next = new Set(prev);
            next.delete(downloadName);
            return next;
        });

        // Trigger download based on name
        try {
            let endpoint = '';
            if (downloadName === 'llm_model' || downloadName === 'LLM Model') {
                endpoint = '/api/ai/download-model';
            } else if (downloadName === 'llama_server' || downloadName === 'llama-server') {
                endpoint = '/api/ai/download-llama-server';
            }

            if (endpoint) {
                await fetch(endpoint, { method: 'POST' });
            }
        } catch (error) {
            console.error('Failed to retry download:', error);
        }
    };

    const handleDismiss = (downloadName: string) => {
        setDismissedDownloads(prev => new Set(prev).add(downloadName));
    };

    // Filter out dismissed downloads
    const visibleDownloads = downloads.filter(d => !dismissedDownloads.has(d.name));

    if (visibleDownloads.length === 0) {
        return null;
    }

    return (
        <div className="fixed bottom-4 right-4 z-50 space-y-2">
            {visibleDownloads.map((download) => (
                <div
                    key={download.name}
                    className={`bg-card border rounded-lg p-4 shadow-lg min-w-[320px] max-w-[400px] ${
                        download.error ? 'border-destructive' : 'border-border'
                    }`}
                >
                    <div className="flex items-center justify-between mb-2">
                        <div className="flex items-center gap-2">
                            {download.error ? (
                                <span className="h-4 w-4 text-destructive">✕</span>
                            ) : (
                                <Download className="h-4 w-4 text-primary animate-pulse" />
                            )}
                            <span className="font-medium text-sm">{download.name}</span>
                        </div>
                        {download.error && (
                            <button
                                onClick={() => handleDismiss(download.name)}
                                className="text-muted-foreground hover:text-foreground"
                                aria-label="Dismiss"
                            >
                                <X className="h-4 w-4" />
                            </button>
                        )}
                    </div>

                    {download.error ? (
                        /* Error message and retry button */
                        <>
                            <div className="text-xs text-destructive mt-2 mb-3">
                                {download.error}
                            </div>
                            <button
                                onClick={() => handleRetry(download.name)}
                                className="flex items-center gap-2 px-3 py-1.5 text-xs bg-primary text-primary-foreground rounded hover:bg-primary/90 transition-colors w-full justify-center"
                            >
                                <RotateCw className="h-3 w-3" />
                                Retry Download
                            </button>
                        </>
                    ) : (
                        <>
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
                        </>
                    )}
                </div>
            ))}
        </div>
    );
}
