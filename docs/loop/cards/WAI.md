---
id: WAI
title: Bots drive walkers: a bot claims a walker's pilot seat and steers it by the PS2's own stick law (re-walker-ai.md: turn in place to 5 deg then walk; the AT-ST / tauntaun strafe branch inside 30 deg; sticks raw and unshaped)
milestone: M2
value: 5
phase: build
status: done
deps: WALK,GSKIP
files: engine/crates/core/src/ai/ride.rs,engine/crates/core/src/ai/walker_drive.rs,engine/crates/core/src/sim/seats.rs,engine/crates/core/src/vehicle/walker.rs,engine/crates/game/src/vehicle_class/walker.rs,engine/crates/core/src/sim/state.rs
declares: sweep
needs:
effort:
branch: wai
resume: landed bbdd58c5 2026-10-02
created: 2026-09-29
alpha: yes
---
## Why
WALK (2026-09-29) moves walkers for a human driver only. No bot boards a walker (`claim_seat` requires
`Sim::hover_params`), so the AT-STs, tauntauns, spiders and kaadus on the pads stand empty unless the player takes them.
On the PS2 bots ride them (BF2 HOT1: both walkers piloted at mission 60 s and again at 177 s; BF1 NAB1: 1-2 of 3 kaadus
in the first minute).

## Evidence — the RE is done (2026-10-01, `analysis/re-walker-ai.md`, [O], verifier 37 / 2 / 0)
- The bot steers with the LEFT stick's X (E+0x2e0, the walker's p3). The law, in the port's terms (§7 of the doc;
  s = n·right, c = n·fwd, n = the flat unit direction to the goal, R = 4 m):
  - plain branch: p3 = clamp(s/(MaxTurnSpeed·dt), ±1); p2 = 0 until c ≥ cos 5° (0.9961935), then p2 = c;
  - strafe branch (class flag bit 0 = `walker::flags::AI`, and c > cos 30°): |s| < 0.4c → p2 = 1, p3 = −s/(0.4c);
    else p2 = 0.4c/|s|, p3 = −sign(s);
  - stopped (no goal, or within R with no route left): p2 = 0, p3 = the plain turn toward the bot's target, else 0.
- The sticks are raw: a walking spider holds (0.997, 1.0). They must not pass `driver_input`'s shaping or the i8 pad.
- The port's `walker::locomotion` C branch already does the rest, including the PS2's own quirk: a BF2 spider / AT-TE /
  AT-AT turns away from its goal while walking and crabs along ~5° off it (live: median 5.31°).
- Replayed against PS2 RAM: 7504 ticks (spider, AT-ST, tauntaun), both sticks and the yaw step equal on every one.
- Tools and data: `lead_tmp/wai/` (`wai_sample.py`, `wai_check.py`, `ps2_rows.csv`, `live_*.csv`).

## Acceptance (the build runs these and pastes them into its report)
1. `cargo test -p core`: the stick function reproduces all 145 rows of `lead_tmp/wai/ps2_rows.csv` to 2e-3 (embed the
   rows in the test; assets stay out of git — these are numbers, not assets).
2. Core tests on flat ground, a bot in the pilot seat, the goal 45° to one side at 150 m:
   - BF2 AT-ST class (bit 0, e08 / e0c = tan 87° / 88°) and a BF1-disc class (no flags, e08 = e0c = 0): the walker turns
     TOWARD the goal's side, the heading error falls under 1° and the ground speed settles at MaxSpeed's 5.0 ± 0.1 m/s;
   - BF2 spider class (no bit 0): the error settles in 4-6.5° (PS2 median 5.31°), the throttle toggles between 0 and
     ≥ 0.996, mean root speed 3.3-3.9 m/s (PS2 3.63).
3. `round_to_outcome`, one line per map: walkers boarded by bots at 60 s / 180 s and metres driven per piloted walker.
   PS2 (`reports/20261001_2340_card_WAI.md`): BF2 HOT1 2 of 2 at 60 s, 1 of 2 at 120 s, 2 of 2 at 177 s; BF2 GEO1 both
   spiders 61-161 s; BF1 NAB1 1-2 of 3 kaadus between 36 and 60 s, 1 of 2 at 90 s, 0 of 1 at 120 s, no kaadu left at
   150-248 s; BF1 HOT1 every
   tauntaun at 40-60 s. The port within 1 walker of those at 60 s on BF2 HOT1 and BF1 NAB1. Path: the PS2 AT-ST covered
   118 m in its best 30 s, a ridden kaadu 445 m. `lead_tmp/audit-kaad/port_bf1_nab1_con*.txt`'s run shows ≥ 1 walker
   boarding.
4. 8 seeds: no map's finish count drops; `veh` world deaths per map before and after.

## Not in this card (re-walker-ai.md §8, §11.5)
Boost (which game situations set speed mode 3 is [H]); the 7.5 s yield after touching another vehicle (the port has no
walker-vehicle contact); the 20 % bail-out (card VBAIL, hover and walker together).

## The original card text (2026-09-29)
- Why: WALK moves walkers for a human driver only; no bot boards a walker; the card's own 'bots that already board
  walkers now move them' was vacuous: none did.
- Evidence: analysis/re-walker.md §4.5 C and §11 (FUN_00262248 → FUN_002a54d8, not transcribed [H]); C's factor is
  negative on BF2 classes for |p3|/|p2| < 19: a bot pad that steers like a hover's would turn the walker the wrong way.
- Acceptance: RE FUN_002a54d8 (what stick values the PS2's AI writes for a walker, and the boost mode 3); build:
  claim_seat admits walker pilots; the driver pad per the RE; the sweep shows walker pilots and their distance.

## Planner note (2026-09-30, M-ALPHA)
`alpha: yes` — bots driving walkers (HOT1, NAB1). Value 4 (kept). Showcase maps: BF1 NAB1, BF1 TAT1 (the port's `tat1`), BF1 YAV2, BF2 UTA1, BF2 MYG1, BF2 HOT1 (PLAN §Milestones M-ALPHA).

## KAAD audit note (2026-09-30, `reports/20260930_0645_audit_KAAD.md`)
- **The PS2's bots ride the NAB1 kaadus.** [measured, PS2 RAM]
  - In `bf1_nab1_spawnmap.p2s` and `bf1_nab1_cp1_cp2_postA_spawn.p2s`, two of the three kaadus carry a Republic bot, a
    clone sharpshooter and a clone trooper. In `bf1_nab1_sideselect.p2s`, one does.
  - The rider's feet (C+0x228) sit 2.03-2.17 m above the kaadu's origin. The rider's C+0x198 holds the point where it
    mounted, which is where the kaadu had crept to (card WALKB1's note).
  - Live, in the 1-unit GCW state, a kaadu was boarded 10.6 s into a sample and ran off at ~18.5 m/s.
- **The port's bots board no walker.** They board every NAB1 hover and no walker in 5,400 ticks of either era
  (`lead_tmp/audit-kaad/port_bf1_nab1_con*.txt`).
- The kaadu belongs in this card's acceptance beside the AT-ST, tauntaun and spider.

- Planner 2026-09-30 21:45: `deps` + GSKIP. This card's diff lies under `engine/crates/core/`, which the selective gates skip for goldens, yet core feeds golden pixels (the gallery vantage's `ray_down` on COR1 / DEA1 / MYG1 / TAN1 / UTA1 / BF2 KAM1, the heightfield under grass and foliage, the walker pose). GSKIP corrects the rule first; `land.py` has no flag to force every group.

## Planner note (2026-10-01 22:00) - value 4 -> 5, and the check the build must run
- **Why 5:** M2 had no value-5 card, and the maintainer's seven `first:` cards (all M3, all on `ai/brain.rs`, so one at a time)
  leave the other lane free. Of the open M2 cards this is the one a player sees first: on both Hoths (BF2 HOT1 is a
  showcase map) and on NAB1 every walker stands empty unless the player takes it, while the PS2's bots ride them
  (KAAD audit above, PS2 RAM). `files:` do not touch the `first:` cards' files.
- **The runnable check (the vehicle referee, the maintainer 2026-09-30: audits found a problem in 6 of 7 vehicle landings).**
  Before the build lands, paste into the report:
  1. PS2 side, measured in the RE session: on BF1 NAB1 and one Hoth, how many walkers carry an AI pilot at 60 s and at
     180 s of an AI battle (`lead_tmp/sqveh/sv_live.py` reads the seats; `ps2-ai-battle-sampler` memory for BF2), and
     one piloted walker's path (distance over 30 s, the sign of its turn toward a goal off to one side).
  2. Port side: a `round_to_outcome` line per map - walkers boarded by AI at 60 s / 180 s, distance per piloted
     walker - within 1 walker of the PS2's count on those two maps, and a core unit test: a bot pad with the goal 45
     degrees to the left turns the walker LEFT on a BF2 class and a BF1 class (the e08 / e0c sign trap above).
  3. 8 seeds: no map's finish count drops; `veh` world deaths per map before and after.
  If the RE session cannot get the PS2 numbers in 1, the card is `todo` with `resume:` naming what blocked it - not
  `parked` (the picker reads `parked` as a branch waiting to land; card LRES).

## Result (2026-10-02, landed bbdd58c; report `reports/20261002_0335_card_WAI.md`)
- Acceptance 1 PASS (145 / 145 rows, worst 3.45e-6). 2 PASS (AT-ST and BF1 class 0.000 deg at 5.000 m/s; spider median
  4.84 deg, 3.76 m/s). 3: BF2 HOT1 1 of the PS2's 2 at 60 s, BF1 NAB1 kaadus 0-2 against 1-2, BF2 GEO1 spiders 2/2;
  BF1 HOT1 tauntauns 0-2 of 4-5 (short of the PS2 — card WAI2). 4 NOT MET on one map: BF1 GEO1 8/8 -> 7/8 (the
  raised-post trap, STLVL's); `veh` deaths 445 -> 528 over 8 seeds (bf1_hot1 0 -> 106, tauntaun riders shot).
- Also built, not on the card: `route::post_hub` (a walker rides to the reachable hub nearest a post its graph does
  not reach — without it nothing was ever boarded on either Hoth), the hull-radius stop, the armed walker's
  stand-and-fight. Hovers were left out of `post_hub` (card VDROP).
