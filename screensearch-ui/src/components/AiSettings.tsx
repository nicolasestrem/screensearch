import { useState } from 'react';
import { Settings, CheckCircle2, AlertCircle, Loader2, Cpu } from 'lucide-react';
import { useStore } from '../store/useStore';
import { aiApi } from '../api/ai';
import { toast } from 'react-hot-toast';

// Provider options
const PROVIDER_OPTIONS = [
    { value: 'local', label: 'Bundled Gemma 4 E4B (vision)', description: 'Optional on-device model for answers, reports, and screenshot understanding; not used for OCR or retrieval' },
    { value: 'http://localhost:11434/v1', label: 'Ollama-compatible', description: 'Local generation server on port 11434' },
    { value: 'custom', label: 'OpenAI-compatible', description: 'Remote or local generation endpoint' },
] as const;

export function AiSettings() {
    const { aiConfig, setAiConfig } = useStore();
    const [isTesting, setIsTesting] = useState(false);
    const [connectionStatus, setConnectionStatus] = useState<'none' | 'success' | 'error'>('none');
    const [customUrl, setCustomUrl] = useState(
        aiConfig.providerUrl !== 'local' &&
        aiConfig.providerUrl !== 'http://localhost:11434/v1'
            ? aiConfig.providerUrl
            : ''
    );

    const isLocal = aiConfig.providerUrl === 'local';
    const isCustom = aiConfig.providerUrl !== 'local' &&
                     aiConfig.providerUrl !== 'http://localhost:11434/v1';

    const handleProviderChange = (value: string) => {
        if (value === 'custom') {
            setAiConfig({ providerUrl: customUrl || 'http://localhost:1234/v1' });
        } else {
            setAiConfig({ providerUrl: value });
        }
        setConnectionStatus('none');
    };

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
        <div className="bg-paper border border-rule rounded-none p-6 space-y-6">
            <div className="flex items-center gap-2 pb-4 border-b border-rule">
                <Settings className="w-5 h-5 text-ink" />
                <h2 className="text-[18px] font-serif text-ink">Answer Generation Provider</h2>
            </div>

            <div className="space-y-4">
                {/* Provider Selection */}
                <div className="space-y-2">
                    <label className="text-sm font-medium">LLM Runtime</label>
                    <select
                        value={isCustom ? 'custom' : aiConfig.providerUrl}
                        onChange={(e) => handleProviderChange(e.target.value)}
                        className="w-full px-3 py-2 bg-paper-2 border border-rule rounded-none focus:outline-none focus:border-ink font-mono text-sm"
                    >
                        {PROVIDER_OPTIONS.map((option) => (
                            <option key={option.value} value={option.value}>
                                {option.label}
                            </option>
                        ))}
                    </select>
                    <p className="text-xs text-muted-foreground">
                        {PROVIDER_OPTIONS.find(o =>
                            isCustom ? o.value === 'custom' : o.value === aiConfig.providerUrl
                        )?.description}
                    </p>
                </div>

                {/* Local Provider Info */}
                {isLocal && (
                    <div className="flex items-center gap-3 p-4 bg-paper-2 border border-rule rounded-none">
                        <Cpu className="w-8 h-8 text-ink flex-shrink-0" />
                        <div>
                            <p className="font-serif text-[15px]">Gemma 4 E4B (Bundled, on-device vision + text)</p>
                            <p className="text-xs font-serif italic text-muted">
                                GPU-accelerated via Vulkan. Works on NVIDIA, AMD, and Intel GPUs.
                                Model runs entirely on your machine - no API key required.
                            </p>
                        </div>
                    </div>
                )}

                {/* Custom URL Input - only shown for custom provider */}
                {isCustom && (
                    <div className="space-y-2">
                        <label className="text-sm font-medium">Custom Provider URL</label>
                        <input
                            type="text"
                            value={aiConfig.providerUrl}
                            onChange={(e) => {
                                setCustomUrl(e.target.value);
                                setAiConfig({ providerUrl: e.target.value });
                            }}
                            placeholder="e.g., http://localhost:1234/v1"
                            className="w-full px-3 py-2 bg-paper-2 border border-rule rounded-none focus:outline-none focus:border-ink font-mono text-sm"
                        />
                        <p className="text-xs text-muted-foreground">
                            For LM Studio or other OpenAI-compatible servers. Ensure path ends with <code>/v1</code>.
                        </p>
                    </div>
                )}

                {/* API Key - hidden for local provider */}
                {!isLocal && (
                    <div className="space-y-2">
                        <label className="text-sm font-medium">API Key (Optional)</label>
                        <input
                            type="password"
                            value={aiConfig.apiKey}
                            onChange={(e) => setAiConfig({ apiKey: e.target.value })}
                            placeholder="sk-..."
                            className="w-full px-3 py-2 bg-paper-2 border border-rule rounded-none focus:outline-none focus:border-ink font-mono text-sm"
                        />
                    </div>
                )}

                {/* Model Name - hidden for local provider */}
                {!isLocal && (
                    <div className="space-y-2">
                        <label className="text-sm font-medium">Model Name</label>
                        <input
                            type="text"
                            value={aiConfig.model}
                            onChange={(e) => setAiConfig({ model: e.target.value })}
                            placeholder="e.g., llama3, gpt-4"
                            className="w-full px-3 py-2 bg-paper-2 border border-rule rounded-none focus:outline-none focus:border-ink font-mono text-sm"
                        />
                    </div>
                )}

                <div className="pt-2 flex items-center gap-4">
                    <button
                        onClick={handleTestConnection}
                        disabled={isTesting}
                        className="flex items-center gap-2 px-4 py-2 bg-paper hover:bg-paper-2 text-ink border border-rule rounded-none disabled:opacity-50 transition-colors"
                    >
                        {isTesting ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
                        {isLocal ? 'Check Local Model' : 'Test Connection'}
                    </button>

                    {connectionStatus === 'success' && (
                        <div className="flex items-center gap-2 text-green-500 text-sm">
                            <CheckCircle2 className="w-4 h-4" />
                            {isLocal ? 'Model Ready' : 'Connected'}
                        </div>
                    )}

                    {connectionStatus === 'error' && (
                        <div className="flex items-center gap-2 text-destructive text-sm">
                            <AlertCircle className="w-4 h-4" />
                            {isLocal ? 'Model Not Ready' : 'Connection Failed'}
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}
