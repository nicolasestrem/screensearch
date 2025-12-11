import { Activity } from 'lucide-react';

export function Logo() {
    return (
        <div className="flex items-center gap-3 group select-none">
            <div className="relative w-10 h-10 rounded-xl bg-gradient-to-tr from-primary to-violet-500 shadow-lg shadow-primary/20 flex items-center justify-center overflow-hidden transition-transform group-hover:scale-105 duration-300">
                <div className="absolute inset-0 bg-white/20 opacity-0 group-hover:opacity-100 transition-opacity" />
                <div className="absolute -inset-1 bg-gradient-to-r from-transparent via-white/30 to-transparent -translate-x-full group-hover:animate-[shimmer_1.5s_infinite]" />
                <Activity className="h-5 w-5 text-white relative z-10" />
            </div>
            <div className="flex flex-col">
                <h1 className="text-lg font-bold tracking-tight bg-clip-text text-transparent bg-gradient-to-r from-foreground to-foreground/70 group-hover:to-primary transition-all duration-300">
                    Screen Memory
                </h1>
                <p className="text-[10px] uppercase tracking-wider font-semibold text-muted-foreground/80 group-hover:text-primary/70 transition-colors">
                    Capture History
                </p>
            </div>
        </div>
    );
}
