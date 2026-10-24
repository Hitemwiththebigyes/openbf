---
id: HASHN
title: Hash-annotated decompiles: every FNV constant in both PS2 decomps followed by its name, and re_check.py hash <name> lists the functions that read a property
milestone: M0
value: 3
phase: build
status: done
deps:
files: scripts/annotate_hashes.py (new), scripts/re_check.py, scripts/tests/test_annotate_hashes.py (new)
declares:
needs:
effort:
branch: hashn
resume: (private workflow note omitted)
created: 2026-10-19
---
## Why
(the maintainer's INBOX 2026-10-19, "a tool to build") The RE often asks "which functions read ODF property X": today that is a hand fnv-hash of the name and a grep for the constant in both of its forms (RULES: the signed / 64-bit forms; `lead_tmp/veh0/*.py`). An annotated copy of each decomp, every FNV constant followed by its name as a comment, makes it one grep.

## Evidence
The decomps `analysis/SWBFRONT.ELF-decomp.c` / `SWBFRNT2.ELF-decomp.c`; name sources: the discs' ODF keys, Locl keys, class names, the `StringsFromExecutable` lists, the bake's property tables; the hash forms Ghidra prints (unsigned hex, signed decimal, 64-bit sign-extended, split lui/addiu pairs).

## Acceptance
1. `scripts/annotate_hashes.py` writes `analysis/<ELF>-decomp.annotated.c` (gitignored like the decomp) for both games; a count of constants resolved per form printed.
2. `re_check.py hash <name>` prints every function (with `fn`) that names the property, from the annotated copy; a test on 5 known properties (e.g. MaxSpeed, SkeletonRootScale, ChunkFrequency, FirePointName, TriggerRadius) against their known readers in `analysis/`.
3. A false-positive check: constants under 0x10000 and known non-hash words are never annotated; the number of names with two or more hash collisions is printed.

## Notes
Tooling only (no game code); value 3.
## Planner checkpoint (2026-10-24 13:06 UTC)
Fixture-first development is possible, but both full corpora, original name sources and the five real-property reader controls must be restored before acceptance. Keep annotation/name-query work separate from RECHK search normalization; no existing receipt is rewritten.

## GitHub checkpoint recovery (2026-10-24)

