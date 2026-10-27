---
id: SSELA
title: The spawn flow in the sim: a human slot is not on the field until it accepts - side (Instant Action level start: team 1 / team 2 / Auto), then post, then class; after a death the 4.0 s timer, then post and class again; the human skips the spawn wave (the PS2's rule); SPAWN_ACCEPT / SPAWN_BACK ride the input log
milestone: M5
value: 5
phase: build
status: done
deps: SSEL
files: engine/crates/core/src/sim/input.rs, engine/crates/core/src/sim/spawn.rs, engine/crates/core/src/sim/step.rs, engine/crates/core/src/sim/state.rs, engine/crates/input/src/lib.rs, engine/crates/game/src/mission.rs, engine/crates/game/src/main/battle.rs, engine/crates/game/src/main/windowed.rs
declares: sim; arena
needs:
effort:
branch: ssela
resume: (private workflow note omitted)
created: 2026-10-20
alpha: yes
domain: sim
---
## Why
The maintainer (2026-10-19, INBOX; Reddit's top ask after the release): at the start of a level the player picks a side (or random),
then a command post, then a class; after a death the post and the class again. The PS2's BF2 does exactly that
(`analysis/re-spawn-flow.md` §0-§6, [O], 26 SUPPORTED / 9 PARTLY / 0 NOT SUPPORTED). In the port today the shell fixes
the player's team before the battle (`mission::pick`, the shell's "Team 1" army is the player's), the player is put on
the field alive at once, and a dead player respawns by himself when the 4.0 s timer AND the team's spawn wave
(`SpawnWaves`) allow it. This card is the SIM half; the screens are SSELB, the death camera SSELC.

## Evidence (the PS2 rule, `analysis/re-spawn-flow.md`)
- One state per viewport: 0 side, 2 spawn map, 3 class, 5 accepted (§1). X accepts (side -> 2; post -> 3 only with a post
  selected; class -> spawn), Triangle backs (3 -> 2; offline nothing on 2; the side screen has no Back).
- The side screen is shown only where sides may be chosen (Instant Action; the campaign and Galactic Conquest open on the
  spawn map, §1). After a death the display re-opens on the spawn map, never the side screen (§1, B2:392616).
- Side options offline: team 1, team 2, Auto Assign (Spectate hidden). Up / Down move. PS2 offline Auto Assign keeps the
  player's current team (team 1 at the start; §2 [O], 3 of 3 maps live). **Named deviation (reversible, §9):** the port's
  Auto Assign draws the team with the PS2's own offline coin flip (`FUN_0036fc40`: `((LCG >> 16) & 1) + 1`) from the
  sim's LCG, because the maintainer asked for "a side (or random)". Reverse: Auto keeps team 1.
- The human never waits for a spawn wave and never spawns without the class accept (§5 [O]): the soldier exists 0.10-0.13 s
  (3-4 updates) after X on all three maps measured. A death sets the timer to 4.0 s (§6.1); the display opens when it has
  run out (2 updates after it reads <= 0, §6.3); the post and the class are kept from the last life (§6.4).
- No tickets left: the display does not open, no respawn (§1).

## Do
- `core::sim::input`: `SPAWN_ACCEPT` and `SPAWN_BACK` bits (edge-triggered, read only while the slot is not on the field,
  as SPAWN_POST_* are); `input` crate: X / Triangle (keyboard: the menu accept / back keys) while dead or before the first
  spawn. Side choice rides SPAWN_POST_UP / DOWN on the side screen (the PS2's Up / Down), or its own bits - builder's call.
- `Sim`: per slot `spawn_stage` (Side / Map / Class / Accepted) and `side_choice` (team 1, team 2, Auto), both hashed
  (compare two different SET values in the hash test - SPSEL's lesson). A slot is not on the field until it accepts the
  class; Accept on the side sets the slot's team (Auto: the coin flip above). After a death: the 4.0 s timer, then stage
  Map with the last post / class kept. The human's respawn skips `SpawnWaves::ready` (BF2 FUN_0034cea8 skips the wave
  tests for a human, `B2:344186-344192`).
- `game`: the battle starts with the player slot reserved and empty (no unit) when the mission allows a side choice;
  `mission::pick` still picks the mission and the armies; `player_team` becomes the side the sim settles.
- Headless referees (`round_to_outcome`, `arena`, the sweep): they already vacate the player slot
  (`Sim::vacate_player_slot`); an un-accepted slot must look the same to them (no unit, no reinforcement spent, not
  counted in `alive()`). Keep an auto-accept switch for tools that need a player on the field (the black box's play,
  `--spawn`, the golden scenes).

## Acceptance (each RUN and pasted into the report)
1. A core test: a slot that never presses accept has no unit after 60 s and its team's reinforcements are untouched;
   Down, Accept (team 2), Accept (post), Accept (class) puts it on the field on team 2 at the chosen post as the chosen
   class within 4 ticks of the last Accept; Back on the class stage returns to the map stage.
2. A core test: after a death the slot is not respawned before 4.0 s and not without the class Accept, and IS respawned
   within 4 ticks of the Accept while its team's `SpawnWaves` says not ready (the human skips the wave).
3. Auto Assign: over 32 seeds both teams occur (the coin flip); a mission flagged as "no side choice" opens on the map stage.
4. An input-log replay test with the new bits reproduces the state hash; two different side choices hash differently.
5. Gates: only declared moves (`sim`, plus any golden SSELB has not taken); sweep / arena / fidelity unchanged (the
   referees' player slot is vacated already).

## Notes
- State 4 (the class select opened while alive at a friendly post, `FUN_00397f00`) is not in scope: which button and which
  object open it is not read (re-spawn-flow §11).
- Split screen: one stage per player slot (the PS2: one display per viewport).
- UISN plays the screens' sounds from these transitions (accept `+0x74`/`+0x64`, back `+0x7c`, re-ambience-music §6.3).
- planner 2026-10-20 05:20: `alpha: yes` (and on SSELB): class select on spawn is on PLAN's alpha checklist, and SSEL carried
  The maintainer's `first: 5` ahead of BSHL's 6, so SSELA / SSELB sort before BSHLA. Undo: `loopctl.py set SSELA alpha=` (and SSELB).

## Build result (2026-10-20, branch ssela 6a26ddc)
- Sim: `Sim::spawn_screens` (input), per-slot `spawn_stage` (Side / Map / Class / Accepted, the PS2's 0/2/3/5) and
  `side_choice`, hashed only when the screens are on or a slot holds a non-default value; `Sim::open_spawn_screens(slot,
  side_choice)` holds the slot's unit dead without spending a reinforcement (the held unit is what the class accept
  brings to life, so `walk.player` always has a unit). Accept on the class spawns ON that tick. The human skips the
  spawn wave in both modes. `spawn()` no longer reuses a dead unit a player slot holds.
- Input: Cross = JUMP | SPAWN_ACCEPT, Triangle = ACTION | SPAWN_BACK. Class screen: D-pad Left = next, Right = previous
  (re-spawn-flow §4) plus R1 / Select. Side: D-pad Up / Down.
- Game: the shell battle calls `open_spawn_screens(0, true)` (every shell battle is Instant Action);
  `OPENBF_SPAWN_SCREENS=0` turns it off; the CLI walk / headless / goldens keep auto-accept. The old egui panel shows a
  placeholder side list and the stage's X / Triangle line until SSELB draws the PS2 screens.
- Acceptance 5 premise corrected: the ARENA keeps its idle player (only the sweep vacates slot 0), so the human
  wave-skip moves `arena 9000 42 tat2` (easy / hard / elite rows; normal identical). With the wave wait put back for the
  human the arena output is byte-identical to main's (`lead_tmp/ssela/arena_mut.txt` vs `lead_tmp/arvs/land/main/
  arena_tat2_1.txt`), so the move is exactly this rule: declared `arena`.
- NOT run: the windowed shell battle with a pad (the game is pad-only; Xvfb's xkey cannot press Cross). The maintainer's first
  shell battle is the check: the side list, then the map, then the class, X each.
