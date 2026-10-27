---
id: WADE
title: A wading soldier shows through the water: draw the skinned soldiers BEFORE the near water, as both PS2s do (opaque model segments bucket 5, the near flat water bucket 8 / 7, blended with Z write over them) - the maintainer's BF1 KAS2 stormtrooper ends at the belt, BF2 FEL1's shins are cut off; and the walkabout's soldier stands on the floor, not 0.59 m over BF2 YAV1's bazaar
milestone: M1
value: 5
phase: build
status: done
deps:
files: engine/crates/render/src/frame.rs,engine/crates/render/src/draw.rs,engine/crates/game/src/gallery.rs
declares: goldens=walkabout_stats.ron
needs:
effort:
branch: wade
resume: landed e8fc7b89 2026-10-06
created: 2026-09-28
defer: 
value_pre_alpha: 4
alpha: yes
---

## Why (rewritten 2026-10-06 after the capture + RE; the original card text is kept below)
The maintainer, playtest 2026-10-04: "Water still isn't transparent ... you can't see its body in the water"
(`playtest/playtest_20261004/2_kas2_stormtrooper_in_water.png`), and his walkabout findings of 2026-09-28: BF2 FEL1's
river cuts a soldier's shins off, BF2 YAV1's bazaar soldier stands ON a water sheet.
- **The water is not too opaque: the soldier is drawn after it.** The port's water writes depth and the skinned
  passes come after the water runs (`render/src/frame.rs`, "They stay after the water runs ... where the disc draws
  it against the model buckets is not in `analysis/`"), so everything under the surface fails the depth test.
- **Both PS2s draw him first** (`analysis/re-water.md` §9.1 [O]): opaque model segments are bucket 5, the near flat
  water bucket 8 (BF1) / 7 (BF2), blended `Cs·As + Cd·(1−As)` with Z test and Z write; buckets are kicked in
  ascending order. His legs show through the water with the same tint as the bed.
- **The stand is right in the sim, wrong in the walkabout**: a free PS2 soldier stands on the bed / floor at every
  depth (22 spots, the port's `Sim::ground_under` within 2.2 cm at 21). BF2 YAV1's bazaar floor is −72.818, 0.32 m
  under the plane; `gallery::ground_stand` puts the walkabout's soldier at max(terrain, the post's y) = −72.225.

## Evidence
- Pairs, PS2 | port (main 2560be8) | port without the water layer, a stormtrooper planted at the PS2 soldier's own
  feet: `engine/artifacts/pcsx2/ref/bf1_kas2/wade/pairs/s094.png` (0.96 m: the maintainer's picture reproduced) and `s148.png`
  (1.50 m), `bf2_fel1/wade/pairs/s061.png` (0.61 m) and `s118.png` (1.18 m), `bf2_yav1/wade/pairs/baz_n.png` (0.32 m);
  the PS2 with its water switched off beside it: `pairs/<spot>__ps2_on_off.png`. FINDINGS F65, F66.
- RE: `analysis/re-water.md` §9 (22 of 24 claims SUPPORTED by the verifier, 2 PARTLY folded in).
- Tools (scratch, `engine/artifacts/lead_tmp/wade/`): `ps2/wade_probe.py` (the PS2 stands), `tools/wade_pose.rs`
  (pose_render + one planted soldier), `tools/wade_check.py` (the referee), `tools/wade_ground.rs` (the port's floor
  at a point), `out/spots.json` (the five PS2 stands and poses the referee replays).

## What to build
1. `render/src/frame.rs`: the skinned `Opaque` draws and their `Spec` passes go BEFORE the first `Water` / `Speckle`
   run of the world loop (after Opaque / Mask / TerrainLayer / FarSheet / Spec); the water then blends over them.
   Skinned `Blend` draws stay in the blend pass (the PS2's translucent segments are drawn after the water and are
   hidden under it, §9.1). Ocean maps: nothing to see (the base is opaque).
2. `game/src/gallery.rs`: the walkabout's reference soldier (3 m in front of the eye) stands on the floor the sim
   gives under his spot (collision first, then terrain), not at the post's own height. Do not move `ground_view`'s
   stand unless the map goldens stay still.
3. Docs: the comment block in frame.rs, `DrawLayer::Water`'s doc, CRATES.md.
Not here: the drowning and the dead triggers (card WDRN), the wader's ripple and splash (note on FXP5), grass / leaf
patches against the water (not read on the PS2).

## Acceptance
- `flock engine/artifacts/loop/ci.lock python3 engine/artifacts/lead_tmp/wade/tools/wade_check.py --bin <branch
  target>/release/examples/wade_pose --out <dir>` prints **PASS**: at all five PS2 stands ≥ 90 % of the soldier's
  submerged silhouette shows with the water on, with a transmission of 0.3–0.95. Main 2560be8 prints FAIL:
  shown 22.0 / 5.8 / 14.3 / 9.8 / 72.9 %, transmission 0.18 / 0.05 / 0.17 / 0.08 / 0.82. Paste both tables.
- The five pair sheets rebuilt on the branch (`tools/pair.py <map> <spot> --tag w|floor`, with `BIN` pointed at the
  branch's binary) and VIEWED: the port's soldier shows through the water as the PS2's does.
- A test that pins the order (a skinned opaque draw is recorded before a water run) and one for the walkabout's
  spot at BF2 YAV1's bazaar (−72.80 ± 0.03, not −72.225).
- Gates: every golden that moves is one where a soldier stands in water, viewed and declared; nothing else moves.

## Original card text (2026-09-28)
The maintainer (INBOX 2026-09-28, walkabout findings (a) and (b)): two soldier-in-water defects on BF2 maps.
- (a) BF2 FEL1 `engine/artifacts/walkabout/bf2_fel1/cp1_cp6_1.png`: the soldier stands on the river bed and the water
  surface cuts his shins off — nothing below the waterline shows through. (Planner viewed it: all four bearings.)
- (b) BF2 YAV1 `engine/artifacts/walkabout/bf2_yav1/cp4_bazaar.png`: the bazaar floor is under a water sheet and the
  soldier's feet sit ON it.

### (original) Evidence / questions
- PCSX2 pairs first: a soldier wading at the FEL1 spot, and the YAV1 bazaar post (water height vs floor vs soldier).
- RE (a): how translucent the PS2's water is over a submerged unit — the water pass is ALPHA 0x44 (`analysis/re-water.md`
  §1), so why does the port's Water layer read opaque here? And where the PS2 stands a soldier in water: on the bed and
  wading, held at some depth, or depth-limited / killed (see DTHR).
- (b): a thin film he stands in, or the port's water too high, or his stand too high? The fix follows the PS2.
- Check whether the gallery's stand (walkabout: the terrain as floor) differs from the sim's stand in water.

### (original) Acceptance
- PCSX2 pairs for both spots viewed; an [O] RE note (re-water.md addendum) answering both questions; build card(s) — the
  render half here (M1), a sim half as its own card if the stand is wrong (M3).

## Notes
- Created 2026-09-28 by the planner. Related: HWAT (the hover float's WMAP wet grid — the same water data), DTHR, WCOL.
- 2026-09-29 (PXWB, lane 2): the PS2 side of the YAV1 bazaar is captured — the PS2 soldier at the bazaar post, four bearings, the port beside it: `pcsx2/ref/bf2_yav1/walk/pairs/cp4_bazaar__sheet.png` (+ the state `pcsx2/ref/states/bf2_yav1_walk_cp4_bazaar_b000.p2s`, the soldier held at his stand in a 0-AI battle). FEL1 cp1_cp6_1 comes with PXWB's FEL1 run.
- 2026-09-29 (PXWB, FINDINGS F54): FEL1 cp1_cp6_1 captured too — the PS2 soldier WADES, his shins visible through a bright turquoise river whose bed shows through; the port's river is darker and flatter: `pcsx2/ref/bf2_fel1/walk/pairs/cp1_cp6_1__sheet.png` (all four bearings; (0,300,640,470) b000 PS2 (54,85,72) vs port (49,65,46)); state `pcsx2/ref/states/bf2_fel1_walk_cp1_cp6_1_b000.p2s`.

## Planner note (2026-09-30, M-ALPHA)
`defer: post-alpha` — not needed for the alpha's six showcase maps or its checklist. Value 4 → 3 until the alpha (restore from `value_pre_alpha`).
- 2026-10-04 (the maintainer's playtest, via Claude; value 3 -> 5 and `defer` cleared at his word): "Water still isn't transparent. Screenshot 2 is empire stormtrooper standing in bf1 kas2 water and you can't see its body in the water." `playtest/playtest_20261004/2_kas2_stormtrooper_in_water.png`: the trooper's legs end in a hard line at the water, nothing shows below it. BF1 KAS2 joins FEL1 and YAV1 as a spot to pair.

- 2026-10-06 (lane 2, capture + RE; report `reports/20261006_1215_card_WADE.md`): the original acceptance is met - five PS2 stands on three maps paired with the port (`pcsx2/ref/<map>/wade/pairs/`), `analysis/re-water.md` section 9 [O], this card re-phased to build, card WDRN opened for the sim half (drowning, dead triggers), a note on FXP5 for the ripple and the splash. The stand itself needs no sim card: the port's floor is the PS2's.
- 2026-10-06 planner: `engine/CRATES.md` taken out of `files:`. The picker refuses a card whose files overlap a card in progress, and 22 open cards listed the map, so any one of them held the other 21 (a chain card in progress held WADE and WTHR). The map's row for a new or moved source file is still owed in the same branch (`scripts/size_report.sh --check-map`); a rebase conflict in it is one line. Undo: add the path back.
- 2026-10-06 21:57 (lane 2, build; report `reports/20261006_2157_card_WADE.md`): LANDED e8fc7b8 (+ goldens 68a615b). The skinned opaque draws and their spec passes go down before the first water run; `wade_check.py` PASS (shown 100 % at all five stands, T_legs 0.64 / 0.53 / 0.70 / 0.60 / 0.94); pair sheets `pcsx2/ref/<map>/wade/pairs/<spot>__branch_wade.png` viewed; the walkabout soldier stands on the floor under him (`gallery::soldier_spot`, no Sim: a golden test must not link core::sim). One golden moved, declared: walkabout_stats.ron (551 of 780 panels, 1 past tolerance).
- Follow-up for the planner (no card opened): `gallery::ground_stand` takes a CUT terrain as the floor, so the walkabout's eye floats 2.7 m over tat1 cp2's sunken hut (post 12.19, cut terrain 14.91) and the soldier, now on the real floor, is out of that sheet's frame; smaller at BF2 GEO1 cp8 (1.89 m) and BF2 HOT1 cp4 (0.83 m). Census of all 187 posts: `lead_tmp/wade/out/stand_census.txt`.
