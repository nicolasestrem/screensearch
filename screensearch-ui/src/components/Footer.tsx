import { Github, Globe } from 'lucide-react';

export function Footer() {
    return (
        <footer className="w-full py-6 mt-auto border-t border-border/40 bg-background/50 backdrop-blur-sm">
            <div className="container mx-auto px-4 flex flex-col md:flex-row items-center justify-between gap-4 text-xs text-muted-foreground">
                <div className="flex items-center gap-1">
                    <span>Created by</span>
                    <span className="font-medium text-foreground">Nicolas Estrem</span>
                </div>

                <div className="flex items-center gap-6">
                    <a
                        href="https://screensearch.app"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="flex items-center gap-2 hover:text-blue-500 transition-colors"
                    >
                        <Globe className="h-3 w-3" />
                        <span>screensearch.app</span>
                    </a>

                    <a
                        href="#"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="flex items-center gap-2 hover:text-foreground transition-colors"
                    >
                        <Github className="h-3 w-3" />
                        <span>Repository</span>
                    </a>
                </div>

                <div className="flex items-center text-[10px] text-muted-foreground/60">
                    <span>ScreenSearch v0.1.0</span>
                </div>
            </div>
        </footer>
    );
}
