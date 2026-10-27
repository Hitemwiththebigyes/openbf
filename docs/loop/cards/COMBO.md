---
id: COMBO
title: Saber combo data: does the PS2 BF2 disc carry the side comb chunk; a bake reader (combos, states, keywords, energy, input lock, deflect), the hero sabre clip banks, and a pin on unknown keywords
milestone: M4
value: 4
phase: re
status: todo
deps:
files: engine/crates/bake/src/anim_sets/bf2.rs
declares:
needs:
effort:
branch:
resume:
created: 2026-10-27
---
## Why
The maintainer 2026-10-27 (INBOX 2026-10-27). Saber heroes need the combo data; today `bake/src/anim_sets/bf2.rs:27` leaves
combo rows unresolved and no `comb` reader exists (`analysis/a2-bake-formats.md:295` only lists the chunk, in the shared
compiled-config format). The combo language (states, transitions, conditions, EnergyCost, InputLock, DeflectAngle) is
described in the official mod-tools docs, `community_mods/gametoast_lua/doc/StarWarsBattlefrontII/documentation/combo.md`, and the hero side in `BF2 Jedi Creation.md` beside it.
Every value comes from the PS2 disc.

## Acceptance
- RE first: does the PS2 BF2 disc's side data carry `comb`? Name the files and counts; if absent, say what the PS2 uses instead.
- A bake reader for `comb`: combos, states, transitions, keywords, condition groups, EnergyCost / EnergyRestoreRate,
  InputLock, DeflectAngle, damage windows, written to the baked RON per hero.
- The hero clip banks resolve: `<AnimationName>_sabre_<clip>_full` beside `human_sabre_*`, every combo clip found.
- A bake test that fails on any unknown keyword, mutation-checked; sim hash unchanged (bake only).
