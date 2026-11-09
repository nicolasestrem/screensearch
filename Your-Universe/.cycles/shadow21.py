"""Cycle 21 worker B: shadow penumbra and shadow length measurement.

Two modes, both on the golden-hour district frame (deterministic camera, the
hillside region east of the avenue is traffic-free so the scene content is
stable between the before and after captures; only the grade may shift tone,
so every threshold is relative to the plateaus of the ramp being measured).

penumbra mode
    python .cycles/shadow21.py penumbra <png> <y0> <y1> <x0> <x1>
    For each row in y0..y1 the luma along x0..x1 runs shadow -> lit. The row's
    penumbra width is the distance between the first crossing of
    shadow + 10% of the ramp and the first crossing of shadow + 90%, in px.
    The tool prints every row and the median. Chosen region: the right edge of
    the round tree's cast shadow at (1440..1526, 505..520), which borders open
    lit ground with no other caster between.

length mode
    python .cycles/shadow21.py length <png> <y> <x0> <x1>
    Prints the luma profile along one row of the cypress shadow corridor
    (base at x~1470, y~731; the shadow streak runs lower-left to x~1398) so the
    shadow tip can be read off; the length/caster-height ratio is computed
    outside from the same-row profile of both captures.

Run: python .cycles/shadow21.py <mode> <png> <args...>
"""
import sys

import numpy as np
from PIL import Image


def luma(path):
    a = np.asarray(Image.open(path).convert("RGB")).astype(np.float32) / 255.0
    return 0.2126 * a[..., 0] + 0.7152 * a[..., 1] + 0.0722 * a[..., 2]


def penumbra(g, path, y0, y1, x0, x1):
    widths = []
    print("%s penumbra rows %d..%d x %d..%d" % (path, y0, y1, x0, x1))
    for y in range(y0, y1 + 1):
        row = g[y, x0:x1 + 1]
        shadow = float(np.median(row[:6]))
        lit = float(np.median(row[-6:]))
        span = lit - shadow
        if span <= 0.02:
            continue
        w10 = w90 = None
        for i, v in enumerate(row):
            if w10 is None and v >= shadow + 0.10 * span:
                w10 = i
            if v >= shadow + 0.90 * span:
                w90 = i
        if w10 is None or w90 is None or w90 < w10:
            continue
        widths.append(w90 - w10)
        print("  y=%d  shadow %.3f lit %.3f  width %d px" % (y, shadow, lit, w90 - w10))
    if widths:
        print("  median width %d px (n=%d)" % (int(np.median(widths)), len(widths)))


def profile(g, path, y, x0, x1):
    row = g[y, x0:x1 + 1]
    print("%s length row y=%d x %d..%d" % (path, y, x0, x1))
    print("  " + " ".join("%d" % round(v * 255) for v in row))


def edge(g, path, y0, y1, x0, x1):
    """Auto-located penumbra: per row, the widest 10-90% luma transition inside the
    window between the window's own plateaus, so slope bands with weak contrast are
    skipped and the reported width is the actual terminator of that row's edge."""
    widths = []
    print("%s edge rows %d..%d x %d..%d" % (path, y0, y1, x0, x1))
    for y in range(y0, y1 + 1):
        row = g[y, x0:x1 + 1]
        a = float(np.median(row[:8]))
        b = float(np.median(row[-8:]))
        span = abs(b - a)
        if span <= 0.06:
            continue
        lo, hi = (a, b) if b > a else (b, a)
        t10 = lo + 0.10 * span
        t90 = lo + 0.90 * span
        x10 = x90 = None
        for i, v in enumerate(row):
            if x10 is None and v >= t10 and v <= t90:
                x10 = i
            elif x10 is not None and (v >= t90 or v <= t10):
                x90 = i
                break
        if x10 is None or x90 is None or x90 <= x10:
            continue
        widths.append(x90 - x10)
        print("  y=%d  %.3f -> %.3f  width %d px" % (y, lo, hi, x90 - x10))
    if widths:
        print("  median %d px (n=%d)" % (int(np.median(widths)), len(widths)))


if __name__ == "__main__":
    if len(sys.argv) < 4:
        print(__doc__)
        raise SystemExit(2)
    mode, path = sys.argv[1], sys.argv[2]
    g = luma(path)
    if mode == "penumbra":
        penumbra(g, path, *[int(v) for v in sys.argv[3:7]])
    elif mode == "length":
        profile(g, path, *[int(v) for v in sys.argv[3:6]])
    elif mode == "edge":
        edge(g, path, *[int(v) for v in sys.argv[3:7]])
    else:
        print(__doc__)
        raise SystemExit(2)
