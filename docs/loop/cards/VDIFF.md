---
id: VDIFF
title: What differs between the PS2, Xbox and PC versions of both games: class / weapon ODF values, mission values, collision, terrain and placed objects, animation clips, and what each version has alone - a table per game in analysis/version-diff.md, counts first, the 10 largest differences each
milestone: M0
value: 4
phase: measure
status: blocked
deps:
files:
declares:
needs:
effort:
branch:
resume: Restore named raw inputs in lead_tmp/vdiff/preflight.json and identify owned Xbox/PC BF1 retail sources; rerun preflight.py, then implement all six per-map/side comparisons and top-ten rankings. No parity measurement or acceptance earned; see reports/20261024_1130_card_VDIFF.md.
created: 2026-10-21
alpha: no
---
## Why
INBOX 2026-10-21 19:30 (the maintainer asked to know the differences). The maintainer's rule: one game under the hood, whatever discs the
player owns - another version may change only the look. Before any non-PS2 base work we need to know whether the
sim-relevant data differs at all (PLAN already notes BF2 Hoth: PC 32 a side against the PS2's 19).

## Do
Per map and side, PS2 vs Xbox vs PC (where owned): (a) every class / weapon ODF property; (b) mission script values (unit
counts, tickets, class mix, posts, timers); (c) collision (bodies, shapes, triangles, bounds per object); (d) terrain
heights and placed objects; (e) animation clips (names, lengths, root motion); (f) what one version has alone (Xbox `_h`
maps and `_bump` textures, PC extras). Tools: `lead_tmp/xbtex/tools/` (xiso.py, cmp.py) for the Xbox side; the port's
own readers for PS2 / PC.

## Acceptance
- `analysis/version-diff.md` with a counts table per game and the 10 largest differences each, every number from a
  script kept in `lead_tmp/vdiff/`.
- Then the planner puts the maintainer's question in STATUS: if the sim data differs, an Xbox-only or PC-only player cannot get
  the PS2's game from their own discs - the port would have to carry the PS2's values (a legal question, the maintainer's call).
  Default until he answers: the PS2 discs stay required; other versions add looks only.
