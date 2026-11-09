/* Localtime camera rig.
   Fixed pitch, snap-only yaw, tiered zoom, clamped pan. One top-level identifier: createCameraRig.
   Every helper, constant and event handler lives inside this factory because the assembler
   concatenates all fragments into a single module scope.

   Notes for the integrator:
   - Q/E are bound here on the window (fallback: the canvas). Do not bind them again in main.
     rotate() additionally suppresses a duplicate call with the same direction inside 30 ms, so a
     second binding cannot turn the camera twice for one key press.
   - update(dt) accepts seconds. A numeric dt above 1 is read as milliseconds.
   - glideTo's onDone also runs when a user gesture cancels the glide, so the dream sequence that
     waits on it can still finish its own bookkeeping. */

export function createCameraRig(THREE, dom, bounds, opts) {
  const options = opts || {};
  const isTouch = !!options.isTouch;

  const PI = Math.PI;
  const TWO_PI = PI * 2;
  const HALF_PI = PI * 0.5;
  const DEG = PI / 180;

  const FOV_DEG = 45;
  const FOV_RAD = FOV_DEG * DEG;
  const HALF_FOV_TAN = Math.tan(FOV_RAD * 0.5);
  const PITCH = 40 * DEG;
  const SIN_PITCH = Math.sin(PITCH);
  const COS_PITCH = Math.cos(PITCH);

  const ROT_MS = 400;
  const ZOOM_SNAP_MS = 220;
  const TIER_MS = 320;
  const WHEEL_IDLE_MS = 250;
  const WHEEL_NOTCH_PX = 100;
  const WHEEL_STEP = 0.22;
  const TAP_MAX_MS = 400;
  const TAP_SLOP_PX = 6;
  const DRAG_START_PX = isTouch ? 8 : 6;
  const TWIST_THRESH = 0.7;
  const TWIST_ACCUM_CAP = TWIST_THRESH * 3;
  const PAN_MARGIN = 6;
  const DISTRICT_PAD = 1.05;
  const ROT_DEDUPE_MS = 30;
  const ZOOM_EPS = 1e-6;

  const TIER_NAMES = ['district', 'block', 'street'];
  const TIER_ZOOM = [1, 0.62, 0.38];
  const ZOOM_MAX = TIER_ZOOM[0];
  const ZOOM_MIN = TIER_ZOOM[TIER_NAMES.length - 1];

  function num(v, fallback) {
    return typeof v === 'number' && isFinite(v) ? v : fallback;
  }

  function clamp(v, lo, hi) {
    return v < lo ? lo : (v > hi ? hi : v);
  }

  function easeInOutCubic(t) {
    return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) * 0.5;
  }

  function tierIndexByName(name) {
    for (let i = 0; i < TIER_NAMES.length; i++) {
      if (TIER_NAMES[i] === name) return i;
    }
    return -1;
  }

  function nearestTierIndex(z) {
    let best = 0;
    let bestD = Infinity;
    for (let i = 0; i < TIER_ZOOM.length; i++) {
      const d = Math.abs(TIER_ZOOM[i] - z);
      if (d < bestD) {
        bestD = d;
        best = i;
      }
    }
    return best;
  }

  // ---------------- bounds ----------------
  const minX = num(bounds && bounds.minX, -120);
  const maxX = num(bounds && bounds.maxX, 120);
  const minZ = num(bounds && bounds.minZ, -120);
  const maxZ = num(bounds && bounds.maxZ, 120);

  const center = new THREE.Vector3();
  if (bounds && bounds.center && typeof bounds.center.x === 'number') {
    center.set(bounds.center.x, num(bounds.center.y, 0), num(bounds.center.z, 0));
  } else {
    center.set((minX + maxX) * 0.5, 0, (minZ + maxZ) * 0.5);
  }

  const halfSpan = Math.sqrt(
    (maxX - minX) * (maxX - minX) + (maxZ - minZ) * (maxZ - minZ)
  ) * 0.5;
  const givenRadius = num(bounds && bounds.radius, 0);
  // An explicit radius wins: it is the framing radius (what the district shot must contain), and
  // it is deliberately independent of the pan extents, which only clamp the look-at target.
  // The diagonal half-span is the fallback for a caller that supplies only extents.
  const radius = givenRadius > 0 ? givenRadius : Math.max(halfSpan, 1);

  function getWindow() {
    if (dom && dom.ownerDocument && dom.ownerDocument.defaultView) {
      return dom.ownerDocument.defaultView;
    }
    return null;
  }

  let viewportW = 1;
  let viewportH = 1;

  function readRect() {
    let w = 0;
    let h = 0;
    let left = 0;
    let top = 0;
    if (dom && typeof dom.getBoundingClientRect === 'function') {
      const r = dom.getBoundingClientRect();
      if (r) {
        w = num(r.width, 0);
        h = num(r.height, 0);
        left = num(r.left, 0);
        top = num(r.top, 0);
      }
    }
    if (!(w > 0)) w = num(dom && dom.clientWidth, 0);
    if (!(h > 0)) h = num(dom && dom.clientHeight, 0);
    if (!(w > 0) || !(h > 0)) {
      const win = getWindow();
      if (win) {
        if (!(w > 0)) w = num(win.innerWidth, 0);
        if (!(h > 0)) h = num(win.innerHeight, 0);
      }
    }
    return { w: w > 0 ? w : 1, h: h > 0 ? h : 1, left: left, top: top };
  }

  function measureViewport() {
    const r = readRect();
    viewportW = r.w;
    viewportH = r.h;
    return r;
  }

  const rect0 = measureViewport();
  let aspect = rect0.w / rect0.h;
  if (!(aspect > 0)) aspect = 1;

  // ---------------- state ----------------
  const camera = new THREE.PerspectiveCamera(FOV_DEG, aspect, 1, 2000);
  camera.up.set(0, 1, 0);

  const target = new THREE.Vector3();
  target.set(center.x, center.y, center.z);

  const glideFrom = new THREE.Vector3();
  const glideToVec = new THREE.Vector3();

  const raycaster = new THREE.Raycaster();
  const ndc = new THREE.Vector2();
  const fallbackHits = [];

  let districtDist = 1;
  let zoomFactor = ZOOM_MAX;

  let yaw = 0;
  let yawStep = 0;
  let yawFrom = 0;
  let yawTo = 0;
  let rotElapsed = ROT_MS;

  let zoomActive = false;
  let zoomFrom = ZOOM_MAX;
  let zoomEnd = ZOOM_MAX;
  let zoomElapsed = 0;
  let zoomDur = 0;

  let wheelIdle = 0;
  let manualZoom = false;
  let manualRotation = false;

  let glideActive = false;
  let glideElapsed = 0;
  let glideDur = 1;
  let glideZoomFrom = ZOOM_MAX;
  let glideZoomEnd = TIER_ZOOM[1];
  let glideDone = null;

  let lastRotateAt = -1e9;
  let lastRotateDir = 0;

  let pointerId = -1;
  let pressActive = false;
  let pressAt = 0;
  let pressX = 0;
  let pressY = 0;
  let lastX = 0;
  let lastY = 0;
  let dragging = false;

  let tapHandler = null;

  let touchGesture = false;
  let gestureBaseSpan = 1;
  let gestureBaseZoom = ZOOM_MAX;
  let gestureLastAngle = 0;
  let twistAccum = 0;
  let twistDir = 0;

  // ---------------- framing ----------------
  function computeDistrictDistance() {
    let a = camera.aspect;
    if (!(a > 0)) a = 1;
    const halfV = FOV_RAD * 0.5;
    const halfH = Math.atan(Math.tan(halfV) * a);
    const half = Math.min(halfV, halfH);
    const s = Math.max(0.02, Math.sin(half));
    districtDist = (radius * DISTRICT_PAD) / s;
    camera.near = Math.max(1, districtDist * 0.004);
    camera.far = Math.max(2000, districtDist * 6);
    camera.updateProjectionMatrix();
    return districtDist;
  }

  function applyCamera() {
    const d = zoomFactor * districtDist;
    const horiz = d * COS_PITCH;
    const vert = d * SIN_PITCH;
    camera.position.set(
      target.x + Math.sin(yaw) * horiz,
      target.y + vert,
      target.z + Math.cos(yaw) * horiz
    );
    camera.up.set(0, 1, 0);
    camera.lookAt(target);
  }

  function clampTarget() {
    const loX = minX - PAN_MARGIN;
    const hiX = maxX + PAN_MARGIN;
    const loZ = minZ - PAN_MARGIN;
    const hiZ = maxZ + PAN_MARGIN;
    target.x = clamp(target.x, loX, hiX);
    target.z = clamp(target.z, loZ, hiZ);
  }

  function frameDistrict() {
    zoomActive = false;
    wheelIdle = 0;
    rotElapsed = ROT_MS;
    yaw = yawStep * HALF_PI;
    yawFrom = yaw;
    yawTo = yaw;
    zoomFactor = ZOOM_MAX;
    zoomFrom = zoomFactor;
    zoomEnd = zoomFactor;
    glideActive = false;
    glideDone = null;
    manualZoom = false;
    manualRotation = false;
    target.set(center.x, center.y, center.z);
    computeDistrictDistance();
    applyCamera();
  }

  function onResize(width, height) {
    const w = num(width, 0) > 0 ? width : viewportW;
    const h = num(height, 0) > 0 ? height : viewportH;
    viewportW = w > 0 ? w : 1;
    viewportH = h > 0 ? h : 1;
    let a = viewportW / viewportH;
    if (!(a > 0)) a = 1;
    camera.aspect = a;
    camera.updateProjectionMatrix();
    if (!manualZoom && !manualRotation) {
      frameDistrict();
      return;
    }
    computeDistrictDistance();
    zoomFactor = clamp(zoomFactor, ZOOM_MIN, ZOOM_MAX);
    zoomEnd = clamp(zoomEnd, ZOOM_MIN, ZOOM_MAX);
    applyCamera();
  }

  // ---------------- zoom ----------------
  function startZoomTween(to, ms) {
    const v = clamp(to, ZOOM_MIN, ZOOM_MAX);
    if (Math.abs(v - zoomFactor) < ZOOM_EPS || !(ms > 0)) {
      zoomActive = false;
      zoomFactor = v;
      applyCamera();
      return;
    }
    zoomFrom = zoomFactor;
    zoomEnd = v;
    zoomElapsed = 0;
    zoomDur = ms;
    zoomActive = true;
  }

  function snapZoomToNearestTier() {
    startZoomTween(TIER_ZOOM[nearestTierIndex(zoomFactor)], ZOOM_SNAP_MS);
  }

  function setTier(name, animate) {
    const idx = tierIndexByName(name);
    if (idx < 0) return;
    cancelGlide();
    manualZoom = true;
    wheelIdle = 0;
    if (animate === false) {
      zoomActive = false;
      zoomFactor = TIER_ZOOM[idx];
      applyCamera();
      return;
    }
    startZoomTween(TIER_ZOOM[idx], TIER_MS);
  }

  // ---------------- rotation ----------------
  function rotate(dir) {
    const sign = dir < 0 ? -1 : 1;
    const now = Date.now();
    if (sign === lastRotateDir && now - lastRotateAt < ROT_DEDUPE_MS) return;
    lastRotateAt = now;
    lastRotateDir = sign;
    cancelGlide();
    manualRotation = true;
    yawStep += sign;
    yawFrom = yaw;
    yawTo = yawStep * HALF_PI;
    rotElapsed = 0;
  }

  // ---------------- glide ----------------
  function cancelGlide() {
    if (!glideActive) return false;
    glideActive = false;
    const cb = glideDone;
    glideDone = null;
    if (cb) cb();
    return true;
  }

  function glideTo(targetVector3, ms, onDone) {
    if (!targetVector3 || typeof targetVector3.x !== 'number') return;
    glideFrom.set(target.x, target.y, target.z);
    glideToVec.set(targetVector3.x, targetVector3.y, targetVector3.z);
    glideZoomFrom = zoomFactor;
    glideZoomEnd = TIER_ZOOM[1];
    glideDur = num(ms, 0) > 0 ? ms : 1;
    glideElapsed = 0;
    glideActive = true;
    glideDone = typeof onDone === 'function' ? onDone : null;
    zoomActive = false;
    wheelIdle = 0;
  }

  // ---------------- pan ----------------
  function panBy(dx, dy) {
    if (!(dx === dx) || !(dy === dy)) return;
    if (dx === 0 && dy === 0) return;
    const dist = zoomFactor * districtDist;
    const px = viewportH > 0 ? viewportH : 1;
    const wpp = (2 * dist * HALF_FOV_TAN) / px;
    const invSin = 1 / SIN_PITCH;
    const sy = Math.sin(yaw);
    const cy = Math.cos(yaw);
    target.x -= (dx * cy + dy * sy * invSin) * wpp;
    target.z -= (dx * -sy + dy * cy * invSin) * wpp;
    clampTarget();
    applyCamera();
  }

  // ---------------- input: press, drag, tap ----------------
  function fireTap(x, y) {
    if (tapHandler) tapHandler(x, y);
  }

  function startPress(x, y) {
    pressActive = true;
    dragging = false;
    pressAt = Date.now();
    pressX = x;
    pressY = y;
    lastX = x;
    lastY = y;
  }

  function movePress(x, y) {
    if (!pressActive) return;
    if (!dragging) {
      const tx = x - pressX;
      const ty = y - pressY;
      if (tx * tx + ty * ty > DRAG_START_PX * DRAG_START_PX) {
        dragging = true;
        cancelGlide();
        measureViewport();
      } else {
        lastX = x;
        lastY = y;
        return;
      }
    }
    const dx = x - lastX;
    const dy = y - lastY;
    lastX = x;
    lastY = y;
    panBy(dx, dy);
  }

  function endPress() {
    if (!pressActive) return;
    const wasDragging = dragging;
    pressActive = false;
    dragging = false;
    if (wasDragging) return;
    if (Date.now() - pressAt > TAP_MAX_MS) return;
    const dx = lastX - pressX;
    const dy = lastY - pressY;
    if (dx * dx + dy * dy > TAP_SLOP_PX * TAP_SLOP_PX) return;
    fireTap(pressX, pressY);
  }

  function cancelPress() {
    pressActive = false;
    dragging = false;
  }

  function onPointerDown(e) {
    if (!e) return;
    if (e.pointerType === 'touch') return;
    if (pointerId !== -1) return;
    if (typeof e.button === 'number' && e.button !== 0) return;
    pointerId = num(e.pointerId, 0);
    measureViewport();
    startPress(num(e.clientX, 0), num(e.clientY, 0));
    if (dom && typeof dom.setPointerCapture === 'function' && e.pointerId !== undefined) {
      try {
        dom.setPointerCapture(e.pointerId);
      } catch (err) {
        // Capture is a convenience: drags still work inside the canvas without it.
      }
    }
  }

  function onPointerMove(e) {
    if (!e) return;
    if (e.pointerType === 'touch') return;
    if (pointerId === -1 || e.pointerId !== pointerId) return;
    movePress(num(e.clientX, 0), num(e.clientY, 0));
  }

  function onPointerUp(e) {
    if (!e) return;
    if (e.pointerType === 'touch') return;
    if (pointerId === -1 || e.pointerId !== pointerId) return;
    pointerId = -1;
    movePress(num(e.clientX, 0), num(e.clientY, 0));
    endPress();
    if (dom && typeof dom.releasePointerCapture === 'function' && e.pointerId !== undefined) {
      try {
        dom.releasePointerCapture(e.pointerId);
      } catch (err) {
        // The pointer may already be gone; nothing to release.
      }
    }
  }

  function onPointerCancel() {
    pointerId = -1;
    cancelPress();
  }

  // ---------------- input: wheel ----------------
  function onWheel(e) {
    if (!e) return;
    if (e.cancelable !== false && typeof e.preventDefault === 'function') e.preventDefault();
    let d = num(e.deltaY, 0);
    if (e.deltaMode === 1) d *= 16;
    else if (e.deltaMode === 2) d *= 100;
    if (d === 0) return;
    cancelGlide();
    zoomActive = false;
    manualZoom = true;
    zoomFactor = clamp(zoomFactor + (d / WHEEL_NOTCH_PX) * WHEEL_STEP, ZOOM_MIN, ZOOM_MAX);
    wheelIdle = WHEEL_IDLE_MS;
    applyCamera();
  }

  // ---------------- input: touch ----------------
  function beginGesture(touches) {
    const a = touches[0];
    const b = touches[1];
    const dx = b.clientX - a.clientX;
    const dy = b.clientY - a.clientY;
    touchGesture = true;
    cancelPress();
    cancelGlide();
    zoomActive = false;
    wheelIdle = 0;
    gestureBaseSpan = Math.sqrt(dx * dx + dy * dy) || 1;
    gestureBaseZoom = zoomFactor;
    gestureLastAngle = Math.atan2(dy, dx);
    twistAccum = 0;
    twistDir = 0;
    measureViewport();
  }

  function onTouchStart(e) {
    if (!e || !e.touches) return;
    if (e.cancelable !== false && e.touches.length >= 2 && typeof e.preventDefault === 'function') {
      e.preventDefault();
    }
    if (e.touches.length >= 2) {
      beginGesture(e.touches);
      return;
    }
    if (touchGesture) return;
    if (e.touches.length === 1) {
      const t = e.touches[0];
      measureViewport();
      startPress(num(t.clientX, 0), num(t.clientY, 0));
    }
  }

  function onTouchMove(e) {
    if (!e || !e.touches) return;
    if (e.touches.length >= 2) {
      if (!touchGesture) beginGesture(e.touches);
      if (e.cancelable !== false && typeof e.preventDefault === 'function') e.preventDefault();
      const a = e.touches[0];
      const b = e.touches[1];
      const dx = b.clientX - a.clientX;
      const dy = b.clientY - a.clientY;
      const span = Math.sqrt(dx * dx + dy * dy) || 1;
      const ratio = gestureBaseSpan / span;
      const nextZoom = clamp(gestureBaseZoom * ratio, ZOOM_MIN, ZOOM_MAX);
      if (Math.abs(nextZoom - zoomFactor) > 1e-4) {
        manualZoom = true;
        zoomFactor = nextZoom;
      }
      const ang = Math.atan2(dy, dx);
      let delta = ang - gestureLastAngle;
      if (delta > PI) delta -= TWO_PI;
      else if (delta < -PI) delta += TWO_PI;
      gestureLastAngle = ang;
      if (delta === delta) {
        twistAccum = clamp(twistAccum + delta, -TWIST_ACCUM_CAP, TWIST_ACCUM_CAP);
      }
      if (twistDir === 0) {
        if (twistAccum >= TWIST_THRESH) {
          rotate(-1);
          twistDir = 1;
          twistAccum = 0;
        } else if (twistAccum <= -TWIST_THRESH) {
          rotate(1);
          twistDir = -1;
          twistAccum = 0;
        }
      } else if (twistDir > 0 && twistAccum <= -TWIST_THRESH) {
        rotate(1);
        twistDir = -1;
        twistAccum = 0;
      } else if (twistDir < 0 && twistAccum >= TWIST_THRESH) {
        rotate(-1);
        twistDir = 1;
        twistAccum = 0;
      }
      if (twistDir !== 0) manualZoom = true;
      applyCamera();
      return;
    }
    if (touchGesture) {
      if (e.cancelable !== false && typeof e.preventDefault === 'function') e.preventDefault();
      return;
    }
    if (e.touches.length === 1 && pressActive) {
      const t = e.touches[0];
      movePress(num(t.clientX, 0), num(t.clientY, 0));
      if (dragging && e.cancelable !== false && typeof e.preventDefault === 'function') {
        e.preventDefault();
      }
    }
  }

  function endGesture() {
    touchGesture = false;
    gestureBaseSpan = 1;
    twistAccum = 0;
    twistDir = 0;
    snapZoomToNearestTier();
    applyCamera();
  }

  function onTouchEnd(e) {
    if (!e || !e.touches) return;
    if (touchGesture) {
      if (e.touches.length >= 2) return;
      endGesture();
      return;
    }
    if (e.touches.length === 0) {
      const t = e.changedTouches && e.changedTouches[0];
      if (t) {
        lastX = num(t.clientX, lastX);
        lastY = num(t.clientY, lastY);
      }
      endPress();
    }
  }

  function onTouchCancel() {
    if (touchGesture) {
      endGesture();
      return;
    }
    cancelPress();
  }

  // ---------------- input: keys and resize ----------------
  function onKeyDown(e) {
    if (!e) return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.repeat) return;
    const t = e.target;
    if (t) {
      const tag = typeof t.tagName === 'string' ? t.tagName : '';
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || t.isContentEditable) return;
    }
    const k = e.key;
    if (k === 'q' || k === 'Q') rotate(-1);
    else if (k === 'e' || k === 'E') rotate(1);
  }

  function onWindowResize() {
    const r = measureViewport();
    onResize(r.w, r.h);
  }

  // ---------------- raycast ----------------
  function raycast(clientX, clientY, meshes, outIntersections) {
    const out = outIntersections && typeof outIntersections === 'object' ? outIntersections : fallbackHits;
    if (out.length) out.length = 0;
    if (!meshes || meshes.length === 0) return out;
    const r = readRect();
    const w = r.w > 0 ? r.w : 1;
    const h = r.h > 0 ? r.h : 1;
    ndc.x = ((clientX - r.left) / w) * 2 - 1;
    ndc.y = -((clientY - r.top) / h) * 2 + 1;
    raycaster.setFromCamera(ndc, camera);
    raycaster.intersectObjects(meshes, false, out);
    return out;
  }

  // ---------------- update ----------------
  function update(dt) {
    let ms = num(dt, 0);
    if (ms <= 0) ms = 0;
    else if (ms <= 1) ms *= 1000;
    if (ms > 1000) ms = 1000;

    let dirty = false;

    if (rotElapsed < ROT_MS) {
      rotElapsed += ms;
      if (rotElapsed >= ROT_MS) {
        rotElapsed = ROT_MS;
        yaw = yawTo;
      } else {
        yaw = yawFrom + (yawTo - yawFrom) * easeInOutCubic(rotElapsed / ROT_MS);
      }
      dirty = true;
    }

    if (wheelIdle > 0) {
      wheelIdle -= ms;
      if (wheelIdle <= 0) {
        wheelIdle = 0;
        snapZoomToNearestTier();
      }
    }

    if (glideActive) {
      glideElapsed += ms;
      let k = glideElapsed / glideDur;
      if (k >= 1) k = 1;
      const e = easeInOutCubic(k);
      target.x = glideFrom.x + (glideToVec.x - glideFrom.x) * e;
      target.y = glideFrom.y + (glideToVec.y - glideFrom.y) * e;
      target.z = glideFrom.z + (glideToVec.z - glideFrom.z) * e;
      zoomFactor = glideZoomFrom + (glideZoomEnd - glideZoomFrom) * e;
      if (k >= 1) {
        glideActive = false;
        target.set(glideToVec.x, glideToVec.y, glideToVec.z);
        zoomFactor = glideZoomEnd;
        clampTarget();
        const cb = glideDone;
        glideDone = null;
        if (cb) cb();
      }
      dirty = true;
    } else if (zoomActive) {
      zoomElapsed += ms;
      let k = zoomElapsed / zoomDur;
      if (k >= 1) {
        k = 1;
        zoomFactor = zoomEnd;
        zoomActive = false;
      } else {
        zoomFactor = zoomFrom + (zoomEnd - zoomFrom) * easeInOutCubic(k);
      }
      dirty = true;
    }

    if (dirty) applyCamera();
  }

  // ---------------- lifecycle ----------------
  function onTap(handler) {
    tapHandler = typeof handler === 'function' ? handler : null;
  }

  const winRef = getWindow();
  const keyTarget = winRef && typeof winRef.addEventListener === 'function' ? winRef : dom;

  function listen(node, type, fn, options) {
    if (node && typeof node.addEventListener === 'function') {
      node.addEventListener(type, fn, options === undefined ? false : options);
    }
  }

  function unlisten(node, type, fn, options) {
    if (node && typeof node.removeEventListener === 'function') {
      node.removeEventListener(type, fn, options === undefined ? false : options);
    }
  }

  function dispose() {
    unlisten(dom, 'pointerdown', onPointerDown);
    unlisten(dom, 'pointermove', onPointerMove);
    unlisten(dom, 'pointerup', onPointerUp);
    unlisten(dom, 'pointercancel', onPointerCancel);
    unlisten(dom, 'wheel', onWheel, { passive: false });
    unlisten(dom, 'touchstart', onTouchStart, { passive: false });
    unlisten(dom, 'touchmove', onTouchMove, { passive: false });
    unlisten(dom, 'touchend', onTouchEnd);
    unlisten(dom, 'touchcancel', onTouchCancel);
    unlisten(keyTarget, 'keydown', onKeyDown);
    unlisten(winRef, 'resize', onWindowResize);
    glideActive = false;
    glideDone = null;
    zoomActive = false;
    pressActive = false;
    dragging = false;
    touchGesture = false;
    tapHandler = null;
    pointerId = -1;
    wheelIdle = 0;
  }

  // ---------------- init ----------------
  computeDistrictDistance();
  applyCamera();

  listen(dom, 'pointerdown', onPointerDown);
  listen(dom, 'pointermove', onPointerMove);
  listen(dom, 'pointerup', onPointerUp);
  listen(dom, 'pointercancel', onPointerCancel);
  listen(dom, 'wheel', onWheel, { passive: false });
  listen(dom, 'touchstart', onTouchStart, { passive: false });
  listen(dom, 'touchmove', onTouchMove, { passive: false });
  listen(dom, 'touchend', onTouchEnd);
  listen(dom, 'touchcancel', onTouchCancel);
  listen(keyTarget, 'keydown', onKeyDown);
  listen(winRef, 'resize', onWindowResize);

  const rig = {
    camera: camera,
    update: update,
    rotate: rotate,
    setTier: setTier,
    frameDistrict: frameDistrict,
    onResize: onResize,
    glideTo: glideTo,
    lookAt: target,
    raycast: raycast,
    onTap: onTap,
    dispose: dispose
  };

  Object.defineProperty(rig, 'tier', {
    get: function () {
      return TIER_NAMES[nearestTierIndex(zoomFactor)];
    },
    enumerable: true,
    configurable: false
  });

  return rig;
}
