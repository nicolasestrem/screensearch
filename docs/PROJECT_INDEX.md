# Project Index

## Product

ScreenSearch is a local-first Windows screen-history application. It captures
screens, extracts text, provides keyword and semantic retrieval, generates
grounded answers and reports, and exposes Windows automation APIs.

## Current Quality Stack

| Capability | Implementation |
|---|---|
| OCR | Native Windows OCR (WinRT Media.Ocr), in-process |
| Embeddings | EmbeddingGemma-300M, 768 dimensions (fastembed, in-process) |
| Vector search | sqlite-vec cosine KNN |
| Lexical search | SQLite FTS5 |
| Fusion | Reciprocal Rank Fusion |
| Reranking | Optional bge-reranker-v2-m3 (fastembed), off by default |
| Citations | Stable `[frame:<id>]` identifiers |
| Optional generation | Auto-discovered local GGUF via llama.cpp, Ollama-compatible, OpenAI-compatible |
| Optional vision | On-device screen understanding via the unified local llama.cpp server (`--mmproj`, Qwen3-VL-4B by default), or an external vision provider |

## Entry Points

| Area | Entry |
|---|---|
| Application | `src/main.rs` |
| API | `screensearch-api/src/routes.rs` |
| Database | `screensearch-db/src/db.rs` |
| OCR | `screensearch-capture/src/ocr_provider.rs` |
| Embeddings | `screensearch-embeddings/src/engine.rs` |
| Generation runtime | `screensearch-llm/` |
| Vision worker | `screensearch-api/src/workers/vision_worker.rs` |
| Vision client | `screensearch-vision/src/client.rs` |
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
| `docs/vision.md` | On-device vision (screen understanding) setup and API |
| `docs/security.md` | Privacy and security boundaries |
| `docs/CODE_NAVIGATION.md` | File-level routing |

## Configuration Sources

`config.toml` controls startup services such as capture, OCR provider,
database, retention, and embedding worker defaults.

The SQLite `settings` row controls runtime capture settings, the optional
generation provider, and the optional vision pipeline. The `vision_*` columns
(`vision_enabled`, `vision_provider`, `vision_model`, `vision_endpoint`,
`vision_api_key`) now configure on-device (or external) screenshot analysis;
with `vision_provider = "local"` the unified llama.cpp server is used for both
generation and vision.

## Generated And External Data

Do not commit:

- `target/`;
- `node_modules/`;
- `screensearch-ui/dist/` unless release policy changes;
- captures and databases;
- logs;
- model weights and caches (including `.models/`);
- API keys.

## Validation Matrix

| Check | Linux | Windows |
|---|---|---|
| Frontend lint/build | Yes | Yes |
| DB and embedding tests | Yes | Yes |
| Most Rust compile checks | Yes | Yes |
| Native capture | Limited | Required |
| Windows OCR | No | Required |
| UI Automation | No | Required |
| Inno Setup installer | No | Required |

## Release Artifacts

- `ScreenSearch-v<version>-Setup-Quality.exe`
- `ScreenSearch-v<version>-Portable.zip`
- `checksums.txt`

Both application packages are self-contained executables. The embedding model
is downloaded and cached on first use; the optional generation GGUF is
auto-discovered locally or downloaded as the default fallback.
