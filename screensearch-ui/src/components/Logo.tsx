import { Activity } from 'lucide-react';

export function Logo() {
    return (
        <div className="flex items-center gap-3 group select-none cursor-pointer">
            <div className="relative w-10 h-10 rounded-xl bg-gradient-to-tr from-blue-600 to-cyan-500 shadow-lg shadow-blue-500/20 flex items-center justify-center overflow-hidden transition-transform group-hover:scale-105 duration-300">
                <div className="absolute inset-0 bg-white/20 opacity-0 group-hover:opacity-100 transition-opacity" />
                <div className="absolute -inset-1 bg-gradient-to-r from-transparent via-white/30 to-transparent -translate-x-full group-hover:animate-[shimmer_1.5s_infinite]" />
                <Activity className="h-5 w-5 text-white relative z-10" />
            </div>

            <span className="text-slate-400 group-hover:text-blue-600 transition-colors duration-300">|</span>

            <div className="flex flex-col">
                <h1 className="text-lg font-bold tracking-tight text-slate-900 dark:text-slate-100 group-hover:text-blue-600 transition-colors duration-300">
                    ScreenSearch
                </h1>
                <p className="text-[10px] uppercase tracking-wider font-semibold text-slate-500 group-hover:text-blue-600/70 transition-colors">
                    Search History
                </p>
            </div>
        </div>
    );
}
