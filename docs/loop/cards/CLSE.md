---
id: CLSE
title: Inside 10 m a PS2 bot's aimed weapon deals 65-90 hp a second, the port's 21-33 (beyond 20 m they are equal): split the port's close-range aiming time by firing, hit share, weapon and speed, the PS2's from SAIR's raw packets, name the cause and build it
milestone: M3
value: 5
phase: build
status: done
deps: FGHT
files: engine/crates/core/src/ai/gunner_foot.rs
declares: sweep; arena; sim
needs:
effort:
branch: clse
resume: landed 77f4c70e 2026-10-09
created: 2026-10-09
alpha: yes
---
## Why
Card FGHT2's range measure (2026-10-09, `analysis/re-soldier-ai.md` 15.5): per second that a bot aims at a living
enemy, the damage that enemy takes is, inside 10 m, **65-90 hp on the PS2 (four BF2 captures) against 21-33 on the
port**; at 10-20 m 23-42 against 13-29; beyond 20 m about equal (18-29 against 11-26). A close fight on the port
lasts two to four times as long as the PS2's: bots that meet at a doorway or on a post stand shooting past each
other. It holds with the PS2's slow think knocked into the port (31-38 inside 10 m), so it is not awareness.

## Evidence
- `engine/artifacts/lead_tmp/fght2/tools/rng.py ps2 <sair units.csv>` / `port <botq dir> <map>` (an "aimer" = a bot
  on foot whose aim target is a living enemy: the PS2's aim order type 0 with an entity, the port's `target`; every
  fall of health is split over the enemies aiming at the victim); outputs `out/rng_kam1_pol1.txt`, `out/rng_more.txt`.
  PS2 captures `lead_tmp/sair/ps2/bf2_{kam1_v2,kam1_playtest,pol1_v2,pol1_alive}_units.csv`; the port's rows
  `lead_tmp/fght2/sw/base/bq/` (main 853ecf0, 8 seeds).
- Caveats to settle first: the PS2's inside-10 m aimer time is 65-140 s a capture; 23-28 % of the PS2's damage has
  nobody aiming (blasts, turrets, strays) against the port's 8-10 %; the port's `target` may be held on a bot that
  cannot fire (no line, reloading, mid-roll).

## What to do
1. Split the port's inside-10 m aimer time: firing or not (shots a second from the `.ev.csv` shot rows), hit share
   by range, the shooter's and the target's speed (the circlers), the weapon in hand (GRAI's pistol up close).
2. The PS2's side from SAIR's raw packets (`sair/tools/rawread.py`): the trigger byte ai+0x80 per sample (shots
   are not sampled; the firing channel C+0x84 is, see `lead_tmp/audit-grai/ps2/firech.py`), the weapon slot, both
   units' speeds - the same split.
3. Name the cause (aim error at short range, burst pauses, the circle spoiling the shooter's own aim, the weapon
   choice) and build the fix as its own switch; null variant first, 8 seeds x 32 maps, the maintainer's Kamino scene.

## Acceptance
- A table PS2 | port: damage per aimer-second by range (0-10, 10-20, 20-30 m) split by firing / not firing and by
  weapon, on BF2 Kamino and Polis Massa - in the report and re-soldier-ai 15.
- Then a landed build with the port's inside-10 m figure at 0.67-1.5 x the PS2's on both maps, kills a minute not
  below today's (Kamino 17.9, Polis Massa 18.2 over 8 seeds), 256 of 256 rounds - or a note saying which caveat
  dissolved the gap, with the numbers.

- 2026-10-09 12:45 planner. **value 4 -> 5, not in BOTV's deps.** Inside 10 m the port's bots deal a third of the PS2's damage a second (21-33 hp/s against 65-90, FGHT2's report): a bot next to the player is three times too harmless, and bots are the maintainer's first concern. Not a dep of BOTV: this is a measure card that may end in a build card of unknown size. If it files a build, the next planner decides whether BOTV waits for it. Undo: `loopctl.py set CLSE value=4`.
- 2026-10-09 16:58 measure + build (lane 2; report `reports/20261009_1658_card_CLSE.md`; `analysis/re-soldier-ai.md` 17). **Measured, the cause named, its rule LANDED 77f4c70; the acceptance bar is met on Kamino and NOT on Polis Massa.** Split (two new PS2 captures with the weapon in hand every sample; the port with a read-only observer): firing time and rate are equal - what differs is the damage a shot lands (a rifle's falls of its target's health per shot inside 10 m: the PS2 0.64, the port 0.20) and the weapon in hand (rifle 52-55 % against 30-31 %, noted on GRAIA). Cause, by code (one verifier, 11 claims: 9 supported, 2 partly, folded in): BF2 scales the aim error vector - x 0.3 against a target that carries no human, x (d / 20 + 0.5), y never below the aim point, zero for a human's passenger - and the port applied the radius whole. Landed for BF2 rounds: Kamino inside 10 m 33.4 -> 53.1 hp per aimer-second (the PS2 70.6: **0.75 x**), Polis Massa 20.6 -> 28.9 (80.8: **0.36 x**); kills a minute Kamino 17.4 -> 18.7, Polis Massa 17.9 -> 17.9, all maps 30.6 -> 31.7; 256 of 256 rounds; team kills 1.9 -> 1.5 % of kills; 9 of 9 mutations fail a test. Follow-ups: CLSE2 (the error block is the target's own, the size term, the class factor, BF1's twin), CLSE3 (Polis Massa's rest).
