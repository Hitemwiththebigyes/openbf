---
id: UORDE
title: Emitter weapons (BF1 clone pilot's arc caster; BF2 dark trooper / Jawa arc casters, the officer's incinerator, the hero flamethrowers) fire no bolt: each shot is one instant cone query at the muzzle that chains damage from target to target (MaxTargets, JumpDeduction, DamageThreshold, MaxJumpDistance, LOS per hop); BF2 adds hit-all, team / alive filters, class scales, a blast per hit, pull and burn damage over time
milestone: M3
value: 3
phase: build
status: todo
deps:
files: engine/crates/core/src/weapon.rs, engine/crates/core/src/sim/guns.rs, engine/crates/game/src/roster.rs
declares: sim; sweep; arena
needs:
effort:
branch:
resume:
created: 2026-10-22
---
## Why
analysis/re-ordnance-kinds.md §8 (card UORD; full reading lead_tmp/uord/helpers/H2_emitter.md): the emitterordnance class's create slot (B1 FUN_00278208, B2 FUN_00315f78) runs one query per weapon shot and returns 0 - no projectile. The port builds these as straight bolts.

## Evidence
- BF1 (B1:198922-199272): box gather at muzzle + 0.9 L x aim, mask 0x812 (no vehicles), cone tan < sin(ConeAngle/2) at the body origin with no front test, nearest along the aim first; D = lerp(Damage, MaxDamage, charge); hop range ConeLength then MaxJumpDistance (axial); LOS mask 0x4c from the chain origin to body + 1 m; FirstRadius on the first; all type / shield scales forced to 1.0; D -= JumpDeduction; stop at MaxTargets or D < DamageThreshold. Verifier batch 4: 8 / 8 SUPPORTED.
- BF2 (B2:305952-307007): mask 0x102a (vehicles), 3-D cone (cos 0.1 within 2.5 m), alive bit, AffectFriends / AffectEnemies, the class's scales, hit-all mode (D = max(Damage, D/N)), Explosion blast per hit, pull (Push < 0), burn timer = max(timer, T), rate x dt per update while alive (FUN_00236f50).
- Fielded: BF1 rep_weap_inf_arccaster (1 / 2 / 3 targets by charge: 100 -> 250, JD 75, DT 75); BF2 imp / tat arc casters, all_weap_inf_incinerator (30 per shot, 15-shot burst), cis / imp_weap_hero_flamethrower (5 per shot + burn 50/s for 5 s).

## Acceptance
- Unit tests on the read rules, mutation-checked: the BF1 arc caster's target count by charge (1 below 1/3, 2 below 5/6, 3 above), a skipped (LOS-blocked) candidate ends nothing, no vehicle in BF1's gather; BF2 hit-all split, the burn's damage over 5 s, team filter.
- A scripted scene: a clone pilot's full charge into three soldiers in a line deals 250 / 175 / 100.
- 8 x 32 sweep 256 / 256 (bots hold these weapons: the clone pilot, the dark trooper, the Rebel officer).

## Notes
- Visual only (no sim): the lightning segments, BF2's extra arcs, the flame effect, the burn bone effects.
- Named-deviation candidates (H2 §6): BF1's missing front test, its feet-point cone test, its dead scale keys, corpses as targets - measure on the PS2 before keeping or dropping any.