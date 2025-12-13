import { useState, useEffect } from 'react';
import { Brain, RefreshCw, Check, X, AlertTriangle } from 'lucide-react';

interface EmbeddingStatus {
    enabled: boolean;
    model: string;
    total_frames: number;
    frames_with_embeddings: number;
    coverage_percent: number;
    last_processed_frame_id: number;
}

export function EmbeddingsStatus() {
    const [status, setStatus] = useState<EmbeddingStatus | null>(null);
    const [loading, setLoading] = useState(false);
    const [generating, setGenerating] = useState(false);

    const fetchStatus = async () => {
        try {
            setLoading(true);
            const response = await fetch('/api/embeddings/status');
            if (response.ok) {
                const data = await response.json();
                setStatus(data);
            }
        } catch (error) {
            console.error('Failed to fetch embedding status:', error);
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

    const triggerGeneration = async () => {
        try {
            setGenerating(true);
            const response = await fetch('/api/embeddings/generate', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ batch_size: 100 }),
            });
            if (response.ok) {
                // Refresh status after generation
                await fetchStatus();
            }
        } catch (error) {
            console.error('Failed to trigger generation:', error);
        } finally {
            setGenerating(false);
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

    if (!status) return null;

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

                {/* Resource Warning */}
                <div className="bg-yellow-500/10 border border-yellow-500/20 rounded-lg p-3 flex gap-3">
                    <AlertTriangle className="h-5 w-5 text-yellow-500 flex-shrink-0" />
                    <div className="text-sm">
                        <p className="font-medium text-yellow-500">Resource Intensive Feature</p>
                        <p className="text-xs text-muted-foreground mt-0.5">
                            Enabling RAG runs a local AI model to embed screen text. This uses significant CPU (~20%) and RAM (~1GB) during processing.
                        </p>
                    </div>
                </div>

                {/* Model Info */}
                <div className="text-sm">
                    <span className="text-muted-foreground">Model: </span>
                    <span className="font-mono">{status.model}</span>
                </div>

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

                {/* Generate Button */}
                <button
                    onClick={triggerGeneration}
                    disabled={generating || !status.enabled}
                    className="w-full flex items-center justify-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                    <RefreshCw className={`h-4 w-4 ${generating ? 'animate-spin' : ''}`} />
                    {generating ? 'Processing...' : 'Generate Embeddings'}
                </button>

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
