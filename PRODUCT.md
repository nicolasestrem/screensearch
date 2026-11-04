# Product

<!-- impeccable:product-schema 1 -->

Source: the maintainer's brief of 2026-11-04 (local-first Windows screen-memory app, single user,
Operate mode) plus repository specs. Facts marked "(specs)" were read from `specs/01_PROJECT_CONTEXT.md`
and `specs/UI_REFERENCE.md` rather than asked in an interview; the specs stay the source of truth.

## Platform

web

## Users

Exactly one user: the maintainer, who builds ScreenSearch and runs it every day on his own Windows
machine. Design for him, not for "most people". There is no onboarding audience, no second persona
and no multi-user or account model (specs: 0.4.5 D1).

His jobs, in order of frequency: find a moment he saw on screen (by text or by meaning), reconstruct
what he worked on and when (sessions, work threads, the timeline), ask a question and get an answer
grounded in his own history with sources he can open, and tune capture, privacy and models.

## Product Purpose

ScreenSearch continuously captures the screen, extracts its text (WinRT OCR, UI Automation as a
text source), embeds it, and makes the history searchable by text and by meaning. It answers
questions about what was seen, citing the captured moments the answer comes from. Everything runs
on the device (specs).

Success: he finds the moment or the work thread he is after in seconds, and he can trust an answer
because every claim points back to evidence he can open.

## Positioning

Fully on-device memory of one person's screen: capture, OCR, embeddings and inference (a local,
supervised llama.cpp sidecar) never leave the machine, with no cloud upload and no default
telemetry. Capture is cheap and enrichment is deferred to on-demand, timer or idle work, never
real-time vision (specs).

## Operating Context

- A Windows 10/11 desktop app: Tauri 2 shell, React UI rendered in WebView2. Windows-only by design
  (specs).
- Every app surface is **Operate mode**: scanability, consistency and the real usage scene outrank
  expression. The tool should disappear into the task.
- The usage scene: at his desk, next to his development tools, dipping in and out through the
  working day and into the evening; light and dark both matter.
- Shell (specs, `UI_REFERENCE` §3 and §8): a fixed StatusRail (telemetry, one row) and a fixed
  NavRail with six entries (Deck, Ask, Work, Recall, Insights, Settings); Session and Moment are
  detail routes; one scrolling content pane per route.
- Settings v2 has eight categories, autosave per field (specs).

## Capabilities and Constraints

- Capabilities (specs): screen capture with privacy exclusions, OCR and UIA text, hybrid full-text
  and semantic search, sessions and work threads, Ask with inline citations, insights, scheduled
  local memory reports, an opt-in authenticated local HTTP API and a stdio MCP wrapper.
- Network (specs): local-only. Exceptions are model downloads, the signed update check, and the
  user-enabled HTTP API. Fonts and icons ship as bundled local files; nothing is fetched at runtime.
- Every view defines all five states: loading, empty, error, partial, populated. No mock data in the
  shipped app, no "Coming soon" (specs).
- Undecided: the v2 visual identity. It is picked by the maintainer at the 0.4.5 P2 direction
  checkpoint and recorded in `specs/UI_REFERENCE.md` §1, not here.

## Brand Commitments

- Name: ScreenSearch. Website: screensearch.app. License: MIT (specs).
- Voice (`UI_REFERENCE` §9): plain, from the user's side, sentence case, active voice. Name things by
  what the user controls ("Pause capture", not "halt pipeline"). Errors say what happened and how to
  fix it, without apologising.
- Writing rule from the maintainer: never use the em dash character in project material.

## Evidence on Hand

- Specs: `specs/01` (context), `specs/02` (strategy), `specs/03` (production spec),
  `specs/UI_REFERENCE.md` (UI contract).
- Design checkpoint material: `docs/kickoffs/P2-directions/`.
- Mockup content is synthetic and illustrative. Never use real captured history, screenshots or
  OCR text in design artifacts.

## Product Principles

1. Local and private by construction: nothing leaves the machine unless he turned it on.
2. Truthful over impressive: honest states, bounded coverage, answers that show their sources.
3. Built for one expert's daily use: density, speed and keyboard reach over hand-holding.
4. Calm: the interface recedes so the history he is looking for stays in front.

## Accessibility & Inclusion

WCAG 2.x AA contrast for all text and controls in every shipped theme, a visible focus indicator on
every interactive element, full keyboard operation, `prefers-reduced-motion` honoured, hit targets
of at least 32px (specs, `UI_REFERENCE` §7).
