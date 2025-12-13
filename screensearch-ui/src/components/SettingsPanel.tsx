import { useState, useEffect } from 'react';
import {
  X,
  Settings as SettingsIcon,
  Monitor,
  Clock,
  Shield,
  Database,
  Pause,
  Play,
  HardDrive
} from 'lucide-react';
import { useStore } from '../store/useStore';
import { TagManager } from './TagManager';
import { EmbeddingsStatus } from './EmbeddingsStatus';
import { useSettings, useUpdateSettings } from '../hooks/useSettings';
import { cn } from '../lib/utils';

type SettingsTab = 'general' | 'capture' | 'privacy' | 'data';

export function SettingsPanel() {
  const { isSettingsPanelOpen, toggleSettingsPanel, isDarkMode, toggleDarkMode } = useStore();
  const { data: apiSettings } = useSettings(isSettingsPanelOpen);
  const updateSettings = useUpdateSettings();

  const [activeTab, setActiveTab] = useState<SettingsTab>('general');

  // Local state for editing
  const [captureInterval, setCaptureInterval] = useState(5);
  const [monitors, setMonitors] = useState<number[]>([0]);
  const [excludedApps, setExcludedApps] = useState<string[]>(['1Password', 'KeePass']);
  const [isPaused, setIsPaused] = useState(false);
  const [retentionDays, setRetentionDays] = useState(30);
  const [newExcludedApp, setNewExcludedApp] = useState('');

  // Load settings from API when available
  useEffect(() => {
    if (apiSettings) {
      setCaptureInterval(Number(apiSettings.capture_interval));
      setMonitors(JSON.parse(apiSettings.monitors || '[]'));
      setExcludedApps(JSON.parse(apiSettings.excluded_apps || '[]'));
      setIsPaused(apiSettings.is_paused === 1);
      setRetentionDays(Number(apiSettings.retention_days));
    }
  }, [apiSettings]);

  if (!isSettingsPanelOpen) return null;

  // Helper function to save settings to API
  const saveSettings = () => {
    updateSettings.mutate({
      capture_interval: captureInterval,
      monitors: JSON.stringify(monitors),
      excluded_apps: JSON.stringify(excludedApps),
      is_paused: isPaused ? 1 : 0,
      retention_days: retentionDays,
    });
  };

  const handleAddExcludedApp = () => {
    if (newExcludedApp.trim() && !excludedApps.includes(newExcludedApp)) {
      const newExcludedApps = [...excludedApps, newExcludedApp.trim()];
      setExcludedApps(newExcludedApps);
      setNewExcludedApp('');
      // Auto-save logic here or defer to saveSettings
      updateSettings.mutate({
        capture_interval: captureInterval,
        monitors: JSON.stringify(monitors),
        excluded_apps: JSON.stringify(newExcludedApps),
        is_paused: isPaused ? 1 : 0,
        retention_days: retentionDays,
      });
    }
  };

  const handleRemoveExcludedApp = (app: string) => {
    const newExcludedApps = excludedApps.filter((a) => a !== app);
    setExcludedApps(newExcludedApps);
    updateSettings.mutate({
      capture_interval: captureInterval,
      monitors: JSON.stringify(monitors),
      excluded_apps: JSON.stringify(newExcludedApps),
      is_paused: isPaused ? 1 : 0,
      retention_days: retentionDays,
    });
  };

  const tabs = [
    { id: 'general', label: 'General', icon: SettingsIcon },
    { id: 'capture', label: 'Capture', icon: Monitor },
    { id: 'privacy', label: 'Privacy', icon: Shield },
    { id: 'data', label: 'Data & AI', icon: Database },
  ] as const;

  return (
    <>
      <div
        className="fixed inset-0 bg-black/50 backdrop-blur-sm z-40 animate-fade-in"
        onClick={toggleSettingsPanel}
      />

      <div className="fixed right-0 top-0 bottom-0 w-full max-w-2xl bg-background border-l border-border z-50 flex flex-col animate-slide-in shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-border">
          <div className="flex items-center gap-3">
            <SettingsIcon className="h-6 w-6 text-primary" />
            <div>
              <h2 className="text-2xl font-bold">Settings</h2>
              <p className="text-sm text-muted-foreground">Manage your ScreenSearch preferences</p>
            </div>
          </div>
          <button
            onClick={toggleSettingsPanel}
            className="p-2 hover:bg-accent rounded-full transition-colors"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Tab Navigation */}
        <div className="flex px-6 border-b border-border overflow-x-auto">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={cn(
                  "flex items-center gap-2 px-6 py-4 text-sm font-medium border-b-2 transition-colors whitespace-nowrap",
                  activeTab === tab.id
                    ? "border-primary text-primary"
                    : "border-transparent text-muted-foreground hover:text-foreground hover:border-border"
                )}
              >
                <Icon className="h-4 w-4" />
                <span>{tab.label}</span>
              </button>
            )
          })}
        </div>

        {/* Content Area */}
        <div className="flex-1 overflow-y-auto p-6">
          <div className="max-w-xl mx-auto space-y-8">

            {/* General Tab */}
            {activeTab === 'general' && (
              <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-300">
                <section className="space-y-4">
                  <h3 className="text-lg font-semibold border-b border-border pb-2">Appearance</h3>
                  <div className="flex items-center justify-between p-4 bg-card rounded-xl border border-border">
                    <div>
                      <p className="font-medium">Dark Mode</p>
                      <p className="text-sm text-muted-foreground">Toggle application theme</p>
                    </div>
                    <button
                      onClick={toggleDarkMode}
                      className={cn(
                        "relative w-14 h-7 rounded-full transition-colors",
                        isDarkMode ? 'bg-primary' : 'bg-secondary'
                      )}
                    >
                      <div
                        className={cn(
                          "absolute top-0.5 left-0.5 w-6 h-6 bg-white rounded-full transition-transform",
                          isDarkMode ? 'translate-x-7' : 'translate-x-0'
                        )}
                      />
                    </button>
                  </div>
                </section>

                <section className="space-y-4">
                  <h3 className="text-lg font-semibold border-b border-border pb-2">Application</h3>
                  <TagManager />
                </section>
              </div>
            )}

            {/* Capture Tab */}
            {activeTab === 'capture' && (
              <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-300">
                <div className="flex items-center justify-between p-4 rounded-xl border border-border bg-card">
                  <div className="flex items-center gap-3">
                    {isPaused ? <Play className="h-5 w-5 text-primary" /> : <Pause className="h-5 w-5 text-primary" />}
                    <div>
                      <p className="font-medium">Capture Status</p>
                      <p className="text-sm text-muted-foreground">{isPaused ? 'Recording Paused' : 'Recording Active'}</p>
                    </div>
                  </div>
                  <button
                    onClick={() => {
                      const newPaused = !isPaused;
                      setIsPaused(newPaused);
                      updateSettings.mutate({
                        capture_interval: captureInterval, monitors: JSON.stringify(monitors), excluded_apps: JSON.stringify(excludedApps), is_paused: newPaused ? 1 : 0, retention_days: retentionDays
                      });
                    }}
                    className={cn(
                      "px-4 py-2 rounded-lg text-sm font-medium transition-colors",
                      isPaused
                        ? "bg-primary text-primary-foreground hover:bg-primary/90"
                        : "bg-destructive/10 text-destructive hover:bg-destructive/20"
                    )}
                  >
                    {isPaused ? 'Resume' : 'Pause'}
                  </button>
                </div>

                <section className="space-y-4">
                  <label className="block text-sm font-medium">Capture Interval</label>
                  <div className="p-4 bg-card rounded-xl border border-border space-y-4">
                    <div className="flex items-center gap-4">
                      <Clock className="h-5 w-5 text-muted-foreground" />
                      <input
                        type="range"
                        min="2"
                        max="30"
                        value={captureInterval}
                        onChange={(e) => setCaptureInterval(parseInt(e.target.value))}
                        onMouseUp={saveSettings}
                        className="flex-1 h-2 bg-secondary rounded-lg appearance-none cursor-pointer"
                      />
                      <span className="w-12 text-right font-mono font-medium">{captureInterval}s</span>
                    </div>
                    <p className="text-xs text-muted-foreground">How often to take a screenshot. Lower values increase storage usage.</p>
                  </div>
                </section>

                <section className="space-y-4">
                  <label className="block text-sm font-medium">Monitors</label>
                  <div className="p-4 bg-card rounded-xl border border-border">
                    <select
                      value={monitors[0] || 0}
                      onChange={(e) => {
                        const newMonitors = [parseInt(e.target.value)];
                        setMonitors(newMonitors);
                        updateSettings.mutate({
                          capture_interval: captureInterval, monitors: JSON.stringify(newMonitors), excluded_apps: JSON.stringify(excludedApps), is_paused: isPaused ? 1 : 0, retention_days: retentionDays
                        });
                      }}
                      className="w-full bg-background border border-input rounded-lg px-3 py-2"
                    >
                      <option value={0}>All Monitors</option>
                      <option value={1}>Monitor 1</option>
                      <option value={2}>Monitor 2</option>
                    </select>
                  </div>
                </section>
              </div>
            )}

            {/* Privacy Tab */}
            {activeTab === 'privacy' && (
              <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-300">
                <section className="space-y-4">
                  <h3 className="text-lg font-semibold border-b border-border pb-2">Excluded Applications</h3>
                  <p className="text-sm text-muted-foreground">Screenshots will be skipped when these apps are in focus.</p>

                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={newExcludedApp}
                      onChange={(e) => setNewExcludedApp(e.target.value)}
                      placeholder="App name (e.g., specific-app)"
                      className="flex-1 bg-background border border-input rounded-lg px-3 py-2"
                      onKeyPress={(e) => e.key === 'Enter' && handleAddExcludedApp()}
                    />
                    <button
                      onClick={handleAddExcludedApp}
                      className="px-4 py-2 bg-secondary hover:bg-secondary/80 rounded-lg text-sm font-medium"
                    >
                      Add
                    </button>
                  </div>

                  <div className="flex flex-wrap gap-2">
                    {excludedApps.map(app => (
                      <div key={app} className="flex items-center gap-2 px-3 py-1.5 bg-secondary/50 rounded-lg text-sm">
                        <span>{app}</span>
                        <button onClick={() => handleRemoveExcludedApp(app)} className="text-muted-foreground hover:text-destructive">
                          <X className="h-3 w-3" />
                        </button>
                      </div>
                    ))}
                  </div>
                </section>
              </div>
            )}

            {/* Data Tab */}
            {activeTab === 'data' && (
              <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-300">
                <section className="space-y-4">
                  <h3 className="text-lg font-semibold border-b border-border pb-2">Storage & Retention</h3>
                  <div className="p-4 bg-card rounded-xl border border-border space-y-4">
                    <div>
                      <label className="block text-sm font-medium mb-2">Retention Period (Days)</label>
                      <input
                        type="number"
                        min="1"
                        max="365"
                        value={retentionDays}
                        onChange={(e) => setRetentionDays(parseInt(e.target.value))}
                        onBlur={saveSettings}
                        className="w-full bg-background border border-input rounded-lg px-3 py-2"
                      />
                    </div>

                    <div className="flex gap-3 pt-2">
                      <button className="flex-1 flex items-center justify-center gap-2 px-4 py-2 bg-secondary hover:bg-secondary/80 rounded-lg text-sm font-medium text-foreground">
                        <HardDrive className="h-4 w-4" />
                        Export Data
                      </button>
                      <button className="flex-1 flex items-center justify-center gap-2 px-4 py-2 bg-destructive/10 hover:bg-destructive/20 text-destructive rounded-lg text-sm font-medium">
                        Clear All Data
                      </button>
                    </div>
                  </div>
                </section>

                <section className="space-y-4">
                  <h3 className="text-lg font-semibold border-b border-border pb-2">AI & Intelligence</h3>
                  <EmbeddingsStatus />
                </section>
              </div>
            )}

          </div>
        </div>
      </div>
    </>
  );
}

