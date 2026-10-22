# OPUS RE (2026-10-22) - [O] UNAUDITED
# The HUD in a seat: BF2's reticle group and weapon panel for a vehicle or turret gun (card VSHUD)

Corpora: B2 = `analysis/SWBFRNT2.ELF-decomp.c`, B1 = `analysis/SWBFRONT.ELF-decomp.c`; DUMP = BF2's HUD layouts as text
(`engine/artifacts/lead_tmp/re/bhud/dump_named.txt`, card BHUD). Tools, captures and check outputs:
`engine/artifacts/lead_tmp/vshud/` (`tools/` readers, `ps2/` the live runs and their save states, `checks/` the
re_check outputs, disassembly and the verifier table). Tags: [V] read (lines cited), [D] derived, [H] hypothesis, [M]
measured on the PS2 (RAM or frame). Builds on `re-heat-hud.md` (the ring, its events, the heat component) and
`re-target-hud.md` / `re-aim-assist.md` 10.4 (the publisher, the reticle on foot).
Hops: (1) the per-player HUD driver `FUN_003b61e0` and which controllable it hands on; (2) the weapon publisher
`FUN_003b3398` for that controllable (its weapon slots, the reticle gate); (3) the reticle placer `FUN_0038e598`;
(4) the health / energy senders `FUN_003b4bb8`, `FUN_003b4668`, `FUN_003b5708`; (5) every vtable slot read from the
ELF (`re_check word`, `eedis.py`); (6) live PCSX2, BF2, three seat kinds with the gun overheated; (7) replays of the
reticle rule over 20 BF2 and 6 BF1 save states; (8) the port's HUD feed; (9) workflow `vshud-re-verify` (section 9).

## Headline
- **BF2 runs the SAME HUD code for a seat as for the soldier** [V][M][O]. `FUN_003b61e0` hands the publishers the
  player's controllable K = record +0x150, else +0x14c, else +0x148 (the soldier's). In a hover, a walker and a
  BUILDING turret alike the seat's controllable is at +0x14c (+0x150 was 0 in all three live seats). So the weapon
  rows show the seat's weapons (one row per channel: the AT-ST shows two), the heat ring, the refire bits and
  "OVERHEATED!" work exactly as on foot (OVHT), and the reticle group stays up.
- **The reticle sits where the SEAT's aim ray goes on screen** [V][M][O]: (0.5, y) of the point 1024 m along the
  controllable's aim ray (K+0x150 + 1024 x K+0xe0) through the drawn camera - the soldier's rule with the seat's K
  (the `ControlsUnit` switch to the soldier's aim is never authored [receipt:44e8e97c]). Measured: a Hoth building turret's camera looks
  along its aim, so the reticle is on the centre (line 240 of 480); the speeder bike's camera looks `TiltValue` 5 deg
  below its aim (line 178.5), the AT-ST's 6 deg (line 166.1). The rule equals the game's stored reticle in 20 of 20
  BF2 states to 0.000 lines.
- **The health bar becomes the vehicle's** [V][M][O]: on entry the soldier's bar is hidden (`healthDisable`) and the
  green `player1vehiclehealthbar` (same place, (0,200,0), red (200,0,0) under 0.3) shows the vehicle's health - for a
  building turret the OWNER building's (Hoth tower 3000 / 3000). The energy bar under it shows the vehicle's own energy
  (bike 50, AT-ST 80; a building turret has none and the bar keeps its last value). A 4 s seating chart comes up on
  entry (card VSEAC, filed).
- **The port** [V]: `hud_model` hides the weapon rows, the reticle and the target readout off foot, sends the soldier's
  health, and `reticle_y` puts a rider's reticle on the centre; nothing sends the vehicle health or seating events.
  Build spec in section 8.

## 1. Which object the HUD reads in a seat (BF2) [V][M][O]
- `FUN_003b61e0` (the per-frame HUD driver; one aligned `jal` from `FUN_00425840` twice, `re-heat-hud.md` 2): for each
  local player record r (DAT_00661330 + i x 0x1a4) it takes **K = r+0x150, else r+0x14c, else r+0x148**
  (B2:411657-411661) and calls the weapon publisher `FUN_003b3398` for channels 0 and 1 with K as argument 4
  (B2:411749). argument 3 is **E = slot +0x24 of the interface at K+0x18** (vtable word at K+0x34; B2:411728-411729).
- The interface slots, ELF words and code (`checks/words.txt`, `checks/asm.txt`): soldier 0x61e948 +0x24 = 0x255718,
  hover 0x61ca30 +0x24 = 0x224548, walker 0x61f610 +0x24 = 0x266df8: each `jr ra; addiu v0,a0,-0x288`, i.e. K - 0x270,
  **the entity itself**; the MountedTurret's 0x620000 +0x24 = 0x2711c8 `lw v0,0x250(a0)` on a0 = T+0x28, i.e.
  *(T+0x278), **the turret's owner** (the armed building, or the vehicle carrying the turret). Slot +0x28 returns the
  same object in all four (0x255720, 0x224550, 0x266e00, 0x2711d0).
- [M] Live (section 5, `tools/seat.py`): bike r+0x14c = 0x1cb3290 = hover entity + 0x270 (vtable 0x61ca80); AT-ST
  r+0x14c = 0x1c627a0 = walker entity 0x1c62530 + 0x270 (vtable 0x61f660); Hoth tower turret r+0x14c = 0x1bae9e0 =
  MountedTurret 0x1bae9d0 + 0x10 (vtable 0x620050); r+0x150 = 0 in all three; r+0x148 stays the soldier's K.
- [H] r+0x150 is a vehicle's gunner seat (a MountedTurret carried by a vehicle): not captured - no pad key moved the
  player from the AT-ST's pilot seat to its turret seat in two probes (`ps2/seat_switch.py`: Circle, Square, D-pad,
  L3, R3, Select, L1, L2, R2, Cross). The HUD reads whichever word is set, so the build does not depend on it.

## 2. The weapon panel in a seat [V][M][O]
- `FUN_003b3398` gets the channel's weapon from K's own vtable (word at K+0x14): slot +0x44 (K, channel) -> index
  (< 0 = none), then slot +0x48 (K, index) -> weapon (B2:409815-409820; asm 0x3b33e0-0x3b3428: a1 = the 10th stack
  argument, then a1 = the index). Per kind (ELF code): soldier 0x61e998 `lb 0x570(K+ch)` / `lw 0x550(K+4i)`; hover
  0x61ca80 `lw 0x30c(K+4ch)` / `lw 0x2fc(K+4i)`; MountedTurret 0x620050 `lw 0x250` / `lw 0x240`; walker 0x61f660
  `lw 0x2d0` / `lw 0x2c0`. Everything after that is the on-foot code. When the weapon, its class (+0x64) or its vt
  +0x98 value differs from the cached one (B2:409829-409844) it sends `disable`, `lockOnDisable`, `target.disable`,
  `target.disableShieldName`, then - with a weapon and the per-player byte +0xd3 clear - `change` (the vt +0x98 value,
  through the NameMesh table to `.mesh`) and `name` (class +0x60, else the class name converted; B2:409852-409867);
  then every frame the counts, `ammoInfinite`, `heat`, `refire`, `weaponsOverheat` (`re-heat-hud.md` 2). So boarding
  a seat is a weapon change: the seat's name shows for 2.5 s.
- `weaponsDisable` / `weaponsEnable` (block +0x140 / +0x13c): disable when neither channel's cache holds a weapon or
  the byte +0x379 of the per-player object *(DAT_00661368 + 4p) is set, else enable, on a change (B2:411750-411766).
- [M] Frames (`ps2/ps2_seat_sheet.png`): the tower turret and the bike show one row (infinite + the laser mesh); the
  AT-ST two rows - channel 0 (`imp_weap_walk_atst_cannon_head`, the lower row, the layout's weapon1) and channel 1
  above it. The name text was faded in every frame (fade-in 0.25, hold 2.5 s after `change`; the saves came later).
- Seat guns' class chains (baked classes): `imp_weap_hover_speederbike_cannon` HeatPerShot 0.15 / HeatRecoverRate 0.5
  / HeatThreshold 0.05 / ShotDelay 0.15; `imp_weap_walk_atst_cannon_head` 0.14 / 0.38 / 0.02 / 0.18;
  `tur_weap_hoth_lasermortar_laser` 0.12 / 0.25 / 0.2 / 0.22; all on `com_weap_veh_laser` (RoundsPerClip 0 =
  unlimited, TriggerSingle 0, HUDTag `hud_vehicle_lasers`).

## 3. The reticle gate in a seat [V][O]
- The group is hidden when slot +0x88 of **E's own vtable** (E from section 1) falls to false: the publisher sends
  `.weapon1.reticule.disable` (B2:409885-409890). The slot per kind (ELF words): soldier 0x61eb88 +0x88 = `FUN_0024dd28`
  (the alive bit, with conditions on the weapon of the soldier's ACTIVE channel - the 4-bit index in E+0x7e0 bits
  16-19 that B2:159940 / 165479 copy from the selected channel; its vt +0x58, and E+0x3e0 when the class has bit 59 of
  +0x1e0; B2:167348-); hover 0x61cc70 and armed building 0x61a470 +0x88 = 0x26bae0
  = `(E+0x228 >> 3) & 1`, **the alive bit**; walker 0x61f850 +0x88 = `FUN_002608f8` = `FUN_002f5950(E+0x270) != 0`,
  **a weapon in the walker's current channel** (B2:180202-180210). A building turret's E is its owner, so the gate is
  the building's alive bit.

## 4. Where the reticle sits in a seat [V][M][O]
- `FUN_0038e598` (a vtable slot: one data word, no `jal` [receipt:76d4d127] [receipt:da62dc6a]): K by the same pick
  (B2:387504-387519); only when byte +0x224 of the record from K's vt +0x80 is set does it use the soldier's K
  (B2:387521-387523); p = K+0x150 + 1024.0 x K+0xe0 (B2:387525-387531), projected (FUN_0057ba90, B2:387544); unless
  DAT_00600b44 or the per-player word +0xc33c == 1 (B2:387546) it sends **(0.5, projected_y x 0.5 + 0.5)** to
  `.weapon1.reticule.position` and `.weapon2.` (B2:387547-387565) - the x is always 0.5.
- Byte +0x224 is the class key **ControlsUnit** (fnv 0x44c77c95; FUN_002f6eb8 B2:285249-285251), zeroed by the record's
  ctor FUN_002f6ce0 (B2:284979), copied by FUN_002f6da0. No LVL of either disc holds the key's hash word (0 against
  399 BF2 / 102 BF1 words of the control HeatPerShot; `checks/controlsunit_scan.txt`) and the name is in no extracted
  file [receipt:44e8e97c]. So **every seat's reticle follows the seat's own aim**.
- [M] The rule replayed on save states (`tools/retproj.py`, the live camera *(0x60c240), AIMA's projection) against the
  reticle element the game stored (`checks/retproj_table.txt`): **20 of 20 states to 0.000 lines** - bike (6 VRIDE
  states + 5 VSHUD) 178.42-178.67, AT-ST 166.094 (4), Hoth tower 240.000 (4), and the on-foot control (the same
  element 0x187cfa0 held 153.419 before boarding). The soldier-K alternative lands off screen in every seated state.
- [M] Why the lines differ (`checks/aimray_vs_camera.txt`): the aim ray against the drawn camera - tower 0.013 deg
  (the camera looks along the aim; the ray starts 4.0 m ahead of the eye on its axis), bike 5.000 deg, AT-ST 6.000 deg
  = the classes' `TiltValue` (5.0, 6.0; the tower class authors none). The aim rays are horizontal at rest.

## 5. Live PS2: the heat ring in each seat kind [M][O]
`ps2/seat_hud.py` (SW renderer): load a battle state, board (or stay seated), hold R1 until heat 0.5 (`half`), until
the overheat bit (`over`), and after re-overheating wait 1.0 s (`cool`); a save state at each (its embedded
Screenshot.png is the frame of that RAM) and a row per update. Runs: BF2 Endor speeder bike (VRIDE's
`bf2_b2bike_1_rest.p2s`), the Endor AT-ST beside it (left the bike, held 2.6 m from the walker, Triangle), the Hoth
tower `turet12` (`tur_bldg_hoth_lasermortar`, from `ref/states/bf2_hot1_walk_cp1_cp6_b000.p2s`).
Sheet `ps2/ps2_seat_sheet.png` (rows tower / bike / AT-ST; columns cold / half / over / cool); ring arcs from each
save state's own frame against the heat 1 - cur in that state's own RAM (`tools/ringarc2.py`, `checks/ringarc2.txt`):

| seat | half: heat / ring | over: heat / ring | cool: heat / ring | refire bits, OVERHEATED! |
|---|---|---|---|---|
| Hoth tower turret | 0.465 / 0.494 | 0.992 / 0.988 | 0.742 / 0.807 | cool frame only |
| speeder bike (hover) | 0.433 / 0.488 | 0.967 / 1.004 | 0.467 / 0.562 | cool frame only |
| AT-ST (walker) | 0.499 / 0.562 | 0.933 / 0.979 | 0.553 / 0.623 | cool frame only |

Every save came a few updates after R1 was released, so every frame is a COOLING one: the ring reads -0.004..+0.095
above the heat, inside one segment (0.033) plus a segment's 0.3 s fade-out at the gun's HeatRecoverRate (tower 0.075,
AT-ST 0.114, bike 0.15) [D]. No ring when cold. The bits and the line appear once the weapon's state is 5 (the `over`
saves were taken before the state turned 5 - `re-heat-hud.md` 2); "OVERHEATED!" shows in the tower's and AT-ST's
`cool` frames, the bike's too (fade-in 0.25 s).
(The first table of this doc took the heat from the row read just BEFORE each save, 0.03-0.07 higher; the verifier
caught it - section 9.)

## 6. Health, energy, the seating chart [V][M][O]
- `FUN_003b4bb8` (one caller, B2:411770 [receipt:da62dc6a]): vehicle = r+0x14c, object = its interface's +0x28
  (B2:410697-410702). Entering: sends `healthDisable` (+0x18); leaving: `healthInVehicleDisable` (+0x24) and
  `vehicle.healthDisable` (+0x3c) (B2:410706-410720). In: `vehicle.health` (+0x34) = (int) object +0x170 floored at 0
  and `vehicle.healthFraction` (+0x38) = +0x170 / +0x174, on a change (B2:410734-410747); `vehicle.seatingMesh`
  (+0x40) on entry (B2:410756-410768), and again whenever bit 32 of *(FUN_003af9d8(p) + 0x3940) is set (cleared there;
  B2:410776-410778 - [H] a seat change). Event names from the registration strings (FUN_003b0aa8, `tools/elfstr.py`).
- The layout: `player1vehiclehealthbar` at (0.064694, 0.827526) - the soldier's bar's place - fill (0,200,0) through
  `player1vehiclehealthcolor` (0..0.29 (200,0,0), 0.3..1 (0,200,0)), alpha 0.5, fade in / out 0.5 s, enabled by
  `vehicle.health`, disabled by `vehicle.healthDisable` (DUMP:90-97, 909-947).
- The soldier's own health while seated: `FUN_003b4668` sends it to `healthInVehicle` / `healthInVehicleFraction`
  (+0x1c / +0x20) while r+0x14c is set (B2:410564-410570; a hero class - bit 33 of its +0x330 - uses the `hero.health`
  slots +0x28 / +0x2c instead, seated or not); no BF2 layout listens to either [receipt:1c4effe5] (the disc's own
  INGAME.LVL neither - both verifiers). With the `healthDisable` sent on entry (above) the soldier's blue bar stays
  hidden for the whole ride [D: the two together; the bar listens to `player1.health` / `.healthDisable` only].
- [M] RAM: the Hoth tower's owner building 0x1c19560 holds 3000 / 3000, the AT-ST 8000 / 8000; the frames show the
  green bar full in every seat.
- Energy: `FUN_003b5708` reads K's vt +0x4c block (B2:411132-411137): soldier K+0x840, hover K+0x1b50, walker
  K+0x1f94, MountedTurret 0 (no send at all, so the bar keeps the last value it had). [M] bike 50 / 50, AT-ST 80 / 80,
  the soldier at the tower 100 / 100.
- The seating chart: `Group("player1vehicleseatinggroup")` at (0.129043, 0.596705), fade in 0.25 / hold 4 / out 0.25,
  enabled by `vehicle.seatingMesh`, a `vehicleseating` element with per-seat icons and texts (empty / self / player /
  AI) (DUMP:2001-2057). [M] the AT-ST cold frame shows "CO-PILOT" (empty) over "Player 1" (self), the tower's
  "Player 1". The port's HUD tree has no `vehicleseating` element [receipt:6d2d5a7e] (the 1-player layout holds 3
  mentions) - new card **VSEAC**.

## 7. BF1 [V][M][O]
- BF1's HUD is code-drawn (PLAN gives BF1 maps BF2's HUD); its reticle placer is the twin: `FUN_002d4b50` picks K =
  member +0xbc, else +0xb8, else +0xb4 (B1:262043-262047), swaps in +0xb4 when the int +0x230 of the record from K's
  vt +0x64 (vtable word at K+0x294) is set (B1:262059-262061; that int is ControlsUnit's BF1 field, FUN_0023da58
  B1:156081-156083), and writes (0.5, projected_y x 0.5 + 0.5) of p = K+0 + 1024 x K+0x110 (B1:262063-262085).
- [M] Replayed on VRIDE's six BF1 bike states (K = hover entity + 0x2a0): the element at 0x142d550 equals the rule in
  6 of 6 (lines 175.706-181.830; x stays 0.5 where the rule's x is 308.45 / 329.72) (`checks/bf1_bike.txt`).

## 8. The port, and what to build
- Has [V]: `core::sim::Sim::seat_look` (the seat gun's eye, look direction, muzzle, barrel), seat guns on
  `WeaponState` (heat, `sim/guns.rs`), `game::walk::vehicle_camera` pitching the view `TiltValue` below the look.
- Lacks [V] [receipt:f2abd005] [receipt:6d2d5a7e]: `hud_model` (game/src/main/battle.rs) gates `weapon`, `weapon2`,
  `reticle` and `target` on `on_foot` and sends the soldier's health; `WalkSim::reticle_y` returns 0.5 for a rider;
  nothing sends `vehicle.health*`, `healthDisable` on entry or `seatingMesh`.
- Build (card VSHUD, phase build; reversible):
  1. `hud_model` in a seat: row 1 = the seat's channel-0 gun, row 2 = its channel-1 gun if the seat has one (the
     on-foot `HudWeapon` fields from the seat gun's class and `WeaponState`; infinite for RoundsPerClip under 1); a
     passenger seat (no gun) sends `weaponsDisable` as the PS2 does with no weapon.
  2. The reticle in a seat: `reticle_y` = the point 1024 m along the seat's aim ray (`SeatLook::eye` + 1024 x
     `SeatLook::view`) through the drawn camera, x 0.5 - the soldier's rule with the seat's ray. Gate: the vehicle /
     building alive (hover, flyer, building turret), a gun on the seat (walker).
  3. The heat ring, refire bits and line: OVHT's events fed from the seat gun's `WeaponState` (rides on OVHT).
  4. Health: on mounting send `player1.healthDisable`, then `player1.vehicle.health` / `.healthFraction` from the
     vehicle's health (a building turret: the turret entity's - the port's turret IS the building, `VehicleKind::Turret`);
     on dismounting `player1.vehicle.healthDisable` and re-send the soldier's health.
  5. Energy: the vehicle's energy if the port models one, else leave the bar as it was (the PS2's turret case).
  Not this card: the seating chart (VSEAC); the target readout in a seat (the publisher reads the seat K's
  acquisition K+0x180 - whether the controller fills it for a seat is [H], not seen: no target was in view in any
  seated state).

## 9. Checks run and the verification (`checks/`)
1. `re_check fn` on 62 cited lines, BF2 and BF1 (`checks/fn.txt`), e.g. *(decompiler detail omitted)*, *(decompiler detail omitted)*, `B2:285251 is in FUN_002f6eb8`.
2. Absence: ControlsUnit name in no extracted file nor ELF, its hash in one setter per game [receipt:44e8e97c], hash
   words in no LVL (own u32 scan with control, `checks/controlsunit_scan.txt`); one `jal` each for FUN_003b3398 /
   FUN_003b4bb8 / FUN_003b5708 / FUN_003b4668, none for FUN_0038e598 [receipt:da62dc6a], one data word for it
   [receipt:76d4d127]; no `healthInVehicle` listener (control `player1.vehicle.health` 31 hits) [receipt:1c4effe5]; the
   port's sources [receipt:f2abd005] [receipt:6d2d5a7e].
3. Twin: BF1 FUN_002d4b50 (section 7) with its own ControlsUnit setter FUN_0023da58, and the RAM replay.
4. Words: 27 vtable slot words (`checks/words.txt`), the slot code (`checks/asm.txt`).
5. Writers: byte +0x224 - the key setter, the ctor zero, the copy (section 4).
- Workflow `vshud-re-verify` (13 agents, medium effort, claims and citations only; `checks/verify_table.txt`): 17
  claims in three batches, one verifier each (12 code claims, 5 measured ones with their commands to re-run); 5
  absence / twin claims to two separate verifiers each. 27 verdicts: 22 SUPPORTED, 5 PARTLY, 0 NOT SUPPORTED.
  The PARTLY ones, folded in: C4 (the soldier's gate reads the ACTIVE channel's weapon, not channel 0), C9 (wording),
  C12 (the change trigger is weapon, class or vt +0x98 value; `change` / `name` gated on a weapon and byte +0xd3),
  M4 (my ring table took the heat from the row before each save, 0.03-0.07 too high; re-measured from the states' own
  RAM, section 5), X2 reader A (the absence holds on the disc too; "the soldier's bar stays hidden" needs C7's
  `healthDisable` - now said). Absence / twin pairs: X1, X3, X4, X5 SUPPORTED by both; X2 SUPPORTED / PARTLY on the
  inference only - the second reader overturned nothing. Extra notes kept: C8's hero case, C7's seat-change re-send,
  C10's +0x379 owner.
- Stays [H]: r+0x150's meaning (a vehicle gunner seat); the target readout in a seat; a passenger seat's HUD; the
  bit-32 re-send of the seating mesh as a seat change.
