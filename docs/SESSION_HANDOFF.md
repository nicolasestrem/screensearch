# Session Handoff — Windows Release Build of ScreenSearch v0.4.35

> Purpose of this file: a precise, unambiguous handoff so the next Claude Code
> session — which will run on the user's **Windows machine** — can resume work
> without re-deriving context. Read this file first. Every path, command, ID, and
> commit hash below is literal and current as of the writing of this file.

---

## 1. Overall objective (what we are ultimately trying to achieve)

Produce a **viable, shippable Windows release bundle of ScreenSearch v0.4.35**
that includes the local PP-OCRv5 / Qwen quality **AI sidecar**. "Viable bundle"
means all three of these artifacts, built and self-consistent:

1. `ScreenSearch-v0.4.35-Setup-Quality.exe` — Inno Setup installer.
2. `ScreenSearch-v0.4.35-Portable.zip` — portable app + sidecar.
3. SHA-256 checksum file covering the above.

This work is part of the `feat/ai-rag-ocr-modernization` branch, which replaces
the legacy 384-dim embedding / OCR stack with the fixed local quality stack
(PP-OCRv5 OCR, Qwen3 embeddings 1024-dim, Qwen3 reranking, sqlite-vec, RRF).
A secondary objective, already completed this session, was improving release
build-pipeline observability.

**The next session's concrete goal:** on the Windows machine, run
`scripts\build-release.ps1 -Version 0.4.35`, obtain the three artifacts above in
`target\release\installers\`, and smoke-test the packaged sidecar.

---

## 2. Why the Windows machine (non-negotiable constraint — do not re-litigate)

The AI sidecar executable and the Inno Setup installer **cannot** be produced on
Linux:

- `sidecar/build.py` runs **PyInstaller**, which bundles the *host* OS Python
  interpreter plus native wheels (`torch`, `paddlepaddle`, `paddleocr`). On Linux
  PyInstaller emits a Linux ELF binary named `screensearch-ai-sidecar` (no
  `.exe`); it does **not** cross-compile to a Windows `.exe`. Proof in-repo:
  `scripts/build-local.sh` runs the identical `sidecar/build.py` to produce a
  *Linux* bundle.
- Inno Setup (`ISCC.exe`) is a Windows-only tool.

Therefore the full Windows bundle must be built either on Windows natively
(the chosen path for next session) or in GitHub Actions on `windows-latest`.
The Rust `screensearch.exe` and the React UI *can* be built on Linux (we did,
via `cargo-xwin`), but those two pieces alone are not a "viable bundle" because
they lack the sidecar.

---

## 3. What we accomplished this session (facts, no ambiguity)

### 3.1 Diagnosis
The user reported `./scripts/build-release.sh 0.4.35 --windows-bundle` appearing
to "hang" at `* Build quality AI sidecar`. It was **not** hung. That `bash`
script does not build the sidecar locally — with `--windows-bundle` it dispatches
the `release.yml` workflow to GitHub Actions and then blocks on
`gh run watch "$run_id" --exit-status` (see `scripts/build-release.sh`).
`gh run watch` only redraws on **step boundaries** and never streams a running
step's stdout, so the single ~7.5-minute sidecar step (pip install of
torch/paddle/sentence-transformers, then PyInstaller `--collect-all` over
`paddle`, `paddleocr`, `paddlex`, `sentence_transformers`) showed as a frozen
line. Measured durations: prior successful run `27511498808` = 7.5 min for that
step; the run observed during diagnosis `27541964692` = 7.3 min, then progressed
normally. Nothing was broken.

### 3.2 Code change — pipeline observability (committed)
Split the single `Build quality AI sidecar` step in
`.github/workflows/release.yml` into three sequential steps so `gh run watch`
advances through visible boundaries:
- `Upgrade pip` → `python -m pip install --upgrade pip`
- `Install sidecar dependencies` → `pip install -r sidecar/requirements.txt`
- `Bundle sidecar with PyInstaller` → `python sidecar/build.py`

Added a heads-up message in `scripts/build-release.sh` immediately before
`gh run watch`, and recorded the change in `CHANGELOG.md` under `[0.4.35]`.

These three files were modified this session:
- `.github/workflows/release.yml`
- `scripts/build-release.sh`
- `CHANGELOG.md`

Committed as **`9258618`** ("ci(release): stage sidecar build for visible
progress") and pushed to `feat/ai-rag-ocr-modernization`. (This handoff file is
committed on top of `9258618`.)

### 3.3 Build run executed
Ran `./scripts/build-release.sh 0.4.35 --windows-bundle` on the Linux host:
- `[1/5]` React UI built.
- `[2/5]` Linux quality checks passed: 14 (`screensearch-api`) + 3
  (`screensearch-db`) + 8 (`screensearch-embeddings`) unit tests all passed;
  ESLint clean; `py_compile` of sidecar files clean.
- `[3/5]` Cross-compiled `target/x86_64-pc-windows-msvc/release/screensearch.exe`
  (18 MB) in 1m44s via `cargo xwin`.
- `[4/5]`/`[5/5]` Created
  `target/x86_64-pc-windows-msvc/release/bundles/ScreenSearch-v0.4.35-Windows-Core-Preview.zip`
  (7.9 MB). NOTE: the core-preview ZIP intentionally does **not** contain the
  AI sidecar.
- Dispatched GitHub Actions **Release run #63**, id **`27543277013`**, on branch
  `feat/ai-rag-ocr-modernization`. At last observation it was running through the
  new split steps correctly. When it finishes it auto-downloads the full Windows
  artifacts to `target/x86_64-pc-windows-msvc/release/bundles/windows-full/`
  **on the Linux host** (not on the Windows machine).

### 3.4 Decision
Next session continues on the user's **Windows machine** using
`scripts\build-release.ps1`. CI run #63 was left to finish as a fallback /
cross-check; it was **not** stopped.

---

## 4. Files required for the next session (Windows build)

Primary script to run:
- `scripts/build-release.ps1` — native Windows full release build (8 steps).

Files that script depends on (verify they exist after `git pull`):
- `sidecar/build.py` — PyInstaller build of the sidecar `.exe`.
- `sidecar/requirements.txt` — sidecar Python deps (torch, paddlepaddle,
  paddleocr, sentence-transformers, transformers, pyinstaller, etc.).
- `sidecar/app.py` — the sidecar FastAPI service (built into the `.exe`).
- `installer/screensearch.iss` — Inno Setup installer definition.
- `scripts/generate-checksums.ps1` — called by step 8.
- `scripts/sign-binary.ps1` — called only if `-SignBinary` is passed (optional).
- `Cargo.toml` — confirm `version = "0.4.35"` (it is, as of `9258618`).
- `config.toml`, `LICENSE`, `README.md` — bundled into the artifacts.

Reference files (read for context, not modified next session):
- `.github/workflows/release.yml` — the CI equivalent; its "Smoke test packaged
  PP-OCRv5" step is the authoritative validation recipe (see §6).
- `scripts/build-release.sh` — the Linux/CI-dispatch path (not used on Windows).
- `scripts/build-local.sh` — Linux native dev bundle (proves the cross-compile
  constraint; not used on Windows).
- `CHANGELOG.md` — keep updating under `[0.4.35]` per repo convention.
- `CLAUDE.md` (root) — project rules: paste verbatim verification output; never
  stub/mock; update CHANGELOG; use feature branches.

---

## 5. Strategy for the next session (Windows machine) — ordered, explicit

**Step 0 — Sync the code.** In PowerShell at the repo root:
```powershell
git fetch origin
git checkout feat/ai-rag-ocr-modernization
git pull
git log --oneline -3   # confirm 9258618 + this handoff commit are present
```

**Step 1 — Verify prerequisites (the .ps1 hard-fails without these):**
- Rust MSVC toolchain: `rustup default stable-msvc` then `cargo --version`.
- Node.js / npm: `npm --version`.
- Python 3 + pip: `python --version` (first sidecar build downloads ~GB of
  torch/paddle wheels).
- Inno Setup 6 present at exactly `C:\Program Files (x86)\Inno Setup 6\ISCC.exe`
  (the script checks this literal path). Install from
  https://jrsoftware.org/isdl.php if missing.

**Step 2 — Run the full build:**
```powershell
.\scripts\build-release.ps1 -Version 0.4.35
```
If Inno Setup or the sidecar deps are not yet installed and you want a quick
core-only build first, use `.\scripts\build-release.ps1 -Version 0.4.35 -SkipSidecar`
— but the final viable bundle (the objective) REQUIRES the sidecar, so do a full
run before considering the task complete.

What the script does (from `scripts/build-release.ps1`):
1. `[1/8]` `npm ci` + `npm run build` (UI)
2. `[2/8]` `cargo build --release` (produces `target\release\screensearch.exe`)
3. `[3/8]` code signing — skipped unless `-SignBinary`
4. `[4/8]` `pip install -r sidecar\requirements.txt` + `python sidecar\build.py`
   (produces `sidecar\dist\screensearch-ai-sidecar\screensearch-ai-sidecar.exe`)
5. `[5/8]` verify `ISCC.exe` exists
6. `[6/8]` `ISCC.exe installer\screensearch.iss`, then rename output to
   `ScreenSearch-v0.4.35-Setup-Quality.exe`
7. `[7/8]` build `ScreenSearch-v0.4.35-Portable.zip` (screensearch.exe + bin/ +
   config.toml + LICENSE + README.md)
8. `[8/8]` `scripts\generate-checksums.ps1` over `target\release\installers`

**Step 3 — Confirm outputs.** Expect in `target\release\installers\`:
```
ScreenSearch-v0.4.35-Setup-Quality.exe
ScreenSearch-v0.4.35-Portable.zip
<checksums file>
```
Per the root `CLAUDE.md`, paste the verbatim tail of the build output (the
"Release Artifacts:" summary the script prints) as proof — do not paraphrase.

**Step 4 — Smoke-test the packaged sidecar (see §6).**

**Step 5 — Record results.** Update `CHANGELOG.md` if anything changed, and if
the build succeeds, the script's own "Next Steps" suggests tagging:
`git tag -a v0.4.35 -m "Release v0.4.35"` then `git push origin v0.4.35` — but
**only tag if the user explicitly approves**, because pushing a `v*.*.*` tag
triggers `release.yml` again (per its `on.push.tags` trigger).

---

## 6. Sidecar smoke-test recipe (authoritative, copied from release.yml)

After a successful build, validate the packaged sidecar exactly as CI does. The
sidecar listens on port **3132** and requires a bearer token. From PowerShell:

```powershell
$env:SCREENSEARCH_AI_SIDECAR_TOKEN = "release-smoke-test"
$exe = "sidecar\dist\screensearch-ai-sidecar\screensearch-ai-sidecar.exe"
$p = Start-Process -FilePath $exe -PassThru
$headers = @{ Authorization = "Bearer $env:SCREENSEARCH_AI_SIDECAR_TOKEN" }

# Poll health (CI allows up to ~60s)
Invoke-RestMethod -Uri "http://127.0.0.1:3132/health" -Headers $headers

# Prepare OCR component, then it is ready for PP-OCRv5 inference
$body = @{ components = @("ocr"); ocr_language = "en" } | ConvertTo-Json
Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:3132/v1/models/prepare" -Headers $headers -Body $body -ContentType "application/json"
```
A healthy `/health` response followed by a successful `/v1/models/prepare`
indicates the packaged sidecar is viable. Stop the process when done
(`Stop-Process -Id $p.Id -Force`).

The main app serves the web UI on `http://localhost:3131` (port 3131); the
sidecar is separate on 3132.

---

## 7. Status of CI run #63 to check at the start of next session

- Run id `27543277013`, branch `feat/ai-rag-ocr-modernization`, dispatched this
  session, left running.
- To check from any machine with `gh` authenticated:
  `gh run view 27543277013 --json status,conclusion`
- If it succeeded, its full Windows artifacts were downloaded to
  `target/x86_64-pc-windows-msvc/release/bundles/windows-full/` **on the Linux
  host only**. That directory will not exist on the Windows machine; the Windows
  session builds fresh with `build-release.ps1`. Treat #63 purely as a
  cross-check that the same bundle builds cleanly in CI — the deliverable for
  next session is the locally built Windows bundle.

---

## 8. Definition of done for next session

All true:
1. `scripts\build-release.ps1 -Version 0.4.35` completed without error.
2. `target\release\installers\` contains `ScreenSearch-v0.4.35-Setup-Quality.exe`,
   `ScreenSearch-v0.4.35-Portable.zip`, and the checksum file.
3. The packaged sidecar passed the §6 smoke test (`/health` + `/v1/models/prepare`).
4. Verbatim build/test output pasted as evidence (per `CLAUDE.md`).
5. `CHANGELOG.md` reflects any new changes; no tag pushed unless the user
   approved it.

---

## 9. Outcome — v0.4.35 Windows bundle COMPLETE (2026-06-15)

The objective was met on the Windows machine. All three artifacts were built in
`target\release\installers\` and the packaged sidecar passed the §6 smoke test
(`/health` → ok; `/v1/models/prepare` + `/v1/models/status` → `state: ready`,
`ready_components: ["ocr"]`).

### Environment deltas from CI (resolved this session)
The canonical `scripts\build-release.ps1` could not run unmodified on this host;
the pipeline was executed manually with these fixes (no edit to that script):
- Rust default toolchain was GNU → switched to `stable-x86_64-pc-windows-msvc`.
- `lld` (required by `.cargo/config.toml` for the msvc target) was absent →
  installed **LLVM** via winget so `lld`/`lld-link` are on PATH (matches CI's
  `windows-latest`).
- Default `python` was 3.14 (no torch/paddle wheels) → built the sidecar in a
  **Python 3.12 venv** (`sidecar\.venv`).
- `installer\resources\vc_redist.x64.exe` was missing (CI downloads it; the
  `.ps1` does not) → downloaded from `https://aka.ms/vs/17/release/vc_redist.x64.exe`.
- Inno Setup **7** is installed at `C:\Program Files\Inno Setup 7\ISCC.exe`; the
  `.ps1` hard-codes the absent v6 path, so `ISCC.exe` was invoked directly.

### Release-blocking bug found and fixed
The first sidecar build crashed at launch with
`OSError: [WinError 1114]` importing torch. Root cause: PyInstaller bundled a
stale MSVC C runtime (`msvcp140`/`vcruntime140_1` at **v14.34**) older than what
PyTorch's `c10.dll` requires (**v14.51**). PyTorch loads its DLLs with a
restricted search (`LoadLibraryExW(..., 0x1100)`) that prefers the bundle's
`_internal` directory over System32, so the stale runtime would have crashed the
shipped app on **every** machine. CI escaped it only because `windows-latest`
had a newer CRT for PyInstaller to bundle. Fixed permanently in
`sidecar/build.py`, which now refreshes the bundled MSVC runtime from the host's
System32 after PyInstaller runs (no-op off Windows). Verified on a clean rebuild.

### Still open (not done — user declined patching the `.ps1`)
For self-sufficient local Windows builds, `scripts\build-release.ps1` would still
need: (a) the Inno Setup path made version-agnostic, and (b) a `vc_redist`
download step. CI (`release.yml`) already covers both.
