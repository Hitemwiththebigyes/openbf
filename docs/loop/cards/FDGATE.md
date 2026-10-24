---
id: FDGATE
title: Wire the differ in as a graded metric: bisect mode (which of two commits first moved the trace), and an optional gate row in gates.py / gates.toml, 'frames matched before first divergence' on a fixed (map, seed, recording) set, with the allowed direction (never fewer) so a landing that makes the port diverge from the PS2 sooner is a MOVED row
milestone: M0
value: 2
phase: build
status: blocked
deps: FDIFF
files: scripts/fdiff.py,scripts/gates.py,engine/artifacts/loop/gates.toml
declares: gates
needs: maintainer — approve [approvals.FDGATE] oracle = "guards" before gates.py edits; FDIFF dependency and real trace/baseline inputs remain required
effort:
branch:
resume: 2026-10-24 07:50 UTC: planner guard preflight found no FDGATE approval. Keep FDIFF dependency; obtain guards approval before gate edits and use only restored real traces for original graded-row/bisect acceptance. Coordinate baseline writes through authorized planner/landing path. No change to approved thresholds. engine/artifacts/loop/reports/20261024_0750_planner.md
created: 2026-10-08
---

## Why
Research rec. 3 and 6 together: gates today check "nothing declared moved", not fidelity (OBST made two maps worse and only an audit saw it). A graded fidelity number with a direction gives a landing a self-check against the PS2.

## Do
1. `fdiff.py bisect <good-sha> <bad-sha> --case <case>`: builds each commit in a scratch worktree (own target dir, capped scope, never the shared one), traces the case, bisects for the first commit whose trace differs from the good one and prints it with fdiff's report. Port-vs-port only (the PS2 trace is a stored golden, not re-run).
2. `gates.toml [fidelity]`: a list of cases (map, seed, stored PS2 trace path under `engine/artifacts/oracle/ps2/`, port InputLog) with `frames_matched = <n>` and `direction = "up"`. `gates.py` runs `game --sim-trace` per case (release build already present for the sim group), `fdiff.py`, and emits a row: value vs expected; PASS equal or higher, MOVED lower (a lower number is a regression candidate), the row marked OPTIONAL so a missing PS2 trace is a SKIP, not a failure. Protected-oracle note: stored PS2 traces are read-only to builders like goldens (research rec. 4).

## Acceptance
- One real case in `gates.toml`; the gate row appears in a gates run's output with its number, pasted.
- A deliberately worsening change (scratch worktree) turns the row MOVED; reverted. Pasted.
- `gates_selftest.py` covers the row logic (PASS / MOVED / SKIP).
- `bisect` run on a planted regression over >= 3 commits finds the right one.

## Notes
Declares `gates`: it adds a gate row, it does not move any existing expected value. If PSREC/PSHASH came back no-go, this card shrinks to the port-vs-port bisect and the fidelity row is dropped (say so in the report).
