"""Cycle 26 air review: the top fifth split into the four bands the reviewer names.

Scratch tool, not part of the deliverable. Definitions copied from the reviewer's table:

  warm   hue 10..60, saturation >= 0.25, value >= 0.45 (the top25.py definition)
  blue   hue 190..260, saturation > 0.08
  luma   0.2126 R + 0.7152 G + 0.0722 B, mean of the band
  sat    HSV saturation, mean of the band
  edge   audit19.py edge density inside the band

Targets for screenshots/iter-26-day.png, rows 0..180:
  0-45     blue >= 55, luma 0.66..0.74, sat 0.10..0.18
  45-90    blue 25..50, warm rising through the band
  90-135   warm 20..45, no streaks
  135-180  warm 60..70, edge <= 1.2
  whole    warm 60..70 and blue >= 5 by the audit19.py definitions

Run: python .cycles/rows26.py screenshots/iter-26-day.png [more.png ...]
"""
import sys

import numpy as np
from PIL import Image

BANDS = ((0, 45), (45, 90), (90, 135), (135, 180))


def load(p):
    return np.asarray(Image.open(p).convert("RGB")).astype(np.float32) / 255.0


def hsv(a):
    mx = a.max(-1)
    mn = a.min(-1)
    d = mx - mn
    s = np.where(mx > 1e-6, d / np.maximum(mx, 1e-6), 0.0)
    h = np.zeros_like(mx)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    m = d > 1e-6
    i = m & (mx == r)
    h[i] = ((g[i] - b[i]) / d[i]) % 6.0
    i = m & (mx == g)
    h[i] = ((b[i] - r[i]) / d[i]) + 2.0
    i = m & (mx == b)
    h[i] = ((r[i] - g[i]) / d[i]) + 4.0
    return h * 60.0, s, mx


def edge_density(g):
    gx = np.abs(np.diff(g, axis=1))
    gy = np.abs(np.diff(g, axis=0))
    m = np.zeros_like(g, dtype=bool)
    m[:-1, :-1] = (gx[:-1, :] + gy[:, :-1]) > 0.08
    return float(m.mean() * 100.0)


def report(path):
    a = load(path)
    h, s, v = hsv(a)
    l = 0.2126 * a[..., 0] + 0.7152 * a[..., 1] + 0.0722 * a[..., 2]
    print(f"{path}  {a.shape[1]}x{a.shape[0]}")
    print("  rows       warm%   blue%    sat    luma    edge   rgb")
    for r0, r1 in BANDS:
        sl = slice(r0, r1)
        warm = ((h[sl] >= 10) & (h[sl] <= 60) & (s[sl] >= 0.25) & (v[sl] >= 0.45)).mean() * 100.0
        blue = ((h[sl] >= 190) & (h[sl] < 260) & (s[sl] > 0.08)).mean() * 100.0
        band = l[sl]
        print(f"  {r0:>3}-{r1:<4} {warm:7.1f} {blue:7.1f} {s[sl].mean():6.3f} {band.mean():6.3f}"
              f" {edge_density(band):6.2f}   {a[sl][..., 0].mean():.2f},{a[sl][..., 1].mean():.2f},"
              f"{a[sl][..., 2].mean():.2f}")
    # audit19.py definitions over the whole frame
    warm_a = ((h >= 20) & (h <= 70) & (s > 0.12)).mean() * 100.0
    blue_a = ((h >= 190) & (h < 260) & (s > 0.12)).mean() * 100.0
    clip = float((a.max(-1) >= 0.999).mean() * 100.0)
    print(f"  whole  warm {warm_a:5.1f}  blue {blue_a:5.1f}  luma {l.mean():.3f}  clip {clip:.3f}")


for p in sys.argv[1:]:
    report(p)
