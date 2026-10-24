# Version differences — VDIFF checkpoint (2026-10-24)

Incomplete: platform differences have not been measured yet.
The original card's acceptance remains open. `NM` below means **not measured**,
including the population denominator; it does not mean equal or zero differences.

| BF1 category | PS2 vs Xbox differences / compared | PS2 vs PC differences / compared |
|---|---|---|
| Class / weapon ODF properties | NM | NM |
| Mission values, per map / side | NM | NM |
| Collision bodies / shapes / triangles / bounds | NM | NM |
| Terrain heights / placed objects | NM | NM |
| Animation names / lengths / root motion | NM | NM |
| Platform-exclusive content | NM | NM |

| BF2 category | PS2 vs Xbox differences / compared | PS2 vs PC differences / compared |
|---|---|---|
| Class / weapon ODF properties | NM | NM |
| Mission values, per map / side | NM | NM |
| Collision bodies / shapes / triangles / bounds | NM | NM |
| Terrain heights / placed objects | NM | NM |
| Animation names / lengths / root motion | NM | NM |
| Platform-exclusive content | NM | NM |

The ten largest differences for each game remain unranked. The Hoth population
example in the card is historical context, not a measurement reproduced here.

The retained prerequisite command is:

```sh
python3 engine/artifacts/lead_tmp/vdiff/preflight.py > engine/artifacts/lead_tmp/vdiff/preflight.json
```

This run found 12 of 12 named paths missing (0 available), recorded individually
in that JSON. These are filesystem probes, not game-content counts. They cover
the ISO container, both PS2 extracts and mission directories, the named BF2 PC
LVL directory, both PC mod-tool script directories, Xbox tools / reader /
comparator, and baked assets. The inventory only checks these named paths;
it establishes neither ownership nor absence of editions elsewhere.

Restore the PS2 raw discs/extracts, identify the owned retail Xbox and PC editions
for each game and their actual paths, and restore the Xbox tools cited by the
card. Mod-tool mission sources alone cannot establish retail-disc parity.
Baked assets can help cross-check decoding but cannot replace raw platform data.
No GPU capture is necessary to begin the offline data comparison.

Continuation: inventory editions and hashes; compare every required category
per map and side using the port's readers; retain the scripts and raw result
rows in `engine/artifacts/lead_tmp/vdiff/`. Preserve repeated ODF properties,
map-local overrides and missing-versus-present values. Match content identities
before comparing format bytes. Report compared populations and unmatched content
separately, then give the ten largest differences per game with an explicit
ranking rule and source rows. Independently verify the final numbers before
closing VDIFF.

Default from VDIFF remains: PS2 discs required; other versions add looks only.
Whether sim data differs, and any resulting question about carrying PS2 values,
remain unresolved. The planner should raise that question after measurement.
