---
id: OVHT
title: The weapon heat gauge on the HUD: a weapon that heats (the pistol, the seat guns) shows heating and cooling on the centre icon so the player sees the overheat coming - the PS2's element, colours and thresholds, BF1 and BF2
milestone: M3
value: 4
phase: re
status: todo
deps:
files: engine/crates/ui/src/hud.rs, engine/crates/ui/src/hud_tree.rs, engine/crates/ui/src/hud_draw.rs, engine/crates/game/src/hud_feed.rs
declares:
needs:
effort:
branch:
resume:
created: 2026-10-21
alpha: yes
---
## Why
The maintainer (INBOX 2026-10-21 18:40 item 3): on the PS2 a weapon with a cooldown shows its heat on the centre HUD icon.
`analysis/re-hud.md` already has BF2's heat ring (30 x `white` segments over -150..150 deg, rgb (239,255,41), beside the
clip ring); BHUD3 listed it among many - this card takes it out of BHUD3 so it does not wait on the rest.

## Do
1. RE: BF1's element (re-hud*.md, the HUD tree / code), and when each game shows it (heat > 0 only? the overheat line?).
2. Build for every heat weapon the sim models (soldier weapons and seat guns; BRSTU's heat model), the overheat state
   as on the PS2. WHUM owns the overheat sound.

## Acceptance
- A picture strip per game: cold, half hot, overheated, cooling - with the sim's heat value beside each frame; the
  PS2's frame beside it (PCSX2, the same weapon).
- A ui test: the ring's lit segments against heat (0, 0.5, 1.0) and the overheat flag; mutation-checked.
- Golden: the HUD golden it moves, declared.
