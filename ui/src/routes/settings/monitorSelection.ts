// Monitor selection for `capture.monitors` (`03 §8`: an empty selection means every
// monitor). Pure so the collapse rule can be tested without a DOM.
//
// The trap this guards (P7 review, PR #161): an empty selection is the "all"
// sentinel, so a selection that happens to cover every connected display is stored
// as `[]`. Comparing only the LENGTHS is wrong — a saved selection can include a
// display that is no longer connected, and "same number of entries" then does not
// mean "every connected display is captured". Example: connected `[0, 1]`, saved
// `[2]`; clicking display 0 yields `[0, 2]`, which has length 2 and would collapse
// to `[]` — turning capture ON for display 1, the opposite of the click.

/**
 * The next selection after toggling one display. An empty selection means "all", so
 * clicking a display then excludes just that one; the selection collapses back to
 * "all" only when it covers exactly the connected displays and nothing else.
 *
 * Removing the last captured display is refused: `[]` is the "all" sentinel
 * (`crates/capture/src/wgc.rs`), so an empty list here would not stop capture, it would
 * start capturing the displays the user had just excluded. Callers surface the refusal
 * by disabling that control (`canToggleOff`).
 */
export function toggleMonitor(
  selected: readonly number[],
  index: number,
  available: readonly number[],
): number[] {
  const next =
    selected.length === 0
      ? available.filter((candidate) => candidate !== index)
      : selected.includes(index)
        ? selected.filter((candidate) => candidate !== index)
        : [...selected, index].sort((a, b) => a - b);
  if (next.length === 0 && selected.length > 0) return [...selected];
  return coversExactly(next, available) ? [] : next;
}

/**
 * Remove one display from the selection — the same rule for a display that is no longer
 * connected, which is removed by its own control: emptying the list is refused, and a
 * selection that ends up covering exactly the connected displays collapses to "all".
 */
export function removeMonitor(
  selected: readonly number[],
  index: number,
  available: readonly number[],
): number[] {
  if (!selected.includes(index)) return [...selected];
  const next = selected.filter((candidate) => candidate !== index);
  if (next.length === 0) return [...selected];
  return coversExactly(next, available) ? [] : next;
}

/** False for the last captured display: deselecting it would mean "capture all". */
export function canToggleOff(
  selected: readonly number[],
  index: number,
  available: readonly number[],
): boolean {
  if (selected.length === 0) return available.length > 1;
  if (!selected.includes(index)) return true;
  return selected.filter((candidate) => candidate !== index).length > 0;
}

/**
 * True when the selection names every connected display and no other display —
 * the only case where `[]` ("all monitors") says the same thing.
 */
export function coversExactly(
  selected: readonly number[],
  available: readonly number[],
): boolean {
  if (available.length === 0) return false;
  if (selected.length !== available.length) return false;
  return available.every((index) => selected.includes(index));
}
