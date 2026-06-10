# Frontend Design System: Brutalist / Minimalist Paper

**Version:** 2.0 (Brutalist Overhaul)
**Theme:** "Brutalist / Minimalist Paper" (Light & Dark Variants)

## Core Philosophy
The UI follows a **Brutalist / Minimalist Paper** design system inspired by physical print media, editorial design, and clean interfaces, matching the design of `screensearch-website`:
- **Paper & Ink Palette:** Flat warm cream/paper backgrounds and deep charcoal ink text in light mode, with a dedicated dark paper variant in dark mode.
- **Zero Border-Radius:** Hard, sharp 90-degree edges on all elements (no rounded corners).
- **Rule Borders:** Fine, solid 1px borders (`border-rule`) separating panels, cards, inputs, and components.
- **Typographic Hierarchy:** Newspaper-style editorial headings using **Newsreader** (Serif) paired with clean geometric UI text using **Geist** (Sans-serif) and technical labels/ids in **Geist Mono** (Monospace).
- **Interactive Shift:** Elements react to interactions with physical offsets (e.g. `hover:-translate-y-0.5 active:translate-y-0`) rather than soft diffuse shadows or light glows.

---

## Global Atmosphere
Root styles defined in `index.css` and registered in `tailwind.config.js`.

### Color Tokens (HSL)
We use tailwind color tokens mapping to raw HSL values defined dynamically under light and dark themes:
- `bg-paper`: Primary paper background.
- `bg-paper-2`: Secondary background layer for elevated widgets/inputs.
- `text-ink`: Primary text color representing deep ink print.
- `text-ink-muted`: Secondary text color for secondary content or helper descriptions.
- `border-rule`: High-contrast fine border lines defining component boundaries.
- `border-accent`: Distinctive highlight border for active/focused elements.

#### Light Mode Colors
- `--color-paper`: `40 23% 97%` (warm soft cream/paper)
- `--color-paper-2`: `40 18% 94%` (slightly darker warm paper for components)
- `--color-ink`: `240 6% 10%` (deep ink black)
- `--color-ink-muted`: `240 4% 45%` (muted grey ink)
- `--color-rule`: `240 6% 15%` (crisp, fine border rules)
- `--color-accent`: `210 100% 50%` (ink blue accent highlight)

#### Dark Mode (.dark) Colors
- `--color-paper`: `240 6% 9%` (rich dark background)
- `--color-paper-2`: `240 6% 13%` (elevated dark paper component layer)
- `--color-ink`: `40 20% 95%` (warm light ink text)
- `--color-ink-muted`: `40 10% 65%` (muted warm text)
- `--color-rule`: `40 20% 25%` (subtle border rules)
- `--color-accent`: `210 100% 65%` (bright blue highlight)

---

## Utility Classes & Containers
The old glassmorphism panels have been completely replaced with paper-based components:
- `bg-paper` / `bg-paper-2` for flat panels.
- `border-rule` / `border-accent` for crisp borders.
- `rounded-none` to guarantee sharp edges.

**Example:**
```tsx
<div className="bg-paper border border-rule p-6 rounded-none">
  Content
</div>
```

---

## Typography
- **Headings & Accents:** Serif (**Newsreader**) for a premium, newspaper editorial feel.
- **Primary UI Text:** Sans-serif (**Geist**) for readability.
- **Technical & Stats:** Monospaced (**Geist Mono**) for timestamp data, logs, and parameters.

---

## Components

### 1. Sidebar (`Sidebar.tsx`)
- Sharp layout with zero border-radius.
- Highlight border left edge indicates the active navigation state.
- Sections headers styled using uppercase `font-mono` tracking wide.

### 2. Search Modal (`SearchInvite.tsx`)
- Detached search panel adopting the flat paper layout.
- Search inputs use an italicized serif font for a premium look.

### 3. GlassCard (`GlassCard.tsx`)
- Fully redesigned to serve as the unified component container. It drops glassmorphism shadows and backdrop filters, replacing them with flat `bg-paper` background overlays, `rounded-none` corners, and solid `border-rule` boundaries.
