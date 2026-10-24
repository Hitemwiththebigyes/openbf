---
id: WPAT
title: Multi-bolt shot patterns and charge weapons: the bowcaster's fan and its charged bolt, the arc caster, any award weapon that charges - shot count, spread pattern, MaxPressedTime and charge damage from the PS2's weapon and ordnance classes, both games; bots fire them the same way
milestone: M3
value: 4
phase: re
status: blocked
deps:
files:
declares:
needs: environment — Restore both primary corpora/extracted weapon classes and PS2 toolkit or verified raw charge/pattern captures.
effort:
branch:
resume: 2026-10-24 10:33 UTC: planner prerequisite hold; no new implementation/acceptance. Read RKSLA/UORD/VTAT first, then both-game weapon-pattern census with full re_check/verifier; write measured build acceptance before rephasing. No constants inferred from historical prose. Exact missing paths: lead_tmp/planner_20261024_1026/preflight.json. Restore or regenerate verified equivalent inputs before clearing hold. reports/20261024_1026_planner.md
created: 2026-10-21
---
## Why
INBOX 2026-10-21 20:00 item 1 (an idea from an outside review; every rule and number comes from openbf's own PS2
corpus). RKSLA, UORD and VTAT touch the charge cycle: read their notes first so this card does not redo them.

## Do
RE both games: the weapon / ordnance class keys for a multi-shot fire (count, pattern, spread) and for a charge (the
press time, the charge's damage and look), which classes use them (a census over every weapon class on both discs), and
how a bot triggers a charge weapon. Then a build spec on this card (`phase=build`).

## Acceptance
- `analysis/re-weapon-patterns.md` with the checks (re_check, the verifier) and the census table.
- Build acceptance to write into the card: a scene test per pattern (bolts per shot, their spread against the class's
  numbers) and per charge weapon (damage at 0, half and full press), mutation-checked.
