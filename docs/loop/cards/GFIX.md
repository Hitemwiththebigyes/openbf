---
id: GFIX
title: gates.py: golden_report skips the walkabout referee's non-image golden (it crashed every golden-moving gate run and landing since 61bfd0b)
milestone: M0
value: 5
phase: cleanup
status: done
deps:
files: scripts/gates.py
declares:
needs:
branch: gfix
resume: landed c45a196c 2026-09-25
created: 2026-09-25
---
- **Why:** since 61bfd0b goldens.sh also regenerates goldens/walkabout_stats.ron; golden_report opens every moved golden as an image and dies with UnidentifiedImageError, so gates.py --auto and land.py phase 1 crash whenever a golden moves (found by WCOLB, 2026-09-25, lead_tmp/wcolb_gates.log).
- **Fix:** list non-.png moves as 'not an image' in QUANT.md and skip the sheet.
- **Acceptance:** every gate PASS, nothing declared.
