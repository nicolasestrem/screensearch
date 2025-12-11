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
        <div className="relative transition-all duration-300 transform group-hover:-translate-y-0.5 group-focus-within:-translate-y-1">
          <div className="absolute inset-0 bg-gradient-to-r from-primary/20 via-primary/10 to-transparent rounded-2xl blur-xl opacity-0 group-focus-within:opacity-100 transition-opacity duration-500" />

          <div className="relative bg-card rounded-2xl shadow-sm border border-border/50 group-hover:border-primary/30 group-focus-within:border-primary/50 group-focus-within:shadow-xl group-focus-within:shadow-primary/5 transition-all duration-300">
            <Search className="absolute left-4 top-1/2 -translate-y-1/2 h-5 w-5 text-muted-foreground group-focus-within:text-primary transition-colors" />
            <input
              type="text"
              value={localQuery}
              onChange={(e) => {
                setLocalQuery(e.target.value);
                setShowAutocomplete(true);
              }}
              onFocus={() => setShowAutocomplete(true)}
              placeholder="Search screen captures..."
              className="w-full pl-12 pr-12 py-4 bg-transparent border-none rounded-2xl text-lg placeholder:text-muted-foreground/50 focus:outline-none focus:ring-0"
            />
            {localQuery && (
              <button
                onClick={() => {
                  setLocalQuery('');
                  setFilters({ searchQuery: '' });
                }}
                className="absolute right-4 top-1/2 -translate-y-1/2 p-1 text-muted-foreground hover:text-foreground bg-muted/50 hover:bg-muted rounded-full transition-all"
              >
                <X className="h-4 w-4" />
              </button>
            )}
          </div>
        </div>

        {/* Autocomplete Dropdown */}
        {showAutocomplete && suggestions.length > 0 && (
          <div className="absolute top-full left-0 right-0 mt-3 bg-card/95 backdrop-blur-xl border border-border/50 rounded-xl shadow-2xl shadow-black/10 z-50 max-h-80 overflow-y-auto animate-in fade-in slide-in-from-top-2 duration-200">
            <div className="p-2 space-y-1">
              {suggestions.map((suggestion, index) => (
                <button
                  key={index}
                  onClick={() => {
                    setLocalQuery(suggestion);
                    setFilters({ searchQuery: suggestion });
                    setShowAutocomplete(false);
                  }}
                  className="w-full px-4 py-3 text-left hover:bg-primary/5 hover:text-primary rounded-lg transition-all flex items-center gap-3 group/item"
                >
                  <Search className="h-4 w-4 text-muted-foreground group-hover/item:text-primary/70" />
                  <span className="font-medium">{suggestion}</span>
                </button>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Filter Stats & Toggle */}
      <div className="flex items-center justify-between px-2">
        <div className="flex items-center gap-4 text-sm text-muted-foreground">
          {hasActiveFilters && (
            <div className="flex items-center gap-2 animate-fade-in">
              <span className="w-1.5 h-1.5 rounded-full bg-primary" />
              <span className="font-medium text-primary">Filters active</span>
            </div>
          )}
        </div>

        <div className="flex items-center gap-3">
          {hasActiveFilters && (
            <button
              onClick={resetFilters}
              className="text-sm text-muted-foreground hover:text-destructive transition-colors px-3 py-1.5 rounded-lg hover:bg-destructive/5"
            >
              Clear all
            </button>
          )}

          <button
            onClick={() => setShowFilters(!showFilters)}
            className={`flex items-center gap-2 px-4 py-2 rounded-xl transition-all duration-200 border ${showFilters
                ? 'bg-secondary text-secondary-foreground border-border'
                : 'bg-background hover:bg-secondary/50 text-muted-foreground hover:text-foreground border-transparent'
              }`}
          >
            <Calendar className={`h-4 w-4 ${showFilters ? 'text-primary' : ''}`} />
            <span className="font-medium">Filters</span>
          </button>
        </div>
      </div>

      {/* Filter Panel */}
      {showFilters && (
        <div className="bg-card/50 backdrop-blur-sm border border-border/50 rounded-2xl p-6 space-y-6 animate-in slide-in-from-top-4 duration-300 shadow-xl shadow-black/5">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
            {/* Left Column */}
            <div className="space-y-4">
              <label className="text-sm font-semibold flex items-center gap-2 text-foreground">
                <Calendar className="h-4 w-4 text-primary" />
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
                    className="w-full px-3 py-2.5 bg-background border border-border rounded-xl text-sm focus:ring-2 focus:ring-primary/20 focus:border-primary/50 outline-none transition-all"
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
                    className="w-full px-3 py-2.5 bg-background border border-border rounded-xl text-sm focus:ring-2 focus:ring-primary/20 focus:border-primary/50 outline-none transition-all"
                  />
                </div>
              </div>
            </div>

            {/* Right Column */}
            <div className="space-y-4">
              <label className="text-sm font-semibold flex items-center gap-2 text-foreground">
                <Monitor className="h-4 w-4 text-primary" />
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
                  className="w-full px-4 py-2.5 bg-background border border-border rounded-xl text-sm focus:ring-2 focus:ring-primary/20 focus:border-primary/50 outline-none transition-all"
                />
              </div>
            </div>
          </div>

          <div className="h-px bg-border/50" />

          {/* Tag Filter */}
          <div className="space-y-3">
            <label className="text-sm font-semibold flex items-center gap-2 text-foreground">
              <TagIcon className="h-4 w-4 text-primary" />
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
                    className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 border ${isSelected
                        ? 'bg-primary/10 text-primary border-primary/20 scale-105 shadow-sm'
                        : 'bg-secondary/50 text-muted-foreground border-transparent hover:bg-secondary hover:text-foreground'
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
