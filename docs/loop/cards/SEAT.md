---
id: SEAT
title: How the PS2 seats a spawn: the floor a soldier appears on at a path node (a node inside a body, under its floor, over another floor) - read it and replace the port's three PROVISIONAL rules (SPAWN_LIFT, the level rule, LLEG's pen seat)
milestone: M3
value: 3
phase: re
status: blocked
deps:
files: engine/crates/core/src/sim/spawn.rs
declares:
needs: restore SEAT primary corpora/ELFs, PCSX2 live tools/states, LLEG raw evidence and map/mission inputs (seat_cloud/preflight.json)
effort:
branch:
resume: BLOCKED 2026-10-24 13:00 UTC: 26 exact prerequisites/0 available; restore corpora/ELFs, PCSX2 kit, LLEG rows and maps/missions, then trace both spawn writers and measure MUS1 crate/roof. No RE acceptance. Report reports/20261024_1300_card_SEAT.md; continuation lead_tmp/seat_cloud/next_prompt.md.
created: 2026-10-07
---
## Why
The port seats a spawn by three rules of its own (`Sim::spawn_ground`, `resolve_spawn_seats`): the ground under the node, the ground 3 m over a node authored under its floor (TAT1, BF2 UTA1), and since LLEG (c0794eb) the lowest un-penned floor within 6 m over a node that lies inside a solid body. How the PS2 does it is not read.
## Evidence
- BF1 Naboo Plains cp3 node (-107.2, -108.5, 63.2), inside the fallen column: the PS2's soldier appears at y -104.24, on the column (live RAM, `lead_tmp/lleg/ps2/nab1_soldiers.csv`, t 273.7 s, one spawn). The port now matches it there.
- BF2 Mustafar cp6 node (-66.2, 66.7, 23.1), inside a crate: the port now seats it on the crate (67.9); the PS2 is not measured. Its node (-72.3, 66.7, 18.2) has a roof at 72.0 over it: a plain ray from 5 m up would land there.
- `FLOOR_GRID=0,0,0,1,0 round_to_outcome 1 1 | grep 'fg seat'` lists the port's seats; the scratch `WALL_GRID` / `PEN_CENSUS` hooks are in `lead_tmp/lleg/local_search_scratch.patch`.
## Acceptance
- `analysis/re-spawn-seat.md` with the spawn's position write traced in both ELFs (the ray or the collision call, its start height), verified; a live-RAM check of BF2 MUS1's crate node and one node under a roof; then a build card or 'the port's rules stand'.