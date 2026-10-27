---
id: SQMK
title: Squad markers off by default (a hidden OPENBF_DEV mode brings them back); each order's text shows once with its unit count
milestone: M1
value: 5
phase: build
status: done
deps:
files: engine/crates/game/src/main/battle.rs,engine/crates/game/src/main.rs,engine/crates/game/src/main/windowed.rs,engine/crates/game/src/squad_vis.rs
declares:
needs:
effort:
branch: sqmk
resume: landed 29185577 2026-10-02
created: 2026-09-30
alpha: yes
---
## Why
The maintainer's INBOX 2026-09-30 11:40 (corrected 12:05): "squad markers should be dropped altogether. Maybe we can keep them for
a hidden 'dev' mode … for normal gameplay it should be invisible." Keep the brief on-screen command text with the count
of units taking it; the teal markers go. Today they are huge flat teal polygons that fill a third of the screen up
close (`lead_tmp/playtest_0930/kam1_play_5.png`, `_6`, `_7`), and the message box prints "Follow me — 3 units" twice
(`kam1_play_5.png`).

## Do
- The in-world squad marker is ON unless `OPENBF_SQUAD_MARKER=0` (`game/src/main/battle.rs:59-81`, the `main.rs` header
  line 64): make it OFF by default. The Hold anchor's `squad_cmd_cross` (`game/src/squad_vis.rs`) off by default too,
  unless SQACK finds the PS2 draws one in normal play.
- A hidden **dev mode**: `OPENBF_DEV=1` (and/or a pause-menu cheat word, as the BF2 cheat words already work) turns the
  marker and cross back on, plus later debug overlays (the AI stuck-unit / route overlay would help STALL). Not in any
  menu. `OPENBF_SQUAD_MARKER` stays as a per-overlay override (1 forces on, 0 forces off).
- The command text: one line per order, brief, fading, with the count ("Follow me — 3 units"), shown ONCE — coalesce an
  identical message (`game/src/main/windowed.rs` ~1199). Draw it in the placeholder box now; BHUD2 restyles it in the PS2
  HUD's message style (note on BHUD2).

## Acceptance
- A test pins the defaults: marker and cross off with no env; on with `OPENBF_DEV=1`; `OPENBF_SQUAD_MARKER` overrides.
- A test: issuing one order yields exactly one message line carrying its unit count.
- A normal play session (a `game` screenshot after a Follow order with three teammates near) shows no marker and no
  cross, and the order's text once. Goldens unchanged (every gate PASS).- EVMSG RE 2026-09-30 (`analysis/re-event-messages.md` §2): on the PS2 an order shows to the ordering player as ONE line of the BF2 systemMessages element (upper-left, 4 lines, fade 0.5 / hold 3.0 / fade 0.5 s) in cyan (32,223,223): Locl 0x731fd493 "Follow me", 0x0e0280e5 "Move out", 0xa0590ed7 "Hold on" (B2 `FUN_00240bf8` D:161347 / 161454 / 161558 / 161696), 0xf00699ad "Get in" (`FUN_002e1670` D:269092), 0x7e3c4096 "Get out" (`FUN_002e1ef0` D:269247); BF1 has the same five strings. No unit count in the PS2 line.

- SQACK RE 2026-09-30 (`analysis/re-squad-orders.md` §2; frames `engine/artifacts/lead_tmp/sqack/scenes/hud_sheet.png`): **BF1's PS2 feedback for an order is a 2-D HUD element, nothing in the world**: a small D-pad cross (`squad_cmd_cross`, 64x64, bottom right at (-120, -50)) with the FOUR order names round it (`gamefont_tiny`: MOVE OUT up, AT EASE left, FOLLOW ME right, HOLD POSITION down; `game.squadcommand.field.*`), the chosen arm at alpha 255 with its `squad_cmd_highlight`, the others at 127; the whole element fades 255 -> 0 over 2.0 s (measured 1.97 s in 11 scenes). It shows on every order, also when nobody took it. No per-unit marker, no anchor cross, no unit count anywhere (B1 `FUN_002d8698`, `FUN_002d84a0` are the only users of the two textures). So "markers off" is the PS2's; "the order's text with its unit count" is the port's own — the PS2's text is the cross's label. The builder may draw the cross as BF1 does (the two textures are already baked from `common_bf1`).

- LANDED 2026-10-02 (2918557; report `reports/20261002_0446_card_SQMK.md`): markers load only under `OPENBF_DEV=1` (`OPENBF_SQUAD_MARKER=1|0` overrides); the message box is `game::event_feed::EventFeed` (a tick's events read once, an identical line shown once); headless `--hud` shows it. Acceptance pair `lead_tmp/sqmk/accept/{normal,dev}/walk_00060.png`. No fade, no cheat word, no BF1 cross (EVMSG / SQACK). The doubling's cause also hits effects and sounds: card EVTK.
