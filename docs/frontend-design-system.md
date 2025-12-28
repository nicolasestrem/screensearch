# Frontend Design System: Sci-Fi Concept

**Version:** 1.0 (Visual Overhaul)
**Theme:** "Dark Future" / "Cyberpunk Lite"

## Core Philosophy
The UI follows a **Sci-Fi / Cinematic** aesthetic characterized by:
- **Atmospheric Depth:** Deep radial gradients and ambient "light blobs" (`.bg-blob`) to create 3D space.
- **Glassmorphism:** Highly saturated, blurred surfaces that feel like physical glass (`backdrop-filter`).
- **Neon Accents:** High-intensity cyan (`#00f2ff`) for active states, mimicking light emission.
- **Precision:** Monospace typography (`font-mono`) for technical data to evoke a "system hud" feel.

---

## Global Atmosphere
Root styles defined in `index.css`.

### Background
Replaces flat colors with a deep void gradient:
```css
background: radial-gradient(circle at 50% 0%, #1a2333 0%, #020617 100%);
```

### Ambient Lighting
Injects colorful blurs behind content:
- **Primary Blob:** Top-left cyan/blue.
- **Secondary Blob:** Bottom-right violet/purple.
- **Grid Pattern:** Subtle vector grid overlay for technical texture.

---

## Utility Classes

### 1. Glass Surfaces
Use for containers, cards, and panels.

| Class | Description |
|-------|-------------|
| `.glass-panel` | Base surface. High blur (20px), low opacity white tint (2%). |
| `.glass-card` | Interactive surface. Adds hover glow and lift. |
| `.glass-panel-cyan` | **High Attention**. Used for the Search Bubble. Cyan-tinted glass. |

**Example:**
```tsx
<div className="glass-panel p-6 rounded-2xl">
  Content
</div>
```

### 2. High-Intensity Accents
Use sparingly for "active" or "powered" items.

| Class | Description |
|-------|-------------|
| `.neon-text` | Cyan text with outer glow. `text-cyan-400 drop-shadow`. |
| `.active-border` | "Beam" border effect. 1px cyan border + inset/outset shadows. |
| `.glow-cyan-lg` | Large outer atmosphere glow. |

**Example:**
```tsx
<button className={isActive ? "active-border neon-text" : "text-muted"}>
  Dashboard
</button>
```

### 3. Typography
- **Primary Font:** Sans-serif (Inter) for UI labels and readability.
- **Technical Font:** Monospaced (JetBrains Mono/Fira Code) for data points, IDs, and timestamps.

**Usage:** Add `font-mono` to technical data.
```tsx
<span className="text-xs text-muted-foreground font-mono">
  {latency_ms}ms
</span>
```

---

## Components

### Search Bubble (`SearchInvite.tsx`)
A floating, detached modal that mimics a "thought bubble" or HUD element.
- **Animation:** Spring physics (mass: 0.8, stiffness: 300).
- **Style:** `glass-panel-cyan` with aggressive backdrop blur.
- **Focus:** Input field scales slightly (`scale: 1.01`) on focus.

### Navigation Sidebar (`Sidebar.tsx`)
- **Active State:** Uses `.active-border` and a CSS-based "glimmer" animation (`animate-[shimmer_2s_infinite]`).
- **Icons:** Glow when active (`drop-shadow`).

### Dashboard Cards (`ProductivityPulse`, `MemoryStatus`)
- **Header:** Icon + Title.
- **Content:** Monospace data displays.
- **Charts:** Uses SVG gradients matching the cyan theme (`#00f2ff` to `#00ff88`).
