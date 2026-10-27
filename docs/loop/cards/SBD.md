---
id: SBD
title: Super battle droid tri-shot, wrist rockets and HUD icons
milestone: M3
value: 5
phase: build
status: in-progress
deps:
files: engine/crates/bake/src/soldier/output.rs,engine/crates/core/src/sim/guns.rs,engine/crates/core/src/sim/ordnance.rs,engine/crates/core/src/sim/state.rs,engine/crates/core/src/sim/step.rs,engine/crates/core/src/sim/step/tests.rs,engine/crates/core/src/weapon.rs,engine/crates/core/src/weapon/tests.rs,engine/crates/game/src/classdb.rs,engine/crates/game/src/event_sounds.rs,engine/crates/game/src/fx.rs,engine/crates/game/src/fx/children.rs,engine/crates/game/src/fx/tests/resupply.rs,engine/crates/game/src/main/headless.rs,engine/crates/game/src/repair_feedback/tests.rs,engine/crates/game/src/roster.rs,engine/crates/game/src/sounds.rs,engine/crates/game/src/walk.rs,engine/crates/game/tests/suite/main.rs,engine/crates/game/tests/suite/resupply_feedback.rs,engine/crates/game/tests/suite/roster.rs,engine/crates/game/tests/suite/weapon_classdb.rs,scripts/rebake.sh
declares: sim; rebake=side
needs:
branch:
created: 2026-10-27
alpha: yes
---

Issue: https://github.com/sprucey149/openbf/issues/27

BF2 SBD weapon mechanics and slot ammunition are shared by both map sets.
Tri-shot tilts, secondary rocket projectiles and the original HUD meshes are
implemented through the existing weapon, ordnance, roster and bake paths.
SBD audio follows the map's source game: BF1 on BF1 maps, BF2 on BF2 maps.
The canonical rocket's original particle trail loads on BF1 CIS battles and
replaces the generic box. Effects retain their own source era's particle rules.

Review removes duplicate soldier projectile spawning, reuses inherited-property
and effect readers, removes ignored effect-era arguments, and identifies the
weapon in reload events so wrist rockets use their own reload sound.

`analysis/sbd-weapons.md` records both discs' numbers, implementation decisions,
headless acceptance, sound provenance and local capture reproduction. Final
validation logs and packed-build evidence are under the ignored
`engine/artifacts/lead_tmp/sbd/review-*` paths. The local distribution is refreshed
before handoff while preserving profiles.

Live original-game firing comparison remains outstanding. Source/bake checks and
port screenshots are not claimed as a completed PS2 side-by-side match.
