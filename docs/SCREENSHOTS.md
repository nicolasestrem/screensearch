# Regenerating the README screenshots

The `screenshots/*.png` used in the README are rendered against **synthetic seed data** — invented
frames, sessions, marks, and text with **no personal content**. This keeps a public, honest preview of
the UI without ever exposing a real capture history (the earlier hero images were live captures and
were removed; see the repo-exposure scrub in the changelog).

Everything runs against an **isolated** app-data dir so your real database is never touched.

## What produces them

1. **Synthetic capture images** — small mock "screens" (a code editor, a terminal running Claude Code,
   a browser on docs, a meeting grid, notes, an inbox, a metrics dashboard) rendered from HTML to PNG.
2. **A demo database** — `crates/store/tests/seed_demo.rs`, an `#[ignore]`d integration test that seeds
   a real store at the current schema through the public `store` API: ~120 frames across a plausible day, ten
   **frozen** overlapping sessions (focus / meeting / concurrent Claude Code), marks, and AI exchanges.
   Frames are dated a few minutes into the future of the seed moment so the background sessions
   scheduler (which only segments the past, and never deletes a frozen row) leaves the curated sessions
   untouched.
3. **The real app** — launched in dev against the isolated dir with a WebView2 remote-debugging port.
   Capture stays **off** (it is user-triggered) and no model is loaded, so nothing on the host screen
   is recorded and no large downloads run. Each route is navigated and captured over CDP (the real
   WebView2, where Tauri IPC serves the seeded data — a plain browser at the dev URL would only show
   empty states).

## Steps

```sh
# 1. Isolate: point a throwaway dev run at a scratch data dir by temporarily setting
#    src-tauri/tauri.conf.json  "identifier"  to e.g. "app.screensearchv2c.demo"
#    (do NOT commit this change). app_data_dir() then resolves to
#    %APPDATA%\app.screensearchv2c.demo\  — a fresh, empty dir.

# 2. Render the mock scene PNGs to a folder (any HTML->PNG path works; the scenes are
#    plain self-contained HTML). Call that folder <scenes>.

# 3. Seed the isolated DB + copy the scene images into place:
DEMO_DB="$APPDATA/app.screensearchv2c.demo/screensearch.db" \
DEMO_FRAMES_DIR="$APPDATA/app.screensearchv2c.demo/frames" \
DEMO_SCENE_DIR="<scenes>" \
DEMO_NOW_MS="$(node -e 'console.log(Date.now())')" \
cargo test -p store --test seed_demo -- --ignored --nocapture

# 4. Launch the app with the WebView2 debug port, capture off, no model:
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" npm run dev

# 5. Attach over CDP (http://127.0.0.1:9222/json -> the main page's webSocketDebuggerUrl),
#    navigate /  /timeline  /recall (type a query)  /insights  /timeline/<id>, and
#    Page.captureScreenshot each. Save to screenshots/.

# 6. Revert the identifier change and delete the scratch data dir.
```

The seeder is dev-only and never runs in CI (`#[ignore]`). Nothing here ships in the app bundle.
