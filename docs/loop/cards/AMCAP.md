---
id: AMCAP
title: Use BF1's ammo reserve allowance consistently on every map
milestone: M2
value: 4
phase: build
status: done
deps:
files: engine/crates/core/src/sim/pickups.rs,engine/crates/core/src/sim/pickups/tests.rs,engine/artifacts/loop/FEEL_NOTES.md
declares: sim; sweep; arena
needs:
effort:
branch: main
resume: Existing PR3 rule recorded as a named merged-game deviation; independent empty, partial and full magazine allowance checks cover primary and held weapons.
created: 2026-10-26
alpha: yes
domain: weapons
---

## Rule and evidence

Record the rule already implemented with public pull request #3; this is not a
second implementation of its ammunition feature. See `analysis/re-ammo-reload.md`,
BF1 `FUN_002ab4d8` and BF2 `FUN_003038c8`.

BF1 counts any nonempty loaded magazine against the total magazine allowance;
BF2 subtracts it only while full. Use BF1's rule on every map, avoiding the extra
reserve magazine obtained by firing once before replenishing. An empty weapon
can still replenish its entire issue. See the AMCAP entry in FEEL_NOTES.

## Acceptance

- With five 55-round magazines, empty, one-round, 54-round and full loaded
  magazines allow respectively 275, 220, 220 and 220 reserve rounds. Exercise
  both primary and held weapons through `give_ammo`; further replenishment
  must do nothing.
- Retain empty-weapon refill/reload coverage for one, five and six magazines.
- Changing the predicate to BF2's full-magazine rule must fail the independent
  partial-magazine assertion. Applicable integration CI and gameplay checks
  belong to the complete PR3 integration, not this arithmetic test alone.
