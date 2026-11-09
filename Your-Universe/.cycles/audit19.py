"""Cycle 19 frame audit. Scratch tool, not part of the deliverable.

Two jobs:
  judge <png> [<png>]   per region colour and structure metrics, plus the
                        dominant quantised colours, so a defect is described by
                        numbers rather than by memory of what the frame looked like.
  flicker <a.png> <b.png>
                        frame difference between two captures of the SAME camera
                        state. Any pixel that moves is a z-fight, a shimmering
                        decal or an unstable instance, because nothing in the
                        district should change between two identical frames of a
                        paused scene.
  corners <png>         the four corner crops and the centre at the street tier,
                        each measured on its own, because the brief says the
                        corners are where a frame dies.

Run: python .cycles/audit19.py judge shots/x.png
"""
import sys

import numpy as np
from PIL import Image


def load(path):
    return np.asarray(Image.open(path).convert("RGB")).astype(np.float32) / 255.0


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


def grad(g):
    dx = np.zeros_like(g)
    dy = np.zeros_like(g)
    dx[:, 1:-1] = g[:, 2:] - g[:, :-2]
    dy[1:-1, :] = g[2:, :] - g[:-2, :]
    return np.abs(dx) + np.abs(dy)


def edge_density(g):
    gx = np.abs(np.diff(g, axis=1))
    gy = np.abs(np.diff(g, axis=0))
    m = np.zeros_like(g, dtype=bool)
    m[:-1, :-1] = (gx[:-1, :] + gy[:, :-1]) > 0.08
    return float(m.mean() * 100.0)


def flat_share(g, win=9, thr=0.012):
    k = win
    pad = k // 2
    gp = np.pad(g, pad, mode="edge")
    acc = np.zeros_like(g)
    for dy in range(0, k):
        for dx in range(0, k):
            acc += np.abs(gp[dy:dy + g.shape[0], dx:dx + g.shape[1]] - g)
    return float((acc / (k * k) < thr).mean() * 100.0)


def region(a, name):
    l = luma(a)
    h, s, v = hue_sat(a)
    sat_mask = s > 0.12
    warm = float(((h >= 20) & (h <= 70) & sat_mask).mean() * 100.0)
    purple = float(((h >= 260) & (h <= 330) & sat_mask).mean() * 100.0)
    blue = float(((h >= 190) & (h < 260) & sat_mask).mean() * 100.0)
    green = float(((h > 70) & (h < 160) & sat_mask).mean() * 100.0)
    print(
        "  %-12s %5dx%-5d luma %.3f  flat %5.1f  edge %5.2f  sat %.3f  warm %5.1f  purple %4.1f  blue %5.1f  green %5.1f"
        % (
            name,
            a.shape[1],
            a.shape[0],
            float(l.mean()),
            flat_share(l),
            edge_density(l),
            float(s.mean()),
            warm,
            purple,
            blue,
            green,
        )
    )
    return dict(luma=float(l.mean()), flat=flat_share(l), edge=edge_density(l), warm=warm, purple=purple)


def bands(a, n=10):
    h = a.shape[0]
    out = []
    for i in range(n):
        sl = a[i * h // n:(i + 1) * h // n]
        l = luma(sl)
        hh, ss, vv = hue_sat(sl)
        out.append(
            (
                i,
                float(l.mean()),
                float(ss.mean()),
                edge_density(l),
                flat_share(l),
            )
        )
    return out


def rgb_hue_sat(r, g, b):
    rf, gf, bf = r / 255.0, g / 255.0, b / 255.0
    mx = max(rf, gf, bf)
    mn = min(rf, gf, bf)
    d = mx - mn
    if d <= 1e-6:
        return 0.0, 0.0
    if mx == rf:
        hh = ((gf - bf) / d) % 6
    elif mx == gf:
        hh = ((bf - rf) / d) + 2
    else:
        hh = ((rf - gf) / d) + 4
    return hh * 60.0, d / mx


def dominant(a, k=10):
    q = np.clip((a * 255).astype(np.int32) // 16, 0, 15)
    keys = q[..., 0] * 256 + q[..., 1] * 16 + q[..., 2]
    counts = np.bincount(keys.ravel(), minlength=4096)
    order = np.argsort(counts)[::-1][:k]
    total = keys.size
    out = []
    for idx in order:
        if counts[idx] == 0:
            continue
        r = (idx // 256) * 16 + 8
        g = ((idx // 16) % 16) * 16 + 8
        b = (idx % 16) * 16 + 8
        hh, ss = rgb_hue_sat(r, g, b)
        out.append((int(counts[idx] * 10000 // total) / 100.0, r, g, b, hh, ss))
    return out


def judge(path):
    a = load(path)
    print("%s  %dx%d" % (path, a.shape[1], a.shape[0]))
    region(a, "whole")
    h = a.shape[0]
    w = a.shape[1]
    region(a[: max(1, h // 10)], "top tenth")
    region(a[max(0, h - h // 3):], "lower third")
    region(a[h // 3:h // 3 + h // 3, : w // 3], "left third")
    region(a[h // 3:h // 3 + h // 3, 2 * w // 3:], "right third")
    print("  bands (tenth, luma, sat, edge, flat):")
    for i, l, s, e, f in bands(a):
        print("    %2d  %.3f  %.3f  %5.2f  %5.1f" % (i, l, s, e, f))
    print("  dominant quantised colours (share %, rgb, hue, sat):")
    for share, r, g, b, hh, ss in dominant(a):
        print("    %5.2f  %3d %3d %3d  hue %6.1f  sat %.2f" % (share, r, g, b, hh, ss))


def flicker(a_path, b_path):
    a = load(a_path)
    b = load(b_path)
    if a.shape != b.shape:
        print("size mismatch %s vs %s" % (a.shape, b.shape))
        return
    d = np.abs(a - b).max(-1)
    total = d.size
    for thr in (0.02, 0.05, 0.12, 0.3):
        print("  pixels differing by more than %.2f: %d (%.4f percent)" % (thr, int((d > thr).sum()), 100.0 * (d > thr).sum() / total))
    mask = d > 0.12
    if mask.any():
        ys, xs = np.nonzero(mask)
        print("  bounding box of strong differences: x %d..%d  y %d..%d" % (xs.min(), xs.max(), ys.min(), ys.max()))
        h, w = d.shape
        grid = np.zeros((4, 4))
        for yy in range(4):
            for xx in range(4):
                grid[yy, xx] = mask[yy * h // 4:(yy + 1) * h // 4, xx * w // 4:(xx + 1) * w // 4].sum()
        print("  4x4 grid of strong-difference counts:")
        for row in grid:
            print("    " + "  ".join("%6d" % v for v in row))


def corners(path):
    a = load(path)
    h, w = a.shape[:2]
    ch = h // 3
    cw = w // 3
    print("%s corner and centre audit (%dx%d)" % (path, w, h))
    for name, sy, sx in (
        ("top-left", 0, 0),
        ("top-right", 0, 2 * cw),
        ("bottom-left", 2 * ch, 0),
        ("bottom-right", 2 * ch, 2 * cw),
        ("centre", ch, cw),
    ):
        region(a[sy:sy + ch, sx:sx + cw], name)


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print(__doc__)
        raise SystemExit(2)
    cmd = sys.argv[1]
    if cmd == "judge":
        for p in sys.argv[2:]:
            judge(p)
    elif cmd == "flicker":
        flicker(sys.argv[2], sys.argv[3])
    elif cmd == "corners":
        corners(sys.argv[2])
    else:
        print(__doc__)
        raise SystemExit(2)
