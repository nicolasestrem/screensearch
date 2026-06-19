# Code Navigation

## Startup And Lifecycle

| Concern | File |
|---|---|
| Main application startup | `src/main.rs` |
| Generation runtime discovery and launch | `screensearch-llm/` |
| API server construction | `screensearch-api/src/server.rs` |
| Shared API state | `screensearch-api/src/state.rs` |
| Route definitions | `screensearch-api/src/routes.rs` |

## Capture And OCR

| Concern | File |
|---|---|
| Capture crate exports | `screensearch-capture/src/lib.rs` |
| OCR result model | `screensearch-capture/src/ocr.rs` |
| OCR processing worker | `screensearch-capture/src/ocr_processor.rs` |
| Provider selection | `screensearch-capture/src/ocr_provider.rs` |
| Windows OCR engine (WinRT Media.Ocr) | `screensearch-capture/src/ocr.rs` |

## Database And Retrieval

| Concern | File |
|---|---|
| SQLite setup and sqlite-vec registration | `screensearch-db/src/db.rs` |
| Schema migrations | `screensearch-db/src/migrations.rs` |
| Records and API models | `screensearch-db/src/models.rs` |
| Embedding and frame queries | `screensearch-db/src/queries.rs` |
| sqlite-vec KNN and RRF | `screensearch-db/src/vector_search.rs` |
| Database integration tests | `screensearch-db/tests/integration_tests.rs` |

## Embeddings And Reranking

| Concern | File |
|---|---|
| Model constants and configuration | `screensearch-embeddings/src/lib.rs` |
| In-process embedding engine (fastembed) | `screensearch-embeddings/src/engine.rs` |
| Text chunker | `screensearch-embeddings/src/chunker.rs` |
| Background indexing | `screensearch-api/src/workers/embedding_worker.rs` |
| Manual indexing and status | `screensearch-api/src/handlers/embeddings.rs` |

## RAG And Generation

| Concern | File |
|---|---|
| Shared hybrid retrieval and reranking | `screensearch-api/src/handlers/rag_helpers.rs` |
| Grounded answer endpoint | `screensearch-api/src/handlers/generate.rs` |
| Search modes | `screensearch-api/src/handlers/search.rs` |
| Report generation endpoints | `screensearch-api/src/handlers/ai.rs` |
| Generation provider client | `screensearch-vision/src/client.rs` |
| Bundled local generation runtime | `screensearch-llm/` |

## Frontend

| Concern | File |
|---|---|
| Settings panel | `screensearch-ui/src/components/SettingsPanel.tsx` |
| Quality-stack status | `screensearch-ui/src/components/EmbeddingsStatus.tsx` |
| Search mode selection | `screensearch-ui/src/components/SearchBar.tsx` |
| Search invitation | `screensearch-ui/src/components/search/SearchInvite.tsx` |
| Generation provider page | `screensearch-ui/src/components/AiSettings.tsx` |
| API client | `screensearch-ui/src/api/client.ts` |
| Shared types | `screensearch-ui/src/types/index.ts` |
| Embedded production assets | `screensearch-ui/dist/` |

## Packaging And CI

| Concern | File |
|---|---|
| Inno Setup installer | `installer/screensearch.iss` |
| Linux development bundle | `scripts/build-local.sh` |
| Linux release preparation | `scripts/build-release.sh` |
| API smoke test | `scripts/verify-api.sh` |
| Windows local bundle helper | `scripts/build-local.ps1` |
| Windows release helper | `scripts/build-release.ps1` |
| Windows checksum/signing helpers | `scripts/generate-checksums.ps1`, `scripts/sign-binary.ps1` |
| GitHub release build | `.github/workflows/release.yml` |
| Quality gates | `.github/workflows/quality.yml` |

## Evaluation And Documentation

| Concern | File |
|---|---|
| Retrieval cases | `evaluation/cases.jsonl` |
| Metrics script | `evaluation/evaluate.py` |
| Quality-stack contract | `docs/ai-quality-stack.md` |
| Architecture | `docs/architecture.md` |
| User workflow | `docs/user-guide.md` |
| API routes | `docs/api-reference.md` |

## Common Change Paths

### Change OCR behavior

Start with `ocr_provider.rs` and `ocr.rs` (the in-process Windows OCR engine).
Verify persisted OCR metadata.

### Change embedding model

Treat this as a database contract change. Update constants in
`screensearch-embeddings/src/lib.rs`, the engine, migration, sqlite-vec
dimension, invalidation metadata, tests, evaluation, settings display, and docs
together.

### Change retrieval ranking

Update `vector_search.rs` and `rag_helpers.rs`, then run the versioned
evaluation set.

### Change settings UI

Install frontend dependencies with `npm ci`. The API build script watches
frontend sources and rebuilds them before Rust embeds the assets.
