"""Sky and haze ladder of the top of the frame, four 45-row bands.

    python .cycles/sky26.py screenshots/iter-26-day.png [more.png ...]

Rows are absolute (0 is the top). The bands are the ones the owner's item 2 is judged on:
  0-45    the zenith: pale blue overhead, never a blown white plate
  45-90   the transition: blue giving way to warm cream
  90-135  warm haze over the far hills
  135-180 the hills themselves and the village skyline

Targets, from the target picture and the owner's words:
  zenith blue >= 55 percent, luma 0.66..0.74
  transition blue 25..50 percent
  haze warm 20..45 percent, no streaks
  hills warm 60..70 percent, edge <= 1.2
"""
import sys

import numpy as np
from PIL import Image


def load(path):
    return np.asarray(Image.open(path).convert("RGB")).astype(np.float32) / 255.0


def hsv(a):
    mx = a.max(-1)
    mn = a.min(-1)
    d = mx - mn
    s = np.where(mx > 0, d / np.maximum(mx, 1e-6), 0.0)
    h = np.zeros_like(mx)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    m = d > 1e-6
    idx = m & (mx == r)
    h[idx] = ((g[idx] - b[idx]) / d[idx]) % 6.0
    idx = m & (mx == g)
    h[idx] = ((b[idx] - r[idx]) / d[idx]) + 2.0
    idx = m & (mx == b)
    h[idx] = ((r[idx] - g[idx]) / d[idx]) + 4.0
    return h * 60.0, s, mx


for path in sys.argv[1:]:
    a = load(path)
    print(f"{path}  {a.shape[1]}x{a.shape[0]}")
    for lo, hi in [(0, 45), (45, 90), (90, 135), (135, 180)]:
        c = a[lo:hi]
        hue, sat, val = hsv(c)
        l = c.mean(-1)
        gx = np.abs(np.diff(l, axis=1))[:-1, :]
        gy = np.abs(np.diff(l, axis=0))[:, :-1]
        edge = np.hypot(gx, gy).mean() * 100
        warm = ((hue >= 10) & (hue <= 60) & (sat >= 0.25) & (val >= 0.45)).mean() * 100
        blue = ((hue >= 180) & (hue <= 260) & (sat >= 0.08)).mean() * 100
        streaks = ((gx >= 0.045) & (gy <= 0.02)).mean() * 100
        print(f"  rows {lo:>3}-{hi:<3} luma {l.mean():.3f} sat {sat.mean():.3f} edge {edge:5.2f} "
              f"warm {warm:5.1f} blue {blue:5.1f} streaks {streaks:4.2f}")
