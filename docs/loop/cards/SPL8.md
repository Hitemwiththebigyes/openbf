---
id: SPL8
title: game/src/viewer/scene.rs (1,546 lines) and game/src/assets.rs (1,514) are over the 1,500 cap with no waiver — split them (SPLIT_RECIPE) or waive them in scripts/size_waivers.txt (merged: SPL9)
milestone: M0
value: 1
phase: cleanup
status: todo
deps:
files: engine/crates/game/src/assets.rs, engine/crates/game/src/main/windowed.rs, engine/crates/game/src/sounds.rs
declares:
needs:
effort:
branch:
resume: 2026-10-24 14:22 UTC: Current source: scene.rs 1453 already under cap; active assets.rs 1781, windowed.rs 1983, sounds.rs 2665. Do not split scene again. Check GCL16 item 9 already shared via battle_resources. One source file per slice; unchanged game/GPU/bake/sim gates still required. reports/20261024_1422_planner.md; lead_tmp/planner_20261024_1417/next_prompt.md.
created: 2026-09-29
---

## Why
Cleanse 2026-09-29: `scripts/size_report.sh` lists `game/src/viewer/scene.rs` at 1,546 non-test lines; SPL7 names four
other files and `scripts/size_waivers.txt` names none. Split it by `engine/artifacts/briefs/SPLIT_RECIPE.md` (one file,
one session, a pure move, every gate PASS) — or, if no seam gives two 400+ line parts, waive it with that reason.

## Evidence

## Acceptance

## Notes

## Merged from SPL9 (planner 2026-09-30 21:45; the maintainer's batch rule: value <= 3, shared files — one session, one gates run)
**SPL9's title:** game/src/assets.rs is 1,514 lines, over the 1,500 cap with no split card or waiver — split it (SPLIT_RECIPE) or waive it in scripts/size_waivers.txt

## Why
Found by the scripts cleanse 2026-09-29 (card CLNS): `scripts/size_report.sh --check-map` lists game/src/assets.rs at 1,514 lines; no SPL card and no waiver name it (SPL7 has the four core/bake files, SPL8 viewer/scene.rs).

## Acceptance
A pure move per engine/artifacts/briefs/SPLIT_RECIPE.md (every gate PASS, nothing declared), or a waiver line with the reason.

Both halves have their own acceptance above; land them as two commits on one branch, one `land.py`.

## Note (cleanse CLNF, 2026-10-06)
Two more `game` files are over the 1,500 cap with no split card or waiver: `game/src/main/windowed.rs` (1,632; WTHR's splashes and the black box hooks) and `game/src/sounds.rs` (1,550). They join this card (same crate, same recipe) rather than a new one; `scene.rs` is 1,770 and `assets.rs` 1,616 today.
