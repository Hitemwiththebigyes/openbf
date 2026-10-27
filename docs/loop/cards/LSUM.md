---
id: LSUM
title: The light sum in display units, and no ambient floor: the PS2 adds ambient + sun + points in GS units and multiplies by the texel once; the port sums linearised colours in linear light (Kamino's lit decks 1.4 x dark) and floors 8 maps' ambient at 60/255 - the prototype takes all 954 pairs 10.62 -> 9.74 with no map up by more than 0.5
milestone: M1
value: 5
phase: build
status: done
deps:
files: engine/crates/render/src/shader.wgsl,engine/crates/bake/src/lighting.rs,engine/crates/render/src/lib.rs,engine/crates/render/src/lighting.rs,engine/crates/render/tests/light_path.rs,engine/CRATES.md
declares: goldens=maps/bf1_bes1.png,maps/bf1_bes2.png,maps/bf1_end1.png,maps/bf1_geo1.png,maps/bf1_hot1.png,maps/bf1_kam1.png,maps/bf1_kas1.png,maps/bf1_kas2.png,maps/bf1_nab1.png,maps/bf1_nab2.png,maps/bf1_rhn1.png,maps/bf1_rhn2.png,maps/bf1_yav1.png,maps/bf1_yav2.png,maps/bf2_cor1.png,maps/bf2_dag1.png,maps/bf2_dea1.png,maps/bf2_end1.png,maps/bf2_fel1.png,maps/bf2_geo1.png,maps/bf2_hot1.png,maps/bf2_kam1.png,maps/bf2_kas2.png,maps/bf2_mus1.png,maps/bf2_myg1.png,maps/bf2_nab2.png,maps/bf2_pol1.png,maps/bf2_tan1.png,maps/bf2_uta1.png,maps/bf2_yav1.png,maps/tat1.png,maps/tat2.png,tat1_hud.png,tat1_overview.png,tat1_spawn.png,units/bf1_all_inf_marksperson.png,units/bf1_all_inf_marksperson_snow.png,units/bf1_all_inf_pilot.png,units/bf1_all_inf_snowtrooper.png,units/bf1_all_inf_soldier.png,units/bf1_all_inf_trooperfleet.png,units/bf1_all_inf_wookie.png,units/bf1_cis_inf_bdroid.png,units/bf1_cis_inf_sbdroid.png,units/bf1_imp_inf_atatpilot.png,units/bf1_imp_inf_atstpilot.png,units/bf1_imp_inf_darktrooper.png,units/bf1_imp_inf_pilot.png,units/bf1_imp_inf_scout.png,units/bf1_imp_inf_shocktrooper.png,units/bf1_imp_inf_snowtrooper.png,units/bf1_imp_inf_stormtrooper.png,units/bf1_rep_inf_arctrooper.png,units/bf1_rep_inf_jettrooper.png,units/bf1_rep_inf_pilot.png,units/bf1_rep_inf_trooper.png,units/bf2_all_inf_bothanspy.png,units/bf2_all_inf_engineer.png,units/bf2_all_inf_marksperson.png,units/bf2_all_inf_markspersonsnow.png,units/bf2_all_inf_snowtrooper.png,units/bf2_all_inf_soldier.png,units/bf2_all_inf_tantive4trooper.png,units/bf2_all_inf_vanguard.png,units/bf2_all_inf_wookie.png,units/bf2_cis_inf_bdroid.png,units/bf2_cis_inf_magnaguard.png,units/bf2_cis_inf_sbdroid.png,units/bf2_imp_inf_atatcommander.png,units/bf2_imp_inf_atatpilot.png,units/bf2_imp_inf_darktrooper.png,units/bf2_imp_inf_gunner.png,units/bf2_imp_inf_scout.png,units/bf2_imp_inf_shocktrooper.png,units/bf2_imp_inf_snowtrooper.png,units/bf2_imp_inf_stormtrooper.png,units/bf2_rep_inf_arctrooper.png,units/bf2_rep_inf_clonecommander.png,units/bf2_rep_inf_ep3armoredpilot.png,units/bf2_rep_inf_ep3heavytrooper.png,units/bf2_rep_inf_ep3jettrooper.png,units/bf2_rep_inf_ep3sniper.png,units/bf2_rep_inf_ep3trooper.png,units/bf2_rep_inf_feluciatrooper.png,units/bf2_rep_inf_jettrooper.png,units/bf2_rep_inf_pilot.png,units/bf2_rep_inf_trooper.png,walkabout_stats.ron; rebake=maps; parity=bf2_cor1,bf2_mus1,bf2_nab2,bf2_yav1
needs:
effort:
branch: lsum
resume: landed bf263f81 2026-10-05
created: 2026-09-26
defer: 
value_pre_alpha: 4
alpha: yes
---

## Build (written by card B1DK's RE, 2026-10-05; `analysis/re-kamino-decks.md` §2-§4, §9) - this section is the card now
**Why.** Kamino's decks are 1.5-2 x darker than the PS2's and the largest term is this one: the PS2 adds ambient +
sun (+ Light2, + points) in GS units and multiplies by the texel once; `shade_unfogged` adds the linearised colours in
LINEAR light. Where two terms are of one size (Kamino: ambient 100, sun (65,85,105)) the port is 1.37-1.40 x dark.
The maintainer's own BF2 Kamino frame shows it (`playtest/playtest_20261004/4_kam1_bots_bunched.png`: a near-black deck).

**The contradiction this card was waiting on is resolved.** The display-unit sum made COR1 / NAB2 / MUS1 worse because
`bake::lighting::AMBIENT_FLOOR` (0.045 linear = 60/255 in display units) gives them an ambient the PS2 does not have
(COR1's Top / Bottom are 0 in RAM). With the PS2's own ambients no map gets worse.

**Evidence** (`engine/artifacts/lead_tmp/b1dk/`; the doc has the tables):
- PS2, BF2 KAM1 deck, patched states: the ambient and sun terms add in display units to 1.5 levels (`fit_sum.py`);
  the linear sum would be 14-18 levels lower.
- Scratch prototype over all 954 pairs (`par/table.md`, knobs in `par/README.md`): main 10.618; the sum alone 10.100
  (COR1 +2.55, NAB2 +3.32); **the sum + the PS2's ambients 9.742**, 23 maps down, none up by more than 0.5 (bf2_yav1
  +0.37, bf2_fel1 +0.31); bf1_kam1 19.29 -> 14.48, bf2_kam1 16.71 -> 12.28, bf1_yav2 7.29 -> 4.74, bf2_nab2 8.81 -> 6.48.
- COR1's plaza floor (LITE's box (420,300,640,440) at `cp1_cp2_postA_spawn`): 41.8/47.1/57.0 against the PS2's
  42.7/46.3/54.2.

**Build.**
1. `render/src/shader.wgsl` `shade_unfogged`: the Lit and Character paths' light = the sum of `linear_to_srgb` of each
   baked colour x its weight (hemisphere ambient, sun x its term, Light2 x n.l) + the point sum, clamped at 255/128;
   then form 1 (decode once, times the linear texel) or form 2 (the texel to display units, multiply, clamp at 1,
   decode) - score both, keep the better all-maps mean (prototype: 9.742 / 9.810; form 2 is the GS's own arithmetic
   and better on Kamino). The prelit paths' dynamic sun + Light2 and the prelit-add `COL0 + dynamic` the same way.
   Scratch WGSL: `lead_tmp/b1dk/scratch_knobs.patch` (`K_GSUM`).
2. `bake/src/lighting.rs`: remove `AMBIENT_FLOOR` / `floor_ambient` (8 manifests move: bf1_bes2, bf2_cor1, bf2_geo1,
   bf2_kas2, bf2_mus1, bf2_nab2, bf2_tan1, bf2_yav1).
3. A referee on the sum itself (a CPU twin of the shader's sum and a GPU pixel test: one lit texel under a known
   ambient and sun reads T x (A + S x n.l) in display units, not the linear sum's value).

**Acceptance (run and paste).**
- `scripts/parity.py`: all 954 pairs <= 9.80 (main 10.618); no map up by more than 0.5 against main; bf1_kam1 <= 14.6,
  bf2_kam1 <= 12.4.
- COR1's floor box above within 6 levels per channel of the PS2's.
- The pixel referee of step 3, mutation-checked (the linear sum must fail it).
- `declares`: every map golden, the tat1 goldens and `walkabout_stats.ron` are listed; the unit sheets move too
  (soldiers take the same sum) - add the ones the gate names. View the sheets: lit models get brighter wherever
  ambient and sun are both strong; COR1 / MUS1 shadows lose the 60/255 floor.

**Notes.**
- Soldiers and vehicles are unmeasured (parity masks the player; the pose render draws no unit). DSHN's "the port's
  soldier base is darker than the PS2's" (card SBLT) is likely this sum - re-measure SBLT's boxes after landing.
- bf2_fel1 +0.31 (17 of 30 poses a little up) and FOLGL's leaves (the note below): lit foliage overshoots under this
  sum. If FEL1 passes the gate, leave it and note the number on card FOLG / SBLT; do not special-case foliage here.
- B1DK (the origin lights, the pass colour) and B1LL (BF1's local lights) build on top of this card.

The text below is the card as it stood before the build was specified.

## Why
The PS2's VU1 computes a model's vertex colour as `amb·mat + Σ Cᵢ·mat·termᵢ` in GS units, and the GS multiplies it into
the gamma-space texel once. The port linearises each colour (`bake::lighting`'s transfer rule) and sums in LINEAR space.
That is exact for a single product but not for a sum or a weight. It reshapes every mid-tone on every lit model, in both
games.

## Evidence
`analysis/re-sun-direction.md` §4.3. The SUND scratch knob `SUND_GSUM=1` sums in GS units and linearises once
(`openbf-wt/sund-scratch`, shader.wgsl). The census is `lead_tmp/re/sund/census_pair.py`:
- **BF1**, with SUND's direction, d⁴ and prelit rule (`ps2d4s → ps2d4s_g`): 67,927 blocks, 14.76 → 10.06, 71.9 % closer.
  - d⁴ against Lambert: 65 % closer under the GS sum, 51 % under the linear sum.
  - All of SUND plus the GS sum: 82 % closer (22.6 → 9.3).
- **BF2** (`ps2 → ps2_g`): 48.5 % closer overall.
  - Better: KAS2 85 %, POL1 85 %, TAT2 80 %, KAM1 62 %.
  - Worse: COR1 3 % (5.5 → 21.3: the floor turns too bright, `lead_tmp/re/sund/sheets/lsum_bf2.png`), MYG1 11 %, MUS1
    4 %, NAB2 31 %.
- LITE's own COR1 measurement matched the PS2 WITH the linear sum (`re-model-lighting.md` §8).

## Acceptance
RE / measure, after SUND lands. Establish why BF2 COR1 / MYG1 contradict the GS-unit sum before building it. Candidates:
- the port's `AMBIENT_FLOOR`;
- the Light.Color and COL0 linearisation;
- the VU clamp at 255/128 under HDR's 0.5;
- COR1's prelit share and Static sun.

Then either a build card (the Lit / Prelit sums in GS units, per game if the evidence splits by game), or close with the
reason. Patched PCSX2 states (`lead_tmp/re/lite/ps2exp/`) can isolate one term on the PS2 itself.

## Notes
- The scratch knob's WGSL is the starting point: sum `linear_to_srgb` of each baked colour × its weight, clamp at
  255/128, then `srgb_to_linear` once (`openbf-wt/sund-scratch` diff; it is removed at SUND's cleanup — the diff is saved
  at `lead_tmp/re/sund/scratch_knobs.patch`).
- **FOLGL data point (2026-09-28, BF2 FEL1, `analysis/re-foliage.md` §16).** At the four PCSX2 poses, with the masks from
  `lead_tmp/re/folgl/measure.py`:
  - Sun-lit caladium leaves (`fel1_prop_foliage_d`): the linear sum is +1.4 −0.2 −1.1 against the PS2; the Lit sum in
    GS units is +6.9 +5.3 +2.1 (worse).
  - `fel1_prop_foliage_b`: −3.7 −2.9 −6.3 linear, −3.1 −2.1 −6.1 in GS units.
  - The same shader edit is saved at `lead_tmp/re/folgl/gsum_knob.patch`. One more BF2 map where GS units lose.
- 2026-09-29 (PXWB, lane 2; FINDINGS F49): BF2 TAT2's sun and shade — the PS2's shaded walls are neutral/cool grey, its sunlit walls bright pink-orange; the port paints both a flat warm beige: `pcsx2/ref/tat2/walk/pairs/cp1_cp2__sheet.png` b180 (20,300,200,440) (85,90,86) vs (90,80,60); `cp5_cp8__sheet.png` b180 (300,80,620,300) (76,78,77) vs (80,69,52); `cp3_cp1__sheet.png` b000 (300,20,620,200) (157,179,183) vs (158,144,115); `cp2_cp6__sheet.png` b000 the sunlit tower (60,40,200,300) (186,139,112) vs (123,112,89). The HUE of the shade (blue channel +25..+68 on the PS2) says the ambient's colour, not only the sum's colour space.

## Planner note (2026-09-30, M-ALPHA)
`defer: post-alpha` — not needed for the alpha's six showcase maps or its checklist. Value 4 → 3 until the alpha (restore from `value_pre_alpha`).
