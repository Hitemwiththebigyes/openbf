---
id: VTHUL
title: A building turret's hull pool is hit through its building: the turret instance's world bodies carry the turret vehicle as owner (CollisionWorld Body.owner, VHULL), so a bolt at the building damages the pool and the turret's own seat's bolts, aim ray and exit path skip it
milestone: M2
value: 4
phase: build
status: todo
deps: VHULL
files: engine/crates/game/src/assets.rs,engine/crates/game/src/vehicle_pads.rs,engine/crates/core/src/collision/world.rs,engine/crates/core/src/sim/seats.rs
declares:
needs:
effort:
branch:
resume:
created: 2026-09-29
defer: 
value_pre_alpha: 4
alpha: yes
note: planner 2026-10-21 17:10: value 3 -> 4, alpha - HTGT's readout now shows a building turret's bar that shots never move (report 20261021_1620_card_HTGT.md)
---
## Why
Split from VHULL (its VTURB note). VTURB's turrets are VehicleKind::Turret vehicles whose pool (MaxHealth, HealthType building) nothing can hit: the building's collision is a world body with no link to the vehicle.

## Evidence
VHULL (report reports/*_card_VHULL.md) gave every Body an owner: Option<VehicleHandle> and HullFilter / ray_with (the owner a ray hit). Turrets get no hull parts (their building is already the world's body). core::sim::seats skips the exit path test for turrets (PROVISIONAL) because HOT1's origin sits inside its mesh.

## Acceptance
- Scene::collision_and_doors records each body's instance; vehicle_pads::install sets owner = the turret handle on its instance's bodies (a CollisionWorld::set_owner); a destroyed turret clears them (the building stays solid, a bolt at it is Hit::World).
- A bolt at a HOT1 turret building damages the pool; the gunner's own bolts and aim ray pass out of it; the exit keeps its path test (HullFilter::except skips the own building) — drop the turret skip if the HOT1 turret_ride exit still lands outside.
- Declared moves: sim/sweep (bots' bolts now hit turret pools).

## Planner note (2026-09-30, M-ALPHA)
`defer: post-alpha` — not needed for the alpha's six showcase maps or its checklist. Value 4 → 3 until the alpha (restore from `value_pre_alpha`).
- 2026-10-21 (HTGT slice 3, lane 1): the HUD's target readout now shows a building turret (picked by its building's
  bounds, `Vehicle::aim_sphere`), so this gap is now on screen: BF2 HOT1 `tur_bldg_hoth_dishturret` (veh 2, instance 68)
  picked from 15 m, 26 rifle bolts with the Building scale forced to 1.0 -> the bar stays 1.0 for 260 ticks (a hover IFT-X
  under the same fire 1.0 -> 0.67). Scratch hook + runner: `engine/artifacts/lead_tmp/htgt/tools/face.sh`,
  `htgt_face_scratch3.diff`, log `face3.log`. The maintainer asked for the bar to drop on turrets too (HTGT Why).

## Planner note
Planner 2026-10-21 20:50: TCOL (first 4, the maintainer's playtest: Laik cannons' upper half lets shots through and never loses health) owns the turret's whole collision shape and its damage; batch with it if the files meet.
