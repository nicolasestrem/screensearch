# Stubs and Incomplete Features

This document tracks all stub implementations, placeholder functionality, and features that are planned but not yet fully implemented in ScreenSearch.

---

## UI Features (Coming Soon)

### Knowledge Graph
- **Location**: `screensearch-ui/src/pages/Dashboard.tsx:31-34`
- **Status**: Placeholder card displayed
- **Description**: Visual relationship mapping between captured content, applications, and topics
- **Sidebar entry**: `screensearch-ui/src/components/Sidebar.tsx:23`

### Analytics Dashboard
- **Location**: `screensearch-ui/src/pages/Dashboard.tsx:36-39`
- **Status**: Placeholder card displayed
- **Description**: Advanced analytics with productivity metrics, trends, and insights
- **Sidebar entry**: `screensearch-ui/src/components/Sidebar.tsx:24`

### Daily Digest Fallback
- **Location**: `screensearch-ui/src/components/dashboard/DailyDigestCard.tsx:93`
- **Status**: STUB comment - falls back to null when AI not configured
- **Description**: Should provide simulated/cached data when AI provider unavailable

---

## Backend Features (Not Implemented)

### Local Vision Model Inference
- **Location**: `screensearch-vision/src/local_model.rs`
- **Status**: Stub implementation - returns error
- **TODOs**:
  - Line 12: Load model weights (Moondream2 / TinyLlama)
  - Line 20: Implement actual inference using candle-transformers
- **Current behavior**: Returns error directing users to use Ollama/External provider
- **Related**: `screensearch-api/src/workers/vision_worker.rs:55-58`

### Server Uptime Tracking
- **Location**: `screensearch-api/src/handlers/system.rs:52`
- **Status**: TODO - returns `None`
- **Description**: Track and report server uptime in health endpoint

### Browser URL Extraction
- **Location**: `screensearch-capture/README.md:90`
- **Status**: Stub implementation
- **Description**: Extract current URL from browser windows during capture

### Plugin System
- **Location**: Referenced in `docs/developer-guide.md:1882`
- **Status**: Not implemented
- **Description**: Extensibility system for custom capture processors or integrations

---

## Frontend Features (Not Implemented)

Listed in `screensearch-ui/Frontend_IMPLEMENTATION_SUMMARY.md:334-337`:

### Infinite Scroll
- **Status**: Not implemented (pagination used instead)
- **Description**: Continuous loading of frames as user scrolls

### Bulk Operations
- **Status**: Not implemented
- **Description**: Select and operate on multiple frames at once (delete, tag, export)

### Search History
- **Status**: Not implemented
- **Description**: Track and recall previous search queries

### Frame Comparison
- **Status**: Not implemented
- **Description**: Side-by-side comparison of two or more captured frames

---

## Platform Support (Coming Soon)

### MacOS Support
- **Location**: `screensearch-website/index.tsx:157`
- **Status**: Coming soon
- **Description**: Native MacOS screen capture and OCR

### Linux Support
- **Location**: `screensearch-website/index.tsx:157`
- **Status**: Coming soon
- **Description**: Native Linux screen capture and OCR

---

## Future Enhancements (Documented in Architecture)

From `docs/architecture.md`:

### Optional Authentication (Line 2753)
- **Status**: Not implemented
- **Description**: API key or token-based authentication for REST endpoints

### PII Detection (Line 2778)
- **Status**: Not implemented
- **Description**: Automatic detection and redaction of personally identifiable information

### Web-based UI Deployment (Line 3086)
- **Status**: Partially implemented
- **Description**: Currently embedded; future standalone deployment options

---

## How to Contribute

When implementing a stub or incomplete feature:

1. Remove the `// TODO:`, `// STUB:`, or `// Not implemented` comment
2. Update this document to mark the feature as completed
3. Add appropriate tests
4. Update CHANGELOG.md with the new functionality
5. Update relevant documentation (README.md, CLAUDE.md, user-guide.md)

---

## Legend

| Status | Meaning |
|--------|---------|
| **Placeholder** | UI element exists but feature not functional |
| **Stub** | Code structure exists but returns mock/error |
| **TODO** | Inline comment marking incomplete code |
| **Not implemented** | Documented feature that doesn't exist yet |
| **Coming Soon** | Publicly announced as upcoming feature |

---

*Last updated: 2025-12-27 (v0.3.0 - AI-First UI Redesign)*

---

## AI-First UI Redesign (v0.3.0)

The following features were implemented as part of the AI-First UI redesign:

### Fully Implemented

| Feature | Location | Status |
|---------|----------|--------|
| **Search Modal (Cmd+K)** | `src/components/search/SearchInvite.tsx` | ✅ Complete |
| **Smart Answer Card** | `src/components/search/SmartAnswer.tsx` | ✅ Complete |
| **Collapsible Sidebar** | `src/components/Sidebar.tsx` | ✅ Complete |
| **Cyan Accent Design System** | `src/index.css` | ✅ Complete |
| **Glassmorphism Cards** | `src/components/ui/GlassCard.tsx` | ✅ Complete |
| **Circular Gauge (Memory Status)** | `src/components/ui/CircularGauge.tsx` | ✅ Complete |
| **Productivity Pulse Chart** | `src/components/dashboard/ProductivityPulse.tsx` | ✅ Complete |
| **Daily Digest Card** | `src/components/dashboard/DailyDigestCard.tsx` | ✅ Complete |
| **Framer Motion Animations** | `src/lib/animations.ts` | ✅ Complete |

### Visual Stubs (Badge: `badge-simulated`)

The following components show a visual stub indicator when data is not available:

| Component | Stub Trigger | Visual Indicator |
|-----------|--------------|------------------|
| **Daily Digest** | AI provider not configured | "Setup Required" badge + setup prompt |
| **Memory Status** | Embeddings API unavailable | Generic "unavailable" message |
| **Smart Answer** | Query empty or AI unavailable | "Configure AI provider" prompt |

### Not Yet Implemented (Coming Soon Cards)

| Feature | Location | Description |
|---------|----------|-------------|
| **Knowledge Graph** | Dashboard | Visual relationship mapping (placeholder card) |
| **Analytics** | Dashboard | Productivity metrics dashboard (placeholder card) |

---

## Design System Notes

The AI-First UI redesign uses the following visual system for incomplete features:

1. **`badge-coming-soon`** - Used in sidebar for features not yet available
2. **`badge-simulated`** - Used when showing placeholder/mock data
3. **"Setup Required" prompt** - Used when feature requires configuration

All stubs are clearly marked visually so users understand the feature state.
