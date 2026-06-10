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
                    "w-full flex items-center gap-3 px-3 py-2.5 text-sm font-medium transition-all duration-200 relative group border-l-2",
                    isSidebarCollapsed && "justify-center",
                    isActive && !isDisabled
                        ? "border-ink bg-paper-2 text-ink"
                        : isDisabled
                            ? "border-transparent text-muted/50 cursor-not-allowed"
                            : "border-transparent text-muted hover:bg-paper-2 hover:text-ink hover:border-rule"
                )}
            >
                <Icon className={cn("h-4 w-4 flex-shrink-0 transition-colors duration-200", isActive && "text-ink")} />

                <AnimatePresence mode="wait">
                    {!isSidebarCollapsed && (
                        <motion.div
                            initial={{ opacity: 0, width: 0 }}
                            animate={{ opacity: 1, width: 'auto' }}
                            exit={{ opacity: 0, width: 0 }}
                            transition={{ duration: 0.2 }}
                            className="flex items-center justify-between flex-1 overflow-hidden"
                        >
                            <span className={cn("whitespace-nowrap transition-all duration-200 font-sans")}>{item.label}</span>
                            {item.comingSoon && (
                                <span className="font-mono text-[10px] text-muted uppercase tracking-wider px-1.5 py-0.5 border border-rule">Soon</span>
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
            className="h-screen bg-paper border-r border-rule flex flex-col z-10"
        >
            {/* Logo */}
            <div className={cn(
                "p-4 flex items-center border-b border-rule",
                isSidebarCollapsed ? "justify-center" : "px-6"
            )}>
                <Logo collapsed={isSidebarCollapsed} />
            </div>

            {/* Search Shortcut */}
            <div className={cn("p-4 border-b border-rule", isSidebarCollapsed && "px-2")}>
                <button
                    onClick={openSearchModal}
                    title={isSidebarCollapsed ? "Search (⌘K)" : undefined}
                    className={cn(
                        "w-full flex items-center gap-3 px-3 py-2.5 text-sm",
                        "bg-paper-2 hover:bg-rule-2 border border-rule",
                        "text-ink transition-all rounded-none",
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
                                <span className="whitespace-nowrap font-serif italic text-[15px]">Search...</span>
                                <div className="flex items-center gap-1 text-xs">
                                    <kbd className="px-1.5 py-0.5 bg-paper border border-rule font-mono text-[10px]">
                                        <Command className="h-2.5 w-2.5 inline" />
                                    </kbd>
                                    <kbd className="px-1.5 py-0.5 bg-paper border border-rule font-mono text-[10px]">K</kbd>
                                </div>
                            </motion.div>
                        )}
                    </AnimatePresence>
                </button>
            </div>

            {/* Navigation */}
            <div className="flex-1 py-4 space-y-6 overflow-y-auto overflow-x-hidden">
                {/* Main Navigation */}
                <div className="space-y-1">
                    <AnimatePresence mode="wait">
                        {!isSidebarCollapsed && (
                            <motion.div
                                initial={{ opacity: 0 }}
                                animate={{ opacity: 1 }}
                                exit={{ opacity: 0 }}
                                className="font-mono text-[11px] text-muted uppercase tracking-[0.12em] mb-3 px-4"
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
                                className="font-mono text-[11px] text-muted uppercase tracking-[0.12em] mb-3 px-4"
                            >
                                AI Features
                            </motion.div>
                        )}
                    </AnimatePresence>
                    {aiFeatures.map(renderNavItem)}
                </div>
            </div>

            {/* Footer Actions */}
            <div className="p-3 border-t border-rule space-y-1 bg-paper-2/50">
                {/* Settings */}
                <button
                    onClick={toggleSettingsPanel}
                    title={isSidebarCollapsed ? "Settings" : undefined}
                    className={cn(
                        "w-full flex items-center gap-3 px-3 py-2.5 text-sm font-medium border-l-2 border-transparent",
                        "text-muted hover:bg-rule-2 hover:text-ink transition-all duration-200",
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
                                className="whitespace-nowrap font-sans"
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
                        "w-full flex items-center gap-3 px-3 py-2.5 text-sm font-medium border-l-2 border-transparent",
                        "text-muted hover:bg-rule-2 hover:text-ink transition-all duration-200",
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
                                className="whitespace-nowrap font-sans"
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
