import { useState, useEffect, useRef } from 'react';
import { toast } from 'react-hot-toast';
import {
  X,
  Settings as SettingsIcon,
  Monitor,
  Clock,
  Shield,
  Database,
  Pause,
  Play,
  HardDrive,
  Download,
  CheckCircle,
  Loader2,
  Cpu
} from 'lucide-react';
import { useStore } from '../store/useStore';
import { TagManager } from './TagManager';
import { EmbeddingsStatus } from './EmbeddingsStatus';
import { useSettings, useUpdateSettings } from '../hooks/useSettings';
import { cn } from '../lib/utils';
import { useQuery, useMutation } from '@tanstack/react-query';

type SettingsTab = 'general' | 'capture' | 'privacy' | 'data';

// Model status types
interface ModelStatus {
  downloaded: boolean;
  downloading: boolean;
  model_name: string;
  model_size_bytes: number;
  model_path: string | null;
}

// Server status types
interface ServerStatus {
  status: 'stopped' | 'starting' | 'running' | 'error';
  port: number;
  idle_seconds: number;
  model_loaded: boolean;
  server_binary_available: boolean;
}

// Format idle time for display
function formatIdleTime(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return secs > 0 ? `${minutes}m ${secs}s` : `${minutes}m`;
}

export function SettingsPanel() {
  const { isSettingsPanelOpen, toggleSettingsPanel, isDarkMode, toggleDarkMode } = useStore();
  const { data: apiSettings } = useSettings(isSettingsPanelOpen);
  const updateSettings = useUpdateSettings();

  const [activeTab, setActiveTab] = useState<SettingsTab>('general');

  // Local LLM model status query
  const { data: modelStatus, refetch: refetchModelStatus } = useQuery<ModelStatus>({
    queryKey: ['model-status'],
    queryFn: async () => {
      const res = await fetch('http://localhost:3131/api/ai/model/status');
      if (!res.ok) throw new Error('Failed to fetch model status');
      return res.json();
    },
    enabled: isSettingsPanelOpen,
    refetchInterval: (query) => query.state.data?.downloading ? 2000 : false, // Poll while downloading
  });

  // Model download mutation
  const downloadModel = useMutation({
    mutationFn: async () => {
      const res = await fetch('http://localhost:3131/api/ai/model/download', { method: 'POST' });
      if (!res.ok) throw new Error('Failed to start download');
      return res.json();
    },
    onSuccess: () => {
      toast.success('Model download started!');
      refetchModelStatus();
    },
    onError: (err: Error) => {
      toast.error(`Download failed: ${err.message}`);
    },
  });

  // Server status query
  const { data: serverStatus, refetch: refetchServerStatus } = useQuery<ServerStatus>({
    queryKey: ['server-status'],
    queryFn: async () => {
      const res = await fetch('http://localhost:3131/api/ai/server/status');
      if (!res.ok) throw new Error('Failed to fetch server status');
      return res.json();
    },
    enabled: isSettingsPanelOpen,
    refetchInterval: (query) => {
      const status = query.state.data?.status;
      // Poll more frequently when starting, less when stable
      if (status === 'starting') return 1000;
      if (status === 'running') return 5000;
      return false;
    },
  });

  // Server control mutations
  const startServer = useMutation({
    mutationFn: async () => {
      const res = await fetch('http://localhost:3131/api/ai/server/start', { method: 'POST' });
      if (!res.ok) throw new Error('Failed to start server');
      return res.json();
    },
    onSuccess: () => {
      toast.success('Server starting...');
      refetchServerStatus();
    },
    onError: (err: Error) => {
      toast.error(`Failed to start: ${err.message}`);
    },
  });

  const stopServer = useMutation({
    mutationFn: async () => {
      const res = await fetch('http://localhost:3131/api/ai/server/stop', { method: 'POST' });
      if (!res.ok) throw new Error('Failed to stop server');
      return res.json();
    },
    onSuccess: () => {
      toast.success('Server stopped');
      refetchServerStatus();
    },
    onError: (err: Error) => {
      toast.error(`Failed to stop: ${err.message}`);
    },
  });

  const updateTtl = useMutation({
    mutationFn: async (ttlSeconds: number) => {
      const res = await fetch('http://localhost:3131/api/ai/server/ttl', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ ttl_seconds: ttlSeconds }),
      });
      if (!res.ok) throw new Error('Failed to update TTL');
      return res.json();
    },
    onSuccess: () => {
      toast.success('Idle timeout updated');
    },
    onError: (err: Error) => {
      toast.error(`Failed to update TTL: ${err.message}`);
    },
  });

  const downloadLlamaServer = useMutation({
    mutationFn: async () => {
      const res = await fetch('http://localhost:3131/api/ai/server/download', { method: 'POST' });
      if (!res.ok) throw new Error('Failed to start download');
      return res.json();
    },
    onSuccess: () => {
      toast.success('llama-server download started!');
      setDownloadingServer(true);
      refetchServerStatus();

      // Poll every 2 seconds for up to 120 seconds to detect download completion
      if (downloadPollInterval.current) {
        clearInterval(downloadPollInterval.current);
      }
      downloadPollInterval.current = setInterval(() => {
        refetchServerStatus();
      }, 2000);

      // Stop polling after 120 seconds (downloads should complete by then)
      setTimeout(() => {
        if (downloadPollInterval.current) {
          clearInterval(downloadPollInterval.current);
          downloadPollInterval.current = null;
        }
        setDownloadingServer(false);
      }, 120000);
    },
    onError: (err: Error) => {
      toast.error(`Download failed: ${err.message}`);
      setDownloadingServer(false);
    },
  });

  // Local state for TTL slider
  const [ttlMinutes, setTtlMinutes] = useState(5);

  // Track server download in progress for polling
  const [downloadingServer, setDownloadingServer] = useState(false);
  const downloadPollInterval = useRef<ReturnType<typeof setInterval> | null>(null);

  // Local state for editing
  const [captureInterval, setCaptureInterval] = useState(5);
  const [monitors, setMonitors] = useState<number[]>([0]);
  const [excludedApps, setExcludedApps] = useState<string[]>(['1Password', 'KeePass']);
  const [isPaused, setIsPaused] = useState(false);
  const [retentionDays, setRetentionDays] = useState(30);
  const [visionEnabled, setVisionEnabled] = useState(false);
  const [visionProvider, setVisionProvider] = useState('ollama');
  const [visionModel, setVisionModel] = useState('ministral-3:3b');
  const [visionEndpoint, setVisionEndpoint] = useState('http://localhost:11434');
  const [visionApiKey, setVisionApiKey] = useState('');
  const [newExcludedApp, setNewExcludedApp] = useState('');

  // Load settings from API when available
  useEffect(() => {
    if (apiSettings) {
      setCaptureInterval(Number(apiSettings.capture_interval));
      setMonitors(JSON.parse(apiSettings.monitors || '[]'));
      setExcludedApps(JSON.parse(apiSettings.excluded_apps || '[]'));
      setIsPaused(apiSettings.is_paused === 1);
      setRetentionDays(Number(apiSettings.retention_days));
      setVisionEnabled(apiSettings.vision_enabled === 1);
      setVisionProvider(apiSettings.vision_provider || 'ollama');
      setVisionModel(apiSettings.vision_model || 'ministral-3:3b');
      setVisionEndpoint(apiSettings.vision_endpoint || 'http://localhost:11434');
      setVisionApiKey(apiSettings.vision_api_key || '');
    }
  }, [apiSettings]);

  // Stop download polling when binary becomes available
  useEffect(() => {
    if (serverStatus?.server_binary_available && downloadingServer) {
      if (downloadPollInterval.current) {
        clearInterval(downloadPollInterval.current);
        downloadPollInterval.current = null;
      }
      setDownloadingServer(false);
      toast.success('llama-server ready!');
    }
  }, [serverStatus?.server_binary_available, downloadingServer]);

  // Cleanup polling on unmount
  useEffect(() => {
    return () => {
      if (downloadPollInterval.current) {
        clearInterval(downloadPollInterval.current);
      }
    };
  }, []);

  if (!isSettingsPanelOpen) return null;

  // Helper function to save settings to API
  const saveSettings = () => {
    updateSettings.mutate({
      capture_interval: captureInterval,
      monitors: JSON.stringify(monitors),
      excluded_apps: JSON.stringify(excludedApps),
      is_paused: isPaused ? 1 : 0,
      retention_days: retentionDays,
      vision_enabled: visionEnabled ? 1 : 0,
      vision_provider: visionProvider,
      vision_model: visionModel,
      vision_endpoint: visionEndpoint,
      vision_api_key: visionApiKey,
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
        vision_enabled: visionEnabled ? 1 : 0,
        vision_provider: visionProvider,
        vision_model: visionModel,
        vision_endpoint: visionEndpoint,
        vision_api_key: visionApiKey,
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
      vision_enabled: visionEnabled ? 1 : 0,
      vision_provider: visionProvider,
      vision_model: visionModel,
      vision_endpoint: visionEndpoint,
      vision_api_key: visionApiKey,
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
                        capture_interval: captureInterval, monitors: JSON.stringify(monitors), excluded_apps: JSON.stringify(excludedApps), is_paused: newPaused ? 1 : 0, retention_days: retentionDays, vision_enabled: visionEnabled ? 1 : 0,
                        vision_provider: visionProvider,
                        vision_model: visionModel,
                        vision_endpoint: visionEndpoint,
                        vision_api_key: visionApiKey,
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
                          capture_interval: captureInterval, monitors: JSON.stringify(newMonitors), excluded_apps: JSON.stringify(excludedApps), is_paused: isPaused ? 1 : 0, retention_days: retentionDays, vision_enabled: visionEnabled ? 1 : 0,
                          vision_provider: visionProvider,
                          vision_model: visionModel,
                          vision_endpoint: visionEndpoint,
                          vision_api_key: visionApiKey,
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

                  {/* Vision Toggle */}
                  <div className="flex items-center justify-between p-4 bg-card rounded-xl border border-border">
                    <div>
                      <p className="font-medium">Vision Engine</p>
                      <p className="text-sm text-muted-foreground">Enable AI analysis and RAG features</p>
                    </div>
                    <button
                      onClick={() => {
                        const newEnabled = !visionEnabled;
                        setVisionEnabled(newEnabled);
                        updateSettings.mutate({
                          capture_interval: captureInterval,
                          monitors: JSON.stringify(monitors),
                          excluded_apps: JSON.stringify(excludedApps),
                          is_paused: isPaused ? 1 : 0,
                          retention_days: retentionDays,
                          vision_enabled: newEnabled ? 1 : 0,
                          vision_provider: visionProvider,
                          vision_model: visionModel,
                          vision_endpoint: visionEndpoint,
                        });
                      }}
                      className={cn(
                        "relative w-14 h-7 rounded-full transition-colors",
                        visionEnabled ? 'bg-primary' : 'bg-secondary'
                      )}
                    >
                      <div
                        className={cn(
                          "absolute top-0.5 left-0.5 w-6 h-6 bg-white rounded-full transition-transform",
                          visionEnabled ? 'translate-x-7' : 'translate-x-0'
                        )}
                      />
                    </button>
                  </div>

                  {/* AI Configuration */}
                  {visionEnabled && (
                    <div className="space-y-4 animate-in fade-in slide-in-from-top-2 duration-300">
                      <div className="p-4 bg-card rounded-xl border border-border space-y-4">
                        <div className="flex items-center gap-2 mb-2">
                          <SettingsIcon className="w-4 h-4 text-primary" />
                          <h4 className="font-semibold">AI Provider Settings</h4>
                        </div>
                        <div className="h-px bg-border my-2" />

                        <div>
                          <label className="block text-sm font-medium mb-2">Provider Protocol</label>
                          <select
                            value={visionProvider}
                            onChange={(e) => setVisionProvider(e.target.value)}
                            className="w-full bg-background border border-input rounded-lg px-3 py-2 font-mono text-sm"
                          >
                            <option value="local">Local (Ministral-3B) - No API needed</option>
                            <option value="ollama">Ollama (Local Server)</option>
                            <option value="openai">OpenAI Compatible (ChatGPT, vLLM, LM Studio)</option>
                          </select>
                        </div>

                        {/* Local Model Status - shown when local provider selected */}
                        {visionProvider === 'local' && (
                          <div className="p-4 bg-secondary/30 rounded-xl border border-border/50 space-y-4">
                            {/* Model Status Section */}
                            <div className="space-y-3">
                              <div className="flex items-center gap-2">
                                <Cpu className="h-4 w-4 text-primary" />
                                <span className="font-medium">Ministral-3B Model</span>
                              </div>

                              {modelStatus?.downloaded ? (
                                <div className="flex items-center gap-2 text-sm text-green-500">
                                  <CheckCircle className="h-4 w-4" />
                                  <span>Model ready ({(modelStatus.model_size_bytes / 1_000_000_000).toFixed(2)} GB)</span>
                                </div>
                              ) : modelStatus?.downloading ? (
                                <div className="flex items-center gap-2 text-sm text-primary">
                                  <Loader2 className="h-4 w-4 animate-spin" />
                                  <span>Downloading... ({(modelStatus.model_size_bytes / 1_000_000_000).toFixed(2)} GB)</span>
                                </div>
                              ) : (
                                <div className="space-y-2">
                                  <p className="text-sm text-muted-foreground">
                                    Model not downloaded. Size: {modelStatus ? (modelStatus.model_size_bytes / 1_000_000_000).toFixed(2) : '2.15'} GB
                                  </p>
                                  <button
                                    onClick={() => downloadModel.mutate()}
                                    disabled={downloadModel.isPending}
                                    className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 rounded-lg text-sm font-medium disabled:opacity-50"
                                  >
                                    <Download className="h-4 w-4" />
                                    Download Model
                                  </button>
                                </div>
                              )}
                            </div>

                            {/* Divider */}
                            <div className="h-px bg-border/50" />

                            {/* Server Status Section */}
                            <div className="space-y-3">
                              <div className="flex items-center justify-between">
                                <div className="flex items-center gap-2">
                                  <HardDrive className="h-4 w-4 text-primary" />
                                  <span className="font-medium">llama-server</span>
                                </div>
                                {serverStatus && (
                                  <div className={cn(
                                    "flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs font-medium",
                                    serverStatus.status === 'running' && "bg-green-500/20 text-green-500",
                                    serverStatus.status === 'starting' && "bg-yellow-500/20 text-yellow-500",
                                    serverStatus.status === 'stopped' && "bg-secondary text-muted-foreground",
                                    serverStatus.status === 'error' && "bg-destructive/20 text-destructive",
                                  )}>
                                    {serverStatus.status === 'running' && <span className="w-1.5 h-1.5 rounded-full bg-green-500 animate-pulse" />}
                                    {serverStatus.status === 'starting' && <Loader2 className="h-3 w-3 animate-spin" />}
                                    {serverStatus.status === 'stopped' && <span className="w-1.5 h-1.5 rounded-full bg-muted-foreground" />}
                                    {serverStatus.status === 'error' && <span className="w-1.5 h-1.5 rounded-full bg-destructive" />}
                                    <span className="capitalize">{serverStatus.status}</span>
                                    {serverStatus.status === 'running' && serverStatus.port && (
                                      <span className="text-muted-foreground">:{serverStatus.port}</span>
                                    )}
                                  </div>
                                )}
                              </div>

                              {/* Server Binary Check */}
                              {serverStatus && !serverStatus.server_binary_available && (
                                <div className="space-y-2">
                                  <p className="text-sm text-muted-foreground">
                                    llama-server binary not found. Download required for local AI.
                                  </p>
                                  <button
                                    onClick={() => downloadLlamaServer.mutate()}
                                    disabled={downloadLlamaServer.isPending || downloadingServer}
                                    className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 rounded-lg text-sm font-medium disabled:opacity-50"
                                  >
                                    {downloadingServer ? (
                                      <Loader2 className="h-4 w-4 animate-spin" />
                                    ) : (
                                      <Download className="h-4 w-4" />
                                    )}
                                    {downloadingServer ? 'Downloading...' : downloadLlamaServer.isPending ? 'Starting...' : 'Download llama-server'}
                                  </button>
                                </div>
                              )}

                              {/* Server Controls - only show when binary is available */}
                              {serverStatus?.server_binary_available && (
                                <>
                                  {/* Running Status Info */}
                                  {serverStatus.status === 'running' && (
                                    <div className="text-xs text-muted-foreground space-y-1">
                                      <p>Idle: {formatIdleTime(serverStatus.idle_seconds)}</p>
                                      {serverStatus.model_loaded && <p className="text-green-500">Model loaded in VRAM</p>}
                                    </div>
                                  )}

                                  {/* Start/Stop Buttons */}
                                  <div className="flex gap-2">
                                    {serverStatus.status === 'stopped' && (
                                      <button
                                        onClick={() => startServer.mutate()}
                                        disabled={startServer.isPending || !modelStatus?.downloaded}
                                        className="flex items-center gap-2 px-3 py-1.5 bg-green-500/20 text-green-500 hover:bg-green-500/30 rounded-lg text-sm font-medium disabled:opacity-50"
                                      >
                                        <Play className="h-3.5 w-3.5" />
                                        Start Server
                                      </button>
                                    )}
                                    {(serverStatus.status === 'running' || serverStatus.status === 'starting') && (
                                      <button
                                        onClick={() => stopServer.mutate()}
                                        disabled={stopServer.isPending}
                                        className="flex items-center gap-2 px-3 py-1.5 bg-destructive/20 text-destructive hover:bg-destructive/30 rounded-lg text-sm font-medium disabled:opacity-50"
                                      >
                                        <Pause className="h-3.5 w-3.5" />
                                        Stop Server
                                      </button>
                                    )}
                                  </div>

                                  {/* TTL Slider */}
                                  <div className="space-y-2 pt-2">
                                    <div className="flex items-center justify-between text-sm">
                                      <span className="text-muted-foreground">Idle Timeout</span>
                                      <span className="font-mono">{ttlMinutes} min</span>
                                    </div>
                                    <input
                                      type="range"
                                      min="1"
                                      max="30"
                                      value={ttlMinutes}
                                      onChange={(e) => setTtlMinutes(parseInt(e.target.value))}
                                      onMouseUp={() => updateTtl.mutate(ttlMinutes * 60)}
                                      onTouchEnd={() => updateTtl.mutate(ttlMinutes * 60)}
                                      className="w-full h-2 bg-secondary rounded-lg appearance-none cursor-pointer"
                                    />
                                    <p className="text-xs text-muted-foreground">
                                      Server automatically shuts down after this period of inactivity to save resources.
                                    </p>
                                  </div>
                                </>
                              )}
                            </div>

                            <p className="text-xs text-muted-foreground pt-1">
                              Runs entirely on your device. Server starts automatically on first AI request.
                            </p>
                          </div>
                        )}

                        {/* Only show URL/API Key/Model fields for non-local providers */}
                        {visionProvider !== 'local' && (
                          <>
                            <div>
                              <label className="block text-sm font-medium mb-2">Provider Base URL</label>
                              <input
                                type="text"
                                value={visionEndpoint}
                                onChange={(e) => setVisionEndpoint(e.target.value)}
                                placeholder={visionProvider === 'ollama' ? "http://localhost:11434" : "https://api.openai.com/v1"}
                                className="w-full bg-background border border-input rounded-lg px-3 py-2 font-mono text-sm"
                              />
                              <p className="text-xs text-muted-foreground mt-1">
                                {visionProvider === 'ollama'
                                  ? "Base URL for Ollama. API path `/api/generate` will be appended."
                                  : "Base API URL. Path `/chat/completions` will be appended."}
                              </p>
                            </div>

                            <div>
                              <label className="block text-sm font-medium mb-2">API Key (Optional)</label>
                              <input
                                type="password"
                                value={visionApiKey}
                                onChange={(e) => setVisionApiKey(e.target.value)}
                                placeholder="sk-..."
                                className="w-full bg-background border border-input rounded-lg px-3 py-2 font-mono text-sm"
                              />
                            </div>

                            <div>
                              <label className="block text-sm font-medium mb-2">Model Name</label>
                              <input
                                type="text"
                                value={visionModel}
                                onChange={(e) => setVisionModel(e.target.value)}
                                placeholder="e.g., llama3, gpt-4o"
                                className="w-full bg-background border border-input rounded-lg px-3 py-2"
                              />
                            </div>
                          </>
                        )}

                        <div className="flex gap-2 pt-2">
                          <button
                            type="button"
                            onClick={saveSettings}
                            className="flex-1 px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 rounded-lg text-sm font-medium"
                          >
                            Save Configuration
                          </button>
                          <button
                            type="button"
                            onClick={async () => {
                              const toastId = toast.loading('Testing connection...');
                              try {
                                const { apiClient } = await import('../api/client');
                                const result = await apiClient.testVisionConfig({
                                  provider: visionProvider,
                                  model: visionModel,
                                  // Use correct endpoint based on provider
                                  // Local provider uses llama-server on port 31130
                                  endpoint: visionProvider === 'local'
                                    ? 'http://127.0.0.1:31130'
                                    : visionEndpoint,
                                  api_key: visionApiKey || undefined
                                });

                                if (result.success) {
                                  toast.success('Connection successful!', { id: toastId });
                                } else {
                                  toast.error(`Connection failed: ${result.message}`, { id: toastId });
                                }
                              } catch (err: any) {
                                toast.error(`Connection error: ${err.message}`, { id: toastId });
                              }
                            }}
                            className="px-4 py-2 bg-secondary hover:bg-secondary/80 rounded-lg text-sm font-medium"
                          >
                            Test
                          </button>
                        </div>
                      </div>

                      <div className="p-4 bg-secondary/20 rounded-xl border border-border/50 text-sm text-muted-foreground">
                        <p>
                          <span className="font-semibold text-foreground">Privacy Note:</span> When generating reports, context from your screen (text, app names)
                          will be sent to the configured provider. Local providers (like Ollama) keep data device-side.
                        </p>
                      </div>
                    </div>
                  )}

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

