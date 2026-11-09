"""Cycle 22 corner diagnosis. Scratch tool, not part of the deliverable.

Crops a thirds-region of a 1600x900 street capture into subcells, reports flat/edge per cell
plus dominant colours, and maps sample pixels back to approximate world ground coordinates
using the camera rig's own constants (FOV 45, pitch 40 deg, street zoom 0.38 of district).

Usage: python .cycles/corner22.py cells <png> <tl|tr|bl|br> [cols rows]
       python .cycles/corner22.py world <tl|tr|bl|br|all> [r0..r3]
"""
import sys
import math
import numpy as np
from PIL import Image

W, H = 1600, 900
FOV = math.radians(45.0)
ASPECT = W / H
PITCH = math.radians(40.0)
TANH = math.tan(FOV * 0.5)
CENTER = (0.0, 12.0, 4.0)
RADIUS = 114.0 * 1.05 / math.sin(FOV * 0.5)
DIST = RADIUS * 0.38        # street tier zoom 0.38
SPAN, HILL, Z_LOW, Z_TOP = 120.0, 26.0, 112.0, -116.0


def smooth(a, b, x):
    t = min(1.0, max(0.0, (x - a) / (b - a)))
    return t * t * (3 - 2 * t)


def district_height(x, z):
    t_raw = (Z_LOW - z) / (Z_LOW - Z_TOP)
    t = min(1.0, max(0.0, t_raw))
    s = t * 3.0
    i = min(2, int(math.floor(s)))
    terr = (i + smooth(0.3, 0.72, s - i)) / 3.0
    y = HILL * (0.3 * t + 0.7 * terr)
    if t_raw > 1:
        y += min(16.0, (t_raw - 1) * 14.0)
    elif t_raw < 0:
        y += max(-5.0, t_raw * 7.0)
    y += math.sin(x * 0.031 + 0.4) * 0.9 + math.cos(z * 0.027 + x * 0.012) * 0.6
    y += 7.0 * smooth(128, 240, abs(x))
    return y


def ground(x, z):
    return district_height(x, z)


def cam(yaw_step):
    yaw = yaw_step * math.pi * 0.5
    horiz = DIST * math.cos(PITCH)
    vert = DIST * math.sin(PITCH)
    return np.array([CENTER[0] + math.sin(yaw) * horiz,
                     CENTER[1] + vert,
                     CENTER[2] + math.cos(yaw) * horiz])


def basis(yaw_step):
    pos = cam(yaw_step)
    tgt = np.array(CENTER)
    f = tgt - pos
    f = f / np.linalg.norm(f)
    up = np.array([0.0, 1.0, 0.0])
    r = np.cross(f, up)
    r = r / np.linalg.norm(r)
    u = np.cross(r, f)
    return pos, f, r, u


def ray(yaw_step, px, py):
    pos, f, r, u = basis(yaw_step)
    x = (2.0 * px / W - 1.0) * TANH * ASPECT
    y = (1.0 - 2.0 * py / H) * TANH
    d = r * x + u * y + f
    return pos, d / np.linalg.norm(d)


def hit(yaw_step, px, py):
    pos, d = ray(yaw_step, px, py)
    t = 1.0
    prev = pos[1] - ground(pos[0], pos[2])
    while t < 3000.0:
        step = 0.5 if t < 60 else 2.0 if t < 400 else 8.0
        t += step
        p = pos + d * t
        cur = p[1] - ground(p[0], p[2])
        if prev > 0 and cur <= 0:
            # bisect
            lo, hi = t - step, t
            for _ in range(24):
                mid = 0.5 * (lo + hi)
                pm = pos + d * mid
                if pm[1] - ground(pm[0], pm[2]) > 0:
                    lo = mid
                else:
                    hi = mid
            pm = pos + d * 0.5 * (lo + hi)
            return pm[0], ground(pm[0], pm[2]), pm[2], 0.5 * (lo + hi)
        if cur <= 0 and prev <= 0:
            return p[0], ground(p[0], p[2]), p[2], t
        prev = cur
    return None


def load(path):
    return np.asarray(Image.open(path).convert("RGB")).astype(np.float32) / 255.0


def luma(a):
    return 0.2126 * a[..., 0] + 0.7152 * a[..., 1] + 0.0722 * a[..., 2]


def flat_share(g, win=9, thr=0.012):
    k = win
    pad = k // 2
    gp = np.pad(g, pad, mode="edge")
    acc = np.zeros_like(g)
    for dy in range(0, k):
        for dx in range(0, k):
            acc += np.abs(gp[dy:dy + g.shape[0], dx:dx + g.shape[1]] - g)
    return float((acc / (k * k) < thr).mean() * 100.0)


def edge_density(g):
    gx = np.abs(np.diff(g, axis=1))
    gy = np.abs(np.diff(g, axis=0))
    m = np.zeros_like(g, dtype=bool)
    m[:-1, :-1] = (gx[:-1, :] + gy[:, :-1]) > 0.08
    return float(m.mean() * 100.0)


def hue_sat(a):
    mx = a.max(-1)
    mn = a.min(-1)
    d = mx - mn
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    hue = np.zeros_like(mx)
    m = d > 1e-6
    rm = m & (mx == r)
    gm = m & (mx == g) & ~rm
    bm = m & (mx == b) & ~rm & ~gm
    with np.errstate(invalid='ignore', divide='ignore'):
        hue[rm] = ((g[rm] - b[rm]) / d[rm]) % 6
        hue[gm] = ((b[gm] - r[gm]) / d[gm]) + 2
        hue[bm] = ((r[bm] - g[bm]) / d[bm]) + 4
    return hue * 60.0, np.where(mx > 1e-6, d / np.maximum(mx, 1e-6), 0.0), mx


REGIONS = {
    'tl': (0, 0), 'tr': (0, 2 * (W // 3)), 'bl': (2 * (H // 3), 0), 'br': (2 * (H // 3), 2 * (W // 3)),
}


def warm_share(a):
    hh, ss, vv = hue_sat(a)
    return float((((hh >= 20) & (hh <= 70) & (ss > 0.12)).mean()) * 100.0)


def blue_share(a):
    hh, ss, vv = hue_sat(a)
    return float((((hh >= 185) & (hh <= 265) & (ss > 0.10)).mean()) * 100.0)


def cells(path, name, cols=4, rows=3):
    a = load(path)
    sy, sx = REGIONS[name]
    ch, cw = H // 3, W // 3
    sub = a[sy:sy + ch, sx:sx + cw]
    print("%s %s (%dx%d)" % (path, name, cw, ch))
    print("      " + "".join("   cell %d,%-3d      " % (c, r) for r in range(rows) for c in range(0, 1)))
    for r in range(rows):
        line = []
        for c in range(cols):
            cell = sub[r * ch // rows:(r + 1) * ch // rows, c * cw // cols:(c + 1) * cw // cols]
            l = luma(cell)
            line.append("flat %5.1f edge %5.2f luma %.3f warm %4.1f blue %4.1f" % (
                flat_share(l), edge_density(l), float(l.mean()), warm_share(cell), blue_share(cell)))
        print(" row %d" % r)
        for c, s in enumerate(line):
            print("   c%d %s" % (c, s))
    # dominant colours whole corner
    q = np.clip((sub * 255).astype(np.int32) // 24, 0, 10)
    flat = q[..., 0] * 121 + q[..., 1] * 11 + q[..., 2]
    ids, counts = np.unique(flat, return_counts=True)
    order = np.argsort(counts)[::-1][:8]
    print(" dominant (24-step buckets):")
    for o in order:
        idx = ids[o]
        rr = (idx // 121) * 24 + 12
        gg = ((idx // 11) % 11) * 24 + 12
        bb = (idx % 11) * 24 + 12
        print("   %5.1f%%  rgb(%3d,%3d,%3d)" % (100.0 * counts[o] / flat.size, rr, gg, bb))


def worldmap(name, ys):
    sy, sx = REGIONS.get(name, (0, 0))
    ch, cw = H // 3 if name in REGIONS else H, W // 3 if name in REGIONS else W
    print("world map %s%s" % (name, ys))
    for r in range(3):
        for c in range(4):
            px = sx + int((c + 0.5) * cw / 4)
            py = sy + int((r + 0.5) * ch / 3)
            h = hit(ys, px, py)
            if h is None:
                print("  cell r%d c%d px(%4d,%3d): no ground hit" % (r, c, px, py))
            else:
                print("  cell r%d c%d px(%4d,%3d): world x %7.1f z %7.1f  groundY %6.1f  dist %6.1f" % (
                    r, c, px, py, h[0], h[2], h[1], h[3]))


def flatmap(path, nx=20, ny=11):
    a = load(path)
    g = luma(a)
    k = 9
    pad = k // 2
    gp = np.pad(g, pad, mode="edge")
    acc = np.zeros_like(g)
    for dy in range(0, k):
        for dx in range(0, k):
            acc += np.abs(gp[dy:dy + g.shape[0], dx:dx + g.shape[1]] - g)
    flat = acc / (k * k) < 0.012
    h, w = g.shape
    print("%s flat share %% per cell (%dx%d), . = under 20, X = over 80" % (path, nx, ny))
    hdr = "     " + "".join("%4d" % int((c + 0.5) * w / nx) for c in range(nx))
    print(hdr)
    for r in range(ny):
        row = []
        for c in range(nx):
            cell = flat[r * h // ny:(r + 1) * h // ny, c * w // nx:(c + 1) * w // nx]
            v = float(cell.mean() * 100)
            row.append("   ." if v < 20 else "   X" if v > 80 else "%4.0f" % v)
        print("%4d " % int((r + 0.5) * h / ny) + "".join(row))


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "cells":
        cells(sys.argv[2], sys.argv[3])
    elif cmd == "map":
        flatmap(sys.argv[2])
    elif cmd == "world":
        yaw = int(sys.argv[3]) if len(sys.argv) > 3 else 0
        worldmap(sys.argv[2], yaw)
    else:
        print(__doc__)
