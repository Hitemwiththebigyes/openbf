---
id: SPRCL
title: No soldier freezes at sprint: a census of every class on both games for the clip the animator picks while sprinting (some bodies stay in the idle pose at sprint speed), the PS2's fallback for a set without a sprint clip, every class fixed and the census kept as a test
milestone: M1
value: 4
phase: re
status: todo
deps:
files: engine/crates/game/src/animator.rs
declares:
needs:
effort:
branch:
resume:
created: 2026-10-21
alpha: yes
---
## Why
The maintainer (INBOX 2026-10-21 17:50 item 2): some characters' bodies freeze in the idle pose while moving at sprint speed.
ANIML says "no action clip in ... sprint". Related: BLKS (the black box's sprint inputs), CSPD (BF2 per-stance turn).

## Do
1. Census (like `layer_census`): per class / skeleton, the clip picked at sprint speed, on both games.
2. RE the PS2's pick when a set has no sprint clip (the fallback table, ANIMS).
3. Fix every class.

## Acceptance
- The census line: classes with no moving clip at sprint, N -> 0, kept as a test over every baked class so a new class
  cannot freeze; a picture strip of one fixed class beside the PS2.
