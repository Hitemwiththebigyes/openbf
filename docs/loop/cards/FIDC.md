---
id: FIDC
title: Fidelity gate: a per-map failure is confirmed against MAIN's own value on the confirm seeds, not against the seeds 1-8 baseline (GRAIA was refused on BF1 Endor standing 0.03 -> 0.75 where main itself reads 0.03 / 0.71 / 1.97 per 8-seed block)
milestone: M0
value: 4
phase: build
status: done
deps:
files:
declares:
needs:
effort:
branch:
resume: DONE 2026-10-11 by Claude in chat (gates.py is a guarded path): gates.py confirm_base / --fidelity-both, baseline file lead_tmp/loop_baselines/fidelity_confirm_main.json; replay: GRAIA dropped, BSPN's bf2_dag1 kept.
created: 2026-10-11
alpha: yes
---
## Why
land GRAIA 2026-10-11 (report reports/20261011_1722_land_GRAIA.md; numbers engine/artifacts/lead_tmp/graia/land/ctl/). The gate compares the confirm block (seeds 9-16) with the baseline taken on seeds 1-8. Where main's seeds 1-8 are a quiet draw the confirm run confirms nothing: stand20_pct@bf1_end1 on main 8ad30f4 (engine = baseline 4c238b1, seeds 1-8 reproduce it seed for seed) is 0.03 on seeds 1-8, 0.71 on 9-16, 1.97 on 17-24. A baseline re-take gives 0.03 again, so the planner's re-take cannot clear it.
## What to build
gates.py --rebaseline-fidelity also stores each per-map value on confirm_seeds (a second table in gates.toml [fidelity], one more 10-minute run at re-take time); fidelity_confirm keeps a per-map failure only if the branch's confirm block is worse than MAIN's confirm block past tolerance. gates.py is an oracle path (guards): the session needs [approvals.FIDC] oracle = "baselines,guards" from the planner or the maintainer, as GRFG has.
## Acceptance
gates.py --self-test with a new case (quiet baseline block, noisy confirm block on both sides -> dropped as noise; worse on both against main's own blocks -> kept); GRAIA's stored branch run (lead_tmp/graia/land/branch) re-scored: stand20_pct@bf1_end1 dropped; BSPN's (lead_tmp/bspn/land/branch) re-scored: stand20_pct@bf2_dag1 KEPT (main 0.31 / 0.38 / 0.34, branch 0.92 / 1.12).
