# OPUS RE (loop lane 2, 2026-10-21, card HCORN) - [O] UNAUDITED
# RE: a hover's detour corners - the radius they are planned at, where the corner walk starts, and what the PS2's own bikes do at a corner already inside their node radius

**The question** (card HCORN; AUDIT HJAM 2026-10-16, `engine/artifacts/lead_tmp/audit-hjam/EVIDENCE.md`). Card HJAM gave
a hover's driver the PS2's node radius at detour corners (ForwardSpeed x 0.5: 20 m for BF1's speeder bike;
`re-hover-ai.md` 5). A leg that starts with its first corner already inside 20 m drops that corner at once; on BF1
Tatooine (tat1) 63 of 72 such starts of leg 54 -> 55 died within 2 s against barrier267's rock (1 of 56 before HJAM).
Open: (a) the corner's offset for a hover; (b) whether barrier267 (`flags: 31`) blocks a hover at all; (c) whether the
PS2's obstacle probe turns a bike off this rock; (d) where the PS2's own BF1 Endor bikes crash.

**Method.** Static: the two games' leg request and unit radius, in the decomp and the disassembly. Live: the path nodes
the PS2 hands its hovers, read every game tick - card HAIS's captures (`lead_tmp/hais/live_b1end1.csv`, BF1 Endor 5
min; `live_end1 / kas2 / geo1.csv`, BF2) and a new one, `lead_tmp/hcorn/ps2/live_b1tat1.csv` (BF1 Tatooine: Dune Sea,
GCW, 599.6 s of mission time, every soldier a bot; `ps2/run_tat1.sh`, the software renderer: the RTX was lost) - and
one RAM read of two save states of that battle, and the disc's model chunks. Port: a scratch prototype on main c6bc4f3 with three switches,
measured over 24 + 24 seeds (`lead_tmp/hcorn/`, section 5). Tools: `lead_tmp/hcorn/tools/` (`nodes.py` corner
offsets, `nodeseq.py` node changes, `legfrom.py` where a leg's corners are seen from, `passes.py` passes by a barrier,
`contact.py` fast barrier contacts, `bounds_ram.py`, `glbbox.py`, `ab.py` the prototype's bars).

## 0. The answers [O]

| question | answer |
|---|---|
| does the PS2 drop a corner already inside R? | **Yes - HJAM's rule is the PS2's.** Of 395 node changes that leave a corner on BF1 Endor, 389 happen 2-20 m from it; all 67 hand-overs of a corner already inside 20 m (Endor) and all 15 on tat1 are left after one tick (2). |
| does it kill the PS2's bikes? | **Yes, 5 of BF1 Endor's 15 crash kills** come within 2 s after such a hand-over on the same bike (barrier646 twice at (-160, -4, 297), barrier954 twice, barrier1039) (3). |
| (a) the corner's offset for a hover | **The hover's own radius r + 0.5 m** from both barrier edges (the soldier's rule, `FUN_00267ab0`), with r = half the X/Z diagonal of the class model's box (B1 `FUN_00235ad0` / B2 `FUN_002a76b8`, kind 6 = EntityHover: x 0.5 where every other kind takes x 0.25; BF1's box is the model's `dummyroot` INFO min / max, read in RAM and found on the disc) (1). Live: BF1 bike 2.94 (130 of 137 Endor corners; r 2.44), BF1 tat1's other two classes 5.42 / 6.08, BF2 bike 2.45, BF2 Kashyyyk 4.39 / 4.83, BF2 hailfire 6.89 (2). The port: 1.5 m for every unit (`ai::DETOUR_RADIUS` 1.0 + 0.5). |
| where the corner walk starts | **At the waypoint just reached** (old waypoint -> next), not where the unit stands: B1 `FUN_002509b0`, B2 `FUN_002c0220` (1); live 200 of 200 Endor legs and 54 of 54 tat1 legs have their first corner on the silhouette seen from the previous node, 42 / 9 of them NOT on the silhouette seen from the bike (2). The port plans from the unit (`route::fill_detour(.., pos, ..)`). |
| (b) barrier267 `flags: 31` | 0x1f includes the hover bit (0x2): a hover plans round it. The PS2 handed its tat1 bikes barrier267's corner (-146.89, 45.38) - the corner at 2.94 m - as a node (4). |
| (c) the probe at this rock | Not settled [H]: on the PS2 the deadly arrival never happened (4), so the probe was never put to it there. |
| (d) the PS2's crash spots | All 15 BF1 Endor crash kills were 1.6-5.3 m from a hover barrier's rectangle (12 within 3 m): the PS2's bikes, too, die on barrier-covered obstacles (3). |
| tat1's rock on the PS2 | 84 rides, 7 crash kills in 10 min, **none within 25 m of barrier267**; 11 piloted passes within 20 m of it, the nearest 4.70 m (4). The port with HJAM: about 2 deaths a round there (5). |
| what makes the port's rock deadly | The corner walk planned from the bike: a bike reaching hub 56 fast from the south-east gets corner (-148, 44) 6.5 m off, drops it, and hits the rock at 37 m/s - 41 of 50 / 54 of 61 such starts die. Planned from the waypoint (the PS2's rule) the leg has no corner, the same bikes still hit the rock but 4 of 50 / 6 of 45 die (5). |

## 1. The leg request: from the waypoint, at the unit's radius [V]

**BF1.** `FUN_002509b0` (the GroundGotoHelper's advance, `RE_LOG` "How a leg is walked, BF1") copies the follower's
current waypoint (`FUN_00267538` = f+0x40, B1:169770) before advancing it (`FUN_00267540`, B1:169775), reads the next
after (B1:169781), and calls `FUN_0026c0d0(r, unit, &from, &to, mask, 1, f+0x50)` (B1:169809) with r = `FUN_0024f128`
(B1:169807) and mask = `FUN_0024f160` (B1:169808) (verifier: 0x250a78 / 0x250a88 / 0x250aa4 / 0x250b60). On the
fallback path (the next waypoint in a barrier, `FUN_002686c0` == 0 -> `FUN_00267648` re-roll) `to` is read again
after the re-roll. `FUN_0026c0d0` has three `jal` words in the ELF (0x24eff0, 0x24f528, 0x250b60), no data word, no
`j` [receipt:b6b9f8c0]. `FUN_0024f128` (B1:168594) = `FUN_00235ad0(x)`, x = the controller's +0x294 vtable slot +0x1c
object; `FUN_0024f160` (B1:168608) = `FUN_00235be8(x)` = *(`FUN_002218d0`(x) + 0x78) = the class's AISizeType.
`FUN_00235ad0` (0x235ad0, B1:149971): the length of the X and Z parts of (bounds+0x90 - bounds+0x80) with bounds =
(vtable+0x38)(x)->+0x1c (`VMUL.xz` 0x235b20, `VADDz.x` 0x235b24, `VSQRT` 0x235b28), x 0.5 when `FUN_00234c00` gives 6
(0x235b48-0x235b54, `lui 0x3f00`), else x 0.25 (`lui 0x3e80` 0x235bc0) save one kind-2 case. `FUN_00234c00`
(B1:149226-149284) returns 6 for DAT_00584ba8 (EntityHover, `RE_LOG` "four BF1 queries"). The corners:
`FUN_00268060` takes them from `FUN_00267ab0` = `FUN_002679f0(r, 0.5)` (B1:187498, 187134), the crossing test
`FUN_00267d18` uses `FUN_002679f0(r, 0)` (B1:187341) (`RE_LOG` 2026-10-09 OBST2, re-verified).

**BF2 (the twin).** `FUN_002c0220` (B2:245847): the same copy (`FUN_002ed2c8` = f+0x40, B2:245916), advance
(`FUN_002ed2d0`, B2:245921), re-read (B2:245925), the same re-roll fallback (B2:245927-245937), and
`FUN_002f3528(r, .., &from, &to, mask, 1, ..)` (B2:245955) with r = `FUN_002a76b8` (B2:245953), mask = `FUN_002a7830`
(B2:245954) - but only when the follower's link flag word (*(f+0x4c)+0x50 -> +0xc) is 0; a set flag takes another
path (B2:245948-245959, not read) [H]. `FUN_002f3528` has three `jal` words, no data word [receipt:b6b9f8c0].
`FUN_002a76b8` (B2:228108): class+0x370 when > 0; for kind 6 (`FUN_002a6138`) sqrt(a^2 + b^2) x 0.5 of the floats at
+0xbe4 / +0xbe8 of the vtable+0x34 object when +0xbe4 > 0, else the bounds' X/Z length x 0.5 (`VMUL.xz` 0x2a778c,
`lui 0x3f00` 0x2a77ec), not cached; other kinds x 0.25, cached at +0x370 (0x2a780c).

**The radius in RAM (BF1).** In both tat1 save states every hover's slot +0x38 is 0x1e5d88 = `jr ra; lw v0,0x240(a0)`:
the bounds are *(*(E+0x240)+0x1c) +0x80 / +0x90. The bike (K 0x16bb900): (-1.25, -0.55, -1.45) .. (1.25, 1.35, 2.74),
r = 2.439; K 0x15806f0: (-2.5, -1.86, -5.98) .. (2.5, 2.5, 2.5), r = 4.920 (`ps2/bounds_ram_*.txt`). Plus 0.5 these
are the two classes' live offsets, 2.94 and 5.42, to the millimetre. *(E+0x240) is the class's model record, and x is
E: the driver's controllable is E+0x2a0, whose +0x294 table (0x531468) has at slot +0x1c `addiu v0,a0,-0x2a0` (verifier
2, by the code; the agent -> controllable pointer chain was not followed in RAM). **The box is the model's own**: the
same six f32 words, min then max, stand in a `dummyroot` node's INFO chunk of the class's model - the bike's in
`SIDE/IMP.LVL` @0x931ac and `SIDE/REP.LVL` @0x138738, K 0x15806f0's twice in `SIDE/ALL.LVL` (@0x108a78, @0x113eb8)
(`tools/bbox_disc.py`; verifier 2 found it after this doc's first search, on rounded print values, came back empty).
The port's baked GLB boxes of the BF1 bike give 1.95 (collision) / 2.01 (visual) - not this box. **BF2**: half the X/Z diagonal of the baked COLLISION model's
box is 6.393 (hailfire), 4.328 (fighter tanks), 1.950 (speeder bike) = the live 6.89 / 4.83 / 2.45 less 0.5
(`bf2_collision_boxes.txt`), and Kashyyyk's 4.39 class (K 0x1a8aef0) is `all_hover_tank.glb`'s 3.889 + 0.5 (verifier 2).
Whether BF2 reads the same model INFO box (its bounds come through vtable +0x40 -> +0x1c, `FUN_002a76b8`) or the
collision model's is [H]: the four classes cannot tell them apart here.

## 2. Live: what the PS2 hands its hovers [O]

| capture | corner nodes (piloted, distinct) | offset k (count) | corner exits within 20 m | hand-overs inside 20 m, left after 1 tick | first corner seen from the previous node / from the bike |
|---|---|---|---|---|---|
| BF1 Endor, 5 min | 137 | 2.94 (130; 2.93-2.94: 132) | 389 of 395 (6 at > 40 m: re-plans) | 67 / 67 | 200 / 158 of 200 |
| BF1 tat1, 10 min | 127 | 2.94 (41) bike; 5.42 (44) K 0x15806f0; 6.08 (25) K 0x17ca210 | 67 of 67 | 15 / 15 | 54 / 45 of 54 |
| BF2 Endor / Kashyyyk / Geonosis | 76 / 108 / 23 | 2.45 (70) / 2.45, 4.39, 4.83 (27, 32, 40) / 6.89 (20) | - | - | - |

`nodes.py` counts a node as a corner when its offsets outside both half-extents of one barrier agree within 5 cm;
the BF2 rows read the active follower's segment point m+0x20 (`sbx/sbz`). `legfrom.py`'s silhouette agrees with a
brute-force silhouette test on 20,000 random points per map (verifier). **The mover point** of a BF1 hover is the BF2
look-ahead (`re-hover-ai.md` 5, `FUN_002a5fa8`): of the 10,839 Endor rows where it cannot reach the node, 10,266 lie
within 0.5 m of the previous node -> node line, 7,280 within 1 m of projection + |v| + 0.2 (speed from one-row
position steps) - the port's `hover_drive::look_ahead` holds for BF1 too.

## 3. The corner drop kills the PS2's own bikes [O]

`hais/loss_b1end1.txt` (BF1 Endor, 15 crash kills with a rider, `re-hover-ai.md` 11) against `end1_nodeseq.txt`: the
kills at 25.21, 25.44, 87.87, 93.78 and 208.42 s each come within 2 s after a hand-over, on the same bike, of a corner
already inside 20 m (24.57, 24.81, 86.17, 92.54/92.57, 207.89 s); no other crash kill has one. Of the 67 such
hand-overs, 6 bikes were lost within 2 s (9 %). Every one of the 15 kills was 1.6-5.3 m from a hover barrier's
rectangle (12 within 3 m, none within 1 m: the bike's half-length is about 2.7 m).

## 4. barrier267 on the PS2 [O]

`ps2/live_b1tat1.csv` (`ps2/loss_b1tat1.txt`): 84 rides, 7 crash kills with a rider (0.083 a ride), none within 25 m of
barrier267's centre (the nearest 26.4 m). `passes.py`: 11 piloted passes within 20 m of its rectangle, the nearest
4.70 m, none under 3 m; 6 node changes over 20 m/s near it in 10 minutes. Bike 0x1ea0360 at 161.5 s was handed
(-136.3, 34.4) -> corner (-146.89, 45.38) -> (-161.3, 53.5): the waypoints of hubs 91 (radius 17.2, centre
(-123.5, 37.8)) and 56, and barrier267's north-east corner at 2.94. Its one hand-over of barrier267's corner inside
20 m was a bike at 1 m/s just boarded.

## 5. The port, and why its bikes die at the rock [D]

**Traced** (`audit-hjam/bb/tr_x0.err`, HJAM build, seed 2, unit 0): on arc 111 -> hub 56 (from hub 91's CENTRE: the
port gives a driver the hub's centre, `route.rs` 959-966) the bike swings 13 m south of its line and reaches hub 56
(20 m) at (-144.6, 37.1), 37 m/s, heading west at the rock. The next leg is planned FROM THE BIKE (`fill_detour(..,
pos, ..)`) at 1.0 m: the bike -> waypoint 55 line crosses barrier267, so it gets corner (-148.5, 44.1), 6.5 m away;
next tick the corner is dropped (HJAM), the goal is the look-ahead on corner -> waypoint 55, and the bike dies against
the rock 4 ticks later. **Prototype** (`lead_tmp/hcorn/hcorn_x_prototype.patch` on c6bc4f3; HCX bit 1 = a hover's
corner walk at r (HCR = 2.44, the BF1 bike's), bit 2 = from the waypoint reached, bit 4 = drivers' waypoints drawn in
the hub like a soldier's; HCX=0 equals main's seed-42 rows for both maps, hash for hash, and so does seeds 25-48's
seed 42), 24 seeds each, `ab_bars.txt` / `ab_bars_b2.txt` / `contact_port.txt`:

| variant | tat1 rock deaths (s1-24 / s25-48) | tat1 bike crashes per ride (s1-24 / s25-48) | bf1_end1 crash per ride (s1-24 / s25-48) | fast ticks within 1 m / 3 m of a hover barrier: tat1, bf1_end1 |
|---|---|---|---|---|
| PS2 | 0 in 10 min | 0.08 (4 / 49) | 0.28 (15 / 54) | 0.69 / 3.50 %, 2.09 / 10.65 % |
| main (HJAM) | 41 / 54 | 0.07 / 0.11 | 0.297 / 0.278 | 0.27 / 3.51, 3.29 / 13.59 |
| from the waypoint | 4 / 6 | 0.04 / 0.04 | 0.255 / 0.255 | 0.35 / 3.66, 3.38 / 13.00 |
| + r 2.44 (both PS2 rules) | 6 / 4 | 0.03 / 0.03 | 0.136 / 0.129 | 0.34 / 3.44, 2.83 / 10.84 |
| + hub points (s1-24) | 1 | 0.06 | 0.280 | 0.77 / 4.86, 3.31 / 12.88 |

- **Every main rock death is on arc 54 -> 55 with the corner** (41 of 50, 54 of 61 such starts); the same leg's starts
  without it (48, 57) lost none. From the waypoint the leg never gets the corner; the starts whose bike -> waypoint
  line crosses the barrier (the same arrival: 50 / 45 without a corner, plus 1 with one) lose 4 / 6. Traced (`tr/x2_s1_u6.err`): such a bike still hits
  the rock at 36.9 m/s and lives (18.5 m/s the next tick) - the PS2's rule changes the odds of a hit the port's
  approach still makes; the PS2's bikes did not come within 4.7 m (4). The approach is OURS [H: the hub centres, the
  drive; not settled].
- **The PS2's radius (bit 1)** puts Endor's bikes at the PS2's distance from barriers (10.84 against 10.65 % within
  3 m) and halves their crashes (0.13 a ride against the PS2's 0.28): what else makes the PS2's bikes crash is not
  known (its corner hand-overs kill 9 %, the prototype's 1.6 %) [H].
- **Hub points for drivers (bit 4)** give Endor the PS2's crash rate but a new tat1 spot, (-216, 24): 27 deaths. Not
  for the build without its own trace.
- On the other 12 hover maps (seeds 1-24, bit 2 alone; bit 1's HCR is wrong for BF2 classes there): crash per ride
  within one block's noise of main (bf1_nab1 0.080 -> 0.052, bf1_yav1 0.539 -> 0.554, BF2 maps 0.000-0.014), the
  near-corner driving share 0.0-4.2 % as on main (`ab_bars_maps.txt`).

## 6. For the build (card HCORN part 1 = items 1 and 3; card HCRAD = item 2)

1. **A hover driver's corner walk from the waypoint it reached**: when a leg is planned on arrival at a hub
   (`advance_route`, `Route::from` = the waypoint reached), search its corners from that point, not from the unit
   (B1 `FUN_002509b0`, B2 `FUN_002c0220`). Scope: hover drivers (the soldier's twin, the same functions, is not
   measured here: `RE_LOG` 2026-10-09 already lists it as a port difference).
2. **At the hover's own radius** (split off to card HCRAD): corners at r + 0.5 from both edges, the leg tested at r; r = half the X/Z diagonal
   of the class model's own box - BF1: the `dummyroot` node's INFO min / max (1; bake it: bike 2.439, K 0x15806f0
   4.920); BF2: that box if it reproduces the four live classes, else the collision model's (1.950 / 3.889 / 4.328 /
   6.393, which do). Measured with it: bf1_end1 0.13 crashes a ride against the PS2's 0.28 (5). If the gates refuse 2
   on that, land 1 alone and keep the soldier's radius as a named PROVISIONAL deviation with these numbers.
3. **The test HJAM lacked**: `hover_drive::drive` past a corner inside the node radius; it must fail with the radius
   handed to `advance_route` zeroed or halved (AUDIT HJAM's M5 / M6).

## 7. Checks run, the verifiers, what stayed [H]

- `re_check.py fn` on every cited line (`lead_tmp/hcorn/checks.txt`); `absent` for the two leg requests' `jal` words
  [receipt:b6b9f8c0]; the constants read in the disassembly (`dis_b1_235ad0.txt`, `dis_b2_2a76b8.txt`); the twin
  opened with its callers.
- **Workflow verifier 1** (`hcorn-re-verify`, 11 agents, claims and citations only, `verify/claims.json`): 21 claims -
  19 SUPPORTED, 2 PARTLY (a count, 130 at 2.94 but 132 in 2.93-2.94; the port citation, 604 is `aim_last_leg` and the
  leg's own call is 986: both fixed above); the 6 absence / twin claims each to a second, separate reader: 6 / 6
  SUPPORTED, none overturned. Folded in: the re-roll fallback, BF2's link-flag branch, the BF2 nodes are the
  follower's segment point, `FUN_00234c00` ends at B1:149284. **Workflow verifier 2** (`hcorn-re-verify-2`, 3 agents,
  the claims added after the first: the RAM bounds, the BF2 boxes, the prototype and contact numbers, one absence):
  7 claims - 5 SUPPORTED, 1 PARTLY (51 crossing starts on the from-the-waypoint build, one of them still with a corner:
  fixed), and the absence ("the bounds are not on the disc") NOT SUPPORTED by both readers - the search had used
  rounded print values; the exact words are in the model INFO chunks (1, re-run by `tools/bbox_disc.py`).
- **Stayed [H]**: which box BF2 reads (model INFO or collision); BF2's link-flag path; the third BF1 tat1 class
  (K 0x17ca210, 6.08) in no save state; the probe at the rock; why the port's bikes arrive at hub 56 heading into the rock (hub centres or the drive) and why
  the PS2's bikes crash twice as often with the same barrier distances; the soldier's corner walk start.
