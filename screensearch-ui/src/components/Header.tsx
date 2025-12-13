import { Settings, Moon, Sun, Activity } from 'lucide-react';
import { useStore } from '../store/useStore';
import { useHealth } from '../hooks/useHealth';

export function Header() {
  const { isDarkMode, toggleDarkMode, toggleSettingsPanel, activeTab, setActiveTab } = useStore();
  const { data: health, isLoading } = useHealth();

  return (
    <header className="sticky top-0 z-50 w-full border-b border-white/10 bg-background/80 backdrop-blur-md supports-[backdrop-filter]:bg-background/60">
      <div className="container mx-auto px-4 h-16">
        <div className="flex items-center justify-between h-full">
          {/* Logo and Title */}
          <div className="flex items-center gap-6">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 bg-gradient-to-br from-primary to-primary/60 rounded-lg flex items-center justify-center shadow-lg shadow-primary/20">
                <Activity className="h-6 w-6 text-primary-foreground" />
              </div>
              <div>
                <h1 className="text-xl font-bold tracking-tight">ScreenSearch</h1>
                <p className="text-xs text-muted-foreground">
                  Your searchable screen capture history
                </p>
              </div>
            </div>

            {/* Navigation */}
            <nav className="flex items-center gap-1 bg-muted/50 p-1 rounded-lg">
              <button
                onClick={() => setActiveTab('timeline')}
                className={`px-3 py-1.5 text-sm font-medium rounded-md transition-all ${activeTab === 'timeline'
                  ? 'bg-background text-foreground shadow-sm'
                  : 'text-muted-foreground hover:text-foreground hover:bg-background/50'
                  }`}
              >
                Timeline
              </button>
              <button
                onClick={() => setActiveTab('intelligence')}
                className={`px-3 py-1.5 text-sm font-medium rounded-md transition-all ${activeTab === 'intelligence'
                  ? 'bg-background text-foreground shadow-sm'
                  : 'text-muted-foreground hover:text-foreground hover:bg-background/50'
                  }`}
              >
                Intelligence
              </button>
            </nav>
          </div>

          {/* Status and Actions */}
          <div className="flex items-center gap-3 md:gap-4">
            {/* Health Status */}
            {!isLoading && health && (
              <div className="hidden md:flex items-center gap-2 px-3 py-1.5 bg-card border border-border rounded-lg">
                <div
                  className={`w-2 h-2 rounded-full ${health.status === 'ok'
                    ? 'bg-green-500 animate-pulse'
                    : health.status === 'degraded'
                      ? 'bg-yellow-500'
                      : 'bg-red-500'
                    }`}
                />
                <span className="text-sm">
                  {health.frame_count.toLocaleString()} frames
                </span>
              </div>
            )}

            <div className="h-6 w-px bg-border/50 hidden md:block" />

            {/* Actions */}
            <div className="flex items-center gap-2">
              <button
                onClick={toggleDarkMode}
                className="p-2.5 text-muted-foreground hover:text-foreground hover:bg-secondary/80 rounded-full transition-all duration-200"
                aria-label={isDarkMode ? 'Switch to light mode' : 'Switch to dark mode'}
                title="Toggle theme"
              >
                {isDarkMode ? (
                  <Sun className="h-4 w-4" />
                ) : (
                  <Moon className="h-4 w-4" />
                )}
              </button>

              <button
                onClick={toggleSettingsPanel}
                className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground text-sm font-medium rounded-full shadow-lg shadow-primary/25 hover:shadow-primary/40 hover:bg-primary/90 transition-all duration-200 active:scale-95"
                aria-label="Open settings panel"
              >
                <Settings className="h-4 w-4" />
                <span className="hidden md:inline">Settings</span>
              </button>
            </div>
          </div>
        </div>
      </div>
    </header>
  );
}
