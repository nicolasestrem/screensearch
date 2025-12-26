import { useState } from 'react';
import { X, Sparkles, Loader2, FileText, Copy, Check, MessageSquare } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import { useStore } from '../store/useStore';
import { cn } from '../lib/utils';
import { aiApi } from '../api/ai';
import { toast } from 'react-hot-toast';

export function AiAssistantPanel() {
  const { isAiPanelOpen, toggleAiPanel, aiConfig } = useStore();
  const [report, setReport] = useState<string | null>(null);
  const [isGenerating, setIsGenerating] = useState(false);
  const [customPrompt, setCustomPrompt] = useState('');
  const [isCopied, setIsCopied] = useState(false);

  // Auto-generate on open if empty? Maybe not, usually user wants to ask something or see summary.
  // We'll stick to manual trigger for now to avoid costs/delays.

  const handleGenerate = async (type: 'daily' | 'weekly' | 'custom' = 'daily') => {
    setIsGenerating(true);
    setReport(null);
    try {
      const now = new Date();
      const startTime = new Date();
      let prompt = undefined;

      if (type === 'daily') {
        startTime.setDate(now.getDate() - 1);
      } else if (type === 'weekly') {
        startTime.setDate(now.getDate() - 7);
      } else {
        startTime.setDate(now.getDate() - 1); // Default to last 24h context for custom
        prompt = customPrompt;
      }

      const result = await aiApi.generateReport({
        provider_url: aiConfig.providerUrl,
        api_key: aiConfig.apiKey || undefined,
        model: aiConfig.model,
        start_time: startTime.toISOString(),
        end_time: now.toISOString(),
        prompt: prompt,
      });

      setReport(result.report);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'Failed to generate report');
    } finally {
      setIsGenerating(false);
    }
  };

  const handleCopy = async () => {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(report);
      setIsCopied(true);
      setTimeout(() => setIsCopied(false), 2000);
    } catch (err) {
      toast.error('Failed to copy');
    }
  };

  return (
    <>
      {/* Backdrop */}
      {isAiPanelOpen && (
        <div 
          className="fixed inset-0 bg-black/20 backdrop-blur-[1px] z-40 transition-opacity duration-300"
          onClick={toggleAiPanel}
        />
      )}

      {/* Slide-over Panel */}
      <div 
        className={cn(
          "fixed top-0 right-0 h-full w-[450px] bg-card/95 backdrop-blur-xl border-l border-primary/20 shadow-2xl z-50 transition-transform duration-300 ease-in-out flex flex-col",
          isAiPanelOpen ? "translate-x-0" : "translate-x-full"
        )}
      >
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-border/50">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-primary/10 text-primary">
              <Sparkles className="h-5 w-5" />
            </div>
            <div>
              <h2 className="text-lg font-semibold">AI Assistant</h2>
              <p className="text-xs text-muted-foreground">Powered by {aiConfig.model}</p>
            </div>
          </div>
          <button 
            onClick={toggleAiPanel}
            className="p-2 hover:bg-secondary rounded-full transition-colors text-muted-foreground hover:text-foreground"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {/* Controls */}
          <div className="space-y-4">
            <div className="grid grid-cols-2 gap-3">
              <button
                onClick={() => handleGenerate('daily')}
                disabled={isGenerating}
                className="flex flex-col items-center justify-center p-4 rounded-xl border border-border hover:border-primary/50 hover:bg-primary/5 transition-all text-sm font-medium gap-2 disabled:opacity-50"
              >
                <div className="p-2 rounded-full bg-secondary">
                  <FileText className="h-4 w-4 text-foreground" />
                </div>
                <span>Daily Summary</span>
              </button>
              <button
                onClick={() => handleGenerate('weekly')}
                disabled={isGenerating}
                className="flex flex-col items-center justify-center p-4 rounded-xl border border-border hover:border-primary/50 hover:bg-primary/5 transition-all text-sm font-medium gap-2 disabled:opacity-50"
              >
                <div className="p-2 rounded-full bg-secondary">
                  <FileText className="h-4 w-4 text-foreground" />
                </div>
                <span>Weekly Update</span>
              </button>
            </div>

            <div className="relative">
              <textarea
                value={customPrompt}
                onChange={(e) => setCustomPrompt(e.target.value)}
                placeholder="Ask about your screen time..."
                className="w-full min-h-[80px] p-4 pr-12 rounded-xl bg-secondary/50 border border-border focus:border-primary/50 focus:ring-0 resize-none text-sm transition-all"
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    handleGenerate('custom');
                  }
                }}
              />
              <button
                onClick={() => handleGenerate('custom')}
                disabled={!customPrompt.trim() || isGenerating}
                className="absolute right-3 bottom-3 p-1.5 rounded-lg bg-primary text-primary-foreground disabled:opacity-50 hover:opacity-90 transition-opacity"
              >
                {isGenerating ? <Loader2 className="h-4 w-4 animate-spin" /> : <MessageSquare className="h-4 w-4" />}
              </button>
            </div>
          </div>

          {/* Result Area */}
          {(report || isGenerating) && (
            <div className="space-y-4 animate-in fade-in slide-in-from-bottom-4 duration-500">
               <div className="flex items-center justify-between">
                  <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider">Analysis Result</h3>
                  {report && (
                    <button
                      onClick={handleCopy}
                      className="p-1.5 hover:bg-secondary rounded-md text-muted-foreground hover:text-foreground transition-colors"
                      title="Copy"
                    >
                      {isCopied ? <Check className="h-3.5 w-3.5 text-green-500" /> : <Copy className="h-3.5 w-3.5" />}
                    </button>
                  )}
               </div>

               {isGenerating ? (
                  <div className="flex flex-col items-center justify-center py-12 space-y-4 text-muted-foreground">
                    <div className="relative">
                      <div className="absolute inset-0 bg-primary/20 blur-xl rounded-full" />
                      <Loader2 className="h-8 w-8 animate-spin text-primary relative z-10" />
                    </div>
                    <p className="text-sm animate-pulse">Analyzing visual memory...</p>
                  </div>
               ) : (
                 <div className="prose prose-sm dark:prose-invert max-w-none p-4 rounded-xl bg-secondary/30 border border-border/50">
                    <ReactMarkdown>{report || ''}</ReactMarkdown>
                 </div>
               )}
            </div>
          )}
        </div>
      </div>
    </>
  );
}
