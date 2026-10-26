# OPUS RE (loop lane 2, 2026-09-26) - [O] UNAUDITED: no Fable pass
# RE: the sun on models — the PS2's own light vector, BF1's ((1 + n·l)/2)⁴ term, BF1's sun-less prelit set (queue SUND)

> **AUDIT 2026-09-26 (loop lane 2, OK — `engine/artifacts/loop/audits.md`).** Re-derived from the ELFs, VU1 and RAM
> before reading this doc. Scripts and verdicts are in `engine/artifacts/lead_tmp/audit-sund/`. One Opus verifier:
> 7 SUPPORTED, 1 PARTLY (the scope note on the VU1 table, below).
> - **Held:**
>   - §1's formula and chain, and §2's quaternion, row 2 and negation. Independent RAM check: BF1 ≤ 0.0039° on 16
>     maps; BF2 ≤ 1e-5° on 16 ground maps plus SPA3.
>   - BF1's d⁴ and BF2's max(0, ·); the 0x40 set's factor 0 and black 0x69c440; §5's absences.
>   - The port's offsets: 34.8–75.5° and 0–104.6°.
>   - PS2 RAM axes = the port's world axes, measured again with the port's terrain. The identity beats the 7 other
>     sign/swap cases on 7 maps: BF1 soldiers' feet on END1 (0.05 m), KAS1 and KAS2; BF2 foliage on DAG1 (0.05 m), END1,
>     FEL1 and YAV1. Flat YAV2 cannot tell the cases apart.
> - **§4 reproduced within a few points** with the audit's own knobs, block metric and mask (`base` = main byte for byte
>   on 206 poses):
>
>   | change | audit | this doc |
>   |---|---|---|
>   | direction alone, BF1 | 68.2 % closer | 68.0 % |
>   | direction alone, BF2 | 68.1 % | 72.2 % |
>   | BF1 prelit set, no sun | 92.8 % | 90.2 % |
>   | d⁴ vs Lambert, linear sum | 50.1 % | 51.4 % |
>   | base → ps2d4s | 63.6 % | 64 % |
>
>   The GS-unit sum was not re-run (LSUM).
> - **Precisions:**
>   - §1.8: BF1 table entries 0x1ab / 0x206 (sun without ambient) share the 0x1c7 / 0x238 d⁴ loops. The table runs to
>     0x30. Its entries 0x2d / 0x2f (0x74e / 0x76c) are the water overlay's paths, whose max(0, ·)³² glint is not a model
>     light term.
>   - §3: Light2 also matches on SPA3 ("Back").
>   - BF1 SunInfo's third key 0x6e793144 is `ShadowColor` (FUN_00493120).
>   - §5: BF2's dead Angle constant is built at 0x3be784 / 0x3be78c; only the ori is in the delay slot.
>   - For §6.4: no BF1 world material takes the multiply `Prelit` path. All 96 BF1 0x40 lit materials carry COL0
>     (`prelit_add`), so the no-sun rule turns no BF1 draw black.

**The question** (card SUND, from `re-water-colour.md` §4; PCSX2 FINDINGS F15, F1–F4, F20, F27): the port's baked sun
is BF1 (−L.z, L.y, L.x) and BF2 (L.x, L.y, −L.z) of the vector the PS2 keeps in RAM. Does the PS2's DIFFUSE lighting of
models use that vector, how exactly (sign, space, form), why does BF2's quaternion reading flip z, and what does the
corrected sun light at the PCSX2 poses?
Method: the loop's RE discipline. The hops below, the five self-checks, ONE Opus verifier given the claim list only
(`engine/artifacts/lead_tmp/re/sund/claims.md` → `verify/VERDICTS.md`), plus two measurements:
- The RAM of all 119 PCSX2 save states against the formulas (`sund/ramcheck.py`, `ramcheck.out`).
- The port rendered at all 200 PCSX2 poses of its 32 maps with the PS2's terms as scratch knobs (`sund/run_port.py`,
  `census.py`, `census_pair.py`; sheets in `sund/sheets/`).

Scratch lives in `engine/artifacts/lead_tmp/re/sund/` (the disc reader `discsun/`, notes, renders). Conventions:
B1 = `analysis/SWBFRONT.ELF-decomp.c`, B2 = `analysis/SWBFRNT2.ELF-decomp.c`. VU1 is decoded with
`scripts/tools/vu1dis.py` and EE code with `scripts/tools/eedis.py`. L = the direction the light TRAVELS (sun →
scene), l = −L. "GS units" means 0x80 = 1.0. Tags: [V] read or measured, [D] derived, [H] guess. Load-bearing claims
are also [O].

## 0. The answer

Both games light a model with ONE world-space vector per directional light, brought into the model's space on VU1. It
is exactly the RAM vector `re-water-colour.md` §4 found. The port's `sun_dir` must BE that vector: the PS2's world axes
are the port's (`pcsx2/pose.py`), so no conversion is left.

| term | PS2 (this RE) | port today | measured (§4) |
|---|---|---|---|
| BF1 sun vector | L = (−sin a0·cos a1, cos a0, −sin a0·sin a1), a = `SunInfo.Angle` in degrees → radians [V][O] | (−L.z, L.y, L.x): off by 35–76° (0° on KAS1) | RAM = formula to ≤ 0.004° on 16 maps |
| BF2 sun vector | row 2 of the matrix of q = (x, y, z, w) = (−a3, a0, −a1, a2), a = `Rotation` as stored [V][O] | (L.x, L.y, −L.z): off by 0–105° | RAM = formula to ≤ 0.001° on 17 maps; Light2 likewise on 5 |
| BF2 Light2 vector | the same reading | the same z-flip | as above |
| BF1 directional form | **C·((1 + n·l)/2)⁴** (VU1 0x1b9 / 0x21f) [V][O] | C·max(0, n·l) | vs Lambert: 51 % of blocks closer (linear sum), 65 % with the GS-unit sum (§4.3) |
| BF2 directional form | C·max(0, n·l) (VU1 0x1e6) [V][O] | the same | — |
| BF1 prelit (0x40) set | **no sun** (factor 0) and black ambient [V][O] | the sun, no ambient (the `Prelit` path with `sun_static` false) | 90 % of affected blocks closer (§4.2) |

The whole BF1 set of changes moves 82 % of the affected blocks toward the PS2 (mean error 22.6 → 9.3 levels, with the
GS-unit sum; 64 %, 22.1 → 11.7 without it). The BF2 direction alone moves 72 % (22.5 → 13.8). F15 (BF1 NAB2's shaded
columns) is this: the columns face away from the PS2's sun.

**Why the port's two conventions agreed with each other.** `re-sky-lighting.md` §4 checked them on BF2 GEO1, which stores
its sun as both `SunInfo.Angle(140, −10)` and a `lght` quaternion; the two readings agreed to 2.8°. But BF2 never reads
`SunInfo.Angle` (§5), and read the PS2's way the two GEO1 vectors are 43° apart. The agreement was one wrong convention
matching another.

## 1. BF1 [V][O]

1. **The vector.** `sky_` SunInfo `Angle` (0xad544418) is read by FUN_002e4dc8. It calls FUN_0049b8b8(Angle[0]·π/180,
   Angle[1]·π/180), with Angle[1] = 0 when only one value is stored (B1 271437–271444). FUN_0049b8b8 is a thunk to
   FUN_004931e0 (561137).
2. **The formula.** FUN_004931e0 (555571–555591) stores
   `DAT_0069c590 = −sin a0·cos a1`, `594 = cos a0`, `598 = −sin a0·sin a1`.
3. **Other writers.** FUN_00493128 normalises and stores (555530–555566). It is called by the reset FUN_00493670
   (555831, vector (0, −1, 0)) and by FUN_004b8e78. FUN_004b8e78 swaps in the preview rig's vector at 0x6fd1d0 (578308)
   and restores the saved one afterwards (578464). FUN_00494d18 zeroes it (556595). There are no other writers
   (`writers`, the verifier's addrscan).
4. **The model draw.** FUN_004ac0d8 picks the light set DAT_006ea930 or, for a segment with flag 0x40, DAT_006ea948
   (569068–569072).
   - When the set's factor +0x14 > 0 (569085), it uploads ONE directional light: the direction is 0x69c590..598
     UNCHANGED (569135–569137), and the colour is `SunInfo.Color` (0x69c588) times the factor with each byte halved.
   - The upload is FUN_0039baa8(buf, &{colour, dir}, 0x55e3c8 ×3) (569139). The three extra slots are a black light
     (ELF words 0, 0, 0, 1.0).
   - FUN_004b1a20 (573225) and the per-frame default FUN_0049bf50 (561529–561541) upload the same vector the same way.
5. **The upload does not negate.** FUN_0039baa8 writes command 0x564 (387933) and, per light, {dir, 1.0} (387945–387948).
   BF2's twin FUN_004973b8 negates (§2.4).
6. **VU1 0x564.** I = −0.5 (0x56f).
   - Per light: 0x57f loads xyz only; 0x581 sets w = vf0.w + I = 0.5; 0x583 sets xyz × I = −0.5·L.
   - The lights go to VU[60..63], as SoA rows after the transpose at 0x50a. The colours go to VU[56..59] as bytes/128.
7. **Into model space.** The model-matrix command (0x3d1–0x3e7) stores the rows ÷ |row0| at VU[83..85]. Then 0x724–0x734
   dots the SoA rows with them into VU[64..67].
8. **The term.** The flags command 0x425 stores (flags & 0x78) >> 2. The lit dispatch (0x16c–0x171) jumps through the
   table at 0x1d. For flags 0x18 (sun + ambient) it lands at 0x1b9; for 0x38 (plus point lights) at 0x21f.
   - At 0x1b9: `d = W + X·n.x + Y·n.y + Z·n.z`, with the normal's +65537 unpack offset folded into vf5 at 0x1c1–0x1c4.
     This reads the old I at 0x1c4, per `vu-ibit-pair-reads-old-i`.
   - `vf7 = d²` (0x1cb), `vf8 = d⁴` (0x1cc), and colour = amb·mat + Σ Cᵢ·mat·dᵢ⁴ (0x1d1–0x1d6), clamped at 255/128.
   - 0x21f squares its d twice too (0x246, 0x251, 0x253).
   - With the record from step 6, **d = 0.5 − 0.5·L·n = (1 + n·l)/2**. So BF1's term is C·((1 + n·l)/2)⁴. It is 1
     facing the sun, 1/16 edge-on and 0 facing away. It is not max(0, n·l).
9. **The factor.** FUN_00493cf0 starts +0x14 at 2.0 (556041) as the start of a minimum over the active regions at
   DAT_0069c548. It is 1.0 when no region applies (556083), then × DAT_00522a90 (556176).
   - DAT_00522a90 = 1.0: the ELF word, and its only reachable writer FUN_00493670 (555807). An unreachable setter stub
     at 0x495068 has no caller.
   - So outside the regions the sun is `SunInfo.Color` at full weight.
10. **The 0x40 (prelit) set carries no sun.** FUN_00496248 publishes the built set as DAT_006ea930 (FUN_00499538,
    557193). It then zeroes the +0x10/+0x14 pair, stores DAT_0069c440 at +0x10 and publishes the result as DAT_006ea948
    (FUN_00499578, 557196–557198).
    - DAT_0069c440 is opaque black: its only writer is the palette init FUN_00492598 (555056–555076), and it reads
      000000ff in all 60 BF1 states.
    - So a 0x40 segment gets factor 0, and FUN_004ac0d8 uploads no sun for it.
    - **Verifier correction (A14):** this holds in ordinary world rendering. FUN_00496248's 0x2000000 mode publishes
      both sets with factor 1.0, and FUN_004ac0d8's other branch (569146–569165) then lights every segment with the
      fixed 0x6fd1c8/0x6fd1d0/0x6fd1e0 rig (the side-select preview's rig, §1.3). That mode is reached only from the
      vtable calls in FUN_001bd6c8 (65175) and FUN_001be1e8 (65885); which objects they draw is [H], but not the world.

## 2. BF2 [V][O]

1. **The parser.** The `lght` Light parser is FUN_00596978.
   - `Position` (0x934f4e0a) is stored as (x, y, −z) (EE 0x596c6c–0x596c88).
   - `Rotation` (0x21ac415f), stored a0..a3, is stored as **q = (x, y, z, w) = (−a3, a0, −a1, a2)** (EE
     0x596c44–0x596c68 with the delay slot 0x596b34; B2 728740–728745).
   - This is the editor-to-game handedness change, applied at load. `Position.z` gets the same flip.
2. **The matrix.** FUN_0053ac20(sp+48, q, position) (EE 0x596df0) builds the rows
   - r0 = (1−2y²−2z², 2(xy+wz), 2(xz−wy)),
   - r1 = (2(xy−wz), 1−2x²−2z², 2(yz+wx)),
   - r2 = (2(xz+wy), 2(yz−wx), 1−2x²−2y²),

   derived lane by lane from the COP2 code. A Type-1 light gets FUN_00595968(obj, **row 2**, colour) (EE 0x596e44).
   FUN_00595b98 normalises that vector into obj+0x50 and copies it to +0x6c..+0x74.
3. **Light1 and Light2.** GlobalLights (FUN_00596380) points DAT_0060c6e0 / DAT_0060c6e4 at the type-1 lights named
   Light1 / Light2 (728304–728306 / 728298–728300). The reset FUN_00597388 creates both pointing (0, −1, 0)
   (728912–728921).
4. **The upload** FUN_0057fd90 copies each directional light's +0x68..+0x74 into a record (715355–715361). FUN_004973b8
   negates the direction (715384; 555294–555297).
   - VU1 0x56b keeps xyz and sets w = 0 (0x58a–0x58c).
   - 0x72e–0x73e brings the lights into model space with the rows VU[86..88].
   - 0x1e6–0x20d computes Σ Cᵢ·mat·max(0, dᵢ) (MAXx at 0x1f8 / 0x200 / 0x20c).
   - So +0x50 is the travel direction L, and l = −L.
5. **Why the port's z is flipped.** For a unit quaternion, row 2 of (−a3, a0, −a1, a2) equals (t.x, t.y, −t.z), where
   t = the port's `bake::lighting::travel_dir` (the stored quaternion read as (w, x, y, z), rotating (0, 0, 1)).
   The verifier checked this on 10,000 random quaternions (max error 9e−16).

## 3. The RAM [V measured]

`sund/ramcheck.py` over all 119 states (`ramcheck.out`, re-run byte-identical by the verifier):
- **BF1.** DAT_0069c590 equals §1.2's formula on the disc's `SunInfo.Angle` to ≤ 0.004° on all 16 BF1 maps with
  states, TAT1 included. The one exception is `bf1_kas2_cw_sideselect`, which caught FUN_004b8e78's preview-rig swap
  mid-flight: the vector equals 0x6fd1d0's (−0.577, −0.577, −0.577) and the colour equals 0x6fd1e0's.
- **BF2.** *(DAT_0060c6e0)+0x50 equals §2's formula on Light1's `Rotation` to ≤ 0.001° on all 17 BF2 maps (and the
  space-Kashyyyk states against SPA3). *(DAT_0060c6e4)+0x50 equals it on Light2 on COR1, END1, MUS1, NAB2 and UTA1.
  Everywhere else Light2 is the reset object (0, −1, 0) with fade 0; MYG1 names a Light2 its `lght` does not define.
- **The port's baked `sun_dir`** is (−L.z, L.y, L.x) on BF1, off by 34.8–75.5° (0° on KAS1, straight down), and
  (L.x, L.y, −L.z) on BF2, off by 0–104.6° (YAV1 0°, KAS2 10°, NAB2 105°).

## 4. The port at the PCSX2 poses [V measured]

**Instrument.** A detached scratch worktree (`openbf-wt/sund-scratch`, never committed) adds knobs:
- `SUND_SUN=ps2`: `sun_dir` becomes the PS2's L, through today's `look::ps2_light_vector` inversion, and BF2's Light2
  z is flipped.
- `SUND_B1STATIC=1`: BF1 prelit draws skip the sun.
- `SUND_D4=1`: the Lit / Prelit sun term becomes ((1 + n·l)/2)⁴.
- `SUND_GSUM=1`: the light terms are summed in GS (gamma) units and linearised once.

All 200 `base` renders equal main's own pose renderer byte for byte (`pristine_check.py`, run before and after the last
knob).

**Census.** `census.py` works on 8×8-pixel block means of the PS2 frame and each port frame, with the HUD and the soldier
masked. It uses the blocks a variant moved by more than 2 levels, and reports the mean |port − PS2| before and after and
the share of blocks that got closer. `census_pair.py` compares two variants over the blocks where they differ.

### 4.1 The direction
| game | blocks | error, port today | error, PS2 vector | closer |
|---|---|---|---|---|
| BF1 (15 maps) | 79,235 | 25.4 | 14.2 | 68.0 % |
| BF2 (17 maps) | 37,156 | 22.5 | 13.8 | 72.2 % |

- **Best cases:** BF1 NAB2 `v5_up` (37.2 → 6.2), TAT1 `v1` (35.4 → 4.4), RHN2 `v3` (29.7 → 4.4), YAV1 `v2_down`
  (44.6 → 4.6); BF2 NAB2 `v3` (12.1 → 2.8), TAT2 `v2` (47.0 → 13.7).
  - The pictures: `sheets/best_bf1.png` (Theed's column, the Dune Sea rock, the Rhen Var wall and the Yavin pillar are in
    shade on the PS2 and sunlit in the port today) and `sheets/best_bf2.png` (Mos Eisley's wall, Geonosis's rock).
- **Worst cases** (`sheets/worse_bf1.png`, `worse_bf2.png`) are other residuals, not the sun:
  - BF1 BES2 `v2`: the port's camera sits against a wall the PS2 does not show.
  - BF2 NAB2 `v1_canal` carries a damage tint.
  - KAM1 / YAV2 are dim, and the changes there are small.
  - MUS1 is its lava glow (F28).

### 4.2 BF1's prelit set without a sun
`ps2 → ps2s`: 8,590 blocks, 39.7 → 17.6, **90.2 % closer**. It is mostly BES2, whose 0x40 walls the port lights
orange today.

### 4.3 BF1's d⁴ term, and the colour space of the sum
- **d⁴ against Lambert, the port's linear sum** (`ps2s → ps2d4s`): 66,326 blocks, 12.74 → 12.21, 51.4 % closer. That is
  better on 7 of the 15 BF1 port maps (BES1, BES2, END1, NAB1, NAB2, RHN1, YAV1) and worse on 8 (KAS2, RHN2, KAM1, KAS1,
  GEO1, TAT1, YAV2, and HOT1 on one block).
- **The same with the light summed in GS units** (`ps2s_g → ps2d4s_g`): 73,864 blocks, 14.11 → 10.10, **65.3 %
  closer**, better on 13 of 15 maps (worse on KAM1, and on KAS1 by 0.04).
- **Everything together on BF1** (`base → ps2d4s_g`): 108,037 blocks, 22.59 → 9.32, 81.6 % closer.
- The GS-unit sum is a port-wide rule, and on BF2 it is mixed (`ps2 → ps2_g`: 48.5 % closer). It is worse on COR1,
  MYG1, NAB2 and MUS1 (COR1's floor turns too bright, `sheets/lsum_bf2.png`) and better on KAS2, POL1, TAT2 and KAM1.
  So it is not part of SUND: card LSUM.

## 5. Side findings

- **Neither game reads `SunInfo.BackAngle` / `BackColor` / `BackDegree`** [V][O] (for card BACKL).
  - No decomp hit in either ELF.
  - No `lui` of either high half, no lui+ori/addiu pair and no data word.
  - The property names are not strings in the ELFs either, so nothing hashes them at runtime.
- **BF2 ignores `SunInfo` entirely** [V][O].
  - Its reader FUN_003be738 compares only Color, and converts it into stack locals that are never used.
  - A compiled-out Angle case sits in a delay slot (0x3be784).
- **BF2's `SunFlare` block carries its own `Angle`** [V] (for card SUNG). It is read by FUN_0046e998 (vtable 0x63acc4;
  HaloInnerRing / HaloMiddleRing / HaloOutterRing / Size / SpikeColor / SpikeSize / Color / Angle). The vector is
  stored at +0xa0: (sin a0 · X, −cos a0, sin a0 · Y), with X, Y the sincos outputs of a1 (which output is which, [H]).
- **A `lght` Position is (x, y, −z) on the PS2** [V]. The port reads it raw, and nothing consumes it yet (card TDYN's
  omni lights will).
- **BF1's active-region darkening** [V code, H meaning]. FUN_00493cf0's region list DAT_0069c548 (FUN_00493ad0) lowers
  a model's sun factor inside a region (+0x70) and blends its ambient toward the region's tint (+0x74). The port has no
  such regions; nothing measured here needs them.

## 6. What the port owes (card SUND, build)

1. **BF1 bake.** `sun_dir` = §1.2's L, computed directly in `mungefs::sky` as the PS2's vector (it replaces
   `direction_to_sun`). Pin it with tests on the RAM vectors: YAV1 (140, −30) → (−0.5567, −0.7660, 0.3214);
   KAS2 (145, 280) → (−0.0996, −0.8192, 0.5648); NAB2 (145, −30) → (−0.4967, −0.8192, 0.2868); KAS1 (180, 0) → (0, −1, 0).
2. **BF2 bake.** Light1 and Light2 directions = row 2 of (−a3, a0, −a1, a2). Pin them with tests: GEO1
   (−0.409077, −0.872693, 0.262547, −0.04618) → (−0.1342, −0.7382, 0.6610); COR1 Light2
   (0.803575, 0.23957, −0.480029, 0.257893) → (−0.6479, −0.6326, −0.4244).
3. **Render.** BF1's sun term is ((1 + n·l)/2)⁴ on every lit path; BF2 keeps max(0, n·l). A per-map form flag reaches
   the globals: `sun_dir`'s padding word is free.
4. **BF1 prelit draws skip the sun**, as BF2 prelit draws skip a `Static()` one. The port's `Prelit` / `PrelitAdd`
   `dynamic` term gets no sun on BF1.
5. **Remove `look::ps2_light_vector`**: it becomes the identity. The water reads `sun_dir` directly.
6. **Fix the tests and docs that pin the old conventions:**
   - tests: `mungefs::sky::tests::geo1_sun_angle_matches_its_light_quaternion`, `mungefs/tests/suite/tat1_lvl.rs`'s
     `direction_to_sun`, the `bake::lighting` sun tests, and `look.rs`'s `the_water_shading_is_the_overlays_constant_block`;
   - module docs: `mungefs::sky`, `mungefs::light`, `bake::lighting`, and `re-sky-lighting.md` §4 (banner added).
7. **Rebake and goldens.** Rebake every map; every lit golden moves. The specular pass reads the same `sun_dir` and
   moves with it; it is written against the PS2's L (`re-specular-pass.md` §2).

PROVISIONAL [O]: the d⁴ weight is summed with the ambient in linear space until LSUM settles the colour space.

## 7. What stays [H]
- The objects drawn in BF1's 0x2000000 mode (§1.10).
- The meaning of BF1's active regions (§5).
- Which sincos output is which in BF2's `SunFlare` vector.
- Whether the PS2's GS-unit sum explains BF2's residuals (LSUM): BF2 COR1 disagrees.
- VU pipeline stalls, assumed interlocked (the verifier did not simulate them).

## 8. Verification
**Self-checks.**
- (1) `re_check.py fn` placed every cited decomp line (B1 271437–578464; B2 531522, 555282–555297, 715355–715384,
  728298–728921) in its named function.
- (2) Absences:
  - `absent` / `writers` for DAT_0069c590 (4 writers: FUN_004931e0, FUN_00493128 ×2, FUN_00494d18) and 0x55e3c8
    (three uses);
  - an ELF scan (lui hi / hi+1 + ori/addiu lo, data words) for BackAngle / BackColor / BackDegree, with Angle and
    SunInfo as positive controls.
- (3) Twins: FUN_0039baa8 ↔ FUN_004973b8; FUN_00493128 ↔ FUN_00595b98; VU1 0x564 ↔ 0x56b, 0x724 ↔ 0x72e,
  0x1b9 ↔ 0x1e6, 0x3d1 ↔ 0x3d1 (both opened).
- (4) Words: 0x55e3c8 (0, 0, 0, 1.0), 0x522a90 (1.0).
- (5) Writers: DAT_00522a90 (1), DAT_0069c440 (the palette init only), DAT_0060c6e0 / e4 (FUN_00596380, the reset
  FUN_00597388, the teardown FUN_005976a0).

**The verifier** (one Opus agent, claim list only, `sund/verify/VERDICTS.md`): 27 SUPPORTED, 2 PARTLY, 0 NOT SUPPORTED.
Folded in:
- A14: the 0x2000000 mode (§1.10).
- C1: 16 BF1 maps, not 15.
- Notes: the unreachable DAT_00522a90 stub, MYG1's missing Light2, BF2's dropped SunInfo Color and dead Angle case.

It could not check VU stalls, the ~20 callees handed the palette base (RAM covers them), or the 0x2000000 objects.
