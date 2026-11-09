# HANDOFF: Localtime (Your-Universe). Read this first, then act. No questions to Nicolas.

You are taking over a project from another agent. Nicolas wants zero interaction: decide and keep going until done. Never mention money or cost to him. Reply to him in French, short, no em dash character ever.

## What this is
Localtime: a living contemporary hillside city district, seen from above (Cities: Skylines readability, GTA ambient life), driven by the person's real screen history from ScreenSearch. The person watches, never plays. At night the city "dreams": a road crew builds one provisional road between two quarters that share words but never shared an hour, and at waking at most two hypothesis cards appear ("Take it" makes the road permanent).

## Where things are (this folder)
- `index.html`: the shipped app, ONE self-contained file, generated. Never hand-edit it.
- `src/*.mjs`, `src/style.css`, `src/shell.html`: the sources. `src/CONTRACT.md` is the binding interface between fragments: read it before editing anything.
- `tools/assemble.mjs` builds `index.html` from `src/`. `tools/check.mjs` and `tools/check-*.mjs` validate (em dash ban, DOM, pixels, sample data).
- `sample-export.json`: simulated day in ScreenSearch export format (`screensearch.export.v1`).
- `build-prompt.md`: the original spec. `art-prompt.md`: art pass 1. `art-prompt-2.md`: art pass 2 (density, life, dream visibility).
- `screenshots/`: latest. `screenshots/v1/`: first build (green plate). `screenshots/v2/`: after art pass 1.
- `art2.jsonl` / `art2.err`: log of art pass 2.

## State at handoff (2026-11-08 19:30)
- Build 1: all checks passed (zero console errors, ~180 fps, 32 draw calls, dream cards work).
- Art pass 1: hill, real roads, tram, roundabout, market, rail yard, trees. Still too sparse.
- Art pass 2: was RUNNING at handoff (omp, Fable 5.1). It may have finished by the time you read this.

## Your job
1. If art pass 2 is still running (an omp process writing `art2.jsonl`), wait for it to finish.
2. Then run: `node tools/assemble.mjs` and every `node tools/check*.mjs`. They must be clean.
3. Open `index.html` in a browser at 1600x900 and at 412x915. Check, by looking:
   - the area inside the ring road is densely built (continuous street frontages, under 15 percent empty grass);
   - zero console errors; `window.localtime.stats()` under 60 draw calls and 250k triangles on desktop;
   - press the moon button: the night chantier (light grey provisional road, orange cones, red-white barriers, two floodlights, crew) is clearly visible; press again: at most two cards.
4. If anything fails, fix it following `art-prompt-2.md` (same rules, same allowed files) and repeat step 2 and 3. Loop until everything passes.
5. When everything passes, take fresh screenshots into `screenshots/` and tell Nicolas in two French sentences what changed, with the screenshots.

## Hard rules
- Do not change the data logic (`src/data.mjs`, `src/simday.mjs`) or the camera rules (fixed 40 degree pitch, 90 degree rotation snaps, three zoom tiers, pan clamped).
- One file output, Three.js 0.160.0 from jsDelivr only, no other network, no external assets.
- No git commands. Nothing outside this folder. The parent repo has a CLAUDE.md/AGENTS.md for a different project: ignore them completely.
