import { useState, useEffect, useMemo } from 'react';
import { Sparkles, RefreshCcw, AlertTriangle, ChevronDown, ChevronUp } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import { GlassCard, GlassCardHeader } from '../ui/GlassCard';
import { ActivityList, ActivityItem } from './ActivityList';
import { apiClient } from '../../api/client';
import { useStore } from '../../store/useStore';
import { useFrames } from '../../hooks/useFrames';
import type { Frame } from '../../types';

export function SmartAnswerCard() {
  const { filters } = useStore();
  const query = filters.searchQuery;
  const [answer, setAnswer] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastQuery, setLastQuery] = useState<string>('');
  const [showActivities, setShowActivities] = useState(true);

  // Get frames to build activity list
  const { data: framesData } = useFrames();

  // Build activity list from frames
  const activities = useMemo<ActivityItem[]>(() => {
    const frames = framesData?.data || [];
    if (frames.length === 0) {
      return [];
    }

    // Group frames by app name
    const appGroups = new Map<string, { frames: Frame[]; latestTime: Date }>();

    frames.forEach((frame: Frame) => {
      const appName = frame.app_name || 'Unknown';
      const existing = appGroups.get(appName);
      const frameTime = new Date(frame.timestamp);

      if (existing) {
        existing.frames.push(frame);
        if (frameTime > existing.latestTime) {
          existing.latestTime = frameTime;
        }
      } else {
        appGroups.set(appName, { frames: [frame], latestTime: frameTime });
      }
    });

    // Convert to activity items, sorted by most recent
    return Array.from(appGroups.entries())
      .map(([appName, data]) => ({
        appName,
        description: data.frames[0]?.window_name || 'No window title',
        timestamp: data.latestTime,
        count: data.frames.length,
      }))
      .sort((a, b) => b.timestamp.getTime() - a.timestamp.getTime())
      .slice(0, 5); // Show top 5
  }, [framesData]);

  // Reset when query changes
  useEffect(() => {
    if (query !== lastQuery) {
      setAnswer(null);
      setError(null);
      setLastQuery(query);
    }
  }, [query, lastQuery]);

  const handleGenerate = async () => {
    if (!query) return;

    setLoading(true);
    setError(null);

    try {
      const response = await apiClient.generateAnswer(query);
      setAnswer(response.answer);
    } catch (err: unknown) {
      console.error('Failed to generate answer:', err);
      setError('Configure AI provider in settings to enable Smart Answers.');
    } finally {
      setLoading(false);
    }
  };

  if (!query) return null;

  return (
    <div className="w-full max-w-4xl mx-auto mb-6 animate-fade-in-up">
      <GlassCard padding="lg">
        <GlassCardHeader
          icon={<Sparkles className="h-5 w-5" />}
          badge={
            answer && (
              <button
                onClick={handleGenerate}
                disabled={loading}
                className="p-1.5 hover:bg-primary/10 rounded-lg transition-colors"
                title="Regenerate"
              >
                <RefreshCcw className={`h-4 w-4 text-primary ${loading ? 'animate-spin' : ''}`} />
              </button>
            )
          }
        >
          Smart Answer
        </GlassCardHeader>

        <div className="space-y-4">
          {/* Generate prompt */}
          {!answer && !loading && !error && (
            <div className="flex flex-col gap-3">
              <p className="text-muted-foreground text-sm">
                Generate an AI-powered answer based on your screen history for: <span className="text-foreground font-medium">"{query}"</span>
              </p>
              <button
                onClick={handleGenerate}
                className="self-start px-4 py-2 bg-primary/10 hover:bg-primary/20 text-primary rounded-lg text-sm font-medium transition-colors flex items-center gap-2 glow-blue-subtle"
              >
                <Sparkles className="h-4 w-4" />
                Generate Smart Answer
              </button>
            </div>
          )}

          {/* Loading state */}
          {loading && (
            <div className="flex items-center gap-3 py-4">
              <RefreshCcw className="h-5 w-5 text-primary animate-spin" />
              <span className="text-muted-foreground">Analyzing your screen history...</span>
            </div>
          )}

          {/* Error state */}
          {error && (
            <div className="flex items-center gap-2 p-3 bg-destructive/10 rounded-lg">
              <AlertTriangle className="h-4 w-4 text-destructive" />
              <span className="text-sm text-destructive">{error}</span>
            </div>
          )}

          {/* Answer */}
          {answer && (
            <div className="prose prose-sm dark:prose-invert max-w-none">
              <ReactMarkdown>{answer}</ReactMarkdown>
            </div>
          )}

          {/* Activity list */}
          {activities.length > 0 && (
            <div className="border-t border-border/50 pt-4 mt-4">
              <button
                onClick={() => setShowActivities(!showActivities)}
                className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
              >
                {showActivities ? (
                  <ChevronUp className="h-4 w-4" />
                ) : (
                  <ChevronDown className="h-4 w-4" />
                )}
                Related Activity ({activities.length})
              </button>

              {showActivities && <ActivityList items={activities} />}
            </div>
          )}
        </div>
      </GlassCard>
    </div>
  );
}
