---
id: FIDSP
title: Fidelity gate counts RLOOP's standing loops: a new metric loop20 (spells of 20 s+ inside a 2 m box in recover / travel, scripts/loop_spells.py over the botq samples the fidelity run already writes, summed per map, direction down) in gates.py's FID_BOTQ set, with its tolerances set from main's two seed blocks
milestone: M0
value: 4
phase: build
status: blocked
deps:
files: scripts/gates.py,scripts/loop_spells.py
declares: oracle=guards
needs: decision — the stall-off control raises loop20 only +1..+5 against main's six-block spread 8 (loop seconds x2.0..2.9, separated); choose a duration-weighted metric (loop seconds or loop30) or amend Acceptance 2, then write the row
effort:
branch: fidsp
resume: 20261024_2231 planner: historical six-block23/19/17/20/15/14, proposed9/6 and PIN20019/14 belong to bace5903, not current main. Inputs absent here; no row enabled or mutation claimed. Candidate next control: disable stall::send_off floor-walk/recycle responses plus watch stalled/recycle outputs on scratch only, null-prove and measure both blocks before acceptance amendment. See engine/artifacts/loop/reports/20261024_2231_planner.md.
created: 2026-10-15
alpha: yes
---
## Why
RLOOP (landed 70bfa91 + a059815 + 70b9c47) took the standing loops from 117 to 27 spells of 20 s+ (16 seeds x 32 maps), the ones over 30 s 81 -> 2. Nothing in the gates watches that number: a later bot card could bring the two-minute stands back and pass every gate (`stuck_max_med` is a median of a different instrument). RLOOP's report asked the planner to wire `scripts/loop_spells.py` into the fidelity numbers; that is a gates.py change, so it is this card. Bots are the maintainer's top concern (root CLAUDE.md).

## Do
- gates.py: a fidelity metric `loop20` (count of loop spells per map, sum over seeds as `recycled` / `noprog` are, FID_SUM) and optionally `loop30`, computed by importing / running loop_spells.py over the fidelity run's botq dir; `dir = "down"`.
- The tolerance from main's own spread: run main's two seed blocks (1-8, 9-16) and three more 8-seed blocks; per-map tolerance = the spread, agg tolerance from the six block totals. Say in the card what the numbers were; the planner writes the baseline rows (the builder cannot edit gates.toml - give the rows in the report).

## Acceptance
- On main (today's): `gates.py --groups fidelity` lists `loop20` per map and agg, equal to `loop_spells.py` run by hand on the same botq dir (paste both).
- A mutation: RLOOP's `PIN_FAILS` set to 200 on a scratch tree -> the gate reports `loop20@agg` worse (paste the line).
- The six block totals and the tolerance chosen in the report.
## Cloud checkpoint 2026-10-24 (FIDSP build)
Uncommitted source and permanent fixture checks in branch fidsp at
`engine/artifacts/lead_tmp/worktrees/fidsp`; recovery patch/receipts/next prompt in `lead_tmp/fidsp/`.
Python fidelity/gate self-tests, seven boundary tests, hand-CLI fixture parity and sum mutation pass.
CI exit 101: missing common-bake gamefont_large and suitable graphics adapter.
Real current-main parity, PIN_FAILS=200 mutation, six block totals and measured tolerances remain owed;
no baseline rows proposed, no gates.toml changes, no commit/landing. See `reports/20261024_0946_card_FIDSP.md`.

## GitHub checkpoint recovery (2026-10-24)

