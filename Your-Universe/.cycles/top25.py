"""Cycle 25+, the top of the frame. Scratch tool, not part of the deliverable.

The owner judges the top 20 percent of the 1600x900 frame: hills that read as clean soft
silhouettes in warm haze, a clean sky gradient, and no streaked tree rows or purple/brown mud.

Run: python .cycles/top25.py screenshots/iter-26-day.png [more.png ...]

Per image it prints, for rows 0..180:
  luma/sat/edge   the same metrics audit19.py uses, restricted to the crop
  warm purple     sunlit warm and violet share of the crop
  haze            share of pixels with saturation under 0.22 and luma over 0.55 (soft air)
  streaks         share of pixels whose horizontal gradient is strong while the vertical one is
                  weak: a horizontal one-pixel line, which is what a distant tree row becomes
  mud             share of pixels that are low saturation and in the hue 20..60 warm-brown band
                  (the "dirty band") plus any violet, which must stay at zero
  rows            edge density in six 30-row slices, so a streak band names its own height
"""
import sys

import numpy as np
from PIL import Image


def load(path):
    a = np.asarray(Image.open(path).convert("RGB")).astype(np.float32) / 255.0
    return a


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


def edges(l):
    gx = np.abs(np.diff(l, axis=1))[:-1, :]
    gy = np.abs(np.diff(l, axis=0))[:, :-1]
    return gx, gy


def report(path):
    a = load(path)
    h, w, _ = a.shape
    cut = int(round(h * 0.20))
    crop = a[:cut]
    l = crop.mean(-1)
    hue, sat, val = hsv(crop)
    gx, gy = edges(l)
    edge_l = np.hypot(gx, gy)
    warm = ((hue >= 10) & (hue <= 60) & (sat >= 0.25) & (val >= 0.45)).mean()
    purple = ((hue >= 250) & (hue <= 320) & (sat >= 0.15)).mean()
    haze = ((sat <= 0.22) & (val >= 0.55)).mean()
    streaks = ((gx >= 0.045) & (gy <= 0.02)).mean()
    mud = ((sat <= 0.30) & (hue >= 20) & (hue <= 60) & (val <= 0.72)).mean()
    print(f"{path}  {w}x{h}  crop rows 0..{cut}")
    print(f"  luma {l.mean():.3f}  sat {sat.mean():.3f}  edge {edge_l.mean() * 100:.2f}  "
          f"warm {warm * 100:.1f}  purple {purple * 100:.2f}  haze {haze * 100:.1f}")
    print(f"  streaks {streaks * 100:.2f}%  mud {mud * 100:.2f}%")
    slices = 6
    step = cut // slices
    parts = []
    for i in range(slices):
        band = l[i * step:(i + 1) * step]
        bgx, bgy = edges(band)
        parts.append(f"{i * step:>3}:{np.hypot(bgx, bgy).mean() * 100:5.2f}")
    print("  rows edge  " + "  ".join(parts))


for p in sys.argv[1:]:
    report(p)
