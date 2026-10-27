---
id: WEARX
title: SSD wear: the gates build only the examples they run (cargo build --release -p game --bins --examples links ~27 example binaries, each the whole engine; the maintainer 2026-10-15 - SSD wear is a big deal)
milestone: M0
value: 5
phase: build
status: done
deps:
files: scripts/gates.py,scripts/land.py
declares: oracle=guards
needs:
effort:
branch: wearx
resume: landed d3b3d3f3 2026-10-16; report reports/20261016_1130_card_WEARX.md
created: 2026-10-15
alpha: yes
first: 1
---
## Why
The maintainer, 2026-10-15: SSD wear is a big deal. Measured that day: the WD SN850X (the system/home drive, btrfs) took 289 GB in 3.6 h of uptime, 245 GB of it from openbf.slice (cgroup io.stat); SMART lifetime 17.5 TB written, 1 % used, 560 power-on hours, so the loop writes ~78 GB/h against a lifetime ~31 GB/h. Almost all of it is cargo output: engine/target is 154 GB (debug 121: deps 71, examples 31, incremental 19; release 33: examples 27), and each lane builds its own target (lead_tmp/*_target 120-175 GB apparent each).
Already done (commit 46c6555): [profile.release] and [profile.dev] debug = line-tables-only, dev incremental = false.

## What to do
gates.py line ~143 runs `cargo build --release -p game --bins --examples`: it links EVERY example of the game crate (27 files: arena, round_to_outcome, leak_census, pose_render, walkabout, ...), each a full engine binary. The gate groups only run a few (grep gates.py for release/examples/: round_to_outcome, arena, leak_census, pose_render, spawn_node_census/ordnance_census if used - verify).
1. Measure first: bytes written by one `gates.py --groups ci sim sweep` run before/after (cgroup io.stat wbytes of a scope, or /proc/PID/io of the build), on the new profiles.
2. Build only the examples each group needs (`--example a --example b`, or per-group builds), keep one build per run so nothing links twice. Sessions that need another example build it on demand with cargo run.
3. Same for the debug-profile builds in ci.sh (cargo test builds the examples too unless --lib --tests/--bins is given: check whether examples need building at all in ci).
Acceptance: gates.py rows identical on main (same PASS/MOVED set as the last full run), write volume per gates run measured before and after in the report. gates.py is a guard file: the card declares oracle=guards.
## Evidence
see Why; /proc and io.stat numbers in the chat of 2026-10-15.
