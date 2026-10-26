# OPUS RE (2026-10-20) - [O] UNAUDITED — Health, ammo and vehicle droids (`powerupstation`): reach, rate, team, animation, sounds, tips (card PWST)

Both games. Sources: the two decomps, the ELFs (disassembly where Ghidra mixes float and integer arguments), the baked
class tables and world manifests, and two live PCSX2 runs (BF1 Naboo Plains, BF2 Hoth; RAM read through /dev/shm, the
player teleported and hurt by RAM writes). Tags: [V] read in the code (lines / ELF addresses cited), [M] measured on the
PS2, [D] derived, [H] hypothesis, [O] load-bearing and unaudited. Line numbers: `analysis/SWBFRONT.ELF-decomp.c` (B1),
`analysis/SWBFRNT2.ELF-decomp.c` (B2). Evidence, tools and logs: `engine/artifacts/lead_tmp/pwst/`. The pickup items
and the give functions the droids share with them are `analysis/re-powerups.md` (§5.3 health, §5.4 ammo, §5.5 BF2
energy and buffs, §5.6 the gain effects); the droid's `AmbientSound` is `re-ambience-music.md` §5.2.

## 0. What the PS2 does, in one page [O]

1. **Who places them.** Three classes of base `powerupstation`: the health droid (`com_item_healthrecharge`, the FX-7),
   the ammo droid (`com_item_weaponrecharge`, the gonk) and BF1's vehicle droid (`com_item_vehiclerecharge`, an R5).
   106 on the six showcase maps (§2.2). Each placed station carries an instance `Radius` (5.0 m; the vehicle droids
   15.0) and is Team 0 [V data].
2. **The reach.** `Radius` builds a SPHERE region at the station's origin; a unit is served while its origin (the
   feet) is strictly inside it, in 3-D: |feet - station| < 5.0 m [V]. Live: served at 4.56 m (BF1) / 4.60 m (BF2),
   not at 5.43-5.55 m / 5.34 m [M].
3. **The rate.** The station has its own clock: every `PowerupDelay` seconds (the remainder kept) it gives ONCE to
   every unit inside, the class's amount for that unit's health type — not a rate times dt [V]. BF1 health droid +15
   health every 1.0 s; BF2 +25 every 1.0 s; the ammo droid +1 clip to every weapon every 2.0 s; BF1's vehicle droid
   +400 health and +1 clip to a vehicle every 1.0 s. Health stops at the maximum; each weapon's reserve stops at
   its max clips (less one when the gun's clip holds a round) [V, M]. Live: BF1 75 -> 300 in 15 steps of +15, one
   per 1.001 s of EE clock (60 frames of 1/60 s at the NTSC 59.94 Hz); BF2 100 -> 400 in 12 steps of +25; BF1 ammo
   0 -> 4 / 3 / 1 (caps 5-1, 4-1, 2-1) in steps of +1 every 2.0 s [M].
4. **The team rule: none.** Friend or foe, every unit inside is served; the code reads no team, and every disc
   station is Team 0 [V]. A unit is served when its controllable slot is non-null (a soldier or a vehicle) and its
   class has an amount for its health type: health / ammo droids give persons and droids, the vehicle droid only
   vehicles [V].
5. **Not while broken.** A station gives (and animates) only while its building state is 0: destroyed (MaxHealth 80,
   BF2's health droid 200; then `DestroyedGeometryName`) or not yet built (BF2's `*_unbuilt` classes) it does nothing
   [V]. A BF2 SetHeroClass hero takes no health from a station (the same refusal as items) [V].
6. **Bots do not seek it.** No code outside the class's own type slots asks for a PowerupStation, in either game
   [V, absent] [receipt:87b3c7e4] [receipt:05cb775d] [receipt:59f52464] [receipt:f206e43f]: a bot is served only when it
   happens to stand inside the sphere.
7. **Sound.** `PowerupSound` (`health_recharge`, `com_item_gonkrecharge`, `vehicle_recharge`) is played as a one-shot
   at the station once per unit served per pass [V]; the health droid's `AmbientSound` `droid_fx7_idle_chatter` is
   asked whenever its handle is idle (re-ambience-music §5.2). The served unit plays its own `HealthGainEffect` /
   `AmmoGainEffect` (re-powerups §5.6) [V].
8. **Tips.** BF1 arms tooltip 10 "THIS IS A RE-SUPPLY DROID" when the local player is served; BF2 arms hint 15 "This
   droid will heal you when you stand near it.", 16 "Stand near this droid to refresh ammo." or 17 "R5 droids will
   repair nearby vehicles." for the local player inside the sphere, served or not [V]. (The PS2 frame by a BF2 health
   droid also shows the HUD's target bar "MEDICAL DROID" - card HTGT's element - and "△ CHANGE SOLDIER CLASS",
   which comes from the soldier's own class-change rule near a post, not from the station, §5.3.)
9. **The animation** (code-driven bones, both games): idle, the droid turns in place by 135-225 deg at
   `IdleRotateSpeed` (alternating direction), waits `IdleWaitTime`, turns again; the gonk rocks about its body bone
   (`IdleWobbleFactor`) six times per turn. Serving, it stops turning: the FX-7's two arms swing +-0.15 rad at 1 Hz and
   its equipment socket spins +-pi rad at 1 Hz; the gonk bobs down by up to `IdleWobbleFactor` m at 1 Hz with its feet
   on the ground. The animation runs only when a viewport sees the station; the give does not wait for it [V].

## 1. Method

Hops: the label table (`powerupstation` -> B1 class ctor `FUN_002aa5d8`, B2 `FUN_00368898`; `lead_tmp/pwup/NOTES.md`)
-> the class vtable -> the class setter (B1 `FUN_002aa820`, B2 `FUN_00368b60`), the instance setter (B1 `FUN_002a9648`,
B2 `FUN_003673b0`), the entity ctor (B1 `FUN_002a9490`, B2 `FUN_003671b0`) and the update (B1 `FUN_002a9750` via the
thunk `FUN_002aad70`, B2 `FUN_003674c0`) -> the gather and region tests -> the give functions -> the tips managers ->
the type ids' readers. Property names by FNV hash (`lead_tmp/veh0/fnv.py`). Then the census of placed instances
(`tools/census.py`), the models' bones (`tools/softcensus_stations.py`) and two live runs (`ps2/heal_live.py`).
Checks: `checks.sh` -> `checks.txt` (fn / word / writers / callers and the searches whose receipts §11 lists). Verifier: §11.

## 2. The classes and where they stand

### 2.1 The class fields [V]

| key (FNV) | B1 class offset (`FUN_002aa820`) | B2 (`FUN_00368b60`) |
|---|---|---|
| `PowerupDelay` 299acaba | +0x1640, default 0.5 (`FUN_002aa5d8` 236369) | +0x670, default 0.5 (363554) |
| `Person/Animal/Droid/Vehicle/BuildingHealth` f51ad206 / 60d746d7 / 9622c73b / 14e381c9 / a6c59e9f | +0x1644 + 4 x health type (236679, 236611, 236639, 236557, 236649) | +0x674 + 4 x type |
| `SoldierHealth` f33b41df | person AND droid (236673) | person and droid (363951) |
| `Person..BuildingAmmo` 738f412e / bfe8e397 / 0fda4dbb / e728d955 / a25f865f; `SoldierAmmo` c3da529f | +0x1660 + 4 x type (236635, 236625, 236566, 236667, 236630); Soldier = person and droid (236661) | +0x694 + 4 x type; Soldier (363927) |
| `SoldierEnergy` 0adc5145 and the five energies; the eight buff keys | - | +0x6b4 + 4 x type (363729); +0x6d4..+0x6f0 |
| `IdleRotateSpeed` ede68c8b / `IdleWaitTime` 044894ab | +0x167c / +0x1680 (236656, 236562) | +0x6f4 / +0x6f8 (363958, 363736) |
| `IdleWobbleNode` 1b5b62ea (+ its bind matrix) / `IdleWobbleFactor` 19e0afd9 | +0x16d0 (+0x1690..) / +0x16dc (236514, 236577) | +0x740 (+0x700..) / +0x74c (363759) |
| `IdleWobbleLeftFoot` 1c1b9c83 / `RightFoot` 6eefb998 | +0x16d4 / +0x16d8 (236600, 236508) | +0x744 / +0x748 |
| `ActiveRotateNode` 2c70cf18 (up to 4) / `ActiveSpinNode` 18275a73 | +0x16e0.. / +0x16f0 (236584, 236572) | +0x750.. / +0x760 (363821) |
| `PowerupSound` a2a034ea / `AmbientSound` 65a920ba | +0x16f4 / +0x16fc (236645, 236615) | +0x764 / +0x76c (363897, 363852) |

Health type = the low 3 bits of the unit's flags (re-powerups §2.2: 0 person, 1 animal, 2 droid, 3 vehicle, 4 building).
Everything else (`MaxHealth`, `GeometryName`, `DestroyedGeometryName`, `ExplosionName`, `MapTexture` / `MapScale`,
`Label`) belongs to the EntityBuilding base.

### 2.2 The disc values [V data] and the census [V data]

| class | PowerupDelay | gives | MaxHealth | PowerupSound | anim |
|---|---|---|---|---|---|
| BF1 `com_item_healthrecharge` | 1.0 | SoldierHealth 15 | 80 | health_recharge (+ Ambient droid_fx7_idle_chatter) | turn 1.5 rad/s, wait 1.75 s; arms pH_tester_arm10 / arm6, spin Equipment_Operator_Arm_Socket |
| BF1 `com_item_weaponrecharge` ("Ammo Dispenser") | 2.0 | SoldierAmmo 1 | 80 | com_item_gonkrecharge | turn 0.5 rad/s, wait 3.0 s; wobble gonk1 0.08, feet foot_l2 / foot_r1 |
| BF1 `com_item_vehiclerecharge` | 1.0 | VehicleHealth 400, VehicleAmmo 1 | 80 | vehicle_recharge | turn 1.5, wait 1.75; no bones named |
| BF2 `com_item_healthrecharge` | 1.0 | SoldierHealth 25 | 200 | as BF1 | as BF1 |
| BF2 `com_item_weaponrecharge` | 2.0 | SoldierAmmo 1 | 80 | as BF1 | as BF1 |
| BF2 `com_item_hero_healthrecharge` | 1.0 | SoldierHealth 60 | 80 | as BF1 | as BF1 (placed on no baked map) |
| BF2 `com_item_{health,weapon}recharge_unbuilt` | as built | as built (health 15) | 80 | | + `com_holo_*`, `com_prop_buildzone` (one on BF2 GEO1) |

Values from `engine/assets_baked/classes/bf1_common.ron`, `bf2_ingame.ron`; the RAM of both runs holds the same
delays and amounts per health type (`ps2/out/*_stations.json`: BF1 health [15, 0, 15, 0, 0], ammo [1, 0, 1, 0, 0],
vehicle health [0, 0, 0, 400, 0] / ammo [0, 0, 0, 1, 0]; BF2 health [25, 0, 25, 0, 0]) [M]. No station class authors
an energy or buff key. BF2 has no vehicle droid model on its disc (`softcensus.txt`).

Placed (`tools/census.py` over the baked manifests, `census_showcase.txt`, `census_all.txt`):

| map | health | ammo | vehicle | total |
|---|---|---|---|---|
| BF1 NAB1 | 5 | 5 | 5 | 15 |
| BF1 TAT1 (`tat1`) | 4 | 5 | 2 | 11 |
| BF1 YAV2 | 9 | 9 | - | 18 |
| BF2 UTA1 | 7 | 10 | - | 17 |
| BF2 MYG1 | 5 | 7 | - | 12 |
| BF2 HOT1 | 9 (+6 in layer `hoth_objective`) | 9 (+9) | - | 33 (18 in conquest) |

Every placed station (967 over the 32 baked maps) has `Radius` 5.0 (vehicle droids 15.0, 20.0 on some), no
`EffectRegion`, Team 0; vehicle droids only on BF1 maps. Live, the RAM held exactly the conquest set: NAB1 15 (5 / 5 / 5,
radii 5 / 5 / 15), HOT1 18 (9 / 9) [M].

## 3. The instance: the region [V]

The instance setter (B1 `FUN_002a9648` 235482-235515, B2 `FUN_003673b0` 362284-362318): `Radius` (0dba4cb3) > 0 and no
region yet -> a new region object from the station's world matrix with radius = the value and shape `sphere`
(afd98518), at entity+0x2dc (B2 +0x2ac); `EffectRegion` (a4f267b8) -> the named world region instead. The region
copies the matrix (centre = +0x30) and the radius (+0x4c) (B1 `FUN_004c2318`, sphere = type 1 at 584327). The test
(B1 `FUN_004c2b08` case 1, 584706-584716; B2 `FUN_00536410`): |p - centre|^2 < radius^2, strict, 3-D. The centre is
fixed when the instance is read: the station's later idle turns do not move it. `EffectRegion` overwrites the slot
with no "already set" test (a name that matches no region leaves 0, and then the station serves nobody); no disc
instance authors it (§2.2).

## 4. The give pass [V, M]

B1 `FUN_002a9750`, B2 `FUN_003674c0`, every frame:
```
if building state (B1 +0x2a4, B2 +0x274) != 0: return                     # 235625; 362442
ambient sound handle                                                      # 235630; 362446
timer (+0x2e0 / +0x2b0) += dt
if timer >= class.PowerupDelay:                                           # 235637-235642; 362451-362458
    timer -= PowerupDelay
    list = up to 64 entities whose origin passes the region test          # FUN_002aa4c0 / FUN_00368780
    for e in list:
        if e.controllable() == null: continue                             # slot +0x68 (B1) / +0x78 (B2): soldier or vehicle
        t = e.flags & 7                                                   # +0xac / +0x228
        gave  = give_health(class.health[t], e)                           # B1 FUN_002ab498; B2 FUN_00369770 (hero refusal)
        gave |= give_ammo(class.ammo[t], e, skip_dispensers = 0)          # B1 FUN_002ab6a0; B2 FUN_003699e8
        BF2: gave |= give_energy(class.energy[t], e) | give_buffs(class's 8 numbers, e)
        if gave: PowerupSound one-shot at the station; BF1 tooltip 10 (§5.2)
        BF2: hints 15 / 16 / 17 for the local player (§5.2)
    state: served anyone -> 3 (unless 2 or 3); nobody -> 1 (if 2 or 3)    # 235686-235697; 362531
if no viewport sees the station (B1 60.0, B2 60.0 / 70.0): return         # 235700; 362542
animation (§6)
```
- The gather: B1 `FUN_00161c00(radius, centre, list, 64, the station, mask 0xe)` (236313), B2
  `FUN_00161a98(..., mask 0x1a)` (363496), then the region test on each entity's matrix origin (236318 / 363501).
- Health (re-powerups §5.3): amount != 0 and (health < max or amount < 0) -> min(health + amount, max). The amount is
  per pass. BF2 adds the regeneration-rate >= 0 condition and, the station calling with the bypass flag 0
  (ELF 0x3675d0), refuses a class with bit 33 of class+0x330 (the SetHeroClass hero; ELF 0x369774-0x36978c).
- Ammo (re-powerups §5.4): every weapon of every weapon holder, reserve + amount clamped to [0, max - 1 if the clip
  holds a round]. The station passes 0 as the dispenser-skip flag (ELF 0x2a989c; the item passes 1, 234852): a
  station refills a dispenser weapon too.
- The pass's clock is the station's, so a unit that arrives gets its first give within (0, PowerupDelay] s. All of a
  map's stations start at 0 together (live: every timer at 0.009 s of 1.0, 1.009 of 2.0 in both runs [M]). One
  PowerupDelay is subtracted per frame, so at most one pass a frame; a broken station's timer freezes.
- No team field is read anywhere in the pass, of the unit or of the station, nor in the gathers [V, absent; both
  verifiers] [receipt:c5b8ec07] [receipt:e140c9b5].
- The gather's broad phase is an axis-aligned cube of half-side = radius around the centre (B1 `FUN_00162a98`), then the
  exact sphere test; BF2's gather callback (0x162a58) adds a squared-distance test of its own [V].
- BF2's differences from BF1 (both verifiers' twin reading): the update first calls `FUN_003d7a20(station, &dt)`
  (362437), a network-game gate that can return early and replace dt; the health give also needs the unit's
  regeneration rate (+0x178) >= 0 and refuses the SetHeroClass hero; energy and buffs; the hints (§5.2) outside the gave
  test; the visibility gate's second distance [V].

Live (`ps2/heal_live.py`, `ps2/out/*_results.json`, `*.csv`; the player's health written once, then sampled every
~4 ms by the EE clock) [M]:

| run | distance (3-D) | health | steps |
|---|---|---|---|
| BF1 NAB1 health droid | 1.37-1.81 m | 75 -> 300 / 300 | 15 x +15, every 1.0009-1.0011 s |
| BF1 | 4.45-4.56 m | 150 -> 210 in 4.5 s | 4 x +15 |
| BF1 | 5.43-5.55 m | 150, 4.5 s | none |
| BF1 ammo droid | 2.01-2.04 m | reserves 0/0/0 -> 4/3/1 (max clips 5/4/2) | +1 to each weapon every 2.0 s, each to max - 1 |
| BF2 HOT1 health droid | 2.00 m | 100 -> 400 / 400 | 12 x +25, every 1.001 s |
| BF2 | 4.60 m | 200 -> 300 | 4 x +25 |
| BF2 | 5.34 m | 200 | none |

The 1.001 s step is 60 game frames of 1/60 s on the NTSC 59.94 Hz clock: one second of game time [D].

## 5. Feedback

### 5.1 Sounds and effects [V]
`PowerupSound` once per served unit per pass, at the station (B1 235658-235669, the station's own vtable +0xc
position; B2 362494). The health droid's `AmbientSound` (re-ambience-music §5.2). The unit's gain effects are the give
functions' (re-powerups §5.6: B1 `FUN_001ffcc0` after an ammo gain, the health-changed handler's `HealthGainEffect`).
The baked sound tables carry the three PowerupSounds and the chatter (by FNV hash in `<map>/sounds/sounds.ron`).

### 5.2 Tips [V]
- BF1: inside the gave branch, when the tooltip manager `DAT_0056449c` still has tooltip 10 pending and the served
  unit is the local player's, `FUN_002dbcf0(mgr, 10, 1)` (235670-235679). Tooltip 10 = `game.tooltips.approach_health_ammo`
  "THIS IS A RE-SUPPLY DROID" (`FUN_002db988` case 10, 266129-266130). ("Pending" is `FUN_002dbf00`'s answer: the
  tooltip's shown flag clear in the default mode; 1 always in the manager's +0x3a4 mode.)
- BF1, a broken station: the HUD function `FUN_002d4b50` (262130-262190), for the EntityBuilding under the player's
  reticle in building state 1 or 2 (destroyed or unbuilt), arms tooltip 9 "BUILD A MEDICAL DROID HERE"
  (`approach_unbuilt_health`) when its kind is a station (slot +0xd4 == 17, 262158) with a health amount, 8 "BUILD AN
  AMMO DROID HERE" (`approach_unbuilt_ammo`) with an ammo amount, and 7 "USE A FUSIONCUTTER TO BUILD HERE" for any other
  building (strings 0x540eb8 / 0x540e90 / 0x540ee0, `FUN_002db988` cases 9 / 8 / 7). [H] the held-weapon test before
  it (`DAT_005894e0`) is the fusion cutter's.
- BF2: for every gathered unit, served or not, that is the local player's, while hint 15, 16 or 17 of `DAT_00661358`
  is pending: hint 15 if the class gives his health type health, 16 if ammo; hint 17 instead when the unit is one of
  three types (`DAT_006894f8`, `DAT_00689960`, `DAT_00689368`; [H] the vehicle types) (362496-362524). The hint table
  (0x5ff3d0, 32 x 0x20, `FUN_00393fd0`): 15 "This droid will heal you when you stand near it.", 16 "Stand near this droid
  to refresh ammo.", 17 "R5 droids will repair nearby vehicles." Each entry holds (2.0, 0, 8.0, 40.0, 5.0, 2.0, bytes
  4, 0x80) before the text hash; [H] delay / show / cool-down times and a 4-shows cap (`FUN_00394500` compares a count
  with the byte 4). `FUN_003943b0` arms a hint only offline (`DAT_00600548`, or `DAT_00600549` when `DAT_0060054f`, is 0)
  and while the manager's +0x39e is set. The port has no tip system yet (§10).

### 5.3 What the PS2 frame shows [M]
`ps2/out/bf2_heal_2m.png` (the end of the BF2 heal run): the target bar "MEDICAL DROID" at the top (the HUD's target
element on the droid the reticle is on; card HTGT) and "△ CHANGE SOLDIER CLASS". That prompt is BF2 hint 5 ("{useButton}
change soldier class", entry 5 of the same table), armed by the HUD update `FUN_003b4668` (410611) when the player's
soldier has bit 17 of +0x4e0, which the soldier update `FUN_00248c38` sets near a post his team can spawn at
(163785) or near a same-team object within its class's +0x338 distance (163841) [V]; the station is team 0, so the
prompt in that frame is the nearby post's, not the station's [D]. `ps2/out/bf1_heal_2m.png`: no text by the BF1 droid
at the end of the run.

## 6. The animation [V]

State +0x2e4 (B2 +0x2b4), timer +0x6f4 (B2 +0x6c4), spin sign byte +0x6f8 (ctor 1), wobble frequency +0x6fc:
- **1, wait**: timer -= dt; below 0 -> state 0, the sign flips, T = (0.75 + 0.5 u) pi / IdleRotateSpeed (u uniform
  from the generator `DAT_0066eec8`), wobble frequency = 12 pi / T (235742-235763; B2 362610-362625). IdleRotateSpeed
  0 -> no turn.
- **0 (and 2), turn**: timer -= dt; below 0 -> state 1 (from 0) / 3 (from 2), timer = 0, then (entering 1) timer =
  IdleWaitTime (236116-236123, 236292). Each frame the whole station turns by sign x IdleRotateSpeed x dt about +Y
  (236129; axes `FUN_00445fa8`: `DAT_0066eef0` = X, `DAT_0066ef00` = Y, `DAT_0066ef10` = Z) - so one turn is 135 to
  225 deg. The wobble bone rotates about Z by IdleWobbleFactor x sin(f x timer) (236165). The feet: 0.2 x that angle is
  computed but never reaches the bones (ELF 0x2aa280-0x2aa30c writes the bone's own row back): in idle the feet keep
  their pose.
- **3, serving**: timer -= dt (unbounded); the wobble bone's local y += IdleWobbleFactor x (cos(2 pi timer) - 1) / 2
  (235813: down 0..0.08 m at 1 Hz on the gonk); the feet's local y = 0 (ELF 0x2a9b58-0x2a9bb8); each ActiveRotateNode i
  rotates about X by 0.15 sin(2 pi (timer - i)) - for integer i every arm in phase (235944); the ActiveSpinNode about
  Z by pi sin(2 pi timer) (236067). No body turn.
- State 2 is never written (the writes: 0 ctor, 1 / 3 pass, 0 wait end, 1 / 3 turn end) [V, absent] [receipt:c5b99960]: a served station
  goes to 3 at once; when a pass serves nobody, 3 -> 1, whose timer is already below 0, so the next frame starts a turn.
- sin / cos are a 2048-entry table built from fdlibm sinf / cosf (`FUN_00445208`).
- The animation needs a viewport to see the station (B1 `FUN_0019fc10(60.0, ...)` walks the viewports' cameras, B2
  `FUN_001ba840(60.0, 70.0, ...)`; [H] the numbers are a distance cut). Off screen the animation's timers and its 0 / 1 / 2
  / 3 timeouts stop too; only the give pass's 3 / 1 changes go on [V].
- The table sine and cosine interpolate between 2048 entries truncating toward zero, so for the negative timer of
  state 3 the value is close to, not exactly, the true sine [V; invisible].
The models split exactly into rigid parts per bone: FX-7 `bone_root` 542 v, the two arms 37 v each, the socket 96 v; the
gonk `gonk1` 120 v, each foot 50 v; 0 triangles across bones, all weights 255/0/0/0 (`softcensus.txt`) [V data].

## 7. The building state [V]

The station is an EntityBuilding (IsA chain PowerupStation -> EntityBuilding -> EntityGeometry -> GameObject ->
EntityEx -> Entity, `FUN_002ab208`; the names from the type-id inits `FUN_00444b88(0x5842c8, "EntityBuilding")` 67634,
0x584a08 "EntityGeometry" 93700, 0x587040 "GameObject" 136314, 0x5842a0 "EntityEx" 66225, 0x584298 "Entity" 66224). Its state +0x2a4: the ctor `FUN_001bf400` sets 0, or 2 when the class has an
unbuilt object (class+0x156c, 66399-66404); the kill `FUN_001bfb88` sets 1 (the destroyed geometry) or 2 (66696-66729);
`FUN_001c00c8` sets 0 again (66885-66888). The station inherits both through its vtable thunks (`FUN_002ab060`,
`FUN_002ab080`). Only state 0 gives, sounds and animates.

## 8. Bots [V, absent] [receipt:59f52464] [receipt:f206e43f]

No bot code in either game looks for, picks or walks to a station [V, absent; two independent verifiers, §11]:
- The type ids (B1 `DAT_00589a48` / `DAT_00589a50`, B2 `DAT_0069bd28` / `DAT_0069bd30`; FNV of "PowerupStation" /
  "PowerupStationClass", 0xa3097771 / 0xfa20affb, held nowhere else in either ELF [receipt:46c4f462]) are loaded only by
  their getters (widescan: B1 0x2ab290 / 0x2ab2b0 / 0x2ab3b0 / 0x2ab3d0, B2 0x3695a8 / 0x3695c0 / 0x369670 / 0x369688),
  reached only from the classes' own IsA functions and one vtable word each [receipt:87b3c7e4] [receipt:05cb775d]
  [receipt:6a21e419] [receipt:5ed4a84b].
- The entity KIND code (B1 slot +0xd4 returns 17 at 0x2ab2c8; B2 slot +0xe0 returns 18 at 0x3695d8) is compared with
  the station's value only in HUD and network code: B1 `FUN_002d4b50` (the tooltips 7 / 8 / 9 of §5.2), `FUN_00306270`
  (a per-kind distance cut under the packet writer `FUN_0030fac8`), `FUN_0030db48` (the NetObj records, built in single
  player too; a station only steps a counter), the range-test helper `FUN_00304750` (two callers, both packet writers)
  [receipt:59f52464]; B2 `FUN_003de218` (per-kind priorities under `FUN_003ed650`), `FUN_003ea6c8` (the NetObj list
  under `FUN_003d5d20`), `FUN_003de018` (no caller, no data word: dead), the helper `FUN_003d4758` (12 callers, all
  bit-stream writers or behind the network-session flags `DAT_00600548` / `DAT_00600549` / `DAT_00600560`; the one
  call from AI code, `FUN_003d7a20` from `FUN_002b0548`, is a network-authority check on the bot itself and returns
  0 offline) [receipt:f206e43f]. The AI's own kind compares are with 1, 2, 3, 4, 5 and 7 (a verifier's census).
- The station-only fields (B1 class+0x1640 with entity +0x2e0, B2 +0x670 with +0x2b0) are read outside the update only
  by the station's own network write / read slots (B1 `FUN_00333348` / `FUN_003333e8`, B2 `FUN_00415e68` / `FUN_00415f08`).
- re-soldier-ai §4.8: no walk to a supply droid in BF2's AI.
So a bot that stands inside the sphere (defending a post, passing by) is healed and re-supplied like the player; none
goes there for it. [H] BF2's engineer repair agent (`FUN_002d6828`, friendly or neutral entities within 75 m that
lack health) may pick a DAMAGED station to repair it - repair, not service; not checked.

## 9. Not in this pass

The vehicle side (which bodies mask 0xe / 0x1a gather besides soldiers; a vehicle droid healing a hover) [H]; the hint
manager's timers (§5.2) [H]; BF2 hint 17's three types [H]; what the 60 / 70 m gate tests exactly [H]; damage to a
station (who shoots one, the explosion) and the BF2 engineer's build / repair; BF2 ammo / energy not measured live.

## 10. The port today, and the build

Today the stations are drawn (COMMON bake, one node each) and do nothing: no code reads the class
(`grep -ri powerupstation engine/crates` is empty) [receipt:73f499eb]. The build, in commits:
1. **Data** (`game::roster` / a station class from `classdb`): per station class PowerupDelay, the per-health-type
   health / ammo (/ BF2 energy) amounts, the sounds, the anim keys; per placed instance its Radius (EffectRegion:
   none on the discs) - only the stations of the loaded layers (HOT1's objective layer is not conquest).
2. **Sim** (`core/src/sim/stations.rs`, step 5c beside the pickups): per station a timer; at >= PowerupDelay (keep
   the remainder) one pass over the living units with origin |feet - centre| < Radius, not seated (a rider is not in
   the world; a vehicle gets its own pass when the class has a vehicle amount), health to the cap and clips to every
   weapon's cap with the pickup's give functions (no dispenser skip), any team; `Event::StationServed {station, unit,
   health, ammo}`. Sim hash moves (declare `sim; sweep; fidelity`).
3. **Draw** (bake: the station models through `bake/src/turret_rig.rs`'s rigid per-node parts; game: §6's state
   machine per station driven by the served events, its own RNG, not the sim's), the idle turn, wobble, arms, spin.
4. **Sound** (`PowerupSound` at the station per served unit, the chatter loop) and the gain effects (the `ItemTaken`
   path's `HealthGainEffect` / `AmmoGainEffect`).
5. **Tips** - BF1 tooltip 10 / BF2 hints 15-17 need a tip system the port does not have: its own card.
Referee for the build: a hurt soldier standing 2 m from a health droid gains +15 (BF1) / +25 (BF2) each 1.0 s to his
max; at 4.6 m he is served, at 5.4 m he is not; an enemy is served too; an ammo droid gives +1 clip to every weapon
each 2.0 s to max - 1; a destroyed station gives nothing (once stations can break).

## 11. Checks run (`lead_tmp/pwst/checks.txt`) and the verifier

1. **fn** - every cited decomp line through `re_check.py fn` (`checks.sh`, 160 lines): each in the function named; the
   first draft's tooltip line (266125, case 9) was corrected to 266129-266130.
2. **absent** - receipts: type ids [receipt:87b3c7e4] (B1), [receipt:05cb775d] (B2); their getters and IsA words in the
   ELFs [receipt:6a21e419], [receipt:5ed4a84b]; the name hashes [receipt:46c4f462]; the kind helpers [receipt:59f52464]
   (B1), [receipt:f206e43f] (B2); state 2 [receipt:c5b99960] (the three hits are other classes' +0x2e4); the team words
   [receipt:c5b8ec07] / [receipt:e140c9b5] (weak alone: the team claim rests on the two verifiers' reading of the loops
   and the give functions); the port [receipt:73f499eb]. `tools/kindscan.py` (`kindscan.txt`): every function that
   calls the kind slot and compares with 17 / 18 - it cannot see a helper handed the kind (the verifiers found two).
3. **twin** - BF2's functions found from the label `powerupstation` and the same property hashes; the update, setters,
   gather and gives read side by side; the differences are listed in §4 (network gate, regeneration test, hero refusal,
   energy / buffs, hints, the gate's second distance, the gather callback's own distance test).
4. **word** - class defaults 0.5 (B1 0x2aa610 / 0x2aa628, B2 0x3688d0 / 0x3688e8); 64, mask 0xe / 0x1a; the give
   arguments (B1 0x2a9880-0x2a989c, B2 0x3675d0-0x36760c); the hero test 0x369774-0x36978c; the apply flags
   0x2ab4a8 / 0x2ab6a4 / 0x3699ec; tooltip 10 0x2a990c; hints 15-17 0x3676cc / 0x3676e8 / 0x3676fc and the table words
   0x5ff5b0.. ; 60.0 / 70.0 (0x2a99b8, 0x367898); the animation constants 2 pi, 0.5, 0.15, pi, 0.2, 0.75, 12 pi
   (37.699112 = 0x4216cbe4); the kind returns 0x2ab2c8 / 0x3695d8; the strings.
5. **writers** - the axis vectors (`FUN_00445fa8` only), the type ids, the two tip managers, the building state's
   writers (§7), the animation state's writers (§6).
6. **Verifier** (workflow `pwst-re-verify`, 5 agents at medium effort: three batch verifiers on 23 claims, two
   independent verifiers on the 5 absence / twin claims; then `pwst-bots-verify`, 2 independent refuters on the
   revised bots claim). Batch claims: 22 SUPPORTED, 1 PARTLY (the IsA chain's last four names unchecked by the
   verifier - added from the type-name inits, §7). Absence / twin, verifier 1 / verifier 2: no team read S / S; state 2
   never written S / S [receipt:c5b99960]; B1 bots by type id P / P and B2 P / S (both found the kind code and its registry, which the claims
   missed); the BF2 twin P / P (three differences unlisted). The second verifier overturned one verdict (B2 bots). The
   revised bots claim (§8): P / S - both found no AI path; both added the two range-test helpers; one marked
   `FUN_003de018` dead and `FUN_0030db48` single-player too. Results: `verify_results.json`, `verify_bots.json`.
   What the verifiers caught that this pass had not: the kind code (17 / 18) and every use of it; the tooltip line; the
   network gate and the regeneration test in BF2's update; that off-screen the animation's timeouts stop too; that
   `EffectRegion` overwrites; the engineer-repair question.

## 12. What stayed [H]

- Which bodies the gathers' masks (B1 0xe, B2 0x1a) cover besides soldiers: the vehicle droid serving a hover is read
  (VehicleHealth on type 3) but not watched.
- The 60 / 70 m visibility gate's exact test; the tip managers' timers (the hint entry words); BF2 hint 17's three types.
- A corpse inside the sphere: no alive test in the pass (health < max is the only gate); whether a dead soldier is still
  gathered was not checked. The port serves the living only.
- BF2's engineer repair of a damaged station (§8); who damages a station (MaxHealth 80 / 200) and its explosion.
- BF2 ammo / energy at a station not measured live (read; BF1's ammo measured).

## 13. Port destruction follow-up (2026-10-26)

The maintainer reported that the FX-7 and gonk on BF1 Bespin Platforms could not be destroyed. The original PWST implementation deliberately omitted destruction (its source named PWSTD); the class loader omitted MaxHealth, DestroyedGeometryName and ExplosionName, station instances had no health, and both projectile targets and explosion victims admitted only soldiers and vehicles.

Original evidence reused [O, UNAUDITED]: sections 2.2 and 7 above, plus `re-build-zones.md` section 1. The EntityBuilding base defaults to HealthType Building. Its death handler switches to the authored destroyed geometry, and a station's update returns while the building state is nonzero. The BF1 death function `FUN_001bfb88` was reread directly in the local original executable's decompilation: its early state guard prevents a second death; it selects the destroyed geometry, updates collision, then calls `FUN_002701a0` for the class's ExplosionName at the building's origin with the building's own entity identity. Accordingly the port's station explosion has the station's team and no soldier owner; the shot that destroys it remains recorded as the destruction event's killer.

The port now loads the disc health/type, destroyed geometry and explosion. Each station's manifest instance is bound to its collision bodies; a predictive bolt remembers the station index and applies the ordinary damage descriptor and BuildingScale. Explosions gather intact stations and apply the existing falloff and terrain/world visibility rules, excluding the target station's own bodies from its visibility ray. Neutral stations remain damageable with friendly fire disabled. A dead station clamps health to zero, stops service, freezes animation, stops its current ambient take and emits one StationDestroyed event. The authored wreck collision is installed before its death blast, whose visibility ray excludes that source instance. The battle view omits all live model/LOD parts and draws the existing wreck model. Health, destruction and motion state enter the simulation hash; station projectile targets have distinct hash tags.

Scope and remaining limits: this implements destruction of already built stations. Fusion-cutter repair/rebuilding and initially unbuilt station activation remain separate work; those weapon/build systems were not added here. The initial bounds-derived blast sphere and render-only idle yaw were replaced after the live comparison in section 14. Existing GLB sphere metadata now carries the original common station model spheres; no new asset format is introduced.

## 14. Live PS2 fidelity check (2026-10-26)

The maintainer pointed to the running emulator in the project's `emulator` junction, targeting `../downhill-cc/emulator`. PCSX2 2.8.2 was running BF1 USA, serial `SLUS-20898`, CRC `503BF9E1`, on Bespin Platforms. The initial paused battle was preserved in state slot 9; the pre-existing slot 1 was left intact. PINE snapshots and the comparison script stay local under `engine/artifacts/lead_tmp/station-fidelity/`.

[V-RAM] `baseline.ram` contains 30 station entities, found through station vtable `0x53eb78` and validated through class vtable `0x53ea48`. The 22 medical/ammo instances all have maximum health 80. Two were already destroyed in the maintainer's initial battle: medical entity `0x1c567e0` at `(114.517, 98.972, 182.409)` and ammo entity `0x1c57940` at `(113.193, 99.012, 184.003)`, both health 0 and building state 1 at entity +0x2a4. Live entities have state 0. This observes native destroyed objects; it is not a controlled count of shots needed to destroy one.

The original COMMON.LVL model SPHR values and the paused game's world-space entity spheres agree exactly in radius, and within 0.00005 m in transformed centre, for all 22 medical/ammo entities. Intact and destroyed model-space measurements:

| Model | Centre x, y, z (m) | Radius (m) |
| --- | --- | --- |
| Medical | 0.16547057, 1.10058284, -0.07259896 | 1.38141489 |
| Medical wreck | -0.22346294, 0.38546023, -0.19447792 | 1.32047153 |
| Ammo | 0, 0.72729754, 0 | 0.82304257 |
| Ammo wreck | 0.00046858, 0.24494109, 0.07472894 | 0.67834860 |

RAM fields used: class at entity +0x2d8, current/max health at +0x8/+0xc, matrix at +0x200, sphere cell/index at +0x100/+0x104, cell entry at cell +0x40 +16*index. The original root matrix and off-centre medical sphere change with idle rotation. [O] `FUN_002a9750` calls the entity matrix setter after its idle yaw update. The port now runs that clock in `core::sim::stations::motion`, moves the station's collision bodies, and transforms the model sphere through the same pose. The viewer reads that simulation motion, and destruction preserves the final yaw for both wreck geometry and collision. Motion has its own deterministic generator and is hashed; it does not consume the soldiers' shared generator.

[O] BF1 EntityBuilding death handler `FUN_001bfb88` updates destroyed geometry/collision before calling explosion `FUN_002701a0`. The port follows this order and excludes the dying source instance from its blast's visibility rays. A regression checks that the first wreck can block a second station's chained blast. Original damage path `FUN_001bc190` applies the damage descriptor's health-type scale; the port uses its existing shared damage implementation. BF2 health 200/ammo 80 and authored wreck/explosion properties remain checked against its disc data, not live BF2 RAM in this comparison.

Verification: `compare.py` checks the local RAM against the rebaked model spheres; `game/tests/suite/station_destruction.rs` checks the measured BF1 values, moving collision, frozen wreck transforms, both games' health and actual collision-hit destruction; the GPU check reads back intact and destroyed draws. Both affected common asset groups were rebaked. The previous bake-input stamp was first checked against unchanged inputs plus the HEAD versions of the three common-metadata edits, then advanced after both common bakes succeeded.

Remaining limits: attempted mouse firing through emulator input automation did not produce a verified native damage sequence, so no native shots-to-destroy result is claimed. The port still steps animation at 30 Hz and animates off-screen stations, whereas the original has a visibility gate and updates per rendered frame. The existing predictive-projectile model and general explosion gather ordering remain port-wide approximations. Repair/rebuilding remains outside this destruction change.
