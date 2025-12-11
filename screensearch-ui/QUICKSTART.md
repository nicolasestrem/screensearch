# Quick Start Guide

## Prerequisites

1. **Node.js 18+** installed
2. **ScreenSearch backend** running on `localhost:3131`

## Installation & Running

### Option 1: Quick Start (Recommended)

```bash
# Navigate to the UI directory
cd "C:\Users\nicol\Desktop\ScreenSearch\screensearch-ui"

# Install dependencies (first time only)
npm install

# Start development server
npm run dev
```

The app will be available at: `http://localhost:3000`

### Option 2: Production Build

```bash
# Build for production
npm run build

# Preview production build
npm run preview
```

## First Time Setup

1. Make sure the backend API is running on `localhost:3131`
2. Test the connection: Open `http://localhost:3131/health` in your browser
3. If the backend is running, you should see a JSON response
4. Start the frontend with `npm run dev`
5. Open `http://localhost:5173` in your browser

## Troubleshooting

### Backend Connection Issues

If you see "Disconnected" in the header:

1. Verify backend is running: `http://localhost:3131/health`
2. Check if the port 3131 is correct
3. Ensure no firewall is blocking the connection

### Build Errors

If you get TypeScript errors:

```bash
# Clear cache and reinstall
rm -rf node_modules package-lock.json
npm install
```

### Port Already in Use

If port 3000 is already in use:

```bash
# Use a different port
npm run dev -- --port 3001
```

## Development Tips

### Keyboard Shortcuts

- `Ctrl/Cmd + K` - Focus search bar
- `Ctrl/Cmd + ,` - Open settings
- `Escape` - Close modals/panels

### Hot Module Replacement

The dev server supports hot reload. Changes to components will update instantly without losing state.

### API Proxy

The dev server proxies API calls through `/api` to `http://localhost:3131`. This avoids CORS issues during development.

### Dark Mode

Dark mode is enabled by default and persists in localStorage. Toggle using the moon/sun icon in the header.

## Next Steps

1. **Search**: Try searching for text captured from your screen
2. **Tags**: Create tags to organize your captures
3. **Settings**: Configure capture intervals and privacy controls
4. **Timeline**: Browse your screen capture history

## Common Issues

**Q: Why is the timeline empty?**
A: The backend needs to capture some screens first. Wait a few minutes for the capture loop to run.

**Q: Images not loading?**
A: Check that the backend has write permissions to store screenshots.

**Q: Search not returning results?**
A: Ensure OCR is enabled in the backend and that frames contain text.

## Support

For issues or questions, refer to the main project documentation or check the backend logs for errors.
