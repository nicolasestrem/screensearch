import { useEffect } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Toaster } from 'react-hot-toast';
import { useStore } from './store/useStore';
import { Sidebar } from './components/Sidebar';
import { SearchBar } from './components/SearchBar';
import { Timeline } from './components/Timeline';
import { SettingsPanel } from './components/SettingsPanel';
import { FrameModal } from './components/FrameModal';
import { ErrorBoundary } from './components/ErrorBoundary';
import { Footer } from './components/Footer';
import { AiAssistantPanel } from './components/AiAssistantPanel';

// Create a client
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
      staleTime: 30000, // 30 seconds
    },
  },
});

/**
 * Main App Layout.
 * Handles:
 * - Dark mode application
 * - Global keyboard shortcuts (Cmd+K, Cmd+,)
 * - Sidebar and Main Content structure
 * - Footer integration
 */
function AppContent() {
  const { isDarkMode } = useStore();

  useEffect(() => {
    // Apply dark mode class to html element
    if (isDarkMode) {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  }, [isDarkMode]);

  useEffect(() => {
    // Keyboard shortcuts
    const handleKeyDown = (e: KeyboardEvent) => {
      // Cmd/Ctrl + K for search focus
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        const searchInput = document.querySelector('input[type="text"]') as HTMLInputElement;
        searchInput?.focus();
      }

      // Cmd/Ctrl + , for settings
      if ((e.metaKey || e.ctrlKey) && e.key === ',') {
        e.preventDefault();
        useStore.getState().toggleSettingsPanel();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  return (
    <div className="flex h-screen w-screen bg-background text-foreground overflow-hidden">
      {/* Sidebar Navigation */}
      <Sidebar />

      {/* Main Content Area */}
      <main className="flex-1 flex flex-col min-w-0 overflow-hidden relative">
        {/* Background Decor */}
        {/* Background Decor - Website matched grid */}
        <div className="absolute inset-0 -z-10 h-full w-full bg-grid [mask-image:linear-gradient(to_bottom,transparent,black_10%,black_90%,transparent)] opacity-60 pointer-events-none" />

        <div className="flex-1 overflow-y-auto flex flex-col">
          <div className="container mx-auto px-4 py-8 max-w-7xl flex-1">
            <div className="space-y-8 animate-in fade-in duration-500">
              <SearchBar />
              <Timeline />
            </div>
          </div>
          <Footer />
        </div>
      </main>

      <SettingsPanel />
      <SettingsPanel />
      <FrameModal />
      <AiAssistantPanel />

      <Toaster
        position="bottom-right"
        toastOptions={{
          className: '',
          style: {
            background: 'hsl(var(--card))',
            color: 'hsl(var(--card-foreground))',
            border: '1px solid hsl(var(--border))',
          },
          success: {
            iconTheme: {
              primary: 'hsl(var(--primary))',
              secondary: 'white',
            },
          },
          error: {
            iconTheme: {
              primary: 'hsl(var(--destructive))',
              secondary: 'white',
            },
          },
        }}
      />
    </div>
  );
}

function App() {
  return (
    <ErrorBoundary>
      <QueryClientProvider client={queryClient}>
        <AppContent />
      </QueryClientProvider>
    </ErrorBoundary>
  );
}

export default App;
