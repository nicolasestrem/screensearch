"""Cycle 21 worker B: tilt-shift blur profile at ten rows.

Compares a frame captured with the post chain against a no-post reference of the
same state. The cleanest reference pair comes from ONE page load with the
#post=0/#post=1 hash toggle (see main.mjs): both screenshots then share one sim
clock, so only the chain differs.

Per tenth of the frame height the tool reports edge density (audit19
definition) and high-frequency energy (mean absolute deviation from the 3x3
box mean) of both frames, and the attenuation the chain applies:

    blur share of band i = 1 - metric_post[i] / metric_nopost[i]

The two frames differ globally (the composite grade, bloom and vignette shift
tone everywhere), so the metric must not read that as blur: pixels where the
difference image has a sharp structural gradient (moving traffic, flickering
windows) are masked out and refilled with each frame's own mask-aware box mean,
which leaves smooth tone differences in the measurement and only content
changes out.

Run: python .cycles/blur21.py <post.png> <nopost.png>
"""
import sys

import numpy as np
from PIL import Image


def load_rgb(path):
    return np.asarray(Image.open(path).convert("RGB")).astype(np.float32) / 255.0


def luma(a):
    return 0.2126 * a[..., 0] + 0.7152 * a[..., 1] + 0.0722 * a[..., 2]


def edge_density(g):
    gx = np.abs(np.diff(g, axis=1))
    gy = np.abs(np.diff(g, axis=0))
    m = np.zeros(g.shape, dtype=bool)
    m[:-1, :-1] = (gx[:-1, :] + gy[:, :-1]) > 0.08
    return float(m.mean() * 100.0)


def box3(g):
    """3x3 box mean with edge clamping (uniform_filter would need scipy)."""
    p = np.pad(g, 1, mode="edge")
    out = np.zeros_like(g)
    for dy in range(3):
        for dx in range(3):
            out += p[dy:dy + g.shape[0], dx:dx + g.shape[1]]
    return out / 9.0


def hf_energy(g):
    """Mean absolute deviation from the 3x3 box mean: the frame's high-frequency
    energy. Sensitive to a 1..2 px gaussian even when a hard edge threshold is not."""
    return float(np.abs(g - box3(g)).mean() * 1000.0)


def box_fill(g, mask, k=15):
    """Fill masked pixels with a mask-aware box mean of the unmasked ones."""
    keep = ~mask
    weight = boxn(keep.astype(np.float32), k)
    filled = boxn((g * keep).astype(np.float32), k)
    out = np.where(weight > 1e-4, filled / np.maximum(weight, 1e-4), g)
    return np.where(mask, out, g)


def boxn(g, k):
    r = k // 2
    p = np.pad(g, r, mode="constant")
    out = np.zeros_like(g)
    for dy in range(k):
        for dx in range(k):
            out += p[dy:dy + g.shape[0], dx:dx + g.shape[1]]
    return out


def content_mask(ra, rb):
    """Structural differences between the two frames: the difference image's own
    gradient, thresholded. Smooth tone shifts (grade, vignette, bloom halos)
    have gradients under 2/255 per pixel; moved traffic and relit windows pass
    16/255. The mask is dilated 5 px so no boundary step is measured."""
    diff = np.abs(ra - rb).max(-1)
    gy, gx = np.gradient(diff)
    mask = (np.abs(gx) + np.abs(gy)) > 16.0 / 255.0
    mask = boxn(mask.astype(np.float32), 11) > 0.0
    return mask


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        raise SystemExit(2)
    post, nopost = sys.argv[1], sys.argv[2]
    ra, rb = load_rgb(post), load_rgb(nopost)
    if ra.shape != rb.shape:
        print("size mismatch %s vs %s" % (post, nopost))
        raise SystemExit(2)
    mask = content_mask(ra, rb)
    gp, gn = luma(ra), luma(rb)
    hp_full = box_fill(gp, mask)
    hn_full = box_fill(gn, mask)
    h = gp.shape[0]
    print("%s vs %s  %dx%d  content-masked share %.3f" % (post, nopost, gp.shape[1], h, mask.mean()))
    print("  band  y-range          edge_post  edge_nopost  blur_share   hf_post  hf_nopost  hf_blur_share")
    for i in range(10):
        s0, s1 = i * h // 10, (i + 1) * h // 10
        ep, en = edge_density(hp_full[s0:s1]), edge_density(hn_full[s0:s1])
        share = 1.0 - ep / en if en > 1e-6 else 0.0
        hp, hn = hf_energy(hp_full[s0:s1]), hf_energy(hn_full[s0:s1])
        hshare = 1.0 - hp / hn if hn > 1e-6 else 0.0
        print("   %2d   %4d..%-4d     %6.2f      %6.2f      %5.3f      %7.2f   %7.2f      %5.3f"
              % (i, s0, s1, ep, en, share, hp, hn, hshare))


if __name__ == "__main__":
    main()
