"""Art-direction metrics for the Localtime frames. Not part of the deliverable.

Every number here is computed from a rendered PNG, so a cycle can claim a visual
change only after the number moves (or after a named region is shown to be the
cause). Run: python .cycles/art.py shots...
"""
import sys
import numpy as np
from PIL import Image


def load(path):
    return np.asarray(Image.open(path).convert('RGB')).astype(np.float32) / 255.0


def hue_sat(a):
    mx = a.max(-1)
    mn = a.min(-1)
    d = mx - mn
    sat = np.where(mx > 1e-6, d / np.maximum(mx, 1e-6), 0.0)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    hue = np.zeros_like(mx)
    m = d > 1e-6
    i = (mx == r) & m
    hue[i] = ((g[i] - b[i]) / d[i]) % 6
    i = (mx == g) & m
    hue[i] = ((b[i] - r[i]) / d[i]) + 2
    i = (mx == b) & m
    hue[i] = ((r[i] - g[i]) / d[i]) + 4
    return hue * 60.0, sat, mx


def luma(a):
    return 0.2126 * a[..., 0] + 0.7152 * a[..., 1] + 0.0722 * a[..., 2]


def edge_density(g):
    """Share of pixels whose 4-neighbour Sobel-ish gradient crosses 0.08."""
    gx = np.abs(np.diff(g, axis=1))
    gy = np.abs(np.diff(g, axis=0))
    m = np.zeros_like(g, dtype=bool)
    m[:-1, :-1] = (gx[:-1, :] + gy[:, :-1]) > 0.08
    return m.mean() * 100.0


def flat_share(g, win=9, thr=0.012):
    """Share of pixels in locally flat areas: a proxy for empty sky and dead ground."""
    k = win
    pad = k // 2
    gp = np.pad(g, pad, mode='edge')
    acc = np.zeros_like(g)
    for dy in range(0, k):
        for dx in range(0, k):
            acc += np.abs(gp[dy:dy + g.shape[0], dx:dx + g.shape[1]] - g)
    return (acc / (k * k) < thr).mean() * 100.0


def report(path):
    a = load(path)
    h, w = a.shape[:2]
    hue, sat, v = hue_sat(a)
    L = luma(a)
    warm = ((hue < 45) | (hue > 340)) & (sat > 0.15)
    cool = (hue >= 165) & (hue <= 260) & (sat > 0.15)
    green = (hue >= 70) & (hue <= 165) & (sat > 0.15)
    dark = L < 0.18
    blown = (a > 0.96).all(-1)
    brightwarm = warm & (L > 0.5)
    bands = np.array_split(np.arange(h), 10)
    out = {
        'file': path,
        'size': (w, h),
        'warm%': round(warm.mean() * 100, 1),
        'cool%': round(cool.mean() * 100, 1),
        'green%': round(green.mean() * 100, 1),
        'dark%': round(dark.mean() * 100, 1),
        'blown%': round(blown.mean() * 100, 2),
        'brightwarm%': round(brightwarm.mean() * 100, 2),
        'meanL': round(float(L.mean()), 3),
        'flat%': round(flat_share(L), 1),
        'edge%': round(edge_density(L), 2),
        'bandEdge%': [round(edge_density(L[b]), 1) for b in bands],
        'bandFlat%': [round(flat_share(L[b]), 1) for b in bands],
        'bandWarm%': [round(warm[b].mean() * 100, 1) for b in bands],
        'bandL': [round(float(L[b].mean()), 3) for b in bands],
    }
    return out


if __name__ == '__main__':
    for p in sys.argv[1:]:
        r = report(p)
        print(f"== {r['file']} {r['size']}")
        print(f"   warm {r['warm%']}  cool {r['cool%']}  green {r['green%']}  dark {r['dark%']}"
              f"  blown {r['blown%']}  brightwarm {r['brightwarm%']}  meanL {r['meanL']}")
        print(f"   edge {r['edge%']}  flat {r['flat%']}")
        print(f"   edge/tenth  {r['bandEdge%']}")
        print(f"   flat/tenth  {r['bandFlat%']}")
        print(f"   warm/tenth  {r['bandWarm%']}")
        print(f"   L/tenth     {r['bandL']}")
