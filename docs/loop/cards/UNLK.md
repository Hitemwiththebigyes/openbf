---
id: UNLK
title: BF2 class unlocks: each side's officer (PointsToUnlock 8) and special class (12: jet trooper, droideka, Wookiee, dark trooper) are locked on the player's class select until the player has scored that many points - the PS2 shows 'Get N more points to unlock!' and a black silhouette; the port reads nothing
milestone: M3
value: 3
phase: re
status: todo
deps:
files:
declares:
needs:
branch:
resume:
created: 2026-09-29
defer: post-alpha
---
## Why
UAUD2 (analysis/unit-audit-bf2.md §3.8). A BF2 rule the player sees at the start of every battle: the officer and special
classes cannot be picked until the player earns points. The port offers every class at once (and today spawns one class per
faction, SPSEL).

## Evidence
- Setter: B2 FUN_0026a340 stores PointsToUnlock 0x714b2463 with '%d' (ELF 0x61fc20) at class+0x374 (186907-186909).
- Disc (census2.py): 8 on every officer (clone commander, MagnaGuard, Bothan spy x3, Imperial officer), 12 on the jet troopers,
  droideka, Wookiees, dark trooper; no other fielded class. RAM agrees (verifier V11).
- PCSX2: lead_tmp/unit-audit/pcsx2_bf2/{rep,cis,end,hot,cor1g}_sheet.png - the class select at battle start shows "Get 8 /
  12 more points to unlock!" on exactly those classes and draws them as black silhouettes; AI soldiers of those classes are
  already fielded ("Unit Count: 3 Max: 4").

## Acceptance
- RE ([O]): the reader of class+0x374 - which score it compares (the player's points this battle? kills? CP captures?),
  whether a class re-locks (on death, on a new battle), whether bots are ever gated, and BF1's absence of the rule.
- Then a build card (after SPSEL gives the spawn screen a class choice): the gate, the silhouette, the text.

## Notes
- Created by UAUD2. The player's score model itself may be missing in the port; the RE should say what feeds it.

## Planner note (2026-09-30, M-ALPHA)
`defer: post-alpha` — not needed for the alpha's six showcase maps or its checklist. Value 3 (kept).

## Planner note
Planner 2026-10-21 20:50: the maintainer (INBOX 17:50 item 4) wants BF1 unlocks and a career unlock too: cards UNLK1 (BF1 tiers), UNLK2 (career counter), UNLK3 (stages + the class-select line) follow this one.
