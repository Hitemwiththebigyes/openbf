---
id: PAD
title: Scripted pad input on the PS2: drive PCSX2 with a pad script and screenshots through its PINE socket on top of engine/artifacts/pcsx2/, so a measurement can be a scripted action (a full-stick vehicle turn read from RAM is the first user) - headless, isolated, under the GPU lock
milestone: M0
value: 5
phase: build
status: todo
deps:
files: engine/artifacts/pcsx2/README.md
declares:
needs:
effort:
branch:
resume:
created: 2026-10-21
alpha: yes
---
## Why
INBOX 2026-10-21 20:00 item 2. The maintainer's vehicle turn rate (VTRN) needs a full-stick turn per vehicle class with degrees /
s read from RAM; other measurements want the same. Some scripted-pad trials already exist (the roll strip kit under
`lead_tmp/rolls/ps2`, the walk and stand probes): read `engine/artifacts/pcsx2/README.md` and those first and build on
what works rather than beside it.

## Do
- One documented entry point (`engine/artifacts/pcsx2/`): a pad script (buttons, stick values, durations in game
  updates or ms), RAM reads sampled each update, screenshots on demand, all under the PCSX2 session's isolation rules.

## Acceptance
- A demo run recorded in the card: BF2 on foot, full right stick for 2 s, the player's yaw from RAM every update, the
  turn rate in deg / s; repeated 3 times, the spread printed. The README section written.
