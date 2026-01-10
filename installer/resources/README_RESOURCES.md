# Installer Resources

This directory contains resources needed for the Inno Setup installer.

## Required Files

### Important Note on Versioning

> [!WARNING]
> The version number in `screensearch.iss` (`#define MyAppVersion`) MUST match the version in `Cargo.toml` and the git tag used for release. Mismatches will cause the installer output filename to differ from what the GitHub Actions workflow expects, leading to release failures.

### 1. icon.ico
**Purpose:** Windows icon for the installer and installed application

**Specifications:**
- Format: ICO (Windows Icon)
- Recommended sizes: 16x16, 32x32, 48x48, 256x256
- Source: Convert assets/icon.png to ICO format

**How to create:**
```powershell
# Using ImageMagick
magick convert ../assets/icon.png -define icon:auto-resize=256,128,96,64,48,32,16 icon.ico

# OR use online converter: https://www.icoconverter.com/
```

### 2. banner.bmp
**Purpose:** Horizontal banner shown at top of installer wizard pages

**Specifications:**
- Format: BMP (24-bit)
- Dimensions: 493 x 58 pixels
- Content: ScreenSearch logo + tagline on gradient background
- Style: Matches brand colors (purple gradient)

**Design guidelines:**
- Keep logo left-aligned
- Use white text for readability
- Gradient should match website theme

### 3. sidebar.bmp
**Purpose:** Vertical image shown on left side of installer wizard

**Specifications:**
- Format: BMP (24-bit)
- Dimensions: 164 x 314 pixels
- Content: ScreenSearch logo or product screenshot
- Style: Clean, professional, matches brand

**Design guidelines:**
- Centered logo or icon
- Subtle background gradient
- Leave margins around edges

### 4. readme.txt
**Purpose:** Information displayed before installation (InfoBeforeFile)

**Content:** Welcome message and quick start guide
(See readme.txt in this directory)

### 5. license.rtf
**Purpose:** RTF-formatted license displayed during installation

**Content:** MIT License in Rich Text Format
(See license.rtf in this directory)

### 6. vc_redist.x64.exe (Auto-downloaded by CI)
**Purpose:** Visual C++ 2015-2022 Redistributable for llama-server.exe

**Specifications:**
- Format: Windows Installer (.exe)
- Size: ~25 MB
- Source: Microsoft (https://aka.ms/vs/17/release/vc_redist.x64.exe)
- **Not stored in git** - Downloaded during CI build

**Why it's needed:**
- `llama-server.exe` (used for embedded LLM) requires Visual C++ Runtime
- Fresh Windows 11 installations missing these DLLs: VCRUNTIME140.dll, MSVCP140.dll
- Installer auto-installs if not already present

**How it works:**
1. GitHub Actions downloads `vc_redist.x64.exe` before building installer
2. Inno Setup bundles it in the installer (adds ~25MB)
3. During installation, `VCRedistNeedsInstall()` checks registry
4. If VC++ Runtime missing, installer runs `vc_redist.x64.exe /install /quiet /norestart`
5. If already installed, step is skipped (no overhead)

**Manual download for local testing:**
```powershell
Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vc_redist.x64.exe" `
    -OutFile "installer/resources/vc_redist.x64.exe"
```

## Creating These Resources

### Quick Start with PowerShell

If you have ImageMagick installed:

```powershell
# Convert existing PNG icon to ICO
magick convert ../../assets/icon.png `
    -background transparent `
    -define icon:auto-resize=256,128,96,64,48,32,16 `
    icon.ico

# Create placeholder BMP files (replace with actual designs)
# These commands create blank colored files - replace with proper designs
magick convert -size 493x58 xc:#667eea banner.bmp
magick convert -size 164x314 xc:#764ba2 sidebar.bmp
```

### Manual Creation

1. **Icon:** Use GIMP or Photoshop to export assets/icon.png as ICO
2. **Banner/Sidebar:** Design in any graphics editor, export as 24-bit BMP

### Online Tools

- **ICO Converter:** https://www.icoconverter.com/
- **BMP Converter:** https://image.online-convert.com/convert-to-bmp
- **Design Tool:** https://www.canva.com/ (free design templates)

## Template Sizes Reference

| File | Dimensions | Format | Purpose |
|------|-----------|--------|---------|
| icon.ico | Multi-size | ICO | App icon |
| banner.bmp | 493×58 | BMP 24-bit | Installer top banner |
| sidebar.bmp | 164×314 | BMP 24-bit | Installer left sidebar |
| readme.txt | N/A | Plain text | Pre-install info |
| license.rtf | N/A | RTF | License agreement |
| vc_redist.x64.exe | N/A (~25MB) | EXE | Visual C++ Runtime (CI-downloaded) |

## Fallback Behavior

If these files are missing:
- Inno Setup will use default visuals
- Installation will still work
- Consider this Phase 2 polish

## For Release

Before final release:
1. Create professional designs for banner.bmp and sidebar.bmp
2. Ensure icon.ico has all standard sizes
3. Test installer appearance on different Windows versions
4. Consider hiring a designer for polished visuals (~$50-100)
