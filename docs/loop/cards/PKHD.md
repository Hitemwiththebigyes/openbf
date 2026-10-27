---
id: PKHD
title: loopctl's busy-files test: a parked card with needs: set holds no files - the planner sets such cards blocked by hand today (SNPR, HJAM) so the cards behind their files can run
milestone: M0
value: 3
phase: build
status: done
deps:
files: scripts/loopctl.py
declares:
needs:
effort:
branch:
resume: (private workflow note omitted)
created: 2026-10-16
alpha: yes
---

## Why
Planner 2026-10-16 04:35 (`reports/20261016_0435_planner.md` item 4): a parked card waiting on the maintainer (`needs:` set) still
holds its `files:` in the picker's busy set (`order()` / `eligible`: `status in (in-progress, parked)`), so SNPR held
`ai/behave.rs` and kept every value-5 bot card (TURR, TPOS, TEAM, TCMP, HVYA) out of the picker; HJAM held `route.rs`.
The workaround (set them `blocked`) loses the land-job route when the maintainer answers.

## What to build
In `scripts/loopctl.py`, a parked card with a non-empty `needs:` adds no files to the busy set; when `needs:` is cleared it
holds them again (and the land job takes it). STATUS / orient still list it as waiting on the maintainer.

## Acceptance
- loopctl self-test: two new cases (parked + needs -> a card on its file is eligible; parked, needs cleared -> it is not);
  one mutation (drop the needs test) fails a case. Paste the self-test output.
- A dry run of the picker on today's cards before / after (no change unless a parked card has `needs:`).

## Notes
After it lands the planner may set SNPR / HJAM back to `parked` with `needs:` (their resume lines name the maintainer's questions).

## GitHub checkpoint recovery (2026-10-24)
