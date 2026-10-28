---
id: SEATCAM
title: A seat camera can end up under the terrain: in BF2 Geonosis's geoturret the camera sat 0.30 m under the terrain sheet, 2 m from the turret at its origin's height, where the class's camera (TrackCenter 0 2.5 0, offset 0 3.6 2.0) puts it ~6 m up; check every turret and vehicle seat
milestone: M2
value: 4
phase: build
status: todo
deps:
files: engine/crates/game/src/walk.rs
declares:
needs:
effort:
branch:
resume:
created: 2026-10-28
---
## Why
The maintainer's playtest 2026-10-28: "when I got in a geonosis turret, the camera went underground", and asked whether the camera drops elsewhere too.

## Evidence
- Black box `engine/artifacts/playtest/prev1.json` of 09:14 (now rotated; build a43f94e5+dirty): BF2 GEO1 seed 48, human seated in vehicle 0 "Geonosian Turret" (`tur_bldg_geoturret`, geometry `geo_bldg_geoturret`) at (-7.42, -61.63, 77.93); camera row `cam` (-9.10, -61.76, 76.86), pitch -0.35.
- Terrain sheet (`Heightfield::height_at`, measured with a throwaway example 2026-10-28): -61.65 under the turret, **-61.46 under the camera**: the camera is 0.30 m below the sheet; no cutter there.
- The class (`assets_baked/classes/bf2_tur.ron`): props "0.0 2.5 0.0", "0.0 0.0 -2.0", "0.0 3.6 2.0" (hash names to resolve: TrackCenter / EyePointOffset / TrackOffset).
- `WalkSim::settle_camera` keeps a camera on the side of the sheet it is on (deliberate, for GEO1's hangar under uncut terrain), so a seat camera that starts under the sheet stays there. Lead: the seat's camera numbers do not reach `vehicle_camera` for this building turret (zeros), placing the camera at the origin's height.

## Acceptance
- Root cause named with the numbers (which seat row reaches `vehicle_camera`, what it holds).
- A test over every baked building turret and vehicle seat of both games: the settled seat camera is above the terrain sheet where the turret stands on it.
- GEO1's hangar case (camera under the sheet on purpose) still holds.
