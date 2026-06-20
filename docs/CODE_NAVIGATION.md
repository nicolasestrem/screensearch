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
| Generation / vision provider client | `screensearch-vision/src/client.rs` |
| Bundled local generation runtime | `screensearch-llm/` |
| Unified server selection (text + vision) | `screensearch-api/src/state.rs` (`get_llama_server`) |

## Vision (Screen Understanding)

| Concern | File |
|---|---|
| `--mmproj` flag, GPU mode + log capture | `screensearch-llm/src/server.rs` (`gpu_active`, `gpu_health_timeout_secs`) |
| Model/projector discovery + quant pick | `screensearch-llm/src/download.rs` (`resolve_mmproj_for`, `resolve_vision_model`, `quant_desirability`, `is_loadable_model_gguf`) |
| Vision worker (queue consumer) | `screensearch-api/src/workers/vision_worker.rs` |
| Vision endpoints (analyze, status, models) | `screensearch-api/src/handlers/vision.rs` (`list_vision_models`) |
| Server status incl. `acceleration` | `screensearch-api/src/handlers/ai.rs` (`get_server_status`) |
| Queue and status queries | `screensearch-db/src/queries.rs` (`enqueue_frame_for_analysis`, `get_unanalyzed_frame_ids`, `get_vision_status`) |
| `vision_*` settings columns + Qwen3-VL default | `screensearch-db/src/migrations.rs` (`012_qwen3vl_vision_default`) |

## Startup & Readiness

| Concern | File |
|---|---|
| Readiness aggregator endpoint | `screensearch-api/src/handlers/system.rs` (`get_readiness`) |
| Startup readiness banner (UI) | `screensearch-ui/src/app/shell/ReadinessBanner.tsx` |
| Non-blocking engine-ready probe | `screensearch-api/src/state.rs` (`embedding_engine_initialized`) |
| SQLite `busy_timeout` | `screensearch-db/src/db.rs` |

## Frontend (Command Deck UI)

The UI was rebuilt greenfield (v0.5.0). It uses `react-router-dom` routing,
TanStack Query, a typed `fetch` client, and a Warm-Graphite design system with
Windows-native fonts. Tree under `screensearch-ui/src/`:

| Concern | File |
|---|---|
| App shell (grid, ⌘K listener) | `src/app/shell/AppShell.tsx` |
| Status rail / nav rail | `src/app/shell/StatusRail.tsx`, `src/app/shell/NavRail.tsx` |
| Command palette (⌘K) | `src/app/shell/CommandPalette.tsx` |
| Router + providers | `src/app/App.tsx` |
| Signature: Scanline Timeline | `src/components/ScanlineTimeline.tsx` |
| Panels / primitives | `src/components/Panel.tsx`, `src/components/ui.tsx` |
| Frame tile/row + image loader | `src/components/FrameTile.tsx`, `src/components/FrameImage.tsx` |
| Deck (home) | `src/pages/Deck.tsx` |
| Recall (Ask RAG + Report) | `src/pages/Recall.tsx` |
| Timeline + Moment detail | `src/pages/Timeline.tsx`, `src/pages/Moment.tsx` |
| Insights (real analytics) | `src/pages/Insights.tsx` |
| Settings (capture/vision/AI/models) | `src/pages/Settings.tsx` |
| Typed API client | `src/lib/api.ts` |
| Query hooks | `src/lib/hooks.ts` |
| Shared types | `src/lib/types.ts` |
| Activity mapping / formatters | `src/lib/activity.ts`, `src/lib/format.ts` |
| Design tokens | `src/index.css`, `tailwind.config.js` |
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
