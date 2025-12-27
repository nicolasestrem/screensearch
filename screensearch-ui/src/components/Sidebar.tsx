import { LayoutDashboard, Clock, FileText, Network, BarChart3, Settings } from 'lucide-react';
import { useStore } from '../store/useStore';
import { cn } from '../lib/utils';
import { Logo } from './Logo';

interface NavItem {
    id: string;
    label: string;
    icon: React.ElementType;
    comingSoon?: boolean;
}

export function Sidebar() {
    const { activeTab, setActiveTab, toggleSettingsPanel } = useStore();

    const mainNavItems: NavItem[] = [
        { id: 'dashboard', label: 'Dashboard', icon: LayoutDashboard },
        { id: 'timeline', label: 'Timeline', icon: Clock },
        { id: 'reports', label: 'Reports', icon: FileText },
    ];

    const aiFeatures: NavItem[] = [
        { id: 'knowledge-graph', label: 'Knowledge Graph', icon: Network, comingSoon: true },
        { id: 'analytics', label: 'Analytics', icon: BarChart3, comingSoon: true },
    ];

    const renderNavItem = (item: NavItem) => {
        const Icon = item.icon;
        const isActive = activeTab === item.id;
        const isDisabled = item.comingSoon;

        return (
            <button
                key={item.id}
                onClick={() => !isDisabled && setActiveTab(item.id as any)}
                disabled={isDisabled}
                className={cn(
                    "w-full flex items-center justify-between gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-200",
                    isActive && !isDisabled
                        ? "bg-primary text-primary-foreground shadow-md shadow-primary/20 glow-blue-subtle"
                        : isDisabled
                            ? "text-muted-foreground/50 cursor-not-allowed"
                            : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                )}
            >
                <div className="flex items-center gap-3">
                    <Icon className="h-4 w-4" />
                    <span>{item.label}</span>
                </div>
                {item.comingSoon && (
                    <span className="badge-coming-soon text-[10px] px-1.5">Soon</span>
                )}
            </button>
        );
    };

    return (
        <div className="h-screen w-64 glass-panel border-r border-border flex flex-col transition-all duration-300 z-10">
            <div className="p-6">
                <Logo />
            </div>

            <div className="flex-1 px-4 space-y-6 overflow-y-auto">
                {/* Main Navigation */}
                <div className="space-y-2">
                    <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3 px-2">
                        Menu
                    </div>
                    {mainNavItems.map(renderNavItem)}
                </div>

                {/* AI Features */}
                <div className="space-y-2">
                    <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3 px-2">
                        AI Features
                    </div>
                    {aiFeatures.map(renderNavItem)}
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
