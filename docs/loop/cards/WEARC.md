---
id: WEARC
title: SSD wear, ci side (split from WEARX): measure the bytes one ci.sh run writes, then stop ci from linking example binaries it never runs (nextest / cargo test build every game example in debug; clippy --all-targets only checks them) - the maintainer 2026-10-15: SSD wear is a big deal
milestone: M0
value: 5
phase: build
status: done
deps:
files: engine/ci.sh
declares:
needs:
effort:
branch: wearc
resume: landed eb331e78 2026-10-16
created: 2026-10-15
alpha: yes
---
## Why
The maintainer, 2026-10-15 (card WEARX, his own session): the loop writes ~78 GB/h to the system SSD, almost all cargo output. WEARX's step 2 (gates.py builds every release example) edits a guard file and waits on the maintainer's approval; its step 3 - ci.sh - does not, so it is split off here (planner 2026-10-15 23:45).

## Do
1. Measure first, on the new profiles (46c6555): bytes written by one `./ci.sh` from a clean-ish target (cgroup io.stat wbytes of a `systemd-run --scope`, or /proc/<pid>/io of the cargo processes), and the size of target/debug/examples after it.
2. ci.sh: `cargo build --workspace` builds no examples; nextest / `cargo test --workspace` builds and links every example as a binary. Keep examples COMPILE-checked (clippy --all-targets already type-checks them) but not linked: e.g. nextest/cargo test with `--lib --bins --tests` (plus `--doc`), and confirm no test reads an example binary (grep tests for `examples/` / CARGO_BIN_EXE). An example a test needs is built by that test's own step.
3. Re-measure.

## Acceptance
- `./ci.sh` on the branch: the same pass / fail / ignored counts as main (1553 / 0 / 46 today), pasted.
- Bytes written per ci.sh run and target/debug/examples size, before and after, in the report.
- A deliberately broken example (scratch edit) still fails ci (clippy), pasted, then restored.
