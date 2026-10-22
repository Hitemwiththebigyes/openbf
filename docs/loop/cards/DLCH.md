---
id: DLCH
title: The Xbox DLC heroes (Asajj Ventress, Kit Fisto) from a source the player owns: the modded PS2 disc's addon dir (PS2 format, the port's own readers) or the PC-format repack - local use only, never in git, no melee combo
milestone: M5
value: 3
phase: re
status: todo
deps: HERO
files:
declares:
needs:
effort:
branch:
resume:
created: 2026-10-21
defer: post-alpha
---
## Why
INBOX 2026-10-21 19:30. `SWBF2_mod.iso` holds them in PS2 format under `DATA/_LVL_PS2/addon/223/SIDE/dlc.lvl`;
`iso/ADDON_XBOX/DLC_XBOX/data/_LVL_PC/SIDE/dlc.lvl` holds a PC-format copy. Both are third-party repacks: local use only.
The maintainer: no melee combo needed. The DLC maps are BF1 maps the port already has. Waits on heroes existing (HERO).

## Acceptance
- Both heroes listed by the class reader from the PS2-format addon (class names, weapons, health), then playable on one
  map with HERO's rules; a test that the port runs unchanged when the addon is absent.
