---
id: BUGX
title: Bug checks, each a test whether or not it reproduces: (a) bots shoot corpses or heal / resupply dead bodies; (c) the hit-direction chevrons carry into the next life; (d) no respawn after a death in water or after Suicide
milestone: M3
value: 4
phase: measure
status: todo
deps:
files:
declares:
needs:
effort:
branch:
resume:
created: 2026-10-21
---
## Why
INBOX 2026-10-21 20:00 item 5 (an outside review's list; the PS2 corpus decides each). Item (b), a weapon that does no
damage to the player, is folded into NODMG.

## Do
For each: a headless scene that tries to make it happen and asserts it does not (a bot with a corpse in its sight, a
medic / ammo class beside a dead team-mate; a hit just before a death and the HUD's chevron timers at the respawn; a
death in water and the Suicide menu action, then the spawn screen and a respawn). A case that reproduces gets its fix
here if small, else its own card.

## Acceptance
- Four tests (a, c, d-water, d-suicide), each mutation-checked; the report says which reproduced on main and what fixed it.
