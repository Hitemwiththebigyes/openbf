---
id: LEAFF
title: Falling leaves: the PS2's LeafSpawner drops leaf particles from drawn leaf patches (YAV, DAG1, KAS2 classes set 6 per patch) — the port has none
milestone: M1
value: 2
phase: re
status: todo
deps: LEAF
files:
declares:
needs:
branch:
resume:
created: 2026-09-30
defer: post-alpha
public_issue: https://github.com/sprucey149/openbf/issues/15
---
## Why
The leafpatch object is also a `LeafSpawner` (RTTI `9LeafPatch` / `11LeafSpawner`): when a patch is first drawn it spawns up to its class's byte +0x80 (hash 0xfd7fae90; 6 on the YAV leafpatch/leafpatch2, DAG1 and KAS2 classes) leaf particles at random points in its ellipsoid, from a pool of 100 that also refills every 30 s from patches near each camera. Decoration (the drifting leaves under the canopy); the port draws none.

## Evidence
`analysis/re-leafpatch.md` §8 (B1 `FUN_001b1f00` 57236–57260, `FUN_001af8d0` the particle init with the terrain height, `FUN_001b0c30` the pool/refill, 56583 the layer-0x100 query; B2 twin `FUN_001c9468`, layer 0x200). The particle motion, texture, size, fade and lifetime are UNREAD.

## Acceptance
RE: the particle's update and draw (`FUN_001afdb0`/`FUN_001b03e0`/`FUN_001b0130` region) with the §5 discipline and a PCSX2 check (a live sampler or a patched state that zeroes the count); then a build card: leaves falling under YAV/DAG1/KAS2 canopies, decoration only.

## Notes
- 2026-09-30 (LEAF RE): the same object also has a SOUND slot — class byte +0x81 (hash 0x3b1948f3, ≤ 5; 3 on the YAV,
  DAG1 and KAS2 classes incl. the YAV bushes): each draw passes the patch's position to B1 `FUN_0019cd78` (B2
  `FUN_001b7b28`), which keeps the nearest five positions per type — [H] an ambient-sound anchor (leaf rustle / birds).
  Read it with the particles or split it off (re-leafpatch §8).

## Planner note (2026-09-30, M-ALPHA)
`defer: post-alpha` — not needed for the alpha's six showcase maps or its checklist. Value 2 (kept).
