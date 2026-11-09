"""Cycle 22 light worker: sunlit / shadow warm shares and clipping, the cycle 21 definition.

The cycle 21 numbers this script has to reproduce (73.8 sunlit warm, 4.5 sunlit cool,
35.4 shadow warm, 9.9 shadow cool, any-channel clipping 1.438 percent) come from
.cycles/warm21.py, so the definition here is copied from it verbatim:

  frame band      rows 15..92 percent of the height (excludes the sky top and the vignette
                  corner at the bottom)
  sunlit mask     luma >= 0.42 in the band   (encoded luma, 0.2126 R + 0.7152 G + 0.0722 B)
  shadow mask     luma <= 0.26 in the band
  hue w           degrees, HSV wheel, in 0..360
  saturation      (max - min) / max
  warm            hue in 20..70 DEGREES INCLUSIVE and saturation > 0.12, counted over the
                  mask pixels (so every share is a percentage OF THE MASK, not of the frame)
  cool            hue in 190..260 and saturation > 0.12, same denominator
  median hue      the median of the warm pixels' hue inside the mask
  clipping        any-channel: max(R,G,B) >= 0.999 encoded, whole frame and inside a 1 px
                  inset; white: min(R,G,B) >= 0.999, same two regions

The extra lines below the warm21 block are diagnostics only (which channel clips, where the
clipping and the low-saturation share sit in the frame); they never change the headline share.

Run: python .cycles/warm22.py <png> [<png>...]
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

    # ---- diagnostics below this line: guide the lamp, never the headline ----
    hh, ss = hue_sat(a)
    clip = mx >= 0.999
    red = float((a[..., 0] >= 0.999).mean() * 100.0)
    green = float((a[..., 1] >= 0.999).mean() * 100.0)
    blue = float((a[..., 2] >= 0.999).mean() * 100.0)
    print("  channels  R %5.3f  G %5.3f  B %5.3f" % (red, green, blue))
    if clip.any():
        ys = np.nonzero(clip)[0]
        rows = [float(np.percentile(ys, q) * 100.0 / h) for q in (5, 50, 95)]
        print("  clip rows p5/p50/p95 %4.1f/%4.1f/%4.1f%% of height  mean rgb %s"
              % (rows[0], rows[1], rows[2], np.round(a[clip].mean(0), 3)))
    low = band & (ss <= 0.12)
    print("  band low-sat %5.1f%% of band  (mean rgb %s)"
          % (100.0 * low.sum() / max(1, band.sum()), np.round(a[low].mean(0), 3)))
    coolm = band & (hh >= 190) & (hh < 260) & (ss > 0.12)
    print("  band cool    %5.1f%% of band  (mean rgb %s)"
          % (100.0 * coolm.sum() / max(1, band.sum()), np.round(a[coolm].mean(0), 3)))


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(__doc__)
        raise SystemExit(2)
    for p in sys.argv[1:]:
        audit(p)
