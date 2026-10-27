---
id: VTURB
title: Building turrets as seat-only vehicles: every armedbuilding instance (BF1 com_weap_gunturret, BF2 tur_* and the armedbuildingdynamic turrets) spawned from the map's instances as a VehicleKind::Turret with its TURRETn gunner seats and guns (VTUR's machinery), enterable, never moving, its hull pool; drawn at its instance
milestone: M2
value: 5
phase: build
status: done
deps: VTUR
files: engine/crates/game/src/vehicle_pads.rs,engine/crates/game/src/vehicle_class.rs,engine/crates/game/src/classdb.rs,engine/crates/core/src/sim/seats.rs,engine/crates/game/examples/turret_ride.rs
declares: sweep
needs:
branch: vturb
resume:
created: 2026-09-29
---
## Why
The maintainer's 09-28 coverage note: NAB1 ships com_weap_gunturret instances and PLAN §Vehicles item (5) wants every building turret usable. Split from VTUR (2026-09-29) to keep that landing one commit-sized change.

## Evidence
analysis/re-vehicles.md §8: armedbuilding uses the same TURRET/TURRET1..8 sections, MountedTurret section and setter (B1 FUN_001c4db8 / FUN_00225f08; B2 FUN_001e8860 / FUN_00270770); no pilot seat; kind 8 = 'turret' to the enter search (B1:119710). [H]: the armed building's entity init (seat ring, +0xac bit 3) was not opened. VTUR landed the seat guns (core::vehicle::gun, core::sim::guns), the fire points (bake::hardpoints — already baked for armedbuilding classes in maps and common) and game::vehicle_pads::seat_gun.

## Acceptance
Every armedbuilding instance of a map (and its common class) becomes a Turret vehicle at the instance's transform with the class's gunner ring (vehicle_class::seats) and guns; ACTION enters it by §5's cone; FIRE fires from its FirePointName; it is drawn by the vehicle viewer (or stays the world model it already is — decide and record); its MaxHealth pool takes bolts once VHULL gives hulls a ray; a test per seat kind; a NAB1 headless run: walk to the gunturret, enter, fire, exit.
