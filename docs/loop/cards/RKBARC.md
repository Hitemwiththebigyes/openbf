---
id: RKBARC
title: Rockets still pass through the BF2 BARC speeder (0/6 after RKHUL): its coll mesh sits 0.96 m below the drawn body (coll y -0.59..0.66, drawn 0.34..1.66; node dummyroot, identity bind) - the only one of 84 side coll meshes off its model; and the STAP misses 2/6 at one GEO1 pad
milestone: M2
value: 4
phase: re
status: todo
deps: RKHUL
files: engine/crates/bake/src/vehicle.rs, engine/crates/game/src/assets.rs
declares:
needs:
effort:
branch:
resume:
created: 2026-10-21
alpha: yes
---
## Why
Card RKHUL's repro on its scratch bake (lead_tmp/rkhul/repro_branch.txt, scorer lead_tmp/rkhul/score.py): walkers now hit 6/6, but rep_hover_barcspeeder stays 0/6 on KAS2 (the rocket lands on terrain 9-111 m behind). The BARC's ordnance bodies are its coll mesh plus six tiny '-so' boxes (pedals, handles, gun fronts); its hull prims are MASK v|t|b ('-vtb', no ordnance, re-collision-layers §1/§5). The coll mesh is 0.96 m below the drawn model in y with x/z matching (lead_tmp/rkhul/coll_census.txt: every other side coll mesh matches its drawn model within 0.06 m, except where a turret top stands above it). The blast-sphere aim point (y +1.04) is above the mesh, so a level shot passes over it. The '-vtb' cylinder (r 1, through boneroot bound at y 1.0) touches the ground at the origin, so the drawn model (and not the mesh) is likely where the PS2 has the body. [H] the PS2 places this coll mesh by boneroot's bind (or its NODE resolves differently) - read the coll loader's NODE use (re-collision-layers §1 hop) or a live BARC's CollisionModel in RAM.
STAP: cis_hover_stap V5 on GEO1 misses at 5 and 20 m (14.2 / 68.8 m terrain), V11 hits 6/6: the STAP's ordnance body is a thin column (x +-0.35); check its heading/tilt at the miss and the trimesh ray against an open column.
## Evidence
lead_tmp/rkhul/coll_census.txt, repro_branch.txt, repro_main.txt; the scratch test lead_tmp/rkhul/rkex_repro.rs (drop into game/tests/suite, mod rkex_repro;, RKEX_MAPS="bf2_geo1 bf2_kas2 bf1_geo1", --ignored --nocapture).
## Acceptance
The same repro: BARC 6/6 and STAP 12/12 at 5/20/60 m on KAS2 + GEO1; the RE answer for where the PS2 puts a vehicle coll mesh (a RAM read or the loader's lines) in analysis/; every other class's hit count unchanged.