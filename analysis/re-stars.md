# The sky's star field — `DomeInfo.Stars()` (issue #16, card STARS)

Both games, PS2. Tags as in the other RE notes: [O] read off the ELF / disc, [V] checked against a PCSX2 frame.

## 0. What the PS2 shows [V]

BF1 RHN2's night sky has a sparse field of 1–2 px white dots, some faintly warm or cool, over the dark sky and fading
into the blue haze: `engine/artifacts/pcsx2/ref/bf1_rhn2/walk/cp0_cp4_b090.png` / `_b180.png` (top of the frame). The
port drew none before this card.

## 1. The block and its loader [O]

The `Stars` key (fnv1a 0x622b0026) sits in `DomeInfo`. Every child key's preimage is a mod-tools sky-file name, pinned by
`mungefs::sky::tests::hash_preimages_are_confirmed`:

| key | hash | BF1 loader `FUN_002e6a50` / BF2 `FUN_003bfc30` | default |
|---|---|---|---|
| `NumStars` | 0x836bde54 | +0x138 = (uint) value; 0 = no stars, nothing built | 0 |
| `RandomSeed` | 0x69bf5463 | +0x13c = (uint) value | 0 |
| `TwinkleFactor` | 0xa435aaa4 | +0x150 | 0.5 |
| `TwinkleFrequency` | 0xa9eee423 | +0x154 | 20 |
| `Color` | 0x3d7e6258 | +0x14c..0x14f, each clamped to a byte (3 or 4 values) | a runtime global (BSS) |
| `StarTexture` | 0xda689440 | +0x15c, the name's hash; the draw functions of §3 do not read +0x15c | — |
| `BrightStarPercent` | 0x736837de | +0x140 = value × 0.01 | 0.08 |
| `AlphaMin` | 0x7de4197d | +0x144 = value × 0.003921569 | 0.05 |
| `ColorSaturation` | 0x0d7ec5c0 | +0x148 | 1.0 |
| `EnableBottom` | 0x02ff3675 | BF2 only: +0x164 = value ≠ 0 | 0 |

BF2's loader also reads 0x1ee51ed4 (degrees → +0x15c, through a sine) and a second key into +0x160: a horizon alpha
fade between the two. The four ground maps' dumps below carry neither; both default to −1, which never fades. Not ported.

Authored ground maps (`lead_tmp/re/refl/skykeys.py dump`):

| map | NumStars | Seed | Twinkle (factor, freq) | Color | Bright % | AlphaMin | Saturation | Bottom |
|---|---|---|---|---|---|---|---|---|
| BF1 KAS1 | 300 | 1 | 0, 0 | 255,255,255,255 | 30 | 5 | 0.2 | — |
| BF1 RHN2 | 2000 | 1 | 0, 0 | 255,255,255,255 | 10 | 10 | 0.2 | — |
| BF2 COR1 | 1000 | 1 | 0.5, 0.1 | 255,255,255,255 | 10 | 10 | 0.2 | 1 |
| BF2 NAB2 | 1000 | 1 | 0.5, 0.1 | 255,255,255,255 | 10 | 10 | 0.2 | — |

BF2 GAL1 and SPA_SKY also author the block (not ground maps).

## 2. The generator [O] (B1 `FUN_002e6e50`, B2 `FUN_003bffd8`, identical)

- 36 cells: 16 sectors of π/8 for sin(elevation) in [0, 0.5); 16 for [0.5, sin 60°); 4 quarter caps (π/2) for
  [sin 60°, 1]. A cell's share of `NumStars` is its share of the hemisphere's area: N/32, N·(sin 60° − 0.5)/16,
  N·(1 − sin 60°)/4. A running accumulator carries the rounding: acc += share; k = floor(acc + 0.5); acc −= k.
- The LCG x ← x·0x19660d + 0x3c6ef35f, seeded `RandomSeed ^ 0xc71c2830`. A random float from two steps a, b:
  `bits((a >> 16 | b & 0xffff0000) >> 9 | 0x3f800000) − 1`.
- Per star, eight steps: azimuth = cell·width + r(s1,s2)·width; e = lo + r(s3,s4)·(hi − lo);
  dir = (√(1−e²)·cos az, e, √(1−e²)·sin az) (`FUN_004453a8` writes sin then cos: its table is built by
  `FUN_00445208` from `FUN_00434218` then `FUN_00434028`, and `FUN_00434218` is the sine that drives BF1's mirror
  wobble, `analysis/re-sky-reflect.md` §2); colour from p1 = r(s5,s6), p2 = r(s7,s8) by `FUN_002e7bb8`:
  - brightness b = 1 if p1 > 1 − bright; else x = (p1/(1 − bright))³, b = x + AlphaMin·(1 − x);
  - R = G = B = b; p2 > 0.7: R ·= 1 − sat·(p2 − 0.7)·3.333; p2 > 0.85: G = b·(1 − 2·sat·(p2 − 0.85)·3.333);
    p2 < 0.4: B ·= 1 − sat·(0.4 − p2)·2.5; p2 < 0.26668: G ·= 1 − sat·(0.26668 − p2)·3.75; bytes = (int)(v·255),
    alpha byte 0xff.

## 3. The draw [O]

- Order: the sky tick (B1 `FUN_002f2308`) draws dome shell 0, then the stars (`FUN_002e7d78(sky+0x104, 0x20031)`),
  then shells 1.. (a cloud deck covers the stars). BF2 `FUN_003c10a8` the same.
- Per cell, a frustum test, then `FUN_002f33c8` (B2 `FUN_003ccb18`): each star a screen-aligned square of side
  `DAT_00518b28 (2.0) · w / (2 · near · 256)` along the camera's right and up = 1/256 of the screen width.
- The GS record `{0x100048, 0x31001, 0x48}` (`DAT_005d3250`): ALPHA 0x48 = Cs·As + Cd (ADDITIVE; the bolt glow's,
  `re-bolt.md`), TEST 0x31001 = Z test ALWAYS, PRIM 0x48 = no texture (the bolts' 0x58 sets TME). So untextured,
  added, no depth, no fog. Material colour = `Color` halved to GS scale; vertex colour = the star's bytes halved.
- Twinkle where `TwinkleFactor` > 0: t = the object's +0x158, which its update (vtable +0xc, `0x002e84a8`:
  `lwc1 $f0, 0x158(a0); add.s $f0, $f0, $f12; swc1`) advances by the frame time — seconds. k = (uint)(t·freq),
  f = t·freq − k; per cell two LCG streams seeded k ^ 0xc71c2830 and (k+1) ^ 0xc71c2830, two steps per star each;
  alpha = A²/255 · (1 − factor·((1−f)·rA + f·rB)·(1 − dir.y/2)).
- BF2 `EnableBottom`: each cell drawn a second time through the mirrored (−y) bounds and matrix.

## 4. The port (`render::stars`)

`mungefs::sky::Stars` → `manifest::ManifestStars` (loader defaults) → `render::StarField`; the renderer rebuilds the
stars with §2 and draws §3 in the sky pass after shell 0 through the bolt glow's additive shader. Deviation: the side
is kept at 1/192 of the window HEIGHT (= 1/256 of a 4:3 width), so a widescreen window does not grow the stars.
`EnableBottom` reaches the renderer only on a BF2 map. The PS2's 2048-entry sine table is replaced by `f32::sin_cos`
(≤ 1e-6 apart on the azimuths).

Check: `engine/artifacts/lead_tmp/stars/rhn2_zoom.png` (PS2 left, port right, the b090 pose): the same sparse white
dots with faint tints, at the same size and density. Star-for-star positions are not confirmed (the pose renders are
not pixel-registered).
