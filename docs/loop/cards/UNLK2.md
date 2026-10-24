---
id: UNLK2
title: The career counter (OURS): the profile counts the player's career kills per lockable class (kills made as that class in a real match, conquest and instant action, bots or not) and saves them with the profile
milestone: M3
value: 3
phase: build
status: blocked
deps:
files: engine/crates/game/src/profile.rs
declares:
needs: Restore class RONs, baked maps/missions and full-CI font/graphics prerequisites listed in lead_tmp/unlk2_cloud/preflight.json
effort:
branch:
resume: 2026-10-24 prerequisite checkpoint; no implementation. Read lead_tmp/unlk2_cloud/next_prompt.md and source_checkpoint.md; implement shared per-tick class attribution and save/load acceptance after restoring runtime inputs. Original Acceptance retained.
created: 2026-10-21
---
## Why
The maintainer (INBOX 2026-10-21 17:50 item 4b). One number per class, per class not per side.

## Acceptance
- A test: kills as a class in a headless match raise that class's count, and the count survives a profile save / load;
  an old profile without the field loads with zeros.
