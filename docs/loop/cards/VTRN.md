---
id: VTRN
title: Vehicles turn too slowly with the controller (the maintainer 10-21): the PS2's turn rate per vehicle class at full stick (deg / s, its response curve and dead zone) against the port's on the same stick input, hover / speeder / walker / flyer, both games - fix where the port is slower
milestone: M2
value: 4
phase: measure
status: todo
deps: PAD
files:
declares:
needs:
effort:
branch:
resume:
created: 2026-10-21
alpha: yes
---
## Why
The maintainer (INBOX 2026-10-21 17:50 item 5): vehicles turn more slowly than in the original games. Check that the stick is not
read through the soldier's look curve or a dead zone twice, and that the vehicle's own turn law (HVRE, VWLK, the air
integrator) is fed the full stick.

## Do
1. The PS2's numbers: a scripted full-stick turn for N s per class (PAD), degrees from RAM, plus half stick for the curve.
2. The port's: the same input through the real input path (controller -> the vehicle), headless.
3. Fix where the port differs, per class.

## Acceptance
- A table per class: PS2 deg / s at full and half stick, the port's before and after; every class within 5 % of the PS2
  or a named reason. A test per vehicle family: full stick for 1 s turns the PS2's degrees.
