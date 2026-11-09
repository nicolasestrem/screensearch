# READY

2026-11-09. Items 1 to 5 of `OWNER-NOTES.md` (14:00 section) are done and `node .cycles/gate.mjs`
passes. Every number below was measured on the build named here, after that build's own capture.

Bundle under test: `index.html` sha256 `adee12c9e43504d6a075d505233dba528aef0c26a5e1b077b3d8e5928fbd5807`.
Service worker `assets/sw.js` sha256 `1961ded3727e35dc3d11e47f0ca263b934b57620a4a797202d2928bfd6eb7264`.

| gallery file | sha256 | bytes |
| `screenshots/desktop-day.png` | 1feba0e5a2e94e7e35936abe03ffd0b84fedb56d744957043eecd3d821718161 | 1,414,240 |
| `screenshots/desktop-night-dream.png` | c59d6ce5e9c77d784020d7a5ad75052e5290302f62a3b343c6f5885c39d158f7 | 1,172,436 |
| `screenshots/pixel6-day.png` | 893092636bc585f7c94d75c0b0454910b7743969df9e2c0680521133f4440235 | 1,693,198 |

## Item 1, the four defect gates

`node .cycles/defects25.mjs`:

```
PASS (a) green wave is on inside the focus block and off after it closes  inside q=0 afterOld=true afterNew=false
PASS (a) agents.mjs reads activeSessionAt and keeps no longestSessionBefore helper
PASS (b) dream.clearCards exists and install clears cards before disposing the dream group  api=true clear@131 dispose@167
PASS (c) userModel is module state, set before the load install, and guards the boot install  declared=true guarded=true set@510 install@533
PASS (d) disposeGroup disposes and clears the shadow map before the no-material return  shadow@1100 null@1138 material@1180
DEFECTS25: PASS
```

`runDefects25Page(8731)`, the same three gates on the live page:

```
cards   before 2, button "Take it", after install 0, dream "awake"           PASS
user    delayed export wins: 2 quarters, "Delta Editor | Gamma Terminal"     PASS
shadow  live GL textures [22,22,22,22] over boot + 3 installs,
        created 37, deleted 15, growth 0                                     PASS
```

## Item 5, the bench gate

All tabs closed first (`browser.close({all:true})`), one bench per fresh tab. `p1LowFps` is the
rate implied by the 99th percentile frame duration over 3,241 measured frames after a 2 s warmup,
so it is the slowest 33 frames of the run, not an average; `p99FrameMs` and `over33` are reported
beside it so the number carries its own denominator.

```
cold cache   {"avgFps":179.8,"p1LowFps":169.5,"p99FrameMs":5.9,"maxFrameMs":24.5,"over33":0,
              "frames":3238,"calls":112,"triangles":397268,
              "phaseMaxMs":[0,6.0,6.3,6.2,24.5,8.1,6.2]}
desktop 1    {"avgFps":180,  "p1LowFps":169.5,"p99FrameMs":5.9,"maxFrameMs":8.2, "over33":0,
              "frames":3241,"calls":112,"triangles":397268,
              "phaseMaxMs":[0,6.2,6.1,6.2,6.1,8.2,6.0]}
desktop 2    {"avgFps":180,  "p1LowFps":169.5,"p99FrameMs":5.9,"maxFrameMs":6.3, "over33":0,
              "frames":3241,"calls":112,"triangles":397268,
              "phaseMaxMs":[0,6.0,6.1,6.3,6.0,6.1,6.1]}
desktop 3    {"avgFps":180,  "p1LowFps":163.9,"p99FrameMs":6.1,"maxFrameMs":8.7, "over33":0,
              "frames":3241,"calls":112,"triangles":397268,
              "phaseMaxMs":[0,6.0,6.0,6.1,6.2,6.3,8.7]}
phone4x      {"avgFps":180,  "p1LowFps":153.8,"p99FrameMs":6.5,"maxFrameMs":12.2,"over33":0,
              "frames":3241,"calls":57, "triangles":171652,
              "phaseMaxMs":[0,7.2,6.7,9.5,12.2,11.3,11.5]}
```

Gate: desktop `p1LowFps >= 100` and no frame over 33 ms after warmup; phone4x `p1LowFps >= 60`.
Met on all five runs, with `over33` zero in every one.

The phone profile's 4x CPU throttle is real, not a label. The same page benched at rate 1 and
rate 4, with every `requestAnimationFrame` callback timed: per-frame JS p99 0.7 ms to 2.5 ms
(3.6x), JS max 17.8 ms to 50.3 ms, and the bench itself moved from 175.4 / max 6.2 to 166.7 /
max 24.3. The p99 lands on the vsync tick because fewer than 33 of 3,241 frames exceed it; the
frames that do are what `maxFrameMs` and `over33` report, and both are in the gate.

Cold first visit: boot frame 562.8 ms with 25.4 ms of JS, before the warmup window, and it appears
with the service worker absent as well, so it is the page's own first-frame build rather than
cache work. After 2000 ms the same run's worst frame is 26.3 ms with zero frames over 33 ms, and
the visit ends with the 12 cache entries below.

## Item 5, the wake window

`wakeSlice(8731)`, one page, one sleep, one wake. These are slots that actually entered, per pool
(ring, tram, crane, vans, buses, walkers), not a frame counter:

```
desktop  frame  1  admitted [4,3,1,2,1,5]  cumulative  16  vehicles 25  peds  5
         frame 15  admitted [4,0,1,1,0,5]  cumulative 170
         frame 30  admitted [3,0,0,1,0,5]  cumulative 309  totals [100,24,14,35,2,133]
phone4x  frame  1  admitted [1,1,1,1,1,3]  cumulative   8  vehicles 19  peds  3
         frame 15  admitted [0,0,1,1,0,2]  cumulative  66
         frame 30  admitted [0,0,0,0,0,2]  cumulative 129  totals [20,10,6,15,2,76]
```

`slice30.mjs` reproduces both series on the shipped block, and adds: a 700-walker pool and a
1-bus pool both spread over 30 frames; a moving target closes inside the 60 frame hard stop; a
rebuild under a dream opens nothing and counts no frames; a window left open at sleep pauses
instead of counting idle frames.

`wakeStall(8731)`: before the change 936.5 ms at the wake edge with 0.9 ms of JS; on this build
3,343 frames with a 12.5 ms maximum and zero frames over 33 ms.

## Item 5, the draw-call line

Measured **105 calls** with assets and **55** in `?noassets=1`, which is what `src/CONTRACT.md`
now states, with the decomposition: 51 world + 24 agents = 75 scene draws, 24 shadow casters, 6
post passes; fallback 25 + 14 = 39, 10 casters, 6 post. Under 80 was not met: it needs 26 calls,
and the only levers are the desktop directional shadow map or merging the fleet's per-model pools
and the world's 51 static passes, both of which change what is drawn.

## Item 4, the ring

Both named crossings, plus the third the quadrant needed, carry a building row on both kerbs out
past the frame edge. Footprint frontage, counted on the final build: southWest 0 empty of 21
on-screen bins (both-sided to 205 of 205), east 0 of 24 (235 of 235), westLower 0 of 24 (235 of
235), hillside 0 of 25 (245 of 245), north 0 of 6 (55 of 55, the designed track edge at the crest).

Corner audit on the four street rotations, floors flat <= 49.5 and edge >= 6.05:

| file | top left | top right | bottom left | bottom right |
| `iter-29-street-r0.png` | 26.8 / 15.33 | 14.9 / 16.21 | 25.0 / 13.82 | 37.0 / 9.16 |
| `iter-29-street-r1.png` | 22.9 / 19.94 | 44.0 / 8.71 | 43.2 / 9.51 | 36.1 / 9.37 |
| `iter-29-street-r2.png` | 46.5 / 8.73 | 33.0 / 17.55 | 42.3 / 6.53 | 48.0 / 7.58 |
| `iter-29-street-r3.png` | 43.3 / 9.44 | 28.8 / 20.78 | 40.3 / 9.66 | 33.9 / 10.29 |

Sixteen of sixteen crops inside both floors. The crop that read 53.0 / 4.49 for three cycles now
reads 42.3 / 6.53.

Top 20 percent of the day frame: streaks 0.34 percent, mud 1.45 percent, row edges 0.04 / 0.29 /
0.29 / 0.11 / 0.30 / 1.77. Streaks and mud are both better than the frame item 2 was accepted on
(0.49 / 1.87). The 150 to 180 row edge is 0.17 over the 1.6 line that item set, and it is left
open and recorded here rather than redefined.

## States and the offline cache

| state | calls | triangles |
| desktop day | 105 | 388,164 |
| desktop street | 105 | 388,956 |
| desktop night | 103 | 389,478 |
| desktop dream | 91 | 334,322 |
| desktop `?noassets=1` | 55 | 321,663 |
| phone day | 54 | 169,656 |

Bench peak 112 calls / 397,268 triangles, under the 400,000 hard gate and 2,732 under the working
line.

Offline: a first load installs 12 entries / 6.68 MB of Cache Storage (shell, sample, manifest, six
packs, three modules); three further navigations with new `?cb=` and `?hour=` leave the count at 12
and `storage.estimate().usage` unchanged; an offline reload renders 103 calls / 390,314 triangles
with 87 models loaded, district and street tiers, no error banner.

## After READY

Cycles 30 onward run under `masterpiece-prompt.md` Phase 2 and 3 and change the build, so these
numbers are pinned to the bundle named above (`adee12c9e435…`). The current bundle and its own
audit live in `ITERATIONS.md` "Cycle 36": day 102 calls / 384,134 triangles, street 102 / 384,926,
night 100 / 388,788, dream 88 / 333,632, without assets 55 / 320,319, phone 54 / 171,162. Desktop
`p1LowFps` 169.5, 166.7 and 169.5 with `over33` 0 and p99 5.9 ms, phone4x 169.5, bench peak 399,188
triangles against the 400,000 hard gate, all sixteen corner crops still inside both floors, top band
streaks 0.35 percent and mud 1.26 percent. The p1 low sits below the 175.4 of cycles 31 to 34 on a
host that had a screen recorder and several browsers resident, so it is recorded as unverified
rather than as a regression: the next cycle re-benches on an idle host.

Cycles 31 to 36 work Phase 3, "life and detail", and every one of them pays for itself rather than
borrowing from the gate: the five spurs were lit (39 to 81 posts, 81 to 111 glow discs), the terraces
went from one frontage to five and then gained a table and two chairs each (15 to 32 terrace units of
32 triangles in one draw call), and nine of them now have somebody standing at them, taking the crowd
from 29 to 38 people. Draw calls came down from 105 to 102 on desktop while all of that was added.
No cycle was checked by an eye: this environment has no image input, so every art judgement recorded
here is a measurement, and `ITERATIONS.md` says so at each step.
