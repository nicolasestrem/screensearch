export function createAgents(THREE, world, model, opts) {
  const options = opts || {};
  const MAX_VEH = options.maxVehicles === undefined ? 420 : Math.max(16, options.maxVehicles | 0);
  const MAX_PED = options.maxPedestrians === undefined ? 700 : Math.max(0, options.maxPedestrians | 0);
  // The one shared pack cache main hands to world as settings.assets and to this file here.
  // Null is the normal case for ?noassets=1 and for the phone, which merges the district tier
  // only while the car kit is street tier; every substitution below then keeps the exact
  // procedural fleet, so the fallback and the phone frames do not change by one triangle.
  const assets = options.assets && typeof options.assets.makeInstanced === 'function'
    ? options.assets : null;

  // The population caps follow the ONE mobile decision main already made for this page: it
  // resolves `coarse pointer || touch screen` once and passes it here as `mobile`. A narrow
  // window with no touch therefore keeps the desktop caps, which is the whole point of the
  // shared flag: cycle 19 shipped a width test here and a width plus touch test in main, so a
  // 700 px desktop window got the phone's render profile with a desk's population. A caller
  // that omits the option gets the desktop caps, matching the desktop profile main would have
  // chosen on a device with no touch.
  const MOBILE_CAPS = options.mobile === true;
  const VEH_BUDGET = MOBILE_CAPS ? Math.max(16, MAX_VEH >> 1) : MAX_VEH;
  const PED_BUDGET = MOBILE_CAPS ? Math.max(1, MAX_PED >> 1) : MAX_PED;

  // The options are ceilings. Peak traffic is about 420 vehicles split between the ring,
  // the avenue, the spurs and the expressway stream, plus the delivery vans and two buses.
  // Every pool is sized once here: nothing below ever reallocates a buffer.
  // Delivery vans and the farm movers share one instanced pool. The first VAN_DELIVERY slots
  // are the parcel and sign vans; the last FARM_COUNT belong to the hillside tracks, so
  // dispatchVan can never take one and the whole addition stays inside the existing draw call.
  const VAN_DELIVERY = MOBILE_CAPS ? 6 : 12;
  const FARM_COUNT = 5;
  const VAN_CAP = VAN_DELIVERY + FARM_COUNT;
  const FARM_BASE = VAN_DELIVERY;
  const BUS_COUNT = 2;
  // The mobile frame is the one under the 70 call, 180k triangle gate, and its night frame is
  // the expensive one: every car slot also carries six lamp plates in the light pool while the
  // lamps are on. These two trims are mobile only, applied after the halving rule:
  // they cut the phone pools to 200 pedestrians and 100 car slots (from 350 and 202), which is
  // about 19.2k triangles off the phone night frame, and leave every desktop pool untouched.
  // 200/100 is still more pedestrians and fewer cars than the 150/142 the phone carried before
  // the desktop raise, so the phone reads as a crowd rather than as traffic.
  const TOUCH_CAR_MAX = 100;
  const TOUCH_PED_MAX = 200;
  const DESKTOP_CAR_CAP = Math.max(24, VEH_BUDGET - VAN_DELIVERY - BUS_COUNT);
  const CAR_CAP = MOBILE_CAPS
    ? Math.min(DESKTOP_CAR_CAP, TOUCH_CAR_MAX)
    : DESKTOP_CAR_CAP;
  const PED_CAP = MOBILE_CAPS
    ? Math.min(PED_BUDGET, TOUCH_PED_MAX)
    : PED_BUDGET;
  const PARK_CAP = MOBILE_CAPS ? 64 : 190;
  // Kerbside fill by road family. The ring keeps about a fifth of its bays empty so the
  // arterial never reads as one continuous rank; the spurs, the secondary streets that leave
  // the frame, fill denser on desktop because "parked cars along kerbs" is in the brief and the
  // desktop frame has the headroom. The mobile profile keeps the older rate, so the phone
  // pays for no car it did not already carry.
  const PARK_SKIP_RING = 0.22;
  const PARK_SKIP_SPUR = MOBILE_CAPS ? 0.34 : 0.14;
  const RAIL_CAP = 16;
  const CLOUD_CAP = 140;

  // ---------------------------------------------------------------- constants
  const RING_SPEED = 9.4;
  // The speed under which a car reads as braking rather than cruising: half the free speed,
  // the point where the stop and go wave has visibly bitten.
  const RING_BRAKE_INV = 1 / (RING_SPEED * 0.5);
  const AVE_SPEED = 8.6;
  const SPUR_SPEED = 8.4;
  const EXP_SPEED = 12.4;
  const EXP_CRAWL = 1.15;
  // The expressway stream is spaced by headway, not by a fixed slot grid: a car keeps about
  // 1.4 s to the one ahead of it, which is 17.4 m at free speed and closes to a stopped queue
  // at a crawl. That is what makes eight cars read as a stream down the deck instead of as two
  // dots per lane eighty metres apart, and it is read off the speed the density term already
  // sets, so it adds no data term of its own. EXP_MIN_GAP is a car and a bumper's gap.
  const EXP_HEADWAY_S = 1.4;
  const EXP_MIN_GAP = 5.2;
  // The braking threshold of the independently driven pools, as a reciprocal so a per car
  // brake level is one multiply: a car under half its free speed reads as braking.
  const AVE_BRAKE_INV = 1 / (AVE_SPEED * 0.5);
  const SPUR_BRAKE_INV = 1 / (SPUR_SPEED * 0.5);
  // ------------------------------------------------------------- car following
  // Every car pool integrates its own arc position, so without a gap law two cars can settle
  // on the same arc length and their bodies intersect. One law fixes all of them: a car may
  // hold at most the data speed that keeps FOLLOW_MIN_GAP of arc between itself and the
  // nearest car ahead in its own lane, which is one FOLLOW_HEADWAY of travel away. The counts,
  // the congestion wave and the green wave are untouched; a queue is what the density does to
  // a lane under this law, not a second source of traffic.
  const FOLLOW_BIN_W = 6.5;      // metres of arc per occupancy bin, about a car and a half
  const FOLLOW_BIN_INV = 1 / FOLLOW_BIN_W;
  // The current bin plus three: searching k bins past the current one guarantees coverage of
  // k * FOLLOW_BIN_W metres from the start of the car's own bin, and 3 * 6.5 = 19.5 covers
  // MIN_GAP + HEADWAY * 11 m/s, the fastest speed target any following pool can ask for.
  const FOLLOW_LOOKAHEAD = 3;
  const FOLLOW_REACH = FOLLOW_BIN_W * FOLLOW_LOOKAHEAD;
  const FOLLOW_MIN_GAP = 4.6;    // centre to centre: a 4.1 m body plus half a metre of air
  const FOLLOW_HEADWAY = 1.1;    // seconds; the safe speed is (gap - MIN_GAP) / HEADWAY
  // A car entering a lane is dropped clear of its neighbours by this much: the gap law plus a
  // quarter metre. It must not exceed the mean spacing a full lane can hold, or a dense lane
  // would have nowhere to put an entering car and every spawn would slide to the far end.
  const FOLLOW_SPAWN_ROOM = FOLLOW_MIN_GAP + 0.3;  // 4.9 m, centre to centre
  const FOLLOW_SPAWN_TRIES = 24;
  const FOLLOW_EASE = 5.5;       // per second: the one acceleration law of every car pool
  // ------------------------------------------------------------ junction yield
  // The follow law above keeps one lane apart. Where two roads cross at grade it cannot: at
  // the two places the ring cuts the avenue's carriageways and at the three spur mouths a ring
  // car and an avenue or spur car pass through the same square metres. The rule there is
  // priority by geometry, not by a data term: the ring is the arterial and keeps the right of
  // way, and a car of the lower road yields to it. Each crossing is resolved at build to an
  // arc on the ring table and an arc on its own lane's table, and a yielding car holds the
  // junction clearance through the same braking law as the lane clamp, so the tail lamps light
  // through the one write path that already exists.
  const JUNCTION_GAP = 5.2;      // metres: the stop point, centre to centre at the crossing
  const JUNCTION_HEADWAY = 1.1;  // seconds: the lane follow headway, the one braking law
  const JUNCTION_REACH = 14;     // metres before the crossing where a car starts to yield
  const JUNCTION_TAIL = 2.6;     // metres past the crossing a ring car still owns the box
  // The yield is a time comparison, not a fixed window of ring arc: a car needs the time it
  // will take to clear the crossing, and the ring is credited with RING_SPEED, so a ring car
  // anywhere inside the resulting window may arrive first. A car crawling at two metres a
  // second therefore needs a wide gap while a car at lane speed crosses a narrow one, which is
  // what keeps a dense lower road from being let through a moving ring.
  const JUNCTION_PAST = 3.6;     // metres past the crossing a clearing car must reach: half a
                                 // body plus the ring's lane offset plus a hand's width
  const JUNCTION_MARGIN = 0.5;   // seconds of clearance kept on top of the crossing time
  const JUNCTION_MIN_V = 2;      // m/s: the slowest a clearing car is assumed to go
  const JUNCTION_SCAN_M = 52;    // metres of ring arc scanned: the longest window a car can ask
  const JUNCTION_SCAN_BINS = 8;  // bins of that scan, plus two behind for the tail
  // Metres of clear lane ahead of the stop line a car must see before it commits: its own arc
  // to the crossing (at most JUNCTION_GAP) plus, past the crossing, the distance it must reach
  // and the follow gap. A car that passes the stop line therefore cannot be brought to rest
  // anywhere before JUNCTION_PAST, because the car ahead already stands a full follow gap
  // beyond that point.
  const JUNCTION_EXIT = JUNCTION_PAST + FOLLOW_MIN_GAP;
  // Metres past a crossing centre a body has fully left the box: the ring's asphalt half width
  // plus half a car. It is the distance a green commitment is measured against, and the window
  // the avenue watches for a ring car still clearing on its own green.
  const JUNCTION_CLEAR = 7.0;
  // Metres: how far a lane centre may pass from the ring centreline and still count as a
  // crossing. A spur's published first point sits up to 1.7 m off the spline the ring table
  // samples, so the tolerance has to cover that; the nearest the avenue lanes come to the ring
  // anywhere else is the roundabout at 4.4 m, which stays well outside it.
  const JUNCTION_MEET = 3.0;
  const VAN_SPEED = 9.8;
  const TRAM_SPEED = 6.2;
  const TRAM_DWELL = 3.0;
  const TRAM_GAP = 8.9;
  const TRAM_SEGS = 3;
  const TRAIN_SPEED = 9.2;
  const TRAIN_SPEED_B = 8.4;
  const TRAIN_DWELL = 2.5;
  const PED_MIN_SPEED = 1.05;
  const PED_MAX_SPEED = 1.75;
  const CROSS_SPEED = 2.1;
  const CROSS_PERIOD = 20.0;
  const CROSS_GO = 8.0;
  const SIGN_PERIOD = 8.0;
  // Signal cycle, in seconds. The avenue mouth runs on four slots: avenue green, avenue amber,
  // ring green, ring amber. The avenue is the hero road of the data, so it takes 24 s of green
  // and the ring 20 s, each with 3 s of amber: 24 + 3 + 20 + 3 = 50 s. The two families read
  // disjoint slots, so they are never green at the same instant inside one junction.
  //
  // The two green times are chosen so that both red halves are shorter than the 30 s window the
  // live probe counts in: the avenue is red for ring green plus ring amber (23 s) and the ring is
  // red for avenue green plus avenue amber (27 s). Every 30 s window therefore contains green
  // for both roads, which is what makes a 30 s crossing count never read zero at the mouth.
  const SIGNAL_AVE_GREEN_S = 24;
  const SIGNAL_RING_GREEN_S = 20;
  const SIGNAL_AMBER_S = 3;
  const SIGNAL_RING_START_S = SIGNAL_AVE_GREEN_S + SIGNAL_AMBER_S;                 // 27 s
  const SIGNAL_AMBER_START_S = SIGNAL_RING_START_S + SIGNAL_RING_GREEN_S;          // 47 s
  const SIGNAL_CYCLE_S = SIGNAL_AMBER_START_S + SIGNAL_AMBER_S;                     // 50 s
  const ROUTE_SPACING = 1.6;
  const FADE_INV = 1 / 6;
  const TWO_PI = Math.PI * 2;

  // Night lighting pool. One instanced mesh carries every light the moving life owns, so the
  // whole night lighting of this file is one draw call: six additive plates per vehicle (a
  // headlight pair, a tail pair, the wash on the road ahead and the pool behind a braking
  // car), five per bus (window band, headlight wash, destination sign, headlight pair) and one
  // per tram carriage (destination sign). Slots are fixed by index, so a pool that shrinks
  // never leaves a lamp behind, and every write is a matrix or a colour into a buffer that is
  // allocated once.
  //
  // The two fixed groups come first and the car plates last, so the live slot set is a prefix
  // of the pool and the mesh can submit only it. A night frame with forty cars then draws 13 +
  // 40 * 6 plates instead of the whole cap, and because the bus band and the tram's destination
  // sign sit below the car plates, a small live car count can never trim away a light that is
  // on screen.
  const CAR_SLOTS = 6;
  const BUS_SLOTS = 5;
  const LIGHT_BUS = 0;
  const LIGHT_TRAM = LIGHT_BUS + BUS_COUNT * BUS_SLOTS;
  const LIGHT_CAR = LIGHT_TRAM + TRAM_SEGS;
  const LIGHT_CAP = LIGHT_CAR + CAR_CAP * CAR_SLOTS;
  // Wash footprint by vehicle kind: car, scooter, van, truck.
  const WASH_W = new Float32Array([3.8, 2.0, 4.2, 4.6]);
  const WASH_L = new Float32Array([10.6, 5.6, 11.2, 11.6]);
  // Half the distance between the two lamps of a pair, and the plate itself: width, height and
  // the depth every plate is given so it stays visible face on and edge wise alike.
  const LAMP_OFF = 0.58;
  const LAMP_W = 0.32;
  const LAMP_H = 0.2;
  const LAMP_D = 0.1;
  // The pack fleet's frame. A model's origin is its wheel contact and its local +Z is the
  // nose, which is the frame writeMatrix already writes, so a model needs no second path.
  // MODEL_W and MODEL_L are the procedural body's footprint (1.92 x 4.1): every model is
  // fitted inside that box, which is what keeps the follow gap, the lane offsets and the
  // junction clearance exactly as measured. MODEL_LIFT is the paint and kerb lip the wheels
  // stand on, so a model never sinks into the carriageway at a grazing view.
  const MODEL_W = 1.92;
  const MODEL_L = 4.1;
  const MODEL_LIFT = 0.04;

  // Pavement centre lines and lane centres, from the road geometry. The avenue is a 31 m
  // boulevard in the rebuilt world: median, tram beds, one carriageway each side, pavement.
  // The fields are read with the pre-boulevard fallbacks so the file still runs on the
  // older cross section.
  const roads = world && world.roads ? world.roads : null;
  const RING_PAVE_OFF = 5.7;
  const AVE_PAVE_OFF = roads && typeof roads.avenuePavementOffset === 'number'
    ? roads.avenuePavementOffset : 8.3;
  const AVE_CWAY = roads && roads.avenueCarriageway &&
    typeof roads.avenueCarriageway.inner === 'number' &&
    typeof roads.avenueCarriageway.outer === 'number' ? roads.avenueCarriageway : null;
  // Cars keep the inner half of one carriageway, buses take the kerb lane: on the boulevard
  // that is the classic kerbside bus lane, and neither ever overlaps the other.
  const AVE_CAR_LANE = AVE_CWAY
    ? AVE_CWAY.inner + 1.55
    : (roads && typeof roads.avenueLaneCentre === 'number' ? roads.avenueLaneCentre : 1.7);
  const AVE_BUS_LANE = AVE_CWAY ? AVE_CWAY.outer - 1.6 : 0;
  const AVE_BUS_ON = AVE_BUS_LANE > 0 && !!roads && !!roads.avenue;
  const RING_LOT_OFF = 3.42;
  const RING_LANE_OFF = 1.2;
  // Delivery vans ride the same carriageway as the cars, never the ring or avenue centre
  // line (the avenue centre is the planted median on the boulevard).
  const AVE_VAN_OFF = AVE_CAR_LANE;
  const RING_VAN_OFF = 1.5;

  // A busy hour of the sample day sums to 1.0 of engaged time across its quarters (the sum
  // of activityLastHour is one hour of screen time spread over the apps). Scaling by 0.94
  // puts the busiest hour at the full pedestrian cap and keeps quiet hours proportional.
  const PED_REF_SUM = 0.94;
  const PED_QUIET = 60;
  const PED_BASE = MAX_PED <= 0 ? 0 : (PED_QUIET < PED_CAP ? PED_QUIET : PED_CAP);

  // Vehicle silhouettes inside one instanced mesh: per instance scale and livery. Buses are
  // their own two instance mesh because their livery band needs its own draw call.
  const KIND_CAR = 0;
  const KIND_SCOOTER = 1;
  const KIND_VAN = 2;
  const KIND_TRUCK = 3;
  // The brief wants scooters everywhere on a Mediterranean evening: one in five moving
  // vehicles is a scooter, which reads as a stream of them at district range without
  // crowding the cars out of the frame.
  const SCOOTER_PCT = 0.2;
  const MIX_VAN_PCT = 0.07;
  const TRUCK_PCT = 0.04;

  // Pedestrian cohorts. The index decides the cohort, so a growing population keeps the
  // same blend at every activity level: a walk along a pavement, a plaza crowd, a crossing.
  // The blend leans on the market square (55 percent) because the brief asks for a crowd
  // there, keeps 30 percent walking the pavement lines and 15 percent on the zebras.
  // Every cohort is one of the three placements below, so no walker leaves a pavement
  // line, a crossing or the plaza.
  const PED_ROUTE = 0;
  const PED_SQUARE = 1;
  const PED_CROSS = 2;

  // ------------------------------------------------------------- small maths
  function clamp(v, a, b) {
    return v < a ? a : (v > b ? b : v);
  }
  function clamp01(v) {
    return v > 1 ? 1 : (v > 0 ? v : 0);
  }
  function mod(a, n) {
    if (n <= 0) return 0;
    const r = a % n;
    return r < 0 ? r + n : r;
  }
  function hash01(i) {
    let x = (i | 0) + 0x9e3779b9;
    x = Math.imul(x ^ (x >>> 16), 0x21f0aaad);
    x = Math.imul(x ^ (x >>> 15), 0x735a2d97);
    x = x ^ (x >>> 15);
    return (x >>> 0) / 4294967296;
  }
  function easeTo(cur, target, rate, dt) {
    const k = dt * rate;
    return cur + (target - cur) * (k > 1 ? 1 : k);
  }

  // ------------------------------------------------------- wake re-population
  // The dream freezes every road pool and empties the walkers. The complaint is not the rate:
  // the car pools keep the counts they had when the dream began, so the single frame that puts
  // the fleet meshes back reveals the whole fleet at once, while the walkers come back at
  // whatever rate one frame's dt allows. This window replaces both with a real slice of work.
  // Every pool brings back its own population over RESYNC_FRAMES frames: on the waking frame the
  // pools drop to zero and each frame after that admits one more `span / RESYNC_FRAMES` of it, in
  // slot order, so a pool of two buses and a pool of seven hundred walkers both finish on frame
  // thirty. A slot that has not entered yet keeps the zero matrix, so the first awake frame draws
  // only the window's first slice (about a thirtieth of each pool) instead of the whole fleet. A
  // backwards data clock or a live data stop opens the same window with only the rate sliced and
  // the population left in place, because a data change is not a sleep; under a sleep neither one
  // opens anything, since nothing steps to spread.
  //
  // Cost on a steady frame: one integer compare per pool. Nothing here allocates: the cursors are
  // fixed arrays and the report is one object mutated in place.
  const RESYNC_FRAMES = 30;
  const RESYNC_MAX_FRAMES = RESYNC_FRAMES * 2;
  const RESYNC_POOLS = 6;
  const RESYNC_RING = 0;
  const RESYNC_AVE = 1;
  const RESYNC_SPUR = 2;
  const RESYNC_EXP = 3;
  const RESYNC_BUS = 4;
  const RESYNC_PED = 5;
  // Per pool: the population it has to bring back, the slots it may enter in one frame, the last
  // target it asked for, the drawn count it reached, and the entrances this frame and this
  // window. Fixed arrays, written in place, never rebuilt.
  const resyncSpan = new Float32Array(RESYNC_POOLS);
  const resyncStep = new Float32Array(RESYNC_POOLS);
  const resyncTarget = new Float32Array(RESYNC_POOLS);
  const resyncShown = new Float32Array(RESYNC_POOLS);
  const resyncAdmit = new Int32Array(RESYNC_POOLS);
  const resyncTotal = new Int32Array(RESYNC_POOLS);
  let resyncOpen = false;
  let resyncFrames = 0;

  const reseedReport = {
    windowFrames: RESYNC_FRAMES,
    maxFrames: RESYNC_MAX_FRAMES,
    open: false,        // a window is running
    settled: true,      // and its negation, which reads better in a log
    kind: 'none',       // what opened the last one
    frames: 0,          // frames the running window has counted
    framesLeft: 0,      // frames of its budget left, 0 when it has none
    span: resyncSpan,   // population each pool has to bring back
    step: resyncStep,   // slots per frame each pool is allowed
    admitted: resyncAdmit,      // slots that crossed the drawn ceiling last frame, per pool
    admittedTotal: resyncTotal, // and since the window opened (entries while growing)
    admittedSum: 0,     // the same, summed over the six pools
    windows: 0,
    lastFrames: 0,      // frames the last completed window took
    wake: 0,
    wrap: 0,
    rebuild: 0
  };

  function poolShownOf(p) {
    if (p === RESYNC_RING) return ringShown;
    if (p === RESYNC_AVE) return aveShown;
    if (p === RESYNC_SPUR) return spurShown;
    if (p === RESYNC_EXP) return expShown;
    if (p === RESYNC_BUS) return busShown;
    return pedShown;
  }

  // The population a sleep has to bring back, read at the sleep edge while every pool still
  // draws what it drew awake. The walkers are captured before hideAllPeds zeroes them.
  function captureResyncSpans() {
    for (let p = 0; p < RESYNC_POOLS; p++) resyncSpan[p] = poolShownOf(p);
  }

  // Open the window. A wake also empties the pools, which is what keeps the first awake frame
  // from revealing the fleet: each pool then steps from zero and hides its own tail on the way,
  // so the entrances it reports are the slots it actually wrote this frame.
  function beginResync(kind) {
    // Nothing steps while the district dreams, so a data change under a sleep has no work to
    // spread: the spans captured at the sleep edge stay authoritative and the wake edge opens the
    // real window. Without this a rebuild under the dream would count thirty idle frames and
    // overwrite the walker span with the zero that rebuild leaves behind.
    if (kind !== 'wake' && dreamPrev) return;
    reseedReport.windows += 1;
    reseedReport.kind = kind;
    if (kind === 'wake') reseedReport.wake += 1;
    else if (kind === 'wrap') reseedReport.wrap += 1;
    else reseedReport.rebuild += 1;
    for (let p = 0; p < RESYNC_POOLS; p++) {
      if (kind !== 'wake') resyncSpan[p] = poolShownOf(p);
      resyncAdmit[p] = 0;
      resyncTotal[p] = 0;
      resyncTarget[p] = resyncSpan[p];
      resyncShown[p] = kind === 'wake' ? 0 : resyncSpan[p];
    }
    if (kind === 'wake') {
      ringShown = 0;
      aveShown = 0;
      spurShown = 0;
      expShown = 0;
      busShown = 0;
      pedShown = 0;
    }
    resyncOpen = true;
    resyncFrames = 0;
    reseedReport.open = true;
    reseedReport.settled = false;
    reseedReport.frames = 0;
    reseedReport.framesLeft = RESYNC_FRAMES;
    reseedReport.admittedSum = 0;
  }

  // One pool's drawn count for this frame. Outside the window the pool keeps its own 3.2 per
  // second ease, which is the cycle 26 ramp. Inside it the pool advances by its own slice,
  // `max(span, target) / RESYNC_FRAMES` slots per frame, clamped at its target, so a two bus pool
  // and a seven hundred walker pool both arrive on frame RESYNC_FRAMES. The drawn count is
  // fractional; the entrance count reported is the change in its ceiling, so a pool whose slice is
  // under one slot still reports one slot when it crosses, and can therefore report more than a
  // thirtieth of its own population on that frame.
  function rampSlots(p, cur, target, dt) {
    resyncTarget[p] = target;
    if (!resyncOpen) {
      // The last window's `step` and `admitted` stay readable here: they describe the window that
      // just ran, and the next window overwrites them when it opens.
      return easeTo(cur, target, 3.2, dt);
    }
    const need = target > resyncSpan[p] ? target : resyncSpan[p];
    const step = need > 0 ? need / RESYNC_FRAMES : 0;
    resyncStep[p] = step;
    let next = cur;
    if (step > 0) {
      if (cur < target) {
        next = cur + step;
        if (next > target) next = target;
      } else if (cur > target) {
        next = cur - step;
        if (next < target) next = target;
      }
    }
    const before = cur > 0 ? Math.ceil(cur) : 0;
    const after = next > 0 ? Math.ceil(next) : 0;
    const moved = after > before ? after - before : before - after;
    resyncAdmit[p] = moved;
    resyncTotal[p] += moved;
    resyncShown[p] = next;
    reseedReport.admittedSum += moved;
    return next;
  }

  // Count the frame that has just stepped its pools and close the window once its budget is spent
  // and every pool has arrived. The hard stop exists for a target that keeps moving under the
  // window (congestion does): without it such a target could hold the window open forever.
  function endResyncFrame() {
    if (!resyncOpen) return;
    resyncFrames += 1;
    if (resyncFrames >= RESYNC_FRAMES) {
      let arrived = true;
      for (let p = 0; p < RESYNC_POOLS; p++) {
        const left = resyncTarget[p] - resyncShown[p];
        if (left > 1 || left < -1) {
          arrived = false;
          break;
        }
      }
      if (arrived || resyncFrames >= RESYNC_MAX_FRAMES) {
        resyncOpen = false;
        reseedReport.open = false;
        reseedReport.settled = true;
        reseedReport.lastFrames = resyncFrames;
        reseedReport.framesLeft = 0;
      }
    }
    reseedReport.frames = resyncFrames;
    if (resyncOpen) {
      const left = RESYNC_FRAMES - resyncFrames;
      reseedReport.framesLeft = left > 0 ? left : 0;
    }
  }

  // ---------------------------------------------------------------- scratch
  const UP = new THREE.Vector3(0, 1, 0);
  const sPos = new THREE.Vector3();
  const sTan = new THREE.Vector3();
  const bFwd = new THREE.Vector3();
  const bRight = new THREE.Vector3();
  const bUp = new THREE.Vector3();
  const mScale = new THREE.Vector3();
  const mMat = new THREE.Matrix4();
  // Zero scale and no translation: the matrix a hidden instance carries, built once.
  const HIDDEN_MAT = new THREE.Matrix4().makeScale(0, 0, 0);
  // Cloud tints. The cloud material is unlit, so an instance colour is the cloud's own value,
  // and every one of these is capped in the linear working space under 0.75: that value is the
  // bloom threshold, and a cloud must never bloom. The old 0xfdf7ec tint (linear luma 0.94)
  // was the second reason the deck read as blown white. Warm is the sunlit side, cool the
  // shade away from it; the cover weight greys both as the hour fragments.
  const cloudTintWarm = new THREE.Color(0xd6c9b4);   // linear max 0.672
  const cloudTintCool = new THREE.Color(0x9aa3ae);   // linear max 0.423
  const cloudTint = new THREE.Color();
  const CLOUD_TINT_MAX = 0.72;
  // The night deck. A city lights its cloud bases from below, so at night a plate is not one
  // flat tint: it is a value that follows the lit bowl under it, the moon side and the distance
  // out to the edge of the deck. All three stops are deep blue and stay that way at every
  // scale: the linear blue channel is about 2.9 to 3.2 times the red, so no plate can mix to
  // grey, and the brightest stop is 0.195 in linear light, far under the 0.95 night bloom
  // threshold, so a lit base glows without blooming. Dreaming multiplies the same ramp down by
  // CLOUD_NIGHT_DREAM: the deck keeps its shape and loses its life with the rest of the city.
  const cloudNightDeep = new THREE.Color(0x1c2333);   // linear max 0.033
  const cloudNightCity = new THREE.Color(0x3a4a63);   // linear max 0.127
  const cloudNightMoon = new THREE.Color(0x46567a);   // linear max 0.195
  const CLOUD_NIGHT_DREAM = 0.62;
  // Seconds between night ramp refreshes: the ramp follows the plates as they drift.
  const CLOUD_NIGHT_REFRESH_S = 2.5;
  const flagColor = new THREE.Color();
  const brakeColor = new THREE.Color();

  // ------------------------------------------------------------- geometries
  function mergeParts(parts) {
    const geos = [];
    let total = 0;
    for (let i = 0; i < parts.length; i++) {
      const p = parts[i];
      const g = new THREE.BoxGeometry(p[0], p[1], p[2]);
      g.translate(p[3], p[4], p[5]);
      const ng = g.index ? g.toNonIndexed() : g;
      geos.push(ng);
      total += ng.attributes.position.count;
    }
    const posArr = new Float32Array(total * 3);
    const norArr = new Float32Array(total * 3);
    let offset = 0;
    for (let i = 0; i < geos.length; i++) {
      const g = geos[i];
      posArr.set(g.attributes.position.array, offset * 3);
      norArr.set(g.attributes.normal.array, offset * 3);
      offset += g.attributes.position.count;
    }
    const out = new THREE.BufferGeometry();
    out.setAttribute('position', new THREE.BufferAttribute(posArr, 3));
    out.setAttribute('normal', new THREE.BufferAttribute(norArr, 3));
    return out;
  }

  const carGeo = mergeParts([
    [1.92, 0.64, 4.1, 0, 0.52, 0],
    [1.62, 0.56, 1.95, 0, 1.06, -0.18]
  ]);
  // The delivery truck is the car body at a taller, boxier scale, so no extra geometry.
  const vanGeo = mergeParts([
    [2.12, 1.95, 4.3, 0, 1.18, 0],
    [1.98, 0.92, 1.5, 0, 0.72, 2.52]
  ]);
  // 2.0 m tall, 0.76 m across the base: people read as warm specks of colour even from the
  // district tier, and the head is a second instanced mesh so it never merges into the body.
  const pedGeo = new THREE.CylinderGeometry(0.28, 0.38, 2.0, 6, 1);
  pedGeo.translate(0, 1.0, 0);
  const pedHeadGeo = new THREE.IcosahedronGeometry(0.23, 0);
  pedHeadGeo.scale(1, 1.1, 1);
  const PED_HEAD_Y = 2.16;
  const riderGeo = mergeParts([
    [0.46, 0.8, 0.44, 0, 0.62, 0],
    [0.34, 0.32, 0.34, 0, 1.22, 0.02]
  ]);
  const busGeo = mergeParts([
    [2.5, 2.5, 11.8, 0, 1.55, 0],
    [2.26, 0.42, 11.0, 0, 2.9, 0]
  ]);
  const busBandGeo = new THREE.BoxGeometry(2.54, 0.46, 11.84);
  busBandGeo.translate(0, 1.32, 0);
  const tramGeo = new THREE.BoxGeometry(2.62, 2.72, 8.4);
  tramGeo.translate(0, 1.45, 0);
  const tramStripeGeo = new THREE.BoxGeometry(2.66, 0.34, 8.42);
  tramStripeGeo.translate(0, 1.02, 0);
  const tramWinGeo = new THREE.BoxGeometry(2.64, 0.78, 7.94);
  tramWinGeo.translate(0, 2.02, 0);
  const brakeGeo = new THREE.BoxGeometry(1.5, 0.16, 0.14);
  // A unit plate scaled per instance. Every light of the moving life reuses it: a headlight,
  // a tail lamp, the wash on the asphalt, a bus window band, a destination sign. One geometry
  // and one mesh is what keeps the whole night lighting at a single draw call.
  const lightGeo = new THREE.BoxGeometry(1, 1, 1);
  const railCarGeo = mergeParts([
    [3.0, 2.6, 12.6, 0, 1.9, 0],
    [2.58, 0.36, 11.8, 0, 3.32, 0]
  ]);
  const payGeo = new THREE.BoxGeometry(1, 1, 1);

  // ------------------------------------------------------------- cloud shell
  // A cloud is a horizontal plate of two triangles, one unit across before the instance scale,
  // so an instance scale is the cloud's width and depth in metres. Seen at the camera's
  // 40 degree pitch a plate reads as a foreshortened stratiform layer, and it costs 2
  // triangles where the icosahedron it replaces cost 20 for a shape that was the problem.
  function makeCloudQuad() {
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(new Float32Array([
      -0.5, 0, -0.5,
      0.5, 0, -0.5,
      0.5, 0, 0.5,
      -0.5, 0, 0.5
    ]), 3));
    geo.setAttribute('normal', new THREE.BufferAttribute(new Float32Array([
      0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0
    ]), 3));
    geo.setAttribute('uv', new THREE.BufferAttribute(new Float32Array([
      0, 0, 1, 0, 1, 1, 0, 1
    ]), 2));
    // Counter-clockwise seen from above: 0, 2, 1 then 0, 3, 2, so the front face looks up and
    // the plate is drawn from the only side a downward camera can ever see it.
    geo.setIndex(new THREE.BufferAttribute(new Uint16Array([0, 2, 1, 0, 3, 2]), 1));
    return geo;
  }

  // The puff itself, built in code because the bundle may fetch nothing but three. Four
  // overlapping radial lobes, each a cubic falloff, unioned and cut by a clear border: a soft
  // stratiform shape rather than a disc, with no texel reaching the quad edge, so no straight
  // edge can show even at the grazing angle this layer is seen at. RGB stays pure white
  // everywhere, because a mipmap or the transparent border must never fringe grey into the
  // alpha; the alpha channel carries the shape and peaks at 0.60, so two stacked plates are
  // still translucent. 128 square: 64 KiB, cheap to build once, and a 90 metre plate at 300
  // metres stretches it over roughly 500 pixels, so the gradient needs the room.
  function makeCloudTexture() {
    const S = 128;
    const data = new Uint8Array(S * S * 4);
    // Lobe centre x, centre y and radius, in unit quad space. The row is longer than it is
    // tall so the union already reads as a bed of cloud before the instance depth squashes it.
    const lx = [-0.40, -0.06, 0.34, 0.62];
    const ly = [0.10, -0.05, 0.05, -0.03];
    const lr = [0.50, 0.72, 0.48, 0.24];
    const PEAK = 0.60;
    let p = 0;
    for (let y = 0; y < S; y += 1) {
      const v = ((y + 0.5) / S) * 2 - 1;
      for (let x = 0; x < S; x += 1) {
        const u = ((x + 0.5) / S) * 2 - 1;
        let a = 0;
        for (let k = 0; k < 4; k += 1) {
          const dx = (u - lx[k]) / lr[k];
          const dy = (v - ly[k]) / lr[k];
          const d = Math.sqrt(dx * dx + dy * dy);
          if (d < 1) {
            const t = 1 - d;
            const f = t * t * (3 - 2 * t);
            if (f > a) a = f;
          }
        }
        const e = Math.max(u < 0 ? -u : u, v < 0 ? -v : v);
        if (e > 0.86) {
          const t = (e - 0.86) / 0.14;
          a *= 1 - t * t * (3 - 2 * t);
        }
        data[p] = 255;
        data[p + 1] = 255;
        data[p + 2] = 255;
        data[p + 3] = (a * PEAK * 255 + 0.5) | 0;
        p += 4;
      }
    }
    const tex = new THREE.DataTexture(data, S, S, THREE.RGBAFormat);
    // Mipmapped and trilinear: one plate is 500 pixels across at its nearest and a few dozen
    // at its farthest, and the alpha has to stay smooth across that whole range.
    tex.generateMipmaps = true;
    tex.minFilter = THREE.LinearMipmapLinearFilter;
    tex.magFilter = THREE.LinearFilter;
    tex.wrapS = THREE.ClampToEdgeWrapping;
    tex.wrapT = THREE.ClampToEdgeWrapping;
    tex.anisotropy = 2;
    tex.needsUpdate = true;
    return tex;
  }

  const cloudGeo = makeCloudQuad();
  const cloudTex = makeCloudTexture();

  // ------------------------------------------------------------- materials
  const carMat = new THREE.MeshLambertMaterial({ color: 0xffffff });
  const riderMat = new THREE.MeshLambertMaterial({ color: 0xffffff, emissive: 0x241e18 });
  const parkMat = new THREE.MeshLambertMaterial({ color: 0xffffff });
  // Unlit, so an instance colour reads as a lit lamp rather than as painted metal.
  const brakeMat = new THREE.MeshBasicMaterial({ color: 0xffffff });
  const vanMat = new THREE.MeshLambertMaterial({ color: 0xf1ead9 });
  const pedMat = new THREE.MeshLambertMaterial({ color: 0xffffff, emissive: 0x3a3227 });
  const pedHeadMat = new THREE.MeshLambertMaterial({ color: 0xffffff, emissive: 0x241e17 });
  const busMat = new THREE.MeshLambertMaterial({ color: 0xf4f1e7 });
  const busBandMat = new THREE.MeshLambertMaterial({ color: 0xffffff });
  const tramMat = new THREE.MeshLambertMaterial({ color: 0xf4f1e8 });
  const tramStripeMat = new THREE.MeshLambertMaterial({ color: 0xc03a30 });
  const tramWinMat = new THREE.MeshLambertMaterial({
    color: 0x2b323c,
    emissive: 0xffc477,
    emissiveIntensity: 0.16
  });
  const railMat = new THREE.MeshLambertMaterial({ color: 0xffffff });
  const payMat = new THREE.MeshLambertMaterial({ color: 0xffffff });
  // Unlit and alpha blended, and the one material in this file that writes no depth: the
  // plates of a deck overlap in a fixed instance order and a soft edge has nothing to sort,
  // so depthWrite would only cut a hole in whichever plate drew first. alphaTest stays 0 so
  // the soft border is never discarded whole. Fog is off on purpose: the scene fog starts at
  // 1100 metres and the entire cloud field is inside 300, so fog could only mix the tint
  // toward the fog colour and leave a grey halo where the alpha fades out.
  const cloudMat = new THREE.MeshBasicMaterial({
    color: 0xffffff,
    map: cloudTex,
    transparent: true,
    depthWrite: false,
    alphaTest: 0,
    fog: false
  });
  // Additive and unlit: a light adds to whatever is behind it and can never grey it, the
  // instance colour is the amount of light it throws, and additive blending needs no
  // per-instance depth sort, so the order of two overlapping lamps cannot change the result.
  const lightMat = new THREE.MeshBasicMaterial({
    color: 0xffffff,
    transparent: true,
    blending: THREE.AdditiveBlending,
    depthWrite: false
  });

  // ---------------------------------------------------------------- road data
  function makeTable(curve, n) {
    const pos = new Float32Array((n + 1) * 3);
    const tan = new Float32Array((n + 1) * 3);
    let len = 0;
    for (let i = 0; i <= n; i++) {
      const u = i / n;
      curve.getPointAt(u, sPos);
      curve.getTangentAt(u, sTan);
      pos[i * 3] = sPos.x;
      pos[i * 3 + 1] = sPos.y;
      pos[i * 3 + 2] = sPos.z;
      tan[i * 3] = sTan.x;
      tan[i * 3 + 1] = sTan.y;
      tan[i * 3 + 2] = sTan.z;
      if (i > 0) {
        const dx = pos[i * 3] - pos[i * 3 - 3];
        const dy = pos[i * 3 + 1] - pos[i * 3 - 2];
        const dz = pos[i * 3 + 2] - pos[i * 3 - 1];
        len += Math.sqrt(dx * dx + dy * dy + dz * dz);
      }
    }
    return { pos: pos, tan: tan, n: n, len: len, step: len / n };
  }

  function tableAt(tab, dist, outPos, outTan) {
    const L = tab.len;
    let d = dist;
    if (d < 0) d = 0;
    else if (d > L) d = L;
    const f0 = d / tab.step;
    let i = f0 | 0;
    let f = f0 - i;
    if (i >= tab.n) {
      i = tab.n - 1;
      f = 1;
    } else if (i < 0) {
      i = 0;
      f = 0;
    }
    const p = tab.pos;
    const t = tab.tan;
    const a = i * 3;
    const b = a + 3;
    outPos.x = p[a] + (p[b] - p[a]) * f;
    outPos.y = p[a + 1] + (p[b + 1] - p[a + 1]) * f;
    outPos.z = p[a + 2] + (p[b + 2] - p[a + 2]) * f;
    outTan.x = t[a] + (t[b] - t[a]) * f;
    outTan.y = t[a + 1] + (t[b + 1] - t[a + 1]) * f;
    outTan.z = t[a + 2] + (t[b + 2] - t[a + 2]) * f;
  }

  function nearestOnTable(tab, px, pz) {
    const p = tab.pos;
    let bestS = 0;
    let bestD = Infinity;
    for (let i = 0; i <= tab.n; i++) {
      const dx = p[i * 3] - px;
      const dz = p[i * 3 + 2] - pz;
      const dd = dx * dx + dz * dz;
      if (dd < bestD) {
        bestD = dd;
        bestS = i * tab.step;
      }
    }
    return { s: bestS, d: Math.sqrt(bestD) };
  }

  // Signed lateral offset of a point from a road, in the same convention the walk lines use
  // (positive is `pos - tan.z * off`), so a door and a pavement line can be compared by side.
  function offsetOn(tab, px, pz) {
    const n = nearestOnTable(tab, px, pz);
    tableAt(tab, n.s, sPos, sTan);
    return -(px - sPos.x) * sTan.z + (pz - sPos.z) * sTan.x;
  }

  // Coarse nearest distance, for clearance tests where a metre of error does not matter.
  function nearDistStride(tab, px, pz, stride) {
    const p = tab.pos;
    let bestD = Infinity;
    for (let i = 0; i <= tab.n; i += stride) {
      const dx = p[i * 3] - px;
      const dz = p[i * 3 + 2] - pz;
      const dd = dx * dx + dz * dz;
      if (dd < bestD) bestD = dd;
    }
    return Math.sqrt(bestD);
  }

  const ringTable = roads && roads.ring ? makeTable(roads.ring, 320) : null;
  const aveTable = roads && roads.avenue ? makeTable(roads.avenue, 200) : null;
  const tramTable = roads && roads.tram ? makeTable(roads.tram, 170) : null;
  const railTable = roads && roads.rail ? makeTable(roads.rail, 240) : null;
  const expTable = roads && roads.expressway ? makeTable(roads.expressway, 240) : null;
  const spurTables = [];
  if (roads && roads.spurs && roads.spurs.length) {
    for (let i = 0; i < roads.spurs.length; i++) {
      if (roads.spurs[i]) spurTables.push(makeTable(roads.spurs[i], 120));
    }
  }
  // Spur cross section: a two way arterial with kerbside parking on one side when the road
  // is wide enough for a lane, a parked car and the gaps between them. The half width comes
  // from the world when it reports one; a ring-like arterial is assumed otherwise.
  const spurN = Math.max(1, spurTables.length);
  const spurLaneOff = new Float32Array(spurN);
  const spurParkOff = new Float32Array(spurN);
  const spurParkOn = new Uint8Array(spurN);
  for (let k = 0; k < spurTables.length; k += 1) {
    let h = 4.4;
    const w = roads ? roads.spurHalfWidth : undefined;
    if (typeof w === 'number') h = w;
    else if (w && typeof w.length === 'number' && k < w.length) h = w[k];
    let lane = h - 3.27;
    if (lane > 1.5) lane = 1.5;
    if (lane < 0.9) lane = 0.9;
    spurLaneOff[k] = lane;
    spurParkOff[k] = h - 1.0;
    spurParkOn[k] = h >= 5.0 ? 1 : 0;
  }

  const ringLen = ringTable ? ringTable.len : 1;
  const aveLen = aveTable ? aveTable.len : 1;
  const tramLen = tramTable ? tramTable.len : 1;
  const railLen = railTable ? railTable.len : 1;
  const expLen = expTable ? expTable.len : 1;
  const EXP_DECK_Y = roads && typeof roads.expresswayDeckY === 'number'
    ? roads.expresswayDeckY : null;

  // Ring position of the roundabout (the avenue / ring junction).
  let junctionS = 0;
  if (ringTable && world.roundabout && world.roundabout.center) {
    junctionS = nearestOnTable(ringTable, world.roundabout.center.x, world.roundabout.center.z).s;
  }
  const zoneS = new Float32Array(2);
  zoneS[0] = junctionS;
  zoneS[1] = mod(junctionS + ringLen * 0.42, ringLen);

  // ------------------------------------------------------- car pool partition
  // One shared instanced mesh carries cars, scooters, vans and lorries at per instance
  // scale. The expressway and every other road gets its own index range, so a road never
  // steals a slot from another and shrinking a pool is a scale ramp, not a reallocation.
  const POOL = CAR_CAP;
  let expCap = expTable ? Math.max(8, Math.round(POOL * 0.29)) : 0;
  let aveCap = aveTable ? Math.max(6, Math.round(POOL * 0.19)) : 0;
  let spurCap = spurTables.length ? Math.max(4, Math.round(POOL * 0.12)) : 0;
  let ringCap = POOL - expCap - aveCap - spurCap;
  if (ringCap < 6) {
    // A tiny vehicle budget goes to the ring and the avenue only.
    ringCap = Math.max(6, Math.round(POOL * 0.6));
    aveCap = POOL - ringCap - expCap - spurCap;
    if (aveCap < 0) aveCap = 0;
  }
  const ringBase = 0;
  const aveBase = ringCap;
  const spurBase = aveBase + aveCap;
  const expBase = spurBase + spurCap;
  if (!expTable) expCap = 0;
  const EXP_LANES = (function () {
    const half = roads && typeof roads.expresswayHalfWidth === 'number'
      ? roads.expresswayHalfWidth : 5.2;
    return half >= 4.8 ? 4 : (half >= 3.6 ? 3 : 2);
  })();
  const EXP_HALF = roads && typeof roads.expresswayHalfWidth === 'number'
    ? roads.expresswayHalfWidth : 5.2;

  // --------------------------------------------------------------- tram stops
  let tramStopS = null;
  let tramStopN = 0;
  if (tramTable && world.tramStops && world.tramStops.length) {
    const raw = [];
    for (let i = 0; i < world.tramStops.length; i++) {
      const st = world.tramStops[i];
      let s = 0;
      if (st && st.position) s = nearestOnTable(tramTable, st.position.x, st.position.z).s;
      else if (st && typeof st.u === 'number') s = mod(st.u, 1) * tramLen;
      raw.push(s);
    }
    raw.sort(function (a, b) { return a - b; });
    tramStopN = raw.length;
    tramStopS = new Float32Array(tramStopN);
    for (let i = 0; i < tramStopN; i++) tramStopS[i] = raw[i];
  }

  // ------------------------------------------------------------ quarter state
  const wqCount = world && world.quarters ? world.quarters.length : 0;
  let qCap = Math.max(12, wqCount);
  let qN = 0;

  // Where each door point leaves its own street for the ring or avenue pavement: baked once
  // per world, reused by every trip. Two slots per door, one per road family.
  const MAX_DOORS = 8;
  const DOOR_SLOTS = Math.max(1, wqCount) * MAX_DOORS * 2;
  const anchorLine = new Int8Array(DOOR_SLOTS);
  const anchorS = new Float32Array(DOOR_SLOTS);
  const anchorX = new Float32Array(DOOR_SLOTS);
  const anchorY = new Float32Array(DOOR_SLOTS);
  const anchorZ = new Float32Array(DOOR_SLOTS);
  anchorLine.fill(-1);
  // Signed side of every door point against both road families, so a trip can refuse a
  // pavement line that sits on the far side of a carriageway.
  const doorOffRing = new Float32Array(Math.max(1, wqCount) * MAX_DOORS);
  const doorOffAve = new Float32Array(Math.max(1, wqCount) * MAX_DOORS);

  let needArr = new Float32Array(qCap);
  let needPrev = new Float32Array(qCap);
  needPrev.fill(-1);
  let needDirty = true;
  let craneCur = new Int16Array(qCap);
  let cranePrev = new Int16Array(qCap);
  cranePrev.fill(-1);
  const accessKind = new Uint8Array(qCap);
  const accessS = new Float32Array(qCap);
  let routes = null;
  let pedRoutes = null;
  let routeQ = 0;
  let walkLines = null;
  let pedRouteList = new Int32Array(0);

  // ---------------------------------------------------------------- meshes
  // 15 draw calls from this file: bodies, riders, brake lamps, parked cars, the dream vans
  // and their payloads, people and heads, the three tram bands, the two bus bands, rail
  // vehicles and the cloud shell. A build with the pack cache adds one draw call per low car
  // model that the avenue wears (six on the desktop kit, none on the phone, which merges no
  // street tier), and those meshes are registered with the dream hide list like the rest.
  const group = new THREE.Group();

  const cars = new THREE.InstancedMesh(carGeo, carMat, CAR_CAP);
  cars.name = 'cars';
  cars.frustumCulled = false;
  const riders = new THREE.InstancedMesh(riderGeo, riderMat, CAR_CAP);
  riders.name = 'riders';
  riders.frustumCulled = false;
  const brakeLights = new THREE.InstancedMesh(brakeGeo, brakeMat, expCap > 0 ? expCap : 1);
  brakeLights.frustumCulled = false;
  const parked = new THREE.InstancedMesh(carGeo, parkMat, PARK_CAP);
  parked.frustumCulled = false;
  const vanMesh = new THREE.InstancedMesh(vanGeo, vanMat, VAN_CAP);
  vanMesh.frustumCulled = false;
  // The payload plate is a delivery and dream sign only: the farm slots never carry one, so
  // the payload pool keeps the old count and adds no triangles to the frame.
  const payloads = new THREE.InstancedMesh(payGeo, payMat, VAN_DELIVERY);
  payloads.frustumCulled = false;
  const peds = new THREE.InstancedMesh(pedGeo, pedMat, PED_CAP);
  peds.name = 'pedestrians';
  peds.frustumCulled = false;
  const pedHeads = new THREE.InstancedMesh(pedHeadGeo, pedHeadMat, PED_CAP);
  pedHeads.frustumCulled = false;
  const tramBody = new THREE.InstancedMesh(tramGeo, tramMat, TRAM_SEGS);
  tramBody.frustumCulled = false;
  const tramStripe = new THREE.InstancedMesh(tramStripeGeo, tramStripeMat, TRAM_SEGS);
  tramStripe.frustumCulled = false;
  const tramWins = new THREE.InstancedMesh(tramWinGeo, tramWinMat, TRAM_SEGS);
  tramWins.frustumCulled = false;
  const busBody = new THREE.InstancedMesh(busGeo, busMat, BUS_COUNT);
  busBody.frustumCulled = false;
  const busBand = new THREE.InstancedMesh(busBandGeo, busBandMat, BUS_COUNT);
  busBand.frustumCulled = false;
  const railCars = new THREE.InstancedMesh(railCarGeo, railMat, RAIL_CAP);
  railCars.frustumCulled = false;
  const clouds = new THREE.InstancedMesh(cloudGeo, cloudMat, CLOUD_CAP);
  clouds.name = 'clouds';
  clouds.frustumCulled = false;
  // The opaque pass always runs first; this only orders the clouds after the sky (renderOrder
  // -1000) and after the transparent light pool (0), so a deck blends over the scene it covers
  // and never over a lamp that is nearer the camera.
  clouds.renderOrder = 2;
  // The night light pool. It starts hidden and is switched on by the night gate, so a day
  // frame renders exactly the meshes it rendered before the pool existed.
  const nightLights = new THREE.InstancedMesh(lightGeo, lightMat, LIGHT_CAP);
  nightLights.name = 'night-lights';
  nightLights.frustumCulled = false;
  nightLights.visible = false;

  group.add(cars);
  group.add(riders);
  group.add(brakeLights);
  group.add(parked);
  group.add(vanMesh);
  group.add(payloads);
  group.add(peds);
  group.add(pedHeads);
  group.add(tramBody);
  group.add(tramStripe);
  group.add(tramWins);
  group.add(busBody);
  group.add(busBand);
  group.add(railCars);
  group.add(clouds);
  group.add(nightLights);
  if (!expTable) brakeLights.visible = false;
  if (!AVE_BUS_ON) {
    busBody.visible = false;
    busBand.visible = false;
  }

  // ------------------------------------------------------------- dream fleet
  // The dream skips every mover step, so the ring and avenue cars, the kerb cars, the tram,
  // the buses and the rail stock keep the last matrices they wrote. With the night gate off
  // they are also dark, so that frame pays ten draw calls for a parked fleet nobody can
  // read. Hiding the pools for the duration is the same picture at none of the cost: the
  // meshes are frozen mid-motion, so waking restores them exactly as today and the first
  // live step picks the movement up again. Baseline visibility is captured once, because
  // the brake and bus pools can be switched off at build by a world without an expressway
  // or an avenue bus lane. Called only on the two dream edges, and guarded, so a repeated
  // frame touches nothing in the scene graph and allocates nothing.
  const fleetMeshes = [
    cars, riders, brakeLights, parked,
    tramBody, tramStripe, tramWins,
    busBody, busBand, railCars
  ];
  const fleetBase = [];
  for (let i = 0; i < fleetMeshes.length; i++) fleetBase.push(fleetMeshes[i].visible ? 1 : 0);
  let fleetHidden = false;

  // The pack fleet's meshes are built later, once the vehicle kinds are known, and they join
  // the same hide list so the dream darkens them with every other mover. Baseline visibility
  // is read at registration, before any dream edge can run.
  function registerFleetMesh(mesh) {
    fleetMeshes.push(mesh);
    fleetBase.push(mesh.visible ? 1 : 0);
    mesh.visible = !fleetHidden && fleetBase[fleetBase.length - 1] === 1;
    return mesh;
  }

  function applyDreamFleet(dreaming) {
    if (dreaming === fleetHidden) return;
    fleetHidden = dreaming;
    for (let i = 0; i < fleetMeshes.length; i++) {
      fleetMeshes[i].visible = !dreaming && fleetBase[i] === 1;
    }
  }

  // ------------------------------------------------------- instance uploads
  // A pool is rewritten every frame while it has live instances, but the upload should follow
  // the change, not the pool: a quiet hour shows ten cars of a hundred and fifteen and a
  // truck-free spur, and three uploads what the attribute's dirty state says. So each pool
  // records the instance spans it wrote and hands three those spans as update ranges. The
  // range records are created once here and reused, so marking a frame allocates nothing.
  const SPAN_CAP = 8;
  const uploadPools = [];

  function makePool(mesh) {
    const pool = {
      mesh: mesh,
      mStart: new Int32Array(SPAN_CAP),
      mCount: new Int32Array(SPAN_CAP),
      mN: 0,
      mAll: 0,
      cStart: new Int32Array(SPAN_CAP),
      cCount: new Int32Array(SPAN_CAP),
      cN: 0,
      cAll: 0,
      mObjs: [],
      cObjs: []
    };
    for (let i = 0; i < SPAN_CAP; i++) {
      pool.mObjs.push({ start: 0, count: 0 });
      pool.cObjs.push({ start: 0, count: 0 });
    }
    mesh.__pool = pool;
    uploadPools.push(pool);
    return pool;
  }

  // Spans are in attribute elements: a matrix instance is 16 floats, a colour instance 3.
  function addSpan(pool, colour, start, count) {
    if (count <= 0) return;
    const S = colour ? pool.cStart : pool.mStart;
    const C = colour ? pool.cCount : pool.mCount;
    let n = colour ? pool.cN : pool.mN;
    if (n > 0) {
      const last = S[n - 1];
      const end = last + C[n - 1];
      if (start >= last && start <= end) {
        const tail = start + count;
        if (tail > end) C[n - 1] = tail - last;
        return;
      }
    }
    if (n >= SPAN_CAP) {
      // Too fragmented to describe this frame: one whole-buffer upload instead.
      if (colour) {
        pool.cN = 0;
        pool.cAll = 1;
      } else {
        pool.mN = 0;
        pool.mAll = 1;
      }
      return;
    }
    S[n] = start;
    C[n] = count;
    n += 1;
    if (colour) pool.cN = n;
    else pool.mN = n;
  }

  // Instances [lo, hi) were written this frame.
  function dirtyMatrix(mesh, lo, hi) {
    const pool = mesh.__pool;
    if (!pool || hi <= lo) return;
    addSpan(pool, false, lo * 16, (hi - lo) * 16);
  }

  function dirtyColour(mesh, lo, hi) {
    const pool = mesh.__pool;
    if (!pool || hi <= lo || !mesh.instanceColor) return;
    addSpan(pool, true, lo * 3, (hi - lo) * 3);
  }

  // Every instance of the pool was written: one whole-buffer upload, no spans.
  function dirtyAllMatrix(mesh) {
    const pool = mesh.__pool;
    if (!pool) return;
    pool.mAll = 1;
    pool.mN = 0;
    if (mesh.instanceMatrix) mesh.instanceMatrix.needsUpdate = true;
  }

  function dirtyAllColour(mesh) {
    const pool = mesh.__pool;
    if (!pool || !mesh.instanceColor) return;
    pool.cAll = 1;
    pool.cN = 0;
    mesh.instanceColor.needsUpdate = true;
  }

  // A span set that covers most of the buffer is not worth describing: one whole-buffer
  // upload then costs less traffic than the range bookkeeping (and allocates nothing),
  // so anything above three quarters of the pool goes out whole.
  function flushPool(pool) {
    const mesh = pool.mesh;
    const im = mesh.instanceMatrix;
    if (im) {
      const ranges = im.updateRanges;
      // Spans left from the last frame mean nothing was uploaded (an invisible pool): fall
      // back to a whole-buffer upload rather than dropping what those spans describe.
      const stale = ranges !== undefined && ranges.length > 0;
      let whole = pool.mAll !== 0 || stale;
      if (!whole && pool.mN > 0) {
        let total = 0;
        for (let i = 0; i < pool.mN; i++) total += pool.mCount[i];
        if (!ranges || total * 4 > im.array.length * 3) whole = true;
      }
      if (whole) {
        if (stale) ranges.length = 0;
        if (pool.mN > 0 || pool.mAll !== 0) im.needsUpdate = true;
        pool.mAll = 0;
        pool.mN = 0;
      } else if (pool.mN > 0) {
        ranges.length = 0;
        for (let i = 0; i < pool.mN; i++) {
          const o = pool.mObjs[i];
          o.start = pool.mStart[i];
          o.count = pool.mCount[i];
          ranges.push(o);
        }
        im.needsUpdate = true;
        pool.mN = 0;
      }
    }
    const ic = mesh.instanceColor;
    if (ic) {
      const cranges = ic.updateRanges;
      const cstale = cranges !== undefined && cranges.length > 0;
      let cwhole = pool.cAll !== 0 || cstale;
      if (!cwhole && pool.cN > 0) {
        let total = 0;
        for (let i = 0; i < pool.cN; i++) total += pool.cCount[i];
        if (!cranges || total * 4 > ic.array.length * 3) cwhole = true;
      }
      if (cwhole) {
        if (cstale) cranges.length = 0;
        if (pool.cN > 0 || pool.cAll !== 0) ic.needsUpdate = true;
        pool.cAll = 0;
        pool.cN = 0;
      } else if (pool.cN > 0) {
        cranges.length = 0;
        for (let i = 0; i < pool.cN; i++) {
          const o = pool.cObjs[i];
          o.start = pool.cStart[i];
          o.count = pool.cCount[i];
          cranges.push(o);
        }
        ic.needsUpdate = true;
        pool.cN = 0;
      }
    } else {
      pool.cN = 0;
      pool.cAll = 0;
    }
  }

  // Called once at the end of every update, after every step has declared its writes.
  function flushUploads() {
    for (let i = 0; i < uploadPools.length; i++) flushPool(uploadPools[i]);
  }

  makePool(cars);
  makePool(riders);
  makePool(brakeLights);
  makePool(parked);
  makePool(vanMesh);
  makePool(payloads);
  makePool(peds);
  makePool(pedHeads);
  makePool(tramBody);
  makePool(tramStripe);
  makePool(tramWins);
  makePool(busBody);
  makePool(busBand);
  makePool(railCars);
  makePool(clouds);
  makePool(nightLights);

  // ------------------------------------------------------------- palette data
  const carPalette = [
    new THREE.Color(0xe4583f), new THREE.Color(0x3f738f), new THREE.Color(0xd9a441),
    new THREE.Color(0x6d8f4e), new THREE.Color(0xeadfce), new THREE.Color(0x8b5a7a),
    new THREE.Color(0x39474f), new THREE.Color(0xf2f0e6), new THREE.Color(0xc2452f),
    new THREE.Color(0x4f7d92)
  ];
  // Small and quick: scooters in saturated paint that pops at district distance.
  const scooterPalette = [
    new THREE.Color(0xe8402f), new THREE.Color(0xf2b134), new THREE.Color(0x3f8fd0),
    new THREE.Color(0x58c06a), new THREE.Color(0xf0f0f0), new THREE.Color(0x333a44)
  ];
  // Delivery vans and lorries: pale boxy bodies with a coloured nose or tailgate.
  const vanPalette = [
    new THREE.Color(0xf1ead9), new THREE.Color(0xe6e0cf), new THREE.Color(0xdcd6c6),
    new THREE.Color(0xe8dcc0)
  ];
  const truckPalette = [
    new THREE.Color(0xf3f0e7), new THREE.Color(0x4f7f96), new THREE.Color(0xd9a43a),
    new THREE.Color(0xc4503a)
  ];
  const avenueGreen = new THREE.Color(0x5bc46e);
  // How far an avenue car's own body colour moves toward the wave green. A blend, never a
  // repaint: the contract allows the segment to tint, and a lane of identical saturated slabs
  // is what a full repaint looked like from the street.
  const WAVE_BLEND = 0.45;
  // The model fleet rides a palette texture shared by every instance, so its instance colour
  // is a multiplier and never a paint. The wave therefore starts from white, the identity
  // that leaves the kit's own livery alone, and takes the same 45 percent step toward the
  // wave green. One colour, built once, so a wave edge allocates nothing.
  const waveTint = new THREE.Color(1, 1, 1).lerp(avenueGreen, WAVE_BLEND);
  // Bright, saturated clothes: reds, yellows, whites, sky blues, greens, a few darks.
  const pedPalette = [
    new THREE.Color(0xe24438), new THREE.Color(0xf6bf3c), new THREE.Color(0xfaf7ef),
    new THREE.Color(0x5cbbea), new THREE.Color(0x53a75e), new THREE.Color(0x2b3037),
    new THREE.Color(0xec6d45), new THREE.Color(0x3578a8), new THREE.Color(0xf6f2e8),
    new THREE.Color(0xd24c42), new THREE.Color(0x86c971), new THREE.Color(0xffd975),
    new THREE.Color(0xe091b0), new THREE.Color(0x323844), new THREE.Color(0xa8dcf4)
  ];
  const pedHeadPalette = [
    new THREE.Color(0xd9a67a), new THREE.Color(0xb07a52), new THREE.Color(0xe8c39e),
    new THREE.Color(0x8a5a3b), new THREE.Color(0x3a2f28), new THREE.Color(0x241d17)
  ];
  const busBandPalette = [new THREE.Color(0xc1362e), new THREE.Color(0x2f7f8f)];
  const railPalette = [
    new THREE.Color(0x31506e), new THREE.Color(0xdfd8c8), new THREE.Color(0xa4553a)
  ];
  const parcelColor = new THREE.Color(0xb5825a);
  const signColor = new THREE.Color(0xf6e7c8);

  // Night light colours. The material is additive, so an instance colour is literally the
  // amount of light the plate throws, in the linear working space: the output transfer lifts
  // it, so a linear 0.6 red leaves the frame near 0.8.
  const lampHead = new THREE.Color(1.02, 0.88, 0.64);
  const lampTail = new THREE.Color(0.48, 0.06, 0.022);
  const lampBrake = new THREE.Color(0.95, 0.28, 0.1);
  const lampSign = new THREE.Color(0.9, 0.55, 0.18);
  const glowHead = new THREE.Color(0.74, 0.52, 0.265);
  const glowBrake = new THREE.Color(0.6, 0.08, 0.03);
  const glowBand = new THREE.Color(0.8, 0.47, 0.2);
  const lampTint = new THREE.Color();

  // -------------------------------------------------------- matrix utilities
  // One body write, without the scratch Matrix4/Quaternion/Vector3 round trip. compose()
  // stores its inputs and result through boxed doubles on shared scratch objects, and that
  // boxing was this file's largest per-frame garbage: it scales with the number of moving
  // instances, so a busy hour threw away megabytes. This writes the identical matrix (a Y
  // rotation, a per axis scale, a translation) straight into the instance buffer:
  //   [ c*sx, 0, -s*sx, 0 | 0, sy, 0, 0 | s*sz, 0, c*sz, 0 | x, y, z, 1 ]
  function writeMatrix(mesh, idx, x, y, z, angle, sx, sy, sz) {
    writeMatrixCS(mesh, idx, x, y, z, Math.cos(angle), Math.sin(angle), sx, sy, sz);
  }

  // The same write with the rotation already resolved, for the four lamp plates of one
  // vehicle: one trig pair serves the head pair, the tail pair and both road pools.
  function writeMatrixCS(mesh, idx, x, y, z, c, s, sx, sy, sz) {
    const arr = mesh.instanceMatrix.array;
    const b = idx * 16;
    arr[b] = c * sx;
    arr[b + 1] = 0;
    arr[b + 2] = -s * sx;
    arr[b + 3] = 0;
    arr[b + 4] = 0;
    arr[b + 5] = sy;
    arr[b + 6] = 0;
    arr[b + 7] = 0;
    arr[b + 8] = s * sz;
    arr[b + 9] = 0;
    arr[b + 10] = c * sz;
    arr[b + 11] = 0;
    arr[b + 12] = x;
    arr[b + 13] = y;
    arr[b + 14] = z;
    arr[b + 15] = 1;
  }

  // A full basis from the tangent, so rail stock pitches with the hillside grade.
  function writeBasis(mesh, idx, x, y, z, tan, sx, sy, sz) {
    bFwd.set(tan.x, tan.y, tan.z);
    if (bFwd.lengthSq() < 1e-8) bFwd.set(0, 0, 1);
    else bFwd.normalize();
    bRight.crossVectors(UP, bFwd);
    if (bRight.lengthSq() < 1e-8) bRight.set(1, 0, 0);
    else bRight.normalize();
    bUp.crossVectors(bFwd, bRight);
    mMat.makeBasis(bRight, bUp, bFwd);
    mMat.scale(mScale.set(sx, sy, sz));
    mMat.setPosition(x, y, z);
    mesh.setMatrixAt(idx, mMat);
  }

  function hideAt(mesh, idx) {
    // The hidden matrix is a constant: compose(0, identity, scale 0) is makeScale(0, 0, 0).
    mesh.setMatrixAt(idx, HIDDEN_MAT);
  }

  // ------------------------------------------------------------- walk lines
  function pushPoint(pts, x, y, z) {
    pts.push(x, y, z);
  }

  function pushTableRange(pts, tab, sA, sB) {
    const a = clamp(sA, 0, tab.len);
    const b = clamp(sB, 0, tab.len);
    const dist = Math.abs(b - a);
    const n = Math.max(1, Math.ceil(dist / tab.step));
    const dir = b >= a ? 1 : -1;
    for (let i = 0; i <= n; i++) {
      tableAt(tab, a + dir * (dist * i) / n, sPos, sTan);
      pushPoint(pts, sPos.x, sPos.y, sPos.z);
    }
  }

  // A lane range: the same curve shifted sideways by a lane offset, so a route rides a
  // carriageway instead of the centre line (which is the median on the boulevard).
  function pushTableRangeOff(pts, tab, sA, sB, off) {
    const a = clamp(sA, 0, tab.len);
    const b = clamp(sB, 0, tab.len);
    const dist = Math.abs(b - a);
    const n = Math.max(1, Math.ceil(dist / tab.step));
    const dir = b >= a ? 1 : -1;
    for (let i = 0; i <= n; i++) {
      tableAt(tab, a + dir * (dist * i) / n, sPos, sTan);
      pushPoint(pts, sPos.x - sTan.z * off, sPos.y, sPos.z + sTan.x * off);
    }
  }

  function pushRingRange(pts, sA, sB) {
    const L = ringTable.len;
    let d = sB - sA;
    if (d > L * 0.5) d -= L;
    else if (d < -L * 0.5) d += L;
    const dist = Math.abs(d);
    const n = Math.max(1, Math.ceil(dist / ringTable.step));
    const dir = d >= 0 ? 1 : -1;
    for (let i = 0; i <= n; i++) {
      tableAt(ringTable, mod(sA + dir * (dist * i) / n, L), sPos, sTan);
      pushPoint(pts, sPos.x, sPos.y, sPos.z);
    }
  }

  // The ring with a lane offset, for routes that must ride a lane instead of the axis.
  function pushRingRangeOff(pts, sA, sB, off) {
    const L = ringTable.len;
    let d = sB - sA;
    if (d > L * 0.5) d -= L;
    else if (d < -L * 0.5) d += L;
    const dist = Math.abs(d);
    const n = Math.max(1, Math.ceil(dist / ringTable.step));
    const dir = d >= 0 ? 1 : -1;
    for (let i = 0; i <= n; i++) {
      tableAt(ringTable, mod(sA + dir * (dist * i) / n, L), sPos, sTan);
      pushPoint(pts, sPos.x - sTan.z * off, sPos.y, sPos.z + sTan.x * off);
    }
  }

  // A pavement walk line: the road curve shifted sideways by the pavement centre offset and
  // baked at pedestrian resolution. road 0 is the ring, road 1 is the avenue.
  function makeWalkLine(tab, off, road, wrap) {
    const pts = [];
    for (let i = 0; i <= tab.n; i++) {
      tableAt(tab, i * tab.step, sPos, sTan);
      pushPoint(pts, sPos.x - sTan.z * off, sPos.y + 0.03, sPos.z + sTan.x * off);
    }
    const line = bakeRoute(pts);
    if (line) {
      line.road = road;
      line.wrap = wrap;
      line.side = off > 0 ? 1 : -1;
    }
    return line;
  }

  function buildWalkLines() {
    walkLines = null;
    if (!ringTable || !aveTable) return;
    const ringR = makeWalkLine(ringTable, RING_PAVE_OFF, 0, true);
    const ringL = makeWalkLine(ringTable, -RING_PAVE_OFF, 0, true);
    const aveR = makeWalkLine(aveTable, AVE_PAVE_OFF, 1, false);
    const aveL = makeWalkLine(aveTable, -AVE_PAVE_OFF, 1, false);
    const out = [];
    if (ringR) out.push(ringR);
    if (ringL) out.push(ringL);
    if (aveR) out.push(aveR);
    if (aveL) out.push(aveL);
    walkLines = out.length ? out : null;
  }

  function pushLineRange(pts, line, sA, sB) {
    const L = line.len;
    let d = sB - sA;
    if (line.wrap) {
      if (d > L * 0.5) d -= L;
      else if (d < -L * 0.5) d += L;
    }
    const dist = Math.abs(d);
    const n = Math.max(1, Math.ceil(dist / line.step));
    const dir = d >= 0 ? 1 : -1;
    for (let i = 0; i <= n; i++) {
      let s = sA + (dir * dist * i) / n;
      s = line.wrap ? mod(s, L) : clamp(s, 0, L);
      tableAt(line, s, sPos, sTan);
      pushPoint(pts, sPos.x, sPos.y, sPos.z);
    }
  }

  function pushLinePoint(pts, line, s) {
    const p = line.wrap ? mod(s, line.len) : clamp(s, 0, line.len);
    tableAt(line, p, sPos, sTan);
    pushPoint(pts, sPos.x, sPos.y, sPos.z);
  }

  // Where a point joins one road family's pavement: closest point on the two lines of that
  // road (0 ring, 1 avenue), with the distance reported.
  const walkProbe = { line: -1, s: 0, d: 0 };
  function nearestWalkOn(px, pz, road) {
    walkProbe.line = -1;
    walkProbe.s = 0;
    walkProbe.d = 0;
    if (!walkLines) return walkProbe;
    let best = Infinity;
    for (let i = 0; i < walkLines.length; i++) {
      if (walkLines[i].road !== road) continue;
      const n = nearestOnTable(walkLines[i], px, pz);
      if (n.d < best) {
        best = n.d;
        walkProbe.line = i;
        walkProbe.s = n.s;
      }
    }
    walkProbe.d = best;
    return walkProbe;
  }

  // Each door point sits on the pavement of one of its quarter's own streets. Walking down
  // that street is walking on pavement, so a trip leaves the door along the street until the
  // street end is closest to the target pavement, then joins there. The two doors of one
  // street fix that street's axis, and the second street's doors fix the cross axis. Anchors
  // are baked for both road families so a trip can be costed either way.
  function buildDoorAnchors() {
    if (!world.quarters || !walkLines) return;
    const ground = typeof world.groundHeightAt === 'function' ? world.groundHeightAt : null;
    for (let q = 0; q < qCap && q < wqCount; q++) {
      const wq = world.quarters[q];
      const dp = wq && wq.doorPoints ? wq.doorPoints : null;
      const n = dp ? Math.min(dp.length, MAX_DOORS) : 0;
      if (n === 0) continue;
      const cx = wq.center ? wq.center.x : dp[0].x;
      const cz = wq.center ? wq.center.z : dp[0].z;
      const radius = wq.radius && wq.radius > 4 ? wq.radius : 20;
      // Cross axis from two doors on different streets, street axis from the two sides of one.
      let vx = 0;
      let vz = 1;
      if (n >= 4) {
        let ax = dp[2].x - dp[0].x;
        let az = dp[2].z - dp[0].z;
        let m = Math.sqrt(ax * ax + az * az);
        if (m > 0.001) {
          vx = ax / m;
          vz = az / m;
        }
      }
      let ux = 1;
      let uz = 0;
      if (n >= 2) {
        let ax = dp[1].x - dp[0].x;
        let az = dp[1].z - dp[0].z;
        const along = ax * vx + az * vz;
        ax -= vx * along;
        az -= vz * along;
        const m = Math.sqrt(ax * ax + az * az);
        if (m > 0.001) {
          ux = ax / m;
          uz = az / m;
        }
      }
      for (let i = 0; i < n; i++) {
        const dx = dp[i].x;
        const dz = dp[i].z;
        const dy = dp[i].y;
        const uOff = (dx - cx) * ux + (dz - cz) * uz;
        const vOff = (dx - cx) * vx + (dz - cz) * vz;
        let half = radius * radius - vOff * vOff;
        half = half > 1 ? Math.sqrt(half) : 1;
        // Never walk past the street end: the street stops at the quarter edge.
        const reach = half * 0.9;
        for (let road = 0; road < 2; road++) {
          const idx = (q * MAX_DOORS + i) * 2 + road;
          let bestScore = Infinity;
          let bx = dx;
          let bz = dz;
          let bestLine = -1;
          let bestS = 0;
          for (let dirI = 0; dirI < 2; dirI++) {
            const sgn = dirI === 0 ? 1 : -1;
            let avail = sgn > 0 ? half - uOff : half + uOff;
            if (avail < 1.5) continue;
            if (avail > reach) avail = reach;
            const stepLen = avail / 4;
            for (let k = 0; k <= 4; k++) {
              const travelled = stepLen * k;
              const px = dx + ux * travelled * sgn;
              const pz = dz + uz * travelled * sgn;
              nearestWalkOn(px, pz, road);
              if (walkProbe.line < 0) continue;
              const score = walkProbe.d + travelled * 0.22;
              if (score < bestScore) {
                bestScore = score;
                bx = px;
                bz = pz;
                bestLine = walkProbe.line;
                bestS = walkProbe.s;
              }
            }
          }
          if (bestLine < 0) {
            nearestWalkOn(dx, dz, road);
            bestLine = walkProbe.line;
            bestS = walkProbe.s;
          }
          anchorLine[idx] = bestLine;
          anchorS[idx] = bestS;
          anchorX[idx] = bx;
          anchorZ[idx] = bz;
          anchorY[idx] = ground ? ground(bx, bz) : dy;
        }
      }
    }
  }

  // Which side of each road a door point stands on, cached once per world.
  function buildDoorSides() {
    if (!aveTable || !ringTable) return;
    for (let q = 0; q < qCap && q < wqCount; q += 1) {
      const wq = world.quarters[q];
      const dp = wq && wq.doorPoints ? wq.doorPoints : null;
      const n = dp ? Math.min(dp.length, MAX_DOORS) : 0;
      for (let i = 0; i < n; i += 1) {
        const idx = q * MAX_DOORS + i;
        doorOffRing[idx] = offsetOn(ringTable, dp[i].x, dp[i].z);
        doorOffAve[idx] = offsetOn(aveTable, dp[i].x, dp[i].z);
      }
    }
  }

  // A finished trip must never enter a carriageway: the stretch a walker actually uses is
  // sampled against both road reserves, so a span that would clip the avenue or the ring is
  // dropped instead of putting a walker on the asphalt.
  function spanOnPavement(route, from, to) {
    const len = to - from;
    const n = Math.max(2, Math.ceil(len / 2.5));
    for (let k = 0; k <= n; k += 1) {
      tableAt(route, from + (len * k) / n, sPos, sTan);
      if (aveTable && nearDistStride(aveTable, sPos.x, sPos.z, 2) < 6.9) return false;
      if (ringTable && nearDistStride(ringTable, sPos.x, sPos.z, 2) < 4.3) return false;
    }
    return true;
  }

  // Resample a polyline into a uniformly spaced table (arc-length parametrised).
  function bakeRoute(pts) {
    const count = pts.length / 3;
    if (count < 2) return null;
    const cum = new Float32Array(count);
    let total = 0;
    for (let i = 1; i < count; i++) {
      const dx = pts[i * 3] - pts[i * 3 - 3];
      const dy = pts[i * 3 + 1] - pts[i * 3 - 2];
      const dz = pts[i * 3 + 2] - pts[i * 3 - 1];
      total += Math.sqrt(dx * dx + dy * dy + dz * dz);
      cum[i] = total;
    }
    if (total < 4) return null;
    const n = Math.max(4, Math.ceil(total / ROUTE_SPACING));
    const step = total / n;
    const pos = new Float32Array((n + 1) * 3);
    const tan = new Float32Array((n + 1) * 3);
    let src = 1;
    for (let k = 0; k <= n; k++) {
      const d = k * step;
      while (src < count - 1 && cum[src] < d) src++;
      const d0 = cum[src - 1];
      const d1 = cum[src];
      const seg = d1 - d0;
      const f = seg > 1e-6 ? (Math.min(d, d1) - d0) / seg : 0;
      const a = (src - 1) * 3;
      const b = src * 3;
      pos[k * 3] = pts[a] + (pts[b] - pts[a]) * f;
      pos[k * 3 + 1] = pts[a + 1] + (pts[b + 1] - pts[a + 1]) * f;
      pos[k * 3 + 2] = pts[a + 2] + (pts[b + 2] - pts[a + 2]) * f;
    }
    for (let k = 0; k <= n; k++) {
      const i0 = k > 0 ? k - 1 : 0;
      const i1 = k < n ? k + 1 : n;
      const dx = pos[i1 * 3] - pos[i0 * 3];
      const dy = pos[i1 * 3 + 1] - pos[i0 * 3 + 1];
      const dz = pos[i1 * 3 + 2] - pos[i0 * 3 + 2];
      const m = Math.sqrt(dx * dx + dy * dy + dz * dz) || 1;
      tan[k * 3] = dx / m;
      tan[k * 3 + 1] = dy / m;
      tan[k * 3 + 2] = dz / m;
    }
    return { pos: pos, tan: tan, n: n, len: total, step: step };
  }

  function doorOf(q, which) {
    const wq = world.quarters[q];
    if (!wq) return null;
    const dp = wq.doorPoints;
    if (dp && dp.length) return dp[which % dp.length];
    return wq.center || null;
  }

  // Delivery route for a van: the road centre lines, from one quarter's door to another's.
  function buildVanRoute(a, b) {
    if (!aveTable || !ringTable) return null;
    const doorA = doorOf(a, 0);
    const doorB = doorOf(b, 1);
    if (!doorA || !doorB) return null;
    const kindA = accessKind[a];
    const kindB = accessKind[b];
    const pts = [];
    const aveOff = AVE_VAN_OFF;
    const ringOff = RING_VAN_OFF;
    pushPoint(pts, doorA.x, doorA.y + 0.04, doorA.z);
    if (kindA === 0) pushTableRangeOff(pts, aveTable, accessS[a], accessS[a], aveOff);
    else pushTableRangeOff(pts, ringTable, accessS[a], accessS[a], ringOff);
    if (kindA === kindB) {
      if (kindA === 0) pushTableRangeOff(pts, aveTable, accessS[a], accessS[b], aveOff);
      else pushRingRangeOff(pts, accessS[a], accessS[b], ringOff);
    } else if (kindA === 0) {
      pushTableRangeOff(pts, aveTable, accessS[a], 0, aveOff);
      pushRingRangeOff(pts, junctionS, accessS[b], ringOff);
    } else {
      pushRingRangeOff(pts, accessS[a], junctionS, ringOff);
      pushTableRangeOff(pts, aveTable, 0, accessS[b], aveOff);
    }
    if (kindB === 0) pushTableRangeOff(pts, aveTable, accessS[b], accessS[b], aveOff);
    else pushTableRangeOff(pts, ringTable, accessS[b], accessS[b], ringOff);
    pushPoint(pts, doorB.x, doorB.y + 0.04, doorB.z);
    return bakeRoute(pts);
  }

  function buildVanRoutes() {
    if (!aveTable || !ringTable || qN < 2) {
      routes = null;
      return;
    }
    const n = qN * qN;
    routes = new Array(n);
    for (let i = 0; i < n; i++) routes[i] = null;
    for (let a = 0; a < qN; a++) {
      for (let b = 0; b < qN; b++) {
        if (a === b) continue;
        routes[a * qN + b] = buildVanRoute(a, b);
      }
    }
  }

  // Pavement distance between two anchors on one road family, used to cost a trip.
  function arcBetween(la, sA, sB) {
    let d = sB - sA;
    if (la.wrap) {
      const L = la.len;
      if (d > L * 0.5) d -= L;
      else if (d < -L * 0.5) d += L;
    }
    return d < 0 ? -d : d;
  }

  function anchorJoinCost(idx, door) {
    const line = walkLines[anchorLine[idx]];
    if (!line) return Infinity;
    tableAt(line, anchorS[idx], sPos, sTan);
    const dx = anchorX[idx] - door.x;
    const dz = anchorZ[idx] - door.z;
    const ex = sPos.x - anchorX[idx];
    const ez = sPos.z - anchorZ[idx];
    return Math.sqrt(dx * dx + dz * dz) + Math.sqrt(ex * ex + ez * ez);
  }

  // A metre walked between a door and the ring/avenue pavement is a street walk plus a short
  // crossing, not a pavement stroll, so it counts for three metres when picking a spine.
  const JOIN_WEIGHT = 3;

  function tripCost(a, iA, b, iB, road) {
    const ia = (a * MAX_DOORS + iA) * 2 + road;
    const ib = (b * MAX_DOORS + iB) * 2 + road;
    if (anchorLine[ia] < 0 || anchorLine[ib] < 0) return Infinity;
    const la = walkLines[anchorLine[ia]];
    const lb = walkLines[anchorLine[ib]];
    if (!la || !lb) return Infinity;
    let cost = (anchorJoinCost(ia, world.quarters[a].doorPoints[iA]) +
      anchorJoinCost(ib, world.quarters[b].doorPoints[iB])) * JOIN_WEIGHT;
    cost += arcBetween(la, anchorS[ia], anchorS[ib]);
    return cost;
  }

  // A pedestrian trip: door point of quarter A to door point of quarter B. Both anchors must
  // land on the same pavement line, because a trip never jaywalks: a pair whose doors face
  // opposite kerbs is served by the zebra crossing cohort instead. Several door pairs are
  // costed and the cheapest pavement walk wins.
  function buildPedRoute(a, b) {
    if (!walkLines) return null;
    const dpA = world.quarters[a] ? world.quarters[a].doorPoints : null;
    const dpB = world.quarters[b] ? world.quarters[b].doorPoints : null;
    const nA = dpA && dpA.length ? Math.min(dpA.length, MAX_DOORS) : 0;
    const nB = dpB && dpB.length ? Math.min(dpB.length, MAX_DOORS) : 0;
    if (nA === 0 || nB === 0) return null;
    let bestCost = Infinity;
    let bestRoad = -1;
    let bestA = 0;
    let bestB = 0;
    for (let road = 0; road < 2; road++) {
      for (let k = 0; k < 4; k++) {
        const iA = (a * 5 + b * 3 + 1 + k * 2) % nA;
        const iB = (b * 7 + a * 2 + 2 + k * 3) % nB;
        const idxA = (a * MAX_DOORS + iA) * 2 + road;
        const idxB = (b * MAX_DOORS + iB) * 2 + road;
        if (anchorLine[idxA] < 0 || anchorLine[idxB] < 0) continue;
        if (anchorLine[idxA] !== anchorLine[idxB]) continue;
        const lineSide = walkLines[anchorLine[idxA]].side;
        const offA = road === 0 ? doorOffRing[a * MAX_DOORS + iA] : doorOffAve[a * MAX_DOORS + iA];
        const offB = road === 0 ? doorOffRing[b * MAX_DOORS + iB] : doorOffAve[b * MAX_DOORS + iB];
        if (offA * lineSide < -0.5 || offB * lineSide < -0.5) continue;
        const cost = tripCost(a, iA, b, iB, road);
        if (cost < bestCost) {
          bestCost = cost;
          bestRoad = road;
          bestA = iA;
          bestB = iB;
        }
      }
    }
    if (bestRoad < 0) return null;
    const idxA = (a * MAX_DOORS + bestA) * 2 + bestRoad;
    const idxB = (b * MAX_DOORS + bestB) * 2 + bestRoad;
    const la = walkLines[anchorLine[idxA]];
    const doorA = dpA[bestA];
    const doorB = dpB[bestB];
    const sA = anchorS[idxA];
    const sB = anchorS[idxB];
    const pts = [];
    pushPoint(pts, doorA.x, doorA.y + 0.04, doorA.z);
    pushPoint(pts, anchorX[idxA], anchorY[idxA] + 0.04, anchorZ[idxA]);
    pushLinePoint(pts, la, sA);
    pushLineRange(pts, la, sA, sB);
    pushPoint(pts, anchorX[idxB], anchorY[idxB] + 0.04, anchorZ[idxB]);
    pushPoint(pts, doorB.x, doorB.y + 0.04, doorB.z);
    const route = bakeRoute(pts);
    if (!route) return null;
    // Arc span where the trip is actually on the pavement, so a walker can appear there
    // rather than at the kerb end of the trip.
    const joinA = anchorJoinCost(idxA, doorA);
    const joinB = anchorJoinCost(idxB, doorB);
    let pavA = joinA < route.len * 0.45 ? joinA : route.len * 0.45;
    let pavB = route.len - joinB;
    if (pavB < pavA + 2) {
      pavA = route.len * 0.35;
      pavB = route.len * 0.65;
    }
    // The walk itself is only ever the pavement stretch between the two joins: the door
    // approach is left to the world's own street paving, so no walker ever steps onto a
    // carriageway. A span that clips a road reserve drops the pair.
    if (!spanOnPavement(route, pavA, pavB)) return null;
    route.pavA = pavA;
    route.pavB = pavB;
    return route;
  }

  function buildPedRouteTable() {
    if (!walkLines || qN < 2) {
      pedRoutes = null;
      return;
    }
    const n = qN * qN;
    if (!pedRoutes || routeQ !== qN) {
      pedRoutes = new Array(n);
      routeQ = qN;
    }
    for (let a = 0; a < qN; a++) {
      for (let b = 0; b < qN; b++) {
        pedRoutes[a * qN + b] = a === b ? null : buildPedRoute(a, b);
      }
    }
  }

  function buildAccess() {
    if (!aveTable || !ringTable) return;
    for (let q = 0; q < qN; q++) {
      const wq = world.quarters[q];
      if (!wq || !wq.center) {
        accessKind[q] = 0;
        accessS[q] = 0;
        continue;
      }
      const na = nearestOnTable(aveTable, wq.center.x, wq.center.z);
      const nr = nearestOnTable(ringTable, wq.center.x, wq.center.z);
      if (nr.d < na.d) {
        accessKind[q] = 1;
        accessS[q] = nr.s;
      } else {
        accessKind[q] = 0;
        accessS[q] = na.s;
      }
    }
  }

  function buildPedRoutes() {
    const out = [];
    const pairs = modelRef && modelRef.pairs ? modelRef.pairs : null;
    if (pedRoutes && pairs) {
      for (let i = 0; i < pairs.length && out.length < 64; i++) {
        const p = pairs[i];
        const a = p && typeof p.a === 'number' ? p.a | 0 : -1;
        const b = p && typeof p.b === 'number' ? p.b | 0 : -1;
        if (a < 0 || b < 0 || a === b || a >= qN || b >= qN) continue;
        const strength = p.strength && p.strength > 0 ? p.strength : 1;
        const repeats = Math.min(8, Math.max(1, Math.round(strength)));
        for (let k = 0; k < repeats && out.length < 64; k++) {
          if (pedRoutes[a * qN + b]) out.push(a * qN + b);
          if (pedRoutes[b * qN + a] && out.length < 64) out.push(b * qN + a);
        }
      }
    }
    if (out.length === 0 && pedRoutes) {
      for (let i = 0; i < qN * qN && out.length < 64; i++) {
        if (pedRoutes[i]) out.push(i);
      }
    }
    pedRouteList = new Int32Array(out.length);
    for (let i = 0; i < out.length; i++) pedRouteList[i] = out[i];
  }

  function buildParcelOrder() {
    const parcels = modelRef && modelRef.parcels ? modelRef.parcels : null;
    if (!parcels || parcels.length === 0) {
      parcelOrder = null;
      parcelCursor = 0;
      return;
    }
    const idx = new Int32Array(parcels.length);
    for (let i = 0; i < parcels.length; i++) idx[i] = i;
    // Insertion sort by at (arrays are small and this runs twice per load).
    for (let i = 1; i < idx.length; i++) {
      const v = idx[i];
      const at = parcels[v] && typeof parcels[v].at === 'number' ? parcels[v].at : 0;
      let j = i - 1;
      while (j >= 0) {
        const u = idx[j];
        const au = parcels[u] && typeof parcels[u].at === 'number' ? parcels[u].at : 0;
        if (au <= at) break;
        idx[j + 1] = u;
        j--;
      }
      idx[j + 1] = v;
    }
    parcelOrder = idx;
    parcelCursor = 0;
  }

  // --------------------------------------------------------------- crossings
  // Zebra crossings, straight from the world: x, z on the avenue axis, dirX / dirZ along the
  // avenue, halfSpan from pavement to pavement. The walk line runs perpendicular to the
  // road, so a crosser never leaves the paint band the world cut into its median.
  const groundY = typeof world.groundHeightAt === 'function'
    ? world.groundHeightAt
    : function () { return 0; };
  let crossLines = null;
  let crossCount = 0;

  function buildCrossings() {
    const out = [];
    const list = world && world.crossings && world.crossings.length ? world.crossings : null;
    if (list) {
      for (let i = 0; i < list.length; i++) {
        const c = list[i];
        if (!c || typeof c.x !== 'number' || typeof c.z !== 'number') continue;
        let dx = typeof c.dirX === 'number' ? c.dirX : 0;
        let dz = typeof c.dirZ === 'number' ? c.dirZ : 1;
        const m = Math.sqrt(dx * dx + dz * dz);
        if (m < 1e-6) {
          dx = 0;
          dz = 1;
        } else {
          dx /= m;
          dz /= m;
        }
        const half = typeof c.halfSpan === 'number' ? c.halfSpan : AVE_PAVE_OFF;
        const px = -dz;
        const pz = dx;
        const pts = [];
        for (let k = 0; k <= 4; k++) {
          const t = (k / 4) * 2 - 1;
          const x = c.x + px * half * t;
          const z = c.z + pz * half * t;
          pushPoint(pts, x, groundY(x, z) + 0.22, z);
        }
        const route = bakeRoute(pts);
        if (route) {
          route.crossHalf = typeof c.halfWidth === 'number' ? c.halfWidth : 2.1;
          out.push(route);
        }
      }
    } else if (aveTable) {
      // Fallback for a world without a crossings list: the zebras the district paints sit
      // where the cross-street grid meets the avenue. One walk line per painted stripe band.
      const grid = [-100, -68, -36, -8, 26, 58, 90];
      for (let g = 0; g < grid.length; g++) {
        let best = -1;
        let bd = Infinity;
        for (let i = 0; i <= aveTable.n; i++) {
          const d = Math.abs(aveTable.pos[i * 3 + 2] - grid[g]);
          if (d < bd) {
            bd = d;
            best = i;
          }
        }
        if (best < 0 || bd > 3) continue;
        const cx = aveTable.pos[best * 3];
        const cz = aveTable.pos[best * 3 + 2];
        if (world.roundabout && world.roundabout.center) {
          const dx = cx - world.roundabout.center.x;
          const dz = cz - world.roundabout.center.z;
          const rr = (world.roundabout.radius || 15) + 2;
          if (dx * dx + dz * dz < rr * rr) continue;
        }
        if (world.marketSquare && world.marketSquare.center) {
          const dzp = cz - world.marketSquare.center.z;
          if (Math.abs(dzp) < 12) continue;
        }
        let tx = aveTable.tan[best * 3];
        let tz = aveTable.tan[best * 3 + 2];
        const tm = Math.sqrt(tx * tx + tz * tz) || 1;
        tx /= tm;
        tz /= tm;
        const px = -tz;
        const pz = tx;
        const pts = [];
        for (let k = 0; k <= 4; k++) {
          const t = (k / 4) * 2 - 1;
          const x = cx + px * AVE_PAVE_OFF * t;
          const z = cz + pz * AVE_PAVE_OFF * t;
          pushPoint(pts, x, groundY(x, z) + 0.22, z);
        }
        const route = bakeRoute(pts);
        if (route) {
          route.crossHalf = 2.1;
          out.push(route);
        }
      }
    }
    crossLines = out.length ? out : null;
    crossCount = out.length;
  }

  // ----------------------------------------------------------------- parking
  // Static cars along the ring kerbs and the spur kerbs, aligned to the road, spaced by a
  // car length plus a margin. World already parks on the secondary streets and on the ring,
  // so its own parked mesh is read first and any occupied spot is skipped.
  let takenXZ = new Float32Array(0);
  let takenN = 0;

  function collectParkedSpots() {
    takenN = 0;
    let mesh = null;
    if (world && world.group && typeof world.group.getObjectByName === 'function') {
      mesh = world.group.getObjectByName('parked');
    }
    if (!mesh || !mesh.instanceMatrix || !mesh.instanceMatrix.array) return;
    const arr = mesh.instanceMatrix.array;
    const cap = Math.min((arr.length / 16) | 0, mesh.count ? mesh.count | 0 : (arr.length / 16) | 0);
    takenXZ = new Float32Array(cap * 2);
    for (let i = 0; i < cap; i++) {
      const o = i * 16 + 12;
      takenXZ[i * 2] = arr[o];
      takenXZ[i * 2 + 1] = arr[o + 2];
    }
    takenN = cap;
  }

  function spotTaken(x, z, r) {
    const rr = r * r;
    for (let i = 0; i < takenN; i++) {
      const dx = takenXZ[i * 2] - x;
      const dz = takenXZ[i * 2 + 1] - z;
      if (dx * dx + dz * dz < rr) return true;
    }
    return false;
  }

  let parkCount = 0;

  function buildParking() {
    collectParkedSpots();
    parkCount = 0;
    if (ringTable) {
      const step = 6.9;
      const slots = Math.floor(ringLen / step);
      for (let k = 0; k < slots && parkCount < PARK_CAP; k++) {
        if (hash01(k * 7717 + 13) < PARK_SKIP_RING) continue;
        const ss = mod(k * step + 2.4, ringLen);
        let dj = ss - junctionS;
        const halfL = ringLen * 0.5;
        if (dj > halfL) dj -= ringLen;
        else if (dj < -halfL) dj += ringLen;
        if (dj < 0) dj = -dj;
        if (dj < 26) continue;
        tableAt(ringTable, ss, sPos, sTan);
        // Both kerbs on two slots out of four, one kerb on the rest: a kerb that is never
        // empty for long, and never double parked.
        const lanes = (k & 3) < 2 ? 2 : 1;
        for (let sideIdx = 0; sideIdx < lanes && parkCount < PARK_CAP; sideIdx += 1) {
          const side = ((k & 1) ? 1 : -1) * (sideIdx === 0 ? 1 : -1);
          const x = sPos.x - sTan.z * RING_LOT_OFF * side;
          const z = sPos.z + sTan.x * RING_LOT_OFF * side;
          if (world.roundabout && world.roundabout.center) {
            const dx = x - world.roundabout.center.x;
            const dz = z - world.roundabout.center.z;
            const rr = (world.roundabout.radius || 15) + 8;
            if (dx * dx + dz * dz < rr * rr) continue;
          }
          if (spotTaken(x, z, 4.8)) continue;
          const sc = 0.94 + hash01(k * 613 + sideIdx * 29) * 0.1;
          const col = carPalette[(hash01(k * 97 + sideIdx * 53) * carPalette.length) | 0];
          writeMatrix(parked, parkCount, x, sPos.y + 0.05, z,
            Math.atan2(sTan.x, sTan.z), sc, sc, sc);
          parked.setColorAt(parkCount, col);
          parkCount += 1;
        }
      }
    }
    for (let t = 0; t < spurTables.length && parkCount < PARK_CAP; t += 1) {
      if (!spurParkOn[t]) continue;
      const tab = spurTables[t];
      const off = spurParkOff[t];
      const step = 6.9;
      const slots = Math.floor(tab.len / step);
      for (let k = 1; k < slots && parkCount < PARK_CAP; k += 1) {
        if (hash01(t * 4099 + k * 37 + 3) < PARK_SKIP_SPUR) continue;
        tableAt(tab, k * step, sPos, sTan);
        const ang = Math.atan2(sTan.x, sTan.z);
        // Both kerbs: a ten metre carriageway leaves the two lanes and the gaps clear.
        for (let sideIdx = 0; sideIdx < 2 && parkCount < PARK_CAP; sideIdx += 1) {
          const sgn = sideIdx === 0 ? 1 : -1;
          const x = sPos.x - sTan.z * off * sgn;
          const z = sPos.z + sTan.x * off * sgn;
          if (spotTaken(x, z, 4.8)) continue;
          const sc = 0.94 + hash01(t * 71 + k * 613 + sideIdx * 41) * 0.1;
          const col = carPalette[(hash01(t * 31 + k * 97 + sideIdx * 17) * carPalette.length) | 0];
          writeMatrix(parked, parkCount, x, sPos.y + 0.05, z, ang, sc, sc, sc);
          parked.setColorAt(parkCount, col);
          parkCount += 1;
        }
      }
    }
    for (let i = parkCount; i < PARK_CAP; i++) hideAt(parked, i);
    parked.count = parkCount;
    dirtyAllColour(parked);
    dirtyAllMatrix(parked);
  }

  // ------------------------------------------------------------- car pools
  const ringS = new Float32Array(ringCap);
  const ringV = new Float32Array(ringCap);
  const ringLane = new Float32Array(ringCap);
  const ringAct = new Uint8Array(ringCap);
  let ringShown = 0;
  let ringDrawn = 0;

  const aveS = new Float32Array(aveCap);
  const aveV = new Float32Array(aveCap);
  const aveLane = new Float32Array(aveCap);
  const aveDir = new Int8Array(aveCap);
  const aveAct = new Uint8Array(aveCap);
  let aveShown = 0;
  let aveDrawn = 0;

  const spurS = new Float32Array(spurCap);
  const spurV = new Float32Array(spurCap);
  const spurLane = new Float32Array(spurCap);
  const spurDir = new Int8Array(spurCap);
  const spurTab = new Int32Array(spurCap);
  const spurAct = new Uint8Array(spurCap);
  let spurShown = 0;
  let spurDrawn = 0;

  const expLane = new Int32Array(expCap);
  const expAct = new Uint8Array(expCap);
  const expLaneBase = new Float32Array(EXP_LANES);
  const expLaneDir = new Int8Array(EXP_LANES);
  const expLaneOff = new Float32Array(EXP_LANES);
  // Metres between two cars of one lane this frame, from the headway below and capped by the
  // space the live cars can share without overlapping. Recomputed every step, no allocation.
  let expSpacing = EXP_MIN_GAP;
  let expShown = 0;
  let expDrawn = 0;

  // One occupancy grid per family that integrates its own cars. The bin counts follow the arc
  // lengths and the lane index of each car is fixed by its lane offset, so the lane a car
  // belongs to never changes after it is built. Every lane is a loop in its own arc
  // coordinate: the ring is a real loop, and the avenue and the spurs re-enter a car at the
  // other end of their table, which the arc coordinate already models as a seam between s =
  // len and s = 0. That is what lets the gap law treat the fixed count as continuous traffic
  // instead of teleporting a car into a hole it would have to hunt for.
  const ringLaneLen = new Float32Array(2);
  ringLaneLen[0] = ringLen;
  ringLaneLen[1] = ringLen;
  const ringLaneDir = new Int8Array(2);
  ringLaneDir[0] = 1;
  ringLaneDir[1] = 1;
  const ringGrid = makeLaneGrid(ringCap, ringLaneLen, ringLaneDir,
    new Int32Array(ringCap), ringS);

  const aveLaneLen = new Float32Array(2);
  aveLaneLen[0] = aveLen;
  aveLaneLen[1] = aveLen;
  // Lane 0 is the west carriageway (dir +1 up the avenue), lane 1 the east one (dir -1 down).
  const aveLaneDir = new Int8Array(2);
  aveLaneDir[0] = 1;
  aveLaneDir[1] = -1;
  const aveGrid = makeLaneGrid(aveCap, aveLaneLen, aveLaneDir,
    new Int32Array(aveCap), aveS);

  const SPUR_LANES = Math.max(2, spurTables.length * 2);
  const spurLaneLen = new Float32Array(SPUR_LANES);
  for (let t = 0; t < spurTables.length; t++) {
    spurLaneLen[t * 2] = spurTables[t].len;
    spurLaneLen[t * 2 + 1] = spurTables[t].len;
  }
  // Per table the even lane runs dir -1 and the odd lane dir +1, the two sides of the arterial.
  const spurLaneDir = new Int8Array(SPUR_LANES);
  for (let l = 0; l < SPUR_LANES; l++) spurLaneDir[l] = (l & 1) ? 1 : -1;
  const spurGrid = makeLaneGrid(spurCap, spurLaneLen, spurLaneDir,
    new Int32Array(spurCap), spurS);

  // --------------------------------------------------------- junction crossings
  // The crossing points are read out of the road tables, never authored: for each avenue
  // carriageway the build walks its own lane centre (the same lateral offset the cars hold) and
  // keeps the arc where that lane crosses the ring centreline. The lateral offset of the lane
  // from the ring changes sign exactly once on the whole avenue, at the northwest lobe, so the
  // crossing is found by the sign change and then bisected to a centimetre. A spur crosses the
  // ring at the first point of its own table, so its crossing arc is the seam at s = 0. One
  // entry per avenue side (index 0 is the west carriageway, index 1 the east, matching the sign
  // of aveLane), one per spur table; -1 marks a crossing the build could not find, and a car on
  // that side then does not yield at all.
  const aveCrossS = new Float32Array(2);
  const aveCrossRingS = new Float32Array(2);
  aveCrossS[0] = -1;
  aveCrossS[1] = -1;
  aveCrossRingS[0] = -1;
  aveCrossRingS[1] = -1;
  const spurCrossRingS = new Float32Array(spurN);
  spurCrossRingS.fill(-1);
  (function buildJunctions() {
    if (!ringTable || !aveTable || aveTable.len <= 1) return;
    for (let side = 0; side < 2; side++) {
      const off = side === 0 ? -AVE_CAR_LANE : AVE_CAR_LANE;
      let prevO = 0;
      let prevS = -1;
      for (let i = 0; i <= aveTable.n && aveCrossS[side] < 0; i++) {
        const s = i * aveTable.step;
        tableAt(aveTable, s, sPos, sTan);
        const lx = sPos.x - sTan.z * off;
        const lz = sPos.z + sTan.x * off;
        const near = nearestOnTable(ringTable, lx, lz);
        tableAt(ringTable, near.s, sPos, sTan);
        // The same signed offset the walk lines use, positive on the same side.
        const o = -(lx - sPos.x) * sTan.z + (lz - sPos.z) * sTan.x;
        const flip = prevS >= 0 && o !== 0 && prevO !== 0 && ((o < 0) !== (prevO < 0));
        if (flip) {
          let lo = prevS;
          let hi = s;
          for (let k = 0; k < 22; k++) {
            const mid = (lo + hi) * 0.5;
            tableAt(aveTable, mid, sPos, sTan);
            const mx = sPos.x - sTan.z * off;
            const mz = sPos.z + sTan.x * off;
            const mn = nearestOnTable(ringTable, mx, mz);
            tableAt(ringTable, mn.s, sPos, sTan);
            const mo = -(mx - sPos.x) * sTan.z + (mz - sPos.z) * sTan.x;
            if ((mo < 0) === (o < 0)) hi = mid;
            else lo = mid;
          }
          aveCrossS[side] = (lo + hi) * 0.5;
          tableAt(aveTable, aveCrossS[side], sPos, sTan);
          const cx = sPos.x - sTan.z * off;
          const cz = sPos.z + sTan.x * off;
          const cn = nearestOnTable(ringTable, cx, cz);
          if (cn.d <= JUNCTION_MEET) aveCrossRingS[side] = cn.s;
          else aveCrossS[side] = -1;
        }
        prevO = o;
        prevS = s;
      }
    }
    for (let t = 0; t < spurTables.length; t++) {
      const tab = spurTables[t];
      if (!tab || tab.n <= 0) continue;
      const n = nearestOnTable(ringTable, tab.pos[0], tab.pos[2]);
      if (n.d <= JUNCTION_MEET) spurCrossRingS[t] = n.s;
    }
  })();

  // Metres from the crossing's ring arc back to the nearest ring car, 0 when one is on the
  // crossing or still inside the tail, or -1 when the ring is clear inside the scan. The ring's
  // two lanes share one arc table but each lane's bin chain is its own, so both are walked; only
  // the bins that cover the scan and the tail are touched, and every candidate is then rejected
  // or accepted by its own arc distance, so a car of the neighbouring bin is neither missed nor
  // counted twice. Nothing is allocated: the ring grid is refilled each frame by the ring step,
  // which runs before both yielding pools, so the lookup sees this frame's ring positions.
  function ringGap(sCross, window) {
    const g = ringGrid;
    if (ringLen <= 0) return -1;
    const scan = typeof window === 'number' ? window : JUNCTION_SCAN_M;
    let best = -1;
    for (let l = 0; l < 2; l++) {
      const nb = g.laneBins[l];
      if (nb <= 0) continue;
      const base = g.laneBase[l];
      const b0 = gridBin(g, l, mod(sCross - JUNCTION_TAIL, ringLen));
      for (let k = 0; k <= JUNCTION_SCAN_BINS + 2; k++) {
        const bb = mod(b0 + k, nb);
        for (let j = g.head[base + bb]; j >= 0; j = g.next[j]) {
          const d = mod(ringS[j] - sCross, ringLen);
          if (d <= scan) {
            if (best < 0 || d < best) best = d;
          } else if (mod(sCross - ringS[j], ringLen) <= JUNCTION_TAIL) {
            return 0;
          }
        }
      }
    }
    return best;
  }

  // The yield limit for one lower-road car, in metres per second, or Infinity when it owes
  // nothing: outside the reach, past the crossing, or with the ring far enough back and a clear
  // exit. A car may pass the stop line when the nearest ring car is further away than the ring
  // could travel in the time this car needs to get clear of the crossing, so the shortcut is
  // scaled by how fast this car is actually going: a car at lane speed asks for a short gap, a
  // crawling one for a long gap, and neither is let through in front of a ring car that would
  // arrive first. The second condition is the car's own lane: it needs the car ahead to stand a
  // follow gap beyond JUNCTION_PAST, otherwise a queue stopping just past the stop line would
  // leave it standing on the ring's lane. A car already inside JUNCTION_GAP is committed and
  // must clear instead of stopping there, and the clamp reaches zero exactly at JUNCTION_GAP, so
  // a car that sees either block as it approaches is brought to rest on the stop point.
  // free is the speed this car would carry into the crossing without the yield.
  function junctionLimit(s, dir, sCross, sRing, len, gap, free) {
    if (sCross < 0 || len <= 0) return Infinity;
    const ahead = mod(dir * (sCross - s), len);
    if (ahead > JUNCTION_REACH || ahead <= JUNCTION_GAP) return Infinity;
    let blocked = false;
    const rg = ringGap(sRing);
    if (rg >= 0) {
      const vClear = free > JUNCTION_MIN_V ? free : JUNCTION_MIN_V;
      const need = ((ahead + JUNCTION_PAST) / vClear + JUNCTION_MARGIN) * RING_SPEED;
      if (rg < need) blocked = true;
    }
    if (!blocked && gap >= 0 && gap < ahead + JUNCTION_EXIT) blocked = true;
    if (!blocked) return Infinity;
    const lim = (ahead - JUNCTION_GAP) / JUNCTION_HEADWAY;
    return lim > 0 ? lim : 0;
  }

  // The speed ceiling a lamp puts on a car, in the same law the lane clamp already uses: the
  // stop line is JUNCTION_GAP before the crossing centre, and the ceiling reaches zero exactly
  // there. A car already inside JUNCTION_GAP is committed and returns Infinity: it is past the
  // point where it could stop without standing on the box, so it clears. Amber counts as not
  // green, so both roads hold on amber; the one that is leaving is already inside the zone and
  // clears, the one that is waiting stays at the line.
  //
  // A green light adds the box rule and one time check. The box rule is the existing one: the
  // leader must already stand JUNCTION_EXIT past the crossing, so a green car cannot be parked
  // on the exit side by the queue ahead. The time check keeps the amber promise: a car may pass
  // the line only if the distance it still has to cover, including JUNCTION_CLEAR past the
  // centre, fits in the green that is left plus the amber. A slow car is therefore held at the
  // line instead of being committed into a red it could not clear.
  function signalClamp(ahead, gap, green, greenLeft, free) {
    if (ahead <= JUNCTION_GAP) return Infinity;
    const lim = (ahead - JUNCTION_GAP) / JUNCTION_HEADWAY;
    if (!green) return lim;
    if (gap >= 0 && gap < ahead + JUNCTION_EXIT) return lim;
    const vv = free > 0.6 ? free : 0.6;
    const need = (ahead + JUNCTION_CLEAR) / vv;
    if (need > greenLeft + SIGNAL_AMBER_S) return lim;
    return Infinity;
  }

  // True when a ring car still stands on a crossing or just past it. The avenue's green starts
  // three seconds after the ring's amber, and this is the second belt: the avenue does not
  // commit into a box a crawling ring car has not fully cleared. The scan is the short window,
  // so a queued ring car on the approach is not mistaken for one in the box.
  function ringBoxBusy(sCross) {
    return ringGap(sCross, JUNCTION_CLEAR) >= 0;
  }

  // Vehicle kinds and liveries, fixed by index so a shrinking pool never repaints.
  const vehKind = new Uint8Array(CAR_CAP);
  const vehSX = new Float32Array(CAR_CAP);
  const vehSY = new Float32Array(CAR_CAP);
  const vehSZ = new Float32Array(CAR_CAP);
  const vehPalIdx = new Uint8Array(CAR_CAP);
  const vehPalLen = new Uint8Array(CAR_CAP);
  const riderDrawn = new Uint8Array(CAR_CAP);

  // ------------------------------------------------------------- pack fleet
  // The moving fleet can wear the low car models of the CC0 car pack instead of the 24
  // triangle box. A model is authored in metres, facing +Z, with its origin at the ground
  // contact, which is exactly the frame writeMatrix already writes, so one model is one
  // scaled matrix through the same write path and every motion rule is untouched.
  //
  // A model is 1.89 to 2.15 m wide and 3.65 to 4.65 m long while the procedural body is
  // 1.92 x 4.1 and the follow gap (4.6 m centre to centre) is sized against that 4.1 m. So
  // every model is fitted by one uniform factor, min(1, 1.92 / width, 4.1 / length), which
  // puts its body strictly inside the box the body length assumption was measured on: no
  // lane offset, follow gap or junction clearance moves, and no two bodies can touch that
  // could not touch before. The factor never exceeds 1, so a model is never stretched.
  //
  // Which slots wear a model is a pool decision, not an index one: the pool nearest the
  // camera is the avenue carriageway, and every slot of it that is not a scooter is modelled.
  // The ring, the spurs, the expressway and the whole parked rank keep the procedural body.
  // The phone stays procedural through the loader itself: it merges the district tier only
  // and the car kit is street tier, so this table stays empty there and nothing is added.
  const MODEL_NAMES = [
    'sedan-low', 'suv-low', 'hatchback-sports-low', 'taxi-low',
    'van-low', 'delivery-low', 'truck-low'
  ];
  // Per kind, the models that may stand in for it, as indices into MODEL_NAMES, in the order
  // a slot hash walks them. A kind with no list (a scooter) keeps the procedural body.
  const MODEL_OF_CAR = [0, 1, 2, 3];
  const MODEL_OF_VAN = [4, 5];
  const MODEL_OF_TRUCK = [6];
  const MODEL_MAX = MODEL_NAMES.length;
  // The modelled pool: the avenue's slots, the pool that runs from the roundabout at the
  // bottom of the frame to the hilltop and the one the street tier reads most. Every other
  // pool keeps the procedural body.
  const MODEL_POOL_BASE = aveBase;
  const MODEL_POOL_CAP = aveCap;
  // The avenue carries two carriageways, one per slot parity: initAvenueTable sets the lane
  // offset from `(i & 1) ? 1 : -1`, so a parity is a whole carriageway, both ends of it. One
  // carriageway wears the pack bodies and the other keeps the procedural car: 35 modelled
  // bodies of 500 triangles cost 17,476 where 63 cost 31,472, and the 63 case measured
  // 395,851 rendered triangles at the busiest hour against this build's ceiling of 385,000.
  // Both carriageways are modelled by setting MODEL_LANE to -1 (accept every slot).
  const MODEL_LANE = 1;
  // Cycle 21 partway fleet: the pack carriageway keeps every slot, and the procedural
  // carriageway joins it one slot in MODEL_LANE_EVERY (its slot ids are all even, so the
  // modulo only ever admits far-lane slots, never a second pack slot). Measured on the
  // day frame: +10 modelled bodies at about 500 triangles each against the 390,000 line.
  // Main's integration call, 9 November: the no-partway base measured 387,710 day /
  // 388,018 street on the assembled cycle 21 tree, so 24 fits both (388,752 / 389,060)
  // with the bench repopulation peak at 397,892 against the 400,000 hard gate. 9999 = off.
  const MODEL_LANE_EVERY = 24;
  const modelList = [];
  const modelMeshes = [];
  const modelDefs = [];
  const modelCount = new Int32Array(MODEL_MAX);
  const modelCursor = new Int32Array(MODEL_MAX);
  const modelSpanLo = new Int32Array(MODEL_MAX);
  const modelSpanHi = new Int32Array(MODEL_MAX);
  const fleetModel = new Int8Array(CAR_CAP);
  const fleetInst = new Int32Array(CAR_CAP);
  const fleetFit = new Float32Array(CAR_CAP);
  // Half the drawn body's length, plus a centimetre, per slot: the night pool hangs its nose
  // and tail plates on it, so a plate always sits on the body that owns it whatever its size.
  const vehNose = new Float32Array(CAR_CAP);
  // Per slot livery multiplier for a modelled body: brightness 0.9 to 1.1 and at most four
  // percent of temperature. A pack model has one palette livery, so without this the avenue
  // would be fifteen identical sedans; the tint is a multiplier, so it can never invent a
  // colour the kit does not own, it only varies the one it has.
  const vehTint = new Float32Array(CAR_CAP * 3);
  // Scooter riders are packed into their own index space: a rider exists only on a scooter
  // slot, so the old 406 slot pool submitted 406 rider blobs to draw the fifth of the fleet
  // that is a scooter. Packing changes an index and a count, not the picture.
  const riderInst = new Int32Array(CAR_CAP);
  let riderN = 0;
  let riderLo = 0;
  let riderHi = 0;
  fleetModel.fill(-1);
  fleetInst.fill(-1);
  riderInst.fill(-1);
  for (let k = 0; k < MODEL_MAX; k++) modelSpanLo[k] = 1 << 30;

  function kindPalette(k) {
    if (k === KIND_SCOOTER) return scooterPalette;
    if (k === KIND_VAN) return vanPalette;
    if (k === KIND_TRUCK) return truckPalette;
    return carPalette;
  }

  function isAvenueSlot(i) {
    return i >= aveBase && i < aveBase + aveCap;
  }

  function modelOfKind(kind) {
    if (kind === KIND_CAR) return MODEL_OF_CAR;
    if (kind === KIND_VAN) return MODEL_OF_VAN;
    if (kind === KIND_TRUCK) return MODEL_OF_TRUCK;
    return null;
  }

  // A slot's model: a hashed start in its kind's list, walked forward to the first model this
  // build actually loaded. Deterministic per slot, so a rebuild, a resize or a wave edge never
  // repaints a car, and a missing model (a build without the street tier) simply leaves that
  // slot procedural.
  function modelForSlot(kind, i) {
    if (MODEL_LANE >= 0 && (i & 1) !== MODEL_LANE && i % MODEL_LANE_EVERY !== 0) return -1;
    const list = modelOfKind(kind);
    if (!list) return -1;
    const start = (hash01(i * 4111 + 97) * list.length) | 0;
    for (let k = 0; k < list.length; k += 1) {
      const idx = list[(start + k) % list.length];
      if (modelDefs[idx]) return idx;
    }
    return -1;
  }

  // Called once, after initVehicleKinds has fixed the kinds and before the first hide pass.
  function initModelFleet() {
    if (!assets) return;
    let loaded = 0;
    for (let k = 0; k < MODEL_MAX; k += 1) {
      const m = assets.models && typeof assets.models.get === 'function'
        ? assets.models.get(MODEL_NAMES[k]) : null;
      if (!m || !m.geometry || !m.material || !m.footprint) continue;
      const w = m.footprint.w > 0 ? m.footprint.w : MODEL_W;
      const d = m.footprint.d > 0 ? m.footprint.d : MODEL_L;
      modelDefs[k] = {
        model: m,
        tris: m.tris > 0 ? m.tris : 0,
        d: d,
        fit: Math.min(1, MODEL_W / w, MODEL_L / d)
      };
      loaded += 1;
    }
    if (loaded === 0) return;
    // Pass one counts the instances each model needs over the modelled pool's slots.
    for (let i = MODEL_POOL_BASE; i < MODEL_POOL_BASE + MODEL_POOL_CAP; i += 1) {
      const mi = modelForSlot(vehKind[i], i);
      if (mi < 0) continue;
      fleetModel[i] = mi;
      modelCount[mi] += 1;
    }
    // One instanced mesh per model that a slot wears, sized to the count and never grown. It
    // borrows the pack geometry and palette material, so the mesh is tagged shared for the
    // teardown and only its own instance attributes are released on a new export.
    for (let k = 0; k < MODEL_MAX; k += 1) {
      if (modelCount[k] === 0 || !modelDefs[k]) continue;
      const mesh = assets.makeInstanced(modelDefs[k].model, modelCount[k], {
        shadows: false,
        tintable: true,
        dynamic: true
      });
      mesh.userData.sharedGeometry = true;
      mesh.name = MODEL_NAMES[k];
      group.add(mesh);
      makePool(mesh);
      registerFleetMesh(mesh);
      for (let i = 0; i < modelCount[k]; i += 1) hideAt(mesh, i);
      dirtyAllMatrix(mesh);
      modelMeshes[k] = mesh;
      modelList.push(mesh);
    }
    // Pass two packs the instance indices, then fixes each slot's fit, lamp nose and tint.
    for (let i = MODEL_POOL_BASE; i < MODEL_POOL_BASE + MODEL_POOL_CAP; i += 1) {
      const mi = fleetModel[i];
      if (mi < 0) continue;
      const def = modelDefs[mi];
      fleetInst[i] = modelCursor[mi];
      modelCursor[mi] += 1;
      // 0.94 to 1.0 of the fit: the envelope holds, and a lane of one model still varies in
      // size. vehSX/SY/SZ are the same uniform number, which is what the lamp maths reads.
      const fit = def.fit * (0.94 + 0.06 * hash01(i * 3571 + 29));
      fleetFit[i] = fit;
      vehSX[i] = fit;
      vehSY[i] = fit;
      vehSZ[i] = fit;
      vehNose[i] = def.d * fit * 0.5 + 0.01;
      const b = i * 3;
      const bright = 0.90 + 0.20 * hash01(i * 1489 + 5);
      const warm = (hash01(i * 1973 + 11) - 0.5) * 0.08;
      vehTint[b] = bright * (1 + warm);
      vehTint[b + 1] = bright;
      vehTint[b + 2] = bright * (1 - warm);
    }
  }
  // The frame's write span per model mesh and for the packed rider pool. Reset before the
  // road steps and flushed once with the other pools, so a quiet lane uploads nothing.
  function beginFleetFrame() {
    riderLo = 1 << 30;
    riderHi = 0;
    for (let k = 0; k < MODEL_MAX; k += 1) {
      modelSpanLo[k] = 1 << 30;
      modelSpanHi[k] = 0;
    }
  }
  function flushFleet() {
    riders.count = riderHi;
    if (riderHi > riderLo) dirtyMatrix(riders, riderLo, riderHi);
    for (let k = 0; k < MODEL_MAX; k += 1) {
      if (!modelMeshes[k]) continue;
      if (modelSpanHi[k] > modelSpanLo[k]) {
        dirtyMatrix(modelMeshes[k], modelSpanLo[k], modelSpanHi[k]);
      }
    }
  }

  function markModelWrite(mi, inst) {
    if (inst < modelSpanLo[mi]) modelSpanLo[mi] = inst;
    if (inst + 1 > modelSpanHi[mi]) modelSpanHi[mi] = inst + 1;
  }

  function hideRider(i) {
    const ri = riderInst[i];
    riderDrawn[i] = 0;
    if (ri < 0) return;
    hideAt(riders, ri);
    if (ri < riderLo) riderLo = ri;
    if (ri + 1 > riderHi) riderHi = ri + 1;
  }

  // A pool that drops a car drops its body in every pool it can be drawn by: the shared
  // procedural mesh, its scooter rider and, when it wears one, its pack instance.
  function hideVehicle(i) {
    hideAt(cars, i);
    hideRider(i);
    const mi = fleetModel[i];
    if (mi >= 0) {
      const inst = fleetInst[i];
      hideAt(modelMeshes[mi], inst);
      markModelWrite(mi, inst);
    }
  }

  // A procedural vehicle always starts from its own palette colour; the wave then moves the
  // avenue slots partway toward the green. That keeps each car its own body colour through the
  // wave, so the avenue reads as traffic flowing under a signal rather than a lane of one
  // colour in green paint. A modelled car starts from its pack livery instead, carried by the
  // palette texture, and the same wave moves its multiplier toward the green. Both paths write
  // through flagColor, so a wave edge allocates nothing and needs no scratch colour.
  function refreshCarColors() {
    for (let i = 0; i < CAR_CAP; i++) {
      const mi = fleetModel[i];
      if (mi >= 0) {
        // A pack body keeps the kit's own livery, so its identity is white and the wave is
        // the multiplier of waveTint. The per slot tint is folded in here, which is why one
        // model can read as fifteen different cars.
        const b = i * 3;
        let r = vehTint[b];
        let g = vehTint[b + 1];
        let bl = vehTint[b + 2];
        if (greenWave && isAvenueSlot(i)) {
          r *= waveTint.r;
          g *= waveTint.g;
          bl *= waveTint.b;
        }
        flagColor.setRGB(r, g, bl);
        modelMeshes[mi].setColorAt(fleetInst[i], flagColor);
        continue;
      }
      const pal = kindPalette(vehKind[i]);
      flagColor.copy(pal[vehPalIdx[i] % vehPalLen[i]]);
      if (greenWave && isAvenueSlot(i)) flagColor.lerp(avenueGreen, WAVE_BLEND);
      cars.setColorAt(i, flagColor);
    }
    dirtyAllColour(cars);
    for (let k = 0; k < MODEL_MAX; k += 1) {
      if (modelMeshes[k]) dirtyAllColour(modelMeshes[k]);
    }
  }

  function initVehicleKinds() {
    for (let i = 0; i < CAR_CAP; i++) {
      const r = hash01(i * 8191 + 41);
      let kind = KIND_CAR;
      let sx = 1;
      let sy = 1;
      let sz = 1;
      if (r < SCOOTER_PCT) {
        kind = KIND_SCOOTER;
        sx = 0.46;
        sy = 0.86;
        sz = 0.5;
      } else if (r < SCOOTER_PCT + MIX_VAN_PCT) {
        kind = KIND_VAN;
        sx = 1.1;
        sy = 1.62;
        sz = 1.32;
      } else if (r < SCOOTER_PCT + MIX_VAN_PCT + TRUCK_PCT) {
        kind = KIND_TRUCK;
        sx = 1.16;
        sy = 2.05;
        sz = 1.58;
      }
      vehKind[i] = kind;
      vehSX[i] = sx;
      vehSY[i] = sy;
      vehSZ[i] = sz;
      // The lamp nose the night pool hangs its plates on: the body's half length plus the one
      // centimetre of air the old fixed 2.06 gave. A modelled slot overwrites this with the
      // half length of the model actually drawn.
      vehNose[i] = 2.06 * sz;
      const pal = kindPalette(kind);
      vehPalLen[i] = pal.length;
      vehPalIdx[i] = (hash01(i * 6151 + 17) * 32) | 0;
    }
    // Riders exist only on scooters: packed into their own index space, the pool submits the
    // fifth of the fleet that has a rider instead of all 406 slots. Nothing about a rider
    // moves; only its index does.
    riderN = 0;
    for (let i = 0; i < CAR_CAP; i++) {
      if (vehKind[i] === KIND_SCOOTER) {
        riderInst[i] = riderN;
        riderN += 1;
      } else {
        riderInst[i] = -1;
      }
    }
  }

  // ------------------------------------------------------ night light state
  // The single switch every light in this file follows: lit when the district is dark and
  // awake, dark in daylight and dark while it dreams, since the dream keeps its own light.
  let lampOn = 0;
  let lampPrev = -1;
  // The brake level last written into a tail pair's colour, so a cruising stream writes no
  // instance colour at all and only a queue repaints its tails when it stops.
  const tailBrake = new Float32Array(CAR_CAP);
  tailBrake.fill(-1);
  // Tail colour writes so far, and the count this frame started at: a pool marks a colour
  // upload only when a brake level moved, so steady traffic keeps colours off the wire.
  let tailWrites = 0;
  let tailMark = 0;

  // One body write for every moving vehicle, plus the scooter rider blob when the body is a
  // scooter and the lamps of the night pool when the district is awake and dark. Called from
  // every road step, so it stays allocation free. `brake` is 0 for a free mover and 1 for a car
  // standing in a queue: it only changes the brightness of the tail pair and the pool behind it.
  function writeVehicle(i, px, py, pz, ang, sc, brake) {
    const mi = fleetModel[i];
    if (mi >= 0) {
      // A pack body: the origin is the wheel contact, local +Z is the nose, so placement is
      // the same angle and one uniform scale. The procedural body of this slot is retired in
      // the same write, so a modelled car can never be drawn twice.
      const inst = fleetInst[i];
      const s = fleetFit[i] * sc;
      writeMatrix(modelMeshes[mi], inst, px, py + MODEL_LIFT, pz, ang, s, s, s);
      markModelWrite(mi, inst);
      hideAt(cars, i);
      if (riderDrawn[i]) hideRider(i);
      if (lampOn) writeVehicleLights(i, px, py, pz, ang, sc, brake);
      return;
    }
    const sy = vehSY[i];
    const lift = (sy - 1) * 0.2 * sc;
    writeMatrix(cars, i, px, py + 0.07 - lift, pz, ang,
      vehSX[i] * sc, sy * sc, vehSZ[i] * sc);
    if (vehKind[i] === KIND_SCOOTER) {
      const ri = riderInst[i];
      writeMatrix(riders, ri, px, py + 0.04, pz, ang, sc, sc, sc);
      riderDrawn[i] = 1;
      if (ri < riderLo) riderLo = ri;
      if (ri + 1 > riderHi) riderHi = ri + 1;
    } else if (riderDrawn[i]) {
      hideRider(i);
    }
    if (lampOn) writeVehicleLights(i, px, py, pz, ang, sc, brake);
  }

  // Every light one vehicle owns, in one pass, all six plates of it. The local +Z axis is the
  // direction of travel and the local +X axis is the right hand side, so the head pair sits
  // ahead of the nose, the tail pair behind the boot, and the wash lies on the road beyond the
  // head pair. One trig pair serves all six matrices.
  function writeVehicleLights(i, px, py, pz, ang, sc, brake) {
    const c = Math.cos(ang);
    const s = Math.sin(ang);
    const kind = vehKind[i];
    const base = LIGHT_CAR + i * CAR_SLOTS;
    // The nose and tail planes come from the body actually drawn: vehNose is the procedural
    // half length times its kind scale, or the pack model's own half length, plus one
    // centimetre, so a tail pair sits on a boot of any size instead of inside it.
    const half = vehNose[i] * sc + 0.07;
    const hx = px + s * half;
    const hz = pz + c * half;
    const lw = LAMP_W * sc;
    const lh = LAMP_H * sc;
    const off = LAMP_OFF * vehSX[i] * sc;
    if (kind === KIND_SCOOTER) {
      // One dot: the two plates of the pair are squeezed until they read as a single lamp at
      // district distance, and a scooter has no rear light worth drawing at this scale, nor a
      // pool behind it.
      writeMatrixCS(nightLights, base, hx, py + 0.52, hz, c, s, lw, lh * 0.8, sc);
      writeMatrixCS(nightLights, base + 1, hx + c * 0.1 * sc, py + 0.52, hz - s * 0.1 * sc,
        c, s, lw, lh * 0.8, sc);
      hideAt(nightLights, base + 2);
      hideAt(nightLights, base + 3);
      hideAt(nightLights, base + 5);
    } else {
      const lampY = py + 0.6 - (vehSY[i] - 1) * 0.2 * sc;
      // Two plates per pair, offset along the car's own right hand axis.
      writeMatrixCS(nightLights, base, hx + c * off, lampY, hz - s * off, c, s, lw, lh, sc);
      writeMatrixCS(nightLights, base + 1, hx - c * off, lampY, hz + s * off, c, s, lw, lh, sc);
      const tx = px - s * half;
      const tz = pz - c * half;
      writeMatrixCS(nightLights, base + 2, tx + c * off, lampY, tz - s * off, c, s, lw, lh, sc);
      writeMatrixCS(nightLights, base + 3, tx - c * off, lampY, tz + s * off, c, s, lw, lh, sc);
      // A tail pair is only repainted when the brake level actually moved: a cruising stream
      // writes no colour at all, and a queue writes once when it stops.
      if (brake > tailBrake[i] + 0.02 || brake < tailBrake[i] - 0.02) {
        tailBrake[i] = brake;
        lampTint.copy(lampTail).lerp(lampBrake, brake);
        nightLights.setColorAt(base + 2, lampTint);
        nightLights.setColorAt(base + 3, lampTint);
        tailWrites += 1;
      }
      if (brake > 0.04) {
        // A standing car pools red behind it, so a queue reads as one line.
        const bz = half + 1.6 + brake * 1.3;
        writeMatrixCS(nightLights, base + 5, px - s * bz, py + 0.1, pz - c * bz, c, s,
          2.6 * sc, 0.02, (1.9 + 1.9 * brake) * sc);
      } else {
        hideAt(nightLights, base + 5);
      }
    }
    // The wash lies flat on the carriageway ahead of the nose: the light lands on the road,
    // which is what turns the avenue and the expressway into ribbons after dark.
    const wl = WASH_L[kind];
    const wz = half + wl * 0.5;
    writeMatrixCS(nightLights, base + 4, px + s * wz, py + 0.1, pz + c * wz, c, s,
      WASH_W[kind] * sc, 0.02, wl * sc);
  }

  // A pool that drops a vehicle drops its lights in the same frame: a lamp left behind would
  // hang in the air, or lie on the asphalt, where the car used to be.
  function hideVehicleLights(i) {
    const base = LIGHT_CAR + i * CAR_SLOTS;
    for (let k = 0; k < CAR_SLOTS; k++) hideAt(nightLights, base + k);
  }

  // ------------------------------------------------------- night light gate
  function stepVehicleLights(night, dreaming) {
    const want = night && !dreaming ? 1 : 0;
    if (want === lampPrev) return;
    lampPrev = want;
    lampOn = want;
    nightLights.visible = want === 1;
    if (want === 1) return;
    // Going dark hides every instance as well as the mesh, so a frame with the lights off
    // carries no residual plate and no stale pool on the asphalt.
    for (let i = 0; i < LIGHT_CAP; i++) hideAt(nightLights, i);
    dirtyAllMatrix(nightLights);
  }

  // Slot colours that never change are painted once, at build: the head pair, the signs, the
  // washes and the brake pools are constants, so only a tail pair is recoloured per frame.
  function initVehicleLights() {
    for (let i = 0; i < CAR_CAP; i++) {
      const base = LIGHT_CAR + i * CAR_SLOTS;
      nightLights.setColorAt(base, lampHead);
      nightLights.setColorAt(base + 1, lampHead);
      nightLights.setColorAt(base + 2, lampTail);
      nightLights.setColorAt(base + 3, lampTail);
      nightLights.setColorAt(base + 4, glowHead);
      nightLights.setColorAt(base + 5, glowBrake);
      hideVehicleLights(i);
    }
    for (let b = 0; b < BUS_COUNT; b++) {
      const base = LIGHT_BUS + b * BUS_SLOTS;
      nightLights.setColorAt(base, glowBand);
      nightLights.setColorAt(base + 1, glowHead);
      nightLights.setColorAt(base + 2, lampSign);
      nightLights.setColorAt(base + 3, lampHead);
      nightLights.setColorAt(base + 4, lampHead);
      for (let k = 0; k < BUS_SLOTS; k++) hideAt(nightLights, base + k);
    }
    for (let t = 0; t < TRAM_SEGS; t++) {
      nightLights.setColorAt(LIGHT_TRAM + t, lampSign);
      hideAt(nightLights, LIGHT_TRAM + t);
    }
    dirtyAllMatrix(nightLights);
    dirtyAllColour(nightLights);
  }

  function visibility(shown, i) {
    const v = (shown - i) * FADE_INV;
    return v > 1 ? 1 : (v > 0 ? v : 0);
  }

  // -------------------------------------------------------------- van pool
  // Only the delivery slots are indexed here; the farm slots at the tail of the same mesh
  // keep their own state in the farm traffic block below.
  const vanOn = new Uint8Array(VAN_DELIVERY);
  const vanS = new Float32Array(VAN_DELIVERY);
  const vanDwell = new Float32Array(VAN_DELIVERY);
  const vanGrow = new Float32Array(VAN_DELIVERY);
  const vanKind = new Uint8Array(VAN_DELIVERY);
  const vanRoute = new Array(VAN_DELIVERY);
  let vanLive = 0;
  let farmN = 0;

  // ------------------------------------------------------------ farm traffic
  // The hillside keeps working at walking pace: one mover on every dirt track and a second on
  // the longest shoulder, five in all. They ride the last slots of the van pool, so the whole
  // addition is five instances of the existing van geometry inside one existing draw call.
  // Each mover follows its published track table, keeps to the right of the ruts, and turns
  // at a track end along a swept half circle: the heading rotates through 180 degrees over
  // FARM_TURN_S while the offset crosses to the other side, never a one-frame flip.
  const FARM_LANE = 0.7;             // rut side from the track centre, metres
  const FARM_CLAMP = 0.9;            // the maximum lateral offset the brief allows
  const FARM_SCALE = 0.55;           // a compact farm ute, so two can pass on a 2.6 m track
  const FARM_TURN_S = 2.6;           // one swept reversal, seconds
  const FARM_SPEEDS = [1.55, 1.9, 1.3, 2.2, 1.7];  // walking pace, metres per second
  const FARM_TINT = new THREE.Color(0xd9a43a);     // the truck palette's dusty ochre
  const farmTracks = [];
  const farmSlotOf = new Int32Array(FARM_COUNT);
  if (roads && roads.tracks) {
    for (let t = 0; t < roads.tracks.length; t += 1) {
      const tab = roads.tracks[t];
      if (tab && tab.n > 0 && tab.len > 0 && tab.xs && tab.zs && tab.nx && tab.nz && tab.ys) {
        farmTracks.push(tab);
      }
    }
    // One mover per track, then a second on the longest one.
    for (let t = 0; t < farmTracks.length && farmN < FARM_COUNT; t += 1) farmSlotOf[farmN++] = t;
    if (farmN < FARM_COUNT && farmTracks.length > 0) {
      let longest = 0;
      for (let t = 1; t < farmTracks.length; t += 1) {
        if (farmTracks[t].len > farmTracks[longest].len) longest = t;
      }
      farmSlotOf[farmN++] = longest;
    }
  }
  const farmS = new Float32Array(FARM_COUNT);
  const farmOff = new Float32Array(FARM_COUNT);
  const farmDir = new Int8Array(FARM_COUNT);
  const farmTurn = new Float32Array(FARM_COUNT);    // negative while driving
  const farmYaw0 = new Float32Array(FARM_COUNT);
  const farmOff0 = new Float32Array(FARM_COUNT);
  const farmPivotX = new Float32Array(FARM_COUNT);
  const farmPivotY = new Float32Array(FARM_COUNT);
  const farmPivotZ = new Float32Array(FARM_COUNT);
  const farmFwdX = new Float32Array(FARM_COUNT);
  const farmFwdZ = new Float32Array(FARM_COUNT);

  // Uniform arc-length lookup on a published track table: the interpolation tableAt does for
  // a curve table, without a second copy of the polyline. Sample i sits at i * step, and the
  // sample normals are unit and right-handed, so (nz, -nx) is the up-track tangent.
  function farmAt(tab, dist, outPos, outTan) {
    let d = dist;
    if (d < 0) d = 0;
    else if (d > tab.len) d = tab.len;
    const f0 = d / tab.step;
    let i = f0 | 0;
    let f = f0 - i;
    if (i >= tab.n) {
      i = tab.n - 1;
      f = 1;
    } else if (i < 0) {
      i = 0;
      f = 0;
    }
    const j = i + 1;
    outPos.x = tab.xs[i] + (tab.xs[j] - tab.xs[i]) * f;
    outPos.y = tab.ys[i] + (tab.ys[j] - tab.ys[i]) * f;
    outPos.z = tab.zs[i] + (tab.zs[j] - tab.zs[i]) * f;
    outTan.x = tab.nz[i] + (tab.nz[j] - tab.nz[i]) * f;
    outTan.y = 0;
    outTan.z = -(tab.nx[i] + (tab.nx[j] - tab.nx[i]) * f);
  }

  function farmBeginTurn(k, tab, endS) {
    farmAt(tab, endS, sPos, sTan);
    let fx = sTan.x;
    let fz = sTan.z;
    const fl = Math.sqrt(fx * fx + fz * fz) || 1;
    fx /= fl;
    fz /= fl;
    let off = farmOff[k];
    if (off > FARM_CLAMP) off = FARM_CLAMP;
    else if (off < -FARM_CLAMP) off = -FARM_CLAMP;
    farmOff0[k] = off;
    farmPivotX[k] = sPos.x;
    farmPivotY[k] = sPos.y;
    farmPivotZ[k] = sPos.z;
    farmFwdX[k] = fx;
    farmFwdZ[k] = fz;
    farmYaw0[k] = Math.atan2(fx * farmDir[k], fz * farmDir[k]);
    farmS[k] = endS;
    farmTurn[k] = 0;
  }

  // The farm slots carry the van body only: no rider, no payload plate, no lamp.
  function farmWrite(k, px, py, pz, yaw) {
    writeMatrix(vanMesh, FARM_BASE + k, px, py + 0.06, pz, yaw,
      FARM_SCALE, FARM_SCALE, FARM_SCALE);
  }

  function stepFarm(dt, dreaming) {
    if (farmN <= 0 || dreaming) return;
    for (let k = 0; k < farmN; k += 1) {
      const tab = farmTracks[farmSlotOf[k]];
      let px = 0;
      let py = 0;
      let pz = 0;
      let yaw = 0;
      if (farmTurn[k] >= 0) {
        farmTurn[k] += dt;
        let u = farmTurn[k] / FARM_TURN_S;
        if (u > 1) u = 1;
        const theta = Math.PI * (u * u * (3 - 2 * u));
        const sin = Math.sin(theta);
        const cos = Math.cos(theta);
        const off0 = farmOff0[k];
        const rad = off0 < 0 ? -off0 : off0;
        const dir = farmDir[k];
        const fx = farmFwdX[k];
        const fz = farmFwdZ[k];
        px = farmPivotX[k] + fx * (dir * rad * sin) - fz * (off0 * cos);
        pz = farmPivotZ[k] + fz * (dir * rad * sin) + fx * (off0 * cos);
        py = farmPivotY[k];
        // The arc direction depends on both the offset side and the travel direction: the
        // heading must follow the path tangent through the whole sweep, not just its ends.
        yaw = farmYaw0[k] + (off0 >= 0 ? 1 : -1) * dir * theta;
        if (farmTurn[k] >= FARM_TURN_S) {
          farmTurn[k] = -1;
          farmDir[k] = dir > 0 ? -1 : 1;
          farmOff[k] = -off0;
          farmS[k] = dir > 0 ? tab.len : 0;
        }
      } else {
        let s = farmS[k] + farmDir[k] * FARM_SPEEDS[k] * dt;
        let end = -1;
        if (farmDir[k] > 0 && s >= tab.len) {
          s = tab.len;
          end = tab.len;
        } else if (farmDir[k] < 0 && s <= 0) {
          s = 0;
          end = 0;
        }
        farmS[k] = s;
        let off = farmOff[k];
        if (off > FARM_CLAMP) off = FARM_CLAMP;
        else if (off < -FARM_CLAMP) off = -FARM_CLAMP;
        farmOff[k] = off;
        farmAt(tab, s, sPos, sTan);
        px = sPos.x - sTan.z * off;
        py = sPos.y;
        pz = sPos.z + sTan.x * off;
        yaw = Math.atan2(sTan.x * farmDir[k], sTan.z * farmDir[k]);
        if (end >= 0) farmBeginTurn(k, tab, end);
      }
      farmWrite(k, px, py, pz, yaw);
    }
    dirtyMatrix(vanMesh, FARM_BASE, VAN_CAP);
  }

  function initFarm() {
    if (farmN <= 0) return;
    // Three r160 allocates an instance colour buffer zero filled, which would paint every
    // delivery van black the moment the farm tint exists. White is the neutral multiplier,
    // so start the whole pool at white and paint only the farm tail.
    if (!vanMesh.instanceColor) {
      vanMesh.instanceColor = new THREE.InstancedBufferAttribute(
        new Float32Array(VAN_CAP * 3).fill(1), 3);
    }
    for (let k = 0; k < farmN; k += 1) {
      const tab = farmTracks[farmSlotOf[k]];
      const slot = FARM_BASE + k;
      let dir = (k & 1) === 0 ? 1 : -1;
      // A second mover on an already used track takes the opposing direction, so the two
      // start apart instead of nose to tail.
      for (let j = 0; j < k; j += 1) {
        if (farmSlotOf[j] === farmSlotOf[k]) { dir = -farmDir[j]; break; }
      }
      farmDir[k] = dir;
      farmS[k] = dir > 0 ? tab.len * (0.1 + 0.14 * (k % 3)) : tab.len * (0.52 + 0.06 * (k % 3));
      farmOff[k] = dir * FARM_LANE;
      farmTurn[k] = -1;
      vanMesh.setColorAt(slot, FARM_TINT);
    }
    dirtyAllColour(vanMesh);
    stepFarm(0, false);
  }

  // -------------------------------------------------------------- bus pool
  const busS = new Float32Array(BUS_COUNT);
  const busAct = new Uint8Array(BUS_COUNT);
  let busShown = 0;

  // ------------------------------------------------------------- rail pool
  let railLive = 0;
  let railAS = 0;
  let railADir = 1;
  let railADwell = 0;
  let railBS = 0;
  let railBDir = -1;
  let railBDwell = 0;
  let railSpanA0 = 0;
  let railSpanA1 = 1;
  let railSpanB0 = 0;
  let railSpanB1 = 1;
  const TRAIN_A_CARS = 4;
  const TRAIN_B_CARS = 3;
  const TRAIN_A_GAP = 13.8;
  const TRAIN_B_GAP = 13.4;
  const RAIL_TRACK_A = 1.75;
  const RAIL_TRACK_B = -1.75;
  const WAGON_TRACK = 5.25;
  const WAGON_SET = 3;

  // -------------------------------------------------------------- ped pool
  const pedOn = new Uint8Array(PED_CAP);
  const pedMode = new Uint8Array(PED_CAP);
  const pedS = new Float32Array(PED_CAP);
  const pedDir = new Int8Array(PED_CAP);
  const pedSpeed = new Float32Array(PED_CAP);
  const pedLane = new Float32Array(PED_CAP);
  const pedGrow = new Float32Array(PED_CAP);
  const pedPhase = new Float32Array(PED_CAP);
  const pedRoute = new Array(PED_CAP);
  const pedAX = new Float32Array(PED_CAP);
  const pedAZ = new Float32Array(PED_CAP);
  const pedDX = new Float32Array(PED_CAP);
  const pedDZ = new Float32Array(PED_CAP);
  const pedSpan = new Float32Array(PED_CAP);
  let pedShown = 0;
  let pedDrawn = 0;
  let pedVisible = 0;
  let pedHi = 0;

  // ------------------------------------------------------- walking cohort
  // The walking population is procedural: one six-sided cylinder and one icosahedron head, 44
  // triangles, which is what lets seven hundred of them move at once. The pack mini characters
  // that stand in the world's crowds are 149 triangles, so swapping the whole population would
  // be a hundred and five more triangles for every walker. The cohort is the reading trade
  // that buys the figures a street viewer actually sees: the nearest in-view walkers wear a
  // pack body, and the rest keep the procedural one.
  //
  // The swap has to withdraw the procedural body, not just hide it. A zero-scale instance is
  // still submitted, so renderer.info would count its 44 triangles and the swap would cost the
  // pack body's 149 instead of the 105 difference. The pedestrian pool therefore repacks its
  // live instances into the prefix it draws, exactly as the rider pool already does: a walker
  // in the cohort simply occupies no procedural instance, `count` is the number of walkers that
  // stayed procedural, and no instance is left behind. The mapping is rewritten every frame
  // from the same per-slot state the walk used before, so no trip, speed, spawn or data rule
  // changes; only the index a body is written at does. The phone profile builds no cohort mesh
  // at all (its caps and its merged district kit leave no triangle room), so it takes the
  // uncompacted path and its frames are the cycle 22 frames byte for byte.
  //
  // Capacity and cost. A pack body at 149 triangles minus the 44 the cylinder and head
  // withdraw is 105, so 60 walkers would cost 6,300; the cycle 23 integration set the budget to
  // 4,200 (40 walkers) because at 60 the bench's wake repopulation peak measured 400,400
  // triangles, over the 400,000 hard gate, while the world's corner spend cannot shrink without
  // losing the corners it bought. The build recomputes the
  // capacity from the bodies it actually chose, so a pack whose cheapest character is heavier
  // shrinks the cohort instead of overrunning the ceiling.
  const COHORT_CAP = 60;
  const COHORT_VARIANTS = 4;
  const COHORT_TRI_BUDGET = 4200;
  const PROC_PED_TRIS = 44;
  // A selection tick every tenth of a second. The cohort is a rendering choice at street
  // distance, so a walker that drifts a metre between ticks does not need to change body; the
  // tick also keeps the instance repacking (and the colour writes that follow it) rare.
  const COHORT_TICK = 0.1;
  // Members keep a 1.35x distance advantage over newcomers, so a walker drifting past the
  // boundary does not flicker between a pack body and a cylinder.
  const COHORT_STICKY = 0.55;
  const cohortMesh = [];
  const cohortMeshTris = [];
  const cohortNames = [];
  const cohortColSlot = [];
  const cohortCursor = new Int32Array(COHORT_VARIANTS);
  const cohortSpanLo = new Int32Array(COHORT_VARIANTS);
  const cohortSpanHi = new Int32Array(COHORT_VARIANTS);
  const cohortColLo = new Int32Array(COHORT_VARIANTS);
  const cohortColHi = new Int32Array(COHORT_VARIANTS);
  const cohortIn = new Uint8Array(PED_CAP);
  const cohortVariant = new Uint8Array(PED_CAP);
  // The position a walker was last drawn at, so a selection tick ranks real places rather than
  // stepping the walk a second time. `pedLit` is "drawn last frame", the only members a tick
  // may rank.
  const pedLastX = new Float32Array(PED_CAP);
  const pedLastY = new Float32Array(PED_CAP);
  const pedLastZ = new Float32Array(PED_CAP);
  const pedLit = new Uint8Array(PED_CAP);
  // Which slot's palette is sitting at instance i of the procedural pool. A walker keeps its
  // clothes when the packing shifts: the colour is written again only where the binding
  // changed, so a steady frame uploads no colour at all.
  const pedColSlot = new Int32Array(PED_CAP);
  pedColSlot.fill(-1);
  // A body scale of 2.07 to 2.33, inside the 2.0 to 2.4 the world crowd already stands at, so
  // a walking figure and a standing figure of the same kit are the same size; a per-slot tone
  // keeps the pack palette from reading as one repeated livery.
  const cohortScale = new Float32Array(PED_CAP);
  const cohortTone = new Float32Array(PED_CAP);
  for (let i = 0; i < PED_CAP; i += 1) {
    cohortScale[i] = 2.2 * (0.94 + 0.12 * hash01(i * 613 + 17));
    cohortTone[i] = 0.88 + 0.24 * hash01(i * 19 + 11);
  }
  const cohortTint = new THREE.Color();
  // The view test and the ranking scratch, hoisted: a tick allocates nothing.
  const cohortFrustum = new THREE.Frustum();
  const cohortMatrix = new THREE.Matrix4();
  const cohortPoint = new THREE.Vector3();
  const cohortBestSlot = new Int32Array(COHORT_CAP);
  const cohortBestScore = new Float32Array(COHORT_CAP);
  let cohortN = 0;         // variant meshes built
  let cohortCap = 0;       // walkers the triangle budget pays for
  let cohortOn = false;    // the cohort is drawing (a debug toggle can turn it off)
  let cohortDrawn = 0;     // pack instances submitted this frame
  let cohortMembers = 0;   // walkers currently assigned to the cohort
  let cohortClock = 0;
  let cohortCam = null;
  const cohortReport = {
    capacity: 0,
    variants: 0,
    models: [],
    drawn: 0,
    tris: 0,
    candidates: 0,
    perWalker: 0,
    budget: COHORT_TRI_BUDGET,
    enabled: false,
    // The measurement switch: setEnabled(false) restores the cycle 22 pedestrian rendering in
    // place, so the cohort's own triangles and calls can be read as a difference on one frame
    // instead of across two loads whose data clocks never agree.
    setEnabled: setCohortEnabled
  };

  // The cohort needs the view, and main creates the rig after this factory and hands down no
  // camera. The renderer already walks every drawn object, so the camera is taken from the
  // documented per-object render hook instead of reaching into main's debug surface: one
  // assignment, one reference read per tick, and a frame of lag nobody can see at 0.1 s.
  function watchCohortCamera(mesh) {
    mesh.onBeforeRender = function (renderer, scene, camera) {
      if (camera) cohortCam = camera;
    };
  }

  // Rank the drawn walkers the frame can see: inside the camera frustum, nearest first, with
  // last tick's members keeping a distance advantage. The frustum is the honest definition of
  // "the people you actually see" at street tier, where a walker behind the camera is nearer
  // in metres than one at the frame's edge. Sorting is an insertion into a fixed 60 entry list,
  // so the tick allocates nothing and a candidate farther than the last entry leaves in one
  // compare.
  function selectCohort(dt) {
    if (!cohortOn || cohortCap <= 0) return;
    const cam = cohortCam;
    if (!cam || !cam.projectionMatrix || !cam.matrixWorldInverse) return;
    cohortClock += dt;
    if (cohortClock < COHORT_TICK && cohortMembers > 0) return;
    cohortClock = 0;
    cohortMatrix.multiplyMatrices(cam.projectionMatrix, cam.matrixWorldInverse);
    cohortFrustum.setFromProjectionMatrix(cohortMatrix);
    const cx = cam.position.x;
    const cy = cam.position.y;
    const cz = cam.position.z;
    let n = 0;
    let cand = 0;
    for (let i = 0; i < PED_CAP; i += 1) {
      if (!pedLit[i]) continue;
      cohortPoint.set(pedLastX[i], pedLastY[i] + 1.0, pedLastZ[i]);
      if (!cohortFrustum.containsPoint(cohortPoint)) continue;
      cand += 1;
      const dx = pedLastX[i] - cx;
      const dy = pedLastY[i] - cy;
      const dz = pedLastZ[i] - cz;
      let score = dx * dx + dy * dy + dz * dz;
      if (cohortIn[i]) score *= COHORT_STICKY;
      if (n >= cohortCap && score >= cohortBestScore[cohortCap - 1]) continue;
      let p = n < cohortCap ? n : cohortCap - 1;
      while (p > 0 && cohortBestScore[p - 1] > score) {
        cohortBestScore[p] = cohortBestScore[p - 1];
        cohortBestSlot[p] = cohortBestSlot[p - 1];
        p -= 1;
      }
      cohortBestScore[p] = score;
      cohortBestSlot[p] = i;
      if (n < cohortCap) n += 1;
    }
    for (let i = 0; i < PED_CAP; i += 1) cohortIn[i] = 0;
    for (let k = 0; k < n; k += 1) {
      const slot = cohortBestSlot[k];
      cohortIn[slot] = 1;
      // The body, the scale and the tone are all a function of the walker's slot, so a figure
      // keeps one character for as long as it walks: only the promotion between the cylinder
      // and a pack body follows the ranking, never the body itself.
      cohortVariant[slot] = slot % cohortN;
    }
    cohortMembers = n;
    cohortReport.candidates = cand;
  }

  // One pack body, written at the next free instance of its variant. The body carries the
  // procedural body's own transform (position, walking heading, spawn and fade scale), so a
  // walker keeps its place whether it wears the cylinder or the character.
  function writeCohort(i, x, y, z, angle, scale) {
    const v = cohortVariant[i];
    const mesh = cohortMesh[v];
    if (!mesh) return;
    const c = cohortCursor[v];
    if (c >= COHORT_CAP) return;
    cohortCursor[v] = c + 1;
    const s = cohortScale[i] * scale;
    writeMatrix(mesh, c, x, y, z, angle, s, s, s);
    if (c < cohortSpanLo[v]) cohortSpanLo[v] = c;
    if (c + 1 > cohortSpanHi[v]) cohortSpanHi[v] = c + 1;
    if (cohortColSlot[v][c] !== i) {
      cohortColSlot[v][c] = i;
      const tone = cohortTone[i];
      cohortTint.setRGB(tone, tone, tone);
      mesh.setColorAt(c, cohortTint);
      if (c < cohortColLo[v]) cohortColLo[v] = c;
      if (c + 1 > cohortColHi[v]) cohortColHi[v] = c + 1;
    }
  }

  function cohortBegin() {
    for (let v = 0; v < cohortN; v += 1) {
      cohortCursor[v] = 0;
      cohortSpanLo[v] = COHORT_CAP;
      cohortSpanHi[v] = 0;
      cohortColLo[v] = COHORT_CAP;
      cohortColHi[v] = 0;
    }
    cohortDrawn = 0;
  }

  function cohortEnd() {
    let tris = 0;
    for (let v = 0; v < cohortN; v += 1) {
      const mesh = cohortMesh[v];
      const n = cohortCursor[v];
      cohortDrawn += n;
      tris += n * cohortMeshTris[v];
      mesh.count = n;
      mesh.visible = n > 0;
      if (cohortSpanHi[v] > cohortSpanLo[v]) dirtyMatrix(mesh, cohortSpanLo[v], cohortSpanHi[v]);
      if (cohortColHi[v] > cohortColLo[v]) dirtyColour(mesh, cohortColLo[v], cohortColHi[v]);
    }
    cohortReport.drawn = cohortDrawn;
    cohortReport.tris = tris;
  }

  // The build picks the cheapest characters the pack carries (149 triangles today, the number
  // the cycle 22 budget booked) and sizes the cohort from the real per-walker cost. A pack
  // whose cheapest body is heavier simply gets a smaller cohort; the ceiling is never raised.
  function initCohort() {
    if (!assets || MOBILE_CAPS) return;
    const people = assets.byCategory('person') || [];
    if (!people.length) return;
    const order = [];
    for (let i = 0; i < people.length; i += 1) {
      const m = people[i];
      if (!m || !m.geometry || !m.material || !(m.tris > 0)) continue;
      order.push(i);
    }
    if (!order.length) return;
    order.sort(function (a, b) {
      const da = people[a].tris;
      const db = people[b].tris;
      return da === db ? a - b : da - db;
    });
    const take = Math.min(COHORT_VARIANTS, order.length);
    const picks = [];
    for (let k = 0; k < take; k += 1) picks.push(order[k]);
    picks.sort(function (a, b) { return a - b; });
    let sum = 0;
    for (let k = 0; k < picks.length; k += 1) sum += people[picks[k]].tris;
    const perWalker = sum / picks.length - PROC_PED_TRIS;
    if (!(perWalker > 0)) return;
    let cap = Math.floor(COHORT_TRI_BUDGET / perWalker);
    if (cap > COHORT_CAP) cap = COHORT_CAP;
    if (cap < 1) return;
    for (let k = 0; k < picks.length; k += 1) {
      const model = people[picks[k]];
      const mesh = assets.makeInstanced(model, COHORT_CAP, {
        shadows: false,
        tintable: true,
        dynamic: true
      });
      mesh.userData.sharedGeometry = true;
      mesh.name = model.name;
      mesh.count = 0;
      mesh.visible = false;
      for (let i = 0; i < COHORT_CAP; i += 1) hideAt(mesh, i);
      makePool(mesh);
      dirtyAllMatrix(mesh);
      group.add(mesh);
      cohortMesh.push(mesh);
      cohortMeshTris.push(model.tris);
      cohortNames.push(model.name);
      cohortColSlot.push(new Int32Array(COHORT_CAP).fill(-1));
    }
    cohortN = cohortMesh.length;
    if (cohortN === 0) return;
    cohortCap = cap;
    cohortOn = true;
    cohortReport.capacity = cap;
    cohortReport.variants = cohortN;
    cohortReport.perWalker = Math.round(perWalker * 100) / 100;
    cohortReport.enabled = true;
    for (let k = 0; k < cohortN; k += 1) cohortReport.models.push(cohortNames[k]);
    watchCohortCamera(peds);
  }

  // The debug toggle the measurement uses: off is exactly the cycle 22 pedestrian frame (the
  // pool goes back to slot-indexed instances and no pack body is submitted), so the cohort's
  // own cost can be read off one page by switching it in place.
  function setCohortEnabled(on) {
    if (cohortN <= 0) return;
    const next = !!on;
    if (next === cohortOn) return;
    cohortOn = next;
    cohortReport.enabled = cohortOn;
    cohortReport.drawn = 0;
    cohortReport.tris = 0;
    if (!next) {
      cohortReport.candidates = 0;
      for (let v = 0; v < cohortN; v += 1) {
        const mesh = cohortMesh[v];
        mesh.count = 0;
        mesh.visible = false;
        cohortCursor[v] = 0;
      }
      for (let i = 0; i < PED_CAP; i += 1) cohortIn[i] = 0;
      cohortMembers = 0;
      cohortDrawn = 0;
    }
  }

  const plaza = world && world.marketSquare && world.marketSquare.center
    ? world.marketSquare
    : null;
  const plazaSize = plaza && typeof plaza.size === 'number' ? plaza.size : 0;
  const plazaOn = !!plaza && plazaSize > 6;
  const PLAZA_CX = plaza ? plaza.center.x : 0;
  const PLAZA_CZ = plaza ? plaza.center.z : 0;
  const PLAZA_HX = plazaOn ? plazaSize * 0.5 - 1.8 : 0;
  const PLAZA_HZ = plazaOn ? plazaSize * 0.31 - 1.5 : 0;
  const PLAZA_LIFT = 0.29;

  // --------------------------------------------------------- plaza stall disks
  // world.mjs raises fourteen market stalls in two loose rows of seven at a three metre pitch,
  // five point seven metres either side of the fountain, and claims each one as a 1.3 m disk
  // of its own occupancy. The plaza crowd has to read that occupancy, so the same row layout is
  // rebuilt here from the published square (world.market carries the radius and the count,
  // marketSquare the centre). A walker is a body, not a point: PED_CLEAR is its half width, so
  // the crowd holds 1.55 m from a stall centre and never stands in a counter.
  const stallR = world && world.market && typeof world.market.stallRadius === 'number'
    ? world.market.stallRadius
    : 1.3;
  const PED_CLEAR = 0.25;
  const STALL_ROW_Z = 5.7;
  const STALL_PITCH = 3;
  const STALL_JIT = [-0.12, 0.15, -0.06, 0.1, -0.15, 0.05, 0.13];
  const STALL_N = plazaOn ? 14 : 0;
  const stallX = new Float32Array(STALL_N > 0 ? STALL_N : 1);
  const stallZ = new Float32Array(STALL_N > 0 ? STALL_N : 1);
  const stallOut = new Float32Array(STALL_N > 0 ? STALL_N : 1);
  if (plazaOn) {
    let n = 0;
    for (let row = 0; row < 2; row++) {
      const face = row === 0 ? 1 : -1;
      for (let k = 0; k < 7; k++) {
        stallX[n] = PLAZA_CX + (k - 3) * STALL_PITCH;
        stallZ[n] = PLAZA_CZ + face * -STALL_ROW_Z + STALL_JIT[k];
        // The row's own outer edge: the back of the counter, away from the fountain.
        stallOut[n] = -face;
        n++;
      }
    }
  }

  // True when the whole walked segment from (ax, az) to (ax + dx * span, az + dz * span) is
  // clear of every stall disk. Segment to point, so a walker that stands still (span 0) is the
  // plain distance test; no allocation and at most STALL_N iterations.
  function stallClear(ax, az, dx, dz, span) {
    const lim = stallR + PED_CLEAR;
    const ll = lim * lim;
    for (let k = 0; k < STALL_N; k++) {
      const px = stallX[k] - ax;
      const pz = stallZ[k] - az;
      let t = px * dx + pz * dz;
      if (t < 0) t = 0;
      else if (t > span) t = span;
      const ex = px - dx * t;
      const ez = pz - dz * t;
      if (ex * ex + ez * ez < ll) return false;
    }
    return true;
  }

  // ------------------------------------------------------------ cloud pool
  const cloudOn = new Uint8Array(CLOUD_CAP);
  const cloudX = new Float32Array(CLOUD_CAP);
  const cloudY = new Float32Array(CLOUD_CAP);
  const cloudZ = new Float32Array(CLOUD_CAP);
  const cloudSX = new Float32Array(CLOUD_CAP);
  const cloudSY = new Float32Array(CLOUD_CAP);
  const cloudSZ = new Float32Array(CLOUD_CAP);
  // A fixed yaw per plate, written with the instance every time it moves: a deck of identical
  // axis aligned quads reads as a rubber stamp, and a yaw costs one cosine and one sine.
  const cloudRot = new Float32Array(CLOUD_CAP);
  const cloudRate = new Float32Array(CLOUD_CAP);
  // Drift lane per plate, in world x. The whole deck wraps across the field, except the first
  // WISP_N plates, which are the plates a clear hour keeps and therefore drift in a narrow
  // lane over the near middle of the district. That lane is what makes the wisp a permanent
  // part of the district framing instead of a shape that happens to be in the frame.
  const cloudMinX = new Float32Array(CLOUD_CAP);
  const cloudMaxX = new Float32Array(CLOUD_CAP);
  let cloudLive = 0;
  let cloudTintPrev = -1;
  let cloudDreamPrev = false;
  let cloudNightPrev = false;
  let cloudNightClock = 0;

  // ------------------------------------------------------------------ signals
  // world.signals is a junction deck: one record for each place two road families share
  // asphalt, one lamp per built head, and a phase code per lamp. Every head was stamped at
  // build with the record it belongs to and the family it governs, so the phase below is read
  // from its junction and its family instead of from a hash of its own position. The lamps of
  // one junction therefore cannot disagree with each other, and the same phase value that
  // lights a lamp is the one the car clamp reads: a viewer never sees a red lamp and a car
  // crossing it.
  const sig = world && world.signals ? world.signals : null;
  const sigHeads = sig && sig.heads && sig.heads.xs && sig.heads.zs ? sig.heads : null;
  const sigState = sig && sig.state && typeof sig.state.length === 'number' ? sig.state : null;
  const sigApply = sig && typeof sig.apply === 'function' ? sig.apply : null;
  const sigN = sigHeads && sigState && sigApply
    ? Math.max(0, Math.min(sigHeads.count | 0, sigState.length))
    : 0;
  const sigTable = sig && sig.junctions && sig.junctions.length ? sig.junctions : null;
  const sigJunctionCount = sigTable ? sigTable.length : 0;
  const sigHeadJunction = sigHeads && sigHeads.junction && sigHeads.junction.length >= sigN
    ? sigHeads.junction : null;
  const sigHeadFamily = sigHeads && sigHeads.family && sigHeads.family.length >= sigN
    ? sigHeads.family : null;
  const SIG_FAMILY_AVENUE = sig && sig.families && typeof sig.families.avenue === 'number'
    ? sig.families.avenue | 0 : 0;
  const SIG_FAMILY_RING = 1;

  // One slot per junction per frame: 0 avenue green, 1 avenue amber, 2 ring green, 3 ring amber.
  // A junction that is not signalised (a spur mouth) holds slot 2 for ever, because the ring
  // keeps the right of way there and the spur's own traffic carries no phase.
  const sigPhase = new Int8Array(sigJunctionCount > 0 ? sigJunctionCount : 1);
  const sigAveLeft = new Float32Array(sigJunctionCount > 0 ? sigJunctionCount : 1);
  const sigRingLeft = new Float32Array(sigJunctionCount > 0 ? sigJunctionCount : 1);
  // The offset each junction's phase runs behind the sim clock, hashed from the junction's
  // phase group so different junctions do not blink in unison. The two arms of the avenue
  // mouth share one group on purpose: they are 20.4 m apart on the ring, and a second offset
  // would park a queue between them that the upstream arm's box rule then refuses to
  // discharge through.
  const sigOffset = new Float32Array(sigJunctionCount > 0 ? sigJunctionCount : 1);
  if (sigJunctionCount > 0) {
    for (let j = 0; j < sigJunctionCount; j += 1) {
      const record = sigTable[j];
      const group = record && typeof record.phaseGroup === 'number' ? record.phaseGroup | 0 : j;
      sigOffset[j] = hash01(group * 7919 + 41) * SIGNAL_CYCLE_S;
    }
  }
  // Cached code per head. It is the whole write guard: a state entry is written and apply()
  // runs only on a frame where at least one head differs from this cache.
  const sigPrev = new Int8Array(sigN > 0 ? sigN : 1);
  if (sigN > 0) sigPrev.fill(-1);
  let sigOrigin = 0;
  let sigOriginSet = false;

  // Which junction each avenue lane side stops at, read from the record's own arm. -1 means the
  // build found no crossing on that side, and the old priority yield then stays in charge.
  const aveSignal = new Int8Array(2);
  aveSignal[0] = -1;
  aveSignal[1] = -1;
  for (let j = 0; j < sigJunctionCount; j += 1) {
    const record = sigTable[j];
    if (!record || record.kind !== 'avenue' || !record.arms) continue;
    for (let k = 0; k < record.arms.length; k += 1) {
      const arm = record.arms[k];
      const side = arm && typeof arm.side === 'number' ? arm.side | 0 : -1;
      if (side === 0 || side === 1) aveSignal[side] = j;
    }
  }
  // The two avenue arms of the mouth, in side order: each is one row of the ring clamp.
  const ringSignalS = new Float32Array(2);
  const ringSignalJ = new Int8Array(2);
  ringSignalS[0] = -1;
  ringSignalS[1] = -1;
  ringSignalJ[0] = -1;
  ringSignalJ[1] = -1;
  for (let side = 0; side < 2; side += 1) {
    const j = aveSignal[side];
    if (j < 0 || aveCrossRingS[side] < 0) continue;
    ringSignalS[side] = aveCrossRingS[side];
    ringSignalJ[side] = j;
  }

  // Crossing counters for the diagnostics: one bucket per second for the last thirty, per
  // junction and family, with a running total, so a read costs nothing and a crossing is one
  // increment. A car is counted when its arc passes the crossing in its own direction of
  // travel, which is exactly the event the junction exists to control.
  const CROSS_BUCKETS = 30;
  const crossAveBucket = new Int16Array(sigJunctionCount * CROSS_BUCKETS);
  const crossRingBucket = new Int16Array(sigJunctionCount * CROSS_BUCKETS);
  const crossAveTotal = new Int16Array(sigJunctionCount);
  const crossRingTotal = new Int16Array(sigJunctionCount);
  let crossSec = 0;
  function rollCrossClock() {
    const sec = clock | 0;
    if (sec === crossSec) return;
    if (sec < crossSec) {
      crossSec = sec;
      return;
    }
    let n = sec - crossSec;
    if (n > CROSS_BUCKETS) n = CROSS_BUCKETS;
    for (let k = 0; k < n; k += 1) {
      const idx = (crossSec + 1 + k) % CROSS_BUCKETS;
      for (let j = 0; j < sigJunctionCount; j += 1) {
        const at = j * CROSS_BUCKETS + idx;
        crossAveTotal[j] -= crossAveBucket[at];
        crossAveBucket[at] = 0;
        crossRingTotal[j] -= crossRingBucket[at];
        crossRingBucket[at] = 0;
      }
    }
    crossSec = sec;
  }
  function noteCross(junction, family) {
    if (junction < 0 || junction >= sigJunctionCount) return;
    const at = junction * CROSS_BUCKETS + (crossSec % CROSS_BUCKETS);
    if (family === SIG_FAMILY_RING) {
      crossRingBucket[at] += 1;
      crossRingTotal[junction] += 1;
    } else {
      crossAveBucket[at] += 1;
      crossAveTotal[junction] += 1;
    }
  }

  // The readback the live harness uses: one record per junction, updated in place, allocated
  // once. It carries the phase both roads are showing and the crossings counted in the last
  // thirty seconds, per junction and per arm.
  const junctionDiag = [];
  for (let j = 0; j < sigJunctionCount; j += 1) {
    const record = sigTable[j];
    const arms = [];
    // The agents' own crossing centre, read from its own tables at the arcs it will clamp on,
    // so a live probe checks boxes against the geometry the cars actually hold, not against the
    // world's placement copy of it.
    let crossX = NaN;
    let crossZ = NaN;
    let ringX = NaN;
    let ringZ = NaN;
    if (record.arms) {
      for (let k = 0; k < record.arms.length; k += 1) {
        const arm = record.arms[k];
        const side = typeof arm.side === 'number' ? arm.side | 0 : -1;
        if (record.kind === 'avenue' && side >= 0 && side <= 1 && aveCrossS[side] >= 0) {
          const off = side === 0 ? -AVE_CAR_LANE : AVE_CAR_LANE;
          tableAt(aveTable, aveCrossS[side], sPos, sTan);
          crossX = sPos.x - sTan.z * off;
          crossZ = sPos.z + sTan.x * off;
          if (aveCrossRingS[side] >= 0) {
            tableAt(ringTable, aveCrossRingS[side], sPos, sTan);
            ringX = sPos.x;
            ringZ = sPos.z;
          }
        }
        arms.push({
          side: side,
          spur: typeof arm.spur === 'number' ? arm.spur | 0 : -1,
          x: arm.x,
          z: arm.z,
          ringS: arm.ringS,
          aveS: arm.aveS,
          avenueCrossed30: 0,
          ringCrossed30: 0
        });
      }
    }
    junctionDiag.push({
      id: record.id,
      kind: record.kind,
      route: record.route,
      x: record.x,
      z: record.z,
      crossX: crossX,
      crossZ: crossZ,
      ringX: ringX,
      ringZ: ringZ,
      signalised: !!record.signalised,
      phase: 0,
      avenueGreen: false,
      ringGreen: false,
      avenueCode: 2,
      ringCode: 2,
      offset: sigOffset[j],
      avenueCrossed30: 0,
      ringCrossed30: 0,
      arms: arms
    });
  }

  // ------------------------------------------------------------ sign / parcel
  let slotN = world && world.signs && world.signs.slotCount ? world.signs.slotCount | 0 : 0;
  let slotQuarter = world && world.signs && world.signs.slotQuarter ? world.signs.slotQuarter : null;
  let signCursor = new Int32Array(slotN > 0 ? slotN : 1);
  let signTimer = 0;
  let signsDirty = true;

  let parcelOrder = null;
  let parcelCursor = 0;
  let parcelInit = false;

  // ------------------------------------------------------------------ state
  const counts = { vehicles: 0, pedestrians: 0 };
  let clock = 0;
  let prevT = NaN;
  let greenWave = false;
  let dreamPrev = false;
  let modelRef = model || null;
  let popScale = 1;
  let popTarget = 1;

  let windowRank = null;
  let windowState = null;
  let winCursor = 0;
  let winSwept = 0;

  let tramS = 0;
  let tramDwell = 0;
  let tramNext = 0;
  // Reused return value for tram.positionAt() callers that pass no target.
  const tramOut = new THREE.Vector3();

  // Time-driven world state, applied on transitions only.
  let lastHour = -999;
  let lastNight = -1;
  let neglectedState = new Uint8Array(qCap);
  neglectedState.fill(2);

  // Bounds for clouds.
  let cloudHalfX = 170;
  let cloudHalfZ = 170;
  let cloudCX = 0;
  let cloudCZ = 0;
  // Base and thickness of the deck, in metres. The base is set by the projection, not by taste.
  // At the desktop district framing the camera is 213.06 m up (12 + 312.79 * sin 40) and the
  // top edge of the frame looks 17.5 degrees below horizontal (40 pitch minus the 22.5 degree
  // half field of view), so the top edge crosses height y at r = (213.06 - y) / tan 17.5, with
  // r the horizontal distance from the camera. A plate at 132 m is therefore inside the frame
  // out to r = 259 m, and one at 178 m out to 112 m: the near and middle deck enters the top
  // of the frame with room, instead of the whole deck sitting above a frame whose top edge is
  // already below the camera. The band is 46 m deep so the deck has parallax and reaches both
  // the upper sky and the band under it, and the tallest roof the sample world can build is a
  // 20 floor tower, a little under 90 m above sea level, so every plate stays about 42 m over
  // the roofs.
  const CLOUD_BASE_Y = 132;
  const CLOUD_BAND_Y = 46;
  // Cycle 30 measured the deck against the frame instead of the prose: a steady hour kept four
  // plates, and a two-frame difference over the day frame moved 0.00 percent of the sky, so the
  // district was standing under empty air. The lane holds five now, the block tier's own lane
  // three, and three spares wait for more cover, so a clear hour is eleven plates and a
  // fragmented one still caps at the pool's 140. The deck also lifts from 118 m to 132 m and
  // deepens to 46 m, which spreads the plates across rows 45 to 90 of a 1600x900 district frame;
  // the same two-frame difference now moves 5.39 percent of that band across 312 columns, while
  // rows 0 to 45 keep streaks 0.00 and the frame's mud reading is 1.26 percent against cycle 28's
  // 1.45. A deck tuned lower (120 m base, 62 m band) measured as no cloud at all in the frame,
  // because cloudBaseY is raised to the tallest quarter label plus six, so the plates were above
  // the top edge; 132 m is the base that was measured to work, not a value chosen by eye. The
  // plates are two triangles each and the whole deck is one instanced mesh, so a thicker deck
  // costs matrix time, not a draw call.
  const CLOUD_WISP_N = 5;
  const CLOUD_WISP_SPARE = 3;
  const CLOUD_WISP_LANE = 40;
  const CLOUD_BLOCK_N = 3;
  const CLOUD_BLOCK_LANE = 30;
  const CLOUD_BLOCK_BAND = 3;
  const CLOUD_BLOCK_SPREAD = 3;
  // Index ranges out of the pool: district lane, block lane, then the district lane's spares.
  const CLOUD_BLOCK_HI = CLOUD_WISP_N + CLOUD_BLOCK_N;
  const CLOUD_LANE_HI = CLOUD_BLOCK_HI + CLOUD_WISP_SPARE;
  let cloudBaseY = CLOUD_BASE_Y;
  const bnd = world && world.bounds ? world.bounds : null;
  if (bnd) {
    if (bnd.center) {
      cloudCX = bnd.center.x || 0;
      cloudCZ = bnd.center.z || 0;
    }
    if (typeof bnd.minX === 'number' && typeof bnd.maxX === 'number') {
      cloudHalfX = Math.max(150, (bnd.maxX - bnd.minX) * 0.8);
    }
    if (typeof bnd.minZ === 'number' && typeof bnd.maxZ === 'number') {
      cloudHalfZ = Math.max(150, (bnd.maxZ - bnd.minZ) * 0.8);
    }
  }
  if (world && world.quarters) {
    let top = -Infinity;
    for (let i = 0; i < world.quarters.length; i++) {
      const q = world.quarters[i];
      if (q && q.labelPosition && q.labelPosition.y > top) top = q.labelPosition.y;
    }
    // The base still follows a world whose tallest roof is above it, but only six metres over
    // the label: the old plus eighty put the whole deck above a frame that cannot look up,
    // which is exactly why the steady sky was empty air.
    if (top > -Infinity) cloudBaseY = Math.max(CLOUD_BASE_Y, top + 6);
  }

  // ------------------------------------------------------- block tier cloud lane
  // The wisp lane is framed for the district tier, and the block tier pulls the camera to 0.62
  // of that distance, so those plates read mid frame there and the top of the phone's block
  // frame is bare sky. The second steady-hour lane is therefore placed by projection, not by
  // taste, from the numbers camera.mjs already fixes: FOV 45, a fixed 40 degree pitch, a 1.05
  // pad, and tier zoom 0.62 for the block tier. At 412x915 the narrow axis is the horizontal
  // one, so districtDist = 114 * 1.05 / sin(atan(tan 22.5 * 412/915)) = 652.86 m, and the
  // block camera stands 0.62 * 652.86 * sin 40 = 260.17 m over the look-at target and
  // 0.62 * 652.86 * cos 40 = 310.07 m behind it. A plate at height y and depth z projects at
  //   A = y - 12 - 260.17,  B = (z - 4) - 310.07,
  //   ndcY = (cos40 A - sin40 B) / (-(sin40 A + cos40 B)) / tan22.5,
  // and the top tenth of the frame is ndcY 0.8 to 1.0. Solving ndcY = 0.90 at the deck base
  // gives B = A * (cos40 + 0.9 tan22.5 sin40) / (sin40 - 0.9 tan22.5 cos40) = 2.815 A, so the
  // lane sits at z = -120 m, 124 m behind the look-at: its plate centres land at ndcY 0.90 and
  // its corners, plate depth and yaw included, stay inside 0.82 to 0.99. The same lane reads
  // ndcY 0.47 to 0.10 at the block framing for the district wisp lane, which is exactly the
  // empty top the fix is for, and ndcY 1.14 at the desktop district framing, so the district
  // picture those plates are not part of does not move.
  let cloudBlockZ = 0;
  const cloudBlockY0 = cloudBaseY;
  {
    const tanHalfV = Math.tan(22.5 * Math.PI / 180);
    const aspect = 412 / 915;
    const halfH = Math.atan(tanHalfV * aspect);
    const frameRadius = bnd && typeof bnd.radius === 'number' ? bnd.radius : 114;
    const district = (frameRadius * 1.05) / Math.sin(halfH);
    const d = district * 0.62;
    const sinP = Math.sin(40 * Math.PI / 180);
    const cosP = Math.cos(40 * Math.PI / 180);
    const cy = bnd && bnd.center ? (bnd.center.y || 0) : 0;
    const cz = bnd && bnd.center ? (bnd.center.z || 0) : 0;
    const camZ = cz + d * cosP;
    const A = cloudBlockY0 - cy - d * sinP;
    const M = 0.90;
    const B = A * (cosP + M * tanHalfV * sinP) / (sinP - M * tanHalfV * cosP);
    cloudBlockZ = camZ + B;
  }

  function windowInit() {
    const wc = world && world.windows && world.windowCount ? world.windowCount | 0 : 0;
    if (wc <= 0 || !world.windows || !world.windowQuarter) return;
    windowRank = new Float32Array(wc);
    for (let i = 0; i < wc; i++) windowRank[i] = hash01(i * 2654435761 + 17);
    windowState = new Uint8Array(wc);
    windowState.fill(2);
    winCursor = 0;
    winSwept = 0;
    needDirty = true;
  }

  function quarterCountNow() {
    const mq = modelRef && modelRef.quarters ? modelRef.quarters.length : 0;
    const n = Math.min(mq, wqCount);
    return n < 0 ? 0 : n;
  }

  // ------------------------------------------------------------ rebuild pass
  function rebuild(next) {
    modelRef = next || null;
    qN = quarterCountNow();
    if (qN > qCap) {
      qCap = qN;
      needArr = new Float32Array(qCap);
      needPrev = new Float32Array(qCap);
      needPrev.fill(-1);
      craneCur = new Int16Array(qCap);
      cranePrev = new Int16Array(qCap);
      cranePrev.fill(-1);
      neglectedState = new Uint8Array(qCap);
      neglectedState.fill(2);
      routes = null;
    }
    buildAccess();
    buildWalkLines();
    buildDoorAnchors();
    buildDoorSides();
    buildVanRoutes();
    buildPedRouteTable();
    buildPedRoutes();
    buildParcelOrder();
    parcelInit = false;
    for (let q = 0; q < qCap; q++) {
      needPrev[q] = -1;
      cranePrev[q] = -1;
      neglectedState[q] = 2;
    }
    for (let i = 0; i < VAN_DELIVERY; i++) {
      if (!vanOn[i]) continue;
      vanOn[i] = 0;
      hideAt(vanMesh, i);
      hideAt(payloads, i);
    }
    vanLive = 0;
    dirtyMatrix(vanMesh, 0, VAN_DELIVERY);
    dirtyMatrix(payloads, 0, VAN_DELIVERY);
    hideAllPeds();
    for (let i = 0; i < slotN; i++) signCursor[i] = 0;
    signsDirty = true;
    signTimer = 0;
  }

  // A live export install calls this, never `rebuild` directly: the rebuild empties every
  // overlay pool, so the refill that follows is a re-population and opens the window. The
  // build-time call at the bottom of the factory keeps the plain rebuild, because the first
  // frames of a page are the loading burst and slicing them would only delay the first look.
  function setDataModelLive(next) {
    rebuild(next);
    beginResync('rebuild');
  }

  // --------------------------------------------------------------- tram step
  function stepTram(dt) {
    if (!tramTable) return;
    if (tramDwell > 0) {
      tramDwell -= dt;
      if (tramDwell < 0) tramDwell = 0;
    } else {
      const step = TRAM_SPEED * dt;
      if (tramStopN > 0) {
        let nextS = tramStopS[tramNext];
        let dist = nextS - tramS;
        if (dist < 0) dist += tramLen;
        if (dist <= step) {
          tramS = nextS;
          tramDwell = TRAM_DWELL;
          tramNext = (tramNext + 1) % tramStopN;
        } else {
          tramS = mod(tramS + step, tramLen);
        }
      } else {
        tramS = mod(tramS + step, tramLen);
      }
    }
    // Three articulated segments with a visible gap between the bodies.
    for (let c = 0; c < TRAM_SEGS; c++) {
      const s = mod(tramS - c * TRAM_GAP, tramLen);
      tableAt(tramTable, s, sPos, sTan);
      const ang = Math.atan2(sTan.x, sTan.z);
      writeMatrix(tramBody, c, sPos.x, sPos.y + 0.1, sPos.z, ang, 1, 1, 1);
      writeMatrix(tramStripe, c, sPos.x, sPos.y + 0.1, sPos.z, ang, 1, 1, 1);
      writeMatrix(tramWins, c, sPos.x, sPos.y + 0.1, sPos.z, ang, 1, 1, 1);
      if (lampOn) {
        // The destination blind on the nose of every carriage, warm against the white body.
        const ca = Math.cos(ang);
        const sa = Math.sin(ang);
        writeMatrixCS(nightLights, LIGHT_TRAM + c,
          sPos.x + sa * 4.24, sPos.y + 2.42, sPos.z + ca * 4.24, ca, sa, 1.05, 0.32, LAMP_D);
      }
    }
    dirtyMatrix(tramBody, 0, TRAM_SEGS);
    dirtyMatrix(tramStripe, 0, TRAM_SEGS);
    dirtyMatrix(tramWins, 0, TRAM_SEGS);
    if (lampOn) dirtyMatrix(nightLights, LIGHT_TRAM, LIGHT_TRAM + TRAM_SEGS);
  }

  function tramPositionAt(out) {
    const o = out || tramOut;
    if (!tramTable) {
      o.set(0, 0, 0);
      return o;
    }
    tableAt(tramTable, tramS, sPos, sTan);
    o.set(sPos.x, sPos.y + 0.1, sPos.z);
    return o;
  }

  // ------------------------------------------------------------ car dynamics
  function ringFactorAt(s, congestion) {
    let f = 1;
    const L = ringLen;
    const w = L * (0.014 + 0.08 * congestion);
    for (let k = 0; k < 2; k++) {
      let dz = s - zoneS[k];
      const half = L * 0.5;
      while (dz > half) dz -= L;
      while (dz < -half) dz += L;
      const ad = dz < 0 ? -dz : dz;
      if (ad < w) {
        const t = ad / w;
        const dip = 0.1 + 0.9 * t * t;
        if (dip < f) f = dip;
      }
    }
    const ph = (s / L) * TWO_PI * 4 - clock * 0.85;
    const wv = Math.sin(ph);
    const stop = 0.5 + 0.5 * wv;
    const wave = 1 - congestion * 0.82 * stop * stop;
    if (wave < f) f = wave;
    return f < 0.06 ? 0.06 : f;
  }

  // ------------------------------------------------------- lane following
  // A lane is an arc length plus a bin index over it. head[b] is the newest car in bin b and
  // next[i] links the rest of that bin's chain, so a lookup walks the three or four bins the
  // clamp can see and never scans the pool. Bins are refilled every frame with fill(-1), which
  // writes no memory that was not allocated at build time, so the whole following model runs
  // without a single allocation in the step.
  function makeLaneGrid(cap, laneLen, laneDir, laneOf, pos) {
    const n = laneLen.length;
    const laneBins = new Int32Array(n);
    const laneBase = new Int32Array(n);
    let total = 0;
    for (let l = 0; l < n; l++) {
      let b = Math.ceil(laneLen[l] * FOLLOW_BIN_INV);
      if (b < 2) b = 2;
      laneBins[l] = b;
      laneBase[l] = total;
      total += b;
    }
    const grid = {
      cap: cap,
      laneLen: laneLen,
      laneDir: laneDir,
      laneBins: laneBins,
      laneBase: laneBase,
      totalBins: total,
      head: new Int32Array(total),
      next: new Int32Array(cap),
      laneOf: laneOf,
      pos: pos
    };
    grid.head.fill(-1);
    return grid;
  }

  function gridBin(g, lane, s) {
    const nb = g.laneBins[lane];
    let b = (s * FOLLOW_BIN_INV) | 0;
    if (b >= nb) b = nb - 1;
    else if (b < 0) b = 0;
    return b;
  }

  function gridInsert(g, i, s) {
    const lane = g.laneOf[i];
    const at = g.laneBase[lane] + gridBin(g, lane, s);
    g.next[i] = g.head[at];
    g.head[at] = i;
  }

  // Distance ahead to the nearest car in the same lane, or -1 when nothing is inside the
  // lookahead. The bins wrap: every pool holds a fixed count on its own arc, so a car beyond
  // the end of the table is the car that re-enters at the start of it. A car of the same bin
  // that is behind us produces a distance close to the lane length and is rejected by the
  // reach test.
  function gridAhead(g, car, s) {
    const lane = g.laneOf[car];
    const len = g.laneLen[lane];
    const nb = g.laneBins[lane];
    const base = g.laneBase[lane];
    const dir = g.laneDir[lane];
    const pos = g.pos;
    const b = gridBin(g, lane, s);
    let best = FOLLOW_REACH;
    for (let k = 0; k <= FOLLOW_LOOKAHEAD; k++) {
      const bb = mod(b + dir * k, nb);
      for (let j = g.head[base + bb]; j >= 0; j = g.next[j]) {
        if (j === car) continue;
        let d = dir > 0 ? pos[j] - s : s - pos[j];
        d = mod(d, len);
        if (d < best) best = d;
      }
    }
    return best === FOLLOW_REACH ? -1 : best;
  }

  // The mirror lookup: distance behind to the nearest car in the same lane, or -1. Only an
  // entry uses it, to make sure the car it drops into a lane is not inside the body behind it.
  function gridBehind(g, car, s) {
    const lane = g.laneOf[car];
    const len = g.laneLen[lane];
    const nb = g.laneBins[lane];
    const base = g.laneBase[lane];
    const dir = g.laneDir[lane];
    const pos = g.pos;
    const b = gridBin(g, lane, s);
    let best = FOLLOW_REACH;
    for (let k = 0; k <= FOLLOW_LOOKAHEAD; k++) {
      const bb = mod(b - dir * k, nb);
      for (let j = g.head[base + bb]; j >= 0; j = g.next[j]) {
        if (j === car) continue;
        let d = dir > 0 ? s - pos[j] : pos[j] - s;
        d = mod(d, len);
        if (d < best) best = d;
      }
    }
    return best === FOLLOW_REACH ? -1 : best;
  }

  // Drop a car entering a lane (a spawn, before it is inserted) into the first arc where both
  // neighbours are at least FOLLOW_SPAWN_ROOM away. Bounded: at most FOLLOW_SPAWN_TRIES steps,
  // each past the car that was too close ahead, so even a full lane terminates instead of
  // scanning the pool. A lane that has no room returns -1: the car waits off stage rather than
  // being dropped inside a neighbour, because a lane that cannot hold another body is a lane
  // that must not show one. The car is not inserted here: the caller inserts it before any
  // speed is asked for, so the frame that admits a car also makes its neighbours follow it.
  function placeCar(g, car, s) {
    const lane = g.laneOf[car];
    const len = g.laneLen[lane];
    const dir = g.laneDir[lane];
    let p = s;
    for (let tries = 0; tries < FOLLOW_SPAWN_TRIES; tries++) {
      const a = gridAhead(g, car, p);
      const b = gridBehind(g, car, p);
      if ((a < 0 || a >= FOLLOW_SPAWN_ROOM) && (b < 0 || b >= FOLLOW_SPAWN_ROOM)) return p;
      const step = a >= 0 && a < FOLLOW_SPAWN_ROOM ? a + FOLLOW_SPAWN_ROOM : FOLLOW_SPAWN_ROOM;
      p = mod(p + dir * step, len);
    }
    return -1;
  }

  // A car must never enter the world already inside the junction approach: the gate treats a
  // car inside JUNCTION_GAP as committed, so a car that spawned there would be run over on its
  // first frame. The lane check runs first and always, then the arc is pushed clear of the box
  // and checked again. Returns -1 when neither the lane nor the escape has room.
  function placeClearOfJunction(g, car, s, sCross, dir, len) {
    let p = placeCar(g, car, s);
    if (p < 0 || sCross < 0 || len <= 0) return p;
    for (let t = 0; t < 3; t++) {
      const ahead = mod(dir * (sCross - p), len);
      if (ahead > JUNCTION_GAP + 0.5) return p;
      p = placeCar(g, car, mod(p + dir * (ahead + JUNCTION_GAP + 1), len));
      if (p < 0) return -1;
    }
    return mod(dir * (sCross - p), len) > JUNCTION_GAP + 0.5 ? p : -1;
  }

  // Ring cars: density is the switch rate, so a fragmented hour packs the ring and stop and
  // go waves form, while a focus block leaves a trickle.
  function stepRingCars(dt, congestion) {
    let target = Math.round(10 + 240 * congestion);
    const scaleCap = Math.round(ringCap * popScale);
    if (target > ringCap) target = ringCap;
    if (target > scaleCap) target = scaleCap;
    if (target < 4) target = 4;
    ringShown = rampSlots(RESYNC_RING, ringShown, target, dt);
    const hi = ringShown < 0.02 ? 0 : Math.min(ringCap, Math.ceil(ringShown));
    const amp = clamp01((congestion - 0.5) / 0.5);
    const g = ringGrid;
    g.head.fill(-1, 0, g.totalBins);
    // Pass one: retire the cars that faded out and refill the bins with the ones that stay.
    // A car that turns visible this frame is marked 2 and placed in pass two, so its entry arc
    // is checked against the live lane before any of its neighbours ask for a speed.
    for (let i = 0; i < hi; i++) {
      const vis = visibility(ringShown, i);
      if (vis <= 0.02) {
        if (ringAct[i]) {
          ringAct[i] = 0;
          hideVehicle(i);
          hideVehicleLights(i);
        }
        continue;
      }
      if (!ringAct[i]) {
        ringAct[i] = 2;
        ringS[i] = mod(ringS[i] + ringLen * 0.37, ringLen);
        ringV[i] = RING_SPEED * 0.5;
      } else if (ringAct[i] === 1) {
        gridInsert(g, i, ringS[i]);
      }
    }
    for (let i = 0; i < hi; i++) {
      if (ringAct[i] !== 2) continue;
      const placed = placeCar(g, i, ringS[i]);
      if (placed < 0) {
        // No room in the lane: the car waits hidden and tries again next frame. Showing it
        // inside a neighbour would be the one thing the gap law exists to prevent.
        ringAct[i] = 0;
        hideVehicle(i);
        hideVehicleLights(i);
        continue;
      }
      ringS[i] = placed;
      gridInsert(g, i, ringS[i]);
      ringAct[i] = 1;
    }
    // Pass two: the data wave sets the wanted speed, the lane sets the ceiling. The clamp is
    // applied after the ease so the safe speed is never overshot, and a car whose gap is at or
    // under MIN_GAP stands still with a full brake level.
    for (let i = 0; i < hi; i++) {
      if (ringAct[i] !== 1) continue;
      const vis = visibility(ringShown, i);
      const s = ringS[i];
      const f = amp > 0.001 ? ringFactorAt(s, amp) : 1;
      const targetV = RING_SPEED * f;
      let v = ringV[i] + (targetV - ringV[i]) * Math.min(1, dt * FOLLOW_EASE);
      if (v < 0) v = 0;
      const gap = gridAhead(g, i, s);
      if (gap >= 0) {
        const lim = (gap - FOLLOW_MIN_GAP) / FOLLOW_HEADWAY;
        if (v > lim) v = lim < 0 ? 0 : lim;
      }
      // Signal obedience: the ring holds at its own stop line at each avenue arm whenever that
      // junction's ring phase is not green, and takes the box and clearance rules when it is.
      // Both arms of the mouth share one phase group, so the pair never costs two stops for a
      // single green, and the queue behind a stopped car forms through gridAhead above.
      for (let a = 0; a < 2; a += 1) {
        const armJ = ringSignalJ[a];
        if (armJ < 0) continue;
        const ahead = mod(ringSignalS[a] - s, ringLen);
        const jl = signalClamp(ahead, gap, sigPhase[armJ] === 2, sigRingLeft[armJ], v);
        if (v > jl) v = jl;
      }
      ringV[i] = v;
      let ns = s + v * dt;
      if (ns >= ringLen) ns -= ringLen;
      ringS[i] = ns;
      // Diagnostics: a ring car crosses a junction when its own arc passes that junction's arm.
      for (let a = 0; a < 2; a += 1) {
        const armJ = ringSignalJ[a];
        if (armJ < 0) continue;
        const before = mod(ringSignalS[a] - s, ringLen);
        const after = mod(ringSignalS[a] - ns, ringLen);
        if (after > before) noteCross(armJ, SIG_FAMILY_RING);
      }
      tableAt(ringTable, ns, sPos, sTan);
      const off = ringLane[i];
      const px = sPos.x - sTan.z * off;
      const pz = sPos.z + sTan.x * off;
      // Under half speed the stop and go wave has the car crawling: the tail pair comes up to
      // brake brightness and a red pool grows behind it.
      const crawl = 1 - v * RING_BRAKE_INV;
      writeVehicle(i, px, sPos.y, pz, Math.atan2(sTan.x, sTan.z), vis,
        crawl > 0 ? crawl : 0);
    }
    for (let j = hi; j < ringDrawn; j++) {
      ringAct[j] = 0;
      hideVehicle(j);
      hideVehicleLights(j);
    }
    const span = hi > ringDrawn ? hi : ringDrawn;
    ringDrawn = hi;
    dirtyMatrix(cars, 0, span);
    if (lampOn) {
      dirtyMatrix(nightLights, LIGHT_CAR, LIGHT_CAR + span * CAR_SLOTS);
      if (tailWrites !== tailMark) dirtyColour(nightLights, LIGHT_CAR, LIGHT_CAR + span * CAR_SLOTS);
    }
  }

  // Avenue cars: a steady trickle, and the full carriageway when a long session is still
  // open at the data clock. The green wave also blends their body colours partway toward the
  // signal green, in refreshCarColors, so the lane reads as flow rather than as paint.
  function stepAvenueCars(dt, congestion) {
    if (!aveTable) return;
    let target = greenWave ? aveCap : Math.round(8 + 42 * congestion);
    const scaleCap = Math.round(aveCap * popScale);
    if (target > aveCap) target = aveCap;
    if (target > scaleCap) target = scaleCap;
    if (target < 2) target = 2;
    aveShown = rampSlots(RESYNC_AVE, aveShown, target, dt);
    const hi = aveShown < 0.02 ? 0 : Math.min(aveCap, Math.ceil(aveShown));
    const speedMul = greenWave ? 1.28 : 1;
    const g = aveGrid;
    g.head.fill(-1, 0, g.totalBins);
    for (let i = 0; i < hi; i++) {
      const vis = visibility(aveShown, i);
      if (vis <= 0.02) {
        if (aveAct[i]) {
          aveAct[i] = 0;
          hideVehicle(aveBase + i);
          hideVehicleLights(aveBase + i);
        }
        continue;
      }
      if (!aveAct[i]) aveAct[i] = 2;
      else if (aveAct[i] === 1) gridInsert(g, i, aveS[i]);
    }
    for (let i = 0; i < hi; i++) {
      if (aveAct[i] !== 2) continue;
      const placed = placeClearOfJunction(g, i, aveS[i],
        aveCrossS[aveLane[i] > 0 ? 1 : 0], aveDir[i], aveLen);
      if (placed < 0) {
        aveAct[i] = 0;
        hideVehicle(aveBase + i);
        hideVehicleLights(aveBase + i);
        continue;
      }
      aveS[i] = placed;
      aveV[i] = 0;
      gridInsert(g, i, aveS[i]);
      aveAct[i] = 1;
    }
    for (let i = 0; i < hi; i++) {
      if (aveAct[i] !== 1) continue;
      const vis = visibility(aveShown, i);
      const dir = aveDir[i];
      const targetV = AVE_SPEED * speedMul;
      let v = aveV[i] + (targetV - aveV[i]) * Math.min(1, dt * FOLLOW_EASE);
      if (v < 0) v = 0;
      const gap = gridAhead(g, i, aveS[i]);
      if (gap >= 0) {
        const lim = (gap - FOLLOW_MIN_GAP) / FOLLOW_HEADWAY;
        if (v > lim) v = lim < 0 ? 0 : lim;
      }
      // Signal obedience at the avenue mouth. The lamp replaced the priority yield: when the
      // avenue is green the car goes, and when it is amber or red it holds JUNCTION_GAP short
      // of the crossing through the lane clamp's own law, so the tail lamps read the queue and
      // the cars behind stop behind it through gridAhead. Amber is a hold for the entering car;
      // a car already inside the gap is committed and clears. The green light keeps the box
      // rule and the clearance-time check, and it waits while a ring car that entered on its
      // own green is still crawling out of the box. v is still the free speed here: that is
      // what both the clearance time and the yield are measured against, since a car about to
      // be held would otherwise always read as slow.
      const side = aveLane[i] > 0 ? 1 : 0;
      const armJ = aveSignal[side];
      if (armJ >= 0 && aveCrossS[side] >= 0) {
        const ahead = mod(dir * (aveCrossS[side] - aveS[i]), aveLen);
        const green = sigPhase[armJ] === 0;
        let jl = signalClamp(ahead, gap, green, sigAveLeft[armJ], v);
        if (green && jl === Infinity && ringBoxBusy(aveCrossRingS[side])) {
          jl = ahead > JUNCTION_GAP ? (ahead - JUNCTION_GAP) / JUNCTION_HEADWAY : Infinity;
        }
        if (v > jl) v = jl;
      } else {
        // A world built without the junction record keeps the old priority yield: the ring is
        // the arterial and the lower road gives way by geometry.
        const jl = junctionLimit(aveS[i], dir, aveCrossS[side], aveCrossRingS[side], aveLen, gap, v);
        if (v > jl) v = jl;
      }
      aveV[i] = v;
      const prevS = aveS[i];
      let s = prevS + dir * v * dt;
      // The lane is a loop in arc space: a car past one end is the car that re-enters at the
      // other, exactly as the pool has always wrapped, but now with a gap it already held.
      if (s > aveLen) s -= aveLen;
      else if (s < 0) s += aveLen;
      aveS[i] = s;
      // Diagnostics: an avenue car crosses its arm when its own arc passes it.
      if (armJ >= 0 && aveCrossS[side] >= 0) {
        const before = mod(dir * (aveCrossS[side] - prevS), aveLen);
        const after = mod(dir * (aveCrossS[side] - s), aveLen);
        if (after > before) noteCross(armJ, SIG_FAMILY_AVENUE);
      }
      tableAt(aveTable, s, sPos, sTan);
      const off = aveLane[i];
      const px = sPos.x - sTan.z * off;
      const pz = sPos.z + sTan.x * off;
      let ang = Math.atan2(sTan.x, sTan.z);
      if (dir < 0) ang += Math.PI;
      // A queued avenue car shows the same brake level the ring writes: nothing while it
      // cruises, red tails and a pool as the gap closes. The lamps only draw at night, so a
      // daytime queue still reads through its spacing.
      let brake = 1 - v * AVE_BRAKE_INV;
      if (brake < 0) brake = 0;
      writeVehicle(aveBase + i, px, sPos.y, pz, ang, vis, brake);
    }
    for (let j = hi; j < aveDrawn; j++) {
      aveAct[j] = 0;
      hideVehicle(aveBase + j);
      hideVehicleLights(aveBase + j);
    }
    const span = hi > aveDrawn ? hi : aveDrawn;
    aveDrawn = hi;
    dirtyMatrix(cars, aveBase, aveBase + span);
    if (lampOn) {
      dirtyMatrix(nightLights, LIGHT_CAR + aveBase * CAR_SLOTS,
        LIGHT_CAR + (aveBase + span) * CAR_SLOTS);
      if (tailWrites !== tailMark) {
        dirtyColour(nightLights, LIGHT_CAR + aveBase * CAR_SLOTS,
          LIGHT_CAR + (aveBase + span) * CAR_SLOTS);
      }
    }
  }

  // Spurs: arterials that leave the frame, so a car wraps from one end to the other instead
  // of turning round in the middle of the view.
  function stepSpurCars(dt, congestion) {
    if (!spurTables.length || spurCap === 0) return;
    let target = Math.round(4 + 26 * congestion);
    const scaleCap = Math.round(spurCap * popScale);
    if (target > spurCap) target = spurCap;
    if (target > scaleCap) target = scaleCap;
    if (target < 2) target = 2;
    spurShown = rampSlots(RESYNC_SPUR, spurShown, target, dt);
    const hi = spurShown < 0.02 ? 0 : Math.min(spurCap, Math.ceil(spurShown));
    const g = spurGrid;
    g.head.fill(-1, 0, g.totalBins);
    for (let i = 0; i < hi; i++) {
      const vis = visibility(spurShown, i);
      const out = spurBase + i;
      if (vis <= 0.02) {
        if (spurAct[i]) {
          spurAct[i] = 0;
          hideVehicle(out);
          hideVehicleLights(out);
        }
        continue;
      }
      if (!spurAct[i]) spurAct[i] = 2;
      else if (spurAct[i] === 1) gridInsert(g, i, spurS[i]);
    }
    for (let i = 0; i < hi; i++) {
      if (spurAct[i] !== 2) continue;
      const sp = spurTab[i] % spurTables.length;
      const tabS = spurTables[sp];
      const placed = tabS
        ? placeClearOfJunction(g, i, spurS[i], 0, spurDir[i], tabS.len)
        : -1;
      if (placed < 0) {
        spurAct[i] = 0;
        hideVehicle(spurBase + i);
        hideVehicleLights(spurBase + i);
        continue;
      }
      spurS[i] = placed;
      spurV[i] = 0;
      gridInsert(g, i, spurS[i]);
      spurAct[i] = 1;
    }
    for (let i = 0; i < hi; i++) {
      if (spurAct[i] !== 1) continue;
      const vis = visibility(spurShown, i);
      const out = spurBase + i;
      const tab = spurTables[spurTab[i] % spurTables.length];
      if (!tab) continue;
      const dir = spurDir[i];
      const targetV = SPUR_SPEED;
      let v = spurV[i] + (targetV - spurV[i]) * Math.min(1, dt * FOLLOW_EASE);
      if (v < 0) v = 0;
      const gap = gridAhead(g, i, spurS[i]);
      if (gap >= 0) {
        const lim = (gap - FOLLOW_MIN_GAP) / FOLLOW_HEADWAY;
        if (v > lim) v = lim < 0 ? 0 : lim;
      }
      // The spur mouth is the crossing: the ring is the arterial and the spur car yields at
      // its own first arc. The same clearance, the same headway, the same brake path.
      const sp = spurTab[i] % spurTables.length;
      const jl = junctionLimit(spurS[i], dir, 0, spurCrossRingS[sp], tab.len, gap, v);
      if (v > jl) v = jl;
      spurV[i] = v;
      let s = spurS[i] + dir * v * dt;
      s = mod(s, tab.len);
      spurS[i] = s;
      tableAt(tab, s, sPos, sTan);
      const off = spurLane[i];
      const px = sPos.x - sTan.z * off;
      const pz = sPos.z + sTan.x * off;
      let ang = Math.atan2(sTan.x, sTan.z);
      if (dir < 0) ang += Math.PI;
      let brake = 1 - v * SPUR_BRAKE_INV;
      if (brake < 0) brake = 0;
      writeVehicle(out, px, sPos.y, pz, ang, vis, brake);
    }
    for (let j = hi; j < spurDrawn; j++) {
      spurAct[j] = 0;
      hideVehicle(spurBase + j);
      hideVehicleLights(spurBase + j);
    }
    const span = hi > spurDrawn ? hi : spurDrawn;
    spurDrawn = hi;
    dirtyMatrix(cars, spurBase, spurBase + span);
    if (lampOn) {
      dirtyMatrix(nightLights, LIGHT_CAR + spurBase * CAR_SLOTS,
        LIGHT_CAR + (spurBase + span) * CAR_SLOTS);
      if (tailWrites !== tailMark) {
        dirtyColour(nightLights, LIGHT_CAR + spurBase * CAR_SLOTS,
          LIGHT_CAR + (spurBase + span) * CAR_SLOTS);
      }
    }
  }

  // The elevated expressway: a packed stream on the deck, every car trailing a red brake
  // lamp. The same ring rule drives it, so a fragmented hour crawls and the lamps brighten.
  // Its count is rule 1's term and nothing else: the only traffic data in the model is
  // switchRateAt, and every other term (screen activity, sessions, parcels) describes the
  // desk, not the motorway, so a quiet data hour keeps eight cars. What carries the busy read
  // is how those eight sit and how bright they are: they hold a headway instead of a slot
  // lattice, and the plates carry a tail lamp glow instead of going nearly black at free speed.
  function stepExpressway(dt, congestion) {
    if (!expTable || expCap === 0) return;
    const scaleCap = Math.round(expCap * popScale);
    let target = Math.round(8 + 72 * congestion);
    if (target > expCap) target = expCap;
    if (target > scaleCap) target = scaleCap;
    if (target < 4) target = 4;
    expShown = rampSlots(RESYNC_EXP, expShown, target, dt);

    const fast = EXP_SPEED * (1 - 0.9 * clamp01((congestion - 0.25) / 0.75));
    const speed = fast < EXP_CRAWL ? EXP_CRAWL : fast;
    for (let l = 0; l < EXP_LANES; l++) {
      let base = expLaneBase[l] + expLaneDir[l] * speed * dt;
      base = mod(base, expLen);
      expLaneBase[l] = base;
    }

    const brake = clamp01((5.5 - speed) / 4.3);
    const hi = expShown < 0.02 ? 0 : Math.min(expCap, Math.ceil(expShown));
    // A lane's cars hold a headway at the current speed, never closer than the minimum gap and
    // never longer than the deck can share out: at eight cars that is two cars per lane at
    // 17 m, and since the two directions run on opposite carriageways their phases keep
    // drifting apart, so the eight plates read as an intermittent stream down the deck. At a
    // jammed hour it closes onto the bumper to bumper spacing the old slot grid gave.
    const laneCars = Math.max(1, Math.ceil(hi / EXP_LANES));
    const headway = speed * EXP_HEADWAY_S;
    expSpacing = headway > EXP_MIN_GAP ? headway : EXP_MIN_GAP;
    const deckFit = expLen / laneCars;
    if (expSpacing > deckFit) expSpacing = deckFit;
    // Tail lamps, not brake lamps, at free speed; saturated red when the wave has bitten. The
    // floor is what makes the eight plates read as a light stream at district range.
    const colScale = 0.4 + 0.6 * brake;
    for (let i = 0; i < hi; i++) {
      const vis = visibility(expShown, i);
      const lane = expLane[i];
      const out = expBase + i;
      if (vis <= 0.02) {
        if (expAct[i]) {
          expAct[i] = 0;
          hideVehicle(out);
          hideAt(brakeLights, i);
          hideVehicleLights(out);
        }
        continue;
      }
      if (!expAct[i]) expAct[i] = 1;
      const slot = (i / EXP_LANES) | 0;
      const s = mod(expLaneBase[lane] + slot * expSpacing, expLen);
      tableAt(expTable, s, sPos, sTan);
      const off = expLaneOff[lane];
      const px = sPos.x - sTan.z * off;
      const pz = sPos.z + sTan.x * off;
      let ang = Math.atan2(sTan.x, sTan.z);
      if (expLaneDir[lane] < 0) ang += Math.PI;
      const deck = EXP_DECK_Y === null ? sPos.y : EXP_DECK_Y;
      writeVehicle(out, px, deck, pz, ang, vis, brake);
      // The lamp sits on the trailing face, which is the back along the travel direction.
      const back = expLaneDir[lane];
      const lx = px - sTan.x * back * 2.16;
      const lz = pz - sTan.z * back * 2.16;
      writeMatrix(brakeLights, i, lx, deck + 0.72, lz, ang, vis, vis, vis);
      const flick = 0.6 + 0.4 * Math.sin(clock * 2.6 + i * 1.9);
      const b = clamp01(colScale * flick + 0.22);
      brakeColor.setRGB(b, b * 0.07, b * 0.05);
      brakeLights.setColorAt(i, brakeColor);
    }
    for (let j = hi; j < expDrawn; j++) {
      expAct[j] = 0;
      // The rider goes with its own car: a scooter that leaves the deck must not leave its
      // blob hanging over the parapet.
      hideVehicle(expBase + j);
      hideAt(brakeLights, j);
      hideVehicleLights(expBase + j);
    }
    const span = hi > expDrawn ? hi : expDrawn;
    expDrawn = hi;
    dirtyMatrix(cars, expBase, expBase + span);
    dirtyMatrix(brakeLights, 0, span);
    dirtyColour(brakeLights, 0, span);
    if (lampOn) {
      dirtyMatrix(nightLights, LIGHT_CAR + expBase * CAR_SLOTS,
        LIGHT_CAR + (expBase + span) * CAR_SLOTS);
      if (tailWrites !== tailMark) {
        dirtyColour(nightLights, LIGHT_CAR + expBase * CAR_SLOTS,
          LIGHT_CAR + (expBase + span) * CAR_SLOTS);
      }
    }
  }

  // ---------------------------------------------------------------- bus step
  // Two long white buses with a coloured band, running the avenue kerb lanes in opposite
  // directions. The kerb lane keeps them clear of the cars in the inner lane.
  function stepBuses(dt) {
    if (!AVE_BUS_ON || !aveTable) return;
    // The buses are a fixed service: the population scale either runs both or, on the
    // lowest notch, retires them with the same fade band as every other pool.
    const target = popScale < 0.32 ? 0 : BUS_COUNT;
    busShown = rampSlots(RESYNC_BUS, busShown, target, dt);
    for (let i = 0; i < BUS_COUNT; i++) {
      const vis = visibility(busShown, i);
      if (vis <= 0.02) {
        if (busAct[i]) {
          busAct[i] = 0;
          hideAt(busBody, i);
          hideAt(busBand, i);
          const lb = LIGHT_BUS + i * BUS_SLOTS;
          for (let k = 0; k < BUS_SLOTS; k++) hideAt(nightLights, lb + k);
        }
        continue;
      }
      if (!busAct[i]) {
        busAct[i] = 1;
        busS[i] = i === 0 ? aveLen * 0.18 : aveLen * 0.72;
      }
      const dir = i === 0 ? -1 : 1;
      let s = busS[i] + dir * (AVE_SPEED * 0.86) * dt;
      if (s > aveLen) s -= aveLen;
      else if (s < 0) s += aveLen;
      busS[i] = s;
      tableAt(aveTable, s, sPos, sTan);
      const off = i === 0 ? AVE_BUS_LANE : -AVE_BUS_LANE;
      const px = sPos.x - sTan.z * off;
      const pz = sPos.z + sTan.x * off;
      let ang = Math.atan2(sTan.x, sTan.z);
      if (dir < 0) ang += Math.PI;
      const py = sPos.y + 0.08;
      writeMatrix(busBody, i, px, py, pz, ang, vis, vis, vis);
      writeMatrix(busBand, i, px, py, pz, ang, vis, vis, vis);
      if (lampOn) {
        const c = Math.cos(ang);
        const s = Math.sin(ang);
        const lb = LIGHT_BUS + i * BUS_SLOTS;
        const nose = 5.97;
        const nx = px + s * nose;
        const nz = pz + c * nose;
        // A lit window band the length of the saloon, the destination sign on the nose, the
        // head pair under it and the wash the bus throws on its own kerb lane.
        writeMatrixCS(nightLights, lb, px, py + 2.02, pz, c, s,
          2.58 * vis, 0.62 * vis, 10.9 * vis);
        writeMatrixCS(nightLights, lb + 1, px + s * 8.8, py + 0.06, pz + c * 8.8, c, s,
          3.1 * vis, 0.02, 5.6 * vis);
        writeMatrixCS(nightLights, lb + 2, nx, py + 2.5, nz, c, s, 1.2 * vis, 0.34 * vis, LAMP_D);
        writeMatrixCS(nightLights, lb + 3, nx + c * 0.82 * vis, py + 0.62, nz - s * 0.82 * vis,
          c, s, LAMP_W * vis, LAMP_H * vis, vis);
        writeMatrixCS(nightLights, lb + 4, nx - c * 0.82 * vis, py + 0.62, nz + s * 0.82 * vis,
          c, s, LAMP_W * vis, LAMP_H * vis, vis);
      }
    }
    dirtyMatrix(busBody, 0, BUS_COUNT);
    dirtyMatrix(busBand, 0, BUS_COUNT);
    if (lampOn) dirtyMatrix(nightLights, LIGHT_BUS, LIGHT_TRAM);
  }

  // -------------------------------------------------------------- rail steps
  function railSAtZ(z) {
    if (!railTable) return 0;
    let best = 0;
    let bd = Infinity;
    for (let i = 0; i <= railTable.n; i++) {
      const d = Math.abs(railTable.pos[i * 3 + 2] - z);
      if (d < bd) {
        bd = d;
        best = i;
      }
    }
    return best * railTable.step;
  }

  function railCarPos(s, trackX, outPos, outTan) {
    tableAt(railTable, s, outPos, outTan);
    outPos.x += trackX;
    return outPos;
  }

  // A regional train of four carriages on the free track, shuttling between the two ends of
  // the yard, and a short three carriage train in the opposite direction on the second
  // track, inside the stretch the parked wagons leave clear. Static freight wagons hold the
  // third track.
  function stepTrains(dt) {
    if (!railTable) return;
    const spanA = railSpanA1 - railSpanA0;
    if (spanA > 30) {
      if (railADwell > 0) {
        railADwell -= dt;
        if (railADwell < 0) railADwell = 0;
      } else {
        railAS += railADir * TRAIN_SPEED * dt;
        const hiS = railSpanA1;
        const loS = railSpanA0 + (TRAIN_A_CARS - 1) * TRAIN_A_GAP;
        if (railAS >= hiS) {
          railAS = hiS;
          railADir = -1;
          railADwell = TRAIN_DWELL;
        } else if (railAS <= loS) {
          railAS = loS;
          railADir = 1;
          railADwell = TRAIN_DWELL;
        }
      }
      for (let k = 0; k < TRAIN_A_CARS; k++) {
        const s = railAS - railADir * k * TRAIN_A_GAP;
        railCarPos(s, RAIL_TRACK_A, sPos, sTan);
        writeBasis(railCars, WAGON_SET + k, sPos.x, sPos.y + 0.5, sPos.z, sTan, 1, 1, 1);
      }
    } else {
      for (let k = 0; k < TRAIN_A_CARS; k++) hideAt(railCars, WAGON_SET + k);
    }
    const spanB = railSpanB1 - railSpanB0;
    if (spanB > 30) {
      if (railBDwell > 0) {
        railBDwell -= dt;
        if (railBDwell < 0) railBDwell = 0;
      } else {
        railBS += railBDir * TRAIN_SPEED_B * dt;
        const hiS = railSpanB1;
        const loS = railSpanB0 + (TRAIN_B_CARS - 1) * TRAIN_B_GAP;
        if (railBS >= hiS) {
          railBS = hiS;
          railBDir = -1;
          railBDwell = TRAIN_DWELL;
        } else if (railBS <= loS) {
          railBS = loS;
          railBDir = 1;
          railBDwell = TRAIN_DWELL;
        }
      }
      for (let k = 0; k < TRAIN_B_CARS; k++) {
        const s = railBS - railBDir * k * TRAIN_B_GAP;
        railCarPos(s, RAIL_TRACK_B, sPos, sTan);
        writeBasis(railCars, WAGON_SET + TRAIN_A_CARS + k, sPos.x, sPos.y + 0.5, sPos.z, sTan, 1, 1, 1);
      }
    } else {
      for (let k = 0; k < TRAIN_B_CARS; k++) hideAt(railCars, WAGON_SET + TRAIN_A_CARS + k);
    }
    dirtyMatrix(railCars, 0, RAIL_CAP);
  }

  // ----------------------------------------------------------------- van step
  function dispatchVan(fromQ, toQ, payloadKind) {
    if (!routes || qN < 2) return false;
    const a = fromQ | 0;
    const b = toQ | 0;
    if (a < 0 || b < 0 || a >= qN || b >= qN || a === b) return false;
    const route = routes[a * qN + b];
    if (!route) return false;
    let slot = -1;
    for (let i = 0; i < VAN_DELIVERY; i++) {
      if (!vanOn[i]) {
        slot = i;
        break;
      }
    }
    if (slot < 0) return false;
    const sign = payloadKind === 'sign';
    vanOn[slot] = 1;
    vanS[slot] = 0;
    vanDwell[slot] = 0;
    vanGrow[slot] = 0;
    vanRoute[slot] = route;
    vanKind[slot] = sign ? 1 : 0;
    vanLive++;
    payloads.setColorAt(slot, sign ? signColor : parcelColor);
    dirtyColour(payloads, slot, slot + 1);
    return true;
  }

  function stepVans(dt, dreaming) {
    let hiSlot = 0;
    let payHi = 0;
    for (let i = 0; i < VAN_DELIVERY; i++) {
      if (!vanOn[i]) continue;
      const route = vanRoute[i];
      if (!route) {
        vanOn[i] = 0;
        vanLive--;
        hideAt(vanMesh, i);
        hideAt(payloads, i);
        hiSlot = i + 1;
        continue;
      }
      const sign = vanKind[i] === 1;
      // Every live van owns a plate slot here: a van that keeps its last matrix while the
      // district sleeps keeps its plate too, and a dwelling van's slot is inside the prefix
      // at no cost because it already holds a zero matrix.
      payHi = i + 1;
      // Nothing moves while the district sleeps except the dream and its own sign vans.
      if (dreaming && !sign) continue;
      if (vanDwell[i] > 0) {
        vanDwell[i] -= dt;
        if (vanDwell[i] <= 0) {
          vanDwell[i] = 0;
          vanOn[i] = 0;
          vanLive--;
          hideAt(vanMesh, i);
          hideAt(payloads, i);
          hiSlot = i + 1;
          continue;
        }
      } else {
        let s = vanS[i] + VAN_SPEED * dt;
        if (s >= route.len) {
          s = route.len;
          vanDwell[i] = 1.4;
        }
        vanS[i] = s;
      }
      let g = vanGrow[i] + dt * 2.2;
      if (g > 1) g = 1;
      vanGrow[i] = g;
      hiSlot = i + 1;
      tableAt(route, vanS[i], sPos, sTan);
      const ang = Math.atan2(sTan.x, sTan.z);
      writeMatrix(vanMesh, i, sPos.x, sPos.y + 0.08, sPos.z, ang, g, g, g);
      if (vanDwell[i] > 0) {
        hideAt(payloads, i);
      } else if (sign) {
        writeMatrix(payloads, i, sPos.x, sPos.y + 2.85, sPos.z, ang, 1.5 * g, 1.0 * g, 0.1);
      } else {
        writeMatrix(payloads, i, sPos.x, sPos.y + 2.55, sPos.z, ang, 0.72 * g, 0.72 * g, 0.72 * g);
      }
    }
    if (hiSlot > 0) {
      dirtyMatrix(vanMesh, 0, hiSlot);
      dirtyMatrix(payloads, 0, hiSlot);
    }
    // The plate pool is a prefix too: a slot that is dwelling or idle carries a zero matrix,
    // so the live count is the last slot that actually wrote this frame.
    payloads.count = payHi;
  }

  // --------------------------------------------------------------- ped steps
  // A walker picks a pavement line by a two draw tournament on the destination quarter's
  // activity: at a busy hour the pavements in front of the busiest quarters carry the most
  // walkers, and at a quiet one both draws score zero and the pick is uniform. Both draws
  // come from the same walk line table, so a walker still only ever walks a pavement.
  function pickPedRoute() {
    const n = pedRouteList.length;
    if (n === 0 || !pedRoutes) return null;
    let idx = pedRouteList[(Math.random() * n) | 0];
    if (n > 1 && qN > 1) {
      const alt = pedRouteList[(Math.random() * n) | 0];
      const qa = idx % qN;
      const qb = alt % qN;
      const na = qa < qCap ? needArr[qa] : 0;
      const nb = qb < qCap ? needArr[qb] : 0;
      if (nb > na) idx = alt;
    }
    return pedRoutes[idx] || null;
  }

  function spawnPed(i) {
    let mode = PED_ROUTE;
    if (!pedRoutes || pedRouteList.length === 0) mode = plazaOn ? PED_SQUARE : PED_CROSS;
    else {
      const r = i % 20;
      if (r < 6) mode = PED_ROUTE;
      else if (r < 17) mode = plazaOn ? PED_SQUARE : PED_ROUTE;
      else mode = crossCount > 0 ? PED_CROSS : PED_ROUTE;
    }
    if (mode === PED_CROSS && crossCount === 0) mode = PED_ROUTE;
    if (mode === PED_ROUTE && pedRouteList.length === 0) return false;
    pedMode[i] = mode;
    pedGrow[i] = 0;
    pedOn[i] = 0;
    pedLane[i] = (hash01(i * 17 + 9) * 2 - 1) * 0.82;
    if (mode === PED_ROUTE) {
      const route = pickPedRoute();
      if (!route) return false;
      pedRoute[i] = route;
      pedDir[i] = (hash01(i * 97 + 3) > 0.5) ? 1 : -1;
      const a = route.pavA !== undefined ? route.pavA : 0;
      const b = route.pavB !== undefined ? route.pavB : route.len;
      pedS[i] = a + Math.random() * (b - a);
      pedSpeed[i] = PED_MIN_SPEED + hash01(i * 31 + 5) * (PED_MAX_SPEED - PED_MIN_SPEED);
    } else if (mode === PED_SQUARE) {
      // A golden angle spiral fills the plaza without overlap, then the fountain is cleared.
      const a = i * 2.399963;
      const rad = Math.sqrt(hash01(i * 31 + 7));
      let ax = PLAZA_CX + Math.cos(a) * rad * PLAZA_HX;
      let az = PLAZA_CZ + Math.sin(a) * rad * PLAZA_HZ;
      const dxf = ax - PLAZA_CX;
      const dzf = az - PLAZA_CZ;
      const df = Math.sqrt(dxf * dxf + dzf * dzf);
      if (df < 4.6) {
        const k = df > 0.001 ? 4.6 / df : 4.6;
        ax = PLAZA_CX + dxf * k;
        az = PLAZA_CZ + dzf * k;
      }
      ax = clamp(ax, PLAZA_CX - PLAZA_HX, PLAZA_CX + PLAZA_HX);
      az = clamp(az, PLAZA_CZ - PLAZA_HZ, PLAZA_CZ + PLAZA_HZ);
      // A base that lands inside a stall disk is pushed out along its row's outward vector,
      // which is the back of the counter and still on the square's paving (the row backs stand
      // 6.9 m from the fountain and the paving reaches 8.5 m). Two passes, so a push can never
      // leave the walker inside a neighbouring disk, and the walker keeps its slot.
      let pushed = 0;
      const lim = stallR + PED_CLEAR;
      for (let pass = 0; pass < 2; pass++) {
        for (let k = 0; k < STALL_N; k++) {
          const ddx = ax - stallX[k];
          const ddz = az - stallZ[k];
          if (ddx * ddx + ddz * ddz < lim * lim) {
            az = stallZ[k] + stallOut[k] * lim;
            pushed = 1;
          }
        }
      }
      let dx = 0;
      let dz = 0;
      if (pushed) {
        // Behind a row the only free line is along it, so a walker there strolls the aisle
        // beside the counters instead of turning back through them.
        dx = hash01(i * 211 + 13) > 0.5 ? 1 : -1;
      } else {
        const wa = hash01(i * 211 + 13) * TWO_PI;
        dx = Math.cos(wa);
        dz = Math.sin(wa);
      }
      let span = (i & 1) === 0 ? 0 : 1.2 + hash01(i * 53 + 19) * 3.4;
      // Bounded shortening: halving the stroll four times either finds a clear stretch or
      // leaves the walker standing, so no frame of the walk can enter a counter.
      for (let t = 0; t < 4; t++) {
        if (stallClear(ax, az, dx, dz, span)) break;
        span *= 0.5;
      }
      if (!stallClear(ax, az, dx, dz, span)) span = 0;
      pedAX[i] = ax;
      pedAZ[i] = az;
      pedDX[i] = dx;
      pedDZ[i] = dz;
      pedSpan[i] = span;
      pedS[i] = span * 0.5;
      pedDir[i] = (hash01(i * 13 + 1) > 0.5) ? 1 : -1;
      pedSpeed[i] = 0.4 + hash01(i * 29 + 23) * 0.42;
    } else {
      const line = crossLines[i % crossCount];
      pedRoute[i] = line;
      const kerb = hash01(i * 43 + 11) > 0.5 ? line.len : 0;
      pedS[i] = kerb;
      pedDir[i] = kerb > 0 ? -1 : 1;
      pedLane[i] = (hash01(i * 61 + 27) * 2 - 1) * (line.crossHalf || 2.1) * 0.72;
      pedSpeed[i] = CROSS_SPEED;
    }
    pedPhase[i] = hash01(i * 13 + 1) * TWO_PI;
    pedOn[i] = 1;
    return true;
  }

  // Crowd count follows the activity of the last hour, exactly as before: 60 quiet walkers
  // scale to the pool at the busiest hour. The population scale clamps the ceiling.
  function stepPeds(dt, act, dreaming) {
    if (dreaming) {
      if (pedShown > 0.001 || cohortDrawn > 0) hideAllPeds();
      return;
    }
    let target = PED_BASE;
    if (act && qN > 0) {
      let sum = 0;
      for (let i = 0; i < qN && i < act.length; i++) sum += clamp01(act[i]);
      let pressure = sum / PED_REF_SUM;
      if (pressure > 1) pressure = 1;
      target = PED_BASE + Math.round((PED_CAP - PED_BASE) * pressure);
    }
    const scaleCap = Math.round(PED_CAP * popScale);
    if (target > scaleCap) target = scaleCap;
    if (target > PED_CAP) target = PED_CAP;
    if (target < 0) target = 0;
    pedShown = rampSlots(RESYNC_PED, pedShown, target, dt);
    if (pedShown < 0.02 && target === 0) pedShown = 0;
    const hi = pedShown < 0.02 ? 0 : Math.min(PED_CAP, Math.ceil(pedShown));
    const goCross = mod(clock, CROSS_PERIOD) < CROSS_GO;
    selectCohort(dt);
    if (cohortOn) cohortBegin();
    let any = false;
    let drawn = 0;
    // The procedural pool submits a prefix, so the walkers that stayed procedural are packed
    // into it from index zero upward and a cohort walker leaves no instance behind. With no
    // cohort (the phone, a build without the pack) the index is the slot, which is the cycle 22
    // mapping exactly, gaps included, so those frames do not move by one instance or one byte.
    let proc = 0;
    let colLo = PED_CAP;
    let colHi = 0;
    for (let i = 0; i < hi; i++) {
      const vis = visibility(pedShown, i);
      if (vis <= 0.02) {
        if (pedOn[i]) {
          pedOn[i] = 0;
          pedGrow[i] = 0;
          pedLit[i] = 0;
          hideAt(peds, i);
          hideAt(pedHeads, i);
          any = true;
        }
        continue;
      }
      if (!pedOn[i]) {
        if (!spawnPed(i)) {
          pedLit[i] = 0;
          continue;
        }
        any = true;
      }
      const mode = pedMode[i];
      let px = 0;
      let py = 0;
      let pz = 0;
      let ang = 0;
      if (mode === PED_ROUTE) {
        let route = pedRoute[i];
        if (!route) {
          if (!spawnPed(i)) continue;
          route = pedRoute[i];
          if (!route) continue;
        }
        const dir = pedDir[i];
        const lo = route.pavA !== undefined ? route.pavA : 0;
        const hi = route.pavB !== undefined ? route.pavB : route.len;
        let s = pedS[i] + dir * pedSpeed[i] * dt;
        if (s > hi || s < lo) {
          const nr = pickPedRoute();
          if (!nr) {
            pedOn[i] = 0;
            pedLit[i] = 0;
            hideAt(peds, i);
            hideAt(pedHeads, i);
            any = true;
            continue;
          }
          pedRoute[i] = nr;
          pedDir[i] = s > hi ? -1 : 1;
          pedGrow[i] = 0;
          const nlo = nr.pavA !== undefined ? nr.pavA : 0;
          const nhi = nr.pavB !== undefined ? nr.pavB : nr.len;
          s = pedDir[i] < 0 ? nhi : nlo;
          route = nr;
        }
        pedS[i] = s;
        tableAt(route, s, sPos, sTan);
        const off = pedLane[i];
        px = sPos.x - sTan.z * off;
        pz = sPos.z + sTan.x * off;
        py = sPos.y;
        ang = Math.atan2(sTan.x, sTan.z);
        if (pedDir[i] < 0) ang += Math.PI;
      } else if (mode === PED_SQUARE) {
        const span = pedSpan[i];
        if (span > 0.01) {
          let s = pedS[i] + pedDir[i] * pedSpeed[i] * dt;
          if (s > span) {
            s = span;
            pedDir[i] = -1;
          } else if (s < 0) {
            s = 0;
            pedDir[i] = 1;
          }
          pedS[i] = s;
        }
        px = pedAX[i] + pedDX[i] * pedS[i];
        pz = pedAZ[i] + pedDZ[i] * pedS[i];
        py = groundY(px, pz) + PLAZA_LIFT;
        ang = Math.atan2(pedDX[i] * pedDir[i], pedDZ[i] * pedDir[i]);
      } else {
        const route = pedRoute[i];
        if (!route) {
          pedOn[i] = 0;
          pedLit[i] = 0;
          hideAt(peds, i);
          hideAt(pedHeads, i);
          any = true;
          continue;
        }
        // Wait at the kerb, then cross in a burst: nobody strolls down the carriageway.
        let s = pedS[i];
        const clearing = s > 0.4 && s < route.len - 0.4;
        if (goCross || clearing) {
          s += pedDir[i] * pedSpeed[i] * dt;
          if (s > route.len) {
            s = route.len;
            pedDir[i] = -1;
          } else if (s < 0) {
            s = 0;
            pedDir[i] = 1;
          }
          pedS[i] = s;
        }
        tableAt(route, s, sPos, sTan);
        const off = pedLane[i];
        px = sPos.x - sTan.z * off;
        pz = sPos.z + sTan.x * off;
        py = sPos.y;
        ang = Math.atan2(sTan.x, sTan.z);
        if (pedDir[i] < 0) ang += Math.PI;
      }
      let g = pedGrow[i] + dt * 1.6;
      if (g > 1) g = 1;
      pedGrow[i] = g;
      const sc = g * vis;
      const bob = Math.sin(pedPhase[i] + clock * 5.5) * 0.05;
      // The last drawn place, for the next selection tick: ranking uses a place the frame has
      // already shown, so a tick never has to step a walk twice.
      pedLastX[i] = px;
      pedLastY[i] = py;
      pedLastZ[i] = pz;
      pedLit[i] = 1;
      if (cohortOn && cohortIn[i]) {
        writeCohort(i, px, py + bob, pz, ang, sc);
      } else {
        const idx = cohortOn ? proc : i;
        if (cohortOn) proc += 1;
        writeMatrix(peds, idx, px, py + bob, pz, ang, sc, sc * (1 + bob * 0.3), sc);
        writeMatrix(pedHeads, idx, px, py + bob + PED_HEAD_Y * sc, pz, ang, sc, sc, sc);
        // Clothes follow the walker, not the instance: the palette is written again only where
        // the repacking put a different slot at this index, so a steady frame uploads nothing.
        if (pedColSlot[idx] !== i) {
          pedColSlot[idx] = i;
          peds.setColorAt(idx, pedPalette[i % pedPalette.length]);
          pedHeads.setColorAt(idx, pedHeadPalette[(i * 3 + 1) % pedHeadPalette.length]);
          if (idx < colLo) colLo = idx;
          if (idx + 1 > colHi) colHi = idx + 1;
        }
      }
      drawn += 1;
      any = true;
    }
    for (let j = hi; j < pedHi; j++) {
      pedOn[j] = 0;
      pedGrow[j] = 0;
      pedLit[j] = 0;
      hideAt(peds, j);
      hideAt(pedHeads, j);
      any = true;
    }
    // The uploaded range covers both the packed prefix and any slot hides above it, exactly the
    // range the unpacked pool uploaded, so the phone's spans do not change.
    const top = cohortOn ? proc : hi;
    let span = top > pedHi ? top : pedHi;
    if (pedDrawn > span) span = pedDrawn;
    pedHi = hi;
    pedDrawn = top;
    pedVisible = drawn;
    if (any) {
      dirtyMatrix(peds, 0, span);
      dirtyMatrix(pedHeads, 0, span);
    }
    if (cohortOn) {
      if (colHi > colLo) {
        dirtyColour(peds, colLo, colHi);
        dirtyColour(pedHeads, colLo, colHi);
      }
      cohortEnd();
    }
  }

  function hideAllPeds() {
    for (let i = 0; i < PED_CAP; i++) {
      if (pedLit[i]) pedLit[i] = 0;
      if (!pedOn[i]) continue;
      pedOn[i] = 0;
      pedGrow[i] = 0;
      hideAt(peds, i);
      hideAt(pedHeads, i);
    }
    pedShown = 0;
    pedDrawn = 0;
    pedHi = 0;
    pedVisible = 0;
    dirtyMatrix(peds, 0, PED_CAP);
    dirtyMatrix(pedHeads, 0, PED_CAP);
    // The dream hides the walkers by submitting none of them; the cohort's bodies have to leave
    // the frame the same way, or the pack figures would stand lit in a sleeping city.
    for (let v = 0; v < cohortN; v += 1) {
      const mesh = cohortMesh[v];
      mesh.count = 0;
      mesh.visible = false;
      cohortCursor[v] = 0;
    }
    if (cohortN > 0) {
      for (let i = 0; i < PED_CAP; i += 1) cohortIn[i] = 0;
      cohortMembers = 0;
      cohortDrawn = 0;
      cohortClock = 0;
      cohortReport.drawn = 0;
      cohortReport.tris = 0;
    }
  }

  // ------------------------------------------------------------- cloud steps
  // Called on a tint change, never per frame. Three layers of the same decision: the cover
  // weight warms and greys the whole deck with fragmentation, the sun side warms each plate and
  // the far side cools it, and a jitter breaks the deck into separate plates. Every channel is
  // clamped at CLOUD_TINT_MAX, under the bloom threshold, so no cloud state can bloom. The sun
  // side is read from the light itself rather than kept as a second copy of the sun direction,
  // so the two cannot drift apart, and the read costs no allocation.
  //
  // At night the deck is a value relationship, not a colour: a plate takes the deep stop, is
  // lifted toward the city stop by how close it sits to the lit bowl under it, is lifted again
  // toward the moon stop on the moon side, and is then scaled down by its distance from the
  // middle of the field, which is where the frame edge samples the deck. The three stops are
  // blue with the blue channel 2.9 to 3.2 times the red, so no blend of them can reach grey,
  // and the brightest plate is 0.195 in linear light against a 0.95 night bloom threshold.
  // Dreaming keeps the same ramp at CLOUD_NIGHT_DREAM of its value.
  function refreshCloudColors(dreaming, night, frag) {
    const nightDeck = night || dreaming;
    if (!nightDeck) {
      cloudTint.copy(cloudTintWarm);
      cloudTint.lerp(cloudTintCool, frag * 0.92);
    }
    let sunX = 1;
    let sunZ = 0;
    if (world && world.lights && world.lights.sun && world.lights.sun.position) {
      const px = world.lights.sun.position.x;
      const pz = world.lights.sun.position.z;
      const len = Math.sqrt(px * px + pz * pz);
      if (len > 1e-3) {
        sunX = px / len;
        sunZ = pz / len;
      }
    }
    const invX = 1 / (cloudHalfX > 1 ? cloudHalfX : 1);
    const invZ = 1 / (cloudHalfZ > 1 ? cloudHalfZ : 1);
    for (let i = 0; i < CLOUD_CAP; i++) {
      const nx = (cloudX[i] - cloudCX) * invX;
      const nz = (cloudZ[i] - cloudCZ) * invZ;
      if (nightDeck) {
        let u = Math.sqrt(nx * nx + nz * nz);
        if (u > 1) u = 1;
        // The lit bowl under the deck: full city value out to a quarter of the field, gone at
        // its edge, so a plate over the town reads brighter than one over the dark hills.
        let cityK = (1 - u) * 1.3333334;
        if (cityK > 1) cityK = 1;
        cityK = cityK * cityK * (3 - 2 * cityK);
        flagColor.copy(cloudNightDeep).lerp(cloudNightCity, cityK);
        // The moon side: -0.5 on the far corner to 0.5 on the moon corner, as in daylight.
        const w = (nx * sunX + nz * sunZ) * 0.5;
        if (w > 0) flagColor.lerp(cloudNightMoon, w * 0.55);
        let dim = 1 - 0.45 * u * u;
        if (dim < 0.5) dim = 0.5;
        if (dreaming) dim *= CLOUD_NIGHT_DREAM;
        flagColor.multiplyScalar(dim);
      } else {
        flagColor.copy(cloudTint);
        // -0.71 on the far corner to 0.71 on the sun corner, a gradient across the deck.
        const w = (nx * sunX + nz * sunZ) * 0.5;
        if (w > 0) flagColor.lerp(cloudTintWarm, w);
        else if (w < 0) flagColor.lerp(cloudTintCool, -w);
      }
      const j = 1 + (hash01(i * 61 + 5) - 0.5) * 0.12;
      flagColor.r = clamp(flagColor.r * j, 0, CLOUD_TINT_MAX);
      flagColor.g = clamp(flagColor.g * j, 0, CLOUD_TINT_MAX);
      flagColor.b = clamp(flagColor.b * j, 0, CLOUD_TINT_MAX);
      clouds.setColorAt(i, flagColor);
    }
    dirtyAllColour(clouds);
    cloudTintPrev = frag;
    cloudNightClock = 0;
  }

  function stepClouds(dt, frag, dreaming, night) {
    const livePrev = cloudLive;
    let touched = 0;
    // A steady hour keeps the two narrow lanes (district wisps and the block tier's own lane),
    // so the floor is CLOUD_BLOCK_HI (eleven plates since cycle 30); cover adds the rest of the
    // pool at 126 per unit of fragmentation, which still caps at CLOUD_CAP on a broken hour.
    let cloudFadeTarget = Math.round(CLOUD_BLOCK_HI + 126 * frag);
    if (cloudFadeTarget > CLOUD_CAP) cloudFadeTarget = CLOUD_CAP;
    if (cloudLive < cloudFadeTarget) {
      let add = 4;
      while (add > 0 && cloudLive < cloudFadeTarget) {
        const i = cloudLive;
        const lo = cloudMinX[i];
        const hi = cloudMaxX[i];
        cloudOn[i] = 1;
        // A full field plate enters from the low x edge, so a growing deck streams in from one
        // side instead of popping into the middle of the frame. Both narrow lanes already sit
        // where the frame wants them, so their plates may seed anywhere in the lane.
        cloudX[i] = lo + hash01(cloudLive * 137 + 5) * (hi - lo) * (i < CLOUD_LANE_HI ? 1 : 0.175);
        cloudLive++;
        add--;
      }
    } else if (cloudLive > cloudFadeTarget) {
      let rem = 4;
      while (rem > 0 && cloudLive > cloudFadeTarget) {
        cloudLive--;
        cloudOn[cloudLive] = 0;
        hideAt(clouds, cloudLive);
        touched = 1;
        rem--;
      }
    }
    if (!dreaming) {
      const drift = 1.2 + 6.4 * frag;
      for (let i = 0; i < cloudLive; i++) {
        if (!cloudOn[i]) continue;
        const lo = cloudMinX[i];
        const hi = cloudMaxX[i];
        let x = cloudX[i] + drift * cloudRate[i] * dt;
        const span = hi - lo;
        while (x > hi) x -= span;
        cloudX[i] = x;
        writeMatrix(clouds, i, x, cloudY[i], cloudZ[i], cloudRot[i], cloudSX[i], cloudSY[i], cloudSZ[i]);
      }
      if (cloudLive > 0) touched = 1;
    }
    if (touched) dirtyMatrix(clouds, 0, cloudLive > livePrev ? cloudLive : livePrev);
    // The night ramp follows position (the lit bowl and the moon side), and the deck drifts, so
    // at a still hour and a still data clock the ramp would otherwise stay frozen on a moving
    // plate. A low cadence keeps it true; it is 140 colour writes and one small upload, and the
    // day deck keeps its old guard exactly.
    cloudNightClock += dt;
    if (cloudTintPrev < 0 || cloudDreamPrev !== dreaming || cloudNightPrev !== night ||
      Math.abs(frag - cloudTintPrev) > 0.03 ||
      (night && !dreaming && cloudNightClock >= CLOUD_NIGHT_REFRESH_S)) {
      cloudDreamPrev = dreaming;
      cloudNightPrev = night;
      refreshCloudColors(dreaming, night, frag);
    }
  }

  // ----------------------------------------------------------- window lights
  function stepWindows(night, dreaming, act) {
    if (!windowRank) return;
    let changed = false;
    for (let q = 0; q < qN; q++) {
      let v = act && q < act.length ? clamp01(act[q]) : 0;
      const base = night ? 0.25 : 0;
      let need = v > base ? v : base;
      if (dreaming) need = 0.1;
      needArr[q] = need;
      const prev = needPrev[q];
      if (prev < 0 || Math.abs(need - prev) > 0.012) {
        needPrev[q] = need;
        changed = true;
      }
    }
    if (!changed && !needDirty) return;
    if (changed) needDirty = true;
    if (!needDirty) return;
    const wc = windowRank.length;
    const batch = Math.max(1, (wc / 10) | 0);
    const wqMap = world.windowQuarter;
    for (let k = 0; k < batch; k++) {
      const i = winCursor;
      winCursor++;
      if (winCursor >= wc) winCursor = 0;
      winSwept++;
      const q = wqMap[i];
      let need = 0;
      if (q >= 0 && q < qCap) need = needArr[q];
      const lit = windowRank[i] < need ? 1 : 0;
      if (windowState[i] !== lit) {
        windowState[i] = lit;
        world.windows.setWindowLit(i, lit === 1);
      }
    }
    if (winSwept >= wc) {
      winSwept = 0;
      needDirty = false;
    }
  }

  // ------------------------------------------------------------------ cranes
  function stepCranes(tData) {
    const list = safeCranes(tData);
    if (!list) return;
    for (let q = 0; q < qN; q++) craneCur[q] = 0;
    for (let i = 0; i < list.length; i++) {
      const c = list[i];
      if (!c) continue;
      const q = c.q | 0;
      if (q < 0 || q >= qCap) continue;
      const f = c.floors | 0;
      craneCur[q] = f < 0 ? 0 : (f > 12 ? 12 : f);
    }
    for (let q = 0; q < qN; q++) {
      if (craneCur[q] !== cranePrev[q]) {
        cranePrev[q] = craneCur[q];
        if (world.cranes && world.cranes.setFloors) world.cranes.setFloors(q, craneCur[q]);
      }
    }
  }

  // ------------------------------------------------ world time + neglect
  function stepWorldTime(wallHour, night) {
    let wall = typeof wallHour === 'number' && wallHour === wallHour ? wallHour : 12;
    wall = mod(wall, 24);
    const bucket = Math.floor(wall);
    const nightFlag = night ? 1 : 0;
    if (bucket === lastHour && nightFlag === lastNight) return;
    lastHour = bucket;
    lastNight = nightFlag;
    if (world.lights && world.lights.setTimeOfDay) world.lights.setTimeOfDay(wall, night);
    if (world.streetLights && world.streetLights.setOn) world.streetLights.setOn(night);
    // Tram windows light with the city: a warm band through every carriage.
    tramWinMat.emissiveIntensity = night ? 1.0 : 0.16;
  }

  // ---------------------------------------------------------------- signals
  // One phase per junction, on the sim clock. clock is the seconds this file already advances
  // for every time-driven mover, and the wall hour seeds where in the cycle the page opens, so
  // a reload does not restart every junction on green.
  //
  // The cycle is 50 s: avenue green 24, avenue amber 3, ring green 20, ring amber 3. A junction
  // holds one slot at a time and the two families read disjoint slots, so avenue green and ring
  // green can never be true together: slot 0 is the avenue's, slot 2 is the ring's, and slots 1
  // and 3 are the two ambers. Amber is not green, so a car entering on either amber holds; a car
  // already past the stop line at that moment is committed and clears. The write is guarded by
  // sigPrev: the state array and apply() are touched only on a frame where some head's code
  // actually changed, and this runs before the road steps, so the lamps drawn this frame and
  // the clamp the cars obeyed this frame are the same value.
  function stepSignals(wallHour) {
    if (sigN <= 0 && sigJunctionCount <= 0) return;
    if (!sigOriginSet) {
      sigOriginSet = true;
      if (typeof wallHour === 'number' && wallHour === wallHour) sigOrigin = wallHour * 3600;
    }
    const t = sigOrigin + clock;
    let changed = false;
    for (let j = 0; j < sigJunctionCount; j += 1) {
      const record = sigTable[j];
      let slot = 2;
      let aveLeft = 0;
      let ringLeft = 0;
      if (record && record.signalised) {
        const m = mod(t + sigOffset[j], SIGNAL_CYCLE_S);
        if (m < SIGNAL_AVE_GREEN_S) {
          slot = 0;
          aveLeft = SIGNAL_AVE_GREEN_S - m;
        } else if (m < SIGNAL_RING_START_S) {
          slot = 1;
        } else if (m < SIGNAL_AMBER_START_S) {
          slot = 2;
          ringLeft = SIGNAL_AMBER_START_S - m;
        } else {
          slot = 3;
        }
      }
      sigPhase[j] = slot;
      sigAveLeft[j] = aveLeft;
      sigRingLeft[j] = ringLeft;
      const diag = junctionDiag[j];
      if (diag) {
        diag.phase = slot;
        diag.avenueGreen = slot === 0;
        diag.ringGreen = slot === 2;
        diag.avenueCode = slot === 0 ? 0 : slot === 1 ? 1 : 2;
        diag.ringCode = slot === 2 ? 0 : slot === 3 ? 1 : 2;
        diag.avenueCrossed30 = crossAveTotal[j];
        diag.ringCrossed30 = crossRingTotal[j];
        for (let k = 0; k < diag.arms.length; k += 1) {
          diag.arms[k].avenueCrossed30 = crossAveTotal[j];
          diag.arms[k].ringCrossed30 = crossRingTotal[j];
        }
      }
    }
    for (let i = 0; i < sigN; i += 1) {
      const j = sigHeadJunction ? sigHeadJunction[i] : 0;
      const family = sigHeadFamily ? sigHeadFamily[i] : SIG_FAMILY_AVENUE;
      const slot = j >= 0 && j < sigJunctionCount ? sigPhase[j] : 2;
      const code = family === SIG_FAMILY_AVENUE
        ? (slot === 0 ? 0 : slot === 1 ? 1 : 2)
        : (slot === 2 ? 0 : slot === 3 ? 1 : 2);
      if (sigPrev[i] !== code) {
        sigPrev[i] = code;
        sigState[i] = code;
        changed = true;
      }
    }
    if (changed && sigApply) sigApply();
  }

  function stepNeglect() {
    if (!modelRef || typeof world.setNeglected !== 'function') return;
    const mq = modelRef.quarters;
    if (!mq) return;
    for (let q = 0; q < qN; q++) {
      const flag = mq[q] && mq[q].neglected ? 1 : 0;
      if (neglectedState[q] === flag) continue;
      neglectedState[q] = flag;
      world.setNeglected(q, flag === 1);
    }
  }

  // ------------------------------------------------------------------- signs
  function applySigns() {
    if (!world.signs || !world.signs.setText || slotN <= 0) return;
    for (let s = 0; s < slotN; s++) {
      let q = -1;
      if (slotQuarter && s < slotQuarter.length) q = slotQuarter[s];
      if (q < 0 || q >= qN) continue;
      const mq = modelRef && modelRef.quarters ? modelRef.quarters[q] : null;
      const phrases = mq && mq.phrases ? mq.phrases : null;
      if (!phrases || phrases.length === 0) continue;
      const c = signCursor[s];
      const text = phrases[c % phrases.length];
      signCursor[s] = (c + 1) % 4096;
      if (typeof text === 'string' && text.length > 0) world.signs.setText(s, text);
    }
  }

  function stepSigns(dt) {
    if (signsDirty) {
      signsDirty = false;
      applySigns();
    }
    signTimer += dt;
    if (signTimer >= SIGN_PERIOD) {
      signTimer -= SIGN_PERIOD;
      if (signTimer > SIGN_PERIOD) signTimer = 0;
      applySigns();
    }
  }

  // ------------------------------------------------------------ parcel clock
  function resyncParcels(t) {
    parcelCursor = 0;
    if (!parcelOrder || !modelRef || !modelRef.parcels) return;
    const parcels = modelRef.parcels;
    const n = parcelOrder.length;
    let i = 0;
    while (i < n) {
      const p = parcels[parcelOrder[i]];
      const at = p && typeof p.at === 'number' ? p.at : 0;
      if (at > t) break;
      i++;
    }
    parcelCursor = i;
  }

  function advanceParcels(t, dreaming) {
    if (!parcelOrder || !modelRef || !modelRef.parcels) return;
    const parcels = modelRef.parcels;
    const n = parcelOrder.length;
    while (parcelCursor < n) {
      const p = parcels[parcelOrder[parcelCursor]];
      const at = p && typeof p.at === 'number' ? p.at : 0;
      if (at > t) break;
      parcelCursor++;
      if (dreaming || !p) continue;
      const from = typeof p.from === 'number' ? p.from | 0 : -1;
      const to = typeof p.to === 'number' ? p.to | 0 : -1;
      if (from < 0 || to < 0 || from === to) continue;
      api.vans.dispatch(from, to, 'parcel');
    }
  }

  // ------------------------------------------------------------- data guards
  function safeSwitch(t) {
    if (!modelRef || typeof modelRef.switchRateAt !== 'function') return 0;
    const v = modelRef.switchRateAt(t);
    return typeof v === 'number' && v === v ? clamp01(v) : 0;
  }
  function safeFrag(t) {
    if (!modelRef || typeof modelRef.fragmentationAt !== 'function') return 0;
    const v = modelRef.fragmentationAt(t);
    return typeof v === 'number' && v === v ? clamp01(v) : 0;
  }
  function safeActivity(t) {
    if (!modelRef || typeof modelRef.activityLastHour !== 'function') return null;
    const v = modelRef.activityLastHour(t);
    return v && typeof v.length === 'number' ? v : null;
  }
  function safeCranes(t) {
    if (!modelRef || typeof modelRef.cranesAt !== 'function') return null;
    const v = modelRef.cranesAt(t);
    return v && typeof v.length === 'number' ? v : null;
  }

  // ------------------------------------------------------------------ update
  // One shared empty sim: update(dt) without a sim argument must not allocate a literal.
  const NO_SIM = {};

  function update(dt, sim) {
    const s = sim || NO_SIM;
    let d = typeof dt === 'number' && dt > 0 ? dt : (typeof s.dt === 'number' ? s.dt : 0);
    if (!(d > 0)) d = 0;
    if (d > 0.1) d = 0.1;
    clock += d;
    rollCrossClock();
    const tData = typeof s.tData === 'number' ? s.tData : 0;
    const night = !!s.night;
    const dreaming = !!s.dreaming;

    // Population scale from the quality ladder, eased over about a second.
    popScale = easeTo(popScale, popTarget, 3.1, d);
    if (Math.abs(popTarget - popScale) < 0.002) popScale = popTarget;

    // Dream edges: peds vanish on sleep and ramp back in after waking, and the parked
    // fleet stops being drawn until the city wakes. Waking opens the re-population window, so
    // the walkers and the cars that the dream emptied come back over RESYNC_FRAMES frames
    // instead of at whatever rate one frame's dt happens to allow.
    if (dreaming !== dreamPrev) {
      dreamPrev = dreaming;
      if (dreaming) {
        // The population the sleep has to bring back, read before the walkers are hidden.
        captureResyncSpans();
        hideAllPeds();
      } else {
        beginResync('wake');
      }
      applyDreamFleet(dreaming);
    }

    const congestion = safeSwitch(tData);
    const frag = safeFrag(tData);
    const act = safeActivity(tData);
    // Every write this frame re-marks its span from scratch: the model meshes and the packed
    // rider pool upload only what this frame touched.
    beginFleetFrame();
    // Night lights first: the gate must be settled before any road step writes a lamp.
    stepVehicleLights(night, dreaming);
    tailMark = tailWrites;
    // The lamps step before any road step runs: the phase a car obeys this frame is the one
    // the lamps show this frame, so a viewer cannot see a red lamp and a car crossing it. The
    // junction clock keeps running while the dream idles the traffic, so waking finds a live
    // cycle instead of a frozen one.
    stepSignals(typeof s.wallHour === 'number' ? s.wallHour : 12);

    // Green wave comes from the session open at tData, when it belongs to a real quarter
    // (data.mjs reports q = -1 for unmatched apps). A session that has already closed earns
    // nothing: the avenue flows while the focus is live, not for the rest of the day.
    const active = modelRef && typeof modelRef.activeSessionAt === 'function' ? modelRef.activeSessionAt(tData) : null;
    const gw = !!active && active.q >= 0;
    if (gw !== greenWave) {
      greenWave = gw;
      refreshCarColors();
    }

    // Parcel schedule, driven by the data clock crossing each at value. A clock that went
    // backwards is a discontinuity (the wrap at t1 + 1 h, or an install that reset it to t0), and
    // it re-opens the window: the pools have been reading schedules the clock has left behind.
    if (!parcelInit) {
      parcelInit = true;
      resyncParcels(tData);
    } else if (tData < prevT) {
      resyncParcels(tData);
      beginResync('wrap');
    }
    advanceParcels(tData, dreaming);
    prevT = tData;

    if (!dreaming) {
      if (ringTable) stepRingCars(d, congestion);
      if (aveTable) stepAvenueCars(d, congestion);
      stepSpurCars(d, congestion);
      stepExpressway(d, congestion);
      stepBuses(d);
      stepTrains(d);
      stepTram(d);
      stepCranes(tData);
      stepSigns(d);
    }
    stepPeds(d, act, dreaming);
    // The window counts its frame here, after every pool has taken its slice, so the frame the
    // budget names is a frame that really carried work: frame RESYNC_FRAMES admits the last
    // slots, and only then can the window close. A sleep does not count: nothing steps under it,
    // so an open window pauses and the next wake opens a fresh one.
    if (!dreaming) endResyncFrame();
    stepVans(d, dreaming);
    stepFarm(d, dreaming);
    stepClouds(d, frag, dreaming, night);
    stepWindows(night, dreaming, act);
    stepWorldTime(typeof s.wallHour === 'number' ? s.wallHour : 12, night);
    stepNeglect();

    // A pool submits its live prefix, not its cap. Every pool in this file allocates its cap
    // once and writes its instances from index zero upward, so the live set is always a prefix
    // of the buffer and count is exactly that prefix. The change is invisible: an idle slot
    // already holds the zero matrix, so the hidden tail was drawing nothing but its own
    // triangles, and the three instance fields below (matrix, colour, count) stay untouched by
    // the dirty-span upload discipline, which still describes the writes it always did.
    // The car mesh is shared by four road families that each own a base index, so its prefix
    // reaches the end of the last live family; the riders share that range and the lamp pool
    // adds its own fixed group under it.
    const carLive = Math.max(ringBase + ringDrawn, aveBase + aveDrawn,
      spurBase + spurDrawn, expBase + expDrawn);
    cars.count = carLive;
    // The riders are a packed pool of their own now and the model meshes are sized at build,
    // so flushFleet sets the rider count and hands three.js the spans both pools wrote here.
    flushFleet();
    brakeLights.count = expDrawn;
    peds.count = pedDrawn;
    pedHeads.count = pedDrawn;
    clouds.count = cloudLive;
    railCars.count = railLive;
    nightLights.count = LIGHT_CAR + carLive * CAR_SLOTS;

    counts.vehicles = Math.round(ringShown + aveShown + spurShown + expShown) +
      vanLive + farmN + Math.round(busShown) + railLive;
    counts.pedestrians = dreaming ? 0 : pedVisible;
    flushUploads();
    return counts;
  }

  // ---------------------------------------------------------------- population
  // Called by main's quality controller with 0.25 .. 1. The pools never reallocate: every
  // step clamps its own live count and the fade band hides or shows instances over about a
  // second, so a notch change is a ramp and not a pop.
  function setPopulationScale(v) {
    let t = typeof v === 'number' && v === v ? v : 1;
    if (t < 0.25) t = 0.25;
    if (t > 1) t = 1;
    popTarget = t;
  }

  // ------------------------------------------------------------ initial fill
  for (let i = 0; i < ringCap; i++) {
    // Kept inboard of the parked cars: a moving car at 1.2 clears a kerb car at 3.42. The
    // offset is one of two lane centres, not a random value in the band: a continuous offset
    // lets two cars whose lanes differ only in sign sit a tenth of a metre apart laterally,
    // and no longitudinal gap law can keep those bodies apart.
    ringLane[i] = hash01(i * 7 + 3) < 0.5 ? -RING_LANE_OFF : RING_LANE_OFF;
    ringS[i] = hash01(i * 131 + 7) * ringLen;
    ringV[i] = RING_SPEED;
    // The side of the ring is the lane: both run the same direction, one on each offset.
    ringGrid.laneOf[i] = ringLane[i] > 0 ? 1 : 0;
  }
  for (let i = 0; i < aveCap; i++) {
    const side = (i & 1) ? 1 : -1;
    aveLane[i] = side * AVE_CAR_LANE;
    aveDir[i] = side > 0 ? -1 : 1;
    aveS[i] = hash01(i * 71 + 13) * aveLen;
    // A carriageway carries one direction, so its side is the lane.
    aveGrid.laneOf[i] = side > 0 ? 1 : 0;
  }
  for (let i = 0; i < spurCap; i++) {
    const side = (i & 1) ? 1 : -1;
    spurTab[i] = spurTables.length ? i % spurTables.length : 0;
    spurLane[i] = side * spurLaneOff[spurTab[i]];
    spurDir[i] = side > 0 ? 1 : -1;
    spurS[i] = hash01(i * 83 + 11) * (spurTables.length ? spurTables[0].len : 1);
    // Two lanes per arterial, numbered by table so a car only ever follows its own road.
    spurGrid.laneOf[i] = spurTab[i] * 2 + (side > 0 ? 1 : 0);
  }
  for (let i = 0; i < expCap; i++) expLane[i] = i % EXP_LANES;
  for (let l = 0; l < EXP_LANES; l++) {
    expLaneOff[l] = (l - (EXP_LANES - 1) * 0.5) * Math.min(3.05, (EXP_HALF * 2 - 1.4) / EXP_LANES + 0.6);
    expLaneDir[l] = expLaneOff[l] < 0 ? -1 : 1;
    // One lane per quarter of the deck, so a handful of cars covers the whole length at an
    // even interval instead of bunching on four random phases.
    expLaneBase[l] = (l / EXP_LANES) * expLen;
  }
  for (let i = 0; i < VAN_DELIVERY; i++) {
    vanRoute[i] = null;
    hideAt(vanMesh, i);
    hideAt(payloads, i);
  }
  // The farm slots stay untouched until initFarm paints and places them after rebuild.
  for (let i = FARM_BASE; i < VAN_CAP; i++) hideAt(vanMesh, i);
  dirtyMatrix(vanMesh, 0, VAN_CAP);
  dirtyMatrix(payloads, 0, VAN_DELIVERY);
  for (let i = 0; i < BUS_COUNT; i++) {
    hideAt(busBody, i);
    hideAt(busBand, i);
    busBand.setColorAt(i, busBandPalette[i % busBandPalette.length]);
  }
  dirtyAllColour(busBand);
  dirtyMatrix(busBody, 0, BUS_COUNT);
  dirtyMatrix(busBand, 0, BUS_COUNT);
  for (let i = 0; i < PED_CAP; i++) {
    pedRoute[i] = null;
    pedSpeed[i] = PED_MIN_SPEED + hash01(i * 31 + 5) * (PED_MAX_SPEED - PED_MIN_SPEED);
    pedLane[i] = (hash01(i * 17 + 9) * 2 - 1) * 0.82;
    pedPhase[i] = hash01(i * 13 + 1) * TWO_PI;
    hideAt(peds, i);
    hideAt(pedHeads, i);
    peds.setColorAt(i, pedPalette[i % pedPalette.length]);
    pedHeads.setColorAt(i, pedHeadPalette[(i * 3 + 1) % pedHeadPalette.length]);
    // The instance starts out carrying its own slot's palette, so the repacking below never
    // rewrites a colour it did not have to.
    pedColSlot[i] = i;
  }
  dirtyAllColour(peds);
  dirtyAllColour(pedHeads);
  dirtyMatrix(peds, 0, PED_CAP);
  dirtyMatrix(pedHeads, 0, PED_CAP);
  for (let i = 0; i < CLOUD_CAP; i++) {
    // The first CLOUD_WISP_N plates are the district deck a clear hour keeps: the same plates,
    // at the same indices, as before the block lane existed. The next CLOUD_BLOCK_N are the
    // block tier's own lane, then the CLOUD_WISP_SPARE plates that join the district lane as
    // cover thickens, and the rest of the pool owns the full field and the whole field's
    // drift. A steady hour keeps both narrow lanes whole (11 + 126 * frag reaches 11 at frag 0).
    const blockPlate = i >= CLOUD_WISP_N && i < CLOUD_BLOCK_HI;
    const wispPlate = i < CLOUD_WISP_N || (i >= CLOUD_BLOCK_HI && i < CLOUD_LANE_HI);
    if (wispPlate) {
      cloudMinX[i] = cloudCX - CLOUD_WISP_LANE;
      cloudMaxX[i] = cloudCX + CLOUD_WISP_LANE;
      cloudZ[i] = cloudCZ + cloudHalfZ * (0.16 + 0.42 * hash01(i * 29 + 4));
    } else if (blockPlate) {
      // The block lane: the same narrow drift, on the depth the projection solved, with a few
      // metres of depth spread so the plates do not read as one slab.
      cloudMinX[i] = cloudCX - CLOUD_BLOCK_LANE;
      cloudMaxX[i] = cloudCX + CLOUD_BLOCK_LANE;
      cloudZ[i] = cloudBlockZ + (hash01(i * 29 + 4) * 2 - 1) * CLOUD_BLOCK_SPREAD;
    } else {
      cloudMinX[i] = cloudCX - cloudHalfX;
      cloudMaxX[i] = cloudCX + cloudHalfX;
      cloudZ[i] = cloudCZ + (hash01(i * 29 + 4) * 2 - 1) * cloudHalfZ;
    }
    cloudX[i] = cloudMinX[i] + hash01(i * 23 + 2) * (cloudMaxX[i] - cloudMinX[i]);
    // The thin band of high wisps, CLOUD_BAND_Y deep, so the deck still has parallax.
    cloudY[i] = blockPlate
      ? cloudBlockY0 + hash01(i * 37 + 6) * CLOUD_BLOCK_BAND
      : cloudBaseY + hash01(i * 37 + 6) * CLOUD_BAND_Y;
    // 35 to 90 metres across, 0.55 to 0.8 of that deep: a plate, not a puff, and large enough
    // that a soft edge reads as cloud rather than as a floating prop. The block lane's plates
    // are a little smaller and flatter, so their corners stay inside the top tenth.
    const sc = blockPlate ? 35 + hash01(i * 41 + 8) * 35 : 35 + hash01(i * 41 + 8) * 55;
    cloudSX[i] = sc;
    cloudSY[i] = 1;
    cloudSZ[i] = sc * (blockPlate ? 0.45 + hash01(i * 47 + 12) * 0.15
      : 0.55 + hash01(i * 47 + 12) * 0.25);
    cloudRot[i] = (hash01(i * 59 + 3) * 2 - 1) * 0.4;
    cloudRate[i] = 0.55 + hash01(i * 53 + 14) * 0.9;
    hideAt(clouds, i);
  }
  dirtyMatrix(clouds, 0, CLOUD_CAP);
  if (!tramTable) {
    for (let c = 0; c < TRAM_SEGS; c++) {
      hideAt(tramBody, c);
      hideAt(tramStripe, c);
      hideAt(tramWins, c);
    }
  }
  dirtyMatrix(tramBody, 0, TRAM_SEGS);
  dirtyMatrix(tramStripe, 0, TRAM_SEGS);
  dirtyMatrix(tramWins, 0, TRAM_SEGS);

  initVehicleKinds();
  initModelFleet();
  initCohort();
  refreshCarColors();
  initVehicleLights();
  for (let i = 0; i < CAR_CAP; i++) {
    // hideVehicle also retires the pack instance of a slot that wears one and the rider of a
    // scooter slot, so the first frame after a build submits only hidden instances.
    hideVehicle(i);
  }
  for (let i = 0; i < (expCap > 0 ? expCap : 1); i++) hideAt(brakeLights, i);
  dirtyAllMatrix(cars);
  dirtyAllMatrix(riders);
  dirtyAllMatrix(brakeLights);
  for (let k = 0; k < modelList.length; k++) dirtyAllMatrix(modelList[k]);

  // ------------------------------------------------------- rail yard start
  if (railTable) {
    railSpanA0 = railSAtZ(-103);
    railSpanA1 = railSAtZ(30);
    railSpanB0 = railSAtZ(29);
    railSpanB1 = railSAtZ(99);
    railAS = railSpanA1 - 40;
    railADir = -1;
    railBS = railSpanB1 - 12;
    railBDir = -1;
    // Static freight wagons on the free track, clear of both moving trains.
    const wagonZ = [-60, -46, -32];
    for (let i = 0; i < WAGON_SET && i < RAIL_CAP; i++) {
      const s = railSAtZ(wagonZ[i]);
      railCarPos(s, WAGON_TRACK, sPos, sTan);
      writeBasis(railCars, i, sPos.x, sPos.y + 0.5, sPos.z, sTan, 0.94, 0.82, 0.94);
      railCars.setColorAt(i, railPalette[2]);
    }
    for (let k = 0; k < TRAIN_A_CARS; k++) railCars.setColorAt(WAGON_SET + k, railPalette[0]);
    for (let k = 0; k < TRAIN_B_CARS; k++) {
      railCars.setColorAt(WAGON_SET + TRAIN_A_CARS + k, railPalette[1]);
    }
    railLive = WAGON_SET + TRAIN_A_CARS + TRAIN_B_CARS;
    for (let i = railLive; i < RAIL_CAP; i++) hideAt(railCars, i);
    dirtyAllColour(railCars);
    dirtyAllMatrix(railCars);
  } else {
    for (let i = 0; i < RAIL_CAP; i++) hideAt(railCars, i);
    dirtyAllMatrix(railCars);
  }

  // ------------------------------------------------------------------- build
  buildCrossings();
  buildParking();
  windowInit();

  // --------------------------------------------------------------------- api
  const api = {
    group: group,
    update: update,
    setDataModel: setDataModelLive,
    setPopulationScale: setPopulationScale,
    vans: { dispatch: dispatchVan },
    tram: { positionAt: tramPositionAt },
    counts: counts,
    // The walking cohort's read-only report: the capacity the triangle budget paid for, the
    // variants it wears, and how many pack bodies the last frame actually submitted. The
    // harness reads it instead of guessing at the scene graph; `setCohort(false)` gives the
    // same page the cycle 22 pedestrian frame, so the cohort's own cost is measurable in place.
    cohort: cohortReport,
    setCohort: setCohortEnabled,
    // The wake re-population window, read-only: the frames it has run, the frames left of its
    // budget, what each pool has to bring back, the slots each pool may enter in one frame, and
    // the slots that actually entered on the last frame and since the window opened.
    reseed: reseedReport,
    // The junction deck reads itself back, as world.market does: the heights and windows of the
    // rule, the arcs it resolved, the cycle, and one live record per junction with the phase
    // each road is showing and how many cars crossed in the last thirty seconds. The harness
    // reads these instead of guessing from matrices, and a regression is a number.
    junctions: {
      gap: JUNCTION_GAP,
      reach: JUNCTION_REACH,
      scan: JUNCTION_SCAN_M,
      tail: JUNCTION_TAIL,
      past: JUNCTION_PAST,
      clear: JUNCTION_CLEAR,
      margin: JUNCTION_MARGIN,
      exit: JUNCTION_EXIT,
      green: SIGNAL_AVE_GREEN_S,
      ringGreen: SIGNAL_RING_GREEN_S,
      amber: SIGNAL_AMBER_S,
      ringStart: SIGNAL_RING_START_S,
      amberStart: SIGNAL_AMBER_START_S,
      cycle: SIGNAL_CYCLE_S,
      aveS: [aveCrossS[0], aveCrossS[1]],
      aveRingS: [aveCrossRingS[0], aveCrossRingS[1]],
      spurRingS: Array.from(spurCrossRingS),
      list: junctionDiag,
      // The pool split, so a live probe can tell a ring car from an avenue car by the instance
      // index it reads, without re-deriving the partition.
      pools: {
        ringBase: 0,
        ringCap: ringCap,
        aveBase: aveBase,
        aveCap: aveCap,
        spurBase: spurBase,
        spurCap: spurCap,
        expBase: expBase,
        expCap: expCap
      }
    }
  };

  // The page-level read hook the measurement uses: the drawn instance count cannot be read off
  // a frame, and main's debug api does not carry the agents object. One object, mutated in
  // place, so reading it allocates nothing and writing it is impossible by convention.
  if (typeof window !== 'undefined' && window) window.localtimeCohort = cohortReport;
  // The same read hook for the wake window: the harness reads the frames it ran and the slot cap
  // from the page instead of inferring them from a frame time.
  if (typeof window !== 'undefined' && window) window.localtimeReseed = reseedReport;

  rebuild(model);
  initFarm();
  return api;
}
