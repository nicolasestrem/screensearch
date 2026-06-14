# Commands Summary

## Build

```bash
cd screensearch-ui
npm ci
npm run build
cd ..

cargo check
cargo build
cargo build --release --locked
powershell -ExecutionPolicy Bypass -File scripts\build-local.ps1
```

Build the UI before Rust because the production assets are embedded into
`screensearch-api`.

## Test

```bash
cargo test --locked -p screensearch-db
cargo test --locked -p screensearch-embeddings --lib

cd screensearch-ui
npm run lint
npm run build
npm audit --audit-level=high

python -m py_compile sidecar/app.py sidecar/build.py evaluation/evaluate.py
```

## Run

```bash
cargo run
```

Manual sidecar development:

```bash
python -m pip install -r sidecar/requirements.txt
python sidecar/app.py
```

## Search

```bash
curl "http://127.0.0.1:3131/api/search/?q=meeting&mode=fts"
curl "http://127.0.0.1:3131/api/search/?q=build%20failure&mode=semantic"
curl "http://127.0.0.1:3131/api/search/?q=build%20failure&mode=hybrid"
```

## RAG Status

```bash
curl "http://127.0.0.1:3131/api/embeddings/status"

curl -X POST "http://127.0.0.1:3131/api/embeddings/enable" \
  -H "Content-Type: application/json" \
  -d "true"

curl -X POST "http://127.0.0.1:3131/api/embeddings/generate" \
  -H "Content-Type: application/json" \
  -d '{"batch_size":50}'

curl -X POST "http://127.0.0.1:3131/api/embeddings/models/prepare"
```

## Grounded Answer

```bash
curl -X POST "http://127.0.0.1:3131/api/generate" \
  -H "Content-Type: application/json" \
  -d '{"query":"What was I debugging this morning?"}'
```

## Generation Provider

```bash
curl -X POST "http://127.0.0.1:3131/api/ai/validate" \
  -H "Content-Type: application/json" \
  -d '{"provider_url":"local","model":"ministral-3b"}'
```

## Frames And Tags

```bash
curl "http://127.0.0.1:3131/api/frames/?limit=10"
curl "http://127.0.0.1:3131/api/tags/"

curl -X POST "http://127.0.0.1:3131/api/tags/" \
  -H "Content-Type: application/json" \
  -d '{"tag_name":"important","color":"#FF0000"}'
```

## Automation

```bash
curl -X POST "http://127.0.0.1:3131/api/automation/click" \
  -H "Content-Type: application/json" \
  -d '{"x":100,"y":200}'

curl -X POST "http://127.0.0.1:3131/api/automation/type" \
  -H "Content-Type: application/json" \
  -d '{"text":"Hello World"}'
```

Automation is Windows-only and the API should remain bound to loopback.

## Evaluation

```bash
python evaluation/evaluate.py evaluation/results.jsonl
```

## Release

```powershell
.\scripts\build-release.ps1 -Version 0.4.35
```

The release build includes the sidecar directory. Prepare uncached model
weights from Settings or `POST /api/embeddings/models/prepare`.
