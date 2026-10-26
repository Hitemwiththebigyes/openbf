# OPUS RE (2026-10-05) - [O] UNAUDITED

# The soldier animator: weapon pose sets, the two body layers, firing on the move (BF1 and BF2)

Card ANIM, lane 2. Corpora: B1 = `analysis/SWBFRONT.ELF-decomp.c`, B2 = `analysis/SWBFRNT2.ELF-decomp.c`; ELF words
by `off = va - 0x160000 + 0x1000`; the discs' `zaa_` banks; 151 BF1 and 178 BF2 PCSX2 save states
(`engine/artifacts/pcsx2/ref/states/`). Tags: **[V]** read at the cited lines, **[M]** measured in PS2 memory or on
the disc, **[D]** derived, **[H]** hypothesis; `[O]` marks the load-bearing claims of this unaudited note. Scratch,
tools and tables: `engine/artifacts/lead_tmp/anim/`. Builds on `re-anim-banks.md` and `re-anim-keys.md` (bank and
key formats) and `re-foley.md` 4.4 (the movement-id chooser).

## 0. Summary

The maintainer's three asks of 2026-10-04, answered from the PS2:

1. **Each weapon has its own way of being held, and it changes only the upper body.** `[O]` A soldier is posed
   by two clip players. The LOWER one drives the hips and legs and - whatever he carries - plays the RIFLE's
   stand / walk / run clip. The UPPER one drives 13 bones (spine, ribcage, neck, head, both arms) and plays the
   carried weapon's version of the same clip: the launcher on the shoulder, the pistol and tools in one hand, the
   droids' own carry. The weapon class names its set with `AnimationBank`. BF1 has 9 fixed sets (rifle, bazooka,
   pistol, grenade, tool, sabre, sbdroid, bdroid, lascannon) with a fallback table; BF2 builds a set per
   (skeleton family, weapon animation class) and fills gaps along parent chains (pistol -> tool -> rifle; any
   family -> human).
2. **The super battle droid has its own set.** `[O]` BF1: weapon set `sbdroid` (the wrist blaster's
   `AnimationBank`), 25 `human_sbdroid_*` clips: its idle, walk, run, a 7-frame shoot and a reload per stance.
   BF2: family `sbdroid`, 25 `sbdroid_rifle_*` clips in CIS.LVL. In both the legs still play the human rifle clips.
3. **Shooting does not stop the walk.** `[O]` While a shoot, charge or reload clip runs, the upper player plays
   it and the lower player keeps the movement clip. Only a soldier standing still plays the action with his legs
   too. Every shot restarts the shoot clip, which blends in over 0.01 s; a reload blends over 0.5 s. No shoot
   clip is drawn during a jump, fall, roll or (BF2) sprint; it is drawn over the jet hover and BF2's soft landing.

Also read: the blend rule (BF1 pulls a kept pose toward the clip by `t / T`; BF2 crossfades linearly, with its
times in a disc chunk, `sanm`); the alert state (2.0-2.5 s after firing or while a target is held: alert clips
and the torso turned onto the aim line; otherwise only the head looks); the aim pose's bones and limits; strafing
(neither game ships standing strafe clips: the soldier runs forward or backward with his legs twisted up to 72
degrees); the turn on the spot; the pool of 10 full animators.

How it was checked: both games' set tables were rebuilt from the discs by a script and compared with the PS2's
memory - **BF1 979 of 979 slots, BF2 665,176 of 665,176 slots over 176 battle states**; the clips on the two
players of every soldier posed in the reference states match the rule - **BF1 155 of 155, BF2 182 of 182** -
including five soldiers caught firing on the move; BF2's blend table in memory equals the disc's - 258 of 258
records.

The port today (`game/src/animator.rs`, `bake/src/soldier.rs`): one player, 34 `human_rifle_*` clips for every
soldier, a 0.15 s crossfade, shoot and reload only while standing still. Section 9 is the change list.

## 1. Names and hashes [V]

Two hashes are in play, and the older notes blur them:

- **Bank, rig and weapon-set names: FNV-1a over byte|0x20.** `FUN_00444b88(out, str)` (B1 `==== 00444b88`) calls
  `FUN_00444b30`, the ODF hash (`0x811c9dc5`, `* 0x1000193`). The global animation registry
  at `0x6ff2d8` is keyed by it: the six soldier banks' keys `DAT_005871b0..c4` are filled by `FUN_0022f900` with
  `FUN_00444b88` over `human_animation_0..4` and `human_animation_alert` (ELF 0x535410-0x535488), and the weapon
  class's set name is matched against nine FNV words in .data (`DAT_005182c0`, section 4).
- **Clip names and bone names: CRC-32** (`FUN_00444bb8`, table `DAT_00521ca8`, `re-anim-banks.md` 2.3).

Erratum for `re-anim-keys.md` 1.1 ("they CRC the string (`FUN_00444b88` -> `FUN_00444bb8`)"): `FUN_00444b88` is
the FNV wrapper. Nothing in that note's decode depends on it.

## 2. BF1: the animation set (`DAT_00517ae8`, 0x16a0 bytes) [V][M]

Built once by `FUN_0022d828` (B1 144173-144886; `FUN_0022d478` allocates it). One global object serves every
soldier class. Its clip tables hold pointers to 0x14-byte **anim records** made by `FUN_0022d2e8` (B1 143978-143992):

| +0 | +4 | +8 | +0xc | +0x10 |
|---|---|---|---|---|
| the bank's clip record (`re-anim-keys.md` 1.2) | f32 root speed along x | f32 root speed along z | loop flag | u8 = (int)(blend seconds x 100) |

### 2.1 The tables

`s` = stance 0 stand, 1 crouch, 2 prone (`PTR_s_stand_00517ba8`). `a` = one of 13 movement actions
(`PTR_s_idle_emote_00517b70`): 0 idle_emote, 1 idle_checkweapon, 2 idle_lookaround, 3 turnleft, 4 turnright,
5 walkforward, 6 walkbackward, 7 runforward, 8 runbackward, 9 walkright, 10 walkleft, 11 runright, 12 runleft.
`w` = one of 9 weapon sets (`PTR_s_rifle_00517bb8`): 0 rifle, 1 bazooka, 2 pistol, 3 grenade, 4 tool, 5 sabre,
6 sbdroid, 7 bdroid, 8 lascannon. `u` = one of 6 upper actions (`PTR_DAT_00517be0`): 0 idle, 1 shoot, 2 shoot2,
3 shoot3, 4 charge, 5 reload. `i` = one of 31 special animations (`PTR_DAT_00517af0`, listed in
`re-anim-banks.md` 2.4; 0x1e repeats `stand_death_backward`).

| table | offset | size | clip name | format string | built at |
|---|---|---|---|---|---|
| `base[s][a]` | +0x30 + s*0x34 + a*4 | 3 x 13 | `human_rifle_<stance>_<action>` | 0x535258 `human_rifle_%s_%s` | 144367-144404 |
| `special[i]` | +0xcc + i*4 | 31 | `human_rifle_<special>` | 0x535248 `human_rifle_%s` | 144330-144366 |
| `W[s][w][a]` | +0x148 + s*0x1d4 + w*0x34 + a*4 | 3 x 9 x 13 | `human_<weapon>_<stance>_<action>` | 0x535270 `human_%s_%s_%s` | 144476-144486 |
| `S[w][i]` | +0x6c4 + w*0x7c + i*4 | 9 x 31 | `human_<weapon>_<special>` | 0x535290 `human_%s_%s` | 144551-144566 |
| `ALERT[w][a]` | +0xb20 + w*0x34 + a*4 | 9 x 13 | `human_<weapon>_stand_<action>_alert` | 0x535280 `%s_alert` on the `W` name | 144487-144506 |
| `U[s][w][u]` | +0xcf4 + s*0xd8 + w*0x18 + u*4 | 3 x 9 x 6 | `human_<weapon>_<stance>_<upper>` | 0x535270 | 144519-144541 |

A name is looked up by CRC in the six banks in the order `human_animation_0, 1, 2, 3, 4, alert`
(`FUN_0022f888`, B1 145544-145565: slots at +0xf7c); a miss leaves the slot 0. The bazooka (w = 1) gets no
`_alert` lookup: its alert row is a copy of its plain stand row (144488-144490).

Blend seconds and loop flag per record:

| records | blend | loop |
|---|---|---|
| movement (`base`, `W`, `ALERT`) | 0.15, prone 0.4 (ELF words `0x3e19999a`, `0x3ecccccd`; 144376-144379, 144457-144461) | 1, except turnleft / turnright (`1 < a - 3U`, 144392) |
| upper `shoot`, `shoot2`, `shoot3` | 0.02 in the code (`0x3ca3d70a`, 144532-144535) - **the stored byte is 1**, 0.01 s, because 0.02 x 100 truncates [M] | 0 |
| upper `idle`, `charge`, `reload` | 0.5 (`0x3f000000`) | 0 |
| special `i` | the pair table `DAT_005354a0` (flag, seconds): jump 0.4, landsoft / landhard 0.05, fall 0.8, thrown_flail 0.5 loop, tumbles 0.1 loop, bounces 0.1, thrown lands 0.4, jetpack_hover 0.15 loop, every other 0.15 | as the table |

`FUN_0022d4f0` (B1 144097-144170) measures each `base` and `ALERT` movement record (a > 4): it plays the clip to
half its length and divides the `DummyRoot` translation by that time, giving the record's root speed (m/s); a zero
or wrong-signed result takes a default of 0.9 (walk) or 3.5 (run), signed + for forward and left, - for backward
and right. In RAM: walkforward 3.687, runforward 6.338, walkbackward -1.245, runbackward -3.724, crouch walk
2.075 / -2.050, prone 1.010 / -0.900, prone walkleft +0.945, walkright -0.900 (the last two defaults) [M]. The
per-weapon `W` records are not measured (their speeds stay 0; the chooser reads the base table only).

### 2.2 The fallbacks, in the builder's order

1. `base` (144405-144441): a stance's idle_emote falls back to stand's; its idle_checkweapon, idle_lookaround,
   turnleft and turnright fall back to that stance's idle_emote.
2. Per weapon set, in order w = 0..8, from its **fallback set** `DAT_00535598` = {rifle: rifle, bazooka: rifle,
   pistol: tool, grenade: tool, tool: pistol, sabre: tool, sbdroid: tool, bdroid: rifle, lascannon: rifle}: every
   empty `W`, `U`, `ALERT` and `S` slot takes the fallback set's slot as it stands at that moment (144577-144646).
3. Crouch grenade shoot / shoot2 / shoot3 take stand's (144647-144664).
4. Per weapon set and stance (144665-144740): empty shoot / shoot2 / shoot3 take the first of the three that
   exists; every empty `W` slot takes the rifle's; every empty `U` slot takes the rifle's, and one still empty takes
   the slot before it (so `charge` ends as the rifle's shoot); empty `ALERT` and `S` slots take the rifle's.

`U[s][w][0]` (`idle`) stays 0 for every set: no `human_<weapon>_<stance>_idle` clip is on the disc.

### 2.3 The body masks

`FUN_0022f650(skeleton, upper, lower)` (B1 145462-145528) ORs the skeleton indices of 13 bones into `upper` and
sets `lower = ~upper`: `bone_a_spine, bone_b_spine, bone_ribcage, bone_neck, bone_head, bone_l_clavicle,
bone_l_upperarm, bone_l_forearm, bone_l_hand, bone_r_clavicle, bone_r_upperarm, bone_r_forearm, bone_r_hand`
(strings 0x5352d0-0x535360, 0x5353e0, 0x5353f0). The set keeps a pair for the human rig (+0x20, +0x28); each
soldier class keeps its own pair for its `SkeletonName` rig (class+0x1d88, +0x1d90, written by the class setter
`FUN_0020f2e0` at 123351-123363, key 0x7012f6cd), and the animator is handed the class's pair (`FUN_00229fc8`
called at 113777-113778). In RAM the human pair is `0xcf7f0` / `0xfffffffffff3080f`: the 13 bones, and everything
else - `DummyRoot`, `bone_root`, `bone_pelvis`, the legs, **and `hp_weapons`** - in the lower mask [M].

### 2.4 Check against the PS2's memory [M]

`lead_tmp/anim/sets_bf1.py` re-builds all six tables from the disc's clip inventory with the rules above;
`ram_sets_bf1.py` compares every slot (clip CRC and frame count) with the set object in a save state:
**979 of 979 slots equal** (`bf1_kas2_walk_cp0_cp2_b000.p2s`).

## 3. BF1: one soldier's pose per update [V][M]

### 3.1 The animator

A pool of **10** animators of 0x2100 bytes hangs off the set (allocated 144742-144766). A drawn soldier without
one takes the least recently used (`FUN_0022f310`, the list's tail; the pose function moves its animator to the
head with `FUN_0022f2a8`), and `FUN_00229fc8` resets it for him (B1 113774-113778). Fields:

| offset | meaning |
|---|---|
| +0x00..0x3f | the soldier's matrix (rows 0 right-handed x = his left, 1 up, 2 forward, 3 position); +0x40 aim direction (`FUN_0022a9d8`); +0x50 the move since the last update |
| +0x60 | animation state (3.6); +0x64 hit-reaction weight clock, +0x68 its clip CRC, +0x6c its scale |
| +0x70 | **alert timer**, seconds, counts down every update (3.4) |
| +0x74 | seconds since the last state change; +0x20bc the fade length it is measured against |
| +0x78 | special animation id, 0x20 = none; +0x7c stance; +0x80 seconds in the state |
| +0x84 | **weapon set** 0-8; +0x88 **upper action** 0-5; +0x8c leg twist angle; +0xa0 movement id 0-12 |
| +0xc0 | skeleton instance; **+0x8d0 the upper player, +0x12a0 the lower player** (clip contexts; current clip at +0x980, phase at +0x9a4); +0x1c70 the pose buffer (quaternion + position per bone, 0x20 B each from +0x10) |
| +0x2080 / +0x2084 | seconds the lower / upper player has been on its clip |
| +0x2088 / +0x2090 | upper / lower bone mask |
| +0x20a0 | snap flag (first update after a reset: no blending); +0x20c4 skeleton scale |

### 3.2 The two players (state without a special animation, `FUN_0022aa10` case 0x20, B1 142605-142860)

```
id     = movement id from FUN_0022e990 (3.5); a result of 0 keeps an old id of 0-4
action = U[stance][weapon][upper action]                      (FUN_0022f280; 0 when no action is running)

LOWER player (+0x12a0), through the lower mask:
    action == 0 or id > 2 :  base[stance][id]   - the RIFLE's movement clip, whatever the weapon -
                             with ALERT[rifle][id] instead when alert, standing, id is 0, 7 or 8 and that
                             row has the clip (the disc: idle_emote and runforward)      (FUN_0022f158)
    else                  :  the action clip itself, its blend lengthened by 0.5 s

UPPER player (+0x8d0), through the upper mask:
    action != 0 :  the action clip, advanced by the plain dt; when it has run its length the action ends
    else        :  the WEAPON's version of the movement clip (FUN_0022f1c8): id' = id, or 0 for the two turns;
                   ALERT[weapon][id'] when alert and standing, else W[stance][weapon][id'], else base[stance][id']
```

So a soldier who fires while he runs keeps the run on his legs and plays the shoot clip on the 13 upper bones; a
soldier who fires standing plays the whole shoot clip. A new upper movement clip starts at the lower player's
phase (`FUN_0022d3d0`, B1 144031-144067), and a walk <-> run change of one direction keeps the phase
(`FUN_0022f328` pairs 5/7, 6/8, 9/11, 10/12).

Call sites, from the instructions (the decompile drops the integer arguments): `0x22b7b0 jal 0x22bbf0` with
`a1 = animator+0x12a0, a3 = animator+0x2090, f12 = +0x2080`; `0x22b7dc jal 0x22bbf0` with `a1 = animator+0x8d0,
a3 = animator+0x2088, f12 = +0x2084`.

### 3.3 Weights and blending (`FUN_0022bbf0`, B1 142904-142929)

```
f = min(1, t_state / fade)                         (+0x74 / +0x20bc; 1 when no fade is pending)
f = f * alertFactor                                (3.4; both players)
blend = record.blend * 0.01 + extra                (extra 0.5 only for an action clip on the lower player)
if t_clip < blend:  f = f * t_clip / blend
if snap or f >= 1:  pose[masked bones] = clip      (FUN_0043fb90; a bone the clip has no track for takes its REST pose)
else:               pose[masked bones] = slerp / lerp(pose, clip, f)   (FUN_004400e0; bones without a track keep theirs)
```

The pose buffer is kept between updates, so a blend is the previous pose pulled toward the clip by a weight that
grows linearly to 1 over the record's blend time - not a crossfade of two clips. Shoot clips (0.01 s) snap.

### 3.4 Alert and the aim pose

`FUN_0022d150` (B1 143914-143928) arms the timer for every weapon set but tool and sabre: 2.5 s if it had run out,
else 2.0. The draw-side driver calls it each update when the soldier flag at +0x2c0 is set, or the weapon in hand
has a live target handle (weapon+0x128) or is charging (B1 113801-113817); the soldier update calls it when the
soldier's flag at +0x438 is set (B1 119890-119892). A shoot or charge action also sets 2.0 (`FUN_0022a958`,
142264-142276). While the timer is above 0 the getters use the `ALERT` rows.

- Layer factor (142740-142757): timer above 2.0 -> min(1, 1.2 - 2(t - 2)); from 0 down to -0.5 -> min(1, 0.2 - 2t);
  else 1. It is a 0.4 s ramp from 0.2 at each change between the alert and the plain rows.
- Aim weight `a` (142859-142877): 0 at t = 2.5 rising to 1 at 2.0, 1 down to 0, falling to 0 at -0.5, 0 after.

`FUN_0022bdf0(a)` (B1 142989-143571) turns the aim direction into the soldier's frame, limits yaw to +-0.6283 and
pitch to +-1.0996 (a seat's own limits replace them through its vtable+0x78; prone allows no downward pitch),
moves a smoothed pair 15 % of the way each update, and with `y = (yaw - leg twist) / 8`, `p = pitch / 8`, both
times the state fade:

- `a > 0.5`: rotates `bone_a_spine` by 1 x, `bone_b_spine` by 4 x and `bone_ribcage` by 8 x (y, p) x (2a - 1), then
  re-derives the ribcage's children (hash words `DAT_00517adc/ae0/ae4` = 0x5de52389, 0x8c1c9bba, 0xe5b51072);
- `a <= 0.5`, stand or crouch: rotates only `bone_neck` by 2 x and `bone_head` by 5 x (y, p) x (1 - 2a) - the
  soldier looks where he would aim;
- prone: always the spine branch at full weight (143333-143335).

### 3.5 Movement id, playback rate and the leg twist (`FUN_0022e990`, B1 144969-145199)

Inputs: the speed along the soldier's x row (positive = his LEFT: the rig's `bone_l_*` sit at +x and
`walkleft`'s root moves +x) and along his forward row, both divided by the skeleton scale; outputs the id, the
scaled dt and a target leg angle.

- Above 8 m/s (64 = v^2) or under 0.3 m/s on both axes: id 0.
- **A strafe clip is used only when the stance has one** (`base[s][10]` or `[12]`, and `[9]` or `[11]`). The disc
  ships none for stand or crouch (`human_rifle_stand_walkleft` and its three siblings are absent; only prone has
  walkleft / walkright). So a standing soldier strafes on the forward or backward clip with his legs twisted:
  - forward when the move angle `atan2(x, z)` is inside a window, else backward. The window is +-1.2566 rad
    (72 deg), +-1.8850 (108 deg) while a forward clip is already playing, and **-1.8064 .. +1.1781 when alert**;
  - the leg angle is the move angle (plus pi for backward), limited to +-1.2566, or **-0.7854 .. +0.5498 when
    alert**; the pose function moves the stored twist 25 % of the way to it each update (142837) and
    `FUN_0022bcc0` rotates the `DummyRoot` pose by it about the up axis.
  - Hence the asymmetry measured in `re-foley.md` 8.2: an alert soldier strafing left (+90 deg) is outside the
    window and plays `runbackward`; strafing right (-90 deg) he plays the forward clip.
- Walk or run: forward runs above 5.0 m/s while already running, 5.5 otherwise (145128-145148). Backward
  compares the negated speed, so it is always the run clip where one exists (145154-145193): stand yes, crouch
  and prone no.
- Rate: `dt * (0.95 v / v_clip + 0.05)`, `v_clip` the record's root speed (145143, 145197).
- With an id under 5 the soldier turns on the spot: the twist follows his facing, and past
  `DAT_00517ab0[s]` = +0.3927, +0.3927, +0.4712 rad the id becomes 4 (turnright), below `DAT_00517ac0[s]` =
  -0.7069, -1.1781, -0.4712 it becomes 3 (turnleft) (142805-142818).

Erratum for `re-foley.md` 4.4: its "left always takes the run clip" through ids 9 / 0xb is the strafe branch's
sign slip, which a standing BF1 soldier never reaches; the same heard result comes from the alert window above.

### 3.6 States and special animations

`FUN_0022a0a8(animator, state, matrix, crc)` (B1 141877-142249) maps the soldier's animation state to a special
id: 0 / 1 / 2 stand / crouch / prone (prone transitions 0x16-0x19; stand <-> crouch has no clip, a fade of at
least 0.8 s; landing from a jump or fall picks landsoft, or landhard after more than 2.0 s in the air), 3 dead
(a death clip by the hit direction, or a rest pose when already down), 4 jump, 5 thrown, 6 fall, 7 jet hover,
8 tumble, 9 thrown landing, 10 / 11 roll (`diveforward`, fade 0.3 s after), 0xc / 0xd a clip named by CRC.

A special plays through `FUN_0022c7f0` (B1 143573-143744): the lower player takes `special[id]` (the rifle's), the
upper player `S[weapon][id]`. Only one call site passes the "upper action allowed" argument - `0x22b214`,
`a2 = 1`, the jet hover (case 0x15); the other eight pass 0. So a jumping, falling, rolling or landing soldier
shows no shoot clip.

Hit reactions: `FUN_0022cd10` picks one of `human_rifle_stand_hit{front,back,left,right}` at random (weight
scale 0.5, clock 1.0), `FUN_0022cea8` a `human_sabre_stand_block_*` by direction; the clip's first frame is blended
over the upper mask, and over the lower mask too when the soldier stands still, with weight scale x min(1, clock),
the clock falling at `DAT_00517acc` = 8 a second (142852-142858, 142899).

## 4. BF1: what drives it [V][M]

- **Weapon set.** The weapon class key `AnimationBank` (0xa7678f8f, `FUN_002864f0` 210216-210219) is hashed and
  matched by `FUN_00283880` (B1 207868-207888) against `DAT_005182c0` = FNV of `rifle, bazooka, pistol, grenade,
  tool, lightsabre, sbdroid, bdroid, cannon` (the ninth, 0x27a2df1c, is the `lascannon` row; no disc weapon
  names it); no match = 0 (rifle). The index is stored at class+0x20, copied to the weapon instance at +0xec
  (207927) and handed to the animator each update for the weapon IN HAND (`FUN_0022a938`, 113798). Disc census
  (171 PROP chunks): grenade 65, rifle 38, pistol 25, tool 17, bazooka 12, bdroid 6, lightsabre 5, sbdroid 3.
  RAM: class+0x20 = weapon+0xec = animator+0x84 on 145 of 145 armed animators.
- **Upper action.** The weapon's state (weapon+0xcc, machine `FUN_00284318` B1 208353-208600): a shot enters
  state `FireAnim + 1` (class+0x198, key 0x5c0246c4 read with `%i` at 210044-210046; no BF1 class authors it, so
  1), a charge 4, a reload 5. The driver (113799-113835) passes states 1-4 on only in the update the weapon's
  start flag (weapon+0xc0, set by vtable slot `FUN_002859f8`) is up, and state 5 every update; in a network
  session a start flag forces 1. `FUN_0022a958` stores the action and, for shoot and charge, clears the upper
  player's clip so that **every shot restarts the shoot clip**. Two weapon classes pick the variant themselves:
  `FUN_0028eb30` (B1 215619-215651, a charged throw - the grenade in RAM) enters state 3 when the hold fraction at
  weapon+0x144 is above 0.7, 2 above 0.4, else the plain 1: the three `human_grenade_stand_shoot*` clips are
  three throw strengths; `FUN_00290b98` (217183-217202) draws one of a class's variants at random.
- **Skeleton.** A class's `SkeletonName` names a `zaf_` rig in the registry (class+0x1d80); the masks follow
  that rig. BF1 values: `cis_inf_bdroid` 7, `all_inf_wookie` 5, `end_inf_ewok` 3, `nab_inf_gungan` 3,
  `cis_inf_sbdroid`, `tat_inf_jawa`, `gen_inf_geonosian`, `imp_darthvader`. The clips are the human banks' for all
  of them, bound by bone name.

## 5. BF1: the sets on the disc and in play [V][M]

`sets_bf1.txt` lists every slot. The six banks hold 208 clips; the tables use 151 (104 full-body with 26 tracks,
45 upper-body with 16 or 17, 2 with 27), 12 are the hit and block clips, 45 are in no table.

| set | own clips (movement / upper / alert / special) | what it changes |
|---|---|---|
| rifle | 19 / 6 / 2 / 30 | the base; alert rows only for idle_emote and runforward |
| bazooka | 9 / 6 / - / 8 | upper-body walk and run (17 tracks), own idle, shoot (26 frames), reload (141 frames), jump, lands, prone moves |
| pistol | 0 / 6 / 0 / 0 | only shoot and reload per stance; everything else is the TOOL set's (one-handed carry) |
| grenade | 0 / 5 / 0 / 0 | stand shoot, shoot2, shoot3, charge and prone shoot (the throws); the rest tool's |
| tool | 11 / 0 / 5 / 9 | upper-body idle, walk, run, alert rows, lands, roll; its shoot and reload are the pistol's |
| sabre | 0 / 2 / 0 / 0 | stand shoot and shoot3 |
| sbdroid | 8 / 5 / 5 / 7 | own idle, walk, run (plain and alert), shoot (7 frames) and reload per stance, jump, lands, roll |
| bdroid | 3 / 2 / 3 / 0 | own idle, walkforward, runforward (plain and alert), stand shoot and reload |
| lascannon | 1 / 0 / 0 / 0 | one clip; no weapon names the set |

RAM, 151 reference states (`ram_animators_bf1.py`, `ram_owner_bf1.py`): of 226 animators drawn on their state's
newest frame, 155 are in the plain case and **155 of 155 hold exactly the two clips section 3.2 predicts** (4 of
them with an action running, none prone). Among them: `rifle_stand_shoot` over `rifle_stand_runforward_alert`
(twice) and `pistol_stand_shoot` over `rifle_stand_runbackward` - firing on the move; `grenade_stand_shoot3` on
both players of a standing thrower; `bazooka_stand_runforward`, `sbdroid_stand_runforward[_alert]`,
`bdroid_stand_runforward`, `tool_stand_runforward` (a pistol and a fusion cutter) over
`rifle_stand_runforward[_alert]`; `rifle_stand_idle_emote_alert` over `rifle_stand_turnright`; 35 idle super
battle droids on `sbdroid_stand_idle_emote` over `rifle_stand_idle_emote`. By class: `cis_inf_battledroid` +
`cis_weap_inf_wrist_blaster` = sbdroid; `cis_inf_assassindroid` + sniper rifle and `cis_inf_pilotdroid` + mortar
launcher = bdroid; the three torpedo launchers = bazooka; every pistol = pistol; `rep_inf_jet_trooper`'s launcher
and the arc caster = rifle.

## 6. BF2: the animation bank and its sets [V][M]

BF2 keeps BF1's design - two clip players, a lower-body and an upper-body bone mask, one kept pose - and replaces
the fixed nine weapon rows by sets built at level load from name PARENTS.

### 6.1 Objects

- **Bank** `DAT_005fdd48`, 0x11af0 bytes (`FUN_00281180` B2 203574-203586, ctor `FUN_00281228` 203613-203651): +0 the
  animator pool, +0x18 the `human` skeleton, +0x1c / +0x20 the human upper / lower mask, **+0x24 thirty sets of
  0x4b8 bytes**.
- **Set** = 151 pairs of pointers {upper slot, lower slot}. Pair index = animation id:

  | ids | pair | names (ELF pointer tables) | accessors (upper / lower) |
  |---|---|---|---|
  | 38 simple | id | 0x5fde48: sprint, jump, jump_forward / backward / left / right, landsoft, landhard, fall, the thrown_* set, stand_getupfront / back, diveforward (0x18), jetpack_hover (0x19), stand_deadhero, three prone transitions, stand_death_*, stand_hit* | `FUN_002814f0` / `FUN_00281510` (B2 203725-203741) |
  | 6 x 13 movement | 38 + move + 13 x block | blocks 0x5fdf30: stand, crouch, prone, standalert, crouchalert, pronealert; moves 0x5fdee0: idle_emote, idle_checkweapon, idle_lookaround (`idle_takeknee` for the two crouch blocks), turnleft, turnright, walkforward, runforward, walkbackward, runbackward, walkright, runright, walkleft, runleft | `FUN_00281420` / `FUN_00281488` (203699-203722) |
  | 3 x 6 weapon actions | 116 + k + 6 x stance | 0x5fdf18: shoot, shoot2, shoot_secondary, shoot_secondary2, charge, reload | `FUN_00281530` / `FUN_00281558` (203743-203759) |
  | 30 custom (melee sets) | id - 18 | per-level combo names | `FUN_00281580` / `FUN_002815c8` |

  BF2's movement order is not BF1's: 5 walkforward, **6 runforward, 7 walkbackward**, 8 runbackward, 9 walkright,
  10 runright, 11 walkleft, 12 runleft.
- **Slot** (12 bytes, `FUN_00278380` B2 197848-197855): the bank's clip record; a word with the source type in bits
  30-31 (1 upper, 2 lower, 3 full) and the clip's root speeds in cm/s (bits 0-14 forward, 15-29 sideways); a pointer
  to the animation's descriptor (6.4).
- **Families** (table 0x5fd3a8, 0x2c bytes: name, FNV at +0x20, parent at +0x24) and **weapon animation classes**
  (table 0x5fd848, 0x30 bytes: name, FNV +0x20, combo +0x24, parent +0x28, alert flag +0x2c). The ELF ships one
  family, `human`, and five classes: `rifle` (root, alert), `bazooka` (parent rifle, alert), `tool` (rifle, no
  alert), `pistol` (tool, alert), `melee` (tool, no alert). A **set is one (family, class) pair** (table 0x5fdc08).

### 6.2 What registers a set

| disc key | effect |
|---|---|
| soldier `AnimationName` 0x98557a5e / `SkeletonName` 0x7012f6cd = `"<name> [<parent>]"` | a family; no parent word = child of `human` (`FUN_00252bb0` -> `FUN_00278588`, B2 171384-171417, 171863-171870, 197987-198023); stored at class+0x1230, masks at class+0x114c / +0x1150 |
| weapon `AnimationBank` 0xa7678f8f | the text after the value's underscore, or the whole value, matched against the class rows -> weapon class+0x20 (B2 320608-320616) |
| weapon `CustomAnimationBank` 0xa7c25382 = `"<name> <parent> [alert\|noalert]"` | a new class row (B2 320641-320654, 198185-198247) |
| melee `ComboAnimationBank` 0x2d32ce43 = `"<name> <parent> <combo>"` | a class row with a combo (B2 337051-337090) |

At class finalise each of the soldier's 8 weapon slots registers the set (class family, weapon's class row)
(`FUN_002517e8` B2 170153-170166). In the 176 battle states: 25 families, all children of `human` but
`grevious`; classes `rifle, bazooka, tool, pistol, melee` plus `sabre` (combo, parent melee) and `spear`
(parent tool, Ewoks). Disc `AnimationBank` values (311 PROP chunks): rifle 131, pistol 115, bazooka 37, tool 28.

### 6.3 Which clip a slot gets (`FUN_00282f78`, B2 204799-216377) [M]

A clip is named `<family>_<class>_<animation>` plus a suffix. For a slot of layer L the loader takes the first that
exists in the family's banks: (1) the bare name, if the animation's *want* equals L; (2) the name + `_upper` or
`_lower`; (3) the bare name, if want is *full*; (4) the name + `_full` (B2 205364-205674; formats 0x620b48
`%s_%s_%s`, 0x620b58 `%s_full`). *Want*: weapon actions upper; movement and simple ids upper - **full only for the
root class of a root family, i.e. `human_rifle`** - and lower for turnleft / turnright (B2 205258-205267, 205338,
206960-206966, 208625-208628). So `human_rifle_stand_runforward` fills both of the rifle set's slots, while
`human_bazooka_stand_runforward` fills only the bazooka set's upper slot.

Empty slots are filled along the two parent chains, in two passes per family:

- pass 1: an **upper** slot tries the PARENT FAMILY with the same class; a **lower** slot tries the PARENT CLASS on
  the same family (alert blocks: the nearest ancestor class with the alert flag);
- pass 2: the other axis, with these aliases first: shoot <-> shoot2, shoot_secondary <-> shoot_secondary2, an
  all-empty crouch action <- stand's, turnleft / turnright / takeknee upper <- that block's idle_emote upper, an
  alert block <- its plain block (a class without the alert flag copies the plain blocks outright),
  jump_forward / backward / left / right and jetpack_hover <- jump, sprint <- stand_runforward.
- A melee-rooted class without a combo gets no weapon-action slots.

The executable statement of these rules is `lead_tmp/anim/sets_bf2.py` (`Model.pass1`, `pass2`, `WP`, `WPa`, `SP`).
Run on the disc's 632 soldier clips with the families, classes and sets each save state registered, it
reproduces the PS2's own tables: **665,176 of 665,176 pair slots equal over 176 battle states (418,420 of them
filled), 38 distinct sets** (`sets_bf2_vs_ram.txt`). The verifier broke one rule at a time in 13 variants and
lost 790 to 418,420 slots each time; only the ORDER of the four name candidates is not tested by the disc's
clips (four reorderings still match) and rests on the decompile. Combo (melee) sets are not compared.

What that gives a trooper: the legs of every `human` set are the rifle's clips; a bazooka, tool or pistol
holder's upper body has its own idle, walk, run, jump, land and sprint clips; the pistol set falls back to the
tool set and has its own alert rows, with a full-body `human_pistol_standalert_runforward`.

At load each lower or full clip's root speed is measured over its first half (`FUN_00282900` B2 204585-204697;
defaults 0.9 walk / 3.5 run, sprint 11.5 from `lui 0x4138` at 0x282a18).

### 6.4 Masks and blend times

- `FUN_00282018` (B2 204216-204259) lists the LOWER body - 10 bones at 0x620ac8: `bone_root, bone_pelvis` and the
  two legs' thigh, calf, foot, toe - and makes upper = every other bone but bone 0. `DummyRoot` is in neither (the
  twist writes it); `hp_weapons` is upper (BF1: lower). RAM: human 0xfd8cf7f0 / 0x0273080e. A rig with bone
  0xb70e2184 - on the disc only the acklay's - takes a 15-bone list at 0x5fdd50 instead.
- Blend times are NOT code constants in BF2. A 12-byte descriptor per (animation, layer) at `DAT_005fdcf8` holds
  BlendInTime and BlendOutTime (bytes, x 100), Loop, AimType (None / Head / Torso / FullBody), a SyncType group and
  a BlendTimeTo list. The LVL chunk `sanm` fills it (`FUN_0027b188` B2 199853-199938): one chunk on the disc, in
  INGAME.LVL, with 192 `Animation("name") { ... }` scopes (`sanm_bf2.txt`). Shipped values, in / out seconds:

  | animations | upper | lower |
  |---|---|---|
  | stand / crouch idles (and alert) | Loop, 0.25 / 0.25, aim Head (alert: Torso) | Loop, 0.25 / 0.25; to its alert twin 0.15 |
  | stand / crouch walk and run (and alert) | Loop, 0.25 / 0.25, synced by direction | Loop, 0.15 / 0.15; to its alert twin 0 |
  | turnleft / turnright | 0.25 / 0.25 | 0.15 / 0.15 |
  | shoot, shoot2 | **0.02 / 0.02** (stored 1 = 0.01), aim Torso | 0.5 / 0.5 |
  | shoot_secondary(2) | 0.15 / 0.15, Torso | 0.5 / 0.5 |
  | charge, reload | 0.5 / 0.5 (reload: aim Head) | 0.5 / 0.5 |
  | sprint | Loop, 0.15 / 0.15 | same |
  | jump and the four directions | 0.01 / 1.0 | same |
  | landsoft, landhard | 0.01 / 0.15 | same |
  | fall | Loop, 1.0 / 0.01 | same |
  | diveforward, deaths, thrown poses | 0.15 / 0.15 | same |
  | jetpack_hover | Loop, 0.15 / 0.25, Torso | same |

  RAM holds the file's values in 258 of 258 (animation, layer) records (`ram_sanm_bf2.py`; the EE's single-precision
  x 100 truncates 0.01 to 0 and 0.02 to 1); `stand_deadhero` and `stand_hit*`, absent from the file, hold 0 / 0.

## 7. BF2: one soldier's pose per update [V][M]

- **Animator**: 0x20f0 bytes, a pool of 10 taken least-recently-posed first (`FUN_00281618`, pose reader's B2
  153721-153739), as BF1. The soldier's own matrix at +0, **upper player +0xd10 through mask +0x58, lower player
  +0x16d0 through mask +0x5c, pose +0x900** (call sites 0x27e84c-0x27e888 and 0x27e940-0x27e97c: `a0 = +0x900,
  a1 = +0x16d0, a2 = +0x5c` / `a1 = +0xd10, a2 = +0x58`; overwrite `FUN_005337a0`, blend `FUN_00533a80`); set +0x84
  (= the weapon in hand's set, `FUN_0027c650` B2 200694-200721); stance +0x20b4; mode +0x20b8 (0x27 locomotion,
  else a simple id); movement id +0x20c0; upper state +0x20e0 (0-5 a weapon action, 6 none, 7 a melee id).
- **Clips, locomotion mode** (`FUN_0027dfe0` B2 201828-201896): the upper player takes the action's upper slot
  when one runs and the set has it, else the movement pair's upper slot; the lower player takes the movement
  pair's lower slot - **or the action's lower slot only when the action has just started and the soldier is not
  moving** (the chooser returned 0); once dropped, the legs do not rejoin that action. Both read the SAME set, with
  the alert blocks (+3) when the alert bit is on. Three side branches: a chooser result of 14 (a slide) puts
  simple id 7, `landhard`, on both players; a melee id's own lower slot is used while the soldier moves within
  0.3 m/s of that clip's root speed; state 0x12 keeps a named clip's first frame on the legs.
- **Other modes** (`FUN_0027d9a8` B2 201394-201621): both players play the simple id's own pair - sprint, the
  jumps, fall, lands, `diveforward`, jet. A weapon action is drawn over them only for landsoft and jet, from the
  STAND action row. The sprint pair runs at |v| / its lower clip's root speed, a plain ratio (card SPRCL,
  `re-sprint-clip.md` 2).
- **Weights** (`FUN_0027fef8`, `FUN_0027ff88`, `FUN_0027ffe8`, 0x27fef8-0x280044): `cur = min(1, t / T)`; the kept
  pose is blended by `(cur - prev) / (1 - prev)`, overwritten at 1 - together a LINEAR crossfade over T from the
  pose at the clip change. No alert factor. `T` = the old clip's BlendTimeTo entry for the new animation, else
  min(new BlendInTime, old BlendOutTime) (B2 202393-202402). Clips of one SyncType group keep their phase.
- **Action** (`FUN_0027c738` B2 200723-200784, map `FUN_00281630` 203802-203837): weapon state 1 -> shoot (or
  shoot_secondary for a secondary-flagged weapon), 2 -> shoot2 / shoot_secondary2, 3 -> charge, 4 -> reload (the
  last two for a primary only). A fire state reaches the animator only in the update the weapon's fired bit is
  up, with *force*, which restarts the clip: every shot restarts `shoot`. A reload clip is rate-fitted to the
  weapon's reload time (+0x20c8). The action ends when its clip finishes.
- **Alert**: a soldier timer (soldier+0x810); `FUN_0024f078(soldier, flag)` stores 2.5 s for a non-zero flag
  (`lui 0x4020` at 0x24f080), else 0. It is armed while the weapon in hand fires, charges or holds a target and
  zeroed for a class without the alert flag (tool, melee) (pose reader's B2 168562-168637). When the driver feeds
  the animator it passes `timer > 0` (`FUN_002810b8`, B2 153834); the bit selects the `standalert_*` /
  `crouchalert_*` blocks.
- **Aim** (`FUN_00280158` B2 202778-203453): type from the UPPER clip's AimType, faded over that clip's
  BlendInTime. Torso: `bone_a_spine` 0.125, `bone_b_spine` 0.375, `bone_ribcage` 0.5 of the yaw and pitch (sum 1).
  Head: `bone_neck` 0.25, `bone_head` 0.375 (sum 0.625). Limits pitch +-1.0996, yaw +-0.6283 from the leg twist;
  the pair eases by min(1, 4.5 dt) (the immediates: `fconst.py b2 0x280158 0x280e38`).
- **Movement** (`FUN_0027ed18` B2 202142-202353): id 0 under 0.3 m/s (0.2 once moving); strafe ids only when the
  set has a left and a right clip - `human_rifle` has none for stand or crouch, so BF2 too strafes on the forward
  or backward clip with the legs twisted; run instead of walk by the soldier's speed mode (`re-foley.md` 4.4),
  and where the walk clip is missing (`stand_walkbackward` is); rate `0.95 v / v_clip + 0.05` on both players.
  Twist eases by min(1, 7.5 dt); turn in place past +0.3927 / -0.7069 rad (prone +-0.4712; words at 0x5fdd20,
  0x5fdd30). landsoft or landhard by 1.5 s in the air; fall becomes `thrown_flail` after 3.0 s.
- RAM, 175 states (`ram_animators_bf2.py`): of 183 animators posed on their state's newest frame, **182 of 182
  with a prediction hold the two clips these rules give** (145 locomotion, 37 simple ids). Among them
  `human_rifle_stand_shoot_full` over `human_rifle_standalert_runbackward`, `human_rifle_stand_shoot_secondary2_full`
  over `human_rifle_standalert_runforward`, `human_bazooka_stand_runforward` over `human_rifle_stand_runforward`,
  `sbdroid_rifle_standalert_runbackward` and `bdroid_rifle_standalert_runforward` over the human rifle legs.
  Thin spots: 2 of the 145 hold an action clip, and no state shows a soldier shooting while standing still, so
  the action's lower slot on the legs is [V] only.

### 7.1 BF2's leg twist and turn, read for card ANIMM (2026-10-15) [O]

Checks run: `re_check.py fn` on every line below (B2:168718/168737/168787/168790 in `FUN_0024f350`, B2:202186/202311/
202353 in `FUN_0027ed18`, B2:201791 in `FUN_0027dfe0`; B1:145084/145127/145143 in `FUN_0022e990`, B1:142784/142836 in
`FUN_0022aa10`); `re_check.py word --f32` b1 0x517ab0 / b2 0x5fdd20 (8 words each); one opus verifier on ten claims: all
SUPPORTED, one PARTLY (the speed mode's 0 cases, below).

- **The twist and the forward bit live in the SOLDIER** (`FUN_0024f350` B2:168715-168782), not the animator: the move
  since the last update (position less +0x820) against rows +0x120 / +0x140 gives `a = atan2(left, ahead)`. Not alert
  (timer +0x810 <= 0): forward inside +-1.2566371, +-1.8849558 while bit 0x80000000000 of +0xa70 is up; outside, a + pi
  (wrapped) and the bit cleared; clamped +-1.2566371. Alert and the weapon's vtable+0x5c returning 0: backward when
  a >= 1.1780972 or a <= -1.8064158, clamped -0.7853982 .. 0.5497787 (BF1's alert rule). Alert and that virtual non-zero:
  backward when the move dotted with +0x5d0 / +0x5d8 is negative, the angle UNCLAMPED [H: which weapons answer non-zero
  is unread; the port takes the first branch]. Under the idle line (dt x class+0x1220 x 0.3, 0.2 once moving) the angle
  is 0 and the bit is SET - so a start from a stand has the wide window. All of it only while +0x7e4 < 3 (else 0, bit
  set). The angle is +0x830.
- Speed mode (B2:168785-168797): 2 above dt x MaxSpeed (class+0x74c) x 0.78 (0.71 already 2), else 1; 0 on the idle
  and +0x7e4 >= 3 paths.
- `FUN_0027ed18` (B2:202293-202315) with no strafe clips reads +0x830 and the bit (id 5, or 7 with the bit clear), +1
  for mode 2 or a missing walk clip; with strafe clips its own angles (id 0xb for left, 9 right; a - pi/2 / a + pi/2).
  `FUN_0027ec68` returns the clip's root speed negated for ids 7-10, so the rate is positive in every direction.
- Turn on the spot (`FUN_0027dfe0` B2:201771-201805, unwind B2:202010-202032): twist plus the facing's change against
  0x5fdd20[s] = 0.3927, 0.3927, 0.4712 (id 4, turn -twist - 0.2) and 0x5fdd30[s] = -0.7069, -0.7069, -0.4712 (id 3,
  0.2 - twist); the turn is unwound at turn / ((frames - 1) / 30) a second from the next update (BF1: frames / 30,
  B1:142782-142811). While moving the twist eases by min(1, 7.5 dt) (B2:202052-202066) toward the chooser's angle [D: the
  link of *(decompiler detail omitted)* to +0x830 needs the disassembly], or 0 when bit 2 of argument 3[0x20e]'s byte 6 is set [H].
- BF1 detail the verifier added: on backward paths `FUN_0022e990` negates the speed and the record's speed is negative,
  so its rate is positive too; the strafe-right threshold tests *(decompiler detail omitted)* (a copy slip that never matters).

## 8. The droids, the officers, and what each game calls the same thing

- **Super battle droid.** BF1: class `cis_inf_battledroid` on its own rig (`SkeletonName cis_inf_sbdroid`), wrist
  blaster on set `sbdroid`: 25 `human_sbdroid_*` clips - idle (70 frames), walk, run, alert rows, a 7-frame shoot
  and a 61-frame reload per stance, jump, lands, roll - over the rifle's legs. BF2: family `sbdroid` x class
  `rifle`: 25 own clips in bank `sbdroid` (CIS.LVL), the same list as `sbdroid_rifle_*` plus a full-body sprint and
  `stand_shoot_secondary_full`; legs from `human_rifle`.
- **Battle droids.** BF1 set `bdroid` (8 clips: idle, walkforward, runforward plain and alert, stand shoot,
  reload). BF2 family `bdroid` x rifle (9 own), x bazooka, pistol, tool (1-2 own each).
- **Launchers, pistols, tools.** BF1 sets bazooka / pistol / tool (section 5); BF2 classes bazooka / pistol / tool.
- **BF2 officers and specials**: families `clonecommander`, `magnaguard`, `impofficer`, `bothanspy`, `marksperson`,
  `wookie`, `ewok`, `geo`, heroes - each 1-3 own clips over the human sets.

| | BF1 | BF2 |
|---|---|---|
| sets | 9 fixed weapon rows, one fallback table | (family, class) pairs with parent chains |
| legs' clip | always the rifle's | the set's lower slot (the rifle's unless the class ships a `_lower` / `_full`) |
| upper mask | 13 named bones | everything but root, pelvis, legs |
| blend | weight `t / T` on the kept pose each update | linear crossfade over T |
| blend times | code: 0.15, prone 0.4, shoot 0.01, reload 0.5 | the disc's `sanm` chunk |
| shoot standing | whole body plays the shoot clip, legs blended over +0.5 s | legs play the action's lower slot (0.5 s) |
| alert | timer on the animator, 2.0 s (+0.5 s ramp) | timer on the soldier, 2.5 s |
| alert rows | rifle: idle_emote, runforward; per-set rows | full `standalert` / `crouchalert` blocks |
| aim spread | spine 1/8, 4/8, ribcage 8/8 (absolute) | 0.125, 0.375, 0.5 (cumulative) |
| animators | 10, least recently used | 10, least recently posed |

## 9. The port against this, and the change list

Today: `bake/src/soldier.rs:156` bakes `SOLDIER_CLIPS`, 34 `human_rifle_*` names, into every soldier (BF2 soldiers
get 24, their names carrying `_full`); `game/src/animator.rs` plays one clip on the whole body with a 0.15 s
crossfade, switches walk to run at 4.5 m/s, and plays shoot or reload only when the soldier stands still
(`select`, lines 81-86).

1. **Bake the sets, not a list.** Per map side, port the two resolvers (`lead_tmp/anim/sets_bf1.py`,
   `sets_bf2.py`) and bake, per (skeleton, weapon set), the upper and lower clip of every animation id, with each
   clip's frames, loop flag, root speed and blend time (BF1: the code's; BF2: the `sanm` chunk, a new `mungefs`
   reader). Referee: the baked table equals the PS2 memory tables the two scripts were checked against.
   **Built by card ANIMS (2026-10-05):** `bake/src/anim_sets.rs` (+ `anim_sets/bf1.rs`, `bf2.rs`), `mungefs/src/sanm.rs`;
   each soldier's `soldiers/<model>.sets.ron`. Tests `bake/tests/suite/anim_sets.rs`: BF1 979 of 979 slots, BF2 11,524 of
   11,524 for a UTA1, a MYG1 and a HOT1 state. What the build added to this note:
   - **Root speed is the POSITION at the half, not the distance from frame 0** [V] (`FUN_0022d4f0` B1 144119-144144:
     the pose at half the clip, `DummyRoot`'s model position over that time). A clip that starts off the origin and
     walks back to it measures 0 and takes the default: that is why prone walkbackward holds -0.900. Ten BF1 speeds
     rebuilt from the disc equal section 2.1's RAM values to 0.0015 m/s.
   - **BF2 registry from the ODFs** [D from RAM, not read in the code; PROVISIONAL]: a weapon with no `AnimationBank`
     on its `BASE` chain (grenades, detpacks, the officers' buffs) registers NO set (`hansolo` holds pistol and tool
     only, with a detpack and a buff in his slots); `(human, melee)` is registered in every state whatever the
     classes. With those two rules the sets the two sides' ODFs give equal the sets in RAM for every family a state
     loaded, but one per state: a Jedi's `(family, rifle)` (`kiadimundi`, `vader`) beside his sabre's combo set -
     what registers it is unread [H]; no hero is baked.
   - The bake's registry is the four soldier sides' whole class lists (a mission loads two sides and a few classes):
     resolving with it gives the three states' sets the same clips, 10,988 of 10,988 slots.
2. **Two players and two masks in `game::animator`.** Lower = everything outside the upper mask; upper = BF1's 13
   bones (BF2: all but root, pelvis, legs). Clip choice per section 3.2 / 7; the weapon in hand picks the set.
3. **The action layer.** Shoot on the frame a shot leaves (restart), charge, reload; ends with its clip; legs join
   only when standing still (BF1 +0.5 s blend; BF2 the action's lower slot).
4. **Blending.** BF1 maps: weight `t / T` on the kept pose; BF2 maps: linear over T, T from the descriptors.
   **Items 2-4 built by card ANIML (2026-10-05, 644695a):** `game/src/animator.rs` (`Rig` over the baked sets,
   `Animator::step`), the set picked in `game/src/soldier_vis.rs` from the weapon in hand, the keys read by
   `game::roster::class_anim`. Census (`ANIM_CENSUS=1` on `round_to_outcome`, seed 1): BF1 NAB1 847 of 847 and BF2
   MYG1 1,513 of 1,513 shots fired on the move show the shoot clip over the legs' movement clip. Ours, PROVISIONAL:
   BF1's per-update pull is applied as `1 - (1 - t/T)^(60 dt)` so the pose does not depend on the frame rate; a
   reload starts on the first update of the weapon's reload and is not restarted if its clip ends first; an empty
   movement slot takes the other gait of its direction, then idle (BF2's missing `stand_walkbackward` runs, as
   section 7 says; the rest of the chain is the port's); the BF1 grenade's shoot2 / shoot3 and the charge clip are
   not driven.
5. **Alert and aim.** The alert timer and rows; the torso / head aim pose with its limits and weights - this is
   what turns the gun to where the soldier shoots, up and down included.
   **Built by card ANIMA (2026-10-06, 87046b1):** `game/src/animator/aim.rs` (the two timers, BF1's layer factor and
   aim weight, BF2's AimType per upper clip, the limits, the easing, the torso and head turns) and the alert rows in
   `Rig::body_slots`. PS2 frames taken for it (BF1 Geonosis, a clone trooper, camera pitch 29.7 deg;
   `engine/artifacts/lead_tmp/anima/pair_aim_up.png`): aiming up without firing the torso is upright, the rifle at
   the hip and only the helmet tilted back; firing and 1 s after the last shot the torso leans back and the rifle is
   raised onto the aim; 5 s after it is the idle carry again - section 3.4's reading, seen [M]. BF1's super battle
   droid (KAS2, `ps2/sheet_cis.png`): idle both forearms hang; firing, the right forearm is up at shoulder height
   and the bolt leaves its wrist. Ours, PROVISIONAL: the turn is about the soldier's own right and up axes, yaw then
   pitch, on a copy of the kept pose; what arms the timer is `soldier_vis`'s rule (he fires, holds the trigger, has a
   target; the two soldier flags stay unread); the yaw term is fed 0 until the leg twist exists (item 6). Census
   (`ANIM_CENSUS=1`, seed 1): with the torso at full weight the gun stands off its clip's own line turned by the
   eased aim by 0.0000 rad mean on BF1 NAB1 (12,217 updates), BF1 GEO1 (2,217), BF2 MYG1 (8,306), BF2 GEO1 (14,907).
6. **Movement.** The PS2's ids, thresholds and rate (`re-foley.md` 4.4 with section 3.5's corrections), the
   strafe twist, the turn on the spot.
7. Later, each its own piece: special animations (jump, fall, lands, thrown, deaths by direction), hit
   reactions, the low-resolution path beyond 10 animators (the port animates every soldier in full - ours).

## 10. Checks run

Scratch `engine/artifacts/lead_tmp/anim/`: `elf.py` (words, strings, pointer tables), `crc.py`, `fconst.py`
(a function's float immediates from the ELF), `sets_bf1.py` / `sets_bf2.py` (the two set builders re-implemented on
the disc inventory), `ram_sets_bf1.py`, `ram_animators_bf1.py`, `ram_owner_bf1.py`, `ram_sets_bf2.py`,
`ram_animators_bf2.py`, `ram_sanm_bf2.py`, `sanm.py`; reader reports `AGENT_B2_BUILDER.md`, `AGENT_B2_POSE.md`
(two Opus readers on BF2; every load-bearing BF2 claim above was then re-checked by the lead in the disassembly
or in RAM); `claims.md`, `VERDICTS.md`, the verifier's scripts in `verify/`.

1. **fn** - `re_check.py fn` over the cited lines (`checks/fn_b1.txt`, `fn_b2.txt`).
2. **absent** - `re_check.py absent b1 DAT_005182c0` -> 1 hit, `FUN_00283880` (the set-name table is .data, no
   writer); `re_check.py writers b1 DAT_00517aa8 DAT_00517acc DAT_00517ab0 DAT_00517ac0` -> 0 (constants);
   `re_check.py callers b1 FUN_0022aa10 / FUN_0022a958 / FUN_0022a938 / FUN_0022d150 / FUN_0022a0a8` and
   `callers b2 FUN_0027ed18 / FUN_0027caf0 / FUN_0027dfe0 / FUN_00280158` (the driver sites); "only the jet hover
   allows the action" is the nine `jal 0x22c7f0` sites of `dis_b1_0022aa10.txt`, eight with `a2 = 0`; `FireAnim` on
   the BF1 disc: `propscan.py bf1 5c0246c4` -> 0 PROP chunks; no standing strafe clip: the name search of
   `sets_bf1.py` (0 of 8 `human_rifle_{stand,crouch}_{walk,run}{left,right}`) and the 49 empty pairs of the BF2
   `human,rifle` set in RAM.
3. **twin** - each BF1 function has its BF2 twin opened: set object (`FUN_0022d828` / `FUN_00282f78`), pose
   (`FUN_0022aa10` / `FUN_0027caf0` + `FUN_0027dfe0`), special player (`FUN_0022c7f0` / `FUN_0027d9a8`), layer weight
   (`FUN_0022bbf0` / `FUN_0027ff88`, `FUN_0027ffe8`), chooser (`FUN_0022e990` / `FUN_0027ed18`), aim (`FUN_0022bdf0` /
   `FUN_00280158`), action setter (`FUN_0022a958` / `FUN_0027c738`), masks (`FUN_0022f650` / `FUN_00282018`). They
   differ where section 8's table says.
4. **word** - the name tables and float immediates above are ELF dumps (`elf.py`, `fconst.py`); `DAT_00535598`,
   `DAT_005354a0`, `DAT_00517ab0`, `DAT_00517ac0`, `DAT_00517acc`, `DAT_00517adc..ae4`, `DAT_005182c0`, B2
   0x5fd848, 0x620ac8, 0x5fdd20, 0x5fdd30, 0x24f080, 0x282a18.
5. **writers** - `DAT_00518be4` / `DAT_00518be8` (the network flags the driver's forced shoot reads): 0 in the ELF,
   written only in session code (`FUN_002fd018`, `FUN_002fdf70`, `FUN_00322f20`, `FUN_00322f48`, `FUN_0031c9d0`,
   `FUN_00174cf8`); an ELF store scan was not run, so "offline never" is [D].

**Verifier** (one Opus agent, foreground, `claims.md` only; `VERDICTS.md`): 51 claims, **41 SUPPORTED, 10 PARTLY,
0 NOT SUPPORTED**; the six measured claims reproduced, each with a reading of its own. The PARTLY ones are folded
in above, each re-read first: the ninth set name is `cannon`; `FUN_0022f158` returns the rifle's alert slot only
where it is filled; prone always aims with the spine; kinds 3 and 4 map to charge and reload for a primary only;
`FUN_0024f078` stores 0 for a zero flag; BF2's second, 15-bone mask list is the acklay's; three side branches of
BF2's clip choice; the 176 (not 178) compared states and the 246,756 slots empty on both sides; two line ranges.
Its own finds: the grenade's three throw strengths (`FUN_0028eb30`), a second BF1 driver of the animator, and that
the disc cannot tell the order of the four name candidates.

What stays **[H]** or unread: a second BF1 animator driver (`FUN_001be1e8`, `FUN_001be8b8`: an entity holding its
animator at +0xa4) was not read; the two BF1 soldier flags that arm the alert timer (+0x2c0 was 0 on all 145 armed
animators in RAM; +0x438 set on 13 of 13 with a fresh timer) and BF2's soldier+0x3e0 - named by effect only;
which soldiers are drawn at the detail level that takes an animator, and the low-resolution path (`humanlz`;
BF1's one-player animator `FUN_0022fd28` on `human_animation_lowrez`); the first-person banks (their clip tables and
the throw's clip in both views: `re-grenade-throw-anim.md`, card GTHRW, 2026-10-22); BF1 states 0xc /
0xd and BF2 states 0xe / 0x11 / 0x12 (clips named by CRC: seats); the thrown / tumble / death picks beyond the
ids; BF2 combo (melee) sets; the rotation order and handedness of the aim pose against a frame; `re-foley.md`'s
speed-mode reading for BF2's run choice was not re-read; BF2 `sanm`'s BlendTimeTo lists and SyncType groups were
decoded but only Loop, AimType and the two blend times were compared with RAM by the lead. Not built or compared
on screen: no PCSX2 picture pair was taken in this session - the build cards carry that.

## 10. BF1's jump is a timed pose sequence (2026-10-25) [O] UNAUDITED

Contributed in the public repo's pull request #1 (a frozen clone jump seen on BF1 Geonosis).
The one-frame counts in `re-anim-banks.md` 3.1 describe individual clips, not the entire jump's motion.
The previous port held the `jump` clip throughout `Stance::Jump` and omitted the landing clip. [D]

Read by the contributor from their own `SWBFRONT.ELF` on the BF1 ISO. The main executable is `/SWBFRONT.ELF`, not the small
`/SLUS_208.98` loader. ELF addresses below use file offset `VA - 0x160000 + 0x1000`.

- `FUN_0022a0a8`: state 4 dispatches through `0x534b10 + 4*4` to `0x22a35c`, which clears state time +0x80
  and selects special 0, `jump`. The pose driver's table at `0x534b50` sends special 0 and 1 to `0x22ad3c`.
  There, `0x22ad3c..0x22ad60` compares +0x80 strictly above the immediate `0x3e99999a` (0.3 s) and changes
  special +0x78 to 4, `fall`; `0x22ad7c` plays the resulting special. The soldier remains in jump state.
  Time +0x80 advances after posing, at `0x22bb24..0x22bb30`. [V]
- On return to a posture, `0x22a1b8..0x22a230` selects `landsoft` (id 2), or `landhard` (id 3) when +0x80 is
  strictly above 2.0 s (`0x40000000`, `0x22a1fc`). The test keys on the OLD special +0x78 (0 `jump`, 1 `jump_up`
  or 4 `fall`) or state +0x60 == 6, not on an airborne state; old special 0x15 `jetpack_hover` always lands soft,
  and no landing is played when the old special is 0x20 or +0x20a0 is set (`0x22a1c0`, `0x22a1cc`). [V]
- `0x22b12c..0x22b168` plays the landing until the lower clip completes. `FUN_0022c7f0` returns completion
  from lower clip seconds >= frames/30 at `0x22cba8..0x22cbf0`. A soft landing may yield to locomotion after
  0.15 s when planar displacement/dt is above 1 m/s (`0x22b16c..0x22b1f4`); it then sets the state fade to
  0.3 s. The clone's rifle-set lower clips are 17-frame `landsoft` and 34-frame `landhard` (bazooka 24/37, tool
23/41, sbdroid 27/41); the port reads the length from the clip. `FUN_0022c7f0` also reports completion at once
when there is no lower clip (`0x22c854`). The landing case eases the leg twist +0x8c to 0.9x its value, the jump
case to 0 (`0x22b134`, `0x22c9e8..0x22ca14`). [V]
- Special playback resets the lower clip clock only when the lower clip changes, and advances it on the first
  update (`0x22c85c..0x22c894`). A changed upper clip starts its clock at zero (`0x22c900`) and copies the lower
  slot's +0x9a4 (`FUN_0022d3d0`), read here as the phase [H] (`0x22c8cc..0x22c918`). Both use the growing clip blend, with factor 1, through `FUN_0022bbf0`
  (`0x22c998..0x22c9e4`). Applying two 60 Hz pulls per original 30 Hz update makes these transitions too fast;
  the jump sequence now uses one original pull per 1/30 s, with fractional pulls between updates. [V/D: the
  per-update gate is `0x22aa8c..0x22aaac`; that it runs at 30 Hz is not proven, so the pull rate is a port design choice]

The runtime fix keeps those BF1 clips and restores their timed sequence, blending and landing interruption.
It changes presentation only. The rejected `thrown_flail` substitution is removed; BF2's jump is not borrowed.
Tests exercise the real clone's changing bone poses, clip names, landing duration and repeat jump, plus synthetic
checks of both body layers' original blend weights and soft/hard landing interruption. No live jump picture pair
was obtained; keyboard injection into the isolated PCSX2 comparison did not trigger a jump.
Re-checked 2026-10-25 against our own BF1 ELF bytes and decomp (capstone, re_check.py, the special-name table
`PTR_DAT_00517af0`): every cited address, branch and constant matches; the wording above carries the corrections.
