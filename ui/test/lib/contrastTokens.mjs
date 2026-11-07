// Token-fence helpers shared by the contrast fences in `test/`: read a theme's
// `tokens.css`, resolve a semantic token through its `var()` chain into the --p-
// primitive layer, and compute WCAG 2.x contrast. Extracted so one implementation of the
// CSS parsing and the contrast math serves every fence (`contrast.test.mjs` for the
// design-system pairs, `answer-prose.test.mjs` for the markdown elements an answer
// renders), instead of two copies drifting apart.
//
// The four semantic blocks (:root[data-theme=...]) are the only colour source a component
// may see; each value is a var() chain into the --p- primitives, so a chain that ends
// anywhere but a hex literal is a hard failure, never a silently skipped pair.

/** Split a stylesheet into `selector -> body` blocks with comments removed. */
export function cssBlocks(css) {
  const stripped = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const blocks = [];
  for (const match of stripped.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    blocks.push({ selector: match[1].trim(), body: match[2] });
  }
  return blocks;
}

/** `--name: value;` declarations of one block, in source order. */
export function declarations(body) {
  const map = new Map();
  for (const match of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    map.set(match[1], match[2].trim());
  }
  return map;
}

const isThemeSelector = (selector) => /^:root\[data-theme="[^"]+"\]$/.test(selector);
const themeName = (selector) => selector.slice(selector.indexOf('"') + 1, selector.lastIndexOf('"'));

/**
 * Layer 1 (the --p- primitives) plus the theme-independent scales. Both live in
 * bare `:root` blocks; a block that also names a data-theme or data-density is
 * a layer-2 theme block and must never leak into this map, otherwise a token
 * missing from one theme would silently resolve through the default theme.
 */
export function primitiveLayer(css) {
  const merged = new Map();
  for (const { selector, body } of cssBlocks(css)) {
    const selectors = selector.split(",").map((part) => part.trim());
    const bare = selectors.includes(":root");
    const scoped = selectors.some((part) => part.includes("data-theme") || part.includes("data-density"));
    if (!bare || scoped) continue;
    for (const [name, value] of declarations(body)) merged.set(name, value);
  }
  return merged;
}

/** Layer 2, one map per theme. */
export function themeLayers(css) {
  const themes = new Map();
  for (const { selector, body } of cssBlocks(css)) {
    for (const part of selector.split(",").map((p) => p.trim())) {
      if (!isThemeSelector(part)) continue;
      const name = themeName(part);
      const existing = themes.get(name) ?? new Map();
      for (const [token, value] of declarations(body)) existing.set(token, value);
      themes.set(name, existing);
    }
  }
  return themes;
}

/**
 * Follow `var(--x)` links until a literal is reached. Resolution order is the
 * theme first, then the primitive layer, which is what the cascade does.
 * Returns `undefined` when the token is not declared in that theme at all.
 */
export function resolve(token, theme, primitives) {
  const seen = [];
  let name = token;
  let value = theme.get(name) ?? primitives.get(name);
  while (value !== undefined && value.startsWith("var(")) {
    const match = /^var\(\s*(--[\w-]+)\s*\)$/.exec(value);
    if (!match) throw new Error(`unsupported var() form for ${name}: ${value}`);
    if (seen.includes(name)) throw new Error(`var() cycle: ${[...seen, name, match[1]].join(" -> ")}`);
    seen.push(name);
    name = match[1];
    value = theme.get(name) ?? primitives.get(name);
  }
  if (value === undefined) return undefined;
  return { token: name, value, chain: [...seen, name] };
}

/** Parse `#rgb` / `#rrggbb` (alpha forms keep their colour channels). */
export function rgb(value) {
  const match = /^#([0-9a-fA-F]{3,8})$/.exec(value);
  if (!match) throw new Error(`not a hex colour literal: ${value}`);
  let hex = match[1];
  if (hex.length === 4) hex = hex.slice(0, 3); // #rgba -> #rgb
  if (hex.length === 8) hex = hex.slice(0, 6); // #rrggbbaa -> #rrggbb
  if (hex.length !== 3 && hex.length !== 6) throw new Error(`not a hex colour literal: ${value}`);
  if (hex.length === 3) hex = hex.replace(/./g, (char) => char + char);
  return [0, 2, 4].map((offset) => parseInt(hex.slice(offset, offset + 2), 16));
}

export function relativeLuminance([r, g, b]) {
  const channel = (value) => {
    const scaled = value / 255;
    return scaled <= 0.03928 ? scaled / 12.92 : ((scaled + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** WCAG 2.x contrast ratio between two `#rrggbb` literals. */
export function contrast(foreground, background) {
  const a = relativeLuminance(rgb(foreground));
  const b = relativeLuminance(rgb(background));
  const lighter = Math.max(a, b);
  const darker = Math.min(a, b);
  return (lighter + 0.05) / (darker + 0.05);
}
