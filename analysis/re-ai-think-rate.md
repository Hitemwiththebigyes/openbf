# OPUS RE (loop lane 1, 2026-10-22, card AITHK) - [O] UNAUDITED
# RE: how often a PS2 soldier bot thinks - BF2's interval by level of detail and its "think now", BF1's round

**The question** (card AITHK; the maintainer 2026-10-19, speed on a weak laptop; card MYGC, 2026-10-22, a fidelity gap). Every
port bot thinks every 10 ticks. What does the PS2 do with far or unseen bots, and what would a deterministic port rule
be? Most of the BF2 rule was read and replayed by card SAIR (`re-soldier-ai.md` 1.1-1.3, 1.6); this note re-checks it
as one rule a build can follow, and adds what SAIR left: **a bot that registers the HUMAN as a threat thinks at the
next step, at any level** (read, and seen on the running PS2: 28 of 28 such entries, 0 of 209 AI entries), the other
"think now" sites, the first think after a spawn, the camera raise and BF1's round in full.
Tags: [V] read (B2 / B1 = lines of `SWBFRNT2.ELF-decomp.c` / `SWBFRONT.ELF-decomp.c`), [D] derived, [H] hypothesis,
[L] seen on the running PS2; `[O]` = put to the verifier workflow (section 9). Scratch, tools, captures:
`engine/artifacts/lead_tmp/aithk/` (`tools/`, `ps2/`). Conventions as in `re-soldier-ai.md`: BF2 `ai` = the
UnitController, `C` = the soldier's controllable (E+0x270), `mv` = ai+0x430; one step = one sim update, 30 a second
(`0x3d088888`); the port's tick is the same 1/30 s (`core/src/lib.rs` `TICK_HZ`).

## 0. The answers

| item | BF2 | BF1 |
|---|---|---|
| who is scheduled | a time-ordered list of UnitControllers; each step at most **10** whose next-think time has come think (1) [O] | a 15-step round: each unit is visited once per round = every 0.5 s (7) [O] |
| the interval | **agent factor x {4, 3, 2, 1, 0.25} s by level 0-4**; trooper (capture walk) 2.0, combat 0.4, every other agent 1.0 (2) [O][L 99.8 % of 292,062 readings, SAIR] | 0.5 s; **1.0 s** when the unit is slower than 10 m/s, over 45 m from the first camera and outside its view (a skip of every other visit) (7) [O][L] |
| the level | from the HUMANS only, at most every 0.5 s, inside a think: 3; under 7 m 4, over 20 m 2; -1 outside the bot's view cone; -1 when the human's body was not drawn; max over humans; then raised to 3 / 4 inside a camera's view within 20 / 7 m + zoom x 8 m. **No human: 0** (3) [O][L] | none (only the camera test above) |
| think now | **a threat entry for a human-controlled unit -> the bot thinks at the next step, at any level** (seen, heard every 0.5 s, hit); an order message or a finished helper -> next step only at levels 3-4 (4) [O][L 28 / 28 human entries pulled, 0 / 209 AI entries] | none (no twin) (7) [O] |
| per step | path following every step; the aim tracker, the visibility factor and the mover's full update every 5th step at levels 0-1, 3rd at 2, every step at 3-4; detours only at 3-4; the soldier's AI input every other step when far and unseen (5) [O] | the soldier's whole update every other frame when far and unseen (7) [O] |
| first think of a life | at once (key 0 / next = now), with no order yet; the first order comes one interval later: **4.0 s with nobody playing**, 1.07 s near the player, 2.04 s far (6) [O][L] | at the unit's next visit |
| the port today | every bot every 10 ticks (0.33 s), both games; phase by unit index; the aim block every 5th tick (8) | the same |

So with no human - the headless sweep, the gates, and every PS2 bots-only column taken with the player parked - every
BF2 bot is at level 0: a capture walk re-decides every 8 s, a fight every 1.6 s, anything else every 4 s; BF1 bots every
1.0 s. Near the human the BF2 battle decides 1-10 times a second and every enemy that registers him reacts at the next
step.

## 1. BF2: the scheduler [O]
- [V] `FUN_002af948` (B2:234062-234149), once per step: each PlayerController's update (list `DAT_005fe088`); every
  15th call (`DAT_005fe084` = 15, ELF word; B2:234086-234089) `FUN_002af788`; then **at most 10** UnitControllers from
  the front of the time-ordered list `DAT_005fe08c`, while their next-think time ai+0x2c4 is not later than now, run
  the think `FUN_002b7530` and are re-inserted by their new time (B2:234094-234126, *(decompiler detail omitted)*); then EVERY
  UnitController of the list runs the per-step `FUN_002b7790` (B2:234127-234133). [verifier: SUPPORTED]
- [V] `FUN_002aef80` (B2:233684-233725) files a controller by class: a PlayerController (IsA `DAT_006980b8`, the string
  "PlayerController" at 0x621a20) into `DAT_005fe088`, a UnitController (IsA `DAT_00698110`) into `DAT_005fe08c` with
  key 0.0 - a new bot thinks at the next step. Equal keys keep list order. [SUPPORTED]
- [V] The think is reached only from that loop: one `jal 0x2b7530` word in the ELF (0x0c0add4c at 0x2afa30), no data
  word [receipt:e3b85d33] (both absence verifiers: SUPPORTED, by their own ELF scans).
- [D] The cap binds only when over 10 bots are due in one step (16 bots of the Kamino capture never were); a port
  rule keeps it, ordered by (next think, list order).

## 2. BF2: the interval [O][L]
- [V] The think (B2:239160-239263): ai+0x2c8 = now; a unit whose entity +0x228 bit 3 is clear (not spawned) gets
  next = now + 100000 and nothing else (B2:239205-239206); otherwise the level (3), the free-space probe, the noise pass
  `FUN_002de708`, the threat list `FUN_002b9a28`, the order queue and agent `FUN_002b5e60`, three weapon rules, the
  perception scan `FUN_002e3aa0`, and **next = now + `FUN_002b6fe8`(ai)** (B2:239258-239261). [SUPPORTED; the verifier
  lists five smaller calls the list left out: `FUN_002f4c78`, the +0x580 message block, `FUN_002a2358`, `FUN_002f5400`,
  `FUN_002baa20`]
- [V] `FUN_002b6fe8` (B2:238925-238944): factor x {4.0, 3.0, 2.0, 1.0, 0.25}[ai+0x560]; the factor is the agent's
  (ai+0x420) vtable slot +0x50, 1.0 with no agent. ELF words 0x621bc8..0x621bd8 = 4.0, 3.0, 2.0, 1.0, 0.25.
- [V] Factors (vtable at agent+8): UnitTrooperAgent 0x623618 +0x50 (word at 0x623668) = 0x2cba38, `lui at,0x4000;
  mtc1; jr ra` = **2.0**; UnitCombatAgent 0x622b40 +0x50 (0x622b90) = `FUN_002c4d18`: **0.4** (`FUN_002ce8a0`, lui
  0x3ecc / ori 0xcccd) when the agent's own state word is 2, else 1.0 (its ctor starts it at 1); UnitWaitAgent 0x623858
  +0x50 (0x6238a8) = `FUN_002c1f18` = **1.0**, the slot of Board / UnBoard / Random / WaitSecondary / Deathmatch /
  Destroy / Hold as well; Follow 0.4 in its state 4; CTF / Defend 0.8-2.0 by state (`re-soldier-ai.md` 1.2, AGENT_A Q1).
  [SUPPORTED]
- [D] In steps (30 a second, the first step at or after next): trooper 240 / 180 / 120 / 60 / 15 by level 0-4, combat
  48 / 36 / 24 / 12 / 3, others 120 / 90 / 60 / 30 / 8.
- [L] SAIR's replay (`lead_tmp/sair/tools/think_replay.py`): ai+0x2c4 - ai+0x2c8 = factor x table[level] on 291,678 of
  292,062 readings of four captures; the rest are think-now entries (4).

## 3. BF2: the level of detail [O][L]
- [V] `FUN_002b7b00` (B2:239522-239661), called only inside the think, recomputes only when now >= ai+0x568 + 0.5
  (B2:239545). For each player record (`DAT_00661330`, stride 0x1a4, count `DAT_00661334`) with +0x130 >= 0 (a human)
  and a controllable (+0x150, else +0x14c, else +0x148 - the vehicle seat first, so a driving human is measured at his
  vehicle): l = 3; squared distance under 49 -> 4, over 400 -> 2 (B2:239590-239594); -1 when `FUN_002a7230`(the bot's
  C, the human, 0) says the human is outside the bot's view (inside 10 m always in view, else within 77.5 deg of the
  bot's aim C+0xe0, `re-soldier-ai.md` 1.3; B2:239602-239605); -1 when the human's object +0xd0 has a render node
  (vcall +0x2c non-zero) that is not marked for the current frame (vcall +0x10 with `FUN_005be0e0()` returns 0). The
  level is the max over humans, 0 when none qualifies; ai+0x564 = the nearest human's record; the loop stops at 4.
  [SUPPORTED, with the render test's two conditions added by the verifier]
- [V] Then (B2:239639-239656), unless already 4: `FUN_001ba840`(size term x 3.0, 20.0, the camera list, the bot's
  position) and, if inside, again with 7.0 -> 4 if inside, else 3. The gate `FUN_002f7c88` is `jr ra; daddu
  v0,zero,zero` (ELF 0x2f7c88) - it returns 0, so the raise always runs. `FUN_001ba840` (B2:63770-63806) tests each
  camera of the list with a far distance min(the camera's own far value, D + camera+0x140 x 8.0) (`DAT_005fc4f4` = 8.0,
  ELF word) - about 28 m and 15 m at zoom 1 - and the sphere of that radius. The size term is `FUN_002a76b8` (0.25 x
  the class box's diagonal, `re-soldier-ai.md` 18.3). The camera list `DAT_005fc4ec` is a pointer set at run time.
  [SUPPORTED]
- [L] (SAIR) the rule without the render term is exact on 87-92 % of 10,777 readings, the misses one level; the cone
  term holds on 2,113 of 2,183 / 1,298 of 1,391 readings; with no human alive every bot sits at level 0; alive bot time
  by level on Kamino 0.3 / 30.9 / 62.4 / 4.2 / 2.2 %.
- [H] What the render mark means in play: the human's own body (or vehicle) not drawn this frame - in third person it
  is drawn almost always. The port has no first-person view (card FPV): treat it as drawn.

## 4. BF2: think now [O][L] (new)
- [V] `FUN_002af840` (B2:234020-234058): ai+0x2c4 = min(ai+0x2c4, now + 0.0001) (B2:234035-234037) and the controller
  is re-inserted by that time - it thinks at the next step. It has exactly five callers: five `jal 0x2af840` words
  (0x0c0abe10 at 0x2b7f38, 0x2b9c40, 0x2b9f98, 0x2c3b00, 0x2c6054), no data word [receipt:c0bd3706] [receipt:e3b85d33]
  (both absence verifiers: SUPPORTED):
  1. **A human-controlled threat, any level.** `FUN_002b9d10` (B2:241146-241264), the add / refresh of an entry in a
     unit's 6-slot threat list (slot = entity, its generation, the kind word, a time ...), ends on every path that
     stores the entry with `FUN_002b0418`(the threat's controllable) -> `FUN_002af840` (B2:241257-241260);
     `FUN_002b0418` (B2:234496-234515) returns the controllable's controller only when it IsA `DAT_006980b8`
     (PlayerController). It returns before the test when the entity is listed with a higher kind than a kind-0 add,
     when a kind-1 add does not find the entity listed, or when no slot can be freed. Its callers: the perception scan and the
     sight-ray results (`FUN_002e3aa0`, `FUN_002e43c8`, kinds 1 / 3), the noise pass `FUN_002de708` (kind 2 within 3 m,
     kind 0 beyond), `FUN_002e50b8`, `FUN_002c7ae8` (kind 4), and `FUN_002b9bb0` - the soldier's damage path
     `FUN_00209220` (B2:120224) and six agent sites (kinds 0 / 3; it also calls `FUN_002af840` itself on one branch,
     B2:241088-241093). [SUPPORTED]
  2. **The human's noise every 0.5 s.** `FUN_002af788` (B2:233993-234017), every 15th step, runs the noise pass for each
     PlayerController (`FUN_002b02f8` = `FUN_002de708`): every enemy AI unit (IsA `DAT_00698110`) within the human's
     noise radius (8-30 m, 30 m with the trigger down since the last pass: `re-soldier-ai.md` 15.1) and not dead ahead
     of him beyond 3.5 m (a dot of 0.87 or more with his facing needs 3.5 m) gets him added (B2:266818-266851) ->
     rule 1 -> it thinks at the next step. [PARTLY: the three gates were added by the verifier]
  3. **An order, near the human.** `FUN_002b7e58` (B2:239703-239740), the message entry: when the agent's message
     handler (+0x58) leaves an order in the message (its first word non-zero), the order is pushed and `FUN_002af840`
     runs only when ai+0x560 > 2 (B2:239733-239735). [PARTLY: the gate is the message's first word, not +0x58's
     return, which goes to the empty `FUN_002f7cb8`]
  4. **A finished helper, near the human.** The base message handler `FUN_002c3730` (data word at 0x622888, 0x622aa8,
     0x6238b0 = UnitWaitAgent's +0x58) takes type 0xb with `FUN_002af840` only when the level > 2
     (B2:248483-248488); type 0xb is sent by `FUN_002c00e8`, `FUN_002c08e8`, `FUN_002c0f50`, the first right after a
     helper vtable call with 100 (B2:245820; 100 = finished, `re-soldier-ai.md` 1.4 [H here]). [SUPPORTED]
  5. **CTF only.** `FUN_002c5e38` (data word at 0x622d00 = UnitCTFOffenseAgent 0x622ca8 + 0x58), case 0xe, for agent
     kinds 3, 4, 6, 7, with no level test (B2:250305-250323). [SUPPORTED]
- [L] **Live, BF2 Kamino** (`ps2/think_live.py`, every game update: each member's ai+0x2c4, +0x2c8, +0x560, +0x568, the
  threat list ai+0x2e0..+0x414, the agent; the human spawned, health pinned, walking; `ps2/bf2_kam1_a_think.pkz`, 4,496
  updates, 258 torn polls dropped). Scored by `ps2/thinknow_live.py` over the 57 s the human was alive (`TMAX=109.4`):
  of 52,049 update pairs of one bot with no think between them, the next think was pulled in 35, **each to now +
  0.0001**. All 28 pairs in which the human entered or changed in the bot's threat list pulled (10 new + 11 changed at
  levels 0-2, 7 changed at 3-4); **none of the 209 pairs in which only an AI entity entered did** (201 at levels 0-2);
  at levels 0-2 nothing else pulled a think (0 of 49,751 pairs); at levels 3-4, 7 pulls came with no new entry
  (rules 3-4). After the human lost his soldier (109.4 s on) his entity still entered threat lists (2 new, 7 changed)
  and pulled nothing - his entity had no PlayerController any more [H: what clears it at death was not read].
- [D] So a level-0 bot that the human walks up to is woken within 0.5 s by his noise, one that sees him or is hit by
  him at the next step; bots fighting bots learn of a new enemy at their next scheduled think (a trooper at level 0:
  up to 8 s) - the PS2's "walks through" (card MYGC).

## 5. BF2: what runs every step [O]
- [V] `FUN_002b7790` (B2:239266-239311): a byte ai+0x2d0 counts down each step and dt is summed into ai+0x2d4; when
  the byte goes negative it is reset to 4 (levels 0-1), 2 (level 2) or 0 (3-4) (B2:239283-239292) and the aim tracker
  `FUN_002ae010`, the visibility factor `FUN_002aec38` (ai+0x7c) and the mover's full update `FUN_002bc1a8` run with
  the summed time; on the other steps only `FUN_002bc210` (B2:239308): **every 5th step at levels 0-1, every 3rd at 2,
  every step at 3-4.** [SUPPORTED; [L] CLSE2 audit, `re-soldier-ai.md` 21.1: 59 % of a bot's tracker updates 0.1 s
  apart, 31 % 0.167 s, 9 % every update]
- [V] The light step `FUN_002bc210` (B2:242808-242818) runs the stand request `FUN_002ba810` or, on the path helper at
  *(mv+0x1c), `FUN_002bd888` -> `FUN_002f1d08` -> the path step `FUN_002f1d80` (or the stand `FUN_002f1d60`): path
  following runs every step. The full step adds the look `FUN_002bcbe0` and `FUN_002f7ce0`, and its path call
  `FUN_002bd628` returns before its detours (pickups, hints, every 2-3 s) when the level is under 3 (B2:243882).
  [PARTLY: the verifier placed the gate on the helper *(mv+0x1c), not on mv; fixed here]
- [V] (SAIR 1.5, not re-verified here) the soldier's AI input `FUN_00249f80` is skipped every other step, the time
  carried over, when `FUN_002a81c0` returns 0: slower than 10 m/s, over 45 m from the first camera, outside a camera's
  view cut at 70 + zoom x 8 m, and no network game.

## 6. A life's first think [O][L]
- [V] The UnitController's ctor `FUN_002b6c20` (B2:238768-238848) sets the level to 3 (B2:238841) and next = now
  (B2:238846), and `FUN_002aef80` files it with key 0 (1): a new bot thinks at the next step. Its level stamp ai+0x568
  is not set there [H: zero, so the first think recomputes the level]. Only three functions write ai+0x2c4: this ctor,
  the think and `FUN_002af840`; the decomp's other writes of +0x2c4 (`FUN_0022b028`, `FUN_0022b2f8`, `FUN_003304e0`,
  `FUN_00330600`, `FUN_0035cb68`, `FUN_004f2da0`, `FUN_004f36f8`) are entity, class and network objects
  [receipt:ab986525] (both absence verifiers: SUPPORTED; their ELF store scans found four more stores, all stack, HUD
  or network).
- [L] (`bot-quality.md`, card CONT's captures) a life's first order came **4.00 s** after spawning (median, 161 lives)
  with the player parked, 1.07 s near / 2.04 s far with him spawned. [D] That is one interval at factor 1.0 (no agent)
  at the level of the spawn think: the first think finds no order yet (the goal's 1 s tick pushes it after the spawn)
  and the order is popped at the second.

## 7. BF1 [O][L]
- [V] `FUN_0025d988` (B1:179237-179329): the round counter `DAT_0051814c` runs 1..15 and a toggle `DAT_00518140` flips
  every step (B1:179253-179256); each step visits count/15 not-yet-visited UnitControllers (+1 on the last count%15
  steps; all that remain at step 15; B1:179276-179279) and marks them visited, so **each is visited once per 15-step
  round, every 0.5 s**; a visit runs `FUN_0025d5f0` (the controller's update when the unit's +0xac bit 3 is set),
  `FUN_00243038`, `FUN_00242740`, `FUN_0025de50` and `FUN_0025d560` unless `FUN_0025d670` says skip. [SUPPORTED]
- [V] `FUN_0025d670` (B1:179129-179157) skips (ctrl+0x318 = 1) only when the last visit was not skipped, the unit's
  parity bit (+0xb4 & 1) matches this step's toggle and `FUN_00236508` returns 0; `FUN_00236508` (B1:150492-150549)
  returns 0 only offline, slower than 10 m/s (B1:150525), over 45 m from the first camera's position
  (*(`DAT_00517420`+0x2c)+0x30; B1:150541) and outside its view (`FUN_0019fc10`, B1:150544). [SUPPORTED] [D] A round is
  15 steps, an odd number, so the toggle at a unit's visiting step alternates and the test is asked every other round:
  **far, slow, unseen units think every 1.0 s**. [L] SAIR 1.6: 43.6-49.7 % of 60,879 such readings skipped, 0.2 % within
  45 m.
- [V] `FUN_00236508`'s second caller is the soldier's own update `FUN_0020aad0` (B1:119747), which then skips the
  soldier's WHOLE update for that frame (dt carried in +0xc08, never two frames in a row, its own parity
  `DAT_00517404`) - any soldier, not only AI. [PARTLY: the claim said "its AI input"; fixed]
- [V] **No twin of BF2's graded rule or of think-now**: BF1 has no next-think field, no per-agent factor and no level
  by the human; its only distance / view term is the camera test above, a 2:1 skip relative to the camera, not to the
  human; the visit's five calls each have one caller (this loop) and nothing re-schedules a visit sooner. [PARTLY x 2:
  both twin verifiers rejected the first wording "no level of detail by distance / view" - the camera test is one]

## 8. The port today, and the rule a build would add [D]
Today (main 672d943): `core/src/ai.rs:297` `THINK_PERIOD = 10` - every bot on foot, both games, re-perceives, re-picks
its target, re-plans (`route::retarget`), claims vehicles, runs the level test and its position's upkeep every 10 ticks
(`ai/behave.rs:102-108`, phase by unit index; the first think at once); the chain that walks and aims runs every tick;
a bot's aim-error block every 5th tick (`unit.rs:696`, CLSE2's "one level"). So with nobody playing the port's
bots think 24x as often as BF2's on a capture walk (0.33 s against 8 s), 12x on anything else, 4.8x in a fight, and
1.5-3x as often as BF1's.
A deterministic port rule from sections 1-7 - every input is sim state:
1. **BF2 ground rules:** per AI unit a `level` (0-4) and the tick it was taken; at a think, when 15 ticks have passed,
   recompute it from `Sim::players` (each human's unit, or his vehicle when he drives): 3; d < 7 m 4, d > 20 m 2; -1
   outside the bot's view cone (10 m / 77.5 deg about its aim); max over humans; then the raise by each human's
   `Unit::ideal_camera` (the sim's own camera, no render state): a sphere of 3 x the bot's size term inside the view
   cut at 20 / 7 m + zoom x 8 m -> 3 / 4. No human (headless, the gates): 0. The render term: always drawn (no FPV).
2. Next think = now + ceil(30 x factor x {4, 3, 2, 1, 0.25}); factor by what the bot does: walking to / at its post
   2.0 (the trooper, a sniper's spot included), fighting 0.4, anything else 1.0 (boarding, a seat, recovering, a squad
   order). At most 10 thinks a tick, by (next think, unit index).
3. **Think now** (next = the next tick): a hit from a human, and every 15 ticks every enemy bot within the human's
   noise radius (the port's hearing rule, `perception.rs`, 15.1's radius); at levels 3-4 also a new order (a squad
   order, a vehicle offer) and a finished walk. (The PS2's sight-ray answer for a human arrives after the think that
   asked for it and wakes the bot one step later; the port's scan answers inside the think and needs no wake.)
4. The first think of a life at once; [L] the PS2's bot then waits one interval (factor 1.0) for its first order - a
   faithful port bot stands 4 s after spawning with nobody playing. Decide with the sweep's referee (card MYGC).
5. The aim block's cadence by level (5 / 5 / 3 / 1 / 1 ticks) - the tracker rule of section 5.
6. **BF1 ground rules:** every 15 ticks; every 30 ticks for a unit slower than 10 m/s, over 45 m from the first human's
   camera and outside its view (headless: no camera, so every slow unit - [D] the port's choice); no think now.
7. Not in this build: the every-other-step skip of a far unseen soldier's input (BF2) or whole update (BF1) - a
   camera-relative saving that changes the physics' step; a later card if the cost profile asks for it.
What it moves: every BF2 sweep round runs at level 0 (MYGC's crude knock-in: bf2_myg1 3-13 -> 4-12, with OPNW 9-7; all
32 maps reshuffle, rounds ~35 % longer, `lead_tmp/mygc/allmaps.txt`); the sim hashes, the sweep and the fidelity gate
(an approval is the planner's / the maintainer's). It reverses `bot-design.md` D1 ("one rate for every bot"): a splitscreen
battle is covered by the max over humans, and the battle away from the player is the PS2's battle, not a sharper one.

## 9. Checks, the verifier, what stayed open
Checks run (output in `lead_tmp/aithk/checks.txt`): `re_check.py fn b2 234126 239206 239545 239590 239647 238841 238846
234035 63791 239285 239308 241263 239734` and `fn b1 179256 179283 179138 179141`; `callers b2 FUN_002af840` (5),
`FUN_002b6c20` (3), `FUN_002b7530` (1), `FUN_002b9d10` (6), `FUN_002c3730` (19), `FUN_002c5e38` (0); `callers b1
FUN_0025d670` (1), `FUN_00236508` (2); `word b2 0x621bc8 5 --f32`, `0x5fc4f4`, `0x5fe084`, `0x2cba38 4`, `0x2ce8a0 4`,
`0x623668`, `0x622b90`, `0x6238a8`, `0x2f7c88 2`; `writers b1 DAT_00518be4 / DAT_00518be8 / DAT_005e9a70 /
DAT_00517cf0`; `absent b2 0x0c0abe10 --elf` [receipt:c0bd3706], `absent b2 0x002af840 0x0c0add4c 0x002b7530 --elf`
[receipt:e3b85d33], `absent b2 "+ 0x2c4) =" "[0xb1] =" "+ 0xb1) ="` [receipt:ab986525].
Verifier: workflow `aithk-re-verify`, 6 agents at medium effort, claims and citations only: four batches (19 claims)
and the four absence / twin claims to two verifiers each, separately. **23 claims: 18 SUPPORTED, 5 PARTLY (C9, C11,
C15, C19 and the BF1 twin claim T1; every correction is folded in above), 0 NOT SUPPORTED; the second verifier
overturned none** (A1, A2, A3 both SUPPORTED, T1 both PARTLY on the same wording). Verdicts:
`lead_tmp/aithk/verify/verdicts.json`.
Stayed [H]: the render mark's meaning in play; ai+0x568 zero at construction; what clears a dead human's controller;
"state 100 = finished" for the type-0xb senders; the port's BF1 headless choice (no camera = far); the factor of the
Follow / CTF / Defend agents by state (SAIR's reading, not re-run here).
