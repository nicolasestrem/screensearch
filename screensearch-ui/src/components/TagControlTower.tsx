import { X } from 'lucide-react';
import { useTags } from '../hooks/useTags';
import { useStore } from '../store/useStore';
import { cn } from '../lib/utils';
import { useState } from 'react';

export function TagControlTower() {
  const { data: tags = [] } = useTags();
  const { filters, setFilters } = useStore();
  const [showAll, setShowAll] = useState(false);

  // Sort tags by usage count if available, or name (currently just name/id)
  // For now, simple list.

  const displayedTags = showAll ? tags : tags.slice(0, 8);
  const hasMore = tags.length > 8;

  const toggleTagFilter = (tagId: number) => {
    const isSelected = filters.tags.includes(tagId);
    setFilters({
      tags: isSelected
        ? filters.tags.filter((id) => id !== tagId)
        : [...filters.tags, tagId],
    });
  };

  if (tags.length === 0) return null;

  return (
    <div className="space-y-3 px-2">
      <div className="flex items-center justify-between text-xs font-semibold text-muted-foreground uppercase tracking-wider px-2">
        <span>Control Tower</span>
      </div>

      <div className="space-y-1">
        {displayedTags.map((tag) => {
          const isActive = filters.tags.includes(tag.id);
          return (
            <button
              key={tag.id}
              onClick={() => toggleTagFilter(tag.id)}
              className={cn(
                "w-full flex items-center justify-between px-3 py-2 rounded-lg text-sm transition-all duration-200 group",
                isActive
                  ? "bg-secondary text-foreground font-medium ring-1 ring-inset ring-border"
                  : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground hover:pl-4"
              )}
            >
              <div className="flex items-center gap-3">
                <div
                  className={cn(
                    "w-2 h-2 rounded-full transition-all duration-300",
                    isActive ? "scale-125 shadow-[0_0_8px_currentColor]" : "opacity-70"
                  )}
                  style={{ backgroundColor: tag.color || 'gray', color: tag.color || 'gray' }}
                />
                <span className="truncate max-w-[120px]">{tag.name}</span>
              </div>
              
              {isActive && (
                <X className="h-3 w-3 opacity-50 group-hover:opacity-100" />
              )}
            </button>
          );
        })}
      </div>

      {hasMore && (
        <button
          onClick={() => setShowAll(!showAll)}
          className="w-full text-xs text-muted-foreground hover:text-primary transition-colors py-1"
        >
          {showAll ? "Show Less" : `Show ${tags.length - 8} More`}
        </button>
      )}
    </div>
  );
}
