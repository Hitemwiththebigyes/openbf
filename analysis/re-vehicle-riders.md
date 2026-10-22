# OPUS RE (loop lane 2, 2026-10-15, card VRIDE) - [O] UNAUDITED
# RE: a vehicle's riders - which seats show their soldier, where he hangs, how he is posed

**The questions** (card VRIDE, split from VTURG). The port draws no rider on any vehicle (`game/src/soldier_vis.rs`
`seat_now`: building turrets only), so a moving speeder bike has nobody on it. What does the PS2 draw? (1) The vehicle
draw's call into the seat function, per vehicle kind. (2) Which seats show their soldier, and the parent node each
hangs from, per vehicle class of both discs. (3) `AnimatedPilotPosition`. (4) How a played seat clip (`human_ride`,
61 frames) is timed, and where a nine-pose rider's numbers come from on a vehicle. One PS2 frame of a bike rider.

**Method.** The building-turret RE (`re-turret-draw.md` 3, [O]) already read the seat slot and the seat function for
the MountedTurret; this pass read the seat function's other callers in both games, the setter keys, the hover's
drive, the soldier's seat pose, and counted every vehicle class's seats by script over the raw LVLs of both discs.
Every rule was then checked on the PS2's own RAM: 7 stored battle states (both games) and two live rides the player
made on a speeder bike (BF1 and BF2 Endor, no AI), one row per game update (564 rows) and 12 save states. A verifier
workflow (6 agents, claims and citations only; every twin and absence claim to two of them) judged 30 claims
(section 9).

Tags: [V] read in the decomp / ELF at the cited lines, [L] measured on the running PS2, [D] derived, [H] hypothesis;
[O] = load-bearing, lifted, unaudited. B1 = `SWBFRONT.ELF-decomp.c`, B2 = `SWBFRNT2.ELF-decomp.c`. E = a vehicle
entity, K = its class, S = a seat block (the vehicle's own: BF1 K+0x14b0, BF2 K+0x380; a MountedTurret section's:
BF1 class+0x50, BF2 +0x60). A matrix is 4 rows (X, Y = up, Z = forward, translation); "M1 x M2" applies M1 first.

## 0. Short answers

1. **One function places every rider** [O][V][L]: BF1 `FUN_0023e028`, BF2 `FUN_002f74c0`, called by each vehicle
   kind's own update with the vehicle's world matrix O (BF1 E+0x200, BF2 E+0x120) and the vehicle's own pose table.
   No pose key: the seat slot gets O itself and hides him. A pose key: `L x E x O` (L = the `PilotPosition`
   hardpoint's bind matrix relative to its parent, E = that parent's entry in the vehicle's pose table now; `L x O`
   when the table has no entry). **`AnimatedPilotPosition`** names a node instead, and the rider takes that node's
   own entry: `E x O`, no L - he rides an ANIMATED bone (the tauntaun's and BF2 one-man AT-ST's `bone_root`, the
   kaadu's `bone_rider`).
2. **Drawn or hidden: the turret rule, per seat** [O][V][L] (`re-turret-draw.md` 3.1): a seat is drawn when it has a
   pose key; BF2 also hides on `IsPilotExposed = 0`; BF1 never hides by it. The census (section 1): BF1 14 vehicle
   pilot seats drawn, 24 hidden; BF2 6 drawn, 41 hidden; every hover tank's PILOT is hidden in both games (no pose
   key) and its GUNNER (TURRET1, `minigun_9pose`) drawn. Speeder bikes, the STAP, the BARC speeder, the tauntaun, the
   kaadu, BF2's one-man AT-ST and the AT-TE's main gunner are drawn; AT-STs, AATs and AT-ATs are not.
3. **Where** [O][L]: in the PS2's RAM every rider of a seat with a pose key sits exactly at `L x E x O`: the bike's
   rider at the bike's own matrix (+0.0002 m up) in 1 + 4 stored rows and 12 of 12 ride states, the fighter tank's
   gunner at (0.027, 0.658, -3.008) / (0.009, 0.658, -3.010) turned with the turret's yaw, the Jedi starfighter's
   pilot at its hardpoint's bind (pitched 22 degrees), the one-man AT-ST's rider swaying on the walk (y 2.72-2.85).
   Every hidden pilot sits at O.
4. **His pose** [O][V][L]: a nine-pose seat on a HOVER takes its three numbers from the hover's drive: the smoothed
   throttle p and turn y (rate 5 per second), `a = y + 4`, `b = y + (p > 0 ? 1 : 7)`, `c = |p|` - frame a blended
   toward frame b by c: he leans with the turn and crouches with the throttle (282 of 282 rows per game). Every other
   vehicle seat gets (-1, -1, -1): the clip plays from frame 0 at 30 frames a second and loops (BF1 [V]; BF2 [H]).
5. **The vehicle's own nine-pose** [V]: the same numbers pose the HOVER itself through its class's `FinAnimation`
   clip (the bike's vanes, the BARC, the fighter tanks, the skiff, the snowspeeder) - not drawn by the port (new card).

## 1. The census [O][L - counted by script over the raw LVL chunks]

`lead_tmp/vride/vehcensus.py bf1|bf2` reads every `entc` of every LVL (BF1 120 files / 710 class names, BF2 165 /
1,416), keeps every class whose root label is a vehicle (hover, walker, flyer, the command kinds, walkerdroid, droid,
carrier: BF1 43, BF2 57) and lists each seat: the class BODY (the pilot seat) and every TURRET section, with
PilotType, PilotPosition, AnimatedPilotPosition, the pose key and IsPilotExposed. `hangs.py` joins each seat with its
model's `skel` (`skel_<g>.txt`, `vturd/skeldump.py`). Verdict counts (pilot seat, verdict): BF1 drawn 14 / hidden 24
/ no soldier 5, gunner seats drawn 13 / hidden 18; BF2 pilot seats 6 / 41 / 10, gunner seats 6 / 31 / 13.

Every seat with a soldier drawn (D = AnimatedPilotPosition, the node itself; else the hardpoint and its parent):

| game | class | seat | pose key | hangs at (model, bind) | parent |
|---|---|---|---|---|---|
| both | imp/rep_hover_speederbike (BF2 imp only) | pilot | 9: speederbike_9pose | hp_active (0, 0, 0) | dummyroot |
| both | imp/rep_hover_fightertank | TURRET1 gunner | 9: minigun_9pose | hp_active (0, 0.658, -3.010) | **turret_y** (the TurretNodeName) |
| both | cis_hover_stap | pilot | A: stap_ride | hp_active (0, 0.360, 0) | cis_hover_stapmesh1 |
| both | all_walk_tauntaun | pilot | A: ride (61 frames) | D bone_root (0, 2.060, -0.248) | animated |
| both | gun_walk_kaadu | pilot | A: ride | D bone_rider (0, 2.210, -0.428) | animated |
| both | rep_walk_atte | TURRET1 main gunner | A: drive | hp_main_gunner (0, 7.097, 1.716) | aimer_maincannon_y (TurretNodeName) |
| both | rep_fly_gunship (+ BF2 kam/rep ride variants) | TURRET4 passenger | A: standing | hp_passenger1/3 | dummyroot / boneroot |
| BF2 | rep_hover_barcspeeder | pilot | 9: barc_9pose | hp_rider (0, 0, 0) | boneroot |
| BF2 | rep_walk_oneman_atst | pilot | A: drive_1manatst | D bone_root (0, 2.942, -0.545) | animated |
| BF1 | tat_hover_skiff | pilot + 3 gunners | A: man_minigun; 9: minigun_9pose | hp_pilot, hp_activel/hp_active/hp_activer | dummyroot |
| BF1 | all_hover_combatspeeder | pilot + gunner | A: drive | hp_pilot, hp_gunner | boneroot |
| BF1 | tat_hover_landspeeder, nab_hover_gianspeeder | pilot + passenger | A: drive | hp_pilot, hp_passenger / hp_gunner | the model's root |
| BF1 | rep_fly_gunship | TURRET2/3 ball gunners | A: drive | hp_gunner1/2 | ball_turret_1/2 |
| BF1 | bes_fly_cloudcar (pilot + TURRET1), all_fly_ywing, cis_fly_geofighter, rep_fly_jedifighter | pilot | A: drive | hp_active (pitched in its bind) | dummyroot |
| BF1 | all_hover_hovernaut | pilot | A: drive | hp_pilot is NOT in the model's skeleton: L stays the seat's default [H] | - |

Hidden although posed: BF2's snowspeeder (pilot `drivesnowspeeder`, gunner `drivesnowspeedergunner`), the A-wing,
X-wing, Y-wing, V-wing and every gunship ball gunner (`IsPilotExposed 0`). No pose key (hidden in both games): every
hover tank's pilot, the AAT, the MTT, the hailfire, the spider walkers, the AT-STs, the AT-AT, BF2's combat speeder,
every flyer not named above.

Showcase maps (a class named in the map's LVL [D]): BF1 NAB1 speeder bike, fighter tank, STAP, kaadu, combat speeder;
TAT1 bikes, fighter tank, STAP, skiff, combat speeder; YAV2 fighter tank, combat speeder. BF2 MYG1 speeder bike,
fighter tanks, BARC, one-man AT-ST, gunship; UTA1 fighter tanks, one-man AT-ST; HOT1 tauntaun, snowspeeder (hidden).

## 2. The seat function and who calls it [O]

### 2.1 The function
- [V] BF1 `FUN_0023e028` (B1:156148): returns when the pilot is 0 (B1:156186-156188). **No pose key** (S+0x210 == 0):
  the pilot's seat slot (vtable at pilot+0x294, +0x6c, `FUN_0020c880`) gets the caller's matrix O unchanged and clip
  0 = hidden (B1:156190-156192; the slot's hide path, `re-turret-draw.md` 3.1). **A pose key**: the entry E is looked
  up only with a table and S+0x1c4 != 0 (B1:156196-156199); with S+0x214 == 0 (B1:156202): E found -> `L x E x O`
  (L = the four rows S+0x1d0..0x200, B1:156203-156266), E not found -> `L x O` (B1:156270-156296, call
  B1:156337-156338); **with S+0x214 != 0 -> `E x O`**, the entry's own rows, no L (B1:156297-156330), and no guard
  for a missing entry. The slot also gets the clip word S+0x210 and S+0x220 (IsPilotExposed).
- [V] BF2 `FUN_002f74c0` (B2:285315), the same three branches: no clip = S+0x210 == -1 -> slot +0x8c of the pilot's
  vtable at pilot+0x14 with O and -1 (B2:285353-285356); lookup B2:285360-285361; byte S+0x214 == 0: E not found
  `L x O` (B2:285366-285401), E found `L x E x O` (B2:285402-285467); else `E x O` (B2:285470-285506).
- [V] The keys (BF1 setter `FUN_0023da58`, BF2 `FUN_002f6eb8`): `AnimatedPilotPosition` (0x11ce2025) sets S+0x214 = 1
  and S+0x1c4 = the CRC of the NODE's own name (B1:156131-156134; B2:285298-285302, a byte); `PilotPosition`
  (0x51ca39a6) stores the hardpoint's matrix relative to its parent at S+0x1d0 and the PARENT's hash at S+0x1c4
  (B1:156093-156101, B2:285261-285269); `Pilot9Pose` (0xa976d065) sets the nine-pose flag (BF1 S+0x224, BF2 byte
  S+0x221) and stores the clip like `PilotAnimation` (0x6e4fc069) (B1:155993-156001, 156103-156109;
  B2:285217-285221, 285271-285276); `IsPilotExposed` (0x2b60e217) S+0x220. In each setter the only write to S+0x214
  is the AnimatedPilotPosition key's (B1:156132, B2:285299; *(decompiler detail omitted)* lists them
  among other classes' fields [receipt:0a7a3fbd]).

### 2.2 The callers (the "vehicle draw's call")
[V] BF1 has 9 direct calls in 7 functions and the address is stored as no data word [receipt:1f34bd79] (9 `jal`
words 0x0c08f80a in the ELF); BF2 10 calls in 8 functions, likewise [receipt:d5e7a676]. Each vehicle caller passes
its own seat S, the pilot word (BF1 E+0x384, BF2 E+0x32c), O = the entity's world matrix and its own pose table:

| kind | BF1 | BF2 | numbers |
|---|---|---|---|
| hover tick | `FUN_001eba60` B1:98422-98428 (O E+0x200, table E+0x1d20, B1:98043-98045) | `FUN_0021ca80` B2:133284-133291 (E+0x120, E+0x17e0) | nine-pose seat: E+0x212c..0x2134 / E+0x1cb0..0x1cb8; else -1 |
| hover input | `FUN_001ea9f8` B1:97487-97491 (only with pilot type 2 and the nine flag) | `FUN_0021b470` B2:132412-132414 (nine flag) | -1 (the tick follows and wins [D]) |
| walker | `FUN_0021b3a0` B1:131809-131811 (table E+0x1ec0) | `FUN_00261208` B2:181103-181104 (E+0x1d7c) | -1 |
| flyer | `FUN_001dbbd8` B1:87186-87188 (E+0x1ce0) | `FUN_00209758` B2:121036-121038 (E+0x1940; skipped when class+0x1208 bit 1) | -1 |
| droideka thread | `FUN_001d1fe8` B1:80318-80320 | `FUN_001f99d0` B2:110035-110036 | -1 |
| PassengerSlot | `FUN_002279e8` B1:140245-140250 (S = slot class+0x50, table 0, O = the owner's) | `FUN_00271b28` B2:192525 (class+0x60) | -1 |
| MountedTurret | `FUN_00223850` (`re-turret-draw.md` 3) | `FUN_0026cea0` | the turret's nine numbers |
| remote terminal | - | `FUN_00232ca0` B2:148084-148088 (zero velocity) | -1 |

- [V] The kinds by their vtables: `FUN_001dbbd8` is the word at 0x5309f4 = EntityFlyer's vtable 0x5308f0 + 0x104;
  `FUN_0021b3a0` at 0x533c1c = the walker's 0x533b18 + 0x104; `FUN_001d1fe8` at 0x53023c (the droideka's second
  thread, `re-droideka-unit.md`). `FUN_002279e8` is called only by `FUN_00228388` (B1:140626), the word at 0x534714
  in the vtable group whose type-info function `FUN_00228800` (B1:140812-140828) names `13PassengerSlot` (0x5348b8).
  BF2 has no type names: `FUN_00271b28` is its twin line for line (called only by `FUN_00272440`, word 0x62031c), and
  `FUN_00232ca0` (called by `FUN_00234188`, word 0x61e21c) is the remote terminal's update - its object is the one
  `FUN_002334d0` hands over (B2:148330-148391), pooled by `FUN_002344c8` with the string `EntityRemoteTerminal`
  (B2:149059, 0x61e530; the object-to-pool link is a verifier's reading, [D]).
- [D] So a vehicle's pilot is placed by his vehicle's own update, its gunners by their MountedTurret's
  (`re-turret-draw.md` 3.2, the owner's table and matrix), every update.
- [H] A PassengerSlot's seat block is its slot class's own and no key was seen reaching it: its rider is hidden.

## 3. Where he hangs, seen in RAM [O][L]

`lead_tmp/vride/ps2ride.py <state>...` reads every pad vehicle (`vrest/ps2veh.py`'s pad ring), its pilot (BF1
E+0x384 = the soldier + 0x2a0, BF2 E+0x32c = the soldier entity + 0x270, the remote terminal's *(decompiler detail omitted)* at B2:148056) and every MountedTurret it owns, and prints `R = M_soldier x inverse(M_vehicle)`.
- Bikes: BF1 Endor 1 rider, BF2 Endor 4 riders (`hais/b1end1_end.p2s`, `hais/end1_end.p2s`): (0, 0.0002, 0), axes
  identity - `hp_active` at the bike's origin under `dummyroot`. The two live rides (`ps2/b1bike_*.p2s`,
  `ps2/bf2_b2bike_*.p2s`, at rest, full throttle, turning both ways, reversing): the same in 12 of 12 states. (Rows
  read during the update disagree by up to 1.3 m while moving - torn reads; 46 of 59 full-throttle rows agree.)
- Fighter tank gunners: BF1 Naboo (0.0269, 0.6581, -3.0079), axes a yaw of 0.18 rad, and (0.0651, 0.6581, -2.7194)
  at yaw 2.70 rad (`fxb/ps2cap/bf1_nab1_mid0/3.p2s`); BF2 Mygeeto (0.0089, 0.6581, -3.0100) at 0.058 rad
  (`ref/states/bf2_myg1_cp2_cp1_con_post1_spawn.p2s`): the hardpoint carried round by the turret node.
- BF1 Jedi starfighter (a flyer): (0, 0.0580, -1.1617), up (0, 0.930, -0.368) = `hp_active`'s bind.
- BF2 one-man AT-ST (`ref/states/bf2_uta1_cp3_con_cp1_postA_spawn.p2s`, two walkers): (0.0820, 2.7173, -0.5445)
  and (-0.0752, 2.8519, -0.5446), up (0, 0.991, 0.136) - the animated `bone_root` (bind (0, 2.942, -0.545)) swaying.
- Hidden pilots (fighter tank, AAT, AT-ST, both games): exactly O.

## 4. His pose [O]

### 4.1 The hover's numbers
- [V] BF1 `FUN_001ec768` (the drive, B1:99598-99626), when the hover has a pilot and its class's `FinAnimation`
  clip is set (K+0x25f8 and K+0x25fc): `p (E+0x67c) += (thr - p) dt 5`; `y (E+0x680) += (clamp(trn - str, -1, 1) -
  y) dt 5`; `a (E+0x212c) = y + 4`; `c (E+0x2134) = |p|` (`FUN_00434110` = |f12|, f12 = the new p at the `jal`
  0x1edd04); `b (E+0x2130) = y + 1` when p > 0, else `y + 7`. Its caller (`FUN_001ea9f8` B1:97634-97698) passes thr,
  str, trn = E+0x300 / 0x304 / 0x308, but first, when the class has no `StrafeSpeed` (K+0x1790 == 0) and str != 0,
  `trn = trn (1 - |str|) + str, str = 0` (B1:97668-97680; half the strafe with the mouse flag).
- [V] BF2 `FUN_0021d8b8` (B2:134693-134723), the same with `clamp(trn + 0.5 str)` and its own fields: gate E+0x32c
  and K+0xbd8 / K+0xbdc; p E+0x54c, y E+0x550, a / b / c E+0x1cb0 / 0x1cb4 / 0x1cb8; `FUN_0052a770` = |f12| at
  0x21f200. a, b and c start at -1 in the constructors (B1:96488-96490, B2:130953-130955), p and y at 0
  (B1:96395-96396, B2:130826-130827) - **corrected 2026-10-22 (card VPOSE)**: this line said "both numbers start
  at -1"; 786 of 786 never-driven hovers in RAM read p = y = 0 (`re-vehicle-pose.md` 4.2). All stay as last
  written when the rider leaves.
- [V] The hover tick hands them to the seat only for a nine-pose seat (B1 K+0x16d4, B2 byte K+0x5a1, B1:98422-98428,
  B2:133284-133291); BF1 and BF2 bikes and the BARC are the only nine-pose PILOT seats on either disc, and each has a
  `FinAnimation` (`fincensus.py`: 2 of 2 per game), so the gate is always open for them.
- [L] The live rides (`ps2/ana_b1.py`, `ana_b1b.py`; the BF2 check in the session): `a = y + 4, b = y + (p > 0 ? 1
  : 7), c = |p|` on 282 of 282 rows in each game; the smoothing with dt = the game clock's step fits 244 of 276
  consecutive BF1 updates within 2e-4 with the remix (median error 6e-8; a fixed 1/30 fits fewer), BF2 258 of 276
  with `trn + 0.5 str` (`trn - str` or `trn` alone: 198). Throttle on -> p 1.0, b = y + 1; reverse -> p -1, b = y + 7;
  BF1 A held -> y -0.998 (the strafe turned into a turn); BF2 W+A -> str -0.71, y -0.353.
- [D] So frames 3-4-5 are the lean (y -1 / 0 / +1) at rest, frames 0-1-2 the same at full throttle, 6-7-8 reversing,
  and the pose is frame a blended toward frame b by |p|.

### 4.2 The played clip
- [V] BF1's soldier pose `FUN_0022aa10` in the seat state (0xc) with a clip hash: a clip that differs from the
  player's current one restarts it at 30 frames a second (`FUN_0043e4f8(30.0)`) and sets the player's loop word to 1
  (B1:142393-142407; MIPS 0x22abdc `sw v0, 0x9b4(s2)`, s2 = the player at +0x12a0); then a >= 0 samples frame a and
  frame b and blends by c (B1:142408-142412), else the clip advances by the frame's time (`FUN_0043e158`,
  B1:142413-142415): `u += dt x 30 / frames`, wrapping past 1 (the loop word). So `human_ride`'s 61 frames loop
  every 61 / 30 s.
- [H] BF2 `FUN_0027caf0` (B2:200931-200950) plays the clip through `FUN_0027fb50` / `FUN_0027fd50` with a constant 1
  (read as the loop); its frame clock is BF2's (frames - 1) / 30.
- [V] The rider draws no hand weapon (states 0xc / 0x11, `re-turret-draw.md` 3.1).

### 4.3 The clips' root tracks (`lead_tmp/vride/clipdump`, both discs' banks)
- [L] `human_ride` holds a constant DummyRoot translation (0, 2.059, -0.297) on all 61 frames (bone_root (0, 0.465,
  0.141)) - almost the tauntaun's `bone_root` bind (0, 2.060, -0.248). Every other rider clip's DummyRoot is 0
  (`speederbike_9pose`, `barc_9pose`, `stap_ride`, `drive_1manatst`, `drive`), and their bone_root carries the
  height (bike 1.13-1.20 by frame, STAP 1.82, BARC 1.24-1.31, one-man AT-ST 0.63).
- [D] The port's soldier bake sets every DummyRoot track to its rest (`bake::soldier::clip_to_tracks`); the turret
  gunners drawn that way matched the PS2's frames (card VTURG, `minigun_9pose`'s DummyRoot is (0, 0, -0.866)).
  [L] (card VRIDE build, 2026-10-15) The PS2 does not draw a constant DummyRoot offset either: a bot riding BF1
  Naboo's kaadu, seen from 9 m (`lead_tmp/vride/ps2/kaadu_frame.py` on `ref/states/bf1_nab1_cp1_cp2_postA_spawn.p2s`,
  frames `ps2/kaadu_shot0.png` / `kaadu_shot1.png`, pilot word E+0x384 set in both, `kaadu_pair01.png`), sits ON the
  saddle; with the offset he would float ~2 m over it. The port's rule (DummyRoot held at rest) is the PS2's.

## 5. The vehicle's own nine-pose: `FinAnimation` [V]
`FinAnimation` (0xef563a98) names a clip in the class's own bank: BF1 K+0x25fc = the clip in bank K+0x25f8
(B1:102412-102420), BF2 K+0xbdc in bank K+0xbd8 (B2:138017-138031, the bank found by `FUN_00224320`,
B2:138123-138134). Right after the rider's numbers, the drive samples it at frame a and frame b, blends by c and fills
the hover's own pose table (B1:99627-99635 -> `FUN_00498040(E+0x1d20)`; B2:134724-134737 -> `FUN_0057eaa0(E+0x17e0)`).
Classes (`fincensus.py`): both games' speeder bikes (`imp_speederbike_9pose`), the fighter tanks
(`rep_fightertank_9pose`), BF1's skiff, BF2's BARC, the snowspeeder (a flyer: its own path not read). The port draws
hovers rigid: the vanes do not move (new card VPOSE, with the vehicle's turret nodes; RE done 2026-10-22,
`re-vehicle-pose.md`: the clip sampled by slerp at frames a and b, blended by slerp, written into the node table;
297 of 297 hovers in RAM replayed to 0.001 deg).

## 6. For the port (card VRIDE build)
What exists: `soldier_vis::seat_now` draws a building turret's gunner at `L x node x O` (`turret_pose::GunnerHang`,
the turret's baked rig); `SeatSpec` reads `PilotPosition`, the pose key and `IsPilotExposed` for every seat, vehicles
included (`gunner_draw`); the vehicle bake writes each node's bind POSITION only (`bake::hardpoints`); walkers have
a posed skin (WSKN, `viewer/walker_pose.rs`); the soldier bake holds three seat clips.
1. **Bake**: the drawn riders' clips into `SEAT_CLIPS` (BF1 bank `human_animation_1` / `_4`, BF2 `human_4`):
   `human_speederbike_9pose`, `human_stap_ride`, `human_ride`, `human_man_minigun`, `human_standing`,
   `human_barc_9pose`, `human_drive_1manatst`; per vehicle class the hardpoints' full model-space bind matrix and
   parent (the skiff's gunners face sideways, the fighters' pilots are pitched) - `rebake=side`.
2. **Seat**: `AnimatedPilotPosition` into `SeatSpec` (a node hung from directly); `gunner_draw` unchanged.
3. **Hang**: a vehicle seat's rider at `O x bind(hardpoint)` on a rigid vehicle (= `L x E x O` with E the parent's
   bind); a gunner whose parent is a TURRET node at the turret's pose when the vehicle's turret is posed (VPOSE; the
   rest pose until then, as the drawn gun); an `AnimatedPilotPosition` rider at the walker skin's node matrix now.
4. **Pose**: a hover's nine-pose rider from p and y smoothed at rate 5 from the driver's own input (BF1 `clamp(trn -
   str)` after the strafe-into-turn remix for a class with no StrafeSpeed, BF2 `clamp(trn + 0.5 str)`), visual state
   only (no sim hash); every other seat plays its clip from frame 0 at 30 frames a second, looping.
5. Hidden: no pose key, BF2 `IsPilotExposed = 0`, PilotType self / remote / none, passengers.

## 7. Still [H] / not read
- BF2's played-clip loop and clock (4.2); BF1's hovernaut pilot whose `hp_pilot` is not in its model (L stays the
  seat's default, not read).
- (build) The port's walker skin poses BF2's one-man AT-ST `bone_root` 0.07-0.22 m higher (y 2.915-2.942) than the
  PS2's RAM (2.717 / 2.852) and without its sideways sway (+-0.029 against 0.082 / -0.075), though at the same walking
  tilt (up (0, 0.991, 0.136)): the walker skin bake holds DummyRoot's translation at rest (`viewer::walker_pose`), so
  the clip's bob is not drawn [H]. The rider rides the node as the port draws it.
- (build) BF2's MountedTurret setter falls through to the seat setter `FUN_002f6eb8` and returns its claim
  (B2:191723-191731), so a walker's TURRET section keeps every seat key; the port's walker scoping
  (`vehicle_class::walker::TURRET_CLAIMS`) keeps only the pose keys now (card VRIDE) - `PilotType`, `EyePointOffset`
  and the camera keys of a walker turret section still fall to the walker's body there.
- The hover input's own call (2.2, -1 numbers) is overwritten by the tick's in the same update [D]; the order of the
  two tasks was read for BF2 only (`re-hover.md` 3).
- The flyers' and walkers' own posed nodes (a gunship's ball turret carries its hidden BF2 gunners; BF1 draws them);
  the snowspeeder's `FinAnimation` path; the seat slot's stored numbers in the soldier (offsets not read).

## 8. Tools and data (`engine/artifacts/lead_tmp/vride/`)
`vehcensus.py`, `hangs.py`, `fincensus.py` (+ `vehcensus_<g>.txt`, `hangs_<g>.txt`, `skel_<g>.txt`,
`fincensus.txt`), `ps2ride.py` (riders in a save state), `clipdump/` (a seat clip's root tracks; `clipdump_bf2.txt`),
`wordat.py`; `ps2/ride_b1.py`, `ps2/ride_b2.py` (the player boards the nearest free vehicle of a class on the running
PS2 and rows are written per game update, a state + frame per phase), `ps2/ana_b1.py`, `ana_b1b.py`, the rows
`ps2/b1bike_rows.json`, `ps2/b2bike_rows.json`, the states and frames (`ps2/b1bike_sheet.png`,
`ps2/b2bike_sheet.png`: the rider on the bike at rest, at full throttle, turning).

## 9. Checks run and the verifier
**Self-checks**: `re_check.py fn` on every cited line; `re_check.py callers` for both seat functions and the thunks;
`re_check.py word` on the vtable words (0x5309f4, 0x533c1c, 0x53023c, 0x534714, 0x53470c, 0x62031c, 0x61e21c), the
type-name string 0x5348b8 and the fabs helpers 0x434110 / 0x52a770; `eedis.py` on the two fabs calls and the loop
word's store; `re_check.py absent` for the absence claims [receipt:1f34bd79] [receipt:d5e7a676]
[receipt:0a7a3fbd]; the FNV hashes of the six keys computed.
**Workflow `vride-re-verify`** (6 agents, effort medium; claims and citations only): 4 first readers over 30 claims
in four batches (seat function, callers, pose numbers, census and RAM re-runs), 2 second readers over the 8 twin and
absence claims. First reading: 28 SUPPORTED, 1 PARTLY (the caller's strafe-into-turn remix, now in 4.1), 1 NOT
SUPPORTED (the claim that the seat path never sets the clip's loop word: it does, 4.2 - corrected). Second reading:
6 SUPPORTED, 1 PARTLY (the BF2 drive's formula supported, the "differs only in the clamp" wording not - restated with
BF2's own fields), 1 NOT SUPPORTED (the same loop-word claim). No claim the first reader supported was overturned.
What the verifiers found that this session had not: the loop word set on a clip change; the caller's remix (also
seen in the live rows); the remote terminal's pool string; the drive's own `FinAnimation` player cleared to no loop.
