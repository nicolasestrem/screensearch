import { useState, useRef, useEffect, useMemo } from 'react';
import { Monitor, Tag as TagIcon, X, Plus } from 'lucide-react';
import { Frame } from '../types';
import { formatRelativeTime, truncateText, highlightText } from '../lib/utils';
import { getOCRText } from '../lib/ocrUtils';
import { useStore } from '../store/useStore';
import { useFrameImage } from '../hooks/useFrames';
import { useAddTagToFrame, useRemoveTagFromFrame, useTags } from '../hooks/useTags';

interface FrameCardProps {
  frame: Frame;
  searchQuery?: string;
}

export function FrameCard({ frame, searchQuery = '' }: FrameCardProps) {
  const [showTagMenu, setShowTagMenu] = useState(false);
  const [imageError, setImageError] = useState(false);
  const tagMenuRef = useRef<HTMLDivElement>(null);
  const { setSelectedFrameId } = useStore();

  const { data: imageUrl } = useFrameImage(frame.id);
  const { data: allTags = [] } = useTags();
  const addTag = useAddTagToFrame();
  const removeTag = useRemoveTagFromFrame();

  const availableTags = allTags.filter(
    (tag) => !frame.tags.some((frameTag) => frameTag.id === tag.id)
  );

  // Memoize the OCR text extraction to avoid re-computation on every render
  const cleanText = useMemo(() => getOCRText(frame.ocr_text), [frame.ocr_text]);

  // Memoize the highlighted text parts to avoid re-computation
  const highlightedParts = useMemo(() => {
    const truncated = truncateText(cleanText, 200);
    return searchQuery ? highlightText(truncated, searchQuery) : [{ text: truncated, isHighlight: false }];
  }, [cleanText, searchQuery]);

  const handleAddTag = (tagId: number) => {
    addTag.mutate({ frameId: frame.id, tagId });
    setShowTagMenu(false);
  };

  const handleRemoveTag = (tagId: number) => {
    removeTag.mutate({ frameId: frame.id, tagId });
  };

  // Close tag menu on outside click
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (tagMenuRef.current && !tagMenuRef.current.contains(event.target as Node)) {
        setShowTagMenu(false);
      }
    }

    if (showTagMenu) {
      document.addEventListener('mousedown', handleClickOutside);
      return () => document.removeEventListener('mousedown', handleClickOutside);
    }
  }, [showTagMenu]);

  return (
    <div className="group/card relative bg-card rounded-xl overflow-hidden shadow-sm hover:shadow-xl hover:shadow-primary/5 transition-all duration-300 ring-1 ring-border/50 hover:ring-primary/20 hover:-translate-y-1">
      {/* Image */}
      <div
        className="relative aspect-video bg-muted/30 cursor-pointer overflow-hidden"
        onClick={() => setSelectedFrameId(frame.id)}
      >
        {imageUrl && !imageError ? (
          <img
            src={imageUrl}
            alt={`Screenshot from ${frame.app_name}`}
            className="w-full h-full object-cover transition-transform duration-500 group-hover/card:scale-105"
            onError={() => setImageError(true)}
            loading="lazy"
          />
        ) : (
          <div className="w-full h-full flex items-center justify-center">
            <Monitor className="h-12 w-12 text-muted-foreground/20" />
          </div>
        )}

        {/* Overlays */}
        <div className="absolute inset-0 bg-gradient-to-t from-black/60 via-transparent to-transparent opacity-0 group-hover/card:opacity-100 transition-opacity duration-300" />

        <div className="absolute top-3 right-3 px-2.5 py-1 bg-black/60 backdrop-blur-md text-white text-[10px] font-medium tracking-wide rounded-full border border-white/10 shadow-sm">
          {formatRelativeTime(frame.timestamp)}
        </div>
      </div>

      {/* Content */}
      <div className="p-4 space-y-3">
        {/* App Info */}
        <div className="flex items-start justify-between gap-3">
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
              <div className="p-1.5 rounded-md bg-secondary text-primary">
                <Monitor className="h-3 w-3" />
              </div>
              <span className="truncate">{frame.app_name}</span>
            </div>
            {frame.window_name && (
              <div className="text-xs text-muted-foreground/80 truncate mt-1.5 pl-8">
                {frame.window_name}
              </div>
            )}
          </div>
        </div>

        {/* OCR Text Preview */}
        {frame.ocr_text && (
          <div className="text-xs text-muted-foreground line-clamp-2 leading-relaxed pl-8 group-hover/card:text-foreground/80 transition-colors">
            {highlightedParts.map((part, index) =>
              part.isHighlight ? (
                <mark key={index} className="bg-yellow-300 dark:bg-yellow-600">
                  {part.text}
                </mark>
              ) : (
                <span key={index}>{part.text}</span>
              )
            )}
          </div>
        )}

        {/* Tags */}
        <div className="pt-2 pl-8 flex flex-wrap items-center gap-1.5 min-h-[1.5rem]">
          {frame.tags.map((tag) => (
            <div
              key={tag.id}
              className="group/tag flex items-center gap-1 px-2 py-0.5 bg-secondary/50 hover:bg-secondary text-secondary-foreground rounded-md text-[10px] font-medium transition-colors border border-transparent hover:border-border"
              style={tag.color ? { backgroundColor: `${tag.color}20`, color: tag.color, borderColor: `${tag.color}40` } : undefined}
            >
              <TagIcon className="h-2.5 w-2.5" />
              <span>{tag.name}</span>
              <button
                onClick={(e) => { e.stopPropagation(); handleRemoveTag(tag.id); }}
                className="opacity-0 group-hover/tag:opacity-100 p-0.5 hover:bg-black/5 rounded-full transition-all"
              >
                <X className="h-2.5 w-2.5" />
              </button>
            </div>
          ))}

          {/* Add Tag Button */}
          <div className="relative" ref={tagMenuRef}>
            <button
              onClick={() => setShowTagMenu(!showTagMenu)}
              aria-label="Add tag to frame"
              aria-expanded={showTagMenu}
              className="opacity-0 group-hover/card:opacity-100 transition-opacity flex items-center gap-1 px-2 py-0.5 border border-dashed border-border rounded-md text-[10px] text-muted-foreground hover:text-foreground hover:bg-secondary/50"
            >
              <Plus className="h-2.5 w-2.5" />
              <span>Tag</span>
            </button>

            {/* Tag Menu */}
            {showTagMenu && availableTags.length > 0 && (
              <div
                className="absolute top-full left-0 mt-2 bg-popover border border-border rounded-lg shadow-xl shadow-black/10 z-50 min-w-[160px] p-1 animate-in fade-in zoom-in-95 duration-100"
                role="menu"
                aria-label="Available tags"
              >
                <div className="text-[10px] font-medium text-muted-foreground px-2 py-1.5 uppercase tracking-wider">
                  Select Tag
                </div>
                <div className="max-h-32 overflow-y-auto">
                  {availableTags.map((tag) => (
                    <button
                      key={tag.id}
                      onClick={() => handleAddTag(tag.id)}
                      role="menuitem"
                      className="w-full px-2 py-1.5 text-left text-xs hover:bg-accent rounded-md transition-colors flex items-center gap-2"
                    >
                      <div
                        className="w-2 h-2 rounded-full ring-1 ring-inset ring-black/10"
                        style={{ backgroundColor: tag.color || '#888' }}
                        aria-hidden="true"
                      />
                      <span>{tag.name}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
