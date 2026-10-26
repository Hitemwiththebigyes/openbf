# OPUS RE (loop lead, 2026-09-25, lane 1) - [O] UNAUDITED: no Fable pass
# RE: BF2's `Effect("Water")` flags — `SpecularEnable` and `OscillationEnable` both default to 1 (queue WSE)

Corpora: B1 = `analysis/SWBFRONT.ELF-decomp.c`, B2 = `analysis/SWBFRNT2.ELF-decomp.c`. ELFs `extracted/bf{1,2}/*.ELF`, one
PT_LOAD each at va 0x160000 / file 0x1000, so **file offset = va − 0x15f000**; every ELF address below was read as
instruction words (`scripts/tools/eedis.py`). Key hashes are FNV-1a 32 over each byte `| 0x20`:
SpecularEnable 0xd4ddec5f, OscillationEnable 0x12e39af3, DisableLowRes 0xa01d9243, OceanEnable 0xa69c431a,
"Water" 0x49c69a10, "Lightning" 0xe812f405, "shell" 0x11e1fc01, "mission" 0x8ff60339. Tags: [V] read at the line or
word / measured, [D] derived (from what is said), [H] open. Every load-bearing claim is `[O]`. Scratch, tools, the
claims list and the verifier's verdicts: `engine/artifacts/lead_tmp/re/wse/` (`claims.md`, `verify/VERDICTS.md`).

## 0. The answers
1. **BF2's flat water draws the speckle layer AND the far `fogspread` sub-pass on every shipped map.** Both are gated
   on `DAT_006615f9` (= `SpecularEnable`), which the Water effect's level reset sets to **1** before the map's config
   is read. The parser overwrites it only when the key is present, and **no disc file carries the key**.
   `re-water.md` §1/§5 read the gate right and its default backwards: an unauthored key means ON. [V] [O]
2. **What shows on the shipped maps:**
   - **The near speckle shows** on all six BF2 flat-water maps: KAS2, YAV1, NAB2, DEA1, FEL1, DAG1. DAG1's authored
     speckle colours are black. [V] [O]
   - **The far speckle also shows on KAS2**, the only one of those six with a far flat pass (§5). [D] [O]
   - **The far fog adds nothing on any shipped map in single-screen play.** KAS2's fog is off, NAB2, DEA1 and FEL1 have
     no far layer, and YAV1 and DAG1 author `DisableLowRes`. [D] [O]
3. **BF1 has no such gate.** Its flat renderer draws both sub-passes unconditionally, and the key exists nowhere in the
   BF1 ELF. [V] [O]
4. **The same default-1 pattern holds for `OscillationEnable`.** That flag gates the 6 cm water bob, and **BF2 FEL1
   authors `OscillationEnable(0)`**, so FEL1's water does not bob on the PS2. The port bobs it. [V] [O]

## 1. The gate and what it covers [V] [O]
- BF2 `FUN_00575fd8` (B2 header 708586) is the flat-water renderer: argument 1 = 1 near, 0 far. Its only caller is the
  flat branch of the driver `FUN_005755b0` (`jal 0x575fd8` at 0x575608); the ocean renderer `FUN_00575670` is the
  driver's other branch.
- **Far block.** B2:708830 *(decompiler detail omitted)*, then 708831 `if (DAT_006615f9 != '\0') {` gates the far set-up:
  - the speckle frame `DAT_0083ac20[(t·DAT_006616a8) % DAT_006616bc]`, bound with the texture "fogspread" (708833
    *(decompiler detail omitted)*, 708834 `FUN_00585380`);
  - the fog colour (708857 `FUN_005804d0`) and the fog matrix (708889 `FUN_00589c60`);
  - a second record {0x144, TEST 0x51011, flags 0x50} (708849–708855) beside record 0x76c190 (708893).
- **Near block.** B2:708897 `else if (DAT_006615f9 != '\0') {` gates the near cross-faded speckle (708901
  `FUN_00585348`, two frames).
- **ELF.** 0x5766d4 `beq s4,zero,0x576848` (s4 = a0 from 0x576020); its delay slot `lui v0,0x66` feeds both tests:
  - near: `lbu v0,0x15f9(v0)` at 0x5766dc, then `beq v0,zero,0x576bb0` at 0x5766e0;
  - far: the same pair at 0x576848 / 0x57684c;
  - far block: `lui a1,0x65; addiu a1,a1,-672` (0x57686c/0x576874) forms 0x64fd60, which holds "fogspread".
- **What the gate skips.** With the byte clear, both tests branch to 0x576bb0, the head of the final tile loop:
  `lw v0,0x164c(s3)` = the tile count, then a stride-0x30 loop testing `+0x29`. So the gate skips the speckle/fog
  SET-UP, and the tile loop runs either way. What that loop draws then, with the main layer's state, is [H]; no
  shipped map takes that path.

## 2. Every writer of the gate byte 0x6615f9 [V] [O]
**Two stores, two loads, nothing else.**
- **Store 1, the reset.** `sb v1,0x15f9(v0)` at 0x56027c in `FUN_005601a8`: v0 = `lui 0x66` (0x560264),
  v1 = `addiu v1,zero,1` (0x560220), so it stores **1** (B2:695896).
- **Store 2, the parser.** `sb v0,0x15f9(s0)` at 0x561124 in `FUN_00560f40`, the `fx__` config parser (B2 header
  696420). It is reached only by `beq v1,v0,0x561110` (0x561084), after `lui v0,0xd4dd; ori v0,v0,0xec5f`
  (0x56107c/0x561080). The value is `(int)record_float != 0` (B2:696565–696567).
- **Loads.** 0x5766dc and 0x576848 (§1).

**The searches behind "nothing else":**
- (a) Every load/store of every width, including unaligned, 128-bit and FPU/VU forms, whose imm16 covers the byte
  from base 0x660000: 4 hits.
- (b) Every lui-formed pointer into [0x661400, 0x6615f9]: 39, each read. They are 8-byte static objects
  0x6614c8–0x661548, two 2-word arrays at 0x661480/0x661488, and a 6-byte bitmap at 0x661518. The highest,
  `s3 = 0x6615d0` in `FUN_005609f0`, is used only by `sw s1,0(s3)` (0x560df4).
- (c) gp = 0x6692f0 (`.reginfo`; crt0 `daddu gp,a0,zero` at 0x3c1df0) could reach the byte, but BF2 `.text` has
  **0** gp-based loads or stores.
- (d) No aligned word in the image holds a pointer into [0x6615c0, 0x6615ff] (the verifier: 334 words point into
  `.sbss`, none there).
- (e) The decomp names `DAT_006615f9` on exactly four lines, 695896, 696567, 708831 and 708897. It has no
  `&DAT_006615…` and no memset into [0x661200, 0x6615f9] except `FUN_0051d8e8(0x661328,0,8)`.
- Limit: a store through a pointer loaded from memory is outside these searches.

**The section.** The byte is `.sbss` (NOBITS 0x661300–0x6618a8), zeroed by crt0 (0x3c1d4c–0x3c1dc4). Its 1 comes
from the reset. The parser could write 1 or 0, but no disc file carries the key (§4).

## 3. When the reset runs — once per mission load, before the map's config is read [V] [O]
**The call chain, each link its only caller** (whole-image jal/j/data-word scans):
- `FUN_005601a8` ← `jal` at 0x461700 in `FUN_004616e8` (B2:523915).
- `FUN_004616e8` ← one data word only, 0x63a6dc: slot +0x24 of vtable 0x63a6b8, the Water effect's.
- The ctor `FUN_004615a8` (B2:523838–523857) installs that vtable, allocates 0x348 bytes and registers the object
  under 0x49c69a10 = "water" (`FUN_004514d0`, B2:523852, into table 0x71fb80).
- `FUN_00450cf8` (B2:512846) constructs the effects (`FUN_004615a8()` at 512863), then calls slot +0x24 of every
  entry in 0x71fb80 (512903; ELF 0x450e98–0x450ea4 `lw a0,8(v1); lw v0,24(a0); lw v0,36(v0); jalr`).
- `FUN_00450cf8` ← `jal` at 0x1750a8 in `FUN_00174b28` (B2:16910), unconditional: no branch before it lands past it.
- `FUN_00174b28` ← 0x175900 in `FUN_001758f8` (argument 0) and 0x175b1c in an undecompiled function at 0x175af8
  (argument 1, `addiu a0,zero,1` at 0x175b04).

**The two states.** Those two functions are the +0xc (enter) slots of state vtables 0x60fe70 (word 0x60fe7c) and
0x60fdf0 (word 0x60fdfc).
- The state objects: 0x66aed8 gets 0x60fe70 (`FUN_00175ef0`), 0x66aee0 gets 0x60fdf0 (`FUN_00175f58`; B2:17500–17501).
- They are queued by `FUN_001749d8` (B2:16660) under **"shell" (0x11e1fc01)** and **"mission" (0x8ff60339)**.
- They are entered by `FUN_00174960` (B2:16642: the old state's +0x14, then the new state's +0xc).
- The "mission" enter runs `jal 0x174b28` (0x175b1c), then `jal 0x4241b0` (0x175b24, ScriptInit: `FUN_001a5418(0x636618)`,
  B2:481913), then `jal 0x424e10` (0x175b2c, ScriptPostLoad, B2:482293).

**Where the map's config is read.** The map's world LVL, which holds its `fx__` `Effect("Water")`, is read in
ScriptInit. The PC referee `community_mods/gametoast_lua/doc/StarWarsBattlefrontII/SourceCode/Common/scripts/KAS/kas2c_con.lua`
has `function ScriptInit()` at line 165 and `ReadDataFile("KAS\\kas2.lvl", "kas2_con")` at line 281. The verifier's own
Lua 5.0 parse of `extracted/bf2/lua/mission/kas2c_con.luac` finds both constants in ScriptInit's prototype. [V]

**Earlier loads hold no water.** `FUN_00174b28` loads MISSION.lvl and runs ScriptPreInit (B2:16781/16799) BEFORE the
reset, which is harmless:
- MISSION.lvl, CORE.lvl and COMMON.lvl carry no `fx__` and no WATR;
- the 12 of 128 PS2 missions that define ScriptPreInit (all space maps) hold only `SetWorldExtents`. [V, verifier]

**Nothing re-runs the reset after a parse.** The only other call through an effect's +0x24, `FUN_00460520`
(B2:523114), is slot +0x28 of another class's vtable (0x63a5b8) calling its own +0x24 (0x4603f8). [V]

## 4. No disc authors `SpecularEnable`; BF1 has no gate [V] [O]
**The disc census** (`lead_tmp/re/wse/hashcensus.py`, a raw byte scan). The little-endian bytes of 0xd4ddec5f occur in:
- 0 of the 186 BF2 `.lvl` files (1493.7 MB);
- 0 of the 137 BF1 `.lvl` files (823.7 MB). The one non-ucfb BF1 `.lvl` (a zip, TEST/TESTH) was decompressed by the
  verifier: 0 there too.

The positive controls in the same scan hit: OceanEnable in 8 BF2 files, DisableLowRes in 3 (DAG1, TAT3, YAV1),
OscillationEnable in 2 (FEL1, TAT3), SpeckleTextures in 8. `fx__` DATA records hold the hashes raw, so a raw scan is
valid. BF1 RHN1 and BF2 TAT3 carry an `Effect("Water")` but no WATR chunk, so they draw no water.

**BF1 has no gate.**
- 0xd4ddec5f is absent from B1 in hex, signed, decimal and sign-extended forms, and from the BF1 ELF (no
  `lui 0xd4dd`/`0xd4de`, no raw word). BF1 uses the same hash scheme: `lui 0xa69c` and `lui 0xa01d` are there.
- BF1's flat renderer `FUN_0048b9c0` (B1 header 551182) splits far/near on argument 1 alone: `beq s5,zero,0x48c0fc` at
  0x48bf98, right after the speckle texture-matrix `jal 0x39da50` (0x48bf90; B1:551403 `FUN_0039da50(...)`, then
  551404 *(decompiler detail omitted)*).
- Its far block at 0x48c0fc opens with the frame loads and `lui a1,0x56; addiu a1,a1,-29152` = 0x558e20 "fogspread".
  No flag byte is tested on either side.

## 5. Where BF2's far flat pass exists at all [V] [O]
- **The far pass needs `DAT_0060bee9`.** The driver `FUN_005755b0` (B2:708249–708271) issues the far call only when
  it is set: *(decompiler detail omitted)*, then *(decompiler detail omitted)*.
- **Its writers.** `.data` initial 1; the reset stores 1 (0x560298, B2:695903); the DisableLowRes arm stores 0
  (0x561258, B2:696560). Its one reader is 0x5755f4.
- **Per map** (`re-fog.md` §6 plus the bakes' manifests; single-screen scopes only):

| BF2 map | far flat pass | fog | so the far fog shows |
|---|---|---|---|
| KAS2 | yes, to 0.9 × FarSceneRange 5000 (0x60c1fc = 0.9) | off (FogRange 0..0) | nothing — the rows are zero, `re-fog.md` §2.1 |
| NAB2, DEA1 | no far layer (FarSceneRange 0) | off | nothing |
| FEL1 | no far layer | on | nothing |
| YAV1 | `DisableLowRes` | on | nothing |
| DAG1, TAT3 | `DisableLowRes` | DAG1 off; TAT3 on, black | nothing |
| KAM1 | the ocean renderer (`OceanEnable` 1) | on | not this path |

- **The fog colour** the pass takes is `DAT_007bb7f0` (`FUN_005804d0`, B2:715482). Its only writer is `FUN_005804b0`
  (0x5804c0/0x5804c4, `swl`/`swr`), called from the sky manager (`FUN_00565170`, B2:698833) and from the **Lightning**
  effect `FUN_0045ae80` (B2:520170/520313; registered 0xe812f405; only BF2 KAM1 has one).
- **[H] Rows from a previous map.** The fog rows 0x7bb800–0x7bb83f have no writer or clearer besides `FUN_00580500`,
  which writes them only when near < far. So rows left by an earlier fog-on map are not ruled out, and `re-fog.md`
  §2.1's "fog off → rows zero" is guaranteed only for the first map after boot. The port has no session order and
  keeps the fresh-boot reading. KAS2's split-screen `FogRange(5,140)` is unchecked.

## 6. BF2's speckle layer is BF1's [V] [O]
- **The same GS record.** BF2 0x76c190 = {0x4148, 0x51001, 0x58}: `addiu v1,zero,0x4148` (0x57a444) → `sd v1,-15984(t2)`
  (0x57a458, t2 = `lui 0x77`); `lui a2,5; ori a2,a2,0x1001` → `sw a2,8(t6)` (0x57a490). That is `FUN_0057a3a8`,
  B2:710719/710725/710740. BF1's speckle record 0x69c3b0 has the same words (`FUN_0048fca0`, B1:553341/553350/553364),
  used at the same two call positions (B2 708893/708926, B1 551448/551479).
- **The same TCC clear.** BF2's near-speckle helper `FUN_00577088` clears TEX0.TCC as BF1's `FUN_0048c848` does
  (B2:709258 / B1:551846 `& 0xfffffffbffffffff`; ELF 0x577620–0x577654 / 0x48ce5c–0x48ce90). So the sheet's alpha
  never reaches the near blend in either game (`re-water-alpha.md` §7.1).
- **The same switch words.** BF1 0x522a5c–0x522a80 and BF2 0x60c1dc–0x60c200 both read
  0, 1, 1, 1, 1, 1, 5, 0, 0x3f666666 (0.9), 0. None has a writer.
- **The same constant block.** BF2's builder `FUN_00576d28` (B2 header 708971) has BF1 `FUN_0048c4f0`'s shape: the
  same 23-qword upload at 0x90, three colour-byte groups /255 (BF2 0x76c138/140/148, BF1 0x69c358/360/368), and a
  light vector (BF2 `*(DAT_0060c6e0+0x50..0x58)`, BF1 0x69c590–598). So W3's flat `SpeckleSpecularColor` is the same
  approximation on both games, and stays PROVISIONAL (the VU1 program is unread).

## 7. `OscillationEnable` — the same pattern, and FEL1 authors it off [V] [O]
- **The flag.** The parser compare `lui v0,0x12e3; ori v0,v0,0x9af3` (0x560f98/0x560f9c) branches `beq` to 0x5610f8,
  which stores `DAT_006615f8 = (int)v != 0` (`sb` at 0x56110c, B2:696485).
- **Default 1.** The reset stores 1 (`sb v1,0x15f8(s0)` at 0x5602a4, B2:695905); the byte is `.sbss`, zero at boot.
- **One reader: the bob.** The only reader is 0x560440 in `FUN_005603e0` (B2:695975): a clear byte sets the bob
  `DAT_0060bef0 = 0`, otherwise B2:695989–695990 gives `0.06·sin(1.8849558·t)`, the 6 cm bob of `re-water.md` §2.
- **Exhaustive.** A widescan of 0x6615f8 finds those three plus one data word at 0x641d18 (`.rodata`); gp has none.
- **Who authors it.** BF2 FEL1 authors `OscillationEnable(0)` (the `fxwater.py` dump of `FEL/FEL1.LVL`), and so does
  TAT3, which has no WATR.
- **BF1 has no such key**: 0 in B1, no `lui 0x12e3`, 0 disc hits. BF1's bob twin `FUN_00475658` (B1 header 538314)
  zeroes the bob when `DAT_005647b8` is 1 (the ocean, with `FUN_00477928`) or 2; flat water always bobs.

## 8. Corrections this makes to the existing docs
- `re-water.md` §1's last bullet and §5's BF2 rows ("no BF2 water map authors it → BF2's far water is NEVER fogged and
  has no speckle") read the gate right and its default backwards: see §0 here. The same holds for §6's draft RE_LOG
  paragraph and §7's W3 line "BF2 maps skip it".
- `re-water.md` citation 8 names B2:696348–696418 (LAYR/CPAT/INFO) as "parser `FUN_00560f40`". Those lines are in
  **`FUN_005609f0`** (B2 header 696265), the WATR chunk parser: its float compares are the tags LAYR, CPAT and INFO.
  `FUN_00560f40` (B2 header 696420) is the `fx__` config parser, and its "config 696472–696600" ends at 696590
  (`FUN_005613c8` starts at 696591).
- `re-fog.md` §2.1: "with fog off the rows are zero" holds for the first map after boot (§5 [H] here). §2.2's
  flat-renderer row has "no fog-flag test". True of the fog-on flag `DAT_007c5764`, but the row sits under
  `SpecularEnable` (§1 here).

## 9. What the port owes (queue WSE's build)
**Before.** `bake::water_glb::manifest_config` bakes `specular_enable: authored.is_some_and(v != 0)`, which is false on
every BF2 map. The viewer's `speckle_look` and `water_look` therefore draw no BF2 speckle and give BF2 far water no fog.
`oscillation_enable` is parsed by `mungefs::fx` and dropped.

**The faithful form:**
1. Bake each flag's value as the PS2 reads it: `authored.map_or(true, |v| v != 0)` for both, i.e. the reset's 1 unless
   authored 0.
2. `speckle_look` keeps `bf1 || specular_enable` (BF1 has no gate).
3. `water_look` gives far water the fog ramp when `(bf1 || specular_enable) && !disable_low_res`. That change is
   invisible on every shipped map (§5).
4. `WaterMotion` lifts flat water by the bob only when `oscillation_enable`, so FEL1 stills. The goldens render at
   t = 0, where the bob is 0.

**Expected golden moves:** the speckle on the BF2 flat-water goldens with water in view and a non-black
`SpeckleSpecularColor` (KAS2 80/150, NAB2 and DEA1 200/100, FEL1 200/150, YAV1 160/255). DAG1's is black. The recipe
and gates are in QUEUE.md's WSE entry.

## 10. Verification
- **Self-checks** (`OPUS_LOOP_PROMPT.md` §5):
  1. Every cited line's enclosing function was printed (`enclosing.py`, `funcmap.py`). One old citation was found in
     the wrong function (§8, cit. 8).
  2. Every absence claim is backed by a search, not a reading: the byte-level access scans, the gp route, the data
     words, the decomp names, and the disc census with positive controls.
  3. Each twin was opened in its own function: BF1 `FUN_0048b9c0`, `FUN_0048c848`, `FUN_0048c4f0`, `FUN_0048fca0` and
     `FUN_00475658` against their BF2 counterparts. The ocean renderers were kept apart.
  4. Every constant was read from its instruction words: the store of 1, the hash compares, the records, the TCC
     mask and the switch words.
  5. For each flag, its default and every writer: the gate, `DAT_0060bee9` and `DAT_006615f8`, each with its section,
     its reset and its parser arm.
- **The verifier.** One `opus` agent, run in the FOREGROUND with the claims and citations only
  (`lead_tmp/re/wse/verify/VERDICTS.md`): **11 SUPPORTED, 6 PARTLY SUPPORTED, 0 NOT SUPPORTED**. All six are folded in
  above:
  - the pointer count 38 → 39, and the gp route added;
  - the section is `.sbss`;
  - the state entry is `FUN_00174960`, not `FUN_00175a88`;
  - two BF1 line numbers were off by one;
  - the Lightning writer, TAT3 and KAM1 were added to §5, and the [H]'s premise corrected;
  - BF1's bob is zero in modes 1 and 2.
  The lead re-read the load-bearing new items itself: `.reginfo` gp and the 0 gp-based accesses; B1:551403/551404;
  `FUN_00174960`/`FUN_001749d8` and the two ctors' vtables; B1:538314–538330.
- **What stays [H]:**
  - what the final tile loop draws when the gate is clear (§1);
  - fog rows surviving across maps (§5), and KAS2's split-screen fog;
  - the speckle's per-vertex colour (the VU1 program).
