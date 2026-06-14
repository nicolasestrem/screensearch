# Repository Guidelines

## Project Structure & Module Organization

ScreenSearch is a Rust 2021 workspace with a React/TypeScript dashboard.

- `src/main.rs` wires the application together.
- `screensearch-capture/`, `screensearch-db/`, `screensearch-api/`, and `screensearch-automation/` contain capture, SQLite/FTS5, Axum API, and Windows automation code.
- `screensearch-embeddings/`, `screensearch-vision/`, and `screensearch-llm/` provide AI features.
- `screensearch-ui/src/` contains the Vite/React frontend.
- Rust tests live in `#[cfg(test)]` modules or crate-level `tests/` directories.
- `docs/`, `screenshots/`, and `installer/` hold documentation, media, and packaging.

## Build, Test, and Development Commands

Run backend commands from the repository root:

- `cargo check` performs a fast workspace check.
- `cargo build` builds all crates; `cargo run` starts the app on `localhost:3131`.
- `cargo test --all-targets` runs Rust tests.
- `cargo fmt --check` verifies Rust formatting.
- `cargo clippy --all-targets -- -D warnings` rejects lint warnings.

In `screensearch-ui/`, use `npm ci`, `npm run dev`, `npm run build`, and `npm run lint` to install, serve, build, and lint the dashboard.

## Coding Style & Naming Conventions

Use `rustfmt` defaults: four-space indentation, `snake_case` modules/functions, and `PascalCase` types. Prefer `thiserror` in libraries and contextual `anyhow` errors in application code. Document public APIs with `///`.

TypeScript is strict. Use two-space indentation, `PascalCase` components, `camelCase` values, and `useX` hook names. Prefer functional components and Tailwind classes.

## Testing Guidelines

Name tests descriptively, such as `test_search_empty_query_returns_error`. Use `#[tokio::test]` for async behavior and temporary SQLite databases. Run focused tests with `cargo test -p screensearch-db`. OCR, capture, and automation changes require Windows validation; Linux uses stubs. Add tests for new behavior without reducing coverage.

## Commit & Pull Request Guidelines

Recent history follows Conventional Commits: `feat:`, `fix:`, `chore:`, and scoped forms such as `fix(ci):`. Keep subjects focused and imperative.

Pull requests should explain the need, summarize implementation, link issues, and list verification commands. Include screenshots for UI changes and note Windows testing for platform-specific work. Formatting, linting, tests, and the frontend build must pass.

## Security & Configuration

Never commit API keys, databases, captures, logs, downloaded models, or user screen data. Keep defaults in `config.toml`; production data belongs in the platform data directory.
