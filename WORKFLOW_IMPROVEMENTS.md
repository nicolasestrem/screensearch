# Workflow Improvements for Issue #12

This file contains the suggested improvements to `.github/workflows/release.yml` that I cannot push due to GitHub App permissions.

## Changes to Apply

In `.github/workflows/release.yml`, replace lines 214-233 (the release body section starting from `# ScreenSearch v${{ steps.get_version.outputs.VERSION }}`) with the following:

```yaml
          body: |
            # ScreenSearch v${{ steps.get_version.outputs.VERSION }}

            Your screen history, searchable and automated - Windows OCR + REST API + AI + RAG

            ---

            ## 📥 How to Download

            **New to GitHub?** Scroll down to the **Assets** section below (just above the "What's New" section) and click on one of these files to download:

            ### Choose Your Download:

            | File Name | Size | Best For | Click to Download ⬇️ |
            |-----------|------|----------|---------------------|
            | `ScreenSearch-v${{ steps.get_version.outputs.VERSION }}-Setup-Full.exe` | ~460 MB | **Most users** - Includes everything, works offline | See Assets section below |
            | `ScreenSearch-v${{ steps.get_version.outputs.VERSION }}-Setup-Lite.exe` | ~11 MB | Smaller download, requires internet for AI features | See Assets section below |
            | `ScreenSearch-v${{ steps.get_version.outputs.VERSION }}-Portable.zip` | ~17 MB | No installation needed - extract and run | See Assets section below |

            > 💡 **Recommended for most users:** Download the **Full Installer** (ScreenSearch-v${{ steps.get_version.outputs.VERSION }}-Setup-Full.exe) from the Assets section below

            ---

            ## 📦 Download Options Explained

            **Full Installer** (~460 MB)
            - ✅ Includes AI RAG Search Model for semantic search
            - ✅ No internet required for AI features
            - ✅ Recommended for offline systems
            - ✅ Complete installation experience

            **Lightweight Installer** (~11 MB)
            - ✅ Smaller initial download
            - ⚠️ Downloads model on first run (requires internet)
            - ✅ Same features as Full Installer after model download

            **Portable ZIP** (~17 MB)
            - ✅ Extract and run, no installation required
            - ✅ Perfect for USB drives or temporary use
            - ⚠️ Downloads model when AI features are used

            ---

            ## What's New
```

## Quick Manual Update for v0.2.0 Release

Alternatively, you can manually edit the existing v0.2.0 release page:

1. Go to: https://github.com/nicolasestrem/screensearch/releases/edit/v0.2.0
2. Add the "How to Download" section (lines 11-33 above) at the very top
3. Click "Update release"

This will immediately help GitHub newcomers find the download links without waiting for the next release.

## Why This Helps

- **Explicit instructions**: Tells users exactly where to click ("Assets section below")
- **Visual table**: Makes it easy to compare download options
- **Prominent placement**: At the top of the release notes, before other content
- **Beginner-friendly language**: Uses "New to GitHub?" to directly address the target audience
- **Visual indicators**: Emojis (📥, ✅, ⚠️, 💡) make content more scannable
