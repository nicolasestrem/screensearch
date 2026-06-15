# Next Session Handoff: AI/RAG/OCR Modernization

## Overall Objective

Finish and validate PR #63 so ScreenSearch v0.4.35 ships a Linux-first
development and release workflow with a working Windows quality bundle:

- PP-OCRv5 performs local OCR through the managed sidecar.
- Windows OCR is used only when PP-OCRv5 is unavailable or a request fails.
- Qwen3 embeddings and reranking can be prepared from Settings.
- the bundled local generation runtime can be downloaded from Settings;
- the Windows installer and portable ZIP contain the complete AI sidecar;
- documentation accurately describes the fixed model contracts, build paths,
  diagnostics, fallback behavior, and release process.

Do not create another branch. Continue on:

```text
feat/ai-rag-ocr-modernization
```

Continue using the existing pull request:

```text
PR #63: https://github.com/nicolasestrem/screensearch/pull/63
```

At the latest verification on June 15, 2026, PR #63 is open, draft,
mergeable, and its head is:

```text
c97317edf7e5db6d674422023dfdb98a7f333782
```

## Confirmed State

The final GitHub Actions Windows release workflow succeeded:

```text
Run: 27511498808
URL: https://github.com/nicolasestrem/screensearch/actions/runs/27511498808
Commit: d15a36d997a3600ae61957a7f91d2533de1317f3
Result: success
Duration: 25m7s
```

Every release step passed, including:

```text
Build React UI
Build Rust release binary
Build quality AI sidecar
Smoke test packaged PP-OCRv5
Build Installer
Create Portable ZIP
Generate checksums
Upload artifacts
```

The packaged PP-OCRv5 smoke test is an executable-level test, not a source
Python test. It starts the PyInstaller sidecar, prepares English OCR models,
generates an image containing `ScreenSearch OCR smoke test`, calls
`POST /v1/ocr`, and requires at least one recognized line.

The final artifacts were downloaded and verified locally in:

```text
target/x86_64-pc-windows-msvc/release/bundles/windows-full/
```

The verified files are:

```text
ScreenSearch-v0.4.35-Setup-Quality.exe 313862430 bytes
ScreenSearch-v0.4.35-Portable.zip       446984262 bytes
checksums.txt                           207 bytes
```

The local SHA-256 calculations match `checksums.txt`:

```text
29ed23b16a8ab5b6513b9e254c277fa0d963c977193330159ba0715c35fdc741  ScreenSearch-v0.4.35-Setup-Quality.exe
751c90987badd9e1d6b1faa150932c29e21b2fd7fe9ef6c08f855fcf1cd425ff  ScreenSearch-v0.4.35-Portable.zip
```

The portable ZIP was also inspected and contains the complete sidecar entry
point at:

```text
bin/screensearch-ai-sidecar/screensearch-ai-sidecar.exe
```

## What Was Accomplished

### 1. Sidecar discovery and Windows bundling

The application now locates the quality sidecar in installed and portable
layouts, including:

```text
bin/screensearch-ai-sidecar/screensearch-ai-sidecar.exe
```

Linux remains the primary development environment. Bash entrypoints build the
local Linux bundle, cross-compile the Windows core preview, dispatch Windows
packaging through GitHub Actions, and download full Windows artifacts.
PowerShell scripts were intentionally retained.

### 2. Actionable sidecar diagnostics

`src/main.rs` no longer discards the managed sidecar's stdout and stderr.
Sidecar lines are forwarded to the ScreenSearch application log:

```text
Quality sidecar: <sidecar output>
```

`sidecar/app.py` now:

- logs full chained exceptions during background model preparation;
- logs unhandled request exceptions;
- returns the exception type and message in authenticated HTTP 500 responses;
- does not log bearer tokens or uploaded image contents.

This exposed the original hidden Windows failure instead of showing only:

```text
PP-OCRv5 request failed (500 Internal Server Error)
```

### 3. PaddleX PyInstaller metadata fix

The first executable smoke test failed during `PaddleOCR(...)` construction:

```text
paddlex.utils.deps.DependencyError:
`OCR` requires additional dependencies.
```

The Python modules were present, but PyInstaller had omitted package
`.dist-info` metadata. PaddleX validates OCR extras with
`importlib.metadata`, so importable modules alone are insufficient.

`sidecar/build.py` now preserves metadata for:

```text
imagesize
opencv-contrib-python
pyclipper
pypdfium2
python-bidi
shapely
```

It also collects all of:

```text
paddle
paddleocr
paddlex
sentence_transformers
```

### 4. Enforced PP-OCRv5 contract

PaddleOCR 3.7 selected PP-OCRv6 for English when only `lang="en"` was passed.
That violated the Settings UI and documentation contract.

The managed constructor now explicitly includes:

```python
PaddleOCR(
    lang=language,
    ocr_version="PP-OCRv5",
    device="cpu",
    enable_mkldnn=False,
    use_doc_orientation_classify=True,
    use_doc_unwarping=False,
    use_textline_orientation=True,
)
```

`enable_mkldnn=False` is required for the current bundled CPU runtime.
PaddlePaddle 3.3.1 failed actual inference through oneDNN with:

```text
NotImplementedError: ConvertPirAttribute2RuntimeAttribute not support
[pir::ArrayAttribute<pir::DoubleAttribute>]
```

Standard Paddle CPU inference passed the packaged executable smoke test.

### 5. OCR response hardening

The sidecar OCR endpoint now:

- copies the PIL-backed NumPy array so inference receives writable memory;
- accepts both rectangular `[x1, y1, x2, y2]` boxes and polygon point arrays;
- ignores unsupported box shapes with a warning instead of returning HTTP 500;
- handles a missing or non-dictionary document preprocessor result.

### 6. Release workflow validation

`.github/workflows/release.yml` now tests real OCR inference before building
release archives. A sidecar that only passes `/health` cannot be released.

Portable staging is deleted and recreated before copying the sidecar:

```powershell
Remove-Item target/release/bin -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path target/release/bin
```

This prevents restored Cargo cache contents from blocking or contaminating a
new portable bundle.

`scripts/build-release.sh --windows-bundle` now identifies the newly
dispatched workflow using branch, commit SHA, and dispatch time. It can no
longer accidentally select an older successful workflow.

## Commits Created During This Debugging Session

```text
a95b179 fix(ai): validate packaged PP-OCR inference
5d0fea5 fix(release): track newly dispatched bundle run
fbab706 fix(ai): log model preparation traceback
378a608 fix(ai): preserve PaddleX OCR dependency metadata
c07312e fix(ocr): enforce PP-OCRv5 CPU inference
d15a36d fix(release): recreate portable bundle staging
```

All commits above were pushed to `origin/feat/ai-rag-ocr-modernization`.

## Workflow Evidence

Use these runs when reviewing the debugging sequence:

```text
27509718437
Failed: PaddleX OCR optional dependencies appeared unavailable because
PyInstaller omitted distribution metadata.

27510063334
Failed with full traceback confirming PaddleX `ocr`/`ocr-core` dependency
validation failure.

27510478370
Model preparation succeeded after metadata preservation. Actual inference
failed in PaddlePaddle 3.3.1 oneDNN/PIR execution. The run also proved that
PaddleOCR defaulted to PP-OCRv6 for English.

27510868937
Packaged PP-OCRv5 inference passed after explicitly selecting PP-OCRv5 and
disabling oneDNN. Installer built. Portable staging then failed because cached
target/release/bin already contained the sidecar.

27511498808
Final success. Packaged PP-OCRv5 smoke test, installer, portable ZIP,
checksums, and artifact upload all passed.
```

## Files Modified In This Session

These files were edited and must be read before changing this work:

```text
.github/workflows/release.yml
docs/ai-quality-stack.md
docs/architecture.md
docs/developer-guide.md
docs/quick-reference.md
docs/user-guide.md
scripts/build-release.sh
sidecar/app.py
sidecar/build.py
src/main.rs
```

This handoff file was added:

```text
docs/NEXT_SESSION_HANDOFF.md
```

## Additional Files Required For Continuation

These files were inspected or define contracts used by the changes. Read them
when validating or extending the implementation:

```text
sidecar/requirements.txt
config.toml
installer/screensearch.iss
scripts/build-local.sh
scripts/build-release.ps1
screensearch-capture/src/ocr_provider.rs
screensearch-capture/src/sidecar_ocr.rs
screensearch-embeddings/src/engine.rs
screensearch-ui/src/components/AiSettings.tsx
screensearch-ui/src/components/SettingsPanel.tsx
docs/cross-compilation.md
docs/commands-summary.md
docs/index.md
README.md
```

Do not delete the PowerShell scripts. The project is Linux-first, but the
PowerShell helpers remain available for Windows maintainers.

## Exact Next Steps

Perform these actions in order.

### 1. Verify repository state

```bash
git switch feat/ai-rag-ocr-modernization
git status --short
git log -1 --oneline
```

Expected head before committing the June 15 artifact-verification update:

```text
c97317e docs: add AI modernization session handoff
```

If `docs/NEXT_SESSION_HANDOFF.md` has been committed after this text was
written, the head will be the artifact-verification documentation commit
instead. Do not reset it.

### 2. Preserve or re-download the final validated Windows artifacts

The artifacts and checksums are already present and verified locally. Do not
delete them unless a fresh download is required. To restore them from the
authoritative successful run:

```bash
rm -rf target/x86_64-pc-windows-msvc/release/bundles/windows-full
mkdir -p target/x86_64-pc-windows-msvc/release/bundles/windows-full
gh run download 27511498808 \
  --repo nicolasestrem/screensearch \
  --name release-artifacts \
  --dir target/x86_64-pc-windows-msvc/release/bundles/windows-full
```

Confirm the directory contains:

```text
ScreenSearch-v0.4.35-Setup-Quality.exe
ScreenSearch-v0.4.35-Portable.zip
checksums.txt
```

Run:

```bash
find target/x86_64-pc-windows-msvc/release/bundles/windows-full \
  -maxdepth 1 -type f -printf '%f %s bytes\n' | sort
sha256sum \
  target/x86_64-pc-windows-msvc/release/bundles/windows-full/ScreenSearch-v0.4.35-Setup-Quality.exe \
  target/x86_64-pc-windows-msvc/release/bundles/windows-full/ScreenSearch-v0.4.35-Portable.zip
cat target/x86_64-pc-windows-msvc/release/bundles/windows-full/checksums.txt
```

The locally calculated hashes must match the values recorded above and
`checksums.txt`.

### 3. Perform Windows end-to-end validation

This is the primary remaining release gate. It requires a Windows machine and
cannot be replaced by another Linux source-level test.

Test either the final installer or the final portable ZIP from run
`27511498808`; do not retest an older artifact.

On first launch:

1. Confirm the log says `ScreenSearch quality sidecar is ready`.
2. Open Settings, Data & AI.
3. Select **Download / verify** for the fixed local quality stack.
4. Wait until OCR preparation reports ready.
5. Confirm new capture frames do not repeatedly log
   `PP-OCRv5 request failed`.
6. Confirm OCR text appears for new frames.
7. Search the log for `Quality sidecar:` and verify model names contain
   `PP-OCRv5`, not `PP-OCRv6`.
8. Enable embeddings and verify Qwen3 model preparation and indexing.
9. Confirm semantic or hybrid search coverage increases above zero.
10. Exercise bundled Ministral model download separately; OCR model preparation
    and generation model download are independent paths.

Expected OCR behavior:

```text
PP-OCRv5 succeeds -> no Windows OCR fallback warning for that frame.
PP-OCRv5 request fails -> the app logs the detailed sidecar error and retries
the frame with Windows OCR when fallback_to_windows = true.
```

### 4. Review PR #63 checks and diff

```bash
gh pr view 63 --repo nicolasestrem/screensearch
gh pr checks 63 --repo nicolasestrem/screensearch
git diff main...HEAD --check
git status --short
```

Do not open another pull request. Update PR #63 only.

GitHub CLI authentication was restored on June 15, 2026. Use `gh` for PR
metadata, checks, comments, thread replies, and workflow logs.

### 5. Address only evidence-based follow-up failures

If Windows validation fails, preserve:

```text
Quality sidecar:
PP-OCRv5 request failed
Model preparation failed while initializing
```

Include the complete chained exception. Do not remove the executable smoke
test or re-enable health-only release validation.

If OCR works, do not change the current `device="cpu"` and
`enable_mkldnn=False` settings without adding and passing a separate Windows
GPU or oneDNN workflow. The current settings are the validated v0.4.35
contract.

### 6. Finish PR #63

After final Windows validation:

1. update PR #63's description with run `27511498808`;
2. record whether installer, portable ZIP, OCR, Qwen preparation, indexing,
   and Ministral download passed on the Windows machine;
3. keep the PR open unless the user explicitly asks to merge it;
4. do not publish a GitHub release unless explicitly requested.

## Verification Already Completed

Successful local checks during this session:

```text
python3 -m py_compile sidecar/app.py sidecar/build.py
npm run build
npm run lint
cargo check --locked -p screensearch-db -p screensearch-embeddings -p screensearch-api
cargo test --locked -p screensearch-db -p screensearch-embeddings -p screensearch-api --lib
cargo xwin build --release --target x86_64-pc-windows-msvc --locked
git diff --check
bash -n scripts/build-release.sh
```

Focused Rust tests passed:

```text
screensearch-api: 14 passed
screensearch-db: 4 passed
screensearch-embeddings: 7 passed
```

The repository-wide native Linux `cargo check --workspace --all-targets` was
blocked by missing system GLib/GObject development packages. The
workspace-wide `cargo fmt --check` also reports extensive pre-existing
formatting differences outside this change. Do not mass-format unrelated Rust
files.

## Non-Blocking Follow-Up

GitHub Actions emitted Node.js 20 deprecation warnings for several action
versions. GitHub indicates Node.js 24 becomes the default on June 16, 2026 and
Node.js 20 is removed on September 16, 2026. Treat action upgrades as a
secondary CI-maintenance task after PR #63's Windows application validation.

Dependabot also reports two vulnerabilities on the default branch. They were
not introduced or addressed by this OCR fix and should be handled separately.

## PR Review Follow-Up Started June 15, 2026

PR #63 received five unresolved inline threads and a comprehensive review with
15 findings. The follow-up implementation addresses:

- atomic per-frame embedding replacement;
- unique frame/chunk identities through migration 010;
- embedding response count validation;
- time-filtered KNN expansion through a guaranteed full-index fallback;
- empty chunk filtering and OCR database-error logging;
- JPEG OCR transport and bounded sidecar uploads;
- removal of the misleading empty in-memory vector index;
- OCR result-length warnings and PaddleOCR 3.x validation;
- serialized cached model initialization;
- removal of the ignored hybrid-search alpha parameter from runtime calls;
- explicit migration and release documentation.

The configured embedding model-name check remains intentionally strict. The
persisted vector contract is fixed for v0.4.35, so accepting a different
quantized or otherwise compatible-looking model would mix non-equivalent
vectors in one index.

Files additionally modified during review follow-up:

```text
CHANGELOG.md
RELEASE_NOTES.md
docs/security.md
screensearch-api/src/handlers/embeddings.rs
screensearch-api/src/handlers/rag_helpers.rs
screensearch-api/src/handlers/search.rs
screensearch-api/src/workers/embedding_worker.rs
screensearch-capture/src/sidecar_ocr.rs
screensearch-db/src/migrations.rs
screensearch-db/src/queries.rs
screensearch-db/src/vector_search.rs
screensearch-db/tests/integration_tests.rs
screensearch-embeddings/src/engine.rs
```

Review-follow-up verification completed:

```text
python3 -m py_compile sidecar/app.py sidecar/build.py
cargo check --locked -p screensearch-db -p screensearch-embeddings -p screensearch-api -p screensearch-capture
cargo test --locked -p screensearch-db
cargo test --locked -p screensearch-embeddings -p screensearch-api --lib
cargo check --locked -p screensearch-capture --all-targets
cargo clippy --locked -p screensearch-db -p screensearch-embeddings -p screensearch-api -p screensearch-capture --all-targets --no-deps -- -D warnings
cargo xwin check --locked --target x86_64-pc-windows-msvc -p screensearch-db -p screensearch-embeddings -p screensearch-api -p screensearch-capture
rustfmt --edition 2021 --check <all changed Rust files>
git diff --check
```

The database integration suite passed 17 tests. The API suite passed 14 tests,
and the embeddings suite passed 8 tests. Native Linux linking of the
`screensearch-capture` test binary remains blocked by pre-existing unresolved
Windows symbols; capture compiles for Linux all-targets and the changed crates
pass the Windows MSVC cross-check.
