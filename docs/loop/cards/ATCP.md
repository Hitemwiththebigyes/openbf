---
id: ATCP
title: BF2 Hoth's AT-AT as a mobile command post: spawn on it, its capture / loss rules and the HUD post icon moving with it, per the PS2
milestone: M2
value: 3
phase: re
status: blocked
deps:
files:
declares:
needs: Restore BF2 Hoth mission/bake and primary corpus/ELF/state toolkit; derive the BF2 mobile-post rule before code or picture acceptance.
effort:
branch:
resume: 2026-10-24 14:22 UTC: planner restoration hold; Restore BF2 Hoth mission/bake and primary corpus/ELF/state toolkit; derive the BF2 mobile-post rule before code or picture acceptance. Original measurable acceptance retained. reports/20261024_1422_planner.md; lead_tmp/planner_20261024_1417/preflight.json.
created: 2026-10-21
---
## Why
INBOX 2026-10-21 20:00 item 4. HOT1 is a showcase map.

## Acceptance
- RE of the PS2's rule (the class keys, the mission script calls, what happens when it dies); then a build with a headless
  test: a unit spawns at the walker's post, the post's place follows the walker, the post is lost when it dies, and the
  map / HUD icon moves (a picture pair against the PS2).
