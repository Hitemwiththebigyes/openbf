---
id: GTHRW
title: The grenade throw is shown: the PS2's throw clip per class and game, played on the soldier in first and third person when he throws (the throw itself exists, card GREN)
milestone: M1
value: 4
phase: re
status: todo
deps:
files: engine/crates/game/src/animator.rs
declares:
needs:
effort:
branch:
resume:
created: 2026-10-21
alpha: yes
---
## Why
The maintainer (INBOX 2026-10-21 17:50 item 1): the grenade throw animation is not shown. ANIMX lists "the thrown / tumble /
death picks" as unread. The upper-body layer exists (ANIML).

## Acceptance
- RE: the clip the PS2 picks for a throw per weapon / class (both games), the layer and the time the grenade leaves the
  hand; then built: a picture strip per game of a throw (third person, and first person), the PS2's beside it; a test
  that a throw sets the throw clip on the upper layer for its duration.
