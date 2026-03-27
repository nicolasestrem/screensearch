import { AiSettings } from '../components/AiSettings';
import { ReportGenerator } from '../components/ReportGenerator';

export function IntelligencePage() {
    return (
        <div className="max-w-6xl mx-auto space-y-8 animate-in fade-in duration-500">
            <div className="space-y-2">
                <h1 className="text-3xl font-bold tracking-tight bg-gradient-to-r from-primary to-purple-600 bg-clip-text text-transparent">
                    Intelligence
                </h1>
                <p className="text-muted-foreground">
                    Generate insights and summaries from your ScreenSearch data using AI.
                </p>
            </div>

            <div className="grid gap-8 md:grid-cols-[400px_1fr]">
                <div className="space-y-6">
                    <AiSettings />

                    <div className="bg-muted/30 p-4 rounded-lg text-sm text-muted-foreground">
                        <p>
                            <strong>Privacy Note:</strong> When generating reports, summaries of your screen metadata
                            (app names, window titles, OCR text) will be sent to the configured AI provider.
                            Local providers (like Ollama) keep data on your device.
                        </p>
                    </div>
                </div>

                <div>
                    <ReportGenerator />
                </div>
            </div>
        </div>
    );
}
