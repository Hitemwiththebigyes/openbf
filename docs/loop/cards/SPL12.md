---
id: SPL12
title: Split: core/src/sim/step.rs (1,767 lines, 990 of them its inline tests from line 775) - move the test module to core/src/sim/step/tests.rs; then core/src/sim/spawn.rs (1,561)
milestone: M0
value: 2
phase: cleanup
status: todo
deps:
files: engine/crates/core/src/sim/spawn.rs
declares:
needs:
effort:
branch:
resume: 2026-10-24 14:22 UTC: Current source: step.rs 1011 with external step/tests.rs 1231 already extracted on main. Active split is spawn.rs 2947 only. Do not re-extract step tests. Preserve strict/map/current counts and unchanged sim/test acceptance; complete runtime gates after restore. reports/20261024_1422_planner.md; lead_tmp/planner_20261024_1417/next_prompt.md.
created: 2026-10-09
---
## Why
Cleanse 14 (2026-10-09): both files are over the 1,500-line cap with neither a split card nor a line in scripts/size_waivers.txt. step.rs grew 1,385 -> 1,767 through card ZOOM (its tests). engine/artifacts/briefs/SPLIT_RECIPE.md: one file, one session, a pure move.

## Acceptance
- `scripts/size_report.sh`: neither file in the over-1,500 list; `--check-map` exit 0 with the new files in engine/CRATES.md.
- Every gate PASS, nothing declared; the sim hashes and the test count (gates.toml [ci] passed) unchanged.