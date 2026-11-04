# P2 direction pick: D or D2

**What they are.** Both are *Logbook*: Signal Console's (C) structure, kept exactly. Same five
screens, same nav, pane and context rail on every screen, same logbook chronology, density,
metrics band, retrieval ledger, citations and 78-cell state gallery. Only the skin changed, and D2
is D byte for byte outside the token block and the skin labels.

- **D, ink skin** (`direction-d.html`): cool slate neutrals (hue 252), one ink-blue accent.
  Success, warning and danger stay green, amber and red, as states only.
- **D2, pine skin** (`direction-d2.html`): near-neutral graphite (hue 160, barely tinted), one
  pine-green accent. Success folds into pine on a neutral wash, so red and amber are the only
  other colours on screen.

Open `compare.html` to see C, D and D2 side by side, or open each file full size; both themes are
complete and the toggle is at the top right.

**How they differ from C.** Same structure and content, a different skin:

| | C, Signal Console | D and D2 |
|---|---|---|
| Accents | cyan for action, violet for relationships, glow | one accent; relationships are neutral surfaces plus a link icon |
| Mono | labels, keys, setting names, buttons, counts | only `.num` (timestamps, counts, durations, sizes) and the numeric field |
| Smallest text | 10px (634 text elements under 12px) | 12px, 0 elements under 12px |
| Labels | tracked uppercase micro-labels | sentence case, weight and tone for hierarchy |
| Ornament | kicker, numbered sections, skeleton sweep, striped thumbnails, halo dots | none |
| Default theme | dark | light (dark is a full peer) |

The critique round also fixed behaviour C shared: result and frame rows are real links, hit
targets reach 32px, the Ask screen shows a finished state, counts agree across pane and rail, the
danger zone has one trigger, and the match highlight is the accent rather than a yellow fourth hue.

**Detect counts** (`impeccable detect`, the pinned 0.1.5 binary named in the brief):

| File | Before | After |
|---|---|---|
| C, Signal Console (baseline) | 323 findings + 6 advisory | (not changed) |
| D, ink | 2 on the first pass (cramped padding) | **0** |
| D2, pine | 0 on the first pass (made from the fixed D) | **0** |

The skill's newer 0.1.18 engine reports 306 on C and 2 on each of D and D2. Both are on the
gallery's disabled Empty-state specimen (opacity 0.55, WCAG 2.2 SC 1.4.3 exempt), and each carries
a justifying HTML comment. Every enabled text pair passes AA in both themes. The worst are D 4.83:1
light and 5.39:1 dark, D2 4.79:1 light and 5.39:1 dark. Raw output is in
`specs/05_BUILD_REVIEW.md`.

**The one decision.** Ink or pine: **D** (cool slate and ink blue, four hues counting the states)
or **D2** (near-neutral graphite and pine green, three hues because success is the accent). The
structure, type and every behaviour are identical, so the pick is only the skin, and it becomes
`specs/UI_REFERENCE.md` §1 for both themes.

The trade-off to weigh: D keeps success visually separate from the accent; D2 is calmer and more
its own, but "Capturing", "Saved" and "completed" share the accent's ink (the critique's main
concern, softened by giving success a neutral wash).
