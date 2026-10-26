# LEAD VERIFICATION (Fable lead, 2026-09-03, session 12): re-read before acceptance — the texture-record field `+0x18` zeroed at `FUN_00582c90` 717331–717332 / 717572–717573 and stored at 717616 (`DAT_0079f4a8`; BF1 twin 562186 / 562227 `DAT_006cf778`), the DL opcode-0x417 writer 556053–556056 (q1 = {rec+0x18, rec+0x10}), and the VU1 register-list builder re-disassembled with the agent's `vudis2.py` over `.vutext` (readelf: off 0x4935c0, len 0x61c0): local 0x4d0 `SQ.xy vf2,3(vi2)` with vf2 = slot[2] and 0x4d8 `ISW.z vi10(=8),3(vi2)` — pair 3 is CLAMP_1 with the zeroed field as data; the seven register numbers the builder emits (0x42, 6, 8, 0, 0x47, 0x14, 0x34) are exactly ALPHA_1, TEX0_1, CLAMP_1, PRIM, TEST_1, TEX1_1, MIPTBP1_1, which pins the decode. Tags and text below are the RE agent's.

# Texture wrap mode (GS CLAMP_1) on the PS2 — BF1 + BF2 (session 12, Fable)

Tags: [V] verified in decomp lines / ELF bytes / disc bytes, [D] derived, [H] hypothesis.
Decomp: `analysis/SWBFRNT2.ELF-decomp.c` (BF2, "D:") and `analysis/SWBFRONT.ELF-decomp.c` (BF1, "B:").
VU1 addresses are LOCAL microcode addresses (see §1.3 for the .vutext → local mapping).

## §0 Headline

1. Wrap is GLOBAL for every model draw, not per texture and not per segment: the VU1 material
   register-list builder (BF2 local 0x4c4–0x4e8, BF1 0x4c8–0x4e2, byte-identical logic) emits an
   A+D pair with register 8 (CLAMP_1) whose data is qword 2 `.xy` of the material slot [V]. [V]
2. That qword is the texture record's 4th u64 (`texrec+0x18`), written by the DL opcode-0x417/0x415
   writer (D:556056, B:388701) and set to 0 unconditionally by the texture packet builder
   (`FUN_00582c90` D:717331/717572/717616; `FUN_0049c5c8` B:561920/562186/562227). [V]
3. So CLAMP_1 = 0x0 → WMS = 0 REPEAT, WMT = 0 REPEAT for ALL five textures (SKY_Hoth on both
   discs, pol1_sky, tan1_sky, nab1_waterfall) and for every other model texture, both games. [V]
4. Nothing in `tex_` carries wrap: INFO is exactly 14 bytes (w,h,bpp,detail,mipDist,mipCount) on all
   five, byte-identical for SKY_Hoth on both discs; children are only NAME/INFO/pal_/BODY. [V]
5. Hoth verdict: the base dome's bottom ring at v = 1.001 IS sampled wrapped on the PS2 (REPEAT +
   bilinear: ~63 % of the texture's top row at that ring); the disc would show the same row-0 line
   wherever the ring is not covered by the mountain shell/fog. No REGION_CLAMP for models. [V]/[H]
6. Port rule: REPEAT/REPEAT everywhere for model materials; there is no data field to honour, so
   any clamp for sky shells is a deliberate port deviation, not a disc behaviour. [D]

## §1 Where CLAMP is written

### 1.1 EE-side A+D writers (all 2D / post paths, none on the model path) [V]
Every `*(u64*)(dl+N) = 8` (register CLAMP_1) in BF2, with its data qword (line−1):
- `FUN_003c9370` D:424018 *(decompiler detail omitted)* (D:424002: *(decompiler detail omitted)* → 5 = CLAMP/CLAMP, else 0)
  then D:424044 restores CLAMP_1 = 0 (REPEAT) after the sprite. Textured sprite (PRIM 0x5e).
  BF1 twin `FUN_002ef540` B:278052/278068/278094 — identical.
- `FUN_00470438` D:532425 = 5; `FUN_0057f8f8` D:715076 = 5; `FUN_005823d0` D:716683/716704/
  716726 = 5; `FUN_00585890` D:718520/718590/718683/718784 = 5; `FUN_00586578` D:718842/719103 = 5,
  D:718877/718993 = 0x378023f802a (REGION_CLAMP u 2..254, v 2..254); `FUN_00587278` D:719215/719365
  = 5; `FUN_0058b188` D:721540/721579/721686 = 5; `FUN_0058b9a8` D:721874/722178 = 0x6fc017fc01a,
  D:721936/721990/722028/722063 = 0x37c013f801a (REGION_CLAMP framebuffer copies), D:722248–722387
  = *(decompiler detail omitted)* built at D:722095 from `DAT_0060c600..0c` (WMS=WMT=2 REGION_CLAMP, MIN/MAX globals).
- `FUN_00581018` D:716020 = 0 (REPEAT) and D:716071 = `(n<<14)|0x73` (WMS=3 REGION_REPEAT, MINU=7,
  MAXU=n; WMT=0) — a 64×64-block screen effect.
- No EE write of register 9 (CLAMP_2) anywhere in either decomp (grep `= 9;` on u64 stores: none).
BF1 list (B:) is the same shape: 277–565k, values 5 / 0 / 0x73-form; the sprite restore at B:278094.
None of these are reached from `FUN_0058ed70` (model segment draw, D:723257) — that path writes NO
GS registers on the EE; it emits display-list records (§1.2).

### 1.2 The model path emits records, not registers [V]
`FUN_0058ed70` builds a 16-byte material entry per texture layer from the .bss tables
`DAT_007c57b0..7c5848` (initialised at runtime in the function at D:724925–724990; elfrd of
0x7c57b0/0x79f4a8 reads zeros because both are .bss, ELF `.bss` = 0x661900..0x840aa4):
- entry = { u64 ALPHA (D:723561/723607: *(decompiler detail omitted)*, a local ∈ {0x44,0x48,0x64,0}
  D:723450–723460), u32 TEST (D:723465–723480; `FUN_00498b88` D:556750 formats AREF<<4), u16 PRIM
  bits (0x58 = IIP|TME|ABE) }; `FUN_00498a88` D:556708 is the generic constructor with that layout;
  `FUN_00498b30` D:556737 splices the texture layer's word `*(layer+0x14)` into ALPHA bits 0–6, sets
  FIX = 0x80 and PRIM |= 0x40 (D:723847). Table pick: D:723895/723898 (`f&4` → 7c57c0), D:723833–
  723837 (`f&2` → 7c57e0/7c57f0).
- `FUN_004979a8` D:555841 writes DL opcode 0x406 + mask + up to 4 entries, reordered to VU qword
  {x=ALPHA.lo, y=ALPHA.hi, z=PRIM16, w=TEST} (D:555879–555881); called D:724012.
- `FUN_00497bd8` D:556013 writes DL opcode 0x417 + mask + 2 qwords per texture record:
  q0 = {TEX0, TEX1}, q1 = {`rec+0x18`, `rec+0x10`=MIPTBP1} (D:556053–556056); called D:723964
  (1 texture) / D:724007 (2–4 textures). The record comes from `FUN_005899b8` = `DAT_007c5728`,
  set in `FUN_00585580` D:718365 ← `FUN_00584618` D:717645 ← `FUN_00582c90` D:716966.
- `FUN_00497cf0` D:556068 writes opcode 0x43a (colours). Raw A+D blocks use opcode 0x449
  (e.g. D:423986) and are the only way EE code writes GS registers.
- Texture record layout (`FUN_00582c90` D:717331–717345 / 717565–717616, stored at
  `DAT_0079f490 + i*0x30`): +0 TEX0 (*(decompiler detail omitted)*, TBP0=addr>>8, TBW, PSM 8/4/32 D:717106–717110,
  TW/TH log2 tables), +8 TEX1 (*(decompiler detail omitted)* = MXL=(mips−1)<<2 | MMAG=1 | MMIN=4 or 5 (0x100/0x140,
  trilinear when argument 5) | L<<19 | K<<32, D:717190–717193 / 717516–717519), +0x10 MIPTBP1
  (D:717283–717291 / 717545–717556), **+0x18 = 0** (*(decompiler detail omitted)*, D:717331–717332
  and D:717572–717573; the only store of that field is D:717616; no other writer of `DAT_0079f4a8`
  in the decomp; `FUN_00499388`/`FUN_004993e8`/`FUN_00498dc8` only read the packet).
  BF1 twin `FUN_0049c5c8` B:561661: zero at B:561920/562186, stored B:562227 (`DAT_006cf778`).

### 1.3 The VU1 microcode writes CLAMP_1 from that zero field [V]
`.vutext` (BF2 ELF off 0x4935c0 len 0x61c0; BF1 off 0x3b23c0 len 0x4260) is a VIF stream: slot 0
DMA tag, slot 1 {FLUSH 0x11000000, MPG 0x4a000000}, then an {NOP,MPG} slot every 256 instructions;
local address A is at slot 2 + A + A/256. The DL is sent raw by `FUN_004949c0` D:553654 (UNPACK
V4-32 0x6c000000, then MSCALF 0x17000000); the dispatcher at local 0x000–0x00d does
`MTIR vi1, vf1; JR vi1` — the DL opcode IS the microcode address (0x406 material, 0x417 texture,
0x43a colour, 0x449 raw A+D; BF1 uses 0x415 for textures, B:388693).
Copy routines: 0x406 (BF2 0x406–0x416) stores the entry at slot−9, 0x417 (0x417–0x426) stores the
two texture qwords at slot−8/−7, 0x43a (0x43a–0x448) at base 0xb−9 = slot+3 → material slot
layout [0]=entry, [1]={TEX0,TEX1}, [2]={rec+0x18, MIPTBP1}, [3]=colour (9-qword stride 8+9i;
BF1: 0x415–0x424, stride 8, stores −7/−6, same order).
Register-list builder, BF2 local 0x4c4–0x4e8 (single caller `BAL` at 0x16d, vi2 = mem[6].x packet
write pointer, vi15 = material slot):
```
0x4c5 LQ vf2,0(vi15)      0x4c9 SQ.yzw vf2,0(vi1)   0x4ca SQ.xy vf2,1(vi2)   ; ALPHA data = slot0.xy
0x4cb LQ vf1,1(vi15)      0x4cc LQ vf2,2(vi15)                                ; vf2 = slot[2]
0x4cf SQ.xy vf2,2(vi1)    0x4d0 SQ.xy vf2,3(vi2)                              ; P[3].xy = slot2.xy
0x4d3 ISW.x vi11(7),0(vi2)                          ; GIFtag NLOOP = 7 A+D pairs
0x4d4 ISW.z 0x42,1(vi2)   0x4d7 ISW.z 6,2(vi2)      0x4d8 ISW.z 8,3(vi2)      ; ALPHA_1, TEX0_1, CLAMP_1
0x4db IOR vi3,vi3,vi5     0x4dc ISW.x vi3,4(vi2)    0x4e0 ISW.z 0,4(vi2)      ; PRIM = slot0.z | mem85.w
0x4e1 ISW.z 0x47,5(vi2)   0x4e3 SQ.xy vf2,7(vi2)    0x4e6/0x4e8 ISW.z 0x14,6 ; 0x34,7
```
Register and data of pair 3 use the same base (vi2) and offset (3): CLAMP_1 ← slot[2].xy ←
DL 0x417 q1.xy ← `texrec+0x18` ← 0. Corroboration: after two `MR32` rotations the same vf2 gives
pair 7 (0x34 MIPTBP1_1) = slot[2].zw = `rec+0x10` = MIPTBP1, exactly where the EE put it.
BF1 0x4c8–0x4e2 is the same instruction sequence (dumped, `wrap_scratch/vudis2.py`).
The only other `IADDIU …,8` sites (0x1a0, 0x320, 0x406/0x40f/0x3bd/0x3ca) are flag tests / slot
arithmetic, not register stores [V]. `IADDIU …,9` never feeds an ISW.z (no CLAMP_2). [V]
TEX0/TEX1 (pairs 2/6) are stored through base vi1 from the same vf2; how vi1 relates to vi2 was not
resolved (see §Not closed) — it does not affect pair 3, whose base and data are both via vi2.
**Resolved 2026-09-24 (queue SPEC's RE, lane 1, `[O]`; `analysis/re-specular-pass.md` conventions):** the
"vi1" was a decoder slip. `vudis2.py` prints `SQ` as `vf{it}, (vi{is})`, but the encoding is `SQ.dest fs(15–11),
imm(it 20–16)`. Re-decoded (`scripts/tools/vu1dis.py bf2 0x4c4 0x4e9`), every pair goes through vi2:
- 0x4c9 `SQ.yzw vf1,0(vi2)` (the GIFtag);
- 0x4ca `SQ.xy vf2,1(vi2)` = slot[0].xy → ALPHA_1;
- 0x4cf `SQ.xy vf1,2(vi2)` = slot[1].xy → TEX0_1;
- 0x4d0 slot[2].xy → CLAMP_1;
- 0x4dd TEST_1;
- 0x4e2 slot[1].zw → TEX1_1;
- 0x4e3 slot[2].zw → MIPTBP1_1.

Pair 3's reading stands. The same decoder also shows the MPG tag word, not the instruction, at every
A = 0x100·k (k ≥ 1). For a multi-pass draw, slot[2].y is the per-pass word `FUN_0058ed70` writes at
`a local` (5 for a tex-gen pass), which lands in CLAMP_1's MINV/MAXV bits and is ignored under REPEAT.

## §2 Scope

### 2.1 Not per texture [V]
Raw `tex_` chunks (offsets are chunk-header offsets in the LVL):
| texture | LVL | tex_ @ | INFO bytes (14) | w×h bpp mips | children |
|---|---|---|---|---|---|
| sky_hoth | BF1 HOT1 | 0xEE658 | `0002 8000 0800 0000 000080bf 0100` | 512×128 8 1 | NAME INFO pal_{INFO 00012000, BODY} BODY |
| sky_hoth | BF2 HOT1 | 0xFBE1C | identical | 512×128 8 1 | same |
| pol1_sky | BF2 POL1 | 0x914F4 | `0001 0001 0800 0000 000080bf 0100` | 256×256 8 1 | same |
| tan1_sky | BF2 TAN1 | 0x49058 | `0001 0001 0400 0000 000080bf 0400` | 256×256 4 4 | pal_ 16 colours, 4 BODY |
| nab1_waterfall | BF1 NAB1 | 0x13F3FC | `8000 8000 0800 0000 000080bf 0100` | 128×128 8 1 | same |
| hoth_mountain | BF1 HOT1 | 0x108440 | `0002 8000 0800 …` | 512×128 8 1 | same |
INFO = u16 w, u16 h, u16 bpp, u16 detailCompressed, f32 mipDistance (−1.0 here), u16 mipCount —
exactly what `mungefs/texture.rs:57–64` reads; no spare word, no FMT_/LVL_/FACE (those tags do not
occur in either decomp: `0x5f544d46`, `0x5f4c564c`, `0x45434146` → no hits). The BF2 loader
`FUN_00593898` D:726405 handles only NAME/INFO/pal_/BODY (D:726432–726585): INFO → +0xc w, +8 h,
+0x24 bpp, +0x18 detail-name hash, +0x44 mipDistance, +0x14 mipCount (optional trailing u16). The
two tiling domes (pol1_sky v −1.71..2.71, tan1_sky v −0.50..1.50) and SKY_Hoth carry the same words
except size/mips — nothing distinguishes them, as the global rule requires.

### 2.2 Not per segment/material [V]
The MTRL flag word (segment+0x18, `f`) only selects the ALPHA/TEST table entry and the bucket
(D:723411–723480, 723895/723898, 723833–723837); `model.rs` segm sub-chunks (MTRL/RTYP/TNAM/…)
carry no wrap field and none is read on the draw path. The material entry has no CLAMP-sized
field (ALPHA u64, TEST u32, PRIM u16).

### 2.3 Global [V]
CLAMP_1 = 0 (REPEAT/REPEAT) is re-emitted with every material packet (§1.3), so 2D code that sets
5 / REGION_* cannot leak into model draws, and the sprite routine's explicit restore to 0
(D:424044, B:278094) is the "resting" state. Consistent with dome_uv.txt: every shell that leaves
[0,1] (Hoth u −0.5..1.5, Yavin u −1.17..2.31, POL1 v −1.71..2.71, TAN1 v −0.5..1.5, cor1_skycity
u −3.57..4.59, fel_sky_dome_trees u −5..5, myg1 mountains u −4.5..5.6) tiles on disc.

## §3 The Hoth ring [V]/[H]
SKY_Hoth is 512×128 (both discs), REPEAT/REPEAT, TEX1 MMAG = linear (D:717190 `| 0x20`), one mip.
The bottom ring at v = 1.0010 → texel row 128.13 → bilinear across the wrap: rows 127 (37 %) and
0 (63 %) [D]. The PS2 samples the texture's TOP row there, same as the port's REPEAT sampler; the
disc shows the same row-0 line unless the ring is hidden (Hoth mountains shell spans el −25.9..9.4°
and the ring sits at −2.8°; fog) — visibility on the disc is not established here [H]. No
REGION_CLAMP/REGION_REPEAT bounds exist for any model texture, so "clamp at the mip's extent" does
not happen on the PS2 [V].

## §4 Generalisation [V]/[D]
- The PS2 clamps NO model texture (this covers `modl` segments drawn via `FUN_0058ed70`; the
  terrain/other VU entry points 0x56b/0x78f were not traced — [H] same builder).
- Wrap is not carried in `tex_`, not in `segm`, not in the ELF tables; it is a constant 0 written
  by `FUN_00582c90`/`FUN_0049c5c8` into `texrec+0x18` and forwarded by the microcode. The port
  should use REPEAT on both axes for every model material and treat any sky-shell clamp as a
  port-side choice (mesh/UV fix or an explicit exception), not a data flag [D].
- Side facts from the same trace, usable for the material RE: TEX1 = MMAG linear, MMIN
  LINEAR_MIPMAP_NEAREST (0x100) or _LINEAR (0x140 when `FUN_0058b168()`≠0 / special hashes
  0x74dbc80e/0x618b7c41/0x60345894 at D:717668–717672), K from a piecewise-log LOD of mipDistance
  (or `DAT_0060c254` when mipDistance < 0, D:717101–717103); the 7 registers a material sets are
  ALPHA_1, TEX0_1, CLAMP_1, PRIM, TEST_1, TEX1_1, MIPTBP1_1 (no TEXA/FOGCOL per material).

## §Reproduction
*(decompiler excerpt omitted from the public copy)*
Ghidra was not needed (0 of 3 runs used).

## §Not closed
- In the VU builder the TEX0/TEX1 pairs (2, 6) are stored through base `vi1`, set outside the
  dumped caller; their exact relation to `vi2` (and the `SQ vf4` stores in the copy routines, whose
  I-bit float tricks I did not decode) is unresolved. It does not touch pair 3 (CLAMP_1), whose
  register and data share base `vi2` and offset 3.
- Terrain / other VU entry points (0x56b, 0x78f) and skinned paths were not traced; assumed to use
  the same builder [H].
- Whether the disc visibly shows the Hoth row-0 line (occlusion by the mountain shell, fog) needs a
  capture; the sampling itself is established.
- `DAT_0060c600..0c` (REGION_CLAMP bounds in `FUN_0058b188`) belong to a framebuffer/post path; not
  identified further.
