---
id: GSBL
title: Blended draws blend in LINEAR space in the port's sRGB target; the GS blends in display (gamma) units: an ADDITIVE spark (1, 0.5, 0) over NAB1 grass reads (255,146,40) linear vs (255,201,42) in GS units — the port's ord_explosion sparks are orange where the PS2's are yellow
milestone: M1
value: 3
phase: measure
status: blocked
deps:
files:
declares:
needs: Restore NAB1 effects/map, PS2 spark crop, port control and approved rendering support; measure G/R and 32-map parity before build or close.
branch:
resume: 2026-10-24 13:06 UTC: planner restoration hold, 6 exact named inputs absent. Restore NAB1 effects/map, PS2 spark crop, port control and approved rendering support; measure G/R and 32-map parity before build or close. Original measurable acceptance unchanged. reports/20261024_1306_planner.md; lead_tmp/planner_20261024_1303/preflight.json.
created: 2026-09-30
---
## Why
FXP2 (2026-09-30): the port's ord_explosion on NAB1 grass reads orange, the PS2's yellow. The GS computes (Cs−Cd)·As+Cd / Cs·As+Cd on the frame buffer's own 8-bit display values (re-fx §6.7, re-bolt §3.3); the port's target is Rgba8UnormSrgb, so wgpu blends linearised values. Every NORMAL/ADDITIVE draw is affected (particles, bolt glows, the blend pass).

## Evidence
Top-40-pixel means of the impact crops: PS2 `lead_tmp/fxp/ps2cap/crop_nab1_impact.png` sparks ~(184,173,93) / (194,182,100) on grass (62,73,42); port `lead_tmp/fxp2/cap/sheet_port_nab1_impact.png` mid-life (246,168,81) / (255,207,123). Arithmetic for one additive texel: linear (1,0.216,0)+(0.048,0.067,0.023) → sRGB (255,146,40); GS units → (255,201,42).

## Acceptance
Measure first: render the fx_capture NAB1 pair (`game/examples/fx_capture.rs`, pose in FXP2's report) with the particle/bolt pass blending in display units (a non-sRGB view of the target, or blending done in the shader) and report the spark G/R ratio against the PS2 crop's; then the parity score (`scripts/parity.py`) for all 32 maps with the blend pass switched too. Build only if the pair's G/R moves toward the PS2's by ≥ 50 % and parity does not rise on any map; else close with the numbers.
