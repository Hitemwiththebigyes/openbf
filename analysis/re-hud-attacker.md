# OPUS RE (2026-10-21) - [O] UNAUDITED
# Where the shot came from: the hit-direction chevrons ("ambush" indicator), BF2 and BF1 — card HATK

Corpora: B2 = `analysis/SWBFRNT2.ELF-decomp.c`, B1 = `analysis/SWBFRONT.ELF-decomp.c`; ELFs `extracted/bf{1,2}/`. Layout:
the BF2 `1playerhud` dump `engine/artifacts/lead_tmp/re/bhud/dump_named.txt` (`re-hud.md`). Kit (all under
`engine/artifacts/lead_tmp/hatk/`): `ps2/hatk_live.py` (BF2 session from a state: the player held beside a fighting
team-mate at 30000 health, camera turns, one row per game update: the player's health, matrix and attacker, the Target
element's tracker, the objective / map-marker lists and the four indicator sprites; strip video), `ps2/hatk_bf1.py`
(the same for BF1), `tools/ambush.py` (hits against the sprite the game fired), `tools/hits.py`, `tools/hitsheet.py`,
`tools/tracker_states.py` (the tracker in every BF2 save state), `tools/flagwrites.py`, `tools/texhashscan.py`,
`checks.txt` (every re_check output), `shots/edges.png` (the four cases), runs `ps2/t1_kam1` … `t6_dea1`, `b2_geo1`.
Tags: [V] read (lines cited), [V-RAM] read in PS2 RAM, [M] measured on PS2 frames / rows, [D] derived, [H] open.
Hops: (1) the layout's Target element and its attacker templates → the style picker `FUN_003ad9b0` → the tracker
records and every writer of their flags; (2) the live tracker while the player is shot (empty) → frames (red chevrons
at the screen edges anyway) → a save state with a chevron up → the sprite in RAM → its texture by hash and its code
(`FUN_003912c0` / `FUN_00391680` / `FUN_003915a8`); (3) its callers = the TakeDamage virtuals; (4) the rule against
32 sampled hits on two maps; (5) the BF1 twin (`ambush_icon` string → `FUN_002d7e28` …) and one BF1 live hit;
(6) re_check (§8); (7) the verifier workflow (§9).

## Headline
- **Both PS2 games show where a shot came from** [V][M][O]: a flat chevron (the `ambush_icon` sheet, 64 × 32) at an
  edge of the screen, pointing out toward the attacker — **left edge** for a hit from the left, **right edge** for a
  hit from the right, **bottom edge** for a hit from behind (the shot travelling level or down), **top edge** for a hit
  from behind travelling up (the attacker below); **nothing for a hit from the front** (the attacker is on screen).
  The quadrants meet at 45°. It appears at full opacity and fades linearly to nothing in **1.0 s**; the four edges are
  independent and a new hit on an edge restarts it. Frames: `lead_tmp/hatk/shots/edges.png`.
- **It is not the layout's Target element** [V][V-RAM][O] [receipt:9028bdc3]: the `1playerhud` Target element's
  `Attacker*` templates (`hud_target_hint_*`, one of them missing from the disc) are dead data —
  no code sets the record bit that selects them (§2) [receipt:9028bdc3], and in 31,211 sampled updates while the player was being shot, and in 179 BF2
  save states, the Target element's tracker never held a record at all. The chevrons are code-drawn: four sprites
  inside the per-viewport HUD object `*(0x661380 + 4·p)`, one of the ten code-drawn pieces the pause menu switches
  (`FUN_00393028`, B2:385840–385850; `re-capture-display.md`).
- **What triggers it** [V][O]: the TakeDamage virtual of the entity a local player controls — the soldier (B2
  `FUN_00238e20`, B1 `FUN_00200c30`), and in BF2 three more classes, in BF1 five (§4) — on every call while the unit is
  alive, **before** the damage is worked out — so [D] a hit that takes no health still flashes it (10 flashes in the
  sampled runs came on updates with no health loss).
- **The direction** [V][M][O]: the call's direction vector (it behaves as the shot's travel, attacker → victim) in the
  victim's own frame (rows left, up, forward — measured on BF2: row 0 · camera-right = −1.00, row 2 · camera-forward =
  +1.00): sideways component beyond the forward one → right / left edge; forward component (shot travelling forward:
  attacker behind) → bottom, or top when it travels up; backward (attacker ahead) → none. Against 32 sampled hits:
  front 9 / 9 none, sides 12 / 14 on the attacker's side (2 none), behind 7 / 9 lit (bottom 4 / 4 when the attacker
  stood level or above, top 3 / 3 when he stood 1 m below; 2 none). Far shots behave the same (43–47 m: right edge,
  bottom edge).
- **BF1 vs BF2** [V][M][O]: the same code function for function (§5), the same 128 × 16 sprites, places and 1 s fade;
  the sheets differ — BF2's `ambush_icon` is dark red (mean 147, 52, 59) with a light rim, narrower (texels 10–53 of
  64); BF1's is a solid orange-red (223, 50, 0) triangle filling the sheet. One BF1 live hit (an EMP blast from behind)
  lit the bottom sprite, timer 1.0 → 0 in 1.0 s, seen on the frames. BF1's other "direction" pieces are not this:
  `hit_indicator` is the reticle's own-hit flash (`ReticuleDisplay`), `DirectionsGroup` is a `hud_arrow` toward the
  nearest post your team does not hold (§6).

## 1. The code-drawn indicator (BF2) [V][O]
- **Records** `FUN_003930b8` (B2:390220–390245): four × 0x14 at 0x6a4ee0 = (x, y, 0, 0, angle) in viewport fractions:
  sprite 0 (1.0, 0.5, −π/2) right edge; 1 (0.5, 1.0, 0) bottom; 2 (0.0, 0.5, +π/2) left; 3 (0.5, 0.0, π) top.
- **Sprites** `FUN_003912c0` (B2:389083; from the per-viewport HUD object's ctor `FUN_00392ae8`): four at stride
  0x150 inside the object `*(0x661380 + 4·p)` + 0x260; each bitmap (+0x90) sized `FUN_005a0120(128, 16, …, 1, 2)`
  (DAT_005ff300 / 304, `re_check word`), turned by the record's angle (`FUN_005a2d58`), timer +0x140 = 1.0.
  `FUN_00391448` (B2:389141) binds hash 0x3e7f6167 to all four [receipt:5a3695b3] — the engine's name hash of
  `ambush_icon` (FNV-1a over each byte | 0x20, so `_` → 0x7f; plain lower-case FNV-1a gives 0xaf963bc7);
  `FUN_003914a8` (B2:389159) places them at the records' (x, y).
- **Trigger** `FUN_00391680` (ELF 0x391680–0x39172c, disassembly in `checks.txt`): v = the vector it is handed;
  s = v0 + v2, d = v2 − v0. s ≥ 0, d < 0 → sprite 0; s ≥ 0, d ≥ 0 → sprite 1 if v1 < 0 else 3; s < 0, d ≥ 0 →
  sprite 2; s < 0, d < 0 → nothing. The chosen sprite is shown and its timer set to 1.0.
- **Fade** `FUN_003915a8` (B2:389198; from the HUD object's update `FUN_00392e30`): a shown sprite's timer −= dt ×
  DAT_005ff308 (1.0); alpha byte = timer × 255; hidden below 0. [M] 714 timer steps: 0.991 / s median (0.988–1.021).
- [V-RAM] `lead_tmp/hatk/ps2/t3_kam1/h1.p2s` (0.6 s + the save's lag after a hit from the left): sprite 2 shown,
  timer 0.233, alpha 59; sprites 0, 1, 3 hidden. The frame (`h1.png`) shows the chevron at the left edge.

## 2. The layout's attacker templates are dead (BF2) [V][V-RAM][O]
- The Target element `player1target` holds 16 templates in the order Attacker / Locked / Hint / HintFlag ×
  (OnScreen, OnScreenBehind, OffScreen, OffScreenBehind) (dump lines 1339–1467). `FUN_003ad9b0` (B2:405603) picks one
  per tracker record from its flags: 4 if bit 4 (locked), else 0 if bit 3, else 12 if bit 8, else 8; + 1 when bit 2 is
  clear, + 2 when bit 1 is clear (`re-hud-map.md` §5).
- **Nothing sets bit 3** [receipt:9028bdc3]: the tracker (`DAT_00825210[p]`, `FUN_00179278`) is reached through 22
  functions, every one decompiled (`addrscan.py bf2 0x825210` — 22 materialisations of the address, no hole); the
  64-bit OR / shift writes in them and in every function they hand the tracker or a record to
  (`tools/flagwrites.py`, 37 functions) set bits 0, 1, 2, 4, 6, 7, 8 and never 3; every write that touches bit 3
  clears it (the ctor's mask B2:19678 / 19685, the resets B2:19875 / 19888 / 20009 / 20570 / 21943, the clear
  B2:20862 in `FUN_0017a908`).
- What the 64 records are: the candidates of the `bSoldierHints` option — profile +0x20c (ScriptCB_SetGeneralOptions
  B2:43736) → `FUN_00435868` (B2:493473) → tracker bit 38 (`FUN_0017c520`) → `FUN_0017c698` → the candidate filter
  `FUN_0017c6e8`'s sixth argument; drawn as the Hint styles (a red `hud_target_hint_onscreen` ring over enemies on
  screen, nothing off screen). [V-RAM] bit 38 is clear and the list empty in all 179 BF2 states
  (`tools/tracker_states.py`) and in all 31,211 sampled updates; the objective list (0x66af58) and the map-marker
  table (`*0x5fb2c0`) were empty too.
- The shooter's side, not the victim's: `FUN_0034e068` (B2:344686, called at the top of the TakeDamage virtuals with
  (attacker, victim)) makes the VICTIM a sticky candidate of a human ATTACKER's tracker for 5 updates (+0x934, +0x940
  = 5) when the tracker exists, holds no live lock and his profile's `bAutoAim` (+0x32) is on or he has no profile —
  the auto-aim's hold on what you just hit.

## 3. The direction it uses (BF2) [V][M][O]
- `FUN_00238e20` multiplies its third argument by the transpose of the soldier's rows E+0x120 / +0x130 / +0x140
  without a translation (a direction, not a point) and hands the result to the trigger (B2:152525).
- [V-RAM] those rows are the soldier's left, up and forward (`ps2/t4_kam1/rows.csv`, 123 samples: row 0 · camera
  right −1.001, row 2 · camera forward +1.001).
- [M] `tools/ambush.py --flip t4_kam1 t5_kam1 t6_dea1` (`ps2/ambush_census.txt`): the rule applied to (victim feet −
  attacker feet), the attacker read from the victim's E+0x190 (its member record → unit → soldier), against the
  sprite the game fired on that update or the next two — 32 hits: right edge 3 / 3 + 0 / 1, left edge 9 / 9 + 0 / 1,
  front none 9 / 9, behind 7 lit / 9 (the game uses the shot's own vertical, not the feet: the 4 hits with the
  attacker level or above all lit the bottom, the 3 with the attacker 1 m below all lit the top). The flipped rule
  matches 26 of 32 hits exactly (the 6 misses: 4 unlit, 2 top / bottom by the feet proxy); the rule applied to
  (attacker − victim) instead matches 2 of 32 (`tools/ambush.py` without `--flip`).
- Not lit: 4 of 32 hits (damage 4.2, 28.9, 71.2, 450): [H] splash damage hands another direction (the 450 and the
  small values look like grenade blasts); not traced. 10 sprite flashes came on updates with no health loss.

## 4. Who calls it [V][O]
- BF2 `FUN_00391680`: four call sites [receipt:25d1e0b7] (jal 0x1f5fa0, 0x21d870, 0x2390d0, 0x2621fc; no data word):
  `FUN_00238e20` — the soldier: [V-RAM] the player's E+0x22c vtable 0x61ea80, slot +0x1c = thunk 0x255d38 →
  `FUN_00238e20`; `FUN_001f5e88`, `FUN_0021d748`, `FUN_002620e8` — [H] droideka, hover, walker by the strings beside
  their vtables (0x61bb48, 0x61cb84 / 0x62b81c CommandHover, 0x61f764 / 0x62bd54 CommandWalker). No BF2 flyer class is
  among them [H].
- BF1 `FUN_002d8028`: six [receipt:cf109fbe]: `FUN_00200c30` (13EntitySoldier, rtti of vtable 0x532d20), `FUN_001cf570`
  (14EntityDroideka), `FUN_001dc508` (13EntityCarrier, 11EntityFlyer), `FUN_001ec638` (11EntityHover, 12CommandHover),
  `FUN_0021c050` (12EntityWalker, 13CommandWalker), and the thunk `FUN_002d9588`, itself unreferenced.
- Gate (both): the unit is alive and its controller index (always B2 E+0x330 / B1 +0x388; B2 E+0x7f0 / B1 +0xbf0
  only gate a vehicle call beside it) maps to a local player (`FUN_003d6dd8` / `FUN_002ff158` ≥ 0) — BF2 takes that
  viewport's object for index 0 / 1 (an index ≥ 2 would hand address 0x260; not reachable with two viewports [H]);
  the trigger runs before the damage call (B2:152525 < 152581; B1:112747 < 112767).

## 5. BF1 [V][M][O]
Twin, function for function: records `FUN_002d91c8` (B1:264660, 0x5b8c58, the same four), sprites `FUN_002d7c80`
(B1:263749, stride 0x130, `FUN_004b5e28(128, 16)` = DAT_00518830 / 834), sheet `FUN_002d7e28` (B1:263816, the string
`ambush_icon` at 0x5409f8 hashed at run time), trigger `FUN_002d8028` (B1:263928), fade `FUN_002d7fa0` (B1:263898,
DAT_00518838 = 1.0), the group at `(&DAT_00564548)[p]` + 0x1ca0. Differences (verifiers): stride 0x130 / timer +0x120
(BF2 0x150 / +0x140); BF1 shows / hides by bit 8 of +0x48 directly; BF1's record init also writes two colour quads at
0x5b8c48 (ff ff 32 ff) and 0x5b8c50 (00 80 00 e6) that nothing here uses [H]. [M] `ps2/b2_geo1`: one hit from
behind lit sprite 1, timer 0.967 → −0.001 over 0.97 s (rows), the orange chevron at the bottom centre on frames
976–984. (A first BF1 run, `b1_geo1`, never left the spawn map — it sampled a bot near the map camera; void.)

## 6. What it is not (BF1) [V]
- `hit_indicator` (B1 string 0x5407c0, only in `FUN_002d4770` B1:261756 [receipt:3218dea2]) is an element of
  `ReticuleDisplay` (rtti, vtable 0x5407f4): a soft white disc coloured by the three per-weapon-slot hit timers
  (`FUN_00297dd8`, set to 0.5 by `FUN_00297db0` from the ordnance impact `FUN_001bc190` B1:64444) — the shooter's own
  hit flash.
- `DirectionsGroup` (rtti; ctor `FUN_002c1328`) turns the `hud_arrow` model (hash 0x46c7f4d0, `FUN_002c13e8`) toward
  the nearest item of DAT_0051833c whose entity (through the item's +0x20 handle) has a team (+0xd8) other than the
  player's (`FUN_002c16f8`) — a pointer to a post to take.

## 7. Placement on the PS2 frame (640 × 480) [M]
Measured on `h1.png` and the strip frames scaled to 640 × 480: the side chevrons 17 px thick × 92 px long, centred
vertically (centre y 241), the apex 32–34 px in from the side edge, the flat side ~50 px in; the bottom chevron
113 × 13 px, centred (x 320), the apex 23 px above the bottom edge. The code places a 128 × 16 sprite per edge at the
viewport points above through the HUD root's mapping (`re-hud.md` §2: x = −0.0059 + 1.0139 fx, a HUD unit = H/378.7 ×
H/427.3); [H] the inset the frames show is not derived from the code here (the sprite's alignment arguments 1, 2 and
the bitmap's texel margins were not traced) — the build uses the measured numbers.

## 8. Checks run (`lead_tmp/hatk/checks.txt`)
1. `re_check fn` on every cited line: all in the named functions.
2. `re_check absent`: the trigger's and the fade's callers [receipt:25d1e0b7] [receipt:cf109fbe]; the mask
   0xfffffffffffffff7 [receipt:9028bdc3]; `ambush_icon` / its hash / `hud_target_hint_offscreen` [receipt:5a3695b3];
   the BF1 strings [receipt:3218dea2].
3. Twin: both games' functions opened side by side (§5); callers walked with `callers` and the jal scan (`refs.py`).
4. `re_check word`: 128.0 / 16.0 / 1.0 at B2 0x5ff300 and B1 0x518830; the strings `ambush_icon`, `hit_indicator`,
   `rep_icon`.
5. `re_check writers`: DAT_005ff308 / DAT_00518838 (no writer: ELF constants) [receipt:48d1d246]
   [receipt:d9f82439], the records' globals (their init only).

## 9. Verifier (workflow `hatk-re-verify`, claims + citations only)
Workflow `hatk-re-verify` (run wf_3111dca1-390), 4 agents at medium effort, given the claims and their citations
only: 14 plain claims in two batches (one verifier each), 3 absence / twin claims to two verifiers working apart.
- Plain: 11 SUPPORTED, 3 PARTLY, 0 NOT SUPPORTED. PARTLY, fixed above: P2 (the name hash ORs each byte with 0x20, it
  does not lower-case — `_` becomes 0x7f), P5 (the controller index is always E+0x330; E+0x7f0 only gates a vehicle
  call; no `< 2` gate on the call, an index ≥ 2 gives base 0), P6 (`FUN_0034e068` also goes on with no profile, returns
  on a null tracker, clears a stale lock). Wording folded in from SUPPORTED notes: Q4 (BF1's index is always +0x388),
  Q6 (the team compared is the entity's behind the item's handle), P1 / Q1 (both inits also write 1.0 to two words past
  the fourth record).
- Absence / twin: A2 (the triggers' callers: 4 in BF2, 6 in BF1, `FUN_002d9588` dead) SUPPORTED by both, each with its
  own whole-ELF jal / j / data / lui scan; T1 (BF1 = BF2 function for function) SUPPORTED by both, with the differences
  listed in §5; A1 (nothing sets bit 3 [receipt:9028bdc3]) PARTLY by both, on the same sentence — the core held under each reader's own
  disassembly scan of 0x179000–0x17d200, but "the only write touching bit 3 is B2:20862" was false (other masks clear
  it too): rewritten in §2. The second verifier overturned nothing the first supported.
- Not given to the verifiers (measured, re-runnable from the kit): the frames, the RAM rows, the 32-hit census, the
  frame axes, the sheets' pixels.

## 10. What stays [H]
- Splash damage: 4 of 32 hits lit nothing; what direction an explosion hands TakeDamage is not traced.
- The BF2 classes of `FUN_001f5e88` / `FUN_0021d748` / `FUN_002620e8` (named by neighbouring strings, not RTTI);
  whether a BF2 flyer pilot gets the chevrons (no flyer caller found; not tested live).
- The pixel inset of the sprites from the code (§7 gives the measured numbers).
- Splitscreen (two viewports): the per-viewport objects exist (index < 2); not tested.

## 11. For the build (port decisions — reversible)
- Sim: `Event::Hit` gains `dir` — the shot's travel direction (the bolt's velocity, normalised, `core/src/sim/combat.rs`);
  for a blast (`core/src/sim/blast.rs`) the victim's centre − the blast's centre, PROVISIONAL (§10). Events are an
  output, not hashed: no sim, sweep or fidelity move.
- HUD: a new `ui/src/hud_ambush.rs` — four timers; `hit(local)` applies §1's rule to the direction in the viewer's unit
  frame (left, up, forward); `tick(dt)` fades 1.0 / s; draw BF2's `ambush_icon` (the port draws BF2's HUD on every map,
  PLAN) at §7's measured places, the side ones anchored to the window's left / right edges (Hor+), the bottom / top
  ones centred. Fed by `game/src/hud_feed.rs` from the Hit events whose victim is the viewer's soldier or the vehicle
  he drives (BF2's list: not a flyer), whatever the damage (the PS2 flashes before the damage is known).
- Test: the rule's quadrants (right / left / behind-down / behind-up / front) with a mutation check; the fade; a golden
  of the four chevrons over the HUD.
