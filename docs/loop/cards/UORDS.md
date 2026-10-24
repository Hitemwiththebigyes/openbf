---
id: UORDS
title: Bolts are swept every tick against units, as on the PS2: the predictive spawn ray tests only terrain and buildings (BF1 0x48 / BF2 0x90), soldiers / vehicles / riders / water / foliage / shields are struck by a prev->current sweep each frame (0x996 / 0x132e) - a soldier who steps out of a bolt's line is missed, one who steps in is hit; the port's one predictive ray against everything is the opposite
milestone: M3
value: 4
phase: build
status: blocked
deps:
files: engine/crates/core/src/projectile.rs, engine/crates/core/src/sim/combat.rs, engine/crates/core/src/sim/ordnance.rs, engine/crates/core/src/weapon.rs
declares: sim; sweep; arena
needs: environment restoration — PS2 bolt captures/tables, baked maps/classes/vehicles, missions and approved gate device
effort:
branch:
resume: 2026-10-24 08:56 UTC cloud prerequisite checkpoint: restore audit-uord PS2 bolt captures/tables and probes, real baked maps/classes/vehicles and missions plus approved gate device; then current-main baseline, isolated build and full original acceptance. See engine/artifacts/loop/reports/20261024_0856_card_UORDS.md and lead_tmp/uords_20261024_0854/inventory.json. No implementation or landing.
created: 2026-10-22
alpha: yes
---
## Why
analysis/re-ordnance-kinds.md §2 (card UORD, [O], two verifiers): every class of the bolt family (bolt, laser, bullet; also missile and shell) shares one flight core and one per-frame sweep. B1 FUN_002764c0 casts ONE predictive ray at the first update with mask 0x48 (buildings + terrain only) when class+0x88 = 1; the per-frame sweep FUN_00273128 (called by FUN_00163198 for every moving ordnance) rays prev -> current with 0x9de until that ray ran, 0x996 after: soldiers 2, vehicles 4, riders 0x10, water 0x80, foliage 0x100, shields 0x800. BF2 FUN_00313600 / FUN_0030fad0: 0x90, 0x13be / 0x132e. This corrects re-blaster-damage §5.1 ('targets cannot dodge'), which the port's core/src/projectile.rs transcribes; its test predictive_raycast_hits_dodgers_and_ignores_late_entrants (sim/combat.rs:739) asserts the opposite of the PS2.

## Evidence
- The doc §2.2 (lines B1:197629-197656, B1:195510-195514, B1:2731-2738; B2:304258-304304, B2:301923-301925); verifier batch 1 C3/C4 SUPPORTED, T2 SUPPORTED by both absence readers.
- Foliage contact (B1 layer 0x100 / B2 0x200) returns 0 in the base collision (B1:195309-195310): the bolt flies on.
- Life-out spawns ExplosionExpire (class+0x34 / +0x48) for every class of the family (B1:195365-195367): the port spawns none for a bolt; [H] a bolt whose predictive ray hit terrain may fire BOTH ExplosionExpire and the impact explosion (BF1 jet trooper rocket authors both) - check on the PS2 before building that part.

## Acceptance
- Scene tests, mutation-checked: a soldier who steps out of the line after the shot is missed; one who steps in is hit; a wall / terrain hit still ends the bolt at the predicted point; foliage does not stop a bolt; water does.
- A PS2 check of one dodge or one late entrant (lead_tmp/uord/ps2/missile_b1.py style sampler on a laser instance, vtable 0x53a568) printed beside the port's.
- 8 seeds x 32 maps 256 / 256; kills a minute and team kills per map within the seed noise, or the move named for the planner (it is a change to every firefight).

## Notes
- Missiles and shells already need the full-mask per-tick sweep (card UORD): build the shared sweep once; whichever card lands second reuses it.
## Note (AUDIT 2026-10-22, lane 2): the PS2 side of the acceptance check exists
- `analysis/re-ordnance-kinds.md` 2.4 [M], both games: a bolt's line written through a stationary soldier's chest
  after its predictive ray ran stops one update later at the chest (BF1 15 / 15, 0.35-0.59 m short of the chest
  centre; BF2 15 / 15, 0.17-0.50 m) and hurts him (14 / 15, 15 / 15); the line 2.5 m aside flies on (14 / 15, 13 /
  15). In battle 0 of 661 BF1 / 409 BF2 predictive kept hits belong to a soldier. Probes and rows:
  `engine/artifacts/lead_tmp/audit-uord/ps2/` (`bolt_b1.py`, `bolt_b2.py`, `knock_read.py`, `b1_knock2.json`,
  `b2_knock1.json`, `knock2_table.txt`, `knockb2_table.txt`). The port's scene test can mirror this knock-in (late
  entrant hit, 2.5 m aside missed, moving target dodges) and print the PS2 table beside its own.
