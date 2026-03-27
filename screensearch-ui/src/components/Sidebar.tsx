import { motion, AnimatePresence } from 'framer-motion';
import { LayoutDashboard, Clock, FileText, Network, BarChart3, Settings, ChevronLeft, Search, Command } from 'lucide-react';
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
    const {
        activeTab,
        setActiveTab,
        toggleSettingsPanel,
        openSearchModal,
        isSidebarCollapsed,
        toggleSidebar
    } = useStore();

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
                onClick={() => !isDisabled && setActiveTab(item.id as 'dashboard' | 'timeline' | 'reports')}
                disabled={isDisabled}
                title={isSidebarCollapsed ? item.label : undefined}
                className={cn(
                    "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-300 relative overflow-hidden group",
                    isSidebarCollapsed && "justify-center",
                    isActive && !isDisabled
                        ? "active-border bg-primary/10 text-primary"
                        : isDisabled
                            ? "text-muted-foreground/50 cursor-not-allowed"
                            : "text-muted-foreground hover:bg-surface-2 hover:text-foreground hover:glow-cyan-subtle"
                )}
            >
                {/* Active Indicator Glimmer */}
                {isActive && !isDisabled && (
                  <div className="absolute inset-0 bg-gradient-to-r from-transparent via-primary/10 to-transparent translate-x-[-100%] animate-[shimmer_2s_infinite]" />
                )}
                
                <Icon className={cn("h-4 w-4 flex-shrink-0 transition-colors duration-300", isActive && "text-primary drop-shadow-[0_0_8px_rgba(0,242,255,0.6)]")} />

                <AnimatePresence mode="wait">
                    {!isSidebarCollapsed && (
                        <motion.div
                            initial={{ opacity: 0, width: 0 }}
                            animate={{ opacity: 1, width: 'auto' }}
                            exit={{ opacity: 0, width: 0 }}
                            transition={{ duration: 0.2 }}
                            className="flex items-center justify-between flex-1 overflow-hidden"
                        >
                            <span className={cn("whitespace-nowrap transition-all duration-300", isActive && "neon-text")}>{item.label}</span>
                            {item.comingSoon && (
                                <span className="badge-coming-soon text-[10px] px-1.5 ml-2">Soon</span>
                            )}
                        </motion.div>
                    )}
                </AnimatePresence>
            </button>
        );
    };

    return (
        <motion.div
            animate={{ width: isSidebarCollapsed ? 72 : 256 }}
            transition={{ duration: 0.3, ease: [0.4, 0, 0.2, 1] }}
            className="h-screen glass-panel border-r border-border flex flex-col z-10"
        >
            {/* Logo */}
            <div className={cn(
                "p-4 flex items-center",
                isSidebarCollapsed ? "justify-center" : "px-6"
            )}>
                <Logo collapsed={isSidebarCollapsed} />
            </div>

            {/* Search Shortcut */}
            <div className={cn("px-4 mb-4", isSidebarCollapsed && "px-2")}>
                <button
                    onClick={openSearchModal}
                    title={isSidebarCollapsed ? "Search (⌘K)" : undefined}
                    className={cn(
                        "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm",
                        "bg-surface-1 hover:bg-surface-2 border border-border hover:border-primary/30",
                        "text-muted-foreground hover:text-foreground transition-all",
                        isSidebarCollapsed && "justify-center"
                    )}
                >
                    <Search className="h-4 w-4 flex-shrink-0" />
                    <AnimatePresence mode="wait">
                        {!isSidebarCollapsed && (
                            <motion.div
                                initial={{ opacity: 0, width: 0 }}
                                animate={{ opacity: 1, width: 'auto' }}
                                exit={{ opacity: 0, width: 0 }}
                                transition={{ duration: 0.2 }}
                                className="flex items-center justify-between flex-1 overflow-hidden"
                            >
                                <span className="whitespace-nowrap">Search...</span>
                                <div className="flex items-center gap-0.5 text-xs">
                                    <kbd className="px-1.5 py-0.5 bg-surface-2 rounded text-[10px]">
                                        <Command className="h-2.5 w-2.5 inline" />
                                    </kbd>
                                    <kbd className="px-1.5 py-0.5 bg-surface-2 rounded text-[10px]">K</kbd>
                                </div>
                            </motion.div>
                        )}
                    </AnimatePresence>
                </button>
            </div>

            {/* Navigation */}
            <div className="flex-1 px-3 space-y-6 overflow-y-auto overflow-x-hidden">
                {/* Main Navigation */}
                <div className="space-y-1">
                    <AnimatePresence mode="wait">
                        {!isSidebarCollapsed && (
                            <motion.div
                                initial={{ opacity: 0 }}
                                animate={{ opacity: 1 }}
                                exit={{ opacity: 0 }}
                                className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3 px-2"
                            >
                                Menu
                            </motion.div>
                        )}
                    </AnimatePresence>
                    {mainNavItems.map(renderNavItem)}
                </div>

                {/* AI Features */}
                <div className="space-y-1">
                    <AnimatePresence mode="wait">
                        {!isSidebarCollapsed && (
                            <motion.div
                                initial={{ opacity: 0 }}
                                animate={{ opacity: 1 }}
                                exit={{ opacity: 0 }}
                                className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3 px-2"
                            >
                                AI Features
                            </motion.div>
                        )}
                    </AnimatePresence>
                    {aiFeatures.map(renderNavItem)}
                </div>
            </div>

            {/* Footer Actions */}
            <div className="p-3 border-t border-border space-y-1">
                {/* Settings */}
                <button
                    onClick={toggleSettingsPanel}
                    title={isSidebarCollapsed ? "Settings" : undefined}
                    className={cn(
                        "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium",
                        "text-muted-foreground hover:bg-surface-2 hover:text-foreground transition-all duration-200",
                        isSidebarCollapsed && "justify-center"
                    )}
                >
                    <Settings className="h-4 w-4 flex-shrink-0" />
                    <AnimatePresence mode="wait">
                        {!isSidebarCollapsed && (
                            <motion.span
                                initial={{ opacity: 0, width: 0 }}
                                animate={{ opacity: 1, width: 'auto' }}
                                exit={{ opacity: 0, width: 0 }}
                                transition={{ duration: 0.2 }}
                                className="whitespace-nowrap"
                            >
                                Settings
                            </motion.span>
                        )}
                    </AnimatePresence>
                </button>

                {/* Collapse Toggle */}
                <button
                    onClick={toggleSidebar}
                    title={isSidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
                    className={cn(
                        "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium",
                        "text-muted-foreground hover:bg-surface-2 hover:text-foreground transition-all duration-200",
                        isSidebarCollapsed && "justify-center"
                    )}
                >
                    <ChevronLeft
                        className={cn(
                            "h-4 w-4 flex-shrink-0 transition-transform duration-300",
                            isSidebarCollapsed && "rotate-180"
                        )}
                    />
                    <AnimatePresence mode="wait">
                        {!isSidebarCollapsed && (
                            <motion.span
                                initial={{ opacity: 0, width: 0 }}
                                animate={{ opacity: 1, width: 'auto' }}
                                exit={{ opacity: 0, width: 0 }}
                                transition={{ duration: 0.2 }}
                                className="whitespace-nowrap"
                            >
                                Collapse
                            </motion.span>
                        )}
                    </AnimatePresence>
                </button>
            </div>
        </motion.div>
    );
}
