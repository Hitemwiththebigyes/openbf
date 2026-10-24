---
id: VCHNK
title: A destroyed vehicle throws its CHUNKn bodies: the baked chunk models launched by re-vehicle-death §6.1, 40-kg cosmetic rigid bodies (gravity, ChunkBounciness/Stickiness, ChunkTerrainCollisions, settle), gone 3 s (BF1) / 8 s (BF2) after resting; the _exp class's NumChunks debris (5 s)
milestone: M2
value: 4
phase: build
status: blocked
deps: VDTH
files: engine/crates/game/src/chunks.rs,engine/crates/game/src/viewer/vehicles.rs,engine/crates/game/src/vehicle_class.rs,engine/crates/manifest/src/lib.rs,engine/crates/bake/src/vehicle.rs
declares: rebake=side
needs: environment restoration — BF1/BF2 SIDE LVLs and vehicle/map bakes, missions and approved GPU acceptance; see lead_tmp/vchnk_cloud/inputs.json
effort:
branch:
resume: Restore and verify inputs in lead_tmp/vchnk_cloud/inputs.json; then loopctl start VCHNK in an isolated checkout. Inspect CHUNKSECTION properties, ChunkNodeName and prim origins before extending bake/parser; implement cosmetic launch/body/debris with launch/lifetime tests, side rebake, scripted destruction frame, CI and applicable gates. No implementation or landing yet.
created: 2026-09-29
alpha: yes
---
## Why
Split from VDTH (2026-09-29): VDTH landed the blast's area damage and a placeholder fireball; the chunks the PS2 throws at a vehicle's death (and the _exp class's debris) are still missing — a destroyed hull just flashes and is gone.

## Evidence
analysis/re-vehicle-death.md §6 [O]: launch law (B1 FUN_001e2630, B1:91931-92126: vel × ChunkVelocityFactor + dir × ChunkSpeed, or U(15,30)/w + the |v|<5 floor; w = the class's blast sphere, now `Sim::blast_setup`), the body (mass 40, restitution ChunkBounciness 0.6, friction ChunkStickiness 0.7, damping 0.001/0.004, B1 g −9.8 / B2 ChunkGravity), settle rules, lifetimes (BF1 3 s / BF2 8 s at rest; explosion debris 5 s), the 8×10 pool. VBAKE bakes the chunk models (ManifestVehicle::chunks). The per-chunk prim positions (the centroid c and d) need the CHUNKSECTION's ChunkNodeName / prim — check what the bake keeps.

## Acceptance
game-only presentation (chunks never touch the sim — the PS2's chunk-vs-vehicle response is [H]): on Event::VehicleDestroyed spawn the class's chunks with §6.1's launch, integrate §6.2 against the heightfield (and world collision if cheap), draw them with the chunk GLBs, remove on the lifetime. Unit tests for the launch law and the lifetime. A capture or scripted frame that shows chunks flying after a scripted destroy (a scratch example, never a golden unless declared).

## Planner note (2026-09-30, M-ALPHA)
`alpha: yes` — explosions: a destroyed vehicle's chunks. Value 4 (kept). Showcase maps: BF1 NAB1, BF1 TAT1 (the port's `tat1`), BF1 YAV2, BF2 UTA1, BF2 MYG1, BF2 HOT1 (PLAN §Milestones M-ALPHA).
