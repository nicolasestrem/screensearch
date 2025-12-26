import { Clock, Settings, Brain } from 'lucide-react';
import { useStore } from '../store/useStore';
import { cn } from '../lib/utils';
import { Logo } from './Logo';
import { TagControlTower } from './TagControlTower';

export function Sidebar() {
    const { activeTab, setActiveTab, toggleSettingsPanel, isAiPanelOpen, toggleAiPanel } = useStore();

    return (
        <div className="h-screen w-64 bg-card border-r border-border flex flex-col transition-all duration-300 z-10 shadow-[4px_0_24px_-12px_rgba(0,0,0,0.1)]">
            <div className="p-6">
                <Logo />
            </div>

            <div className="flex-1 px-4 space-y-2 overflow-y-auto custom-scrollbar">

                <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-4 px-2">
                    Menu
                </div>
                
                {/* Timeline Tab */}
                <button
                    onClick={() => setActiveTab('timeline')}
                    className={cn(
                        "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-200",
                        activeTab === 'timeline'
                            ? "bg-primary text-primary-foreground shadow-md shadow-primary/20"
                            : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                    )}
                >
                    <Clock className="h-4 w-4" />
                    <span>Timeline</span>
                </button>

                {/* Intelligence Action */}
                <button
                    onClick={() => toggleAiPanel()}
                    className={cn(
                        "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-200 mt-1",
                        isAiPanelOpen
                            ? "bg-primary text-primary-foreground shadow-md shadow-primary/20"
                            : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                    )}
                >
                    <Brain className="h-4 w-4" />
                    <span>Intelligence</span>
                </button>

                <div className="pt-6 pb-4">
                    <TagControlTower />
                </div>
            </div>

            <div className="p-4 border-t border-border mt-auto">
                <button
                    onClick={toggleSettingsPanel}
                    className="w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium text-muted-foreground hover:bg-secondary hover:text-foreground transition-all duration-200"
                >
                    <Settings className="h-4 w-4" />
                    <span>Settings</span>
                </button>
            </div>
        </div>
    );
}
