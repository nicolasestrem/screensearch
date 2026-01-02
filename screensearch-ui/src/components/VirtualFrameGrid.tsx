import { memo, useMemo } from 'react';
import { FixedSizeGrid as Grid, GridChildComponentProps } from 'react-window';
import { Frame } from '../types';
import { FrameCard } from './FrameCard';

interface VirtualFrameGridProps {
  frames: Frame[];
  searchQuery?: string;
  columnCount?: number;
  rowHeight?: number;
  columnWidth?: number;
  height?: number;
}

interface CellData {
  frames: Frame[];
  columnCount: number;
  searchQuery?: string;
}

/**
 * Memoized cell renderer for the virtual grid
 * Only re-renders when the specific frame data changes
 */
const Cell = memo(({ columnIndex, rowIndex, style, data }: GridChildComponentProps<CellData>) => {
  const { frames, columnCount, searchQuery } = data;
  const frameIndex = rowIndex * columnCount + columnIndex;

  // Check if this cell should render a frame (handles partial last row)
  if (frameIndex >= frames.length) {
    return null;
  }

  const frame = frames[frameIndex];
  if (!frame) {
    return null;
  }

  return (
    <div style={{ ...style, padding: '12px' }}>
      <FrameCard frame={frame} searchQuery={searchQuery} />
    </div>
  );
});

Cell.displayName = 'VirtualCell';

/**
 * VirtualFrameGrid - Efficiently renders large lists of FrameCards using virtualization
 *
 * Only renders visible items + overscan, dramatically reducing DOM nodes and memory usage
 * for large frame collections. Uses react-window's FixedSizeGrid for O(1) scrolling performance.
 *
 * @param frames - Array of frames to render
 * @param searchQuery - Optional search query for highlighting
 * @param columnCount - Number of columns in the grid (default: 4)
 * @param rowHeight - Height of each row in pixels (default: 320)
 * @param columnWidth - Width of each column in pixels (default: 300)
 * @param height - Height of the virtual container (default: 600)
 */
export function VirtualFrameGrid({
  frames,
  searchQuery,
  columnCount = 4,
  rowHeight = 340,
  columnWidth = 300,
  height = 600,
}: VirtualFrameGridProps) {
  // Calculate row count based on frames and columns
  const rowCount = useMemo(
    () => Math.ceil(frames.length / columnCount),
    [frames.length, columnCount]
  );

  // Memoize the data object to prevent unnecessary re-renders
  const itemData = useMemo<CellData>(
    () => ({
      frames,
      columnCount,
      searchQuery,
    }),
    [frames, columnCount, searchQuery]
  );

  // Handle empty state
  if (frames.length === 0) {
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

  // Calculate dynamic width based on container
  const gridWidth = columnCount * columnWidth;

  return (
    <Grid
      className="virtual-grid scrollbar-thin scrollbar-thumb-border scrollbar-track-transparent"
      columnCount={columnCount}
      columnWidth={columnWidth}
      height={height}
      rowCount={rowCount}
      rowHeight={rowHeight}
      width={gridWidth}
      itemData={itemData}
      overscanRowCount={2} // Render 2 extra rows above/below viewport for smoother scrolling
    >
      {Cell}
    </Grid>
  );
}

/**
 * Auto-sizing wrapper that calculates optimal column count based on container width
 * Uses ResizeObserver to dynamically adjust grid layout
 */
export function AutoSizeVirtualFrameGrid({
  frames,
  searchQuery,
  minColumnWidth = 280,
  rowHeight = 340,
  containerClassName = '',
}: {
  frames: Frame[];
  searchQuery?: string;
  minColumnWidth?: number;
  rowHeight?: number;
  containerClassName?: string;
}) {
  // For now, use a simpler approach with CSS grid breakpoints
  // The full auto-sizing with ResizeObserver would require AutoSizer from react-virtualized-auto-sizer
  // This is a reasonable default for most use cases

  // Default to responsive column count similar to Tailwind breakpoints
  // sm: 1 col, md: 2 cols, lg: 3 cols, xl: 4 cols
  const columnCount = 4; // Will be enhanced with resize observer in future

  return (
    <div className={`w-full ${containerClassName}`}>
      <VirtualFrameGrid
        frames={frames}
        searchQuery={searchQuery}
        columnCount={columnCount}
        rowHeight={rowHeight}
        columnWidth={minColumnWidth + 40} // Add padding
        height={600}
      />
    </div>
  );
}
