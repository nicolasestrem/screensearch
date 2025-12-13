import { useState } from 'react';
import { FileText, Loader2, Sparkles, Copy, Download, Check } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import { aiApi } from '../api/ai';
import { toast } from 'react-hot-toast';
import { useStore } from '../store/useStore';

export function ReportGenerator() {
    const { aiConfig } = useStore();
    const [isGenerating, setIsGenerating] = useState(false);
    const [reportType, setReportType] = useState<'daily' | 'weekly' | 'custom'>('daily');
    const [customPrompt, setCustomPrompt] = useState('');
    const [report, setReport] = useState<string | null>(null);
    const [isCopied, setIsCopied] = useState(false);

    const handleGenerate = async () => {
        setIsGenerating(true);
        setReport(null);
        try {
            const now = new Date();
            const startTime = new Date();
            let prompt = undefined;

            if (reportType === 'daily') {
                startTime.setDate(now.getDate() - 1);
            } else if (reportType === 'weekly') {
                startTime.setDate(now.getDate() - 7);
            } else {
                startTime.setDate(now.getDate() - 1);
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
            toast.success('Report generated successfully');
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
            toast.success('Copied to clipboard');
            setTimeout(() => setIsCopied(false), 2000);
        } catch (err) {
            toast.error('Failed to copy');
        }
    };

    const handleSave = () => {
        if (!report) return;
        const blob = new Blob([report], { type: 'text/markdown' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `intelligence-report-${new Date().toISOString().split('T')[0]}.md`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
        toast.success('Report saved');
    };

    return (
        <div className="space-y-6">
            <div className="bg-card border border-border rounded-lg p-6 space-y-6">
                <div className="flex items-center gap-2 pb-4 border-b border-border">
                    <Sparkles className="w-5 h-5 text-primary" />
                    <h2 className="text-lg font-semibold">Generate Intelligence Report</h2>
                </div>

                <div className="space-y-4">
                    <div className="flex gap-4">
                        {(['daily', 'weekly', 'custom'] as const).map((type) => (
                            <button
                                key={type}
                                onClick={() => setReportType(type)}
                                className={`px-4 py-2 rounded-md border text-sm font-medium transition-colors capitalize ${reportType === type
                                    ? 'bg-primary text-primary-foreground border-primary'
                                    : 'bg-background hover:bg-accent text-muted-foreground border-input'
                                    }`}
                            >
                                {type} {type !== 'custom' && 'Summary'} {type === 'custom' && 'Prompt'}
                            </button>
                        ))}
                    </div>

                    {reportType === 'custom' && (
                        <textarea
                            value={customPrompt}
                            onChange={(e) => setCustomPrompt(e.target.value)}
                            placeholder="Ask a specific question about your activity or request a custom analysis..."
                            className="w-full h-24 px-3 py-2 bg-background border border-input rounded-md focus:outline-none focus:ring-2 focus:ring-primary resize-none"
                        />
                    )}

                    <button
                        onClick={handleGenerate}
                        disabled={isGenerating}
                        className="w-full py-3 bg-gradient-to-r from-primary to-purple-600 text-white font-semibold rounded-lg hover:opacity-90 transition-opacity disabled:opacity-50 flex items-center justify-center gap-2"
                    >
                        {isGenerating ? (
                            <>
                                <Loader2 className="w-5 h-5 animate-spin" />
                                Analyzing ScreenSearch...
                            </>
                        ) : (
                            <>
                                <Sparkles className="w-5 h-5" />
                                Generate Report
                            </>
                        )}
                    </button>
                </div>
            </div>

            {report && (
                <div className="bg-card border border-border rounded-lg shadow-lg animate-in fade-in slide-in-from-bottom-4 duration-500 overflow-hidden">
                    <div className="flex items-center justify-between p-4 border-b border-border bg-muted/30">
                        <div className="flex items-center gap-2">
                            <FileText className="w-5 h-5 text-primary" />
                            <h3 className="text-lg font-semibold">Intelligence Report</h3>
                        </div>
                        <div className="flex items-center gap-2">
                            <button
                                onClick={handleCopy}
                                className="p-2 hover:bg-background rounded-md transition-colors text-muted-foreground hover:text-foreground"
                                title="Copy to clipboard"
                            >
                                {isCopied ? <Check className="w-4 h-4 text-green-500" /> : <Copy className="w-4 h-4" />}
                            </button>
                            <button
                                onClick={handleSave}
                                className="p-2 hover:bg-background rounded-md transition-colors text-muted-foreground hover:text-foreground"
                                title="Save as Markdown"
                            >
                                <Download className="w-4 h-4" />
                            </button>
                        </div>
                    </div>
                    <div className="p-6 overflow-x-auto prose prose-sm dark:prose-invert max-w-none prose-headings:font-bold prose-h1:text-xl prose-h2:text-lg prose-p:leading-relaxed prose-pre:bg-muted prose-pre:p-4 prose-pre:rounded-lg">
                        <ReactMarkdown>
                            {report}
                        </ReactMarkdown>
                    </div>
                </div>
            )}
        </div>
    );
}
