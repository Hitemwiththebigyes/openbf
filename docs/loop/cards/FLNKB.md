---
id: FLNKB
title: Flank routes cross the arcs a mission blocks for the round: 6 of 10 on BF2 Kashyyyk (the wooden walls), 16 of 18 on Death Star (compactor, Connection115 / 146), 6 of 10 on Tantive IV (turbine) - leave blocked arcs out of the flank search (AUDIT FLNK)
milestone: M3
value: 4
phase: build
status: todo
deps:
files: engine/crates/core/src/nav/flank.rs, engine/crates/core/src/nav/flank/tests.rs, engine/crates/core/src/route_patch.rs
declares: sim; sweep; arena
needs:
effort:
branch:
resume:
created: 2026-10-20
alpha: yes
---
## Why
AUDIT FLNK (2026-10-20, PARTLY): flank routes cross arcs the mission closes to its AI for the whole round. In the port, ScriptPostLoad's BlockPlanningGraphArcs patches the route table (core::route_patch), and the port never unblocks (it destroys nothing). The flank search (nav/flank.rs flank_search) walks every soldier arc and never reads the patch, and neither does plan_leg's flank hop. The +35 % bound is measured against a main route the block has inflated. Death Star 27>32: main 847 m (135 m on the all-open table), flank 135 m, ratio 0.16; 10 of 18 Death Star routes are under 0.7 of their main route, against none under 0.87 on the other nine M5 maps.

## Evidence (engine/artifacts/lead_tmp/audit-flnk/tools/blocked.py: the disc plan, the bake's plnp record XORed on, the port's flank rules)
- BF2 KAS2 (mask 0b1111): blocked arcs 31 woodl, 32 woodc, 33 woodr; 6 of 10 flank routes cross one.
- BF2 DEA1 (mask 0b1): blocked arcs 65 Connection115, 81 Connection122, 97 compactor, 101 Connection146; 16 of 18 routes cross one.
- BF2 TAN1 (mask 0b10000): blocked arc 139 turbine; 6 of 10 routes cross it.
- What the bots do: on DEA1, flankers arrive 10 of 99 against 8 of 716 other attackers of the same pairs (sw/obs_b1/ep), so they walk ways the mission closed. On KAS2 the wall hop is refused at use (plan_leg's dead-arc / floor test) and the flanker detours to rejoin (snapshot bb/kas2_s7, unit 31: 2 -> 3 -> 29, then the table round to hub 32).
- The card's showcase gains rest partly on this: DEA1 B11 31.3 -> 26.4 (32 seeds) and B7 36.5 -> 28.6 are the largest of the M5 maps.
- The blocks are listed in game::mission (arc_blocks: DEA1 compactor, KAS2 the gate and the three wooden walls, TAN1 turbine). The audit's N1 agent's patched.py reproduced the port's DEA1 table (18 routes) only with the block applied.

## What to build
Leave the arcs a blocked group closes out of the flank search: the arcs the authored table uses that the patched table does not, which core::route_patch can compute exactly from record 0 against the current mask. Alternatively refuse a route that is shorter than its main route by more than a stated factor; pick by data and say why. Rebuild the table if the block mask ever changes in a round.

## Acceptance (run and paste)
- tools/blocked.py's count (routes through a blocked arc) is 0 on BF2 KAS2, DEA1 and TAN1, checked against the port's own table (a test or the sweep's flank line listing each route's arcs).
- A test with a blocked group on a small graph: the flank route must not use the blocked arc (mutation: drop the exclusion -> the test fails).
- 8-seed sweep: the other 29 maps' flank lines are unchanged (pairs / routes / shares); DEA1's B11 and B7 reported before and after.

## Planner note
Planner 2026-10-21 20:50: DESTR (the compactor vent / panels and TAN1's turbine as shootable objects) is the same object; read its census first.
