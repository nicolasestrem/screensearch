// Localtime: filmic post chain. Tilt-shift, mild bloom on lit windows, gentle grade.
//
// Owner: G. Contract:
//   createPost(THREE, renderer, scene, camera, { enabled, msaa, fxaa, bloom, resolutionScale })
//   returns { render(), setSize(w, h), setResolutionScale(s), setBloom(b), setDistanceGrade(k),
//             enabled, dispose() }.
//
// Frame cost, all on one shared full-screen triangle:
//   1. scene            -> sceneTarget    (device resolution, MSAA on desktop)
//   2. sceneTarget      -> halfTargetA    (tilt-shift: horizontal gaussian, half resolution)
//   3. halfTargetA      -> halfTargetB    (tilt-shift: vertical gaussian)
//   4. halfTargetB      -> bloomTargetA   (bright pass, quarter resolution)
//   5. bloomTargetA     -> bloomTargetB   (bloom: horizontal gaussian)
//   6. bloomTargetB     -> bloomTargetA   (bloom: vertical gaussian)
//   7. sharp + blurred + bloom -> canvas  (two-band mask, FXAA when MSAA is off, grade)
//
// That is 6 extra draws with bloom, 3 without (steps 2, 3 and 7): the bloom
// bright pass and its two blur passes are skipped outright when bloom is off, so
// the cost really disappears instead of rendering into a factor of zero.
//
// Tilt-shift: the sharp band spans the middle 62 percent of the frame height,
// which is where the district sits, and the mask feathers over the next 30
// percent so the top and bottom of the frame fall softly out of focus. The blur
// radius is about half of the earlier, much heavier setting, and it runs at half
// the post resolution because this is a low-frequency effect.
//
// Bloom: a threshold of 0.75 linear luminance keeps the daylight glow on lit
// windows, lamps and the sun disc, and off the sky and the stone. The bright pass
// is a box downsample to a quarter of the post resolution, then two separable
// gaussian passes. It is added back in linear light before the grade.
//
// Bloom at night is a different job: the frame is dark, so a wide, strong glow
// would wash a smooth field over the street and the window rows and dissolve the
// sodium ribbons the light is there to draw. Night therefore uses its own higher
// threshold (0.95, knee 0.30, so both the onset and the saturation point sit
// above their daylight values), a shorter blur radius and a little over a third
// of the daylight strength. Only genuinely bright sources (window cores, lamp
// heads, headlights) reach the bright pass, and the glow they add is tight
// enough to leave the edge structure of the street intact.
//
// Edge quality: `msaa` (default true) asks for a 4-sample scene target on WebGL2
// with at least 4 max samples. When that is unavailable and `fxaa` is true
// (default true), the composite applies an inline 8-tap FXAA to the sharp
// sample instead. Never both: `msaa: false, fxaa: true` is the mobile profile.
//
// Resolution scale: 0.5 to 1, applied to the post chain targets only (the blur
// and bloom buffers), never to the scene target, so geometry and shadows keep
// their resolution while a quality controller can shed post cost. The blur radii
// are expressed in post-chain texels and scale with the knob, so the blur and
// the glow keep the same footprint on screen and only the sampling cost changes.
//
// Grade: the composite warms the midtones in daylight (deep shade and highlights
// keep their own colour, and saturation is restored so greens and greys stay
// crisp), lifts the highlights a little warmer, lifts the midtones by a small
// daylight exposure that black does not feel, grades shade and distance toward
// violet-blue, adds a barely visible vignette, and lifts near-black pixels toward
// a deep blue at night. Warm and cool therefore contrast in the same frame: the
// town keeps the sun's warmth in its midtones and highlights while the shade of
// its streets and the distance behind it carry the sky's cool. The cool grade is
// a per-pixel mix keyed on luminance and on height in the frame, which the fixed
// 40 degree pitch makes a stable depth proxy, with three keys and one target: the
// global shade key (near-black), the distance key (upper frame) and the
// lower-frame shade key, which widens the luminance range and weighs itself
// toward the bottom of the frame so the street and the near hillside read cool
// too. Its target is luma-constant, so the tonal structure and the edge contrast
// are not disturbed. The day and night amounts come from the scene haze colour,
// read once per frame. Every day term is multiplied by exactly zero at night, so
// the night frame stays bit-identical. Night stays deep blue, never black: the
// night floor is almost pure blue (1.0 against 0.05 red and 0.12 green), so the
// darkest pixels cannot settle on the grey-green the ambient moon and haze leave
// behind, and because blue carries little luminance the lift costs the night none
// of its edge contrast.
//
// A night white guard in the same pass closes the one hole the additive emitters
// leave. Two near-neutral night sources over one pixel (the dream's unlit road
// under its flood pools, the sodium ribbon and road paint, a window pane under
// its halo, the bloom on top) can pass 1.0 in linear light, where the 8-bit
// canvas would clamp all three channels and turn the hue into a flat white
// plate. A highlight whose low channel is high, which is the near-neutral sum
// that can actually reach white, is eased toward a ceiling below white with one
// shared scale on all three channels, so the hue ratio survives and the
// brightest thing on screen stays the brightest, only warm. Saturated sodium
// sources and every value below the knee keep their exact transfer, and the day
// frame is untouched.
//
// Colour management: three renders into a render target in linear space, and a
// raw ShaderMaterial output is not colour managed, so the composite pass applies
// the exact sRGB transfer function itself. That keeps the sky gradient smooth and
// avoids any dependency on post-processing addons. Because three also skips tone
// mapping when the target is a render target, the renderer must stay on its
// default NoToneMapping (the whole page is graded by world.lights.setTimeOfDay).
//
// Distance grade weight: the distance key is keyed on height in the frame, and
// that is a depth proxy only at the district distance, where the rig frames the
// whole bounds sphere and everything in the upper frame really is far away. At
// the block and street tiers the camera stands close enough that the upper frame
// holds geometry a couple of hundred metres off, and a full-strength key paints
// that near city slate violet: a monochrome grey town. setDistanceGrade(k) scales
// only the distance amount by k (0..1), so main can weight the key by the camera's
// distance to its look-at. The height key and the luminance key inside the
// distance term, and both shade keys, are untouched.
//
// setSize(w, h) takes CSS pixels, exactly like renderer.setSize(). The device
// pixel ratio is read from the renderer, so a ratio change (browser zoom, monitor
// move) is picked up on the next frame without allocating anything per frame.

export function createPost(THREE, renderer, scene, camera, opts) {
  const settings = opts || {};

  // Tilt-shift band: sharp inside 0.62 of the half-height, fully blurred past
  // 0.92, so the middle of the frame holds focus.
  const BAND_EDGE = 0.62;
  const BAND_FEATHER = 0.3;
  // Measured in half-resolution texels. The gaussian below reaches 3.23 times
  // this radius, so the far crests soften without dissolving: the old 0.7
  // smeared the hill silhouettes and the thin tree crowns across the sky band.
  // Cycle 21: 0.55 still reads as an effect rather than a lens; the golden-hour
  // brief wants the far hillside near-sharp, so the radius drops to 0.36 and the
  // band profile is re-measured on the day frame.
  const BLUR_RADIUS = 0.45;
  // Bloom stays mild in daylight: only pixels well above mid grey contribute,
  // and the result is added back at 0.36 in linear light. Night carries its own
  // threshold, radius and strength: higher onset, shorter reach, weaker glow.
  const BLOOM_THRESHOLD = 0.75;
  const BLOOM_KNEE = 0.35;
  const BLOOM_THRESHOLD_NIGHT = 0.95;
  const BLOOM_KNEE_NIGHT = 0.3;
  const BLOOM_RADIUS = 1.4;
  const BLOOM_RADIUS_NIGHT = 0.75;
  const BLOOM_STRENGTH = 0.5;
  const BLOOM_DAY_WEIGHT = 0.72;
  const BLOOM_NIGHT_STRENGTH = 0.22;
  // Dream bloom. While the dream runs the frame is one warm chantier inside a dark district,
  // and the glow has to reach the works instead of sitting on the source: the strength rises,
  // the radius widens and the night onset drops a little. All three are lerped by the dream
  // weight, so an awake night frame is bit-identical to before, and the day frame never sees
  // them at all. The sky is safe by construction: the top tenth of the night frame measures
  // under this threshold even at its brightest, and the measurement after the change keeps
  // its luma and its blue share (see ITERATIONS.md).
  const BLOOM_DREAM_STRENGTH = 0.26;
  const BLOOM_DREAM_THRESHOLD = 0.90;
  const BLOOM_DREAM_RADIUS = 1.10;
  // Night white guard. Every night emitter is authored hot on purpose, and two
  // of them over one pixel can pass 1.0 in linear light: the dream's
  // provisional road (a neutral 0.85 albedo sheet that the ten percent district
  // light cannot dim) under its two additive flood pools, with the sodium
  // ribbon and the white paint on the streets it crosses, and the bloom on top.
  // The sum clips all three channels and the hue is erased, which is the flat
  // white hole the art brief forbids. The guard keys on the low channel: it
  // compresses a highlight only as far as it is nearly neutral, and a saturated
  // sodium line (blue about 0.25 to 0.5 of the peak) keeps its exact value, so
  // the field of warm windows and the ribbons are not dimmed to pay for the
  // defect. Above the knee one scale shared by the three channels eases the
  // peak toward a ceiling below white, so the hue ratio survives; the knee at
  // 0.90 linear and the slope at 40 put that ceiling at 0.925 linear, 0.969
  // encoded, under the 0.985 white test. Below the knee and the white key the
  // transfer is exact, so the deep blue floor and the dark share do not move,
  // and only pixels that were going to clip all three channels are touched.
  // Day is untouched: the sun disc may take the canvas ceiling.
  const NIGHT_SHOULDER_KNEE = 0.90;
  const NIGHT_SHOULDER_SLOPE = 40.0;
  const NIGHT_WHITE_LO = 0.45;
  const NIGHT_WHITE_HI = 0.70;
  // Grade: warm midtones, warm highlights, cool shade and cool distance, night
  // floor. The night floor is blue-dominant on purpose, so near-black pixels
  // cannot read grey.
  const DAY_WARM = 0.05;
  const DAY_SAT = 0.09;
  const NIGHT_LIFT = 0.014;
  const NIGHT_LIFT_NIGHT = 0.052;
  const HIGH_WARM = 0.045;
  // Shade and distance grade. Shade is the low-luminance end (surfaces lit by
  // sky ambient only). Distance is a screen-height key, which is only a depth
  // proxy at the district tier, and even there the far hills carry their own
  // haze in the terrain's vertex colours and the scene fog. The key therefore
  // starts in the top fifth of the frame and stays mild. Its daylight target is
  // a warm value-matched haze, not the old violet: a violet target mixed with
  // the warm sky was the smeared purple band over the distant hills. Night is
  // multiplied out by the day term, so its palette is untouched.
  const COOL_SHADE_LO = 0.006;
  const COOL_SHADE_HI = 0.034;
  const COOL_DIST_LO = 0.80;
  const COOL_DIST_HI = 1.0;
  const COOL_SHADE_AMOUNT = 0.9;
  // A veil, not a coat: at full strength the old distance key replaced the far
  // hills' own hue with the grade target, which is how the warm haze turned into
  // a flat band. A third of that strength keeps the sky's blue and the ridge
  // layers' own values visible through the warm air.
  const COOL_DIST_AMOUNT = 0.18;
  // Lower-frame shade: the same warm veil, but on a wider luminance key and
  // weighted toward the bottom of the frame. The
  // global shade key above only reaches near-black, where the distance key does
  // not, so the street and the near hillside stayed warm. This key reaches the
  // shadowed surfaces of the lower frame (linear luma under about 0.05) and
  // fades out by 58 percent of the height, which is below the sharp band's
  // middle. A lit wall or roof sits far above the key, so it keeps its colour.
  const COOL_DOWN_LO = 0.014;
  const COOL_DOWN_HI = 0.048;
  const COOL_DOWN_AMOUNT = 0.82;
  const COOL_DOWN_FADE_LO = 0.10;
  const COOL_DOWN_FADE_HI = 0.42;
  // Daylight exposure: a small gain applied to the encoded pixel, keyed on the
  // encoded midpoint (max + min) / 2, the same lightness the frame's dark share
  // is measured on. The gate opens at 0.20, just above the 0.18 dark threshold,
  // so every pixel that reads as dark is bit-for-bit untouched, and it closes
  // again in the top fifth so highlights never clip. A uniform multiply of the
  // encoded channels moves value only, so hue and saturation are exactly
  // preserved. Night is multiplied by zero, so its frame is bit-identical.
  const DAY_EXPOSURE = 0.06;
  const EXPOSURE_LO = 0.20;
  const EXPOSURE_HI = 0.34;
  const EXPOSURE_TOP_LO = 0.80;
  const EXPOSURE_TOP_HI = 0.98;
  const VIGNETTE = 0.09;
  // Post chain resolution knob.
  const MIN_SCALE = 0.5;
  const MAX_SCALE = 1;
  // Scene target multisampling on WebGL2.
  const MSAA_SAMPLES = 4;

  const VERT_SOURCE = `
    varying vec2 vUv;

    void main() {
      vUv = position.xy * 0.5 + 0.5;
      gl_Position = vec4( position.xy, 0.0, 1.0 );
    }
  `;

  // Separable gaussian, 5 bilinear taps for a radius of 4 texels. uDir selects
  // the axis, so all four blur passes share one program body: the tilt-shift
  // pair at half resolution and the bloom pair at quarter resolution differ only
  // in uTexel and uRadius.
  const BLUR_FRAG_SOURCE = `
    uniform sampler2D uTex;
    uniform vec2 uTexel;
    uniform vec2 uDir;
    uniform float uRadius;
    varying vec2 vUv;

    void main() {
      vec2 off = uTexel * uDir * uRadius;
      vec4 sum = texture2D( uTex, vUv ) * 0.2270270270;
      sum += ( texture2D( uTex, vUv + off * 1.3846153846 ) + texture2D( uTex, vUv - off * 1.3846153846 ) ) * 0.3162162162;
      sum += ( texture2D( uTex, vUv + off * 3.2307692308 ) + texture2D( uTex, vUv - off * 3.2307692308 ) ) * 0.0702702703;
      gl_FragColor = sum;
    }
  `;

  // Bright pass and 2x downsample in one: four bilinear taps average the source
  // span, which keeps the glow from crawling when the camera pans. A soft knee
  // above the threshold avoids a hard cut on a slowly brightening window.
  const BRIGHT_FRAG_SOURCE = `
    uniform sampler2D tSrc;
    uniform vec2 uTexel;
    uniform float uThreshold;
    uniform float uKnee;
    varying vec2 vUv;

    void main() {
      vec3 c = texture2D( tSrc, vUv + vec2( -0.5, -0.5 ) * uTexel ).rgb;
      c += texture2D( tSrc, vUv + vec2( 0.5, -0.5 ) * uTexel ).rgb;
      c += texture2D( tSrc, vUv + vec2( -0.5, 0.5 ) * uTexel ).rgb;
      c += texture2D( tSrc, vUv + vec2( 0.5, 0.5 ) * uTexel ).rgb;
      c *= 0.25;
      float luma = dot( c, vec3( 0.2126, 0.7152, 0.0722 ) );
      gl_FragColor = vec4( c * smoothstep( uThreshold, uThreshold + uKnee, luma ), 1.0 );
    }
  `;

  const COMPOSITE_FRAG_SOURCE = `
    uniform sampler2D tSharp;
    uniform sampler2D tBlur;
    uniform sampler2D tBloom;
    uniform vec2 uSharpTexel;
    uniform float uBand;
    uniform float uFeather;
    uniform float uWarm;
    uniform float uSat;
    uniform float uLift;
    uniform float uNightLift;
    uniform float uNight;
    uniform float uBloom;
    uniform float uHigh;
    uniform float uVig;
    uniform float uFxaa;
    uniform float uShadeLo;
    uniform float uShadeHi;
    uniform float uDistLo;
    uniform float uDistHi;
    uniform float uShadeAmt;
    uniform float uDistAmt;
    uniform float uDownLo;
    uniform float uDownHi;
    uniform float uDownAmt;
    uniform float uDownFadeLo;
    uniform float uDownFadeHi;
    uniform float uShoulderLo;
    uniform float uShoulderK;
    uniform float uWhiteLo;
    uniform float uWhiteHi;
    uniform float uExposure;
    uniform float uExpLo;
    uniform float uExpHi;
    uniform float uExpTopLo;
    uniform float uExpTopHi;
    varying vec2 vUv;

    vec3 encodeSRGB( vec3 c ) {
      c = max( c, vec3( 0.0 ) );
      vec3 low = c * 12.92;
      vec3 high = 1.055 * pow( c, vec3( 1.0 / 2.4 ) ) - 0.055;
      return mix( low, high, step( vec3( 0.0031308 ), c ) );
    }

    float lumaOf( vec3 c ) {
      return dot( c, vec3( 0.2126, 0.7152, 0.0722 ) );
    }

    // Inline FXAA: four corner taps find the edge direction, four taps along that
    // direction average across the edge, eight taps in total. Runs only when the
    // scene target has no MSAA, so the desktop path never pays for both.
    vec3 fxaaSample( vec2 uv ) {
      vec3 nw = texture2D( tSharp, uv + vec2( -1.0, -1.0 ) * uSharpTexel ).rgb;
      vec3 ne = texture2D( tSharp, uv + vec2( 1.0, -1.0 ) * uSharpTexel ).rgb;
      vec3 sw = texture2D( tSharp, uv + vec2( -1.0, 1.0 ) * uSharpTexel ).rgb;
      vec3 se = texture2D( tSharp, uv + vec2( 1.0, 1.0 ) * uSharpTexel ).rgb;

      float lNW = lumaOf( nw );
      float lNE = lumaOf( ne );
      float lSW = lumaOf( sw );
      float lSE = lumaOf( se );
      float lMin = min( min( lNW, lNE ), min( lSW, lSE ) );
      float lMax = max( max( lNW, lNE ), max( lSW, lSE ) );

      vec2 dir = vec2( -( ( lNW + lNE ) - ( lSW + lSE ) ), ( lNW + lSW ) - ( lNE + lSE ) );
      float reduce = max( ( lNW + lNE + lSW + lSE ) * 0.03125, 0.0078125 );
      float scale = 1.0 / ( min( abs( dir.x ), abs( dir.y ) ) + reduce );
      dir = clamp( dir * scale, vec2( -8.0 ), vec2( 8.0 ) ) * uSharpTexel;

      vec3 a = 0.5 * ( texture2D( tSharp, uv - dir * ( 1.0 / 6.0 ) ).rgb + texture2D( tSharp, uv + dir * ( 1.0 / 6.0 ) ).rgb );
      vec3 b = 0.5 * ( texture2D( tSharp, uv - dir * 0.5 ).rgb + texture2D( tSharp, uv + dir * 0.5 ).rgb );
      vec3 average = 0.25 * ( nw + ne + sw + se );

      float lB = lumaOf( b );
      vec3 picked = ( lB < lMin || lB > lMax ) ? a : b;
      // With no edge contrast the direction means nothing, so fall back to the
      // plain average instead of smearing a flat area sideways.
      return mix( average, picked, smoothstep( 0.02, 0.12, lMax - lMin ) );
    }

    void main() {
      vec3 sharp = uFxaa > 0.5 ? fxaaSample( vUv ) : texture2D( tSharp, vUv ).rgb;
      vec3 blurred = texture2D( tBlur, vUv ).rgb;
      float d = abs( vUv.y - 0.5 ) * 2.0;
      float maskValue = smoothstep( uBand, uBand + uFeather, d );
      vec3 c = mix( sharp, blurred, maskValue );

      // Bloom in linear light, before any grading, so lit windows glow without
      // lifting the parts of the frame that are not lit.
      c += texture2D( tBloom, vUv ).rgb * uBloom;

      float luma = lumaOf( c );
      float day = 1.0 - uNight;

      // Warm the midtones only: deep shadows and bright highlights keep their
      // own colour, so nothing turns uniformly orange.
      float mid = smoothstep( 0.04, 0.30, luma ) * ( 1.0 - smoothstep( 0.55, 0.95, luma ) );
      float lift = mid * day;
      c *= mix( vec3( 1.0 ), vec3( 1.0 + uWarm, 1.0 + uWarm * 0.34, 1.0 - uWarm * 0.58 ), lift );
      // Put back a little saturation that the warm multiply costs, so greens and
      // greys stay crisp instead of drifting toward the same warm grey.
      c = mix( vec3( luma ), c, 1.0 + uSat * lift );

      // A gentle warm lift in the highlights: sunlit stone and the sun disc keep
      // a warm edge instead of going cream.
      float high = smoothstep( 0.52, 0.95, luma ) * day;
      c *= mix( vec3( 1.0 ), vec3( 1.0 + uHigh, 1.0 + uHigh * 0.42, 1.0 - uHigh * 0.5 ), high );

      // Daylight exposure is applied after encoding, at the end of main, where a
      // uniform channel multiply leaves hue and saturation exactly as they are
      // and only the value moves.

      // Warm shade and distance grade. Two keys drive one per-pixel mix. The
      // shade key is low linear luminance, where the stone is lit only by sky
      // ambient; the distance key is height in the frame, a stable depth proxy
      // at the rig's fixed pitch, so the far hills and the sky sit in the upper
      // frame. Bright pixels are excluded from the distance key, so the sun disc
      // and sunlit stone keep their own colour. Both legs are warm and
      // value-matched, and the distance leg is applied as a light veil, so the
      // hills keep their own blue-to-cream layering instead of being replaced by
      // one flat grade hue.
      float shadeKey = 1.0 - smoothstep( uShadeLo, uShadeHi, luma );
      float distKey = smoothstep( uDistLo, uDistHi, vUv.y )
        * ( 1.0 - smoothstep( 0.42, 0.95, luma ) );
      // The lower-frame key: a wider luminance key, faded in toward the bottom
      // of the frame. It only reaches shade (linear luma under about 0.05), so
      // the lit walls and terracotta roofs stay warm; it is the same warm target
      // below, not a second hue path.
      float downKey = 1.0 - smoothstep( uDownLo, uDownHi, luma );
      float downMask = 1.0 - smoothstep( uDownFadeLo, uDownFadeHi, vUv.y );
      float coolShade = smoothstep( 0.30, 0.70, shadeKey );
      float coolDown = smoothstep( 0.25, 0.75, downKey ) * downMask;
      float coolDist = smoothstep( 0.55, 0.92, distKey );
      float coolValue = max( max( c.r, c.g ), c.b );
      // Two warm, value-matched targets. The near leg is a light cream, the far
      // leg a pale warm haze; both carry most of the pixel's own value, so tonal
      // structure, edge contrast, the mean value and the dark share survive.
      // Mixing from a golden hill into a cream in RGB stays in the warm hues;
      // the old rose-to-violet path was what dragged every far crest into
      // purple, and the night frame never sees either target because the whole
      // term is multiplied by the day weight.
      vec3 nearHaze = coolValue * vec3( 1.010, 0.945, 0.835 );
      vec3 farHaze = coolValue * vec3( 0.995, 0.975, 0.880 );
      float coolMix = max( max( coolShade * uShadeAmt, coolDown * uDownAmt ), coolDist * uDistAmt ) * day;
      // The near leg takes the first part of the ramp and the far leg the top,
      // so the shaded foreground and the top band both settle on a warm veil
      // instead of crossing a cold midpoint.
      float toNear = smoothstep( 0.10, 0.45, coolMix );
      float toFar = smoothstep( 0.35, 0.80, coolMix );
      c = mix( mix( c, nearHaze, toNear ), farHaze, toFar );

      // Near-black floor, split in two so the daylight term is untouched. In
      // daylight a quarter strength of the old floor stays under the shadows and
      // keeps them out of crushed black whatever the haze reports. At night the
      // floor is much heavier and almost pure blue: 1.0 blue against 0.05 red and
      // 0.12 green. Blue carries about 7 percent of the luminance, so this lifts
      // the dark pixels to a deep blue without raising their luminance, which
      // leaves the night's tonal structure and edge contrast standing.
      float floorW = 1.0 - smoothstep( 0.0, 0.20, luma );
      c += uLift * 0.25 * day * floorW * vec3( 0.40, 0.54, 1.0 );
      c += uNightLift * uNight * floorW * vec3( 0.05, 0.12, 1.0 );

      // Barely there vignette: about 4 percent at the frame edge and 9 at the
      // corners, well short of anything that reads as camera falloff.
      vec2 q = vUv - 0.5;
      c *= 1.0 - uVig * dot( q, q ) * 2.0;

      // Night white guard. Additive sources are authored hot on purpose, and two
      // of them over the same pixel pass 1.0 in linear light, where a hard clip
      // erases the hue and leaves a flat white hole: the dream's provisional
      // road (neutral 0.85, unlit by design) under its two flood pools is the
      // measured case, with the sodium ribbon, the white paint it crosses and
      // the bloom stacking around it. The key is the low channel, so the guard
      // acts only on the sum that has gone near-neutral, which is the pair that
      // can actually reach white; a saturated sodium source keeps its own value
      // and only the near-white stack is eased, with one shared scale on all
      // three channels so the hue ratio survives. The brightest thing in the
      // frame stays the brightest thing, now warm instead of white. Below the
      // knee and below the white key the transfer is exact, so the deep blue
      // floor and the dark share are untouched; only pixels that were going to
      // clip all three channels are changed. Night only.
      if ( uNight > 0.5 ) {
        float peak = max( max( c.r, c.g ), c.b );
        if ( peak > uShoulderLo ) {
          float low = min( min( c.r, c.g ), c.b );
          float white = smoothstep( uWhiteLo, uWhiteHi, low );
          if ( white > 0.0 ) {
            float head = peak - uShoulderLo;
            float eased = head / ( 1.0 + head * uShoulderK );
            c *= mix( peak, uShoulderLo + eased, white ) / peak;
          }
        }
      }

      // A small exposure lift, keyed on the encoded midpoint (max + min) / 2, the
      // same lightness the dark share is measured on. The gate opens at 0.20,
      // above the 0.18 dark threshold, so a pixel that reads as dark is
      // bit-for-bit untouched, and it closes again in the last fifth of the
      // range so highlights never clip. Scaling the encoded channels by one
      // factor moves value only: hue and saturation are exactly preserved, so no
      // surface leaves its colour family. Multiplied by day, so the night frame
      // is untouched.
      vec3 outc = encodeSRGB( c );
      float oMax = max( max( outc.r, outc.g ), outc.b );
      float oMin = min( min( outc.r, outc.g ), outc.b );
      float oMid = ( oMax + oMin ) * 0.5;
      float expKey = smoothstep( uExpLo, uExpHi, oMid )
        * ( 1.0 - smoothstep( uExpTopLo, uExpTopHi, oMid ) );
      outc *= 1.0 + uExposure * expKey * day;
      gl_FragColor = vec4( outc, 1.0 );
    }
  `;

  function pickTargetType() {
    const caps = renderer.capabilities || {};
    if (!caps.isWebGL2) return THREE.UnsignedByteType;
    try {
      const gl = renderer.getContext();
      if (!gl || typeof gl.getExtension !== 'function') return THREE.UnsignedByteType;
      const half = gl.getExtension('EXT_color_buffer_half_float');
      if (half) return THREE.HalfFloatType;
      const full = gl.getExtension('EXT_color_buffer_float');
      if (full) return THREE.HalfFloatType;
      return THREE.UnsignedByteType;
    } catch (err) {
      return THREE.UnsignedByteType;
    }
  }

  const targetType = pickTargetType();

  // Multisampling needs WebGL2 and at least four samples. Anything less falls
  // back silently to no MSAA, and the FXAA decision below picks up the slack.
  function detectSamples() {
    const caps = renderer.capabilities || {};
    if (caps.isWebGL2 !== true) return 0;
    if (settings.msaa === false) return 0;
    const maxSamples = typeof caps.maxSamples === 'number' ? caps.maxSamples : 0;
    if (maxSamples < MSAA_SAMPLES) return 0;
    return MSAA_SAMPLES;
  }

  const msaaSamples = detectSamples();
  const fxaaEnabled = settings.fxaa !== false && msaaSamples === 0;

  function makeTarget(depthBuffer, samples) {
    const rt = new THREE.WebGLRenderTarget(1, 1, {
      minFilter: THREE.LinearFilter,
      magFilter: THREE.LinearFilter,
      format: THREE.RGBAFormat,
      type: targetType,
      depthBuffer: depthBuffer,
      stencilBuffer: false,
      generateMipmaps: false,
      samples: samples,
    });
    rt.texture.generateMipmaps = false;
    if (THREE.NoColorSpace !== undefined) rt.texture.colorSpace = THREE.NoColorSpace;
    return rt;
  }

  const sceneTarget = makeTarget(true, msaaSamples);
  const halfTargetA = makeTarget(false, 0);
  const halfTargetB = makeTarget(false, 0);
  const bloomTargetA = makeTarget(false, 0);
  const bloomTargetB = makeTarget(false, 0);

  const blurMaterial = new THREE.ShaderMaterial({
    uniforms: {
      uTex: { value: sceneTarget.texture },
      uTexel: { value: new THREE.Vector2(1, 1) },
      uDir: { value: new THREE.Vector2(1, 0) },
      uRadius: { value: BLUR_RADIUS },
    },
    vertexShader: VERT_SOURCE,
    fragmentShader: BLUR_FRAG_SOURCE,
    depthTest: false,
    depthWrite: false,
  });

  const brightMaterial = new THREE.ShaderMaterial({
    uniforms: {
      tSrc: { value: halfTargetB.texture },
      uTexel: { value: new THREE.Vector2(1, 1) },
      uThreshold: { value: BLOOM_THRESHOLD },
      uKnee: { value: BLOOM_KNEE },
    },
    vertexShader: VERT_SOURCE,
    fragmentShader: BRIGHT_FRAG_SOURCE,
    depthTest: false,
    depthWrite: false,
  });

  const compositeMaterial = new THREE.ShaderMaterial({
    uniforms: {
      tSharp: { value: sceneTarget.texture },
      tBlur: { value: halfTargetB.texture },
      tBloom: { value: bloomTargetA.texture },
      uSharpTexel: { value: new THREE.Vector2(1, 1) },
      uBand: { value: BAND_EDGE },
      uFeather: { value: BAND_FEATHER },
      uWarm: { value: DAY_WARM },
      uSat: { value: DAY_SAT },
      uLift: { value: NIGHT_LIFT },
      uNightLift: { value: NIGHT_LIFT_NIGHT },
      uNight: { value: 0 },
      uBloom: { value: BLOOM_STRENGTH * BLOOM_DAY_WEIGHT },
      uHigh: { value: HIGH_WARM },
      uVig: { value: VIGNETTE },
      uFxaa: { value: fxaaEnabled ? 1 : 0 },
      uShadeLo: { value: COOL_SHADE_LO },
      uShadeHi: { value: COOL_SHADE_HI },
      uDistLo: { value: COOL_DIST_LO },
      uDistHi: { value: COOL_DIST_HI },
      uShadeAmt: { value: COOL_SHADE_AMOUNT },
      uDistAmt: { value: COOL_DIST_AMOUNT },
      uDownLo: { value: COOL_DOWN_LO },
      uDownHi: { value: COOL_DOWN_HI },
      uDownAmt: { value: COOL_DOWN_AMOUNT },
      uDownFadeLo: { value: COOL_DOWN_FADE_LO },
      uDownFadeHi: { value: COOL_DOWN_FADE_HI },
      uShoulderLo: { value: NIGHT_SHOULDER_KNEE },
      uShoulderK: { value: NIGHT_SHOULDER_SLOPE },
      uWhiteLo: { value: NIGHT_WHITE_LO },
      uWhiteHi: { value: NIGHT_WHITE_HI },
      uExposure: { value: DAY_EXPOSURE },
      uExpLo: { value: EXPOSURE_LO },
      uExpHi: { value: EXPOSURE_HI },
      uExpTopLo: { value: EXPOSURE_TOP_LO },
      uExpTopHi: { value: EXPOSURE_TOP_HI },
    },
    vertexShader: VERT_SOURCE,
    fragmentShader: COMPOSITE_FRAG_SOURCE,
    depthTest: false,
    depthWrite: false,
  });

  const blurUniforms = blurMaterial.uniforms;
  const brightUniforms = brightMaterial.uniforms;
  const compositeUniforms = compositeMaterial.uniforms;

  // One triangle covering the clip volume, so no vertex data is amortised per
  // draw and the passes cost nothing but their fragment work.
  const quadGeometry = new THREE.BufferGeometry();
  quadGeometry.setAttribute('position', new THREE.Float32BufferAttribute([
    -1, -1, 0,
    3, -1, 0,
    -1, 3, 0,
  ], 3));
  quadGeometry.boundingSphere = new THREE.Sphere(new THREE.Vector3(), 4);

  const quadScene = new THREE.Scene();
  const quadCamera = new THREE.Camera();
  const quadMesh = new THREE.Mesh(quadGeometry, compositeMaterial);
  quadMesh.frustumCulled = false;
  quadMesh.matrixAutoUpdate = false;
  quadScene.add(quadMesh);

  const size = {
    cssW: 1, cssH: 1, dpr: 1,
    invHalfX: 1, invHalfY: 1, invQuarterX: 1, invQuarterY: 1,
  };
  const bufferScratch = new THREE.Vector2(1, 1);

  let bloomOn = settings.bloom !== false;
  let postScale = clampScale(settings.resolutionScale);
  let dreamWeight = 0;
  let disposed = false;

  function clampScale(value) {
    const s = typeof value === 'number' && isFinite(value) ? value : 1;
    if (s < MIN_SCALE) return MIN_SCALE;
    if (s > MAX_SCALE) return MAX_SCALE;
    return s;
  }

  function applySize(cssW, cssH, dpr) {
    const pxW = Math.max(2, Math.round(cssW * dpr));
    const pxH = Math.max(2, Math.round(cssH * dpr));
    const halfW = Math.max(2, Math.round(pxW * 0.5 * postScale));
    const halfH = Math.max(2, Math.round(pxH * 0.5 * postScale));
    const quarterW = Math.max(1, Math.round(halfW * 0.5));
    const quarterH = Math.max(1, Math.round(halfH * 0.5));

    sceneTarget.setSize(pxW, pxH);
    halfTargetA.setSize(halfW, halfH);
    halfTargetB.setSize(halfW, halfH);
    bloomTargetA.setSize(quarterW, quarterH);
    bloomTargetB.setSize(quarterW, quarterH);

    size.invHalfX = 1 / halfW;
    size.invHalfY = 1 / halfH;
    size.invQuarterX = 1 / quarterW;
    size.invQuarterY = 1 / quarterH;

    brightUniforms.uTexel.value.set(size.invHalfX, size.invHalfY);
    compositeUniforms.uSharpTexel.value.set(1 / pxW, 1 / pxH);

    brightUniforms.tSrc.value = halfTargetB.texture;
    compositeUniforms.tSharp.value = sceneTarget.texture;
    compositeUniforms.tBlur.value = halfTargetB.texture;
    compositeUniforms.tBloom.value = bloomTargetA.texture;

    size.cssW = cssW;
    size.cssH = cssH;
    size.dpr = dpr;
  }

  function currentPixelRatio() {
    const dpr = typeof renderer.getPixelRatio === 'function' ? renderer.getPixelRatio() : 1;
    return dpr > 0 ? dpr : 1;
  }

  // Night is read from the scene haze, which world keeps equal to the sky
  // horizon: a warm day horizon (about 0.66 in linear luminance) and a deep
  // night one (about 0.03). That is the only per-frame colour state the
  // compositor needs, and reading it costs three property loads and no
  // allocation. A scene without fog grades as day.
  function nightAmount() {
    const fog = scene ? scene.fog : null;
    const col = fog ? fog.color : null;
    if (!col) return 0;
    const luma = 0.2126 * col.r + 0.7152 * col.g + 0.0722 * col.b;
    const k = (0.3 - luma) / 0.24;
    if (k <= 0) return 0;
    if (k >= 1) return 1;
    return k;
  }

  // One blur draw. Setting uniforms in place never allocates, and the four blur
  // passes run through this same call and the same program.
  function drawBlur(source, invX, invY, dirX, dirY, radius) {
    blurUniforms.uTex.value = source;
    blurUniforms.uTexel.value.set(invX, invY);
    blurUniforms.uDir.value.set(dirX, dirY);
    blurUniforms.uRadius.value = radius;
    quadMesh.material = blurMaterial;
    renderer.render(quadScene, quadCamera);
  }

  function setSize(w, h) {
    const cssW = w > 0 ? w : 1;
    const cssH = h > 0 ? h : 1;
    applySize(cssW, cssH, currentPixelRatio());
  }

  function setResolutionScale(scale) {
    const next = clampScale(scale);
    if (next === postScale) return;
    postScale = next;
    applySize(size.cssW, size.cssH, size.dpr);
  }

  function setBloom(on) {
    bloomOn = on !== false;
  }

  // Dream weight in 0..1, applied to the night bloom only: main sets it while the dream runs
  // (its own transition, so no per-frame write) and clears it when the district wakes. At 0 the
  // night bloom is exactly the authored night one; the day bloom never reads this value.
  function setDreamBloom(k) {
    let weight = typeof k === 'number' && isFinite(k) ? k : 0;
    if (weight < 0) weight = 0;
    else if (weight > 1) weight = 1;
    dreamWeight = weight;
  }

  // Distance grade weight in 0..1, scaling the authored distance amount only. The
  // key itself (height in the frame, minus the bright pixels) and both shade keys
  // keep their own amounts, so this one write cannot move the shade grade or the
  // street grade. 1 restores the authored grade exactly; a value out of range is
  // clamped, so a caller that overshoots at the far tier cannot invert the mix.
  function setDistanceGrade(k) {
    let weight = typeof k === 'number' && isFinite(k) ? k : 1;
    if (weight < 0) weight = 0;
    else if (weight > 1) weight = 1;
    compositeUniforms.uDistAmt.value = COOL_DIST_AMOUNT * weight;
  }

  function render() {
    if (disposed) return;

    const dpr = currentPixelRatio();
    if (dpr !== size.dpr) applySize(size.cssW, size.cssH, dpr);

    if (!api.enabled) {
      renderer.setRenderTarget(null);
      renderer.render(scene, camera);
      return;
    }

    renderer.setRenderTarget(sceneTarget);
    renderer.render(scene, camera);

    const night = nightAmount();
    compositeUniforms.uNight.value = night;
    // Daylight bloom keeps its measured 0.36. Night fades to a weaker glow so
    // the bloom cannot veil the street the sodium light is drawing, and the dream lifts it back
    // part way while it runs, because the chantier is the only lit thing in that frame.
    const dayBloom = BLOOM_STRENGTH * BLOOM_DAY_WEIGHT;
    const dream = night * dreamWeight;
    const nightBloom = BLOOM_NIGHT_STRENGTH + (BLOOM_DREAM_STRENGTH - BLOOM_NIGHT_STRENGTH) * dream;
    compositeUniforms.uBloom.value = bloomOn
      ? dayBloom + (nightBloom - dayBloom) * night
      : 0;
    // The night knee starts and saturates above the daylight one, so a merely
    // bright surface never reaches the bright pass after dark. The dream onset is lower, which
    // is what lets the warm wash under the road and the two lamp heads reach the pass at all.
    const nightThreshold = BLOOM_THRESHOLD_NIGHT
      + (BLOOM_DREAM_THRESHOLD - BLOOM_THRESHOLD_NIGHT) * dream;
    brightUniforms.uThreshold.value = BLOOM_THRESHOLD + (nightThreshold - BLOOM_THRESHOLD) * night;
    brightUniforms.uKnee.value = BLOOM_KNEE + (BLOOM_KNEE_NIGHT - BLOOM_KNEE) * night;

    // Radii are measured in post-chain texels, so a lower resolution scale shrinks
    // the buffers and the radii with them. The blur footprint stays the same on
    // screen and the knob only changes what the effect costs. Night pulls the
    // bloom radius in, which keeps the glow on the source instead of spreading
    // it over its neighbours; the dream widens it again so the glow reaches the works.
    const tiltRadius = BLUR_RADIUS * postScale;
    const nightRadius = BLOOM_RADIUS_NIGHT * (1 + (BLOOM_DREAM_RADIUS - 1) * dream);
    const bloomRadius = BLOOM_RADIUS * postScale * (1.0 - (1.0 - nightRadius) * night);

    // Tilt-shift: horizontal then vertical at half post resolution.
    renderer.setRenderTarget(halfTargetA);
    drawBlur(sceneTarget.texture, size.invHalfX, size.invHalfY, 1, 0, tiltRadius);

    renderer.setRenderTarget(halfTargetB);
    drawBlur(halfTargetA.texture, size.invHalfX, size.invHalfY, 0, 1, tiltRadius);

    if (bloomOn) {
      // Bright pass, then two separable passes at quarter post resolution.
      brightUniforms.tSrc.value = halfTargetB.texture;
      quadMesh.material = brightMaterial;
      renderer.setRenderTarget(bloomTargetA);
      renderer.render(quadScene, quadCamera);

      renderer.setRenderTarget(bloomTargetB);
      drawBlur(bloomTargetA.texture, size.invQuarterX, size.invQuarterY, 1, 0, bloomRadius);

      renderer.setRenderTarget(bloomTargetA);
      drawBlur(bloomTargetB.texture, size.invQuarterX, size.invQuarterY, 0, 1, bloomRadius);
    }

    compositeUniforms.tSharp.value = sceneTarget.texture;
    compositeUniforms.tBlur.value = halfTargetB.texture;
    compositeUniforms.tBloom.value = bloomTargetA.texture;
    quadMesh.material = compositeMaterial;
    renderer.setRenderTarget(null);
    renderer.render(quadScene, quadCamera);
  }

  function dispose() {
    if (disposed) return;
    disposed = true;
    renderer.setRenderTarget(null);
    sceneTarget.dispose();
    halfTargetA.dispose();
    halfTargetB.dispose();
    bloomTargetA.dispose();
    bloomTargetB.dispose();
    blurMaterial.dispose();
    brightMaterial.dispose();
    compositeMaterial.dispose();
    quadScene.remove(quadMesh);
    quadGeometry.dispose();
  }

  // Initial size comes from the renderer so the first frame is never 1x1 even if
  // the caller sets its size before calling setSize.
  if (typeof renderer.getDrawingBufferSize === 'function') {
    renderer.getDrawingBufferSize(bufferScratch);
    const dpr = currentPixelRatio();
    const cssW = Math.max(1, bufferScratch.x / dpr);
    const cssH = Math.max(1, bufferScratch.y / dpr);
    applySize(cssW, cssH, dpr);
  }

  const api = {
    enabled: settings.enabled === false ? false : true,
    render: render,
    setSize: setSize,
    setResolutionScale: setResolutionScale,
    setBloom: setBloom,
    setDreamBloom: setDreamBloom,
    setDistanceGrade: setDistanceGrade,
    dispose: dispose,
  };

  return api;
}
