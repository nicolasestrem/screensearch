# Crowd models: measured, staged, ready (cycle 20 finding)

Kenney **Mini Characters** is staged at `.cache/packs/kenney-mini-characters/` (43 GLB, 5.32 MB,
`License.txt` says Creative Commons Zero). It is registered in `tools/fetch-assets.mjs` as
`kenney-mini-characters` and fetched with `--only`, so `assets/` is untouched until it is packed.
`assets/LICENSES/kenney-mini-characters.txt` already ships the proof.

## The 13 usable bodies

`character-female-a..f` and `character-male-a..g`, measured from the GLB:

| | tris | width x height x depth (m) |
|---|---|---|
|female a..f|790..924|0.77..1.10 x 0.67..0.78 x 0.42..0.53|
|male a..g|686..877|0.77 x 0.66..0.79 x 0.34..0.46|

`male-a` is 771 tris, 0.77 x 0.67 x 0.34 m. Total 10,416 tris for all 13, average 801. Every model's
origin is at the feet (`y` starts at 0.00), the same convention the vehicles use. All primitives
share one external `Textures/colormap.png`, so palette dedupe collapses the whole pack to **one
material**.

They are chunky minifigs, so plan on a scale near 2.2x: 0.67 m becomes about 1.47 m and 0.79 m about
1.74 m, i.e. a plausible adult height range, while staying deliberately stylised. Scale per instance
within 2.0..2.4 and tint per instance so no row looks cloned.

## No tool change needed

`KHR_texture_transform` is declared but carries no offset or scale, and each model's `TEXCOORD_0`
already addresses its own cell of the shared colormap (male-a spans u 0.094..0.969, male-c u
0.094..0.469, worker u 0.031..0.969, all at v 0.275..0.975). So the existing
`tools/pack-assets.mjs` path packs them unchanged; do not add transform baking on their account.

## The catch: they are skinned

Each GLB has a skeleton (`torso`, `head`, `arm-left`, `arm-right`, `leg-right`) and two skinned
primitives (`body-mesh`, `head-mesh`, skin 0 and 1). Baking node transforms gives the bind pose,
which reads as a person standing with arms slightly out, and that is fine for standing crowds. It is
**not** fine for walkers: a rigid body sliding along a pavement looks worse than the current
animated boxes, and true skinning on an `InstancedMesh` needs either per-instance bones or a vertex
shader driven by uniforms, which is a feature, not an afternoon.

So the honest use is static crowds, which is also where the frame needs help:

- market buyers and stall keepers (standing, grouped, facing the stalls)
- the tram stop queue and the bus shelter
- cafe tables under the parasols, two per table facing each other
- the plaza and the church forecourt
- street-tier corners: two or three figures near the kerb, a pair at a shop door

Seven variants on the phone, all thirteen on the desktop. One `InstancedMesh` per model, so seven
calls costs about 5,600 tris for 40 people, and thirteen costs 10,416 tris for about 70. That is
affordable inside the current headroom (desktop day sits at 354,973 of 400,000, phone at 165,677 of
180,000) but it must be measured against the street-detail work landing in the same cycle, so rank it
in the detail ladder and report the gate table before and after.

## What is excluded

`EXCLUDE` in `tools/fetch-assets.mjs` now also skips `alien`, `knight`, `raider`, `robot`, `soldier`,
`weapon`, `bow`, `spear`, `arrow`, so the fantasy cast and the weapon props of this kit never reach
selection. Accessibility props (`aid-cane`, `aid-crutch`, `aid-glasses`, `aid-sunglasses`,
`aid-mask`, `aid_hearing`, `aid-defibrillator`) and `item-luggage` remain available and are worth a
few instances in the market and at the station.
