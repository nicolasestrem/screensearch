import { useState, useEffect } from 'react';
import { Newspaper, RefreshCw, AlertCircle } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import { GlassCard, GlassCardHeader } from '../ui/GlassCard';
import { useStore } from '../../store/useStore';

interface DigestCache {
  content: string;
  generatedAt: Date;
  date: string;
}

// Cache key for session storage
const DIGEST_CACHE_KEY = 'screensearch_daily_digest';

export function DailyDigestCard() {
  const { aiConfig } = useStore();
  const [digest, setDigest] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const today = new Date().toISOString().split('T')[0] ?? '';

  // Check cache on mount
  useEffect(() => {
    const cached = sessionStorage.getItem(DIGEST_CACHE_KEY);
    if (cached) {
      try {
        const parsedCache: DigestCache = JSON.parse(cached);
        // Only use cache if it's from today
        if (parsedCache.date === today) {
          setDigest(parsedCache.content);
          return;
        }
      } catch {
        sessionStorage.removeItem(DIGEST_CACHE_KEY);
      }
    }
    // Auto-generate on mount if AI is configured
    if (aiConfig.providerUrl && aiConfig.model) {
      generateDigest();
    }
  }, [today, aiConfig.providerUrl, aiConfig.model]);

  const generateDigest = async () => {
    setLoading(true);
    setError(null);

    try {
      // Get today's date range
      const startOfDay = new Date();
      startOfDay.setHours(0, 0, 0, 0);
      const endOfDay = new Date();
      endOfDay.setHours(23, 59, 59, 999);

      const response = await fetch('/api/ai/generate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          provider_url: aiConfig.providerUrl,
          model: aiConfig.model,
          api_key: aiConfig.apiKey || undefined,
          start_time: startOfDay.toISOString(),
          end_time: endOfDay.toISOString(),
          prompt: `Generate a brief daily activity summary (3-5 bullet points) highlighting:
- Key tasks and activities completed
- Applications used most
- Notable patterns or focus areas
Keep it concise and actionable. Use bullet points.`,
          max_frames: 100,
        }),
      });

      if (!response.ok) {
        throw new Error('Failed to generate digest');
      }

      const data = await response.json() as { report?: string; summary?: string };
      const content: string = data.report || data.summary || 'No activity recorded today.';

      // Cache the result
      const cache: DigestCache = {
        content,
        generatedAt: new Date(),
        date: today,
      };
      sessionStorage.setItem(DIGEST_CACHE_KEY, JSON.stringify(cache));

      setDigest(content);
    } catch (err) {
      console.error('Failed to generate digest:', err);
      setError('Configure AI provider in settings to enable daily digest.');
      // STUB: Fallback to simulated data when AI not configured
      setDigest(null);
    } finally {
      setLoading(false);
    }
  };

  // MOCK SUMMARY if AI not configured (Visual Fidelity)
  if (!aiConfig.providerUrl || !aiConfig.model) {
     const mockDigest = `
- **Productivity Peak**: High activity detected in **VS Code** between 10:00 AM and 11:30 AM, focusing on frontend components.
- **Research Session**: Extensive browsing in **Chrome** regarding "React Performance Patterns" and "Canvas API" (14:15 - 15:00).
- **Communication**: Intermittent usage of **Slack** throughout the day, with rapid switching context.
- **System**: 450 new frames indexed. RAG memory updated.
     `;

    return (
      <GlassCard padding="lg" className="h-full">
        <GlassCardHeader
            icon={<Newspaper className="h-5 w-5" />}
            badge={<span className="badge-simulated">Daily Summary</span>}
        >
            Daily Digest
        </GlassCardHeader>
        <div className="prose prose-sm dark:prose-invert max-w-none">
          <ReactMarkdown
            components={{
              ul: ({ children }) => (
                <ul className="space-y-2 list-none pl-0">{children}</ul>
              ),
              li: ({ children }) => (
                <li className="flex items-start gap-2">
                   <div className="w-1.5 h-1.5 rounded-full bg-primary mt-2 flex-shrink-0 shadow-[0_0_8px_rgba(37,99,235,0.8)]" />
                   <span className="text-foreground/90">{children}</span>
                </li>
              ),
              strong: ({children}) => <span className="text-primary-light font-semibold">{children}</span>
            }}
          >
            {mockDigest}
          </ReactMarkdown>
        </div>
      </GlassCard>
    );
  }

  return (
    <GlassCard padding="lg" className="h-full">
      <GlassCardHeader
        icon={<Newspaper className="h-5 w-5" />}
        badge={
          <button
            onClick={generateDigest}
            disabled={loading}
            className="p-1.5 hover:bg-primary/10 rounded-lg transition-colors"
            title="Refresh digest"
          >
            <RefreshCw className={`h-4 w-4 text-primary ${loading ? 'animate-spin' : ''}`} />
          </button>
        }
      >
        Daily Digest
      </GlassCardHeader>

      {loading && !digest && (
        <div className="flex flex-col items-center justify-center py-8">
          <RefreshCw className="h-8 w-8 text-primary animate-spin" />
          <p className="text-sm text-muted-foreground mt-2">Generating summary...</p>
        </div>
      )}

      {error && !digest && (
        <div className="flex items-center gap-2 p-3 bg-destructive/10 rounded-lg">
          <AlertCircle className="h-4 w-4 text-destructive" />
          <p className="text-sm text-destructive">{error}</p>
        </div>
      )}

      {digest && (
        <div className="prose prose-sm dark:prose-invert max-w-none">
          <ReactMarkdown
            components={{
              ul: ({ children }) => (
                <ul className="space-y-2 list-none pl-0">{children}</ul>
              ),
              li: ({ children }) => (
                <li className="flex items-start gap-2">
                  <span className="w-1.5 h-1.5 rounded-full bg-primary mt-2 flex-shrink-0" />
                  <span>{children}</span>
                </li>
              ),
            }}
          >
            {digest}
          </ReactMarkdown>
        </div>
      )}
    </GlassCard>
  );
}
