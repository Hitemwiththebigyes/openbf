---
id: AIMG
title: No sim referee exercises the aim magnet: aim_demo fires 120 ticks with enemies=0 (magnet -1 every tick), so AIMF's acquisition moved no gate — add an aim replay with enemies in and out of the cone, a friend on the crosshair, and a wall
milestone: M0
value: 2
phase: build
status: blocked
deps: AIMF
files: scripts/sim_gates.sh, engine/crates/game/src/main.rs, engine/crates/game/src/main/headless.rs, scripts/land.py
declares: sim
needs: (private workflow note omitted)
effort:
branch:
resume: 20261024_2231 planner: 69ddd8f/b7f2b26 resolve the old CLN6 CI failure (historical combined1932 pass/53skip); current-main rerun remains owed. AIMG appendix patch is unapplied and guarded; no approval granted. After explicit approval, implement/test it and publish aim_targets only through normal gated baseline process. See engine/artifacts/loop/reports/20261024_2231_planner.md.
created: 2026-09-28
tries: 1
---
## Why
AIMF (73632e1) rewrote the soldier acquisition; the gates showed sim/simhash/sweep/arena identical because no referee log puts a target in front of a player (aim_demo motion.csv: magnet column -1 on all 120 ticks, enemies 0). Only the unit tests in `core::sim::input` guard it.

## Do / Acceptance
A recorded or scripted headless aim replay (dummy rig: enemy just off the ray, friend on the crosshair, enemy past LockOnRange + r) whose motion.csv magnet column changes; hash it in sim_gates.sh (gpu_tests/sim_gates edits may be classifier-denied -> For the maintainer). Declares: sim (new baseline).
