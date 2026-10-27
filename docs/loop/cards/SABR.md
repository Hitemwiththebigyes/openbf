---
id: SABR
title: Saber blade, block and throw: the blade from the ODF, block by the combo states, the throw; every number from the PS2
milestone: M4
value: 3
phase: build
status: todo
deps: COMBO,HERO
files: engine/crates/core, engine/crates/render
declares:
needs:
effort:
branch:
resume:
created: 2026-10-27
---
## Why
The maintainer 2026-10-27 (INBOX). After COMBO, part of HERO's build split. The ODF keys and the throw are described in the
official mod-tools docs, `community_mods/gametoast_lua/doc/StarWarsBattlefrontII/documentation/BF2 Jedi Creation.md` (LightSaberLength / Width / Texture, the saber throw); the
block rule in `combo.md` beside it (DeflectAngle, EnergyCost).

## Acceptance
- The saber blade drawn from the ODF (LightSaberLength, LightSaberWidth, LightSaberTexture), on player and AI heroes.
- Block (the combo states' deflect rule and energy drain) and the saber throw; every number from the PS2 disc or a PCSX2
  measurement, none invented.
- Tests for block angle and throw flight/return; HERO's order and the maintainer's hero rules (killable, no time drain) hold.
