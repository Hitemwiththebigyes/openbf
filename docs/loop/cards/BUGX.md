---
id: BUGX
title: Bug checks, each a test whether or not it reproduces: (a) bots shoot corpses or heal / resupply dead bodies; (c) the hit-direction chevrons carry into the next life; (d) no respawn after a death in water or after Suicide
milestone: M3
value: 4
phase: build
status: parked
deps:
files: engine/crates/ui/src/menu.rs, engine/crates/game/src/main/windowed.rs, engine/crates/game/src/main/windowed/front.rs, engine/crates/game/src/pause.rs, engine/crates/game/src/lib.rs, engine/crates/game/src/walk.rs, engine/crates/core/src/sim.rs, engine/crates/core/src/sim/input.rs, engine/crates/core/src/sim/state.rs, engine/crates/core/src/sim/step.rs, engine/crates/core/src/sim/combat.rs, engine/crates/game/src/main/headless.rs, engine/crates/game/src/blackbox.rs, engine/crates/game/examples/round_to_outcome.rs, engine/crates/game/tests/suite/bug_checks.rs, engine/crates/game/tests/suite/main.rs
declares:
needs: (private workflow note omitted)
effort:
branch: bugx-suicide-20261024
resume: 20261025 local pass 3: Respawn slice validated headless on 6 real maps with mutation controls; fix_pass3.patch (disc key game.pause.suicide + stuck-command test) proven, not applied. Owed: live window steps, localized popup/Yes/No text, medic/ammo soldiers.
created: 2026-10-21
---
## Why
INBOX 2026-10-21 20:00 item 5 (an outside review's list; the PS2 corpus decides each). Item (b), a weapon that does no
damage to the player, is folded into NODMG.

## Do
For each: a headless scene that tries to make it happen and asserts it does not (a bot with a corpse in its sight, a
medic / ammo class beside a dead team-mate; a hit just before a death and the HUD's chevron timers at the respawn; a
death in water and the Suicide menu action, then the spawn screen and a respawn). A case that reproduces gets its fix
here if small, else its own card.

## Acceptance
- Four tests (a, c, d-water, d-suicide), each mutation-checked; the report says which reproduced on main and what fixed it.

## Fixture-first continuation (2026-10-24 10:33 UTC)
Source development can begin with synthetic scenes in an isolated loopctl worktree. First reproduce each case on unchanged main, keeping corpse-targeting/service and HUD/respawn paths distinct; then mutation-check all four original cases. Where a full game path needs missing maps/fonts, checkpoint that exact gap without labelling a fixture a runtime reproduction. All applicable CI/landing gates remain owed; no permission to close from source-only results.

## Fixture checkpoint (2026-10-24 10:56 UTC)
Test-only, uncommitted branch `bugx` at `lead_tmp/worktrees/bugx`; unchanged production base `3d487d9`.
Four synthetic fixtures PASS before/after mutations; each injected target-lifetime, station-alive, chevron-fade and environmental-respawn-timer fault is rejected and restored. No bug reproduced in these limited scopes; no production fix.
These are not the original four complete cases: station service does not cover medic/ammo soldiers; no Suicide menu action exists in the current Row/MenuAction/handler; HUD uses supplied model/empty layout, and water uses an authored death region under a synthetic plane. Full original Acceptance is still owed.
CI exits 101 (game suite 194 pass / 2 fail / 1 ignore): absent `gamefont_large` and graphics adapter. Fmt/clippy/build pass; no landing/gate waiver. Exact nine absent prerequisites, caller inventory, candidate patch and logs: `lead_tmp/bugx_cloud/`.
Report `reports/20261024_1056_card_BUGX.md`; continuation `lead_tmp/bugx_cloud/next_prompt.md`.

## GitHub checkpoint recovery (2026-10-24)

