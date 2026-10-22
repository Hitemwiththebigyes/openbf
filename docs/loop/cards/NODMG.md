---
id: NODMG
title: Shots that hit but do no damage (the maintainer's playtest 10-21): every bolt / bullet that ends in a unit-impact effect against the target's health on its next tick - per weapon, target class, map and game; name the case, fix it at the root, keep the census as a test (the Hailfire droid's chaingun on the player included)
milestone: M3
value: 5
phase: measure
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
first: 3
---
## Why
The maintainer (INBOX 2026-10-21 18:40 item 1): a bullet hits a unit, the impact particle shows, and the health does not always drop.
It hurts every fight. INBOX 20:00 item 5b adds a weapon that seems to do no damage to the player at all: start with the
Hailfire droid's chaingun.

## Do
1. Reproduce headless first: a census over a round per map (both games) - every shot that ends in a unit-impact effect
   (the effect the player sees) against a health change on that unit on the next tick. Split by weapon, target class,
   map, BF1 / BF2, and by the target's state: shield up, armour / damage scale, friendly fire, boarding, ragdoll / thrown,
   seated in a vehicle or turret, the tick it dies or spawns, a blast after a direct hit (order of damage and blast).
   The black box's `--events` view holds the hits; a scratch print beside it is fine.
2. Name each case where the effect fires and no damage lands, with its count. Some may be the PS2's own rule (a
   vehicle-scaled weapon on a soldier, a zero-damage class): check those against the ODF / `analysis/` before calling
   them bugs, and say which.
3. Fix the real ones at the root (the hit test and the effect must agree on what was hit).
4. The Hailfire chaingun at the player: a scene test (the player in front of a bot Hailfire, invincibility off) with
   the damage per second it should do from its ODF.

## Acceptance
- The census line before and after: impact effects with no health change on a damageable target, per game, from N to 0
  (or to the named PS2-rule cases only, each with its ODF line). The census kept as a test on a fixed scene.
- The chaingun test passes and fails under a mutation that drops its damage.
- Sweep / fidelity: only what the fix declares.

## Planner note
`files:` left empty for the measure phase (no code is written), so the parked ARVSA branch's combat.rs tests do not hold it back. At `phase=build` fill `files:` - likely engine/crates/core/src/sim/combat.rs, sim/blast.rs, sim/vehicles.rs; ARVSA (parked, waits on the RTX) adds tests to combat.rs and blast.rs: rebase on it once it lands.
