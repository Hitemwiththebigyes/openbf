---
id: MUSW
title: BF2 MUS1's control-room windows are shut in the port: at cp6 (Lava Bridge Control) the PS2 looks through two windows onto the lava river and the stirrer arms, the port draws a solid wall panel there (mus1_prop_window x2, 12.5 m from the post) — find why (an animated shutter the PS2 opens, or a segment the port draws opaque) and fix
milestone: M1
value: 3
phase: re
status: blocked
deps:
files:
declares:
needs: Restore matching primary PS2 corpora/ELFs, MUS1 source/bake and original cp5_cp6 b000 state/camera/images plus capture tools; remeasure after the existing door/bind work.
effort:
branch:
resume: 20261025_0420 UTC RE input hold, 0/29 minimum paths present. Recheck current-main window7/window8 against the original state; distinguish closed still-render pose from battle door state before any alpha/segment edit. reports/20261025_0420_card_MUSW.md; no unchanged-input retry.
created: 2026-09-29
defer: post-alpha
alpha: 
tries: 1
---
## Why
PXWB (2026-09-29): `engine/artifacts/pcsx2/ref/bf2_mus1/walk/pairs/cp5_cp6__sheet.png` b000 (crop
`lead_tmp/pxwb/frames/mus1_cp5_b000_window.png`): the PS2's two windows show the glowing lava and a stirrer arm —
(60,230,230,330) PS2 (72,53,30) vs port (22,21,19); (390,230,560,330) (45,32,24) vs (22,21,19). The port draws a dark
panelled wall there.

## Evidence
- The manifest has `mus1_prop_window` window7 (12.4 m at yaw 343 from cp6) and window8 (12.6 m at yaw 17) exactly at the
  two openings, and `mus1_prop_spatula` stir4 (22.5 m, yaw 0) behind them.
- The baked `mus1_prop_window.glb`: 3 primitives, all material `mus1_bldg_control04`, OPAQUE, flags 0x10201,
  light prelit_add.
- [H] (a) the prop is a shutter with an animation (the PS2 draws it open / at another frame), or (b) a segment the port
  draws opaque that the PS2 skips or blends (ELFR's segment rule on BF2), or (c) the three segments are LODs/states.

## Acceptance
- The cause named from the model's segments / anim and the ELF's draw; the port's frame at this pose shows the lava
  through both windows like the PS2's; the other `mus1_prop_window` instances checked at their posts.

## Planner note (2026-09-30, M-ALPHA)
`defer: post-alpha` — not needed for the alpha's six showcase maps or its checklist. Value 3 (kept).
