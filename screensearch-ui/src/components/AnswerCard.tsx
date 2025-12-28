
import { useState, useEffect } from 'react';
import { Sparkles, RefreshCcw, AlertTriangle } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import { apiClient } from '../api/client';
import { useStore } from '../store/useStore';

export function AnswerCard() {
    const { filters } = useStore();
    const query = filters.searchQuery;
    const [answer, setAnswer] = useState<string | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [lastQuery, setLastQuery] = useState<string>('');

    // Reset when query changes significantly (optional, or just rely on manual trigger)
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
        } catch (err: any) {
            console.error("Failed to generate answer:", err);
            setError("Failed to generate answer. Ensure the Vision Engine is enabled in settings.");
        } finally {
            setLoading(false);
        }
    };

    if (!query) return null;

    return (
        <div className="w-full max-w-4xl mx-auto mb-6 animate-in slide-in-from-top-4 duration-500">
            <div className="bg-card/50 backdrop-blur-sm border border-border/50 rounded-xl p-6 shadow-sm">
                <div className="flex items-start justify-between gap-4">

                    <div className="flex-1 space-y-4">
                        <div className="flex items-center gap-2 text-primary">
                            <Sparkles className="h-5 w-5" />
                            <h3 className="font-semibold">AI Intelligence</h3>
                        </div>

                        {!answer && !loading && !error && (
                            <div className="flex flex-col gap-2">
                                <p className="text-muted-foreground text-sm">
                                    Generate a concise answer based on your search results.
                                </p>
                                <button
                                    onClick={handleGenerate}
                                    className="self-start px-4 py-2 bg-primary/10 hover:bg-primary/20 text-primary rounded-lg text-sm font-medium transition-colors flex items-center gap-2"
                                >
                                    <Sparkles className="h-4 w-4" />
                                    Generate Answer
                                </button>
                            </div>
                        )}

                        {loading && (
                            <div className="flex items-center gap-2 text-muted-foreground">
                                <RefreshCcw className="h-4 w-4 animate-spin" />
                                <span className="text-sm">Analyzing screen history...</span>
                            </div>
                        )}

                        {error && (
                            <div className="flex items-center gap-2 text-destructive bg-destructive/10 p-3 rounded-lg">
                                <AlertTriangle className="h-4 w-4" />
                                <span className="text-sm font-medium">{error}</span>
                            </div>
                        )}

                        {answer && (
                            <div className="prose prose-sm dark:prose-invert max-w-none">
                                <ReactMarkdown>{answer}</ReactMarkdown>
                            </div>
                        )}
                    </div>

                    {answer && (
                        <button
                            onClick={handleGenerate}
                            disabled={loading}
                            className="p-2 hover:bg-secondary rounded-lg text-muted-foreground transition-colors"
                            title="Regenerate"
                        >
                            <RefreshCcw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
                        </button>
                    )}
                </div>
            </div>
        </div>
    );
}
