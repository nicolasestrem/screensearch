import * as THREE from 'three';

// Localtime composition root. tools/assemble.mjs appends this fragment last; it is the
// only fragment with side effects at load (the single main() call at the bottom).
//
// Owner: H. Seam: src/CONTRACT.md, section "main.mjs (H)".
// One top-level identifier: main. Every helper, constant and state variable lives in its
// closure because the assembler merges all fragments into one module scope.
//
// Besides the bootstrap this fragment owns four things the seam document does not:
// the adaptive quality controller (profile detection, the notch ladder, slow stepping),
// the benchmark harness behind ?bench=1, the ?hour= pin that makes a screenshot
// deterministic, and the label declutter pass (measured on a slow timer, never per frame).
function main() {
  const doc = document;
  const win = window;
  const canvas = doc.getElementById('view');
  if (!canvas) return;

  // ---------------- constants ----------------
  const SKY_COLOR = 0x9ec3d6;
  const SPEED = 60; // data milliseconds per real millisecond
  const HOUR_MS = 3600000;
  const SIM_DAY_TZ = 3600000; // the built-in day is authored at UTC+1 (Europe/Paris in November)
  const MAX_VEHICLES = 420;
  const MAX_PEDESTRIANS = 700;
  const RING_SAMPLES = 96;
  // The pack loader's deadline. It runs beside the sample fetch, so this is also the longest
  // the first install can wait when the packs are missing; a phone gets a shorter one.
  const ASSET_TIMEOUT_MS = 8000;
  const ASSET_TIMEOUT_MOBILE_MS = 6000;
  const TEXTURE_KEYS = [
    'map',
    'alphaMap',
    'normalMap',
    'emissiveMap',
    'roughnessMap',
    'metalnessMap',
    'aoMap',
    'lightMap',
    'bumpMap',
    'envMap'
  ];

  // Quality controller. The ladder is one flat list of notches, generated in the exact
  // order the product asks for: post resolution, then pixel ratio, then shadow map, then
  // population. Only the dimensions that still have room from the profile's start point
  // contribute notches, so a phone ladder never tries to raise a shadow map it never had.
  const FPS_WINDOW_MS = 2000;
  const FPS_DROP = 57;
  const FPS_HEADROOM = 59;
  const HEADROOM_MS = 10000;
  const STEP_COOLDOWN_MS = 1000;
  const POST_SCALES = [1, 0.75, 0.5];
  const SHADOW_SIZES = [2048, 1024, 0];
  const POPULATION_SCALES = [1, 0.75, 0.5];
  const PIXEL_STEP = 0.25;
  const PIXEL_FLOOR = 1;

  // Distance grade. The post grade's cool distance key is keyed on height in the frame, which
  // is a depth proxy only at the district distance, so main weights it by the camera distance
  // to the look-at. These are camera.mjs's TIER_ZOOM factors for the block and district tiers:
  // the ramp runs from block to district, and DIST_GRADE_MIN_STEP is the smallest change worth
  // a uniform write.
  const DIST_GRADE_BLOCK_ZOOM = 0.62;
  const DIST_GRADE_FULL_ZOOM = 1;
  const DIST_GRADE_MIN_STEP = 0.002;

  // Labels. Their boxes are measured on a slow timer (never inside the render loop) and
  // re-measured only when the viewport or the camera orientation changes.
  const LABEL_MARGIN = 6;
  const DECLUTTER_MS = 250;
  // The label box is pinned to an even integer width and an integer height (see
  // measureLabels), so its cap is a whole number of pixels too. The portrait media query is
  // the only place the cap differs; it is cached here, never queried in the render loop.
  const PORTRAIT_PHONE = typeof win.matchMedia === 'function'
    ? win.matchMedia('(max-width: 520px) and (orientation: portrait)')
    : null;
  const LABEL_CAP_DESKTOP = 0.46;
  const LABEL_CAP_PHONE = 0.4;

  // Benchmark. A fixed 20 s script after a 2 s warmup, one sample per frame. The auto run
  // (and the quality ladder) additionally wait out the install burst, because the first seconds
  // after install carry the shader compile and upload cost and would otherwise be reported as
  // the machine's steady state.
  const BENCH_WARMUP_MS = 2000;
  const BENCH_TOTAL_MS = 20000;
  const BENCH_SETTLE_MS = 4000;
  const BENCH_CAPACITY = 8192;
  const BENCH_PHASE_AT = [0, 3000, 5000, 7000, 10000, 14000, 17000];

  // ---------------- query parameters ----------------
  // ?hour=NN pins the wall clock so a screenshot is reproducible; ?bench=1 runs the fixed
  // benchmark after the first frame; ?noassets=1 forces the procedural world and ?assets=street
  // adds the lazy street tier. They are independent and may be combined.
  const query = parseQuery(win.location ? win.location.search : '');
  const hourPin = parseHour(query.hour);
  const benchRequested = isFlagOn(query.bench);
  // ?noassets=1 (or a bare ?noassets) never touches the pack loader at all, so the fallback
  // path is testable without a manifest on disk and without a failed request in the console.
  const assetsOff = query.noassets !== null && (query.noassets === '' || isFlagOn(query.noassets));
  const streetRequested = query.assets !== null && String(query.assets).trim().toLowerCase() === 'street';
  // ?post=0 renders straight to the canvas with the post chain disabled. It is a measurement
  // affordance, not a product setting: the blur and bloom audits compare a frame against its
  // own no-post reference, and ?post=0 is what captures that reference without touching the
  // quality profiles a user sees.
  const postOff = postQueryOff(query.post);

  function parseQuery(search) {
    const out = { hour: null, bench: null, assets: null, noassets: null, post: null };
    if (typeof search !== 'string' || search.length < 2) return out;
    const body = search.charAt(0) === '?' ? search.slice(1) : search;
    const parts = body.split('&');
    for (let i = 0; i < parts.length; i += 1) {
      const part = parts[i];
      if (!part) continue;
      const split = part.indexOf('=');
      const rawKey = split < 0 ? part : part.slice(0, split);
      const rawValue = split < 0 ? '' : part.slice(split + 1);
      let key = rawKey;
      let value = rawValue;
      try {
        key = decodeURIComponent(rawKey);
        value = decodeURIComponent(rawValue);
      } catch (err) {
        // A malformed escape keeps the raw pair; the value then just fails the number test.
      }
      if (key === 'hour') out.hour = value;
      else if (key === 'bench') out.bench = value;
      else if (key === 'assets') out.assets = value;
      else if (key === 'noassets') out.noassets = value;
      else if (key === 'post') out.post = value;
    }
    return out;
  }

  function parseHour(raw) {
    if (raw === null || raw === undefined) return null;
    const text = String(raw).trim();
    if (text === '') return null;
    const value = Number(text);
    if (!isFinite(value)) return null;
    if (value < 0) return 0;
    if (value > 24) return 24;
    return value;
  }

  function isFlagOn(raw) {
    if (raw === null || raw === undefined) return false;
    const text = String(raw).trim().toLowerCase();
    return text === '1' || text === 'true' || text === 'yes' || text === 'on';
  }

  // ?post=0 (or a bare ?post) renders straight to the canvas with the post chain
  // disabled; it is a measurement affordance for the blur and bloom audits, which
  // compare a frame against its own no-post reference. A numeric value means the
  // number (0 off, anything above 0 on), a bare presence behaves like ?noassets.
  function postQueryOff(raw) {
    if (raw === null || raw === undefined) return false;
    const text = String(raw).trim();
    if (text === '') return true;
    const value = Number(text);
    if (isFinite(value)) return !(value > 0);
    return !isFlagOn(text);
  }

  // ---------------- profiles ----------------
  // Desktop: MSAA renderer and post target, shadows on with a 2048 map, pixel ratio capped
  // at 2 and never above the device, full population and every scattered detail.
  // Mobile: no MSAA, FXAA in the composite, shadows off, pixel ratio capped at 1.5, half
  // resolution post, half population, one detail notch down.
  //
  // There is exactly ONE mobile test on this page and it is the one below: a coarse pointer or
  // a touch screen. It is resolved once, at boot, into `isTouch`, and everything that changes
  // behaviour between a phone and a desk reads that one flag. A narrow window with no touch is
  // a small desktop window, not a phone: it keeps the desktop profile (shadows, full post) and
  // the desktop population caps, so its frame is never a phone's quality carrying a desk's
  // crowd. The flag is also handed to createAgents as opts.mobile, which is how the population
  // caps and the render profile can never disagree again (cycle 19 shipped two rules: a width
  // test here and a width plus touch test in agents.mjs).
  const coarse = typeof win.matchMedia === 'function' && win.matchMedia('(pointer: coarse)').matches;
  const touchScreen = typeof navigator !== 'undefined' && navigator.maxTouchPoints > 0;
  const isTouch = coarse || touchScreen;

  function detectMode() {
    return isTouch ? 'mobile' : 'desktop';
  }

  function profileFor(name) {
    if (name === 'mobile') {
      return {
        mode: 'mobile',
        ratioCap: 1.5,
        postEnabled: true,
        postMsaa: false,
        postFxaa: true,
        postScale: 0.5,
        shadows: false,
        shadowMap: 0,
        population: 0.5,
        detail: 1
      };
    }
    return {
      mode: 'desktop',
      ratioCap: 2,
      postEnabled: true,
      postMsaa: true,
      postFxaa: false,
      postScale: 1,
      shadows: true,
      shadowMap: 2048,
      population: 1,
      detail: 2
    };
  }

  function ratioCapFor(targetProfile) {
    const dpr = win.devicePixelRatio || 1;
    return dpr > targetProfile.ratioCap ? targetProfile.ratioCap : dpr;
  }

  function notchOf(base, patch) {
    return {
      postScale: patch.postScale === undefined ? base.postScale : patch.postScale,
      pixelRatio: patch.pixelRatio === undefined ? base.pixelRatio : patch.pixelRatio,
      shadowMap: patch.shadowMap === undefined ? base.shadowMap : patch.shadowMap,
      population: patch.population === undefined ? base.population : patch.population
    };
  }

  // Built once per profile or device pixel ratio change. Degrading the post resolution is
  // the cheapest and least visible step, so it comes first; the shadow map is the most
  // visible, but it still comes before population, which the eye catches on the roads.
  function buildLadder(targetProfile, cap) {
    const built = [];
    let current = notchOf(
      {
        postScale: targetProfile.postScale,
        pixelRatio: cap,
        shadowMap: targetProfile.shadowMap,
        population: targetProfile.population
      },
      {}
    );
    built.push(current);
    let i = 0;
    for (i = 0; i < POST_SCALES.length; i += 1) {
      if (POST_SCALES[i] < current.postScale - 1e-6) {
        current = notchOf(current, { postScale: POST_SCALES[i] });
        built.push(current);
      }
    }
    while (current.pixelRatio > PIXEL_FLOOR + 1e-6) {
      let next = Math.round((current.pixelRatio - PIXEL_STEP) * 1000) / 1000;
      if (next < PIXEL_FLOOR) next = PIXEL_FLOOR;
      if (next >= current.pixelRatio - 1e-6) break;
      current = notchOf(current, { pixelRatio: next });
      built.push(current);
    }
    for (i = 1; i < SHADOW_SIZES.length; i += 1) {
      if (current.shadowMap > SHADOW_SIZES[i]) {
        current = notchOf(current, { shadowMap: SHADOW_SIZES[i] });
        built.push(current);
      }
    }
    for (i = 0; i < POPULATION_SCALES.length; i += 1) {
      if (POPULATION_SCALES[i] < current.population - 1e-6) {
        current = notchOf(current, { population: POPULATION_SCALES[i] });
        built.push(current);
      }
    }
    return built;
  }

  // ---------------- environment flags ----------------
  // `coarse`, `touchScreen` and `isTouch` are resolved once in the profiles section above, so
  // the profile, the world build, the camera rig and the agent caps all read one value.
  let mode = detectMode();
  let profile = profileFor(mode);
  let ladder = buildLadder(profile, ratioCapFor(profile));
  let notch = 0;
  let stage = ladder[0];

  // ---------------- renderer and scene ----------------
  const renderer = new THREE.WebGLRenderer({
    canvas: canvas,
    antialias: profile.postMsaa,
    powerPreference: 'high-performance',
    alpha: false,
    stencil: false,
    depth: true
  });
  renderer.setPixelRatio(stage.pixelRatio);
  renderer.setSize(win.innerWidth, win.innerHeight, false);
  renderer.shadowMap.enabled = profile.shadows;
  // Cycle 21 golden-hour photograph work: PCFSoft's kernel is a fixed ~3x3 span that
  // ignores shadow.radius, so the penumbra is locked to one shadow-map texel and the
  // lower sun cannot soften it. Plain PCF is a 17-tap kernel that scales with
  // shadow.radius (verified in the r160 shadowmap_pars_fragment), which is what lets
  // the wrapped light rig widen the penumbra. Set here at boot, before any material
  // compiles, so the shadow programs never link twice.
  renderer.shadowMap.type = THREE.PCFShadowMap;
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.setClearColor(SKY_COLOR, 1);
  // The post pass renders the scene several times per frame. Without a manual reset every
  // render() would zero info, so stats() would report only the last pass.
  renderer.info.autoReset = false;
  renderer.clear();

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(SKY_COLOR);

  // ---------------- DOM handles ----------------
  const labelsRoot = doc.getElementById('labels');
  const cardEl = doc.getElementById('card');
  const cardTitle = cardEl ? cardEl.querySelector('.card-title') : null;
  const cardTime = cardEl ? cardEl.querySelector('.card-time') : null;
  const cardPhrase = cardEl ? cardEl.querySelector('.card-phrase') : null;
  const fpsEl = doc.getElementById('fps');
  const dropEl = doc.getElementById('drop');
  const fileInput = doc.getElementById('file');

  // The four controls labels must never overlap. Collected once; the boxes are read on
  // the slow declutter timer, never in the render loop.
  const avoidEls = [];
  const avoidBoxes = new Float32Array(16);
  let avoidCount = 0;
  let acceptedBoxes = new Float32Array(0);
  let acceptedCount = 0;
  let labelMeasureDirty = true;
  // The overlay container's own corner, read on the declutter timer. It is subtracted from
  // every projected point so a safe-area inset moves the overlay without moving the maths.
  let labelOriginX = 0;
  let labelOriginY = 0;
  let measuredYaw = NaN;
  let measuredW = -1;
  let measuredH = -1;

  function cacheAvoidTargets() {
    avoidEls.length = 0;
    const ids = ['rot-left', 'rot-right', 'sleep', 'open-file'];
    for (let i = 0; i < ids.length; i += 1) {
      const el = doc.getElementById(ids[i]);
      if (el) avoidEls.push(el);
    }
  }

  // ---------------- state ----------------
  let model = null;
  // The pack assets: one loader for the whole session, one Assets object reused by every
  // install (a dropped export never re-reads a pack), and a live report for the debug API.
  let assets = null;
  let assetLoader = null;
  let assetTiers = null;
  const assetReport = { loaded: false, models: 0, tier: [] };
  let world = null;
  let agents = null;
  let rig = null;
  let dream = null;
  let post = null;
  let postToggleWired = false;

  // Distance grade state: the framing product (radius * pad) sampled while the rig is framed
  // to the district, and the last weight handed to the post composite (-2 forces a first write).
  let distGradeFraming = 0;
  let distGradeSent = -2;

  let tData = 0;
  let wallHour = 0;
  let night = false;
  let nightOverride = false;
  let gapActive = -1;
  let wakeHook = null;
  // Mirrors dream.state into post.setDreamBloom on a transition only (see tick).
  let dreamBloomOn = false;

  let frames = 0;
  let fpsClock = 0;
  let fpsValue = 0;
  let fpsVisible = false;
  // The last text the readout actually received. Comparing the numbers instead of the string
  // keeps a still frame loop from building one every quarter second.
  let fpsWrote = false;
  let fpsShownFps = -1;
  let fpsShownCalls = -1;
  let fpsShownTris = -1;
  let fpsShownVehicles = -1;
  let fpsShownPeds = -1;
  let firstFrameAt = 0;
  let lastFrameStamp = 0;

  // Quality measurement. A fixed ring keeps a rolling 2 s window without allocating.
  const frameRing = new Float32Array(480);
  let ringStart = 0;
  let ringLen = 0;
  let ringSum = 0;
  let qualityCooldown = 0;
  let headroomMs = 0;
  // Milliseconds of frame time counted since the last install. The quality ladder stays out of
  // the way until this passes BENCH_SETTLE_MS, so the loading burst cannot walk it down.
  let bootAgeMs = 0;

  // Benchmark state.
  const benchFrames = new Float64Array(BENCH_CAPACITY);
  let benchRunning = false;
  let benchStartAt = 0;
  let benchPhase = 0;
  let benchCount = 0;
  let benchSum = 0;
  let benchResolve = null;
  let benchPromise = null;

  const labels = [];
  const hits = [];
  const probe = new THREE.Vector3();
  const clockDate = new Date();
  const sim = { tData: 0, wallHour: 0, night: false, dreaming: false, dt: 0 };

  // dream.mjs registers its own wake invoker through ui.onWake and drives the night
  // override through setNightOverride, so main neither owns nor overrides those transitions.
  // `compact` is the same device flag buildWorld gets: on the phone the dream drops the props
  // that cannot be read at 412 px, which is what keeps that frame inside the call ceiling.
  const ui = {
    cardsRoot: doc.getElementById('cards'),
    compact: isTouch,
    setNightOverride: function (on) {
      nightOverride = !!on;
    },
    onWake: function (fn) {
      wakeHook = typeof fn === 'function' ? fn : null;
    }
  };

  // ---------------- small helpers ----------------
  function nowStamp() {
    if (win.performance && typeof win.performance.now === 'function') return win.performance.now();
    return Date.now();
  }

  function wallHourNow() {
    if (hourPin !== null) return hourPin;
    clockDate.setTime(Date.now());
    return (
      clockDate.getHours() +
      clockDate.getMinutes() / 60 +
      clockDate.getSeconds() / 3600 +
      clockDate.getMilliseconds() / 3600000
    );
  }

  function browserTzOffset() {
    clockDate.setTime(Date.now());
    return -clockDate.getTimezoneOffset() * 60000;
  }

  function formatDuration(ms) {
    const minutes = Math.round((ms || 0) / 60000);
    if (minutes < 1) return 'under a minute today';
    if (minutes < 60) return minutes + ' min today';
    const hours = Math.floor(minutes / 60);
    const rest = minutes % 60;
    return (rest === 0 ? hours + ' h' : hours + ' h ' + rest + ' min') + ' today';
  }

  function hexColor(value) {
    let text = ((value | 0) & 0xffffff).toString(16);
    while (text.length < 6) text = '0' + text;
    return '#' + text;
  }

  // ---------------- quality controller ----------------
  // Applying a notch touches the renderer, the post chain, the world lights and the agents.
  // It runs on resize and on a step only, never per frame. The world and agent hooks are
  // only called when their value actually changes: setShadowMapSize replaces a texture, so
  // a window drag must not walk it.
  let appliedShadow = -1;
  let appliedDetail = -1;
  let appliedPopulation = -1;

  function applyViewport() {
    if (!stage) return;
    renderer.setPixelRatio(stage.pixelRatio);
    renderer.setSize(win.innerWidth, win.innerHeight, false);
    if (post) {
      post.setSize(win.innerWidth, win.innerHeight);
      post.setResolutionScale(stage.postScale);
    }
  }

  function applyStage() {
    if (!stage) return;
    applyViewport();
    if (world) {
      if (appliedShadow !== stage.shadowMap) {
        appliedShadow = stage.shadowMap;
        world.lights.setShadowMapSize(stage.shadowMap);
      }
      if (appliedDetail !== profile.detail) {
        appliedDetail = profile.detail;
        world.setDetailLevel(profile.detail);
      }
    }
    if (agents && appliedPopulation !== stage.population) {
      appliedPopulation = stage.population;
      agents.setPopulationScale(stage.population);
    }
    invalidateLabelMeasure();
  }

  function resetQualityWindow() {
    ringStart = 0;
    ringLen = 0;
    ringSum = 0;
  }

  function pushFrameMs(ms) {
    if (!(ms > 0)) return;
    if (ringLen === frameRing.length) {
      ringSum -= frameRing[ringStart];
      ringStart = (ringStart + 1) % frameRing.length;
      ringLen -= 1;
    }
    frameRing[(ringStart + ringLen) % frameRing.length] = ms;
    ringLen += 1;
    ringSum += ms;
    // Keep the window close to 2 s: drop the oldest sample while the rest still covers it.
    while (ringLen > 1 && ringSum - frameRing[ringStart] >= FPS_WINDOW_MS) {
      ringSum -= frameRing[ringStart];
      ringStart = (ringStart + 1) % frameRing.length;
      ringLen -= 1;
    }
  }

  function windowFps() {
    return ringSum > 0 ? (ringLen * 1000) / ringSum : 0;
  }

  function stepQuality(direction) {
    const next = notch + direction;
    if (next < 0 || next >= ladder.length) return;
    notch = next;
    stage = ladder[notch];
    applyStage();
    resetQualityWindow();
    headroomMs = 0;
    qualityCooldown = STEP_COOLDOWN_MS;
  }

  // One notch down under 57 fps, one notch back up after 10 s above 59 fps. Between steps a
  // cooldown keeps the ladder from oscillating, which would read as flicker on screen.
  function trackQuality(frameMs) {
    if (bootAgeMs < BENCH_SETTLE_MS) {
      // The install burst (shader compiles, buffer uploads, the first shadow map) is not the
      // machine's steady state. Measured on a 4x throttled phone profile: the ladder dropped to
      // notch 1 on a 50.8 fps first reading while steady state was 180 fps at notch 0.
      bootAgeMs += frameMs;
      resetQualityWindow();
      headroomMs = 0;
      return;
    }
    if (frameMs > 1000) {
      // A hidden tab or an OS stall is not a rendering cost, so it must never walk the
      // ladder down: drop the stale window and start measuring again.
      resetQualityWindow();
      headroomMs = 0;
      return;
    }
    pushFrameMs(frameMs);
    if (benchRunning) return;
    if (qualityCooldown > 0) {
      qualityCooldown -= frameMs;
      if (qualityCooldown < 0) qualityCooldown = 0;
    }
    if (ringSum < FPS_WINDOW_MS * 0.75) return;
    if (qualityCooldown > 0) return;
    const fps = windowFps();
    if (fps < FPS_DROP) {
      stepQuality(1);
      return;
    }
    if (fps > FPS_HEADROOM) {
      headroomMs += frameMs;
      if (headroomMs >= HEADROOM_MS) {
        headroomMs = 0;
        stepQuality(-1);
      }
    } else {
      headroomMs = 0;
    }
  }

  // ---------------- benchmark ----------------
  // A fixed 20 s script over the existing rig and dream APIs. Frame times come from
  // performance.now() deltas, so the numbers do not depend on the animation loop's own
  // timestamp precision. The first 2 s are warmup and are excluded from every figure.
  function runBench() {
    if (benchRunning) return benchPromise;
    benchRunning = true;
    benchStartAt = 0;
    benchPhase = 0;
    benchCount = 0;
    benchSum = 0;
    resetQualityWindow();
    headroomMs = 0;
    api.bench = null;
    benchPromise = new win.Promise(function (resolve) {
      benchResolve = resolve;
    });
    return benchPromise;
  }

  function runBenchPhase(index) {
    if (index === 0) {
      if (rig) rig.frameDistrict();
      return;
    }
    if (index === 1 || index === 2) {
      if (rig) rig.rotate(1);
      return;
    }
    if (index === 3) {
      if (rig) rig.setTier('street', true);
      return;
    }
    if (index === 4) {
      if (dream) dream.sleep();
      return;
    }
    if (index === 5) {
      if (dream) dream.wake();
      return;
    }
    if (rig) rig.frameDistrict();
  }

  function benchFrame(stamp, frameMs) {
    if (!benchRunning) return;
    if (benchStartAt === 0) {
      if (!rig || !model) return;
      benchStartAt = stamp;
      return;
    }
    const elapsed = stamp - benchStartAt;
    while (benchPhase < BENCH_PHASE_AT.length && elapsed >= BENCH_PHASE_AT[benchPhase]) {
      runBenchPhase(benchPhase);
      benchPhase += 1;
    }
    if (elapsed >= BENCH_WARMUP_MS && benchCount < benchFrames.length) {
      benchFrames[benchCount] = frameMs;
      benchCount += 1;
      benchSum += frameMs;
    }
    if (elapsed >= BENCH_TOTAL_MS) finishBench();
  }

  function finishBench() {
    benchRunning = false;
    const count = benchCount;
    const info = renderer.info.render;
    const result = {
      avgFps: 0,
      p1LowFps: 0,
      maxFrameMs: 0,
      calls: info.calls,
      triangles: info.triangles,
      mode: mode
    };
    if (count > 1) {
      const sorted = [];
      for (let i = 0; i < count; i += 1) sorted.push(benchFrames[i]);
      sorted.sort(function (a, b) {
        return a - b;
      });
      // The 1 percent low is the rate implied by the 99th percentile frame DURATION: the
      // slowest one percent of the measured frames. Reading the 1st percentile duration
      // instead would report a fast frame and hide the stall this number exists to catch.
      const p99 = sorted[Math.min(count - 1, Math.ceil(count * 0.99) - 1)] || 1;
      const mean = benchSum / count;
      result.avgFps = Math.round((1000 / mean) * 10) / 10;
      result.p1LowFps = Math.round((1000 / p99) * 10) / 10;
      result.maxFrameMs = Math.round(sorted[count - 1] * 10) / 10;
    }
    if (win.console && typeof win.console.log === 'function') {
      win.console.log('bench ' + JSON.stringify(result));
    }
    api.bench = result;
    const done = benchResolve;
    benchResolve = null;
    benchPromise = null;
    if (done) done(result);
    // Leave the district as found: a run that ended mid-dream wakes the city again.
    if (dream && dream.state === 'dreaming') dream.wake();
  }

  // ---------------- teardown ----------------
  function disposeMaterial(material) {
    if (!material) return;
    for (let i = 0; i < TEXTURE_KEYS.length; i += 1) {
      const texture = material[TEXTURE_KEYS[i]];
      if (texture && typeof texture.dispose === 'function') texture.dispose();
    }
    if (typeof material.dispose === 'function') material.dispose();
  }

  function disposeGroup(group) {
    if (!group) return;
    if (group.parent) group.parent.remove(group);
    scene.remove(group);
    group.traverse(function (object) {
      const tag = object.userData;
      // A mesh built from the asset cache borrows its geometry, its material and the pack
      // palette texture, which stay alive for the next install. Only the instance attributes
      // (instanceMatrix, instanceColor) belong to the mesh, and mesh.dispose() releases those
      // and nothing else, so the shared objects must not be freed here.
      const shared = !!(tag && tag.sharedGeometry === true);
      if (object.isInstancedMesh && typeof object.dispose === 'function') object.dispose();
      if (shared) return;
      if (object.geometry && typeof object.geometry.dispose === 'function') object.geometry.dispose();
      const material = object.material;
      if (!material) return;
      if (Array.isArray(material)) {
        for (let i = 0; i < material.length; i += 1) disposeMaterial(material[i]);
      } else {
        disposeMaterial(material);
      }
    });
  }

  // ---------------- labels ----------------
  function invalidateLabelMeasure() {
    labelMeasureDirty = true;
  }

  function rebuildLabels() {
    labels.length = 0;
    if (labelsRoot) {
      while (labelsRoot.firstChild) labelsRoot.removeChild(labelsRoot.firstChild);
    }
    if (!model || !labelsRoot) {
      acceptedBoxes = new Float32Array(0);
      return;
    }
    for (let i = 0; i < model.quarters.length; i += 1) {
      const quarter = model.quarters[i];
      const el = doc.createElement('div');
      el.className = 'label';
      el.style.setProperty('--q', hexColor(quarter.color));
      el.textContent = quarter.name;
      el.style.display = 'none';
      labelsRoot.appendChild(el);
      labels.push({
        el: el,
        anchor: findWorldQuarter(quarter.idx),
        px: -100000,
        py: -100000,
        // bw is even and bh is an integer, both measured once per viewport: half of an even
        // width is an integer, so the centred box and its origin are whole pixels.
        bw: 0,
        bh: 0,
        // The integer translate last written, or NaN before the first write.
        tx: NaN,
        ty: NaN,
        w: 0,
        h: 0,
        measured: false,
        shown: false,
        hidden: true,
        domOn: false
      });
    }
    acceptedBoxes = new Float32Array(labels.length * 4);
    acceptedCount = 0;
    measuredYaw = NaN;
    measuredW = -1;
    measuredH = -1;
    labelMeasureDirty = true;
  }

  function findWorldQuarter(idx) {
    if (!world || !world.quarters) return null;
    for (let i = 0; i < world.quarters.length; i += 1) {
      if (world.quarters[i].idx === idx) return world.quarters[i];
    }
    return null;
  }

  // Per frame: project and place on whole CSS pixels. Nothing here reads layout, nothing here
  // allocates, and nothing is written unless the integer origin actually moved: a still camera
  // and a still scene leave every label untouched, so no string is built at all.
  //
  // The origin is plain integer arithmetic. The box's top-left is
  // (px - bw / 2, py - bh), where px and py are the rounded projection, bw is the even integer
  // width and bh the integer height, so the box spans whole pixels and its bottom edge still
  // hangs on the projected anchor. The old translate(-50%,-100%) shifted by half of a
  // fractional text width, which is exactly what put 692.890625 on screen.
  function updateLabels() {
    if (!rig || labels.length === 0) return;
    const camera = rig.camera;
    const width = win.innerWidth;
    const height = win.innerHeight;
    // The overlay container may sit inside the safe area; subtracting its corner keeps the
    // label on the same page pixel the projection named. The inset is a whole number of CSS
    // pixels, so the difference stays whole.
    const originX = labelOriginX;
    const originY = labelOriginY;
    for (let i = 0; i < labels.length; i += 1) {
      const label = labels[i];
      if (!label.anchor || !label.anchor.labelPosition) {
        label.shown = false;
        continue;
      }
      probe.copy(label.anchor.labelPosition).project(camera);
      const behind = probe.z > 1;
      let px = 0;
      let py = 0;
      if (!behind) {
        px = Math.round((probe.x * 0.5 + 0.5) * width);
        py = Math.round((-probe.y * 0.5 + 0.5) * height);
      }
      const inExtents = !behind && px > -200 && px < width + 200 && py > -200 && py < height + 200;
      label.shown = inExtents;
      if (!inExtents) continue;
      label.px = px;
      label.py = py;
      const tx = px - (label.bw >> 1) - originX;
      const ty = py - label.bh - originY;
      if (tx === label.tx && ty === label.ty) continue;
      label.tx = tx;
      label.ty = ty;
      label.el.style.transform = 'translate(' + tx + 'px, ' + ty + 'px)';
    }
    applyLabelVisibility();
  }

  function applyLabelVisibility() {
    for (let i = 0; i < labels.length; i += 1) {
      const label = labels[i];
      const on = label.shown && !label.hidden;
      if (on !== label.domOn) {
        label.el.style.display = on ? '' : 'none';
        label.domOn = on;
      }
    }
  }

  // Measuring forces layout, so it happens only on the slow timer and only when the
  // viewport, the label set or the camera orientation changed. All labels are shown for
  // the measurement pass and the visibility decision lands before the browser paints.
  //
  // This is also where the box becomes whole pixels. The natural width and height are read
  // first, with both pins cleared (a pinned box would only measure itself back), then the box
  // is pinned to the next even integer width and to the next integer height. Even matters: the
  // label is centred above its anchor, and half of an odd width is a half pixel. The pins are
  // re-applied on every pass; this runs on a timer, never in the render loop.
  function measureLabels() {
    const share = PORTRAIT_PHONE && PORTRAIT_PHONE.matches ? LABEL_CAP_PHONE : LABEL_CAP_DESKTOP;
    let cap = Math.floor(win.innerWidth * share);
    cap -= cap % 2;
    for (let i = 0; i < labels.length; i += 1) {
      const el = labels[i].el;
      // The natural box is only readable with the pins off: a pinned box measures itself.
      el.style.width = '';
      el.style.height = '';
      el.style.display = '';
    }
    for (let i = 0; i < labels.length; i += 1) {
      const label = labels[i];
      const rect = label.el.getBoundingClientRect();
      let w = Math.ceil(rect.width);
      w += w % 2;
      if (w > cap) w = cap;
      const h = Math.ceil(rect.height);
      // Both pins go back on every pass, because the pass starts by clearing them; this runs
      // on the slow timer, never in the render loop. When a pin changes, the box origin moves
      // with it and the next frame rewrites that label's transform.
      if (w > 0) {
        label.bw = w;
        label.el.style.width = w + 'px';
      }
      if (h > 0) {
        label.bh = h;
        label.el.style.height = h + 'px';
      }
      label.w = label.bw;
      label.h = label.bh;
      label.measured = label.bw > 0 && label.bh > 0;
      label.domOn = true;
    }
    labelMeasureDirty = false;
  }

  // Rank based declutter. Labels arrive in quarter rank order, so the highest-ranked box
  // is accepted first and any lower-ranked box that lands within 6 px of it, of a control
  // or of the screen edge is hidden.
  function declutterPass() {
    if (!rig || !labelsRoot || labels.length === 0) return;
    const camera = rig.camera;
    const root = labelsRoot.getBoundingClientRect();
    const originX = root.left;
    const originY = root.top;
    const viewW = root.width;
    const viewH = root.height;
    if (!(viewW > 0 && viewH > 0)) return;

    if (originX !== labelOriginX || originY !== labelOriginY) {
      labelOriginX = originX;
      labelOriginY = originY;
    }

    const yaw = camera.quaternion.y;
    if (labelMeasureDirty || Math.abs(yaw - measuredYaw) > 0.0001 || viewW !== measuredW || viewH !== measuredH) {
      measureLabels();
      measuredYaw = yaw;
      measuredW = viewW;
      measuredH = viewH;
    }

    avoidCount = 0;
    for (let i = 0; i < avoidEls.length && avoidCount < 4; i += 1) {
      const rect = avoidEls[i].getBoundingClientRect();
      const at = avoidCount * 4;
      avoidBoxes[at] = rect.left - LABEL_MARGIN;
      avoidBoxes[at + 1] = rect.top - LABEL_MARGIN;
      avoidBoxes[at + 2] = rect.right + LABEL_MARGIN;
      avoidBoxes[at + 3] = rect.bottom + LABEL_MARGIN;
      avoidCount += 1;
    }

    acceptedCount = 0;
    for (let i = 0; i < labels.length; i += 1) {
      const label = labels[i];
      label.hidden = true;
      if (!label.anchor || !label.measured || !label.shown) continue;
      const halfW = label.w * 0.5;
      const left = label.px - halfW;
      const right = label.px + halfW;
      const top = label.py - label.h;
      const bottom = label.py;
      if (left < originX + LABEL_MARGIN) continue;
      if (right > originX + viewW - LABEL_MARGIN) continue;
      if (top < originY + LABEL_MARGIN) continue;
      if (bottom > originY + viewH - LABEL_MARGIN) continue;
      let blocked = false;
      for (let b = 0; b < avoidCount; b += 1) {
        const at = b * 4;
        if (left < avoidBoxes[at + 2] && right > avoidBoxes[at] && top < avoidBoxes[at + 3] && bottom > avoidBoxes[at + 1]) {
          blocked = true;
          break;
        }
      }
      if (blocked) continue;
      for (let a = 0; a < acceptedCount; a += 1) {
        const at = a * 4;
        if (
          left < acceptedBoxes[at + 2] + LABEL_MARGIN &&
          right > acceptedBoxes[at] - LABEL_MARGIN &&
          top < acceptedBoxes[at + 3] + LABEL_MARGIN &&
          bottom > acceptedBoxes[at + 1] - LABEL_MARGIN
        ) {
          blocked = true;
          break;
        }
      }
      if (blocked) continue;
      const at = acceptedCount * 4;
      acceptedBoxes[at] = left;
      acceptedBoxes[at + 1] = top;
      acceptedBoxes[at + 2] = right;
      acceptedBoxes[at + 3] = bottom;
      acceptedCount += 1;
      label.hidden = false;
    }
    applyLabelVisibility();
  }

  // The slow timer keeps the layout reads out of the render loop entirely; the render loop
  // only applies the cached decision.
  function declutterTimer() {
    declutterPass();
  }

  // ---------------- building card ----------------
  function showCard(quarterIdx) {
    if (!cardEl || !model) return;
    const quarter = model.quarters[quarterIdx];
    if (!quarter) return;
    if (cardTitle) cardTitle.textContent = quarter.name;
    if (cardTime) cardTime.textContent = formatDuration(quarter.totalMs);
    if (cardPhrase) cardPhrase.textContent = quarter.lastPhrase || '';
    cardEl.hidden = false;
  }

  function hideCard() {
    if (cardEl && !cardEl.hidden) cardEl.hidden = true;
  }

  function handleTap(clientX, clientY) {
    if (!rig || !world || !world.buildingQuarter) return;
    const found = rig.raycast(clientX, clientY, world.pickTargets, hits);
    let quarterIdx = -1;
    if (found && found.length) {
      for (let i = 0; i < found.length; i += 1) {
        const hit = found[i];
        if (hit.instanceId === undefined || hit.instanceId === null) continue;
        // A model mesh carries its own instanceId -> quarter map; the procedural building
        // mesh is the one that reads the world's shared map.
        const meshTag = hit.object ? hit.object.userData : null;
        const map = meshTag && meshTag.quarterOf ? meshTag.quarterOf : world.buildingQuarter;
        const idx = map[hit.instanceId];
        if (idx >= 0) {
          quarterIdx = idx;
          break;
        }
      }
    }
    if (quarterIdx >= 0) showCard(quarterIdx);
    else hideCard();
  }

  // ---------------- clock ----------------
  // main owns the clock only. agents.mjs owns every world write that follows from it
  // (sun and street lamps, neglected quarters), so publishing the effective values on sim
  // is the single contract between the two, and the fast-forward really moves time.
  // A pinned ?hour= replaces the local clock; the dream's night override still wins.
  function applyClock() {
    wallHour = wallHourNow();
    night = wallHour < 7 || wallHour >= 19 || nightOverride;
    // The dream fast-forwards to night: the effective hour jumps to 22 and sim.night stays
    // true, so the agents light warm windows and sodium lamps instead of a daylight sun.
    sim.wallHour = nightOverride ? 22 : wallHour;
    sim.night = night;
  }

  function advanceClock(dt) {
    if (!model) return;
    tData += dt * 1000 * SPEED;
    if (!(tData >= model.t0)) tData = model.t0;
    const end = model.t1 + HOUR_MS;
    if (tData >= end) tData = model.t0;
  }

  function checkGaps() {
    if (!model || !dream) return;
    const gaps = model.gaps;
    if (!gaps || !gaps.length) return;
    let inside = -1;
    for (let i = 0; i < gaps.length; i += 1) {
      if (tData >= gaps[i].start && tData < gaps[i].end) {
        inside = i;
        break;
      }
    }
    if (inside === gapActive) return;
    const previous = gapActive;
    gapActive = inside;
    if (inside >= 0) {
      dream.notifyGap(gaps[inside].start, gaps[inside].end);
    } else if (previous >= 0 && dream.state === 'dreaming') {
      // The dream also notices the resumed data, but the gap is main's signal: wake through
      // the invoker dream.mjs registered, or directly if there is none. wake() is idempotent.
      if (typeof wakeHook === 'function') wakeHook();
      else dream.wake();
    }
  }

  // ---------------- distance grade ----------------
  // The composite's cool distance key is keyed on height in the frame, and that only stands
  // for depth at the district distance, where the rig frames the whole bounds sphere and the
  // upper frame really is the far city and the hills. At the block and street tiers the upper
  // frame holds geometry a couple of hundred metres off, and a full-strength key paints that
  // near city slate violet. The key is therefore weighted by the camera's distance to its
  // look-at: zero at or below the block distance, the authored amount at the district
  // distance, smooth in between.
  //
  // camera.mjs places the camera at zoomFactor * districtDist, with TIER_ZOOM [1, 0.62, 0.38]
  // for [district, block, street], and frames the district so the whole bounds sphere fits the
  // narrower half angle of the current aspect. The two ends of the ramp are therefore
  // DIST_GRADE_BLOCK_ZOOM and DIST_GRADE_FULL_ZOOM times that framing distance. A portrait
  // viewport keeps the vertical field of view but shrinks the horizontal one, so it frames from
  // farther out and the district tier lands past the top of the ramp, where the weight clamps
  // at 1 instead of overshooting.

  function framingSin(aspect) {
    // sin(min(halfV, halfH)) for the live field of view: the factor camera.mjs divides the
    // framing radius by, clamped to 0.02 as it does before dividing.
    const halfV = rig.camera.fov * (Math.PI / 360);
    const a = aspect > 0 ? aspect : 1;
    const halfH = Math.atan(Math.tan(halfV) * a);
    const s = Math.sin(halfV < halfH ? halfV : halfH);
    return s > 0.02 ? s : 0.02;
  }

  // Sampled while the rig is framed to the district, where the camera sits at exactly
  // districtDist from the look-at: one distance read recovers radius * DISTRICT_PAD, so this
  // file repeats neither constant and cannot drift from camera.mjs. That product does not
  // depend on the aspect, so a resize only moves the framing distance through framingSin.
  function sampleFramingDistance() {
    if (!rig) return;
    const d = rig.camera.position.distanceTo(rig.lookAt);
    if (d > 0) distGradeFraming = d * framingSin(rig.camera.aspect);
  }

  function updateDistanceGrade() {
    if (!post || !rig || !(distGradeFraming > 0)) return;
    const framing = distGradeFraming / framingSin(rig.camera.aspect);
    const distance = rig.camera.position.distanceTo(rig.lookAt);
    const block = framing * DIST_GRADE_BLOCK_ZOOM;
    let t = (distance - block) / (framing * (DIST_GRADE_FULL_ZOOM - DIST_GRADE_BLOCK_ZOOM));
    if (!(t > 0)) t = 0;
    else if (t > 1) t = 1;
    // Smoothstep, so the grade is exactly off through the block tier and exactly the authored
    // amount at the district distance, with no seam where the tiers meet.
    const k = t * t * (3 - 2 * t);
    let delta = k - distGradeSent;
    if (delta < 0) delta = -delta;
    if (delta <= DIST_GRADE_MIN_STEP) return;
    distGradeSent = k;
    post.setDistanceGrade(k);
  }

  // ---------------- installation ----------------
  function ensurePost() {
    if (post || !rig) return;
    post = createPost(THREE, renderer, scene, rig.camera, {
      enabled: profile.postEnabled && !postOff,
      msaa: profile.postMsaa,
      fxaa: profile.postFxaa
    });
    // Measurement affordance, not a product setting: navigating to #post=0 (or
    // back to #post=1) flips the chain in place without a reload, so a pair of
    // screenshots can share one sim clock and differ only by the chain. Nothing
    // else listens for the hash, and the UI never sets it.
    if (!postToggleWired && typeof win.addEventListener === 'function') {
      postToggleWired = true;
      win.addEventListener('hashchange', function () {
        if (post) post.enabled = win.location.hash.indexOf('post=0') < 0;
      });
    }
    post.setSize(win.innerWidth, win.innerHeight);
    // A fresh chain carries the authored distance amount, so the next frame has to push the
    // live weight again whatever the guard thinks it sent last.
    distGradeSent = -2;
    // ...and the same for the dream bloom, which a resize across the profile boundary would
    // otherwise drop on a chain that is recreated mid-dream.
    post.setDreamBloom(dreamBloomOn ? 1 : 0);
  }

  function recreatePost() {
    if (!post) {
      ensurePost();
      return;
    }
    post.dispose();
    post = null;
    ensurePost();
  }

  // The dream group is hidden until the first sleep, and renderer.compile only walks visible
  // objects, so without this its programs linked in the frame that first showed the chantier:
  // that first sleep measured 35.7 ms on the 4x throttled phone, against a 33 ms ceiling.
  // dream.warm() shows the group for the length of the compile and restores it, which keeps
  // that knowledge in dream.mjs; this side only owns the renderer.
  function warmDream() {
    if (!dream || !rig || typeof renderer.compile !== 'function') return;
    dream.warm(function () {
      renderer.compile(scene, rig.camera);
    });
  }

  // A fascia board and a barrier stripe are both read at a grazing angle at the street tier,
  // where trilinear filtering alone smears the lettering. Both atlases are mipmapped already;
  // this gives them the widest anisotropic sampling the device offers, which costs nothing at
  // build time and nothing per frame. The renderer lives here, so main owns it.
  function setAtlasAnisotropy() {
    if (!renderer || typeof renderer.capabilities === 'undefined') return;
    const max = renderer.capabilities.getMaxAnisotropy ? renderer.capabilities.getMaxAnisotropy() : 1;
    if (!(max > 1)) return;
    const signTexture = world && world.signs ? world.signs.texture : null;
    if (signTexture && signTexture.anisotropy < max) {
      signTexture.anisotropy = max;
      signTexture.needsUpdate = true;
    }
    const barrierMesh = dream && dream.group ? dream.group.getObjectByName('barriers') : null;
    const barrierMap = barrierMesh && barrierMesh.material ? barrierMesh.material.map : null;
    if (barrierMap && barrierMap.anisotropy < max) {
      barrierMap.anisotropy = max;
      barrierMap.needsUpdate = true;
    }
  }

  // ---------------- golden-hour photograph rig ----------------
  // Cycle 21: the day branch in world.mjs keeps one fixed late-afternoon SUN_DIR
  // (about 25 degrees elevation) and one shadow frustum for every daylight hour, and
  // its penumbra is locked to one shadow-map texel. The photograph brief wants a
  // lower sun, longer and softer shadows, a warmer key and a cooler fill. world.mjs
  // is frozen for this cycle but exposes the live lights, so the day branch is
  // re-aimed here after it runs, on every hour-bucket change agents.mjs drives
  // through stepWorldTime. Night passes through untouched, so the night palette and
  // the dream frames never see these overrides. The sun's azimuth is kept: only the
  // elevation drops, so the haze band (keyed on the horizontal direction) and every
  // ground-plane shadow direction stay where they were.
  const GOLDEN_DIR = new THREE.Vector3(0.8983, 0.2588, 0.3626).normalize();
  const GOLDEN_SUN_DIST = 460;   // world.mjs SUN_DIST, the light rides the same orbit
  const GOLDEN_TARGET_Y = 10;    // world.mjs SUN_TARGET (0, 10, 0)
  const GOLDEN_FRUSTUM = 230;    // was 178: a 15 degree sun lengthens every shadow
  const GOLDEN_SUN_COLOR = new THREE.Color(0xffbe70);
  const GOLDEN_HEMI_COLOR = new THREE.Color(0xcdd6e0);
  const GOLDEN_HEMI_GROUND = new THREE.Color(0x8a8478);
  const GOLDEN_SUN_INTENSITY = 6.2;
  const GOLDEN_HEMI_INTENSITY = 0.88;
  const GOLDEN_SHADOW_RADIUS = 5; // shadow-map texels, 0.225 m each at the wider frustum

  // The sky dome is a ShaderMaterial the world builds behind its own closure, and its
  // uSunDir drives both the sun disc and the haze band's azimuth. Find it once per
  // install by shape: the only ShaderMaterial carrying uSunDir next to uTop/uHorizon.
  function findSkySunDirUniform(scene) {
    let found = null;
    scene.traverse(function (obj) {
      if (found || !obj.material || !obj.material.uniforms) return;
      const u = obj.material.uniforms;
      if (u.uSunDir && u.uTop && u.uHorizon) found = u.uSunDir;
    });
    return found;
  }

  function wrapGoldenLights(world) {
    const lights = world.lights;
    if (!lights || !lights.sun || !lights.hemi || typeof lights.setTimeOfDay !== 'function') return;
    const sun = lights.sun;
    const hemi = lights.hemi;
    const original = lights.setTimeOfDay;
    const skySunDir = findSkySunDirUniform(scene);
    sun.shadow.radius = GOLDEN_SHADOW_RADIUS;
    const cam = sun.shadow.camera;
    cam.left = -GOLDEN_FRUSTUM;
    cam.right = GOLDEN_FRUSTUM;
    cam.top = GOLDEN_FRUSTUM;
    cam.bottom = -GOLDEN_FRUSTUM;
    cam.updateProjectionMatrix();
    lights.setTimeOfDay = function (hour, night) {
      original(hour, night);
      if (night) return;
      sun.position.copy(GOLDEN_DIR).multiplyScalar(GOLDEN_SUN_DIST);
      sun.position.y += GOLDEN_TARGET_Y;
      sun.color.copy(GOLDEN_SUN_COLOR);
      sun.intensity = GOLDEN_SUN_INTENSITY;
      hemi.color.copy(GOLDEN_HEMI_COLOR);
      hemi.groundColor.copy(GOLDEN_HEMI_GROUND);
      hemi.intensity = GOLDEN_HEMI_INTENSITY;
      if (skySunDir) skySunDir.value.copy(GOLDEN_DIR);
    };
  }

  function install(nextModel) {
    disposeGroup(world ? world.group : null);
    disposeGroup(agents ? agents.group : null);
    disposeGroup(dream ? dream.group : null);
    world = null;
    agents = null;
    dream = null;
    nightOverride = false;
    gapActive = -1;
    labels.length = 0;

    // `assets` is the one shared object from boot: a later install reuses it instead of
    // re-reading the packs, and null simply leaves the procedural world in place.
    world = buildWorld(THREE, nextModel, {
      shadows: profile.shadows,
      isTouch: isTouch,
      assets: assets
    });
    scene.add(world.group);
    wrapGoldenLights(world);

    agents = createAgents(THREE, world, nextModel, {
      maxVehicles: MAX_VEHICLES,
      maxPedestrians: MAX_PEDESTRIANS,
      // The one mobile decision, made in the profiles section and passed down: the population
      // caps and the render profile then cannot disagree about what this device is.
      mobile: isTouch,
      // The fleet draws its bodies from the same pack cache the world uses; a null cache
      // (no assets, or a phone that never loads the street tier) keeps the procedural car.
      assets: assets
    });
    scene.add(agents.group);

    if (!rig) {
      rig = createCameraRig(THREE, canvas, world.bounds, { isTouch: isTouch });
    }
    rig.onResize(win.innerWidth, win.innerHeight);
    rig.frameDistrict();
    // frameDistrict leaves the camera at exactly the district distance, which is the one moment
    // the framing distance can be read straight off the rig.
    sampleFramingDistance();
    rig.onTap(handleTap);

    dream = createDream(THREE, world, nextModel, agents, rig, ui);
    scene.add(dream.group);

    ensurePost();
    // The profile and the current notch are applied to whatever was just built, so loading
    // a second export never resets the adaptive quality the page had settled on.
    appliedShadow = -1;
    appliedDetail = -1;
    appliedPopulation = -1;
    applyStage();
    // A fresh build compiles new programs and uploads new buffers, so the ladder gets a fresh
    // settle window instead of reading that burst as a slow machine.
    bootAgeMs = 0;

    model = nextModel;
    tData = model.t0;
    rebuildLabels();
    hideCard();
    applyClock();
    sim.tData = tData;
    sim.dreaming = false;
    sim.dt = 0;

    api.model = model;
    api.world = world;
    api.rig = rig;

    // One synchronous frame so the page is never blank while waiting for the loop.
    setAtlasAnisotropy();
    renderer.info.reset();
    updateDistanceGrade();
    if (post) post.render();
    else renderer.render(scene, rig.camera);
    updateLabels();
    declutterPass();
    firstFrameAt = nowStamp();
    // Then the dream compile, deferred so it lands after the first frame and inside the
    // loading window that the bench and the quality ladder already exclude.
    win.setTimeout(warmDream, 0);
  }

  // ---------------- FPS readout ----------------
  // Writes only when the composed text would differ: five integers are compared first, so a
  // steady viewport with the readout open costs no string and no DOM write per tick. When the
  // text does change it is written into the existing text node, so the readout replaces no
  // node and the node count cannot drift while it is open.
  function writeFps() {
    if (!fpsEl) return;
    const info = renderer.info.render;
    const counts = agents ? agents.counts : null;
    const fps = Math.round(fpsValue);
    const calls = info.calls;
    const tris = info.triangles;
    const vehicles = counts ? counts.vehicles : 0;
    const peds = counts ? counts.pedestrians : 0;
    if (
      fpsWrote &&
      fps === fpsShownFps &&
      calls === fpsShownCalls &&
      tris === fpsShownTris &&
      vehicles === fpsShownVehicles &&
      peds === fpsShownPeds
    ) {
      return;
    }
    fpsWrote = true;
    fpsShownFps = fps;
    fpsShownCalls = calls;
    fpsShownTris = tris;
    fpsShownVehicles = vehicles;
    fpsShownPeds = peds;
    const text = fps + ' fps / ' + calls + ' calls / ' + tris + ' tris / ' + vehicles + ' vehicles / ' + peds + ' peds';
    const node = fpsEl.firstChild;
    if (node && node.nodeType === 3) node.nodeValue = text;
    else fpsEl.textContent = text;
  }

  function toggleFps() {
    fpsVisible = !fpsVisible;
    if (!fpsEl) return;
    fpsEl.hidden = !fpsVisible;
    if (fpsVisible) writeFps();
  }

  function trackFps(dt) {
    frames += 1;
    fpsClock += dt;
    if (fpsClock >= 0.25) {
      fpsValue = frames / fpsClock;
      frames = 0;
      fpsClock = 0;
      if (fpsVisible) writeFps();
    }
  }

  // ---------------- frame ----------------
  function tick() {
    const stamp = nowStamp();
    let frameMs = lastFrameStamp ? stamp - lastFrameStamp : 0;
    lastFrameStamp = stamp;
    if (!(frameMs > 0)) frameMs = 0;
    let dt = frameMs / 1000;
    if (dt > 0.1) dt = 0.1;

    benchFrame(stamp, frameMs);

    advanceClock(dt);
    applyClock();
    checkGaps();

    sim.tData = tData;
    sim.dreaming = !!dream && (dream.state === 'dreaming' || dream.state === 'waking');
    sim.dt = dt;
    // The dream owns its own transitions, so main only mirrors the arrival and the departure of
    // that state into the post chain, once per transition: the dream bloom is what carries the
    // chantier's light across the district instead of leaving it sitting on the ribbon.
    if (post && typeof post.setDreamBloom === 'function' && dreamBloomOn !== sim.dreaming) {
      dreamBloomOn = sim.dreaming;
      post.setDreamBloom(dreamBloomOn ? 1 : 0);
    }

    if (rig) rig.update(dt);
    if (agents) agents.update(dt, sim);
    if (dream) dream.update(dt, sim);

    renderer.info.reset();
    updateDistanceGrade();
    if (post) post.render();
    else if (rig) renderer.render(scene, rig.camera);

    updateLabels();
    trackQuality(frameMs);
    trackFps(dt);
  }

  // ---------------- input wiring ----------------
  // Q and E are bound by the camera rig itself (src/camera.mjs binds them on window,
  // snapping 90 degrees with a 400 ms tween). Main only owns the chevrons, F and Escape.
  function onKeyDown(event) {
    if (!event || event.ctrlKey || event.metaKey || event.altKey || event.repeat) return;
    const target = event.target;
    if (target) {
      const tag = typeof target.tagName === 'string' ? target.tagName : '';
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || target.isContentEditable) return;
    }
    const key = event.key;
    if (key === 'f' || key === 'F') {
      toggleFps();
    } else if (key === 'Escape') {
      hideCard();
    }
  }

  function onResize() {
    const width = win.innerWidth;
    const height = win.innerHeight;
    // The chevrons read their centre from --chev-top, so it is refreshed before anything else
    // touches the canvas: one integer write per resize, nothing per frame.
    placeChrome();
    const nextMode = detectMode();
    if (nextMode !== mode) {
      // Crossing the 800 px boundary (or gaining a coarse pointer) swaps the whole
      // profile: post target, FXAA, shadows, pixel ratio cap, population and detail.
      mode = nextMode;
      profile = profileFor(mode);
      ladder = buildLadder(profile, ratioCapFor(profile));
      notch = 0;
      stage = ladder[0];
      renderer.shadowMap.enabled = profile.shadows;
      // The new pixel ratio lands on the renderer first, so the recreated post allocates
      // its targets at the right size instead of being resized straight afterwards.
      applyViewport();
      recreatePost();
      applyStage();
      resetQualityWindow();
      headroomMs = 0;
      qualityCooldown = 0;
    } else {
      const cap = ratioCapFor(profile);
      if (cap !== ladder[0].pixelRatio) {
        // Same profile, different device pixel ratio (monitor move or browser zoom): the
        // ladder is rebuilt around the new cap and the notch index is preserved.
        ladder = buildLadder(profile, cap);
        if (notch >= ladder.length) notch = ladder.length - 1;
        stage = ladder[notch];
        applyStage();
      } else {
        applyViewport();
      }
    }
    if (rig) {
      rig.onResize(width, height);
      if (rig.tier === 'district') rig.frameDistrict();
    }
    invalidateLabelMeasure();
    declutterPass();
  }

  function showDrop(on) {
    if (dropEl) dropEl.hidden = !on;
  }

  function loadFromText(text) {
    let parsed = null;
    try {
      parsed = JSON.parse(text);
    } catch (err) {
      return false;
    }
    try {
      loadExport(parsed);
      return true;
    } catch (err) {
      return false;
    }
  }

  function readFile(file) {
    if (!file || typeof file.text !== 'function') return;
    file.text().then(function (text) {
      loadFromText(text);
    }).catch(function () {
      // an unreadable file leaves the current district untouched
    });
  }

  // The two chevrons centre on the viewport through ONE custom property, written here and on
  // resize only. 50% of an odd viewport height is a half pixel, and translateY(-50%) then
  // leaves the box at y 433.5 on a 412x915 phone; an integer top cannot. The control height
  // is read once from layout (44 px, or 48 px in the portrait phone layout), so the portrait
  // breakpoint stays in the stylesheet and is never duplicated here.
  let chevTop = -1;

  function placeChrome() {
    const rotLeft = doc.getElementById('rot-left');
    const rotRight = doc.getElementById('rot-right');
    let box = rotLeft ? rotLeft.offsetHeight : 0;
    if (!(box > 0) && rotRight) box = rotRight.offsetHeight;
    if (!(box > 0)) box = 44;
    const top = Math.max(0, Math.round((win.innerHeight - box) / 2));
    if (top === chevTop) return;
    chevTop = top;
    doc.documentElement.style.setProperty('--chev-top', top + 'px');
  }

  function wireInput() {
    const rotLeft = doc.getElementById('rot-left');
    const rotRight = doc.getElementById('rot-right');
    const sleepButton = doc.getElementById('sleep');
    const openFile = doc.getElementById('open-file');

    if (rotLeft) {
      rotLeft.addEventListener('click', function () {
        if (rig) rig.rotate(-1);
      });
    }
    if (rotRight) {
      rotRight.addEventListener('click', function () {
        if (rig) rig.rotate(1);
      });
    }
    if (sleepButton) {
      sleepButton.addEventListener('click', function () {
        if (dream) dream.toggle();
      });
    }
    if (openFile && fileInput) {
      openFile.addEventListener('click', function () {
        fileInput.click();
      });
    }
    if (fileInput) {
      fileInput.addEventListener('change', function () {
        const files = fileInput.files;
        if (files && files.length) readFile(files[0]);
        fileInput.value = '';
      });
    }

    win.addEventListener('keydown', onKeyDown);
    win.addEventListener('resize', onResize);
    win.addEventListener('orientationchange', onResize);

    win.addEventListener('dragover', function (event) {
      event.preventDefault();
      if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
      showDrop(true);
    });
    win.addEventListener('dragleave', function (event) {
      if (!event.relatedTarget) showDrop(false);
    });
    win.addEventListener('drop', function (event) {
      event.preventDefault();
      showDrop(false);
      const transfer = event.dataTransfer;
      if (!transfer || !transfer.files || !transfer.files.length) return;
      readFile(transfer.files[0]);
    });
    win.addEventListener('dragend', function () {
      showDrop(false);
    });
  }

  // ---------------- asset packs ----------------
  // assets.mjs owns the fetch, the GLB parse and the per-pack material; main owns the one
  // loader instance, the deadlines and the merge. load() never throws and resolves null on
  // any failure (including its own timeout), so the procedural world always has a fallback.
  function ensureAssetLoader() {
    if (assetLoader) return assetLoader;
    if (typeof createAssetLoader !== 'function' || typeof GLTFLoader !== 'function') return null;
    try {
      assetLoader = createAssetLoader(THREE, GLTFLoader);
    } catch (err) {
      assetLoader = null;
    }
    return assetLoader;
  }

  // The street tier arrives as a second, independent Assets object. Its models join the
  // district set so one object serves every placement; the district geometry and material
  // stay as they are, because the models are distinct entries.
  function mergeModels(into, extra) {
    if (!into || !into.models || !extra || !extra.models) return;
    extra.models.forEach(function (assetModel, name) {
      if (!into.models.has(name)) into.models.set(name, assetModel);
    });
  }

  function loadAssets() {
    if (assetsOff) return win.Promise.resolve(null);
    // A file:// page cannot fetch the packs, so it keeps the procedural build without spending
    // a request that the browser would only report as a failed load.
    if (win.location && win.location.protocol === 'file:') return win.Promise.resolve(null);
    const loader = ensureAssetLoader();
    if (!loader) return win.Promise.resolve(null);
    // The phone gets the shorter deadline: a missing pack must not hold the first frame past
    // the boot budget there.
    const mobile = detectMode() === 'mobile';
    const timeoutMs = mobile ? ASSET_TIMEOUT_MOBILE_MS : ASSET_TIMEOUT_MS;
    const districtLoad = loader.load({ tier: 'district', timeoutMs: timeoutMs });
    // The desktop asks for both tiers beside each other, never one after the other, so the
    // merged cache lands inside one deadline. The phone keeps the district set only (its
    // triangle ceiling leaves no room for street detail) unless ?assets=street asks for it.
    const streetLoad = (!mobile || streetRequested) ? loader.load({ tier: 'street', timeoutMs: timeoutMs }) : null;
    return districtLoad.then(function (district) {
      if (!district) return null;
      assetTiers = ['district'];
      if (!streetLoad) return district;
      return streetLoad.then(function (street) {
        if (street) {
          mergeModels(district, street);
          assetTiers = ['district', 'street'];
        }
        return district;
      });
    }).then(function (loaded) {
      if (!loaded) return null;
      assetReport.loaded = true;
      // `models` is a Map keyed by model name (src/CONTRACT.md: "Assets = { models: Map<name,
      // Model> ... }", built in assets.mjs as Promise.resolve(new Map()) and filled with
      // models.set), so the count is its size. A `.length` here reports undefined and the
      // summary silently claims zero models.
      assetReport.models = loaded.models ? loaded.models.size : 0;
      assetReport.tier = assetTiers ? assetTiers.slice() : [];
      return loaded;
    });
  }

  // ---------------- offline cache ----------------
  // Registered once, after the first frame is on screen: the worker install itself must never
  // sit in front of the first paint, and a second install (a dropped export) has nothing to add.
  let cacheRegistered = false;

  function registerCache() {
    if (cacheRegistered) return;
    cacheRegistered = true;
    if (typeof registerOfflineCache !== 'function') return;
    registerOfflineCache();
  }

  // ---------------- data loading ----------------
  async function fetchSample() {
    if (typeof win.fetch !== 'function' || typeof win.AbortController !== 'function') return null;
    // A file:// document cannot read a sibling file with fetch in Chromium: the request always
    // fails and the browser logs it as a network error. That is the documented fallback path, so
    // skip the doomed request and generate the day directly, keeping the console clean.
    if (win.location && win.location.protocol === 'file:') return null;
    const controller = new win.AbortController();
    const timer = win.setTimeout(function () {
      controller.abort();
    }, 1500);
    try {
      const response = await win.fetch('sample-export.json', { signal: controller.signal, cache: 'no-store' });
      if (!response || !response.ok) throw new Error('sample unavailable');
      const parsed = await response.json();
      if (!parsed || parsed.schema !== 'screensearch.export.v1') throw new Error('unexpected schema');
      return parsed;
    } catch (err) {
      // Served over HTTP but unreachable or malformed: the simulated day takes over silently.
      return null;
    } finally {
      win.clearTimeout(timer);
    }
  }

  function analyze(exportObject, tzOffsetMs) {
    if (typeof tzOffsetMs === 'number') return analyzeExport(exportObject, { tzOffsetMs: tzOffsetMs });
    return analyzeExport(exportObject);
  }

  function usable(candidate) {
    return !!(
      candidate &&
      candidate.frames &&
      candidate.frames.length > 0 &&
      candidate.quarters &&
      candidate.quarters.length > 0 &&
      candidate.t1 > candidate.t0
    );
  }

  function boot() {
    // The packs load beside the sample export, so the first frame install() renders
    // synchronously already carries the model instances. Both branches settle on their own
    // deadline (1500 ms sample, 8000 ms packs, 6000 ms on a phone) and both resolve instead of
    // rejecting, so this wait can only delay the first frame by the pack deadline, never hang.
    win.Promise.all([fetchSample(), loadAssets()]).then(function (settled) {
      const sample = settled[0];
      assets = settled[1];
      let next = null;
      if (sample) {
        try {
          // A real export was captured on this machine: the browser's local offset is the
          // right hour-of-day reading for its frames.
          next = analyze(sample, browserTzOffset());
        } catch (err) {
          next = null;
        }
      }
      if (!usable(next)) {
        // The built-in day is defined as 2026-11-05 08:00 to 23:00 at UTC+1, independent of
        // where the page happens to run.
        next = analyze(generateSimulatedDay(), SIM_DAY_TZ);
      }
      install(next);
      // A portrait phone opens at the block tier. The district framing is computed for the
      // bounds sphere and a 412x915 aspect, so the city becomes a band about a third of the
      // screen with the rest empty sky; block is one of the three legal tiers and fills the
      // portrait frame. The default is applied once, here, and nowhere else: install has
      // already framed the district and sampled the framing distance, and the resize path is
      // left alone, so a user who zooms afterwards keeps the tier they chose.
      if (profile.mode === 'mobile' && PORTRAIT_PHONE && PORTRAIT_PHONE.matches) {
        rig.setTier('block', false);
      }
      win.setInterval(declutterTimer, DECLUTTER_MS);
      renderer.setAnimationLoop(tick);
      // The first frame is already on screen (install rendered it synchronously), so the
      // offline cache worker registers here, never in front of the paint.
      registerCache();
      // The benchmark starts after the first frame, but not immediately: the first seconds
      // after install are the shader compile and upload burst (measured as a 177 ms frame when
      // the run began at once, against 11 ms for the same script started four seconds later).
      // A gate number has to describe the settled machine, so the auto run waits for it.
      if (benchRequested) win.setTimeout(runBench, BENCH_SETTLE_MS);
    });
  }

  // ---------------- debug API ----------------
  function stats() {
    const info = renderer.info.render;
    return {
      fps: Math.round(fpsValue * 10) / 10,
      calls: info.calls,
      triangles: info.triangles,
      vehicles: agents && agents.counts ? agents.counts.vehicles : 0,
      pedestrians: agents && agents.counts ? agents.counts.pedestrians : 0,
      shadows: !!(stage && stage.shadowMap > 0),
      // Read-only dream phase, the same string state() reports. A frame cannot be labelled
      // 'dreaming' from outside without it: the .dream-card buttons only exist in 'offered',
      // which is the end of a dream, not the dream.
      dream: dream ? dream.state : 'awake'
    };
  }

  function state() {
    return {
      tData: tData,
      t0: model ? model.t0 : 0,
      t1: model ? model.t1 : 0,
      wallHour: wallHour,
      night: night,
      nightOverride: nightOverride,
      dream: dream ? dream.state : 'awake',
      tier: rig ? rig.tier : 'district',
      quarters: model ? model.quarters.length : 0,
      frames: model ? model.frames.length : 0,
      gap: gapActive,
      labels: labels.length,
      firstFrameAt: firstFrameAt
    };
  }

  function quality() {
    return {
      notch: notch,
      pixelRatio: stage ? stage.pixelRatio : 1,
      postScale: stage ? stage.postScale : 1,
      shadowMap: stage ? stage.shadowMap : 0,
      population: stage ? stage.population : 1,
      mode: mode
    };
  }

  function debugFraming() {
    const result = { minX: 0, minY: 0, maxX: 0, maxY: 0, width: 0, height: 0 };
    if (!rig || !world || !world.roads || !world.roads.ring) return result;
    const ring = world.roads.ring;
    const camera = rig.camera;
    const width = win.innerWidth;
    const height = win.innerHeight;
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (let i = 0; i <= RING_SAMPLES; i += 1) {
      const u = i / RING_SAMPLES;
      if (typeof ring.getPointAt === 'function') ring.getPointAt(u, probe);
      else ring.getPoint(u, probe);
      probe.project(camera);
      const px = (probe.x * 0.5 + 0.5) * width;
      const py = (-probe.y * 0.5 + 0.5) * height;
      if (px < minX) minX = px;
      if (px > maxX) maxX = px;
      if (py < minY) minY = py;
      if (py > maxY) maxY = py;
    }
    if (minX === Infinity) return result;
    result.minX = Math.round(minX);
    result.minY = Math.round(minY);
    result.maxX = Math.round(maxX);
    result.maxY = Math.round(maxY);
    result.width = result.maxX - result.minX;
    result.height = result.maxY - result.minY;
    return result;
  }

  function loadExport(exportObject) {
    if (!exportObject || typeof exportObject !== 'object') {
      throw new Error('localtime: export must be an object');
    }
    if (exportObject.schema !== 'screensearch.export.v1') {
      throw new Error('localtime: unsupported schema ' + String(exportObject.schema));
    }
    const next = analyzeExport(exportObject, { tzOffsetMs: browserTzOffset() });
    if (!usable(next)) {
      throw new Error('localtime: export has no usable frames');
    }
    install(next);
    return {
      ok: true,
      schema: exportObject.schema,
      frames: next.frames.length,
      quarters: next.quarters.length,
      names: next.quarters.map(function (quarter) {
        return quarter.name;
      }),
      t0: next.t0,
      t1: next.t1,
      tzOffsetMs: next.tzOffsetMs
    };
  }

  const api = {
    stats: stats,
    state: state,
    loadExport: loadExport,
    model: null,
    world: null,
    rig: null,
    debugFraming: debugFraming,
    bench: null,
    runBench: runBench,
    quality: quality,
    // Live report of the pack cache: loaded stays false when the manifest is missing, when
    // ?noassets=1 forced the fallback, or when the loader timed out. tier lists what merged.
    assets: assetReport
  };
  window.localtime = api;

  // ---------------- start ----------------
  cacheAvoidTargets();
  // Before the first paint: the chevrons must never be seen at a half-pixel origin.
  placeChrome();
  wireInput();
  boot();
}

main();
