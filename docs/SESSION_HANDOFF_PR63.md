# Session Handoff — PR #63 (feat/ai-rag-ocr-modernization)

**Read this file first.** It is the authoritative continuation point for the work
on PR #63. Date written: 2026-06-16. Author: Claude (Opus 4.8). All paths are
Windows absolute or repo-relative from `C:\Users\nicol\Documents\GitHub\screensearch`.

---

## 1. Overall objective (do not lose sight of this)

PR #63, branch `feat/ai-rag-ocr-modernization`, modernizes ScreenSearch's local
OCR / RAG / AI runtime. It replaced the old in-Rust ONNX OCR+embeddings with a
managed **Python "quality sidecar"** (`screensearch-ai-sidecar.exe`) that runs:
- **PP-OCRv5** (PaddleOCR) for OCR,
- **Qwen3-Embedding-0.6B** for embeddings (1024-dim, stored in sqlite-vec),
- **Qwen3-Reranker-0.6B** for reranking,
with hybrid retrieval (FTS5 + sqlite-vec + Reciprocal Rank Fusion). Text
generation ("intel reports", daily digest, answers) is delegated to a **separate
managed `llama-server`** (bundled Ministral-3B GGUF) or a remote OpenAI-compatible
provider — this is intentional and must stay separate from the Python sidecar.

The end goal of PR #63 is a stable, fast, GPU-capable Windows build that captures
screens, OCRs them, indexes embeddings, and answers questions — then merge to
`main` and cut a release.

This session's slice of that objective: make startup fast, make the monitor
picker work, remove dead ONNX-era code, and **GPU-accelerate OCR** so capture
keeps up.

---

## 2. What was accomplished this session (all PUSHED to PR #63)

Run `git log --oneline -8` to confirm. The six commits, newest first:

| Commit | Summary |
|--------|---------|
| `6d643d0` | feat(sidecar): GPU-accelerated OCR (paddle CUDA) + CPU embeddings fallback |
| `5dbb6cf` | fix(capture): don't freeze reconfiguration when OCR is backed up |
| `1eefd33` | docs: non-blocking sidecar, native Windows build, Python 3.12.0 trap |
| `b3334eb` | fix(sidecar): refuse to build with Python 3.12.0 (frozen scipy crash) |
| `12a4e1c` | feat: non-blocking sidecar startup, OCR self-heal, ONNX-era cleanup |
| (prior)   | `4f2fb31`, `798a79b` etc. were already on the branch before this session |

### 2.1 Non-blocking startup (`src/main.rs`)
`ensure_quality_sidecar` previously **blocked** the whole launch up to 60s polling
`/health`. It now spawns the sidecar and polls readiness in a detached
`tokio::spawn`, returning `Some(child)` immediately. The API/UI come up in ~2s.
Verified live: process start → API listening = ~2.06s, before the sidecar reported
ready.

### 2.2 OCR self-heal (`screensearch-capture/src/ocr_provider.rs`)
`OcrProviderEngine::new` no longer gates on an initial health check (it used to
**permanently** demote OCR to Windows when the sidecar was still cold). It now
always constructs `PreferredProvider::PpOcr` (+ Windows fallback); `process_image`
tries the sidecar first per request, so OCR upgrades to PP-OCRv5 automatically once
the sidecar is healthy. The now-unused `health_check` method was removed from
`screensearch-capture/src/sidecar_ocr.rs`.

### 2.3 Capture-freeze fix (`src/main.rs`) — fixes "toggling a monitor stops capture"
The capture-drain loop used a blocking `frame_tx.send().await`. When OCR backed up
(channel full), the whole `tokio::select!` loop parked and could not process
monitor reconfiguration or shutdown — so toggling a monitor appeared to freeze
capture. Now uses `try_send`:
```rust
while let Some(frame) = capture_engine.try_get_frame() {
   use tokio::sync::mpsc::error::TrySendError;
   match frame_tx.try_send(frame) {
       Ok(()) => {}
       Err(TrySendError::Full(_)) => break,   // shed load via the bounded queue
       Err(TrySendError::Closed(_)) => break,
   }
}
```

### 2.4 Dead ONNX-era cleanup
- Removed 7 unused `[embeddings]` config fields (`model`, `model_name`,
  `embedding_dim`, `max_chunk_tokens`, `chunk_overlap`, `hybrid_search_alpha`,
  `max_context_chunks`) from `EmbeddingsSettings` in `src/main.rs`, its
  `default_embeddings_settings()`, and `config.toml`. Only `enabled`/`batch_size`
  remain. (Serde has no `deny_unknown_fields`, so old config files still load.)
- Deleted `screensearch-embeddings/src/chunker.rs` (`TextChunker`) and its export
  in `screensearch-embeddings/src/lib.rs`; chunking is delegated to the sidecar
  `/v1/chunk` endpoint.
- Bumped version `0.4.36 → 0.4.37` (`Cargo.toml`, `installer/screensearch.iss`).
- Bench vectors 384→1024-dim in `screensearch-db/benches/db_benchmarks.rs`.

### 2.5 Python 3.12.0 build guard (`sidecar/build.py`)
Python **3.12.0** has a PEP 709 codegen bug that makes the *frozen* sidecar crash
importing scipy (`NameError: name 'obj' is not defined` in
`scipy/stats/_distn_infrastructure.py`), silently degrading OCR to Windows. Proven
by bisection: same scipy 1.17.1 + PyInstaller 6.21.0, only the interpreter changed
— 3.12.0 fails, 3.12.10 works. `sidecar/build.py` now hard-errors under Python <
3.12.1. **Never build the sidecar with `C:\Python312` (it is 3.12.0).**

### 2.6 Native-Windows build fixes (`scripts/build-release.ps1`)
- Imports the MSVC env via `vswhere` and sets
  `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=link.exe` (because
  `.cargo/config.toml` hard-codes the cross-compile-only `lld` linker — **do NOT
  edit `.cargo/config.toml`**).
- Detects Inno Setup 6 or 7 (not just the hardcoded 6 path).
- New `-PythonExe` parameter to choose the sidecar's Python.
- New `-Gpu` switch (see §2.7).

### 2.7 GPU-accelerated OCR (the big one) — `sidecar/app.py`, `sidecar/build.py`, `sidecar/requirements-gpu.txt`, `scripts/build-release.ps1`
**Why:** OCR was ~60s/frame on CPU (`avg_time=58410ms` in the user's logs), which
backed up the capture queue ("Frame queue full, dropping oldest frame") and timed
out embedding requests. On the GPU it is ~1.5s/frame.

**Architecture decision (forced, not a preference) — MEMORIZE THIS:**
Only **OCR (paddle) runs on the GPU; embeddings/reranking (torch) stay on CPU.**
torch and paddle each bundle their own CUDA runtime DLLs under identical names
(`cublas64_12.dll`, `cudart64_12.dll`, …) and there is **no CUDA build both support
that also targets Blackwell** (torch ships cu128; paddle ships cu126/cu129; cu126
predates Blackwell sm_120). Loading both GPU runtimes in one process collides with
`OSError: [WinError 127]`. torch-CPU has no CUDA DLLs, so it coexists with
paddle-gpu cleanly. OCR was the only component that needed the GPU.

Code in `sidecar/app.py`:
```python
def paddle_device() -> str:
    try:
        import paddle
        if paddle.device.is_compiled_with_cuda() and paddle.device.cuda.device_count() > 0:
            return "gpu"
    except Exception:
        logger.exception("Paddle GPU probe failed; using CPU for OCR")
    return "cpu"
```
`_load_ocr_model` now uses `device=paddle_device()` and disables the doc- and
textline-orientation classifiers (`use_doc_orientation_classify=False`,
`use_textline_orientation=False`) — screen text is upright, so they were overhead.
`device()` (torch) stays `cpu` because torch is the CPU build. `/health` now
reports `ocr_device` alongside `device`.

`sidecar/build.py` auto-detects a CUDA build (`paddle.device.is_compiled_with_cuda()`
or `torch.version.cuda`) and adds `--collect-all torch --collect-all nvidia` so the
`nvidia/*/bin/*.dll` CUDA libs are bundled.

`scripts/build-release.ps1 -Gpu` install order (CUDA wheels need their own indexes;
a plain requirements file pulls CPU torch from PyPI):
```powershell
pip install "paddlepaddle-gpu>=3,<4" -i https://www.paddlepaddle.org.cn/packages/stable/cu129/
pip install -r sidecar\requirements-gpu.txt   # torch (CPU) + paddleocr + server deps, NO paddlepaddle
```

**Verified on the user's RTX 5060 Ti (Blackwell, compute capability 12.0, 16GB):**
- paddle matmul on GPU: 0.19s; PP-OCRv5 warm OCR (venv): 0.96–1.6s.
- **Frozen 5.4GB GPU bundle**: `/health` reports `"ocr_device":"gpu"`, warm OCR
  request **1.62s** (HTTP 200, 40 lines) vs ~60s CPU.

---

## 3. Exact environment state (verify these still exist next session)

- **Repo / branch**: `C:\Users\nicol\Documents\GitHub\screensearch`, branch
  `feat/ai-rag-ocr-modernization` (== `origin`, clean tree). PR **#63** (open, draft).
- **GPU**: NVIDIA GeForce RTX 5060 Ti, Blackwell, compute capability **12.0**, 16GB,
  driver 610.47. Confirm with `nvidia-smi`.
- **Python interpreters**:
  - `C:\Python312\python.exe` = **3.12.0 — DO NOT use for sidecar builds** (frozen
    scipy crash; `build.py` will reject it).
  - `C:\Users\nicol\ss-venv312\` = uv venv on patched **3.12.10**, has the **CPU**
    sidecar deps (torch CPU + paddlepaddle CPU + paddleocr + sentence-transformers).
  - `C:\Users\nicol\ss-venv-gpu\` = uv venv on **3.12.10**, has the **GPU** sidecar
    deps: `paddlepaddle-gpu==3.3.1` (cu129), `torch 2.12.0+cpu`, paddleocr 3.7.0,
    sentence-transformers, fastapi, uvicorn, pyinstaller. **This is the venv to use
    for GPU sidecar builds.**
- **Built GPU sidecar (validated, 5.4GB)**: `sidecar\dist\screensearch-ai-sidecar\`
  — the on-directory PyInstaller bundle, `ocr_device=gpu`. CUDA DLLs at
  `sidecar\dist\screensearch-ai-sidecar\_internal\nvidia\cublas\bin\cublas64_12.dll`, etc.
- **Rust binary**: `target\release\screensearch.exe` (v0.4.37, includes the
  `try_send` capture fix; UI embedded).
- **CPU release artifacts (from earlier this session, sidecar was CPU)**:
  `target\release\installers\ScreenSearch-v0.4.37-Setup-Quality.exe` and
  `-Portable.zip`. NOTE: these bundle the **CPU** sidecar, not the GPU one.
- **Inno Setup**: `C:\Program Files (x86)\Inno Setup 6\ISCC.exe` (present).
- **MSVC vcvars**: `C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat`.

---

## 4. Next steps (ordered, unambiguous)

### Step A — Decide and produce the distributable GPU bundle (NOT done this session)
This session built and validated the GPU **sidecar** (`sidecar\dist\...`) and the
Rust binary, but did **not** build the installer/portable ZIP that bundles the GPU
sidecar. To produce it, run ONE of:

- **Fast (reuse the validated GPU sidecar dist):**
  ```powershell
  .\scripts\build-release.ps1 -Version 0.4.37 -SkipSidecar
  ```
  This rebuilds UI + Rust and packages the existing `sidecar\dist\screensearch-ai-sidecar`
  (currently the GPU build) into the installer + ZIP. PRECONDITION: confirm
  `sidecar\dist\screensearch-ai-sidecar` is still the GPU build — check
  `find sidecar/dist/screensearch-ai-sidecar -ipath "*nvidia*cublas*64*.dll"` returns a hit.
- **From scratch (rebuilds the GPU sidecar too, ~25 min):**
  ```powershell
  .\scripts\build-release.ps1 -Version 0.4.37 -Gpu -PythonExe "C:\Users\nicol\ss-venv-gpu\Scripts\python.exe"
  ```
The resulting installer/ZIP will be ~5–6GB. Output lands in
`target\release\installers\`. The version is still 0.4.37; if the user wants a
distinct artifact, bump to 0.4.38 first (`Cargo.toml` workspace+package version and
`installer\screensearch.iss` `MyAppVersion`) and add a CHANGELOG entry.

### Step B — Confirm GPU OCR end-to-end inside the running app (NOT fully shown this session)
This session proved the frozen sidecar does GPU OCR standalone, and that the app
spawns+connects to it (`sidecar_ready` in ~20s, no fallback warnings). It did NOT
capture an in-app OCR timing because the screen was static (the frame-differ
correctly skipped unchanged frames → `frames=0`). To close this:
1. `cd` to repo root, run `target\release\screensearch.exe` with `RUST_LOG=info`
   piped to a log (it auto-discovers `sidecar\dist\screensearch-ai-sidecar`).
2. Generate screen activity (move/resize windows) so the differ emits frames.
3. Grep the log for `OCR Metrics:` and confirm `avg_time` is ~1000–2000ms (GPU),
   not ~58000ms (CPU), and that there are no `Frame queue full` warnings.
Paste the verbatim metrics line as proof.

### Step C — (Optional, larger) Embeddings/reranking on GPU
Currently CPU (forced by the torch↔paddle CUDA conflict, §2.7). If the user wants
embeddings/reranking on GPU too, the only clean way is to run paddle-OCR and
torch-embeddings in **separate processes** (e.g., a second sidecar process, or
move embeddings/reranking into a torch-GPU subprocess). This is a real design
change — scope it explicitly and get approval before starting. Do NOT attempt to
load torch-CUDA and paddle-CUDA in one process (it WILL fail with WinError 127).

### Step D — Toward merging PR #63
- Re-run the review fixes verification listed in the PR body.
- `cargo build --release` (with the MSVC linker env, see §5) must pass; clippy on
  touched crates clean (repo-wide clippy/fmt are NOT clean — pre-existing debt).
- Decide CPU-default vs GPU-default release strategy: the default
  `build-release.ps1` (no `-Gpu`) ships the portable CPU sidecar; `-Gpu` ships the
  GPU one (requires/uses an NVIDIA GPU, falls back to CPU OCR when absent). The
  repo currently has no CI path for the GPU bundle — `.github/workflows/release.yml`
  builds the CPU sidecar.

### Step E — (Deferred research, low priority)
`docs/TODO/explore-pure-rust-onnx-vs-sidecar.md` tracks an exploration of replacing
the Python sidecar with in-process Rust ONNX. Not started; only pursue if the user
asks.

---

## 5. Strategy for the next session (follow in order)

1. **Re-orient (5 min).** Read this file. Then verify reality, do not assume:
   ```bash
   git -C "C:/Users/nicol/Documents/GitHub/screensearch" log --oneline -8
   git -C "C:/Users/nicol/Documents/GitHub/screensearch" status -sb
   ls -la sidecar/dist/screensearch-ai-sidecar/screensearch-ai-sidecar.exe   # GPU build present?
   "C:/Users/nicol/ss-venv-gpu/Scripts/python.exe" -c "import paddle; print(paddle.device.is_compiled_with_cuda(), paddle.device.cuda.device_count())"
   ```
2. **Ask the user which next step (A–E) they want.** Do not assume. Step A
   (distributable GPU bundle) and Step B (in-app GPU OCR proof) are the most likely
   immediate wants.
3. **For any cargo/clippy/test command on this native Windows host**, set the MSVC
   linker env first (the lld config breaks native builds):
   ```powershell
   cmd /c "`"C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat`" >nul 2>&1 && set" |
     ForEach-Object { if ($_ -match '^(.*?)=(.*)$') { Set-Item -Path "env:$($matches[1])" -Value $matches[2] } }
   $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = "link.exe"
   $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = "-C target-feature=+crt-static"
   $env:SKIP_UI_BUILD = "1"   # only when you already built screensearch-ui/dist
   ```
   (`scripts/build-release.ps1` already does this internally; the manual form is for
   ad-hoc `cargo` commands.)
4. **PowerShell quirks observed this session:** the PowerShell tool does NOT accept
   bash heredocs (`<<'EOF'`) — write a `.py`/`.ps1` file and run it, or use the Bash
   tool for heredocs. Background sidecars can linger on port 3132 across runs (a
   stale `python3.12 app.py` blocked a rebuild with `WinError 10048`); before
   launching a sidecar, kill leftovers: `Get-Process screensearch-ai-sidecar,python3.12 | Stop-Process -Force`
   and check `Get-NetTCPConnection -LocalPort 3132 -State Listen`.
5. **GPU verification is best done via `nvidia-smi`**, not Task Manager (Task
   Manager's default GPU graphs hide the CUDA "Compute_0" engine; CUDA work shows
   ~0% there). The user was confused by this — surface it proactively.
6. **Never claim a build works without running the frozen artifact.** PyInstaller +
   CUDA packaging is the risk area; always launch the frozen sidecar and check
   `/health` `ocr_device` + a real `/v1/ocr` request before declaring success.
7. **Always paste verbatim verification output** (build tails, `OCR Metrics` lines,
   `/health` JSON) — the user's CLAUDE.md requires it and explicitly forbids
   stubbing/mocking/paraphrasing.

---

## 6. Complete list of files touched this session (committed) + reference files

### Modified/created and committed (the change set for PR #63 this session)
- `src/main.rs` — non-blocking sidecar spawn; `try_send` capture drain; removed 7
  dead `[embeddings]` fields + their defaults.
- `screensearch-capture/src/ocr_provider.rs` — OCR self-heal (no startup health gate).
- `screensearch-capture/src/sidecar_ocr.rs` — removed dead `health_check`.
- `screensearch-embeddings/src/lib.rs` — removed `mod chunker;` + `TextChunker` export.
- `screensearch-embeddings/src/chunker.rs` — **deleted**.
- `config.toml` — trimmed `[embeddings]` to `enabled`/`batch_size`.
- `Cargo.toml` — version 0.4.37 (workspace + package).
- `installer/screensearch.iss` — `MyAppVersion` 0.4.37.
- `screensearch-db/benches/db_benchmarks.rs` — bench vectors 1024-dim.
- `sidecar/app.py` — `paddle_device()`, OCR `device=paddle_device()`, orientation
  classifiers off, `/health` `ocr_device`.
- `sidecar/build.py` — Python <3.12.1 guard; `_is_gpu_build()` + CUDA collect flags.
- `sidecar/requirements-gpu.txt` — **new**, GPU dep set (no torch/paddle index lines).
- `scripts/build-release.ps1` — MSVC linker env, Inno 6/7 detection, `-PythonExe`,
  `-Gpu`.
- `CHANGELOG.md` — 0.4.37 entries.
- `docs/architecture.md` — non-blocking startup + OCR self-heal.
- `docs/ai-quality-stack.md` — "Startup And Fallback", "GPU Acceleration".
- `docs/developer-guide.md` — "Native Windows release build", "GPU-accelerated OCR (-Gpu)".
- `docs/SESSION_HANDOFF_PR63.md` — **this file**.

### Files you will READ next session (not necessarily change)
- `sidecar/app.py` (OCR/embeddings/rerank endpoints, device selection)
- `sidecar/build.py`, `sidecar/requirements.txt`, `sidecar/requirements-gpu.txt`
- `scripts/build-release.ps1`, `installer/screensearch.iss`
- `src/main.rs` (`ensure_quality_sidecar` ~line 743; capture select loop ~line 600)
- `screensearch-capture/src/ocr_provider.rs`, `screensearch-capture/src/sidecar_ocr.rs`
- `screensearch-db/src/vector_search.rs`, `screensearch-db/src/queries.rs`,
  `screensearch-db/src/migrations.rs` (retrieval/embeddings storage — LIVE, do not remove)
- `screensearch-api/src/handlers/ai.rs`, `screensearch-api/src/handlers/rag_helpers.rs`
  (generation via llama-server — separate from the Python sidecar)
- `docs/ai-quality-stack.md`, `docs/developer-guide.md`, `docs/architecture.md`

### Memory files (auto-loaded; cross-reference)
- `memory/native-windows-build-workaround.md`
- `memory/sidecar-python-3120-codegen-bug.md`
- `memory/gpu-sidecar-ocr-build.md`

---

## 7. Known-good facts to NOT re-derive (saves time/tokens next session)
- The frozen GPU sidecar **works** on the 5060 Ti: `ocr_device=gpu`, ~1.6s warm OCR.
- torch+paddle cannot share a CUDA runtime on Blackwell (WinError 127). Settled.
- MKLDNN must stay **disabled** for PP-OCRv5 (crashes detection under PaddleOCR 3.x
  PIR: `ConvertPirAttribute2RuntimeAttribute not support`). Re-confirmed this session.
- `vector_search.rs` is **live** (cosine + RRF hybrid). Do not "clean it up".
- The generation LLM (`screensearch-llm`, Ministral via llama-server) is the
  intended backend for reports/answers — do NOT move it into the Python sidecar.
- `C:\Python312` is 3.12.0 (bad for sidecar builds). Use `ss-venv-gpu` /
  `ss-venv312` (3.12.10).
