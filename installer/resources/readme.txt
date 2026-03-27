Welcome to ScreenSearch v0.2.0!

ScreenSearch is an intelligent screen capture and OCR application for Windows that automatically captures your screen, extracts text, and enables powerful semantic search across your screen history.

SYSTEM REQUIREMENTS
- Windows 10 (build 17763+) or Windows 11
- 64-bit (x64) architecture
- Windows OCR Language Pack (English included by default)
- 500 MB disk space minimum

WHAT YOU'RE INSTALLING
This installer will:
- Install ScreenSearch application to Program Files
- Create Start Menu shortcuts
- (Full installer only) Install ONNX embedding model for semantic search
- Configure the application to run on startup (optional)

GETTING STARTED
After installation:
1. ScreenSearch will launch automatically
2. Look for the system tray icon
3. The web interface will open at http://localhost:3131
4. Configure capture settings in config.toml (optional)

FIRST RUN
On first run, ScreenSearch will:
- Initialize the SQLite database
- (Lite installer) Download the ONNX model if embeddings are enabled (449 MB)
- Start capturing screenshots based on your configuration
- Begin OCR processing of captured frames

CONFIGURATION
Edit config.toml in the installation directory to customize:
- Capture interval (default: 3000ms)
- OCR confidence threshold
- API server port
- Storage format (JPEG quality)
- Embedding settings

PRIVACY & DATA
- All data is stored locally on your machine
- Screenshots saved to: [InstallDir]\captures\
- Database location: [InstallDir]\screensearch.db
- No telemetry or external connections (except optional model download)

SUPPORT
- GitHub: https://github.com/nicolasestrem/screensearch
- Issues: https://github.com/nicolasestrem/screensearch/issues
- Documentation: https://github.com/nicolasestrem/screensearch/blob/main/README.md

LICENSE
ScreenSearch is open source software released under the MIT License.
See LICENSE file for full terms.

Thank you for using ScreenSearch!
