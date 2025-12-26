# Cross-Compilation Guide: Linux to Windows

## Overview

This guide covers building ScreenSearch Windows binaries from Linux systems using `cargo-xwin`. Cross-compilation enables CI/CD automation on Linux runners and provides flexibility for developers working on Linux who need to produce Windows executables.

**Important**: Cross-compilation builds Windows executables that run on Windows. It does not enable ScreenSearch to run natively on Linux.

## Prerequisites

### System Requirements

- **Linux Distribution**: Ubuntu 20.04+, Debian 11+, Arch, Fedora, or similar
- **Rust**: 1.70 or later (`rustup update`)
- **Node.js**: 18+ (for UI build)
- **Disk Space**: ~2GB for SDK caching

### Required Tools

Install the following tools before cross-compiling:

```bash
# 1. Install system dependencies (Ubuntu/Debian)
sudo apt-get update
sudo apt-get install -y clang lld llvm

# For Arch-based systems
sudo pacman -S clang lld llvm

# For Fedora/RHEL
sudo dnf install clang lld llvm

# 2. Install cargo-xwin
cargo install cargo-xwin

# 3. Add Windows target to Rust toolchain
rustup target add x86_64-pc-windows-msvc

# 4. Verify installation
cargo xwin --version
rustc --print target-list | grep x86_64-pc-windows-msvc
```

## Quick Start

### Basic Build

```bash
# 1. Clone repository
git clone https://github.com/nicolasestrem/screensearch.git
cd screensearch

# 2. Build UI (platform-agnostic)
cd screensearch-ui
npm install
npm run build
cd ..

# 3. Cross-compile to Windows
cargo xwin build --release --target x86_64-pc-windows-msvc

# 4. Find binary
ls -lh target/x86_64-pc-windows-msvc/release/screensearch.exe
```

### Fast Iteration (Skip UI Build)

```bash
# Skip UI build for faster compilation during development
SKIP_UI_BUILD=1 cargo xwin build --release --target x86_64-pc-windows-msvc

# Or check compilation without building
cargo xwin check --target x86_64-pc-windows-msvc --workspace
```

### Build Specific Workspace Crates

```bash
# Build individual crates
cargo xwin build --release --target x86_64-pc-windows-msvc -p screensearch-capture
cargo xwin build --release --target x86_64-pc-windows-msvc -p screensearch-db
cargo xwin build --release --target x86_64-pc-windows-msvc -p screensearch-api
```

## Configuration Files

### .cargo/config.toml

The project includes a `.cargo/config.toml` file with the following configuration:

```toml
[target.x86_64-pc-windows-msvc]
linker = "lld"
rustflags = [
    "-C", "link-arg=-fuse-ld=lld",
    "-C", "target-feature=+crt-static",
]
```

**What this does**:
- Uses LLVM's `lld` linker (cross-platform, fast)
- Statically links MSVC runtime (`+crt-static`) to reduce DLL dependencies
- Ensures MSVC ABI compatibility for Windows APIs

### build.rs Cross-Compilation Detection

The `build.rs` script automatically detects cross-compilation and uses the correct npm commands:

- **When cross-compiling**: Uses `which npm` and `npm` (Linux commands)
- **When building natively on Windows**: Uses `where npm` and `npm.cmd` (Windows commands)

This ensures the UI build process works correctly regardless of the build platform.

## Common Issues and Solutions

### Issue: cargo-xwin not found

**Symptom**:
```
error: no such command: `xwin`
```

**Solution**:
```bash
cargo install cargo-xwin
# Ensure ~/.cargo/bin is in your PATH
export PATH="$HOME/.cargo/bin:$PATH"
```

### Issue: Linker errors (LLD not found)

**Symptom**:
```
error: linker `lld` not found
```

**Solution**:
```bash
# Ubuntu/Debian
sudo apt-get install lld

# Arch
sudo pacman -S lld

# Verify installation
which lld
```

### Issue: UI not embedded in binary

**Symptom**:
Binary size is small (~5-10MB instead of ~15-20MB) or web interface shows "UI not built" page.

**Solution**:
```bash
# Build UI before Rust compilation
cd screensearch-ui
npm install
npm run build
cd ..

# Verify UI build artifacts exist
ls -la screensearch-ui/dist/

# Then rebuild
cargo xwin build --release --target x86_64-pc-windows-msvc
```

### Issue: Windows CRT linking errors

**Symptom**:
```
error LNK2019: unresolved external symbol __CxxFrameHandler3
```

**Solution**:
Ensure `.cargo/config.toml` has `crt-static` flag:

```toml
[target.x86_64-pc-windows-msvc]
rustflags = ["-C", "target-feature=+crt-static"]
```

### Issue: ONNX Runtime not found (at runtime on Windows)

**Symptom** (on Windows):
```
The code execution cannot proceed because onnxruntime.dll was not found
```

**Solution**:
This is expected behavior. The ONNX Runtime DLL is loaded dynamically:
- First run with embeddings enabled downloads the model and DLL from HuggingFace (~449MB)
- Set `embeddings.enabled = true` in `config.toml` to trigger download
- The DLL is bundled with the model download

### Issue: npm not found during build

**Symptom**:
```
cargo:warning=npm not found. Skipping UI build.
```

**Solution**:
```bash
# Install Node.js 18+
# Ubuntu/Debian
curl -fsSL https://deb.nodesource.com/setup_18.x | sudo -E bash -
sudo apt-get install -y nodejs

# Verify installation
node --version
npm --version
```

### Issue: Slow first build

**Symptom**:
First cross-compilation takes 10-15 minutes.

**Explanation**:
This is normal. `cargo-xwin` downloads the Windows SDK on first use (~500MB). Subsequent builds are much faster due to caching.

**Solution**:
```bash
# Pre-download SDK (optional)
cargo xwin build --target x86_64-pc-windows-msvc --help

# SDK is cached in ~/.cache/cargo-xwin
```

### Issue: Binary doesn't run on Windows

**Symptom**:
Binary transfers to Windows but fails to execute.

**Troubleshooting checklist**:
1. **Verify binary is valid Windows executable**:
   ```bash
   file target/x86_64-pc-windows-msvc/release/screensearch.exe
   # Should output: PE32+ executable (console) x86-64, for MS Windows
   ```

2. **Check binary size** (should be 15-20MB with UI):
   ```bash
   du -h target/x86_64-pc-windows-msvc/release/screensearch.exe
   ```

3. **Test on actual Windows machine** - Wine is not supported for testing

4. **Check Windows version** - Requires Windows 10/11

## Testing and Validation

### What Can Be Tested on Linux

✅ **Compilation**:
```bash
cargo xwin build --release --target x86_64-pc-windows-msvc
# Should complete without errors
```

✅ **Binary creation**:
```bash
ls -lh target/x86_64-pc-windows-msvc/release/screensearch.exe
file target/x86_64-pc-windows-msvc/release/screensearch.exe
```

✅ **UI embedding**:
```bash
# Compare binary sizes
cargo xwin build --release --target x86_64-pc-windows-msvc
SIZE_WITH_UI=$(stat -c%s target/x86_64-pc-windows-msvc/release/screensearch.exe)

SKIP_UI_BUILD=1 cargo xwin build --release --target x86_64-pc-windows-msvc
SIZE_WITHOUT_UI=$(stat -c%s target/x86_64-pc-windows-msvc/release/screensearch.exe)

echo "With UI: $SIZE_WITH_UI bytes"
echo "Without UI: $SIZE_WITHOUT_UI bytes"
# UI should add ~5-10MB
```

### What Requires Windows Testing

❌ **Runtime execution** - Windows APIs required
❌ **Screen capture** - Windows Graphics APIs
❌ **OCR functionality** - Windows OCR language packs
❌ **UI Automation** - UIAutomation COM APIs
❌ **Tray icon** - Win32 system tray APIs
❌ **Integration tests** - Most require Windows runtime

### Validation Workflow

1. **Build on Linux**:
   ```bash
   cargo xwin build --release --target x86_64-pc-windows-msvc
   ```

2. **Transfer to Windows**:
   ```bash
   # Copy to Windows machine via SCP, USB, network share, etc.
   scp target/x86_64-pc-windows-msvc/release/screensearch.exe user@windows-pc:/path/
   ```

3. **Test on Windows**:
   - Run `screensearch.exe`
   - Verify API server starts on http://localhost:3131
   - Test web UI loads correctly
   - Test screen capture, OCR, search, automation features
   - Verify tray icon appears

## CI/CD Integration

### GitHub Actions Workflow

The project includes `.github/workflows/cross-compile-linux.yml` for automated cross-compilation.

**Workflow features**:
- Runs on `ubuntu-latest` runners
- Installs `cargo-xwin` and dependencies
- Builds React UI
- Cross-compiles Windows binary
- Uploads artifact for download
- Optional Windows smoke test job

**Triggering the workflow**:
```bash
# Push to main/develop branch
git push origin feature/cross-compile-linux-to-windows

# Or create pull request to main
```

**Downloading artifacts**:
1. Go to GitHub Actions tab
2. Click on workflow run
3. Download `screensearch-windows-x86_64-cross-compiled` artifact
4. Extract and test on Windows

### Local CI Simulation

```bash
# Simulate CI build process locally
# 1. Clean state
cargo clean

# 2. Build UI
cd screensearch-ui && npm ci && npm run build && cd ..

# 3. Cross-compile
cargo xwin build --release --target x86_64-pc-windows-msvc --locked

# 4. Verify
ls -lh target/x86_64-pc-windows-msvc/release/screensearch.exe
```

## Performance Optimization

### Build Caching

**cargo-xwin SDK cache**:
```bash
# SDK cached at:
~/.cache/cargo-xwin

# Size: ~500MB
du -sh ~/.cache/cargo-xwin
```

**Cargo build cache**:
```bash
# Use sccache for faster rebuilds (optional)
cargo install sccache
export RUSTC_WRAPPER=sccache

# Verify caching works
sccache --show-stats
```

### Faster Builds

```bash
# 1. Skip UI build for iteration
SKIP_UI_BUILD=1 cargo xwin build --target x86_64-pc-windows-msvc

# 2. Use check instead of build for validation
cargo xwin check --target x86_64-pc-windows-msvc --workspace

# 3. Build specific crate
cargo xwin build --target x86_64-pc-windows-msvc -p screensearch-db

# 4. Parallel compilation (default, but can be tuned)
export CARGO_BUILD_JOBS=8
```

## Advanced Topics

### Custom Linker Configuration

If you need different linker settings, modify `.cargo/config.toml`:

```toml
[target.x86_64-pc-windows-msvc]
linker = "lld"
rustflags = [
    "-C", "link-arg=-fuse-ld=lld",
    "-C", "target-feature=+crt-static",
    # Add custom flags here
]
```

### Debug Builds

```bash
# Cross-compile debug build (faster compilation, larger binary, includes symbols)
cargo xwin build --target x86_64-pc-windows-msvc

# Debug binary location
target/x86_64-pc-windows-msvc/debug/screensearch.exe
```

**Note**: Windows PDB debug symbols are not fully generated on Linux. Use release builds for production.

### Alternative Targets

While ScreenSearch currently targets `x86_64-pc-windows-msvc`, you could potentially support:

```bash
# 32-bit Windows (not recommended, untested)
rustup target add i686-pc-windows-msvc
cargo xwin build --target i686-pc-windows-msvc

# ARM64 Windows (experimental, requires ARM64 Windows SDK)
rustup target add aarch64-pc-windows-msvc
cargo xwin build --target aarch64-pc-windows-msvc
```

## Troubleshooting Checklist

Before reporting issues, verify:

- [ ] `cargo-xwin` is installed: `cargo xwin --version`
- [ ] Windows target is added: `rustup target list | grep x86_64-pc-windows-msvc`
- [ ] System dependencies installed: `which lld`, `which clang`
- [ ] UI is built: `ls screensearch-ui/dist/`
- [ ] `.cargo/config.toml` exists with correct configuration
- [ ] Rust toolchain is updated: `rustup update`
- [ ] Cargo cache is not corrupted: Try `cargo clean`

## Limitations

1. **No runtime testing on Linux** - Windows APIs cannot run on Linux (expected)
2. **PDB debug symbols** - Not fully generated on Linux
3. **Wine not supported** - Do not attempt to test with Wine
4. **Installer creation** - Inno Setup requires Windows (only produces `.exe`, not installer)
5. **Integration tests** - Cannot run on Linux (use `#[cfg(target_os = "windows")]` guards)

## Additional Resources

- **cargo-xwin documentation**: https://github.com/rust-cross/cargo-xwin
- **windows-rs documentation**: https://github.com/microsoft/windows-rs
- **Rust cross-compilation guide**: https://rust-lang.github.io/rustup/cross-compilation.html
- **ScreenSearch CI/CD**: `.github/workflows/cross-compile-linux.yml`

## Getting Help

If you encounter issues not covered in this guide:

1. Check existing GitHub Issues: https://github.com/nicolasestrem/screensearch/issues
2. Review the plan file: `.claude/plans/woolly-tinkering-orbit.md`
3. Consult `CLAUDE.md` for development context
4. Open a new issue with:
   - Linux distribution and version
   - Rust version (`rustc --version`)
   - cargo-xwin version
   - Full build output
   - Binary file info (`file screensearch.exe`)

---

**Last Updated**: 2025-12-26
**Rust Version Tested**: 1.70+
**cargo-xwin Version Tested**: 0.16+
