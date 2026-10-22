---
id: XBTEX
title: The Xbox discs as texture donors: pack.py / rebake.sh find the Xbox BF1 and BF2 images in iso/ by their XDVDFS magic, a bake reader for the Xbox tex_ layout (DXT1 / DXT2-3 as is, the uncompressed formats unswizzled), and bake::tier takes the LARGEST copy per texture name across PS2 / Xbox / PC with the PS2 alpha mask
milestone: M1
value: 5
phase: build
status: in-progress
deps:
files: engine/crates/bake/src/tier.rs, scripts/pack.py, scripts/rebake.sh
declares:
needs:
effort:
branch: xbtex
resume:
created: 2026-10-21
alpha: yes
---
## Why
The maintainer (INBOX 2026-10-21 19:30): "pack.py should take whatever versions the player owns and use the best textures".
First look and tools: `engine/artifacts/lead_tmp/xbtex/README.md` (keep the dir). Measured there: the best copy is per
texture, not per disc (BF2 GEO1: Xbox alone has the largest of 11 names, PC alone 6; BF2 KAS2: PC wins), and BF1 gains
most (no PC BF1; Xbox BF1 NAB2 has the largest copy of 35 textures).
The maintainer's rule (19:45): another version changes ONLY the look - never a number or shape the sim reads (collision, ODF
values, mission values, terrain heights, animation timing that moves a unit, hit spheres). The PS2 is the reference.

## Do
- An XDVDFS reader in Rust inside `bake` (like `extract-iso`; `tools/xiso.py` is the reference: partition at 0 trimmed or
  0x18300000 full), found by the magic, not the filename.
- `bake xbox-tex`: `tex_` = NAME / INFO (20 bytes: w, h, depth, mips, type, D3DFORMAT, body bytes) / one BODY. Formats
  0x0C DXT1, 0x0E DXT2/3 (not swizzled); 0x00 L8, 0x04 A4R4G4B4, 0x05 R5G6B5, 0x06 A8R8G8B8, 0x19 A8, 0x1A A8L8 (Morton
  swizzled). The format codes are to be verified by decoding and looking.
- `bake::tier`: the largest copy per name across PS2 / Xbox / PC; the PS2 alpha mask carried as for tier 2.
- `_bump` maps are not this card (a look card of their own if wanted).

## Acceptance
- A per-format decode test on one known texture each (a picture checked by eye + a hash).
- A census per map: textures taken from each source (counts) - BF2 GEO1 and BF1 NAB2 at least.
- A contact sheet of the 20 biggest changes for the maintainer. The sim hash and every non-golden gate unchanged (look only).
