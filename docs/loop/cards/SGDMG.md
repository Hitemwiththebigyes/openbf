---
id: SGDMG
title: A bolt that lands in its first update strikes at age 0 - BF2's shotgun pellet does its full 65 (head 195) up close, as PCSX2 shows, not the falloff's 48.3
milestone: M3
value: 5
phase: done
status: done
deps:
files: engine/crates/core/src/projectile.rs; engine/crates/core/src/sim/combat.rs; analysis/re-ordnance-kinds.md
declares: sim
needs:
effort:
branch: sgdmg
resume:
created: 2026-10-28
---
## Why
A playtest found the BF2 shotgun far weaker than on the PS2. The port billed every pellet as one update (1/30 s)
old when it first struck, so the age falloff (65 -> 15 over 0.1 s) cut it to 48.3: a point-blank body shot did
386 and left a 400-health soldier alive.

## Evidence
PCSX2, Dagobah, health reset before each shot, target 3 m ahead: every reading a multiple of 65 (520 = 8 body,
1560 = 8 head, mixes such as 1430 = 1 body + 7 head). `analysis/re-ordnance-kinds.md` section 7 has the table.

## Change
`Bolt::advance` adds a tick to the age only when the flight goes on, so the update in which a bolt lands uses the
age it had when that update began (the PS2 runs every ordnance of the bolt family, missile included, through
the same core and sweep). A falloff that drops (the shotgun) does one step more at impact; one that rises (the
SBD wrist rocket, 100 -> 250 over 0.5 s) one step less: its 3 m chest hit is 100 + 250 blast = 350 of a clone's
400, not 360. The moved pins: `v5_bolt_flight_time_to_20m_target` (hit age 7 ticks, not 8) and
`sbd::a_wrist_rocket_to_the_chest_on_coruscant_deals_its_hit_and_its_blast`, `sbd::sbd_fires_bolts_and_rockets_with_identical_gameplay_on_both_map_sets` (130 -> 120).

## Acceptance
`sim::combat::tests::a_pellet_landing_in_its_first_update_does_its_full_damage`: 65 body and 195 head at 3 m,
48.3 at 100 m. Mutation (the old age rule) fails it with 48.33.

## Open
The 10 m and 25 m PS2 readings (the steps past the first update) are not measured.
