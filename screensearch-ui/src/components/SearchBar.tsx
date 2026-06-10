import { useState, useEffect, useRef } from 'react';
import { Search, X, Calendar, Tag as TagIcon, Monitor } from 'lucide-react';
import { useStore } from '../store/useStore';
import { useSearchKeywords } from '../hooks/useSearch';
import { useTags } from '../hooks/useTags';
import { debounce } from '../lib/utils';
import { format } from 'date-fns';

export function SearchBar() {
  const { filters, setFilters, resetFilters } = useStore();
  const [localQuery, setLocalQuery] = useState(filters.searchQuery);
  const [showAutocomplete, setShowAutocomplete] = useState(false);
  const [showFilters, setShowFilters] = useState(false);
  const autocompleteRef = useRef<HTMLDivElement>(null);

  const { data: suggestions = [] } = useSearchKeywords(
    localQuery,
    localQuery.length > 2
  );
  const { data: tags = [] } = useTags();

  // Debounced search query update
  const debouncedSetQuery = useRef(
    debounce((query: string) => {
      setFilters({ searchQuery: query });
    }, 300)
  ).current;

  useEffect(() => {
    debouncedSetQuery(localQuery);
  }, [localQuery, debouncedSetQuery]);

  // Cleanup debounced function on unmount
  useEffect(() => {
    return () => {
      // Cancel any pending debounced calls when component unmounts
      debouncedSetQuery.cancel();
    };
  }, [debouncedSetQuery]);

  // Close autocomplete on outside click
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (
        autocompleteRef.current &&
        !autocompleteRef.current.contains(event.target as Node)
      ) {
        setShowAutocomplete(false);
      }
    }

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const hasActiveFilters =
    filters.dateRange.start ||
    filters.dateRange.end ||
    filters.applications.length > 0 ||
    filters.tags.length > 0;

  return (
    <div className="space-y-6 w-full max-w-4xl mx-auto">
      {/* Main Search Bar */}
      <div className="relative group z-30" ref={autocompleteRef}>
        <div className="relative transition-all duration-300 transform">
          <div className="relative bg-paper border border-rule rounded-none group-focus-within:border-ink transition-all duration-300">
            <Search className="absolute left-4 top-1/2 -translate-y-1/2 h-5 w-5 text-muted-foreground group-focus-within:text-primary transition-colors" />
            <input
              type="text"
              value={localQuery}
              onChange={(e) => {
                setLocalQuery(e.target.value);
                setShowAutocomplete(true);
              }}
              onFocus={() => setShowAutocomplete(true)}
              placeholder="What did I work on yesterday?"
              className="w-full pl-12 pr-12 py-4 bg-transparent border-none rounded-none text-lg font-serif italic placeholder:text-muted/50 focus:outline-none focus:ring-0"
            />
            {localQuery && (
              <button
                onClick={() => {
                  setLocalQuery('');
                  setFilters({ searchQuery: '' });
                }}
                className="absolute right-4 top-1/2 -translate-y-1/2 p-1 text-muted hover:text-ink bg-paper-2 hover:bg-rule rounded-none transition-all border border-rule"
              >
                <X className="h-4 w-4" />
              </button>
            )}
          </div>
        </div>

        {/* Autocomplete Dropdown */}
        {showAutocomplete && suggestions.length > 0 && (
          <div className="absolute top-full left-0 right-0 mt-3 bg-paper border border-ink z-50 max-h-80 overflow-y-auto animate-fade-in-up">
            <div className="p-2 space-y-1">
              {suggestions.map((suggestion, index) => (
                <button
                  key={index}
                  onClick={() => {
                    setLocalQuery(suggestion);
                    setFilters({ searchQuery: suggestion });
                    setShowAutocomplete(false);
                  }}
                  className="w-full px-4 py-3 text-left hover:bg-paper-2 hover:text-ink border-b border-rule last:border-0 rounded-none transition-all flex items-center gap-3 group/item"
                >
                  <Search className="h-4 w-4 text-muted group-hover/item:text-ink" />
                  <span className="font-medium">{suggestion}</span>
                </button>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Search Mode Toggle */}
      <div className="flex justify-center">
        <div className="bg-paper-2 p-1 rounded-none flex items-center gap-1 border border-rule">
          {(['fts', 'semantic'] as const).map((mode) => (
            <button
              key={mode}
              onClick={() => setFilters({ searchMode: mode })}
              className={`px-3 py-1.5 rounded-none text-xs font-mono uppercase tracking-wider transition-all ${filters.searchMode === mode
                  ? 'bg-ink text-paper shadow-none ring-0'
                  : 'text-ink hover:text-ink hover:bg-rule-2'
                }`}
            >
              {mode === 'fts' ? 'Exact Match' : 'Smart Search'}
            </button>
          ))}
        </div>
      </div>

      {/* Filter Stats & Toggle */}
      <div className="flex items-center justify-between px-2">
        <div className="flex items-center gap-4 text-sm text-muted-foreground">
          {hasActiveFilters && (
            <div className="flex items-center gap-2 animate-fade-in">
              <span className="w-1.5 h-1.5 rounded-full bg-ink" />
              <span className="font-serif text-[15px] text-ink">Filters active</span>
            </div>
          )}
        </div>

        <div className="flex items-center gap-3">
          {hasActiveFilters && (
            <button
              onClick={resetFilters}
              className="text-sm text-ink hover:text-warn transition-colors px-3 py-1.5 rounded-none border border-transparent hover:border-warn hover:bg-paper-2"
            >
              Clear all
            </button>
          )}

          <button
            onClick={() => setShowFilters(!showFilters)}
            className={`flex items-center gap-2 px-4 py-2 rounded-none transition-all duration-200 border ${showFilters
              ? 'bg-paper-2 text-ink border-rule'
              : 'bg-paper hover:bg-paper-2 text-muted hover:text-ink border-rule'
              }`}
          >
            <Calendar className={`h-4 w-4 ${showFilters ? 'text-primary' : ''}`} />
            <span className="font-serif text-[15px]">Filters</span>
          </button>
        </div>
      </div>

      {/* Filter Panel */}
      {showFilters && (
        <div className="bg-paper border border-rule p-6 space-y-6 animate-fade-in-up">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
            {/* Left Column */}
            <div className="space-y-4">
              <label className="text-sm font-serif text-[18px] flex items-center gap-2 text-ink">
                <Calendar className="h-4 w-4 text-muted" />
                Date Range
              </label>
              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1.5">
                  <span className="text-xs text-muted-foreground ml-1">From</span>
                  <input
                    type="date"
                    value={filters.dateRange.start ? format(filters.dateRange.start, 'yyyy-MM-dd') : ''}
                    onChange={(e) =>
                      setFilters({
                        dateRange: {
                          ...filters.dateRange,
                          start: e.target.value ? new Date(e.target.value) : null,
                        },
                      })
                    }
                    className="w-full px-3 py-2 bg-paper-2 border border-rule rounded-none text-sm font-mono focus:border-ink outline-none transition-all"
                  />
                </div>
                <div className="space-y-1.5">
                  <span className="text-xs text-muted-foreground ml-1">To</span>
                  <input
                    type="date"
                    value={filters.dateRange.end ? format(filters.dateRange.end, 'yyyy-MM-dd') : ''}
                    onChange={(e) =>
                      setFilters({
                        dateRange: {
                          ...filters.dateRange,
                          end: e.target.value ? new Date(e.target.value) : null,
                        },
                      })
                    }
                    className="w-full px-3 py-2 bg-paper-2 border border-rule rounded-none text-sm font-mono focus:border-ink outline-none transition-all"
                  />
                </div>
              </div>
            </div>

            {/* Right Column */}
            <div className="space-y-4">
              <label className="text-sm font-serif text-[18px] flex items-center gap-2 text-ink">
                <Monitor className="h-4 w-4 text-muted" />
                Application
              </label>
              <div className="space-y-1.5">
                <span className="text-xs text-muted-foreground ml-1">App Name</span>
                <input
                  type="text"
                  placeholder="e.g., Chrome, VS Code"
                  value={filters.applications[0] || ''}
                  onChange={(e) =>
                    setFilters({
                      applications: e.target.value ? [e.target.value] : [],
                    })
                  }
                  className="w-full px-4 py-2 bg-paper-2 border border-rule rounded-none text-sm font-sans focus:border-ink outline-none transition-all"
                />
              </div>
            </div>
          </div>

          <div className="h-px bg-rule" />

          {/* Tag Filter */}
          <div className="space-y-3">
            <label className="text-sm font-serif text-[18px] flex items-center gap-2 text-ink">
              <TagIcon className="h-4 w-4 text-muted" />
              Tags
            </label>
            <div className="flex flex-wrap gap-2">
              {tags.map((tag) => {
                const isSelected = filters.tags.includes(tag.id);
                return (
                  <button
                    key={tag.id}
                    onClick={() => {
                      setFilters({
                        tags: isSelected
                          ? filters.tags.filter((id) => id !== tag.id)
                          : [...filters.tags, tag.id],
                      });
                    }}
                    className={`px-3 py-1.5 rounded-none text-[11px] font-mono uppercase tracking-wider transition-all duration-200 border ${isSelected
                      ? 'bg-ink text-paper border-ink shadow-none'
                      : 'bg-paper-2 text-ink border-rule hover:bg-rule hover:text-ink'
                      }`}
                    style={
                      isSelected && tag.color
                        ? { backgroundColor: `${tag.color}20`, color: tag.color, borderColor: `${tag.color}40` }
                        : undefined
                    }
                  >
                    {tag.name}
                  </button>
                )
              })}
              {tags.length === 0 && (
                <p className="text-sm text-muted-foreground italic">No tags available</p>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
