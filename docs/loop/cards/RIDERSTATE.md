---
id: RIDERSTATE
title: Vehicle rider lifecycle consistency under death, exit and slot reuse
milestone: M0
value: 4
phase: build
status: done
deps:
files: engine/crates/core/src/ai/ride.rs, engine/crates/core/src/sim/state.rs, engine/crates/core/src/sim/state/tests.rs
declares:
needs:
effort:
branch:
resume: Finite audit closed: existing lifecycle fixtures pass; no defect/new coverage gap established. See reports/20261025_0540_card_RIDERSTATE.md; unrelated core scanner failure recorded, content acceptance remains separate.
created: 2026-10-25
---
## Why
Vehicle occupancy and unit handles must remain internally consistent through lifecycle transitions. Existing synthetic simulation fixtures make these invariants executable without interpreting new original-game behavior.

## Acceptance
Inspect existing rider lifecycle code and tests. Exercise death while seated, explicit exit, vehicle removal, and unit/vehicle slot reuse using the existing fixture helpers. Verify stale handles never identify replacement entities and seat/unit ownership agrees after the supported transitions. Derive expected behavior from existing documented engine invariants, not guessed PS2 constants.

If an actual defect is found, preserve a minimal failing-before regression, fix its cause and show that regression and relevant core tests pass. Do not add tests that merely repeat implementation or invent a bug. If no defect or meaningful coverage gap is found, record the finite audit once and close this audit scope with evidence; do not keep reopening it. Existing content-dependent gameplay acceptance remains separate and unearned.

