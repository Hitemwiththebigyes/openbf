---
id: VPOSE
title: A vehicle's own posed nodes are drawn: its TURRET sections' TurretNodeName / AimerNodeName / barrel nodes by the MountedTurret law building turrets already use (card VTURD), and a hover's FinAnimation nine-pose (speeder bike vanes, BARC, fighter tanks, skiff) from the drive's smoothed throttle and turn - the port draws every hover rigid, so a fighter tank's turret never turns with its gunner's aim and a bike's vanes never move
milestone: M2
value: 4
phase: re
status: todo
deps:
files: engine/crates/bake/src/vehicle.rs,engine/crates/bake/src/turret_rig.rs,engine/crates/game/src/viewer/vehicles.rs
declares: rebake=side
needs:
effort:
branch:
resume:
created: 2026-10-15
alpha: yes
---
## Why
Found by card VRIDE's RE (analysis/re-vehicle-riders.md 5, 2.2): a vehicle's TURRET section is the same MountedTurret as a building turret - its tick writes the TurretNodeName / AimerNodeName / barrel matrices into the VEHICLE's pose table (re-turret-draw.md 2) - and a hover's drive also poses the hover's own model by its class's FinAnimation clip (frame a = y + 4 toward b = y + 1 | 7 by c = |p|, the smoothed throttle p and turn y, B1 FUN_001ec768 B1:99627-99635, B2 FUN_0021d8b8 B2:134724-134737). The port draws every hover rigid in its bind pose (game/src/viewer/vehicles.rs): on BF1 NAB1 / TAT1 / YAV2 and BF2 MYG1 / UTA1 a fighter tank fires while its turret looks forward; card VRIDE's gunner hangs from that turret node (re-vehicle-riders 1: hp_active under turret_y) and stays at its rest until this lands.

## Evidence
- re-vehicle-riders.md 5 [V]: FinAnimation (0xef563a98) per class (lead_tmp/vride/fincensus.py: both bikes imp_speederbike_9pose, the fighter tanks rep_fightertank_9pose, BF1 skiff tat_skiff_9pose, BF2 BARC barcspeeder_9pose, the snowspeeder); the live rows of the drive's numbers lead_tmp/vride/ps2/b1bike_rows.json, b2bike_rows.json.
- re-turret-draw.md 2 [O]: the node law; RAM: the BF1 Naboo / BF2 Mygeeto fighter-tank gunner turned with the turret's yaw (re-vehicle-riders 3).
- Not read: whether every vehicle model splits into rigid parts per node (card VTURD's softcensus.py ran on armed buildings only; per-bone rigid draws failed for the tauntaun - RULES dead end).

- Note from VRIDE's build (2026-10-15, re-vehicle-riders 7): the posed WALKER skin is drawn without its clip's
  DummyRoot translation (`viewer::walker_pose`, the bake holds it at rest): BF2's one-man AT-ST `bone_root` sits
  0.07-0.22 m higher than the PS2's RAM (port y 2.915-2.942 against 2.717 / 2.852 in
  `ref/states/bf2_uta1_cp3_con_cp1_postA_spawn.p2s`) and without its sideways sway (+-0.029 against 0.082 / -0.075),
  at the same walking tilt. The rider rides that node, so he is as high as the walker's body is drawn. [H] the bob;
  measure the walker clip's DummyRoot track first (`lead_tmp/vride/clipdump`).

## Acceptance
- RE first (bounded): lead_tmp/vturd/softcensus.py over every vehicle model with a TurretNodeName / AimerNodeName / FinAnimation clip (triangles spanning bones per model); the FinAnimation clips' bones (which nodes move).
- A test: a fighter tank's turret_y at the RAM's yaw puts the gunner's hardpoint at (0.0089, 0.6581, -3.0100) in the tank frame (BF2 MYG1, lead_tmp/vride/ps2ride.py on ref/states/bf2_myg1_cp2_cp1_con_post1_spawn.p2s) within 1 cm; a bike's vane node posed from the PS2 rows' (a, b, c) equals the clip's blended frame; mutation: a rigid tank fails.
- Pictures: a fighter tank turned PS2 beside the port.