# Screen Memory User Guide

## Table of Contents

1. [Introduction](#introduction)
2. [System Requirements](#system-requirements)
3. [Installation & Setup](#installation--setup)
4. [Configuration](#configuration)
5. [Using the Application](#using-the-application)
6. [Privacy Controls](#privacy-controls)
7. [Troubleshooting](#troubleshooting)
8. [FAQ](#faq)

---

## Introduction

### What is Screen Memory?

Screen Memory is a Windows screen capture and OCR (Optical Character Recognition) tool that continuously monitors and captures your screen content, extracts text, and stores everything in a searchable local database. Think of it as a personal search engine for everything you've seen on your computer screen.

### Key Features

- **Automatic Screen Capture**: Captures your screen at configurable intervals (default: every 3 seconds)
- **Intelligent OCR**: Extracts text from screenshots using Windows OCR API for fast and accurate results
- **Smart Frame Differencing**: Automatically skips unchanged screens to save resources
- **Searchable Database**: All captures stored locally in SQLite with full-text search capabilities
- **Web Interface**: Modern, responsive web UI for searching and browsing your screen history
- **Privacy First**: All data stays on your machine - no cloud, no tracking
- **REST API**: Query captured content and control automation via HTTP
- **Performance Optimized**: Less than 5% CPU usage and under 500MB RAM

### Use Cases

- **Research**: Find that article or quote you saw weeks ago but can't remember where
- **Productivity**: Track time spent on different applications and websites
- **Reference**: Recover information from closed windows or applications
- **Documentation**: Automatically capture meeting notes, code snippets, or design iterations
- **Debugging**: Trace back through your workflow to identify when issues started
- **Learning**: Review tutorials, documentation, or learning materials you've viewed

### How It Works

Screen Memory runs quietly in the background, capturing your screen every few seconds. Each capture is processed through OCR to extract any visible text, then stored in a local SQLite database along with metadata like timestamp, active application, and window title. The web interface provides powerful search and timeline views to help you find exactly what you're looking for.

---

## System Requirements

### Minimum Requirements

**Operating System**:
- Windows 10 (build 17763 or higher)
- Windows 11 (all versions)

**Hardware**:
- 2 GB RAM minimum (4 GB recommended)
- 500 MB free disk space (plus storage for captured frames)
- Dual-core processor or better

**Software Dependencies**:

For Backend (Rust Application):
- Rust toolchain 1.70 or higher
- Visual Studio Build Tools (for Windows API compilation)
- Windows OCR language pack (usually pre-installed)

For Frontend (Web Interface):
- Node.js 18.0.0 or higher
- npm 9.0.0 or higher (included with Node.js)
- Modern web browser (Chrome 90+, Firefox 88+, Safari 14+, Edge 90+)

### Recommended Configuration

**For Standard Use**:
- 8 GB RAM
- SSD storage for database
- Quad-core processor
- Single monitor setup

**For Heavy Use** (Multi-monitor, high resolution):
- 16 GB RAM
- 1 GB+ free disk space
- High-speed SSD
- Multi-core processor (6+ cores)

### Network Requirements

- No internet connection required
- Localhost network access (127.0.0.1)
- Ports 5173 and 3131 available

### Display Requirements

- Minimum resolution: 1280x720
- Recommended: 1920x1080 or higher
- Supports multi-monitor setups

---

## Installation & Setup

### Prerequisites Installation

#### Step 1: Install Rust Toolchain

1. Download Rust from: https://rustup.rs/
2. Run the installer and follow prompts
3. Verify installation:
   ```bash
   rustc --version
   cargo --version
   ```
   You should see version 1.70 or higher.

#### Step 2: Install Visual Studio Build Tools

1. Download from: https://visualstudio.microsoft.com/downloads/
2. Select "Build Tools for Visual Studio"
3. During installation, ensure these workloads are selected:
   - Desktop development with C++
   - Windows 10 SDK
4. Complete installation and restart your computer

#### Step 3: Install Node.js

1. Download from: https://nodejs.org/
2. Download the LTS version (18.x or higher)
3. Run installer with default options
4. Verify installation:
   ```bash
   node --version
   npm --version
   ```

### Backend Installation

#### Step 1: Clone or Download Project

Navigate to your desired installation directory:
```bash
cd "C:\Users\nicol\Desktop"
```

If using Git:
```bash
git clone https://github.com/nicolasestrem/screen-memories.git
cd screen-memories
```

#### Step 2: Build the Backend

```bash
# Development build (faster compilation, includes debug info)
cargo build

# Production build (optimized for performance)
cargo build --release
```

This process may take 5-15 minutes on first build as dependencies are downloaded and compiled.

#### Step 3: Verify Backend Installation

```bash
# Run the backend
cargo run --release
```

You should see output indicating:
- Database initialization
- Screen capture engine started
- OCR processor ready
- API server running on http://localhost:3131

### Frontend Installation

#### Step 1: Navigate to UI Directory

```bash
cd "\path\to\app\Screen Memory\screen-ui"
```

#### Step 2: Install Dependencies

```bash
npm install
```

This downloads all required packages (React, TypeScript, Vite, etc.). Takes 2-5 seconds depending on connection speed.

#### Step 3: Start the Frontend

For development:
```bash
npm run dev
```

For production build:
```bash
npm run build
npm run preview
```

The web interface will be available at: http://localhost:5173

### First-Time Setup

#### Step 1: Verify Backend Connection

Open http://localhost:3131/health in your browser. You should see JSON output:
```json
{
  "status": "ok",
  "version": "1.0.0",
  "uptime": 123,
  "frame_count": 0
}
```

#### Step 2: Access Web Interface

Open http://localhost:5173 in your browser.

**Header Checks**:
- Green status indicator (connected)
- Frame count showing (starts at 0)
- Dark mode toggle functional
- Settings button accessible

#### Step 3: Initial Configuration

1. Click the settings icon (gear) in the header
2. Review default capture settings
3. Add applications to exclude list (see Privacy Controls section)
4. Adjust capture interval if needed (default 3 seconds is recommended)

#### Step 4: Wait for First Captures

The system begins capturing immediately. After 10-15 seconds, you should see:
- Frame count increasing in header
- Frames appearing in timeline view
- OCR text extracted and searchable

---

## Configuration

All backend configuration is managed through `config.toml` in the project root directory. Edit this file before starting the application to customize behavior.

### Capture Settings

**Section**: `[capture]`

```toml
# Capture interval in milliseconds
# Lower = more captures, higher CPU usage
# Recommended: 3000-5000 (3-5 seconds)
interval_ms = 3000

# Enable frame differencing to skip unchanged screens
# Highly recommended for performance
enable_frame_diff = true

# Frame difference threshold (0.0 - 1.0)
# Lower = more sensitive to changes
# 0.006 = skip if less than 0.6% of screen changed
diff_threshold = 0.006

# Maximum frames buffered in memory before writing to database
# Higher = better performance, more memory usage
max_frames_buffer = 30

# Monitor indices to capture (empty array = all monitors)
# Example: [0] for primary monitor only
monitor_indices = []

# Include mouse cursor in screenshots
include_cursor = true

# Draw border around captured window
draw_border = false
```

**Common Adjustments**:
- **Slower Machine**: Increase `interval_ms` to 5000 or higher
- **High Activity**: Decrease `diff_threshold` to 0.003 for more sensitivity
- **Single Monitor**: Set `monitor_indices = [0]` to capture only primary display

### OCR Settings

**Section**: `[ocr]`

```toml
# OCR engine selection
# Options: "windows" (recommended), "tesseract"
engine = "windows"

# Minimum confidence threshold (0.0 - 1.0)
# OCR results below this are discarded
# Lower = more results but more false positives
min_confidence = 0.7

# Number of concurrent OCR worker threads
# Should not exceed CPU core count
worker_threads = 2

# Maximum retry attempts for failed OCR
max_retries = 3

# Delay between retries in milliseconds
retry_backoff_ms = 1000

# Store frames even if no text detected
# Disable to save disk space
store_empty_frames = false

# Internal queue size for frame processing
channel_buffer_size = 100

# Enable performance metrics logging
enable_metrics = true

# How often to report metrics (seconds)
metrics_interval_secs = 60
```

**Optimization Tips**:
- **Better Quality**: Increase `min_confidence` to 0.8 or higher
- **More Coverage**: Decrease `min_confidence` to 0.6
- **Multi-core CPU**: Increase `worker_threads` to 4 (but test for stability)
- **Disk Space Concerns**: Set `store_empty_frames = false`

### API Settings

**Section**: `[api]`

```toml
# Host address to bind API server
# 127.0.0.1 = localhost only (secure)
# 0.0.0.0 = accessible from network (use with caution)
host = "127.0.0.1"

# API server port
# Default: 3131
port = 3131

# CORS origin for web requests
# Empty string = permissive (allows all origins)
cors_origin = ""
```

**Security Note**: Keep `host = "127.0.0.1"` unless you specifically need network access. Never expose to internet without authentication.

### Database Settings

**Section**: `[database]`

```toml
# Path to SQLite database file
# Relative or absolute path
path = "screen_memories.db"

# Maximum connections in connection pool
max_connections = 50

# Minimum idle connections
min_connections = 3

# Timeout for acquiring connection (seconds)
acquire_timeout_secs = 10

# Enable Write-Ahead Logging for better concurrency
enable_wal = true

# Cache size in KB (negative value = KB of RAM)
# -2000 = use 2MB of memory for cache
cache_size_kb = -2000
```

**Performance Tuning**:
- **SSD Storage**: Keep defaults
- **HDD Storage**: Increase `cache_size_kb` to -5000 (5MB) for better performance
- **High Load**: Increase `max_connections` to 100

### Privacy Settings

**Section**: `[privacy]`

```toml
# Applications to exclude from capture
# Matches process name or window title (case-insensitive)
excluded_apps = [
    "1Password",
    "KeePass",
    "Bitwarden",
    "LastPass",
    "Password",
    "Bank",
    "Chase",
    "Wells Fargo",
    "PayPal",
]

# Automatically pause capture when screen locks
pause_on_lock = true
```

See **Privacy Controls** section for detailed guidance.

### Performance Settings

**Section**: `[performance]`

```toml
# Target maximum CPU usage percentage
# System will throttle if exceeded
max_cpu_percent = 5

# Maximum memory usage in MB
# Warning logged if exceeded
max_memory_mb = 500
```

### Logging Settings

**Section**: `[logging]`

```toml
# Log verbosity: "trace", "debug", "info", "warn", "error"
level = "info"

# Write logs to file
log_to_file = true

# Log file path
log_file = "screen_memories.log"

# Maximum log file size before rotation (MB)
max_log_size_mb = 100

# Number of rotated log files to keep
log_rotation_count = 5
```

**Debugging**: Set `level = "debug"` for troubleshooting, but remember to change back to "info" for normal use.

---

## Using the Application

### Starting the Application

#### Backend (Required)

Open a terminal in the project root:
```bash
cargo run --release
```

**Expected Output**:
```
[INFO] Screen Memory v1.0.0
[INFO] Initializing database: screen_memories.db
[INFO] Database ready with 0 frames
[INFO] Starting screen capture (interval: 3000ms)
[INFO] OCR processor ready (2 workers)
[INFO] API server listening on http://127.0.0.1:3131
```

Leave this terminal open. The application runs until you press Ctrl+C.

#### Frontend (Web Interface)

Open a second terminal in `screen-ui/`:
```bash
npm run dev
```

**Expected Output**:
```
VITE v5.4.11  ready in 500 ms

➜  Local:   http://localhost:5173/
➜  press h + enter to show help
```

Open http://localhost:5173 in your browser.

### Web Interface Overview

#### Header

**Left Side**:
- **Logo**: Screen Memory branding
- **Health Indicator**: Color-coded status
  - Green pulse: Connected and healthy
  - Yellow: Degraded performance
  - Red: Error or disconnected

**Right Side**:
- **Frame Count**: Total captures in database
- **Last Capture**: Timestamp of most recent capture
- **Dark Mode Toggle**: Moon/sun icon
- **Settings Button**: Gear icon

#### Main Content Area

Three primary views:

1. **Timeline View** (default)
2. **Search Results View** (when searching)
3. **Settings Panel** (slide-in from right)

### Search Functionality

#### Basic Search

1. Click the search bar at the top
2. Type your query (e.g., "meeting notes")
3. Results appear automatically after 300ms debounce
4. Click any result to view full details

#### Search Features

- **Real-time**: Results update as you type
- **Highlighted Matches**: Search terms highlighted in yellow
- **Auto-complete**: Suggestions appear after 3 characters
- **Relevance Ranked**: Most relevant results first

#### Advanced Filters

Click the "Filter" button to expand filter panel:

**Date Range**:
- Start date: Only results after this date
- End date: Only results before this date
- Both optional - leave blank for no time filter

**Application Filter**:
- Type or select application name
- Filters to captures from specific apps only
- Example: "Chrome" shows only browser captures

**Tag Filter**:
- Click tags to include in filter
- Multiple tags = OR logic (shows frames with any selected tag)
- Visual tag chips with colors

**Active Filters**:
- Filter button shows count badge when active
- Blue highlight indicates active filters
- "Clear Filters" button resets all

#### Search Tips

- **Exact Phrases**: Use quotes: `"exact phrase"`
- **Partial Matches**: Searches work with partial words
- **Case Insensitive**: Searches ignore case
- **Special Characters**: Most special characters are searchable

#### Keyboard Shortcuts

- `Ctrl/Cmd + K`: Focus search bar
- `Escape`: Clear search or close panels

### Timeline View

#### View Modes

**Grid View** (default):
- Cards in responsive grid layout
- Best for browsing visually
- Shows thumbnails prominently

**List View**:
- Vertical list with larger preview
- Better for reading OCR text
- More compact for scrolling

Toggle with buttons in top-right of timeline.

#### Date Grouping

Frames automatically grouped by date:
- Today
- Yesterday
- Earlier dates show full date
- Click date headers to collapse/expand groups

#### Frame Cards

Each card displays:

**Top Section**:
- Application icon and name
- Window title (if available)
- Relative timestamp (e.g., "5m ago")
  - Hover for exact timestamp

**Middle Section**:
- Screenshot thumbnail (click to enlarge)
- Fallback icon if image unavailable

**Bottom Section**:
- OCR text preview (first 200 characters)
- Search term highlighting
- "..." indicates truncated text

**Tags Section**:
- Colored tag chips
- Hover to see remove button (X)
- "Tag" button to add more tags

#### Pagination

- 20 frames per page
- "Previous" and "Next" buttons at bottom
- Page resets when filters change
- Page numbers displayed

### Frame Details Modal

Click any frame card to open detailed view:

**Header**:
- Application and window name
- Exact timestamp
- Close button (X)

**Image Section**:
- Full-size screenshot
- Click to zoom (if implemented)
- Scrollable if larger than viewport

**OCR Text Section**:
- Complete extracted text
- Copy button for clipboard
- Scrollable for long text

**Metadata Section**:
- Capture time
- Application details
- Tags with management buttons

**Navigation** (ready for implementation):
- Previous/Next frame buttons
- Keyboard arrows for navigation

**Closing**:
- Click X button
- Click backdrop (outside modal)
- Press Escape key

### Tag Management

Tags help organize and categorize your captures.

#### Creating Tags

**Via Settings Panel**:
1. Open Settings (gear icon)
2. Scroll to "Tag Management" section
3. Enter tag name
4. Click color picker to select color
5. Click "Create Tag"

**Default Colors**: System provides color palette, or use custom colors.

#### Adding Tags to Frames

**Method 1 - From Frame Card**:
1. Locate frame in timeline
2. Click "Tag" button on card
3. Select tag from dropdown
4. Tag appears immediately

**Method 2 - From Frame Modal**:
1. Open frame details
2. Click "Add Tag" button
3. Select from available tags
4. Multiple tags can be added

#### Removing Tags

**From Frame Card**:
1. Hover over tag chip
2. Click X button that appears
3. Tag removed immediately

**From Frame Modal**:
1. Click X on tag chip
2. Confirmation may appear
3. Tag relationship removed

#### Managing Tags

**In Settings Panel**:

**Edit Tag**:
1. Click edit icon (pencil) next to tag
2. Modify name or color
3. Click save
4. All frames update automatically

**Delete Tag**:
1. Click delete icon (trash) next to tag
2. Confirm deletion
3. Tag removed from all frames
4. Tag data deleted permanently

#### Tag Best Practices

- **Descriptive Names**: Use clear, searchable names
- **Color Coding**: Assign colors by category (e.g., red for urgent)
- **Consistency**: Use same tag names for similar content
- **Don't Over-tag**: 3-5 tags per frame is usually sufficient

### Settings Panel

Click the gear icon or press `Ctrl/Cmd + ,` to open.

#### Capture Status

**Pause/Resume**:
- Toggle button at top of panel
- Pauses all screen capturing
- OCR processing stops
- Database remains accessible
- API continues running

**Status Indicator**:
- Green: Actively capturing
- Gray: Paused

#### Appearance Settings

**Dark Mode**:
- Toggle between light and dark themes
- Setting persists across sessions
- Affects entire application
- Smooth transition animation

#### Capture Configuration

**Capture Interval**:
- Slider from 2 to 30 seconds
- Default: 3 seconds
- Live update (requires backend support)
- Lower = more captures, higher CPU usage

**Monitor Selection**:
- Dropdown shows all connected monitors
- "All Monitors" captures everything
- Select specific monitor to limit capture
- Useful for multi-monitor privacy

#### Privacy Controls

See dedicated **Privacy Controls** section below.

#### Database Management

**Retention Days**:
- Number field (1-365 days)
- Automatically deletes older frames
- 0 or blank = keep forever
- Cleanup runs daily

**Export Data**:
- Click "Export" button
- Downloads database backup
- Includes all frames and metadata
- Format: SQLite database file

**Clear All Data**:
- Click "Clear All Data" button
- Confirmation dialog appears
- Permanently deletes all captures
- Cannot be undone

#### Tag Management

Integrated TagManager component:
- View all tags
- Create new tags
- Edit existing tags
- Delete tags
- See tag usage count

### Keyboard Shortcuts Reference

| Shortcut | Action |
|----------|--------|
| `Ctrl/Cmd + K` | Focus search bar |
| `Ctrl/Cmd + ,` | Open/close settings panel |
| `Escape` | Close modal, panel, or clear search |
| `Enter` | Submit search |
| `/` | Focus search (alternative) |

---

## Privacy Controls

Screen Memory is designed with privacy as a priority. All data stays local, but you still need to configure exclusions for sensitive applications.

### Application Exclusion

#### Configuring Excluded Apps

**Method 1 - Config File** (Recommended):

Edit `config.toml`:
```toml
[privacy]
excluded_apps = [
    "1Password",
    "KeePass",
    "Bitwarden",
    "Password",
    "Bank",
    "Chase",
]
```

Restart backend for changes to take effect.

**Method 2 - Settings Panel** (if implemented):

1. Open Settings panel
2. Navigate to "Privacy" section
3. Click "Add Application"
4. Enter application name or window title
5. Click "Add"

#### How Exclusion Works

The system performs **case-insensitive substring matching** on:
- Process name (e.g., "1password.exe")
- Window title (e.g., "1Password - Main Vault")

If either matches an excluded pattern, the frame is skipped entirely - no capture, no OCR, no database entry.

#### Recommended Exclusions

**Password Managers**:
```toml
"1Password"
"KeePass"
"Bitwarden"
"LastPass"
"Dashlane"
```

**Financial Applications**:
```toml
"Bank"
"Chase"
"Wells Fargo"
"PayPal"
"Venmo"
"Credit"
"Tax"
"TurboTax"
```

**Email** (if containing sensitive info):
```toml
"Gmail - Inbox"
"Outlook - Mail"
```

**Private Browsing**:
```toml
"InPrivate"
"Incognito"
```

**Medical**:
```toml
"Medical"
"Health"
"Patient"
```

### Screen Lock Detection

**Configuration**:
```toml
[privacy]
pause_on_lock = true
```

**Behavior**:
- When you lock your screen (Win+L), capture pauses
- Upon unlock, capture resumes automatically
- Prevents capturing login screen or lock screen
- No user intervention required

### Manual Pause/Resume

Use the Settings panel to manually control capture:

**When to Pause**:
- Entering sensitive information
- Viewing private documents
- Sharing screen in meetings
- Handling confidential work

**Remember**: Pausing is not persistent across restarts. The system resumes capture on next launch.

### Data Security Best Practices

1. **Encrypt Your Drive**: Use Windows BitLocker or similar
2. **User Account Protection**: Use Windows account password
3. **Regular Cleanup**: Set retention days to limit data accumulation
4. **Backup Security**: Encrypt database backups if storing externally
5. **Local Network**: Keep `host = "127.0.0.1"` in config
6. **Review Exclusions**: Regularly audit excluded apps list

### Data Location

All data stored in:
- **Database**: `screen_memories.db` (project root)
- **Logs**: `screen_memories.log` (project root)
- **Frontend Cache**: Browser localStorage

**Deletion**:
To completely remove all data:
1. Stop the application
2. Delete `screen_memories.db`
3. Delete `screen_memories.log`
4. Clear browser data for localhost:3100

---

## Troubleshooting

### Common Issues

#### Backend Won't Start

**Symptom**: Error when running `cargo run`

**Possible Causes**:

1. **Missing Rust Toolchain**
   ```bash
   # Verify installation
   rustc --version

   # If missing, install from https://rustup.rs/
   ```

2. **Visual Studio Build Tools Not Installed**
   - Download from https://visualstudio.microsoft.com/downloads/
   - Install "Desktop development with C++" workload

3. **Database Locked**
   ```
   Error: database is locked
   ```
   - Close any other instances of the application
   - Delete `screen_memories.db-shm` and `screen_memories.db-wal` files
   - Restart application

4. **Port Already in Use**
   ```
   Error: Address already in use (os error 10048)
   ```
   - Another application using port 3131
   - Change port in `config.toml`:
     ```toml
     [api]
     port = 3132
     ```
   - Update frontend API calls accordingly

#### OCR Not Working

**Symptom**: Frames captured but no text extracted

**Solutions**:

1. **Windows OCR Language Pack Missing**
   - Open Settings > Time & Language > Language
   - Click "Add a language"
   - Install English (United States)
   - Ensure OCR language pack is installed

2. **Windows Version Too Old**
   ```bash
   # Check build number
   winver
   ```
   - Must be build 17763 or higher
   - Update Windows if needed

3. **OCR Confidence Too High**
   - Edit `config.toml`:
     ```toml
     [ocr]
     min_confidence = 0.5
     ```
   - Lower threshold includes more results

4. **Insufficient Permissions**
   - Run terminal as Administrator
   - Check Windows Defender settings

#### High CPU Usage

**Symptom**: CPU usage above 10-15%

**Solutions**:

1. **Increase Capture Interval**
   ```toml
   [capture]
   interval_ms = 5000  # Change from 3000
   ```

2. **Enable Frame Differencing**
   ```toml
   [capture]
   enable_frame_diff = true
   diff_threshold = 0.006
   ```

3. **Reduce OCR Workers**
   ```toml
   [ocr]
   worker_threads = 1  # Change from 2
   ```

4. **Disable Metrics**
   ```toml
   [ocr]
   enable_metrics = false
   ```

5. **Check for 4K/High-DPI Displays**
   - Higher resolutions require more processing
   - Consider capturing specific monitor only:
     ```toml
     [capture]
     monitor_indices = [0]
     ```

#### High Memory Usage

**Symptom**: RAM usage above 500MB

**Solutions**:

1. **Reduce Frame Buffer**
   ```toml
   [capture]
   max_frames_buffer = 15  # Change from 30
   ```

2. **Lower OCR Queue Size**
   ```toml
   [ocr]
   channel_buffer_size = 50  # Change from 100
   ```

3. **Disable Empty Frame Storage**
   ```toml
   [ocr]
   store_empty_frames = false
   ```

4. **Database Cleanup**
   ```sql
   -- Run in SQLite browser
   VACUUM;
   ```

#### Frontend Connection Issues

**Symptom**: "Disconnected" in header, or Network Error

**Solutions**:

1. **Backend Not Running**
   - Verify: http://localhost:3131/health
   - Start backend: `cargo run --release`

2. **Wrong Port**
   - Check `config.toml` for API port
   - Update frontend API client if changed

3. **Firewall Blocking**
   - Add exception for ports 3100 and 3131
   - Windows Firewall > Advanced Settings > Inbound Rules

4. **CORS Issues**
   - Edit `config.toml`:
     ```toml
     [api]
     cors_origin = "http://localhost:3100"
     ```

#### Frontend Won't Start

**Symptom**: Error when running `npm run dev`

**Solutions**:

1. **Node Modules Corrupted**
   ```bash
   rm -rf node_modules package-lock.json
   npm install
   ```

2. **Wrong Node Version**
   ```bash
   node --version  # Must be 18+
   ```
   - Update Node.js if needed

3. **Port 3100 In Use**
   ```bash
   npm run dev -- --port 3001
   ```

4. **TypeScript Errors**
   ```bash
   # Clear Vite cache
   rm -rf node_modules/.vite
   npm run dev
   ```

#### Search Not Working

**Symptom**: Search returns no results

**Solutions**:

1. **No Frames Captured Yet**
   - Wait 1-2 minutes for initial captures
   - Check frame count in header

2. **OCR Failed**
   - Check logs: `screen_memories.log`
   - Verify OCR settings

3. **FTS Index Missing**
   ```bash
   # Rebuild database (backs up first!)
   sqlite3 screen_memories.db ".dump" > backup.sql
   # Delete DB, restart app to rebuild
   ```

#### Images Not Displaying

**Symptom**: Frame cards show placeholder icons

**Solutions**:

1. **Backend Image Endpoint Issue**
   - Test: http://localhost:3131/frames/1/image
   - Check backend logs for errors

2. **Storage Permission**
   - Verify write permissions on project directory
   - Check disk space

3. **CORS or Path Issues**
   - Check browser console (F12) for errors
   - Verify API proxy configuration in `vite.config.ts`

### Performance Tuning

#### For Slow Systems

```toml
[capture]
interval_ms = 10000  # Capture every 10 seconds
max_frames_buffer = 10

[ocr]
worker_threads = 1
channel_buffer_size = 25
store_empty_frames = false
```

#### For Fast Systems

```toml
[capture]
interval_ms = 2000  # Capture every 2 seconds
max_frames_buffer = 50

[ocr]
worker_threads = 4
channel_buffer_size = 200
```

#### For Large Databases

After accumulating 100k+ frames:

```bash
# Compact database
sqlite3 screen_memories.db "VACUUM;"

# Set retention policy
# Edit config.toml (if implemented):
[database]
retention_days = 30
```

### Debug Logging

Enable detailed logging for troubleshooting:

1. **Edit `config.toml`**:
   ```toml
   [logging]
   level = "debug"
   ```

2. **Restart backend**

3. **Review logs**:
   - File: `screen_memories.log`
   - Real-time: View terminal output

4. **Reset logging** after issue resolved:
   ```toml
   [logging]
   level = "info"
   ```

### Getting Additional Help

If issues persist:

1. **Check logs**: Review `screen_memories.log` for error messages
2. **Test components individually**:
   - Backend: http://localhost:3131/health
   - Frontend: Console errors (F12)
3. **Review GitHub Issues**: Check for known issues
4. **Collect diagnostics**:
   - System info: `systeminfo` in CMD
   - Log file
   - Config file
   - Error screenshots

---

## FAQ

### General Questions

**Q: Is Screen Memory free?**
A: Yes, it's open-source software. Check the LICENSE file for details.

**Q: Does it work on Mac or Linux?**
A: Currently Windows-only. Mac and Linux support are planned for future releases.

**Q: Is my data uploaded to the cloud?**
A: No. All data stays on your local machine. There are no network connections except localhost.

**Q: Can I use this at work?**
A: Check your employer's policies first. Ensure you comply with data retention and privacy regulations.

**Q: How much disk space does it use?**
A: Varies by usage. Typical: 100-500 MB per month. Database can be cleaned up periodically.

### Privacy & Security

**Q: Can others on my network see my captures?**
A: Not if you keep `host = "127.0.0.1"` in config (default). This restricts access to localhost only.

**Q: What if I forget to exclude a sensitive app?**
A: Add it to the exclusion list, then manually delete those frames:
   ```sql
   DELETE FROM frames WHERE app_name LIKE '%AppName%';
   ```

**Q: Is the database encrypted?**
A: Not by default. Use Windows BitLocker or VeraCrypt for full-disk encryption.

**Q: Can I password-protect the application?**
A: Not built-in currently. Rely on Windows account password and drive encryption.

### Features & Usage

**Q: Can I search by date?**
A: Yes, use the date range filter in the search panel.

**Q: Can I export specific frames?**
A: Currently exports entire database. Selective export planned for future releases.

**Q: How do I recover deleted frames?**
A: Deleted frames cannot be recovered unless you have a database backup.

**Q: Can I edit OCR text?**
A: Not currently. OCR text is read-only.

**Q: Why are some words misspelled in OCR results?**
A: OCR isn't perfect. Adjust `min_confidence` for better accuracy, though this may reduce coverage.

**Q: Can I capture specific windows only?**
A: Not currently. You can exclude specific apps, but not whitelist-only specific apps.

### Performance

**Q: Why is the timeline loading slowly?**
A: Large databases (100k+ frames) can slow queries. Set retention days or use more specific filters.

**Q: Does it affect gaming performance?**
A: Minimal impact (<5% CPU). Add game to excluded apps list for zero impact.

**Q: Can I run this on a laptop?**
A: Yes, but may impact battery life. Consider increasing capture interval on battery power.

**Q: How many frames can the database handle?**
A: Tested up to 1 million frames. Performance may degrade; use retention policies.

### Technical

**Q: Can I use a different OCR engine?**
A: Config supports Tesseract, but Windows OCR is recommended for performance.

**Q: Can I change the database location?**
A: Yes, edit `path` in `[database]` section of `config.toml`.

**Q: Can I access the API from other applications?**
A: Yes, the REST API is documented in the main README. Use it to build custom integrations.

**Q: Can I run multiple instances?**
A: Not recommended. Database locking issues may occur.

**Q: Is there a mobile app?**
A: No, desktop-only currently.

### Automation

**Q: What automation features are available?**
A: The API includes automation endpoints for UI element interaction, clicking, typing, etc. See API documentation.

**Q: Can I script repetitive tasks?**
A: Yes, via the `/automation/*` API endpoints using any HTTP client.

**Q: Can it detect UI elements like Selenium?**
A: Yes, uses Windows UIAutomation API with Playwright-inspired selectors.

### Troubleshooting FAQs

**Q: Why is the timeline empty?**
A: Wait for initial captures (10-15 seconds). Check backend is running and frame count increasing.

**Q: Search returns nothing but I know text was there?**
A: OCR may have failed or confidence was too low. Check logs and adjust `min_confidence`.

**Q: Frontend shows "Disconnected"?**
A: Backend not running or port mismatch. Verify http://localhost:3131/health works.

**Q: Why are there duplicate captures?**
A: Frame differencing may be disabled or threshold too low. Enable and adjust:
   ```toml
   enable_frame_diff = true
   diff_threshold = 0.006
   ```

### Future Features

**Q: Will there be semantic search?**
A: Vector embeddings for semantic search are on the roadmap.

**Q: Can you add cloud sync?**
A: Planned as optional feature while maintaining local-first approach.

**Q: Will there be a system tray icon?**
A: Planned for easier pause/resume control.

**Q: Can I get notifications for specific keywords?**
A: Not currently implemented but under consideration.

---

## Appendix

### Useful Commands Reference

**Backend**:
```bash
# Start in release mode
cargo run --release

# Check without running
cargo check

# Run tests
cargo test

# Clean build artifacts
cargo clean
```

**Frontend**:
```bash
# Development server
npm run dev

# Production build
npm run build

# Preview production
npm run preview

# Lint code
npm run lint
```

**Database Management**:
```bash
# Open database
sqlite3 screen_memories.db

# Count frames
SELECT COUNT(*) FROM frames;

# Recent captures
SELECT timestamp, app_name FROM frames ORDER BY timestamp DESC LIMIT 10;

# Compact database
VACUUM;
```

### Resource Links

- **Project Repository**: [GitHub](https://github.com/nicolasestrem/screen-memories)
- **Issue Tracker**: [GitHub Issues](https://github.com/nicolasestrem/screen-memories/issues)
- **Rust Documentation**: https://www.rust-lang.org/learn
- **React Documentation**: https://react.dev/
- **SQLite Documentation**: https://www.sqlite.org/docs.html

### Version Information

This guide is for Screen Memory version 1.0.0.

**Last Updated**: December 10, 2025

---

**Screen Memory** - Your digital memory, locally stored and searchable.
