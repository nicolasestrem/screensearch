# Project Index

## Product

ScreenSearch is a local-first Windows screen-history application. It captures
screens, extracts text, provides keyword and semantic retrieval, generates
grounded answers and reports, and exposes Windows automation APIs.

## Current Quality Stack

| Capability | Implementation |
|---|---|
| OCR | PP-OCRv5 |
| OCR fallback | Windows OCR |
| Embeddings | Qwen3-Embedding-0.6B, 1024 dimensions |
| Vector search | sqlite-vec cosine KNN |
| Lexical search | SQLite FTS5 |
| Fusion | Reciprocal Rank Fusion |
| Reranking | Qwen3-Reranker-0.6B |
| Citations | Stable `[frame:<id>]` identifiers |
| Optional generation | Ministral, Ollama-compatible, OpenAI-compatible |

## Entry Points

| Area | Entry |
|---|---|
| Application | `src/main.rs` |
| API | `screensearch-api/src/routes.rs` |
| Database | `screensearch-db/src/db.rs` |
| OCR | `screensearch-capture/src/ocr_provider.rs` |
| Embeddings | `screensearch-embeddings/src/engine.rs` |
| Sidecar | `sidecar/app.py` |
| Frontend | `screensearch-ui/src/main.tsx` |
| Installer | `installer/screensearch.iss` |

## Documentation

| Document | Audience |
|---|---|
| `README.md` | Product overview and common commands |
| `docs/user-guide.md` | Installation, settings, search, troubleshooting |
| `docs/architecture.md` | Runtime and data architecture |
| `docs/api-reference.md` | Current HTTP routes and payloads |
| `docs/developer-guide.md` | Development, testing, release |
| `docs/quick-reference.md` | Commands and active contracts |
| `docs/ai-quality-stack.md` | Model contract and evaluation |
| `docs/security.md` | Privacy and security boundaries |
| `docs/CODE_NAVIGATION.md` | File-level routing |

## Configuration Sources

`config.toml` controls startup services such as capture, OCR provider,
database, retention, and embedding worker defaults.

The SQLite `settings` row controls runtime capture settings and optional
generation-provider settings. Existing `vision_*` database names configure
generation only.

## Generated And External Data

Do not commit:

- `target/`;
- `node_modules/`;
- `screensearch-ui/dist/` unless release policy changes;
- captures and databases;
- logs;
- model weights and caches;
- PyInstaller `build/` and `dist/`;
- API keys.

## Validation Matrix

| Check | Linux | Windows |
|---|---|---|
| Frontend lint/build | Yes | Yes |
| DB and embedding tests | Yes | Yes |
| Most Rust compile checks | Yes | Yes |
| Native capture | Limited | Required |
| Windows OCR fallback | No | Required |
| UI Automation | No | Required |
| Sidecar bundle | Limited | Required |
| Inno Setup installer | No | Required |

## Release Artifacts

- `ScreenSearch-v<version>-Setup-Quality.exe`
- `ScreenSearch-v<version>-Portable.zip`
- `checksums.txt`

Both application packages contain the sidecar runtime directory. Model weights
download on first use.
