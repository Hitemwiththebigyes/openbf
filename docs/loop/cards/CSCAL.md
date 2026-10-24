---
id: CSCAL
title: Class counts scale with the team size (the maintainer's decision 10-21 18:10): a match with more units a side than the mission's default fields more of the special classes - each AddUnitClass min and max scaled by (team size / the mission's default), never below the PS2's own numbers, so the default match is the PS2's exactly
milestone: M3
value: 4
phase: build
status: blocked
deps:
files:
declares:
needs: environment — Restore generated mission RONs and class bake to census all authored team-size/min/max values.
effort:
branch:
resume: 2026-10-24 10:33 UTC: planner prerequisite hold; no new implementation/acceptance. Census current setup/caps first, choose and report rounding/curve under the maintainer authorization, then preserve exact default counts/hash, double-size curve and unchanged heroes acceptance. Exact missing paths: lead_tmp/planner_20261024_1026/preflight.json. Restore or regenerate verified equivalent inputs before clearing hold. reports/20261024_1026_planner.md
created: 2026-10-21
---
## Why
The maintainer (INBOX 2026-10-21 18:10): "playing with more units per team would also mean more special units too" (droideka,
jet trooper, dark trooper, wookiee, officers). For UCNT, DEKF and the instant-action unit-count override.

## Do
One function in core owns it (covered by the sim hash); the class select's "Unit Count" and the "Too many units
already!" test read the scaled cap. The card decides and tells the maintainer (STATUS): the rounding, linear or gentler (e.g. a
square root so 500 a side does not field 40 droidekas), whether heroes scale (default: they do not).

## Acceptance
- Tests: the mission's default team size gives the PS2's min / max byte for byte on every baked mission (a census);
  double the size gives the chosen curve's numbers; heroes unchanged. Sim hash unchanged at default counts.
