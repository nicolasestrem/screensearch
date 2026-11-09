"""Cycle 27 probe: is a structure standing in a street, or dropped in a field?

    python .cycles/isolate27.py screenshots/iter-27-day.png screenshots/iter-27-boxes.json

The JSON is written by `.cycles/float25.js` `structureScreen(8731)`: one record per published
structure with its projected screen position and an approximate box size in pixels.

Per structure the script measures the apron around the box (the ring between 1.0x and 2.2x the
box half-size, excluding the box footprint itself):

  asphalt   desaturated mid-luma pixels: a carriageway or a car park
  pavement  desaturated light pixels: kerb and walk
  grass     hue 40..110 with real saturation: dry ground, the thing a floating box stands on
  buildings the rest of the frame: neighbours, which is what a block looks like

A structure whose apron is mostly grass is floating. A structure with asphalt or pavement on at
least one side is in a block.
"""
import json
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


def apron_share(img, cx, cy, hw, hh):
    H, W, _ = img.shape
    x0 = max(0, int(cx - hw * 2.2))
    x1 = min(W, int(cx + hw * 2.2))
    y0 = max(0, int(cy - hh * 2.2))
    y1 = min(H, int(cy + hh * 2.2))
    if x1 - x0 < 8 or y1 - y0 < 8:
        return None
    patch = img[y0:y1, x0:x1]
    ph, pw, _ = patch.shape
    yy, xx = np.mgrid[0:ph, 0:pw]
    dx = (xx - pw / 2) / max(1.0, hw)
    dy = (yy - ph / 2) / max(1.0, hh)
    outer = (dx * dx + dy * dy) >= 1.0
    if outer.sum() < 64:
        return None
    px = patch[outer]
    hue, sat, val = hsv(px.reshape(-1, 1, 3))
    hue = hue.ravel()
    sat = sat.ravel()
    val = val.ravel()
    asphalt = ((sat < 0.18) & (val >= 0.18) & (val <= 0.55)).mean()
    # The original window. A loftier luma-based window was tried in cycle 27 and reverted: it
    # scored the pre-change build higher than the changed one on five of seven plots, so it was
    # not isolating kerbs. At golden hour a warm stone footway sits at sat 0.25 to 0.55 and this
    # window therefore misses it; the kerb evidence for cycle 27 is the footway's measured luma
    # (2.5 to 3 times the tarmac beside it), not this share.
    pavement = ((sat < 0.22) & (val > 0.55) & (val <= 0.82)).mean()
    grass = ((hue >= 40) & (hue <= 110) & (sat >= 0.25)).mean()
    return {"asphalt": round(float(asphalt) * 100, 1), "pavement": round(float(pavement) * 100, 1),
            "grass": round(float(grass) * 100, 1)}


def main():
    png = sys.argv[1]
    meta = json.load(open(sys.argv[2], encoding="utf-8"))
    img = load(png)
    print(f"{png}  {img.shape[1]}x{img.shape[0]}  {len(meta)} structures")
    floating = 0
    for rec in meta:
        shares = apron_share(img, rec["x"], rec["y"], rec["hw"], rec["hh"])
        if shares is None:
            print(f"  offscreen  {rec['type']:<10} {rec['via']:<8} ({rec['wx']}, {rec['wz']})")
            continue
        street = max(shares["asphalt"], shares["pavement"])
        flag = "FLOATING" if shares["grass"] >= 40 and street < 8 else "block"
        if flag == "FLOATING":
            floating += 1
        print(f"  {flag:<8}  {rec['type']:<10} {rec['via']:<8} ({rec['wx']:>7}, {rec['wz']:>7})  "
              f"asphalt {shares['asphalt']:>5}%  pavement {shares['pavement']:>5}%  grass {shares['grass']:>5}%")
    print(f"  floating: {floating} of {len(meta)}")


main()
