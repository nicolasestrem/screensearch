# Cross Compilation: Linux To Windows

Cross compilation can validate and produce the Rust Windows executable. It
does not replace Windows testing or build the final Inno Setup installer.

## Requirements

- Rust stable;
- Node.js 22;
- `cargo-xwin`;
- `x86_64-pc-windows-msvc`;
- Clang and LLD compatible with the current Microsoft SDK.

```bash
cargo install cargo-xwin
rustup target add x86_64-pc-windows-msvc
```

## Build

Build the frontend first because it is embedded into the API crate:

```bash
cd screensearch-ui
npm ci
npm run build
cd ..

cargo xwin build --release \
  --target x86_64-pc-windows-msvc \
  --locked
```

Output:

```text
target/x86_64-pc-windows-msvc/release/screensearch.exe
```

## Sidecar

The Python sidecar is not cross-compiled by Cargo. Build it on Windows:

```powershell
python -m pip install -r sidecar\requirements.txt
python sidecar\build.py
```

Output:

```text
sidecar\dist\screensearch-ai-sidecar\
```

The final application layout must include:

```text
screensearch.exe
bin/
  screensearch-ai-sidecar/
    screensearch-ai-sidecar.exe
    ...
```

## What Cross Compilation Validates

- Windows Rust type checking and linking;
- embedded frontend assets;
- most workspace crate integration.

It does not validate:

- screen capture behavior;
- PP-OCRv5 runtime imports;
- Windows OCR fallback;
- model downloads;
- UI Automation;
- tray behavior;
- GPU inference;
- installer behavior.

Run those checks on Windows or in Windows CI.

## Common Problems

### Old UI in the executable

Rebuild `screensearch-ui/dist/` before Cargo.

### Microsoft STL rejects the compiler

Upgrade Clang/LLD to the version required by the SDK downloaded by
`cargo-xwin`, clear the xwin cache if needed, and rebuild.

### Sidecar not found on Windows

Cross compilation creates only the Rust executable. Copy or package the
Windows sidecar directory in the expected `bin/` location.

### Models unavailable

Model weights are not part of the Rust build. PP-OCRv5 and Qwen models download
when **Download / verify** is selected or when a model is first used. The
sidecar runtime must already be present in `bin/`.

## CI

`.github/workflows/cross-compile-linux.yml` performs Linux-to-Windows compile
validation. `.github/workflows/release.yml` is the authoritative Windows
packaging workflow.
