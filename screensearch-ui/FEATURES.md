# ScreenSearch UI - Features Documentation

## Core Components

### 1. SearchBar Component
**Location**: `src/components/SearchBar.tsx`

**Features**:
- Real-time search with 300ms debounce for performance
- Auto-complete suggestions from indexed content (triggered after 3 characters)
- Advanced filter panel with:
  - Date range picker (start and end dates)
  - Application name filter
  - Tag-based filtering with visual tag selector
- Filter state persistence during session
- Visual indicators for active filters
- Quick filter reset button

**User Interactions**:
- Type to search with live suggestions
- Click filter button to expand/collapse filter panel
- Select date range using native date inputs
- Click tags to add/remove from filter
- Clear all filters with one click

### 2. Timeline Component
**Location**: `src/components/Timeline.tsx`

**Features**:
- Grid and list view modes
- Frames grouped by date with collapsible sections
- Pagination (20 items per page)
- Automatic page reset on filter changes
- Loading states with skeleton screens
- Empty state messaging
- Real-time frame count display
- Smooth animations for content transitions

**User Interactions**:
- Toggle between grid/list views
- Navigate pages with previous/next buttons
- Click frames to open modal view
- Scroll through grouped date sections

### 3. FrameCard Component
**Location**: `src/components/FrameCard.tsx`

**Features**:
- Thumbnail image with fallback icon
- Application and window name display
- Relative timestamp with hover tooltip for exact time
- OCR text preview (truncated to 200 characters)
- Search query highlighting in OCR text
- Tag display with color coding
- Inline tag management:
  - Add tags via dropdown menu
  - Remove tags with hover buttons
- Click to view full frame in modal

**User Interactions**:
- Click card to open detailed view
- Hover over tags to see remove button
- Click "Tag" button to add new tags
- View timestamp tooltip for exact capture time

### 4. FrameModal Component
**Location**: `src/components/FrameModal.tsx`

**Features**:
- Full-size screenshot display
- Complete OCR text with copy-to-clipboard
- Application and window context
- Exact timestamp display
- Tag visualization
- Keyboard navigation (Escape to close)
- Previous/Next navigation (ready for implementation)
- Backdrop click to close

**User Interactions**:
- Click backdrop or X button to close
- Press Escape key to close
- Copy OCR text to clipboard
- Navigate to adjacent frames (UI ready)

### 5. SettingsPanel Component
**Location**: `src/components/SettingsPanel.tsx`

**Features**:
- Slide-in panel from right side
- Capture status control (pause/resume)
- Appearance settings:
  - Dark mode toggle with persistence
- Capture configuration:
  - Interval slider (2-30 seconds)
  - Monitor selection dropdown
- Privacy controls:
  - Excluded applications list
  - Add/remove apps dynamically
- Database management:
  - Retention days configuration
  - Export data button
  - Clear all data button
- Integrated TagManager component

**User Interactions**:
- Click settings icon or use Ctrl/Cmd + , to open
- Pause/resume capture with single click
- Adjust capture interval with slider
- Add apps to exclusion list
- Manage tags inline

### 6. TagManager Component
**Location**: `src/components/TagManager.tsx`

**Features**:
- Tag CRUD operations (Create, Read, Update, Delete)
- Color picker for tag customization
- Inline editing mode
- Confirmation for deletions
- Empty state messaging
- Animated form transitions

**User Interactions**:
- Create new tags with name and color
- Edit existing tags inline
- Delete tags with confirmation
- Color selection with native picker

### 7. Header Component
**Location**: `src/components/Header.tsx`

**Features**:
- Application branding and logo
- Real-time health status indicator:
  - Green pulse for "ok"
  - Yellow for "degraded"
  - Red for "error"
- Frame count display
- Last capture timestamp
- Dark mode toggle
- Settings panel trigger
- Responsive design (mobile-friendly)

**User Interactions**:
- Toggle dark mode with moon/sun icon
- Open settings panel
- View system health at a glance

## API Integration

### API Client
**Location**: `src/api/client.ts`

**Features**:
- Axios-based HTTP client
- Centralized endpoint management
- Request/response interceptors for logging
- Automatic error handling
- Type-safe request/response models
- 30-second timeout
- Blob handling for images

**Endpoints Implemented**:
- `GET /health` - System health check
- `GET /search` - Search with filters
- `GET /search/keywords` - Autocomplete
- `GET /frames` - List frames
- `GET /frames/:id` - Get single frame
- `GET /frames/:id/image` - Get frame image
- `GET /tags` - List tags
- `POST /tags` - Create tag
- `PUT /tags/:id` - Update tag
- `DELETE /tags/:id` - Delete tag
- `POST /frames/:id/tags/:tagId` - Add tag to frame
- `DELETE /frames/:id/tags/:tagId` - Remove tag from frame
- Automation endpoints (find-elements, click, type, scroll, etc.)

## React Query Hooks

### useSearch Hook
**Location**: `src/hooks/useSearch.ts`

- Caching with 30-second stale time
- Automatic refetching disabled on window focus
- Keyword suggestions with 60-second cache

### useFrames Hook
**Location**: `src/hooks/useFrames.ts`

- Frame list with pagination support
- 10-second stale time for recent data
- Auto-refetch every 30 seconds
- Individual frame fetching
- Frame image loading with 5-minute cache
- Separate cache for images (10-minute garbage collection)

### useTags Hook
**Location**: `src/hooks/useTags.ts`

- Tag list with 60-second stale time
- Mutation hooks with optimistic updates
- Automatic cache invalidation on changes
- Toast notifications for all operations

### useHealth Hook
**Location**: `src/hooks/useHealth.ts`

- Auto-refetch every 10 seconds
- 3 retry attempts with 1-second delay
- Connection status monitoring

## State Management

### Zustand Store
**Location**: `src/store/useStore.ts`

**Persisted State**:
- Dark mode preference
- View mode (grid/list)

**Session State**:
- Search filters (query, date range, apps, tags)
- Selected frame ID for modal
- Settings panel open/closed

**Actions**:
- `toggleDarkMode()` - Switch theme
- `setFilters()` - Update search filters
- `resetFilters()` - Clear all filters
- `setViewMode()` - Change grid/list view
- `setSelectedFrameId()` - Open/close frame modal
- `toggleSettingsPanel()` - Show/hide settings

## Utility Functions

### Date Formatting
- `formatDate()` - Short date format
- `formatTime()` - 12-hour time format
- `formatDateTime()` - Combined date and time
- `formatRelativeTime()` - Human-readable relative time (e.g., "5m ago")

### Text Processing
- `highlightText()` - Add HTML marks for search highlighting
- `truncateText()` - Truncate with ellipsis
- `debounce()` - Debounce function calls

### Styling
- `cn()` - Conditional className utility using clsx

## Performance Optimizations

1. **Debounced Search**: 300ms delay prevents excessive API calls
2. **React Query Caching**: Intelligent cache management with stale times
3. **Lazy Image Loading**: Frame images loaded on-demand
4. **Optimistic Updates**: UI updates before server confirmation
5. **Pagination**: Only 20 frames loaded at a time
6. **Memoization**: React Query handles result memoization
7. **Skeleton Screens**: Loading states prevent layout shift

## Keyboard Shortcuts

- `Ctrl/Cmd + K` - Focus search input
- `Ctrl/Cmd + ,` - Open settings panel
- `Escape` - Close modal or panel

## Responsive Design

- Mobile-first approach with Tailwind breakpoints
- Desktop-optimized (as per requirements)
- Fluid layouts that adapt to viewport
- Touch-friendly controls on mobile

## Accessibility Features

1. **Semantic HTML**: Proper heading hierarchy and landmarks
2. **ARIA Labels**: Buttons and interactive elements labeled
3. **Keyboard Navigation**: Full keyboard support
4. **Focus Management**: Visible focus indicators
5. **Color Contrast**: WCAG AA compliant color ratios
6. **Alt Text**: Images have descriptive alt text

## Dark Mode

- System preference detection
- Manual toggle in header
- Persistent across sessions
- Smooth transitions between themes
- All components theme-aware

## Error Handling

1. **ErrorBoundary**: Catches React component errors
2. **API Error Handling**: Interceptors log and display errors
3. **Toast Notifications**: User-friendly error messages
4. **Loading States**: Clear feedback during operations
5. **Empty States**: Helpful messages when no data
6. **Retry Logic**: Automatic retries for failed requests

## Toast Notifications

Using `react-hot-toast`:
- Bottom-right position
- Theme-aware styling
- Success, error, and info variants
- Auto-dismiss after 3 seconds
- Icon indicators

## Future Enhancements (Ready for Implementation)

1. **Frame Navigation**: Previous/Next buttons in modal (UI ready)
2. **Infinite Scroll**: Alternative to pagination
3. **Advanced Search**: Boolean operators, regex support
4. **Bulk Operations**: Select multiple frames for tagging
5. **Export Features**: Download selected frames
6. **Keyboard Shortcuts Panel**: Help modal showing all shortcuts
7. **Search History**: Recent searches dropdown
8. **Frame Comparison**: Side-by-side view
9. **OCR Confidence**: Display OCR accuracy scores
10. **Virtual Scrolling**: For extremely large datasets
