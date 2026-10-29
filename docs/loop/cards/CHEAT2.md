---
id: CHEAT2
title: Five session cheats on the PS2's pause-menu Morse code - invincibility (sos), infinite ammo (ammo), no reloading (clip, split from ammo), infinite stamina (run), unlock all classes (all), plus `cheat` for all five; on until the main menu
milestone: M3
value: 3
phase: build
status: done
deps: UNLK3
files: engine/crates/core/src/sim/state.rs,engine/crates/core/src/weapon.rs,engine/crates/core/src/sim/step.rs,engine/crates/core/src/sim/guns.rs,engine/crates/core/src/sim/ordnance.rs,engine/crates/core/src/sim/pickups.rs,engine/crates/core/src/sim/remotes.rs,engine/crates/ui/src/menu.rs,engine/crates/game/src/cheats.rs,engine/crates/game/src/pause.rs,engine/crates/game/src/unlocks.rs,engine/crates/game/src/main/windowed.rs,engine/crates/game/src/main.rs,engine/crates/game/tests/suite/cheat_ammo.rs,engine/crates/game/tests/suite/unlocks_bf1.rs,analysis/re-cheat-codes.md
declares:
needs:
effort:
branch: cheats
resume: 2026-10-28 built on playtest-20261028; waiting for the maintainer's playtest (the windowed wiring has no automated test).
created: 2026-10-28
---
## Why
The maintainer 2026-10-28: "some cheats I want to add in addition to what we have: invincibility, infinite ammo, infinite
stamina, unlock locked characters, no reloading. The no reloading is a part of the infinite ammo currently, so those
need to be separated. [...] each of these needs a morse code like the bf2 ps2 version had, and when a cheat code is
toggled, it remains toggled until the session ends, ie the game is exited or the user quits back to the main menu,
then everything toggles back to off." Later the same day: one more word, `cheat`, toggles all five together.

## Acceptance
- Each word's exact D-pad sequence toggles its cheat; a second entry toggles it back; a wrong letter is dropped; a
  wrong word resets (`game/src/cheats.rs` tests).
- `cheat`: all five on if any is off, all five off if all are on (`game/src/cheats.rs`).
- Infinite ammo vs no reloading vs both on a real weapon, per tick (`suite cheat_ammo`).
- Unlock-all makes a 12-point class spawnable at 0 points (`suite unlocks_bf1`).
- The flags survive a mission change and clear at the main menu (`game/src/cheats.rs`).
- Left/Right never step a pause-menu row while paused (`game/src/pause.rs`).

## Build (2026-10-28)
- Words and D-pad sequences: `analysis/re-cheat-codes.md` "Port". `sos`, `ammo` are the PS2's; `clip`, `run`,
  `all`, `cheat` are ours (`ui::menu::OUR_WORDS`, matched by the same FNV-1a lookup; no PS2 hash collides).
- `core::sim::Cheats`: `infinite_ammo` now only fixes the reserve; new `no_reload` and `all_classes`;
  `id_for_word`, `toggle_five`. Weapon (`FireInput::no_reload`, `WeaponState::free_reload`): `ammo` = the clip
  empties and the reload fills it without touching the reserve (even an empty one); `clip` = shots draw from the
  reserve first so the clip stays full, then the clip to 0, never a reload; both = nothing drawn. Players only (the
  sim's `ammo_cheats`), on foot, held grenades and vehicle seats.
- `game::cheats::Session` holds the flags, `hud`/`slow` and the Morse buffers; `App::battle_resources` installs it
  into every new battle's `Sim`; pause rows and Morse change the same flags; the playlist's end (Quit or last
  map) turns them all off, and so does exit. Never saved; not hashed; not in replays (as before).
- A recognised word plays `shell_menu_accept` and says the line in the HUD feed.
- `[ours]` While paused, Left/Right never reach the rows (the Respawn popup keeps them): the in-battle sliders
  now step only from the shell's Options page.
- `--cheat` takes `clip`, `run`, `all` too; `--cheat ammo` alone no longer skips reloads.
