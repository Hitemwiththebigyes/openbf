---
id: BTURN
title: BF2 bots turn at 4 x MaxTurnSpeed on the PS2 (20 rad/s for a 5.0 class; the port's bots turn at 5.0): measure how often the port's cap binds, then build it for BF2 only
milestone: M3
value: 3
phase: measure
status: todo
deps:
files: engine/crates/core/src/ai/gunner_foot.rs
declares:
needs:
effort:
branch:
resume:
created: 2026-10-22
alpha: yes
---
## Why
Card LOOKR's RE (`analysis/re-look-ramp.md` 2 and 4, [V], two blind verifiers): a BF2 bot's soldier turns at
**4 x MaxTurnSpeed** - `FUN_00238068` returns class +0x754 x 4.0 when E+0x330 < 0 (B2:151844-151851; the 4.0 is
0x3c014080 at 0x23807c), the yaw rule `FUN_00247fa8` uses it flat for a bot (B2:162912-162913), and the AI's stick
writer `FUN_00249f80` hands the same value to its converter `FUN_002a54d8` (B2:164694-164697). For a 5.0 class that is
20 rad/s (1146 deg/s) where the port's bots turn at 5.0 (`core/src/ai/gunner_foot.rs` 233: rx = -e_yaw / (max_turn_speed
x dt), clamped). BF1's bots are flat MaxTurnSpeed (`FUN_002098b8` B1:119340-119341, the port's rule already). A bot that
turns 4x faster reacts to a flanker in a quarter of the time: the maintainer's top concern is the bots.

## Do
1. Measure first: in a BF2 sweep on main, how often a bot's yaw stick saturates (|rx| = 1 after the clamp) and the time a
   bot takes to face a new target (from the target's pick to an error under the fire gate), per map - if the cap almost
   never binds, close the card with the numbers (the port's own aim tracker may be the limit).
2. If it binds: BF2 bots turn at 4 x MaxTurnSpeed (the soldier's rate for a bot, and the same value in `gunner_foot`'s
   stick), behind one constant; BF1 unchanged. Sweep 8 x 32 + the maintainer's Kamino scene; sort any refusal by the rule.

## Acceptance
- The measure table (saturated share and time-to-face per BF2 map, main) in the card.
- If built: a scene test - a BF2 bot facing 180 deg away from a visible enemy faces it in the PS2's time (pi / 20 s plus
  its aim tracker's lag) and a BF1 bot in pi / 4; the mutation (x 1) fails it; the sweep gate's rows.