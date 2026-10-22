# OPUS RE (2026-10-22) - [O] UNAUDITED
AUDIT 2026-10-22 (loop lane 1): PARTLY - the clock, the 8 m/s line and the census hold (re-measured live); BF2's sprint
also turns the root toward the travel, and the droids' 11.50 m/s is the engine's fallback, which the port's zero-motion
bake must take (section 7).

# The clip a sprinting soldier plays: BF2's sprint pair and its clock, BF1's 8 m/s idle line, and the port's freeze

Card SPRCL, lane 2. Corpora: B1 = `analysis/SWBFRONT.ELF-decomp.c`, B2 = `analysis/SWBFRNT2.ELF-decomp.c`; ELF words by
`off = va - 0x160000 + 0x1000`; the discs' `zaa_` banks; 181 BF2 PCSX2 save states; two live PCSX2 runs (BF2 Geonosis,
BF1 Geonosis). Tags: **[V]** read at the cited lines, **[M]** measured, **[D]** derived, **[H]** hypothesis; `[O]` marks
the load-bearing claims. Scratch, tools and outputs: `engine/artifacts/lead_tmp/sprcl/`. Builds on
`re-soldier-anim-layers.md` (sets, players, §6.3 aliases, §7 other modes) and `re-stance-roll.md` §1 (BF1 has no sprint).

## 0. Summary

The maintainer (INBOX 2026-10-21 17:50, item 2): some soldiers freeze in the idle pose while they move at sprint speed.

1. **The freeze is BF1's, and it is the port's.** `[O]` On main f44cdcf a census that runs the port's own animator for every
   soldier class at 1.5 x MaxSpeed (the port's sprint) finds **BF1: 140 of 146 (class, weapon) rows of the whole class db,
   100 of 104 fielded (class, body, set) rows, on `human_rifle_stand_idle_emote` (legs) and the set's idle (upper); BF2:
   0 of 324 and 0 of 104.** The six BF1 rows that move are the Wookiee classes (SkeletonRootScale over 1.125). Cause: the
   port keeps BF1's chooser rule "over 8 m/s, idle" (B1 FUN_0022e990, faithful), and BF1's sprint - which the PS2 does
   not have (re-stance-roll §1) - is the port's own: 1.5 x a 6.0-7.0 m/s MaxSpeed is 9.0-10.5 m/s.
2. **BF2 always has a sprint pair, by the PS2's own fallback.** `[O]` In RAM, 0 of 2,777 set rows over 181 BF2 states have
   an empty sprint slot; a set with no sprint clip of its own takes `stand_runforward` (re-soldier-anim-layers §6.3 pass 2,
   seen in RAM on the pistol, melee and spear sets). The port's BF2 freezes nowhere.
3. **But BF2's sprint clip runs on the wrong clock in the port.** `[O]` The PS2 plays the sprint pair, both players, at
   `|v| / v_clip` - the speed over the LOWER sprint clip's root speed, a plain ratio (B2 FUN_0027caf0 case 0); the
   locomotion clips' law `k + (1 - k) 0.05` is a different one. Live: 118 sprint updates fit the plain ratio to a mean
   0.00005 of phase per update; the eased law misses by 0.0005, the port's plain clock by 0.009 (the feet cover 76-128 %
   of the ground at full sprint, 52-87 % at the sprint's start).

Fix (card SPRCL build, section 6): BF1's sprint skips the 8 m/s line (ours: BF1 sprints on the stand's run clips at any
speed); BF2's sprint pair plays at `|v| / v_clip(lower)`; the census kept as a test over every class. Knocked into a
scratch binary: BF1 140 -> 0 and 100 -> 0, BF2 stays 0, and the port then replays the PS2's 118 sprint updates to a mean
0.00004 (main 0.00902).

## 1. BF1: the idle line [V]

- `FUN_0022e990` (the movement chooser, header B1:144969) opens with *(decompiler detail omitted)* (B1:144989-144991) **before** it divides the two components by *(decompiler detail omitted)* (B1:144992-144995). `[O]` The 64.0
  is `lui at,0x4280` at 0x22e994 (word 0x3c014280).
- Its caller `FUN_0022aa10` case 0x20 (B1:142603-142640) hands it the animator's move dotted with two of its rows, each
  over the frame time *(decompiler detail omitted)* (`DAT_005644e0`, B1:142349, 142367; B1:142617-142618; both forced to 0 when
  *(decompiler detail omitted)*, B1:142619-142622), and *(decompiler detail omitted)* as *(decompiler detail omitted)* (the SkeletonRootScale,
  re-soldier-skeleton-skins 7). A 0 result keeps the previous movement id if it is 4 or less (an idle or a turn), else 0
  (B1:142632-142633). So a BF1 soldier faster than 8 m/s - measured before the scale - stands in an idle.
- BF1's animator has no sprint id: the 31 special names at 0x517af0 (`jump, jump_up, landsoft, landhard, fall`, 13
  `thrown_*`, `stand_getupfront / back, diveforward, jetpack_hover`, four prone transitions, five `stand_death_*`) and
  the six upper actions at 0x517be0 hold none, nor do the movement (0x517b70), stance and set lists beside them
  (two verifiers' dumps). The string `sprint` is in no B1 / B2 decomp line and in the BF1 ELF only inside
  `ScriptCB_usprintf` [receipt:14d649b3]. With re-stance-roll §1 (no sprint state): **BF1 has no twin of BF2's sprint
  case.** `[O]`
- No BF1 soldier bank holds a sprint clip `[M]`: the CRC-32 of 1,188 `<skeleton>_<set>_<anim>[_full|_upper|_lower]`
  names inside the 41 `zaa_` chunks of every BF1 LVL (`sprint_clip_scan_zaa.py`, `clip_scan_bf1_zaa.txt`): one sprint
  name matches once (`gungan_rifle_sprint`, COMMON.LVL 0x26596c), a chance match inside key data (the next u16 would be
  65,372 frames; 0.4 chance hits expected); the controls match (6 `human_*_stand_runforward`, 5
  `human_*_stand_idle_emote`); the same scan of BF2 finds `human_rifle_sprint_full` and the other sprint clips. A
  verifier's structural parse of every BF1 MINA clip table (413 entries, 312 CRCs, 22,848 names with 11 sprint
  spellings) found none either. The CRC words of `human_rifle_sprint[_full]`, `human_bazooka_sprint` and
  `human_tool_sprint` are in neither ELF [receipt:7c0e02f9]. The port's 21 BF1 `sets.ron` name no sprint clip.

## 2. BF2: the sprint pair and its clock [V][M]

- `FUN_0027caf0` (the pose driver, header B2:200847) switches on the animator's mode `+0x20b8` (B2:200962-200964). Mode 0
  is simple id 0, `sprint` (table 0x5fde48 starts with a pointer to "sprint"; B2:222734). **Case 0** (B2:200965): the
  sprint pair's LOWER slot (`FUN_00281510`, B2:200967; ELF `jal 0x281510` at 0x27d6c0), rate 1.0 (B2:200969), and while
  dt > 0 (B2:200970) **rate = planar speed / (slot word bits 0-14 x 0.01)** (B2:201048) - the lower clip's forward root
  speed in cm/s. `[O]` The 0.01 is the pooled word at 0x6206c0 (0x3c23d70a), `lwc1 f1,0x6c0(v1)` at 0x27d848, then
  `cvt.s.w / mul.s / div.s` at 0x27d858-0x27d868. It calls `FUN_0027d9a8(dt, rate, ...)` (B2:201050, `jal 0x27d9a8` at
  0x27d884) and stores 6 (`runforward`) as the movement id `+0x20c0` (B2:201051, `sw v0,0x20c0(s2)` at 0x27d894).
- The speed: the animator's move `+0xc0` dotted with its rows 2 and 0, each over dt (B2:201033-201034), and the root of
  their squares (B2:201036-201046) - the velocity `FUN_0027dfe0` hands the chooser (B2:201744-201759), **not divided by
  the root scale** (the chooser divides; case 0 does not). dt = `DAT_00661378` (B2:200881) plus a kept remainder
  (B2:200911).
- `FUN_0027d9a8` (B2:201394) plays that lower slot with `FUN_0027f820(dt x rate)` (B2:201449, 201453) and, with no
  weapon action, the pair's UPPER slot with `FUN_0027f458(dt x rate)` (B2:201478-201479): **both players at dt x rate.**
  `[O]` (While a mode-7 custom slot or an action slot is in play the players take *(decompiler detail omitted)* instead,
  B2:201467, 201504; neither happens in a sprint: case 0 passes no action.)
- A null lower slot (no custom slot either) makes it return at once, before any clip is set or the pose written
  (B2:201450-201451) - the soldier would keep his last pose. It never arises on the shipped data: **0 of 2,777 set rows
  over the 181 BF2 states (183 files; the two menu states load no bank) have an empty sprint slot on either layer**
  (`sprint_ram_bf2.py`, `sprint_ram_bf2.txt`; a verifier's own scan without any filter: 0 empty, 0 null clips). `[O]`
  The pass-2 alias `sprint <- stand_runforward` fills them: the pistol sets (but bdroid's, which has its own
  `bdroid_pistol_sprint_full`), the melee set and the Ewoks' spear set sprint with a `*_stand_runforward` upper.
  `human_rifle_sprint_full`'s slot word reads 9.16 m/s (the bake: 9.1627); bdroid / sbdroid sprint clips 11.50.
- The locomotion clips use another law: `k + (1 - k) x 0.05`, k = v / v_clip (B2:202349, `FUN_0027ed18`).

**Live** `[M]` (`ps2/sprint_strip.py`, a copy of GTHRW's throw strip: one row per game update of the soldier and his
animator, video beside; BF2 Geonosis no-AI state, clone trooper, MaxSpeed 7.0; `ps2/rate.py`, `ps2/rate_bf2.txt`): two
sprint trials (L3 held 2.0 s while running), 59 + 59 updates in mode 0 with `human_rifle_sprint_full` on BOTH players:

| law for the lower clip's phase step (phase `+0x2074`, 15 frames) | sprint | sprint_2 |
|---|---|---|
| dt x v / 9.1627 x 30 / 14 (case 0) | mean 0.00005 | 0.00006 |
| dt x (0.95 v / 9.1627 + 0.05) x 30 / 14 (locomotion) | 0.00048 | 0.00049 |
| dt x 30 / 14 (the port today) | 0.00904 (max 0.0147) | 0.00904 |

Control: the run trial (mode 0x27, `human_rifle_stand_runforward`, 6.69 m/s) fits the locomotion law to 0.00001 over 72
updates, and the sprint law misses it by 0.0001. The speed ramps 7.0 -> 10.5 m/s over the 0.5 s SprintAccelerateTime.

## 3. The port today [V][M]

- `game/src/animator.rs`: `stance_of` sends `Stance::Sprint` to `Special::Sprint` on BF2 (line 533) and to the stand's
  feet on BF1 (line 536); `choose` divides the velocity by the scale (line 339) and then returns None over 8 m/s on BF1
  (lines 362-363); `step` maps only a WalkForward pick to RunForward for a BF1 sprint (lines 1165-1169) and None goes to
  `turn_on_spot` (line 1174). So a BF1 sprint over 8 m/s stands in the idle. `[O]`
- The rate factor `walk` is the PS2's locomotion law for a moving movement clip only (lines 1222-1227); a `Body::Special`
  plays at dt x 1.0 (lines 1229-1233; two verifiers: no other speed factor anywhere on the players). BF2's sprint
  therefore runs on the plain clock. `[O]`
- **Census** `[M]` (`openbf-wt/sprcl-scratch` example `sprint_census`, kept as `lead_tmp/sprcl/sprint_census.rs`; every
  class of the 57 shipped missions' rosters through `FactionRoster::pair` + `SoldierVisuals::load_cpu`, and every
  soldier class of each game's four-faction db with a baked body; every weapon slot; 45 updates of `Stance::Sprint`
  straight ahead at 1.5 MaxSpeed; a row freezes when either player's clip is no sprint / run / walk clip):
  BF1 140 / 146 db rows and 100 / 104 fielded rows freeze (both players on `*_stand_idle_emote`), BF2 0 / 324 and
  0 / 104 (`census_main.tsv`). Not covered: 5 BF1 and 9 BF2 hero classes that do not build or have no baked body.
- **Knock-in** `[M]` (the two rules of section 6 behind `SPRCL_KNOCK`): BF1 0 / 146 and 0 / 104, BF2 0 / 324 and 0 / 104
  (`census_knock.tsv`); without the switch the output equals main's byte for byte (`census_null.tsv`). The PS2's 118 sprint
  rows (dt, speed) fed through the port's animator on BF2 `rep_inf_ep3trooper` (`sprint_replay`): phase step against the
  PS2's, main mean 0.00902 (max 0.01477) -> knock-in 0.00004 (max 0.00009).
- BF2 PS2 sprint rate per class at full sprint (port census speeds, `bf2_sprint_rate.txt`): `human_rifle_sprint_full`
  (60 classes) 0.98-1.31, bdroid / sbdroid clips 0.78-1.04; at the sprint's start (MaxSpeed) 0.52-0.87. The port: 1.00.

## 4. Ours and provisional

- **BF1's sprint is ours** (the PS2 has none): it keeps the stand's movement clips (the run, with the legs' twist, the
  alert rows and an action over it, as today) and does not take the 8 m/s idle line while sprinting. BF2's own rule for a
  set without a sprint clip is the same picture (`sprint <- stand_runforward`). Its clock stays the locomotion law
  (0.95 k + 0.05; at 9.75 m/s on a 6.7 m/s run clip 1.43 against the plain ratio's 1.46). [D] To reverse: the BF1 arm
  in `step` / `choose`.
- The 8 m/s line otherwise reads the velocity BEFORE the root scale (B1:144989 before 144992); the port reads it after.
  It matters only for a scaled BF1 class between 8 and 8 x scale m/s outside a sprint (the Wookiee, 6.0 m/s MaxSpeed,
  does not get there on foot) [D].
- The port's sprint speed (1.5 x MaxSpeed for every class, `core::soldier::CONTROL_SPEED`) is not this card's; the
  census uses it.
- [H] The bake's root speed (9.1627) against the PS2's slot word (whole cm/s: 9.16): 0.03 %, not built.

## 5. Checks run

- `re_check.py fn`: B1:144990 / 144993 in FUN_0022e990; B1:142613 / 142626 / 142629 in FUN_0022aa10; B2:200968 /
  200970 / 201040 / 201050 in FUN_0027caf0; B2:201447 / 201449 / 201456 / 201486 / 201494 in FUN_0027d9a8; B2:202186 in
  FUN_0027ed18.
- `re_check.py word`: b1 0x22e994 = 0x3c014280 (`lui at,0x4280`, 64.0); b2 0x6206c0 = 0x3c23d70a (0.01); b2 0x27d6c0 =
  0x0c0a0544 (`jal 0x281510`); b2 0x27d848 = 0xc46106c0 (`lwc1 f1,0x6c0(v1)`); b2 0x27d858 / 5c / 68 = cvt.s.w, mul.s,
  div.s; b2 0x27d884 = 0x0c09f66a (`jal 0x27d9a8`); b2 0x27d894 = 0xae4220c0 (`sw v0,0x20c0(s2)`).
- `re_check.py absent`: [receipt:7c0e02f9] (four sprint clip CRC words, both corpora and both ELFs: 0); [receipt:14d649b3]
  (`sprint`: 0 decomp lines; BF1 ELF 1 raw hit = `ScriptCB_usprintf`, BF2 ELF 2 = the name table's "sprint" and usprintf).
- Twin: BF1 has no sprint state or id (section 1), so no twin of case 0 is claimed.
- Writers: none flag-gated here; the mode `+0x20b8` = 0 while sprinting is read live (stance 3, mode 0 on all 120 rows).

**Verification** (workflow `sprcl-re-verify`, 7 agents at medium effort, claims and citations only): 25 verdicts on 20
claims - 17 SUPPORTED, 7 PARTLY, 1 NOT SUPPORTED. NOT SUPPORTED: the 0.01 I first cited (`lui/ori 0x3c23d70b` at
0x27d008) belongs to another branch; case 0 loads its own from 0x6206c0 (fixed above, re-read by me). PARTLY, folded in:
the thrown_* count (13, not 15; both absence verifiers), the null-slot return's scope (only without a custom slot; both),
the pistol sets' alias (bdroid's has its own clip), a line number (B2:202349), and the action exception of the port's
rate. The two absence verifiers of each group agreed on every claim; none was downgraded to [H].

## 6. Build (card SPRCL, phase build)

1. BF1: while `Stance::Sprint`, the chooser takes the forward run past the 8 m/s line (`choose` gets the sprint flag;
   the line otherwise reads the unscaled speed, B1:144989).
2. BF2: `Body::Special(Special::Sprint)` plays both players at `|v| / v_clip`, v_clip the lower sprint clip's forward root
   speed (`Rig::clip_speed`), v unscaled; 1.0 when none is baked (B2:201048, 201453, 201479).
3. Tests: the census over every class and weapon of both games (data-gated on the bake): 0 rows on an idle or turn clip
   at sprint (main: BF1 140 / 146); the PS2's sprint rows (a fixture from `ps2/rate_bf2.txt`) through the BF2 trooper's
   rig within 0.0002 of phase per update (main 0.009); a synthetic-rig BF1 test at 9.75 m/s. Mutation-check each.
4. Picture strip: the port's BF2 clone trooper sprint beside `lead_tmp/sprcl/ps2/out/bf2_sprint.mkv`, the port's BF1
   trooper sprint beside the PS2's BF1 run `bf1_run.mkv` (the same run clip; BF1 has no sprint).

## 7. AUDIT 2026-10-22 (loop lane 1): PARTLY - the clock and the census hold; the sprint turns the root, and droids divide by a fallback [V][M]

Re-derived before the session's report was read. Live rows: the auditor's own sampler
(`engine/artifacts/lead_tmp/audit-sprcl/tools/tsamp.py`, GTHRW's audit sampler: one raw row per game update of the
soldier block and the animator the soldier points at) and decoder `tools/sdec.py` (clip names by CRC-32); BF2
Coruscant ai0, rebel trooper (MaxSpeed 7.0), SW renderer (the RTX was lost). Seven trials: sprint straight, sprint then
stick 45 deg right, sprint then 45 deg left, sprint while turning, a pistol sprint (R2 switches to the pistol), a rifle
control, a run diagonal (`ps2/run1/`, `ps2/run2/`, fits `ps2/mutfit.txt`). Workflows `audit-sprcl-re` (13 agents) and
`audit-sprcl-verify2` (2 separate readers of 7.2-7.4's new claims): `wf/results.json`, `wf/verify2.json`.

What holds, reproduced: §1's line (B1:144989 before 144992; two readers), BF1 has no sprint (31-slot special list,
§1's keys - BF1 holds none of BF2's sprint class keys SprintAccelerateTime 0x56389879, EnergyDrainSprint 0x134361b9,
EnergyCostJumpSprint 0xa44b8f28, EnergyMinSprint 0xf03ffabb, BF2 has each [receipt:141fe3cb]; an independent parse of
every BF1 MINA clip table, 413 entries, 396 resolved, none a sprint, the 17 left ruled out by their sorted slots);
case 0's law (§2, every reader SUPPORTED; the speed is sqrt of x^2 + z^2 only, vmul.xz / vaddz.x / vsqrt at
0x27d814-0x27d824); the RAM census (0 of 2,777, re-run byte for byte and by a second reader written from
FUN_002814f0 / FUN_00281510); the port census on main 9193a4c (BF1 140 / 146 and 100 / 104, BF2 0 / 324 and 0 / 104,
byte-equal to `census_main.tsv`), the knock-in (0 everywhere, null byte-equal) and the replay (0.00902 -> 0.00004).

### 7.1 The law, measured on both players with clips that differ [M]
Over the auditor's 412 mode-0 update pairs per player: phase step = |m_xz| / 9.16 x 30 / (F - 1), m_xz the move +0xc0
on the animator's own rows +0x00 / +0x20, mean |err| 0.000000 (max 0.000000). dt cancels: the step does not depend on
the frame time. The mutants all miss: the bake's 9.1627 0.00002; the forward component m_z instead of |m_xz| 0.006 on
the 47 diagonal pairs; the upper clip's own root speed 0.0215 on the 74 pistol pairs (upper
`human_pistol_stand_runforward`, 19 frames, advanced at the LOWER `human_rifle_sprint_full`'s rate); the locomotion law
0.0004-0.0006; 30 / F 0.004; the port's clock 0.008-0.013. So the divisor is the slot's whole cm/s (9.16, §4's [H]
settled; the session's 0.00005 was this constant bias - an agent found exactly 9.16000 in the session's own rows), the
speed is the planar magnitude, and the upper player runs at the lower clip's rate whatever its own clip.

### 7.2 The sprint turns the root toward the travel [V][M] `[O]`
Case 0 also calls `FUN_0052b090` (atan2f; its error path names "atan2f") on the local x / z velocity (jal at 0x27d830,
f12 / f13 kept from the divisions at 0x27d7f4 / 0x27d800) and hands it to `FUN_0027d9a8` as its third float (f14,
0x27d878). There, B2:201534-201548: the animator's +0x20cc eases toward it (0 instead when byte +0x20e6 & 4) by
min(1, 7.5 dt), and `FUN_00280048` (B2:202741-202775) writes it as a yaw quaternion (0, sin a/2, 0, cos a/2) on the bone
whose CRC is `dummyroot` - the root twist [V; R2, R3 and both second readers SUPPORTED]. Live: all 412 sprint pairs
(69 with the travel over 1 deg off ahead) fit the ease to 1.9e-7 deg (max 3.5e-6; `ps2/twistfit.txt`); the stick at
45 deg gives a travel of -14.04 / +14.04 deg and the root follows it, 0.25 of the gap per update; after a turn the
travel reached 45 deg off the facing (a wall slide [H]). The locomotion driver eases the same way toward the travel
relative to the chosen direction clip (`FUN_0027dfe0` B2:202058-202069; its target is written by `FUN_0027ed18`
through a pointer the export hides, MIPS 0x27e1c0).
The port's `Body::Special(Sprint)` eases the twist to 0 (`game/src/animator.rs` 1255-1260) while its sprint stays on
up to 60 deg off ahead (`core/src/soldier.rs` 582-585): its sprinter crabs with a straight root. §6 omits this.

### 7.3 11.50 m/s is the engine's sprint fallback, not the droid clips' speed [V] `[O]`
`FUN_00282900` (B2:204581) measures a slot's clip with `FUN_00282700` - the `dummyroot` translation from frame 0 to
half the clip, z to bits 0-14, x to 15-29 (B2:204527-204576) - only for a record with the lower bit 31 set
(B2:204602), and for anim id 0 (`FUN_002a01b8` returns its argument below 0x26) replaces a forward speed <= 0 with
11.5 m/s (B2:204683-204695; `lui at,0x4138` at 0x282a18, Ghidra's 17533.5 = 11.5 x 100 + 16383.5) [V; V-A PARTLY on
the range only, V-B SUPPORTED]. The slots reading 11.50 (bdroid / sbdroid rifle, bdroid pistol, aalya sabre) are this
fallback. The port bakes `bdroid_rifle_sprint_full`, `bdroid_pistol_sprint_full` and `sbdroid_rifle_sprint_full` with
`root_motion: (0.0, 0.0, 0.0)` (side_bf2/soldiers/cis_inf_*.anims.ron), so §6's "1.0 when none is baked" would play a
BF2 droid's sprint at 1.0 where the PS2 plays |v| / 11.5 (0.78-1.04 at full sprint, 0.52-0.70 at its start).

### 7.4 Wording and counts
- §3: 22 BF2 hero classes are not covered by the census (15 do not build, 7 have no body), not 9.
- §1: the special list has 31 slots and 30 names (0x517b68 repeats `stand_death_backward`). "`sprint` is in no decomp
  line" holds for whole identifiers only: B2:222734 is the label `PTR_s_sprint_005fde48`, and both corpora have
  `ScriptCB_usprintf` lines. The CRC-word search of four sprint clip names [receipt:7c0e02f9] has no positive control:
  clip names are formatted at run time (B1 `FUN_0022d828`, "human_rifle_%s"), so names both games use are missing as
  words too; the MINA parse above is the evidence.
- §1: while a weapon action is up (`FUN_0022f280` non-null, id <= 2) a fast BF1 soldier's legs play the action clip,
  and a stored turn id 3 / 4 keeps its turn clip (B1:142632-142644, two readers).
- §2: the ramp lasts 0.35 s (0.333 m/s per update in the session's captures and the auditor's): com_inf_default
  authors `SprintAccelerateTime` 0.35, which the port reads; 0.5 is the constructor default. In both sets of captures
  the speed also drops to MaxSpeed and ramps again after a 2 cm step down (10.50 -> 8.16 -> 7.66 m/s) [M]; not chased
  (movement).
- §2: the run control cannot separate the two laws' shapes (its speed is constant); the sprint rows separate them.
- Not this doc's: the PS2's sprint speed is ControlSpeed's sprint factor x MaxSpeed per class (`FUN_00240bf8` case 3,
  B2:160477-160481; ctor 1.5, heroes author 2.5-5.0); the port's fixed 1.5 is card CSPD's.
