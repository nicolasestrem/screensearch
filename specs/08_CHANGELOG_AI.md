# 08 - AI Changelog

The 0.4.5 arc shipped: P1 through P10, landed as PRs #150-#166. Its full record, including every
phase's build narrative, gate output and review-fix pass, is archived at
`specs/archive/08_CHANGELOG_AI.v0.4.5.md`; that archive is frozen and this live file carries only
the current arc.

## Current arc

### 2026-11-07: v0.4.5 installed-build QA fixes (branch `fix/0.4.5-qa-bugs`, from `main` `9c6a2cb`)

Phase: three defects reported from hand-testing the installed 0.4.5 candidate, plus the rebuild and
reinstall of that candidate. Decisions taken without asking are `07` QA2 D1-D4; the root causes and
the deliberately unfixed observations are `07` QA2 F1-F3 and QA2 U1-U3. PR #168 (updater slug) was
merged first, as instructed. No tag is created and no release is published; the version stays
`0.4.5` (`07` QA2 D2).

**Bug 1, a settings save always warned about the mark hotkey.** `overlay.rs` had two registration
paths and only the save path had a guard, and that guard compared chord *strings*. `init_*` had no
guard at all and wrote `None` into the stored chord on any failure, including the
`HotKey already registered` failure that a chord this process already holds must produce. One
failed startup attempt therefore erased the record, every later save retried the duplicate
registration and toasted, and a spelling variant of the same chord slipped past the string guard.
The field state was confirmed before any code changed: an OS probe from another process returned
Win32 `1409` for both `Ctrl+Alt+M` and `Ctrl+Alt+Z`, so this app already held both while the toast
claimed nothing was registered, and the missing "; still using the previous combination" suffix
identified the "nothing registered" branch. The fix replaces both paths with one
`apply_hotkey(env, slot, requested)` over an internal `HotkeyEnv` seam (the real `AppHotkeys` over
the plugin manager and `OverlayState`, plus a fake for tests): ownership is detected before any
attempt through the stored chord compared canonically (both sides parsed with the plugin's own
`Shortcut` parser) or through `GlobalShortcut::is_registered`, a failure never rewrites a live
chord, a repeated failure for an unchanged chord retries silently, and every user-facing sentence
goes through `humanize_hotkey_error`, which classifies the plugin's stable English phrases instead
of echoing its `Debug` payload. Nine unit tests drive the real function; the pre-fix logic was
restored temporarily to prove they catch the bug (5 failed, including a verbatim reproduction of the
field message), then the file was restored byte-identically (SHA-256 `7eadd0d…` checked both ways).

**Bug 2, Ask v2 printed Markdown literally.** `routes/Ask.tsx` pre-wrapped the answer in a `<p>`, a
P10 shortcut its own header comment recorded. The Markdown setup (react-markdown, GFM, the one link
override that keeps model-output links out of the WebView) now lives in
`components/domain/answerProseMarkdown.ts`, rendered by a thin `AnswerProse` component that
`AnswerStream` and both Ask answer positions (persisted turn and live stream) use. `AnswerStream`
keeps its own container classes, focus behavior, thinking trace and citation tiles unchanged.
Chunk evidence from a throwaway `vite build`: the Markdown stack is its own chunk
(`index-Cq5jl2Gv.js`, 157.07 kB raw / 47.56 kB gzip) behind `AnswerProse`; no entry chunk
(`main`, `react-vendor`, `query`, `globals`, `router`, `openExternal`, `overlay`, `EmptyState`,
`useAsk`, `FrameImage`) contains any of `micromark`, `react-markdown`, `remark-gfm` or
`hast-util-to-jsx-runtime`. The new test renders a representative answer through the real modules
(SSR) and asserts `<strong>`, `<ul>`, `<ol>`, `<em>`, `<code>`, `<pre>`, `<h1>`, `<blockquote>`,
`<table>` and a routed `<a>`, with no literal `**`; it also checks the element-to-token table
against the shipped `deck` typography block and puts every answer text token through the WCAG fence
in all four themes (308 pairs, all at least 4.5:1, tightest 4.88:1 for `--text-tertiary` bullets on
`--surface-selected`). No token failed, so nothing was re-coloured.

**Bug 3, Settings controls moved with the copy.** `SettingRow`'s control slot was a content-sized
flex item; `Toggle` is `flex items-start justify-between`, which has no slack in a shrink-to-fit
box, so the switch landed right after the hint text. The slot is now one fixed column
(`min-w-0 w-full max-w-full md:w-96`, the `--space-96` token, clamped by `max-w-full` for narrow
cells) taken from an exported `settingRowLayout` descriptor that depends only on the row variant, so
no label, hint or live state can move it. No primitive and no category file was changed. The visual
statement is proven in the installed app, not here.

**Pre-fix probes.** Both UI tests were reasoned from real code, not from the shipped pixels: `git
diff` on `Ask.tsx` shows only the two answer paragraphs (plus comments) replaced, and the settings
fix is a class-string change in one component. The installed-build check below is the acceptance
evidence for both.

### 2026-11-07: v0.4.5 release preparation and QA (branch `chore/0.4.5-release`, from `main` `440d2e9`)

Phase: the maintainer's 0.4.5 release kickoff. The kickoff replaces stop-at-ambiguity with "decide,
record it in `07`, continue", so every decision this pass took without asking is recorded as
`07` QA D1-D6 with the evidence that forced it. No tag is created and no release is published by
this pass; the installer is validated through a `workflow_dispatch` dry run and an installed copy.

**Version.** `0.4.2` to `0.4.5` in all six declaration sites: workspace `Cargo.toml` (crates inherit
`version.workspace = true`), root `package.json`, `ui/package.json`, `src-tauri/tauri.conf.json`
(the four fields the release workflow's tag-vs-version guard compares), plus both npm lock roots and
the 19 local packages in `Cargo.lock`. Third-party packages that happen to sit at `0.4.2`
(`constant_time_eq`, `wasm-streams`, `windows-strings`) were not touched, and `npm ci` on both locks
still passes.

**Docs.** `CHANGELOG.md`'s `[Unreleased]` became `[0.4.5] - 2026-11-07`. `01 §8` now states the
release, schema **15** and the single-user audience. `docs/release-notes/v0.4.5.md` is the new
release body (summary, `<!-- SCREENSHOTS -->` placeholder for the maintainer's images, What's new,
Breaking with the exact P6/P8 wire changes, Agents). `README.md` gained the 0.4.5 feature section
and lost the claims that had gone stale (schema 13, forward-only migrations, six screens, thirteen
tools). The archive sweep ran per `04 §7`: `specs/archive/{05,06,07,08}_*.v0.4.5.md` are
byte-identical copies (SHA-256 recorded in `07` QA D4), and the live logs keep only what is still
open.

**Gate (on the merged tree, before the docs and after the version bump; raw logs kept outside the
repository and summarised here).** UI first, in order: `npm ci` 0, `npm run lint` 0,
`npm test` **160 passed / 0 failed**, `npm run build` 0; `node scripts/stage-mcp.mjs` 0;
`cargo fmt --all -- --check` 0; `cargo clippy --workspace --all-targets -- -D warnings` 0;
`cargo build --workspace` 0; `cargo test --workspace` **86 result lines, 1616 passed, 0 failed,
16 ignored**; `git diff --exit-code -- ui/src/bindings` prints nothing.

**Static release checks.** No `ui/src` file mentions any of the 25 names the deleted
`tokens.compat.css` mapped (`--bg-base`, `--ink-muted`, `--glow-scan`, ...), and no remote font is
referenced from `ui/src/styles`. The kickoff's literal token grep cannot be empty because three of
its names (`--accent`, `--accent-wash`, `--font-mono`) are v2 semantic tokens defined in
`ui/src/styles/tokens.css`; `07` QA D3 records both raw results and why the intent-preserving check
is the one that means anything. The dev-only `/kitchen` route is absent from the production
`ui/dist` bundle.

**P4 harness.** Regenerated the fixture at the committed seed (content digest reproduced exactly),
served it with the real embedder and answer sidecar through junctions onto the installed models (no
download), and ran the 300-pair evaluation: `300/300` turns, zero failures, entity recovery **90.3%**
(PASS against 80.0%) and p95 **2306 ms** (PASS against 3485 ms), while retrieval recall@10 37.0%,
known-fact 65.7% and citation validity 71.4% land on the frozen baseline rather than their targets.
The fixture cannot exercise Ask v2 (`crates/evalfixture/src/serve.rs` serves the legacy
`ask_context`), so those rows measure the pre-P6 pipeline; `07` QA F1 records the finding with its
raw numbers and leaves its resolution (serving Ask v2 from the fixture, or the live-database run) to
the maintainer. Nothing was re-baselined to make a number move.
