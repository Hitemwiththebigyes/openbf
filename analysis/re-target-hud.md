# OPUS RE (2026-10-21) - [O] UNAUDITED
# The target's name and health on the HUD (BF2's target readout, BF1's enemy bar) — card HTGT

Corpora: B2 = `analysis/SWBFRNT2.ELF-decomp.c`, B1 = `analysis/SWBFRONT.ELF-decomp.c`. Layout: the BF2 `1playerhud`
dump `engine/artifacts/lead_tmp/re/bhud/dump_named.txt` (`re-hud.md`). RAM = the 8 BF2 save states in
`engine/artifacts/pcsx2/ref/states/` whose HUD holds a target (`lead_tmp/htgt/tools/scan_states.py`). Kit (all under
`engine/artifacts/lead_tmp/htgt/`): `tools/ram_checks.py` (every RAM fact below, re-runnable; output `ram_checks.txt`),
`tools/htgt_live.py` (a BF2 session from a state with a target: video + RAM rows, pokes the target's health),
`tools/bf1_enemy_pick.py` (BF1: holds the nearest enemy 10 m down the player's aim ray), `shots/` (the states' frames:
`contact.png`, `topbars.png`, `contact_bf1.png`), `live/{cor1,tan1,geo1,bf1e}/` (videos, rows, strips), `checks.txt`
(every re_check output). Tags: [V] read (lines cited), [V-RAM] read in save-state RAM, [M] measured on PS2 frames,
[D] derived, [H] open. Load-bearing claims carry [O].
Hops: (1) the event names in the layout → their strings in the ELF → the registration `FUN_003b0aa8` → the publisher
`FUN_003b3398`; (2) its target → the PlayerController's acquisition `FUN_002b0548`; (3) colours `FUN_0036d688` /
`FUN_0036fec8` and RAM; (4) the name `FUN_003b2b98` / `FUN_001e28a8`; (5) the bar's motion, BarBitmap
`FUN_0039f978` / `FUN_0039fce8` / `FUN_0039f790`; (6) 8 PS2 frames + 3 live BF2 trials; (7) BF1: the HUD class list,
the HealthGroup `FUN_002c2530` / `FUN_002c4468`, the name tag `FUN_002d1df8`, 6 frames + one staged enemy;
(8) re_check (§8); (9) the verifier workflow (§9).

## Headline
- **BF2 shows the name and health of whatever the reticle picks, from the first update it is picked, at the top centre;
  no hit and no hold is needed** [V][M][O] [receipt:c983e4eb]. The target is the PlayerController's per-channel
  acquisition winner (the object the reticle colour comes from): soldiers, droids, vehicles, turrets, buildings,
  destructible props, a capital ship — anything alive with finite health inside the weapon's cone and in sight. Frames:
  BOOKCASE (prop, 1000/1000), LAVA BRIDGE CONTROL (building), HOTH DEFENSE TURRET, LASER TURRET, HAILFIRE TANK,
  ALLIANCE AAC-1, REPUBLIC ATTACK CRUISER, CLONE TROOPER — all untouched, at or near full health.
- **What shows**: the class's localised name (Text, `gamefont_tiny`, grey α 0.6 with a black drop shadow), under it a
  bar = health / max health in the relation colour on a dark backing; a cyan "Shield" line under the name while the
  target's shield holds (droidekas only while deployed). No number is drawn (the `target.health` and `target.name`
  events reach no element).
- **Colours** [V][V-RAM][M][O]: friend (1, 86, 213) blue; enemy (200, 100, 100) red; a TEAM-0 object (props, empty
  vehicles, unowned turrets, buildings) (255, 255, 255) white; a non-zero team that is NEUTRAL to you (223, 223, 32)
  yellow. The bar is drawn at that colour × the group's (192, 224, 224) at α 0.8 (a group's Color multiplies its
  children — §6; the port does not do this yet).
- **The bar moves at once** (no easing); the span it lost stays as a copy that fades out over 1 s and thins toward its
  bottom edge; a span it gained shows as a brighter copy fading over 1 s [V][M][O].
- **Lost target**: name and bar fade out over 1 s, linear, keeping the last value; the bar turns white at once
  (white is sent as the colour of "no target") [V][M][O]. A switch to another target swaps the name and value and
  keeps them up (one frame with the backing dipped was seen at a TAN1 switch, `live/tan1/strip_all.png`).
- **BF1 is different** [V][M][O] [receipt:0da26cd4]: no class name; a red bar with the hud_health figure under the
  player's own health bar (top right), shown only while the reticle's pick is an ENEMY, fading out over 2 s otherwise.
  BF1's only name display is a world-space tag with another HUMAN player's name over his unit (team-mates always, an
  enemy only while picked; splitscreen / network — bots never get one). The port draws BF2's HUD on every map (PLAN,
  The maintainer 2026-09-03), so BF1 maps get BF2's readout.

## 1. The events (BF2) [V][O]
`FUN_003b0aa8` registers, per local player p, a block at 0x6a91f8 + p·0x31c (B2:407888) and per weapon channel w = 0, 1
an array of 26 event handles at block + 0x68 + w·0x68 (B2:407980). Slots 0xb–0x15 (B2:408003–408023; strings
`re_check word` §8): `player%d.weapon%d.target.` + name, className, shieldName, health, healthFraction, disable,
disableShieldName, teamColor, hit, hitCritical, hitColor. The layout uses className, shieldName, healthFraction,
disable, disableShieldName and teamColor for the readout (hit / hitCritical / hitColor are the reticle's flashes, card
BHUD3); **nothing listens to `target.name` or `target.health`** [receipt:9ee2eefe].

## 2. Which object is the target (BF2) [V][V-RAM][O]
- `FUN_003b61e0` calls the publisher `FUN_003b3398` for both channels of each local player (B2:411749), with K = the
  player's controllable (player+0x150, else +0x14c, else +0x148; B2:411654–411659).
- The target is the channel weapon's W+0x10c (its magnet, id-checked) when live, else **K+0x180+8·ch** (B2:410036–
  410060). The PlayerController update `FUN_002b0548` writes K+0x180+8·ch = its acquisition winner (and +0x184 its id)
  for each channel that has a weapon, every update offline (the network bytes DAT_00600548 / 549 that would stagger it
  are 0 in all 8 states; `re-spawn-flow.md`) (B2:236649–236653); W+0x10c = the winner when it is a valid magnet (in
  LockOnRange + r and passing the weapon's Target bits), else 0 (B2:236630–236645). (K+0x190 is a candidate scored with
  a fixed 0.25 slope, B2:236572/236656 — not the readout: in COR1 and space Kashyyyk K+0x190 is another object; in the
  other 6 states it is the same one.)
- [V-RAM] `ram_checks.py` finds the player's K (its +0x14 vtable 0x61e998) in all 8 states: the HUD's per-weapon cache
  (0x6a9050 + w·0x28: +0x14 target, +0x18 id) = K+0x180 / +0x188; where channel 2 picks nothing (TAN1, GEO1) the
  weapon-2 cache is empty too; K+0x1a0 (relation) = 1 on TAN1's clone, −1 on GEO1's tank, 0 on the team-0 targets.
- The acquisition is BF1's tournament (`re-aim-assist.md` §3.3) with BF2's changes: query mask 0x3b, LOS mask 0x90,
  candidates skip `EntityItem` unless `EntityMine` (B2:236424), eligible = (alive bit 3 of +0x228, or kind 5/6) and
  +0x26e bit 0 and vtbl+0x9c(self) (B2:236430–236440), a human's controller skips classes with +0x330 bit 36
  (B2:236441–236446), cloaked enemy soldiers and the shooter's own vehicle skipped (`re-aim-assist.md` §3.9). The cone
  is the weapon's LockOnAngle (+π/4 for the held target while K+0x38+4·ch bit 0) out to ExtremeRange + r. [V-RAM] type
  ids are fnv1a (lower-case) of class names: 0x689510 EntityItem, 0x6896d8 EntityMine, 0x688ea0 EntityBuildingClass,
  0x689300 EntityDroideka (`ram_checks.py`).
- The publisher then DROPS the target (no readout) when its class has bit 37 of +0x330 — the class key
  **HideHealthBar** (fnv 0xe921a7d8, setter `FUN_0026a340` B2:187094: bit := value ≠ 0; cleared by the EntityClass
  ctor B2:186480, inherited by the copy ctor B2:186544) — or when it is not alive (+0x228 bit 3) unless its class IsA
  EntityBuildingClass (B2:410102, 410093–410122). Key 0xa1c0f27b = fnv("ShowHealthBar") SETS the same bit when its
  value is non-zero (B2:186979; the instructions 0x26a8b8–0x26a8ec: strtol, `beq v0,zero` → skip, `or` 0x2000000000),
  i.e. it hides too. In the port's baked classes HideHealthBar = 1 on two (`bf2_end1`, `bf2_nab2`), 0x1b2d820b /
  0xa1c0f27b on none [receipt:897dd40a]; a verifier found 0x1b2d820b = 1 on a destructable building of the (unported)
  BF2 space maps SPA2–SPA4, which a human's pick therefore skips.

## 3. What it sends, update by update (BF2) [V][O]
- **Target changed** (B2:410140–410187): `disable` and `disableShieldName`, the cached value := −1; no new target →
  `teamColor` := white (0x69bd98); a new target → `name` and `className` (each when non-null).
- **Every update with a target**: `teamColor` (B2:410224–410253) = white when the target's team (signed nibble, bits
  32–35 of the 64-bit +0x268) is 0, else `FUN_0036d688`(relation byte teamTable[viewer]+0x84+team): > 0 friend
  0x69bd88, < 0 enemy 0x69bd90, 0 neutral 0x69bda0 (B2:366788–366813). Only `FUN_0036fec8` writes the four
  (B2:368270–368290): 0x69bd88 = (1, 0x56, 0xd5); the others copy the global table 0x76c228 / 0x76c200 / 0x76c250.
  [V-RAM] 0156d5ff, c86464ff, ffffffff, dfdf20ff. The reticle's colour listens to the same event: **BF2's friendly
  reticle is blue (1, 86, 213), not BF1's green** — the port's `RETICLE_COLORS` (`ui/src/hud.rs`) is BF1's.
- **Health** (B2:410255–410300): obj = target vtbl+0x2c; shield = obj+0x17c, shown when > 0.0001 (for a droideka only
  while its +0x228 bit 5, shield up). Shield shown: health/healthFraction from shield / obj+0x180 and `shieldName` =
  Locl 0x66049aa8 "Shield". Else `disableShieldName`, and unless HideHealthBar: `health` = (int)max(obj+0x170, 0),
  `healthFraction` = that / obj+0x174. The cache then stores the SHIELD value, not the health (B2:410279), so the
  health is re-sent every update ([V-RAM] the cache reads 0.0 in all 8 states and through the live pokes) — invisible.
- Any damage moves the bar the next update: the readout reads the health, not hit events (bullets, grenades, rockets,
  falls alike) [V].
- **No hold, no hit gate** [V][M][O]: nothing between taking the target and the sends depends on a hit, damage or a
  timer [receipt:c983e4eb] (the hit timers `FUN_003505c0` / `FUN_003505f0` are read only in the hit-flash block at
  the function's end, B2:410434–410440; two verifiers found no timer); the 8 frames show untouched targets.

## 4. The name (both games) [V][V-RAM][O]
At class load BF2 `FUN_001e28a8` (B2:93071–93081) and BF1 `FUN_001bef00` (B1:66186–66192) format `entity.%s` with the
class name, turn its first `_` into `.`, and store Locl(fnv(key)) at class+0x20 (BF2) / +0x30 (BF1).
`FUN_003b2b98` sends that string (re-localised when it is itself a key, else as is). `cis_tread_hailfire` →
`entity.cis.tread_hailfire` → "Hailfire Tank"; `rep_inf_ep3_rifleman` → "Clone Trooper" (`lead_tmp/re/evmsg/keys.py`);
[V-RAM] class+0x20 = the frame's text in 8 of 8 states. The font draws it in capitals. The port bakes the full English
table (`common_bf2/locl/english.ron`; "Hailfire Tank", "Clone Trooper", "Bookcase", "Shield" present).

## 5. How the bar moves (BF2 BarBitmap) [V][M][O]
- `FUN_0039f978` (B2:397238–397310): a value change > 1e-4 sets the u range (ScaleTexture, +0x374 bit 0) and — under
  +0x370 bit 33, on by default (ctor B2:397161), cleared only by parser key 0xf5b1a000 (B2:397426), which no element of
  the layout carries — the rect, at once (no easing). When the bar was visible last update (bit 35 ← the render
  flags' bit 8, B2:397306) and the flash time for the direction (rise `#2b76908f` → +0x364, fall `#2deba703` → +0x368;
  parser B2:397410–397424) exceeds 1e-4, `FUN_0039fce8` copies the bar one z in front, over the changed span (new
  right edge → old one; its u range too when both share a texture), fading out over that time (B2:397337–397380,
  397293–397300).
- `FUN_0039f790` (B2:397185–397233): on a FALL the copy's top edge moves down over the fade, from the bar's height to
  `FlashyScale` units: top = bottom − (h·(1−f) + FlashyScale·f), f = the fade's progress. Defaults: FlashyScale 0.2,
  both times 0 = no flash (B2:397156–397161). The target fill sets FlashyScale 1 and both times 1.
- [M] live COR1 (`live/cor1/strip_fall1.png`, `strip_rise.png`): 1000 → 600: the bar jumps to 60 %, the 60–100 % span
  stays and thins to a line at its bottom while fading, gone after ~1 s; 600 → 800: the 60–80 % span is brighter
  (two copies over each other) for ~1 s.

## 6. Layout, fades and measured pixels (BF2, 640×480 frame) [V][M][O]
- Data (`dump_named.txt` 863–993): Group `player1targethealthbar` (0.5, 0.059272) Viewport, Scale (0.263237, 0.23,
  0.23), α 0.8, Color (192, 224, 224), PropagateAlpha. Backing `player1targethealth_blankfill`: value 1, rect
  1.18923 × 0.181946 Center/Center at (0, 0.253896) Scale (0.924622, 0.498552), Color (39, 40, 46), FadeIn 0.1,
  FadeOut 1, on `healthFraction` / off `disable`. Fill `player1targethealth_fill`: rect 1.18923 × 0.181946 Center/Top
  at (0, 0.227078) Scale (0.890714, 0.222995), texture `hud_jetpack_and_energybar_fill` uv (0, 0.5)–(1, 1) (white),
  FlashyScale 1, flash 1 / 1, ColorChangeRate 0.01, FadeOut 1, value + on `healthFraction`, off `disable`, colour
  `teamColor`. Group `player1targetnamegroup` (0.5, 0.012799): className Text `gamefont_tiny` Center/Top at
  (0, 0.065248) α 0.6, Selected style, FadeOut 1 + a black shadow twin (0.000263, 0.067829) z 255; shieldName at
  (0, 0.106972) α 0.6 Color (40, 255, 255). 2playerhud repeats both per player; 4playerhud has the bars for players
  1–3 and no name groups.
- [M] 6 frames: fill x 232–410 (179 px), rows 45–48 full + 49 partial; backing ≈ x 231–414, rows 43–52; name glyphs
  rows 34–40, centred at x ≈ 320. Drawn fill = colour × (192, 224, 224)/255 at α 0.8 over the backing: enemy
  (127, 76, 76), friend (12, 72, 162), team 0 (158, 184, 185) — each within 2 levels of that model. The same multiply
  holds for the player's own health bar (group Color (255, 231, 255), fill (0, 150, 213), α 0.6): measured (8, 90, 136),
  model (8, 90, 135); without it G would be 99 [M]. So a group's Color multiplies its children; the port's
  `hud_draw.rs` uses each element's own colour only (a gap that also tints the port's health bar).
- Fades [V data + fader §2.1 of `re-hud.md`]: name and fill appear at once, the backing over 0.1 s; a disable then a
  className in one update keeps them up (hide 2→3, show 3→2); on loss all three fade over 1 s. [M] COR1: the name's
  brightness falls linearly from 146 to the background in ~1.0 s (11.64 → 12.6 s); TAN1 / GEO1: the bar turns pale
  white the frame the pick is lost and fades with the name.

## 7. BF1 [V][M][O]
- BF1's HUD is code; BF1's ELF holds none of BF2's target event strings [receipt:0da26cd4]. Two of its HUD classes
  read the reticle's pick (the verifiers' search of the pick's handle check, +0xb4 against K+0x34, found only these
  two HUD functions and game logic):
- **The HealthGroup's enemy bar.** The update `FUN_002c2530` (from `FUN_002d8f68`) hands the player's controllable to
  `FUN_002c4468` (B1:252035–252040, 252069, 252084, 252110; on foot K+0xe4 = K, [V-RAM] KAS2). `FUN_002c4468` shows
  the bar only while K+0x30 (the reticle's channel-0 pick, id-checked) is set AND K+0x40 < 0 (enemy)
  (B1:252675–252688): +0xc08 := 1, α 0xff, visible (B1:252720–252722), width ∝ clamp((pick+8)/(pick+0xc)) = health /
  max (B1:252704–252716, 252765), the icon element +0xa30 set to `hud_health`; else `FUN_002c4820` fades it, +0xc08
  −= dt·0.5 (2 s) (B1:252772–252790). Colour: the enemy red DAT_0069c448 written into the colour words of the
  elements +0xb50 / +0xac0 / +0xa30 in the ctor (B1:251855–251874); while +0xc00 > 0 the bar's colour eases toward
  DAT_005b0630 (B1:252746–252763; the +0xc00 writer was not found [H]). No text.
- **NameDisplayThread** (RTTI `17NameDisplayThread`; update `FUN_002d1df8`, a vtable slot): for each player record
  whose unit has a player index K+0xe8 ≥ 0 (B1:260285–260295) — a HUMAN: [V-RAM] KAS2 K+0xe8 = −1 on 53 bots, 0 on the
  player — other than your own, it writes the record's NAME (+0x1c, `FUN_004ba5c8`, B1:260455) in `gamefont_tiny` at
  the projected point 1.2 × the unit's model height (B1:260373), in its team's colour, α 255 − 4·(d − 20) past 20 m
  (gone by ~84 m, B1:260434–260435); a team-mate always, an enemy only while he is your channel's pick
  (B1:260316–260344). So BF1 names other humans over their heads in splitscreen / network games; it never shows a
  class name, and never names a bot. (Not this card: BF2's HUD is the port's on every map.)
- [M] 4 frames with a friend pick (GEO1 building, HOT1 vehicle, KAS2 ×2 soldier): no second bar. Staged enemy
  (`live/bf1e`, KAS2, 583 of 589 rows with K+0x30 = the enemy and K+0x40 = −1): the red bar with a pink figure under
  the player's blue one (`live/bf1e/fr/mid640.png`, `topright.png`).

## 8. Checks run (`lead_tmp/htgt/checks.txt`)
1. `re_check fn` on every cited line: all in the named functions (B2 FUN_003b0aa8, FUN_003b61e0, FUN_003b3398,
   FUN_002b0548, FUN_0036d688, FUN_0036fec8, FUN_001e28a8, FUN_003b2b98, FUN_00269d98, FUN_00269f38, FUN_0026a340,
   FUN_0039f978, FUN_0039f538, FUN_0039f790, FUN_0039fce8; B1 FUN_002c1f80, FUN_002c2530, FUN_002c4468, FUN_002c4820,
   FUN_002d1df8).
2. `re_check absent`: BF2's target event strings in BF1 — 0 in the BF1 ELF, 2 in BF2's [receipt:0da26cd4];
   `target.name"` / `target.health"` in the layout dump — 0, control `target.className"` 12 [receipt:9ee2eefe];
   class keys 0x1b2d820b / 0xa1c0f27b in the baked classes — 0, control HideHealthBar 2 files [receipt:897dd40a].
   The hit timers `FUN_003505c0(` / `FUN_003505f0(` — in FUN_003b3398 only at B2:410434–410440 [receipt:c983e4eb];
   BF1 `0xc00) = ` — the HealthGroup only zeroes / decrements it [receipt:6332a48d].
3. Twin: `callers` — FUN_003b3398 only from FUN_003b61e0; BF1 FUN_002c4468 only from FUN_002c2530 (3 calls), which
   only `FUN_002d8f68` calls.
4. `re_check word --str`: 0x631260 / 0x6312d0 / 0x6312f8 / 0x631320 / 0x631350 / 0x631288 = the event strings;
   0x6196c8 (B2) and 0x52e1a8 (B1) = `entity.%s`. constscan: 0x66049aa8 built at 0x3b308c and 0x3b407c (the two
   publishers), 0xe921a7d8 at 0x26a648 (the setter).
5. `re_check writers`: DAT_0069bd88 / 90 / 98 / a0 — one writer each, FUN_0036fec8. Bit 37: ctor, copy ctor, setter
   (two keys).

## 9. Verifier (workflow `htgt-re-verify`, 4 agents at medium effort, claims + citations only)
- 14 plain claims, two batches, one verifier each: 10 SUPPORTED, 4 PARTLY, 0 NOT SUPPORTED. PARTLY, fixed above: C3
  (W+0x10c is cleared, not left, when the winner is not valid; the update is staggered by the online-only network
  byte; K+0x190 can be the same object; the RAM script did not locate K — it does now), C4 (a line range), C10 (the
  rect and the flash sit under +0x370 bit 33, default on), C12 (the three colour words are inside the elements
  +0xb50 / +0xac0 / +0xa30; the visible bit is at B1:252722; the +0xc00 colour ease was left out).
- 5 absence / twin claims, each to two verifiers working apart: A2, A3, A4 SUPPORTED by both; **A1 (BF1 shows no
  target name) NOT SUPPORTED by both** — each found `NameDisplayThread` (`FUN_002d1df8`) drawing a player's name over
  the picked enemy; rewritten in §7 with its human-only gate, which I read (B1:260285–260295) and measured (K+0xe8);
  T1 (the BF1 twin) PARTLY by both — BF1's counterpart is split in two (the bar, and the human-only name tag); fixed.
  The second verifier overturned nothing the first had supported (both agreed on all five); A3's second verifier added
  the space-map caveat.
- Not given to the verifiers (measured, re-runnable from the kit): the frames' pixels and the live strips (§5, §6).

## 10. What stays [H]
- The flash copy's colour on a red or blue bar (the copy is the bar's render element copied, `FUN_005a2c88`, so the
  bar's colour [D]; only a white bar was filmed).
- Which object `target vtbl+0x2c` returns for a vehicle with a driver (the name path via vtbl+0x74's controller) — the
  frames show vehicles by their own class name; a piloted enemy vehicle was not filmed.
- BF1's +0xc00 colour flash on the enemy bar: no writer found in the HealthGroup [receipt:6332a48d] (the `= 1` writes
  are in `FUN_0020aad0`, another object; an aliased write) — not ported.

## 11. For the build (port decisions — reversible)
- The HUD target = the sim's reticle pick (`Sim::reticle_pick`, `core/src/sim/input.rs`), widened to the PS2's
  candidate set: vehicles, turrets, damageable buildings / props (today it walks soldiers only). Faithful: the same
  winner feeds the magnet (list-order effect, `re-aim-assist.md` §3.3), so this is a sim change for the player's assist.
- Feed (`ui/src/hud.rs feed`): className = Locl(`entity.<first _ → .>`) of the target's class; healthFraction; the
  colour rule of §3 (team 0 white, neutral yellow, friend blue, enemy red) for the bar AND the reticle; disable on loss
  with white; shieldName / disableShieldName.
- Interpreter (`ui/src/hud_tree.rs`, `hud_draw.rs`): the BarBitmap flash (§5) and a group's Color multiplying its
  children (§6) — the second also moves the health / stamina bars (golden).
