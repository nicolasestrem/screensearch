# ScreenSearch - Frontend UI

Modern React-based web interface for ScreenSearch, a Windows screen capture and OCR tool.

## Features

- **Real-time Search**: Search through captured screen content with auto-complete
- **Timeline View**: Visual timeline of all captured screens with thumbnails
- **Tag Management**: Organize captures with custom tags
- **Settings Panel**: Configure capture intervals, privacy controls, and database management
- **Dark Mode**: Full dark mode support with theme persistence
- **Responsive Design**: Desktop-optimized interface with fluid layouts
- **Performance Optimized**: < 100ms interaction response time

## Tech Stack

- **React 18** - UI framework
- **TypeScript** - Type safety
- **Vite** - Build tool and dev server
- **TanStack Query** - Data fetching and caching
- **Zustand** - Global state management
- **Tailwind CSS** - Styling
- **Lucide React** - Icons
- **React Hot Toast** - Notifications
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
