---
id: HOLOF
title: The command post hologram's own flare (seen looking back down the beam: alpha A g^8 x the class's flare factor, size growing toward the top's radius) and the icon sheet's scan-line UV animation; the port draws neither
milestone: M1
value: 2
phase: build
status: todo
deps: LFLR
files: engine/crates/game/src/holo.rs
declares:
needs:
effort:
branch:
resume:
created: 2026-10-22
---
## Why
Left from CPHOL and LFLR. analysis/re-point-lights.md 15.6 (B2 FUN_00469628 529168-529188): for g < 0 the beam's +0x124 alpha = A g^8 x volume +0x194, radius +0x120 = mean start + g^8 (mean end - mean start)/2 - a flare seen only looking back along the beam (from above a post, looking down). Card LFLR built the same rule for EntityLight cones (game/src/entity_glow.rs flare(), render::Flare); the hologram's +0x194 value (which class key, HoloFlareIntensity?) is not read. 15.7: the icon sheet com_icon_controlzone is white with horizontal scan lines in its alpha (163-255); whether its UVs scroll is not read.

## Evidence
re-point-lights 15.6, 15.7, 16. The reusable code: entity_glow::flare (the g^8 rule) and render::Flare.

## Acceptance
- A unit test: a post seen from above (g < 0) draws one Flare with alpha A g^8 x the factor read from the class; from the side none (mutation-checked).
- The factor's key named from FUN_002172a8 / the holo class setter with re_check fn lines, or [H] with a RAM read of +0x194 on one BF2 save state.
- The scan-line animation: read the icon material's flags (MTRL) and the draw; build it or record that the PS2 does not scroll it.
- Goldens move only where a post is seen from above (likely none).
