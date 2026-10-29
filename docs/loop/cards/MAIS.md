---
id: MAIS
title: Mission AI settings - the scripts' view multiplier, team aggressiveness, snipe suitability distance and AI capture bans reach the bots
milestone: M3
value: 5
phase: done
status: done
deps: BSTK
files: scripts/gen_missions.py; engine/crates/game/src/mission.rs; engine/crates/core/src/sim/state.rs; engine/crates/core/src/sim/round.rs; engine/crates/core/src/cp.rs; engine/crates/core/src/commander.rs; engine/crates/core/src/ai/snipe.rs; engine/crates/core/src/ai/behave/brain.rs; engine/crates/core/src/ai/gunner.rs; engine/crates/core/src/ai/man.rs; engine/crates/game/src/main/battle.rs; engine/crates/game/examples/round_to_outcome.rs; engine/crates/game/examples/arena.rs
declares: sim
needs:
effort:
branch: botup
resume:
created: 2026-10-28
---
## Why
The mission scripts set AI knobs that the mission RON dropped (`lead_tmp/botre/xbox_pc.md` finding 4): BF2 Dagobah's
bots see 35 % as far and Felucia's 65 % on the PS2, and openbf's saw 100 %.

## Evidence
What the conquest scripts the port plays set (PS2 bytecode, `gen_missions.py`'s own call replay):

| knob | missions | PS2 meaning |
|---|---|---|
| `SetAIViewMultiplier` | BF2 dag1 0.35, fel1 0.65 (both eras) | clamp [0.1, 1], times every AI's view distance (`FUN_002e37c0` / `FUN_002e3910`, re-ai-difficulty 3, 5.1) [V] |
| `SetTeamAggressiveness` | 27 missions, 0.7-1.0: BF1 and BF2 end1 Imperials 0.7, BF1 nab1 0.75, kam1 / rhn2c 0.9, many 0.95, the rest 1.0 | stored (BF2 team+0x90); no reader known in BF2 (open question 5). BF1: re-ai-difficulty 3 names `FUN_00238090` as its reader, but that function multiplies `+0x94` of its first argument, which re-grenades correction 1 and re-soldier-ai 24.1 [O] read as the WEAPON's issued clips; the team's own setter `FUN_002ae948` also writes a `+0x94`. Unresolved [H] |
| `AISnipeSuitabilityDist` | BF2 cor1, dea1, mus1, pol1, tan1, tat3: 30 (default 70) | clamp [0, 100]; radius of an entity query centred that far along a snipe hint (`FUN_002d8398`), whose answer the BF2 trooper never uses (re-soldier-ai 20: "changes nothing") [V] |
| `AICanCaptureCP(cp, team, false)` | BF2 nab2: all six posts barred to team 3 (the Gungan locals) | bit `team` of cp+0x80: an AI soldier of that team does not count in the capture scan (re-cp-capture 4) [V] |
| `SetAIDamageThreshold` | BF2 kam1: `Comp1`..`Comp10`, every one 0.0 | entity+0x18c, default 0, read only as a flag != 0 (re-ai-difficulty 3, open question 7) [V store, H meaning] |

`SetDefenderSnipeRange(170)` (BF1 and BF2 Hoth) was not in this card's list; BF2's is a stub, BF1's is a snipe-hint
query radius (re-ai-difficulty 3): not built.

## Change
- `gen_missions.py` reads the five calls (last call wins; `AICanCaptureCP` bans still in force) and writes an
  `ai: (view_mult, aggressiveness, snipe_suitability, no_capture)` block, only where the script sets one. A non-zero
  `SetAIDamageThreshold` would be named under PROVISIONAL as not carried; none exists in a conquest script, and
  0.0 is the default, so the threshold needs no field.
- `MissionDef::ai` / `apply_ai` put them into `Sim::mission_ai` (an input, not hashed; Lua team t = port team t-1)
  and the bans on the posts by name, after the posts are in (game battle, sweep, arena).
- View: `Sim::ai_tier(team)` is the difficulty tier with `view_mult` times the mission's; every view distance a bot
  uses reads it (the think's perception, the seat gunner, the turret man's candidates, the sniper's lane).
- Capture: `Sim::counts_in_cp` skips an AI soldier of a barred team (a human still counts).
- Missing values are the engine defaults (1.0, 1.0, 70 m, no bans), so a mission that sets none plays as before.

## Deviation
1. **Aggressiveness [H, ours]**: no reader is known, so the port gives it one effect: a team defends a threatened
   post with `DEFEND_SHARE / aggr` of itself (`commander::defend_share`, at most all of it) - 0.7 keeps 6 of 16 on a
   post instead of 4. 1.0 is card DEFS's quarter. Only the BF2 goal holds posts (BF1's draws again at once from an
   own post), so BF1 maps carry the value and do not change.
2. **Snipe suitability [ours]**: a sniper's lane counts ground out to `2 d` only (the far edge of the PS2's own
   query sphere). The default 70 m reaches past every tier's sight; the indoor maps' 30 m stops a lane at 60 m,
   under the Hard (61 m) and Elite (73 m) sight only.
3. **Capture ban in the draw [ours]**: a bot does not draw a post its team may not capture to take it (a held one
   is still defended). The PS2's draw has no such test, and its barred bots would stand on the post for nothing.
   In conquest only BF2 Naboo's locals are barred, and the port fields no locals: no change today.

## Acceptance
Tests (each mutation-checked: the rule broken, the test fails, the file restored):
`perception::tests::the_missions_view_multiplier_shortens_every_bots_sight` (an enemy at 30 m seen, unseen at 0.35;
at 20 m seen at Hard x 0.35, not at Normal; mutant: the think's tier without the multiplier),
`commander::tests::a_less_aggressive_team_keeps_more_defenders` (mutants: `defend_share` ignoring the value; the
plan not reading it), `commander::tests::a_post_barred_to_the_teams_bots_is_not_drawn_to_take`,
`sim::round::tests::a_post_barred_to_a_teams_bots_does_not_count_them`,
`ai::snipe::tests::the_snipe_suitability_distance_bounds_the_lane`,
`mission::tests::the_missions_ai_settings_reach_the_sim` (mutant: Lua team = port team), and
`mission::tests::shipped_missions_carry_their_ai_settings` (skips without missions).

Sweep `round_to_outcome -- 60000 42`, `STUCK_CENSUS=200` (before = BSTK; `lead_tmp/botre/runs/MAIS/`): 34/34 finish
both, stuck_max median 180 -> 180, sum 5640 -> 5640. Three maps moved (the sweep plays each map's `_con.ron`):

| map | before | after |
|---|---|---|
| bf2_dag1 (view 0.35) | t0 wins 345 s, fight 46 % of foot-s, overwatch 179 s, contact within 20 m 11.7 % | t0 wins 480 s, fight 23 %, overwatch 0, contact20 25.1 % |
| bf2_fel1 (view 0.65) | t1 wins 408 s, fight 39 %, overwatch 133 s, contact20 10.6 % | t0 wins 316 s, fight 38 %, overwatch 40 s, contact20 17.9 % |
| bf2_end1 (Imperials 0.7) | t1 wins 525 s, guard 86 s | t1 wins 582 s, guard 348 s |

On Dagobah a Normal bot's sight (17 m) is barely over a lane's nearest point (15 m), so snipers find no spot: the fog
ends overwatch there.

## Regeneration
The RON files are game data (gitignored; tracked on the local assets branch only). After this commit:
`python3 scripts/gen_missions.py` (with `extracted/` and `community_mods/` present; `scripts/pack.py` runs it with
`--ps2-only`). Against the current files it only adds the `ai` blocks (43 of 61 files); the four Tatooine files
last written `--ps2-only` also gain their PC-source header lines. A regenerated set is in
`lead_tmp/botre/runs/MAIS/missions_regenerated/`.

## Open
- The clone-wars missions are not in the sweep (dag1 and fel1 cw carry the same multipliers).
- `SetDefenderSnipeRange` (BF1's snipe-hint radius) and `SetAIDamageThreshold`'s meaning are not built.
- re-ai-difficulty 3's BF1 aggressiveness consumer conflicts with re-grenades correction 1 (above); one of the two
  notes needs correcting.
