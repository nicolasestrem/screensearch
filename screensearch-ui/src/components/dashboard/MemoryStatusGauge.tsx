import { useState, useEffect } from 'react';
import { Brain, RefreshCw } from 'lucide-react';
import { GlassCard, GlassCardHeader } from '../ui/GlassCard';
import { CircularGauge } from '../ui/CircularGauge';

interface EmbeddingStatus {
  enabled: boolean;
  model: string;
  total_frames: number;
  frames_with_embeddings: number;
  coverage_percent: number;
  last_processed_frame_id: number;
}

export function MemoryStatusGauge() {
  const [status, setStatus] = useState<EmbeddingStatus | null>(null);
  const [loading, setLoading] = useState(true);

  const fetchStatus = async () => {
    try {
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
    // Refresh every 30 seconds
    const interval = setInterval(fetchStatus, 30000);
    return () => clearInterval(interval);
  }, []);

  if (loading) {
    return (
      <GlassCard className="flex flex-col items-center justify-center min-h-[280px]">
        <RefreshCw className="h-8 w-8 text-primary animate-spin" />
        <p className="text-sm text-muted-foreground mt-2">Loading memory status...</p>
      </GlassCard>
    );
  }

  // If no status or embeddings disabled, show placeholder
  if (!status) {
    return (
      <GlassCard className="flex flex-col items-center justify-center min-h-[280px]">
        <Brain className="h-12 w-12 text-muted-foreground/50 mb-3" />
        <p className="text-sm text-muted-foreground">Memory status unavailable</p>
      </GlassCard>
    );
  }

  return (
    <GlassCard padding="lg">
      <GlassCardHeader icon={<Brain className="h-5 w-5" />}>
        Memory Status
      </GlassCardHeader>

      <div className="flex flex-col items-center">
        <CircularGauge
          value={status.coverage_percent}
          size={160}
          label="Indexed"
          sublabel={status.enabled ? 'RAG Active' : 'RAG Disabled'}
        />

        <div className="mt-4 w-full space-y-2 text-sm">
          <div className="flex justify-between text-muted-foreground">
            <span>Total Frames</span>
            <span className="font-medium text-foreground">
              {status.total_frames.toLocaleString()}
            </span>
          </div>
          <div className="flex justify-between text-muted-foreground">
            <span>Indexed</span>
            <span className="font-medium text-foreground">
              {status.frames_with_embeddings.toLocaleString()}
            </span>
          </div>
          <div className="flex justify-between text-muted-foreground">
            <span>Model</span>
            <span className="font-mono text-xs text-primary truncate max-w-[120px]">
              {status.model.split('/').pop()}
            </span>
          </div>
        </div>

        {/* Status indicator */}
        <div className="mt-4 flex items-center gap-2">
          <div
            className={`w-2 h-2 rounded-full ${
              status.enabled ? 'bg-green-500 animate-pulse' : 'bg-muted-foreground'
            }`}
          />
          <span className="text-xs text-muted-foreground">
            {status.enabled ? 'Semantic search enabled' : 'Using keyword search'}
          </span>
        </div>
      </div>
    </GlassCard>
  );
}
