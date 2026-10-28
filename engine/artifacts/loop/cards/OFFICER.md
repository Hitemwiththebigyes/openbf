---
id: OFFICER
title: BF2 officers use their area abilities
milestone: M3
value: 4
phase: measure
status: in-progress
deps:
files: engine/crates/core/src/weapon, engine/crates/core/src/sim/abilities.rs, engine/crates/core/src/sim/pickups.rs, engine/crates/core/src/sim/ordnance.rs, engine/crates/game/src/roster.rs, engine/crates/game/src/fx
declares: sim; ci=+15
needs: maintainer validation per Astra instruction
effort:
branch: codex/bf2-officer-abilities
resume: Run the added tests and compare officer effects, supply, query boundaries and presentation with live BF2 before accepting issue 47.
created: 2026-10-28
---

The maintainer assigned the exact scope of upstream issue #47 in this chat. At inspection the issue had no claim comments; this session has not posted externally. Scope: original ability research and implementation for officers. Hero copies and Force powers remain deferred.

Implementation preserves fourteen officer class entries and adds parsed radius/team/timer/value configuration, secondary-channel activation, offense/defense factors, healing, attributed poison, caster-on-success logic, recipient visibility, replay hashing, original activation sounds and default aura effects. A provisional bot support policy selects useful abilities and generic attack scoring excludes them. Source evidence, original/port comparison and remaining approximations are in `analysis/re-officer-abilities.md`.

Thirteen core tests and two data-gated integration tests are added. The maintainer's explicit Astra instruction says not to validate outputs and to ask the maintainer to validate to save usage; accordingly no tests, Clippy, render/playtest validation or packed-build smoke checks were run. Release compilation and packing are still required by the local playtest-build instruction. This card remains in progress until the tests and original-game comparison are accepted. The checkout does not contain `engine/artifacts/loop/STATUS.md`.

Release compilation succeeded. Packed all 23,923 current asset/mission files into a temporary executable, then replaced `E:/Projects/openbf/dist/openbf.exe` after packing succeeded. `dist/openbf_data` and its profile/playtest files were preserved. No baked inputs changed, so no rebake was required. Logs: `engine/artifacts/lead_tmp/officer-abilities-release.log`, `officer-abilities-pack.log`. The installed output has not been validated.

## Cleanup and commit request

The maintainer requested architecture/duplication review and a commit. Cleanup moves support selection into the pure AI input path and timed state into `sim/buffs.rs`; shares pickup/ability refresh, perception visibility, bolt/blast/poison hit/death bookkeeping, particle spawning and unit following; removes per-tick target-vector allocation and duplicate roster parsing. No new code comments, dependencies or baked assets were added. Validation remains assigned to the maintainer under the Astra instruction; this commit does not close issue #47 or complete its original-game comparison.

The final reviewed source compiled successfully in release mode without compiler warnings (`officer-abilities-review-release-final.log`). Tests and Clippy were not run. After committing, rebuild and repack the committed source before final handoff; the corresponding logs are `officer-abilities-review-committed-release.log` and `officer-abilities-review-committed-pack.log`.
