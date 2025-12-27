# Complete File Index - ScreenSearch Frontend

Base Directory: `screensearch-ui/`

*Last updated: v0.3.0 - AI-First UI Redesign*

## Configuration Files (8)

1. `package.json`
2. `tsconfig.json`
3. `tsconfig.node.json`
4. `vite.config.ts`
5. `tailwind.config.js` - Extended theme with glassmorphism, cyan accents
6. `postcss.config.js`
7. `.eslintrc.cjs`
8. `.gitignore`

## Entry & HTML Files (3)

9. `index.html`
10. `src/main.tsx`
11. `src/App.tsx` - Integrates SearchInvite modal

## Type Definitions (2)

12. `src/types/index.ts`
13. `src/vite-env.d.ts`

## API Integration (1)

14. `src/api/client.ts`

## React Query Hooks (4)

15. `src/hooks/useSearch.ts`
16. `src/hooks/useFrames.ts`
17. `src/hooks/useTags.ts`
18. `src/hooks/useHealth.ts`

## State Management (1)

19. `src/store/useStore.ts` - Extended with search modal + sidebar state

## Utility Functions (2)

20. `src/lib/utils.ts`
21. `src/lib/animations.ts` - Framer Motion animation variants (v0.3.0)

## Core Components (10)

22. `src/components/Header.tsx`
23. `src/components/SearchBar.tsx`
24. `src/components/Timeline.tsx`
25. `src/components/FrameCard.tsx`
26. `src/components/FrameModal.tsx`
27. `src/components/SettingsPanel.tsx`
28. `src/components/TagManager.tsx`
29. `src/components/LoadingSkeleton.tsx`
30. `src/components/ErrorBoundary.tsx`
31. `src/components/Sidebar.tsx` - Collapsible with framer-motion (v0.3.0)
32. `src/components/Logo.tsx` - Updated with collapsed mode support (v0.3.0)

## Search Components (v0.3.0)

33. `src/components/search/SearchInvite.tsx` - Cmd+K modal with glassmorphism
34. `src/components/search/SmartAnswer.tsx` - AI answer with activity sources

## Dashboard Components (v0.3.0)

35. `src/components/dashboard/DailyDigestCard.tsx` - AI daily summaries
36. `src/components/dashboard/MemoryStatusGauge.tsx` - RAG indexing status
37. `src/components/dashboard/ProductivityPulse.tsx` - Cyan gradient chart

## UI Components (v0.3.0)

38. `src/components/ui/GlassCard.tsx` - Glassmorphism container with glow variants
39. `src/components/ui/CircularGauge.tsx` - SVG radial progress with cyan gradient
40. `src/components/ui/ComingSoonCard.tsx` - Placeholder for upcoming features

## Pages

41. `src/pages/Dashboard.tsx` - ScreenSearch Intel home dashboard

## Styles (1)

42. `src/index.css` - CSS design tokens, glassmorphism utilities, cyan accents

## Documentation (8)

43. `README.md`
44. `QUICKSTART.md`
45. `INSTALLATION.md`
46. `FEATURES.md`
47. `Frontend_IMPLEMENTATION_SUMMARY.md`
48. `PROJECT_COMPLETE.md`
49. `FILE_INDEX.md` (this file)

## Environment (1)

50. `.env.example`

---

## Total Files: 50+

## Directory Structure

```
screensearch-ui/
├── index.html
├── package.json
├── tsconfig.json
├── tsconfig.node.json
├── vite.config.ts
├── tailwind.config.js
├── postcss.config.js
├── .eslintrc.cjs
├── .gitignore
├── .env.example
│
├── README.md
├── QUICKSTART.md
├── INSTALLATION.md
├── FEATURES.md
├── Frontend_IMPLEMENTATION_SUMMARY.md
├── PROJECT_COMPLETE.md
└── FILE_INDEX.md
│
└── src/
    ├── main.tsx
    ├── App.tsx
    ├── index.css              # Design tokens, glassmorphism utilities
    ├── vite-env.d.ts
    │
    ├── api/
    │   └── client.ts
    │
    ├── components/
    │   ├── Header.tsx
    │   ├── SearchBar.tsx
    │   ├── Timeline.tsx
    │   ├── FrameCard.tsx
    │   ├── FrameModal.tsx
    │   ├── SettingsPanel.tsx
    │   ├── TagManager.tsx
    │   ├── LoadingSkeleton.tsx
    │   ├── ErrorBoundary.tsx
    │   ├── Sidebar.tsx        # Collapsible (v0.3.0)
    │   ├── Logo.tsx           # Collapsed mode (v0.3.0)
    │   │
    │   ├── search/            # v0.3.0
    │   │   ├── SearchInvite.tsx
    │   │   └── SmartAnswer.tsx
    │   │
    │   ├── dashboard/         # v0.3.0
    │   │   ├── DailyDigestCard.tsx
    │   │   ├── MemoryStatusGauge.tsx
    │   │   └── ProductivityPulse.tsx
    │   │
    │   └── ui/                # v0.3.0
    │       ├── GlassCard.tsx
    │       ├── CircularGauge.tsx
    │       └── ComingSoonCard.tsx
    │
    ├── pages/
    │   └── Dashboard.tsx      # ScreenSearch Intel
    │
    ├── hooks/
    │   ├── useSearch.ts
    │   ├── useFrames.ts
    │   ├── useTags.ts
    │   └── useHealth.ts
    │
    ├── lib/
    │   ├── utils.ts
    │   └── animations.ts      # Framer Motion (v0.3.0)
    │
    ├── store/
    │   └── useStore.ts        # Extended state (v0.3.0)
    │
    └── types/
        └── index.ts
```

## Key Entry Points

| Purpose | File |
|---------|------|
| Development | `src/main.tsx` |
| Configuration | `vite.config.ts` |
| Styles & Design Tokens | `src/index.css` |
| API Client | `src/api/client.ts` |
| Main App | `src/App.tsx` |
| Search Modal | `src/components/search/SearchInvite.tsx` |
| Dashboard | `src/pages/Dashboard.tsx` |
| Animation System | `src/lib/animations.ts` |

## Quick Commands

```bash
# Navigate to project
cd screensearch-ui

# Install dependencies
npm install

# Start development server
npm run dev

# Build for production
npm run build

# Preview production build
npm run preview

# Run linter
npm run lint
```

## Import Paths

When importing in your code:

```typescript
// Core Components
import { Header } from '@/components/Header'
import { Sidebar } from '@/components/Sidebar'
import { Timeline } from '@/components/Timeline'

// Search Components (v0.3.0)
import { SearchInvite } from '@/components/search/SearchInvite'
import { SmartAnswer } from '@/components/search/SmartAnswer'

// Dashboard Components (v0.3.0)
import { DailyDigestCard } from '@/components/dashboard/DailyDigestCard'
import { ProductivityPulse } from '@/components/dashboard/ProductivityPulse'

// UI Components (v0.3.0)
import { GlassCard } from '@/components/ui/GlassCard'
import { CircularGauge } from '@/components/ui/CircularGauge'

// Hooks
import { useSearch } from '@/hooks/useSearch'
import { useFrames } from '@/hooks/useFrames'
import { useTags } from '@/hooks/useTags'

// API
import { apiClient } from '@/api/client'

// Store
import { useStore } from '@/store/useStore'

// Types
import type { Frame, Tag, SearchParams } from '@/types'

// Utils & Animations (v0.3.0)
import { formatDate, cn } from '@/lib/utils'
import { modalVariants, fadeInUp } from '@/lib/animations'
```

Note: `@/` is aliased to `src/` in vite.config.ts
