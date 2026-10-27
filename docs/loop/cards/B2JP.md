---
id: B2JP
title: BF2 Jabba's Palace (TAT3) is not a port map: TAT/TAT3.LVL with tat3c_con / tat3g_con (+ 1flag), standard rosters and Gamorrean-guard locals, is on the disc and in BF2's Instant Action list (row 7) - bake it under its own key
milestone: M1
value: 3
phase: build
status: blocked
deps:
files: scripts/rebake.sh,scripts/gen_missions.py
declares: rebake=maps,sounds; goldens=maps/bf2_tat3.png; sweep
needs: the maintainer's decision to adopt bf2_tat3 (assets_baked, missions, golden); GAM guard bake + locals runtime; PS2 pairs
effort:
branch:
resume: Main registration checkpoint e9d9012; restore TAT3 inputs and complete original Acceptance including guard assets/runtime. See reports/20261025_0031_card_B2JP.md; do not repeat unchanged input audit.
created: 2026-09-29
defer: post-alpha
tries: 1
---
## Why
UAUD2 (analysis/unit-audit-bf2.md §2). PLAN's north star is all maps of both games; the port bakes 17 BF2 ground maps and
not Jabba's Palace. No record of a decision to drop it was found (PLAN, PLAN_LOG, STATUS, INBOX, cards; FOGN mentions a TAT3
fog golden only).

## Evidence
- Disc: extracted/bf2/lvl/DATA/_LVL_PS2/TAT/TAT3.LVL; scripts tat3c_con, tat3g_con, tat3c_1flag, tat3g_1flag.
- Rosters (census2_con.md): tat3c_con REP vs CIS 16/150 each (ep3 set + droideka; heroes Aayla / Maul); tat3g_con IMP vs
  ALL 16/150 (heroes Boba Fett / Luke); both add team 3 locals gam_inf_gamorreanguard 3 (SetUnitCount 2), a melee axe
  (gam_weap_inf_axe, BASE com_weap_inf_lightsaber), SIDE/GAM.LVL.
- BF2's IA planet list shows Jabba's Palace at row 7 (pcsx2 README, cor1g_menu.png).

## Acceptance
- A map key (e.g. bf2_tat3) baked (map, sounds, nav, classes), its conquest RONs from gen_missions.py (MAPS entry), a
  golden viewed; the sweep, walkabout and PCSX2 pairs include it.

## Notes
- The Gamorreans are locals (the port's locals are carried, not consumed - MSND/TEAMS); their melee weapon needs HERO's
  saber machinery.

## Planner note (2026-09-30, M-ALPHA)
`defer: post-alpha` — not needed for the alpha's six showcase maps or its checklist. Value 3 (kept).
