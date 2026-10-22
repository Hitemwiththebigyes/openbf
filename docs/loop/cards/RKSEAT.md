---
id: RKSEAT
title: Seat guns' exploding shells (since RKEX every vehicle / turret gun whose ordnance names an explosion explodes - faithful, never measured): carry the own-side rule to seat gunners and count mounted mates - seat guns cause 116 of the 131 own-side rocket / shell kills left on main (57 blast + 59 direct-hit in 256 rounds), mounted mates 6 more
milestone: M2
value: 4
phase: build
status: todo
deps:
files: engine/crates/core/src/sim/guns.rs, engine/crates/core/src/ai/gunner.rs, engine/crates/core/src/ai/gunner_foot.rs, engine/crates/core/src/sim/step.rs
declares: sim; sweep; arena
needs:
effort:
branch:
resume:
created: 2026-10-20
alpha: yes
---
## Why
AUDIT RKSLF 2026-10-20 (engine/artifacts/loop/audits.md; evidence engine/artifacts/lead_tmp/audit-rkslf/wf/summary.txt, controls C3 / C5). Card RKEX (459a6c3) fills `ordnance.explosion` in `roster::weapon_from_db`, which `game::vehicle_pads::seat_gun` also uses, so every seat weapon whose ordnance names Explosion / ExplosionImpact / ExplosionName now explodes on impact. That is the PS2's behaviour (both audit verifiers: B1 FUN_00276810 / B2 FUN_00313c40 are the Bullet, Laser, FatRay, Missile and Shell collision slots and call the explosion base), but no commit message said so and the landing never measured it: over 32 maps x seeds 1-8 (256 rounds) seat guns set off 26,590 blasts, killing 1,286 enemy soldiers and taking 37,059 HP off enemy vehicles (84 % on Mygeeto, tur_weap_recoilless_rifle 2000 direct + 400 blast).
The port's own-side rules (OURS: RKEX friend_in_blast, RKSLF release_blasts_own_side) cover soldiers on foot only (sim::step phase 4 sends a mounted unit to `seat_gun` and continues before the check). Of the 131 own-side kills by exploding ordnance left on main, 116 come from seat guns (57 blast + 59 direct hit; bf1_nab1, bf1_kam1, bf1_nab2, tat1, bf1_yav2 most), 13 from on-foot shots that were clear at release (7 mates walking in, 6 mounted mates), 0 from shots the check could see. Both rules also skip MOUNTED mates, but a building-turret gunner takes the blast (`blast_unit` spares only non-turret riders) and a destroyed own-team vehicle kills its riders: 6 kills.
## Acceptance
- On the 32-map sweep, seeds 1-8 and 9-16, with the audit's observer (`lead_tmp/audit-rkslf/tools/audit_rk.rs` + `obs_patch.py`, AUDIT_RKX mnt 1 kmate + AUDIT_RKD same-team from seat guns): seat-gun own-side kills 116 -> 25 or fewer per 256 rounds; seat-gun enemy kills not below 90 % of today's (1,286 blast kills, seeds 1-8); kills a minute and the arena gates within noise.
- A turret gunner (and a rider) within DamageRadiusOuter + 1 m of the blast counts as a mate for both rules (the 6 kills); a scene test each, mutation-checked.
- A FEEL_NOTES line (OURS: the PS2's gunners fire regardless).
## Notes
The PS2 bar for seat-gun team kills is not measured; the fidelity gate's teamkill_pct (dir down) is at 0.85 % on main against the parent's 1.14 %, with seat-gun blasts off 0.82 %.