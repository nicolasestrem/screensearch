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

Use the repository release script:

```bash
./scripts/build-release.sh 0.4.35
```

Output:

```text
target/x86_64-pc-windows-msvc/release/screensearch.exe
target/x86_64-pc-windows-msvc/release/bundles/
  ScreenSearch-v0.4.35-Windows-Core-Preview.zip
```

The preview ZIP contains the cross-compiled core executable, configuration,
license, and README. All ML (OCR and embeddings) runs in-process in Rust, so
there is no separate runtime to package.

## Installer

Publishing the validated tag delegates platform-specific packaging (the Inno
Setup installer) to the Windows GitHub Actions runner:

```bash
./scripts/build-release.sh 0.4.35 --windows-bundle
./scripts/build-release.sh 0.4.35 --publish
```

`--windows-bundle` downloads the installer, portable ZIP, and checksums without
publishing. `--publish` additionally creates the release tag and draft GitHub
release.

## What Cross Compilation Validates

- Windows Rust type checking and linking;
- embedded frontend assets;
- most workspace crate integration;
- `fastembed`/`ort` linking for `x86_64-pc-windows-msvc` (no Python required).

It does not validate:

- screen capture behavior;
- native Windows OCR runtime;
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

### Models unavailable

Model weights are not part of the Rust build. The EmbeddingGemma-300M embedding
model (and optional `bge-reranker-v2-m3`) download from Hugging Face on first
use; the optional answer-generation GGUF is discovered locally or downloaded by
the bundled llama.cpp server.

## CI

`.github/workflows/cross-compile-linux.yml` performs Linux-to-Windows compile
validation. `.github/workflows/release.yml` is the authoritative Windows
packaging workflow.
