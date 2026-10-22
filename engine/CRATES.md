# engine/ — the crate and module map

Read this before grepping code. One row per source file: `path` lines — what it owns · key public items. Paths
are relative to `engine/crates/`; counts are `wc -l` on `662ba97` (2026-09-24, SKO's build — SPL6's map commit and
the 2026-09-24 cleanse refreshed every row `--check-map` listed as stale, and each landing since the rows it moved).
`scripts/size_report.sh` prints the live counts; its `--check-map` exits 1 when a source file is missing here or a
path here is gone. A split (queue SPL1–SPL7, `engine/artifacts/briefs/SPLIT_RECIPE.md`) updates its rows. Each
module's `//!` header is its spec.

## The crates

| crate | owns | depends on (workspace; external) | src lines |
|---|---|---|--:|
| `mungefs` | the PS2 LVL (`ucfb`) and ISO9660 readers; the runtime never links it | flate2, thiserror | 15,247 |
| `bake` | the offline tool: LVL → GLB / KTX2 / OGG / RON in `engine/assets_baked/` | mungefs, manifest; glam, png, serde(_json), ron, anyhow | 19,796 |
| `manifest` | the bake↔runtime schema, data only | serde | 3,747 |
| `pak` | the game's read-only data: a directory, or the archive appended to the packed game (card PACK) | — | 734 |
| `core` | the deterministic sim: soldiers, weapons, AI, posts, collision | glam, serde, parry3d, libm | 75,065 |
| `render` | the wgpu renderer, windowed or headless | wgpu, winit, glam, bytemuck, pollster, image, thiserror | 9,558 |
| `ui` | BF2's HUD (interpreter + layout), egui menus, shell, spawn and loading screens | manifest; egui(-wgpu, -winit), wgpu, winit, glam | 6,863 |
| `input` | gilrs pad → `PadState` | core; gilrs | 646 |
| `audio` | kira playback behind the `backend` feature; the streamed music decoder | pak; kira (optional), anyhow | 866 |
| `net` | empty home for the locked LAN feature | — | 10 |
| `game` | the binary + library: assets, viewer, play loop, goldens | core, manifest, pak, input, render, ui, audio; glam, winit, gilrs, ron, serde(_json), gltf, ktx2, wgpu, anyhow; dev: bake, mungefs, image | 50,516 |

Flow: disc → `mungefs` → `bake` → `engine/assets_baked/` (schema `manifest`) → `game` → `core`, `render`, `ui`,
`audio`. 117,987 src lines in 216 `.rs` files; tests and examples add 22,748 lines in 77 files.

## Standing invariants — breaking one is not a refactor
- **`core` is deterministic:** no wall clock, no OS randomness (`SimRng` + `GameLcg` are the only entropy), no
  host libm (`core::fmath` on the `libm` crate; its `BANNED` referee test, which also scans `game/src`'s sim-input
  files since DET1), parry with `enhanced-determinism`.
  `--sim-hash 100` is pinned by a test in `core/src/sim/state.rs` — a failure is a lead decision, never a value to
  update.
- **The runtime never parses LVL:** only `bake` depends on `mungefs` (`game` as a dev-dependency only).
- **`manifest` is one definition for both sides**, round-tripped by `manifest/src/tests.rs`; a new field means
  rebaking every group that writes a manifest (`land.py` turns a declared `rebake=maps` into `maps tat1 tat2`).
- **The bake is byte-deterministic:** any `scripts/cmp_bake.sh` DIFF is your change.
- **The goldens render the PS2 base texture tier** (the tests force `Ps2`).
- **PS2 behaviour cites `analysis/`;** an unproven constant says `PROVISIONAL`. GPU referees run only via
  `scripts/gpu_tests.sh`; gate values live only in `engine/artifacts/loop/gates.toml` (`scripts/gates.py` runs and compares them).

## mungefs — disc formats (bake-only; each header holds the byte layout)
- `mungefs/src/lib.rs` 314 — the `ucfb` chunk walker, FNV-1a tags · `Chunk`, `open`, `fnv1a_lowercase`
- `mungefs/src/iso.rs` 213 — ISO9660 reader for the disc images (its user is `bake extract-iso`, card PACK: the disc's DATA/_LVL_PS2 tree minus its FMV, byte-identical to the dev tree's files) · `Iso`
- `mungefs/src/rle.rs` 147 — the PS2 RLE codec (`tex_`, `pal_`, `HGT8`, the terrain's `ALPH` walk) · `decode`, `decode_prefix`
- `mungefs/src/config.rs` 252 — munged config trees (`path`, `lght`, `sky_`, `fx__` incl. particles, `hud_`, `ffx_`, `sanm`, `snd_` / `mus_`, foliage) · `parse_tree`
- `mungefs/src/texture.rs` 325 — PS2 `tex_` + palette → RGBA8 · `Texture`
- `mungefs/src/texture_pc.rs` 1076 — PC `tex_` (D3D9 surfaces) → RGBA8, the tier-2 source · `find_textures`
- `mungefs/src/model.rs` 855 — `modl` → segments + material flags and the MTRL's specular fields · `Model`, `Segment`
- `mungefs/src/skel.rs` 387 — `skel` bind poses · `Skeleton`
- `mungefs/src/anim.rs` 662 — `zaa_`/`zaf_` animation banks and rigs · `AnimBank`, `find_banks`
- `mungefs/src/sanm.rs` 197 — BF2's `sanm` chunk (INGAME.LVL): the soldier animator's per-animation loop flag, blend in / out times, aim type, sync group and `BlendTimeTo` (card ANIMS, re-soldier-anim-layers §6.4) · `find_sanm`, `SoldierAnimations::get`, `stored_hundredths`
- `mungefs/src/entity.rs` 267 — `entc` classes, `gmod` LOD tables (model + triangle count) and BF2's INFO cloth list · `EntityClass`, `GeomModel`
- `mungefs/src/cloth.rs` 360 — BF2 `CLTH` cloth meshes (card CLOTH, re-cloth 1 [O]): texture, rest positions + UVs, pinned vertices 0..F-1 and their bones, triangles, the three distance lists, the COLL shapes; the first chunk per name (`mungefs/tests/suite/cloth_disc.rs`: all 58 disc chunks) · `Cloth`, `ClothShape`, `find_cloths`
- `mungefs/src/foley.rs` 273 — a SOUND LVL's `ffx_` foley tables per `lvl_` sub-level: `FoleyFXSoldier` records and the `FoleyFXGroup`s that list them (card FOLY, re-foley §1 `[O]`) · `find_all`, `FoleyLevel`, `FoleyGroup`, `FoleySoldier`
- `mungefs/src/world.rs` 280 — `wrld` instances, regions, hints, barriers · `World`
- `mungefs/src/terrain.rs` 1337 — `tern` heightfield, layers and their `ALPH` alphas, cutters, `LOWR` · `Terrain`, `LowResSheet`
- `mungefs/src/water.rs` 418 — `tern → WATR` water planes · `Water`
- `mungefs/src/foliage.rs` 493 — `tern → FOLG` (a 4-bit layer mask per cell) and the `prp_` foliage file: the four
  layers' meshes and the path lines (`analysis/re-foliage.md`) · `FolgMask`, `FoliageFile`, `FoliageMesh`, `PathLine`
- `mungefs/src/hud.rs` 315 — BF2 INGAME.LVL's `hud_` layouts (a `config` tree + the recovered record names, card BHUD) · `parse`, `HudLayout`, `name`, `NAMES`
- `mungefs/src/font.rs` 161 — BF2 CORE.LVL `font` chunks: glyph sheets + 24-byte glyph records · `parse`, `Font`, `Glyph`
- `mungefs/src/fx.rs` 1219 — `fx__` `Effect("Water")` (texture, tiling, scroll), BF2's `Effect("HDR")` (card GLOW), `Effect("Lightning")` + `LightningBolt` (card KSKY: the dome darkening) and `Effect("Precipitation")` / `Effect("Wind")` (card WTHR) · `WaterConfig`, `HdrConfig`, `LightningConfig`, `LightningBoltConfig`, `PrecipitationConfig`, `WindConfig`
- `mungefs/src/particle.rs` 1038 — `fx__` particle effects: the `ParticleEmitter` trees, every key of re-fx §2, the ctors' defaults, §3's continuous rule, the EE FPU's rounding (card FXP1, `[O]`) · `Effect`, `Emitter`, `Range`, `ps2_mul`, `ps2_sub`
- `mungefs/src/sky.rs` 421 — `sky_` dome (its `Reflect` shells and `ReflectionColor`), sun, ambient, fog, far range · `Sky`
- `mungefs/src/light.rs` 308 — `lght` lights (BF2 only) · `Lights`
- `mungefs/src/locl.rs` 164 — CORE.LVL's `Locl` text tables, one per language: `[fnv(key)][size][UTF-16]` entries (card EVMSG) · `parse`, `find`, `Locl`, `key`
- `mungefs/src/coll.rs` 908 — `coll` meshes, `prim` primitives; a mesh's triangles (a fan per leaf, a signed-fan group repaired) · `CollisionMesh`, `Primitives`
- `mungefs/src/coll_fan.rs` 491 — the signed-fan repair (card STARC): coplanar leaves round a non-convex leaf whose fans overlap are a face with a hole or a notch (BF2 POL1's radar-room doorway); the region their directed edges leave, holes bridged, ear-clipped · `repairs`
- `mungefs/src/path.rs` 304 — `PATH` spawn and AI paths · `Path`
- `mungefs/src/plan.rs` 837 — `plan` graph + BF2 `plnp` patches · `Plan`, `Plnp`
- `mungefs/src/soundbank.rs` 305 — `SampleBank` samples · `SampleBank`
- `mungefs/src/sound_props.rs` 628 — `snd_` sound properties; the `soundparameterized` record's layers and graphs and a `ParameterGraph`'s points (card VSND); a `mus_` chunk's `Music` records (card AMBM); the mixer `bus` and a take's lock (`take_lock`, cards MIXH, UCALL) · `SoundProp`, `find_named_sublevels`, `MusicRecord`, `find_music`
- `mungefs/src/stream.rs` 292 — music/VO streams · `Stream`

## bake — the `bake` binary (groups: `scripts/rebake.sh`)
- `bake/src/main.rs` 399 — the CLI, one subcommand per bake (`map`, `common`, `soldier`, `vehicle`, `effects`, `sounds`, …)
- `bake/src/lib.rs` 66 — module roots · `write_ron`
- `bake/src/map.rs` 2192 — a map LVL → models, textures, terrain, water, collision, nav, `manifest.ron`, its own
  `classes.ron`, its `effects.ron` (plus the TUR sublevels its worlds place, `side_classes`); the common
  bake · `bake_map`, `bake_common`
- `bake/src/side_classes.rs` 66 — the BF2 SIDE TUR.LVL sublevels a map's worlds place without carrying the
  class (card VTUR2): models, collision, skel nodes, weapons, joined to the map bake's walk ·
  `side_classes_lvl`, `pull_side_classes`
- `bake/src/shell.rs` 328 — the BF2 PS2 shell's assets (card BSHLA, re-bf2-shell §1, §4): SHELL.LVL's `logo`, the movie's menu loops (the MVS index, the `mcfg` properties `shell_main` / `shell_sub_left`, each segment transcoded by ffmpeg to a JPEG frame pack; read out of the ISO when the extracted tree has no MOVIES/) and SOUND/SHELL.LVL's bank + music index, into the BF2 common bake's `shell` directory · `bake_shell`, `mvs_segments`, `movie_segments`, `read_movie`
- `bake/src/alpha.rs` 1350 — ALPHA RULE v2, the per-draw alpha verdict (PROVISIONAL); a word the disc alpha-tests is a cutout whatever its `mod` bits (queue item 9); BF1 draws take the ELF's segment rule where the census disagrees (card ELFR: Normal without 0x02/0x04 OPAQUE, Scroll with 0x04 BLEND; `OPENBF_BF1_ALPHA_CENSUS=1` turns it off) · `verdict`, `census_verdict`, `bf1_elf_verdict`, `Bf1Class`, `disc_alpha_tests`, `keep_alpha`
- `bake/src/spec.rs` 194 — the specular second pass: which segments draw it, their power and flat colour (`extras.spec_pass`), the sheets whose alpha is its gloss weight (every bake keeps it, queue DSHN), and the disc's `specular` table (`bake specular`, queue SPEC, `[O]`); a CORE.LVL texture's level 0 — the table and BF2's `com_sfx_omniproj` (`bake omniproj`, card FXBL) · `spec_pass`, `SpecPass`, `pass_textures`, `bake_table`, `bake_core_texture`
- `bake/src/tier.rs` 544 — texture tiers: PS2, or PC stock where larger · `Tier`, `tier2`
- `bake/src/detail.rs` 129 — `<name>_dtl` detail maps · `write_detail_for`, `ALPHA_TEST`
- `bake/src/glb.rs` 711 — model GLB writer, `alphaMode` per material; on a map bake the disc's COL0 as `COLOR_0` and
  each segment's VU1 lighting path as `extras.light` (queue LITEB; BF2's self-lit `glow`/`unscaled` on an HDR map, card GLOW) · `model_to_glb`, `world_model_to_glb`, `LightPath`, `light_path`, `glow_path`
- `bake/src/far_lowd.rs` 146 — the far scene's LOWD extension: a geometry's LOWD triangles its LOD0 lacks, as `<lowd>_far` (card UTAS; BF2 UTA1's cliffs only) · `extension`
- `bake/src/gltf_bin.rs` 103 — the shared GLB BIN builder · `Bin`
- `bake/src/skin_glb.rs` 276 — skinned GLB: skeleton, skin, clips (or a skeleton and clips alone) · `skinned_to_glb`
- `bake/src/terrain_glb.rs` 882 — heightfield → the base terrain GLB (LTEX[0]) and the layers GLB (every layer above it, `ALPH` alpha per vertex, card TALPH), COLR as the GS vertex byte per game, the PS2's per-layer UVs and quad split · `terrain_to_glb`, `terrain_layers_to_glb`, `gs_vertex_byte`, `layer_uv`
- `bake/src/water_glb.rs` 470 — `WATR` → water GLB + config · `water_to_glb`
- `bake/src/lowres_glb.rs` 219 — `LOWR` → `lowres_<name>.glb` · `lowres_to_glb`
- `bake/src/ktx2w.rs` 227 — KTX2 RGBA8 writer, full mip chain · `write_rgba8`
- `bake/src/lighting.rs` 999 — `sky_` + `lght` → lighting (sun, ambient, BF2's Light2 and `Static()` bits), fog, sky ·
  `lighting_from`, `sky_from`
- `bake/src/soldier.rs` 1637 — SIDE soldiers, weapons, clips (BF2: the cloths they wear as extra segments, `cloth.rs`); a soldier segment's alpha (the disc's per-segment rule,
  queue SKA; a pass sheet keeps its gloss alpha, DSHN); a weapon's `skel` nodes beside its GLB (card FXMF: the
  fire point) · `bake_soldier_anims`, `bake_weapon`, `SOLDIER_CLIPS`, `segment_blends`, `weapon_nodes`
- `bake/src/vehicle.rs` 547 — a SIDE LVL's vehicle classes → `side_<game>/vehicles/<side>/`: every LOD, the destruction
  chunks, `coll` + `prim`, rigid in the skeleton's bind pose (card VBAKE; a model-space body's `BNAM` parts on
  their bones, VBIND), each class's skel nodes, a walker's
  record (WALK) and skin — its top LOD with `JOINTS_0`, `walkers/<model>.glb` (WSKN) · `bake_vehicles`, `VEHICLE_BASES`
- `bake/src/anim_sets.rs` 354 — the PS2's soldier animation sets resolved from the disc (card ANIMS, re-soldier-anim-layers §2, §6): the clip inventory, what `soldiers/<model>.sets.ron` holds (sets as clip indices, the two bone masks, BF1's record facts, BF2's `sanm` facts), a clip's root speed as the PS2 measures it · `Inventory`, `SoldierSets`, `plan_bf1`, `plan_bf2`, `masks`, `root_speed`
- `bake/src/anim_sets/bf1.rs` 231 — BF1's six tables over nine weapon sets and the fallback table (`FUN_0022d828`; 979 slots equal to PS2 memory) · `build`, `Tables`, `FALLBACK`, `weapon_set`
- `bake/src/anim_sets/bf2.rs` 732 — BF2's (family, weapon class) sets: what the class ODFs register and the loader's two passes along the parent chains (`FUN_00282f78`) · `Registry`, `Registry::register`, `build`, `base_name`
- `bake/src/walker.rs` 211 — a walker class's anim table (card WALK, re-walker §5: the 11 entries' clips from its
  `AnimationName` bank, frames, (N−1)/30, loop flag, track 0's root z) and every prim's model-space origin at the
  bind pose; its skin GLB (card WSKN: the skeleton, every entry's clip and `_leftup`/`_rightup` layers) and each leg
  pair's layer bones · `bake`, `clips`, `prim_points`, `skin_glb`, `layer_bones`, `ENTRIES`
- `bake/src/bind_pose.rs` 362 — a placed model in its `skel`'s bind pose (card TBIND): bone-space vertices and
  `BNAM` segments through their bone's bind matrix, door models too, and a door geometry's `PRNT` prims with them
  (card STDOR) · `BindPose`, `pose_bnam` (the vehicle bake's too), `BindPose::rest_prims`
- `bake/src/hardpoints.rs` 155 — a vehicle's or armed building's `skel` nodes in model space at the bind pose (card
  VTUR: the seats' fire points, turret nodes; card VRIDE: the bind axes a rider hangs by), for the map, common and
  side bakes; a class's skeleton lookup ·
  `attach`, `of`, `wants`, `Skeletons`
- `bake/src/turret_rig.rs` 482 — an armed building's model, and a health / ammo droid's (card PWST,
  re-powerup-station §6), as rigid per-node parts (card VTURD, re-turret-draw 1, 2.4,
  6.1 `[O]`): a soft segment cut into one GLB primitive per node (a model with a triangle across nodes stays whole),
  and the class's rig - every `skel` node's bind matrix and parent, the node of each drawn primitive · `TurretRigs`
  (`of`, `split`, `attach`)
- `bake/src/leafpatch.rs` 299 — the `leafpatch` classes' properties and the attach replay (card LEAF,
  re-leafpatch §2–§3 `[O]`: one current class across `entc`s, cap 30/16, the hardpoint's bind rows by CRC) ·
  `leaf_class`, `attach`, `is_leafpatch`, `is_attach_prop`
- `bake/src/entity_light.rs` 322 — BF2's EntityLights (card BPNT, re-point-lights §2 `[O]`): a `Light` class's colour,
  `OmniRadius` and `Static`, and the attach replay that hangs them on a prop's hardpoints, at the carrier's origin
  where the model lacks the hardpoint (card B1DK, re-kamino-decks §5) · `light_class`, `attach`, `Miss`
- `bake/src/cloth.rs` 254 — a BF2 soldier's cloth as rigid-skinned GLB segments (card CLOTH slice 1, re-cloth 2, 4.4, 5 [O]): the gmod INFO list x a `cloth` class x the `CLTH` chunk, placed `v . XFRM . Bind(PRNT)`, pins on their bones, free vertices on the nearest pin's, no step yet (CLOTH2) · `worn_cloths`, `cloth_segment`
- `bake/src/classes.rs` 496 — `entc`/`wpnc`/`ordc`/`expc` → `classes/*.ron` · `bake_classes`
- `bake/src/effects.rs` 393 — each LVL layer's `fx__` particle effects → `effects.ron` (common, maps, `side_<game>/effects/<side>/` with the textures/models they name), the cracked names `effect_names.txt` (card FXP1) · `Collected`, `bake_side`, `names`
- `bake/src/door.rs` 465 — a BF2 `door` class's rig: its triggers, the `open` clip's per-frame motion of each
  prim and each drawn submesh it moves, its Open/Close sounds (`analysis/re-bf2-doors.md`, queues DR2, DRV, `[O]`) ·
  `rigs`, `rig`, `DoorSpec`
- `bake/src/foliage.rs` 666 — the terrain foliage placed as the PS2 places it (card FOLG, re-foliage §4/§6/§9): the
  ELF's random table (read at bake time), the EE's rounding, the LUMI tint, the path-block props → `foliage.bin`;
  the `grasspatch` classes' properties and their sprite atlas (card FOLGG, §7; the atlas read from the ELF) ·
  `bake`, `place`, `tint`, `path_props`, `rng_table`, `grass_class`, `grass_atlas`
- `bake/src/foliage/tests.rs` 442 — the placement and tint against instances read out of the PS2's RAM (the height is `game::viewer::foliage`'s); the ELF table and grass atlas, the grass class, the path props
- `bake/src/sounds.rs` 784 — a SOUND LVL → WAVs + `sounds.ron` (with its `Music` records, `music`; every record with the sub-level that defines it, card SNDE), `Inherit` templates resolved through the game's CORE.LVL; the engine records resolved per sub-level (`engines`: layers in the layered record's order, graphs joined by layer name) · `bake_sounds`
- `bake/src/foley.rs` 85 — the LVL's foley tables → `<map>/sounds/foley.ron` (card FOLY; written by `bake sounds`) · `table`, `write`
- `bake/src/sound_names.rs` 367 — names for hashed sound properties · `build_dictionary`
- `bake/src/global_sound.rs` 552 — GLOBAL VO, per-map streams · `bake_global_sound`
- `bake/src/adpcm.rs` 61 — SPU2 ADPCM decode · `decode`
- `bake/src/ogg.rs` 281 — Vorbis with a reproducible serial · `encode_mono`
- `bake/src/beep.rs` 111 — the BF1 loading beep · `bake_beep`
- `bake/src/hud.rs` 364 — BF2's HUD into the common bake: the five layouts, the four fonts, the COMMON/CORE sheets the layouts name and the spawn screens' button pieces and pad icons (`SCREEN_SHEETS`, card SSELB) (called by `bake_common`), each game's English text table (`bake locl`, card EVMSG), and a CORE.LVL's fonts under a prefix (`bake fonts`: BF1's as `bf1_<font>`, card SQHUD) · `bake_hud`, `bake_fonts`, `bake_locl`
- `bake/src/loadscreen.rs` 170 — BF1 loading-screen LVLs (the zoom sequence) and BF2's still pools → KTX2 + RON · `bake_loadscreen`

## manifest — the schema
- `manifest/src/lib.rs` 1954 — `manifest.ron` / `common.ron` / `vehicles.ron` / `effects.ron` / `sounds.ron` / `foley.ron` / `load.ron` / `classes/*.ron` records and the `foliage.bin` codec · `Manifest`, `CommonManifest`, `VehicleManifest`, `ManifestWalker`, `ClassDef`, `FxSet`, `SoundsRon`, `FoleyRon`, `LoadScreen`, `SoldierSkins`, `ManifestTurretRig`, `music_body`, `foliage_bin`
- `manifest/src/anim_sets.rs` 288 — `soldiers/<model>.sets.ron`: a soldier's animation sets over its GLB's clips, and the two games' slot name tables · `SoldierSets`, `Bf1Sets`, `Bf2Sets`, `bf1::weapon_set`, `bf2::base_name`
- `manifest/src/hud.rs` 137 — `hud/*.ron`, `fonts/*.ron`, the `locl` text table · `HudIndex`, `HudLayout`, `HudNode`, `HudArg`, `HudFont`, `HudGlyph`, `HudLocl`, `locl_key`
- `manifest/src/shell.rs` 136 — the shell bake's `shell.ron` (its movies, sheets, music) and the movie frame pack (`OBJM`: a JPEG per frame) · `ShellIndex`, `ShellMovie`, `FramePack`
- `manifest/src/tests.rs` 1368 — the round trip: what the bake writes, the runtime reads

## core — the sim (tick order: the header of `core/src/sim.rs`)
- `core/src/lib.rs` 59 — module roots, the tick rate · `TICK_HZ`, `TICK_DT`
- `core/src/sim.rs` 144 — the facade: the module header (the tick order), the test fixtures the parts' tests
  share, and a re-export of every item in the eight parts below, so no `crate::sim::X` path names a part (queue SPL5)
  · `Sim`, `PadState`, `InputLog`, `Event`, `buttons`
- `core/src/sim/state.rs` 1933 — what the sim is and how it is hashed: the `Sim` struct, its counters, cheats and
  per-tick events, the constructors, the queries (a unit by handle, the ground under a point — the foot as a ring of
  rays: over a slit, GRND, and up the top of a short steep bevel, RAIL — the terrain ray),
  `Sim::hash`, the door tick and whether a body is a leaf of a door that is opening (`door_opening`, card MOUTH2) or a point lies in such a leaf's box (`opening_leaf_over`, card DOORS), the grenades on the ground bots are warned of (`Hazard`, card HOLD); the hash tests, the
  pinned `--sim-hash 100` one among them · `Sim`, `Event`, `NavStats`, `Cheats`, `DeathCause`, `Hazard`,
  `LETHAL_LANDING_SPEED`, `omni_radius_for`
- `core/src/sim/input.rs` 910 — what drives a unit: the pad and its button bits, the input log and its replay,
  step 2's aim assist (the stick bend, then FUN_0025e098's acquisition over units, vehicles and building turrets:
  the magnet and the reticle's pick, card HTGT); the replay, pad-slot, aim-assist and acquisition tests ·
  `PadState`, `buttons`, `InputLog`, `AimTarget`, `Sim::run_log`, `Sim::magnet_at`, `cone_score`
- `core/src/sim/spawn.rs` 2965 — who is on the field: a spawn through the reinforcement gate (on the floor of its node and on the level, `spawn_ground`; a post's node inside a solid body on top of it, as the PS2 — the pen test `floor_reach` and `resolve_spawn_seats`, card LLEG), the no-overspawn The human's spawn-map post: `pick_spawn_post` (along + 0.25 perp²), `keep_or_nearest_post`, `spawn_post_selected` (card SPSEL).
  shedding, the class tables and the AI's class weights (default 4 : standard 2 : special 1, card AICLW), the spawn screen's class choice, the respawn at a friendly post, the spawn waves, where a team may spawn (its posts' nodes and its live command
  walkers' six class points, card CWSP) and which of them a bot takes (one post drawn by BF2's `AISpawnWeight`, kept from the end of the respawn delay to the spawn, then one of that post's nodes, card BSPN; a human still draws from the flat node list); the spawn, class, spawn-screen and seat tests · `Sim::spawn`, `Sim::spawn_ai`, `Sim::ai_class`, `Sim::spawn_points`, `Sim::spawn_posts`, `Sim::draw_spawn_post`, `walker_point`, `SpawnWaves`, `Sim::floor_reach`, `Sim::floor_flood` (the same flood stopped at a goal, with the walk's length; card SPNC's census), `Sim::floor_path` (its steps, best-first to a hub's disc within a band of the line: card TGAP's wall test), `PEN_CELLS`, `SEAT_REACH`
- `core/src/sim/blast.rs` 680 — an explosion's area damage (card VDTH, re-vehicle-death §2-§5): the ≤ 20 gather in the
  blast box, LOS past terrain and the world, the falloff on each victim's sphere, TakeDamage with the team rules; a
  destroyed vehicle's class explosion goes through it · `Sim::explode`, `soldier_blast_sphere`, `BLAST_GATHER_MAX`
- `core/src/sim/bounds.rs` 1501 — the playable level (cards KFLR, BNDS, LVLF: a trench cut into the terrain is on it; `analysis/re-kill-floor.md`): each hub's floor
  walked from the posts along the arcs and voted by its neighbours (`Sim::nav_floors` — the authored hub heights are
  not floors; card STLVL: a post pins only the hub whose disc it stands in, a seam between two models is not a drop,
  and `Sim::floor_reaches_hub` asks the same walk whether a unit is on a hub's storey), the arcs no soldier can walk (the floor walk, and card TGAP's wall test: walled both ways round the walls), the hubs he is trapped in (`Sim::hub_trapped`; pinned per map by `game/tests/suite/nav_walls.rs`), the level test (on the terrain, or level with the floor a walk from a
  near hub reaches before its first `BARR` barrier — a walk a barrier stopped allows a kerb's drop, not a hull's, card
  BOWL), the rim census, the point a lost bot walks back to and its FLOOR walk there (card RETW: an A* over a 0.75 m grid to
  the first cell of a hub's disc that is on the level, pulled straight along what a soldier's body clears), the kill floor and the hard landing under the map — both
  kill past the invincibility cheat, as a death region does · `Sim::resolve_nav_floors`, `Sim::level_at`,
  `Sim::rim_drop`, `Sim::return_point`, `Sim::return_path`, `Sim::floor_walk_to` (card RLOOP: the same floor search to one point), `Sim::floor_line`, `ReturnPath`, `Sim::arc_unwalkable`, `Sim::lands_under_the_map`, `Level`, `BOUND_DROP`,
  `RIM_DROP`, `KILL_FLOOR_DROP`
- `core/src/sim/combat.rs` 1022 — a bolt's impact (damage — a soldier's or a vehicle's hull pool, D12's human damage layer, the kill and its ticket; then its ordnance's impact explosion through `Sim::explode`, card RKEX) and the
  environmental deaths (a death region, a lethal landing, the kill floor, a landing under the map — which of them
  the invincibility cheat spares: `DeathCause::cheat_spares`); the bolt, cheat, damage and death tests ·
  `Sim::kill_environment`
- `core/src/sim/round.rs` 372 — the command posts and how the round ends: capture (soldiers on foot only), the bleed, D12's Elite
  handicap, domination, the outcome; the capture, defeat and handicap tests · `Sim::add_cp`
- `core/src/sim/seats.rs` 698 — enter, drive, exit (card VSEAT, re-vehicles §5, re-hover §5.2): step 1c's use
  input (the ACTION edge; the aim-cone pick, CanEnter's seat, a player's AI takeover) and the bots' enter / exit
  requests (VAI; a driver out with its stuck window spent marks the vehicle, VMESH; a new pilot's need-gunner call and
  its answers, UTALK), the 9-point exit placement (a
  building turret's without the path test, VTURB; no point shut inside a map body, then a ring past that body's box,
  VMESH: `Sim::shut_in`, `Sim::shut_in_solid`),
  a rider's pad written into its vehicle's `HoverInput` (the stick shaping and the full-stick timer; a bot's as an
  AI driver), the driverless clear and the riders carried after the hovers · `Sim::use_pass`, `Sim::ai_use_pass`, `Sim::enter_target`,
  `Sim::enter_seat`, `Sim::enter_vehicle`, `Sim::exit_vehicle`, `driver_input`
- `core/src/sim/seats/tests.rs` 352 — enter / refuse / take over / drive / turn / exit / shaping on flat ground with
  the skiff's numbers
- `core/src/sim/guns.rs` 496 — seat guns (card VTUR, re-vehicles §5/§9): step 1c's seat switch (SWITCH_WEAPON, FUN_0020b5e8),
  step 4's rider, human or bot (VTAI) — the look, the 512 m camera ray (past a turret's own building), the aimer (a
  building turret's lays its barrel's line on the point; a bot's takes its own aim point, card TURR), the seat's weapons fired through
  `WeaponState` and `Bolt::spawn`, a bot's shot held to the fairness rule; a seat's live look / eye / barrel for a
  bot's perception and aim; gun states made when anyone rides an armed seat · `Sim::next_seat`, `Sim::switch_pass`,
  `Sim::seat_gun`, `Sim::seat_look`, `SeatLook`
- `core/src/sim/guns/tests.rs` 503 — switch, fire point, aimer rate and clamp, a side gun's rest, the pilot's
  locked yaw, the hash with nobody aboard, a gunner's bolt hitting a soldier, a bot gunner firing at what it sees;
  Mygeeto's recoilless turret: the camera ray past its own building, the barrel's line on the point, the muzzle at
  its fire point (card TURR)
- `core/src/sim/step.rs` 2248 — `Sim::step`, one tick in the header's order, and its unit separation; the wall
  pass's velocity slide (card JWAL); a human's shot from the fire point at what the aim ray hits (card AIMA:
  `Sim::aim_hit`, `AIM_RAY_START` / `AIM_RAY_LENGTH`); the soldier's zoom routine - the press, its cancels, the
  divided turn rates (card ZOOM: `Sim::tick_zoom`); the movement, zoom, wall-pass, ledge, stamina-cheat, AI-fire and
  aim-ray tests · `Sim::step`
- `core/src/sim/vehicles.rs` 1739 — the vehicle slots: spawn with slot reuse, the handle queries, the live team
  (§7.1: the last entrant's in any seat, neutral when emptied), the two occupancy writers that keep `Seat.occupant`
  and `Unit.mount` in step, removal; the hull pool's damage (bolts through VehicleScale, collision damage) and
  death (the event, the riders, no wreck); tick stage 7, the spawn pads (§7: follow the post, spawn, the idle
  decay); tick stage 3a's walkers (WALK: `Sim::tick_walkers`, the rays and building contacts they meet); the
  hulls in the collision world (VHULL: `Sim::sync_hulls`); the slot, seat, team-table, despawn, damage,
  hash-append, pad and hull tests · `Sim::spawn_vehicle`,
  `Sim::seat_unit`, `Sim::unseat_unit`, `Sim::vehicle_team`, `Sim::damage_vehicle`, `Sim::impact_vehicle`,
  `Sim::destroy_vehicle`, `Sim::add_vehicle_pad`
- `core/src/rng.rs` 49 — SCE `rand()`, the seeded RNG · `SimRng`
- `core/src/lcg.rs` 140 — the gameplay LCG (spread, delays) · `GameLcg`
- `core/src/fmath.rs` 363 — `sin_cos`/`atan2`/`asin` on `libm`; the `BANNED` referee, over every file under
  `core/src` (module directories included) and `game/src` bar its presentation files (`PRESENTATION`)
- `core/src/soldier.rs` 1298 — stances, movement along the floor's plane, the floor contact (and card SKIRT's three PS2 rules that carry a run up a short steep band: the flattest normal, the dead zone, the overlap's lift-off), roll, energy, the sprint's start, the jump and its gates · `Soldier`, `SoldierConstants`, `GroundRules`
- `core/src/soldier/tests.rs` 1254 — the stance vectors, the ground rules, the PS2's jump trials (take-off, air stick, gates), its uphill run and its contact rules on Kamino's pod skirt (card SKIRT)
- `core/src/unit.rs` 1066 — soldier + health + weapon + team, the acquisition's box / aim point, the PS2's camera sections per game and
  stance, the ideal chase camera, a human's aim ray and the fire point (card AIMA, re-aim-assist §10); a human's zoom:
  the ZOOM camera sections and the scope view's eye and aim (card ZOOM, re-zoom §5) · `Unit`, `Zoom`,
  `UnitSpec`, `UnitHandle`, `AIM_BOX_MIN`/`MAX`, `CameraSection`, `camera_section`, `IdealCamera`, `SPAWN_PITCH`; BF2's aim-error block every unit carries about itself as a target and the direction its own error points (card CLSE2: `AimBlock` - decay, budget, growth by change of velocity, the tumbling direction; its tests)
- `core/src/vehicle.rs` 505 — the second entity kind (re-vehicles §5-§8): the seat ring, one hull pool (the
  class's HealthType), the kind, the live and home teams, the hull-contact damage formula, a class's seat setup
  (VSEAT; its AISizeType route table, VAI; its seats' guns, VTUR), the seats' gun states · `Vehicle`, `VehicleSpec`, `SeatSetup`, `VehicleHandle`, `Seat`, `SeatKind`, `Mount`, `is_ring`, `impact_damage`, `collision_desc`
- `core/src/vehicle/hover.rs` 819 — the hover integrator (cards HOVR, HCON, HOVX, HHUL, HSTEP; re-hover §4/§6/§7/§13): BF2's drive (spring
  branch, the yaw slew and its BoostSpeed scale, tilt feedback, §6.19; the boost latch and energy pool, the bike
  jump), the contact response
  (each body re-queried after the last push-out: the spring spheres, then the class's hull bodies through K7-K9),
  then the spring tick (the PS2's order, re-vehicle-rest §5),
  the PS2's sphere-against-terrain rule (§13.5); stepped at tick stage 3a by
  `Sim::tick_hovers` (the pad's timer is the seat pass's, `ramp_stick_timer`) · `HoverParams`, `HoverInput`,
  `HoverState`, `JumpParams`, `HullBody`, `HullShape`, `terrain_sphere`, `step`, `drive`, `springs`, `contacts`, `ContactSource`, `quat_x`/`quat_y`/`quat_z`
- `core/src/vehicle/walker.rs` 1003 — the walker integrator (card WALK, re-walker §2-§9): §3's aim rates and the
  forced throttle on a slope around §4's locomotion (the stick classes, speed / boost / energy, the aim window
  and the camera-relative turn, the clip state machine and its clock, root motion along the legs), §8's terrain
  probes, §6's legs (lift, 3 g fall, two-pair pitch and roll, each pair's layer blend), §9 C7's building contacts;
  the drawn model's legs heading · `WalkerParams`, `WalkerState`, `Clip`, `LegPair`, `WalkerWorld`, `step`, `legs_rot`, `TRANSITIONS`
- `core/src/vehicle/walker/tests.rs` 409 — §14.6's AT-ST numbers on a flat ground: 5.0 m/s at full stick, the
  ×0.5 floor, 10 m/s boosting for 4 s then 2.2 s to recover, ≤ 1.0 / 0.6 rad/s, the turn-step thresholds, the
  one-pair lift, the 29.4 fall, the stride and reverse sequences
- `core/src/vehicle/gun.rs` 636 — a seat's guns (card VTUR, re-vehicles §5/§9): the aimer (PS2 defaults, rate-limited,
  clamped), the weapons, fire point, pivot and eye, the barrel's bind heading, the model-frame angles; the run-time
  state · `Aimer`, `GunSetup`, `GunWeapon`, `GunState`, `AIM_RAY`, `dir`, `angles`, `turned`
- `core/src/vehicle/hover/tests.rs` 989 — each equation against re-hover §9's hand numbers; the skiff settling (its bob band in the HSTEP order),
  accelerating, turning and stopping at a wall on a flat plane; the energy pool, boost and bike jump by hand; two
  runs bit-identical; §13.5's terrain rule by hand, a hull box's layers, a BF2 fighter tank stopped at a thin post
  on `p_cube1`'s face (HHUL)
- `core/src/vehicle_pad.rs` 189 — a vehicle spawn pad's state (re-vehicles §7): the §7 defaults, the class per
  team (a flyer's with its LandedHeight, VREST), the countdown, `SpawnCount`, the per-vehicle idle counters · `VehiclePad`, `PadDef`, `PadClass`
- `core/src/weapon.rs` 1748 — fire cycle, spread, damage, the acquisition's class fields from `wpnc`/`ordc`; the
  `grenade` weapon class's keys and the PS2's clip rule for a class slot (card GREN); the zoom keys and the press's
  steps (card ZOOM: `zoom_press`, `zoom_at`); the AI's range triple (card FGHT: `ai_range`) · `WeaponClass`, `Health`,
  `Thrown` (`BF1_THROWS_AS_BF2`, `throw_as_bf2`: a BF1 map's grenade thrown by BF2's rules, card GRNP), `WeaponState::with_clips`
- `core/src/weapon/tests.rs` 717 — re-blaster-damage §9's reference vectors and the state-machine behaviours
- `core/src/sticky.rs` 551 — the thrown ordnance (card GREN, re-grenades §4 `[O]`): the `sticky` class (gravity, the
  `Stick*` mask replayed in file order), one grenade's flight, the bounce's constants, stick, rest and the fuse
  that starts at rest; the PS2's own launch / bounce rows as tests · `StickyClass`, `Sticky`, `bounce`, `throw_dir`
- `core/src/laser.rs` 324 — a `bolt` / `laser` class's look (card FXB, re-bolt §1–§4 `[O]`): the laser keys through
  the setters (LaserLength resets Glow/BlurLength), the cap, BF2's End-colour fade, the width rule; the bullet base's
  light keys and BF2's light gate (card FXBL, §5.2); render-only · `LaserLook`
- `core/src/flash.rs` 309 — a weapon class's muzzle flash (card FXMF, re-muzzle-flash §1–§4 `[O]`): the flash keys
  through the setter (the three presets, the copy ctor's per-game model), the fade min(A, 2A·t), the unnormalised
  aim matrix; render-only · `MuzzleFlashLook`, `matrix`
- `core/src/impact.rs` 208 — what a struck bolt and an explosion spawn (card FXP3, re-fx §7 `[O]`): the ordnance's
  `ImpactEffect*` slots by struck surface (the bullet ctor's defaults, the setters), `TrailEffect`, the explosion
  class's `Effect` (BF2 a list of five) / `WaterEffect`; render-only · `Surface`, `ImpactLook`, `ExplosionLook`
- `core/src/projectile.rs` 162 — bolts, one predictive ray at spawn · `Bolt`
- `core/src/sim/pickups.rs` 463 — card PWUP in the tick (re-powerups §3.2, §3.4, §4, §5): a dead soldier's roll
  and drop (the class table's walk, the pool cap, each game's drop velocity), the item (gravity, the floor ray, life
  30 -> -1 s), the pickup by touch (health, clips to every weapon, BF2 energy and the offense / defense timers; only
  when it gives something) and the buffs' factor on damage · `ItemClass`, `DropTable`, `Pickup`, `Buffs`,
  `Sim::add_item_class`, `drop_item`, `tick_pickups`, `buff_scale` (in `combat.rs`)
- `core/src/sim/pickups/tests.rs` 379 — the table census (10,000 rolls a table), 31 s, the pool cap, who takes what
  per game, the 0.5 s flight rule, the drop velocity, the head-on reach, BF2 energy and buffs, the hash
- `core/src/sim/stations.rs` 254 — card PWST in the tick (re-powerup-station §3, §4): the health / ammo / vehicle
  droids; each station's clock (`PowerupDelay`, the remainder kept) and its pass over everyone strictly inside its
  `Radius` sphere (3-D, feet; any team; health to the cap and clips with the pickups' give rules; vehicles health
  only), the serving flag; the class's animation keys for the draw · `StationClass`, `StationAnim`, `Station`,
  `Sim::add_station_class`, `tick_stations`
- `core/src/sim/stations/tests.rs` 225 — once a second to max (BF1 +15, BF2 +25), the kept remainder, the strict
  3-D sphere, both teams and not the dead, the ammo droid's clip every 2 s, a full unit not served
- `core/src/sim/ordnance.rs` 990 — card GREN in the tick (re-grenades §2.3, §2.4, §3.3, §4.2): the two weapon channels
  (which trigger has the soldier, the two switch inputs, every held weapon's cycle), the throw, and phase 5b — each
  grenade's fuse, flight, sweep ray (the terrain by BF2's own segment test, card GRNP), contact, the bots' warning
  and the blast through `Sim::explode`; the cycle, channel, switch and throw-to-blast tests, seven PS2 BF2 throws row
  for row (`core/tests/data/grenade_ps2_bf2.txt`), a BF1 grenade thrown as BF2's, a steep throw at a mesh floor
  rests on it (card GRFL: the bodies' ray is not pulled back) · `Sim::held_weapons`, `Sim::launch`, `Sim::tick_stickies`
- `core/src/aim_assist.rs` 286 — BF1 aim-assist magnetism · `apply`
- `core/src/heightfield.rs` 359 — terrain height query and its triangle's normal; the marched ray's probe (a cell's corner bounds answer a point clear of them, and how far along the ray that holds) · `Heightfield`, `HeightProbe`
- `core/src/collision.rs` 328 — the facade: the module header (the layer masks, determinism, the parry 0.22.1
  contact bug), the step and ground constants, the parry conversions the parts share, the test fixtures their
  tests share, and a re-export of every item in the four parts below, so no `crate::collision::X` path names a
  part (queue SPL3) · `STEP_HEIGHT`, `WALKABLE_MIN_NY`, `TERRAIN_BURY`, `NO_GROUND`
- `core/src/collision/shape.rs` 1250 — the soldier capsule, the AABB, a body's shape (its ray cast: a cylinder in closed form, `ray_cylinder`, card OBST2 — held to parry's cast and, card OBSV, to an oracle that knows only the inside test: misses, grazes, starts inside, short rays), a body's extent on the ground in a given frame (`Body::ground_extent`, the vehicle footprints of `core/src/ai/avoid.rs`) and the mesh builder, the layer
  mask, bodies (a hull's with its vehicle as `owner`), a vehicle class's hull part, terrain cutters; the layer-mask
  tests · `Capsule`, `Aabb`, `Shape`, `trimesh`, `Layers`, `Body`, `HullPart`
- `core/src/collision/world.rs` 909 — the baked world and its broad phase, an XZ grid of body indices built once
  that offers each query its candidates in index order; its movers (DR2's doors), filed under an envelope; the live
  vehicles' hulls (VHULL), visited after the map's bodies, and who skips whose (`HullFilter`) ·
  `CollisionWorld`, `BodyGrid`, `CollisionWorld::move_body`, `CollisionWorld::set_hull`, `HullFilter`
- `core/src/collision/rays.rs` 1053 — the cutter test, the ground ray, the world ray bolts and LOS take, the marched
  terrain ray, BF2's own terrain segment test (card GRNP, re-grenades §13.2 `[O]`: the first bisection midpoint within
  0.1 m of the ground, not the crossing — the grenade's sweep takes it), the lead solve · `is_cut_by`,
  `CollisionWorld::ray_down`, `CollisionWorld::ray`, `CollisionWorld::ray_with` (a `HullFilter`, the hull's owner in
  `RayHit`), `CollisionWorld::ray_past` (past the map bodies of up to two instances: a turret's own building, card
  TURR), `ray_heightfield`, `ray_terrain_ps2` · card VMESH: `CollisionWorld::enclosing` (the mesh body a point is shut in, by crossing parity: 2 of 3 rays up and 7 of 8 sideways odd)
- `core/src/collision/walls.rs` 2316 — the wall pass (and `wall_sweep_small`: the sweep against only the small bodies, what the AI's look-ahead asks, card OBST2): the push-out (a body's wall contact first, a ceiling's horizontal part: card WTUN), the soldier's LEDGE rule (card JWAL,
  `analysis/re-soldier-walls.md`; a riser below the step inside the capsule's footprint is a kerb, `top_in_disc`, card SKIRT), its sub-steps along the move and no push through a face (card WPSH), the sweep
  (Phases A.2–A.4), the parry contact workaround, a hover hull box's or cylinder's contacts (`shape_contacts`, HHUL) · `CollisionWorld::push_out`, `CollisionWorld::push_out_soldier` (and `_hulls`, the AI's hull probe, card STFAL),
  `CollisionWorld::push_out_soldier_from`, `CollisionWorld::wall_sweep` (`wall_sweep_body`: and the body that answered, for a diagnostic), `mesh_contact`, `pushes_past`,
  `SWEEP_CONTACT_DIST`
- `core/src/region.rs` 209 — box / sphere / cylinder regions · `Region`
- `core/src/cp.rs` 602 — posts: capture machine, bleed, domination · `CommandPost`
- `core/src/door.rs` 263 — BF2's self-opening door: the trigger spheres, the open parameter, the sound latch
  and the pose between clip frames (queues DR2, DRV, `[O]`) · `Door`, `DoorBody`, `Trigger`, `Edge`, `sample`
- `core/src/battle.rs` 361 — reinforcements, no-overspawn · `Battle`
- `core/src/announcer.rs` 793 — the commander's voice-over rules (post lines, bleeding, low reinforcements, the gates), the
  one-line-at-a-time voice queue, and the bleed rate BF2's conquest script hands the manager (3 s at every post event of a
  last stand, card ANNC2); a line a unit's own handle holds (card UTALK, `Line::speaker`); pure, no audio · `Announcer`,
  `VoiceQueue`, `PostLines`, `ScriptBleed`
- `core/src/music.rs` 740 — the battle's music, pure (card AMBM, re-ambience-music 3 `[O]`): the state by reinforcements per game (`state_bf1` PROVISIONAL, `state_bf2`), the manager's table and update, a `Music`'s fade / min playback / min inactive / max playback, one Music at a time, the battle's end; tests against 227 PS2 RAM states and Naboo's take timeline · `Ambient`, `Player`, `Music`, `Rule`
- `core/src/nav.rs` 1504 — the planning graph: the PS2's locate (`locate`: BF1 hub sphere, BF2 disc, 3-D corridors, the nearest arc; card LOC3) and the port's XZ unit locate (`locate_unit_on`, deviation D17), the PS2 router, the corner walk round barriers (`detour`: the PS2's lazy search - the silhouette corners of the barriers a segment actually crosses, card OBST2), the mission's barrier switches (`set_barriers`) · `NavGraph`
- `core/src/nav/tests.rs` 811 — the planning graph's tests: both locates, the chooser, the BFS, jet arcs, the route table's bytes, barriers, the corner walk (a crowded band, a ring of rails, a shut end, the silhouette pair; card OBST2)
- `core/src/nav/flank.rs` 321 — card FLNK (`analysis/bot-design.md` 6.2, D5; OURS): the FLANK TABLE built when the posts register (`Sim::build_flank`): per ordered pair of post hubs the authored set and main route of the soldier route table, and the shortest other way with the authored arcs at x3 kept when at most half its arcs are authored and it is at most 1.35 x the main route (the counts of the bot design's M5 search, `flank.py`); each pair's share drawn per match between 10 and 30 % (`share_draw`, seed and pair); an attack's origin post (`origin`, within 2 arcs) and where a flanker joins its route (`join`, within 3) · `FlankTable`, `FlankRoute`, `NavGraph::flank_table`, `NavGraph::authored_route`, `share_draw`, `LONGER`, `AUTHORED_SHARE`, `AUTHORED_COST`, `SHARE_MIN`, `SHARE_MAX`, `ORIGIN_HOPS`, `JOIN_HOPS`
- `core/src/nav/flank/tests.rs` 138 — the flank table on a diamond (both ways, lengths, the walk), an other side over +35 %, a way mostly over authored arcs, the share draw's bounds and mean
- `core/src/route_patch.rs` 289 — BF2 `plnp` route patches: Block/UnblockPlanningGraphArcs on the route table · `RoutePatches`
- `core/src/ai.rs` 965 — the facade: the module header (Phases A–A.4, and which steering is ours), every AI tuning
  constant, kept together (PROVISIONAL; VAI's `BOARD_*` / `DRIVE_*`, VTAI's `GUNNER_WAIT`), the test fixtures the parts' tests share, and a
  re-export of every item in the parts below, so no `crate::ai::X` path names a part (queue SPL4) · `HOLD_RANGE`, `THINK_PERIOD`, `PROBE_DIST`
- `core/src/ai/state.rs` 1787 — what a bot carries between ticks (the fields grouped by the layer that owns them, card SPL11), and its hash: the route leg, the steering's last
  pick, the per-think diagnostics, its trigger on a single-shot weapon (STTAP), its vehicle business (VAI), its way back to the level (BNDS), the position it holds
  (HOLD), the no-net-progress watch (RCYC), the mover's books and the walk's outcome and the behaviour's name (MOVR: `MoverState`, `Walk`, `Behaviour`), what its position is in a fight (FGHT: `FightState` — stand, walk to a ring point, pause; REACT: on its way to the position a hit sent it to, and the tick its roll is pressed, `HoldState::roll_at`); the AI-hash test · `AiState`, `FightState`, `Route`, `NavTick`, `SteerPick`,
  `RouteSource`, `RideState`, `LostState`, `HoldState`, `StallState`
- `core/src/ai/stall.rs` 787 — the last resort (card RCYC): a bot on the level that wants to walk and makes no net
  progress (`RECYCLE_ROAM` from where its count began in `RECYCLE_AFTER`, card OBSR; new ground by `RECYCLE_NET` in the
  endgame) is given up and respawns (`DeathCause::Stalled`), and so is one whose walk fails `PIN_FAILS` progress windows
  without leaving one place (`PIN_ROAM`; card RLOOP: about 22 s, whatever plans it is handed; a bot off the level dies as lost) - unless `send_off` finds it a floor walk to its leg's waypoint (`Sim::floor_walk_to`, walked as a `LostState::pinned` spell; once per place, `PIN_WALKED`); what is and is not recycled, as tests (a bot under its
  post's floor or on a roof over it is, card STEND; a bot lapping a yard it cannot leave is, card OBSR; a bot shut in one place with two plans in turn is, within the bound, and one with a floor walk out walks it, card RLOOP) · `watch`, `send_off`
- `core/src/ai/command.rs` 341 — a squad order's AI side: the commanded unit's scratch state and the order clear;
  the D6–D8 follow / hold / release tests · `CmdState`, `AiState::cmd_clear`
- `core/src/ai/draw.rs` 368 — the AI's randomness: the named salts, the per-unit hash draws, the D13 stance pick;
  the stance tests · `unit_rand`, `unit_unit`, `unit_dir`, `route_rand`, `stance_hold`
- `core/src/ai/route.rs` 1494 — objectives and legs: the commander's objective or Phase A's own rule, the leg planner
  (the authored table, BFS, the straight line), the point of the next hub a soldier's leg walks at — the PS2's waypoint, drawn per unit per leg on the hub's sphere, again while it lies in a barrier, off the hub's own floor or off the floor walked from the unit (`hub_point`, card HUBP; a stuck unit's re-plan walks centre to centre, `centre_leg`), the corner walk, the same-level test (a resolved hub's floor: `hub_on_level`, card BNDS), the hub a soldier routes
  from or is hunted at — on its own storey, not the roof over it or the cave under it (`level_hub`, card STLVL), the route type (a driver's is
  its vehicle's), a walker's hub for a post its type cannot reach (`post_hub`, card WAI), the point of a post's capture region a last leg walks to — drawn per arrival, standable, never the centre — or the enemy contesting the region (`post_point`, `aim_last_leg`, card PARR), the route's advance — the corners behind, then the hub reached on its level (`advance_route`, `hub_reached`, card BNDS; from `think`, card SPL11; the leg's line and the arrival plane run from the waypoint just reached to the next, `Route::from`, card HUBP) · `plan_leg`, `start_hub`, `level_hub`, `mover`, `post_hub`, `advance_route`; for card SPNC's census `first_leg` (a unit's first leg from today's start hub and from the bare locate before card DAGS) and `floor_walked`
- `core/src/ai/route/tests.rs` 1071 — the routing tests: the corridor, the corners, the level test, the hub a unit routes from, a post's point, a hub reached on its level, a unit's own point of the next hub (200 units, a barrier, a rim and a wall, a stuck unit's centre leg, the leg from the waypoint reached; card HUBP), a corner walk over the leg's array continued (card OBST2), a bot spawned on a deck over a lower route planning from the deck's own hub (card DAGS), the leg line from the last corner passed and a hop's new progress window (card MOUTH)
- `core/src/ai/avoid.rs` 1272 — the AVOIDANCE of a bot on foot (card OBST, `analysis/bot-design.md` 4.7; ours, PROVISIONAL): the heading the mover walks on its pick tick — one side per obstacle kept as a commitment (`AiState::slide` + `MoverState::round`; a rim barrier is marked `Round::rim` and gets no allowance from the progress window, card OBSR), an inside corner turned along the next wall on the same hand, the leave rule (the obstacle is left only when the free straight way ends nearer the aim point than the walk has been; until then along it and round its end, on its own level), a small body passed by the tangent of its disc before it is touched (`SteerPick::Round`), a STANDING vehicle passed by its footprint (card OBSV: the rectangle about the hull parts at a soldier's height, turned at its corner on one side chosen once, `Round::foot`; a walk point a vehicle is parked on is reached at its side, `at_foot_of`); the tests of going round (a block, a crate, a turret, a hull, a 20 m and a 40 m wall, one box at 252 yaws and offsets as a map body and as a parked hull, the side kept, a moving / boarded / overhead hull, a bot beside a hull, a waypoint and a detour corner under a vehicle, a yard, the level, the rim mark) · `pick`, `clear`, `footprint`, `foot_ahead`, `at_foot_of`, `ROUND_MAX`, `ROUND_TRAVEL`, `LEAVE_LOOK`, `LOOK_DIST`, `SMALL_BODY`, `CLIMB_MAX`, `FOOT_LOOK`, `FOOT_STILL`, `FOOT_CLEAR`
- `core/src/ai/steer.rs` 1437 — the steering's PROBES (card OBST: the heading picked from them is `avoid.rs`'s): the wall sweep ahead (and the hull probe), a leaf of a door that is opening as no wall (`StopKind::Door`, card MOUTH2: the bot walks on at it) and as no steep floor (card DOORS: the diagonal edge of BF2 Death Star's blast door under the steep-floor probe), a `BARR` barrier at an edge of the level as a wall (`barrier_ahead`, card BNDS), the floor probe (card STFAL), the steep-floor probe (`steep_ahead`, card STHOP; a band the soldier's run carries him over is no wall, `short_band`, card SKIRT), the ramp test (`ramp_clear`, card OFFG: the level sweep asked again along the floor up a ramp);
  the walking tests (the slide, the stuck side-step, the pad's frame); which test shuts the way ahead, as a diagnostic the sim never calls (`way_ahead`, card BLKB) · `rotate_xz`, `way_ahead`, `Stop`, `StopKind`
- `core/src/ai/tactic.rs` 1437 — card TPOS, `analysis/bot-design.md` 4.1 / 4.6: the POSITION TABLE built once when the posts are in (`Sim::build_positions`, DERIVED, not hashed; card TPOSA): Cover / Snipe hints, 8 points outside each soldier barrier, each hub's centre + 8 ring points, kept when standable; per position its floor, source, a hint's facing and stances, its hub, LOW and CORNER cover over 16 directions; an 8 m grid index (`within`). The TACTICIAN (`fight`, `choose`): a fighter's position - candidates (the 32 nearest with cover from the target in the band, within `COVER_REACH` 15 m, and the 32 nearest within 8 m), yes / no filters, three tries (cover = crouch-shut toward the target + the confirming ray), teammates a blast apart where the place allows (`SPREAD`), the score; kept while its conditions hold (re-scored every `RESCORE`, the PS2's 5 s; an open one asks for cover every 2 s; the 5-12 s timer from cover takes only cover; the mover's arrival counts only near the position, `walk_arrived`); the hide / fire cycle and the next position after 5-12 s; two hits leave it (`hit`); a walk there ends near it (`cut_short`: within 1.5 m it is there where the bot stopped, else failed, the last four failures remembered), a new position is 3 m from the one left, a Cover hint hides by its arrow (the PS2's rule, `Position::hides_toward`) and one at a barrier's edge is moved out of it; reservations granted in the write-back (`grant`) and renewed every tick a position is held (`keep`, card TPOSR); the Tactician runs on a remembered target as on a seen one (`Foe`: the cycle, the hit / timer leaves; the re-score waits for sight), a position hit twice not asked for again; B12 (`covered_from_target`); the fight-time split's events (`EV_*` on `NavTick::tpos_ev`, the funnel `NavTick::tpos_funnel`; the sweep's `fightsplit` / `tactician` / `tfunnel` lines) · `PositionTable`, `Position`, `Source`, `sector_bit`, `sector_pair`, `build`, `fight`, `grant`
- `core/src/ai/tactic/tests.rs` 1372 — the table's and the Tactician's tests: the crate's cover taken and cycled, spacing, the endgame, two hits, no winner, a move not a twitch, a walk cut short / past its deadline / gaining nothing for 1 s, a position behind a barrier, a hint's arrow, a covered candidate past 40 nearer open points, the 15 m reach, the 5 s re-score, an open position giving way to cover, the drop test, a hint pushed out of a barrier's margin, a walk ended short of the position held, the timer from cover taking only cover; card TPOSR: a hidden bot's reservation, cycle and leave tests, a position just left reserved, a hint's side, the tier's move share, a corner's lean, a friend's line, a grenade, the band's inner edge, the confirming ray, the move from the one left, B12's counter, a storey up, the walk's deadline, no FFLN step at a position
- `core/src/ai/team.rs` 558 — card TEAM, `analysis/bot-design.md` 4.1 / 4.3: what a team knows together. The UNIT GRID (8 m cells hashed into 256 buckets, a counting sort of the alive units, built in `Sim::step` just before the think loop and closed after it; DERIVED, not hashed) and `Sim::units_within` (the grid's answer inside the think window, the plain loop outside it, both in unit-index order), read by `perception::scan` / `tell`, the gunner's friend-in-line test and the follow gap; the FOLLOW GAP (`follow_ease`: a bot eases to half stick a stride behind a team-mate walking ahead the same way; bar B3); the TEAM BOARD (`board`, once a second per team): the COVER role (card TEAM) and the FLANK role (card FLNK, bot design 6.2: per (origin post, attacked post) group, `flank_cap` = the pair's share of the group rounded, none under 3, given to members on their first leg and re-planned onto the flank route; ends with the attack) · `FLANK`, `FLANK_GROUP_MIN`, `flank_cap`, `UnitGrid`, `CELL`, `BUCKETS`, `FOLLOW_GAP`, `FOLLOW_STICK`, `follow_ease`
- `core/src/ai/team/tests.rs` 471 — the grid against the plain loop on 1,000 random scenes, the sim asking the grid only while it is live, the follow gap's cases (ahead, off the cone, behind, an enemy, standing, other ways, another storey, abreast); the Cover role; the Flank role on the diamond (2 of 10 at a share of 0.2, none of 2, none at Easy, the walk over the other side and the role's end)
- `core/src/ai/behave.rs` 2546 — what a bot on foot is DOING (card SPL11, `analysis/bot-design.md` 3): `think`, one priority chain a tick — perceive and retarget on the think period, then what walks the bot (the return to the level, the run from a grenade, the held position, a fighter's ring point, the vehicle it boards, the leg) and what it looks at — which hands exactly one `AimRequest` to the gunner and one `MoveRequest` to the mover and writes no stick itself; a seated bot's tick goes to `ride`. Until their cards replace them: the position a bot holds and fights from (card
  HOLD: the spot, the step out of a friend's line, the close hold; its hold at a hub, the dwell, the shuffle and the strafe on a burst went with card TPOS), the on-hit dodge, the run from a grenade, the return walk of a bot off the level along its floor walk (card RETW) and its jump at a low wall (card BNDS); a failed walk's ladder (card MOVR: `recover` — the post given up for the commander to draw another); the FIGHT (card FGHT, re-soldier-ai 4.4-4.5: a seen target inside the weapon's `MaxRange` + 10 m, not a rider, is fought from a position of the bot's fixed style — `fight_position`: it stands to shoot on a spot 4 m clear of the other fighters, `stand_spot`, or circles: a walk over the graph to a point of the ring round its target, `ring_point` / `ring_walk`, a 1.9 s pause, the next; `style`); the HIT (card REACT, ours: a bot on a position that is hit draws once per burst over its tier's row — stays, strafes 2.5 m, rolls aside once it is under way, or moves to a ring point round the target it sees / a spot within 7 m — `react`; no step of a holder goes through a death region, `react_ok`, and a roll or a move ends in a hub of the graph, `in_hub`); the position, fight, reaction, grenade, lost-bot and dodge tests · `think`, `Tick`, `lost_jump`
- `core/src/ai/arsenal.rs` 746 — the weapon a bot on foot fights with, and its grenade (card GRAI, `analysis/re-grenades.md` 7): the PS2's choice over every slot once a think (ammunition, the AI's target-kind mask, the range band, plenty of ammunition, the score — PROVISIONAL), the grenade test (a 3 s window in 30 per bot, a target in a group of its own side, BF2's difficulty roll), the explosive claim (one thrower per target and team), what the pick means for the pad, the low-arc ballistic aim of a throw; a target that rides is its VEHICLE to the choice (card HVYW, `analysis/re-soldier-ai.md` 22: the hull's kind against the mask, the PS2's "too weak" rule with its exemption by ordnance - the launcher for a tank, no rifle at one), and OURS: a heavy weapon (the launchers, the wrist rocket) at a soldier only in a crowd of three, a bot holding one prefers the vehicle as its target, a vehicle nothing it holds is for is spared only past 10 m, the standing fight is for a weapon with the hull in its band (a grenade is thrown in passing) · `think`, `choose`, `score`, `against_vehicle`, `is_crowd`, `vehicle_bonus`, `grenade_forced`, `in_window`, `is_group`, `armed`, `Armed`, `lob`, `lob_aim`, `LOB_PITCH`; card SNPR's `sidearm` (a sniper's pistol inside the rifle's MinRange or for a target within 25 m and coming on, kept to its reach)
- `core/src/ai/arsenal/tests.rs` 1664 — the solver, the window, the group test, BF2's roll, the mask, the choice's preferences, a bot's throws in its window landing by the target; card HVYW's: the too-weak rule per game and ordnance, the choice for a vehicle, a heavy weapon's crowd, and five scenes (a rocket at a tank in range and line, at a 4-man group and not at a lone soldier, none with a friend walking into its line, no rifle at a walker, who takes which target), the rifle at a vehicle within 10 m, no fight position for a launcher out of its band or a grenade, no standing for a throw at a vehicle that cannot be made, the stand for a throw at soldiers from the tick the grenade is picked
- `core/src/ai/snipe.rs` 339 — card SNPR (OURS, the maintainer's sniper with a purpose): a bot with a scoped rifle picks the Snipe hint (else a high hub) within 60 m its route reaches with the best lane onto enemy ground (enemy posts and their hubs in sight from 15 m to its sight, weighted by distance), one sniper a spot, walks there, holds it (behaviour Overwatch, a reason to stand) while an enemy has been seen in the last 20 s, then picks again with that spot excluded · `is_sniper`, `pick`, `lane`, `upkeep`, `holds`, `arrive`, `walk_failed`, `SNIPE_REACH`, `SNIPE_QUIET`, `SNIPE_SPACING`
- `core/src/ai/snipe/tests.rs` 341 — the long-lane hint over a nearer short-lane one, two snipers on one tick take two spots, the spot left after 20 quiet seconds and kept while an enemy is in sight, the pistol at 15 / 22 m and the rifle past its reach, a perch 2.5 m over the floor given up from under it
- `core/src/ai/gunner_foot.rs` 1969 — the gunner of a bot on foot, the ONLY writer of its look sticks and FIRE (card SPL11): the aim point at a seen enemy - BF1's lead point offset by the shooter's tracked error (`track_error`; PROVISIONAL: the PS2's BF1 has none, card CLSE2B), BF2's offset by the TARGET's own error block (`unit::AimBlock`) through `radius` (size, stance, speed, cap) along the shooter's tumbling direction (card CLSE2) - BF2's scaling of the error vector by target, range and the weapon class's AIBubble keys (cards CLSE, CLSE2: `error_vector`), the aim tracker and the slew, the bursts and pauses by the weapon class's Barrage keys under each game's own rule and the trigger's `MaxRange` test (card CLSE4), the charge, the tap of a single-shot weapon (card STTAP: `tap`), no shot with a friend in the line of fire (card FFLN: `friend_in_line`; card HVYW: a missile's line is wider by what the shooter and the friend walk in its wind-up and flight; card RKEX: `friend_in_blast`, no exploding shot with a friend within its blast of the target), the AI sniper table's spread; the aim and fire tests · `run`, `AimRequest`, `GunReport`, `yaw_toward`, `seen_aim`, `fire_discipline`, `track_error`, `radius`, `stance_factor`, `range_table`
- `core/src/ai/gunner_foot/tests/radius_rows.rs` 89 — 80 rows of PS2 RAM (BF2 Kamino, units at rest: size term, stance word, error, visibility, cap, radius) that `gunner_foot::radius` must reproduce (card CLSE2)
- `core/src/ai/mover.rs` 1240 — the mover of a bot on foot, the ONLY writer of its move sticks, ROLL_STANCE and JUMP (card SPL11): the commitment to a destination and the arrived latch (card MOVR: a bot that arrives stands, a request that barely moves is ignored), arrival at a post or a defence spot on the spot's own floor (card STEND:
  `off_floor`, `SPOT_FLOOR_TOL`), the walk block (the side-step offset, the steering's heading picked every third tick and held, the short step to a held position, the last step's scale, a lost bot's jump), the routeless dodge's strafe, the stance controller, the roll; then the progress books — the side-step countdown and the stuck window (a window with no progress is a failed walk), a lost bot's return window, the last resort's watch (`stall`); it answers the walk's outcome; the arrival, deadband, blocked-walk, think-phase, short step and hold range tests · `run`, `MoveRequest`, `Moved`, `stuck_window`
- `core/src/ai/ride.rs` 766 — a bot and a vehicle (card VAI): the boarding claim (the pilot seat of a hover or a
  walker, or a teammate's armed gunner seat, VTAI) and the walk to it (WAI2: over the graph, a claim lost to another
  bot, a command walker claimed whatever the trip), the seated bot's dispatch (a gunner, a walker's or a hover's
  pilot; a building turret's gunner is `man`'s, card TURR), the wait for a gunner, the drop hub (WAI); the
  board-drive-dismount and gunner-claim tests
  · `claim`, `claim_seat`, `board_goal`, `board_point`, `command`, `drive`, `board_reach`, `at_drop`
- `core/src/ai/man.rs` 563 — a bot in a building turret (card TURR, `analysis/bot-design.md` 6.1, D11; ours): the
  purpose test (attack: an enemy the team knows of in an arc, in reach, the muzzle's line clear; defence: a turret by
  a front-line post with a known enemy near it), the offer pass (the nearest free bot within the PS2's 50 m, one a
  turret, two a post, none in the endgame), the five exits and the turret closed after leaving or an unreached walk
  · `offer_pass`, `purpose`, `hittable`, `defends`, `front_line`, `known`, `arc_holds`, `arc_of`, `exit_rule`, `endgame`,
  `seated`, `is_turret`, `seat_gun`
- `core/src/ai/man/tests.rs` 923 — the offer's two clauses with their controls, the post's cap, the nearest bot,
  each exit, the turret closed after leaving and after an unreached walk, the turn to what the team knows, a turret
  boarder walking past a near enemy; the gunner's aim (TAIM, merged): Mygeeto's recoilless turret hitting a soldier
  40 m out, the target inside the arc, the sight past its own building, a turret on a tower hitting a soldier below
- `core/src/ai/hover_drive.rs` 520 — a bot at a hover's pilot seat (card VDROP, re-hover-ai.md): the PS2's stick law
  (the throttle / strafe pair or cos(error), the squared turn, the fade inside ForwardSpeed / 2, the obstacle probe's
  shifted goal and halved throttle, the unstick manoeuvre with BF1's 4 s / BF2's 15 s cooldown, BF2's slow-turn
  factor), the goal |v| + 0.2 m ahead on the path segment, the path follower's stuck test, raw sticks through
  `RideState::hover`, and the driver around it (the probe's ray, the dismount at the post, stuck or routeless)
  · `sticks`, `Class`, `Body`, `Order`, `look_ahead`, `stuck`, `probe_ray`, `drive`
- `core/src/ai/hover_drive/tests.rs` 312 — the law against 347 PS2 RAM rows (four sticks and the unstick state);
  the two bikes' pairs, the probe, the manoeuvre's states, the look-ahead and the stuck test by hand
- `core/src/ai/hover_drive/tests/ps2_rows.rs` 381 — the 347 rows (numbers read from PS2 RAM, card HAIS's ps2_rows.csv) · `ROWS`, `BRANCHES`
- `core/src/ai/walker_drive.rs` 346 — a bot at a walker's pilot seat (card WAI, re-walker-ai.md): the PS2's stick
  law (turn in place to 5° then walk; the strafe branch inside 30° for a bit-0 class; stopped), raw sticks through
  `RideState::sticks`, and the driver around it (an armed walker stands and fights at its stop; WAI2: a command
  walker's pilot stays in, a tauntaun rides on from its drop hub to the post) · `sticks`, `Class`, `Body`, `drive`,
  `ARRIVE`, `COMMAND_ENDGAME`
- `core/src/ai/walker_drive/tests.rs` 419 — the law against 145 PS2 RAM rows; a bot's AT-ST,
  BF1-disc class and spider on flat ground (convergence, 5.0 m/s, the spider's 5° crab; a BF1-map spider, e0c 0,
  under 2°, card WALKB1); the ride to the post
- `core/src/ai/walker_drive/tests/ps2_rows.rs` 155 — the 145 rows (numbers read from PS2 RAM, the RE session's ps2_rows.csv) · `ROWS`
- `core/src/ai/gunner.rs` 200 — a seated bot's seat gun (card VTAI): the look stick toward its SEEN target (D10's
  aim point; a walker pilot's at the walker's own rates, WAI; a turret gunner with nothing seen at its idle point,
  TURR), FIRE under on foot's reaction / range / burst gates and a barrel cone; the point it returns is a building
  turret's aimer's (`ManState::aim`, the PS2's AI seat) · `aim`, `Look`
- `core/src/commander.rs` 2509 — each AI unit's kept target post: the PS2's Conquest goal (the draw by post value, the
  re-draw triggers per game; `analysis/re-conquest-goal.md`), defend hints, the cap of defenders a post and one defender a hint (card DEFS, ours) ·
  `TeamPlan`, `plan`, `draw_post`, `defend_cap`, `post_threatened`
- `core/src/perception.rs` 1259 — contacts, view cone, LOS, target choice (a building turret's gunner sees from its
  muzzle and past its own building, and chooses inside its arc: card TURR); what a friend within 20 m sees is heard
  (card TEAM: `tell`, a tier column, held back on every tier) · `scan`, `tell`, `choose_target`, `choose_target_with`, `los_seat`, `past_buildings`
- `core/src/skill.rs` 617 — difficulty tiers (how quickly a bot runs from a grenade, card HOLD; what a bot on a position does when it is hit, card REACT: `Tier::react`), the skill vector · `Difficulty`, `Skill`, `Tier`
- `core/src/squad.rs` 1060 — BF1's four squad orders on bots (card SQACK, `analysis/re-squad-orders.md`): the press marks the units in reach, each takes it at its next think and answers; follow (two per leader), hold, the release · `Order`, `Pending`, `Command`, `steer`

## game — the binary and its library
- `game/src/main.rs` 533 — the binary's CLI: its `//!` header lists every flag and `OPENBF_*` variable (bar two test hooks), `main`
  parses them, and `ViewerArgs` and `BattleSetup` carry them to the modes in the three parts below (queue SPL2; the
  binary's root owns `game/src/`, the library's directory, so the parts load by `#[path]`)
- `game/src/main/battle.rs` 624 — the battle every mode builds and shows: the map load and upload, the teams, the
  `WalkSim` setup (the pads' class models kept for the viewer), the marker/target/bolt/post boxes, the squad icons, the HUD and spawn-screen models ·
  `make_walk`, `walk_draws`, `hud_model`
- `game/src/main/headless.rs` 828 — the offscreen modes: `--bench`, and `--headless` (a captured frame, a
  `--walk-replay` — or a `--ride-script`'s log — with its PNG series and CSVs, `--capture-orbit`, `--scene-report`) · `run_bench`, `run_headless`
- `game/src/main/windowed.rs` 1939 — the windowed game: the `App` state machine (the shell, the loading screen, the
  battle's per-frame loop, reading each stepped tick's events in `advance_with`'s hook) and the glue only a window uses — the pause menu, cheat words, the
  profile autosave, the dev-mode black box's hooks (F9, quit, the stuck alarm), the battle's audio out (event sounds,
  engine loops, foley, music takes: `play_event_sounds`, `start_take`) · `run_windowed`, `run_shell`
- `game/src/main/windowed/front.rs` 231 — the shell's front screens (card BSHLA): `FrontShell` (the PS2 screens' state, the movie, the shell music, the Instant Action setup and the settings page over them) and `App::redraw_shell` (input incl. the keyboard's bits, the flow, the movie frame into the HUD pass, the draw) · `FrontShell`
- `game/src/lib.rs` 46 — module roots
- `game/src/assets.rs` 1682 — a baked map dir → CPU scene; the texture tier; a geometry's collision bodies and
  their subsets (the map's world, a vehicle class's hull, VHULL) · `load_scene`, `Scene`, `TextureTier`,
  `mode_layer_index`, `body_layers`, `hull_parts`
- `game/src/viewer.rs` 56 — the facade: the XFRM transform convention, and a re-export of every item in the
  parts below, so no `crate::viewer::X` path names a part (queue SPL1)
- `game/src/viewer/look.rs` 1286 — a map's look as render state: the water's tint, fog and reach, the clear colour,
  the fog scale, the far cull and `LOWR` range (both flagged), BF2's far fade, the lighting, the water's speckle and its
  VU1 overlay constants · `water_look`, `speckle_look`, `water_shading`, `far_cull`, `lowres_range`, `apply_lighting`
- `game/src/viewer/scene.rs` 1941 — the scene upload: terrain draw chunks, the `LOWR` sheet, the water, the grass and leaves, instanced
  models (a door's moving submeshes posed by `pose_doors`; an armed building's node parts recorded for `turret_pose`;
  a droid's draws for `stations`;
  the foliage's path props and models; a geometry's LOWD
  extension past `NearSceneRange[1]`), sky shells, the texture cache, the untextured census, the speckle flipbook · `upload_scene`, `SceneGpu`, `SpeckleAnim`, `sky_wrap_v`
- `game/src/viewer/foliage.rs` 222 — the terrain foliage's model instances stood on the terrain (height, tilted up,
  yaw, scale), their tint and fade · `records`, `models`, `matrix`
- `game/src/viewer/grass.rs` 467 — the grass patches (card FOLGG, re-foliage §6–§7): each class's sprite template
  built as B1 `FUN_001ae120` does (the PS2's LCG, seeded per class), each patch stood on three terrain samples, the
  world instances of a grass class · `template`, `patch_matrix`, `build`
- `game/src/viewer/leaf.rs` 505 — the leaf patches (card LEAF, re-leafpatch §4–§5): each class's sprite template built
  as B1 `FUN_001b38f8` does (half-ball, box, the branching vine; RAM-reproduced for 23 classes), the objects — prop
  attachments, world instances, FOLG `LeafPatch` records — with their frame and tint · `template`, `objects`,
  `object_matrix`, `build`
- `game/src/viewer/upload.rs` 328 — GPU upload primitives: per-submesh and skinned meshes, textures, baked `.ktx2`
  files, the unit cube, billboard quad and effect sprite · `upload_submeshes`, `upload_skinned`, `upload_texture_file`
- `game/src/viewer/pickups.rs` 153 — the items on the field drawn (card PWUP, re-powerups §4): each `Sim::pickups`
  entry as its class's model from `common_<game>/models/`, upright at the item, uploaded on first use; BF1 hides it
  for its last 2 s; drawn through `VehicleVisuals::draws` so every draw site has it · `PickupVisuals`, `item_drawn`
- `game/src/viewer/vehicles.rs` 513 — the battle's vehicles drawn (card VDRAW): each class's LOD0 found in the map's
  bake, then every side's vehicle bake, its textures across them all; every live `Sim::vehicles` entry drawn rigid at its
  sim transform through the world path — a walker with a baked skin posed, CPU-skinned into per-walker dynamic
  meshes (WSKN) · `VehicleVisuals`, `VehicleSides`, `find_model`, `find_texture`
- `game/src/viewer/stations.rs` 479 — the health / ammo / vehicle droids drawn moving (card PWST slice 2,
  re-powerup-station §6 `[O]`): the PS2's wait / turn / serving machine per station off the sim's `serving`, its own
  LCG; the idle turn about +Y, the gonk's rock and bob, the FX-7's arm swing and socket spin over the baked rig ·
  `StationAnimState`, `StationNodes`, `pose`, `StationDraw`, `SceneGpu::pose_stations`
- `game/src/viewer/turret_pose.rs` 652 — a building turret drawn turned to its aim (card VTURD, re-turret-draw 2 `[O]`):
  the PS2's three node matrices (the `TurretNodeName` node from the look, the `AimerNodeName` node along the aimer's
  direction, a barrel slid back for a quarter second after its shot) over the baked rig, each part at
  `instance · node · bind⁻¹`; the scene's turret records; where a seated gunner hangs (`L x node x O`) and the
  nine-pose numbers (card VTURG, re-turret-draw 3 `[O]`) · `turret_node`, `aimer_node`, `barrel_node`, `pose`,
  `part_matrices`, `PoseSpec`, `TurretDraw`, `SceneGpu::pose_turrets`, `pose_turrets_at`, `GunnerHang`,
  `nine_pose`, `RiggedTurret`, `rigged_turrets`; the zoomed gunner's own view without his building (card VTURZ,
  re-turret-draw 4) · `SceneGpu::hidden_for`, `view_draws`
- `game/src/viewer/walker_pose.rs` 287 — a walker's pose from its anim state (card WSKN, re-walker §5, §6, §10): the
  entry's clip at the player's time, each leg pair's `_leftup`/`_rightup` layer by P+0x120 over its layer bones'
  subtrees, bone 0's 2h, `bone_root`'s pitch, roll and SkeletonRootScale; one-bone CPU skinning; a node's matrix
  now for a rider (card VRIDE) · `WalkerRig`, `node_now`, `skin`
- `game/src/viewer/camera.rs` 199 — the golden pose, a pose as a camera, the XFRM instance transform, the
  auto-framing · `GOLDEN_CAM`, `instance_matrix`, `overview_bounds`, `look_at`
- `game/src/walk.rs` 1705 — play mode: the player in `core::Sim` (its handle follows the slot through a respawn), the chase camera (the PS2's ideal camera, tilted below the aim, AIMA)
  and the reticle's place on screen, the zoom's projection (S and the FOV eased as the PS2's, card ZOOM), a rider's seat camera (VSEAT, PROVISIONAL; turned by a gun seat's look,
  VTUR; a building turret's zoomed view at the eye point with the class's `FirstPersonFOV`, card VTURZ) · `WalkSim`,
  `vehicle_camera`, `turret_scope_camera`, `ps2_reticle_y`
- `game/src/ride.rs` 134 — a scripted ride (VSEAT): walk to the nearest boardable vehicle, enter, drive 200 m,
  exit, recorded as an `InputLog` (`--ride-script`) · `script`, `Ride`
- `game/src/layout.rs` 832 — posts, spawn paths, nav graph from the manifest · `MapLayout`, `nav_graph`
- `game/src/mission.rs` 772 — mission RON → teams, tickets, spawn timing, the sound sub-level the script loads (`sound`, card SNDE), the start-of-round `plnp` blocks and `DisableBarriers` calls, the script's voice-over and music setters, the camera shots (card SSELB); which of a map's missions two armies run and the player's team in it (`pick`, card ERAM) · `MissionDef`, `MusicCalls`, `Pick`
- `game/src/roster.rs` 1328 — side classes → factions and unit specs; a class's other weapon slots built for the
  sim and registered (card GREN) · `FactionRoster`, `weapon_from_db`, `thrown_weapon_from_db`, `held_slots`,
  `ClassRoster::register`, `laser_look`, `flash_look`, `impact_look`, `explosion_look`
- `game/src/shell_movie.rs` 230 — the shell's background movie (card BSHLA): the baked frame packs played by property, in order and looped, each frame decoded (zune-jpeg) on a worker thread · `ShellMovie`, `load_index`, `shell_dir`, `decode`
- `game/src/classdb.rs` 533 — `classes.ron` with BASE inheritance, a map's own beside the sides, a name matched
  ignoring case (`resolve`); a soldier class's weapon slots the PS2's way (`weapon_slots`: the parent's copy, then
  `WEAPONSECTION` / numbered keys) · `ClassDb`, `WeaponSlot`
- `game/src/vehicle_class/walker.rs` 844 — a walker class (card WALK, re-walker §11): ctor defaults, the setter's
  conversions (tan of the three angle props, sin(TurnThreshold/2), the flag bits), the leg pairs' foot prims, the
  building prims, the turret setter's first refusal (`TURRET_CLAIMS`); a BF1-disc walker takes BF2's body values
  (card WALKB1, re-walker §13.9: `bf1_body`; the kaadu's gait from its clip; `OPENBF_BF1_WALKER_PACE=1` = BF1's
  pace); the census tests · `WalkerClass`,
  `WALKER_BASES`
- `game/src/vehicle_class.rs` 1426 — a hover class's drive/spring props (PS2 ctor defaults; BF2's boost, energy and
  jump), its seat ring from the
  FLYERSECTION / WalkerSection sections, its HealthType, its AISizeType (`ai_size`), whether it goes neutral when emptied, its damage-effect bands and
  `ExplosionName`, the seats' camera and gun props (VTUR), the nodes the drawn turret turns by (VTURD) and the seated
  soldier's pose key and draw rule (VTURG) · `HoverClass`, `seats`, `SeatSpec`, `GunnerDraw`, `PilotPose`, `health_type`, `neutral_when_empty`, `DamageEffect`,
  `damage_effects`, `explosion_name`; the bodies its lines name into the terrain and building subsets and the
  hull bodies they resolve to (HHUL, re-hover §13.2) · `HoverClass::hull`, `hull_names`; a building turret (`armedbuilding`, VTURB): its `BuildingSection` sections,
  a gunner-only ring, a building's HealthType · `TURRET_BASES`, `is_turret`; a flyer class's LandedHeight (VREST,
  re-vehicle-rest §2: the authored number, else minus the baked vis-box min y, in prop order) · `FLYER_BASES`, `landed_height`
- `game/src/vehicle_pads.rs` 1102 — a map's `com_item_vehicle_spawn` instances → `PadDef`s (and each class's
  `GeometryName`): the side table
  (Atk = team 1, Def = team 2), the post's load-time team, the classes absent from the db, a mission's extra side
  files (`mission_side_files`: BF1 NAB1's gun, KAAD; BF2's `bf2_tur.ron` on the 11 maps whose conquest reads TUR, VTUR2); `install` puts them in
  a battle's sim with each class's hover or walker numbers (`walker_bakes`) and seat setup, each seat's guns (weapons, the baked fire points,
  the aimer; VTUR); every armed-building instance with a gunner seat as a `VehicleKind::Turret` at the instance
  (VTURB) with its building's bounds as its acquisition sphere (`building_sphere`, HTGT); each class's hull parts from the map's or a side's bake (VHULL), a hover's hull bodies (`hover_hull`, HHUL) · `build`, `add_turrets`, `install`,
  `seat_setup`, `seat_gun`, `hardpoints`, `hull_parts`, `PadBuild`, `TurretDef`,
  `class_key_team`
- `game/src/collision_layers.rs` 486 — which bodies are in a collision layer · `layer_bodies`
- `game/src/capture_feed.rs` 398 — the HUD's command-post capture icon's rule (card CPBAR, `analysis/re-capture-display.md` §3.1): the first post whose region holds the player while he counts, the line and colours of a neutral / enemy post, the contested blink by the battle clock, nothing on his own post; ours behind `Rules` (his own post while it is being lost, the direction mark from `CommandPost::update`'s sign) · `icon`, `for_unit`, `meter_sign`, `PostState`, `Rules`
- `game/src/camera_collide.rs` 370 — chase-camera pull-in (PROVISIONAL) · `CameraBlockers`
- `game/src/ambience.rs` 338 — the world's fixed sound emitters (card AMBS, re-ambience-music 1-2 `[O]`): `soundstream` / `soundstatic` regions (min = w / divisor, max = mute = w), `com_snd_amb_static` objects (their `MinDistance` / `MaxDistance`), props and buildings whose class has a `SoundProperty` (BF2: at the prop x `SoundOffset`), listed from the battle's manifest and classes; referee `game/tests/suite/ambience_ps2.rs` (the PS2's sound entities in RAM, `game/tests/data/ambience_ps2.txt`) · `Ambience`, `Emitter`, `Kind`, `emitters`, `with_distances`
- `game/src/animator.rs` 1392 — the soldier animator, visual only: two clip players (upper / lower bone masks) into one kept pose, the set of the weapon in hand, the shoot / reload action over the movement clip, BF1's pull and BF2's crossfade, the alert clip rows, the root twist toward a roll's travel (cards ANIML, ANIMA, ROLLS, re-soldier-anim-layers §3, §7, re-roll-picture §7) · `Animator`, `Rig`, `AnimInput`, `ClassAnim`, `RollClock`, `body`, `travel`, `find_clip`
- `game/src/animator/aim.rs` 515 — alert and the aim pose: the alert timer of each game, BF1's layer factor and aim weight, BF2's aim type per clip, the aim's limits and easing, the torso (spine a / spine b / ribcage) or head (neck / head) turned onto the aim line (card ANIMA, re-soldier-anim-layers §3.4, §7) · `Aim`, `AimStep`, `AimKind`, `global`
- `game/src/animator/census.rs` 325 — what the animators hold over a headless round (`ANIM_CENSUS` on `round_to_outcome`): the layered shots, the alert updates and rows, the gun against the aim line, the root twist against a roll's travel per direction (card ROLLS) · `Census`
- `game/src/animator/tests.rs` 971 — the animator on a synthetic rig whose clips say which one a bone was sampled from
- `game/src/skin.rs` 307 — skeleton + pose → joint palette · `Skeleton`, `Pose`
- `game/src/soldier_vis.rs` 2148 — skinned soldier assets and animators; a soldier standing outside the sim with his own animator (`still_draws`, the spawn screens', card SSELB); the hand weapon is the slot IN HAND (card WHELD: `weapon_in_hand`, `DrawPlan::weapon_in_slot`; a HideOnFire throw draws none) (a rider is not drawn, but in a building
  turret that shows him: card VTURG, hung at the seat's hardpoint on the posed node, posed by the seat's clip, no
  hand weapon - `set_turrets`, `drawn`; or on a vehicle seat that shows him: card VRIDE, `O x RiderHang`, a hover
  pilot's nine-pose from the drive's smoothed throttle and turn, an animated-node rider on the walker skin -
  `set_walker_rigs`, `rider_now`); bodies and hand weapons
  cached apart, so classes sharing a body hold their own guns (card UWPN: `DrawPlan`); the held weapon's fire
  point and muzzle flash (card FXMF: the flash model at `hp_weapons` × `FirePointName`, timed on the sim tick) ·
  `SoldierVisuals`, `DrawPlan`, `WeaponAsset`, `FlashModel`, `fire_point`
- `game/src/squad_vis.rs` 237 — the squad-order marker, a dev-mode overlay (off in normal play) · `squad_draws`, `markers_on`
- `game/src/event_feed.rs` 1343 — the PS2's event messages for one viewer (card EVMSG, `analysis/re-event-messages.md`): the sim's events → the lines of BF2's `player1systemMessages` under the PS2's gate (only a human's death; post lines are the port's), per-viewer colours, soldier / team / post names from the baked text, the statistics with BF2's points, and the squad order cross's state (arm, timer, the count of takers; card SQHUD); the spawn screens' texts by key path, by the ELF's hash and a weapon's name (card SSELB); the reticle's pick as the target readout (card HTGT: a soldier's, vehicle's or turret's class name, health); the hit-direction chevron edges a hit on the viewer (or the hover / walker he pilots) lights, in that unit's frame (card HATK) · `EventFeed`, `Texts`, `points`, `order_arm`, `ambush_edge`, `POST_LINES`
- `game/src/event_sounds.rs` 859 — a tick's events as sounds (fire - once per trigger pull, not per bolt: card SNDX, re-weapon-sounds 7.7 - reload, impact, door, the player's squad order line), the units' hurt and death cries (card UCRY: `hurts`' 2 % of the health left, `cries`; `hurt` / `death` log rows), each station's `AmbientSound` ask (card PWST, `station_asks`: the FX-7's chatter), read once per stepped tick (card EVTK; its test counts sounds, effects and remnants against the events at 144 / 60 / 30 / 15 Hz); the dev referee `SOUND_LOG=<csv>` on a headless walk replay, with the take, gain and pitch of every start (`take` rows) and the world's emitters heard at tick 0 and once a second (`ambience` rows: sound / kind / level / gain, card AMBS) · `each`, `cries`, `hurts`, `station_asks`, `fire_sound_starts`, `listener`, `SoundLog`, `sound_names`
- `game/src/foley.rs` 747 — soldier foley (card FOLY, re-foley `[O]`): the battle's sound sub-levels, best first - the mission's own, then the map's (`sublevels`, `battle_levels`; one rule for the library, the foley table and the engine records, card SNDE) - and an other-era army's own sub-level (`foreign_armies`); the battle's sub-level table, the surface under a soldier (terrain / water; the prop under the feet is PROVISIONAL-missing), the PS2's movement clip rules of both games, a step per half clip cycle for a drawn soldier, the stance sounds · `Foley`, `Table`, `Surface`, `clip_bf1`, `clip_bf2`, `View`, `Who`, `WHO`
- `game/src/engine_sound.rs` 377 — a vehicle's engine loop (card VSND, re-vehicle-sounds `[O]`): the record's graphs and factors, the PS2's engine instance (on with a pilot, the `TurnOffTime` run-down, the hover's filtered acceleration, the walker's slip), one instance per vehicle per tick; presentation only · `graph`, `factors`, `Instance`, `EngineSounds`, `EngineClass`, `Voice`
- `game/src/engine_sound/tests.rs` 413 — the AAT's and Utapau's AT-ST graphs inline (rest, full speed, the filter, the run-down, the walker's left end) and the PS2's live multipliers replayed through the baked NAB1 records (data-gated)
- `game/src/dev.rs` 40 — the hidden dev mode (`OPENBF_DEV=1`) · `on`
- `game/src/unit_probe.rs` 563 — one unit's state as a diagnostic row (where it is, its objective and leg, the behaviour that ran and the going-round commitment — card OBST2, `Doing`, `round*` —, the steering's
  pick, what it walked at this tick and what shuts the way along the direction the steering was asked, with the body
  (card BLKF; nothing for a bot that did not walk), the hold's goal / spot / time left / strafe side, the boarding
  target's place, lost / stuck): the sweep's `UNIT_SAMPLES` row and the black box's,
  one field list · `Probe`, `locus_of`, `LEGEND`
- `game/src/blackbox.rs` 2050 — the playtest black box (card BLKB; DEV SCAFFOLDING, one module to delete): a fixed ring of
  the last 60 s (every unit every 0.5 s, the human every tick for 5 s, the world every 2 s, the events, 10 s trails at
  4 Hz, each with its own last tick); the header names the BUILD's commit (`game/build.rs` stamps
  `OPENBF_BUILD_COMMIT` / `OPENBF_BUILD_DIRTY` into the binary) beside the checkout's HEAD; the sounds in their own
  ring (card SNDB: every one-shot the mixer was handed in 60 s - `SoundLib::handed` - and `sound_digest`, the digest's
  section with an OVERLOAD line per tick whose starts sum over full scale); schema 3, a schema 1 or 2 file still reads; written as JSON to `artifacts/playtest/last.json` (+ `prev1..5`) on F9, quit, a panic and the stuck alarm;
  read-only on the sim, in no hash · `BlackBox`, `Snapshot`, `Meta`, `on`, `write_rotating`, `read`, `SoundRow`, `sound_digest`
- `game/src/fx.rs` 1069 — effects (card FXP2): the baked particle-effect layers looked up map → sides → common,
  the live effects and their submit (with the muzzle flashes sorted in); the spawn hooks (card FXP3, re-fx §7
  `[O]`): an `Impact`'s surface effect (riding a struck hull) and its ordnance's explosion, a destroyed vehicle's
  explosion; the PROVISIONAL placeholder quads when no effect data is baked · `Fx`, `Library`, `LiveEffect`
- `game/src/fx/emitter.rs` 844 — the PS2's emitter runtime (re-fx §5 `[O]`): the LCG, bursts and the lifetime
  budget, BF1's 20-variant cache, spawn, the Transformer's closed forms (the Move quirk), the ANIMATED frame ·
  `Instance`, `Particle`, `Frame`, `Lcg`
- `game/src/fx/draw.rs` 368 — how the PS2 draws them (re-fx §6 `[O]`): the far fade, density thinning and
  sizeMul, each Type's shape (PARTICLE / ANIMATED / BILLBOARD / SPARK / STREAK) · `view_terms`, `emitter_draws`
- `game/src/bolt_vis.rs` 695 — the laser bolts as the PS2 draws them (card FXB, re-bolt §3–§4): per sim bolt its
  drawn line (the weapon's fire point to the sim's impact), length, colours; the impact remnants (a bolt the sim spawns and
  resolves in one tick too) and the FadeOutTime fade; BF2's bolt lights (card FXBL, §5.2) · `BoltVis`, `bolt_draw`
- `game/src/bolt_light.rs` 227 — BF2's omni lights on the terrain (card FXBL, re-bolt §5.3 `[O]`): the height factor
  f, the per-patch gather (box within r, the rank, the first 3) on the disc's 8-cell patches · `gather`, `OmniLight`
- `game/src/point_light.rs` 364 — BF2's local lights as the renderer takes them (card BPNT, re-point-lights `[O]`): the
  authored lights, the EntityLights, the command post's hologram light in the viewer's relation colour, the VU's colour
  packing, a model's sphere, and a battle frame's submit (models + the terrain pass) · `still_lights`, `submit`,
  `command_post`, `model_sphere`, `Relation`
- `game/src/holo.rs` 748 — the command post's hologram (card CPHOL, re-point-lights §3, §10, §15 `[O]`): the class's keys, the state per post (the 2 s fade across a capture, the one-update dips, the pops, the turn) stepped at the PS2's update rate on the sim's clock, the beam and the owner's icon as particle draws in the VIEWER's relation colour, the still render's · `Holos`, `Holo`, `HoloArt`, `holo_draws`, `still_draws`, `switched_on`
- `game/src/water_anim.rs` 1177 — moving water: FFT ocean (whole cells reaching the far pass, nothing past it), foam, bob · `Ocean`, `Foam`, `WaterMotion`, `cells_in_range`
- `game/src/weather.rs` 931 — the map's precipitation and wind (card WTHR, re-weather.md): N particles in one unit cube tiled in 5 m cells, the cell walk and its rain shadows, streaks and flakes as draws, splashes · `Weather`, `Precip`, `Wind`, `Shadow`, `View`, `cell_walk`
- `game/src/sounds.rs` 2672 — runtime sounds: lookup (a name's definition is the battle's own sub-level's, as the PS2 loads the one its mission names; another sub-level's only for a name it does not define - card SNDE, `for_battle`, `load_for`; referees `game/tests/suite/sound_sublevels.rs` against the disc and probe `game/examples/audit_takes.rs`), pick, the PS2's level by distance (`analysis/re-sound-distance.md`); 2-D voice lines with their length; the engine layers' looping voices (`loop_voices`); a `Music` record with its takes' places on the disc (`music`, `MusicTrack`); the one-shots handed to the mixer, kept for the black box when `handed` is on (card SNDB); the world's emitters' voices (card AMBS: a looping voice in range, a disc stream kept from the battle's start, a one-shot re-asked when idle; `ambient_voices`, `ambient_heard`, `ambient_take`); each unit's ONE voice for its cries (card UCRY, `cry`, `cries`, `play_cries`: a cry cuts the one still sounding; per tick only a unit's last cry starts); each station's ONE ambient handle (card PWST, `station_ambient`, `play_station_ambient`: asked every tick while it is not sounding, through the property's draw); a unit's line for the voice queue through BF1's stream gates (card UCALL, `ask_line`: one take drawn, silent inside its own lock, then the property's interval and chance, then the level; `line_secs` times a queued line without a mixer) · `SoundLib`, `Refused`, `attenuation`, `Handed`, `AmbientHeard`, `AmbientTake`
- `game/src/spawn_page.rs` 290 — the spawn screens' content for the player's slot (card SSELB): the side rows and highlight, the map's objective and legend texts, the class panel's lines as `FUN_00396430` assembles them (Unit Count = the class's live team-mates, `FUN_0036f538`) · `page`
- `game/src/spawn_preview.rs` 519 — the spawn screens' 3-D soldiers (card SSELB slices 3-4 and 6, re-spawn-flow §2, §4.1): the side select's highlighted team's first class standing in the camera shot, placed in the camera's frame to cover the PS2's pixels [M], shrunk about the eye, tinted by the UI model's colour word, dimmed with the backdrop; the class select's line-up on the PS2's six-place ring with its one-third-second slide, the UI scene mapped into the camera's frame by a perspective fitted to the PS2 [M] · `shown`, `stands`, `place`, `place_at`, `lineup_at`, `ring_place`, `ring_at`, `Preview`, `SLOTS`, `SLIDE_RATE`, `SIDE_AT`, `SOLDIER_YAW` (one turn for both screens), `SIDE_TINT`, `SHRINK`, `LINEUP_SHRINK`
- `game/src/spawn_backdrop.rs` 244 — the spawn screens' level-start view (card SSELB, re-spawn-flow §1.1): the mission's `AddCameraShot` views as cameras (48 deg, no axis flip), the PS2's next-shot draw (`FUN_001b9d00`, on a presentation LCG of its own) and the backdrop that cuts on each screen change until the first spawn · `CameraShot`, `ShotPicker`, `Backdrop`
- `game/src/music.rs` 425 — the battle's music fed from the sim (card AMBM): the mission's music calls + the bank's `Music` records → `core::music`, a frame's changes for the voice and the log; the music volume option (card MUSV: profile `music_volume`, `OPENBF_MUSIC_VOLUME`, 0 = no take); `MUSIC_LOG`'s headless log · `BattleMusic`, `HeadlessLog`, `volume_option`
- `game/src/announce.rs` 742 — the commander fed from the sim: mission lines (keyed by the armies really in the battle,
  not the script's team numbers) + post tables → `core::announcer`, played per frame by `game/src/main/windowed.rs`;
  census probe `game/examples/audit_annc.rs` (a whole round per mission, every line asked and started, against its own
  model of the PS2 script's bleed rate); the units' combat calls (card UCALL: `calls` per tick, `gate_calls` through the
  bank's gates into the queue); card UTALK: the AI's squad talk through the same path, and the low-health voice (a living
  unit at or under its class's threshold asks its `LowHealthSound` while its own handle - `VoiceQueue::holds` - is idle;
  voice-stream lines only); card UVO2: a BF2 battle's unit voice through `crate::vo` (`Commander::vo`, `vo_body`) ·
  `Commander`, `CallAsk`, `battle_lines`
- `game/src/vo.rs` 982 — BF2's unit voice, the class's `VOSound` lines (card UVO2, `analysis/re-unit-chatter.md` 3): the
  PS2's event table, an entry's event and condition word, a class's chain of entries, the context (the object above /
  below, behind / left / right, near / far, its kind and `VOUnitType`), the ranking, the speaker rule (50 m of the
  subject - the player when none is named - an enemy 31 in 101, the subject's nearest team-mate when no speaker is
  named), AcquiredTarget (71 in 101), FriendlyFire, Grenade, the human's killing sprees and the slaughter counters ·
  `Vo`, `VoLines`, `Entry`, `Said`, `Who`, `Body`, `context`, `best`, `class_lines`, `EVENTS`, `ASKED`
- `game/src/profile.rs` 197 — the autosaved profile · `Profile`
- `game/src/render_blend.rs` 377 — render interpolation (card RINT): the windowed frame draws the units' feet and turn, the vehicles' place, turn, gun looks and walker aim and the zoom ease blended between the last two ticks at the accumulator's fraction (a respawn, a death, a seat change or a teleport snaps); written into the walk for the draw lists and restored bit for bit before anything else reads it, so the sim never sees it; `OPENBF_RENDER_BLEND=0` off · `Blend`, `Pose`, `blend`, `fraction`, `lerp_angle`, `UNIT_SNAP_M`, `VEHICLE_SNAP_M`
- `game/src/hud_feed.rs` 387 — BF2's HUD on screen: loads the baked layout, fonts, sheets and meshes (the weapons' own models from the side bakes), runs `ui::hud_tree` / `ui::hud_draw` per frame, paints through `render::hud`; per weapon row (card HUDW) a BF1 class keys BF2's table by its own name, a model the layout does not place gives way to the icon sheet, and a world model's opaque parts are drawn without their sheet's alpha · `HudView`
- `game/src/minimap.rs` 569 — the minimap's feed (card BMAP): the map square from `mapbounds` (else the posts' box + 100 m), the `<world>_map` image, each post's and class's `MapTexture` / `MapScale` / view radii, the mission's north turn, and the per-frame model from the battle (posts, detected units, vehicles); the spawn map page's Spawn view (card SSELB: posts + the team's command walkers, the pick's ring); `still` / `spawn_still` for captures and goldens · `MapStatic`, `ClassIcon`, `bounds_of`, `still`, `spawn_still`
- `game/src/gallery.rs` 1330 — headless map / unit / walkabout sheets (the water stepped to 0 s per eye; the walkabout's soldier on the floor under him, `soldier_spot`) · `map_sheet`, `vantage_camera`, `model_class` (a unit sheet holds its class's own weapon)
- `game/src/bench.rs` 365 — `--bench` per adapter · `Report`
- `game/src/frame_profile.rs` 688 — the windowed game's frame profile (cards ISPD, HITCH): the frame RECORD (dev mode, `OPENBF_FRAME_RECORD=0` off) - a histogram of frame times, every frame over 25 ms with its breakdown (phases, sim ticks run, their AI and hook ms, the image wait, GPU ms, the GPU objects made), the five longest past the first 5 s, a census of the objects made; the black box writes it (`hitches`) and the game prints its summary line at the battle's end - and `OPENBF_FRAME_PROFILE=<csv>`'s row per frame · `FrameProfile`, `Record`, `Hitches`, `FrameRow`, `HITCH_MS`, `SETTLE_S`
- `render/src/lib.rs` 406 — the crate root: the `//!` header (the sky shells' rule and what is not built), the
  `Renderer` struct and what every part shares with it — its render `Target`, the shader's `Globals`,
  `PALETTE_STRIDE` — plus `RenderError`, `TIME_WRAP` and a re-export of every public item in the parts
  below, so no `render::X` path names a part (queue SPL6) but the two public modules `render::hud` and `render::affine` · `Renderer`, `RenderError`, `TIME_WRAP`
- `render/src/lighting.rs` 234 — the map's light as the renderer holds it: the sun, Light2, the hemisphere ambient,
  the fog, the far fade, the flat water's colour constants; the CPU twin of the shader's light sum in GS units (card
  LSUM) · `Lighting`, `DirLight`, `Fog`, `FarFade`, `WaterShading`, `light_sum`
- `render/src/sky.rs` 386 — the sky shells: a shell's material mode (the ELF's rule under the sky flag) and its
  placement (the dome's Y lerp, a `Reflect` shell's mirrored copy, a sky object's offset), `set_sky` and the per-frame
  model matrix; the placement and material-rule tests · `SkyShell`, `SkyMode`, `Placement`, `mirror_squash`
- `render/src/draw.rs` 631 — the draw list a caller hands the frame: a static draw, its pass, its lighting path (the
  PS2's VU1 program, queue LITEB), the foliage's fade, the far scene's model / LOWD switch (card LOD: one dithered
  threshold per pixel; `instance_cull`, `fragment_kept` = the shader's test, `all_discarded` = a draw it discards whole, dropped
  by the frame (card LODP); GPU referee `render/tests/suite/lod_switch.rs`),
  a skinned draw · `DrawCmd`, `DrawLayer`, `LightPath`, `Fade`, `LodSwitch`, `LodSide`, `SkinnedDrawCmd`
- `render/src/device.rs` 356 — the GPU side: the two constructors (the adapter and `OPENBF_ADAPTER`, the window
  surface or the headless texture), the depth target, the mesh and texture uploads, size / resize / idle ·
  `Renderer::new_windowed`, `Renderer::new_headless`, `HEADLESS_FORMAT`
- `render/src/pipelines.rs` 752 — the construction (`finish_init`): the bind group layouts, the shader module and
  every pipeline — opaque, mask, blend, water, the `_fade` twins, the two sky modes, the skinned passes, the bolts,
  BF2's terrain light pass — each a
  `PipelineDesc`; the white texture and the first buffers
- `render/src/frame.rs` 1393 — one frame: the draw-list checks, the globals, the frustum cull, the draw order
  (`draw_key`, `blend_order`, `sort_far_to_near`), the instance and palette buffers, the passes (the sky, the
  opaque / mask runs — interrupted after the terrain's layers for BF2's terrain light pass —, the skinned
  opaque draws, the water runs over them (card WADE), ONE blend pass — world and skinned far to near together, the laser bolts), the overlay; the draw-order tests · `Renderer::render_frame`, `FrameStats`, `OverlayFn`
- `render/src/hud.rs` 443 — the HUD pass (card BHUD2): window-pixel triangles over the frame, blended as DISPLAY values through a non-sRGB view of the frame (`Renderer::display_view_format`), meshes depth-tested against themselves · `HudPass`, `HudVertex`, `HudBatch`, `HudTexture`
- `render/src/grass.rs` 497 — the grass patches' pass (card FOLGG, `[O]`): the per-view sprite buffer in the patch's
  frame (normal sprites facing the camera, flat fronds, the sway), the 20 m fade, far-to-near patch runs; the
  GS blend is PROVISIONAL; the leaves ride its buffers and pipeline · `Renderer::set_grass`, `GrassClass`,
  `GrassPatch`, `GrassSprite`, `sprite_quad`
- `render/src/leaf.rs` 375 — the leaf patches (card LEAF, re-leafpatch §6–§7 `[O]`): the per-view sprite buffer in
  template space (centred squares, the ±0.3 twist, the wind's top-edge sway, the ELF's UV tables), the object fades
  (40 % of MaxDistance, near 6 m, NearSceneRange), blended; BF2 lit on ±Y PROVISIONAL · `Renderer::set_leaves`,
  `LeafClass`, `LeafObject`, `LeafSprite`
- `render/src/light_beam.rs` 320 — the PS2's "lightbeam" renderer (card CPHOL): a hologram's column as the 22-point fan in the plane through its axis that faces the eye, its alphas by the view (1 − g², the eye inside the cone, the far fade), not drawn where its box reaches the near plane; BF2 modulates a white sheet, BF1 draws it untextured · `Beam`, `vertices`, `view_factor`, `fan`, `crosses_near`
- `render/src/grass.wgsl` 78 — `vs_grass` / `fs_grass` (grass and leaves; BF2 leaves' static `Lit` on the vertex
  normal), appended to `shader.wgsl` as one module
- `render/src/bolt.rs` 314 — the laser bolts (card FXB, re-bolt §3 `[O]`): the PS2's ribbon strip (cap, tapered body,
  the nose-on alpha split), each bolt's core (NORMAL) then glow (ADDITIVE), after the blend pass; depth
  tested, no depth write, no fog · `Renderer::set_bolts`, `BoltDraw`, `ribbon`
- `render/src/particles.rs` 456 — the particle effects' shapes per view (card FXP2, re-fx §6 `[O]`: sprite, world quad,
  ribbon; a world-space mesh, the muzzle flash) and the one far-to-near list they share with the bolts, merged into runs · `Renderer::set_particles`,
  `ParticleDraw`, `ParticleShape`
- `render/src/bolt.wgsl` 31 — `fs_bolt_core` / `fs_bolt_glow` (GS MODULATE, unfogged), appended after `grass.wgsl`
- `render/src/terrain_light.rs` 398 — BF2's terrain light pass (card FXBL, re-bolt §5.3 `[O]`): `com_sfx_omniproj`
  projected down from 5 m above each light, per patch its ≤ 3 lights, added in gamma bytes over a copy of the frame
  (the frame is copied after the terrain's layers) · `Renderer::set_omniproj`, `Renderer::set_terrain_lights`,
  `TerrainLights`, `LitPatch`
- `render/src/terrain_light.wgsl` 71 — `vs_terrain_light` / `fs_terrain_light`, appended after `shader.wgsl`
- `render/src/affine.rs` 154 — the PS2's screen-space vertex-colour blend (card BPRL, re-vertex-colour-interp §3–§5 `[O]`): the
  guard band and the per-vertex weight depth the shader's `affine_w` twins, which passes take it, and the referee (UTA1's
  pen floor against the PS2 raster model) · `guard`, `guard_w`, `layer_bit`, `AFFINE_BIT`
- `render/src/point_light.rs` 291 — BF2's local lights on models (card BPNT, re-point-lights §5–§7 `[O]`): the frame's
  light list, the per-view filing, the per-model gather (≤ 4 omnis, 1 spot as a point, prelit skips Static) and the
  shader's table; the term is `point_sum` in `shader.wgsl`, added in `gs_lit`'s sum · `Renderer::set_point_lights`,
  `PointLight`, `LitSphere`, `SpotCone`
- `render/src/filter.rs` 438 — the DomeInfo frame filter (card BFLT, re-glow §3 `[O]`): over a copy of the finished 3D
  frame, before the overlay, frame += Intensity/128·max(0, box(frame) − Threshold) in display bytes, the box one PS2
  half-scale pixel at native resolution, separable (card ISPF: a row pass into a 10-bit target, then the column pass;
  the copy read through a non-sRGB view) · `Renderer::set_frame_filter`, `FrameFilter`, `box_side`
- `render/src/filter.wgsl` 85 — `vs_filter` / `fs_filter_rows` / `fs_filter` (`box_sum`: two texels a linear sample), its own module
- `render/src/specular.rs` 210 — the specular second pass (`DrawLayer::Spec`, queue SPEC, `[O]`): the lookup table's upload and bind group, the per-frame `SpecParams`, and `spec_texcoord`, the tex-gen's CPU twin · `Renderer::set_specular_table`, `spec_texcoord`
- `render/src/capture.rs` 167 — the headless readback and the PNG encoders (with and without the alpha plane); the
  padding and encoder tests · `Renderer::capture_rgba`, `encode_png`, `encode_png_rgb`
- `render/src/shader.wgsl` 1005 — the world shader (the grass, bolt, terrain-light and filter passes add their own
  `.wgsl`; the HUD's is inline in `hud.rs`): the lighting paths (`shade_unfogged`, the light sum in GS units `gs_lit`, `point_sum`), the alpha passes
  (`MASK_CUTOFF`), fog, water and its speckle, sky, the skinned and specular entries, the terrain layers, the far sheet
- `render/src/made.rs` 202 — card HITCH: every GPU object the render crate makes goes through one wrapper here (`buffer`, `texture`, `bind_group`, ...), noting its kind, label and bytes for the frame recorder (`take`, once a frame; off until `enable`) · `Made`, `note`, `take`, `texture_bytes`
- `render/src/mesh.rs` 458 — vertex formats, GPU meshes · `Vertex`, `MeshData`
- `render/src/texture.rs` 303 — RGBA8 → bind group; samplers · `TextureDesc`
- `render/src/camera.rs` 119 — the fly camera (Y-up, −Z forward) · `Camera`
- `render/src/frustum.rs` 175 — frustum culling, a box's distance range from the eye (card LODP) · `Frustum`, `distance_range`
- `render/src/gpu_timer.rs` 206 — GPU ms per section of the frame (card ISPD): `OPENBF_GPU_TIMING=1`, timestamp queries and an async readback ring · `GpuTimer`

## pak — the game's data, on disk or packed (card PACK)
Every runtime read of baked data goes through `pak` (`read`, `read_to_string`, `exists` / `is_file` / `is_dir`, a
sorted `read_dir`, a seekable `open` for the music); a direct `std::fs` read of `assets_baked/` or `missions/` in
`game` or `audio` is a bug the packed game shows as a missing asset. Files the player owns (profile, black box, logs)
stay on `std::fs`. `game`'s `main` mounts `assets_baked` and `missions`: the directory when it exists, else the
archive. `game --write-pack <out> <name>=<dir>...` writes the packed copy (`scripts/pack.py` runs it on a stripped
copy); a packed game starts with dev mode off (`dev::flag`).
- `pak/src/lib.rs` 573 — the archive (`[exe][bodies][index][footer]`, identical files stored once), the mounts and the
  std::fs look-alikes · `Archive`, `write_pack`, `collect`, `mount`, `packed`, `read`, `read_dir`, `open`, `Reader`
- `pak/src/tests.rs` 161 — round trip, re-pack, a damaged footer, ranged reads, dedupe, the disk fallback

## ui, input, audio, net
- `ui/src/lib.rs` 31 — module roots; re-exports `egui`
- `ui/src/hud.rs` 786 — the HUD's model, its event feed for BF2's tree (bars, counts, the event lines, the statistics pop-up, the target's name and health with BF2's target colours, card HTGT) and the egui lines the tree does not carry yet (banner, timer, pad warning) · `HudModel`, `HudWeapon`, `HudTarget`, `HudMessage`, `HudStats`, `message_color`, `target_color`, `feed`, `draw_hud`, `draw_pad_warning`
- `ui/src/hud_tree.rs` 1820 — BF2's HUD interpreted (card BHUD2, `analysis/re-hud.md` §2): the baked tree as elements, their events, the fader, colour ease and pulse, a group's colour tinting its children and the BarBitmap flash (card HTGT, `analysis/re-target-hud.md` §5-6), the `hudtransforms` NameMesh / NumberColor tables, and the MultiLineText message box (card EVMSG: queue, admit, expire, scroll, word wrap) · `Hud`, `Elem`, `Fader`, `BarFlash`, `EventValue`, `MultiLine`, `Line`
- `ui/src/hud_draw.rs` 974 — the HUD laid out: the PS2's frame → window mapping, Hor+ anchors, bitmaps, bars (with their flash copy), segmented rings, glyph runs (with the Shadow style), the message box's lines, meshes → window-pixel triangles in z-order; a spawn screen's veil and page over or instead of the tree (card SSELB) · `draw`, `HudFrame`, `HudArt`, `anchor_shift`, `MESH_UNIT`
- `ui/src/hud_capture.rs` 298 — the capture icon's model and draw (card CPBAR, `analysis/re-capture-display.md` §3.3-4): `hud_flag_timer` as one picture cut at the line into the top and bottom colours, at the PS2's frame box, Hor+ at the right edge; ours: the direction mark (the minimap's pointer) on the line · `CaptureIcon`, `Side`, `Mark`, `Part`, `picture`, `BOX`
- `ui/src/hud_ambush.rs` 318 — the hit-direction chevrons (card HATK, `analysis/re-hud-attacker.md`): the PS2's quadrant rule on the shot's way in the victim's (left, up, forward) frame, the four edges' timers (full, then a linear 1.0 s fade), BF2's `ambush_icon` at the PS2 frames' measured places, Hor+ at the window's side edges · `Ambush`, `Edge`, `edge`, `visible_box`, `SHEET`
- `ui/src/hud_map.rs` 670 — BF2's minimap, the Map element's Small view (card BMAP, `analysis/re-hud-map.md`): the model the game feeds, the world → map rule (eye-centred, heading-up, the class's view radius, posts clamped to the rim), the backdrop's image rect, the post blink, and the draw inside `hud_draw`'s frame; the Spawn view for the spawn map page (card SSELB: the whole map at `PositionSpawn`, the north turn, the `hud_icon_flash` ring) · `MapModel`, `MapView`, `MapIcon`, `heading`, `view_radius`, `post_color`, `window_point`
- `ui/src/hud_squad.rs` 434 — BF1's squad order cross, the `SquadCommandGroup` element (card SQHUD, `analysis/re-squad-orders.md` §2b): the model the game feeds (labels, arm, timer, the port's count of takers), the cross sheet, the chosen arm's highlight, the four `bf1_gamefont_tiny` labels and the 2 s fade, laid over `hud_draw`'s frame at the PS2's measured place, Hor+ at the right edge · `SquadCross`, `Part`, `centre`, `label_anchor`, `CENTRE`
- `ui/src/menu.rs` 589 — pause menu, session cheats · `MenuModel`
- `ui/src/shell.rs` 350 — Instant Action setup · `ShellModel`
- `ui/src/shell_ps2.rs` 594 — the BF2 PS2 shell's front screens in the HUD pass (card BSHLA, re-bf2-shell §2-§4, §9): Press START, Main Menu (Multiplayer and Splitscreen disabled: grey 110, skipped by focus), Singleplayer, Options; the stack's measured layout rule (`stack_layout`), the flow and focus (`ShellMenus::step`), the movie / logo / prompt / help lines · `ShellMenus`, `Stack`, `StackRow`, `RowId`, `Page`, `ShellCommand`, `stack_layout`
- `ui/src/spawn.rs` 506 — the spawn screen's egui panel (the battle without spawn screens, and the fallback with no HUD pass) · `SpawnModel` (post and class selections are the sim's, card SPSEL)
- `ui/src/spawn_draw.rs` 819 — the PS2 BF2 spawn screens in the HUD pass (card SSELB, re-spawn-flow §2-§4, re-bf2-shell §3): the 0.51 veil in display values, the side select's button stack (`bf2_buttons_*`), the class panel (title tab, `border_3_pieces` box, lines), the help lines with the pad icons, the map page (the Map element's Spawn view, the `player1mapkey` legend and the objective line from the layout's numbers); geometry measured on PCSX2 frames, Hor+ anchors · `SpawnPage`, `window_point`
- `ui/src/loading.rs` 72 — the BF1 loading screen · `draw_loading`
- `ui/src/overlay.rs` 188 — egui ↔ winit ↔ wgpu glue · `EguiOverlay`
- `input/src/lib.rs` 646 — gilrs → `PadState`, PS2 layout · `Mapping`, `pad_from_gamepad`, `stick_arms` (the left stick presses the D-pad arms: menus and the spawn map); `open_gilrs` (the only
  way to open gilrs: `input/pads.txt`'s project SDL mappings + gilrs's DB + `SDL_GAMECONTROLLERCONFIG`), `log_pads` /
  `pad_warning` (per-pad resolution log, dead camera/FIRE warning), `TRIGGER_PRESS`; `input/examples/pads.rs` prints the log
- `audio/src/lib.rs` 339 — kira playback through the master limiter; a looping voice with settable gain, rate and pan; a one-shot that can be cut (card UCRY) · `Audio`, `Voice`, `Shot`
- `audio/src/limiter.rs` 170 — the master limiter on the main track (card MIXH): 2 ms lookahead, stereo-linked, ceiling at full scale, 80 ms release; a mix under full scale passes untouched · `Limiter`
- `audio/src/stream.rs` 357 — streamed music (card AMBM): a take's SPU2 ADPCM decoded block by block (stereo: a 16 KiB left block, then a right one) from the bake's byte-for-byte copy of the disc's stream (`manifest::music_body`, card PACK) through `pak::open`, played through kira's streaming sound · `StreamTake`, `TakeReader`, `MusicVoice`, `Audio::play_stream`
- `net/src/lib.rs` 10 — empty until the LAN phase

## Tests, examples, goldens, data
- `engine/ci.sh` = fmt + clippy `-D warnings` + build + `cargo test --workspace`. Unit tests sit in each module's
  `mod tests`; `<crate>/tests/` holds integration tests, which skip without `extracted/` or `engine/assets_baked/`.
  Each file directly under a crate's tests dir is its own binary linking the whole engine (~130 MB written per link), so
  `<crate>/tests/suite/main.rs` gathers game's, render's, bake's and mungefs' integration tests as modules of ONE binary each (card WEARY, SSD wear; a
  test id is `suite <file>::<name>`); a new test file goes there with a `mod` line. Beside game's stay the goldens and
  `walkabout_stats` (found by name by gates.py / goldens.sh) and `texture_tier` / `capture_display_ps2` (they set the
  process-wide texture tier).
- GPU referees: `render/tests/suite/` (one binary, card WEARY), run only by `scripts/gpu_tests.sh` (then `scripts/goldens.sh --check`).
- Goldens: `engine/goldens/maps/`, `engine/goldens/units/` and the HUD / shell / spawn PNGs beside them; referees
  `game/tests/golden_gallery.rs` and the other `golden_*.rs`; drawn by `game/src/gallery.rs`.
  `engine/goldens/walkabout_stats.ron` is per-panel numbers, not a picture: `game/tests/walkabout_stats.rs` renders
  every walkabout at 1/4 and fails the named panel whose mean / σ / flat-grey share moved (`OPENBF_TEST_DROP_TEXTURE`
  in `game/src/viewer/scene.rs` is its test hook); run it by hand, `cargo test -p game --test walkabout_stats -- --ignored`.
- Gate tools are examples: `game/examples/round_to_outcome.rs` (the sweep; `scripts/sweep_seeds.py` runs it over N seeds
  and tables finish rate, stuck_max spread and deaths by cause per map, card SWPN; its `recycle_lv` lines, `FLOOR_GRID`
  and `NAV_FLOORS` show a recycle against the routing's own floors, the floors round a spot and every hub's resolved
  floor, card STLVL; its `endgame` line times a round from the tick a side stands clamped at 3 alive or fewer, and
  `TRACE_UNIT` names a rider's vehicle, card STEND; `BOT_SAMPLES` writes every unit at 4 Hz and the shots / hits /
  kills that `scripts/bot_quality.py` turns into bot-quality numbers beside the PS2's `scripts/bot_quality_ps2.json`,
  `sweep_seeds.py --botq`, card BOTM), `game/examples/arena.rs` (one tier per run, or `vs a:b` / `vs all`: a tier per team through `Sim::team_difficulty`, the higher tier's kill share and paired t over seeds, card ARVS),
  `game/examples/walkabout.rs`, `game/examples/water_frames.rs`, `game/examples/leak_census.rs` (the collision
  leak census, card LEAK: a soldier driven at every wall of every map, runs that cross a face or end enclosed /
  buried / off the map, counted by mechanism; its accepted list `leak_census_accepted.txt`), `game/examples/ordnance_census.rs`
  (the ordnance floor census, card GRFL: 4 grenades on every floor a soldier stands on, the ones that pass it keyed
  by the body crossed; its accepted list `ordnance_census_accepted.txt`), `game/examples/pose_render.rs`
  (the port at a PCSX2 pose, water stepped, no soldier or HUD — `scripts/parity.py`'s PS2 parity score and gate, card PARITY), `game/examples/fx_capture.rs`
  (one particle effect spawned on the ground and rendered over its life from a pose, card FXP2), `game/examples/blackbox_read.rs`
  (the digest of a black box snapshot: the header, the human's last 10 s, the focus unit, blocked bots, events; card BLKB —
  `round_to_outcome` writes one headless with `BLACKBOX_AT` / `BLACKBOX_ON`); census tools beside them (`vehicle_gallery.rs`: one
  side's baked vehicles on a contact sheet; `hover_drive.rs` / `vehicle_ride.rs`: one hover driven, or entered,
  driven 200 m and left, on a real map; `walker_drive.rs`: a human-driven walker's phases, the speed each settles at, the planted foot's slip and a replay, `--frames` its chase view;
  `turret_ride.rs`: one building turret entered, fired and left; `spawn_node_census.rs`: every post's spawn node on every map against the first leg a bot spawned there plans, before and after card DAGS's rule, against its accepted list `spawn_node_census_accepted.txt`, card SPNC) and in
  `mungefs/examples/`. Replays and hashes: `scripts/sim_gates.sh`.
- `engine/missions/` — 57 RONs from `scripts/gen_missions.py`. `engine/assets_baked/` (gitignored) comes from
  `scripts/rebake.sh`; `scripts/scratch_bake.sh` + `scripts/cmp_bake.sh` check it.

## Where do I change X?
| to change … | start in | also |
|---|---|---|
| weapon fire rate, spread, damage | `core/src/weapon.rs` (`WeaponClass::from_props`) | `bake/src/classes.rs`, `game/src/roster.rs` |
| the muzzle flash (model, size, colour, fade, where on the gun) | `core/src/flash.rs` (`MuzzleFlashLook`), `game/src/roster.rs` (`flash_look`), `game/src/soldier_vis.rs` (`flash_draws`, `fire_points`) | `bake/src/soldier.rs` (`weapon_nodes`), `bake/src/alpha.rs` (`flash_textures`), `manifest/src/lib.rs` (`WeaponNodes`), `render/src/particles.rs` (`ParticleShape::Mesh`), `game/src/fx.rs` (`submit_with`) |
| how a laser bolt looks (ribbons, lengths, colours, the impact remnant) | `core/src/laser.rs` (`LaserLook`), `game/src/roster.rs` (`laser_look`) | `game/src/bolt_vis.rs`, `render/src/bolt.rs`, `render/src/bolt.wgsl`, `render/src/pipelines.rs` |
| BF2's bolt lights on the terrain (the pink wash) | `game/src/bolt_vis.rs` (`omni_lights`), `game/src/bolt_light.rs` (`gather`) | `core/src/laser.rs` (`LaserLook::light`), `render/src/terrain_light.rs`, `render/src/terrain_light.wgsl`, `render/src/frame.rs`, `game/src/viewer/scene.rs` (`install_omniproj`), `bake/src/spec.rs` (`bake omniproj`) |
| soldier speed, stances, energy | `core/src/soldier.rs` (`SoldierConstants::from_props`) | `game/src/classdb.rs` |
| a faction's classes | `game/src/roster.rs` | `game/src/mission.rs` |
| which class a bot or a respawning player takes | `core/src/sim/spawn.rs` (`ai_class`, `respawn_slot`) | `game/src/roster.rs` |
| a mission's teams, tickets, timing | `scripts/gen_missions.py` → `engine/missions/` | `game/src/mission.rs` |
| post capture, bleed, domination | `core/src/cp.rs` | `core/src/region.rs`, `core/src/sim/round.rs` (`update_cps`) |
| reinforcements, respawn | `core/src/battle.rs` | `core/src/sim/spawn.rs` (`SpawnWaves`, `respawn_slot`) |
| one tick's order, a new tick stage | `core/src/sim/step.rs` (`Sim::step`) | the header of `core/src/sim.rs` |
| a bolt's damage, a kill, an environmental death | `core/src/sim/combat.rs` (`resolve_bolt`, `kill_environment`) | `core/src/weapon.rs` (`Health`) |
| a class's weapon slots, the two channels, the switch inputs, a thrown grenade (cycle, flight, bounce, stick, fuse, blast) | `game/src/classdb.rs` (`weapon_slots`), `core/src/sim/ordnance.rs`, `core/src/sticky.rs` | `game/src/roster.rs` (`held_slots`, `thrown_weapon_from_db`), `core/src/weapon.rs` (`Thrown`), `core/src/unit.rs` (`HeldWeapon`), `game/src/main/battle.rs` (the HUD's grenade row) |
| pickups: what a class drops, the item's numbers and box, the pool size; the drop, the item, the touch | `game/src/classdb.rs` (`drop_table`), `game/src/roster.rs` (`drop_entries`, `item_class_from_db`, `item_pool`), `core/src/sim/pickups.rs` | `core/src/sim/combat.rs` (`unit_killed`, `kill_environment`: the drop; `buff_scale`), `core/src/unit.rs` (`UnitSpec::drops`, `Unit::buffs`), `game/tests/suite/roster.rs` (`drop_tables_are_the_discs`), `game/src/viewer/pickups.rs` (the draw), `game/src/event_sounds.rs` (`PowerupSound`, the landing), `game/src/fx.rs` (the taker's gain effect) |
| health / ammo / vehicle droids (`powerupstation`): the class numbers, the placed stations, the pass | `game/src/roster.rs` (`station_class_from_db`, `add_stations`), `core/src/sim/stations.rs` | `game/src/vehicle_pads.rs` (`install` places them), `core/src/sim/pickups.rs` (`give_ammo`), `game/tests/suite/vehicle_pads.rs` (`the_showcase_maps_place_their_droids`), `game/src/event_sounds.rs` (`PowerupSound`, `station_asks`: the chatter), `game/src/sounds.rs` (`station_ambient`), `game/src/fx.rs` (the gain effect) |
| a droid's idle turn and serving moves (the FX-7's arms and socket, the gonk's rock and bob) | `game/src/viewer/stations.rs` | `bake/src/turret_rig.rs` (the per-node parts), `core/src/sim/stations.rs` (`StationAnim`, `Station::serving`), `game/src/roster.rs` (`station_anim`), `game/src/viewer/scene.rs` (the draws), `game/tests/suite/station_draw.rs` (BF1 Naboo's 15 droids end to end) |
| an explosion's area damage, a vehicle's death blast, a rocket's impact blast | `core/src/sim/blast.rs` (`Sim::explode`) | `core/src/sim/combat.rs` (`bolt_explodes`), `game/src/roster.rs` (`impact_explosion`, `explosion_class`), `core/src/weapon.rs` (`ExplosionClass`, `OrdnanceClass::explosion`, `blast_falloff`), `game/src/vehicle_pads.rs` (`blast_setup`), `game/src/vehicle_class.rs` (`blast_sphere`) |
| how a bot aims, moves, fires | `core/src/ai/behave.rs` (`think`: what it is doing), `core/src/ai/gunner_foot.rs` (the look sticks and FIRE), `core/src/ai/arsenal.rs` (which weapon, and the grenade), `core/src/ai/mover.rs` (the move sticks, stance, roll, jump) | `core/src/perception.rs`, `core/src/skill.rs`, the constants in `core/src/ai.rs` |
| where a bot walks: its objective, its route leg | `core/src/ai/route.rs` (`retarget`, `plan_leg`; `leg_point`: the PS2's aim along a leg, from the last corner passed) | `core/src/commander.rs`, `core/src/nav.rs` |
| how a bot gets round a wall, a crate, a parked vehicle | `core/src/ai/avoid.rs` (`pick`: the side, the corner, the leave rule, the early turn; `foot_ahead`: a standing vehicle's footprint) | `core/src/ai/steer.rs` (the probes: `wall_ahead`, `floor_ahead`), `core/src/ai/mover.rs` (`stuck_window`: a bot going round is not stuck), `core/src/collision/walls.rs` (`wall_sweep`) |
| which posts the AI attacks | `core/src/commander.rs` | `core/src/nav.rs` |
| AI routes, barriers, where a post or a unit is on the graph | `core/src/nav.rs` | `mungefs/src/plan.rs`, `game/src/layout.rs`, `game/tests/suite/locate_ps2.rs` (locate against 1,004 live PS2 path goals, `game/tests/data/locate_ps2.txt`) |
| blocked route groups (`plnp`) | `core/src/route_patch.rs` | `game/src/mission.rs` (`arc_blocks`), `scripts/gen_missions.py` |
| squad orders | `core/src/squad.rs` | `core/src/ai/command.rs`, `game/src/event_sounds.rs` (the player's line), `game/src/announce.rs` (the units' answers through the voice queue, `Commander::orders`), `game/src/squad_vis.rs` (the dev-mode marker), `ui/src/hud_squad.rs` (the D-pad cross in the HUD), `game/src/event_feed.rs` (the cross's state and its count of takers, `EventFeed::squad`), `game/tests/suite/hud_layout.rs` (the cross against three PS2 frames), `game/tests/suite/squad_orders_ps2.rs` (the PS2's scenes on the port) |
| aim assist | `core/src/aim_assist.rs` | `core/src/sim/input.rs` (`acquire_magnet`, `apply_assist`) |
| the zoom (R3): a weapon's steps, what cancels it, the zoomed turn rate, the shoulder and scope cameras, the zoomed FOV | `core/src/weapon.rs` (`zoom_press`), `core/src/sim/step.rs` (`tick_zoom`), `core/src/unit.rs` (`Zoom`, `camera_section`, `scope_point`) | `game/src/walk.rs` (`step_tick`, `fov_y`) |
| where a human's shot goes, the chase camera's tilt, the reticle's height | `core/src/unit.rs` (`camera_section`, `ideal_camera`, `aim_ray`, `fire_point`), `core/src/sim/step.rs` (`aim_hit`, `human_shot`) | `game/src/walk.rs` (`WalkSim::camera`, `reticle_y`), `ui/src/hud.rs` (`HudModel::reticle_y`, `reticle_y`), `game/tests/suite/hud_layout.rs`, `analysis/re-aim-assist.md` §10 |
| how a soldier follows the ground (ramps, folds, fall by airtime; per game) | `core/src/soldier.rs` (`Soldier::step`, `resolve_ground`, `GroundRules`) | `core/src/sim/step.rs` (3a': contact after the wall pass), `core/src/sim/state.rs` (`ground_under_n`, `Sim::ground_rules`), `core/src/ai/steer.rs` (`steep_ahead`: too steep to climb = a wall for the AI) |
| walking on models, walls | `core/src/collision/walls.rs` (the wall pass), `core/src/collision/rays.rs` (`ray_down`) | `core/src/sim/state.rs` (`ground_under`), `game/src/collision_layers.rs`, `game/examples/leak_census.rs` (the check) |
| BF2 doors (triggers, the open clip, the moving box, the drawn door, the sounds; a bot at a door that is opening) | `core/src/door.rs`, `game/src/assets.rs` (`Scene::collision_and_doors`), `game/src/viewer/scene.rs` (`SceneGpu::pose_doors`), `game/src/main/windowed.rs` (`play_event_sounds`) | `bake/src/door.rs`, `core/src/collision/world.rs` (`move_body`), `core/src/sim/state.rs` (`door_opening`, `opening_leaf_over`), `core/src/ai/steer.rs` (`StopKind::Door`; no steep floor in an opening leaf's box) |
| what a bolt or a line of sight hits | `core/src/collision/rays.rs` (`CollisionWorld::ray`) | `core/src/projectile.rs`, `core/src/perception.rs` |
| the pad mapping | `input/src/lib.rs` (`Mapping`) | `core/src/sim/input.rs` (`PadState`, `buttons`) |
| where the shot came from (the hit-direction chevrons) | `ui/src/hud_ambush.rs` (the rule, fade, draw) | `game/src/event_feed.rs` (`ambush_edge`: which hits, whose frame), `core/src/sim/combat.rs` / `blast.rs` / `vehicles.rs` (`Event::Hit` / `VehicleHit`'s `dir`), `ui/src/hud.rs` (`HudModel::ambush`), `game/tests/golden_hud.rs` (`hud_ambush.png`), `analysis/re-hud-attacker.md` |
| the event messages (who killed you, posts, orders) and the points + kills pop-up | `game/src/event_feed.rs` (the gate, names, colours, points), `ui/src/hud_tree.rs` (`MultiLine`) | `ui/src/hud.rs` (`HudMessage`, `HudStats`, `feed`), `ui/src/hud_draw.rs` (`multiline`), `bake/src/hud.rs` (`bake_locl`), `mungefs/src/locl.rs`, `manifest/src/hud.rs` (`HudLocl`), `game/src/walk.rs` (`post_keys`), `game/tests/suite/hud_layout.rs` (the PS2-frame checks), `analysis/re-event-messages.md` |
| the HUD (BF2's tree, drawn) | `ui/src/hud_tree.rs` (elements, events, fades), `ui/src/hud_draw.rs` (layout, anchors) | `ui/src/hud.rs` (`HudModel`, `feed`, the egui lines), `game/src/hud_feed.rs` (`HudView`: art + per-frame run), `render/src/hud.rs` (the pass), `game/src/main/battle.rs` (`hud_model`), `game/src/roster.rs` (`weapon_hud`), `bake/src/glb.rs` (`hud_model_to_glb`: HUD meshes keep COL0), `game/tests/suite/hud_layout.rs` (the PS2-frame check), `game/examples/hud_capture.rs`, `analysis/re-hud.md` |
| the minimap (where a post or unit sits, the backdrop, the wedge) | `ui/src/hud_map.rs` | `game/src/minimap.rs` (the feed: map square, icons, detection), `game/src/main/battle.rs` (`hud_model`, `make_walk`), `game/src/hud_feed.rs` (`map_point`), `game/tests/suite/hud_layout.rs` (the PS2-frame check), `analysis/re-hud-map.md` |
| menus, shell, spawn screen | `ui/src/menu.rs`, `ui/src/shell.rs`, `ui/src/spawn.rs`, `ui/src/spawn_draw.rs` (the PS2 screens) | `game/src/main/windowed.rs`, `game/src/main/battle.rs` (`spawn_model`), `game/src/spawn_page.rs`, `game/src/spawn_backdrop.rs`, `game/src/spawn_preview.rs` (the screens' soldiers), `game/tests/golden_spawn.rs` |
| the sky dome darkened by the Lightning effect (Kamino, BF1 Yavin 2: shell 0 at 153/255) | `game/src/viewer/scene.rs` (`sky_shell_tint`), `manifest/src/lib.rs` (`ManifestLightning::dome_tint`) | `mungefs/src/fx.rs` (`LightningConfig`), `bake/src/map.rs` |
| the DomeInfo frame filter (the whole-frame lift of every BF1 map and BF2 TAT2) | `bake/src/lighting.rs` (`filter_from`), `render/src/filter.rs` | `mungefs/src/sky.rs` (`DomeInfo`), `manifest/src/lib.rs` (`ManifestFilter`), `game/src/viewer/look.rs` (`frame_filter_from`), `render/src/frame.rs` |
| fog, sun, ambient, sky | `bake/src/lighting.rs` | `game/src/viewer/look.rs`, `game/src/viewer/scene.rs` (sky shells), `render/src/shader.wgsl` |
| how vertex colour and the local lights blend across a triangle (the PS2's screen-space blend, its guard band, the off switch) | `render/src/affine.rs`, `render/src/shader.wgsl` (`affine_w`, `AFFINE_COLOUR`) | `render/src/frame.rs` (the instance's bit, the frame's guard) |
| how a world model is lit (COL0, prelit, unlit, Light2, `Static()`) | `bake/src/glb.rs` (`light_path`) | `render/src/draw.rs` (`LightPath`), `render/src/shader.wgsl` (`shade_unfogged`), `game/src/assets.rs` (`light_of`), `bake/src/lighting.rs` |
| BF2's local lights on models (lamps, EntityLights, the command post's light) | `game/src/point_light.rs`, `render/src/point_light.rs` (`gather`), `render/src/shader.wgsl` (`point_sum`, `gs_lit`) | `bake/src/lighting.rs` (`point_lights_from`), `bake/src/entity_light.rs`, `bake/src/glb.rs` (`model_sphere`), `manifest/src/lib.rs` (`ManifestPointLight`, `ManifestClassLight`), `mungefs/src/light.rs`, `game/src/viewer/scene.rs` (`DrawCmd::lit`), `game/src/assets.rs` (`CpuMesh::sphere`), `render/src/draw.rs` (`DrawCmd::lit`), `render/src/frame.rs` |
| the command post's hologram (the beam, the icon, the relation colours, the fade across a capture, the flicker) | `game/src/holo.rs`, `render/src/light_beam.rs` | `render/src/particles.rs` (`ParticleShape::Beam`), `game/src/point_light.rs` (`submit`: the light × I), `game/src/viewer/scene.rs` (`SceneGpu::holo`), `core/src/cp.rs` (the meters) |
| the terrain's texture layers (which, their alphas, UVs, draw order) | `mungefs/src/terrain.rs` (`patch_alpha`), `bake/src/terrain_glb.rs` (`terrain_layers_to_glb`, `layer_uv`) | `game/src/viewer/scene.rs` (`upload_terrain_layer_chunks`), `render/src/draw.rs` (`DrawLayer::TerrainLayer`), `render/src/pipelines.rs`, `render/src/shader.wgsl` (`fs_terrain_layer`), `render/src/frame.rs` (`draw_key`) |
| how the terrain is lit (unlit, COLR × the per-game GS scale) | `bake/src/terrain_glb.rs` (`gs_vertex_byte`), `bake/src/map.rs` (`source_game`) | `render/src/draw.rs` (`LightPath::Terrain`), `render/src/shader.wgsl`, `game/src/viewer/scene.rs` |
| the terrain foliage (FOLG layers, `prp_`, path props, fade, grass patches) | `bake/src/foliage.rs`, `mungefs/src/foliage.rs` | `manifest/src/lib.rs` (`ManifestFoliage`, `foliage_bin`, `ManifestGrassClass`), `game/src/viewer/foliage.rs`, `game/src/viewer/grass.rs`, `render/src/grass.rs`, `game/src/viewer/scene.rs`, the leaf patches `bake/src/leafpatch.rs`, `game/src/viewer/leaf.rs`, `render/src/leaf.rs`, `render/src/draw.rs` (`Fade`), `render/src/shader.wgsl` (`fade_alpha`, `far_cull`), `core/src/heightfield.rs` (`normal_at`) |
| a particle effect's data (the `fx__` emitters, per layer) | `mungefs/src/particle.rs`, `bake/src/effects.rs` | `manifest/src/lib.rs` (`FxSet`), `bake/src/map.rs` (the effect sheets keep alpha), `scripts/rebake.sh` (side: `bake effects`), `mungefs/tests/suite/fx_lvl.rs` (census + PS2 RAM vector) |
| how a particle effect runs and draws (bursts, the Transformer, the shapes, blends, the sort) | `game/src/fx/emitter.rs`, `game/src/fx/draw.rs` | `game/src/fx.rs` (`Library`, `Fx::submit`), `render/src/particles.rs`, `render/src/bolt.wgsl` (the shared fragment shaders) |
| which effect a hit or a blast spawns (the struck surface, the ordnance's and explosion's names) | `core/src/impact.rs` (`ImpactLook`, `ExplosionLook`), `game/src/fx.rs` (`Fx::ingest_sim`, `Fx::explode`) | `core/src/sim/combat.rs` (`push_impact`, `Event::Impact`), `game/src/roster.rs` (`impact_look`, `explosion_look`), `game/src/vehicle_pads.rs` (`blast_setup`) |
| the water's look | `bake/src/water_glb.rs`, `mungefs/src/fx.rs` | `game/src/viewer/look.rs` (`water_look`, `speckle_look`), `game/src/viewer/scene.rs` (the draws, `SpeckleAnim`), `render/src/draw.rs` (`DrawLayer::Speckle`), `render/src/shader.wgsl` |
| the water's motion | `game/src/water_anim.rs` | `game/tests/suite/water_anim.rs` |
| weather: rain, snow, ash, embers, pollen (the particles, the cells drawn, rain shadows, wind, splashes) | `game/src/weather.rs` | `mungefs/src/fx.rs` (`PrecipitationConfig`, `WindConfig`), `bake/src/map.rs`, `manifest/src/lib.rs` (`ManifestPrecipitation`, `ManifestWind`; the `rainshadow…` regions), `render/src/particles.rs` (`ParticleShape::Streaks`), `game/src/main/windowed.rs` (`rain_splashes`, `tracked_velocity`), `game/src/gallery.rs` and `game/examples/pose_render.rs` (a still's weather), `game/tests/suite/weather_ps2.rs` (PS2 RAM vectors) |
| a side vehicle's model, LODs, chunks, collision | `bake/src/vehicle.rs` | `manifest/src/lib.rs` (`VehicleManifest`), `scripts/rebake.sh` (side), `game/examples/vehicle_gallery.rs`, `game/src/viewer/vehicles.rs` (drawn in a battle) |
| a vehicle in the sim, its seats and riders | `core/src/vehicle.rs` | `core/src/sim/vehicles.rs`, `core/src/sim/seats.rs` (enter / drive / exit), `game/src/walk.rs` (`vehicle_camera`) |
| how a walker moves (clips, turn, legs) | `core/src/vehicle/walker.rs` | `game/src/vehicle_class/walker.rs`, `bake/src/walker.rs`, `core/src/sim/vehicles.rs` (`tick_walkers`), `game/examples/walker_drive.rs` |
| a building turret drawn turned to its aim (which node turns, the barrels' recoil) | `game/src/viewer/turret_pose.rs` | `bake/src/turret_rig.rs` (the per-node parts, the rig), `manifest/src/lib.rs` (`ManifestTurretRig`), `game/src/vehicle_class.rs` (`SeatSpec`: the node names, `PitchTurnFactor`, `BarrelRecoil`), `game/src/viewer/scene.rs` (the parts' draws), `game/tests/suite/turret_pose.rs` (the PS2's own rows, `game/tests/data/turret_pose_ps2.txt`; the drawn pose against the seat's gun), `game/examples/pose_render.rs` (`POSE_TURRET`) |
| a building turret's gunner drawn or hidden, where he hangs, his seat pose (card VTURG) | `game/src/soldier_vis.rs` (`seat_now`) | `game/src/vehicle_class.rs` (`SeatSpec::gunner_draw`, `PilotPose`), `game/src/viewer/turret_pose.rs` (`GunnerHang`, `nine_pose`), `game/src/animator.rs` (`Animator::seat`), `bake/src/soldier.rs` (`SEAT_CLIPS`), `game/tests/suite/turret_gunner.rs` (the census, the PS2's gunner rows `game/tests/data/turret_gunner_ps2.txt`, the draw end to end), `game/examples/pose_render.rs` (`POSE_GUNNER`) |
| a building turret's two views: third person on the seat's look, R3's zoom (the toggle, BF2's stick zoom, the eye on the aim frame, the class's `FirstPersonFOV`, his building left out of his own view; card VTURZ) | `core/src/sim/guns.rs` (`tick_seat_zoom`, `seat_zoom`, `SeatLook::scope_eye`) | `core/src/vehicle/gun.rs` (`GunState::zoom`, `GunSetup::scope_eye`, `ray_eye`), `core/src/sim/seats.rs` (`ride`: the press), `core/src/sim/vehicles.rs` (`unseat_unit`: the pilot change), `game/src/walk.rs` (`seat_camera`, `turret_scope_camera`, `zoom_fovs`), `game/src/vehicle_class.rs` (`SeatSpec::first_person_fov`), `game/src/viewer/turret_pose.rs` (`hidden_for`, `view_draws`), `game/tests/suite/turret_views.rs` (the camera on the look, the own-view hide against a second view, the PS2's zoomed camera on 8 RAM rows), `game/examples/pose_render.rs` (`POSE_ZOOM`), `analysis/re-turret-draw.md` 4.1 |
| a vehicle's rider drawn or hidden, where he hangs, his seat pose (card VRIDE) | `game/src/soldier_vis.rs` (`rider_now`, `step_hover_nines`) | `game/src/vehicle_class.rs` (`RiderHang`, `SeatSpec::animated_pilot_position`, `is_seat_pose_key`), `game/src/vehicle_pads.rs` (`PadBuild::riders`), `game/src/viewer/turret_pose.rs` (`HoverNine`), `game/src/viewer/walker_pose.rs` (`WalkerRig::node_now`), `game/src/viewer/vehicles.rs` (`walker_rigs`), `bake/src/hardpoints.rs` (the bind axes), `bake/src/soldier.rs` (`SEAT_CLIPS`), `game/tests/suite/vehicle_rider.rs` (the census `game/tests/data/vehicle_rider_census.txt`, the PS2's placements, the bike rides `game/tests/data/vehicle_rider_bike_<game>.txt`, the draw end to end) |
| how a walker is drawn (skin, pose, leg layers) | `game/src/viewer/walker_pose.rs` | `bake/src/walker.rs` (`skin_glb`), `bake/src/vehicle.rs`, `bake/src/glb.rs` (`model_to_glb_joints`), `game/src/viewer/vehicles.rs`, `game/src/assets.rs` (`load_glb_joints`) |
| a seat's gun (weapons, aimer, fire point, the seat switch) | `core/src/vehicle/gun.rs` | `core/src/sim/guns.rs`, `game/src/vehicle_pads.rs` (`seat_gun`), `bake/src/hardpoints.rs` (the fire points) |
| how a bot boards, drives and leaves a vehicle, and fires a seat gun | `core/src/ai/ride.rs`, `core/src/ai/walker_drive.rs` (a walker's sticks), `core/src/ai/gunner.rs`; a building turret: `core/src/ai/man.rs` (card TURR) | `core/src/ai/behave.rs` (the dispatch), `core/src/sim/seats.rs` (`ai_use_pass`, `ride`), `core/src/ai/route.rs` (`mover`) |
| where a team spawns (posts' nodes, a command walker's points, the opening wave) | `core/src/sim/spawn.rs` (`spawn_points`, `walker_spawn_points`, `pad_walker_spawn_points`, `player_spawn_posts` / `pick_spawn_post`: the human's spawn-map pick) | `game/src/vehicle_class/walker.rs` (`SpawnPointLocation`), `game/src/walk.rs` (`add_posts`, `opening_wave`), `game/src/main/battle.rs` (`spawn_model`), `game/examples/round_to_outcome.rs` (`SPAWN_CENSUS`) |
| vehicle spawn pads (which class, when, the decay) | `core/src/vehicle_pad.rs` | `core/src/sim/vehicles.rs` (`tick_vehicle_pads`), `game/src/vehicle_pads.rs` |
| where a pad's vehicle stands (the pad's ray; a parked flyer's LandedHeight) | `core/src/sim/vehicles.rs` (`tick_vehicle_pads`, `PAD_RAY`) | `game/src/vehicle_class.rs` (`landed_height`), `game/src/vehicle_pads.rs` (`vis_floors`, `install`), `bake/src/vehicle.rs` + `manifest/src/lib.rs` (`ManifestVehicle::vis_min_y`), `game/tests/suite/vehicle_pads.rs` (the class table) |
| a vehicle's class numbers and seats | `game/src/vehicle_class.rs` | `game/src/classdb.rs` (`load_map`), `bake/src/map.rs` (the map's `classes.ron`) |
| texture alpha, cutouts | `bake/src/alpha.rs` | `render/src/shader.wgsl` (`MASK_CUTOFF`) |
| a soldier's alpha (opaque or blended, never tested) | `bake/src/soldier.rs` (`segment_blends`) | `render/src/draw.rs` (`SkinnedDrawCmd::layer`), `render/src/frame.rs` (the skinned opaque draws, `blend_order`) |
| the specular second pass (which segments, its colour, the table, the tex-gen) | `bake/src/spec.rs` | `game/src/viewer/scene.rs` (the world draws, the table, the BF2 near-scene cull), `game/src/soldier_vis.rs` (a side model's: `part_spec`, `with_spec`), `bake/src/skin_glb.rs` (a soldier's `extras.spec_pass`), `render/src/specular.rs`, `render/src/shader.wgsl` (`vs_spec`, `vs_spec_skinned`, `fs_spec`), `render/src/frame.rs` (`draw_key`, the skinned pass after the skinned opaque run, both before the water) |
| a pass's blend or depth state, a new pipeline | `render/src/pipelines.rs` (`finish_init`, `PipelineDesc`) | its field on `Renderer` in `render/src/lib.rs`, its selection in `render/src/frame.rs`, `render/src/shader.wgsl` |
| the order the frame draws in | `render/src/frame.rs` (`render_frame`, `draw_key`, `blend_order`, `sort_far_to_near`) | `render/src/draw.rs` (`DrawLayer`) |
| where a sky shell sits, how it composites | `render/src/sky.rs` (`Placement`, `SkyMode`) | `game/src/viewer/scene.rs` (the sky shells), `bake/src/lighting.rs` |
| a geometry drawn as its LOWD past the near scene (card LOD: the model to `NearSceneRange[2]`, the LOWD from `[1]`, one dithered threshold; `OPENBF_LOD=0` = off) | `game/src/viewer/scene.rs` (`upload_scene`), `game/src/viewer/look.rs` (`lod_bands`, `lod_enabled`) | `game/src/assets.rs` (`Scene::resolve_lowd`), `render/src/draw.rs` (`LodSwitch`, `instance_cull`, `all_discarded`), `render/src/frame.rs` (the distance drop, `draw_key`'s LOWD rank: card LODP), `render/src/shader.wgsl` (`far_cull`), `render/tests/suite/lod_switch.rs` |
| what the PS2's far scene adds when the switch is off (a LOWD's extra geometry, BF2 UTA1's far wall) | `bake/src/far_lowd.rs` | `manifest/src/lib.rs` (`ManifestGeometry::far`), `game/src/assets.rs` (`Scene::resolve_far`), `game/src/viewer/scene.rs`, `render/src/draw.rs` (`cull_near`) |
| PC / HD textures | `bake/src/tier.rs` | `game/src/assets.rs` |
| soldier animation | `game/src/animator.rs` | `bake/src/soldier.rs`, `bake/src/anim_sets.rs` (the PS2's sets per weapon, baked), `manifest/src/anim_sets.rs` (the file), `game/src/roster.rs` (`class_anim`: a class's family and its weapons' banks), `game/src/animator/aim.rs` (alert, the aim pose), `game/src/soldier_vis.rs` (the weapon in hand picks the set; what makes a soldier alert), `game/src/gallery.rs` (a unit sheet through its set), `mungefs/src/sanm.rs` |
| sounds | `bake/src/sounds.rs` | `game/src/sounds.rs`, `game/src/event_sounds.rs` (which event starts which sound), `game/src/ambience.rs` (the world's ambience streams and loops), `audio/src/lib.rs`, `scripts/sound_bake_check.py` (every baked take against ffmpeg's decode of the disc) |
| a unit's hurt and death cries (card UCRY: a surviving hit over 2 % of the health left, the kill; the class's `HurtSound` / `DeathSound`; one voice per unit) | `game/src/event_sounds.rs` (`hurts`, `cries`) | `core/src/sim/state.rs` (`Event::Hit::health_left`), `game/src/sounds.rs` (`cry`, `cries`, `play_cries`), `game/src/roster.rs` (`sound_key`), `game/src/walk.rs` (`Cries`), `audio/src/lib.rs` (`Shot`), `game/src/main/windowed.rs` (`play_event_sounds`), `game/tests/suite/unit_voice.rs` (the rule, the victim's class, every showcase class's voice and take), `analysis/re-unit-chatter.md` 2.7, 3.6 |
| BF1's combat calls (card UCALL: "there he is" at the tracker's take, "grenade!" from the first AI soldier within 7.5 m of a live grenade once a second, "heard something" when hearing turns a bot; the class's `AcquiredTargetSound` / `FleeSound` / `HeardEnemySound` on the voice queue; never hide / approach / predamage) | `core/src/sim/step.rs` (the write-back's `Event::Call`, `grenade_calls`), `game/src/announce.rs` (`Commander::calls`, `gate_calls`) | `core/src/perception.rs` (`choose_target` returns the take), `core/src/ai/behave.rs` (`NavTick::acquired`, `heard_turn`), `core/src/sim/state.rs` (`Call`, `GRENADE_WARN_DELAY`), `game/src/sounds.rs` (`ask_line`), `game/src/walk.rs` (`Cries::call`), `game/src/roster.rs`, the take locks (`mungefs/src/sound_props.rs` `take_lock`, `bake/src/sounds.rs` `take_locks`, `manifest` `SoundProp::take_locks`), `game/src/event_sounds.rs` (`SoundLog::voice`: SOUND_LOG's `call` / `voice` rows), `game/tests/suite/unit_calls.rs` (every fielded BF1 class's own voice, the Gungans'), `analysis/re-unit-chatter.md` 1, 2 |
| BF1's AI squad talk and the low-health voice (card UTALK: a trooper's "follow me" (0.3) as it sets out for its post and "move out" as it arrives; a pilot that takes a seat with a free gun calls for a gunner (0.3) and every friendly AI on foot within 30 m answers "yes" - they do not board; `LowHealthSound` under `LowHealthThreshold`, the Gungans' 1.1) | `core/src/sim/step.rs` (the write-back's follow / move out), `core/src/sim/seats.rs` (`need_gunner_call`), `game/src/announce.rs` (`Commander::calls`, `gate_calls`) | `core/src/ai/behave.rs` (`NavTick::trooper`), `core/src/sim/state.rs` (`Call`, `FOLLOW_CHANCE`, `NEED_GUNNER_CHANCE`), `core/src/announcer.rs` (`Line::speaker`, `VoiceQueue::holds` / `release`), `game/src/roster.rs` (`cries`), `game/src/walk.rs` (`Cries`, `LOW_HEALTH_THRESHOLD`), `game/src/sounds.rs` (`streamed`), `game/tests/suite/unit_calls.rs`, `analysis/re-squad-orders.md` 5, `analysis/re-unit-chatter.md` 2.8 |
| BF2's unit voice (card UVO2: the class's `VOSound` lines picked by event and context, said only by a unit within 50 m of the event's subject - the player when none is named; AcquiredTarget at the tracker's take, FriendlyFire, Grenade, KillingSpree4 / 8 for a human's kills, Nearby Friendly / Enemy Slaughter; BF2 steps past a locked take) | `game/src/vo.rs` | `game/src/announce.rs` (`Commander::calls`, `vo_body`, `gate_calls`), `core/src/sim/step.rs` (the tracker's take and the grenade warning on BF2 too), `core/src/sim/state.rs` (`Call::Vo`), `game/src/roster.rs` (`cries`: `vo`), `game/src/walk.rs` (`Cries::vo`), `game/src/sounds.rs` (`ask_line_with`), `game/tests/suite/unit_vo.rs` (every fielded BF2 showcase class's lines and unit type against the disc census, the pick by unit type, a take for each), `analysis/re-unit-chatter.md` 3, 5 |
| which sub-level's definition of a sound a battle plays (the era of a commander's line) | `game/src/sounds.rs` (`map_prop`, `find_named`), `game/src/foley.rs` (`sublevels`, `foreign_armies`) | `scripts/gen_missions.py` + `game/src/mission.rs` (`sound`), `bake/src/sounds.rs` (`level`), `game/tests/suite/sound_sublevels.rs`, `game/examples/audit_takes.rs` |
| vehicle engine loops (which record, on / off, what feeds the graphs) | `mungefs/src/sound_props.rs`, `bake/src/sounds.rs` (`engines`) | `game/src/engine_sound.rs`, `game/src/sounds.rs` (`loop_voices`), `game/src/main/windowed.rs` (`play_event_sounds`) |
| footsteps and stance sounds (which row, which surface, when a step starts, who steps) | `mungefs/src/foley.rs`, `bake/src/foley.rs` | `game/src/foley.rs`, `game/src/main/windowed.rs` (`play_event_sounds`) |
| the battle's music (which Music, when, fades and gaps, the streamed take) | `core/src/music.rs` | `game/src/music.rs`, `game/src/sounds.rs` (`music`), `audio/src/stream.rs`, `game/src/mission.rs` (`MusicCalls`), `scripts/gen_missions.py`, `mungefs/src/sound_props.rs` (`find_music`), `game/src/main/windowed.rs` |
| the commander's voice-overs (which line, when, the voice queue) | `core/src/announcer.rs` | `game/src/announce.rs`, `game/src/layout.rs` (`CommandPostDef::vo`), `game/src/mission.rs` (`VoiceDef`), `scripts/gen_missions.py`, `game/tests/suite/announcer_ps2.rs` |
| the BF2 HUD's data (layouts, fonts, the extra sheets) | `bake/src/hud.rs` | `mungefs/src/hud.rs` (record names), `mungefs/src/font.rs`, `manifest/src/hud.rs`, `analysis/re-hud.md` |
| a manifest field | `manifest/src/lib.rs` | its writer and reader; rebake every manifest group |
| a new disc chunk | a new `mungefs` module | a `bake` module |
| a new bake group | `bake/src/main.rs` | an `if want <group>` block in `scripts/rebake.sh` and its name in `REBAKE_GROUPS` there |
| a CLI or `OPENBF_*` flag | `game/src/main.rs` | — |
| the capture icon (the post he stands in, on the HUD) | `game/src/capture_feed.rs` (the rule) | `ui/src/hud_capture.rs` (the draw), `core/src/sim/round.rs` (`Sim::cp_inside`, `counts_in_cp`: one rule with the capture tick), `game/src/main/battle.rs` (`hud_model`), `game/tests/capture_display_ps2.rs` (the PS2's rows; the frame box on screen), `analysis/re-capture-display.md` |
| what a playtest snapshot holds, when it is written | `game/src/blackbox.rs` (the rings, the file, the sound rows' digest), `game/src/unit_probe.rs` (a unit's row), `game/src/sounds.rs` (`handed`: the sound rows) | `game/src/main/windowed.rs` (F9, quit, the alarm), `game/examples/blackbox_read.rs` (the digest), `game/examples/round_to_outcome.rs` (`HeadlessBox`), root `CLAUDE.md` |
| a golden's framing | `game/src/gallery.rs` | `game/src/viewer/camera.rs` (`GOLDEN_CAM`), `scripts/goldens.sh` |
| what the sim hash covers | `core/src/sim/state.rs` (`Sim::hash`) | a declared [SWEEP] re-baseline |
