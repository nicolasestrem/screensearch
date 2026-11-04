# P2 design system v2 captures

Evidence for the P2 pull request. Every image is rendered from the dev build against **synthetic
content**: the `/kitchen` gallery's invented samples, or a route's own error state. No captured
history, screenshot or OCR text from a real machine appears here, and `docs/SCREENSHOTS.md` still
governs the README images (which need the seeded DB and the Tauri app).

Why a plain browser: `/kitchen` renders the primitives and tokens without IPC, so it is captured
from `vite` dev at 1440x900. The unconverted routes (`/`, `/recall`, `/timeline`, `/settings`) need
Tauri IPC, so in a plain browser they show their real *error* states instead of populated ones. Those
four captures are therefore shim evidence only: proof that every v1 token resolves and nothing renders
unstyled or transparent on the v2 layer. Populated screenshots of those routes stay with the refont
phase (P10), per `docs/SCREENSHOTS.md`.

| File | What it shows |
|---|---|
| `kitchen-<theme>-samples.png` | `/kitchen?theme=<theme>`, the five product samples (Settings, search, session, Ask, gallery index) |
| `kitchen-<theme>-gallery.png` | the primitive gallery, every primitive in every state |
| `shim-deck.png`, `shim-recall.png`, `shim-timeline.png`, `shim-settings.png` | the four unconverted routes through `tokens.compat.css`, on `pine-light` |

`<theme>` is one of `pine-light` (default), `pine-dark`, `ink-light`, `ink-dark`. Regenerate by
running `npm run dev` in `ui/` and capturing the same URLs at 1440x900.
