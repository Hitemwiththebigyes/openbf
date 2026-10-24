# Community projects — what they are, what we can use, what we must not

Assessed 2026-08-30 (Opus session 2), at the maintainer's request, against three projects
he linked plus the ones PLAN already names. This is a **reference document**:
it records the assessment and the rules for using each source. It is not a plan.

The single most important line in this file:

> **All three projects target the PC (2005) build. We target the PS2 builds.
> Every fact taken from them is a HYPOTHESIS to verify against our own PS2
> data, never a fact to port.**

That is not caution for its own sake. Our A2 format audit inherited PC-derived
assumptions and *every one of the following turned out wrong for PS2*, each
costing a visible bug:

| PC-derived claim | PS2 reality | What it cost |
|---|---|---|
| `modl INFO` is 68 B | 72 B on BF2 (PS2 included) | Every TAT2 building dequantized into a shifted box |
| 256-colour CLUTs are CSM1-swizzled | LINEAR | Speckled every 8-bit texture on both discs |
| `HEXP` = u16 per 8×8 patch | 8-B record per 16×16 patch | Terrain heights unreconstructable until refuted |
| Terrain has per-patch `VBUF`/`IBUF` | PS2 has none; `HGT8` + `HEXP` + VU generation | Whole terrain path had to be re-derived |
| `WATR` is PC-only | Ships on BF2 DEA1 | A map failed to bake |

So the verification rule is earned, not theoretical.

---

## 1. BF2GameExt — `S1thK3nny/BF2GameExt` — **MIT — the useful one**

**What it is:** not a port. A DLL-injection + raw-assembly patcher that hooks
the *retail PC executable*. Its ceiling is forever "what Zero Engine can do on
Windows x86" — it cannot be portable and cannot touch BF1. That is irrelevant
to why it matters to us.

**Why it matters:** its `docs/RE/` holds ~30 reverse-engineering write-ups whose
file list reads like our HOLD FOR FABLE list. Stated goal: "a complete Ghidra
decompilation of the game engine."

Priority order for mining, matched to what it unblocks here:

| Their document | What it unblocks for us | Our status |
|---|---|---|
| `AIComparison_BF1_vs_BF2.md` | **Deferred design item #1** — infantry AI base: BF1 vs BF2 vs merge | Parked since P0, blocks AI work |
| `EntityCarrierSystem.md`, `EntityFlyer_vftables.h`, `FlyerAI.md`, `FlyerContrailSystem.md` | **P7 vehicles** — seats, turrets, flyer physics | HOLD #1, our next phase |
| `AnimationResearch.md`, `ProceduralAnimation.md` | Animation blend layers (upper-body vs full-body) | HOLD #2, `game::animator` is PROVISIONAL |
| `FirstPersonAnimationSystem.md` | `humanfp` rig | HOLD #3 |
| `ParticleSystem.md` | The `fx__` chunk | HOLD #4 — why our effects are placeholders |
| `SoldierCollisionSystem.md` | Cross-check the prone-capsule defect the 2026-08-30 audit found (`collision.rs` ray radius vs segment radius) | Open finding |
| `SoundSystem.md` | `shell_menu_accept` source; sample `frequency` outside 4-48 kHz | HOLD #9 + new hold |
| `CharacterWeaponSystem.md`, `WeaponClassFactory.md`, `barrel-fire-origin.md` | Cross-check `core::weapon` | Ported, would benefit from a second source |
| `game_struct_reference.md`, `lua_api_bindings.md` | Vocabulary and struct shapes for locating the PS2 analogues | General accelerant |

**How to use it:** as a *map*, not a source of constants. Their addresses and
vftables are for the PC binary; the value is knowing **what a system is called,
what its fields mean, and what its shape is**, which turns "find the needle in
15,000 functions" into "confirm the needle" in our PS2 decomp. Any number taken
from it must be re-derived from our own corpus before it enters `core`.

**Note the irony:** the features they had to *build* — prone, sprint,
first-person animations, gamepad aim assist, rumble — are things the PS2 build
already has and we have already transcribed from the PS2 decomp. On those
specific systems we are ahead of them, and their notes may even reference PS2
behaviour as the target.

**License:** MIT. Usable with attribution. The only one of the three that is.

**Careful:** there are TWO projects called BF2GameExt. `PrismaticFlower/BF2GameExt`
is a narrower memory-limit patcher. The one with the RE corpus is
`S1thK3nny/BF2GameExt`.

---

## 2. Zenit — `natanalt/zenit` — **NO LICENSE — read only, copy nothing**

**What it is:** the architectural mirror of this project — Rust, wgpu, an engine
for SWBF2 from scratch — but PC-targeted, and it parses `.lvl` at runtime and
runs a real Lua 5.0.2 VM (both of which we deliberately do not do).

**What it offers us: very little.** Its published `.lvl` format spec documents
**four** chunk types (`lvl_`, `tex_`, `scr_`, plus `entc`/`fx__`/`coll` as
undocumented stubs), is PC-only with no mention of platform differences, and the
author states the information "may be either missing, or possibly incorrect."
We are far past it: we have cracked the PS2 texture RLE codec, CLUT layout,
terrain heights, softskin, animation keyframes, sound banks including alias
records, and material flags — none of which it covers.

The author describes it as "an extreme work in progress" that will "stay in this
state for a while."

**License: none attached.** That means all rights reserved, NOT public domain.
Reading it to understand the problem is fine. **Copying any code from it is not.**

---

## 3. SWBF2Phoenix — `Ben1138/SWBF2Phoenix` — **AGPL-3.0 — read only**

**What it is:** the genuinely different idea. A Unity compatibility layer that
loads the original assets *and runs the game's original compiled Lua scripts*
against a reimplemented API. Requires Unity and an installed copy of the game.
Built on LibSWBF2 (which, per A2, has no PS2 support).

**The strategic point it raises.** Phoenix gets broad content working fast
because it *delegates gameplay to the original scripts* instead of
reimplementing them. We made the opposite call — PLAN Architecture #3, "data-
driven missions, no Lua VM in v1" — converting mission Lua to RON instead.

The consequence is worth stating plainly: **the entire existing SWBF2 mod
ecosystem is Lua + ODF. Phoenix can run those mods; we currently cannot.**
**Resolved 2026-08-30 (the maintainer):** [PLAN Architecture #3](../PLAN.md#architecture-locked) records that the Lua mod ecosystem is not a goal and closes the mlua-shim/Phoenix question. A worthwhile specific mod may instead be ported to native Rust as a first-class feature.

**License: AGPL-3.0.** Viral. Copying code from it would force this project to
AGPL. Read for understanding; never copy.

---

## Why we build from the PS2 builds, and what it cost

Recorded here because it is the question a newcomer will ask, and because the
answer is not the obvious one.

**The stated reason was controller support. That reason is right, but not for
the reason it looks like.** A from-scratch port writes its own input layer
either way (we use `gilrs`), so "the PC version is keyboard-and-mouse" would not
by itself have blocked a PC-based port. What actually matters is that the PS2
builds contain **the gameplay systems built around a controller**, as traced
constants we can transcribe:

- BF1 passive aim-assist magnetism (`analysis/re-aim-assist.md`) — the PC build
  has no such system; BF2GameExt had to *invent* gamepad aim assist.
- The contextual roll/stance button and BF1 prone (`analysis/re-stance-roll.md`)
  — PC lacks prone and sprint; BF2GameExt had to add them.
- PS2 turn rates, deadzones, capsule table, spawn waves.

So the PS2 discs are the correct source for *feel*, which is this project's
whole north star. That is a substantive justification, not a superficial one.

**And for BF1 there was never a choice.** No PC copy of BF1 is in hand, and
none of the three community projects touches BF1 at all. Any BF1 content has to
come from the PS2 disc. Using PS2 for both games also means ONE consistent
source and one set of format quirks, rather than merging a PS2 BF1 bake with a
PC BF2 bake across two different engine revisions and two different asset
pipelines.

**What it cost, honestly:**

1. **No community tooling applies.** LibSWBF2, swbf-unmunge, zenit_lvl and
   Phoenix are all PC-only, so we cracked the PS2 formats ourselves.
2. **PC-derived documentation actively misled us** — the errata table at the top
   of this file.
3. **A smaller reference corpus** for cross-checking a finding.

**But that cost is now largely sunk in our favour: the formats are cracked.**
The expensive part of the PS2 choice is behind us (P1–P6 done, all 32 maps bake
and play). Switching bases now would discard that for a build that lacks the
systems we care about most.

**Verdict: PS2 was the right base, and it would still be the right base today.**

---

## Is the PC copy of SWBF2 useful? Yes — as a second oracle, not a base

The maintainer owns SWBF2 on PC (not BF1). Concrete uses, in value order:

1. **It makes BF2GameExt's RE corpus directly usable.** Their write-ups are keyed
   to the PC binary — specifically a Steam retail build (`Build_2006SteamRetail.md`).
   With that executable in hand we can follow their addresses and vftables in a
   real binary, understand the system fully, then locate the PS2 analogue by
   structure, string and constant matching. Without the PC exe their docs are
   prose; with it they are a working map. **This is the biggest single use.**

2. **A second sample of the same engine for our ambiguous questions.** Several
   open holds are "what does this field mean" rather than "what is the value":
   `segm MTRL` bit 0x04, the `fx__` chunk, `LOWR` terrain, the odd sample
   `frequency` values. The PC build stores much of this *uncompressed and
   unswizzled*, so where the PS2 is opaque the PC file can be the Rosetta stone
   for the SEMANTICS — with the layout still verified against PS2.

3. **It de-risks the classic-plus path (HOLD #8).** Harrisonfog's remaster ships
   PC-format LVLs and we already read them (`mungefs::texture_pc`, `bake pc-tex`).
   To name-match HD textures onto our PS2 textures we need the PC↔PS2 name
   correspondence — and the stock PC install is the reference to diff the
   remaster against. This is the most immediately practical use after #1.

4. **The PC mod tools ship Lua SOURCES.** A6 already established that the PC
   mod-tools `.lua` files ARE the mission scripts and that our RON generates from
   them with PS2 winning on deltas. Owning the PC side supports that pipeline
   directly — and mission-Lua extraction is currently HOLD #6.

**What the PC exe is NOT good for:** it is not a substitute base (it lacks the
systems we are being faithful to), it does nothing for BF1, and it must never be
a source of gameplay constants — those come from the PS2 decomp.

**Guardrails are unchanged.** A PC install, its executable and its assets are
treated exactly like the PS2 ISOs: local-only, gitignored, never committed,
never distributed. Reading BF2GameExt's MIT docs is fine with attribution;
Zenit and Phoenix are read-only.

---

## MEASURED 2026-08-30: the PC textures really are better, for about a third of them

The maintainer supplied his PC SWBF2 install (`PC_swbf2_exe/`, gitignored) and asked
whether its textures are worth using over the PS2 ones, "or if we're upscaling
everything anyway then it may not matter."

Measured with the existing `bake pc-tex` reader against our PS2 bake, matching
by lowercase name, over four shared maps:

| map | PC textures | PS2 baked | name-matched | PC LARGER | PC smaller | same |
|---|---|---|---|---|---|---|
| tat2 (Mos Eisley) | 60 | 44 | 38 | **18** | 0 | 20 |
| bf2_hot1 (Hoth) | 51 | 42 | 39 | **6** | 0 | 33 |
| bf2_kas2 (Kashyyyk) | 74 | 55 | 55 | **12** | 1 | 42 |
| bf2_dea1 (Death Star) | 108 | 79 | 79 | **35** | 0 | 44 |

Typical improvement is a clean **2x in each dimension** — `sandcrawler`
256->512, `tat2_bldg_set01..05` 256->512, `sky_tatooine2` 512x128->1024x256.
Nothing is meaningfully smaller (one texture on kas2).

**Two conclusions.**

1. **It still matters even though we plan to upscale.** Real 2x source art beats
   a 2x ML upscale every time — it is the artists' actual detail rather than an
   inference. Where the PC ships a bigger texture that should win over our
   upscaler, and the upscaler becomes the FALLBACK for what PC does not improve
   (and for all of BF1, which has no PC counterpart here).

2. **Take PC only where it is LARGER, not merely where it exists.** PC textures
   are DXT1/DXT3 — lossy block compression — while the PS2 ones are palettized.
   Different artifacts, not strictly ordered: at EQUAL dimensions a PS2
   palettized texture may well be cleaner than a DXT1 one. So "PC exists" is the
   wrong trigger; "PC is bigger" is the right one.

**Name matching works.** 79/79 and 55/55 matched on the two biggest maps by
direct lowercase name, so no mapping table is needed — which also de-risks the
Harrisonfog override, since the remaster uses the same PC naming.

**Refinement to the override chain — PROPOSED 2026-08-30, CONFIRMED by the maintainer
and IMPLEMENTED 2026-08-31** (`bake::tier`):

> **Harrisonfog HD > PC stock (only where larger) > our upscale > PS2 original**

BF1 is unaffected: no PC BF1 here, so BF1 keeps "Harrisonfog (Bespin only) >
our upscale > PS2 original".

## MEASURED 2026-08-31: "larger" is not enough — it must be the SAME ART

Full coverage this time, not four maps: 1,063 baked BF2 textures, 1,040
name-matched, **370 (34.8 %) strictly larger on the PC in both dimensions**.

Downsampling each candidate back to the PS2 size and comparing gives a clean
bimodal answer. Median mean-absolute difference **4.5 of 255**, median luma
cross-correlation **0.988** — three quarters of the set is the same painting at
higher resolution, which is exactly what the tier is for. But the tail is a
DIFFERENT PAINTING under the same name:

| texture | PS2 | PC | luma correlation |
|---|---|---|---|
| `sky_hoth` | hazy white blizzard | blue sky with clouds | −0.12 |
| `hoth_mountain` | bright snowy peaks | darker, recut | −0.07 |
| `mus1_main_2` | — | — | −0.10 |
| `tat2_main_2`, `geo_main_1/2/3` | terrain sheets | different sheets | ≈ 0 |
| `uta1_prop_rock_1_ps2` (named `_ps2`!) | one rock face | another | 0.03 |
| `fel1_sky_clouds`, `sky_endor`, `sky_kam1`, `sky_tatooine2` | — | — | 0.08–0.48 |

Taking those would change the map's CONTENT — a clear blue Hoth is not a
classic-plus Hoth — and content fidelity is non-negotiable (PLAN Architecture
#5). So the shipped rule rejects a candidate only when it agrees on NEITHER
levels NOR structure (mean difference > 12/255 AND correlation < 0.5). Both
tests are needed: correlation alone rejects smooth textures where it is
meaningless, and difference alone rejects the same art merely graded
differently — the PS2 art is systematically brighter than the PC art.

24 of the 370 are rejected that way, and 11 more as detail-compressed bases
(their `_dtl` companion is registered to the PS2 pixels), leaving **312 taken**.

Cost: BF2 per-map texture memory roughly doubles (bf2_cor1 19.2 → 58.2 MB,
tat2 15.6 → 30.7 MB) with **no measurable frame-time change** on the
integrated GPU. Block compression is therefore a residency question, not a
frame-time one, and it becomes live on the Harrisonfog tier (~250 MB/map).

Still open: the Harrisonfog x PC overlap is unmeasured (the canonical set is
three unextracted installers), so the PC tier's NET value once HD lands could
be smaller than 312.

---

## Sources

- <https://github.com/S1thK3nny/BF2GameExt> — MIT; the RE corpus
- <https://github.com/PrismaticFlower/BF2GameExt> — separate, narrower project of the same name
- <https://github.com/natanalt/zenit> — no license
- <https://gist.github.com/natanalt/2ef697e53e56d6abfb42a644f6317d68> — zenit's `.lvl` spec (4 chunk types, PC-only)
- <https://github.com/Ben1138/SWBF2Phoenix> — AGPL-3.0
- <https://exp111.github.io/SWBF2/> — exp111's SWBF2 notes (already in PLAN)
