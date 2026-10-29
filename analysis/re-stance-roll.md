# RE: Soldier stance machine (BF1) + roll implementations (BF1 & BF2)

Sources: `analysis/SWBFRONT.ELF-decomp.c` (BF1, SLUS_208.98) and
`analysis/SWBFRNT2.ELF-decomp.c` (BF2, SLUS_212.40); raw ELFs in `extracted/`
for data-section dumps (vtables, string tables, defaults). All addresses are EE
virtual addresses. Hashes are the engine's FNV-1a-with-`|0x20` ("lowercase")
key hash (`fnv1a_lowercase`, = BF1 FUN_00444b30 — see re-sound-banks.md).

Everything below marked **[V]** was verified by reading the decompiled code
and/or dumping ELF data; **[H]** = hypothesis / needs PCSX2 confirmation.

---

## 1. Big picture

Both games use the same architecture:

- A per-soldier **stance/state integer** drives a big `switch` in the soldier's
  per-tick update. BF1: field `soldier+0xbe4`, update `FUN_00205cc8`.
  BF2: field `soldier+0x7e4`, update `FUN_00240bf8`.
- A guarded **SetStance(soldier, newState)** function records
  `prev = cur; cur = new; timeInState = 0`. BF1: `FUN_00209758`
  (prev at +0xbe8, timer at +0xbec). BF2: `FUN_00247bf8`
  (prev at +0x7e8, timer at +0x7ec). **[V]**
- **Transition actions** (crouch/prone/stand/jump/roll/jet) are virtual methods
  on a "controllable" sub-object, invoked from a command layer; each plays a
  3rd-person anim set from the class, a 1st-person view anim, and posts a
  weapon event.
- Per-stance movement is applied by scaling the stick inputs before a shared
  movement integrator. BF1 hardcodes the stance factors; **BF2 moved the same
  numbers into ODF data (`ControlSpeed`)** with identical defaults.

BF1 has **no sprint** and has **two roll states** (roll-left 0xb / roll-right
0xa) triggered by jump+strafe. BF2 collapsed roll into **one state (5)** with a
free direction, put it on the **same button as crouch** (moving = roll,
stationary = crouch toggle — exactly the P3 design), added **sprint (state 3)**
and an **energy/stamina pool** that jump/roll/sprint spend. BF2's prone code
still exists but its "go prone" action is a stub returning 0 — prone is
unreachable. **[V]**

---

## 2. Stance enums

### BF1 (`soldier+0xbe4`) [V, from switch in FUN_00205cc8 + transition fns]

| id | state | notes |
|----|-------|-------|
| 0 | stand | |
| 1 | crouch | |
| 2 | prone | aim pitch clamped to ±0.4 rad (FUN_00205cc8 @~0x207240) |
| 3 | dead/dying | SetStance refuses to leave 3; after 4 s → corpse cleanup |
| 4 | jump (airborne) | entered by FUN_00208b78 |
| 5 | thrown/flying (ragdoll flail) | entered by FUN_00209738; SetStance special-cases entry (weapon checks) |
| 6 | falling | stand falls >1 s → 6; jet fuel empty → 6 |
| 7 | jetpack thrust/hover | entered by FUN_00209540 (jet jump) |
| 8 | tumbling on ground | from landing 5 fast (speed²>36, or >9 with LCG rand>0.3 — same rand at DAT_0066eed0) |
| 9 | getting up | timer = dur(anim 0x10 `stand_getupfront`); slow+timeout → FUN_00209018 → 0 |
| 10 (0xa) | roll RIGHT | forced strafe +1.7 |
| 11 (0xb) | roll LEFT | forced strafe −1.7 |
| 12 (0xc) | mount/seat transition (keeps +0xc10) | [H — inferred from FUN_0020aad0 (+0x414 path)] |

### BF2 (`soldier+0x7e4`) [V, from switch in FUN_00240bf8]

| id | state | notes |
|----|-------|-------|
| 0 | stand | |
| 1 | crouch | |
| 2 | prone | engine-complete but **no entry path** (GoProne vtable slot +0xa8 = FUN_00246ec8 `return 0`) |
| 3 | **sprint** | speed ramps MaxSpeed → MaxSpeed×ControlSpeed[sprint].fwd over SprintAccelerateTime |
| 4 | jump (airborne) | |
| 5 | **roll** (any direction) | one state; direction = velocity at entry |
| 6 | jetpack (jump-jet type) | fuel drain JetFuelCost |
| 7 | jetpack (hover type, class+0x778 bit3) | |
| 8 | falling | |
| 9 | thrown/flail | uses ControlSpeed idx 7 ("tumble") for air control |
| 10 (0xa) | tumbling on ground | same speed²>36 / rand>0.3 selection as BF1, LCG at DAT_0073d870 |
| 11 (0xb) | getting up | exit ≥0.5 s & slow → stand |
| 12 (0xc) | special fall/spin | turn clamped ±6π rad/s |
| 13 (0xd) | (roll-adjacent, exits at 0.5 s like roll) [H] | |
| 14 (0xe) | seated/turret align state | turn π rad/s toward target |
| 15/16 (0xf/0x10) | death states | SetStance refuses to leave |
| 17 (0x11) | (special, preserves +0xa88/+0xa98) [H] | |

Both games also keep a separate **posture** value (0 stand / 1 crouch /
2 prone) used for collision/aim data selection: BF1 `soldier+0xbe0`
(*(decompiler detail omitted)*; rolls set it to 1 = crouch profile; selects one of three
0xb4-byte aim blocks at `class+0x1b60 + posture*0xb4`). BF2 packs it into bits
22-23 of `soldier+0x7e0` (roll sets crouch bit 0x400000). **[V]**

---

## 3. BF1 stance machine

### 3.1 Function map

| addr (BF1) | role | conf |
|------|------|------|
| FUN_00205cc8 | Soldier per-tick update; stance switch; input-byte dispatch | high |
| FUN_00209758 | SetStance(soldier, id); blocks leaving 3; entry checks for 5 | high |
| FUN_00208b00 | Action: crouch (state 1); anim class+0x68; weapon evt 8 | high |
| FUN_00209018 | Action: raise (crouch→stand, prone→stand, land→stand); state→0 | high |
| FUN_00208c70 | Action: go prone (stand→2 via anim 0x16, crouch→2 via 0x18); weapon evt 7 | high |
| FUN_00208f60 | Stand-up headroom probe (offset +0.55, probe 1.35, need result >0.9999) then FUN_00209018 | high |
| FUN_00208b78 | Action: jump (state 4); vy = class+0x183c; +fwd boost; anim class+0x48; weapon evt 4; flag 0x40 | high |
| FUN_002093d0 | Action: **roll left** (state 0xb); anim class+0x58; weapon evt 6; flag 0x80; 1st-person RollLeft | high |
| FUN_00209488 | Action: **roll right** (state 0xa); same anim slot class+0x58 | high |
| FUN_00209540 | Action: jet-jump (state 7); fuel gate/cost; push class+0x18d8 | high |
| FUN_00209738 | Action: thrown (state 5) | high |
| FUN_002098b8 | Movement integrator (dt, fwdIn, strafeIn, turnIn, brakeScale, soldier) | high |
| FUN_0020aad0 | Command processor: posture target +0x3c8, rollL +0x3cc, rollR +0x3d0, jump +0x3e0; drives inputs via FUN_00233500 | high |
| FUN_0022f3d8 | anim duration = u16 frames(anim+8) × 1/30 s | high |
| FUN_0020d238 / FUN_0020d940 | EntitySoldierClass ctor (defaults below) | high |
| FUN_0020f2e0 | EntitySoldierClass::ReadProperty (hash→field map below) | high |
| FUN_001911a0 / FUN_001a5188.. | 1st-person view-anim config (keys "RollLeft","RollRight","Crouch","CrouchToProne","StandToProne","ProneToStand","CrouchToStand") + per-player triggers | high |
| 0x00532be0 | soldier controllable vtable (base incl. this-offset −0x2a0). Slots: +0x7c crouch, +0x80 jump, +0x84 prone, +0x8c rollL, +0x90 rollR (dumped from ELF) | high |

Special-anim id table (ELF data @ **0x00517af0**, verified dump): 0x00 jump,
0x01 jump_up, 0x02 landsoft, 0x03 landhard, 0x04 fall, 0x05 thrown_flail, …,
0x10 stand_getupfront, 0x11 stand_getupback, **0x14 diveforward**,
0x15 jetpack_hover, **0x16 stand_getdown_prone**, **0x17 prone_getup_stand**,
**0x18 crouch_getdown_prone**, 0x19 prone_getup_crouch, 0x1a-0x1d deaths. **[V]**

### 3.2 Transition triggers (what the code actually checks)

> **CORRECTED 2026-09-09 (session 17, `analysis/re-stance-side.md` A, lead-verified):** the two
> block labels below were the wrong way round. Block A is the AI/agent controller path; block B is
> the LOCAL PAD path. The labels are fixed in place below; nothing else in this section changed.

Two command layers exist (both funnel into the same vtable actions):

**A. FUN_0020aad0 (int command block — the AI/AGENT CONTROLLER path [V, corrected 2026-09-09]):**
gated by transition timer `+0xc0c` ≤ 0 (or ≥10, see 3.4):
- `+0x3c8` = target posture: 0 → FUN_00209018 (raise), 1 → crouch, 2 → prone.
- `+0x3cc` / `+0x3d0` = roll-left / roll-right request (with latch `+0x3d4`
  cleared once state leaves 10/11 — one roll per press).
- `+0x3e0` jump edge (+ latch `+0x400`): if already state 4 → **jet-jump**
  (double-tap jump = jetpack, FUN_00209540); else jump.

**B. Byte command block dispatched inside FUN_00205cc8 (state cases; the LOCAL
PAD path — `PlayerController::update` FUN_0025e098, pad bits 2/3 [V, corrected 2026-09-09]):** bytes `+0x2ee` (jump), `+0x2f0`/`+0x2f1` (stance):
- stand: `+0x2f0==-1 && +0x2f1==1` → crouch; `+0x2f0==+1 && +0x2f1!=2` → prone;
  jump byte:
  - `strafe > 0.9 && |strafe| > |fwd|` → **roll right**
  - `strafe < −0.9 && |strafe| > |fwd|` → **roll left**
  - else, if timeInState > 0.5 → jump.
- crouch: `-1/==1` → stand-up probe; `+1/==2` → prone; jump byte: same
  roll checks, else (t>0.5) raise to stand.
- prone: `-1/==1` or (jump && t>0.5) → stand-up probe. **No roll from prone.**

Inputs: `+0x300` fwd, `+0x304` strafe, `+0x308` turn, `+0x30c` pitch (floats;
confirmed by FUN_0020aad0 passing them against MaxSpeed/MaxStrafeSpeed/
MaxTurnSpeed/MaxPitchSpeed in FUN_00233500). **[V]**

So the notorious BF1 roll = **jump button while the stick is ≥0.9 deflected
and more sideways than forward** (path B), and/or a dedicated roll-left/right
command (path A) whose pad mapping still needs PCSX2 confirmation.

### 3.3 Per-stance movement (call sites in FUN_00205cc8's switch) [V]

`FUN_002098b8(dt, fwdIn·F, strafeIn·S, turnIn·T, brake, soldier)`:

| state | fwd F | strafe S | turn T | brake |
|-------|-----|--------|------|-------|
| 0 stand | 1.0 | 1.0 | 1.0 | 1.2 (1.7 while transition-lock timer ∈ (0,10)) |
| 1 crouch | **0.7** | **0.6** | 1.0 | 1.0 |
| 2 prone | **0.3** | **0.2** | **0.5** | 1.0 |
| 3 dead | 0 | 0 | 0 | t² ramp |
| 4/5/6 air | 0.2 | 0.2 | 0.5 | 0 (0 if no air-control flag) |
| 7 jet | complex (hover doubles inputs; JetPush accel) | | | 0.3 |
| 8 tumble | 0 | 0 | 0.1 | 0.2 |
| 9 getup | 0 | 0 | 0.1 | 0.4 |
| 0xa roll R | 0.1 | **forced +1.7** | 0.6 | 1.0 |
| 0xb roll L | 0.1 | **forced −1.7** | 0.6 | 1.0 |

Inside FUN_002098b8 (class = `soldier+0x958` ptr):
- target fwd speed = `class+0x1834` (MaxSpeed) × fwdIn; target strafe =
  `class+0x1838` (MaxStrafeSpeed) × strafeIn.
- acceleration toward target = `class+0x1830` (Acceleraton) × **1.2**
  (×0.2 airborne, ×0.3 jet, ×0.01 when traction `+0x964`<0.75=DAT_005178ac).
- braking (input released / overspeed) = `class+0x1830 × 0.6(DAT_005178a4) ×
  brakeScale`; plus rolling friction term `−accel×dt×0.4(DAT_005178a8)` along
  velocity when grounded.
- gravity (airborne) = **−18.0** m/s²; −13.5 in states 5/9.
- yaw rate = `class+0x1844 + (class+0x1840 − class+0x1844)·soldier[+0x3c0]`
  (blend MaxPitchSpeed↔MaxTurnSpeed by an aim factor), × turnIn × dt.
- grounded flag = bit1 of `soldier+0xbfd`.

### 3.4 Transition timing / gating (BF1 lock timer `soldier+0xc0c`) [V]

Float, decremented by dt each tick (FUN_00205cc8 @~0x206c00); crossing 10
downward snaps to 0 (values >10 encode a softer lock class):

- 0 < t < 10: movement + turn inputs zeroed; stand brake boosted to 1.7.
- t ≥ 10: movement/turn allowed; only stance/jump command bytes cleared.
- any t > 0: stance/jump/roll command bytes are cleared (no transitions).

Writers:
- go prone: t = dur(stand_getdown_prone 0x16) or dur(crouch_getdown_prone 0x18).
- prone→stand: t = dur(prone_getup_stand 0x17).
- landing from jump: t = **10.6** (0.6 s soft lock, move allowed); if airborne
  >2 s: t = **0.5** (hard lock — landing stun). getup(9): t = dur(0x10).
- crouch↔stand: **no lock (instant)**. Rolls: **no lock on exit**.

Firing: fire processing (FUN_00204578) is **skipped in states 3, 5, 0xa, 0xb**
→ **cannot fire during a BF1 roll**. Weapon events on transitions
(FUN_00284e50): 4 jump, 5 land, 6 roll, 7 prone-down, 8 crouch, 9 raise. **[V]**

### 3.5 BF1 class fields / ODF keys (parser FUN_0020f2e0; defaults from ctor FUN_0020d238) [V]

| field | key (hash) | default | meaning |
|-------|-----------|---------|---------|
| +0x1830 | **Acceleraton** (0x9f8a0040 — yes, the misspelling is the key BF1 accepts) | 70.0 | ground accel base |
| +0x1834 | MaxSpeed (0x86df0364) | 9.0 | fwd speed m/s |
| +0x1838 | MaxStrafeSpeed (0x3437a33d) | 7.0 | strafe speed |
| +0x183c | ? (0xb421860c, name uncracked) | 8.0 | jump velocity m/s |
| +0x1840 | MaxTurnSpeed (0x5433f589) | 2.0 | rad/s |
| +0x1844 | MaxPitchSpeed (0xb5534be0) | 0.5 | rad/s (blended into yaw too) |
| +0x1848/4c | PitchLimits (0x3403b139, deg→rad ×0.017453294) | −0.8 / +1.1 rad | aim pitch clamp (prone overrides to ±0.4) |
| +0x185c/60/64 | CollisionScale (0x1a87bacf, 3 floats) | 1,0,0 | per-posture collision scale [H on order] |
| +0x18d8 | JetJump (0xc4b2cb20) | 7.0 | jet up-push |
| +0x18dc | JetPush (0xf0b75e90) | 14.0 | jet accel |
| +0x18e0 | JetFuelRechargeRate (0x0825c1bf) | 0 | fuel/s regen (fuel at soldier+0xc6c, 0..1) |
| +0x18e4 | JetFuelCost (0x36d9de4d) | 0.2 | fuel/s while jetting |
| +0x18e8 | JetFuelInitialCost (0x1ebb5841) | 0 | fuel per activation |
| +0x18ec | JetFuelMinBorder (0xf8a37c8a) | 0.1 | min fuel to activate |
| +0x190c | JetType (0x91fabb5c; value "hover"=0x70ceab2d → 1) | 0 | 0 jump-jet, 1 hover |

### 3.6 BF1 roll spec (port target)

- **Entry** (from stand or crouch only): jump command + |strafe| ≥ 0.9 &
  |strafe| > |fwd| → state 0xa (right) / 0xb (left); or explicit rollL/rollR
  command (FUN_0020aad0). Latch prevents auto-repeat. No energy cost, no
  cooldown beyond the latch.
- **Motion**: normal ground integrator with inputs overridden:
  strafe = ±1.7, fwd = stick_fwd×0.1, turn ×0.6, brake 1.0. So the soldier
  accelerates at 70×1.2 = 84 m/s² toward a lateral target of
  MaxStrafeSpeed×1.7 (= 11.9 m/s stock) and holds it for the roll.
- **Duration**: exits when timeInState > **0.75 × frames(diveforward)/30**
  (anim id 0x14 in bank DAT_00517ae8; frames are LVL data, not in ELF).
- **Exit**: returns to the **previous stance** (`FUN_00209758(prev)` with
  prev = soldier+0xbe8) — roll from crouch lands back in crouch. No post-lock.
- **Effects**: posture set to crouch (hitbox/aim block 1) for the duration;
  cannot fire; cannot start another transition (state cases 0xa/0xb have no
  dispatch); weapon event 6; flag 0x80 at +0xbfe; 3rd-person anim from class
  slot +0x58 (the anim-set builder decides the actual clip; duration source is
  `diveforward`); 1st-person "RollLeft"/"RollRight" camera anim
  (FUN_001a5188/FUN_001a51e8).
- No invulnerability found anywhere in the roll path. **[V]**

---

## 4. BF2 stance machine & roll

### 4.1 Function map

| addr (BF2) | role | conf |
|------|------|------|
| FUN_00240bf8 | Soldier per-tick update; stance switch on +0x7e4 | high |
| FUN_00247bf8 | SetStance; refuses to leave 0xf/0x10; platform-velocity handoff on 4/6/9 | high |
| FUN_00246850 | Action: stand up (headroom via heights DAT_0061ed50; prone→crouch fallback; per-prev-state locks) | high |
| FUN_00246e38 | Action: crouch (state 1); anim class+0x68; weapon evt 8 | high |
| FUN_00246ec8 | Action: go prone — **stub, returns 0** (prone removed) | high |
| FUN_00246ed0 | Action: jump (state 4); vy += class+0x764; fwd×class+0x768, strafe×class+0x76c | high |
| FUN_002471b0 | Action: **roll** (state 5); velocity impulse to RollSpeedFactor×MaxSpeed; anim class+0x58; weapon evt 6 | high |
| FUN_00247440 / FUN_002474f0 | Action: sprint start (3) / stop (3→0) | high |
| FUN_002476a0 | Action: jet activate (from 4/8; fuel gates; → 6 or 7 by class+0x778 bit3) | high |
| FUN_00247a80 / FUN_00247ab8 | jet off (6/7→8) / set thrown (9) | high |
| FUN_00247ad8 | **Sprint input predicate** (see below) | high |
| FUN_00247b88 | **Roll input predicate** (see below) | high |
| FUN_002475f0 | Roll energy spend (EnergyCostRoll) + dispatch vtable+0xb0 | high |
| FUN_00247528 | Jump energy spend (EnergyCostJump / EnergyCostJumpSprint if over MaxSpeed) + vtable+0xac | high |
| FUN_002480a0 | Ground movement integrator (dt, fwdTargetSpeed, strafeTargetSpeed, turnRate, accel) | high |
| FUN_00248560 | Air/roll integrator (dt, turn, fwdAccel, sideAccel) — drag exp(−0.012·|v|·dt), gravity −18 | med-high |
| FUN_00281510/FUN_00278398 | anim lookup / frame count (duration = (frames−1)/30) | med |
| FUN_00252bb0..FUN_00255418 region | EntitySoldierClass::ReadProperty | high |
| ctor near line 169320 (0x0024xxxx) | EntitySoldierClass defaults | high |
| 0x0061e998 | soldier controllable vtable (this-offset −0x270). Slots: +0xa0 standup, +0xa4 crouch, +0xa8 prone-stub, +0xac jump, +0xb0 roll, +0xb4 sprint, +0xb8 stop-sprint, +0xbc jet-on, +0xc0 jet-off (dumped from ELF) | high |

Special-anim table @ **0x005fde48** (dumped): 0x00 sprint(!), 0x01 jump,
0x02 jump_forward, … **0x14** stand_getupfront (ERRATUM 2026-09-08: this file
first said 0x11; the ELF table at 0x5fde48 says 0x14 and 0x11 is
`thrown_bouncebacksoft` — `analysis/re-anim-banks.md` §5, RE_LOG 2026-09-08),
**0x18 diveforward**,
0x19 jetpack_hover, 0x1b prone_getup_stand, 0x1c crouch_getdown_prone. **[V]**

Per-stance capsule heights: **DAT_0061ed50 = {1.9 stand, 1.6 crouch, 0.55
prone}** (used by stand-up clearance probe: needs 1.9−h headroom). **[V]**

### 4.2 Input predicates (the contextual button) [V]

Command bits read in the update switch: `soldier+0x2b4` ([0xad]) = jump,
`+0x2b8` ([0xae]) = **crouch/roll button**, `+0x2bc` ([0xaf]) bit0 = sprint.
"Pressed edge" = `(bits & 3) == 3`.

- **Roll predicate FUN_00247b88(fwd, strafe)**: `fwd² + strafe² ≥ 0.81`
  (stick ≥ 0.9) AND NOT (fwd ≤ 0 with |strafe| ≤ |fwd|·1.732) — i.e. any
  direction within 120° of straight ahead; the cone of ±60° about straight
  back is refused. (CORRECTED 2026-10-12, card ROLLS: this line said "±30°";
  1.732 = tan 60° and the angle is measured from the BACK axis. Live on BF2:
  the stick straight back crouches, at 135° it stands a crouched soldier up —
  `re-roll-picture.md` 5. The port's `Soldier::roll_predicate` copies the code
  and was right all along.)
- Stand state, crouch/roll button: predicate true → **roll**
  (via energy spend FUN_002475f0), predicate false → **crouch**. Crouch state:
  predicate true → roll from crouch; false (t>0.5) → stand. Prone state:
  button → stand. **Moving = roll, stationary = stance change. [V]**
- **Sprint predicate FUN_00247ad8**: stick ≥ 0.9, fwd ≥ 0, and
  |strafe| < fwd·0.577 (±30° cone) to start; keeps sprinting up to
  |strafe| < fwd·1.732 (±60°) — hysteresis. Start additionally requires actual
  speed ≥ 0.9×MaxSpeed and energy not depleted (FUN_00247660, flag +0xab8). Which speed, and what a slope does
  to it: 4.2b.
- Jump: energy cost EnergyCostJump (EnergyCostJumpSprint when moving faster
  than MaxSpeed, i.e. sprint-jump costs more).

### 4.2b The sprint start's speed test, and a slope (card SPUP, 2026-10-06) [V][V-PS2]

> The maintainer could not start a sprint while running uphill. Read in the stand case of `FUN_00240bf8` and run live on the
> PS2 (BF2, a free-standing soldier, one row per game update): 8 terrain slopes on Hoth and Kamino's long ramp.
> Checks: `re_check.py fn b2 160321 160328 160329` (all three in `FUN_00240bf8`). No verifier agent (a build card;
> the one code claim is three lines and every row of the live table re-states it). BF1 has no sprint (section 1).

- **The test [V].** With the sprint bit held (`[0xaf] & 1`) and the stick predicate true, the stand case takes the
  3-component dot of the velocity (E+0x550) with the body matrix's forward row (E+0x140) and asks
  `MaxSpeed (class +0x74c) x 0.9 < dot` - strictly over - then `FUN_00247660` (the depleted bit, the sprint action)
  (B2:160321-160329). The forward row is level (its y read 0.000 on every live row), so the number is the facing's
  share of the HORIZONTAL velocity, not the speed along the floor and not the planar length.
- **Held, not pressed [V-PS2].** The bit is polled on every update: the start comes on the first update the pad word
  shows L3 (the row after the press), 0.9 s into a run or 0.6 or 1.2.
- **A slope [V-PS2].** The full-stick run up a floor of angle a is MaxSpeed x cos^2 a along the floor (the mover's
  uphill factor, re-soldier-ground.md), so the test's number is MaxSpeed x cos^3 a and the sprint starts up to
  a = 15.1 deg (cos^3 = 0.9) and not above:

  | spot | floor (normal) | v . fwd / MaxSpeed (7.0) | cos^3 | L3 held 1.2 s |
  |---|---|---|---|---|
  | Hoth flat | 1.65 deg | 0.999 | 0.999 | sprint on the first polled update |
  | Hoth s10 | 9.79 | 0.957 | 0.957 | same |
  | Hoth s12 | 13.34 | 0.932 | 0.921 (the floor eases along the run) | same |
  | Kamino ramp x 3 | 14.77 | 0.904 | 0.904 | same, 3 of 3 |
  | Hoth s17 | 16.12 | 0.887 | 0.887 | never (36 updates held) |
  | Hoth s18 | 19.27 | 0.850 | 0.841 | never |
  | Hoth s21 / s25 / s24 | 21.9 / 23.0 / 24.9 | 0.818 / 0.790 / 0.747 | 0.799 / 0.780 / 0.747 | not on the floor: ONE update of stance 3 each, on the update after a row with the floor bit clear (a terrain seam), back to the stand on the next |

  The one-update sprint off a seam on the steep three is recorded, not explained [H: the velocity of the first update
  off the floor is not slope-scaled; what ends it the update after was not read].
- Rows and tools: `engine/artifacts/lead_tmp/spup/ps2/` (`spup_probe.py`, `spup_read.py`, `out/bf2_hot1_*.csv`,
  `out/bf2_kam1_*.csv`), spots by `spup/tools/slopes.py`.

**The port (SPUP):** the run up a slope is already the PS2's (`core/src/soldier/tests.rs`
`the_uphill_run_is_the_ps2s` pins four rows of the table). The start test is the PS2's on a level floor and OURS on a
slope (the maintainer, 2026-10-05): the speed along the floor against 0.9 x what the slope rule allows a full stick, so a
sprint starts on any slope a soldier runs up (`Soldier::sprint_start_speed`). Why the maintainer could not start one on
Kamino's ramp, where the PS2 can: his pad's stick peaks at 0.984 of full travel, which put the run at 0.890 x
MaxSpeed there; the left stick now reads 0.95 of its travel as full (`input::STICK_FULL`, ours).

### 4.3 Per-stance movement — data-driven `ControlSpeed` [V]

ODF key **ControlSpeed** (hash 0x47bf39c1), format
`ControlSpeed = "<stance> <fwd> <strafe> <turn>"`; stance-name hashes: stand
0x75493323→idx0, crouch 0x4e4fa58b→1, prone 0x6dd99a25→2, sprint 0x76f15155→3,
jump 0xa73f5c0d→4, roll 0x16ff3d1e→5, jet 0xd7e330f4→6, tumble 0x4c009bc2→7.
Arrays: fwd `class+0x77c+idx·4`, strafe `+0x79c+idx·4`, turn `+0x7bc+idx·4`.

Defaults (ctor) — note stand/crouch/prone match BF1's hardcoded values:

| idx stance | fwd | strafe | turn |
|-----|-----|--------|------|
| 0 stand | 1.0 | 1.0 | 1.0 |
| 1 crouch | **0.7** | **0.6** | 1.0 |
| 2 prone | **0.3** | **0.2** | **0.5** |
| 3 sprint | 1.5 | 0.5 | 0.2 |
| 4 jump | 0.03 | 0.03 | 0.5 |
| 5 roll | 0.02 | 0.02 | **0.6** |
| 6 jet | 0.3 | 0.3 | 1.0 |
| 7 tumble | 0.0 | 0.0 | 0.1 |

Ground states use FUN_002480a0 with target speed = input × Max(Speed/Strafe)
× factor; velocity approaches target at `Acceleration` (+0x748, default 70)
per second (×0.1 if traction < 0.7); gravity −18 airborne. Note asymmetry:
**backpedal uses MaxStrafeSpeed**, forward uses MaxSpeed (BF2's stand
case: `fwd>0 ? MaxSpeed : MaxStrafeSpeed`; **BF2 only** — corrected by AUDIT
AIRC 2026-10-02: BF1's `FUN_002098b8` has one forward cap, class+0x1834, for
both signs, §3.3, and a BF1 soldier walks back at MaxSpeed: −6.5004 m/s live,
crouched −4.550; §4.3b). Sprint (state 3) ramps the fwd
cap from MaxSpeed to MaxSpeed×ControlSpeed[sprint].fwd over
`SprintAccelerateTime` (+0x774, hash 0x56389879, default 0.5 s). While the
transition-lock timer (+0x4e4) runs with flag bit 0x400, accel is ×1.7
(×0.05 = DAT_005fd094 with harder bit 0x2000) — BF2's version of BF1's 1.7
transition braking. **[V]**

### 4.3a The stick in the air — both games, code + PCSX2 on open floor (card AIRC, 2026-10-02) [V][V-PS2]

> **AUDIT 2026-10-02 (AIRC): PARTLY.** Both air rules hold: re-derived from the decomp and the ELF instructions, then
> replayed against a second set of live rows from other maps (BF2 Coruscant, BF1 Theed; 43 jump trials): 612 BF2
> updates within 1.8e-6 m/s at dt = 1/30, 540 BF1 updates within 2.7e-5 m/s at dt = the clock's own 33.37 ms (BF1
> integrates the frame time, BF2 a fixed 1/30). The table's numbers reproduce (0.935 m / 1.8513 m/s; 1.213 m /
> 1.3000 m/s). One Opus verifier: 22 SUPPORTED, 3 PARTLY (wording), 0 NOT. Corrections: (1) "never lowered" — the
> REVERSED stick lowers an axis at the same step, down to the negative target (the table's own run-back row); it is
> only never braked. (2) `FUN_00248560` is not only the air mover: BF2's ROLL (case 5, B2:160679, ControlSpeed row 5)
> runs it too — the port's roll still reads 0.02 as a target speed (card ROLLM). (3) Every standing trial here pressed
> the stick AFTER the take-off; the take-off update itself is §4.3b, and it is most of what "stand, push the stick and
> jump" does on the PS2. Evidence `engine/artifacts/lead_tmp/audit-airc/README.md`.

The jump row above is NOT a speed cap. The air states do not go through the ground mover's target approach:

- **BF2**, `FUN_00240bf8` cases 4 / 8 / 9 (jump, fall, thrown; B2:160583–160607) call the air mover
  `FUN_00248560(dt, turn, fwd × ControlSpeed.fwd[idx] × Acceleration, −strafe × ControlSpeed.strafe[idx] ×
  Acceleration, s)`, idx 4 (7 for state 9; zero when flag bit 3 of s+0x228 is clear). `FUN_00248560`
  (B2:163191–163300): `v *= exp(|v| · dt · −0.012)` (163266, all three components), then
  `v += forward · (p3 · dt) + right · (p4 · dt)` — the stick is an ACCELERATION, 0.03 × 70 = **2.1 m/s²**, with no
  target and no cap — then −18 · dt on vy (not while the grounded bit is set) and the move.
- **BF1**, `FUN_00205cc8` cases 4 / 5 / 6 (B1:117557–117575) call the shared mover
  `FUN_002098b8(dt, fwd × 0.2, strafe × 0.2, turn × 0.5, brake 0, s)`. In it (B1:119223–119250) the acceleration is
  Acceleraton × 1.2, × 0.2 when the grounded bit is clear; each axis is RAISED toward stick × MaxSpeed (+0x1834, forward
  and back alike) / MaxStrafeSpeed (+0x1838) and never lowered (the brake is the caller's 0). No drag.

`re_check.py fn`: B2:160605 in FUN_00240bf8, B2:163238 / 163272 in FUN_00248560, B1:117575 in FUN_00205cc8,
B1:119223 in FUN_002098b8.

**Measured** (`lead_tmp/airc/ps2/air_jump.py`, one SW PCSX2 session per game, a no-AI battle; BF2 Mos Eisley, rebel
soldier MaxSpeed 7.0; BF1 Yavin Arena, MaxSpeed 6.5 / 4.5; one row per game update = the last RAM read of each clock
value; `analyse.py out <game> [trial]`; CSVs in `lead_tmp/airc/ps2/out/`). Forward speed in m/s, travel from the
take-off point to the landing:

| trial | BF2 | BF1 |
|---|---|---|
| stand, jump, no stick | 0.000 m | 0.000 m |
| stand, jump, stick forward | +0.0700 a tick; 1.851 m/s at the last airborne tick; **0.935 m** | 1.3003 m/s from the first tick (0.2 × 6.5); **1.214 m** |
| stand, jump, stick back | −1.851 m/s; −0.935 m | −1.300 m/s (0.2 × MaxSpeed, not the strafe speed); −1.214 m |
| stand, jump, stick left | 1.851 m/s; 0.935 m | 0.9002 m/s (0.2 × 4.5); 0.840 m |
| stand, jump, stick forward from 0.4 s | 1.037 m/s; 0.315 m | +0.5606 a tick to 1.300 m/s; 0.663 m |
| run, jump, stick released | 9.096 → 8.220 (the drag only); 7.740 m | 8.499 → 8.499; 7.937 m |
| run, jump, stick held | 9.098 → 9.886; 8.571 m | 8.499 → 8.499; 7.938 m |
| run, jump, stick back | 9.097 → 6.539; 6.906 m | 8.499 → −1.300 by tick 14 (−2.8031, then −0.5606 a tick); 0.361 m |
| run, jump, stick left | fwd 8.217, side 1.737; 7.544 m + 0.800 m | fwd 8.498, side 0.900; 7.937 m + 0.841 m |

- BF2's drag: 9.0986 → 9.0546 in one update at |v| 12.12 (exp, not 1 − x: that would be 9.0544).
- BF1's take-off update still has the grounded bit: its acceleration is 84 · dt = 2.8031, not × 0.2 (0.5606).
- Both games: no gravity on the take-off update (vy 8.0000, 8.0000, 7.3995 BF1; 8.0050, 7.9794, 7.3540 BF2 — the
  second is the drag alone), apex 1.912 m (BF1) / 1.861 m (BF2), 27 airborne updates. BF2's vy is 8.005 =
  √(2 · 18 · 1.78). The run's boost: BF2 6.998 → 9.0986 (× 1.300), BF1 6.499 → 8.499 (card JARC).
- "A jump against a wall goes straight up" (`re-soldier-walls.md` §5) was the wall taking the stick's speed off again
  every update; its t3 row 762 ms shows the 0.5605 m/s the stick had just added.

The port (`core/src/soldier.rs` `Soldier::air_steer`) had read the row as a target speed (0.03 × MaxSpeed =
0.27 m/s), so a standing jump with the stick forward travelled 0.2 m and a running jump was braked.

### 4.3b The take-off update and the other callers of the movers (AUDIT AIRC, 2026-10-02) [V][V-PS2]

Read from the decomp and the instructions, then measured (`engine/artifacts/lead_tmp/audit-airc/ps2/jump_probe.py`:
one row per update with the game's own stick, stance, timer, floor flag and body rows; `replay.py`, `port_cmp.py`,
`roll_cmp.py`). `re_check.py fn`: B2:162086 / 162100 / 162152 in FUN_00246ed0, B2:160679 in FUN_00240bf8, B1:118187 /
118190 in FUN_00208b78, B1:119236 in FUN_002098b8.

- **Order inside the update [V-PS2].** The stance's GROUND mover runs with the update's stick first, then the jump is
  applied to the velocity it left. One row of a standing soldier with the stick and Cross pressed together: the stick
  appears, the feet move 0.078 m, stance 4, v_fwd 3.045 (BF2: 70/30 × 1.3) or 4.810 (BF1: 84 × dt + 2.0). The next
  update still has the floor bit: no gravity (BF2: the drag alone; BF1: acceleration × 1, not × 0.2). 28 moving
  updates in all, apex 1.861 m (BF2) / 1.912 m (BF1). Travel with the stick held: **3.75 m (BF2), 4.59 m (BF1)**,
  against 0.935 / 1.213 m when the stick comes after the take-off.
- **BF2 `FUN_00246ed0` [V]** (ELF 0x246ed0–0x247198): refused unless the stance is 0 or 3. vy **+=** class+0x764 (an
  `add.s`, 0x247034; live: a jump on the update the feet came back down takes off at 6.9355 = 8.0050 − 1.0695). With
  vf = v·forward: v += forward · vf · (k − 1), k = JumpForwardSpeedFactor (+0x768) when vf > 0, else
  JumpStrafeSpeedFactor (+0x76c); the side component × JumpStrafeSpeedFactor. Live: 7.000 → 9.101 forward, −5.250 →
  −5.250 backward, 5.250 → 5.250 sideways. **Controller branch** (0x246f28–0x24701c): when the soldier's controller
  passes the type test `FUN_002b6ad0` and its vector at +0x4f0 has |·|² > 0.1, v is SET to normalize((x, 0.01, z)) ×
  MaxSpeed × JumpForwardSpeedFactor + (0, class+0x764, 0) — the AI mover's jump request (RE_LOG "BF2 arc `flags`"):
  a bot's flagged-arc jump is a full boosted leap from any speed. That the type is the AI controller is [D].
- **BF1 `FUN_00208b78` [V]** (B1:118169–118215): vy **=** class+0x183c (a store); v += forward · (class+0x183c ×
  stick_fwd × 0.25): **+2.0 m/s with the stick forward, −2.0 back, 0 released** — additive, not × 1.3077 (JARC's open
  question). Live: 6.500 → 8.500, −6.500 → −8.503, 0 → +2.0 from a stand. No side term, no controller branch. Cross
  with a pure strafe is BF1's side ROLL (stance 10 / 11: 4.50 → 7.65 m/s = 1.7 × MaxStrafeSpeed), not a jump.
- **BF2's roll [V][V-PS2]**: case 5 calls `FUN_00248560` with ControlSpeed row 5 (0.02 × 70 = 1.4 m/s²) — the drag
  and an added acceleration, no target. Live (MaxSpeed 7.0, RollSpeedFactor 1.5, 26 updates): stick held 10.50 →
  10.56 m/s, 8.781 m; released → 9.62, 8.405 m; back → 8.67, 8.034 m; sideways +0.99 m/s, 0.383 m aside. Replay
  within 1.3e-6 m/s (a target approach: 4.7e-2).
- **BF1's ground cap [V][V-PS2]**: `FUN_002098b8` B1:119236–119243 uses class+0x1834 for a forward target of either
  sign. Live: stick back −6.5004 m/s (crouch −4.550 = 0.7 × 6.5), forward 6.5004, strafe 4.5007; the ramp is 2.806,
  3.929, 5.051, 6.174, 6.500 (84 · dt, then 84 · dt less the brake 70 × 0.6 × 1.2 · dt).
- **A fly-height term [V]** follows the mover in both air cases: within 5 m of the mission's ceiling
  vy += ((ceiling − y) − 5) × dt × 10 (BF2 0x2441e4–0x244228, BF1 0x207a10–0x207a58). The ceiling is BF2's
  DAT_00697e54 (written by `SetMaxPlayerFlyHeight`, B2:32250) and BF1's DAT_005873b0 (`SetMaxFlyHeight`, B1:26984);
  both default to 200.0 (`re_check.py writers`). Not ported.

The port (main fbd921f): the jump is taken BEFORE the tick's movement, vy is set, the forward component × 1.3 for both
signs in both games, and the take-off tick has gravity — 26 moving updates, apex 1.605 / 1.647 m; stick and jump on one
tick 0.810 m (BF2) / 1.127 m (BF1); BF2 backward jump −6.80 m/s; BF1 backward −5.85 m/s; BF1 walks back at 4.5 m/s.
Cards JARC (the take-off update), ROLLM (the roll's mover), BF1BK (BF1's backpedal).

**JARC landed (2026-10-03, main d3ff7dc).** `Soldier::step` runs the stance's ground mover first and `Soldier::take_off`
applies the jump to the velocity it left; the next update (`first_air`) has no gravity; BF2 adds the jump velocity and
boosts a forward component only; BF1 sets it and adds 0.25 x jump velocity x the forward stick; BF2's JumpHeight is read
as sqrt(2 x 18 x h). The port now: 28 moving updates, apex 1.861 / 1.912 m; stick and jump on one tick 3.033 m/s and
3.74 m (BF2), 4.333 m/s and 4.12 m (BF1 - short of the PS2's 4.810 / 4.59 by BF1's first ground update, 84 x dt against
the port's 70 x dt: card BF1BK); BF2 backward jump -5.25 m/s. Not ported: BF2's controller branch (the AI's posted jump
direction), a jump from an airborne stand (`FUN_00246ed0` tests the stance, not the floor bit [V]; whether its caller
does is unread [H]), BF1's side roll, the fly-height term. Ours: a soldier in the air mantles 0.3 m
(`core::collision::AIR_STEP`), not the ground's 0.6 m.

### 4.3c The jump's gates and the ledge it mounts (AUDIT JARC, 2026-10-03: PARTLY) [V][V-PS2]

> **AUDIT JARC holds 4.3b.** Re-derived from both decomps and the ELF words, then against 32 new PS2 trials (stick and
> jump on one update with a diagonal, a pure strafe and a back diagonal; sprint; crouch; BF2 Coruscant, BF1 Theed) with
> the port's `Soldier` stepped by the PS2's own per-update stick: BF2 takes off within 0.007 m/s, 28 | 28 moving
> updates, apex within 1 mm, travel within 1 cm (from a stand 3.743 | 3.743 m; a diagonal 3.461 | 3.462 m along and
> 2.832 | 2.832 m aside; a diagonal run 6.509 | 6.512 m; a sprint jump leaves at 13.651 | 13.650 m/s). BF1's jump rule
> is exact; its residuals (4.58 | 4.12 m from a stand, -8.50 | -6.50 m/s backward) are the ground mover's, card BF1BK.
> Evidence `engine/artifacts/lead_tmp/audit-jarc/README.md`; verifier 26 claims: 24 SUPPORTED, 2 PARTLY, 0 NOT.

What 4.3b left unread or [H], read now. `re_check.py fn`: B2:160283 / 160308 / 160318 / 160394 / 160530 / 160560 in
FUN_00240bf8, B2:162075 / 162086 in FUN_00246ed0, B2:162401 in FUN_00247528, B2:170974 in FUN_00252bb0; B1:117386 /
117414 / 117439 in FUN_00205cc8.

- **The caller [V].** The pad's jump is `FUN_00240bf8` -> `FUN_00247528` -> vtable slot +0xac (`FUN_00255b88`, ELF word
  0x61ea44) -> `FUN_00246ed0`. `FUN_00247528` takes the energy first (class+0x864, or the larger of +0x864 / +0x868
  when the forward speed is over MaxSpeed; `FUN_001e2010` refuses only a depleted bar) and gives it back when the jump
  returns 0. Each stance case calls its ground mover (stand B2:160283, sprint B2:160505) and reads the buttons after
  it: 4.3b's order is in the code. BF1: the mover at B1:117386, the jump through slot +0x80 at B1:117439. The AI's
  jump calls slot +0xac itself (`FUN_00249f80`, B2:164612 / 164644) and pays no energy.
- **A press off the floor is taken for 0.3 s [V][V-PS2].** Stand and sprint: on the first update with the floor bit
  clear the stance timer is zeroed and bit 0x80 of the flags byte set (B2:160285-160290); the buttons are skipped only
  when 0x80 is set and the timer is >= 0.3 (B2:160304-160310, sprint B2:160530). Live, two take-offs on an update that
  began off the floor: `audit-airc/ps2/out/bf2_run_release_2.csv` rows 17-19 (flags 0x80, vy +0.13 and -0.47, then
  6.9355 = -1.0695 + 8.0050) and `audit-jarc/ps2/out/bf2_b_sprint_rel.csv` rows 29-30 (flag 0, vy +0.122, then 7.527
  = 0.122 - 0.6 + 8.005). BF1 reads the buttons while its stand timer is under 10.3: off the floor the timer goes to
  10.0 when it had reached 5.0 (a 0.3 s window) and to 10.3 otherwise (B1:117388-117414).
- **No jump from a crouch [V][V-PS2].** `FUN_00246ed0` returns 0 unless the stance is 0 or 3 (B2:162075-162077); in
  the crouch case Cross with the timer over 0.5 calls slot +0xa0, stand up (B2:160394-160397). Live: Cross in a crouch
  gives stance 1 -> 0 with vy 0, the next Cross jumps (`bf2_crouch_jump.csv`). BF1: 3.2 above.
- **BF1's stand jump waits 0.5 s in the stance [V][V-PS2]** (B1:117439, `0.5 < timer`): Cross 0.2 s after a landing
  does nothing (`bf1_crouch_jump.csv`, the press at 2.803 s).
- **JumpHeight [V].** `FUN_00252bb0`, hash 0xc415d23a: class+0x764 = sqrtf((h + h) x 18.0) (B2:170972-170976), the
  constructor's default 8.0 (B2:169388). On the disc: 1.78 (com_inf_default), 2.5 (com_hero_default), 3.5
  (com_jedi_default), 4.5 (imp_hero_emperor); BF1 has no such key. The port's conversion is the PS2's for every class.
- **The ledge a pressed jump mounts [V-PS2].** The soldier walks into a vertical face with a flat top, stops, jumps
  with the stick held, and the whole jump is raised at its take-off row so the one ledge stands in for lower ones.
  Margin = the feet's apex less the top, read from the rows (`audit-jarc/ps2/rim_jump.py --shifts`).
  BF2 (Kamino, a top 2.405 m over the deck): never at -0.51 .. +0.354 m (12 trials, 2 of 2 at +0.354), always at
  +0.421 .. +0.741 m (13 of 13). A 1.861 m jump mounts a 1.44 m ledge and not a 1.51 m one: with 2.1 m/s2 of air stick
  the foot sphere needs about 0.4 s over the top to roll round the edge.
  BF1 (Kashyyyk Docks, the 2.259 m lobe of soldier-walls 5.1): no at -0.45, -0.35 (the plain jump) and -0.25 (8 of
  8), over at -0.15 .. +0.15 (8 of 8; 16 trials, two passes alike). A 1.912 m jump mounts a 2.07 m ledge and not a
  2.17 m one: BF1's air stick is at 1.3 m/s at once, and what is left is the foot sphere on the edge.
  Unshifted BF2 jumps at the 2.4 m ledge never mount (4), nor one whose feet are put 0.10 m over the top at the apex
  (12). The running jump is not measured.

The port after JARC (main 8a35a92), beside these:
- a press on a tick with `grounded` false is dropped. A moving soldier in the stand / sprint stance is off the floor
  on 1.0-5.7 % of ticks (Kamino 5.7 %, both Hoths 3.4-4.7 %, Naboo Plains 2.8 %; 24 rounds), in runs of 1-5 ticks;
- a crouched soldier jumps at full height; the lock after a landing is 0.3 s in both games;
- a pressed jump mounts a 1.75 m ledge in BF2 (margin +0.11; 1.80 m before JARC) and 2.16 m in BF1 (margin -0.25;
  2.20 m before). JARC's taller jump reaches no taller ledge. BF1's is inside the PS2's bracket; BF2's is 0.24-0.31 m
  over the PS2's. `AIR_STEP` is ours, and "the PS2 has no step at all" was one 2.26 m wall;
- a jump straight up from under a thin deck ends ON the deck when its top is up to 2.60 m (BF2) / 2.70 m (BF1) over
  the floor (2.35 / 2.40 m before JARC): no ceiling stop (card VCOL), the taller jump, the ground query's whole step;
- four rule parts pass every soldier / step / lost-jump test when broken: BF2's side factor, the JumpHeight read,
  d3ff7dc's steep-face exception, the first-air landing guard.
Cards JGATE (the gates, the missing tests), JLEDG (the ledge); a note on VCOL.

> **Built by card JGATE (2026-10-05, main 0cbe7b7):** the first two bullets and two of the four untested rule parts
> (BF2's side factor, the JumpHeight read) no longer hold for the port. `Soldier::jump_open` takes a press on the 1st
> to the 9th update that began off the floor (the PS2's timer is 0 on the first, so 8 x 33.37 ms = 0.267 < 0.3 and
> 9 x 33.37 ms is not) [D from B2:160285-160310]; BF1 by its stand timer (`Soldier::stand_age`: 5 s old, or put at
> 5 s by a touch-down, B1:117407-117409; the same timer now picks its 0.7 / 1.0 s fall). A crouch stands up on the
> button 0.5 s in; BF1's stand waits 0.5 s; `Soldier::pay_jump` is FUN_00247528 / FUN_001e2010 (refuses only a
> depleted bar, empties a short one). Not ported: BF2 zeroing the stance timer when the floor is lost (the port keeps
> `air_time` beside it), the 0.3 s gate on the stand's OTHER buttons (roll, sprint), BF1's prone getting up on the
> jump button (3.2). The port's sprint is one stance with the stand for BF1's timer (ours: BF1 has no sprint).

> **AUDIT JGATE (2026-10-05): PARTLY.** The rules above hold, on the PS2 and in the port; three numbers in the
> "Built by" paragraph do not: BF2's window is 10 updates (9 is BF1's), a short energy bar is overdrawn and not
> emptied, and after a landing the gate is the lock (BF1 0.6 s). 4.3d has them; card JGAT2 builds them.

### 4.3d The gate's update counts, the energy bar and the landing lock (AUDIT JGATE, 2026-10-05) [V][V-PS2]

Re-derived from both decomps and the ELF, then 84 PS2 pad trials with the game's own jump word, stance-timer bits,
lock timer and energy bar per update (BF2 Coruscant, BF1 Theed; a standing soldier lifted 2.2 m by one write so the
stand stance loses the floor). `re_check.py fn`: B2:160119 / 160287 / 160308 / 160530 / 159428 / 159431 in
FUN_00240bf8, B2:92626 in FUN_001e2010, B2:92505 in FUN_001e1e28, B2:170681 / 170684 in FUN_002517e8, B2:162401 in
FUN_00247528, B2:161868 in FUN_00246850, B2:235889 in FUN_002b0548; B1:117352 / 117388 / 117414 / 117439 / 117514 in
FUN_00205cc8, B1:118507 in FUN_00209018, B1:179957 in FUN_0025e098. `re_check.py word b2 0x243790 2`: 0x3c013e99,
0x3421999a. Verifier, 25 claims: 20 SUPPORTED, 5 PARTLY (folded in below), 0 NOT. Evidence
`engine/artifacts/lead_tmp/audit-jgate/README.md`.

- **The button block is the player's [V].** Every stance case reads the buttons inside *(decompiler detail omitted)*
  (B2:160304; BF1 *(decompiler detail omitted)*, B1:117414): the window, the crouch's rise and the energy take are a player's
  soldier's. The AI's jump is `FUN_00249f80` -> slot +0xac, no energy (4.3c).
- **BF2's window is 10 updates, BF1's 9 [V][V-PS2].** The timer gets dt before the switch (B2:160119) and is zeroed
  on the first update that began off the floor; the test is `c.lt.s timer, 0x3e99999a` with bc1fl (0x24378c-0x2437a8).
  BF2's dt is 0x3d088888 - one unit under f32 1/30 - and the EE's add rounds down: nine steps read 0x3e999999
  (0.29999998), ten 0x3eaaaaaa (`ps2/out/bf2_liftonly_0.csv`). Live: a press on the n-th update that began off the
  floor jumps at n = 6..10 (15 of 15; n = 10: vy -5.400 -> +2.005) and not at n = 11 (0 of 3). BF1's frame is
  33.37 ms and its timer starts at 10.0 against 10.3: n = 5..9 jump (12 of 12), n = 10, 11 do not (0 of 6); a stance
  under 5 s old reads 10.3 at once (0 of 6), and after a touch-down (timer 5.0) the next spell takes the press (4 of
  4). 4.3c's "8 x 33.37 ms" is BF1's count applied to both games. The same zeroing restarts the sprint's speed ramp:
  one update off a floor seam took a sprinter from 10.50 to 7.67 m/s (`bf2_s_sprint_jump_short_0.csv` rows 26-33).
- **The energy bar [V][V-PS2].** A bar is {config pointer, value, flags}; config = {max, floor, threshold} at
  class+0x84c. `FUN_001e2010(cost, bar)`: cost <= 0 -> 1; flag bit 0 (depleted) -> 0; value -= cost; below 0 it sets
  the bit and clamps at the FLOOR (B2:92617-92636). The floor is minus the dearest of class+0x864 (EnergyCostJump),
  +0x868 (EnergyCostJumpSprint), +0x86c (EnergyCostRoll) and +0x88c: `FUN_001e1e28` x 4 in the class post-load
  `FUN_002517e8` (B2:170681-170684). The threshold is EnergyMinSprint / EnergyOverheat (`FUN_001e1cb8`, default
  20.0); `FUN_001e1fc0` clears the bit at value >= threshold. The rate (B2:160209-160225): -EnergyDrainSprint in
  stance 3; in stances 0-2 EnergyRestoreIdle when the horizontal speed squared is under 1.0, else EnergyRestore; 0 in
  stances 4 and up (the selected weapon is handed the rate through its slot +0x74 first, B2:160226-160228). The roll
  takes its cost the same way (`FUN_002475f0`). RAM, the stock soldier: [100, -30, 20], restore 10, idle 15, drain
  20, costs 0 / 30 / 25. Live: a jump from a stand costs nothing; a sprint jump 30; with the bar at 10.0 two updates
  before, the take-off row reads -21.333 with the bit set and the jump taken, the bar holds through the 28 jump
  rows, rises 0.333 a row while he still slides over 1 m/s and 0.5 standing, and the bit clears at 20.5, 85 updates
  after the landing row. Not exercised by any row: the clamp at the floor, a roll on a short bar.
- **The lock after a landing [V][V-PS2].** BF2 `FUN_00246850` from stance 4 / 8 with the old timer <= 1.5 and flag
  0x2000 clear: +0x4e4 = 0.3, flags | 0x800 (else 0.5, | 0x1c00). While it is over 0 the jump / crouch / sprint words
  get `(w & ~0x1f) | 0xc` (B2:159428-159433) and it is decremented later in the update (B2:159773-159776); the pad
  routine turns 0xc + down into 0x01, down with no edge (`FUN_002b0548` B2:235889-235892). Live: 0.3 on the landing
  row L, 3.0e-8 on L+9, the word masked on L+1..L+10 (10 of 10); a press polled on L+11 reads 0x01 and does not
  jump, one on L+12 jumps. BF1 `FUN_00209018` case 4: lock 10.6, or 0.5 after 2 s in the air (3.4); the byte reads
  0 on L+1..L+18 (12 of 12), a press polled on L+19 reads 2 (`FUN_0025e098` B1:179957-179983: down on a byte of 0)
  and the first that jumps is polled on L+20. So after a landing BF1's 0.5 s stance wait (4.3c) never binds; it
  binds after a crouch's rise, a roll, a spawn.
- **BF1's crouch and roll, live.** Circle crouches; Cross 0.30 s into the crouch does nothing, 2.2 s in gives
  stance 0 with vy 0 (`bf1_b_crouch_jump_*.csv`; the JARC audit's BF1 trial pressed R3 and never crouched). Cross
  with the stick full sideways gives stance 10 / 11 (3.2's roll).

The port (main c2fd1c4) beside these: the window is 9 ticks in both games; an overdrawn bar is set to 0 and the bar
is clamped at 0 every tick, restores in every stance but Sprint, by the stick, un-depletes at a constant 25, and a
roll is refused on a short bar - after an overdrawn sprint jump the port's soldier is depleted for 22 ticks standing
where the PS2's is for 85; the first press taken after a landing is on L+10 (BF2) and L+16 (BF1). Five rule parts
pass every soldier test when broken (`audit-jgate/mutations.txt`). Card JGAT2.

### 4.4 BF2 class fields / ODF keys (soldier movement subset) [V]

| field | key (hash) | default |
|-------|-----------|---------|
| +0x748 | Acceleration 0x2fa0ba9d (alias Acceleraton 0x9f8a0040) | 70.0 |
| +0x74c | MaxSpeed | 9.0 |
| +0x750 | MaxStrafeSpeed | 7.0 |
| +0x754 | MaxTurnSpeed | 2.0 |
| +0x758 | MaxPitchSpeed 0xb5534be0 | 1.0 |
| +0x75c/60 | PitchLimits (deg→rad) | −0.8/+1.1 |
| +0x764 | JumpHeight 0xc415d23a (stored as jump *velocity*) | 8.0 |
| +0x768 | JumpForwardSpeedFactor 0x8ef41ef6 | 1.3 |
| +0x76c | JumpStrafeSpeedFactor 0x32505412 | 1.0 |
| +0x770 | **RollSpeedFactor 0x07ff90da** | **1.7** |
| +0x774 | SprintAccelerateTime 0x56389879 | 0.5 |
| +0x85c | EnergyRestoreIdle 0x082eaf37 | 15.0 |
| +0x860 | EnergyDrainSprint 0x134361b9 | 20.0 |
| +0x864 | EnergyCostJump 0x0b4a7fd4 | 10.0 |
| +0x868 | EnergyCostJumpSprint 0xa44b8f28 | 30.0 |
| +0x86c | **EnergyCostRoll 0xca24cd33** | **10.0** |
| +0x870 | JetJump 0xc4b2cb20 | 7.0 |
| +0x874 | JetPush 0xf0b75e90 | 14.0 |
| +0x878 | JetAcceleration 0x8aac2908 | 20.0 |
| +0x87c | JetFuelRechargeRate | 0 |
| +0x880 | JetFuelCost | 0.2 |
| +0x884 | JetFuelInitialCost 0x1ebb5841 | 0 |
| +0x888 | JetFuelMinBorder | 0.1 |
| +0x88c | JetEnergyDrain 0x0c2425ea | 0 |

(BF2 jet fuel lives at soldier+0xb90 for the jet system and +0x2e4·4=+0xb90…
the 0..1 fuel refills at JetFuelRechargeRate outside states 6/7; energy pool at
soldier+0xab0 with depleted flag +0xab8 bit0.)

### 4.5 BF2 roll spec (port target)

- **Picture** (card ROLLS, 2026-10-12, `re-roll-picture.md`): ONE clip,
  `diveforward`, for every direction, with the skeleton's root turned toward
  the direction of travel (25 % of the rest per update) — a side roll is a dive
  head first to that side. BF1 (§3.6) does the same with a fixed ±90°.
- **Entry**: crouch/roll button while stance ∈ {stand, crouch, sprint}, stick
  ≥ 0.9 in any direction except the ±60° about straight back (§4.2, corrected
  2026-10-12); costs EnergyCostRoll
  (10) from the stamina pool (refunded if the action fails); blocked while
  flag 0x2000 (hard lock) or if RollSpeedFactor ≤ 0 (**RollSpeedFactor = 0
  disables rolling per class** — clean ODF switch).
- **Impulse**: on entry, if current planar speed ∈ (0.1, RollSpeedFactor ×
  MaxSpeed), velocity is scaled up **to exactly RollSpeedFactor × MaxSpeed**
  (15.3 m/s stock) along its current fwd/side direction — direction is the
  *movement* direction, not just facing.
- **During (state 5)**: near-zero control (ControlSpeed roll 0.02 → steering
  accel = 0.02×70 = 1.4 m/s²; turn ×0.6); momentum decays only through the
  air/roll integrator's `exp(−0.012·|v|·dt)`-style drag (FUN_00248560); posture
  bit = crouch.
- **Duration**: exits when timeInState > **0.75 × (frames(diveforward)−1)/30**
  (anim id 0x18, bank DAT_005fdd48) — note the −1 vs BF1.
- **Exit**: **always to stand** (vtable+0xa0 StandUp), which applies a 0.3 s
  soft lock (timer +0x4e4 = 0.3, flag 0x800).
- 1st person: side-dominant rolls play RollLeft/RollRight camera anims;
  forward roll plays none. Weapon event 6. No i-frames found. **[V]**

### 4.6 BF1 vs BF2 roll diff summary

| aspect | BF1 | BF2 |
|--------|-----|-----|
| trigger button | jump (+ stick) or dedicated roll command | crouch button (contextual: moving=roll) |
| stick requirement | ≥0.9 deflection, strafe-dominant (sideways only) | ≥0.9 deflection, any direction except straight back |
| directions | left/right only (2 states, 0xa/0xb) | any (1 state, 5) |
| from stances | stand, crouch | stand, crouch, sprint (not prone) |
| cost | none | 10 energy (EnergyCostRoll) |
| motion model | forced strafe input ±1.7 into ground integrator (accel-limited ramp to MaxStrafeSpeed×1.7 = 11.9) | one-shot velocity impulse to RollSpeedFactor×MaxSpeed = 15.3, then ballistic w/ drag |
| speed constant | hardcoded 1.7 (0x3fd9999a in FUN_00205cc8) | data: RollSpeedFactor (default 1.7) |
| duration | 0.75 × frames(diveforward)/30 | 0.75 × (frames−1)/30 |
| exit stance | previous stance | always stand + 0.3 s soft lock |
| fire during roll | blocked (fire handler skipped) | roll control factors ~0; firing believed blocked by same anim gating [H] |
| turn during roll | ×0.6 (hardcoded) | ×0.6 (ControlSpeed default) |
| posture/hitbox | crouch profile | crouch bit |

The 1.7 constant, the `diveforward` clip, the 0.75 exit fraction, the crouch
posture, weapon event 6 and the flag bit 0x80 are shared — BF2 is a
data-driven refactor of the same roll.

---

## 5. Reference-trace test vectors (derivable now)

Using stock defaults (MaxSpeed 9, MaxStrafeSpeed 7, Accel 70), dt = 1/30
(see a5-tickrate.md for the real tick):

1. **BF1 roll ramp**: enter roll-right from stand at rest. Lateral speed
   v(n+1) = min(v(n) + 70·1.2·dt, 7·1.7=11.9). Expect v = 2.8, 5.6, 8.4,
   11.2, 11.9, 11.9 … per 1/30-tick; position = cumulative sum ×dt.
   Exit at t > 0.75·F/30 (F = diveforward frames) back to stand.
2. **BF1 crouch/prone caps**: hold full forward: stand target 9.0·1.0,
   crouch 9.0·0.7 = 6.3, prone 9.0·0.3 = 2.7; strafe caps 7.0 / 4.2 / 1.4.
   Backpedal cap = MaxStrafeSpeed×factor (7.0 stand).
3. **BF1 stand→prone lock**: on FUN_00208c70, timer = dur(0x16); assert zero
   movement inputs reach the integrator until timer ≤ 0, brake scale 1.7 on
   the first stand tick after getting up while timer ∈ (0,10).
4. **BF1 jump**: vy jumps to 8.0 exactly; horizontal fwd bonus
   = 8.0 × fwdInput × 0.25 along facing (FUN_00208b78). Landing sets timer
   10.6 (0.6 s no-transition) or 0.5 hard lock if airtime > 2 s.
   Gravity −18: apex after 8/18 = 0.444 s.
5. **BF2 roll impulse**: sprinting at 13.5 m/s (9×1.5), roll → speed becomes
   exactly 15.3 m/s in the velocity direction on the entry tick; energy drops
   by 10. From a slow walk (|v| ≤ 0.1) no impulse is applied.
6. **BF2 sprint ramp**: cap(t) = 9 + (13.5−9)·min(t/0.5, 1) while predicate
   holds; drops to stand cap when |strafe| crosses fwd·1.732.
7. **BF2 stance cycle**: stationary + crouch button: stand→crouch;
   again → stand (t>0.5 s gate). Moving (stick ≥0.9, not straight back) +
   same button → state 5.

Anim frame counts (diveforward, getdown/getup clips) are LVL data — pin them
via asset extraction or a PCSX2 peek (BF1: `[[DAT_00517ae8]+0xcc+id·4] → [ptr]
+8` u16 frames) before freezing tests 1/3.

## 6. Open questions (PCSX2 side-by-side should confirm)

1. **BF1 pad mapping**: which physical inputs write posture target +0x3c8
   (stance cycle?) and the dedicated rollL/rollR requests +0x3cc/+0x3d0 vs the
   byte path's jump+strafe predicate; i.e. is path A (FUN_0020aad0) the local
   player and path B the AI, or vice versa. Memory-write breakpoints on
   soldier+0x3cc / +0x2f0 settle it.
2. **BF1 roll from prone** via the +0x3cc command path — the state machine
   never dispatches rolls from prone, but FUN_0020aad0 doesn't obviously gate
   on stance; verify it's impossible in practice.
3. Exact **diveforward / getdown / getup frame counts** (data) → concrete roll
   and lock durations.
4. BF1 jump-velocity ODF key name (hash **0xb421860c** → class+0x183c) — code
   verified, name uncracked (it is *not* JumpHeight; BF2 uses JumpHeight).
5. CollisionScale (BF1 +0x185c trio) semantics — which value maps to which
   posture, and how BF1 derives per-stance capsule height (BF2's 1.9/1.6/0.55
   table is explicit; BF1's probe uses 0.55/1.35 constants in FUN_00208f60).
6. Whether BF2 blocks firing during state 5 (BF1's explicit skip was found;
   BF2's equivalent gate wasn't located — likely via the 0x1c00/0x800 lock
   flags or weapon event 6).
7. First-person view-anim configs (BF1 FUN_001911a0 keys RollLeft/RollRight/…)
   — where the data chunk comes from (CORE.LVL?) if we want camera feel parity.

## 7. Port notes for openbf soldier v0

- The locked design (one contextual button: moving=roll, stationary=cycle) is
  literally BF2's scheme plus BF1's prone: implement BF2's predicate
  FUN_00247b88 (stick ≥ 0.9, not straight-back) for the roll branch and a
  stand→crouch→prone cycle on the stationary branch.
- Recommended constants to carry (cite these addresses in code):
  ControlSpeed defaults table (BF2 ctor / BF1 call sites FUN_00205cc8),
  gravity −18, accel 70 (BF1 ground model ×1.2 / brake ×0.6·scale if BF1 feel
  is wanted; BF2's symmetric model is simpler), roll factor 1.7, roll exit at
  0.75×clip, BF2 stand-up heights {1.9, 1.6, 0.55}, prone pitch clamp ±0.4,
  jump vy 8.0 with fwd carry (BF1 ×0.25·input hardcoded vs BF2
  JumpForwardSpeedFactor 1.3 as a multiplier — pick BF2's).
- Choose one roll motion model: BF1 accel-ramp (strafe ±1.7) vs BF2 impulse
  (1.7×MaxSpeed). BF2's reads better for all-direction rolls.

## 8. Port: the prone double tap (`[ours]`, both games, the maintainer 2026-10-28)

A named deviation from both discs, on every map. The maintainer's rule: "have prone happen only if a troop taps
the crouch button twice within say 1 second. Otherwise if in crouch, you can jump to stand again, or crouch button
again to stand again. Same with prone." It replaces §7's stationary cycle Stand → Crouch → Prone → Stand, which put
a soldier prone on any second tap however late, and a crouched bot that tapped to stand up prone first.

With the stick idle (the roll predicate false), a ROLL_STANCE edge does (`Soldier::stance_tap_target`):

| stance | tap taken | result |
|---|---|---|
| Stand / Sprint | after `STANCE_TAP_WAIT` 0.5 s in the stance (as before) | Crouch, instant, no lock |
| Crouch | at once | **Prone** if `time_in_stance` ≤ `PRONE_DOUBLE_TAP` 1.0 s, else **Stand** (instant, no lock) |
| Prone | after 0.5 s and the `prone_down_lock` (40 updates) | Stand, `prone_up_lock` (45 updates, hard) |

JUMP edge: a crouch **or a prone** older than `CROUCH_RISE_WAIT` 0.5 s stands up and never takes off; the prone
through `prone_up_lock` (BF1's own prone case does this, `jump && t > 0.5` → the stand-up probe, §3.2; BF2's
prone did not). With the stick moving the button rolls exactly as before (§4.2, `Soldier::roll_predicate`);
sprint, the locks and the other stance arms are unchanged.

Why the window is the crouch's own `time_in_stance`: a crouch is entered only by the stand's tap and is instant,
so the time since the first tap IS the time in the crouch; no new state is carried or hashed. The crouch takes its
tap at once (it used to wait 0.5 s like every stance) so a quick double tap (press, release, press: two updates)
counts; the 0.5 s waits of the stand and the prone are kept.

Bots (`ai/mover.rs`, D13): the controller presses only where the soldier will take the press
(`Soldier::stance_tap_after` / `jump_stands_after` mirror the step's order: clocks advance, then the lock, the
edge and the wait are read), and **never a press whose result is Prone unless the unit wants prone**. Crouch: one
tap. Prone: two taps two updates apart. Up from prone: one tap. Up from crouch: JUMP after 0.5 s (a tap inside the
window would be the prone), or a tap once the window has passed. Measured on `round_to_outcome 18000 42 bf2_kas2
bf2_nab2 bf1_nab1 bf1_kam1` with a temporary stderr log at the soldier step: prone entries **571 before** (408 by
units wanting crouch, 99 stand, 64 no wish; none wanted prone) → **0 after**. Tests: `soldier/tests/prone_tap.rs`,
`ai/draw.rs` (`a_crouched_defender_given_a_route_stands_without_going_prone`, the prone Cover hint test).
