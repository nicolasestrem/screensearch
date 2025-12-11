import { useEffect, useState } from 'react';
import { X, Monitor, Clock, ChevronLeft, ChevronRight, Copy, Check, Tag as TagIcon, Plus } from 'lucide-react';
import { useStore } from '../store/useStore';
import { useFrame, useFrameImage } from '../hooks/useFrames';
import { useTags, useAddTagToFrame, useRemoveTagFromFrame } from '../hooks/useTags';
import { formatDateTime } from '../lib/utils';
import toast from 'react-hot-toast';

export function FrameModal() {
  const { selectedFrameId, setSelectedFrameId } = useStore();
  const [copied, setCopied] = useState(false);
  const [showTagPicker, setShowTagPicker] = useState(false);

  const { data: frame, isLoading } = useFrame(selectedFrameId || 0, !!selectedFrameId);
  const { data: imageUrl } = useFrameImage(selectedFrameId || 0, !!selectedFrameId);
  const { data: allTags = [] } = useTags();
  const addTagToFrame = useAddTagToFrame();
  const removeTagFromFrame = useRemoveTagFromFrame();

  useEffect(() => {
    if (selectedFrameId) {
      // Prevent body scroll when modal is open
      document.body.style.overflow = 'hidden';
      return () => {
        document.body.style.overflow = 'unset';
      };
    }
  }, [selectedFrameId]);

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!selectedFrameId) return;

      if (e.key === 'Escape') {
        setSelectedFrameId(null);
      }
      // Add arrow key navigation for next/prev frames if needed
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [selectedFrameId, setSelectedFrameId]);

  const handleCopyText = async () => {
    if (frame?.ocr_text) {
      const textToCopy = typeof frame.ocr_text === 'string'
        ? frame.ocr_text
        : JSON.stringify(frame.ocr_text);
      await navigator.clipboard.writeText(textToCopy);
      setCopied(true);
      toast.success('Text copied to clipboard');
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const handleAddTag = async (tagId: number) => {
    if (!selectedFrameId) return;
    try {
      await addTagToFrame.mutateAsync({ frameId: selectedFrameId, tagId });
      setShowTagPicker(false);
      toast.success('Tag added successfully');
    } catch (error: any) {
      // Extract specific error message from API response
      const message = error.response?.data?.error ||
                     error.response?.data?.message ||
                     error.message ||
                     'Failed to add tag';
      toast.error(message);
      console.error('Error adding tag:', error);
    }
  };

  const handleRemoveTag = async (tagId: number) => {
    if (!selectedFrameId) return;
    try {
      await removeTagFromFrame.mutateAsync({ frameId: selectedFrameId, tagId });
      toast.success('Tag removed successfully');
    } catch (error: any) {
      // Extract specific error message from API response
      const message = error.response?.data?.error ||
                     error.response?.data?.message ||
                     error.message ||
                     'Failed to remove tag';
      toast.error(message);
      console.error('Error removing tag:', error);
    }
  };

  // Get tags that are not already assigned to this frame
  const availableTags = allTags.filter(
    (tag) => !frame?.tags?.some((frameTag) => frameTag.id === tag.id)
  );

  // Helper function to safely display OCR text
  const getOcrText = () => {
    if (!frame?.ocr_text) return '';
    if (typeof frame.ocr_text === 'string') {
      return frame.ocr_text;
    }
    // Handle OCRTextData object or array
    return JSON.stringify(frame.ocr_text, null, 2);
  };

  if (!selectedFrameId) return null;

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-black/80 backdrop-blur-sm z-50 animate-fade-in"
        onClick={() => setSelectedFrameId(null)}
      />

      {/* Modal */}
      <div className="fixed inset-0 z-50 flex items-center justify-center p-4 pointer-events-none">
        <div
          className="bg-background border border-border rounded-lg shadow-2xl max-w-6xl w-full max-h-[90vh] overflow-hidden pointer-events-auto animate-slide-in"
          onClick={(e) => e.stopPropagation()}
        >
          {isLoading ? (
            <div className="flex items-center justify-center h-96">
              <div className="text-center">
                <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-primary mx-auto mb-4" />
                <p className="text-muted-foreground">Loading frame...</p>
              </div>
            </div>
          ) : frame ? (
            <div className="flex flex-col h-full">
              {/* Header */}
              <div className="flex items-center justify-between p-4 border-b border-border">
                <div className="flex items-center gap-3">
                  <Monitor className="h-5 w-5 text-muted-foreground" />
                  <div>
                    <h3 className="font-semibold">{frame.app_name}</h3>
                    {frame.window_name && (
                      <p className="text-sm text-muted-foreground">
                        {frame.window_name}
                      </p>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-4">
                  <div className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Clock className="h-4 w-4" />
                    <span>{formatDateTime(frame.timestamp)}</span>
                  </div>
                  <button
                    onClick={() => setSelectedFrameId(null)}
                    className="p-2 hover:bg-accent rounded-lg transition-colors"
                  >
                    <X className="h-5 w-5" />
                  </button>
                </div>
              </div>

              {/* Content */}
              <div className="flex-1 overflow-y-auto p-6 space-y-6">
                {/* Image */}
                {imageUrl && (
                  <div className="bg-muted rounded-lg overflow-hidden">
                    <img
                      src={imageUrl}
                      alt={`Screenshot from ${frame.app_name}`}
                      className="w-full h-auto"
                    />
                  </div>
                )}

                {/* OCR Text */}
                {frame.ocr_text && (
                  <div className="space-y-2">
                    <div className="flex items-center justify-between">
                      <h4 className="text-sm font-semibold uppercase text-muted-foreground">
                        Extracted Text
                      </h4>
                      <button
                        onClick={handleCopyText}
                        className="flex items-center gap-2 px-3 py-1.5 text-sm bg-secondary text-secondary-foreground rounded-lg hover:bg-secondary/80 transition-colors"
                      >
                        {copied ? (
                          <>
                            <Check className="h-4 w-4" />
                            <span>Copied</span>
                          </>
                        ) : (
                          <>
                            <Copy className="h-4 w-4" />
                            <span>Copy</span>
                          </>
                        )}
                      </button>
                    </div>
                    <div className="bg-card border border-border rounded-lg p-4">
                      <p className="text-sm whitespace-pre-wrap">{getOcrText()}</p>
                    </div>
                  </div>
                )}

                {/* Tags */}
                <div className="space-y-2">
                  <div className="flex items-center justify-between">
                    <h4 className="text-sm font-semibold uppercase text-muted-foreground flex items-center gap-2">
                      <TagIcon className="h-4 w-4" />
                      Tags
                    </h4>
                    {availableTags.length > 0 && (
                      <button
                        onClick={() => setShowTagPicker(!showTagPicker)}
                        className="flex items-center gap-1 px-2 py-1 text-xs bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
                      >
                        <Plus className="h-3 w-3" />
                        <span>Add Tag</span>
                      </button>
                    )}
                  </div>

                  {/* Tag Picker Dropdown */}
                  {showTagPicker && (
                    <div className="bg-card border border-border rounded-lg p-3 space-y-2 animate-slide-in">
                      <p className="text-xs text-muted-foreground">Select a tag to add:</p>
                      <div className="flex flex-wrap gap-2">
                        {availableTags.map((tag) => (
                          <button
                            key={tag.id}
                            onClick={() => handleAddTag(tag.id)}
                            disabled={addTagToFrame.isPending}
                            className="px-3 py-1.5 rounded-full text-sm transition-opacity hover:opacity-80 disabled:opacity-50"
                            style={{
                              backgroundColor: tag.color || '#888',
                              color: 'white',
                            }}
                          >
                            {tag.name}
                          </button>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Assigned Tags */}
                  {frame.tags.length > 0 ? (
                    <div className="flex flex-wrap gap-2">
                      {frame.tags.map((tag) => (
                        <div
                          key={tag.id}
                          className="group relative px-3 py-1.5 rounded-full text-sm"
                          style={{
                            backgroundColor: tag.color || '#888',
                            color: 'white',
                          }}
                        >
                          <span>{tag.name}</span>
                          <button
                            onClick={() => handleRemoveTag(tag.id)}
                            disabled={removeTagFromFrame.isPending}
                            className="absolute -top-1 -right-1 w-5 h-5 bg-destructive text-destructive-foreground rounded-full opacity-0 group-hover:opacity-100 transition-opacity disabled:opacity-50 flex items-center justify-center"
                          >
                            <X className="h-3 w-3" />
                          </button>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <p className="text-sm text-muted-foreground">No tags assigned</p>
                  )}
                </div>
              </div>

              {/* Footer Navigation */}
              <div className="flex items-center justify-between p-4 border-t border-border">
                <button className="flex items-center gap-2 px-4 py-2 bg-secondary text-secondary-foreground rounded-lg hover:bg-secondary/80 transition-colors">
                  <ChevronLeft className="h-4 w-4" />
                  <span>Previous</span>
                </button>
                <span className="text-sm text-muted-foreground">
                  Frame #{frame.id}
                </span>
                <button className="flex items-center gap-2 px-4 py-2 bg-secondary text-secondary-foreground rounded-lg hover:bg-secondary/80 transition-colors">
                  <span>Next</span>
                  <ChevronRight className="h-4 w-4" />
                </button>
              </div>
            </div>
          ) : (
            <div className="flex items-center justify-center h-96">
              <p className="text-muted-foreground">Frame not found</p>
            </div>
          )}
        </div>
      </div>
    </>
  );
}
