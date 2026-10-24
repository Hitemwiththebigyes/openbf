---
id: LODV
title: Vehicles take the far scene's model / LOWD switch: a vehicle past NearSceneRange[1] is drawn as its LOWD (67 vehicle LOWDs in side_bf1/2), as card LOD does for world instances
milestone: M1
value: 3
phase: build
status: blocked
deps: LOD
files: engine/crates/game/src/viewer/vehicles.rs
declares:
needs:
effort:
branch:
resume: BLOCKED/incomplete 2026-10-24: 12/12 runtime/GPU prerequisite paths absent; restore baked maps/common/classes/missions/vehicle sides and suitable adapter, then baseline and implement model/LOWD pairing. Acceptance unchanged. Report reports/20261024_1331_card_LODV.md; kit lead_tmp/lodv_cloud/next_prompt.md
created: 2026-10-19
---
## Why
Card LOD switched world instances and path props only; vehicles (per-frame draws in viewer/vehicles.rs) still draw their model at every distance on far-scene maps - a named deviation in LOD's report. The PS2 far scene draws every LOWD-bearing geometry as its LOWD past NearSceneRange[1] (analysis/re-lod.md §7). Also a speed item for weak GPUs (card ISPD).

## Evidence
side_bf1/side_bf2 bakes hold 67 vehicle *LOWD.glb (e.g. cis_hover_aatLOWD, all_fly_xwingLOWD); render::LodSwitch + look::lod_bands are on main after LOD.

## Acceptance
A test: a vehicle draw list on a FarSceneRange>0 map carries the model with LodSide::Model and its LOWD with LodSide::Lowd (none on a FarSceneRange(0) map), mutation-checked; a pose_render of a parked vehicle at ~1.5x NearSceneRange[2] shows the LOWD; goldens predicted (likely none: no golden has a far vehicle).