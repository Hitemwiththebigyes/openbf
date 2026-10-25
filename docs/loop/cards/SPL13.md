---
id: SPL13
title: Split: core/src/ai/behave.rs is 2,314 lines (over the 2,000 strict cap, no card since SPL11 made it; size_report --strict fails on it) - a pure move by SPLIT_RECIPE where a seam yields two parts of 400+ lines; then the three core files just over 1,500 with no card: ai/gunner_foot.rs 1,684, sim/state.rs 1,600, sim/vehicles.rs 1,585
milestone: M0
value: 3
phase: cleanup
status: blocked
deps:
files: engine/crates/core/src/ai/behave.rs, engine/crates/core/src/ai/behave/brain.rs, engine/crates/core/src/ai/behave/position.rs, engine/crates/core/src/ai/behave/position/tests.rs, engine/crates/core/src/sim/vehicles.rs
declares:
needs: local-proof — restore the canonical pure_move_check helper and obtain its original exit0/PURE MOVE receipt; planner orders recipe-compliant re-split/reflow repair instead of waiving nine unmatched lines; vehicles remains its own later session
effort:
branch:
resume: 20261024_2231 planner: CRATES pointer fix is committed08ab825, do not reapply it. Retain measured behave sweep/arena neutrality; do not accept hash100/600 as think coverage. Canonical helper exit1 is still unpassed: repair split/reflow under the existing recipe until exit0, no helper weakening. Then vehicles1752 alone. See engine/artifacts/loop/reports/20261024_2231_planner.md.
created: 2026-10-13
---

## Why
Cleanse 15 (2026-10-13): `scripts/size_report.sh --strict` exits 1 on main 0487827 - `core/src/ai/behave.rs` is 2,314 lines (1,973 at cleanse 14; +451 since through CLSE4, DEFS, DOORS, REACT, HVYW's parents) and has had no split card since SPL11, which made it. Cleanse 14 left this for the planner and no card was filed. CLNV adds a waiver line naming this card so the strict report is honest meanwhile; delete that line when the split lands. `core/src/commander.rs` (2,508) is SPL7's.

## Evidence
`scripts/size_report.sh --strict` on 0487827: "2 src files over 2,000 lines without a waiver: commander.rs 2508, ai/behave.rs 2314".

## Acceptance
- `scripts/size_report.sh --strict` exit 0 with behave.rs's waiver line deleted; behave.rs under 1,500; `--check-map` exit 0 with the new files in engine/CRATES.md.
- Every gate PASS, nothing declared; the sim hashes and the test count (gates.toml [ci] passed) unchanged.
- One file a session (SPLIT_RECIPE): behave.rs first; gunner_foot.rs, sim/state.rs and sim/vehicles.rs each only where a seam yields two parts of 400+ lines, else a waiver line with that reason.

## Notes
Bot cards land in behave.rs all the time (RLOOP, FLNK, TEAM are queued): take this between two of them, on a quiet tree.

## GitHub checkpoint recovery (2026-10-24)

