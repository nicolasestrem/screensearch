# ScreenSearch Documentation

ScreenSearch is a local-first Windows screen-history application with
PP-OCRv5, Qwen3 retrieval, sqlite-vec, grounded generation, and Windows
automation.

## Start Here

| Need | Document |
|---|---|
| Install, configure, search, troubleshoot | [User Guide](user-guide.md) |
| Commands and active model contracts | [Quick Reference](quick-reference.md) |
| HTTP routes and payloads | [API Reference](api-reference.md) |
| Build and contribute | [Developer Guide](developer-guide.md) |
| Understand system design | [Architecture](architecture.md) |

## AI And OCR

| Document | Scope |
|---|---|
| [AI Quality Stack](ai-quality-stack.md) | PP-OCRv5, Qwen3, sqlite-vec, RRF, evaluation |
| [Embedded LLM](embedded-llm.md) | Optional bundled Ministral generation runtime |
| [Architecture](architecture.md) | Separation between retrieval and generation |
| [User Guide](user-guide.md#what-each-ai-component-does) | User-facing component roles |

The quality stack is fixed for this release:

- PP-OCRv5 with Windows OCR fallback;
- Qwen3-Embedding-0.6B, 1024 dimensions;
- sqlite-vec cosine KNN;
- FTS5 plus Reciprocal Rank Fusion;
- Qwen3-Reranker-0.6B.

The selectable LLM in settings is used only for descriptions, answers,
digests, and reports.

## Development

| Document | Scope |
|---|---|
| [Code Navigation](CODE_NAVIGATION.md) | Find implementation by feature |
| [Project Index](PROJECT_INDEX.md) | Crates, entry points, validation matrix |
| [Testing](testing.md) | Focused and Windows-native validation |
| [Cross Compilation](cross-compilation.md) | Linux-to-Windows Rust builds |
| [Security](security.md) | Data and network boundaries |
| [Frontend Design System](frontend-design-system.md) | Dashboard visual conventions |

## Build Reminder

The production UI is embedded in the Rust binary. Install its dependencies
before the first Cargo build:

```bash
cd screensearch-ui
npm ci
cd ..
cargo build
```

The API build script rebuilds changed frontend sources automatically.

## Services

| Service | Address |
|---|---|
| Dashboard and API | `127.0.0.1:3131` |
| Quality sidecar | `127.0.0.1:3132` |
| Bundled generation server | `127.0.0.1:31130` |

## Configuration

`config.toml` controls startup services:

- `[capture]`;
- `[storage]`;
- `[ocr]`;
- `[database]`;
- `[privacy]`;
- `[embeddings]`;
- `[api]`.

Runtime settings in SQLite control capture state, retention, exclusions, and
the optional generation provider. The legacy `vision_*` field names refer to
generation, not PP-OCRv5 or Qwen retrieval.

## Troubleshooting

| Problem | Reference |
|---|---|
| Old settings UI | [User Guide](user-guide.md#the-settings-panel-still-shows-old-ai-wording) |
| Sidecar unavailable | [User Guide](user-guide.md#quality-sidecar-unavailable) |
| Reindex required | [User Guide](user-guide.md#reindex-required) |
| Semantic search empty | [User Guide](user-guide.md#semantic-search-returns-few-results) |
| Release packaging | [Developer Guide](developer-guide.md#release) |

## Current Version

- Application: `0.4.35`
- Platform: Windows 10/11 production, Linux development support
- Language: Rust 2021
- UI: React, TypeScript, Vite 8
- Database: SQLite, FTS5, sqlite-vec
- API: Axum

Last updated: June 14, 2026.
