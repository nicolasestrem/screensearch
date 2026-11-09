"""Cycle 28 repair audit. Scratch tool, not part of the deliverable.

One table for the four street rotations and the day frame, on the exact files named, with the
SHA-256 of each file printed beside the numbers so a stale capture cannot be measured as if it
were fresh.

Run: python .cycles/fx28aud.py screenshots/iter-28-street-r0.png ... [--day a.png ...]
"""
import hashlib
import sys

import numpy as np

sys.path.insert(0, ".cycles")
import audit19  # noqa: E402  (scratch tool, same directory)


def corners(path):
    a = audit19.load(path)
    h, w = a.shape[:2]
    ch, cw = h // 3, w // 3
    out = {}
    for name, sy, sx in (("TL", 0, 0), ("TR", 0, 2 * cw), ("BL", 2 * ch, 0), ("BR", 2 * ch, 2 * cw)):
        sub = a[sy:sy + ch, sx:sx + cw]
        g = audit19.luma(sub)
        out[name] = (round(audit19.flat_share(g), 1), round(audit19.edge_density(g), 2))
    return out


def top20(path):
    a = audit19.load(path)
    crop = a[: int(round(a.shape[0] * 0.20))]
    l = crop.mean(-1)
    hue, sat, val = audit19.hue_sat(crop)
    gx = np.abs(np.diff(l, axis=1))[:-1, :]
    gy = np.abs(np.diff(l, axis=0))[:, :-1]
    streaks = ((gx >= 0.045) & (gy <= 0.02)).mean() * 100.0
    mud = ((sat <= 0.30) & (hue >= 20) & (hue <= 60) & (val <= 0.72)).mean() * 100.0
    return round(streaks, 2), round(mud, 2)


def sha(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def main(argv):
    street, day = [], []
    mode = street
    for a in argv:
        if a == "--day":
            mode = day
            continue
        mode.append(a)
    print("file                                                     sha256[:16]   bytes  TL flat/edge    TR flat/edge    BL flat/edge    BR flat/edge   maxFlat minEdge")
    worst_flat = 0.0
    worst_edge = 9.9
    for p in street:
        c = corners(p)
        mf = max(v[0] for v in c.values())
        me = min(v[1] for v in c.values())
        worst_flat = max(worst_flat, mf)
        worst_edge = min(worst_edge, me)
        print("%-56s %s %8d  %5.1f/%5.2f  %5.1f/%5.2f  %5.1f/%5.2f  %5.1f/%5.2f   %5.1f  %5.2f"
              % (p.split("/")[-1], sha(p)[:16], len(open(p, "rb").read()),
                 *[x for k in ("TL", "TR", "BL", "BR") for x in c[k]], mf, me))
    for p in day:
        s, m = top20(p)
        print("%-56s %s %8d  streaks %5.2f  mud %5.2f"
              % (p.split("/")[-1], sha(p)[:16], len(open(p, "rb").read()), s, m))
    if street:
        print("STREET WORST: flat %.1f (gate <= 49.5)  edge %.2f (gate >= 6.05)  %s"
              % (worst_flat, worst_edge, "PASS" if worst_flat <= 49.5 and worst_edge >= 6.05 else "FAIL"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
