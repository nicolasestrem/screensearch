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
./scripts/build-local.sh
./scripts/build-local.sh --release
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

```bash
./scripts/build-release.sh 0.4.35
./scripts/build-release.sh 0.4.35 --windows-bundle
./scripts/build-release.sh 0.4.35 --publish
```

The first command validates and cross-compiles from Linux. `--windows-bundle`
runs Windows packaging in GitHub Actions and downloads the installer, portable
ZIP, and checksums without creating a release. `--publish` creates and pushes
`v0.4.35`, producing a draft release.

The validation command also creates a clearly labeled core preview:

```text
target/x86_64-pc-windows-msvc/release/bundles/
  ScreenSearch-v0.4.35-Windows-Core-Preview.zip
```

It does not contain the Windows AI sidecar.

The existing PowerShell scripts remain available for direct Windows use. Linux
development and release preparation should use the Bash entrypoints above.

The release build includes the sidecar directory. Prepare uncached model
weights from Settings or `POST /api/embeddings/models/prepare`.
