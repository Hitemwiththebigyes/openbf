---
id: WTHIN
title: Soldiers through thin and narrow geometry (the LEAK census's tunnel residue after WTUN, 1,427 runs): slots narrower than the soldier's 1 m capsule (BF1 NAB2 (-13, 17), a hip-high slab 0.54 m from a sheet) and props of 0.2-0.4 m parts (BF2 POL1 body 837), where the pass's one contact per body and two iterations fight each other — name the classes by count, then decide the rule (keep him out of the slot, or resolve all contacts jointly)
milestone: M3
value: 3
phase: re
status: blocked
deps: WTUN
files:
declares: leak; sweep; arena; sim
needs: Restore NAB2/POL1/MYG1 bakes/missions, tunnel sites/probe and audit band data; classify current-main mechanisms and retain float/closed controls.
branch:
resume: 2026-10-24 11:57 UTC: planner prerequisite hold; 7/7 exact named paths absent. Restore NAB2/POL1/MYG1 bakes/missions, tunnel sites/probe and audit band data; classify current-main mechanisms and retain float/closed controls. All original acceptance retained. reports/20261024_1151_planner.md; inventory/continuation lead_tmp/planner_20261024_1151/.
created: 2026-09-29
alpha: yes
---
## Why
WTUN (analysis/re-soldier-walls.md §4.3) removed the two skip mechanisms; what is left of the census's `tunnel` class is geometry the capsule cannot fit, where a push out of one face lands the axis past another. Player-visible but rare (1,427 of 1.83 M runs; top sites 24, 22, 21 runs).
## Evidence
- Sites: `lead_tmp/wtun/sites_v3.txt` (grep ' tunnel '). Probe: `lead_tmp/wtun/tools/wtun_probe.rs` (a scratch example; add parry3d as a game dev-dependency).
- NAB2 walk start (-13.10, 20.75) yaw 0.0068: tick 28 body 28 pushes -Z 0.35 into a 0.54 m slot, body 836's sheet (z 17) answers too little, tick 29 the axis is past it.
- POL1 sprint start (-122.93, -143.47): body 837, 14 triangles in contact, pushes +Z 0.37 / -Z 0.17.
## Acceptance
Classify the residue by mechanism with counts; fix what is the pass's own (e.g. all contacts of a body resolved together, or refuse the move into a slot); the census's accepted `* tunnel` line re-points; float/closed must not rise (MYG1 crevice trap, WPSH).

## Planner note (2026-09-30, M-ALPHA)
`alpha: yes` — no soldier out of bounds. Value 3 (kept). Showcase maps: BF1 NAB1, BF1 TAT1 (the port's `tat1`), BF1 YAV2, BF2 UTA1, BF2 MYG1, BF2 HOT1 (PLAN §Milestones M-ALPHA).

## AUDIT note (2026-09-30, LEAK → LEAK3)
The tunnel residue (1,427 runs) is counted by a sampler that reaches only part of the walls.
- The audit's band sampler (`lead_tmp/audit-leak/`) finds 154 more tunnel sites / 549 runs.
- Examples: BF1 END1 body 997 at (74–134, −318…−342), from a deck at y 93.25; BF2 POL1 box 119 at (122, −42), 54 runs;
  BF1 BES2 box 682 at (−82, 2).
- Re-count with LEAK3's sampler before closing.
