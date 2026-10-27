---
id: BPRL
title: Vertex colour blends in screen space on the PS2 (the GS), cut only at the guard band: the port's perspective blend puts a post's pool on big floors under the camera (UTA1's pen +0/+16/+38 against +0/+1/+3, COR1's cp2 floor 103 against 70) - build the GPU-safe affine blend for model colour and point lights; and renders with no battle light the posts a mission kills (DEA1 CP6)
milestone: M1
value: 4
phase: build
status: done
deps:
files: engine/crates/render/src/shader.wgsl engine/crates/render/src/affine.rs engine/crates/render/src/frame.rs engine/crates/render/src/lib.rs engine/crates/game/src/point_light.rs engine/crates/game/examples/pose_render.rs engine/CRATES.md
declares: goldens=maps/bf1_bes1.png,maps/bf1_bes2.png,maps/bf1_end1.png,maps/bf1_geo1.png,maps/bf1_hot1.png,maps/bf1_kam1.png,maps/bf1_kas1.png,maps/bf1_kas2.png,maps/bf1_nab2.png,maps/bf1_rhn1.png,maps/bf1_rhn2.png,maps/bf1_yav1.png,maps/bf1_yav2.png,maps/bf2_cor1.png,maps/bf2_dag1.png,maps/bf2_dea1.png,maps/bf2_end1.png,maps/bf2_fel1.png,maps/bf2_geo1.png,maps/bf2_hot1.png,maps/bf2_kam1.png,maps/bf2_kas2.png,maps/bf2_mus1.png,maps/bf2_myg1.png,maps/bf2_nab2.png,maps/bf2_pol1.png,maps/bf2_tan1.png,maps/bf2_uta1.png,maps/bf2_yav1.png,maps/tat1.png,maps/tat2.png,tat1_hud.png,tat1_overview.png,tat1_spawn.png,units/bf1_all_inf_marksperson.png,units/bf1_all_inf_marksperson_snow.png,units/bf1_all_inf_pilot.png,units/bf1_all_inf_snowtrooper.png,units/bf1_all_inf_soldier.png,units/bf1_all_inf_trooperfleet.png,units/bf1_all_inf_wookie.png,units/bf1_cis_inf_bdroid.png,units/bf1_cis_inf_sbdroid.png,units/bf1_imp_inf_atatpilot.png,units/bf1_imp_inf_atstpilot.png,units/bf1_imp_inf_darktrooper.png,units/bf1_imp_inf_pilot.png,units/bf1_imp_inf_scout.png,units/bf1_imp_inf_shocktrooper.png,units/bf1_imp_inf_snowtrooper.png,units/bf1_imp_inf_stormtrooper.png,units/bf1_rep_inf_arctrooper.png,units/bf1_rep_inf_jettrooper.png,units/bf1_rep_inf_pilot.png,units/bf1_rep_inf_trooper.png,units/bf2_all_inf_bothanspy.png,units/bf2_all_inf_engineer.png,units/bf2_all_inf_marksperson.png,units/bf2_all_inf_markspersonsnow.png,units/bf2_all_inf_snowtrooper.png,units/bf2_all_inf_soldier.png,units/bf2_all_inf_tantive4trooper.png,units/bf2_all_inf_vanguard.png,units/bf2_all_inf_wookie.png,units/bf2_cis_inf_bdroid.png,units/bf2_cis_inf_magnaguard.png,units/bf2_cis_inf_sbdroid.png,units/bf2_imp_inf_atatcommander.png,units/bf2_imp_inf_atatpilot.png,units/bf2_imp_inf_darktrooper.png,units/bf2_imp_inf_gunner.png,units/bf2_imp_inf_scout.png,units/bf2_imp_inf_shocktrooper.png,units/bf2_imp_inf_snowtrooper.png,units/bf2_imp_inf_stormtrooper.png,units/bf2_rep_inf_arctrooper.png,units/bf2_rep_inf_clonecommander.png,units/bf2_rep_inf_ep3armoredpilot.png,units/bf2_rep_inf_ep3heavytrooper.png,units/bf2_rep_inf_ep3jettrooper.png,units/bf2_rep_inf_ep3sniper.png,units/bf2_rep_inf_ep3trooper.png,units/bf2_rep_inf_feluciatrooper.png,units/bf2_rep_inf_jettrooper.png,units/bf2_rep_inf_pilot.png,units/bf2_rep_inf_trooper.png,walkabout_stats.ron; parity=bf2_kam1
needs:
effort:
branch: bprl
resume: landed b9c79397 2026-10-03
created: 2026-09-30
alpha: yes
---
## Why
The PS2 does not skip the post's light on these floors (the RE's answer, `analysis/re-vertex-colour-interp.md`, docs
6bc2437). Its GS blends a triangle's vertex colour in SCREEN space, and the VU1 clipper cuts a triangle only beyond the
guard band; the port's GPU blends in perspective. With the camera on a post, a floor made of big triangles round a
vertex under the light shows the lit vertex's colour up the screen in the port and almost none of it on the PS2. The
same blend carries COL0, so such floors are also brighter or darker in the port with no light at all. UTA1 and MYG1
are showcase maps. A second, separate fault: a render with no battle lights posts the mission kills (DEA1's CP6).

## Evidence
- A raster model of the PS2's clip and blend, drawn from the disc at the PS2's pose, gives the PS2's measured gain on
  UTA1's pen floor: +1.2/+3.0 against +1.2/+2.9 (G/B); a perspective blend gives +20/+49
  (`lead_tmp/bprl/uta_pred.py`, `gsraster.py`; doc section 4).
- Prototype (`lead_tmp/bprl/affine_wclamp.patch`, scratch tree at 951e3e4, 954 stored poses, doc section 5): all maps
  10.672 -> 10.639; bf2_cor1 6.745 -> 5.945 (`walk/cp2_cp3` 5.99-7.00 -> 2.46-3.53; floor box 103/92/68 -> 70/63/48,
  PS2 70/65/51); bf2_tan1 -0.48, bf2_myg1 -0.18, bf2_uta1 -0.15, bf2_hot1 -0.13; bf2_kam1 +0.73, bf1_bes2 +0.31.
- Plain `@interpolate(linear)` is a trap: a black wedge on MYG1 `walk/cp4_cp5_con_b090` (box (416,384,627,470) 17.6
  against the PS2's 68.7) and a white frame at `bf1_bes2 cp0_cp4_postA_v2` (+84.9).
- DEA1: `dea1g_con` / `dea1c_con` call `KillObject("CP6")`; six post lights in PS2 RAM, none within 54 m of cp6;
  `point_light::still_lights` never applies the mission (doc section 6). `walk/cp5_cp6_b090` whole frame: PS2
  24.2/32.3/41.8, port 21.8/40.2/67.8.

## Acceptance (each one a check to RUN and paste)
1. The blend (`render/src/shader.wgsl`): model draws' vertex colour (rgb) and the point sum as
   persp(a wc) / persp(wc), wc = max(w, |x_view| / 5.093, |y_view| / 4.461, 0.1) - the guard in view angles so a
   wider window keeps the PS2's - behind one const that switches it off. A named deviation from perspective shading.
2. Referee test (new, CPU side): `gsraster.py`'s numbers as vectors - the weights for the pen's fan triangle
   (vertex 132 under the eye) at the stored pose give the box means of doc section 4 within 0.5 level; mutation
   check: wc = w fails it.
3. `scripts/parity.py`: bf2_cor1 <= 6.0 (`walk/cp2_cp3` four bearings <= 4.0), no map up by more than 0.5 except
   bf2_kam1 (declared: the post's pool had hidden a deck 1.8 x too dark, card B1DK), all-maps mean <= 10.65.
4. Boxes within 5 levels of the prototype's: COR1 `cp2_cp3_b000` (0,340,640,470) 69.9/62.8/47.6; UTA1
   `cp3_con_cp1_b000` card box B - R <= +5 (base +36, PS2 +7); MYG1 `cp4_cp5_con_b090` (416,384,627,470) >= 60;
   TAN1 `cp0_cp6con_b270` wall (0,230,640,330) and TAT2 `cp3_cp1_b000` whole frame unchanged within 1 level.
5. The stills: `still_lights` takes the posts of the map's conquest mission (roster, killed, owners) and
   `pose_render` the viewer's team from the pose JSON (`team`): DEA1 `walk/cp5_cp6_b090` whole-frame blue within 10
   of the PS2's 41.8 (today 67.8); bf2_dea1's parity better; HOT1 and KAM1 pairs re-scored with the PS2 player's team.
6. Goldens: VIEW every sheet that moves and declare them by name; a golden that turns darker or brighter near the
   camera on a big floor is the intent, anything else is not.

## Result (2026-10-03, landed b9c7939 + goldens f59f315; report `reports/20261003_0555_card_BPRL.md`)
- 1 done; 2 referee 0.12 / 0.22 level (bar 0.5), mutation fails; 3 all maps 10.618, COR1 5.945 (cp2_cp3 2.46-3.53),
  KAM1 +0.655 declared, nothing else over +0.32; 4 COR1 69.9/62.8/47.6, UTA1 B - R +0.7, MYG1 66.0, TAN1 / TAT2
  within 0.4; 5 DEA1 blue 42.3 (PS2 41.8), 552 of 558 PS2 post lights matched; 6 87 goldens declared and viewed.
- Changed from the card: the guard is in SCREEN units (the PS2's at any zoom), and `pose_render` takes no team - the
  pose JSON's `team` is the post's, the PS2 player is on team 1 in all 95 states.

## Notes
- Not in scope, left as they are until measured: terrain, water, sky, vertex ALPHA, and the per-pixel N.L of lit
  models (doc section 8).
- UTA1's pen floor ends 8-14 levels above the PS2 once the pool is gone (86/87/87 against 72/77/79): the floor's own
  level, a separate lead.
- The maintainer: this follows the PS2's look where it differs from "correct" shading. Reversible by the const. Say in INBOX if
  the port should instead keep the perspective blend as a modern deviation.

## The card as the RE found it (2026-09-30, kept)
### Why
BPNT (landed 2026-09-30) lights every model draw with BF2's local lights by `analysis/re-point-lights.md` §5–§7. On most
maps that is what the PS2 shows (parity over the 17 BF2 maps 11.22 → 10.43; TAT2's blue wall and red tower, TAN1's pink
wall, KAM1's and HOT1's corridors). On a few PRELIT models the port's post light is far stronger than the PS2's, so a
blue or red pool appears under a command post that the console does not draw. UTA1 is a showcase map.

### Evidence (`analysis/re-point-lights.md` §14 "Open")
- UTA1 cp3_con_cp1 b000, the pen floor (`uta1_bldg_lizardpen`, prelit_add, a floor vertex 0.35 m from the post's light):
  the PS2 gains +0/+1.2/+2.9 from its model omnis (`lead_tmp/bpnt/frames/u3__ctl.png` against
  `u3__code_57fed4_*.png`, box x 2–35 %, y 80–98 %; the diff image shows the soldier lit and the floor barely); the
  port gains +0/+15.8/+38.1 (`lead_tmp/bpnt/after_sheet.png` row 4).
- DEA1 walk/cp5_cp6 b090 (+10.0 block error) and b000; COR1 walk/cp2_cp3 (four bearings, +3.3):
  `lead_tmp/bpnt/dbg_worse.png`, `dbg_tat2.png` (PS2 | lights off | lights on).
- Ruled out by the build: the VU path (0x210 and 0x222 share the point loop at 0x23a), the MTRL diffuse (ff ff ff ff on
  every segment), the entity BASE (both `prop`), the gather's score (the post's light is the pen's only candidate).
- Unread: FUN_0057cf60's draw flags — 0x10 or bit 31 skips the upload (the last uploaded set stays), 0x4000000 takes the
  default set (B2 712763–712787) — and who sets them per entity or per pass.

### Acceptance
- RE: which model draws do not take the gathered set (the flags' writers and what they mean), [V] with the five checks
  and the verifier; a PS2 knock-in or RAM read of the pen floor's draw flags if the reading is not decisive.
- Build: the UTA1 pen floor box within ~5 levels of the PS2's gain; TAN1 cp0_cp6con b270's wall and TAT2 cp3_cp1 b000
  not worse; BF2 parity (`scripts/parity.py`) not worse on any map and better on DEA1 and COR1.

### Notes
- The port lights the posts for the mission's team 1 in a render with no battle (`game::point_light::still_lights`);
  the PCSX2 player is on team 2 on HOT1 and KAM1, so those stored pairs compare a pink port with a blue PS2. If the
  parity score is to judge this card, give `pose_render` the PS2 player's team first (read it from the state).
