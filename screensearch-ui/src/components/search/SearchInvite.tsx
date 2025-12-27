import { useState, useEffect, useRef, useCallback } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Search, X, Sparkles } from 'lucide-react';
import { useStore } from '../../store/useStore';
import { useSearchKeywords } from '../../hooks/useSearch';
import { useFrames } from '../../hooks/useFrames';
import { debounce } from '../../lib/utils';
import { SmartAnswer, ActivitySource } from './SmartAnswer';
import { apiClient } from '../../api/client';
import type { Frame } from '../../types';

interface SearchInviteProps {
  isOpen: boolean;
  onClose: () => void;
}

export function SearchInvite({ isOpen, onClose }: SearchInviteProps) {
  const { filters, setFilters } = useStore();
  const [localQuery, setLocalQuery] = useState('');
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [isGeneratingAnswer, setIsGeneratingAnswer] = useState(false);
  const [smartAnswer, setSmartAnswer] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const { data: suggestions = [] } = useSearchKeywords(
    localQuery,
    localQuery.length > 2
  );

  const { data: framesData } = useFrames();

  // Build activity sources from frames
  const activitySources: ActivitySource[] = (framesData?.data || [])
    .slice(0, 5)
    .map((frame: Frame) => ({
      id: frame.id,
      app: frame.app_name || 'Unknown',
      context: frame.window_name || 'No title',
      timeAgo: getTimeAgo(new Date(frame.timestamp)),
    }));

  // Debounced search
  const debouncedSetQuery = useRef(
    debounce((query: string) => {
      setFilters({ searchQuery: query });
    }, 300)
  ).current;

  // Focus input when modal opens
  useEffect(() => {
    if (isOpen) {
      setTimeout(() => inputRef.current?.focus(), 100);
      setLocalQuery('');
      setSmartAnswer(null);
    }
  }, [isOpen]);

  // Handle escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        onClose();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  const handleInputChange = useCallback((value: string) => {
    setLocalQuery(value);
    setShowSuggestions(true);
    debouncedSetQuery(value);
  }, [debouncedSetQuery]);

  const handleSearch = useCallback((query: string) => {
    setLocalQuery(query);
    setFilters({ searchQuery: query });
    setShowSuggestions(false);
    // Navigate to timeline for results
    useStore.getState().setActiveTab('timeline');
    onClose();
  }, [setFilters, onClose]);

  const handleGenerateAnswer = async () => {
    if (!localQuery) return;

    setIsGeneratingAnswer(true);
    try {
      const response = await apiClient.generateAnswer(localQuery);
      setSmartAnswer(response.answer);
    } catch (error) {
      console.error('Failed to generate answer:', error);
      setSmartAnswer(null);
    } finally {
      setIsGeneratingAnswer(false);
    }
  };

  const handleSuggestionClick = (suggestion: string) => {
    handleSearch(suggestion);
  };

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          {/* Backdrop with blur */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 search-modal-overlay z-40"
            onClick={onClose}
          />

          {/* Modal */}
          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: -20 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: -20 }}
            transition={{ type: 'spring', damping: 25, stiffness: 300 }}
            className="fixed inset-x-4 top-[12%] max-w-2xl mx-auto z-50"
          >
            <div className="glass-panel-cyan rounded-2xl overflow-hidden border-gradient-cyan glow-cyan-lg">
              {/* Search Input Area */}
              <div className="relative">
                <Search className="absolute left-5 top-1/2 -translate-y-1/2 h-5 w-5 text-primary" />
                <input
                  ref={inputRef}
                  type="text"
                  value={localQuery}
                  onChange={(e) => handleInputChange(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && localQuery) {
                      handleSearch(localQuery);
                    }
                  }}
                  placeholder="What did I work on yesterday?"
                  className="w-full pl-14 pr-24 py-5 bg-transparent text-lg placeholder:text-muted-foreground/50 focus:outline-none"
                />

                {/* Right side controls */}
                <div className="absolute right-4 top-1/2 -translate-y-1/2 flex items-center gap-2">
                  {localQuery && (
                    <button
                      onClick={() => {
                        setLocalQuery('');
                        setSmartAnswer(null);
                      }}
                      className="p-1.5 text-muted-foreground hover:text-foreground bg-surface-2 hover:bg-surface-3 rounded-lg transition-all"
                    >
                      <X className="h-4 w-4" />
                    </button>
                  )}
                  <div className="flex items-center gap-1 px-2 py-1 bg-surface-2 rounded-lg text-xs text-muted-foreground">
                    <span>esc</span>
                  </div>
                </div>
              </div>

              {/* Search mode toggle */}
              <div className="px-5 pb-4 flex items-center gap-3 border-b border-glass-border/30">
                <div className="flex items-center gap-1 p-1 bg-surface-1 rounded-lg">
                  {(['fts', 'semantic'] as const).map((mode) => (
                    <button
                      key={mode}
                      onClick={() => setFilters({ searchMode: mode })}
                      className={`px-3 py-1.5 rounded-md text-xs font-medium transition-all ${
                        filters.searchMode === mode
                          ? 'bg-primary text-primary-foreground glow-cyan-subtle'
                          : 'text-muted-foreground hover:text-foreground hover:bg-surface-2'
                      }`}
                    >
                      {mode === 'fts' ? 'Exact Match' : 'Smart Search'}
                    </button>
                  ))}
                </div>

                {localQuery && (
                  <button
                    onClick={handleGenerateAnswer}
                    disabled={isGeneratingAnswer}
                    className="flex items-center gap-2 px-3 py-1.5 bg-primary/10 hover:bg-primary/20 text-primary rounded-lg text-xs font-medium transition-all glow-cyan-subtle"
                  >
                    <Sparkles className={`h-3.5 w-3.5 ${isGeneratingAnswer ? 'animate-spin' : ''}`} />
                    {isGeneratingAnswer ? 'Generating...' : 'Get Smart Answer'}
                  </button>
                )}
              </div>

              {/* Results Area */}
              <div className="max-h-[60vh] overflow-y-auto">
                {/* Smart Answer */}
                {smartAnswer && (
                  <div className="p-5 border-b border-glass-border/30">
                    <SmartAnswer answer={smartAnswer} sources={activitySources} />
                  </div>
                )}

                {/* Suggestions */}
                {showSuggestions && suggestions.length > 0 && !smartAnswer && (
                  <div className="p-2">
                    {suggestions.map((suggestion, index) => (
                      <motion.button
                        key={index}
                        initial={{ opacity: 0, x: -10 }}
                        animate={{ opacity: 1, x: 0 }}
                        transition={{ delay: index * 0.05 }}
                        onClick={() => handleSuggestionClick(suggestion)}
                        className="w-full px-4 py-3 text-left hover:bg-surface-2 rounded-lg transition-all flex items-center gap-3 group"
                      >
                        <Search className="h-4 w-4 text-muted-foreground group-hover:text-primary" />
                        <span className="text-foreground group-hover:text-primary transition-colors">
                          {suggestion}
                        </span>
                      </motion.button>
                    ))}
                  </div>
                )}

                {/* Empty state with helpful tips */}
                {!localQuery && !smartAnswer && (
                  <div className="p-6 text-center space-y-4">
                    <div className="flex justify-center">
                      <div className="p-3 rounded-xl bg-primary/10 glow-cyan-subtle">
                        <Sparkles className="h-6 w-6 text-primary" />
                      </div>
                    </div>
                    <div>
                      <h3 className="font-medium text-foreground mb-1">Search Your Screen Memory</h3>
                      <p className="text-sm text-muted-foreground">
                        Ask questions like "What did I work on yesterday?" or search for specific apps and content.
                      </p>
                    </div>
                    <div className="flex flex-wrap justify-center gap-2 pt-2">
                      {['VS Code yesterday', 'Chrome articles', 'meeting notes'].map((hint) => (
                        <button
                          key={hint}
                          onClick={() => handleInputChange(hint)}
                          className="px-3 py-1.5 text-xs bg-surface-2 hover:bg-surface-3 text-muted-foreground hover:text-foreground rounded-lg transition-all"
                        >
                          {hint}
                        </button>
                      ))}
                    </div>
                  </div>
                )}
              </div>

              {/* Footer */}
              <div className="px-5 py-3 border-t border-glass-border/30 flex items-center justify-between text-xs text-muted-foreground">
                <div className="flex items-center gap-4">
                  <span className="flex items-center gap-1">
                    <kbd className="px-1.5 py-0.5 bg-surface-2 rounded text-[10px]">↵</kbd>
                    <span>to search</span>
                  </span>
                  <span className="flex items-center gap-1">
                    <kbd className="px-1.5 py-0.5 bg-surface-2 rounded text-[10px]">esc</kbd>
                    <span>to close</span>
                  </span>
                </div>
                <span className="text-primary/70">ScreenSearch Intel</span>
              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

// Helper function to format time ago
function getTimeAgo(date: Date): string {
  const seconds = Math.floor((new Date().getTime() - date.getTime()) / 1000);

  if (seconds < 60) return 'just now';
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}
