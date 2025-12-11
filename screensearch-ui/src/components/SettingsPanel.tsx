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
} from 'lucide-react';
import { useStore } from '../store/useStore';
import { TagManager } from './TagManager';
import { useSettings, useUpdateSettings } from '../hooks/useSettings';

export function SettingsPanel() {
  const { isSettingsPanelOpen, toggleSettingsPanel, isDarkMode, toggleDarkMode } =
    useStore();

  const { data: apiSettings } = useSettings(isSettingsPanelOpen);
  const updateSettings = useUpdateSettings();

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
      // Auto-save
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
    // Auto-save
    updateSettings.mutate({
      capture_interval: captureInterval,
      monitors: JSON.stringify(monitors),
      excluded_apps: JSON.stringify(newExcludedApps),
      is_paused: isPaused ? 1 : 0,
      retention_days: retentionDays,
    });
  };

  const handleTogglePause = () => {
    const newIsPaused = !isPaused;
    setIsPaused(newIsPaused);
    // Auto-save
    updateSettings.mutate({
      capture_interval: captureInterval,
      monitors: JSON.stringify(monitors),
      excluded_apps: JSON.stringify(excludedApps),
      is_paused: newIsPaused ? 1 : 0,
      retention_days: retentionDays,
    });
  };

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-black/50 backdrop-blur-sm z-40 animate-fade-in"
        onClick={toggleSettingsPanel}
      />

      {/* Panel */}
      <div className="fixed right-0 top-0 bottom-0 w-full max-w-2xl bg-background border-l border-border z-50 overflow-y-auto animate-slide-in">
        <div className="p-6 space-y-6">
          {/* Header */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <SettingsIcon className="h-6 w-6" />
              <h2 className="text-2xl font-bold">Settings</h2>
            </div>
            <button
              onClick={toggleSettingsPanel}
              className="p-2 hover:bg-accent rounded-lg transition-colors"
            >
              <X className="h-5 w-5" />
            </button>
          </div>

          {/* Capture Status */}
          <div className="bg-card border border-border rounded-lg p-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                {isPaused ? (
                  <Play className="h-5 w-5 text-primary" />
                ) : (
                  <Pause className="h-5 w-5 text-primary" />
                )}
                <div>
                  <h3 className="font-semibold">Capture Status</h3>
                  <p className="text-sm text-muted-foreground">
                    {isPaused ? 'Paused' : 'Active'}
                  </p>
                </div>
              </div>
              <button
                onClick={handleTogglePause}
                disabled={updateSettings.isPending}
                className={`px-4 py-2 rounded-lg transition-colors ${
                  isPaused
                    ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                    : 'bg-destructive text-destructive-foreground hover:bg-destructive/90'
                }`}
              >
                {isPaused ? 'Resume' : 'Pause'}
              </button>
            </div>
          </div>

          {/* Appearance */}
          <div className="space-y-4">
            <h3 className="text-lg font-semibold">Appearance</h3>
            <div className="bg-card border border-border rounded-lg p-4">
              <div className="flex items-center justify-between">
                <div>
                  <p className="font-medium">Dark Mode</p>
                  <p className="text-sm text-muted-foreground">
                    Toggle between light and dark themes
                  </p>
                </div>
                <button
                  onClick={toggleDarkMode}
                  className={`relative w-14 h-7 rounded-full transition-colors ${
                    isDarkMode ? 'bg-primary' : 'bg-secondary'
                  }`}
                >
                  <div
                    className={`absolute top-0.5 left-0.5 w-6 h-6 bg-white rounded-full transition-transform ${
                      isDarkMode ? 'translate-x-7' : 'translate-x-0'
                    }`}
                  />
                </button>
              </div>
            </div>
          </div>

          {/* Capture Settings */}
          <div className="space-y-4">
            <h3 className="text-lg font-semibold flex items-center gap-2">
              <Clock className="h-5 w-5" />
              Capture Settings
            </h3>
            <div className="bg-card border border-border rounded-lg p-4 space-y-4">
              <div>
                <label className="block text-sm font-medium mb-2">
                  Capture Interval (seconds)
                </label>
                <input
                  type="range"
                  min="2"
                  max="30"
                  value={captureInterval}
                  onChange={(e) => setCaptureInterval(parseInt(e.target.value))}
                  onMouseUp={saveSettings}
                  onTouchEnd={saveSettings}
                  className="w-full"
                />
                <div className="flex justify-between text-sm text-muted-foreground mt-1">
                  <span>2s</span>
                  <span className="font-medium text-foreground">
                    {captureInterval}s
                  </span>
                  <span>30s</span>
                </div>
              </div>

              <div>
                <label className="block text-sm font-medium mb-2">
                  <Monitor className="inline h-4 w-4 mr-1" />
                  Monitor Selection
                </label>
                <select
                  value={monitors[0] || 0}
                  onChange={(e) => {
                    const newMonitors = [parseInt(e.target.value)];
                    setMonitors(newMonitors);
                    updateSettings.mutate({
                      capture_interval: captureInterval,
                      monitors: JSON.stringify(newMonitors),
                      excluded_apps: JSON.stringify(excludedApps),
                      is_paused: isPaused ? 1 : 0,
                      retention_days: retentionDays,
                    });
                  }}
                  className="w-full px-3 py-2 bg-background border border-input rounded-md"
                >
                  <option value={0}>All Monitors</option>
                  <option value={1}>Monitor 1</option>
                  <option value={2}>Monitor 2</option>
                  <option value={3}>Monitor 3</option>
                </select>
              </div>
            </div>
          </div>

          {/* Privacy Settings */}
          <div className="space-y-4">
            <h3 className="text-lg font-semibold flex items-center gap-2">
              <Shield className="h-5 w-5" />
              Privacy Controls
            </h3>
            <div className="bg-card border border-border rounded-lg p-4 space-y-4">
              <div>
                <label className="block text-sm font-medium mb-2">
                  Excluded Applications
                </label>
                <p className="text-sm text-muted-foreground mb-3">
                  These applications will not be captured
                </p>
                <div className="flex gap-2 mb-3">
                  <input
                    type="text"
                    value={newExcludedApp}
                    onChange={(e) => setNewExcludedApp(e.target.value)}
                    onKeyPress={(e) => e.key === 'Enter' && handleAddExcludedApp()}
                    placeholder="e.g., 1Password"
                    className="flex-1 px-3 py-2 bg-background border border-input rounded-md"
                  />
                  <button
                    onClick={handleAddExcludedApp}
                    className="px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
                  >
                    Add
                  </button>
                </div>
                <div className="flex flex-wrap gap-2">
                  {excludedApps.map((app) => (
                    <div
                      key={app}
                      className="flex items-center gap-2 px-3 py-1.5 bg-secondary text-secondary-foreground rounded-lg"
                    >
                      <span>{app}</span>
                      <button
                        onClick={() => handleRemoveExcludedApp(app)}
                        disabled={updateSettings.isPending}
                        className="hover:text-destructive transition-colors"
                      >
                        <X className="h-4 w-4" />
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>

          {/* Database Settings */}
          <div className="space-y-4">
            <h3 className="text-lg font-semibold flex items-center gap-2">
              <Database className="h-5 w-5" />
              Database Management
            </h3>
            <div className="bg-card border border-border rounded-lg p-4 space-y-4">
              <div>
                <label className="block text-sm font-medium mb-2">
                  Data Retention (days)
                </label>
                <input
                  type="number"
                  min="1"
                  max="365"
                  value={retentionDays}
                  onChange={(e) => setRetentionDays(parseInt(e.target.value))}
                  onBlur={saveSettings}
                  className="w-full px-3 py-2 bg-background border border-input rounded-md"
                />
                <p className="text-sm text-muted-foreground mt-1">
                  Automatically delete captures older than this
                </p>
              </div>

              <div className="flex gap-2">
                <button className="flex-1 px-4 py-2 bg-secondary text-secondary-foreground rounded-lg hover:bg-secondary/80 transition-colors">
                  Export Data
                </button>
                <button className="flex-1 px-4 py-2 bg-destructive text-destructive-foreground rounded-lg hover:bg-destructive/90 transition-colors">
                  Clear All Data
                </button>
              </div>
            </div>
          </div>

          {/* Tag Manager */}
          <div className="space-y-4">
            <TagManager />
          </div>
        </div>
      </div>
    </>
  );
}
