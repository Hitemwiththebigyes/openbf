---
id: MUSB1
title: BF1's music state rule checked live: the manager's state word against the reinforcement counts over one BF1 round on the PS2, then drop core/src/music.rs state_bf1's PROVISIONAL label or fix its thresholds (item 5 of ANNC3, split off: needs a PCSX2 session)
milestone: M3
value: 3
phase: measure
status: blocked
deps:
files: engine/crates/core/src/music.rs
declares:
needs: restore PCSX2 session toolkit, BF1 disc/RAM reader and captures, BF1 Naboo bake/sounds and mission; live music comparison unavailable
effort:
branch:
resume: 2026-10-24 cloud checkpoint: reports/20261024_1150_card_MUSB1.md; lead_tmp/musb1_cloud/preflight.json (21 probes, 1 available). Restore inputs, log a full BF1 PS2 round and current-main MUSIC_LOG by ticket counts, include states distinguishing BF1/BF2, then retain or fix rule with evidence. PROVISIONAL retained; acceptance unchanged.
created: 2026-10-21
alpha: yes
---
## Why
Filed by ANNC3 (2026-10-21) from its item 5 (MUSV's report): BF1's music state rule is still PROVISIONAL (core/src/music.rs state_bf1; analysis/re-ambience-music.md section 11). ANNC3's session had no RTX (adapter lost), so no PCSX2 session.
## Evidence
- engine/artifacts/lead_tmp/ambm/tools/ram_music.py reads the manager's state word.
- MUSV found a windowed battle and a headless one cannot be compared tick for tick (an idle human sits in a slot); compare by ticket counts, not times.
## Acceptance
The RAM log's state changes over one BF1 round against the port's MUSIC_LOG state changes for the same ticket counts, pasted; then the label dropped or the thresholds fixed (a unit test of the fixed thresholds).
