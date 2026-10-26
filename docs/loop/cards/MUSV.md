---
id: MUSV
title: The battle's music proven in the window, and a music volume: the windowed music frame runs without a sound device so win_run.sh logs the same 'music ... start' lines as the headless round (AMBM's windowed wiring is exercised by no check), and one music gain in the pause menu's options (the PS2's ingamemusic bus)
milestone: M1
value: 5
phase: build
status: done
deps: AMBM
files: engine/crates/game/src/main/windowed.rs,engine/crates/game/src/main.rs,engine/crates/game/src/music.rs,engine/crates/game/src/profile.rs,engine/crates/ui/src/menu.rs,engine/CRATES.md
declares: ci=+1
needs:
effort:
branch: musv
resume: landed 9f2aa099 2026-10-06
created: 2026-10-03
alpha: yes
---
## Why
AMBM (1bde70c) gave the battle its music, and no check has run the path the maintainer will use. The rules, the records, the
timing and the decoder are tested, and a headless round logs the changes (`MUSIC_LOG`), but `main/windowed.rs` starts
a take only behind an open sound device, and the windowed game under Xvfb opens none (`win_run.sh` runs it with an
empty ALSA config; `Audio::new()` returns `None`). So if the windowed wiring is wrong, the first to find out is the maintainer,
by hearing nothing. Second: the music has no level of its own. The PS2 has an `ingamemusic` bus beside the effects.
Filed by the planner from INBOX 2026-10-03 07:10 (report `reports/20261003_0710_card_AMBM.md`, "Not checked").

## What to build
1. **The music frame runs without a mixer.** The windowed game's per-frame music step (feed the manager from the sim,
   ask for a take, follow its fade) runs whether or not a device opened; only the last call (hand the take to kira)
   is skipped when there is none. Each start, stop and state change prints the same line `MUSIC_LOG` prints headless
   (`music <t> s start <name> ...`), in dev mode.
2. **A music volume option**: one gain on the music voice, beside the effects' level, in the pause menu's options
   (`ui/src/menu.rs`) and as an environment variable for a headless or scripted run; 0 = no take is started. The
   default is the stream property's own gain (what AMBM plays today).
3. Not this card's acceptance, a lead: BF1's state rule (team 2's state alone) is PROVISIONAL, read from the
   instructions; `analysis/re-ambience-music.md` 11 names the live check (a BF1 battle where the two teams' states
   differ, the asked Music read from RAM with `lead_tmp/ambm/tools/ram_music.py`). If an audit takes AMBM, that is
   its first live check; a session here with a free PCSX2 slot may run it and drop the label.

## Acceptance
- `scripts/tools/win_run.sh` on BF1 Naboo Plains with a scripted spawn (no sound device): the
  window's log shows `music ... start all_nab_amb_start` at the first spawn and the change to `..._amb_middle` when
  the reinforcements pass the threshold, at the same game times (within 0.1 s) as `MUSIC_LOG=2` on `round_to_outcome`
  for the same seed and team. Paste both logs' music lines into the report.
- The same run with the music volume at 0: no `start` line; at the default: the lines above.
- A test pins the option's default and its range; `hud_layout` / menu goldens move only by the new menu row
  (declare `goldens=` by name if the pause menu is in one).
- sim, simhash, sweep, arena, leak identical (nothing here is in the sim).

## Notes
- Mutation-check the windowed check once: break the windowed start (skip the manager's update) and see the log lose
  its lines.
- UISN (interface sounds) also edits `main/windowed.rs`; the picker will not run the two side by side.
## Landed 2026-10-06 (9f2aa09) - report `reports/20261006_1200_card_MUSV.md`
- Window log = headless log in Musics, takes and order (BF1 Naboo, seed 1); volume 0 (env and profile) prints no line; mutation run prints none.
- NOT met as written: the 0.1 s time match (window 16.3 s, headless 12.6 s for `middle`) - the window's battle has an idle human in a slot, so its reinforcements fall on other ticks.
- Item 3 (BF1 state rule live on the PS2) not done; `state_bf1` stays PROVISIONAL and no open card holds it.
