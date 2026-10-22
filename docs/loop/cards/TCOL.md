---
id: TCOL
title: Building turrets and Laik cannons are whole bodies: bolts, rockets, blasts and grenades stop on the upper half and hurt it, the damage reaches the gunner by the PS2's rule, and a turret is destroyed (wreck, respawn or pad) if the PS2's is
milestone: M2
value: 5
phase: re
status: todo
deps:
files:
declares:
needs:
effort:
branch:
resume:
created: 2026-10-21
alpha: yes
first: 4
---
## Why
The maintainer (INBOX 2026-10-21 18:40 item 2, BF2 Geonosis?): rockets, bullets and grenades pass through the UPPER half of a Laik
cannon and hit its base; its health never drops; its gunner takes no damage. A building turret has no hull collision
today (TURRA: "a building turret has no hull"); RKHUL fixed walkers by putting each collision piece where its bone binds,
RKBARC does the BARC.

## Do
1. RE (both games): the turret's collision prims and their PRNT bones (RKHUL's rule); does a seated unit take the
   turret's hits, the blast, both or neither; can a building turret's health reach 0, and what then (wreck, respawn
   time, TBLD's pad, an explosion). Census every building turret / cannon class on both discs.
2. Build: every building turret and cannon gets its whole shape in the collision world, bolts / rockets / blasts /
   grenades damage its pool, the gunner rule as read, destruction as read.

## Acceptance
- A repro per class (BF2 GEO1's Laik cannon first): rockets and bolts aimed at the upper half stop on it N/N (was 0/N
  where it passes today), its health falls by the weapon's damage, the gunner's health moves by the PS2's rule.
- A census over every building turret class on both discs: bodies per class against the PS2's prims.
- Related cards: VTHUL (a bolt at the building damages the turret's pool), TURRA (exit 3 reads the hull hits), TBLD
  (a destroyed turret's pad). Batch with VTHUL if the files meet.

## Planner note
`files:` left empty for the re phase (no code is written), so the parked ARVSA branch's combat.rs tests do not hold it back. At `phase=build` fill `files:` - likely engine/crates/core/src/sim/vehicles.rs, sim/combat.rs, sim/blast.rs; ARVSA (parked, waits on the RTX) adds tests to combat.rs and blast.rs: rebase on it once it lands.
