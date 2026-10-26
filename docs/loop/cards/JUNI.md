---
id: JUNI
title: One jump for the merged game - BF1 maps jump with BF2's take-off and air steering (the maintainer, 2026-10-25: "jump physics the same across all bf1 and bf2 maps ... match the better performing game"); the vertical take-off already landed with pull request #2
milestone: M3
value: 4
phase: build
status: todo
deps:
files: engine/crates/core/src/soldier.rs,engine/crates/core/src/soldier/tests.rs,engine/crates/core/src/sim/step/tests/ramps.rs,engine/artifacts/loop/FEEL_NOTES.md
declares: sim; sweep; arena; fidelity
needs:
effort:
branch:
resume:
created: 2026-10-25
alpha: yes
domain: physics
---

## Why
The maintainer, 2026-10-25: "we should have jump physics the same across all bf1 and bf2 maps for our version. Remember that
since we are merging both games together, some behavior must match the better performing game in that regard for
better consistency and overall better game experience." He likes BF2's uphill jumping. The port still jumps per
`GroundRules`, so a soldier's jump changes from map to map.

Landed already (public pull request #2, integrated 2026-10-25): the vertical take-off. BF1 maps ADD the jump velocity
to the vertical velocity, as BF2's PS2 does (B2 FUN_00246ed0, add.s at 0x247034); BF1's PS2 stores it
(FUN_00208b78, swc1 at 0x208bb4). FEEL_NOTES 2026-10-25 JUNI.

Still per game in `core::soldier` (re-stance-roll.md §4.3a/§4.3b, cards JARC and AIRC):
- the planar take-off: BF2 reshapes the planar velocity with JumpForwardFactor / JumpStrafeFactor; BF1 adds the facing
  x 0.25 x jump velocity x the forward stick (`BF1_JUMP_PUSH`);
- the air steering: BF2 drags (exp(-AIR_DRAG |v| dt)) and adds stick x ControlSpeed[jump] x Acceleration; BF1 raises
  each axis toward stick x BF1_AIR_SPEED x MaxSpeed and never lowers it;
- the late-press window: BF2 takes a press within 0.3 s of losing the floor; BF1 only for a stance 5 s old or one
  that touched down from an earlier hop.

## Acceptance
1. Decide per item above which game's rule is the merged game's (default BF2's, the maintainer's call on the uphill jump); write
   each as a FEEL_NOTES named deviation with both games' PS2 citations. Keep the PS2 RE notes as they are.
2. One code path for the chosen rules on every map; the class constants (JumpHeight, JumpForwardFactor, ...) stay
   each class's own.
3. The tests that pin BF1's PS2 arcs (JARC, AIRC, the late-press window) are re-scoped to the merged rule, each with
   a mutation that brings back the old per-game rule and fails; the PS2 numbers stay in the notes.
4. Paired 32 maps x 8 seeds sweep and fidelity, arena and sim gates; the moves declared; bots still finish every
   round. A by-hand feel check on a BF1 map (the maintainer).
