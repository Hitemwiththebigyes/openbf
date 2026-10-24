---
id: MYGP
title: BF2 Mygeeto: hover speeders get stuck on the slope of the pit at (-70, -1.5, 179) and their riders get out and fall into the death region - 35 such deaths in 96 rounds on main; trace three, check the PS2's hovers there, fix
milestone: M2
value: 4
phase: measure
status: blocked
deps:
files:
declares: sim; sweep; arena
needs: environment — restore MYGP map/mission, raw MOUTH evidence and PS2 capture inputs; exact paths in lead_tmp/mygp_20261024_0959/inventory.json
effort:
branch:
resume: Restore Mygeeto/common BF2 bake, classes/vehicles, bf2_myg1_con.ron, mouth/build/worlddeaths.py plus raw BOT_SAMPLES, and PCSX2 README/session/ps2live with local BF2 disc/state. See reports/20261024_0959_card_MYGP.md and lead_tmp/mygp_20261024_0959/inventory.json. Then rebuild current main, trace three pit cases including riders/route/hover graph, capture PS2 vehicles for 600 s, remeasure 96 Mygeeto rounds and all-map recycles over 24 seeds before cause/fix.
created: 2026-10-12
alpha: yes
domain: vehicle
---
## Why
Found by MOUTH's build (2026-10-12) while it sorted a rise in death-region deaths: on BF2 Mygeeto, a showcase map, hover
speeders get stuck on the slope of the pit at (-69.7, -1.5, 178.6) / (-80, 162); their two riders get out and fall into
the death region. It is on main and not MOUTH's: 35 such deaths in 96 rounds before MOUTH, 37 with it.

## Evidence
- `engine/artifacts/lead_tmp/mouth/build/worlddeaths.py <BOT_SAMPLES csv>` (lives ended with no kill event: the spot and
  the last 3 s), seed 23; `loop/reports/20261012_1331_card_MOUTH.md`, "Death regions".
- Main's own 24-seed blocks read 4 / 7 / 9 / 15 deaths in the region.

## Acceptance
1. Three cases traced on today's main (a headless snapshot at the tick, `blackbox_read --unit`): how the hover came onto
   the slope (its route's leg, the hover graph's end, a shove), why it cannot leave, why its riders get out there.
2. The PS2 at the same place, one line: do its hovers go near the pit (`ps2live.py`, BF2 Mygeeto, 10 minutes of vehicle
   positions)?
3. The cause named and fixed, or split into a build card with the numbers. Bar: deaths in Mygeeto's death region at most
   10 in 96 rounds (main 35); no map's recycles up over 24 seeds.

## Cloud measurement checkpoint (2026-10-24)
Blocked on restoration at main `3d487d9`; no new measurement or cause verdict.
18 of 19 checked paths absent; inventory and scoped alternate search: `lead_tmp/mygp_20261024_0959/inventory.json`.
MOUTH report remains present (lines 44-46); its 35/96 and 4/7/9/15 blocks are historical, not today's baseline.
Three current-main snapshot traces, 600 s PS2 positions, <=10/96 region deaths and no map recycle rise over 24 seeds remain owed.
Report: `reports/20261024_0959_card_MYGP.md`. Original acceptance preserved.
