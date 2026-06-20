import { useEffect, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Loader2, CheckCircle2, Download, AlertCircle, CircleDashed, X } from 'lucide-react';

type StageState =
    | 'ready'
    | 'initializing'
    | 'loading'
    | 'downloading'
    | 'needs_setup'
    | 'disabled';

interface ReadinessStage {
    id: string;
    label: string;
    state: StageState;
    detail: string;
    progress?: number | null;
    eta_seconds?: number | null;
}

interface ReadinessResponse {
    core_ready: boolean;
    all_ready: boolean;
    stages: ReadinessStage[];
}

function formatEta(seconds?: number | null): string {
    if (seconds == null || seconds <= 0) return '';
    if (seconds < 60) return `${seconds}s left`;
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return s > 0 ? `${m}m ${s}s left` : `${m}m left`;
}

function StageIcon({ state }: { state: StageState }) {
    switch (state) {
        case 'ready':
            return <CheckCircle2 className="h-4 w-4 text-green-500 flex-shrink-0" />;
        case 'downloading':
            return <Download className="h-4 w-4 text-primary flex-shrink-0 animate-pulse" />;
        case 'initializing':
        case 'loading':
            return <Loader2 className="h-4 w-4 text-primary flex-shrink-0 animate-spin" />;
        case 'needs_setup':
            return <AlertCircle className="h-4 w-4 text-amber-500 flex-shrink-0" />;
        case 'disabled':
        default:
            return <CircleDashed className="h-4 w-4 text-muted-foreground flex-shrink-0" />;
    }
}

/**
 * Startup readiness banner.
 *
 * Polls `/api/system/readiness` and, while the backend is warming up (loading
 * the search model, downloading/loading the local AI server, etc.), shows a
 * non-blocking banner that explains in plain language what's happening and —
 * for tracked downloads — roughly how long is left. It only appears when there
 * is actual warm-up to report (so a fast, fully-cached launch shows nothing),
 * and auto-dismisses shortly after everything is ready.
 */
export function StartupStatus() {
    const [dismissed, setDismissed] = useState(
        () => sessionStorage.getItem('startupStatusDismissed') === '1'
    );
    // Only surface the banner once we've actually observed warm-up, so a launch
    // where everything is already cached never flashes a banner.
    const [sawWarmup, setSawWarmup] = useState(false);

    const { data } = useQuery<ReadinessResponse>({
        queryKey: ['system-readiness'],
        queryFn: async () => {
            const res = await fetch('/api/system/readiness');
            if (!res.ok) throw new Error(`readiness ${res.status}`);
            return res.json();
        },
        enabled: !dismissed,
        // Poll while warming up; stop once everything is ready.
        refetchInterval: (query) => (query.state.data?.all_ready ? false : 1500),
        // Local endpoint: retry quickly during the brief window before the API
        // binds, rather than the default long exponential backoff.
        retry: 3,
        retryDelay: 500,
    });

    useEffect(() => {
        if (data && (!data.core_ready || !data.all_ready)) {
            setSawWarmup(true);
        }
    }, [data]);

    // Once ready, hold the "all set" confirmation briefly, then dismiss.
    useEffect(() => {
        if (data?.all_ready && sawWarmup && !dismissed) {
            const t = setTimeout(() => setDismissed(true), 3000);
            return () => clearTimeout(t);
        }
    }, [data?.all_ready, sawWarmup, dismissed]);

    const dismiss = () => {
        sessionStorage.setItem('startupStatusDismissed', '1');
        setDismissed(true);
    };

    if (dismissed || !data) return null;
    // Nothing to report (fully cached launch): stay invisible.
    if (!sawWarmup && data.all_ready) return null;

    const title = !data.core_ready
        ? 'Starting ScreenSearch…'
        : data.all_ready
            ? 'ScreenSearch is ready'
            : 'Getting AI features ready…';

    return (
        <div className="fixed top-4 left-1/2 -translate-x-1/2 z-50 w-[min(92vw,460px)]">
            <div className="bg-card/95 backdrop-blur border border-border rounded-xl shadow-lg p-4 space-y-3">
                <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                        {data.all_ready ? (
                            <CheckCircle2 className="h-4 w-4 text-green-500" />
                        ) : (
                            <Loader2 className="h-4 w-4 text-primary animate-spin" />
                        )}
                        <span className="font-medium text-sm">{title}</span>
                    </div>
                    <button
                        onClick={dismiss}
                        className="text-muted-foreground hover:text-foreground"
                        aria-label="Dismiss startup status"
                    >
                        <X className="h-4 w-4" />
                    </button>
                </div>

                {!data.core_ready && (
                    <p className="text-xs text-muted-foreground">
                        You can start searching as soon as core services finish starting.
                    </p>
                )}

                <ul className="space-y-2">
                    {data.stages.map((stage) => {
                        const pct =
                            stage.state === 'downloading' && stage.progress != null
                                ? Math.min(Math.max(stage.progress, 0), 100)
                                : null;
                        const eta = formatEta(stage.eta_seconds);
                        return (
                            <li key={stage.id} className="flex items-start gap-2.5">
                                <span className="mt-0.5">
                                    <StageIcon state={stage.state} />
                                </span>
                                <div className="min-w-0 flex-1">
                                    <div className="flex items-center justify-between gap-2">
                                        <span
                                            className={`text-sm ${
                                                stage.state === 'disabled'
                                                    ? 'text-muted-foreground'
                                                    : 'text-foreground'
                                            }`}
                                        >
                                            {stage.label}
                                        </span>
                                        {pct != null && (
                                            <span className="text-xs font-mono text-muted-foreground whitespace-nowrap">
                                                {pct.toFixed(0)}%{eta ? ` · ${eta}` : ''}
                                            </span>
                                        )}
                                    </div>
                                    <p className="text-xs text-muted-foreground leading-snug">
                                        {stage.detail}
                                    </p>
                                    {pct != null && (
                                        <div className="w-full bg-secondary rounded-full h-1.5 overflow-hidden mt-1.5">
                                            <div
                                                className="h-full bg-primary transition-all duration-300"
                                                style={{ width: `${pct}%` }}
                                            />
                                        </div>
                                    )}
                                </div>
                            </li>
                        );
                    })}
                </ul>
            </div>
        </div>
    );
}
