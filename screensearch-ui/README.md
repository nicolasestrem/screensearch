# ScreenSearch - Frontend UI

Modern React-based web interface for ScreenSearch, a Windows screen capture and OCR tool. Features an AI-First UI with a Brutalist / Minimalist Paper design system, light and dark modes, search modal (Cmd+K), and sharp editorial typography.

## Features

### Core Features
- **Real-time Search**: Search through captured screen content with auto-complete
- **Timeline View**: Visual timeline of all captured screens with thumbnails
- **Tag Management**: Organize captures with custom tags
- **Settings Panel**: Configure capture intervals, privacy controls, and database management
- **Dark Mode**: Full dark mode support with theme persistence and custom HSL paper variables
- **Responsive Design**: Desktop-optimized interface with fluid layouts
- **Performance Optimized**: < 100ms interaction response time

### Brutalist & Minimalist Paper Design (v0.4.0)
- **Paper & Ink Palette**: Custom warm cream/paper (`bg-paper`) backgrounds and deep ink (`text-ink`) text with a dark paper variant.
- **Zero Border Radius**: Crisp 90-degree sharp corners across the entire application interface.
- **Rule Borders**: Fine, solid 1px borders (`border-rule`) demarcating components and inputs.
- **Editorial Typography**: Styled headers and search inputs using Newsreader (Serif) typography, with Geist (Sans) for UI copy and Geist Mono (Monospace) for data metrics.
- **Search Modal (Cmd+K)**: Flat paper search interface with Smart Answers and activity sources.
- **Collapsible Sidebar**: Zero border-radius sidebar with active states marked by clear border highlights.
- **ScreenSearch Intel Dashboard**: AI-powered productivity insights with Daily Digest, Memory Status gauge, and a minimalist Productivity Pulse chart.

## Tech Stack

- **React 18** - UI framework
- **TypeScript** - Type safety
- **Vite** - Build tool and dev server
- **TanStack Query** - Data fetching and caching
- **Zustand** - Global state management
- **Tailwind CSS** - Styling with custom brutalist/paper theme tokens
- **Framer Motion** - Animation library (v0.3.0+)
- **Lucide React** - Icons
- **React Hot Toast** - Notifications
- **React Markdown** - Markdown rendering for AI responses
- **date-fns** - Date formatting
- **Axios** - HTTP client

## Getting Started

### Prerequisites

- Node.js 18+ and npm
- ScreenSearch backend running on `localhost:3131`

### Installation

```bash
# Install dependencies
npm install

# Start development server
npm run dev

# Build for production
npm run build

# Preview production build
npm run preview
```

The application will be available at `http://localhost:5173`

## Project Structure

```
screensearch-ui/
├── src/
│   ├── api/              # API client and endpoints
│   │   └── client.ts     # Axios-based API client
│   ├── components/       # React components
│   │   ├── Header.tsx
│   │   ├── SearchBar.tsx
│   │   ├── Timeline.tsx
│   │   ├── FrameCard.tsx
│   │   ├── FrameModal.tsx
│   │   ├── TagManager.tsx
│   │   └── SettingsPanel.tsx
│   ├── hooks/            # React Query hooks
│   │   ├── useSearch.ts
│   │   ├── useFrames.ts
│   │   ├── useTags.ts
│   │   └── useHealth.ts
│   ├── store/            # Zustand store
│   │   └── useStore.ts
│   ├── types/            # TypeScript definitions
│   │   └── index.ts
│   ├── lib/              # Utility functions
│   │   └── utils.ts
│   ├── App.tsx           # Main app component
│   ├── main.tsx          # Entry point
│   └── index.css         # Global styles
├── public/               # Static assets
├── package.json
├── tsconfig.json
├── vite.config.ts
└── tailwind.config.js
```

## API Integration

The frontend communicates with the backend API on `localhost:3131`. The Vite dev server proxies API requests through `/api` to avoid CORS issues.

### Key Endpoints

- `GET /health` - System health check
- `GET /search` - Search screen captures
- `GET /frames` - List captured frames
- `GET /tags` - List all tags
- `POST /tags` - Create new tag
- `POST /automation/*` - Computer automation endpoints

## Development

### Environment Setup

The application uses Vite's built-in proxy configuration to connect to the backend:

```typescript
// vite.config.ts
server: {
  proxy: {
    '/api': {
      target: 'http://localhost:3131',
      changeOrigin: true,
      rewrite: (path) => path.replace(/^\/api/, ''),
    },
  },
}
```

### Keyboard Shortcuts

- `Cmd/Ctrl + K` - Focus search bar
- `Cmd/Ctrl + ,` - Open settings
- `Escape` - Close modal/panel

### Code Style

- TypeScript strict mode enabled
- ESLint with React hooks plugin
- Functional components with hooks
- CSS-in-JS avoided in favor of Tailwind

## Performance Optimization

- React Query caching with intelligent stale times
- Lazy loading of frame images
- Debounced search input (300ms)
- Optimistic UI updates for mutations
- Virtualization ready for large datasets

## Building for Production

```bash
# Build optimized production bundle
npm run build

# Output will be in dist/ directory
# Serve with any static file server
```

The production build is optimized with:
- Code splitting
- Tree shaking
- Asset optimization
- CSS minification

## Browser Support

- Chrome/Edge 90+
- Firefox 88+
- Safari 14+

## License

Part of the ScreenSearch project.
