---
id: KBMR
title: Configurable keyboard and mouse action bindings
milestone: M3
value: 4
phase: build
status: parked
deps:
files: engine/crates/game/src/kbm.rs,engine/crates/game/src/profile.rs,engine/crates/game/src/main/windowed.rs,engine/crates/ui/src/menu.rs,engine/crates/ui/src/menu
declares: ci=+20; goldens=shell_options.png
needs: by-hand remapping playtest and remaining UI follow-up; implementation integrated
effort:
branch:
resume: Implementation in PR21 is integrated with maintainer fixes. Preserve remaining end-to-end device acceptance and deferred polished UI.
created: 2026-10-26
public_issue: https://github.com/sprucey149/openbf/issues/13
---

## Why

The roadmap promises remapping. Keyboard/mouse action bindings currently use
`Bindings::pc_default`; players need to customize them through the controls UI.
Existing sensitivity and invert settings remain available.

## Proposed acceptance

- Change a keyboard/mouse action binding in the controls UI, save it in the
  profile, and retain it after restarting. Older profiles keep current defaults.
- Restore defaults and handle conflicts, cancelled capture, focus loss and
  press/release edges without stuck or duplicate actions.
- Keep default-input behavior, controller input and existing deterministic
  replays unchanged. Preserve the current simulation/input boundary.
- Retain discriminating persistence/input tests, normal integration checks and
  a before/after UI demonstration. Record any required manual checks honestly.

One feature branch/PR. Agree on the final scope and assignment before coding;
an early draft PR should make overlap visible. This proposal does not authorize
unrelated input, camera, simulation or vehicle changes.
