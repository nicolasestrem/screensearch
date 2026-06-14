# ScreenSearch Quality Sidecar

The sidecar provides local PP-OCRv5, Qwen3 embeddings, and Qwen3 reranking on
`127.0.0.1:3132`. Models are downloaded into the standard Hugging Face and
Paddle caches on first use and are not included in the small core installer.

For development:

```bash
python -m venv .venv
.venv/Scripts/pip install -r requirements.txt
python app.py
```

Set `SCREENSEARCH_AI_SIDECAR_TOKEN` in both processes to require bearer
authentication. Build the Windows runtime directory with `python build.py`;
the executable is written under
`dist/screensearch-ai-sidecar/screensearch-ai-sidecar.exe`.
