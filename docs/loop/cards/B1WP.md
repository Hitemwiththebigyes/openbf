---
id: B1WP
title: BF1 Geonosis / Hoth: a live command walker is a held post (BF1's rule, re-command-walker-post 0.6 [O]) - the port's BF2 all-posts countdown ends all 16 rounds the moment the attacker's ONE fixed post falls (some at 77 s on 197/235 tickets); team 1 wins 16-0 on both
milestone: M3
value: 4
phase: build
status: todo
deps:
files: engine/crates/core/src/cp.rs, engine/crates/core/src/sim/round.rs
declares:
needs: maintainer — PLAN locks 'Conquest: BF2 rules'; on BF1 Geonosis/Hoth that rule ends every round when the attacker's one fixed post falls (walkers ignored). May a live command walker count as a held post on BF1 maps (BF1's own rule; default proposed)?
effort:
branch:
resume:
created: 2026-10-22
alpha: yes
---
## Why
Card WINS (2026-10-22): BF1 Geonosis and BF1 Hoth go to team 1 in 16 of 16 seeds, and EVERY one of WINS's 16 re-run rounds
ends by the port's 20 s all-posts countdown (`cp::tick_domination`), some at 77-200 s with both sides on 150-235 tickets
(`lead_tmp/wins/run1/bq/bf1_{geo1,hot1}_s*.ev.csv`, the `cp` rows: every post team 1's, 20 s, then the end). The attacker
(BF1 GEO1 Republic, BF1 HOT1 Empire) starts with ONE fixed post (GEO1 `1110`, HOT1 `11011`) and its AT-TEs / AT-ATs.
On the PS2 BF1 a live command walker's post counts: `analysis/re-command-walker-post.md` section 0.6 / 7 [O] - the BF1 team
update `FUN_002ad830` sums ValueBleed over every scheduled post the team owns (the walker's 10) and the 20 s "no posts"
countdown runs only at value 0, so a live AT-AT holds it off. The port plays BF2's rule on every map (PLAN, Modes & rules,
"Conquest: BF2 rules"), and BF2's own rule leaves the walker out - which is fine on BF2's maps (they register more fixed
posts) and ends every BF1 walker battle the moment the one fixed post falls.

## Question for the maintainer (needs:)
PLAN locks "Conquest: BF2 rules". Default proposed: on a BF1 map, a live command walker's post counts as a held post for
the all-posts countdown (and its ValueBleed 10 in the bleed sum) - BF1's own rule, needed only where the mission has
command walkers (BF1 GEO1, HOT1). Reversal: a flag on the mission.

## Acceptance
- A sim test: a team owning no fixed post but a live command walker is not defeated after 20 s (BF1 mission); it is after
  the walker dies (mutation-checked).
- 16 seeds: no BF1 GEO1 / HOT1 round ends by domination while the loser has a live walker; report the win share (main
  0-16 both).