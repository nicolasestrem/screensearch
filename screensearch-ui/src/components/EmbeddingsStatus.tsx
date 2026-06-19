import { useState, useEffect } from 'react';
import { Brain, RefreshCw, Check, X, AlertTriangle, Download } from 'lucide-react';
import { toast } from 'react-hot-toast';

interface EmbeddingStatus {
    enabled: boolean;
    model: string;
    provider: string;
    model_version: string;
    dimension: number;
    reindex_required: boolean;
    engine_ready: boolean;
    error?: string;
    total_frames: number;
    frames_with_embeddings: number;
    coverage_percent: number;
    last_processed_frame_id: number;
}

export function EmbeddingsStatus() {
    const [status, setStatus] = useState<EmbeddingStatus | null>(null);
    const [loading, setLoading] = useState(false);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [generating, setGenerating] = useState(false);
    const [preparingModels, setPreparingModels] = useState(false);
    const [selectedPercentage, setSelectedPercentage] = useState<25 | 50 | 100>(50);

    const fetchStatus = async () => {
        try {
            setLoading(true);
            const response = await fetch('/api/embeddings/status');
            if (response.ok) {
                const data = await response.json();
                setStatus(data);
                setLoadError(null);
            } else {
                setLoadError(`Status request failed (${response.status})`);
            }
        } catch (error) {
            console.error('Failed to fetch embedding status:', error);
            setLoadError(error instanceof Error ? error.message : 'Failed to load status');
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => {
        fetchStatus();
    }, []);

    const toggleEnabled = async () => {
        if (!status) return;
        try {
            const response = await fetch('/api/embeddings/enable', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(!status.enabled),
            });
            if (response.ok) {
                const data = await response.json();
                setStatus(data);
            }
        } catch (error) {
            console.error('Failed to toggle embeddings:', error);
        }
    };

    const triggerGeneration = async (percentage: number) => {
        if (!status) return;

        try {
            setGenerating(true);
            // Calculate batch size based on percentage of frames without embeddings
            const framesWithoutEmbeddings = status.total_frames - status.frames_with_embeddings;
            const batchSize = Math.max(1, Math.ceil((framesWithoutEmbeddings * percentage) / 100));

            const response = await fetch('/api/embeddings/generate', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ batch_size: batchSize }),
            });
            const data = await response.json();
            if (response.ok && data.success) {
                await fetchStatus();
                toast.success(data.message);
            } else {
                toast.error(data.message || data.error || "Failed to start generation");
            }
        } catch (error) {
            console.error('Failed to trigger generation:', error);
            toast.error("Failed to trigger generation");
        } finally {
            setGenerating(false);
        }
    };

    // Pre-load (and, on first run, download + cache) the in-process embedding
    // model. The request blocks until the model is ready, then returns the
    // updated status.
    const prepareModels = async () => {
        try {
            setPreparingModels(true);
            const response = await fetch('/api/embeddings/models/prepare', { method: 'POST' });
            const data = await response.json();
            if (!response.ok) {
                throw new Error(data.error || 'Failed to load the embedding model');
            }
            setStatus(data);
            toast.success('Embedding model ready');
        } catch (error) {
            const message = error instanceof Error ? error.message : 'Failed to load the embedding model';
            toast.error(message);
        } finally {
            setPreparingModels(false);
        }
    };

    if (loading && !status) {
        return (
            <div className="bg-card border border-border rounded-lg p-4">
                <div className="animate-pulse flex items-center gap-3">
                    <div className="h-5 w-5 bg-muted rounded" />
                    <div className="h-4 w-32 bg-muted rounded" />
                </div>
            </div>
        );
    }

    if (!status) {
        if (loadError) {
            return (
                <div className="bg-destructive/10 border border-destructive/30 rounded-lg p-4 text-sm flex items-start gap-3">
                    <AlertTriangle className="h-5 w-5 text-destructive flex-shrink-0" />
                    <div>
                        <p className="font-medium text-destructive">Could not load AI status</p>
                        <p className="text-xs text-muted-foreground mt-1">{loadError}</p>
                        <button
                            onClick={fetchStatus}
                            className="mt-2 inline-flex items-center gap-2 px-3 py-1.5 rounded-md bg-secondary hover:bg-accent text-xs"
                        >
                            <RefreshCw className="h-3.5 w-3.5" />
                            Retry
                        </button>
                    </div>
                </div>
            );
        }
        return null;
    }

    const coverageColor = status.coverage_percent > 80
        ? 'text-green-500'
        : status.coverage_percent > 50
            ? 'text-yellow-500'
            : 'text-muted-foreground';

    return (
        <div className="space-y-4">
            <h3 className="text-lg font-semibold flex items-center gap-2">
                <Brain className="h-5 w-5" />
                AI Embeddings (RAG)
            </h3>
            <div className="bg-card border border-border rounded-lg p-4 space-y-4">
                {/* Enable/Disable Toggle */}
                <div className="flex items-center justify-between">
                    <div>
                        <p className="font-medium">Semantic Search</p>
                        <p className="text-sm text-muted-foreground">
                            Enable AI-powered search and reports
                        </p>
                    </div>
                    <button
                        onClick={toggleEnabled}
                        className={`relative w-14 h-7 rounded-full transition-colors ${status.enabled ? 'bg-primary' : 'bg-secondary'
                            }`}
                    >
                        <div
                            className={`absolute top-0.5 left-0.5 w-6 h-6 bg-white rounded-full transition-transform ${status.enabled ? 'translate-x-7' : 'translate-x-0'
                                }`}
                        />
                    </button>
                </div>

                {/* Model Info */}
                <div className="text-sm">
                    <span className="text-muted-foreground">Model: </span>
                    <span className="font-mono">{status.model}</span>
                    <span className="text-muted-foreground"> · {status.dimension}d · {status.model_version}</span>
                </div>

                {/* In-process embedding model */}
                <div className="border border-border rounded-lg p-3 space-y-2">
                    <div className="flex items-center justify-between gap-3">
                        <div>
                            <p className="text-sm font-medium">Embedding model</p>
                            <p className="text-xs text-muted-foreground">
                                Runs in-process via ONNX Runtime. First load downloads and
                                caches the model (~a few hundred MB).
                            </p>
                        </div>
                        <button
                            onClick={prepareModels}
                            disabled={preparingModels}
                            className="inline-flex items-center gap-2 px-3 py-2 rounded-md bg-secondary hover:bg-accent text-sm disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                            {preparingModels
                                ? <RefreshCw className="h-4 w-4 animate-spin" />
                                : <Download className="h-4 w-4" />}
                            {preparingModels ? 'Loading...' : 'Download / verify'}
                        </button>
                    </div>
                    {status.engine_ready ? (
                        <p className="text-xs text-green-500">
                            Embedding model is downloaded and loaded.
                        </p>
                    ) : (
                        <p className="text-xs text-muted-foreground">
                            Not loaded yet — it will load on first use, or click Download / verify.
                        </p>
                    )}
                    {status.error && (
                        <p className="text-xs text-destructive">{status.error}</p>
                    )}
                </div>

                {status.reindex_required && (
                    <div className="bg-primary/10 border border-primary/30 rounded-lg p-3 text-sm">
                        Existing vectors were created by another model and must be regenerated.
                    </div>
                )}

                {/* Coverage Bar */}
                <div>
                    <div className="flex items-center justify-between text-sm mb-1">
                        <span className="text-muted-foreground">Embedding Coverage</span>
                        <span className={coverageColor}>
                            {status.coverage_percent.toFixed(1)}%
                        </span>
                    </div>
                    <div className="w-full bg-secondary rounded-full h-2 overflow-hidden">
                        <div
                            className="h-full bg-primary transition-all duration-500"
                            style={{ width: `${Math.min(status.coverage_percent, 100)}%` }}
                        />
                    </div>
                    <p className="text-xs text-muted-foreground mt-1">
                        {status.frames_with_embeddings.toLocaleString()} / {status.total_frames.toLocaleString()} frames
                    </p>
                </div>

                {/* Generate Embeddings Section */}
                <div className="space-y-3">
                    <div className="flex items-center justify-between text-sm">
                        <span className="text-muted-foreground font-medium">Process frames:</span>
                        <span className="text-xs text-muted-foreground">
                            {status.total_frames - status.frames_with_embeddings > 0
                                ? `${(status.total_frames - status.frames_with_embeddings).toLocaleString()} remaining`
                                : 'All frames indexed'}
                        </span>
                    </div>

                    {/* Percentage Selector Buttons */}
                    <div className="grid grid-cols-3 gap-2">
                        {([25, 50, 100] as const).map((percentage) => {
                            const framesWithoutEmbeddings = status.total_frames - status.frames_with_embeddings;
                            const framesToProcess = Math.max(1, Math.ceil((framesWithoutEmbeddings * percentage) / 100));
                            const isSelected = selectedPercentage === percentage;

                            return (
                                <button
                                    key={percentage}
                                    onClick={() => {
                                        setSelectedPercentage(percentage);
                                        triggerGeneration(percentage);
                                    }}
                                    disabled={generating || !status.enabled || framesWithoutEmbeddings === 0}
                                    className={`flex flex-col items-center justify-center gap-1 px-3 py-2.5 rounded-lg border transition-all disabled:opacity-50 disabled:cursor-not-allowed ${
                                        isSelected && !generating
                                            ? 'bg-primary text-primary-foreground border-primary shadow-sm'
                                            : 'bg-card border-border hover:bg-accent hover:border-primary/50'
                                    }`}
                                >
                                    <span className="text-lg font-bold">{percentage}%</span>
                                    {!generating && framesWithoutEmbeddings > 0 && (
                                        <span className="text-[10px] opacity-80 font-mono">
                                            {framesToProcess.toLocaleString()} frames
                                        </span>
                                    )}
                                    {generating && isSelected && (
                                        <RefreshCw className="h-3 w-3 animate-spin" />
                                    )}
                                </button>
                            );
                        })}
                    </div>

                    <p className="text-xs text-muted-foreground text-center">
                        Click a percentage to process that portion of unindexed frames
                    </p>
                </div>

                {/* Status Indicator */}
                <div className="flex items-center gap-2 text-sm">
                    {status.enabled ? (
                        <>
                            <Check className="h-4 w-4 text-green-500" />
                            <span className="text-green-500">RAG enabled for reports</span>
                        </>
                    ) : (
                        <>
                            <X className="h-4 w-4 text-muted-foreground" />
                            <span className="text-muted-foreground">Using traditional search</span>
                        </>
                    )}
                </div>
            </div>
        </div>
    );
}
