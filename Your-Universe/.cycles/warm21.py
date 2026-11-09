"""Cycle 21 worker B: warm share in sunlit versus shadowed regions, and clipping.

Regions are keyed on encoded luminance inside the city band of the frame (rows
15..92 percent of the height, which excludes the sky band and the vignette
corner at the very bottom):

    sunlit  mask: luma >= 0.42   (lit walls, roofs, sunlit ground)
    shadow  mask: luma <= 0.26   (cast shadow and shaded ground)

Per mask the tool reports the audit19 warm share (hue 20..70, sat > 0.12), the
cool share (hue 190..260, sat > 0.12) and the mean hue of the warm pixels, so
directionality (a warm key against a cooler fill) is measurable separately in
the two light regimes instead of only in the whole frame.

Clipping is reported on the whole frame: the share of pixels whose any channel
sits at or above 0.999 encoded, the share of pure-white pixels (all three
channels), and the same two counts inside a 1 px inset so the frame border is
not counted.

Run: python .cycles/warm21.py <png> [<png>...]
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
    return hue * 60.0, sat


def shares(a, mask):
    h, s = hue_sat(a)
    sel = mask & (s > 0.12)
    warm = (h >= 20) & (h <= 70) & sel
    cool = (h >= 190) & (h < 260) & sel
    n = int(mask.sum())
    warm_share = 100.0 * int(warm.sum()) / max(1, n)
    cool_share = 100.0 * int(cool.sum()) / max(1, n)
    warm_hue = float(np.median(h[warm])) if warm.any() else 0.0
    return n, warm_share, warm_hue, cool_share


def audit(path):
    a = load(path)
    h, w = a.shape[:2]
    l = 0.2126 * a[..., 0] + 0.7152 * a[..., 1] + 0.0722 * a[..., 2]
    band = np.zeros_like(l, dtype=bool)
    band[int(0.15 * h):int(0.92 * h), :] = True
    sunlit = band & (l >= 0.42)
    shadow = band & (l <= 0.26)
    ns, ws, hs, cs = shares(a, sunlit)
    nd, wd, hd, cd = shares(a, shadow)
    print("%s  %dx%d" % (path, w, h))
    print("  sunlit  %6d px  warm %5.1f  (median hue %5.1f)  cool %5.1f" % (ns, ws, hs, cs))
    print("  shadow  %6d px  warm %5.1f  (median hue %5.1f)  cool %5.1f" % (nd, wd, hd, cd))
    mx = a.max(-1)
    any_clip = float((mx >= 0.999).mean() * 100.0)
    white_clip = float(((a.min(-1) >= 0.999)).mean() * 100.0)
    inset = np.zeros_like(mx, dtype=bool)
    inset[1:-1, 1:-1] = True
    any_clip_i = float((mx >= 0.999)[inset].mean() * 100.0)
    white_clip_i = float((a.min(-1) >= 0.999)[inset].mean() * 100.0)
    print("  clipping  any-channel %5.3f  white %5.3f  (1px inset: any %5.3f  white %5.3f)"
          % (any_clip, white_clip, any_clip_i, white_clip_i))


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(__doc__)
        raise SystemExit(2)
    for p in sys.argv[1:]:
        audit(p)
