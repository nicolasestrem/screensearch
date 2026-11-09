"""Speck detector: the static signature of z-fighting, aliasing debris and floating decals.

A coplanar surface pair produces colour that jumps between two values from pixel to pixel along
the seam, so it reads as salt-and-pepper noise on an otherwise flat surface. This looks for
exactly that: a pixel that differs strongly from its four neighbours while those four neighbours
agree with each other, sitting inside a locally uniform patch (a 5x5 window whose pixels are
mostly one flat value). The uniformity test is what separates a real defect from a legitimately
busy texture: a one-pixel checkerboard has agreeing neighbours too, but its window is half
opposite colour, so it is rejected.

Reported counts are *isolated* specks, so two adjacent bad pixels read as zero: a defect that
dense is visible as a stripe in the statistics anyway.

Not part of the deliverable. Run: python .cycles/specks.py <png> [<png>]
"""
import sys

import numpy as np
from PIL import Image

WINDOW = 5
UNIFORM = 0.6


def load(path):
    return np.asarray(Image.open(path).convert("RGB")).astype(np.float32) / 255.0


def box_sum(mask, k):
    """Sum over a k x k window per channel, with edge padding."""
    a = mask.astype(np.float32)
    if a.ndim == 2:
        a = a[..., None]
    h, w = a.shape[0], a.shape[1]
    pad = k // 2
    a = np.pad(a, ((pad, pad), (pad, pad), (0, 0)), mode="edge")
    c = np.cumsum(np.cumsum(a, axis=0), axis=1)
    c = np.pad(c, ((1, 0), (1, 0), (0, 0)))
    out = c[k:k + h, k:k + w] - c[0:h, 0:w] - c[k:k + h, 0:w] + c[0:h, 0:w]
    return out[..., 0] if mask.ndim == 2 else out


def specks(path, thr=0.22, agree=0.05, grid=8):
    a = load(path)
    h, w = a.shape[:2]
    up = np.roll(a, 1, 0)
    down = np.roll(a, -1, 0)
    left = np.roll(a, 1, 1)
    right = np.roll(a, -1, 1)
    nb = [up, down, left, right]
    d_self = np.maximum.reduce([np.abs(a - n).max(-1) for n in nb])
    pairs = [
        np.abs(up - down).max(-1),
        np.abs(left - right).max(-1),
        np.abs(up - left).max(-1),
        np.abs(down - right).max(-1),
        np.abs(up - right).max(-1),
        np.abs(down - left).max(-1),
    ]
    agree_max = np.maximum.reduce(pairs)

    # Local uniformity: the share of the 5x5 window that sits within `agree` of the window mean.
    k = WINDOW
    local_mean = box_sum(a, k) / (k * k)
    close = (np.abs(a - local_mean).max(-1) < agree).astype(np.float32)
    uniform = box_sum(close, k) / (k * k)

    mask = (d_self > thr) & (agree_max < agree) & (uniform >= UNIFORM)
    mask[0, :] = False
    mask[-1, :] = False
    mask[:, 0] = False
    mask[:, -1] = False

    total = int(mask.sum())
    cells = np.zeros((grid, grid), dtype=np.int64)
    ys, xs = np.nonzero(mask)
    for y, x in zip(ys, xs):
        cells[min(grid - 1, y * grid // h), min(grid - 1, x * grid // w)] += 1
    print("%s  %dx%d  specks %d  (%.4f percent of pixels)" % (path, w, h, total, 100.0 * total / (w * h)))
    flat = [(int(cells[r, c]), r, c) for r in range(grid) for c in range(grid)]
    flat.sort(reverse=True)
    if flat[0][0] > 0:
        print("  hottest 8x8 cells (row, col, count):")
        for n, r, c in flat[:8]:
            if n == 0:
                break
            print("    row %d col %d : %5d   x %d..%d  y %d..%d"
                  % (r, c, n, c * w // grid, (c + 1) * w // grid, r * h // grid, (r + 1) * h // grid))
    return total


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(__doc__)
        raise SystemExit(2)
    for p in sys.argv[1:]:
        specks(p)
