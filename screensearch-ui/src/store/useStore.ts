import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { FilterState } from '../types';

interface AppStore {
  // Theme
  isDarkMode: boolean;
  toggleDarkMode: () => void;

  // Filters
  filters: FilterState;
  setFilters: (filters: Partial<FilterState>) => void;
  resetFilters: () => void;

  // View Mode
  viewMode: 'grid' | 'list';
  setViewMode: (mode: 'grid' | 'list') => void;

  // Selected Frame
  selectedFrameId: number | null;
  setSelectedFrameId: (id: number | null) => void;

  // Navigation
  activeTab: 'timeline' | 'intelligence';
  setActiveTab: (tab: 'timeline' | 'intelligence') => void;

  // AI Config
  aiConfig: {
    providerUrl: string;
    apiKey: string;
    model: string;
  };
  setAiConfig: (config: Partial<{ providerUrl: string; apiKey: string; model: string }>) => void;

  // Settings Panel
  isSettingsPanelOpen: boolean;
  toggleSettingsPanel: () => void;
}

const defaultFilters: FilterState = {
  dateRange: {
    start: null,
    end: null,
  },
  applications: [],
  tags: [],
  searchQuery: '',
};

export const useStore = create<AppStore>()(
  persist(
    (set) => ({
      // Theme
      isDarkMode: true,
      toggleDarkMode: () => set((state) => ({ isDarkMode: !state.isDarkMode })),

      // Filters
      filters: defaultFilters,
      setFilters: (filters) =>
        set((state) => ({
          filters: { ...state.filters, ...filters },
        })),
      resetFilters: () => set({ filters: defaultFilters }),

      // View Mode
      viewMode: 'grid',
      setViewMode: (mode) => set({ viewMode: mode }),

      // Selected Frame
      selectedFrameId: null,
      setSelectedFrameId: (id) => set({ selectedFrameId: id }),

      // Navigation
      activeTab: 'timeline',
      setActiveTab: (tab) => set({ activeTab: tab }),

      // AI Config
      aiConfig: {
        providerUrl: 'http://localhost:11434/v1',
        apiKey: '',
        model: 'deepseek-v3.2:cloud',
      },
      setAiConfig: (config) =>
        set((state) => ({
          aiConfig: { ...state.aiConfig, ...config },
        })),

      // Settings Panel
      isSettingsPanelOpen: false,
      toggleSettingsPanel: () =>
        set((state) => ({ isSettingsPanelOpen: !state.isSettingsPanelOpen })),
    }),
    {
      name: 'screen-memories-store',
      partialize: (state) => ({
        isDarkMode: state.isDarkMode,
        viewMode: state.viewMode,
        aiConfig: state.aiConfig,
      }),
    }
  )
);
