# Installation & Setup Guide

## Complete React Frontend for ScreenSearch

This guide will walk you through installing and running the ScreenSearch frontend application.

## Prerequisites

### Required
- **Node.js**: Version 18.0.0 or higher
- **npm**: Version 9.0.0 or higher (comes with Node.js)
- **ScreenSearch Backend**: Running on `localhost:3131`

### Verify Prerequisites

```bash
# Check Node.js version
node --version
# Should output: v18.x.x or higher

# Check npm version
npm --version
# Should output: 9.x.x or higher
```

## Installation Steps

### 1. Navigate to Project Directory

```bash
cd "C:\Users\nicol\Desktop\ScreenSearch\screensearch-ui"
```

### 2. Install Dependencies

```bash
npm install
```

This will install all required packages:
- React 18.3.1
- TypeScript 5.7.2
- Vite 5.4.11
- TanStack Query 5.59.20
- Tailwind CSS 3.4.15
- Zustand 5.0.1
- Axios 1.7.7
- And more...

**Expected output**: Dependencies should install without errors. This may take 2-5 minutes on first install.

### 3. Verify Installation

Check that `node_modules` directory was created:

```bash
ls node_modules
```

## Running the Application

### Development Mode (Recommended)

```bash
npm run dev
```

**Expected output**:
```
VITE v5.4.11  ready in 500 ms

➜  Local:   http://localhost:5173/
➜  Network: use --host to expose
➜  press h + enter to show help
```

The application will be available at: **http://localhost:5173**

### Production Build

To create an optimized production build:

```bash
# Build the application
npm run build

# Preview the production build
npm run preview
```

The build output will be in the `dist/` directory.

## Project Structure Overview

```
screensearch-ui/
├── src/
│   ├── api/                 # API client and HTTP requests
│   │   └── client.ts
│   ├── components/          # React components
│   │   ├── ErrorBoundary.tsx
│   │   ├── FrameCard.tsx
│   │   ├── FrameModal.tsx
│   │   ├── Header.tsx
│   │   ├── LoadingSkeleton.tsx
│   │   ├── SearchBar.tsx
│   │   ├── SettingsPanel.tsx
│   │   ├── TagManager.tsx
│   │   └── Timeline.tsx
│   ├── hooks/               # React Query hooks
│   │   ├── useFrames.ts
│   │   ├── useHealth.ts
│   │   ├── useSearch.ts
│   │   └── useTags.ts
│   ├── lib/                 # Utility functions
│   │   └── utils.ts
│   ├── store/               # Zustand state management
│   │   └── useStore.ts
│   ├── types/               # TypeScript type definitions
│   │   └── index.ts
│   ├── App.tsx              # Main application component
│   ├── main.tsx             # Application entry point
│   ├── index.css            # Global styles
│   └── vite-env.d.ts        # Vite type declarations
├── public/                  # Static assets
├── index.html               # HTML entry point
├── package.json             # Dependencies and scripts
├── tsconfig.json            # TypeScript configuration
├── vite.config.ts           # Vite configuration
├── tailwind.config.js       # Tailwind CSS configuration
├── postcss.config.js        # PostCSS configuration
├── .eslintrc.cjs            # ESLint configuration
├── .gitignore               # Git ignore rules
├── README.md                # Project overview
├── QUICKSTART.md            # Quick start guide
├── FEATURES.md              # Feature documentation
└── INSTALLATION.md          # This file
```

## Configuration

### Environment Variables

Create a `.env` file in the root directory (optional):

```bash
cp .env.example .env
```

Edit `.env`:
```
VITE_API_URL=http://localhost:3131
VITE_DEV_PORT=3000
```

### Vite Configuration

The Vite configuration (`vite.config.ts`) includes:
- React plugin for fast refresh
- Path aliases (`@/` points to `src/`)
- API proxy to avoid CORS issues

### TypeScript Configuration

Strict mode is enabled with:
- No implicit any
- Strict null checks
- No unchecked indexed access

## Testing the Installation

### 1. Check Backend Connection

Before starting the frontend, verify the backend is running:

```bash
curl http://localhost:3131/health
```

Or open in browser: http://localhost:3131/health

**Expected response**:
```json
{
  "status": "ok",
  "version": "1.0.0",
  "uptime": 12345,
  "frame_count": 42,
  "last_capture": "2025-12-10T10:30:00Z"
}
```

### 2. Start Frontend

```bash
npm run dev
```

### 3. Open Browser

Navigate to: http://localhost:3000

### 4. Verify Features

**Header**:
- Should show "ScreenSearch" logo
- Health status indicator (green dot if connected)
- Frame count display
- Dark mode toggle
- Settings button

**Search Bar**:
- Should render empty search input
- Filter button should be present

**Timeline**:
- If backend has captures, they should display
- If no captures, should show "No frames found" message

## Troubleshooting

### Issue: "Cannot find module 'vite'"

**Solution**: Reinstall dependencies
```bash
rm -rf node_modules package-lock.json
npm install
```

### Issue: "Port 3000 already in use"

**Solution**: Use a different port
```bash
npm run dev -- --port 3001
```

### Issue: "Network Error" in browser console

**Solution**:
1. Verify backend is running on port 3131
2. Check firewall settings
3. Try accessing http://localhost:3131/health directly

### Issue: TypeScript errors during build

**Solution**: Clear TypeScript cache
```bash
rm -rf node_modules/.vite
npm run dev
```

### Issue: Tailwind styles not loading

**Solution**: Verify PostCSS is configured
```bash
# Check if postcss.config.js exists
ls postcss.config.js

# Restart dev server
npm run dev
```

### Issue: Images not displaying

**Solution**:
1. Check browser console for 404 errors
2. Verify backend `/frames/:id/image` endpoint is working
3. Check CORS configuration on backend

## Development Workflow

### Hot Module Replacement (HMR)

Vite provides instant hot reload:
1. Save any file in `src/`
2. Browser updates automatically
3. State is preserved (React Fast Refresh)

### Code Linting

```bash
# Run ESLint
npm run lint

# Fix auto-fixable issues
npm run lint -- --fix
```

### Type Checking

```bash
# Run TypeScript compiler in check mode
npx tsc --noEmit
```

### Build for Production

```bash
# Create optimized build
npm run build

# Output will be in dist/ directory
# Includes:
# - Minified JavaScript
# - Optimized CSS
# - Code splitting
# - Asset hashing
```

## Browser Compatibility

**Supported Browsers**:
- Chrome/Edge 90+
- Firefox 88+
- Safari 14+

**Note**: Internet Explorer is NOT supported.

## Performance Benchmarks

Expected performance metrics:

- **Initial Load**: < 2 seconds on fast connection
- **Time to Interactive**: < 3 seconds
- **Search Response**: < 100ms (as per requirements)
- **Frame Card Render**: < 50ms per card
- **Bundle Size**: ~500KB gzipped

## Next Steps

After successful installation:

1. Read **QUICKSTART.md** for usage guide
2. Review **FEATURES.md** for feature documentation
3. Check **README.md** for project overview
4. Explore the UI and test all features

## Getting Help

If you encounter issues:

1. Check browser console for errors
2. Review backend logs for API errors
3. Verify all prerequisites are met
4. Check that ports 3000 and 3131 are available

## Additional Resources

- [Vite Documentation](https://vitejs.dev/)
- [React Documentation](https://react.dev/)
- [TanStack Query Documentation](https://tanstack.com/query)
- [Tailwind CSS Documentation](https://tailwindcss.com/)
- [TypeScript Documentation](https://www.typescriptlang.org/)
