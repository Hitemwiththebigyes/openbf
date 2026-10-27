---
id: SPL14
title: Split: game/src/soldier_vis.rs is 2,071 lines (over the 2,000 strict cap, no card; past it through USKN) - a pure move by SPLIT_RECIPE; then the four files over 1,500 with no card: core/src/ai/state.rs 1,787, game/src/walk.rs 1,664, bake/src/soldier.rs 1,637, core/src/sim/bounds.rs 1,501
milestone: M0
value: 2
phase: cleanup
status: todo
deps:
files: engine/crates/game/src/soldier_vis.rs, engine/crates/game/src/soldier_vis/runtime.rs, engine/crates/game/src/soldier_vis/tests.rs, engine/crates/core/src/ai/state.rs, engine/crates/game/src/walk.rs, engine/crates/bake/src/soldier.rs, engine/crates/core/src/sim/bounds.rs
declares:
needs: ai/state.rs remaining split, then every original gate PASS; SPLSIDE output extraction implemented with full validation pending
effort:
branch:
resume: soldier_vis split validated; walk.rs now 1222 and bounds.rs 1500. Independent bake/soldier.rs output extraction implemented as SPLSIDE, full content/GPU gates pending. ai/state.rs 1995 remains behind parked GBOARD/WG* ownership. Original complete gates remain; reports/20261026_0803_planner.md.
created: 2026-10-20
tries: 1
---

## Why
Cleanse 19 (2026-10-20, card CLN14): `scripts/size_report.sh --strict` exits 1 on main 7ba685f for two files without a waiver. `core/src/sim/spawn.rs` (2,758) is SPL12's; CLN14 adds its waiver line. `game/src/soldier_vis.rs` (2,071; grew through USKN's skeletons and skins) had no card; CLN14 adds a waiver line naming this card. Four more files are over the 1,500 cap with neither a card nor a waiver: core/src/ai/state.rs 1,787, game/src/walk.rs 1,664, bake/src/soldier.rs 1,637, core/src/sim/bounds.rs 1,501. engine/artifacts/briefs/SPLIT_RECIPE.md: one file, one session, a pure move; soldier_vis.rs first.

## Acceptance
- `scripts/size_report.sh`: soldier_vis.rs not in the over-1,500 list, and its waiver line deleted; `--check-map` exit 0 with the new files in engine/CRATES.md; `--strict` exit 0.
- Every gate PASS, nothing declared; goldens unchanged and the test count (gates.toml [ci] passed) unchanged.
