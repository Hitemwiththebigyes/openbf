# OPUS RE (2026-10-28) - [O] UNAUDITED — The remote droid: the `remote` weapon, the `droid` entity, its blaster and its destruct (card REMOTE)

Corpora: B1 = `analysis/SWBFRONT.ELF-decomp.c` (SLUS-20898), B2 = `analysis/SWBFRNT2.ELF-decomp.c` (SLUS-21240); line
numbers are of those files. Tags: [V] read at the cited lines, [M] measured on the PS2 (PCSX2, live RAM), [D] derived,
[H] hypothesis, [O] load-bearing for the port. Anything not [V]/[M] is PROVISIONAL in code.
Disc values: the baked class chains (`engine/assets_baked/classes/{bf1,bf2}_*.ron`), read by name and by FNV-1a hash.
**No live PCSX2 capture was made for this note.** Nothing here is [M].

## 0. What the PS2 does, in one page [O]

| question | BF1 | BF2 | tag |
|---|---|---|---|
| who carries it | 8 soldier classes: the marksmen, the clone sharpshooter, the scout troopers, the sniper and assassin droids; channel 1, 1 clip | 13 classes counting variants: the officers and the Wookiee warriors; channel 1, 1 clip | V (disc census, `game/tests/suite/remote_droids.rs`) |
| the weapon | class `remote`, a `dispenser` subclass: the press throws the droid entity named by `OrdnanceName` like a mine; one droid per weapon; no second throw while it lives | same (`OffhandWeapon 1` on `com_weap_inf_remotedroid`) | V |
| launch | dispenser throw: aim x (MinStrength + (MaxStrength - MinStrength) x charge) x `Velocity` + the owner's velocity, at `InitialSalvoDelay` 0.3 s | same | V |
| control | 0xc43b77dd = 1: the soldier's controller moves to the droid at launch; the soldier stands | same | V |
| retaking control | a fresh press of the remote weapon's trigger while its droid lives | same | V |
| giving control back | [H] the exit/ACTION button; the droid stays where it is until it runs down | same | H |
| flight | hover on a lift spring over the ground ray (`SetAltitude` 1.75 / imp 2.0), stick drives toward `MaxSpeed` 9 / `MaxStrafeSpeed` 7 under an `Acceleraton` cap of 70 (BF1 only reads the misspelled key) | `SetAltitude` 2.0, `Acceleration` 25 | V |
| how long it lasts | `MaxHealth` 40, `AddHealth` -1/s: 40 s | `MaxHealth` 100, `AddHealth` -1/s, `EnergyAutoRestore` -1.7/s: energy out first, about 58.8 s | V, H (energy max 100) |
| what it fires | channel 0 `*_remotedroid_blaster`: single-shot bolt every 0.2 s, 60 m/s, 10 damage, no vehicle/building damage | 0.15 s, 160 m/s, 60 damage, vehicle/building x 0.1 | V |
| the destruct | channel 1 `*_remotedroid_destruct`: one fresh press starts a 3.0 s (`MaxPressedTime`) countdown that cannot be cancelled -> an orbital salvo: 4 rounds 0.75 s apart, from 600 m at 70°, spread 2.5 m, each a 1000-damage bolt that explodes for 500 over 10.5 m | the same press and countdown -> the incinerator: a 12 m, 10000-damage burst at the droid (person/droid/animal only), plus `com_sfx_weap_remotedroid_exp` | V |
| its health | shot and blasted like a droid (`DroidScale`); destroyed -> its `ExplosionName` (BF1 50 damage over 3/5 m; BF2 a 0-damage look) | same | V |
| bots | not read in BF1 | the weapon suitability scores a `remote` at a fixed 1000; bots were seen flying droids in earlier captures (11 of 267 mounted bots) | V (B2), M (old capture, not this card) |

## 1. Class labels [V]

| label | BF1 | BF2 |
|---|---|---|
| weapon `remote` | `FUN_00292350` (B1:217943), derives from the dispenser `FUN_0028de58` (B1:214816) | `FUN_003465f8` (B2:327129), derives from the dispenser `FUN_00336858` (B2:316613) |
| weapon `destruct` | `FUN_0028bbb8` (B1:213118) | `FUN_00332ff0` (B2:314102) |
| entity `droid` | `FUN_001cb208` (B1:74919), size 0x25c0, base the common controllable `FUN_001e30c8` (B1:91993) | `FUN_001f0cf8` (B2:102731) |
| droid class setter | `FUN_001cb768` (B1:75160) | `FUN_001f12c0` (B2:102994) |

## 2. The remote weapon [V]

- 0xc43b77dd (disc 1) -> class+0x410 (B2 +0x4c0): take control on launch.
- 0xed422f60 (default 1) -> +0x414 (B2 +0x4c1): enables the "kill own droid" branch of the update.
- Fire `FUN_002921e0` (B1:217850): the dispenser fire `FUN_0028d958` (B1:214512); if the thrown item is a controllable it
  keeps its handle at weapon+0x150 (one droid per weapon) and, with +0x410, hands the soldier's controller (soldier+0xe4)
  to it through vtable +0x58.
- `FUN_00292100` (B1:217802) / `FUN_00292158` (B1:217826): no new fire while that droid lives.
- Update `FUN_00291ef8` (B1:217682): droid alive and a fresh trigger press (+0x70 == 1) -> control back to the droid.
  A press of a second byte (+0x74, unidentified; [H] reload) with +0x414 set kills the droid through
  `FUN_001bbc98(droid+8, …)` (B1:63906).
- The weapon's destructor kills its droid.
- Disc: `RoundsPerClip` 1, `ShotDelay` 1.4, `InitialSalvoDelay` 0.3, `AnimationBank` grenade, the thermal-detonator
  hand model, `FireSound com_weap_inf_remotedroid_fire`, `OrdnanceName <side>_weap_inf_remotedroid_ord`. The slot is 1
  clip.

## 3. The droid class [V]

Constructor defaults (identical in both games unless noted): `SetAltitude` 0, `GravityScale` 1, `LiftSpring` 5,
`LiftDamp` 2, `Acceleraton` 70, `MaxSpeed` 9, `MaxStrafeSpeed` 7, `MaxTurnSpeed` 2, `MaxPitchSpeed` 0.5, `PitchLimits`
±1 rad, `LevelSpring` 10, `LevelDamp` 3, CollisionThreshold 7.5 / 2.5.

- The hover-style keys on the disc (`ForwardSpeed`, `ReverseSpeed`, `strafeSpeed`, `turnRate`, `pitchRate`, `traction`,
  `deceleration`) are NOT read by the droid class; the defaults above govern speed and turning.
- BF1 matches only "Acceleraton" (misspelled); the disc authors "acceleration" 1.0, so BF1 keeps 70. BF2 also matches
  "Acceleration" (disc 25).
- `GravityScale` is authored "1.0//2"; the number parse stops at the comment, so 1.0.
- Authored: `SetAltitude` BF1 imp 2.0 / others 1.75, BF2 2.0; `LiftSpring` 10; `LiftDamp` 3; `LevelSpring` 5;
  `LevelDamp` 3; `PitchLimits` -50..50°; `EyePointOffset` (0, 0.5, 0); `TrackOffset` (0, -0.25, 1.5) BF1 / (0, -0.25,
  1.8) BF2; `TiltValue` 5.
- Health: `MaxHealth` BF1 40 / BF2 100, `AddHealth` -1 (per second, a drain). 0x465909fb = 30 and 0xeb460a76 = 300 sit
  in the health block, meaning unknown.
- BF2 `EnergyAutoRestore` -1.7: energy drains 1.7/s; at energy <= 0 `FUN_001ee610(1.0)` (B2:100734) is called. [H] that
  kills it and energy starts at 100, so 58.8 s.
- BF2 0x83e12771 (0, 0, 18) and 0xe11ce401 10° are an alternate camera offset/pitch while droid state +0x494 is 1..3
  ([H] the destruct charge). 0xf9765272 is the effect `com_sfx_weap_remotedroidcharge2`.
- `ExplosionName`: BF1 `*_remotedroid_exp2` (50 damage, radii 3 / 5, DroidScale 1); BF2 `*_remotedroid_exp1` (0 damage).
- WEAPONSECTION 1 `*_remotedroid_blaster` (channel 0, ammo 0 = infinite); WEAPONSECTION 2 `*_remotedroid_destruct`
  (channel 1, ammo 1).
- `AmbientSound remotedroid_engine_lp`, `Ambient2Sound remotedroid_radio`; geometry `com_weap_probedroid`; health icon
  0x0db7468e `HUD_com_probedroid_health_icon`; `AISizeType` BF1 SMALL / BF2 SOLDIER; `PilotType remote`
  (controllable class+0x21c = 3).

## 4. The flight step [V] — BF1 `FUN_001c9fd8` (B1:73969)

Arguments dt, forward stick, strafe stick, turn stick. With a = `SetAltitude` > 0:

1. Ray down 10a: ground distance h, normal n (no hit: h = 10a, k = 0).
2. k = clamp(1 - (h - a) / (9a), 0, 1).
3. vel.y += ((GravityScale - 1) k + 1) (-9.8) dt — gravity FIRST.
4. f = k 9.8 + LiftSpring (a - h) - LiftDamp (vel · n), using the velocity after step 3; f > 0 -> vel += n f dt.
5. Frame: forward / right from the yaw, perpendicular to the frame normal (n.x k, n.y + 0.1, n.z k) normalised.
6. c = Acceleraton k max(n.y, 0) dt; vel += fwd clamp(fwdIn MaxSpeed - vel · fwd, ±c); the same along right with
   MaxStrafeSpeed.
7. Yaw rate = -MaxTurnSpeed x turn (the AI: MaxPitchSpeed + (MaxTurnSpeed - MaxPitchSpeed) x factor).
8. The body up vector levels toward n through LevelSpring / LevelDamp; pos += vel dt. World collision is the common
   controllable's.
9. Speed² > 25 casts a second ray along the velocity that adjusts n.

[D] Because gravity is applied before the damping term reads the velocity, the steady state is not a: with v = -9.8 dt
after step 3 and n = Y, f = 9.8 k needs LiftSpring (a - h) - LiftDamp v to cancel the gravity deficit, which settles at
h - a = LiftDamp 9.8 dt / (LiftSpring + 9.8 / (9a)): 0.092 m at 30 Hz for BF1 (1.84 m for an authored 1.75).

## 5. The destruct [V]

- `MaxPressedTime` 3.0, `RoundsPerClip` 1, `ChargeSound` BF1 `timer_countdown` / BF2 `com_weap_remote_charge_fire`,
  0x4c5aa5aa = 0.3 (the charge sound's pitch step).
- BF1 instance vtable 0x53b5d0 (`python scripts/tools/elfrd.py extracted/bf1/SWBFRONT.ELF 53b5d0 48`): +0x74 fire entry
  `FUN_0028b898` (B1:212999) spawns a salvo object `FUN_00271f90` (B1:194436) when the class has an ordnance; +0x7c
  `FUN_0028b9d0` (B1:213069) explodes the class's ExplosionClass (0xca0c1378 -> class+0x204, if any) at the droid and
  kills the droid; +0x80 charge start `FUN_0028b7a8` (B1:212962: timer +0xd0 = 0, limit +0xd4 = class+0x200 =
  `MaxPressedTime`, 0x2819ffa0 in the setter `FUN_0028bcd8`); +0x84 accumulation `FUN_0028b868` (B1:212988).
- The predicates (not in the decompile; `python scripts/tools/eedis2.py bf1 0x0028bb08 0x0028bbb8`): charge +0xa4 at
  0x28bb08 — idle: the trigger byte == 1 (a fresh press); charging: timer < limit, the trigger NOT read; fire +0xa8 at
  0x28bb50 — charging and limit <= timer. Through the base state machine `FUN_00284318` (B1:208134; state 4 = charge:
  fire predicate -> fire, else charge predicate -> accumulate, else end) one press runs the countdown to the end and
  fires the destruct at `MaxPressedTime`, held or not; it cannot be cancelled. [V]
- BF2 the same: instance vtable 0x628690, charge start +0xb4 `FUN_00332740` (B2:313782: timer +0xbc = 0, limit +0xc0 =
  class+0x260), accumulation +0xb8 `FUN_003327f8` (B2:313808); predicates `python scripts/tools/eedis2.py bf2 0x00332f38
  0x00332ff0`: charge 0x332f38 — idle: trigger bits 0-1 == 3 (pressed), charging: timer < limit; fire 0x332f88 —
  state 3 (BF2's charge) and limit <= timer. [V]
- Salvo setter `FUN_002725a0` (B1:194686): [0] 0x3186fbfe yaw (deg -> rad, disc 0); [1] 0x8339fb35 pitch (disc 70°,
  default π/2); [2] 0xecdb9d1d distance (disc 600, default 100); [3] `SalvoCount` 4; [4] `SalvoDelay` 0.75; [5]
  `InitialSalvoDelay` 0; [6] 0xebd81a5e spread 2.5; [7] the ordnance (0x6fd92be8 in BF1, `OrdnanceName` in BF2).
- Salvo update `FUN_00272078` (B1:194482): M = rotX(-pitch) rotY(yaw); each round starts at target + M (spread (r1 -
  r2), spread (r3 - r4), distance) and flies along -M.z, r1..r4 from the game LCG.
- BF1 round `*_orbital_attack_salvo_ord`: bolt 300 m/s, life 3 s, 1000 damage, explodes as
  `*_orbital_attack_salvo_exp` (500 damage, radii 10.5 / 10.5, push 10).
- BF2 `com_weap_droid_incinerator_ord`: an `emitterordnance`, radius 12, ConeAngle 120, MaxTargets 10, 10000 damage,
  Person / Droid / Animal x 1, Vehicle / Building x 0; its explosion `com_weap_droid_incinerator_exp` is a 0-damage look;
  0x76f3beea is the sound `com_sfx_weap_remotedroid_exp`.

## 6. The AI

- B2 `FUN_002aaba0` (B2:218296), the weapon suitability, scores a WeaponRemoteClass at a fixed 1000. [V]
- BF1's choice of the remote and the droid's own AI driving were not read. [H]

## 7. Port decisions

| decision | tag |
|---|---|
| ACTION returns control; the droid stays until it runs down; a fresh press of the remote's trigger retakes it | H |
| One fresh press of the droid's secondary trigger starts the 3 s countdown; it runs to the destruct whatever the trigger, the control hand-back or the owner does next (until the droid dies) | V (§5) |
| The BF2 incinerator is a direct radius-12 burst with the ordnance's scales, under the normal blast friendly-fire rule; MaxTargets and the 120° cone are not modelled | H |
| The owner's death ends the droid (its explosion goes off); the owner boarding a vehicle only returns control | H |
| Bot use: launch with an enemy target within 40 m and the remote ready; steer at the nearest enemy in line of sight; close to `OptimalRange`, back off under half of it; fire within `MaxRange` when the yaw error is under 0.15 rad; hand back after 10 s with no target; no destruct | OURS |
| Pitch turns at `MaxTurnSpeed` | OURS |
| Collision sphere 0.35 m | OURS, provisional |
| The frame normal's (n.x k, n.y + 0.1, n.z k) shaping and the velocity ray (step 9) are omitted; the frame uses n | OURS |
| Engine and radio loops are not played | gap |
