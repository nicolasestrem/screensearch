"""Cycle 20 scratch measurement. Frame statistics and box luma.

Run: python .cycles/measure20.py <png> [<boxes.json>]

Prints the whole frame luma histogram, flat share and edge density (same
definitions as audit19.py), then the mean luma and bright-pixel share inside
every box the browser reported (road corridor, towers, market square), so the
"brightest cluster" claim is a number and not a memory of the frame.
"""
import json
import sys

import numpy as np
from PIL import Image


def load(path):
    return np.asarray(Image.open(path).convert("RGB")).astype(np.float32) / 255.0


def luma(a):
    return 0.2126 * a[..., 0] + 0.7152 * a[..., 1] + 0.0722 * a[..., 2]


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
    print("  %-14s %4dx%-4d luma %.3f  flat %5.1f  edge %5.2f  p50 %.3f  p99 %.3f  bright %.1f%%"
          % (name, a.shape[1], a.shape[0], float(l.mean()), flat_share(l), edge_density(l),
             float(np.percentile(l, 50)), float(np.percentile(l, 99)),
             float((l > 0.80).mean() * 100.0)))


def box(a, name, b, scale=1.0):
    h, w = a.shape[:2]
    x0 = max(0, int(round(b["x"] * scale)))
    y0 = max(0, int(round(b["y"] * scale)))
    x1 = min(w, int(round((b["x"] + max(1, b["w"])) * scale)))
    y1 = min(h, int(round((b["y"] + max(1, b["h"])) * scale)))
    crop = a[y0:y1, x0:x1]
    if crop.size == 0:
        print("  %-14s empty" % name)
        return {}
    l = luma(crop)
    stats = dict(
        name=name,
        x=x0, y=y0, w=x1 - x0, h=y1 - y0,
        luma=float(l.mean()),
        p50=float(np.percentile(l, 50)),
        p95=float(np.percentile(l, 95)),
        p99=float(np.percentile(l, 99)),
        bright=float((l > 0.80).mean() * 100.0),
        lit=float((l > 0.55).mean() * 100.0),
        flat=flat_share(l),
        edge=edge_density(l),
    )
    print("  %-14s [%4d,%4d %4dx%-4d] luma %.3f  p50 %.3f  p95 %.3f  p99 %.3f  >0.80 %5.1f%%  >0.55 %5.1f%%  flat %5.1f  edge %5.2f"
          % (name, stats["x"], stats["y"], stats["w"], stats["h"], stats["luma"], stats["p50"],
             stats["p95"], stats["p99"], stats["bright"], stats["lit"], stats["flat"], stats["edge"]))
    return stats


def main():
    path = sys.argv[1]
    a = load(path)
    print("%s  %dx%d" % (path, a.shape[1], a.shape[0]))
    region(a, "whole")
    h = a.shape[0]
    region(a[: max(1, h // 10)], "top tenth")
    region(a[max(0, h - h // 3):], "lower third")
    l = luma(a)
    hist, edges = np.histogram(l, bins=10, range=(0.0, 1.0))
    total = l.size
    print("  luma histogram (0.0 to 1.0, ten bins, share %):")
    print("    " + "  ".join("%5.1f" % (100.0 * c / total) for c in hist))
    print("    " + "  ".join("%5.2f" % edges[i] for i in range(len(edges) - 1)))
    if len(sys.argv) > 2:
        with open(sys.argv[2], "r", encoding="utf-8") as fh:
            boxes = json.load(fh)
        vp = boxes.get("viewport") or {}
        scale = 1.0
        if vp.get("W"):
            scale = a.shape[1] / float(vp["W"])
        print("  boxes (scale %.3f to the captured pixels):" % scale)
        for i, b in enumerate(boxes.get("towers", [])):
            box(a, "tower-%d" % i, b["box"], scale)
        if boxes.get("market"):
            box(a, "market", boxes["market"], scale)
        if boxes.get("corridor"):
            box(a, "corridor", boxes["corridor"], scale)
        if boxes.get("works"):
            box(a, "works", boxes["works"], scale)


if __name__ == "__main__":
    main()
