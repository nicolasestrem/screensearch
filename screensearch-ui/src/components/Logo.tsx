import { Cpu } from 'lucide-react';
import { cn } from '../lib/utils';

interface LogoProps {
    collapsed?: boolean;
}

export function Logo({ collapsed = false }: LogoProps) {
    return (
        <div className="flex items-center gap-3 group select-none cursor-pointer">
            {/* Logo Icon with Cyan Glow */}
            <div className="relative w-10 h-10 rounded-xl bg-primary/20 flex items-center justify-center overflow-hidden transition-all duration-300 group-hover:scale-105 glow-cyan-subtle">
                <div className="absolute inset-0 bg-white/10 opacity-0 group-hover:opacity-100 transition-opacity" />
                <Cpu className="h-5 w-5 text-primary relative z-10" />
            </div>

            {/* Text - Hidden when collapsed */}
            <div
                className={cn(
                    "flex flex-col overflow-hidden transition-all duration-300",
                    collapsed ? "w-0 opacity-0" : "w-auto opacity-100"
                )}
            >
                <div className="flex items-center gap-1.5 whitespace-nowrap">
                    <span className="text-lg font-bold font-display tracking-tight text-foreground">ScreenSearch</span>
                </div>
            </div>
        </div>
    );
}
