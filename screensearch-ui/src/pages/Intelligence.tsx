import { AiSettings } from '../components/AiSettings';
import { ReportGenerator } from '../components/ReportGenerator';

export function IntelligencePage() {
    return (
        <div className="max-w-6xl mx-auto space-y-8 animate-in fade-in duration-500">
            <div className="space-y-2">
                <h1 className="font-serif text-[34px] lg:text-[42px] leading-[1.08] tracking-[-0.01em] text-ink">
                    Intelligence
                </h1>
                <p className="text-ink-2 text-[16px] leading-[1.6]">
                    Generate insights and summaries from your ScreenSearch data using AI.
                </p>
            </div>

            <div className="grid gap-8 md:grid-cols-[400px_1fr]">
                <div className="space-y-6">
                    <AiSettings />

                    <div className="bg-paper-2 border border-rule p-4 rounded-none text-sm font-serif italic text-muted">
                        <p>
                            <strong className="font-sans font-bold">Privacy Note:</strong> When generating reports, summaries of your screen metadata
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
