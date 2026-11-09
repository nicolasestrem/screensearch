# Cycle 21 plan (draft, from the cycle 20 measurements)

Owner item 5 is the one still standing: *"Overall still reads as a toy. Push toward the target photo:
richer facade detail at District tier (window rhythm, balconies, shop awnings), roofs with clutter,
softer and longer shadows, a subtler tilt-shift, warmer golden light, more readable vehicles and
crowds."* That sentence names seven levers, and none of them has been worked yet. Cycle 21 takes all
seven and splits them by file so two workers cannot collide.

## Worker A, world and models: the fabric of the city itself

Files: `src/world.mjs`, `tools/pack-assets.mjs`, `tools/asset-selection.json`, `assets/` (and therefore
`tools/stamp-sw.mjs` after packing).

1. **Balconies** on the procedural mid-rise facades, district tier: a slab plus a rail, the rail either
   three thin bars or one plate, placed on the same bay rhythm the windows already use, on the rows
   where the data says residential. Instanced, one or two draw calls, ranked in the detail ladder.
2. **Shop awnings** over the ground-floor shop bays that already exist (`shopAt`), with a few colour
   variants, and a valance so they read at 40 degrees pitch rather than as flat plates.
3. **Roof clutter**: water tanks, chimneys, AC boxes, antennas, satellite dishes, stair bulkheads,
   parapet caps, a few solar panels, drying racks. This is the single cheapest silhouette win at the
   district distance because it breaks the roofline, which is what makes a city read as inhabited
   rather than extruded.
4. **Crowds.** Stage and pack `kenney-mini-characters` (already fetched, measured, and documented in
   `.cycles/notes20-crowds.md`: 13 bodies, about 800 tris each, feet at y=0, one shared colormap, UVs
   already address their atlas cells so no packer change is needed). They are skinned, so bake the
   bind pose and use them for **standing** crowds only: market buyers and stall keepers, the tram stop
   queue and the bus shelter, cafe tables under the parasols, the plaza and the church forecourt, and
   street corners. Keep the procedural walkers for anyone moving: a rigid body sliding along a
   pavement looks worse than an animated box. Seven variants on the phone, all thirteen on desktop,
   tinted and scaled per instance within 2.0..2.4x so no row looks cloned. Accessibility props
   (`aid-cane`, `aid-crutch`, `aid-glasses`) and luggage are available too.
5. **Budget.** Phone night awake is 174,725 of 180,000, the tightest number in the project and
   world-side. Everything above must be ranked so the phone keeps under 174,000, and if the facade
   detail cannot fit, find the 3k somewhere the phone genuinely does not need it (the phone already
   drops window frame rings, shop glazing, contact blobs and parasols) rather than shipping over.

## Worker B, light and lens: how the frame is photographed

Files: `src/post.mjs`, `src/main.mjs` (light rig only), `src/shell.html` if a CSS change is needed.
Do not touch `src/world.mjs`, `src/dream.mjs`, `src/assets.mjs` or `assets/`.

1. **Softer, longer shadows.** The brief asks for both. That means a lower sun (the day hour pinned at
   the golden hour the data picks), a wider shadow camera frustum around the district, and a softer
   filter: check what the renderer allows at three r160 (PCFSoftShadowMap plus a larger
   shadow.mapSize costs memory, a larger radius costs samples). Measure the shadow terminator width
   in pixels and the shadow length as a multiple of the caster height, before and after, because
   "softer" and "longer" are both measurable on the frame.
2. **A subtler tilt-shift.** Cycle 19's blur radius is 0.55. Measure the current falloff profile down
   the frame (the blur amount at ten rows) and reduce it so the far city and the near pavement stay
   legible while the mid-ground stays crisp. The owner's target photo has a gentle, photographic
   falloff, not a miniature effect.
3. **Warmer golden light.** The whole-frame saturation is 0.386 and the lower third warm share is 80.7
   percent, so the grade is already warm; the lever left is *directionality*: a warmer key with a
   cooler fill in shadow makes the same palette read as sunlight rather than as a tint. Measure the
   warm share in sunlit versus shadowed regions separately, not just the whole frame, and keep
   clipping at 0.000 percent.
4. **Readable vehicles.** The avenue fleet is modelled on one carriageway. Spending triangles here is
   the visible win the owner named, so if budget exists, take the second carriageway partway
   (`MODEL_LANE`) or upgrade the procedural cars' night plates and silhouettes.

## Both workers

- Report with `.cycles/audit19.py` (`judge`, `corners`) and `.cycles/specks.py`, never with adjectives.
- Every change must keep desktop under 390,000 and phone under 174,000 triangles, with draw calls at
  or under 118 desktop and 68 phone so the gates are not the binding constraint.
- Zero console errors, `node .cycles/gate.mjs` green, screenshots regenerated.
