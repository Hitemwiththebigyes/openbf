# OPUS RE (2026-09-30) - [O] UNAUDITED

# RE: squad orders (BF1 + BF2) — the player's line, who answers, what each order makes them do

Card SQACK (loop lane 2, max effort). Corpora: `analysis/SWBFRONT.ELF-decomp.c` (B1), `analysis/SWBFRNT2.ELF-decomp.c`
(B2), both ELFs (functions Ghidra skipped were read as disassembly), the soldier ODFs and the sound LVLs, live PS2 RAM
(PCSX2, BF1 NAB1 Clone Wars, the player a CIS super battle droid: 11 scripted scenes). Tags: [V] read (lines cited,
each found INSIDE the named function by `lead_tmp/sqack/cite.py`), [L] measured live on the PS2, [D] derived, [H]
hypothesis; `[O]` marks the load-bearing claims. Scratch, tools, logs: `engine/artifacts/lead_tmp/sqack/`.

## 0. Headline

BF1 has four orders on the D-pad. The player's soldier says a line at once, a small D-pad cross with the four order
names fades in the HUD for 2 s, and EVERY friendly AI unit inside the order's radius takes the order at its next think
(0-0.5 s later) and says "yes". There is no squad list, no per-unit marker, no unit cap except on FOLLOW ME.

| D-pad (on foot) | HUD label | type | the player's line | reaches (3-D) | what a unit that takes it does | ends when |
|---|---|---|---|---|---|---|
| Up | MOVE OUT | 0 | `<s>_inf_com_chatter_tac_moveout` | 45 m | a holding / following unit whose order came from this player is released; nothing else | — |
| Left | AT EASE | 1 | the same line | 45 m | exactly the same as MOVE OUT | — |
| Down | HOLD POSITION | 2 | `<s>_inf_com_chatter_tac_hold` | 30 m | stands where it is, looks around, fights from there (`UnitHoldAgent`) | the player is > 20 m away for 20 thinks (10 s); MOVE OUT / AT EASE; the player's death |
| Right | FOLLOW ME | 3 | `<s>_inf_com_chatter_tac_follow` | 30 m, on foot only | the first TWO units to think follow him (`UnitFollowAgent`): trail him, then stand within 3 m; board his vehicle | the player is > 20 m away for 20 thinks; MOVE OUT / AT EASE; HOLD; the player's death |

The acknowledgement is `ai_<s>_inf_com_chatter_tac_yes`, a 3-D line at the unit, on the battle's voice stream, queued
behind whatever is playing. Every unit that takes the order asks for it (also the third and later units of a FOLLOW ME,
which then do NOT follow), but each request draws one of three takes at random and a take drawn in the last 1 s (2 s for
Rebels and Empire) is silent — so one order is answered by 1 to 3 lines, about one second apart [V][L].

BF2 does not have this. Its ELF still holds BF1's line + HUD function, unreferenced; its live order is one contextual
button: aim at a friendly AI soldier and press — he follows (or stops); hold the button 1 s — all followers stop (§7).

Corrections to landed RE, §8: RE_LOG's squad entry names BF1's agents one off (its "UnitHoldAgent" is
`UnitRandomAgent`, its "UnitFollowAgent" is `UnitHoldAgent`, its "fly agent" is the real `UnitFollowAgent`), and the
"1-in-9" chance is 1 in 18. `core/src/squad.rs`'s BF1 notes inherit both.

## 1. From the D-pad to the order [O]

- **Input** [V]. The player controller `FUN_0025e098` keeps a press counter per input bit in the Controllable: a byte
  becomes 1 on the first frame its bit is set (B1:180208-180240 for +0x58; bits 10, 11, 9, 8 -> +0x58, +0x5a, +0x5c,
  +0x5e, 180208 / 180263 / 180318 / 180373). The soldier update `FUN_00205cc8` issues when a byte equals 1 (soldier
  +0x2f8 -> type 0, +0x2fa -> 2, +0x2fc -> 3 with his own entity in the order, +0x2fe -> 1; B1:117915-117973).
  [L] Right = FOLLOW, Down = HOLD, Up = MOVE OUT, Left = AT EASE; the order exists one frame (33 ms) after the press.
  Vehicle updates map the same bytes to types 0, 2, 4, 5 (`FUN_001d9180` B1:85518-85543, and five more classes:
  `refs.py b1 0x260f38` lists 18 call sites in 6 updates + the AI's entry). A droideka's update has no such block.
- **The order** = {issuer's character, type, time, entity, entity id} (5 words).
- **Issuer `FUN_00260f38`** [V]:
  1. nothing after the battle is over (`FUN_0033e580` = `DAT_00519084`, B1:181662) or without a unit;
  2. type 4 only from a SEATED issuer (his unit's controller is null: B1:181668-181683; RAM: every seated unit of 45);
  3. position kind: 0 on foot, 1 seated, 3 seated in a `MountedTurret` (181688-181700); kind 2 ("passenger") is never
     produced — its four labels and eight sound handles are unreachable;
  4. **the line and the HUD (§2) — before any receiver is looked for** (181702): an order into empty air still speaks
     ([L] scene N0: issued 60 m up, no receiver, line + HUD as usual);
  5. an AI issuer ON FOOT delivers only types 4 and 5 (his controller is not a `PlayerController`, 181708-181721): the
     AI's "follow" and "move out" chatter orders nobody (§5);
  6. on foot, or type 4: every entry of the character table (`DAT_00518388` +0x38 / +0x3c, stride 0xe8) that is not the
     issuer, not a local human (+0x9c < 0), friendly (team relation > 0, `FUN_00221890`), of a class with +0x7c == 0
     ([D] the hero flag: such classes are also left out of the team's unit count, B1:221522), and inside the radius —
     **3-D distance, 30 m, 45 m for types 0 and 1, x3 for a receiver in a vehicle** (181724-181793; `lui 0x41f0`
     @0x2610d4, `0x4234` @0x2610f0, `0x4040` @0x261228). Type 3 skips receivers in vehicles; type 4 takes only units on
     foot that are not droidekas; an on-foot issuer never reaches a unit riding a flyer. **No cap, no nearest-first, no
     line of sight.** Delivery is one store: controller +0x254 = 1, +0x258.. = the order (`FUN_002631e0`);
  7. seated and not type 4: the order goes to the seats of his own vehicle that are not player-controlled (181834-181851).

## 2. The player's line and the HUD [O]

`FUN_00260be0(order, kind)`:

- **Which sound** [V]: the issuer's CLASS carries a table of 28 handles at class +0x172c (parser `FUN_00228f68`
  B1:141091-141225; the ODF property names, cracked here by hash): 13 for a human issuer at +0x68.., 13 for an AI issuer
  at +0.., then the two answers.

  | slot | ODF property (AI twin: `AI` prefix) | kind / type | retail value (`<s>` = all, imp, rep, cis) |
  |---|---|---|---|
  | +0x68 | `SCFieldMoveOutSound` | field, 0 and 1 | `<s>_inf_com_chatter_tac_moveout` |
  | +0x70 | `SCFieldHoldSound` | field, 2 | `..._hold` |
  | +0x78 | `SCFieldFollowSound` | field, 3 | `..._follow` |
  | +0x80 / +0x88 | `SCDriverGetInSound` / `GetOutSound` | driver, 4 / 5 | `..._needgunner` / `..._moveout` |
  | +0x90..+0xa8 | `SCPassenger{MoveOut,Stop,GetIn,GetOut}Sound` | unreachable | `spreadout`, `stop`, `needgunner`, `moveout` |
  | +0xb0 / +0xb8 | `SCGunnerAllClearSound` / `SteadySound` | gunner, 0-1 / 2 | `..._allclear` / `..._stop` |
  | +0xc0 / +0xc8 | `SCGunnerGetInSound` / `GetOutSound` | gunner, 4 / 5 | `..._needgunner` / `..._moveout` |
  | +0xd0 | `AISCResponseYesSirSound` | the acknowledgement | `ai_<s>_inf_com_chatter_tac_yes` |
  | +0xd8 | `AISCResponseNoSirSound` | never played (A1) | `ai_<s>_inf_com_chatter_tac_no` |

  `SCResponseYesSirSound` / `NoSirSound` (hashes 0x765be0f2 / 0xc989ade6) are in the ODFs and read by nothing. All 36
  soldier classes of the four armies carry the same 30 properties with their side's names (census of
  `assets_baked/classes/bf1_*.ron`); exceptions: the two smuggler classes use `wok_...` in both sets, the droideka
  uses the non-`ai_` names in its AI set (its "yes" then plays on the tac stream).
- **How it plays** [V]: a human issuer's property has its 3-D bit cleared and is played with the queue flag 0 — at
  once, cutting off whatever the slot plays (ANNC §1: BF1 restarts the slot); an AI issuer's line is 3-D and queued
  (B1:181620-181626). The player set is on stream `cw_tac_vo` / `gcw_tac_vo` — a slot of its own, so the order never
  cuts the commander or the chatter off; the AI set and the answers are on `cw_vo` / `gcw_vo` with everything else.
  Data (CORE.LVL templates): player line Gain 0.6, bus soundfx; 2-3 takes of 0.5-1.4 s each (rep / cis / all / imp
  tables: `annc/linetab.py`). [L] the line starts one frame after the press in all 11 scenes.
- **The same take twice** [V][L]: every take of every command line is `segment(id, 1, 1.0, 0)`: a take cannot be
  drawn again for 1.0 s, and the draw is made before that test (`FUN_0046b290` -> `FUN_0046bf10`), so the same order
  given twice inside a second is silent with chance 1/takes.
- **HUD** [V][L]: for a local human issuer with a valid kind / type pair, `FUN_002d8698(hud + 0x2160, kind, arm)`:
  the element `SquadCommandGroup` (texture `squad_cmd_cross` 64x64 at (-120, -50) from the screen's bottom right, four
  `gamefont_tiny` labels at +-25, a `squad_cmd_highlight` quad per arm) shows the kind's four labels
  `game.squadcommand.<field|driver|passenger|gunner>.<up|down|right|left>`, the chosen arm at alpha 255 with its
  highlight, the others at 127; the element's alpha is timer x 255, timer 1.0 falling 0.5 per second — **a 2.0 s fade**
  (B1:264191-264210, 264166-264172; arm table `DAT_00518168` = {0, 3, 1, 2, 2, 3}; [L] visible 1.97 s in every scene).
  English: field MOVE OUT / HOLD POSITION / FOLLOW ME / AT EASE (up / down / right / left); driver — / — / PILE IN /
  EVERYONE OUT; gunner ALL CLEAR / HOLD HER STEADY / HOP IN / BAIL OUT. Frames: `lead_tmp/sqack/scenes/hud_sheet.png`.
- **No marker in the world** [V]: the two textures are named by one function, the HUD element's setup `FUN_002d84a0`;
  "squad" occurs in four strings of the ELF (the label format, the two textures, the element's class name). No unit
  count is shown either. No animation is started by the issuer (the function touches sound and HUD only).
- A seated human pressing up / down as DRIVER (types 0 / 2) has no label, no line and no HUD, but the order still goes
  to the AI in his vehicle [V, verifier].

## 2b. The cross on screen (card SQHUD, build, 2026-10-03) [V][M][L]

Read for the build; not run past a verifier (the element's draw path is not traced - see the [H] below).

- **Constructor `FUN_002d81c8`** (B1:264004-264069) [V]: the cross bitmap `FUN_004b5e28(64, 64, +0x90, 1, 1)` - a
  64 x 64 rect centred both ways (`FUN_004b5e28` B1:576018: mode 0 = 0..size, 1 = centred, 2 = -size..0); per arm a
  text at `DAT_00518848[i]` = (x, y, h, v) and a highlight bitmap `FUN_004b5e28(32, 32, ..., 1, 2)` - centred across,
  hung by its bottom edge. The group's position is `DAT_0051883c / 40` = (-120, -50), its timer +0x120 = 1.0.
- **Tables** (`re_check.py word b1`): `0x518848` = (0, -25, 2, 1), (0, 25, 2, 0), (25, 0, 0, 2), (-25, 0, 1, 2) -
  up, down, right, left; the picture gives h 0 = left edge, 1 = right edge, 2 = centred and v 0 = top, 1 = bottom,
  2 = centred. `0x518888` = (0, -16, pi), (0, 16, 0), (16, 0, -pi/2), (-16, 0, pi/2) - the highlight's place and turn.
  `0x518844` = 0.5 (the fade a second). Strings: 0x540a68 `squad_cmd_cross`, 0x540a78 `gamefont_tiny`, 0x540a88
  `squad_cmd_highlight`.
- **Setup `FUN_002d84a0`** (B1:264101-264130) [V]: the cross's matrix is scale 0.5 at (0, 0) (`FUN_004b7df8`), each
  highlight's is the table's turn at scale 0.5 at the table's place (`FUN_004b7e48`: x' = c x - s y, y' = s x + c y).
  So the cross is 32 x 32 on the origin and a highlight is 16 x 16 with its hung edge 16 from the origin and its
  far edge (the sheet's point) on the origin.
- **Anchor** [V][M]: `FUN_002d85d0` -> `FUN_00479f00(DAT_005b8ca8, DAT_005b8cac, viewport, element, 1)` stores the
  fractions (1.0, 1.0) (written at B1:264690) at element +0x80 / +0x84 with flag 1; the scene's draw `FUN_004b8e78`
  (B1:578350-578356) turns a flagged element's origin into rect.min + fraction x rect.size of the scene's rect.
  RAM (`lead_tmp/sqack/nab1_cis_inlevel.p2s`, scene `*(0x6776a0)`): screen 512 x 448, rect (0.05, 0.05, 0.9, 0.9).
- **Measured** [L] (`lead_tmp/sqack/scenes/{F3,H3,A3}/hud.png`, the 824 x 618 picture of the 512 x 448 frame): the
  cross's opaque texels x 561-610, y 500-539 in all three: 49 wide (32 units) and 39 tall - a unit down the screen
  is 448 / 480 of a frame row (the highlights agree: 15 rows across the right arm, 20.6 along the down arm). Centre
  (363.8, 376.6) frame px; the rule above gives (366.4, 378.9) - 2.6 / 2.3 px off, the scene's projection is not
  traced [H]. (Erratum, cleanse 17 2026-10-16: `re-capture-display.md` §4 traced it for x - the plain scene matrix,
  1.0221 frame px per unit, gives 0.95 x 512 - 120 x 1.0221 = 363.75; y is still [H].) Chosen labels: FOLLOW ME x 628-771, y 516-532; HOLD POSITION x 490-675, y 563-579; AT EASE x 438-537,
  y 516-532; MOVE OUT (dim) x 519-641, y 469-485. The widths are BF1's own `gamefont_tiny` (CORE.LVL: line 18,
  capitals 7 x 13 in rows 5-18) with 1 px between glyphs, and each label has a dark copy one pixel down and right
  (the Shadow style). The rows fit a line box 23.2 units tall with the font's rows 3.8 lower [L] - BF1's text
  element is not read, PROVISIONAL.
- **Port**: `ui/src/hud_squad.rs`; `game/tests/hud_layout.rs` holds every edge above within 2 PS2 pixels (the centre
  and the line box are fitted to these frames; the sizes, arm offsets, label anchors and text widths are not). The
  count of units that took the order beside the chosen label is the port's own (the maintainer, INBOX 2026-09-30).

## 3. Who takes the order, who answers, when [O]

- **Think cadence** [V]: the AI manager `FUN_0025d988` gives each `UnitController` one full update per 15-frame cycle
  (counter `DAT_0051814c` 1..15, count/15 controllers per frame, B1:179253-179296; `slti ..,16` @0x25da38) = every
  0.5 s at the PS2's 30 fps; a slow unit more than 45 m from the view and out of sight thinks every other cycle
  (`FUN_0025d670`). Agents' update functions run only in that full update (`DAT_00588290 == 0`).
- **Consuming** [V]: the order is read by `FUN_00253118`, the 5th of the 8 interrupt checks at the top of an agent's
  update (`FUN_00253cd0`); the pending flag is cleared at the end of every full update (`FUN_00262f30`, `sw zero,
  0x254` @0x263000). So a unit takes the order at its next think, 0-0.5 s after the press — or never, if one of the
  four earlier checks fires in that think, or its agent does not ask (a unit that is dying; a `UnitCombatAgent` in
  one sub-state; a droid-repair agent). [L] 31 receivers in 8 scenes: consumed after 0.03-0.47 s (median 0.27 s);
  one unit that died at the press never consumed it.
- **The agent's veto** [V]: virtual +100 is 1 for every agent class but `UnitFollowAgent`, which refuses a second
  FOLLOW ME while it follows (no line, nothing recorded).
- **The answer** [V]: after the veto and BEFORE the per-type switch, the unit plays its class's +0xd0 handle at its own
  position with the queue flag (B1:171544) — for every type, whatever the switch then decides.
- **What is heard** [V][L]: the request goes through the voice queue of ANNC §1 (one line at a time per slot, at most
  4 pending, a commander line ahead of chatter). Three gates thin the answers out:
  1. the take: 3 takes, drawn at random, each locked for its interval after a draw — 1.0 s for `ai_rep` / `ai_cis`,
     **2.0 s for `ai_all` / `ai_imp`**, none for the single-take `wok` line (`segs.py`). With n units answering inside
     half a second, the first always speaks, the second with chance 2/3, a third only if it draws the last free take;
  2. the 3-D level (MinDistance 7 m, RollOff 3, mute 150 m): a far unit's answer has a lower key, so it queues behind
     nearer ones and is dropped first;
  3. the queue itself (4 pending).
  [L] 12 orders: 3 receivers -> 2, 3, 2 answers; 4 -> 3, 3; 5 -> 3, 3, 3. First answer 0.07-0.30 s after the press
  when the voice slot was idle, else when the playing line ended (up to 1.7 s); the next answers follow one line
  length apart (1.0-1.6 s).
- The order is recorded as the unit's last accepted order (controller +0x26c) for every type and outcome.

## 4. What each order does [O]

`FUN_00253118`'s switch (B1:171549-171688). A new goal is pushed with `FUN_00252140`: it replaces the unit's current
goal when the new goal is itself an order (goal 9 / 10) or the current agent is Board / Combat / Random / UnBoard /
WaitSecondary; otherwise the current goal is kept queued behind it and resumes afterwards (so a released trooper goes
back to what it was doing). Only `UnitMobileController` units (soldiers, hovers, walkers) act on goals 9 / 10:
turret gunners ignore them and flyer pilots take only HOLD — but they still answer.

### 4.1 MOVE OUT and AT EASE (types 0, 1) [V][L]
One case. A waiting-in-vehicle or parked-flyer agent is poked; a unit whose current goal is an order (9 or 10) AND
whose last accepted order came from this issuer has its agent ended (state 100). No new goal. Nothing in the binary
tells the two types apart except the HUD arm. [L] scenes M3 / A3: followers / holders back in their trooper agent
0.03-0.2 s after the press, 20-60 m away 10 s later; every receiver within 45 m answered, ordered before or not.

### 4.2 HOLD POSITION (type 2) [V][L]
Goal 10 -> `UnitMobileController` state 0xb -> `UnitHoldAgent` (vtable 0x536b68; update `FUN_00248828`). No cap.
- The agent never moves the unit: it only gives the GotoHelper a LOOK-AT — the enemy it has targeted, else a point 10 m
  away in a random direction (own position + (rand-0.5) x 20 in x and z), re-rolled with chance 1 in 18 per think
  (`FUN_002325d0(1, 9)` doubles its bound). (A unit on a building turret looks along its spawn facing +-30 deg.)
  It keeps targeting and shooting from the spot.
- Released (state 100) when the issuer of its last order has no unit (dead), or after 20 thinks with him farther
  than 20 m (3-D; a 1.0 timer less 0.05 per think, never refilled) — 10 s, 20 s for an LOD-skipped unit.
- Refused for a unit riding a flyer in flight. A unit in a ground vehicle holds too (the vehicle stops).
- [L] H3, H5, run1b: 9 holders, max displacement 0.1-0.3 m over 10-14 s; released 9.5, 9.5, 9.8, 13.1 s after the
  player passed 20 m; one that was already 22 m away at the order: 12.3 s after the order.

### 4.3 FOLLOW ME (type 3) [V][L]
- Taken only when the order's entity (the issuer's unit at the press) still exists, has fewer than 2 followers and the
  receiver has none itself (`FUN_00261520(e)` = 1 + the units whose mover follows e; refuse at > 2, B1:171641-171657;
  `slti ..,3` @0x253518). **Two followers per leader — the first two receivers to think**, not the nearest. The rest
  answer "yes" and carry on. (A follower also quits if it ever counts 3 or more, B1:163080-163085.)
- Goal 9 -> state 10 -> `UnitFollowAgent` (vtable 0x536a58; `FUN_002474e0` / `FUN_00247b18`):
  state 2 walk to where the leader stood (the mover's follow entity is set) until within 0.45 m -> state 3;
  state 3 follow; nearer than a + 5 m -> state 4; mover stuck -> state 2;
  state 4 go to a point (rand-0.5) x 2b in x and z of the LEADER's frame (at least 1 m from him) and stand; leader
  farther than b + 6 m -> state 3. a = 0, b = 3.0 (`FUN_00260b30`; RAM 0.0 / 3.0; `SetAIFormationRadius` is called by
  no script on either disc).
- Released: leader gone; leader farther than 20 m for 20 thinks; any new order replaces it. **When the leader gets into
  a vehicle the follower gets a board goal for that vehicle** (droidekas are just released).
- [L] F3, F5, F5b, M3, run1: 2 followers in 6 of 6 orders with 3-5 receivers; a follower reached 1-2 m of a standing
  leader in 2 s, trailed a walking leader at 5-12 m, stood 1.3-3.8 m from him 10 s after the order.

### 4.4 Vehicle orders (types 4, 5) [V]
PILE IN / HOP IN (4, seated issuer): friendly AI on foot within 30 m, not droidekas, get goal 5 = board the order's
vehicle. EVERYONE OUT / BAIL OUT (5): the AI seats of the issuer's vehicle get goal 6 (leave) and may not board again
for 20 s (`FUN_00297da8(now + 20)`). A gunner's ALL CLEAR / HOLD HER STEADY are types 0 / 2 to the vehicle's seats
(release / hold, §4.1-4.2). Not measured live.

## 5. The AI's own squad talk [V]

`FUN_002613f8(p, order)` issues with probability p: a trooper whose first path leg fails says FOLLOW (p 0.3,
`FUN_0024b7a0`); a trooper agent leaving one branch says MOVE OUT (p 1.0, `FUN_0024baf0`); an AI waiting in a vehicle
seat says the type-4 line (p 0.3, `FUN_0024d020`; a type 3 from a seat becomes 4). The lines are the `ai_` twins:
3-D, queued, and limited by their property's inherited PlayInterval 20 +- 5 s — at most one per 15-25 s per line.
Only the seated issuers' orders are delivered (§1.5): AI on foot within 30 m board the caller's vehicle and answer
"yes" — the `needgunner` / `yes` pairs heard in a battle with no order from the player (card note, ANNC run 1).

Correction (card GUNWT, 2026-10-19, `re-driver-wait.md` 4 and 8): "first path leg fails" is wrong - the trooper says
FOLLOW when it sets out for a post it is not at (`FUN_00235258` = inside the goal's radius, card UTALK). A driving AI
runs the trooper agent too, so each set-out of a SEATED trooper is the type-4 "need gunner" call (0.3): the waiting
driver (`UnitWaitSecondaryAgent`, entered once per AI pilot entry, 4 s window) is one of two sources, and neither
tests the vehicle's seats.

## 6. PS2 live scenes [L]

`lead_tmp/sqack/sq_live.py` (pad script in game-clock seconds; samples every 8 ms the clock, six stream slots + queue
counts, the HUD element, and every character's position, controller state, goal, pending and last order, follow
entity); `scenes.sh` runs each scene from one quick-saved state (`nab1_cis_inlevel.p2s`); `summ.py` -> `scenes/SUMMARY.txt`.
Set-up knock-in: the N nearest on-foot AI teammates are written onto a ring 5-8 m round the player before the press.

| scene | order(s) | receivers (taken after) | became | answers (after the press) | 10 s later |
|---|---|---|---|---|---|
| F3 | FOLLOW, 3 near, walk 4 s | 3 (0.27, 0.30, 0.33 s) | 2 FOLLOW, 1 no change | 2 (0.30, 1.87 s) | followers 3.2, 3.8 m; third 20.8 m |
| F5 | FOLLOW, 5 near | 5 (0.03-0.47 s) | 2 FOLLOW (the two that thought first) | 3 (0.07, 1.10, 2.10 s) | 1.7, 2.9 m; others 20-64 m |
| F5b | FOLLOW, 5 at 8-10 m | 5 (0.17-0.43 s) | 2 FOLLOW | 3 (0.30, 1.30, 2.87 s) | 1.3, 2.8 m |
| H3 | HOLD, 3 near | 3 (0.27-0.33 s) | 3 HOLD | 3 (1.70, 3.14, 4.20 s) | where they stood (7.0, 9.7, 5.1 m) |
| H5 | HOLD, 5 near | 4 (0.10-0.33 s) + 1 dying | 4 HOLD | 3 (0.73, 1.74, 3.30 s) | where they stood |
| M3 | FOLLOW, then MOVE OUT at +3.7 s | 4 within 45 m (0.07-0.20 s) | 2 FOLLOW -> trooper | 3 (1.03, 2.57, 3.67 s) | 19-77 m away |
| A3 | HOLD, then AT EASE at +3.7 s | 4 (0.03-0.20 s) | 3 HOLD -> trooper | 3 (0.77, 1.60, 3.04 s) | 21-66 m away |
| N0 | three orders 60 m above the group | 0, 0, then 4 at 19-29 m (MOVE OUT, 45 m) | — | line + HUD each time | — |
| run1 / run1b | FOLLOW, walk, HOLD, walk away | §4.2 / §4.3 numbers | | | |

Sound clock != game clock (the sound clock runs through pauses); a slowed emulator (another lane's gates) stretches
wall time only — every time above is game-clock.

## 7. BF2 [V] (not run live)

- **BF1's system is dead code**: the line + HUD function survives at 0x2e2480-0x2e27e4 (same handle offsets, calls the
  play function and the HUD function `FUN_00391c90` at 0x2e27b4) with no reference in the ELF (`refs.py b2 0x2e2480`);
  the issuer is an empty `jr ra` (`FUN_002e27e8`). The localisation still has the 16 labels. BF2 soldier ODFs carry no
  `SC*Sound` properties; their command lines are `VOSound` events.
- **The live order** (`FUN_00240bf8`, B2:161199-161520), on the command button's press edge, with a friendly AI unit
  under the reticle (soldier +0x3f0):
  a soldier: query 0xf; not my follower -> if no free slot, one follower is released (0x12 to the first that answers),
  then 0x10 "follow", AI event 0x16, HUD message "Follow me"; already my follower -> 0x12, event 0x18, "Move out".
  A hero (class bit set by `SetHeroClass`) only fires the event. A hover or walker (player class byte +0x5a3): command
  4 "wait for pickup", events 0x1a / 0x1b, "Hold on".
- **Holding the button 1.0 s** releases every follower (0xf to all AI, then 0x12 to all; event 0x18; "Move out").
- **Cap**: `FUN_00249f30` — a human leads rank + 1 units: 1 below 20 medals, 2 below 100, 3 below 300, else 4
  (`FUN_00360120`); 2 in a session game; an AI leader 4.
- **Lines**: AI-event table 0x624ed8: 0x16 `SC_Follow`, 0x17 `SC_FollowResponse`, 0x18 `SC_StopFollow`, 0x19
  `SC_StopFollowResponse`, 0x1a / 0x1b `SC_VehicleWaitUp` (+`Response`), 0x1c / 0x1d `SC_GetIn`, 0x1f / 0x20
  `SC_GetOut` -> the ODF's `<side>_command_{follow,stopFollow,stopVehicle,getIn,getOut}` and `<side>_response_*` on the
  quick voice stream. The player's line at the press; **the unit's response 0.5-1.2 s later** (uniform; `FUN_002c2ef8`
  for follow, `FUN_002ca4f0` for stop).
- Agents: state 0xe = `UnitFollowAgent` (`FUN_002c9418`, vtable 0x623318, handler `FUN_002ca4f0`), state 0xf =
  `UnitHoldAgent` (`FUN_002caa40`) — RE_LOG has these one off too. BF2's follow motion (squad table `DAT_005fe6a8`,
  thresholds 4 / 10 / 20 m in `FUN_002c9bc0`) was not transcribed [H].

## 8. Corrections to landed RE

- `analysis/RE_LOG.md` "Squad commands, bounded" (2026-09-01): "9 -> UnitHoldAgent `FUN_00248cb8` (hold point = the
  unit's position; 10 m radius; wander after 3 s)" describes `UnitRandomAgent` (goal 8); "0xb -> UnitFollowAgent
  `FUN_00248498` / `FUN_00248828`" is `UnitHoldAgent`; the "fly agent `FUN_002474e0` / `FUN_00247b18`" is
  `UnitFollowAgent` (RTTI: `rtti.py b1 0x536a58 0x536b68 0x536c70`). The same shift in its BF2 half. "LOS and a count
  cap not read" -> no LOS; FOLLOW caps at 2; "1-in-9" -> 1 in 18.
- `analysis/re-sound-global.md` §3: "30 hashed ODF keys (names not cracked)" -> §2's table.
- `engine/crates/core/src/squad.rs` header: "the hold point is the unit's own position (BF1's rule)" is right by
  accident (a holder does not move at all); "BF1 wanders after about 3 s idle" is `UnitRandomAgent`, not an order.

## 9. What the port needs (the build half of SQACK)

PLAN plays BF1's orders on every map (the maintainer, INBOX 2026-09-30). Against `core/src/squad.rs` + `game/src/main/windowed.rs`:

| | port today | PS2 |
|---|---|---|
| orders | Follow, Hold, Disperse, Clear | FOLLOW ME, HOLD POSITION, MOVE OUT = AT EASE (release); no Disperse |
| reach | 20 m XZ, 6 nearest | 30 m (45 m release) 3-D, everyone; x3 for units in vehicles |
| follow cap | 6 | 2, the first two to think; the rest answer and carry on |
| timing | same tick | each unit at its next think (0-0.5 s) |
| follow | slot 4 m, re-roll 8 ticks, release 40 m / 20 s | walk to leader; < 5 m: a point within +-3 m in his frame; > 9 m: follow; release 20 m / 10 s; boards his vehicle |
| hold | hint search within 10 m, wander every 3 s, release 40 m / 20 s from the ANCHOR | stand still, look around (1/18 per think), release when the LEADER is > 20 m for 10 s |
| release | Clear: everything the slot ordered, any distance | MOVE OUT / AT EASE: units within 45 m whose order came from this player; leader's death |
| player line | `<s>_..._tac_{follow,hold,spreadout,allclear}`, only when somebody took it (Clear always) | `follow`, `hold`, `moveout` (both release orders); always; own stream; cuts its own previous line |
| answer | none | `ai_<s>_..._tac_yes` at each taker, queued, take lock 1 s (rep, cis) / 2 s (all, imp) |
| HUD | per-unit icons (SQMK removes them) | the D-pad cross with four labels, chosen one bright, 2 s fade; nothing in the world |

Acceptance, as a port-vs-PS2 check of the scenes of §6 (the PS2 side is `scenes/SUMMARY.txt`): (1) receivers = the AI
teammates within 30 m (45 m for the release orders) in 3-D; (2) FOLLOW: exactly 2 followers of 3 and of 5 receivers,
standing <= 4.3 m from a still leader 10 s later, >= 5 m while he walks; (3) HOLD: every receiver, displacement
<= 0.3 m in 10 s, released 10 +- 1 s after the leader passes 20 m; (4) release orders: the ordered units back in their
own goal within 0.5 s; (5) lines by name: the player's at the press tick, the first answer within 0.5 s on an idle
voice (else next in the queue), 2-3 answers for 3-5 takers, none sharing a take inside its lock; (6) no world marker.
PROVISIONAL in the port: which two units follow (the PS2's is its think-slice order: use the port's own think order),
the LOD-skip, the look-around while holding.

## 9b. What the port does after SQACK (build, 2026-10-03)

`core/src/squad.rs`, `core/src/ai/command.rs`, `game/src/announce.rs` (`Commander::orders`),
`game/src/event_sounds.rs`; the scenes of §6 on the port: `game/tests/squad_orders_ps2.rs`.

- D-pad: Up MOVE OUT, Left AT EASE, Down HOLD POSITION, Right FOLLOW ME. The press marks every AI teammate on foot
  within 30 m (45 m for the two release orders), 3-D, no cap; each takes it at its own next think and answers
  (`Event::OrderTaken`), whatever the order then does to it. A follower refuses a second FOLLOW ME with no answer.
- FOLLOW ME: the first two to think (the port's think order: unit index among those thinking on a tick; think period
  10 ticks, the PS2's is 15). States 2 / 3 / 4 as §4.3: walk to where the leader stood (0.45 m; the port also ends it
  1.5 m from the leader himself, two soldiers do not overlap), follow, under 5 m take a point of the +-3 m square in
  his frame (at least 1 m off) and stand, follow again past 9 m. Beyond 10 m the follower routes over the hub graph.
- HOLD POSITION: every taker; no movement at all. No look-around (the unit keeps its normal aim).
- Release: MOVE OUT / AT EASE for units this player ordered; the leader beyond 20 m (3-D) for 300 ticks of thinks,
  summed and never refilled; the player's death.
- Lines: the player's `<s>_inf_com_chatter_tac_moveout|hold|follow` at the press tick, whoever is in reach (by the
  side's name, not the class's `SCField*Sound`; it does not cut its own previous line). Each taker asks for
  `ai_<s>_inf_com_chatter_tac_yes` at its position on the battle's voice queue (key 0.5 x the 3-D level, behind a
  commander line); the request draws one of 3 takes and is silent when that take was drawn inside its lock (1 s rep /
  cis, 2 s all / imp, from §3 - the bake carries no segment interval).
- The order's text was one cyan line without a count until card SQHUD; now the press shows the D-pad cross (§2b)
  and prints no line.

Port scenes (flat ground, the player a CIS soldier, a ring of 5 m; takes of 1.0 s):

| scene | receivers | became | answers after the press | 10 s later | PS2 |
|---|---|---|---|---|---|
| F3 | 3 | 2 FOLLOW | 2 (0.00, 1.47 s) | +2 s 1.0 / 2.7 m; walking 11.3 / 13.4 m; +10 s 2.8 / 2.6 m | 2; 1.3 / 1.2; 11.7 / 11.9; 3.2 / 3.8 m |
| F5 | 5 | 2 FOLLOW | 3 (0.00, 1.47, 2.93 s) | 1.0 / 2.5 m | 2; 3 answers; 1.7 / 2.9 m |
| H3 | 3 (a 4th at 35 m is out) | 3 HOLD | 2 (0.00, 1.47 s) | 0.00 m moved; released 9.8 / 9.9 / 9.9 s after the leader passed 20 m | 3; 0.1-0.3 m; 9.5-9.8 s |
| M3 | 4 within 45 m | 2 FOLLOW -> free after 0.30 s | 2 (0.30, 1.77 s) | | 4; 0.07-0.10 s; 3 answers |
| A3 | 4 | 3 HOLD -> free after 0.30 s | 2 (0.30, 1.77 s) | | 4; 0.03-0.10 s; 3 answers |
| N0 | 0 (FOLLOW), 1 at 35 m (MOVE OUT) | | the player's line plays | | the same |

BF1 Naboo, the real battle, the idle player (`squad_log` replay): FOLLOW ME at tick 30 reached 5, 2 followed and stood
2 m from him; HOLD POSITION at tick 3030 reached 1 unit 30 m away, released 10 s later (PS2: a holder 22 m away at the
order went 12.3 s after it).

Not built: the orders from a vehicle seat (§4.4; a seated player's D-pad does
nothing), a follower boarding its leader's vehicle, orders to bots in vehicles (and the x3 reach), the AI's own
orders (§5; since card UTALK, e22e702, the AI's follow / move out / need gunner lines and the "yes" answers are
built, the boarding they cause is card GUNWT, `re-driver-wait.md` 7), the LOD-skipped think, the holder's look-around. The earlier port's Disperse (`CommandKind::Disperse`
and the commander's spread rule) is still in the code and no pad reaches it.

## 10. Checks run

- **fn** (check 1): `lead_tmp/sqack/cite.py b1|b2` — every cited line searched inside the named function
  (`cites_b1.txt` 287 rows, `cites_b2.txt` 79 rows, 0 misses).
- **absent / callers** (check 2): the class sound table is read at two sites only (`FUN_00260be0`, `FUN_00253118`;
  0x172c / 0x17fc / 0x1804 grep) — the "no" handle is never played; `FUN_00260f38`: 18 jal sites in 6 updates +
  `FUN_002613f8` (`refs.py`, which also sees vtable words); `squad` strings: 4 in B1, 1 in B2; `SetAIFormationRadius`
  in 0 `.luac` (`/usr/bin/grep -rlaF`, sanity hit 44 files for `SetBleedingVoiceOver`); BF2 0x2e2480: no jal, j or
  data word.
- **twin** (check 3): BF2's twin of the line function found as undecompiled code and shown dead; BF2's own order
  chain opened (issuer, delivery, cap, handlers, event table).
- **word** (check 4): 30 / 45 / x3 (0x2610d4, 0x2610f0, 0x261228); 20 m^2 = 400 and 0.05 (0x248904 / 0x24893c,
  0x247df8 / 0x247e30); +5 / +6 (0x247f60, 0x248110); 0.2 (0x247ed4); +20 s (0x2535e8); caps `slti 3` / `2` / `4`
  (0x253518, 0x253524, 0x247bcc); cycle `slti 16` (0x25da38); HUD rate 0.5 (word 0x518844), arm table 0x518168;
  BF2 1.0 s hold (0x246314), event table 0x624ed8.
- **writers** (check 5): `DAT_00588290` (two writers, 1 at rest), `DAT_0058831c` / `DAT_00588320` (init 0 / 3.0 +
  the script setter no script calls; RAM 0.0 / 3.0), `DAT_00519084` (ANNC), controller +0x290 (set only for building
  turrets), controller +0x254 (set only by `FUN_002631e0`).
- **Verifier** (one opus agent, foreground, given `lead_tmp/sqack/claims.md` — claims and citations only; report
  `lead_tmp/sqack/verifier_report.md`): **39 SUPPORTED, 8 PARTLY, 2 NOT SUPPORTED of 49**. All folded in above and the
  decisive ones re-run here: the take interval is 2.0 s for `ai_all` / `ai_imp` and absent for `wok` (my claim said
  1.0 s everywhere — `segs.py` on BES / TAT / KAS); the look re-roll is 1 in 18 (`FUN_002325d0` doubles its bound);
  type 4's gate is "the issuer is seated", not a free-seat count (RAM: one-entry lists, 13 seated units with a null
  controller); an AI issuer in a seat is not restricted to types 4-5; BF2's mass release is a 1.0 s HOLD of the
  button, not a press at an empty reticle; a BF2 hero cannot take followers; a fourth "squad" string; the LOD skip of
  the think cycle; five agent classes whose goal is replaced, not resumed. It also cracked the ODF property names
  (re-checked by hash here) and found the AI lines' inherited 20 +- 5 s interval.

## 11. Stayed [H] / not done

- Class +0x7c as "hero": read from two uses, not from the ODF property that sets it (no hero in the measured battle).
- The pad bits 8-11 as D-pad directions: [L] only (the raw-input mapping was not read).
- BF2: nothing measured live; the follow motion, the command button's raw bit, vehicle get-in / get-out not read.
- BF1 vehicle orders (§4.4) and the AI-in-seat deliveries: read, not run.
- Which earlier interrupt checks pre-empt an order in practice (0 of 31 live receivers lost one, the dying unit aside).
- The 3-D key's part in dropping far answers (§3 gate 2) is from the queue code of ANNC, not isolated live.

## 12. Reproduction

```
python3 engine/artifacts/lead_tmp/sqack/cite.py b1|b2                 # the citation table
python3 engine/artifacts/lead_tmp/sqack/rtti.py b1 0x536a58 0x536b68 0x536c70   # Follow / Hold / Random
python3 scripts/tools/refs.py b2 0x2e2480 0x391c90  # BF2's dead twin
python3 engine/artifacts/lead_tmp/sqack/segs.py extracted/bf1/sound/DATA/_LVL_PS2/SOUND/TAT.LVL _tac_yes
python3 engine/artifacts/lead_tmp/sqack/locl.py bf1 game.squadcommand.field.up game.squadcommand.field.left
engine/artifacts/pcsx2/pcsx2_session.py start iso/SWBF1.iso --name sqack --state engine/artifacts/pcsx2/ref/states/bf1_nab1_sideselect.p2s --boot-wait 14 --renderer sw
#   K, K, K to spawn; `savestate` once; then:
engine/artifacts/lead_tmp/sqack/scenes.sh sqack engine/artifacts/lead_tmp/sqack/scenes
python3 engine/artifacts/lead_tmp/sqack/summ.py engine/artifacts/lead_tmp/sqack/scenes/F3 ...
```
