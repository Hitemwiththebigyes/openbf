---
id: UNLK3
title: Unlock stages from career kills (OURS): 0 kills = the full points (8 / 12), 25 kills = half (4 / 6), 100 kills = unlocked for good - one table in profile; the class select shows the progress ('14 / 25 kills with the Jet Trooper')
milestone: M3
value: 3
phase: build
status: todo
deps: UNLK1,UNLK2
files: engine/crates/game/src/profile.rs
declares:
needs:
effort:
branch:
resume:
created: 2026-10-21
---
## Why
The maintainer (INBOX 2026-10-21 17:50 item 4b). The kill numbers and whether officers count as special are asked in STATUS
(default: 25 / 100, officers count with their 8-point tier).

## Acceptance
- Tests at 0, 24, 25, 99 and 100 kills for an 8- and a 12-point class; a picture of the class select line.
