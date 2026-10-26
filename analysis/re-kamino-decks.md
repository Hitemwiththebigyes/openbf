# OPUS RE (2026-10-05) - [O] UNAUDITED
# RE: why Kamino's decks are 1.5-2 x darker in the port than on the PS2, in both games — the light sum's colour space, the ambient floor, lights hung on a missing hardpoint, BF1's local lights and its sun-coloured specular pass (card B1DK)

**The question** (card B1DK; `re-kamino.md` §5, `re-vertex-colour-interp.md` §5): BF1 Kamino's landing decks read
(65,79,83) on the PS2 and (41,46,52) in the port at every bearing; BF2's mid-floor 82/84/89 against 45/48/51. Which
term lights these floors? The card named three leads (the Specmap pass, a BF1 local-light path, the DomeInfo filter)
and asked for one PS2 knock-out per candidate.

**Method.** The PS2's deck taken apart first: 31 reloads of patched save states under PCSX2's software renderer, one
term off at a time (§1). Then the port taken apart at the same poses with a scratch build (knobs by environment; the
no-knob build is byte-identical to main's renders), and every hypothesis scored over all 954 stored PCSX2 pairs (§4).
The code reading follows the loop's RE discipline: the five self-checks, one helper reader for BF1's local-light
path (`b1dk/re/AGENT_BF1.md`; the lead re-read its load-bearing lines and re-ran its censuses), and one foreground
Opus verifier on the claim list: **27 SUPPORTED, 3 PARTLY, 0 NOT SUPPORTED** of 30, all folded in (§10).

Scratch, scripts, frames, states, renders and score files: `engine/artifacts/lead_tmp/b1dk/` (below `b1dk/`). B1 =
`analysis/SWBFRONT.ELF-decomp.c`, B2 = `analysis/SWBFRNT2.ELF-decomp.c`; VU1 = each game's main microprogram, read with
`scripts/tools/vu1dis.py`; "GS units" = 0x80 is 1.0; "display units" = the frame's own 0..255 values. A box is
(x0,y0,x1,y1) on the 640x480 frame, mean R/G/B. Tags: [V] read (lines cited) or measured (files cited), [V-RAM] read out
of PS2 RAM, [V-PS2] measured on PCSX2 frames, [D] derived, [H] hypothesis; load-bearing claims are also [O]. The port is
main 35c93e6.

## 0. The answer

No single term. The deck is a LIT, un-prelit fan of 24 triangles 13.3 m long round the post plus a 48-triangle ring
out to 22.2 m, all normals straight up, no COL0, with a power-4 specular pass (both discs; §5.1). Five things
separate the port's deck from the PS2's, in this order of size at the stored poses:

| # | term | the PS2 | the port (main) | effect on the deck | games |
|---|---|---|---|---|---|
| 1 | **the colour space of the light sum** | ambient + sun (+ points) are added in GS units, then the GS multiplies by the texel once (§2) | each colour is linearised and the SUM is made in linear light: `texel x (ambient + sun x n.l)` | Kamino's ambient 100 and sun (65,85,105) are of one size, the worst case: the PS2's base is **1.37-1.40 x** the port's (52/57/62 against 38/41/44) | both, every lit model on every map |
| 2 | **the ambient floor** | COR1's ambient is 0, MUS1's (30,10,10), NAB2's ground 0 … (§3) | the bake lifts any ambient channel under 0.045 linear, which is 60/255 in display units | not Kamino's. It is why the display-unit sum looked wrong on BF2 in 2026-09 (card LSUM): with the PS2's own ambients no map gets worse | BF2 x 7 maps, BF1 BES2 |
| 3 | **EntityLights on a missing hardpoint** | BF2 creates them anyway, at the CARRIER's origin (§5): each Kamino bunker carries two `kamlight` (181,172,163) and one `kamlight_blue` (2,50,100), r 20, not Static, in the middle of its deck | the bake skips a light whose hardpoint the skeleton lacks: Kamino bakes none | at the walk poses +5 to +14 levels; 4.5 m from the post about 21-25 of the deck's 124/119/127 | BF2 (KAM1 10 lights, POL1 2, DEA1 1) |
| 4 | **BF1's local lights** | every BF1 post carries an omni, r 8, at its building's `hp_hologram` bone (0.32 m up; 1.36 m on the large post), in the team-relation colour — friend (100,200,100), enemy (200,100,100), neutral (192,192,192); 8 maps also carry EntityLights; every model draw takes up to 4 (§6) | BF1 has no local light at all | the green wash round a friendly post (94/136/112 against 62/71/80 without it); small where the camera stands ON the post (the stored walk poses) | BF1, every map |
| 5 | **BF1's sun-coloured specular pass** | the pass colour is the SunInfo bytes raw, (65,85,105)/128 on Kamino | `SpecPass::rgb_under` is handed the manifest's LINEAR sun colour: (0.105,0.181,0.281) where the PS2 has (0.508,0.664,0.820) (§7) | the part of the deck that changes with the bearing: PS2 +22/+30/+37 at bearing 270, port +3/+5/+9 | BF1, maps with a dark sun |

What the card's three leads turned out to be: the Specmap pass is drawn but 3-5 x too weak on BF1 (row 5); BF1 does
have a local-light path (row 4); the DomeInfo filter is built (BFLT) and only multiplies the gap (the PS2's deck is
above its threshold 50, the port's below).

**Prototype** (scratch tree, rows 1, 2, 3 and 5 on, the BF1 posts lit by BF2's rule): the eight BF1 deck boxes of the
card read within 10-13 levels per channel of the PS2's (main: 21-58 under); bf1_kam1's parity 19.29 -> 13.4, bf2_kam1's
16.71 -> 11.7; **all 954 pairs 10.618 -> 9.707**, no map up by more than 0.5 (§4).

Still short after all five: BF2's bearing-270 highlight, which is the specular pass plus the HDR bloom it triggers
(card GLOWB, §8).

## 1. The PS2 taken apart [V-PS2]

`b1dk/ps2/mk2.py` (BF2) and `mk1.py` (BF1) cut copies of a save state with terms patched in EE RAM; `run.py` boots each
under the software renderer, waits 7-8 s, saves; the saved state's own frame is measured. Every variant, controls
included, freezes the Lightning effect (next flash at 1e9 s). Each patch asserts the word it replaces; the saved
states still hold their patches and each set shares one camera (verifier).

### 1.1 BF2 KAM1, `bf2_kam1_walk_cp2_cp3_b000` (the eye on post cp3, looking down -Z)

Boxes clear of the soldier and the HUD: midR (400,278,627,346), nearR (430,360,627,470). Frames
`b1dk/ps2/frames/k2__*.png`, sheet `b1dk/sheet_k2a_first.png`.

| variant | what is off | midR | nearR |
|---|---|---|---|
| ctl / ctl2 | nothing | 60.0 65.5 71.5 / 61.6 67.2 73.5 | 71.4 76.7 84.9 / 70.5 75.8 84.0 |
| nobloom | HDR bloom (GlowThreshold 4.0) | 61.1 66.6 72.8 | 71.1 76.3 84.7 |
| nosoft | DomeInfo Softness | 63.3 68.9 75.3 | 72.0 77.3 85.2 |
| nospec | specular pass (record 0x7c57b0, weight 0) | 61.0 66.6 72.8 | 61.7 66.9 75.2 |
| nopoint | every model's omnis and spot (0x57fed4 / 0x57fecc) | 55.4 60.1 65.6 | 57.5 62.8 69.4 |
| nobloom+nospec+nopoint | all three | 52.1 56.6 61.7 | 48.1 53.4 60.0 |
| noamb | the global set's Top / Bottom | 27.5 33.4 40.5 | 38.6 43.9 52.3 |
| nosun | global light 1's colour | 45.0 45.9 46.9 | 51.5 51.5 53.0 |
| noamb+nosun (Z) | both | 8.2 9.3 10.6 | 19.9 19.8 21.5 |
| noamb+nosun+nopoint | all light | 3.6 3.9 4.3 | 9.6 9.6 10.0 |
| nosun+ambhalf | sun off, ambient = 0.5 | 57.4 58.4 59.0 | 61.5 61.5 62.7 |
| noamb+sunhalf | ambient off, sun colour = 0.5 | 44.1 45.3 46.2 | 56.6 56.5 57.8 |
| **port, main** | | 39.2 42.1 45.1 | 43.9 46.4 50.7 |
| port, no points, no specular layer | | 37.9 41.0 44.0 | 35.4 39.2 43.6 |

- Bloom and Softness: nothing on this deck at this bearing. The specular pass: nothing at mid-floor, about 10 near
  the camera (the port's adds 6-8 there).
- **The base (ambient + sun) is the largest term of the gap**: PS2 52.1/56.6/61.7 with everything else off against
  the port's 37.9/41.0/44.0 — 1.37 / 1.38 / 1.40 (§2).
- The local lights add 5-7 at mid-floor and 13-15 near the camera at this pose, and far more once the lit centre of
  the fan is in view (§1.3).

### 1.2 BF1 KAM1, `bf1_kam1_cp0_cp2_beta1_spawn` (8 m from the friendly post cp2)

Frames `b1dk/ps2/frames/k1__*.png`, sheet `b1dk/sheet_k1a.png`. fanL (200,215,420,235) is the lit fan beside the
post, deckR (440,300,630,440) the deck away from it.

| variant | what is off | fanL | deckR |
|---|---|---|---|
| ctl2 | nothing | 93.6 135.5 111.6 | 55.4 61.9 67.0 |
| noomni | every omni (`FUN_00494b08` returns 0, 0x494b74) | **62.2 71.4 79.7** | 53.9 60.5 65.0 |
| nospec | the Specular class's pass (record 0x6f4540, weight 0) | 94.0 135.2 112.2 | 55.9 62.3 67.6 |
| nofilter | DomeInfo Filter | 82.5 112.8 95.5 | 51.5 57.0 60.9 |
| noamb | `DAT_0069c570` (static draws' ambient) | 46.0 84.2 61.5 | 16.9 22.6 27.6 |
| nosun | `DAT_0069c588` | 67.6 100.4 68.4 | 33.9 34.2 32.6 |
| noomni+nospec+nofilter | all three | 59.3 66.3 72.5 | 51.0 56.8 60.5 |

- **The green wash is the post's omni** (the card's knock-out): fanL loses 31/64/32, the post's (100,200,100).
- The deck takes `DAT_0069c570`, the static draws' ambient (`re-model-lighting.md` §12): it goes dark with it.
- With the filter undone (v = (v' + gT)/(1 + g), g = 50/128, T = 50, above T) deckR's ambient and sun terms are 37.0
  and 20.0 (R), 36.0 / 24.4 (G), 34.6 / 29.6 (B): sun / ambient = 0.54 / 0.68 / 0.86 against the authored
  (65,85,105) x ((1 + 0.906)/2)^4 / 100 = 0.54 / 0.70 / 0.87. BF1's d^4 term (`re-sun-direction.md` §1.8) and the
  byte scale hold, and the two terms ADD on the frame.
- The port has no frame to set beside this one: at this pose its eye is inside a `kam_prop_pillar` (`re-kamino.md` §5,
  card BFC; about 85 % of its frame is that slab). The port side of BF1 is measured at the walk poses (§4.2).
- A trap for RAM readers (verifier): `FUN_004b8e78` swaps (192,192,192) / (255,255,255) into BF1's ambient and sun
  words while it draws, and `k1__noamb` / `k1__nosun` were saved inside it — the patched values sat in its stack
  frame and both patches held.

### 1.3 BF2 KAM1 turned to bearing 270, the eye 4.5 m from the post

`lead_tmp/bpnt/turn.py` turns the chase camera round the held soldier; the eye lands at (-64.58, 62.13, 300.52), yaw
270.7. Frames `b1dk/ps2/frames/k2w__*.png`, sheet `b1dk/sheet_k2w.png`; the port at that exact pose `b1dk/var/k2w/`.
near (430,340,640,470), midR (430,278,627,336).

| | near | midR |
|---|---|---|
| PS2 ctl | 123.8 119.0 127.0 | 97.0 101.8 110.1 |
| PS2 nobloom | 105.7 102.4 109.0 | 97.3 102.4 110.3 |
| PS2 nospec | 95.0 90.8 98.4 | 82.0 87.1 95.2 |
| PS2 nobloom+nospec | 95.7 91.6 99.2 | 84.0 89.3 97.4 |
| PS2 nopoint | **54.9 59.6 65.3** | 69.7 75.2 81.9 |
| PS2 nobloom+nospec+nopoint | 42.8 47.5 53.5 | 54.4 60.0 66.8 |
| port, main | 65.3 54.7 58.7 | 54.5 55.6 59.9 |
| port, prototype (§4), no points | 52.0 58.0 64.0 | 66.2 73.4 80.0 |
| port, prototype | 100.6 95.3 102.6 | 85.7 91.2 98.8 |

- With the fan's lit centre in view the local lights are the deck's largest term: +69/+59/+62 near the camera with
  bloom and the pass on, +52.9/+44.2/+45.7 with both off. The knock-out removes FOUR lights there: the bunker's three
  (§5) and cp3's own post light 0.44 m away, which the port already draws. In the port's display-unit model the post
  light alone is +27.8/+13.9/+13.8 and the three origin lights +20.8/+23.4/+24.8 (verifier): the missing lights are
  worth 21-25 levels in that box.
- The specular pass adds about 10 and the bloom about 18 — and the bloom adds nothing once the pass is off: the
  highlight pushes the white deck lines over the bloom's threshold.
- With the points off the prototype's deck is the PS2's to 3 levels in both boxes.

## 2. The light sum is made in GS units; the port makes it in linear light [V][V-PS2][O]

**The PS2.** A lit vertex's colour is one sum in GS units, and the GS multiplies it into the texel once.
- BF2 VU1, directional lights (0x1e6-0x20d): colour = sum over the four slots of (light colour x material) x
  max(0, N.L) + the 65536 pack offset (0x1f4-0x203), clamped by `MINIi` at I = 0x478000ff, i.e. 255/128 (0x209), stored
  in the vertex's colour slot (0x20d). Plain Lambert: no square, no wrap.
- BF2 VU1, the ambient pass (0x1bb-0x1d2): it loads that stored colour (`LQ vf5, 1(vi11)`, 0x1c8 / 0x1d0), adds
  mat x (VU[92] + VU[93]) + mat x (VU[92] - VU[93]) x n'y (0x1bf-0x1ce), clamps at the same 255/128 (0x1cf) and
  stores it back (0x1d2). The local lights' loop adds into the same slot before it (`re-point-lights.md` §7).
- BF1 VU1 (0x21f-0x285) makes it one accumulation: 65536 + ambient VU[80] x mat (0x22c-0x22d) + the sun rows x d^4
  (0x251, 0x253, 0x255-0x259) + the omni rows x their weight (0x274-0x27a), one clamp at 255/128 (0x281), stored (0x285).
- The frame buffer holds display values (PSMCT32, `re-specular-pass.md` §6) and BF2's HDR composite only doubles the
  half-scale frame back (`re-model-lighting.md` §1): nothing between the VU and the screen is a transfer curve [D].

**Measured on the PS2** (§1.1; `b1dk/fit_sum.py`; nearR, a box that is deck and nothing else): the texel level from
the ambient-0.5 variant is T = 83.3/83.3/82.4; the ambient term (the two controls' mean - noamb) is 0.389/0.388/0.391
x T (authored Top 100/255 = 0.392); the sun term (controls - nosun) is 0.233/0.297/0.381 x T (authored
(65,85,105)/255 x N.L 0.906 = 0.231/0.302/0.373); and T x (0.392 + sun x 0.906) + the unlit remainder =
71.8/77.7/84.6 against the controls' 71.0/76.2/84.5. The terms add in display units, to 1.5 levels; the linear sum
would give 57.9/61.1/66.2. (A second box, (10,278,230,336), fits to 1.9. A box holding rails or lamps does not fit the
closed form — midR's T comes out 13 % high — so a deck-only box is the one to use.)

**The port** [V]. `render/src/shader.wgsl` `shade_unfogged`: `lit = base.rgb * detail * (ambient + globals.sun_color *
ndotl)` (and `+ light2`), where `ambient`, `sun_color` and `light2_color` are the manifest's colours, each linearised
by the bake (`bf2_kam1`: `sun_color (0.0529, 0.0908, 0.1413)`, `ambient_sky 0.1274`). Linearising each factor is right
for a PRODUCT (texel x colour); it is not for a SUM. The local lights are already added the PS2's way (`with_points`
encodes, adds, decodes), as are the specular add and the fog lerp. The prelit paths add their dynamic sun and Light2
in linear light too.

**The size of it.** The error is largest where two terms are of one size, and Kamino is that case: ambient 100/255,
sun (65,85,105) x 0.906 (BF2) or x 0.825 (BF1's d^4). The port's own arithmetic (the exact sRGB curve, N.L applied to
the linear sun) falls short of A + S by 1.37/1.40/1.41 on BF2 and 1.34/1.37/1.38 on BF1 (a pure 2.2 power law would
say 1.40/1.45/1.46). Measured, BF2 midR with everything else off: PS2 52.1/56.6/61.7, port 37.9/41.0/44.0 =
**1.37/1.38/1.40**. Where one term carries the light (a lone sun, a prelit model) the two sums agree.

## 3. The ambient floor [V][V-RAM]

`bake/src/lighting.rs`: `const AMBIENT_FLOOR: f32 = 0.045` — "lowest ambient any map renders with, linear. PROVISIONAL
and ours" — lifts any ambient channel below it, on both ambients (`floor_ambient`). 0.045 linear is 60/255 in
display units. It was written when the port lit a map with one sun and the ambient (COR1 authors Top/Bottom 0/0/0
and would have been black); Light2, COL0, the prelit rule and the local lights have landed since.

It changes eight manifests; the PS2's own values, from each map's walk states (BF2 `*(0x60c6e8)` + 0x50 / + 0x60,
BF1 `DAT_0069c570`):

| map | PS2 Top / Bottom (bytes) | the port's floored channel(s) |
|---|---|---|
| bf2_cor1 | (0,0,0) / (0,0,0) | both, all channels: 60/255 where the PS2 has 0 |
| bf2_mus1 | (30,10,10) / (0,0,0) | both, all channels |
| bf2_nab2 | (66,77,102) / (0,0,0) | Bottom |
| bf2_kas2 | (100,100,100) / (50,50,55) | Bottom |
| bf2_yav1 | (75,83,85) / (42,44,20) | Bottom |
| bf2_tan1 | (243,243,243) / (52,52,54) | Bottom |
| bf2_geo1 | (100,80,50) / (90,60,30) | the blue channel of both |
| bf1_bes2 | (50,50,70) | red and green |

**Why card LSUM read BF2 as "mixed" in 2026-09.** The display-unit sum was tried then (`re-sun-direction.md` §4.3)
and made COR1, NAB2, MUS1 and MYG1 worse. On today's main the same knob alone still does: COR1 +2.55, NAB2 +3.32,
MUS1 +0.59, YAV1 +0.53 (MYG1 +0.03). Every one of those carries the floor. In a linear sum 0.045 beside a sun of
0.4-1.0 is nothing; in display units it is a 60/255 ambient the PS2 does not have. With the PS2's own ambients the
same knob gives COR1 -0.10, NAB2 -2.33, MUS1 -0.84, YAV1 +0.37 (§4.1), and COR1's plaza floor — LITE's box
(420,300,640,440) at `cp1_cp2_postA_spawn` — reads 41.8/47.1/57.0 against the PS2's 42.7/46.3/54.2 (main
40.5/45.0/53.9; the display-unit sum with the floor 58.8/64.1/74.9).

## 4. The port taken apart and scored [V measured]

Scratch worktree `openbf-wt/b1dk-scratch` at 35c93e6, target `lead_tmp/b1dk-scratch_target`, patch
`b1dk/scratch_knobs.patch`. Knobs (environment; none set = main, checked byte for byte on four poses and by the
all-maps score 10.618 = main's): `B1DK_K_GSUM=1` the Lit and Character paths' ambient + directional + point sum in
display units, linearised once, times the linear texel (`2`: the texel times the sum in display units too, clamped
where the GS clamps); `B1DK_NOFLOOR=1` §3's PS2 ambients; `B1DK_SPECSUN=1` §7; `B1DK_BF1_CP=...` BF1 post lights by
BF2's rule; `B1DK_ATTACH_ORIGIN=1` (the bake) §5; `B1DK_K_NOPOINT / NOSUN / NOAMB` for the port's own knock-outs.
`b1dk/par/README.md` lists the knobs of every scored run.

### 4.1 All 954 pairs (`scripts/parity.py run`, mean block error; `b1dk/par/table.md` has every map with its dluma)

| | all maps | bf1_kam1 | bf2_kam1 | bf2_cor1 | bf2_nab2 | bf2_mus1 | bf1_yav2 | bf1_bes2 | bf1_rhn2 | bf1_nab2 | bf1_geo1 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| main (no knob) | 10.618 | 19.29 | 16.71 | 5.94 | 8.81 | 7.93 | 7.29 | 11.23 | 10.75 | 10.22 | 11.60 |
| the PS2's ambients alone | 10.531 | 19.29 | 16.71 | 5.81 | 7.49 | 7.49 | 7.29 | 10.53 | 10.75 | 10.22 | 11.60 |
| display-unit sum alone | 10.100 | 14.48 | 12.28 | 8.50 | 12.13 | 8.52 | 4.74 | 10.74 | 9.17 | 8.77 | 10.21 |
| **sum + ambients** | **9.742** | 14.48 | 12.28 | 5.85 | 6.48 | 7.10 | 4.74 | 9.64 | 9.17 | 8.77 | 10.21 |
| the same, sum form 2 | 9.810 | 13.88 | 11.72 | 6.07 | 8.17 | 7.53 | 4.01 | 9.87 | 9.06 | 9.42 | 9.02 |
| sum + ambients + §7 | 9.707 | 13.61 | 12.28 | 5.85 | 6.48 | 7.10 | 4.74 | 9.64 | 9.17 | 8.77 | 10.21 |

With sum + ambients 23 maps fall by more than 0.05, 5 stay within it and 4 rise: bf2_yav1 +0.37 (one broken pose,
`walk/cp5_tflank_b270`, block 54 -> 68: the port's frame there is a wall), bf2_fel1 +0.31 (17 of 30 poses a little
up: the jungle's lit foliage, as FOLGL's leaves did under this sum, card LSUM's note), bf1_yav1 +0.20, bf1_end1
+0.13. No map fails the parity gate's 0.5. Form 1 against form 2 is not settled by the score: 2 is the GS's own
arithmetic and better on Kamino, 1 is better overall by 0.07 (the PS2 truncates twice at 8 bits under HDR's half
scale, and its deck reads 4-8 % under the exact product) — the build picks by the score.

### 4.2 BF1 Kamino's deck: the card's box (450,330,640,470) at the eight walk poses

| pose | PS2 | main | sum | sum + §7 (form 1) | sum + §7 + posts (form 1) | (form 2) |
|---|---|---|---|---|---|---|
| cp0_cp2 b000 | 65 79 83 | 41 46 52 | 61 70 79 | 63 72 82 | 64 76 84 | 68 79 86 |
| cp0_cp2 b090 | 63 72 79 | 40 45 50 | 59 68 77 | 60 69 77 | 61 72 79 | 65 75 82 |
| cp0_cp2 b180 | 78 91 101 | 42 48 55 | 62 72 82 | 68 79 90 | 69 82 91 | 73 85 94 |
| cp0_cp2 b270 | 85 102 116 | 43 49 58 | 62 73 85 | 75 88 102 | 76 91 103 | 80 95 106 |
| cp4_cp4 b000 | 73 82 92 | 42 46 52 | 61 70 80 | 63 73 82 | 66 76 85 | 70 79 88 |
| cp4_cp4 b090 | 61 72 77 | 40 44 49 | 59 67 76 | 59 68 77 | 62 71 80 | 66 74 82 |
| cp4_cp4 b180 | 71 84 94 | 42 47 54 | 62 71 82 | 67 78 89 | 70 81 92 | 74 84 95 |
| cp4_cp4 b270 | 85 102 116 | 43 49 58 | 62 73 85 | 75 88 102 | 78 91 105 | 82 94 107 |

Worst channel against the PS2: main 58; sum + §7 14 (form 2: 11); with the posts 13 (form 2: 10). The deck now changes
with the bearing, as the PS2's does. bf1_kam1's parity: main 19.29, sum 14.48, + posts 14.20, + §7 **13.44** (form
2: 13.23). **The prototype's posts are not BF1's**: they use BF2's s^2 weight and stand 0.32 m over every post, cp12
included; BF1's weight is s^4 and the light sits at the bone (§6). No s^4 render was made; at these poses the posts
are worth 1-3 levels either way.

### 4.3 BF2 Kamino

`bake_origin` = a scratch bake of KAM1 with §5's lights (`b1dk/scratch_bake_kam1.sh`; the control bake of the same
binary renders byte-identical to main's). The port's light list then holds the PS2's ten, place for place
(`PXC_LIGHTS=1` against `lead_tmp/bpnt/lights.py`).

| | bf2_kam1 parity | cp2_cp3_b000 midR / nearR | cp2_cp3_b090 (430,278,627,346) |
|---|---|---|---|
| PS2 | | 60 65 71 / 72 78 86 | 75 81 87 |
| main | 16.71 | 39 42 45 / 44 46 51 | 39 43 46 |
| + the origin lights | 15.40 | 49 53 57 / 57 60 66 | 49 53 57 |
| + the sum | 12.28 | 54 59 64 / 60 66 72 | 55 61 66 |
| + both | **11.74** | 64 70 76 / 73 80 87 | 64 71 77 |
| + both, form 2 | 11.42 | 66 72 77 / 75 81 88 | 67 73 78 |

The card's BF2 box (256,278,627,346) holds the PS2's own soldier (x 270-370), whose pack the local lights turn
near-white: its 82/84/89 is not the deck's level. The soldier-free right half is the last column.

## 5. BF2: a light hung on a hardpoint the model lacks sits at the carrier's origin [V][V-RAM][O]

### 5.1 The deck and the bunker (disc)
`kam_bldg_landplatform1` (both discs; the baked GLB): `kam1_bldg_plat1` is a fan of 24 triangles from a centre
vertex at the model's origin to a ring at r 13.27 m, `kam1_bldg_plat2` a 48-triangle ring from 13.27 to 22.18 m; every
normal (0,1,0); no COL0; MTRL 0x10089 (BF2) / 0x31 Specmap (BF1), power 4. The post and a `kam_bldg_bunker1` stand
within half a metre of that centre vertex. So whatever lights the centre vertex is spread over the whole fan by the
GS's screen-space blend (`re-vertex-colour-interp.md`), and a light at the origin with r 20 reaches the ring.

`kam_bldg_bunker1` (class base `building`) authors `AttachOdf kamlight` + `AttachToHardpoint hp_light_a`, the same at
`hp_light_b`, and `kamlight_blue` at `hp_light_c`; `kam_bldg_pod2` one `kamlight` at `hp_light_a`. `kamlight`: base
`Light`, Color 181 172 163, OmniRadius 20, no Static key, FlareIntensity 0; `kamlight_blue` the same with Color
2 50 100. The bunker's `skel` has ONE bone, `bunker_export`; pod2's one, `pod_2` (`b1dk/skel.py`). None of the
hardpoints exists.

### 5.2 The code
- `FUN_001d4db8` (B2 82625; its one caller `FUN_00213310`, 126688) walks the class's attach records. For a record
  whose flag bit 0 is set it copies the CARRIER's world matrix (82750-82766) into the matrix it hands the attached
  class's factory (82768), stores the object in the attachment table and calls `FUN_00451e58(object, carrier,
  record[0], 0)` (82823). Nothing tests the hardpoint first.
- `FUN_00451e58` (513481) clears the attachment's bone slot +0x2c (513511), then looks the hardpoint up in the
  carrier's pose (`FUN_00539150`, 513519) and, failing that, in the model's skeleton (`FUN_0057f1b8`, 513528). A miss
  leaves +0x2c at 0; the function neither returns early nor destroys the object, and ends with
  `FUN_004525c8(object, identity)` (513536).
- `FUN_004525c8` (513956) places a static attachment ONCE and then zeroes its hardpoint and parent link; only
  AttachDynamic classes are re-attached each update (`FUN_001d4970`) (verifier). With no bone the placement is the
  carrier's own transform [D; RAM below is the evidence].

### 5.3 RAM
`b1dk/origin_ram.py`: every live omni within 5 cm of a carrier instance's translation — **6 of 6 instances, 13
lights**:
- KAM1 three bunkers at (-60.954, 60.311, 303.491), (-54.555, 69.402, -47.853), (22.993, 78.213, -76.960): each two
  (181,172,163) r 20 and one (2,50,100) r 20, not Static; pod2 at (-14.630, 60.265, 102.251): one (181,172,163) r 20.
- POL1 `pol_prop_cavern` (skeleton: one bone `cavern_1`; lights at `hp_static`, `hp_static_1`): two Static
  (188,234,255) r 8 at (-17, 23, -20).
- DEA1 `dea1_bldg_beam_generator` (one bone; `hp_lighting`): one r 24, not Static, (129,129,164) = the class's
  (200,200,255) x its StrobeRandom intensity.
Over the conquest states of all 17 BF2 maps these 13 are the only such lights (verifier).

### 5.4 The port, and the scope
`bake/src/entity_light.rs` `attach` skips a hardpoint the skeleton lacks (`continue`), and a class with no skeleton.
`b1dk/attach_census.sh` bakes all 17 BF2 maps both ways: only KAM1 (0 -> 4 class lights on 2 classes = 10 placed),
POL1 (0 -> 2), DEA1 (15 -> 16) and UTA1 (5 -> 11) change. UTA1's six are `uta1_bldg_tower`'s `hp_tower0..5` (its
skeleton has none of them); the one placed tower is in a mode layer that is not conquest's, and conquest RAM holds
only the four of `uta1_bldg_supporter`, whose hardpoints exist — no stored state can check the tower [H].

What the three bunker lights sum to at full weight is (181,197,212)/128 in GS units. On the deck (N up, the lights
4 cm over the floor's plane) the VU's term gives about 0.25 x (1 - d^2/400)^2 of it: 0.37/0.40/0.43 at the centre
vertex, 0.11/0.12/0.13 at the 13.3 m ring.

## 6. BF1 lights its models with omni lights too [V][V-RAM][V-PS2][O]

Read by the helper (`b1dk/re/AGENT_BF1.md`, with its checks pasted); the lines below were re-read by the lead and its
censuses re-run. BF1 has ONE model light type, `RedOmniLight` (vtable 0x558f48), no Static bit, no spot, no bolt light.

- **Who takes them.** The RedModel draw `FUN_00496248` (B1 557165-557199): for a draw whose flags have none of 0x10,
  0x2000000, 0x20000 it calls `FUN_00493cf0(set, sphere, 4, static, shadow)` (557187 / 557190, its only callers, both
  with 4). The omni loop (556092-556174) walks the view's list `DAT_0069c5ec`, takes a light whose sphere meets the
  model's, scores it (0.3 R + 0.45 G + 0.25 B) x min(1, r^2/4d^2) / max(1, e^2/3600) and keeps the best four —
  BF2's rule. The set is published, then only its ambient word (+0x10 = black) and sun factor (+0x14 = 0) are
  rewritten and it is published again for segment flag 0x40 (557193-557198): **a prelit draw keeps its omnis**.
  Skinned draws go through the same draw (soldiers; read, not measured). The terrain takes none: its light pass
  `FUN_00487908` runs only when `DAT_005229b4` != 0 (549303, in the terrain draw `FUN_00487ad8`); ELF 0, no store.
- **The per-view list.** `FUN_00493ad0` (555930) files the global list through `FUN_00494b08` (556489): sphere r + 10
  against the frustum, skipped when the light's +0x50 mask has the view's bit.
- **The term** (BF1 VU1 0x21f-0x285, re-read): w = s^4 x att, s = 0.5 + ((L - p).N) x y ~ (1 + N.L)/2 (0x24a-0x252),
  squared at 0x254 and again at 0x256, att = (1 - min(1, d^2/r^2))^2 (0x244-0x250):

      colour = amb x mat + sum_sun C x mat x ((1 + N.l)/2)^4 + sum C x mat x ((1 + N.L)/2)^4 x (1 - min(1, d^2/r^2))^2

  one clamp at 255/128, then COL0 (multiply; add on a 0x40 segment). **BF1 differs from BF2's point term in the
  power only: 4, not 2** — an instruction diff of the two loops shows that, the sun's d^4 and `SQ.xyzw` for `SQ.xyz`.
  Edge-on a BF1 light gives 1/16 of its colour where BF2's gives 1/4.
- **The command post's light.** `FUN_0029c970` (226106) builds the post's `HoloOdf` hologram at the bone
  `hp_hologram` of the post BUILDING's model (226210), else at the building's matrix; the hologram's ctor
  `FUN_001e6370` (94393) makes an omni of range `HoloLightRadius` (8 on `com_holo_controlzone`) and, for `HoloType`
  aligned, a second — one per split-screen view (masks 2 and 1; only the first ever lights view 0). Its colour, every
  hologram update, is the viewer's team's relation colour x the hologram's intensity (a 2 s fade, a random one-update
  dip to 0.4-0.8, off for the last `HoloTurnOnTime` of a capture). The table (`FUN_002ad490`, 238528-238548;
  constants `FUN_00492598`): **friend (100,200,100), enemy (200,100,100), neutral (192,192,192), a side-5 "locals"
  team (223,223,32)** — BF1's friend is green where BF2's is blue.
- **Where it sits.** The bone: (0, 0.320, 0) in `com_bldg_controlzone`, (0, 1.363, 0) in
  `com_bldg_controlzone_large` (Kamino's cp12), (-2.52, 0.670, -0.14) in END1's bunker panel, (0.44, -1.649, 18.92) in
  HOT1's shield generator (`b1dk/re/hp_census.py`; RAM agrees on every post). BF2's port constant 0.32 has the same
  gap on a large post [D].
- **RAM** (the 150 battle states of the 15 `bf1_*` maps; `b1dk/re/omni_census.py`, `holo_rule.py`): hologram lights
  = 2 x the posts, range 8; the colour of 1856 of 1856 equals the rule (1836 on the render list; 20 are black and
  unregistered — a seventh HOT1 hologram no post owns, one RHN2 post off in one state). The Dune Sea's `tat1_*`
  states follow the same rule (verifier).
- **EntityLights.** BF1's `AttachToHardpoint` keeps an attachment only when the class model's skeleton HAS the bone
  (`FUN_001fb6e8`, 109163-109192: the count is incremented only when `FUN_00496f18` finds it) — the opposite of
  BF2 (§5). So Kamino's bunker lights do not exist in BF1 (RAM: the class's attach count is 0; no (181,172,163)
  light), and the port's existing skip rule IS BF1's. But BF1 does hold EntityLight omnis on eight maps: HOT1 43
  (`whitelight` (128,128,98) r 10 x 42 + one r 16), KAS1 49 (`redlight` (253,163,35) r 10 x 48 + a bonfire r 40),
  RHN2 7, BES2 4, END1 4, KAS2 4, GEO1 1 (RAM), TAT1 5 (disc replay, no state). The port bakes EntityLights for
  BF2 only (`bake/src/map.rs`).
- **Other makers**: mines (an EntityItem's `LightRadius`), and the explosion's `LightFlash` (range = `LightRadius` as
  authored, a linear fade; none of BF2's x20 / x3 / +3 m) — listed, not built here.

## 7. BF1's sun-coloured specular pass is fed the linear sun colour [V][O]

- The PS2: a BF1 Specular-class segment whose MTRL colour B is zero or white draws its pass with the four bytes of
  `DAT_0069c588`, the SunInfo colour, raw — 0x80 is 1.0 (`re-specular-pass.md` §2 [O], B1 573408-573425).
- The port: `game/src/assets.rs` `SpecPass::rgb_under(sun_color)` returns `sun_color x 255/128` for such a pass, and
  its three callers hand it the LINEAR colour (`viewer/scene.rs` the manifest's; `soldier_vis.rs` and
  `viewer/vehicles.rs` the renderer's copy), while `fs_spec` adds `min(t x rgb, 1) x base.a` in display units. For
  Kamino's (65,85,105) the pass colour is (0.105,0.181,0.281); the bytes over 128 are (0.508,0.664,0.820): 4.8 / 3.7 /
  2.9 x too weak. A near-white sun hides it (YAV1's (240,240,235): 1.73 against 1.88).
- On the frames: the PS2's deck gains 22/30/37 from bearing 090 to 270 (§4.2) — the sun's own ratio (65:85:105 =
  1 : 1.31 : 1.62; 22:30:37 = 1 : 1.36 : 1.68) — where the port's pass adds 3/5/9. With the colour encoded before
  the x 255/128 the port's b270 box goes from 62/73/85 to 75/88/102 (PS2 85/102/116), and bf1_kam1's parity falls by
  1.20 on main and by 0.87 on top of the display-unit sum. No other map's score moves by more than 0.005. (A ratio
  argument and a port measurement; no PS2 knock-out of the pass at bearing 270 was run.)
- BF2 is untouched: its pass colour is the MTRL's own (`rgb` baked).

## 8. What is still short

- **BF2's bearing-270 highlight.** At the stored pose `walk/cp2_cp3_b270`, box (430,340,640,470): PS2 157/163/171,
  the prototype 107/114/122 (main 76/79/84). §1.3's knock-outs at a turned pose split the PS2's highlight into the
  specular pass (about 10) and the HDR bloom it triggers (about 18): the bloom is card GLOWB's. The port's pass alone
  adds 34 at the stored pose, so the pass is not the short term [D].
- **BF1 bearing 270**: 10-14 under after everything (§4.2) [H: the PS2's highlight under the filter, or the sum form].
- **The local lights' spread across the fan.** At §1.3's pose the prototype's point term is 49/37/39 near the camera
  where the PS2's is 53/44/46 without bloom and pass, and 38/30/32 at the left where the PS2's is 28/24/26: the size
  is right, the distribution over the 13 m triangles is not exact [H: the port's guard-band weight against the PS2's
  clip-then-blend; the VU's fast reciprocal root, up to 9 % high].
- **BF1's cloning-room post** (`walk/cp3_cp12`): the prototype's post light makes the port's interior brighter than
  the PS2's (block 14.5 -> 16.1) — s^2 where BF1 has s^4, and 0.32 m where the large post's bone is 1.36 m (§6).

## 9. What the port owes

1. **Card LSUM (build; was `re`, deferred): the light sum in display units, and no ambient floor.**
   - `render/src/shader.wgsl` `shade_unfogged`: the Lit and Character paths' light = sum of `linear_to_srgb` of each
     baked colour x its weight (hemisphere ambient, sun x its term, Light2 x n.l) + the point sum, clamped at 255/128,
     then decoded once and multiplied into the linear texel (form 1), or the texel taken to display units, multiplied
     and clamped at 1 (form 2) — pick by the all-maps score (§4.1). The prelit paths' dynamic sun + Light2 and the
     prelit-add path's `COL0 + dynamic` the same way. `with_points` folds into it. The scratch WGSL is
     `b1dk/scratch_knobs.patch` (`K_GSUM`).
   - `bake/src/lighting.rs`: `AMBIENT_FLOOR` and `floor_ambient` removed (the authored ambient, even 0). Eight
     manifests move (`rebake=maps`).
   - Not measured here: soldiers and vehicles (parity masks the player and the pose render draws no unit). DSHN's
     note that the port's soldier base is darker than the PS2's (`re-spec-gloss.md` §8, card SBLT) is the same sum.
   **Built by card LSUM (2026-10-05, bf263f8 + goldens 9adbfc0).** `shade_unfogged` sums in GS units through
   `gs_lit` (the CPU twin is `render::lighting::light_sum`; the GPU referee `render/tests/light_path.rs`
   `the_lit_sum_is_made_in_display_units`: texel 160 under ambient 100 and sun 105 reads 128, the linear sum 87).
   `AMBIENT_FLOOR` is gone; the maps bake moved in the eight manifests of §3 and nowhere else. Measured on main after
   the rebake, all 954 pairs: 10.618 -> **9.742**, per map exactly §4.1's "sum + ambients" row (bf1_kam1 14.48,
   bf2_kam1 12.28, bf2_cor1 5.85, bf2_nab2 6.48; the largest rise bf2_yav1 +0.37, bf2_fel1 +0.31). Form 2 was built
   over the prelit paths as well and scored 10.052 (the prototype's 9.810 had it on the Lit path only), so form 1 is
   kept and the frame buffer's clamp at white is not modelled. COR1's plaza box: 41.8/47.1/57.0 against the PS2's
   42.7/46.3/54.2. Left as it was: the grass patches' own sum (`render/src/grass.wgsl`) still adds in linear light
   (unmeasured); soldiers and vehicles take the new sum and are brighter on every unit sheet, still unmeasured against
   the PS2 (card SBLT).
2. **Card B1DK (build, after LSUM): BF2's origin lights and the pass colour.**
   - `bake/src/entity_light.rs` `attach`: on a BF2 map a hardpoint the skeleton lacks, or a class with no skeleton,
     leaves the light at the carrier's origin (§5): KAM1 10 lights, POL1 2, DEA1 1 (and UTA1's tower class).
   - `game/src/assets.rs` `SpecPass::rgb_under` and its three callers: the sun colour in display units (§7).
   **Built by card B1DK (2026-10-05, 71c3855 + goldens e09d428).** `bake::entity_light::attach` takes a `Miss` rule
   (`Origin` on BF2, `Skip` on BF1; test `a_missing_hardpoint_is_the_origin_on_bf2_and_nothing_on_bf1`); the maps bake
   moved in four manifests (bf2_kam1, bf2_pol1, bf2_dea1, bf2_uta1) and nowhere else. `PXC_LIGHTS=1 pose_render`
   lists KAM1's ten r-20 lights at §5.3's positions, POL1's two at (-17, 23, -20), DEA1's one at (-105.00, 49.98,
   -107.93). `rgb_under` encodes the linear sun colour itself (the callers are unchanged). Measured on main, all 954
   pairs: 9.742 -> **9.689**; bf1_kam1 14.48 -> 13.61, bf2_kam1 12.28 -> 11.74, no other map by more than 0.005.
   BF2 deck boxes (PS2 / port): `walk/cp2_cp3_b090` (430,278,627,346) 75.5/80.7/87.0 / 64.4/71.2/76.5; `b000`
   (430,360,627,470) 72.1/77.5/85.9 / 73.0/79.6/87.2; the turned pose (430,278,627,336) 97.0/101.8/110.1 /
   85.7/91.2/98.8. BF1 deck box (450,330,640,470): cp0_cp2 b000 65.4/78.6/83.3 / 62.9/72.5/82.0, b090 62.5/71.7/79.0 /
   59.9/68.7/77.4, b180 77.5/90.9/100.7 / 67.7/78.8/89.7, b270 84.6/101.6/115.8 / 74.6/88.1/101.6; cp4_cp4 b000
   72.9/82.4/91.9 / 63.1/72.8/82.3, b090 60.8/72.2/76.7 / 59.2/67.9/76.6, b180 70.8/83.6/94.1 / 67.1/78.1/89.0, b270
   84.7/101.6/116.0 / 74.7/88.2/101.7. The worst channel is blue at bearing 270, 14.2 and 14.3 short (the card's bar
   was 14); what is left there is §6's local lights (card B1LL). UTA1's six tower lights are baked on the code alone
   (§5.4 [H]): no golden and no stored pose shows them.
3. **Card B1LL (new, build, after B1DK): BF1's local lights** (§6) — the posts' omni at the building's `hp_hologram`
   bone in BF1's relation colours (`still_lights` and the battle's `submit`), BF1's EntityLights at the bones that
   exist (the bake's present rule, un-gated for BF1: eight maps), and the s^4 point weight on a BF1 map.
4. **Notes**: GLOWB — §1.3's split of the bearing-270 highlight (pass 10, bloom 18) and the stored-pose box. LFLR / BPNT —
   `dea1_light_flicker` (StrobeRandom, r 24) joins the steady EntityLights; a large post's light is 1.36 m up, not
   0.32. BFC — the port's eye inside a `kam_prop_pillar` at `cp0_cp2_beta1_spawn` hides the best BF1 referee pose.

## 10. Checks, the verifier, what stays [H]

**Self-checks** (`b1dk/re/checks.txt`): `re_check.py fn` on every cited B1 / B2 line; `callers` of `FUN_00451e58` (3:
two in `FUN_001d4970`, one in `FUN_001d4db8`), `FUN_001d4db8` (1), `FUN_001d5260` (1), `FUN_00493cf0` (2, both in
`FUN_00496248`, both with 4), `FUN_00493ad0` (1), `FUN_00494b08` (1); `word` for the patch sites 0x494b74 (BF1,
0x0002102b) and 0x57fecc / 0x57fed4 (BF2); BF2 VU1 0x174-0x212 and BF1 VU1 0x21f-0x287 re-read with `vu1dis.py`.
The helper's own checks (refs.py, addrscan, rtti, widescan for the four unread hologram colour keys) are pasted in
its report.

**Instrument checks**: the scratch renderer with no knob = main's binary byte for byte (four poses) and scores 10.618
= main; the scratch bake with no knob renders byte-identical to the shipped bake; two PS2 controls per set (they
differ by 1-2 levels in a box).

**The verifier** (one foreground Opus agent, claims and citations only; `b1dk/re/CLAIMS.md` ->
`b1dk/verify/VERDICTS.md`): **27 SUPPORTED, 3 PARTLY, 0 NOT SUPPORTED** of 30. It recomputed every box, re-read the
RAM with its own code, re-rendered and re-scored seven maps (230 poses) to the same values, and checked every saved
variant state for its patch and its camera. Folded in: A5 (the 1.37-1.41 is the port's exact-sRGB arithmetic, a
2.2 power law says 1.40-1.46); D3 (cp12's light is 1.363 m up, the other six posts 0.320); D12 (line 549303 is in
`FUN_00487ad8`, which gates `FUN_00487908`); the four lights behind §1.3's knock-out and their split; the 1836 /
1856 registered hologram lights; the placed-once static attachment; the three `rgb_under` callers; the
`FUN_004b8e78` swap; and that the prototype's BF1 posts are BF2's s^2 at 0.32 m.

**Still [H] / not done**: no s^4 render of BF1's posts and no PS2 knock-out of BF1's pass at bearing 270; soldiers
under the display-unit sum (unmeasured); form 1 against form 2; UTA1's campaign tower lights; the hologram's update
rate and which post timer is capture; BF1's vertex alpha taking the light sum (`SQ.xyzw`); the composition inside
`FUN_00452988`; the EE side of BF1's upload beyond `FUN_004ac0d8` / `FUN_0039b968` (helper only).

## 11. Reproduction

`b1dk/`: `ps2/mk2.py`, `mk1.py`, `run.py` (the PS2 variants; SW renderer, a pcsx2 slot, no gpu.lock), `boxes.py`,
`fit_sum.py` (§2), `lights1.py` (BF1's omni list), `origin_ram.py` (§5.3), `skel.py`, `attach_census.sh`,
`scratch_bake_kam1.sh`, `port.py` (the scratch renderer at stored poses with knobs), `bf1_cp_render.py`,
`score_dir.py`, `par_table.py`, `scratch_knobs.patch` (apply to a detached worktree of 35c93e6); `re/` (the helper's
report and scripts, the checks, the claims), `verify/` (the verdicts and the verifier's own scripts). The patched
state copies (`ps2/states/`) were deleted; `mk*.py` re-makes them.
