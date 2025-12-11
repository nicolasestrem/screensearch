import { useState } from 'react';
import { Settings, CheckCircle2, AlertCircle, Loader2 } from 'lucide-react';
import { useStore } from '../store/useStore';
import { aiApi } from '../api/ai';
import { toast } from 'react-hot-toast';

export function AiSettings() {
    const { aiConfig, setAiConfig } = useStore();
    const [isTesting, setIsTesting] = useState(false);
    const [connectionStatus, setConnectionStatus] = useState<'none' | 'success' | 'error'>('none');

    const handleTestConnection = async () => {
        setIsTesting(true);
        setConnectionStatus('none');
        try {
            const result = await aiApi.validateConnection({
                provider_url: aiConfig.providerUrl,
                api_key: aiConfig.apiKey || undefined,
                model: aiConfig.model,
            });

            if (result.success) {
                setConnectionStatus('success');
                toast.success(result.message);
            } else {
                setConnectionStatus('error');
                toast.error(result.message);
            }
        } catch (error) {
            setConnectionStatus('error');
            toast.error(error instanceof Error ? error.message : 'Connection failed');
        } finally {
            setIsTesting(false);
        }
    };

    return (
        <div className="bg-card border border-border rounded-lg p-6 space-y-6">
            <div className="flex items-center gap-2 pb-4 border-b border-border">
                <Settings className="w-5 h-5 text-primary" />
                <h2 className="text-lg font-semibold">AI Provider Settings</h2>
            </div>

            <div className="space-y-4">
                <div className="space-y-2">
                    <label className="text-sm font-medium">Provider URL</label>
                    <input
                        type="text"
                        value={aiConfig.providerUrl}
                        onChange={(e) => setAiConfig({ providerUrl: e.target.value })}
                        placeholder="e.g., http://localhost:11434/v1"
                        className="w-full px-3 py-2 bg-background border border-input rounded-md focus:outline-none focus:ring-2 focus:ring-primary"
                    />
                    <p className="text-xs text-muted-foreground">
                        For Ollama or LM Studio, ensure path ends with <code>/v1</code> (e.g. <code>http://localhost:1234/v1</code>).
                    </p>
                </div>

                <div className="space-y-2">
                    <label className="text-sm font-medium">API Key (Optional)</label>
                    <input
                        type="password"
                        value={aiConfig.apiKey}
                        onChange={(e) => setAiConfig({ apiKey: e.target.value })}
                        placeholder="sk-..."
                        className="w-full px-3 py-2 bg-background border border-input rounded-md focus:outline-none focus:ring-2 focus:ring-primary"
                    />
                </div>

                <div className="space-y-2">
                    <label className="text-sm font-medium">Model Name</label>
                    <input
                        type="text"
                        value={aiConfig.model}
                        onChange={(e) => setAiConfig({ model: e.target.value })}
                        placeholder="e.g., llama3, gpt-4"
                        className="w-full px-3 py-2 bg-background border border-input rounded-md focus:outline-none focus:ring-2 focus:ring-primary"
                    />
                </div>

                <div className="pt-2 flex items-center gap-4">
                    <button
                        onClick={handleTestConnection}
                        disabled={isTesting}
                        className="flex items-center gap-2 px-4 py-2 bg-secondary text-secondary-foreground rounded-md hover:bg-secondary/80 disabled:opacity-50 transition-colors"
                    >
                        {isTesting ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
                        Test Connection
                    </button>

                    {connectionStatus === 'success' && (
                        <div className="flex items-center gap-2 text-green-500 text-sm">
                            <CheckCircle2 className="w-4 h-4" />
                            Connected
                        </div>
                    )}

                    {connectionStatus === 'error' && (
                        <div className="flex items-center gap-2 text-destructive text-sm">
                            <AlertCircle className="w-4 h-4" />
                            Connection Failed
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}
