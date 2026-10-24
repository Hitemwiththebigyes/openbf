---
id: STLG
title: The stall at a leg's mouth as a standing number: card MOUTH's detector over the sweep's 4 Hz samples (the row needs the leg's waypoint), the rate per map in scripts/bot_quality.py and gates.toml [fidelity] (direction down), and the maintainer's Kamino scene prints it beside the reversals - so no later card brings the walkway miss back
milestone: M3
value: 4
phase: build
status: blocked
deps: MOUTH
files: scripts/bot_quality.py, engine/crates/game/examples/round_to_outcome.rs, engine/crates/game/src/unit_probe.rs
declares:
needs: environment — restore MOUTH detector/raw evidence, scene script and baked maps/missions for unchanged-stdout, instrument and mutation checks
effort:
branch:
resume: 2026-10-24 cloud preflight: no implementation; reports/20261024_0844_card_STLG.md and lead_tmp/stlg_20261024_0844/prerequisites.json. Probe already has from/next/wp/walk; BOT_SAMPLES omits them. Restore pstall.py and same-seed captures plus scene_seeds.sh/bakes; then isolated build, 32-map stdout cmp, 8-seed Kamino agreement within 15 percent and rule-1 mutation at least 1.5x; planner owns metric baseline wiring.
created: 2026-10-12
alpha: yes
---
## Why
Card MOUTH (landed 2026-10-12, main 273a441) took BF2 Kamino's four walkway legs from 32 stalls in 8 x 3 min to 0 and
BF1 Bespin's deaths under the floor from 46 to 7 - and no gate saw the defect before or would see it come back: the
scene counts trails over 6 reversals (a stall is one or two), the sweep's noprog wants 3 s (these are 1.5 - 3 s).
This is acceptance 5 of the original MOUTH card, split off because a card that asks for a new gate lands the
instrument and files the wiring (RULES).

## Evidence
- The detector: `engine/artifacts/lead_tmp/mouth/tools/pstall.py` (its header has the rule; it reads black-box
  snapshots and PS2 captures). On the port it finds 93 % of the card's own stalls (the steering's `stop` within 1.5 m).
- The PS2's rate by the same rule: BF2 Kamino 8.7, BF1 Bespin Platforms 3.6 per 20 route bot-minutes
  (`lead_tmp/mouth/out/kam1_ps2_P.txt`, `bes1_ps2_P.txt`). Port after MOUTH, 8 seeds x 3 min: Kamino 20.2 (616 in 609
  leg bot-minutes; main before 23.5), BF1 Kamino 7.7.
- The 4 Hz sample row (`BOT_SAMPLES`, `game/src/unit_probe.rs`) has no leg waypoint and no from / next hub: the
  detector needs W (the waypoint), the leg (from, next) and `walk` == 5.

## Acceptance
1. `BOT_SAMPLES` rows carry the leg (from hub, next hub), the waypoint XZ and the walk kind; stdout of
   `round_to_outcome 60000 42` stays byte-identical with samples on (cmp, 32 maps).
2. `scripts/bot_quality.py` prints P-stalls per 20 route bot-minutes per map and over all maps, with the share the
   three worst unit-lives hold; on main's 8-seed samples its Kamino value is within 15 % of pstall.py's on snapshots of
   the same seeds (the instrument check), and a build with MOUTH's rule 1 switched back reads at least 1.5 x main's on
   Kamino's four walkway legs (the mutation check).
3. gates.toml [fidelity] gets the metric (direction down) through the planner's baseline re-take - the builder does
   not write gates.toml; `scene_seeds.sh`'s line prints the stall count beside the reversals.