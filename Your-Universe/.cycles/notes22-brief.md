# Cycle 22 brief (spawned 9 November)

Two workers, disjoint files:

## Cycle22World (src/world.mjs only)
Flaw 4 and 5 of the cycle 21 closure list.
- Corner fill: at hour 17 the street tier's failing corners are r1 top-right (flat 58.7,
  edge 5.58), r3 top-left (flat 52.8, edge 5.24), r3 bottom-left (flat 45.0, edge 4.96),
  r0 bottom-right (flat 50.7, edge 6.40). Target: every corner flat below 55.0 and edge at or
  above 5.31, which beats cycle 20 on both axes at the current hour.
- Details pool saturation: cursor 7,560 against a 6,653 budget. Reconcile duplicates and
  displaced rank-2 entries, spend the freed budget on the corners, keep street under 390,000
  and 118 calls.

## Cycle22Light (src/main.mjs + src/post.mjs only)
Flaws 2 and 1 (in that order of measurement).
- Sunlit warm share 73.8 against cycle 20's 76.7, with any-channel clipping at 1.438 percent
  against a 1.5 percent ceiling: warm the photograph through sun colour temperature, hemi
  ground bounce, shade keys and distance grade, never through intensity. Target warm >= 76.0
  with clipping <= 1.2 percent, purple at or under 5.5 on the dream, night palette held.
- Dream entry costs 48-78 ms against cycle 20's 17 ms: prime the dream materials and shaders
  at load (renderer.compile or a 1x1 hidden pass) so the entry frame carries no first-use cost;
  first paint still under 2 s.

# Protocol notes learned in cycle 21 (keep)
- Benches: median of three, idle machine only; identical builds span p1Low 48 to 175 when other
  work runs concurrently, so a single run proves nothing. `trackQuality`'s frameMs > 1000 guard
  is cycle 20 code documenting the same environment artifact.
- Captures: `write("screenshots/x.png", await page.screenshot())` inside `tab.run`;
  `screenshot({ path })` does not write here, and `tab.emulate` does not apply the mobile
  profile. Use `.cycles/measure.js` helpers for the phone rig.
- Only audit19 numbers compare across cycles; the day forensic hour is 17.

# Cycle 23 candidates (from the owner's items still open)
1. **Walking crowd.** The owner's target look names "animated pedestrians". Cycle 21 shipped a
   standing pack crowd plus procedural walkers. Bake two or three poses per character and swap
   per instance along the pedestrian paths (or attach the pack bodies to the existing pedestrian
   agents), instanced, no skinning update per frame, phone pool capped. This is the biggest
   remaining "toy" lever in item 5.
2. **Readable vehicles, second half.** `MODEL_LANE_EVERY = 6` (+4,540) needs budget bought back
   by the world pass first; the bench repopulation peak is 397,848 against 400,000.
3. **Facade rhythm at District tier**: shop awnings exist; window rhythm and balcony depth at
   district distance is the remaining item 5 line.
4. **The tail frames** at dream entry if the priming pass does not remove them fully.

## Cycle 23 walker study (read-only, 9 November)
Pedestrians today are two procedural instanced meshes in agents.mjs (`peds` = pedGeo,
`pedHeads` = pedHeadGeo, PED_CAP 700 desktop / 200 phone), matrices written per frame along the
trip paths. A pack-character walker is therefore a per-variant InstancedMesh swap of pedGeo with
the same matrix write path; the pose is static, the motion is the path, which is what low-poly
city games ship. Budget to solve first: 700 x ~150 tris = 105k desktop (current pedGeo is a
fraction of that), 200 x 150 = 30k phone. Options: reduce PED_CAP on desktop, keep procedural
peds at distance and pack bodies near the camera (distance-keyed variant choice), or decimate
the persons further. The head mesh disappears with the swap (the models carry heads).

## Walker cost, measured constants (for cycle 23)
pedGeo is a 6-sided cylinder (24 tris) and pedHeadGeo an icosahedron detail 0 (20 tris): a
procedural pedestrian is 44 triangles. A pack person is 149. Full swap costs +105 per walker:
700 walkers = +73,500 desktop (would put the street frame near 462k, impossible) and 200 =
+21,000 phone (over the 174,000 line). So the swap must be a cohort: only the nearest N walkers
wear pack bodies (60 near-camera walkers = +6,300 tris, 0 extra calls if folded into the same
instanced meshes), with procedural capsules beyond the cohort and on the phone unless the count
fits. Alternatively reduce PED_CAP desktop-side; the data rule ties the count to app overlap, so
a cap change must be documented as a visual-density choice, not a data change.
