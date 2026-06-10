import { useEffect, lazy, Suspense } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Toaster } from 'react-hot-toast';
import { useStore } from './store/useStore';
import { Sidebar } from './components/Sidebar';
import { SearchBar } from './components/SearchBar';
import { Timeline } from './components/Timeline';
import { SmartAnswerCard } from './components/search/SmartAnswerCard';
import { SearchInvite } from './components/search/SearchInvite';

import { SettingsPanel } from './components/SettingsPanel';
import { FrameModal } from './components/FrameModal';
import { ErrorBoundary } from './components/ErrorBoundary';
import { Footer } from './components/Footer';
import { DownloadProgress } from './components/DownloadProgress';
import { Loader2 } from 'lucide-react';

// Lazy load heavy pages for better initial bundle size
// Dashboard is the default page, so we keep it eagerly loaded
import { DashboardPage } from './pages/Dashboard';

// Intelligence page is heavy (report generation, markdown rendering)
// Only load when user navigates to reports tab
const IntelligencePage = lazy(() =>
  import('./pages/Intelligence').then(module => ({ default: module.IntelligencePage }))
);

// Loading fallback component for lazy-loaded pages
function PageLoader() {
  return (
    <div className="flex items-center justify-center h-64">
      <Loader2 className="h-8 w-8 animate-spin text-primary" />
    </div>
  );
}

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
 * - Global keyboard shortcuts (Cmd+K for search modal, Cmd+, for settings)
 * - Sidebar and Main Content structure
 * - SearchInvite modal integration
 * - Footer integration
 */
function AppContent() {
  const { isDarkMode, activeTab, isSearchModalOpen, openSearchModal, closeSearchModal } = useStore();

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
      // Cmd/Ctrl + K for search modal
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        openSearchModal();
      }

      // Cmd/Ctrl + , for settings
      if ((e.metaKey || e.ctrlKey) && e.key === ',') {
        e.preventDefault();
        useStore.getState().toggleSettingsPanel();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [openSearchModal]);

  return (
    <div className="flex h-screen w-screen bg-background text-foreground overflow-hidden">
      {/* Sidebar Navigation */}
      <Sidebar />

      {/* Main Content Area */}
      <main className="flex-1 flex flex-col min-w-0 overflow-hidden relative paper-grid">
        <div className="flex-1 overflow-y-auto flex flex-col">
          <div className="container mx-auto px-4 py-8 max-w-7xl flex-1">
            {activeTab === 'dashboard' && <DashboardPage />}
            {activeTab === 'timeline' && (
              <div className="space-y-8 animate-fade-in-up">
                <SearchBar />
                <SmartAnswerCard />
                <Timeline />
              </div>
            )}
            {activeTab === 'reports' && (
              <Suspense fallback={<PageLoader />}>
                <IntelligencePage />
              </Suspense>
            )}
          </div>
          <Footer />
        </div>
      </main>

      <SettingsPanel />
      <FrameModal />
      <SearchInvite isOpen={isSearchModalOpen} onClose={closeSearchModal} />
      <DownloadProgress />

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
