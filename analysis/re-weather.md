# OPUS RE (2026-10-05) - [O] UNAUDITED — Weather: precipitation (rain, snow, ash, embers, pollen), wind, rain shadows, lightning flashes, thunder and the sky bolt (BF1 and BF2)

Card WTHR (lane 2). The PS2 draws rain on both Kaminos and BF1 Yavin 2, snow on both Hoths and BF1 Rhen Var, ash on
BF2 Mygeeto, embers on BF2 Mustafar and pollen on BF2 Felucia; lightning flashes on both Kaminos and BF1 Yavin 2, with
bolts in the sky on the Kaminos. The port draws none of it (it darkens Kamino's dome between flashes, card KSKY). This
note is the whole system as the two ELFs run it, checked against PS2 memory and PS2 frames.

Scratch: `engine/artifacts/lead_tmp/wthr/` — `tools/` (census.py, regions.py, ram.py, ramcensus.py, predict.py,
flake_fade.py, lightning_ram.py, bolt_ram.py, bolt_census.py, bolt_predict.py), `vectors/` (three test vectors for
the build), `out/` (disassemblies, overlays), `agents/` (two readers' reports, re-checked here), `claims.md`,
`checks.txt`, `VERDICTS.md`, `verify/` (the verifier's scratch). Tags: [V] read (lines cited), [V-RAM] read out of PS2
RAM (PCSX2 save states, `engine/artifacts/pcsx2/ref/states/`), [V-PS2] measured on PS2 frames, [D] derived, [H]
hypothesis. B2 = `analysis/SWBFRNT2.ELF-decomp.c`, B1 = `analysis/SWBFRONT.ELF-decomp.c`; "VU 0x…" = a VU1 micro
address of BF2 (`scripts/tools/vu1dis.py`; the sprite overlay: `lead_tmp/audit-sund/verify/vuoverlay.py bf2
1899`). Names hash as FNV-1a-32 with every byte OR 0x20 (B2 `FUN_00539308`) — an underscore hashes as 0x7f, so
`fx_snow` is 0x6f1bd9e7 [V-RAM].

## 0. Answer

| what | the PS2 | § |
|---|---|---|
| which maps | Precipitation on 9 maps, Wind on the two Hoths and the two Kaminos, Lightning on BF1 KAM1 / YAV2 and BF2 KAM1, a sky bolt on the two Kaminos only | 1 |
| the particles | N = `ParticleDensity` (18–80) particles live in ONE unit cube; the cube is **tiled over the world in 5 m cells**: every cell near the camera shows the same N particles | 2 |
| motion | each falls at `Velocity` m/s plus the wind plus its own offset (within ±`VelocityRange` per axis); twice a second every offset, alpha and spin steps 20 % toward a new random extreme | 2.3 |
| a rain streak | **one GS line, 1 pixel of the 512-wide frame**, from the drop (the particle's alpha) up to where it was `StreakLength`/30 s ago as the moving player sees it (alpha 0); colour = `Color` applied twice: Kamino's (216,220,228) draws as **(91,94,101)** | 3.1, 3.4 |
| a flake | a camera-facing textured square, edge **2.83 × `ParticleSize`**, spun by its phase, at the particle's own alpha — **no distance fade** | 3.2, 3.4 |
| how far | cells whose centre is nearer than `Range` (12.5–20 m) along the view axis; a streak's alpha fades over the last 34 % and by dpx/d past the distance where `ParticleSize` is 2 pixels wide | 3.3 |
| under a roof | `regn` regions named `rainshadow…` (4–23 a map): a cell overlapping one in view is not drawn, and the walk then reaches 2.5 × further through shut cells | 4 |
| splashes | Kamino, Yavin 2: `GroundEffectsPerSec` splash effects a second per 0.65·spread² m² of the view's footprint, dropped by a ray onto terrain or buildings, none within 1 m of a rain shadow | 5 |
| wind | a wandering vector: glide to a new target near the base over 1/rate s, hold 0.5/rate s; nothing but precipitation reads it | 6 |
| a flash | groups of 2–5 flashes every 3–5 s; each lifts the sky dome and the FOG colour toward white for 0.2 s and rewrites sun and ambients — **BF1 and BF2 drive the sun in opposite senses** | 7 |
| thunder | one crack per group, one sub-crack per further flash; unpositioned; silent while every listener is inside a sound space | 7.4 |
| the bolt | per group one bolt 200 m ahead of the player (Y 200 → 0), and every 4–7 s one 800 m out (Y 300 → 100): a tapering additive ribbon, half-width 30 m, 0.5 s, with 2 branches and 1 flickering child | 8 |

## 1. The disc and what is live [V bytes][V-RAM]

`tools/census.py weather` walks every `fx__` chunk of every LVL of both discs (50 carry one); `tools/ramcensus.py`
reads the Precipitation and Wind objects out of one walk state per map (32 maps).

| LVL | Type | texture | ParticleSize | Range | Velocity | VelocityRange | density | AlphaMinMax | Color | StreakLength | RotationRange | CameraCross / Axial | GroundEffect, per s, spread | rainshadow regions |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| BF1 HOT1 | Quads | fx_snow | 0.015 | 15 | 2 | 0.8 | 80 | 0.3–0.45 | 255,255,255 | – | 2 | 1 / 1 | – | 4 |
| BF1 KAM1 | Streaks | – | 0.06 | 12.5 | 8 | 1 | 80 | 0.8–1 | 216,220,228 | 1.7 | – | 0.2 / 1 | rainsplash, 7, 8 | 15 |
| BF1 RHN1 | Quads | fx_snow | 0.015 | 20 | 3 | 0.8 | 64 | 0.3–0.45 | 255,255,255 | – | 2 | 1 / 1 | – | 9 |
| BF1 YAV2 (= YAV2_H) | Streaks | – | 0.02 | 12.5 | 9 | 1 | 50 | 0.6–0.9 | 216,220,228 | 1.5 | – | 0.2 / 1 | rainsplash, 8, 8 | 23 |
| BF2 FEL1 | Quads | fx_pollen | 0.015 | 15 | −0.3 | 1 | 18 | 0.5–0.9 | 215,255,225 | – | 25 | 0.5 / 1 | – | 0 |
| BF2 HOT1 | Quads | fx_snow | 0.015 | 15 | 2 | 0.8 | 80 | 0.3–0.45 | 255,255,255 | – | 2 | 1 / 1 | – | 4 |
| BF2 KAM1 | Streaks | – | 0.06 | 12.5 | 8 | 1 | 80 | 0.8–1 | 216,220,228 | 1.7 | – | 0.2 / 1 | com_sfx_rainsplash, 7, 8 | 17 |
| BF2 MUS1 | Quads | fx_ember | 0.025 | 15 | −0.3 | 1 | 20 | 0.5–0.9 | 255,255,255 | – | 25 | 0.5 / 1 | – | 11 |
| BF2 MYG1 | Quads | fx_silt | 0.03 | 15 | −0.05 | 1.25 | 50 | 0.3–0.45 | 220,220,230 | – | 2 | 1 / 1 | – | 9 |

- Every block authors `Enable(1)` and `ParticleDensityRange(0)`. A negative `Velocity` rises (embers, pollen, ash).
- `Effect("Wind")`: both Kaminos `Velocity(1, 0.3)`, `VelocityRange(1)`, `VelocityChangeRate(0.1)` (0xecec52a4);
  both Hoths `Velocity(4, 0.3)`, `VelocityRange(1)`, `VelocityChangeRate(0.2)`. No other map: its wind is zero.
- `Effect("Lightning")`: BF1 KAM1, BF1 YAV2 (and YAV2_H), BF2 KAM1 — the same numbers on all three but the two sound
  names (re-kamino.md §1). `LightningBolt("skybolt")`: BF1 KAM1 and BF2 KAM1 only. The discs' other two
  `LightningBolt` blocks are a weapon's (`arccaster_lightning` in BF1 COMMON.LVL, `com_sfx_lightning` in BF2 INGAME.LVL).
- Textures: `fx_snow` is in BF1 COMMON.LVL and BF2 INGAME.LVL, `fx_pollen` / `fx_ember` / `fx_silt` in their map's
  LVL, the bolt's `lightning` in each KAM1.LVL. The splash effects are particle effects the port already bakes
  (`common_bf2/effects.ron` `com_sfx_rainsplash`, `common_bf1/effects.ron` `rainsplash`).
- **Live** [V-RAM]: Precipitation is enabled (byte +0x1c) on exactly those nine maps with every number of the table in
  its fields (§2.1) and a tile size of 5.0 on all nine; every other map holds the ctor's defaults, disabled. Wind is
  enabled on the two Hoths and the two Kaminos only. BF1 RHN2 has seven rainshadow regions and NO precipitation (the
  card's "flakes on RHN2" is not this effect). No mission script was searched for a switch [H].
- `Effect("ColorControl")` (`WorldBrightness` 0x46c70d98, `WorldContrast` 0xb8fa74ad, `WorldSaturation` 0xcf985717;
  BF2 KAM1 0.46 / 0.48 / 0.65) is read by neither ELF: the three hashes are in no decompiled line, no instruction
  immediate and no data word (`lead_tmp/re/koc/hashscan.py`); only the effect's name 0x952bd04d is. Nothing to port.

## 2. Precipitation: the object and its particles [O]

BF2: singleton `DAT_0060139c` (200 bytes), ctor `FUN_00462928` (524748), vtable 0x63a798 — +0x18 render
`FUN_00475740`, +0x2c update `FUN_004637f0`, +0x30 parse `FUN_00462c70` (ELF words). BF1: `DAT_00519514`, ctor
`FUN_0037df90` (370041), vtable 0x5496f8 — render `FUN_00390af8`, update `FUN_0037eeb0`, parse `FUN_0037e358`.
**Every BF1 field is at the BF2 offset + 0xc** [V ctor, parser, RAM].

### 2.1 Fields (BF2 offsets) [V 524748–524812, 524885–525077][V-RAM all nine maps]

| off | key (hash) | default | note |
|---|---|---|---|
| +0x1c | `Enable` (0xaf8bb8ce) | 0 | `FUN_00451678` (513114): stores the byte and joins / leaves the update and render lists; `FUN_004516e0` updates enabled effects only (513153–513154) |
| +0x44, +0x48 | – | **5.0**, 0.2 | the tile (cell) size in metres and its inverse; no key writes them |
| +0x4c | `Texture` (0x3c6468f4) | 0 | the name's hash |
| +0x50 | `Type` (0x5127f14d) | 1 | 1 when the value hashes to 0x4a59a280 ("Streaks"), else 0 (Quads) |
| +0x54 | `ParticleSize` (0x55e41b16) | 0.02 | |
| +0x58 | `Range` (0xfadc0cd2) | 10 | |
| +0x5c | `Color` (0x3d7e6258) | (192,192,192,160) | 4 bytes; alpha 255 when the block gives 3 numbers (all nine maps) |
| +0x60, +0x64 | `AlphaMinMax` (0x220c0c21) | 0.5, 0.5 | one argument sets both |
| +0x68, +0x6c | `ParticleDensity` (0xc393fff1), `ParticleDensityRange` (0x2214ef7e) | 30, 0 | the particle COUNT; the range is 0 on every map |
| +0x70, +0x74 | `Velocity` (0x32741c32), `VelocityRange` (0x5469303f) | 10, 1 | fall speed in m/s, down positive |
| +0x78 | `StreakLength` (0xb16a1adb) | 1/30 | stored × 0.033333335: the streak's length in SECONDS of travel (525026; B1 370250) |
| +0x7c, +0x80 | `CameraCrossVelocityScale` (0x8f369c79), `CameraAxialVelocityScale` (0x1c4564ca) | 1, 1 | |
| +0x84, +0x88 | `Rotation` (0x21ac415f), `RotationRange` (0xf2617b74) | 0, 2 | no map authors `Rotation` |
| +0x8c, +0x90 | – | 0.4, 0 | the re-roll clock's rate (no key) and the clock |
| +0x94, +0x98, +0x9c | – | – | the current density (what is drawn), the particle count, the particle array (0x30 bytes each) |
| +0xa0 | `GroundEffect` (0x51689473) | 0 | the particle-effect class of that name (`FUN_00539150(0x71fc88, 0x200, hash)`) |
| +0xa4 | `GroundEffectsPerSec` (0x885ac868) | 10 | **BF2 divides by the view count at parse** (525031); BF1 stores it raw (370276) |
| +0xa8 | `GroundEffectSpread` (0xd23d97a4) | 20 | |
| +0xac, +0xb0 | – | 0 | a splash timer per view |
| +0xb4.. | – | – | the rain-shadow region list (first node +0xb8, count +0xc4) |

### 2.2 Setup `FUN_004630f8` (525079; B1 `FUN_0037e878` 370400) [V]
count = (int)(density + density range). Per particle (525107–525216): **position = three uniform randoms in [0, 1)**;
velocity offset = (rand − 0.5)·2·`VelocityRange` per axis; alpha uniform in [`AlphaMin`, `AlphaMax`]; Quads only:
spin rate = `Rotation` + `RotationRange`·(rand − 0.5), phase = rand. The randoms are the engine's one LCG
(`DAT_0073d868`; BF1 `DAT_0066eec8`): x ← x·0x19660d + 0x3c6ef35f, two steps per float. The port must use a stream of
its own, not the sim's. (BF2 runs the setup at the end of the parse, 524923–524926; BF1 in the first update.)

### 2.3 Update `FUN_004637f0(dt, this)` (525223; B1 `FUN_0037eeb0` 370536) [V]
1. **The re-roll clock** (525290–525386): c ← c + 0.4·dt; when c > `DAT_006013a0` (**0.2**, ELF word, no writer; B1
   `DAT_00519518` 0.2): c is capped at 1 (BF2 only), then with factor c — the density moves toward density ± range;
   every velocity offset toward (±1, ±1, ±1)·`VelocityRange` (three random signs); every alpha toward `AlphaMin` or
   `AlphaMax` (one random bit); every Quad's spin rate toward `Rotation` ± `RotationRange`/2 — and c ← c − 0.2. So
   every 0.5 s each particle steps ~20 % toward a new random extreme.
2. **Motion** (525389–525438, disassembly checked): v = (wind.x, −`Velocity`, wind.z), wind = the Wind object's
   current vector (§6); position ← frac(position + (v + offset)·dt / tile) per axis (the code adds 1000 before taking
   x − (int)x). The cube is the world modulo 5 m: a particle at unit position u stands at (cell index + u)·5 m.
3. Quads: phase ← phase + rate·dt, kept as x − (int)x (it runs in (−1, 1) for a negative rate) (525440–525451).
4. The splashes (§5).
The camera does not enter the motion: the particles are world-anchored. The two `Camera…VelocityScale` keys only shape
a streak (§3.1); the Quads path reads neither.

## 3. Precipitation: what is drawn [O]

Render `FUN_00475740` (534978; B1 `FUN_00390af8` 380441). It packs ONE batch of the N particles in the cube's own
frame (u·tile − tile/2) and draws that batch once per cell, moved to the cell's centre.

### 3.1 A streak (Type 1) [V 535200–535530, vector code disassembled]
- camvel = the velocity of the entity the view's camera tracks: vtable +0x20 of the view's tracker
  (`*(DAT_005fc4ec + 0x24 + 4i)`, 535213–535216) = `FUN_001bc018` (64970) for the player camera [V-RAM vtable
  0x617d18]: the vector behind vtable +0x4c of the tracked entity, zero without one [D that it is the velocity].
  When either scale ≠ 1 it is taken into camera space, x and y × `CameraCrossVelocityScale`, z ×
  `CameraAxialVelocityScale`, and back (535218–535247).
- fall = (wind.x, −`Velocity`, wind.z) (535253–535255); **streak = −(fall − camvel)·S**, S = `StreakLength`/30
  (535256–535261), shortened to one tile if longer (535273–535286).
- Per particle (535459–535521): vertex A = the particle, colour `Color` RGB with **alpha = the particle's alpha**;
  vertex B = A + streak − offset·S, colour `Color` RGB with **alpha 0** (535518). Kamino standing still: 8.1 m/s ×
  0.0567 s = **0.46 m**; Yavin 2 0.45 m.
- A `side` vector normalize(row(+0x50) × fall) is built (535430–535455) and overwritten before any call: dead.

### 3.2 A flake (Type 0) [V 535108–535199]
Per particle one packed vertex — the position, and a 4th word (int)(`ParticleSize`·256) (535171) — one float
2·phase − 1 (535177) and one colour (`Color` RGB, alpha = the particle's). The texture is bound by its hash
(`FUN_00584a90(+0x4c)`, 535548).

### 3.3 The cells and their alpha — no rain shadow in view [V 535557–535733]
- `FUN_0057b850` (711660): near = camera +0x130, W = the near plane's width = (+0x138)·2·near / zoom (+0x140).
- **dpx = `ParticleSize`·near·256 / W = `ParticleSize`·128 / tan(fov_h / 2)** (535559): the distance at which
  `ParticleSize` spans 2 pixels of the 512. With the walk states' cameras [V-RAM: tan 0.637 in BF2, 0.445 in BF1]:
  BF2 KAM1 12.06 m, snow / pollen 3.01, MYG1 6.03, MUS1 5.02; BF1 KAM1 17.25, YAV2 5.75, the snows 4.31.
- far = `Range` + near·(zoom − 1) (535570) — `Range` at zoom 1. The cells are the index box floor(corner / tile) of
  the camera frustum's bounding box for (near·zoom, far) (535572, 535602–535610; the box at the camera copy's +0x180 /
  +0x190, `FUN_0057ac68` [D: apex and far corners]), walked near side first on each axis (535613–535633).
- Per cell (535655–535721): centre c = (index + 0.5)·tile, depth d = (camera position − c)·row(+0x50). Drawn when
  d < far and the cell's box is not outside the frustum (`FUN_0057beb8` ≠ −1; 711997: −1 wholly outside a plane, 0
  crossing the near plane or the 7× guard planes — the batch then gets the clip flag 0x100 — else inside) with
  **a = (dpx / d if d > dpx, else 1) × ((`Range` − d) / (0.34·far) if d > 0.66·far)**; the batch's colour word is
  `Color` RGB with alpha = `Color`.a·a (535701, 535712–535714), its matrix the identity moved to c (535710).
  (Zoomed, far > `Range` and the last factor goes negative: the byte wraps [V decompile only].)

### 3.4 The low-level draw [V: decompile, VU1 microcode, ELF and RAM words; `agents/AGENT_C_draw.md` re-checked]
- **Streaks are GS lines.** `FUN_00496040` (554482) is the engine's line drawer: its batch header's 4th word is 2
  (554565) where the triangle-strip emitters write 4; VU 0x481 / 0x486 store it and VU 0x4c8 / 0x4db–0x4e0 OR it into
  the pass's PRIM: **primitive 2, LINE STRIP**. It emits every vertex twice (554580–554587) so the generic strip code
  (no kick on a chunk's first two vertices) yields exactly one line per streak, A → B. **No width is computed and
  `ParticleSize` is not in the vertex data**: a streak is 1 pixel of the 512 × 448 frame, Gouraud from the head's alpha
  to 0. `ParticleSize` only sets dpx.
- **A streak's colour** (VU 0x2e6–0x2f5, 0x445–0x448): GS byte = ⌊h_vertex·h_cell / 128⌋ for RGB and A alike, both
  inputs already h = (c + 1) >> 1. Untextured, the GS shows that byte as is, so RGB is `Color` applied twice and tops
  out at 128: **Kamino's (216, 220, 228) draws as (91, 94, 101)**; alpha (128 = opaque) = particle alpha × cell alpha.
  [V-PS2] at the predicted streak heads of three BF2 KAM1 frames the pixel median is (80–82, 89–90, 95–97) over a
  (34–39, 44–47, 47–52) background — (91, 94, 101) at α ≈ 0.85; "applied once" (108, 110, 114) would read ~15 higher.
- **Flakes run the generic particle sprite overlay** (`FUN_004966f0` 554715, trailer op 0x758 at 554918; MPG slot
  1899, the one `FUN_00595020` uses for PARTICLE; re-fx.md §6.1) with their own packing: the size word is ×256 (the
  generic path's ×128 and its 0.00314 rotation scale are EE-side, 727046–727047, and do not apply), so h =
  ⌊`ParticleSize`·256⌋ / 256 (VU 0x7a3, 0x7c2) and the sprite is a screen-aligned square of half-width √2·h — **edge
  2.83·h**: 3.3 cm for the snows and pollen (h 3/256), 6.6 cm embers (6/256), 7.7 cm ash (7/256); perspective-sized,
  no minimum pixel size; angle = π·frac(2·phase − 1), one turn per unit of phase (VU 0x7c3–0x7ce, 0x7e3–0x7ec);
  UV (0,0)–(1,1).
- **A flake's colour is its own stream only**: with a colour stream the overlay never loads the per-cell colour (VU
  0x776–0x77c branch over 0x77d–0x781; 0x7bd, 0x7ec–0x7ef). So §3.3's alpha — the distance fades and `Color`.a — does
  **not** reach a flake: it is drawn at `Color` RGB (once) × texel and the particle's alpha × texel alpha in every
  drawn cell, and pops with its cell at `Range`. [V-PS2] `tools/flake_fade.py` on BF2 MUS1: the frame's dot lift at
  the predicted embers is level with depth (p75 90 at 7–9 m, 72 at 11–13 m, where §3.3 would give 0.63 and 0.25).
- **GS state** (records written once by `FUN_00477240` 535894–535903; B1 `FUN_00392688` 381396–381403, same values):
  streaks 0x720bf0 = {0x100044, TEST 0x51001, PRIM 0x48}; textured flakes 0x720c00 = {0x100044, TEST 0x53001, PRIM
  0x58}. ALPHA 0x44 = (Cs − Cd)·As + Cd (ordinary alpha blend); TEST = alpha test NEVER with AFAIL FB_ONLY / RGB_ONLY,
  depth GEQUAL: **depth tested, never written**; PRIM has no FGE: **not fogged**, no line antialiasing. Flake texture:
  MODULATE with texture alpha, bilinear, REPEAT (HOT1's `fx_snow` is 32 × 32 PSMT4 [V-RAM]).
- The effect renders at priority 3 of the renderable list (`FUN_00563900(this, 3)`, with the other effects; the sky is
  0); weather and particles share one texture bucket list (0xc; BF1 0xd). Order inside it: not traced [H].
- BF1: the same draw at shifted addresses — line emitter `FUN_00399ed0` (header w 2, 386902–386905), sprite emitter
  `FUN_0039ac40`, overlay slot 1889 (155 of 168 words equal BF2's, the rest addresses), the same records. No rule
  difference found.

## 4. Rain shadows [O]

### 4.1 The regions [V disc][V-RAM]
The Precipitation ctor registers a region class named "rainshadow" (the string at 0x63a750; B1 370058). A world `regn`
joins a class when its NAME begins with the class's name: `FUN_005ae3b8` (744916) compares over the class name's
length (744940–744942) — `rainshadow0`, `rainshadow14` and `rainshadow` all match [V-RAM: the live lists hold every
such region — BF2 KAM1 17, HOT1 4, MUS1 11, MYG1 9; BF1 KAM1 15, HOT1 4, RHN1 9, YAV2 23]. `tools/regions.py all`
lists them. The live region is the engine's ordinary one (rows +0x00..+0x20, centre +0x30, size +0x40, kind +0x54;
re-cp-capture.md §5, `core/src/region.rs`) plus an "in view" byte at +0x70 (BF1 an int at +0x80). Kinds in RAM:
- both Kaminos and BF2 MYG1: all **kind 7** — an upright cylinder, radius √(SIZE.x² + SIZE.z²), half-height SIZE.y,
  its yaw dropped;
- the Hoths: two **kind 5** (axis-aligned boxes) and two kind 7; BF1 RHN1 nine kind 5; BF1 YAV2 21 kind 5 and two
  kind 2; BF2 MUS1 eight kind 5 and three **kind 2** (boxes with a real rotation).

### 4.2 Which cells are dry — the walk with a rain shadow in view [V 535574–535886]
- Before the cells every region's "in view" byte is set by a frustum test against the (near·zoom, far) camera copy
  (`FUN_00464db8` 525921, at 535580). **When at least one is in view** (535586–535594) the walk changes: far2 =
  2.5·far, the frustum becomes (near, far2), and the fades use far2.
- The cells of the (near, far2) index box are walked y layer by y layer, x row by x row, z cell by z cell, near side
  first, carrying a **pseudo-distance v** instead of the depth. With (ax, ay, az) = tile·|row(+0x50)| per axis and d0
  the depth of the first cell, a y layer starts from dy = d0 + (layers walked)·ay; for the cell at x step i, z step k,
  with `up` = the cell at (i − 1, k) and `prev` = the cell at (i, k − 1) — before the first of each: an open cell at
  dy + 2.5·(k·az − ax), resp. dy + 2.5·(i·ax − az) — (535763–535808):

  | prev | up | v |
  |---|---|---|
  | open | open | max(up.v + 2.5·ax, prev.v + 2.5·az) |
  | open | shut | up.v + ax |
  | shut | open | prev.v + az |
  | shut | shut | min(up.v + ax, prev.v + az) |

  A cell with v < far2 whose box is in the frustum is drawn — **unless it overlaps a region that is in view**
  (`FUN_00537208(region, centre, half = tile/2)`, 535835–535839): then it is marked shut (535842) and not drawn. A
  drawn cell's alpha is §3.3's with v for d and far2 for both `Range` and far (535846–535852). An x row with no
  v < far2 ends the layer.
- What that does: in the open a step costs 2.5 × its depth, so rain still ends near `Range` — but the dpx term bites
  2.5 × sooner (Kamino: streaks dim from ~4.8 m instead of 12 m whenever a shadow is in view). Through shut cells a
  step costs 1 ×, so from inside a hall the rain past the doorway is drawn out to as much as 2.5 × `Range`.
- The overlap test `FUN_00537208` (666566; B1 `FUN_004c3b00` 585406, the same cases) by kind: **5** two axis-aligned
  boxes, |Δ| < half + size on all three axes; **7** an upright cylinder: apart when |Δy| ≥ half + size.y or |Δx| ≥
  half + r or |Δz| ≥ half + r, touching when |Δx| < half and |Δz| < half, else when (|Δx| − half)² + (|Δz| − half)² <
  r² (6 and 8: the same about x and z); **1** a sphere against the box; **4** a plane (666734–666752); **2, a rotated
  box, is NOT an overlap test**: per world axis the region's projected half-size ALONE against |Δ|, per region axis
  the cell's projected half-size ALONE against the local |Δ| (0x537450–0x537510) — true only for cells round the
  region's centre, so the rotated shadows (3 on BF2 MUS1, 2 on BF1 YAV2) dry almost nothing. Kinds 3 and ≥ 9: false.

## 5. Splashes (`GroundEffect`) [O]

BF2 `FUN_004637f0` 525456–525888; BF1 `FUN_0037eeb0` 370776–371230. Only with a `GroundEffect` that exists (the two
Kaminos, BF1 Yavin 2). Per view i:
- d = `GroundEffectSpread` + 3 (525478). Box B = the axis-aligned bounds of the camera position and the four points
  pos − Z·d ± X·0.577·d ± Y·0.433·d (X, Y, Z = the camera's rows +0x30, +0x40, +0x50; it looks down −Z)
  (525494–525790, disassembled): the footprint of a 60° × 46.8° view out to d.
- **interval = 0.65·spread² / (rate · B.Δx · B.Δz)** s (525792–525793), rate = `GroundEffectsPerSec` / views in BF2,
  `GroundEffectsPerSec` in BF1 (370788). Kamino looking level: B ≈ 12.7 × 11 m → ~23.5 splashes a second.
- The view's timer += dt; while it exceeds the interval one point is tried and the interval subtracted
  (525810–525875): x, z uniform in B; a ray from y = camera.y + 50 straight down 100 m (`FUN_00161c60(100, p, (0,−1,0),
  …, 0x90, 1)`, 525832–525835 — the ordnance ray's mask, terrain | buildings, re-collision-layers.md; B1
  `FUN_00161d68` with 0x48, 371170); the splash goes to the hit. It is dropped when a **1 m sphere at that point
  touches a rain-shadow region that is in view** (`FUN_00536f10(1.0, region, point)`, 525838–525845). Else one
  instance of the effect is taken and started there (525851–525857); with the effect's pool empty the timer is
  wrapped and the view's loop ends (525853–525855).
- For a second view, a point inside the first view's box with the cameras within 50 m in height is skipped
  (525862–525873).
- The "in view" bytes read here are the render's (§4.2), with a second writer: when the interval is already under
  the timer before dt is added (525797), every region's byte is set from its overlap with B (`FUN_004650c0` 526118;
  B1 `FUN_00380840`).

## 6. Wind [O]

BF2: singleton `DAT_00601394`, ctor `FUN_00461e00` (524361), vtable 0x63a818. BF1: `DAT_0051950c`, ctor
`FUN_0037d268` (369615), vtable 0x549778.
- Parse `FUN_00461ee0` (524411; B1 `FUN_0037d348` 369665): `Velocity(a, b)` → base = (a, 0, b) at +0x30;
  `VelocityRange` → +0x40; `VelocityChangeRate` → +0x44; at the block's end the clock +0x48 = 0 and "from" (+0x50),
  target (+0x60) and current (+0x70) = base.
- Update — vtable +0x2c = 0x461ff8, **not in the decompile** (a hole, 0x461ff8–0x4623ec; B1 0x37d570); from the
  disassembly (`out/b2_wind_update.asm`) [V asm]:
  ```
  t = clock + rate * dt;  clock = t
  if t < 1.0:   current = from + (target - from) * t                 # a straight line
  else:         current = from = target
                if t > 1.5:                                          # hold, then a new target
                    L = |base|;  R = VelocityRange
                    if L <= 0.01 * R:  target = base + (a, 0, b) * R                       # a, b uniform in [-1, 1)
                    else:              target = base + base * (R / L) * a
                                                + (base.z, 0, -base.x) * (R * R / ((R + L) * L)) * b
                    clock = 0
  ```
  Kamino: a 10 s glide and a 5 s hold, 1.04 ± 1 m/s along the base and up to ± 0.49 across; Hoth: 5 s and 2.5 s,
  4.01 ± 1 along and ± 0.2 across.
- [V-RAM] `bf2_kam1_walk_cp0_cp1_b000`: from (1.2584, 0, 0.6926), target (1.1839, 0, 0.6062), clock 0.8667 →
  current (1.1938, 0, 0.6177) = from + (target − from)·clock to four decimals. `bf2_hot1_walk_cp0_cp5_b000`: clock
  1.02, from = target = current (the hold).
- BF1's update holds the same floating-point and VU operations (61 of 61; the first 53 instructions identical) [D].
- **No other reader** in either game: every load of the singleton is in its own make / unmake and the Precipitation
  update and render (BF2 0x463ea8, 0x47596c; BF1 0x37f534, 0x390d5c; BF1's three accessor stubs have no caller).
  The wind moves rain and flakes and tilts the streaks; flags, foliage and dust do not read it.

## 7. Lightning: the flashes [O] (`agents/AGENT_B_lightning.md`, re-checked; the dome part is re-kamino.md §1)

BF2: singleton `DAT_006012e8`, vtable 0x63a008, tick `FUN_0045ae80(dt, L)` (519989), parser `FUN_0045aa30`. BF1:
`DAT_005194c4`, tick `FUN_00378120` (366839), parser `FUN_00377c40`. Fields (BF2; BF1's differ, §7.3): colour floats
+0x24, `SunlightFadeFactor` +0x34, `SkyDomeDarkenFactor` +0x38, `BrightnessMin` +0x3c, `FadeTime` F +0x40, flash gap
+0x44 / +0x48, sub-flash gap +0x4c / +0x50, count +0x54 / +0x58, t +0x64, next +0x68, flashes left +0x6c, B +0x80,
sunlum +0x84, the saved lights +0x8c.., saved fog colour +0xe0, sound handles +0xe4 / +0xec, bolt timer +0xfc with its
interval +0xf4 / +0xf8 = **4.0 / 7.0 from the ctor — not a key** (519761–519762). BF1 [V-RAM by `lightning_ram.py`]:
`Color` one u32 +0x24, the four factors +0x28 / +0x2c / +0x30 / +0x34, flash gap +0x38 / +0x3c, sub-flash gap +0x40 /
+0x44, count +0x48 / +0x4c, t +0x58, next +0x5c, left +0x60, B +0x80, sunlum +0x84, saved sun and ambients as three
u32 +0x8c / +0x90 / +0x94, saved fog +0xd0, sounds +0xd4 / +0xdc, bolt interval +0xe4 / +0xe8, timer +0xec.

### 7.1 The schedule [V 520079–520144]
```
t += dt
if next < t:
    t = 0
    if left < 1:                                   # a new GROUP
        left = (int)(NumMin + frand * ((NumMax - NumMin) + 0.9999))      # 2..5 on the three maps
        play SoundCrack ;  bolt timer = 0          # so every group spawns a bolt this tick (section 8.1)
    left -= 1
    B = BrightnessMin + frand * (1 - BrightnessMin)                      # 1 on the three maps
    if left < 1:  next = uniform(TimeBetweenFlashesMinMax)               # 3..5 s to the next group
    else:         next = uniform(TimeBetweenSubFlashesMinMax) ; play SoundSubCrack      # 0.01..0.5 s
```
After the parse: t = 0, next = the flash gap's max, left = 0. A group of N flashes plays one crack and N − 1
sub-cracks. `frand` is the engine's one LCG (§2.2).

### 7.2 The fade [V 520152–520313]
Every tick with t < F: b0 = B·(1 − t/F), **b = b0 + (frand − 0.5)·(1 − b0)·b0** (a flicker, largest at half
brightness); the dome (sky shell 0) bytes = white·(1 − f) + black·f + 0.5 with **f = `SkyDomeDarkenFactor`·(1 − b)**,
and **the fog colour is set to the same bytes**; the lights are rewritten (§7.3). On the one tick where t passes F the
sun, the ambients and the fog colour are restored from the copies taken on the effect's first tick and the dome
returns to f = `SkyDomeDarkenFactor` (153). A sub-flash arriving inside a fade restarts it; nothing is restored in
between. The flash direction drawn from `HorizonAngleMinMax` and a random azimuth (520094–520105) is stored at L+0x70
and read nowhere; the tick writes no light's direction and never touches the second directional light [V].

### 7.3 The lights — the two games differ [V asm both][V-RAM]
k = 1 − `SunlightFadeFactor` (0.9), s = sunlum·(t/F).
- **BF1** (byte colours: sun `_DAT_0069c588`, ambients `_DAT_0069c570` / `_DAT_0069c578`; sunlum = the saved sun's
  (r + g + b) / 765 — 0.333 on KAM1, 0.298 on YAV2): while s < b: **sun = `Color`·(b − s)**, each ambient =
  k·(saved + 0.333·saved sun). Once s ≥ b: g = (s − b)/sunlum, sun = saved sun·g, ambients = k·(saved + 0.333·(1 − g)·
  saved sun) (367058–367283). The flash light starts at full `Color` and dies; the real sun then climbs back.
- **BF2** (float colours: Light1 `*DAT_0060c6e0` +0x1c, the ambient object `*DAT_0060c6e8` +0x1c / +0x50 top / +0x60
  bottom): the same ambient terms, but **sun = `Color`·(1 − (b − s))**, the complement (520182–520246) — and BF2's
  sunlum is (uint)(int(r) + int(g) + int(b)) / 765 of 0..1 FLOATS (520071–520074): **0** for any real sun, so s = 0
  and the other branch (sun = saved sun·(1 − g), 520248–520288; it would divide by zero) never runs. BF2's
  directional light is therefore BLACK at a flash's peak and climbs to white `Color` as the dome fades, then snaps
  back; what flashes in BF2 is the dome, the fog colour and +10–24 % of ambient.
- [V-RAM] (`tools/lightning_ram.py`): `bf2_kam1_cw_sideselect` (t 0.0667, dome 224 → b 0.696): sunlum 0.0, live sun
  (0.3061, 0.3061, 0.3061) = `Color`·(1 − b); ambient (0.4082, 0.4317, 0.4552), top (0.4293, 0.4528, 0.4763), bottom
  (0.3870, 0.4105, 0.4340) = the formulas to four decimals; fog = dome = 224. `bf1_kam1_cw_sideselect` (dome 193 →
  b 0.392, s 0.167): sun (57, 57, 57) = `Color`·(b − s); ambients (109, 115, 121) and (149, 154, 176) exact.
  `bf1_kam1_walk_cp1_cp10_b000` (t 0): sun 255, dome 255. At rest the live lights equal the saved ones.
- B (+0x80) is written by neither ctor nor parser, and the fade branch runs for the first F seconds of a level before
  any flash: what it holds then is not read [H].

### 7.4 Thunder [V]
`FUN_0045ae00(handle)` (519963; B1 `FUN_00378098` 366811), called only from the two places of §7.1: when one of the
four listener slots (0x73d990, 0x50 each) is active and its sound space (+0x40) is the default one (`&DAT_00744a10`) it
plays `FUN_0043ceb0(handle, 0, 0, 0, 1)` — **no position, no delay, no distance rule**; 2D or 3D, gain and pitch are
the sound definition's own (`kam_amb_thunder` / `kam_amb_thundersub`, `yav_amb_…`; not opened [H]). So thunder is
silent while every listener stands inside a sound-space region.

## 8. The sky bolt [O] (`agents/AGENT_B_lightning.md`, re-checked)

### 8.1 Spawn [V 520317–520412][V-RAM]
```
bolt timer -= dt ;  if bolt timer >= 0: return
cls = the effect class named "skybolt" (hash 0x559a2771) ;  if cls and an instance is free:
    v   = normalize((frand - 0.5, 0, frand - 0.5) - back.xyz, then y := 0.01)     # back = camera row +0x50, view 0
    top = eye + v * dist ;  top.y = height                                        # an ABSOLUTE world height
    foot = top + ((frand - 0.5) * 20, -200, (frand - 0.5) * 20)                   # DAT_006012ec = 200.0, no writer
bolt timer = 4 + frand * 3
```
(dist, height) = **(200, 200) on the tick a group started** (520087–520088), else **(800, 300)** (520080–520081). So
each group throws a bolt 200 m ahead of where the player looks (within ±27° per axis for a level camera), Y 200 → 0,
and between groups one appears every 4–7 s at 800 m, Y 300 → 100, with no flash and no thunder. The top is fixed in
the world. Without the class (BF1 YAV2) nothing else changes. The instance pool ("LightningBoltEffectObject", 16 by
default) is shared with the weapon bolts.
[V-RAM] `tools/bolt_census.py` over 32 KAM1 / YAV2 states: four live bolts — two at a horizontal 200.0 m, top 200.0,
foot 0.0 (mid-flash BF1 KAM1 states), two at 799.8–799.9 m, top 300.0, foot 100.0 (one BF1, one BF2, no flash
running); no skybolt class in any YAV2 state. [V-PS2] the BF1 far one is on screen exactly where RAM puts it
(`out/farbolt_bf1_kam1_cp0_cp2_beta1_spawn.png`): **the 800 m bolts are drawn** (BF2's: off screen in its one state [H]).

### 8.2 The class and the instance [V 518577–518826, 518946–519140]
Class fields (BF2; BF1 4 lower), defaults → skybolt: `Color` +0x48 (255,255,255,255), `ChildColor` +0x4c
(255,255,255,150), `Texture` +0x50, `Width` +0x54 2 → 30, `FadeTime` +0x58 1 → 0.5, `SpreadFactor` +0x5c 1 → 20,
`InterpolationSpeed` +0x60 10 → 0.4, `BranchFactor` +0x64 0.3 → 0.5, `BranchSpreadFactor` +0x68 0.8 → 8,
`BranchLength` +0x6c 4 → 80, `MaxBranches` +0x70 5 → 2 (≤ 10), `BreakDistance` +0x74 2 → 20, `TextureSize` +0x78 5 →
30, `ChildBreakDistance` +0x7c 1.5 → 15, `ChildTextureSize` +0x80 3 → 8, `ChildWidth` +0x84 0.3 → 1,
`ChildSpreadFactor` +0x88 1 → 10, `NumChildren` +0x8c 3 → 1 [V-RAM: the live class holds these].
Instance: start +0x70, end +0xd0, life +0xe0 (starts at `FadeTime`), seeds A, B +0xf8 / +0xfc (two raw LCG draws at
creation), phase +0x100, intensity +0x104 (1; only the weapon changes it). Update `FUN_004594f0`: life −= dt (dead at
≤ 0); phase += `InterpolationSpeed`·dt; past 1: A = B, phase −= 1, B = a new draw. The sky bolt lives 0.5 s, so its
phase only reaches 0.2: one shape drifting a fifth of the way to a second.

### 8.3 Render and shape [V 519192–519611]
- `FUN_00459898`: fade = min(1, 2·life/`FadeTime`) (full for the first half, then linear to 0); the main strip
  (`Width`, `SpreadFactor`, phase, `BreakDistance`, `TextureSize`, seeds A, B, `Color` with alpha fade·a); then
  `NumChildren` child strips (`ChildWidth`, `ChildSpreadFactor`, phase 0, `ChildBreakDistance`, `ChildTextureSize`,
  `ChildColor`) whose two seeds are FRESH draws of the engine LCG on every render call (519288–519310): the child
  flickers every frame.
- `strip` = `FUN_00459c50` (B1 `FUN_00376a98`):
  ```
  A = seedA ^ 0xc71c2830 ;  B = seedB ^ 0xc71c2830           # two private LCG streams, the engine's constants
  D = (end - start) / len ;  n = max(3, (int)(len / BreakDistance)) ;  if n >= 101: draw nothing
  ribbon begins at start: half-width W, alpha 0
  for each of the n - 1 inner points (axis point c, every len / n):
      offA = ((f - .5), (f - .5), (f - .5)) * Spread from 6 steps of A, minus its part along D ;  offB likewise from B
      off  = offA + (offB - offA) * phase ;  point = c + off ;  ribbon point: half-width W, colour col
      2 more steps of each stream; if f(A) < BranchFactor and fewer than MaxBranches so far:
          a branch from this point along normalize(off + D * BranchSpreadFactor), this W
      W -= Width / (n + 1)
  ribbon ends at end: alpha 0
  each branch (last first): strip(point, point + dir * BranchLength, seeds seedA + i + 1 / seedB + i + 1, no branches)
  ```
  f(x1, x2) = the float of two consecutive stream states (§2.2's recipe). skybolt: len ≈ 200 → n = 10, 20 m
  segments, half-width 30 m tapering to 5.5 m, offsets within ±10 m; branches 80 m, n = 4; the child n = 13,
  half-width 1 m. Branches come from the first inner points that pass, so they cluster at the top.
- The ribbon (`FUN_00569328` modes 1–3, pair emitter `FUN_00569188` 701522; re-bolt.md §3.2 has modes 4 / 5): per
  point two vertices P ± side, **side = normalize(T × (P − eye))·width**, T = the sum of the two neighbouring
  segments — the width is a HALF-width in metres, facing the camera about its own tangent. u = axis distance /
  `TextureSize` along the bolt, v = 0 / 1 across. Colour bytes (c + 1) >> 1 — and RGB halved once more in BF2 when
  the level's HDR effect is on (re-bolt.md §3.3; KAM1 authors `Effect("hdr")`; not re-read [H]). A strip that does
  not fit the 512-vertex batch is dropped whole.
- State: blend mode 1 = record 0x7ca840 {0x100048, TEST 0x51001, PRIM 0x58}: **additive (Cs·As + Cd), depth tested,
  no depth write, not fogged**, texture MODULATE. A far-scene object (its sphere is 100 m across at 200 m or more).
- [V-PS2] `tools/bolt_predict.py` on `bf1_kam1_cw_sideselect`: the main strip rebuilt from the live instance's seeds
  lies on the bolt in the state's own screenshot (`out/bolt_sideselect_overlay.png`; mean luminance along it 56.9
  against 38.9–48.3 for the polyline shifted 12–40 px, 47.0 with the seeds not XORed, 50.4 with other seeds) — one
  bolt, one frame, modest contrast.

## 9. PS2 checks of the precipitation model [V-RAM][V-PS2]

`tools/predict.py <state>` reads the particle array, the wind, the camera and the region list out of a save state,
runs §2–§4 and scores the predicted streaks (or flakes) against the state's own embedded screenshot (the frame lags
the RAM, so a time offset is scanned).
- BF2 KAM1, the four outdoor walk states: one sharp peak at −4/60 s, correlation 0.346 / 0.474 / 0.289 / 0.533
  (recall 0.55–0.72), ≤ 0.02 three frames away. Controls at the peak: tile 4 m 0.013, tile 6 m 0.008, shuffled
  positions 0.012. The two indoor states (cp0_cp1, cp3_cp2) and BF1's cp3_cp12: the model draws **no** streak on
  screen and the frames show none — without §4.2's shut cells it would draw 559, 583 and 317 (the verifier's control).
- BF1 KAM1: five states peak at −4/60 s with 0.143–0.162 (one at −2/60, 0.171); cp5_cp11's controls −0.003 / 0.001 /
  0.003. BF1 YAV2 cp2_cp3: 0.151 (tile 4 0.027, shuffled 0.002).
- Flakes, BF2 MUS1: precision 0.47 (tile 4 0.054, tile 6 0.032, shuffled 0.056); BF2 MYG1 0.33.
- What this proves: the unit cube and its RAM layout, the 5 m tiling, the cell walk, the rain-shadow rule (kinds 5
  and 7), the streak's direction and fade sense. What it does not: the fall speed and the frame lag are one fitted
  product; the streak length is only loosely fixed (0.5× and 2× score 0.315 / 0.303 against 0.346); the 2.5× path's
  alpha gets weak support (0.302 without it).

## 10. The port and the build

The port has: `mungefs::fx` (`LightningConfig`, `LightningBoltConfig`, no Precipitation / Wind), `ManifestLightning`
(the flash fields "wait for WTHR"), `Sky` dome tint (`sky_shell_tint`), the particle effects (`game/src/fx.rs`,
`Fx::spawn` by name, `submit_with` extra draws; `render/src/particles.rs` Sprite / Ribbon / Quad / Mesh, one colour
per draw), `core/src/region.rs` (the point tests). Two cards:
- **WTHR (build) — precipitation**: parse + bake `Precipitation` and `Wind` and the `rainshadow…` regions; a
  viewer-side module (not sim state, its own LCG) for §2–§5; a streak pass (per-vertex alpha: head a, tail 0; the
  PS2's 1 px of 512 scaled to the output — a streak 1/512 of the frame's width — is the faithful-yet-modern reading,
  one FEEL_NOTES line); flakes through the sprite path (edge 2.83·h, no fade); splashes through `Fx::spawn`.
  The PS2 blends in display units: (91, 94, 101) is a display colour.
- **WTHL (new) — lightning**: §7's schedule and fade driving `dome_tint`, the fog colour and the sun / ambient
  uniforms per game; thunder through the sound layer; §8's bolt as additive ribbons.
- Tests the build can run (acceptance): the three vectors in `lead_tmp/wthr/vectors/` (`predict.py --export`,
  `bolt_predict.py --export`): the cell walk's cells and alphas for a rain-shadow view (BF2 KAM1 cp1_cp6, 27 cells) and
  a plain one (BF2 MUS1, 28); the bolt polyline from seeds 0x647e4c2f / 0xc7f0f06d; the RAM rows of §6 and §7.3 as
  unit tests; and the frame test: the port's weather layer at the state's pose with the state's particles injected
  scores against the PS2 screenshot as `predict.py` does (the Python model: 0.53 on cp5_cp5; shuffled ≤ 0.02).

## 11. Still [H] / not established
- Draw order of weather against particles and the bolt inside the far pass; BF2's far bolts on screen; the 500.0 in
  the bolt's scene record.
- Which clock dt is (frame or game time, paused or not) for the effect updates.
- The thunder sounds' definitions (2D / 3D, gain); whether a mission script switches an effect.
- A clipped streak (cell crossing the near plane) may blend twice; the rounding of the colour product (±1).
- The frustum box corners behind the cell walk (`FUN_0057ac68`) and the exact cull test are read for shape only: the
  port's own box-vs-frustum test decides edge cells, which feeds §4.2's recurrence at the frustum's rim.
- `FUN_00161c60` on a miss (taken as fraction 1: the splash 50 m under the camera).
- BF1's bolt code and Lightning parser were read by one reader and not by the verifier (B8's RAM is BF1's).
- Lightning's B before the first flash (§7.3); BF1's ambient A / B = which of top / bottom.

## 12. Checks run (`lead_tmp/wthr/checks.txt`, `VERDICTS.md`)
- `re_check.py fn` on every cited line (202 rows), `word` (0x6013a0 = 0.2, 0x519518 = 0.2, 0x6012ec = 200.0, the
  vtables, "rainshadow"), `absent` / `writers` on every singleton and constant, `hashscan.py` for the ColorControl
  keys, `refs.py` for BF1's accessor stubs. `addrscan.py` misses a store whose base register is restored in a
  branch-likely delay slot (two real pointer stores were missed); the verifier re-ran every "only readers" claim with
  its own scan and all held.
- Two Opus readers (the low-level draw; lightning and the bolt), their load-bearing lines re-read here and their two
  surprising claims settled on PS2 evidence (flakes take no distance fade — `flake_fade.py`; BF2's sun is the
  complement of BF1's — `lightning_ram.py`).
- One Opus verifier on 69 claims and citations only: **64 SUPPORTED, 5 PARTLY, 0 NOT SUPPORTED**. The five, folded in:
  the splash test is a 1 m sphere, and the "in view" byte has a second writer (§5); region kind 4 is handled (§4.2);
  BF1's update has no cap of the re-roll clock (§2.3); the frame scores' scope and two numbers (§9).

## 13. The build (card WTHR, 2026-10-06) — what the port does and what it measured
`game/src/weather.rs` runs §2–§6; `mungefs::fx` parses the two blocks, the manifest carries them (the rain shadows
are the manifest's regions named `rainshadow…`, sorted into kinds 2 / 5 / 6–8 by the region init's rule).
- **Vectors** (`game/tests/weather_ps2.rs`, `game/tests/data/weather/`): the port's cell walk gives the model's cells
  on three PS2 views — BF2 KAM1 cp1→cp6 27 cells, BF2 MUS1 cp0→cp5 28, BF2 KAM1 cp0→cp1 (inside a hall) 4 — with the
  worst alpha gap 5.8e-6, the same far / fade start / dpx and the same streak vector.
- **"In view"**: the port flags a region when its world-axis bounding box is not wholly outside one plane of the
  (near, `Range`) frustum; on BF2 KAM1 cp1→cp6 that flags the same 3 of 17 regions PS2 RAM had flagged [V-RAM, one
  view].
- **The frame test** (`lead_tmp/wthr/tools/port_frame.py`: the port's `pose_render` draws the weather alone with a
  state's particles and wind injected and run −4/60 s, scored as `predict.py` scores the model): BF2 KAM1 cp5_cp5
  0.444 (the Python model 0.533; shuffled particles 0.017); cp1_cp6 0.281, cp2_cp3 0.392, cp4_cp4 0.236 — each about
  0.82 × the model's score (the port's frame is resampled from 835 × 640 to the PS2's 512 × 448). The two indoor
  poses (cp0_cp1, cp3_cp2): 0 streak pixels, with the BAKED map's regions and the port's own "in view" test.
- **Port choices**: a streak is a camera-facing strip the PS2's pixel wide as an ANGLE (2·tan(fov_h/2)/512 per metre
  of depth, on the PS2's 4:3 horizontal), never under one output pixel; `dpx` keeps the PS2's 4:3 horizontal in a
  wide window; zoom is a narrower fov with the far term near·(zoom − 1) dropped; the second writer of the "in view"
  flags (§5) is left out; the splash's 1 m sphere test is the region grown by 1 m [D]; BF1's uncapped re-roll clock
  is capped like BF2's (no difference at frame dt); the blend is the port's linear one (all particles').
- **The bake**: nine manifests gain `precipitation` (four of them `wind`), and `fx_pollen` / `fx_ember` / `fx_silt`
  keep their alpha (a flake composites by it); nothing else in the maps group moves (5,503 files the same).
