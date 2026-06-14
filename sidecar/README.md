# ScreenSearch Quality Sidecar

The sidecar provides local PP-OCRv5, Qwen3 embeddings, and Qwen3 reranking on
`127.0.0.1:3132`. The runtime is included in Windows packages, while model
weights are downloaded into the standard Hugging Face and Paddle caches.

For development:

```bash
python -m venv .venv
.venv/Scripts/pip install -r requirements.txt
python app.py
```

Set `SCREENSEARCH_AI_SIDECAR_TOKEN` in both processes to require bearer
authentication. Build the Windows runtime directory with `py -3.12 build.py`;
the executable is written under
`dist/screensearch-ai-sidecar/screensearch-ai-sidecar.exe`.

Repository debug builds also search this `dist/` path. A Cargo build does not
install Python dependencies or build the sidecar automatically.

For a complete native Linux development build, run:

```bash
./scripts/build-local.sh
```

Windows sidecars are built on the Windows GitHub Actions release runner because
Linux PyInstaller output is not Windows-compatible.

Model management endpoints:

- `GET /v1/models/status` reports `idle`, `preparing`, `ready`, or `error`;
- `POST /v1/models/prepare` initializes PP-OCRv5, Qwen3 embeddings, and Qwen3
  reranking in a background thread, downloading uncached weights.

The settings panel accesses these endpoints through the application API.
Progress identifies the current component; the upstream model hosts do not
provide a reliable aggregate byte percentage across all model files.
