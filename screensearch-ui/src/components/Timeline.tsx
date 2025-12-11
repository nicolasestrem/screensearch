import { useState, useEffect } from 'react';
import { Grid, List, Loader2, AlertCircle } from 'lucide-react';
import { useStore } from '../store/useStore';
import { useFrames } from '../hooks/useFrames';
import { FrameCard } from './FrameCard';
import { format } from 'date-fns';

export function Timeline() {
  const { filters, viewMode, setViewMode } = useStore();
  const [page, setPage] = useState(0);
  const limit = 20;

  const queryParams = {
    limit,
    offset: page * limit,
    ...(filters.dateRange.start && {
      start_time: filters.dateRange.start.toISOString(),
    }),
    ...(filters.dateRange.end && {
      end_time: filters.dateRange.end.toISOString(),
    }),
    ...(filters.applications[0] && {
      app_name: filters.applications[0],
    }),
    ...(filters.searchQuery && {
      q: filters.searchQuery,
    }),
  };

  const { data, isLoading, isError, error } = useFrames(queryParams);

  // Reset page when filters change
  useEffect(() => {
    setPage(0);
  }, [filters]);

  if (isLoading && page === 0) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-primary" />
      </div>
    );
  }

  if (isError) {
    return (
      <div className="flex flex-col items-center justify-center h-64 space-y-4">
        <AlertCircle className="h-12 w-12 text-destructive" />
        <div className="text-center">
          <p className="text-lg font-medium">Failed to load frames</p>
          <p className="text-sm text-muted-foreground">
            {error instanceof Error ? error.message : 'Unknown error'}
          </p>
        </div>
      </div>
    );
  }

  if (!data || data.data.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-64 space-y-4">
        <div className="text-center">
          <p className="text-lg font-medium">No frames found</p>
          <p className="text-sm text-muted-foreground">
            Try adjusting your search filters or wait for new captures
          </p>
        </div>
      </div>
    );
  }

  const totalPages = Math.ceil(data.pagination.total / limit);

  // Group frames by date
  const framesByDate = data.data.reduce((acc, frame) => {
    const date = format(new Date(frame.timestamp), 'yyyy-MM-dd');
    if (!acc[date]) {
      acc[date] = [];
    }
    acc[date].push(frame);
    return acc;
  }, {} as Record<string, typeof data.data>);

  return (
    <div className="space-y-6">
      {/* View Mode Toggle */}
      <div className="flex items-center justify-between">
        <div className="text-sm text-muted-foreground">
          Showing {data.data.length} of {data.pagination.total} frames
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setViewMode('grid')}
            className={`p-2 rounded-lg transition-colors ${viewMode === 'grid'
              ? 'bg-primary text-primary-foreground'
              : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
              }`}
          >
            <Grid className="h-4 w-4" />
          </button>
          <button
            onClick={() => setViewMode('list')}
            className={`p-2 rounded-lg transition-colors ${viewMode === 'list'
              ? 'bg-primary text-primary-foreground'
              : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
              }`}
          >
            <List className="h-4 w-4" />
          </button>
        </div>
      </div>

      {/* Frames Timeline */}
      <div className="space-y-12">
        {Object.entries(framesByDate).map(([date, frames]) => (
          <div key={date} className="relative space-y-6">
            {/* Date Header */}
            <div className="sticky top-16 z-20 py-4 -mx-4 px-4 bg-background/95 backdrop-blur-sm border-b border-border/40 transition-all duration-200">
              <div className="flex items-baseline gap-3">
                <h2 className="text-2xl font-bold tracking-tight text-foreground">
                  {format(new Date(date), 'EEEE')}
                </h2>
                <div className="h-1 w-1 rounded-full bg-muted-foreground/30" />
                <p className="text-muted-foreground font-medium">
                  {format(new Date(date), 'MMMM d, yyyy')}
                </p>
                <div className="ml-auto text-xs font-mono text-muted-foreground/50 tabular-nums px-2 py-0.5 rounded-md bg-secondary/30">
                  {frames.length} captures
                </div>
              </div>
            </div>

            {/* Frames Grid/List */}
            <div
              className={
                viewMode === 'grid'
                  ? 'grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-6'
                  : 'space-y-4 max-w-3xl mx-auto'
              }
            >
              {frames.map((frame) => (
                <FrameCard
                  key={frame.id}
                  frame={frame}
                  searchQuery={filters.searchQuery}
                />
              ))}
            </div>
          </div>
        ))}
      </div>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-center gap-2 pt-6">
          <button
            onClick={() => setPage(Math.max(0, page - 1))}
            disabled={page === 0}
            className="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            Previous
          </button>
          <div className="flex items-center gap-2">
            {Array.from({ length: Math.min(5, totalPages) }, (_, i) => {
              const pageNum =
                totalPages <= 5
                  ? i
                  : page < 3
                    ? i
                    : page >= totalPages - 3
                      ? totalPages - 5 + i
                      : page - 2 + i;

              return (
                <button
                  key={pageNum}
                  onClick={() => setPage(pageNum)}
                  className={`w-10 h-10 rounded-xl text-sm font-medium transition-all duration-200 ${page === pageNum
                      ? 'bg-primary text-primary-foreground shadow-md shadow-primary/20 scale-105'
                      : 'text-muted-foreground hover:text-foreground hover:bg-secondary/50'
                    }`}
                >
                  {pageNum + 1}
                </button>
              );
            })}
          </div>
          <button
            onClick={() => setPage(Math.min(totalPages - 1, page + 1))}
            disabled={page >= totalPages - 1}
            className="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            Next
          </button>
        </div>
      )}

      {/* Loading Indicator for Pagination */}
      {isLoading && page > 0 && (
        <div className="flex items-center justify-center py-4">
          <Loader2 className="h-6 w-6 animate-spin text-primary" />
        </div>
      )}
    </div>
  );
}
