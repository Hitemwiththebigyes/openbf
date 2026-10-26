---
id: SPL10
title: core/src/collision/walls.rs is 2,086 lines (over the 2,000 strict cap) and manifest/src/lib.rs 1,602, with no split card — split each where a seam yields two parts of 400+ lines (SPLIT_RECIPE), else keep the waiver
milestone: M0
value: 2
phase: cleanup
status: todo
deps: SPLENV
files: engine/crates/core/src/collision/walls.rs,engine/crates/core/src/collision/walls/queries.rs,engine/crates/core/src/collision/walls/tests.rs,engine/crates/manifest/src/lib.rs,scripts/size_waivers.txt
declares:
needs:
effort:
branch:
resume: SPLENV independently owns environment schema extraction; retain wall-slice local receipt and its 11-line literal-proof debt. Complete original gates and scratch bake DIFF=0 before closing.
created: 2026-09-30
tries: 1
---
## Why
Cleanse CLNC (2026-09-30): `scripts/size_report.sh --strict` exited 1 on walls.rs (2,086; it grew through JWAL / WTUN / LEAK3 / WPSH / STHOP) and neither file was named by an SPL card. CLNC waived both in `scripts/size_waivers.txt` pointing here.

## Acceptance
`scripts/size_report.sh` lists neither file over 1,500 lines, or the waiver line gives the 'no seam' reason; every gate PASS with nothing declared (walls.rs carries the FULL sim set incl. the leak census; manifest the scratch bake at DIFF=0); the two waiver lines deleted when the split lands; CRATES.md rows current. One file per session (SPLIT_RECIPE).
- 2026-10-06 planner: `engine/CRATES.md` taken out of `files:`. The picker refuses a card whose files overlap a card in progress, and 22 open cards listed the map, so any one of them held the other 21 (a chain card in progress held WADE and WTHR). The map's row for a new or moved source file is still owed in the same branch (`scripts/size_report.sh --check-map`); a rebase conflict in it is one line. Undo: add the path back.

## Independent remaining split (2026-10-26)
SPLENV owns the remaining source extraction as a separate card. This preserves
all original acceptance, scratch-bake equality and prior validation debt.
