---
id: JETP
title: Jet and jump packs: BF1 dark trooper (JetJump 21, no fuel cost), BF1 jet trooper (JetJump 5, JetPush 8, JetType hover, fuel), BF2's jet troopers, the Geonosians' wings - the port has no jet physics, only a nav flag
milestone: M3
value: 3
phase: re
status: blocked
deps:
files:
declares:
needs: restore both PS2 corpora/ELFs, class data, SAIR jet evidence and runtime inputs before retry
effort:
branch:
resume: 20261024_2347 input hold; both-game jet physics and arc RE remain unproved; restore report inputs before another attempt; engine/artifacts/loop/reports/20261024_2347_card_JETP.md
created: 2026-09-29
defer: post-alpha
tries: 1
---

## Why
UAUD1 (analysis/unit-audit-bf1.md §3.6). The dark trooper fields on every BF1 GCW map and the jet trooper on every BF1 CW map; the BF1
setter reads JetJump at +0x18d8 (B1 123562) and JetPush (123503). The port turns `JetJump > 0` into a planning-graph
flag only (game/src/roster.rs 396, core/src/nav.rs 53-61). PLAN §LOCKED FEATURE SET: "Jet/jump-pack: BF2 feel;
infinite fuel via cheat".

## Evidence
| class | JetJump | JetPush | fuel cost / initial / recharge | other |
|---|---|---|---|---|
| imp_inf_dark_trooper (+snow) | 21.0 | 0.0 | 0.0 / 0.4-0.5 / 0.025 | JetEffect darktrooper_jetpack |
| rep_inf_jet_trooper | 5.0 | 8.0 | 0.2 / 0.25 / 0.1 | JetType hover, EngineSound rep_inf_Jetpack_engine_parameterized |
| geo_inf_geonosian (local) | 8.0 | 8.0 | 0.1 / 0.2 / 0.03 | JetDuration 5.0, JetType hover |

## Acceptance
- RE ([O]) both games' jet state (the jump, the hover push, fuel, the BF2 feel PLAN names); the AI's use of
  `JET_JUMP` arcs.
- Build it in `core` (sim / sweep declared) with the effect and sound hooks.

## Notes
- UAUD2 (2026-09-29, analysis/unit-audit-bf2.md §3.6): BF2 values. Jet troopers (`rep_inf_ep3_jettrooper`, `_ep2_`): JetJump 5, JetPush
  8, JetAcceleration 30, JetType hover, fuel cost 0.16 / initial 0.25 / recharge 0.12 / min border 0.24, effect
  `rep_sfx_jetpack`, their own ControlSpeed jet row (1.5, 1.25, 1.25). Dark trooper: JetJump 21, JetPush 0,
  JetAcceleration 10, fuel 0 / 0.5 / 0.075 / 0.49, `imp_sfx_jetpack_darktrooper`. The jedi heroes: JetJump 10,
  JetEnergyDrain 40.

## Planner note (2026-09-30, M-ALPHA)
`defer: post-alpha` — not needed for the alpha's six showcase maps or its checklist. Value 3 (kept).

- **AUDIT AIRC, 2026-10-02 (its verifier's find):** a fly-height term follows the mover in both games' air cases
  (jump / fall / thrown): within 5 m of the mission's ceiling (BF2 `SetMaxPlayerFlyHeight` -> DAT_00697e54, BF1
  `SetMaxFlyHeight` -> DAT_005873b0; both default 200.0 - `re_check.py writers`)
  vy += ((ceiling - feet.y) - 5) x dt x 10 (BF2 0x2441e4-0x244228 in `FUN_00240bf8`, BF1 0x207a10-0x207a58 in
  `FUN_00205cc8`; `analysis/re-stance-roll.md` §4.3b). The port has no fly height for soldiers; it matters to this
  card's jets and to any map whose ceiling is within 5 m of a jump's apex.

## SAIR 2026-10-02 - when a PS2 bot jets (`analysis/re-soldier-ai.md` 4.7, AGENT_F Q2, AGENT_H Q6)
- In a fight (attack sub-helper): any jet class with fuel hops every randint(6, 12) s to a random clear spot 20-57 m
  away with open sky above it (the straight line is tested for barriers only where the script calls
  `SetAllowBlindJetJumps(0)` - the Kamino scripts); jump-jet classes go to a JetJump hint (TYPE "4") within 40 m every
  randint(3, 8) s and jet 80 m along its direction; hover-jet classes fly at a target 20 m or more away when the
  fuel lasts and no barrier crosses the line. On a route: the follower sets jump or jet from the connection's flags.
  The deathmatch agent jets straight up at random. BF1: an evade jump every 3-8 s and JetJumpHintHelper.

