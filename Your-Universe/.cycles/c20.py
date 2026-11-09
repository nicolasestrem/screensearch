"""Cycle 20 region metrics: luma / flat share / edge density per named rectangle.

Not part of the deliverable. The district frame is measured at 2000x1125, so the
rectangles below are in that space; pass `--scale` for other file sizes.

Run: python .cycles/c20.py regions <png> [--scale 1.25] [--name x0,y0,x1,y1 ...]
"""
import sys

import numpy as np
from PIL import Image

REGIONS = {
    # The three margins the owner pointed at, where a big block used to stand alone in a
    # field: the left flank, the right flank and the strip below the ring's south arc.
    'left': (0, 360, 470, 1010),
    'right': (1530, 360, 2000, 1010),
    'bottom': (470, 880, 1530, 1125),
    # The hillside above the ring's north-west arc, where the new arterial climbs out.
    'hillside': (0, 250, 620, 520),
    # The ring interior, as the control that should not move much.
    'core': (700, 480, 1300, 820),
    # The union of the forty-two pack-model lots' projected bounding boxes at 1600x900, times
    # 1.25 for the 2000x1125 capture: where the model facades stand in the district frame.
    'modellots': (776, 390, 1214, 711),
}


def load(path):
    return np.asarray(Image.open(path).convert('RGB')).astype(np.float32) / 255.0


def luma(a):
    return 0.2126 * a[..., 0] + 0.7152 * a[..., 1] + 0.0722 * a[..., 2]


def warm_share(a):
    """Share of pixels in a lit warm band, the same gate audit19 uses: hue 20..70 degrees
    with real saturation, which is what a lit window and a lit wall both read as."""
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
    hue *= 60.0
    mask = (hue >= 20) & (hue <= 70) & (sat > 0.25)
    return float(mask.mean()) * 100.0


def flat_share(g, win=9, thr=0.012):
    h, w = g.shape
    pad = win // 2
    p = np.pad(g, pad, mode='edge')
    mx = np.full((h, w), -1.0, dtype=np.float32)
    for dy in range(win):
        for dx in range(win):
            np.maximum(mx, p[dy:dy + h, dx:dx + w], out=mx)
    lo = np.full((h, w), 2.0, dtype=np.float32)
    for dy in range(win):
        for dx in range(win):
            np.minimum(lo, p[dy:dy + h, dx:dx + w], out=lo)
    return float(((mx - lo) < thr).mean()) * 100.0


def edge_density(g):
    gx = np.abs(np.diff(g, axis=1))
    gy = np.abs(np.diff(g, axis=0))
    return float(((gx[:-1, :] > 0.08).mean() + (gy[:, :-1] > 0.08).mean()) * 0.5) * 100.0


def main():
    args = sys.argv[1:]
    if not args or args[0] != 'regions':
        print(__doc__)
        return 1
    path = args[1]
    scale = 1.0
    custom = []
    i = 2
    while i < len(args):
        if args[i] == '--scale':
            scale = float(args[i + 1])
            i += 2
        elif args[i] == '--name':
            parts = args[i + 1].split(',')
            custom.append((parts[0], tuple(int(v) for v in parts[1:])))
            i += 2
        else:
            i += 1
    img = load(path)
    h, w = img.shape[:2]
    print('%s  %dx%d' % (path, w, h))
    boxes = custom if custom else [(k, v) for k, v in REGIONS.items()]
    for name, (x0, y0, x1, y1) in boxes:
        X0 = max(0, int(round(x0 * scale)))
        Y0 = max(0, int(round(y0 * scale)))
        X1 = min(w, int(round(x1 * scale)))
        Y1 = min(h, int(round(y1 * scale)))
        sub = img[Y0:Y1, X0:X1]
        g = luma(sub)
        print('  %-10s %4dx%-4d luma %.3f  flat %5.1f  edge %5.2f  warm %5.1f'
              % (name, X1 - X0, Y1 - Y0, float(g.mean()), flat_share(g), edge_density(g),
                 warm_share(sub)))
    return 0


if __name__ == '__main__':
    sys.exit(main())
