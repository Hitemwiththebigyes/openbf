//! The bake <-> runtime manifest schema, in one place.
//!
//! `bake` writes `manifest.ron` / `common.ron`; `game` reads them back. Until
//! now each side carried its own copy of every struct — the bake's deriving
//! `Serialize`, the runtime's deriving `Deserialize` — and the two had to be
//! kept in step by hand. They are ONE definition here, deriving both.
//!
//! Two rules this crate exists to hold:
//!
//! * **Field order is the wire format.** RON writes fields in declaration
//!   order, so reordering a struct rewrites every manifest. The order below is
//!   the order the bake has always written; a new field goes at the END.
//! * **Every `#[serde(default)]` is load-bearing.** It is what lets a manifest
//!   baked before a field existed still load. Each one carries the note saying
//!   which bake it keeps readable. Defaults do not affect what the bake
//!   WRITES — serde only consults them when reading.
//!
//! This crate depends on nothing but `serde`: it is data, not behaviour.
//! Anything that needs `bake`'s or `game`'s internals stays there.

use serde::{Deserialize, Serialize};

pub mod anim_sets;
pub mod hud;
pub mod shell;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestInstance {
    pub class: String,
    pub name: String,
    /// 3x3 rotation rows + translation, Z-up PS2 world space as stored.
    pub rotation: [[f32; 3]; 3],
    pub translation: [f32; 3],
    pub props: Vec<(u32, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestRegion {
    pub shape: String,
    pub name: String,
    pub rotation: [[f32; 3]; 3],
    pub translation: [f32; 3],
    pub size: [f32; 3],
    pub props: Vec<(u32, String)>,
}

/// Entity class (entc): the instance-class side of the render-geometry chain
/// instance.class -> class.geometry -> geometry.model, plus what the world
/// needs per class: its collision layers, a door's rig and a vehicle's
/// hardpoints. The gameplay props live in `classes/<game>_<lvl>.ron`
/// ([`ClassDef`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestClass {
    pub name: String,
    /// Parent class (entc BASE), e.g. "prop", "hover", "commandpost".
    pub base: String,
    /// GeometryName prop -> key into `geometries`. None = no render geometry.
    pub geometry: Option<String>,
    /// The class's COLLISION-LAYER properties, `(layer, body)` in entc PROP
    /// order — the order is load-bearing, because a property REPLACES its
    /// layer's subset with the bodies accumulated so far
    /// (analysis/re-collision-layers.md §2).
    ///
    /// `layer` is one of `soldier | vehicle | building | terrain | ordnance |
    /// targetable | vehicle_only` (the seven hashes the two ELFs consume);
    /// `body` is the property's value lowercased verbatim — a primitive NAME,
    /// the literal `collisionmesh` (the `coll` mesh body), or the sentinels
    /// `none` / `clear` meaning "this layer's subset is EMPTY".
    ///
    /// Defaulted for bakes made before the layers were read (a manifest
    /// without it behaves as "no collision property" = every body in every
    /// layer, which is exactly the engine's default).
    #[serde(default)]
    pub collision: Vec<(String, String)>,
    /// The DOOR rig of a class whose BASE is `door` (queue DR2,
    /// `analysis/re-bf2-doors.md` §2–§3 [V][O]): the trigger spheres and the
    /// per-frame motion the `open` clip gives each collision primitive.
    /// `None` on every other class, and then not written at all, so a
    /// manifest with no door is byte-identical to one baked before the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub door: Option<ManifestDoor>,
    /// Every node of the model's `skel` in MODEL space at the bind pose, for
    /// a class whose BASE is a vehicle label or `armedbuilding` (card VTUR):
    /// the seats' `FirePointName`, `TurretNodeName` and `PilotPosition` name
    /// these (`analysis/re-vehicles.md` §5, §8). Empty on every other class
    /// and on a bake from before VTUR, and then not written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hardpoints: Vec<ManifestHardpoint>,
    /// The `leafpatch` objects the class hangs on its model's hardpoints
    /// (card LEAF, `analysis/re-leafpatch.md` §2 [O]): `AttachOdf` +
    /// `AttachToHardpoint`, replayed as the PS2's setter does
    /// (`bake::leafpatch::attach`). Empty on every other class, and then not
    /// written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attach: Vec<ManifestAttach>,
    /// The class drawn IN PLACE of this one until it is built (card VTUR2):
    /// its [`BUILD_ZONE_PROP`]. `None` on every other class, and then not
    /// written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_zone: Option<String>,
    /// The EntityLights the class hangs on its model's hardpoints (card BPNT,
    /// `analysis/re-point-lights.md` §2 [O]): `AttachOdf` naming a class
    /// whose BASE is `Light`, then `AttachToHardpoint`
    /// (`bake::entity_light::attach`). Only lights with `OmniRadius` > 0 —
    /// the PS2 registers no other. Empty on every other class, and then not
    /// written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lights: Vec<ManifestClassLight>,
    /// The node rig of an armed building's model (card VTURD,
    /// `analysis/re-turret-draw.md` 2 [O]) and of a health / ammo / vehicle
    /// droid's (base `powerupstation`, card PWST): every `skel` node's bind
    /// matrix and which node each drawn submesh rides, so the viewer can turn
    /// the gun with its aim and move the droid's nodes. `None` on every other
    /// class, and then not written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turret_rig: Option<ManifestTurretRig>,
}

/// An armed building's or a powerup droid's model as rigid parts
/// ([`ManifestClass::turret_rig`]).
/// The PS2 keeps one whole model-space matrix per named node and draws each
/// segment through its node's (`re-turret-draw.md` 2.4), so a part drawn at
/// the bind pose moves by `node(now) · bind⁻¹`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestTurretRig {
    /// The model's `skel` nodes, in `skel` order.
    pub nodes: Vec<ManifestRigNode>,
    /// Per primitive of the drawn model's GLB (its segments in order after
    /// the bake has cut each soft segment into one per node, bar the empty
    /// ones): the index in `nodes` of the node it is bound to, `None` for a
    /// segment bound to none.
    pub parts: Vec<Option<u32>>,
}

/// One node of a [`ManifestTurretRig`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestRigNode {
    /// As the `skel` spells it.
    pub name: String,
    /// Its parent's index in the rig; `None` for a root.
    pub parent: Option<u32>,
    /// Its MODEL-space bind matrix: the X, Y and Z axes, then the origin.
    pub bind: [[f32; 3]; 4],
}

/// One EntityLight a class carries ([`ManifestClass::lights`]): an omni at
/// the hardpoint's MODEL-space bind origin — its world position is that
/// point through the instance matrix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestClassLight {
    /// The light class, as the ODF spells it.
    pub class: String,
    pub pos: [f32; 3],
    /// The light class's `Color` bytes (never Overbright).
    pub color: [u8; 3],
    /// `OmniRadius`, metres.
    pub radius: f32,
    /// The light class's `Static`.
    pub is_static: bool,
}

/// The entc PROP (fnv 0x97C931C5; its name is not cracked) by which BF2's
/// buildable turrets — `tur_bldg_laser`, `_beam`, `_chaingun`,
/// `_guided_rocket` in SIDE/TUR.LVL — name `com_prop_buildzone`, beside a
/// `com_holo_turret` hologram (0x48542041). The PS2 shows that flat pad, not
/// the turret, where one stands at battle start; their `built` twins
/// (`tur_weap_built_gunturret`, `tur_bldg_built_beam`, …) author neither and
/// stand as turrets. PROVISIONAL [D]: PCSX2 frames of BF2 COR1, TAT2, YAV1
/// (card VTUR2); how an engineer builds one is untraced. Two map classes
/// author it too, in no conquest world: DEA1's `dea1_prop_hack_panel`
/// (→ `dea1_prop_panel`, [H] the same "until built" meaning) and END1's
/// `end_weap_treesmash` (itself). The game applies it on BF2 only: BF1's
/// `com_weap_gunturret` and `com_prop_barrier_02` author it as well, and
/// whether BF1's turrets start built is open (card BZONE1).
pub const BUILD_ZONE_PROP: u32 = 0x97C9_31C5;

/// One attached `leafpatch` object: the class it makes and where, as rows 1
/// (up) and 3 (origin) of the hardpoint's MODEL-space bind matrix — the only
/// rows the leaf object keeps (re-leafpatch §5: yaw and scale are dropped).
/// Its world frame is these rows times the instance matrix (row vectors).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestAttach {
    /// The `leafpatch` class, as the ODF spells it.
    pub class: String,
    /// The hardpoint, as the ODF spells it.
    pub hardpoint: String,
    pub up: [f32; 3],
    pub pos: [f32; 3],
}

/// One `skel` node of a vehicle or armed-building model: its name (as the
/// skel spells it) and its bind-pose origin in model space (+x right, +y up,
/// +z forward — the frame the model is drawn in).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestHardpoint {
    pub name: String,
    pub pos: [f32; 3],
    /// Its model-space bind AXES (X, Y, Z), beside `pos` the bind matrix a
    /// rider hangs by (card VRIDE, `analysis/re-vehicle-riders.md` 3: the
    /// Jedi starfighter's `hp_active` is pitched 22 degrees). `None` for a
    /// weapon's nodes and a bake from before the card, and then not written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<[[f32; 3]; 3]>,
}

/// A hand-held weapon model's `skel` nodes (card FXMF,
/// `analysis/re-muzzle-flash.md` §4.1), baked beside its GLB by `bake
/// weapon` at [`weapon_nodes_path`]: each node's origin in its PARENT node's
/// frame — what the PS2 resolves `FirePointName` to. Every soldier weapon's
/// nodes hang off an identity root, so that is also the model frame the
/// weapon is drawn in. Empty for a model with no `skel`.
pub type WeaponNodes = Vec<ManifestHardpoint>;

/// `<side dir>/weapons/<model>.nodes.ron`.
pub fn weapon_nodes_path(side_dir: &std::path::Path, model: &str) -> std::path::PathBuf {
    side_dir.join("weapons").join(format!("{model}.nodes.ron"))
}

/// A music stream's ADPCM (`baked: false` in a stream index), copied out of
/// its sound LVL byte for byte by the bake (card PACK), so the game streams it
/// from its own data: `<index dir>/music/<lvl stem>_<data offset hex>.bin`.
/// A segment's `offset` is inside this file.
pub fn music_body(source: &str, data_offset: u64) -> String {
    let stem = source.rsplit_once('.').map_or(source, |(s, _)| s);
    format!("music/{}_{data_offset:x}.bin", stem.to_ascii_lowercase())
}

/// A BF2 `door` class's rig, baked from its ODF (`AnimationTrigger`,
/// `AnimationName`, `Animation`), its model's `skel` and the named clip of
/// its `zaa_` bank (`analysis/re-bf2-doors.md` §2 hops 2–5, §3 hops 7–9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestDoor {
    /// The `AnimationTrigger` pairs, ≤ 3, in ODF order — a soldier inside
    /// any of the spheres opens the door.
    pub triggers: Vec<ManifestDoorTrigger>,
    /// The clip's frame count; BF2 plays it end to end in (frames − 1)/30 s.
    pub frames: u16,
    /// Every collision primitive the clip MOVES, by prim name, with its
    /// MODEL-space motion per frame: the delta `D(f) = B(f)·B(0)⁻¹` of its
    /// `PRNT` bone, as (rotation xyzw, translation) — the prim's world
    /// transform at frame f is `instance · D(f) · XFRM`. Frame 0 is the
    /// identity (the door shut, where the prim was authored).
    pub prims: Vec<ManifestDoorPrim>,
    /// Every DRAWN submesh the clip moves (queue DRV): a door model's
    /// segments are hard-bound to a `skel` bone by their `BNAM` and stored in
    /// model space, so a submesh's world transform at frame f is
    /// `instance · D(f)` with `D` its bone's delta, the rule [`Self::prims`]
    /// follows. `submesh` is the index of the model GLB's primitive — the
    /// model's segments in order, bar the empty ones the GLB skips. Empty on
    /// a bake from before DRV, and then not written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<ManifestDoorPart>,
    /// The ODF's `OpenSound` / `CloseSound` (`re-bf2-doors.md` §2 hop 5,
    /// §3): played on the trigger's rising and falling edge. `None` where
    /// the ODF names none, and on a bake from before DRV.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_sound: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_sound: Option<String>,
}

/// One drawn submesh of a door the clip moves (see [`ManifestDoor::parts`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestDoorPart {
    pub submesh: u32,
    /// The segment's `BNAM` bone.
    pub bone: String,
    /// `frames` entries of (rotation xyzw, translation), MODEL space.
    pub motion: Vec<([f32; 4], [f32; 3])>,
}

/// One `AnimationTrigger` pair: the hardpoint (a `skel` node) and the radius.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestDoorTrigger {
    pub hardpoint: String,
    pub radius: f32,
    /// The hardpoint's MODEL-space position at the clip's frame 0.
    pub position: [f32; 3],
}

/// One moving collision primitive of a door (see [`ManifestDoor::prims`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestDoorPrim {
    pub name: String,
    /// `frames` entries of (rotation xyzw, translation).
    pub motion: Vec<([f32; 4], [f32; 3])>,
}

/// Geometry LOD table (gmod). `model` = best-detail modl name (LOD0 first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestGeometry {
    pub name: String,
    pub model: String,
    /// All LOD entries as stored: (tier, modl name), e.g. ("LOWD", "xLOWD").
    pub lods: Vec<(String, String)>,
    /// The LOWD triangles that `model` does not have — the far scene's
    /// extension (card UTAS, `analysis/re-far-lowd.md` §6, `bake::far_lowd`):
    /// a baked model drawn per instance only past `NearSceneRange[1]`. `None`
    /// on every geometry but BF2 UTA1's cliffs, and skipped when `None` so the
    /// other manifests do not change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub far: Option<String>,
}

/// Zero Editor path (spawn paths, AI paths): nodes = (position, quaternion
/// as stored — (w, x, y, z), see mungefs::path).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestPath {
    pub name: String,
    pub nodes: Vec<([f32; 3], [f32; 4])>,
    /// Defaulted: absent in bakes made before path props were read.
    #[serde(default)]
    pub props: Vec<(String, u32)>,
}

/// Game-mode world layer (BF2 maps: per-mode `wrld`s inside `lvl_` wrappers,
/// e.g. tat2_con + tat2_turrets). The runtime merges the layer matching the
/// selected mode into the base world at load (`game::assets::Scene`'s
/// `apply_mode_layer`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestLayer {
    /// Key = first world's name in the wrapper ("tat2_con").
    pub name: String,
    /// Defaulted: absent in bakes made before the world list was recorded.
    #[serde(default)]
    pub worlds: Vec<String>,
    pub regions: Vec<ManifestRegion>,
    pub instances: Vec<ManifestInstance>,
    /// This layer's own AI data — its `Hint`/`BARR` nodes, and its planning
    /// graph when it ships a `plan` of its own. `None` only when the layer
    /// authors none of the three; a layer with hints but no plan is
    /// `Some((plan: None, ...))` and falls back to the base graph. APPENDED
    /// after `instances`, and defaulted so a manifest baked before the nav
    /// data still loads.
    #[serde(default)]
    pub nav: Option<ManifestNav>,
}

/// `source_game` for a manifest baked before the field existed: those are all
/// BF1 (pre-P5).
fn default_game() -> String {
    "bf1".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub map: String,
    /// "bf1" | "bf2" — from the model material record size (BF2 = 25 B),
    /// falling back to "bf2" when per-mode world layers exist. Defaulted to
    /// "bf1" for pre-P5 bakes.
    #[serde(default = "default_game")]
    pub source_game: String,
    pub terrain_name: Option<String>,
    pub sky_name: Option<String>,
    pub terrain_glb: Option<String>,
    pub models: Vec<String>,
    pub textures: Vec<String>,
    /// Textures at least one DRAW composites — DERIVED from the ALPHA RULE v2
    /// verdicts the bake writes on the GLB materials (`bake::alpha`), and
    /// **READ BY NOTHING**. The renderer picks a draw's pass from its
    /// material's `alphaMode`, not from this list, because a texture can be
    /// OPAQUE on one draw and BLEND on another; the field is kept for
    /// manifest compatibility (and so a report can see the rollup at a
    /// glance). Defaulted so a manifest baked before this field still loads.
    #[serde(default)]
    pub transparent: Vec<String>,
    /// Which source filled a texture's classic-plus TIER 2: (name, "pc").
    /// Only textures that HAVE a second tier are listed, so a BF1 bake — and
    /// any bake run without the PC install — writes an empty list. The base
    /// tier is not listed because there is nothing to say about it: every
    /// `textures` entry has a `<name>.ktx2` and it is always the PS2 disc's
    /// own pixels. See `bake::tier`, and [`Manifest::texture_tier2`] for the
    /// override's file name. Defaulted for bakes made before the tier system.
    #[serde(default)]
    pub texture_tiers: Vec<(String, String)>,
    pub classes: Vec<ManifestClass>,
    pub geometries: Vec<ManifestGeometry>,
    pub regions: Vec<ManifestRegion>,
    pub instances: Vec<ManifestInstance>,
    /// Absent in pre-P4 bakes.
    #[serde(default)]
    pub paths: Vec<ManifestPath>,
    /// Absent in BF1 bakes (single world).
    #[serde(default)]
    pub layers: Vec<ManifestLayer>,
    /// The map's own sun/ambient/fog, reduced from its `sky_` and `lght`
    /// chunks by `bake::lighting` (PROVISIONAL — see that module). Absent in
    /// pre-2026-08-31 bakes, which fall back to [`ManifestLighting::default`]
    /// — the engine's old hardcoded look, so they render as they used to.
    #[serde(default)]
    pub lighting: ManifestLighting,
    /// The map's sky dome shells and the low-res-terrain reference. Absent in
    /// pre-2026-08-31 bakes.
    #[serde(default)]
    pub sky: Option<ManifestSky>,
    /// The map's water surface, from `tern -> WATR`. `None` on the 22 ground
    /// maps that ship none. See `bake::water_glb`. Absent in pre-2026-08-31
    /// bakes.
    #[serde(default)]
    pub water: Option<ManifestWater>,
    /// The map's `coll` meshes and `prim` primitives, merged by LOWERCASE
    /// geometry name. APPENDED after `water` so a manifest's existing RON text
    /// is unchanged up to this point; defaulted so a pre-collision manifest
    /// still loads.
    #[serde(default)]
    pub collision: Vec<ManifestCollision>,
    /// `tern -> CUTS -> CUTR`: one entry per cutter, plane order as read. A
    /// point inside a cutter has no terrain under it (the PS2's height query
    /// returns its −300.0 sentinel — analysis/re-terrain-heights.md).
    #[serde(default)]
    pub terrain_cutters: Vec<ManifestCutter>,
    /// The authored AI data: the base world's planning graph (`plan`/`plnp`,
    /// when it has one) plus the `Hint` and `BARR` nodes of every unwrapped
    /// `wrld`. APPENDED
    /// after `terrain_cutters` so a manifest's existing RON text is unchanged
    /// up to this point; defaulted so a pre-nav manifest still loads. The
    /// runtime builds the sim's nav graph from it (`game::layout`).
    #[serde(default)]
    pub nav: Option<ManifestNav>,
    /// Classic-plus tier 2: `(texture name, file)` for every texture the bake
    /// wrote a higher-resolution override for, the file relative to
    /// `textures/` as its base `<name>.ktx2` is — e.g.
    /// `("sand", "sand.tier2.ktx2")`. Both files are always present together
    /// and draw the same silhouette (`bake::tier`); which one the renderer
    /// binds is the maintainer's runtime toggle (PLAN §Classic-plus assets), and the sim
    /// reads neither. APPENDED after `nav` so a manifest's existing RON text
    /// is unchanged up to this point; defaulted so a single-tier bake still
    /// loads.
    #[serde(default)]
    pub texture_tier2: Vec<(String, String)>,
    /// `lowres_<terrain>.glb`, the far-LOD terrain sheet the map's `tern`
    /// ships — what both PS2s draw past the near scene (`bake::lowres_glb`,
    /// analysis/re-lowr.md). `None` on the 16 terrains that carry no `LOWR`
    /// and on every model-built map.
    ///
    /// This says the sheet EXISTS, not that it is drawn: the draw is gated on
    /// `sky.low_res_terrain` being authored, which BF2 DAG1 and NAB2 leave
    /// out although they ship a sheet (re-lowr.md §0.4). APPENDED after
    /// `texture_tier2` so a manifest's existing RON text is unchanged up to
    /// this point; defaulted so a pre-sheet bake still loads.
    #[serde(default)]
    pub lowres_glb: Option<String>,
    /// The terrain foliage the map's `tern -> FOLG` and `prp_` place
    /// (`bake::foliage`, analysis/re-foliage.md). `None` on a map whose FOLG
    /// places nothing and whose `prp_` has no path line. APPENDED after
    /// `lowres_glb`; defaulted so a pre-foliage bake still loads.
    #[serde(default)]
    pub foliage: Option<ManifestFoliage>,
    /// `terrain_layers_<terrain>.glb`: the terrain's texture layers above
    /// LTEX[0], one primitive per layer with the `ALPH` "over" alpha in the
    /// vertex colour, drawn blended over `terrain_glb` in ascending order
    /// (`bake::terrain_glb`, analysis/re-terrain-layers.md, card TALPH).
    /// `None` where no patch paints a second layer and on every model-built
    /// map. APPENDED after `foliage`; defaulted so a pre-TALPH bake still
    /// loads (and draws LTEX[0] alone, as it did).
    #[serde(default)]
    pub terrain_layers_glb: Option<String>,
    /// Every `leafpatch` class the map ships (card LEAF,
    /// `analysis/re-leafpatch.md` §3): what a class's [`ManifestAttach`], a
    /// world instance and a FOLG `LeafPatch` mesh draw. APPENDED after
    /// `terrain_layers_glb` and written only when non-empty, so a map without
    /// leaves bakes byte-identical to before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leaf: Vec<ManifestLeafClass>,
    /// BF2's `Effect("HDR")` when the map ENABLES it (card GLOW,
    /// `analysis/re-glow.md` §2.1 [O]): the 13 BF2 maps whose self-lit
    /// segments the bake wrote as `glow` / `unscaled` (`bake::glb`) and
    /// whose bright pixels bloom. `None` on every BF1 map and on BF2 DAG1,
    /// END1, TAT2, YAV1. APPENDED after `leaf` and written only when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hdr: Option<ManifestHdr>,
    /// BF2's authored local lights (card BPNT, `analysis/re-point-lights.md`
    /// §1 [O]): every omni and spot `Light` of every non-stub `lght` chunk,
    /// in chunk order. The directional ones stay in [`Manifest::lighting`].
    /// Empty on every BF1 map and on a BF2 map that authors none. APPENDED
    /// after `hdr` and written only when non-empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub point_lights: Vec<ManifestPointLight>,
    /// `Effect("Lightning")` when the map ENABLES it (card KSKY,
    /// `analysis/re-kamino.md` §1 [O]): BF1 KAM1, BF1 YAV2 and BF2 KAM1, whose
    /// sky dome the PS2 darkens between flashes. `None` on the other 29 maps.
    /// APPENDED after `point_lights` and written only when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning: Option<ManifestLightning>,
    /// `Effect("Precipitation")` when the map ENABLES it (card WTHR,
    /// `analysis/re-weather.md` §1 [O]): nine maps. Its rain shadows are the
    /// [`Manifest::regions`] whose name begins with `rainshadow`. APPENDED
    /// after `lightning` and written only when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precipitation: Option<ManifestPrecipitation>,
    /// `Effect("Wind")` when the map enables it (both Kaminos, both Hoths):
    /// only the precipitation reads it. APPENDED, written only when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind: Option<ManifestWind>,
}

/// The Precipitation effect's authored numbers
/// (`mungefs::fx::PrecipitationConfig`; the ELF ctor's defaults where a map
/// leaves a key out). `game::weather` runs it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestPrecipitation {
    /// `Type("Streaks")`: lines (rain); else textured squares (flakes).
    pub streaks: bool,
    /// `Texture`: the flake's sheet.
    pub texture: Option<String>,
    pub particle_size: f32,
    pub range: f32,
    /// `Color`, 0..255 RGBA.
    pub color: [u8; 4],
    pub alpha_min_max: [f32; 2],
    /// `ParticleDensity`: the particle count of the one tiled cube.
    pub particle_density: f32,
    pub particle_density_range: f32,
    /// Fall speed, m/s, down positive.
    pub velocity: f32,
    pub velocity_range: f32,
    /// `StreakLength` as authored (the PS2 stores it / 30, seconds).
    pub streak_length: f32,
    pub camera_cross_velocity_scale: f32,
    pub camera_axial_velocity_scale: f32,
    pub rotation: f32,
    pub rotation_range: f32,
    /// `GroundEffect`: the splash's particle-effect name.
    pub ground_effect: Option<String>,
    pub ground_effects_per_sec: f32,
    pub ground_effect_spread: f32,
}

/// The Wind effect's authored numbers (`mungefs::fx::WindConfig`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestWind {
    /// `Velocity(a, b)`: the base vector is (a, 0, b), m/s.
    pub velocity: [f32; 2],
    pub velocity_range: f32,
    pub velocity_change_rate: f32,
}

/// The Lightning effect's authored numbers (`mungefs::fx::LightningConfig`;
/// the ELF ctor's defaults where a map leaves a key out). The viewer consumes
/// [`Self::dome_tint`] at rest; the flash fields wait for the lightning card
/// (WTHL), which animates `brightness`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestLightning {
    /// `SkyDomeDarkenFactor`: dome shell 0 draws at white·(1 − f) between
    /// flashes; every other shell stays white.
    pub sky_dome_darken_factor: f32,
    /// `SunlightFadeFactor`.
    pub sunlight_fade_factor: f32,
    /// `Color`, 0..255 as authored: what a flash lifts the lights toward.
    pub color: [f32; 3],
    /// `BrightnessMin`.
    pub brightness_min: f32,
    /// `FadeTime`, seconds.
    pub fade_time: f32,
    /// `TimeBetweenFlashesMinMax`, seconds.
    pub time_between_flashes: [f32; 2],
    /// `TimeBetweenSubFlashesMinMax`, seconds.
    pub time_between_sub_flashes: [f32; 2],
    /// `NumSubFlashesMinMax`.
    pub num_sub_flashes: [f32; 2],
    /// `HorizonAngleMinMax`, degrees as authored.
    pub horizon_angle: [f32; 2],
    /// `SoundCrack` / `SoundSubCrack`.
    pub sound_crack: Option<String>,
    pub sound_sub_crack: Option<String>,
}

impl ManifestLightning {
    /// The colour byte the PS2 stores for dome shell 0 while a flash is at
    /// `brightness` (0 = between flashes, the resting look):
    /// white·(1 − f) + black·f with f = factor·(1 − brightness), truncated
    /// (B2 `FUN_0045ae80` 520152–520170, 520289–520300). 153 at rest for the
    /// discs' 0.4 — what PS2 RAM holds.
    pub fn dome_byte(&self, brightness: f32) -> u8 {
        let f = self.sky_dome_darken_factor.clamp(0.0, 1.0) * (1.0 - brightness.clamp(0.0, 1.0));
        (255.0 * (1.0 - f)) as u8
    }

    /// [`Self::dome_byte`] as the tint a shell is drawn with. The GS
    /// multiplies in gamma space, so the factor is linearised for the port's
    /// linear pipeline (the rule `ManifestSky::reflection_color` follows) —
    /// 255 stays exactly 1.0.
    pub fn dome_tint(&self, brightness: f32) -> f32 {
        let u = self.dome_byte(brightness) as f32 / 255.0;
        // sRGB EOTF, IEC 61966-2-1: the curve the dome's texture is sampled
        // through.
        if u <= 0.040_45 {
            u / 12.92
        } else {
            ((u + 0.055) / 1.055).powf(2.4)
        }
    }
}

/// A spot's axis and outer cone ([`ManifestPointLight::cone`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ManifestSpotCone {
    /// Unit direction the spot points, engine world space.
    pub dir: [f32; 3],
    /// `Cone`'s second value: the outer cone's FULL angle, radians.
    pub outer: f32,
}

/// One authored omni or spot light ([`Manifest::point_lights`]). The PS2
/// lights a model with a spot as a point: no cone
/// (`analysis/re-point-lights.md` §6 [O]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestPointLight {
    /// The `Light` block's name.
    pub name: String,
    /// `Type(3)`: a model draw takes at most one spot beside its four omnis.
    pub spot: bool,
    /// A one-way spot's axis and cone, for the gather's back-cone test
    /// ([`ManifestSpotCone`]); `None` on an omni and on a `Bidirectional`
    /// spot, which the test never rejects.
    pub cone: Option<ManifestSpotCone>,
    /// Engine world space (the PS2's load negates the stored z).
    pub pos: [f32; 3],
    /// `Color` as authored, 0..1.
    pub color: [f32; 3],
    /// `Range`, metres.
    pub range: f32,
    /// `Static()`: a prelit model and the terrain skip the light.
    pub is_static: bool,
    /// `PS2Overbright()`: packed ×255 instead of ×128 — twice the colour.
    pub overbright: bool,
}

/// The bloom's authored numbers (`mungefs::fx::HdrConfig`; the ELF's defaults
/// where a map leaves a key out).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ManifestHdr {
    /// Blur iterations (each two five-tap sub-passes).
    pub num_bloom_passes: u32,
    /// Each tap weighs `int(int(w · 127) · 0.2) / 128`.
    pub max_total_weight: f32,
    /// Luma cut in the half-scale frame: `2 ·` this of display white blooms.
    pub glow_threshold: f32,
}

/// How a `leafpatch` template places its sprites (class +0x94).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LeafLayout {
    /// The upper half-ellipsoid of `radius` × `height` (the default).
    Ball,
    /// `0x35a4393b` (w, h, d): uniform in that box.
    Box([f32; 3]),
    /// `0xcdbc383d` (x, z wobble): the recursive branching vine.
    Vine([f32; 2]),
}

/// A `leafpatch` class: its ODF properties as the PS2's setter stores them
/// (B1 `FUN_001b27b8`, defaults from the constructor `FUN_001b21e0`;
/// `analysis/re-leafpatch.md` §3). The template the PS2 builds from these
/// (`FUN_001b38f8`) is the runtime's (`game::viewer::leaf`). Names in the
/// docs of hashed fields are the RE's, not the disc's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestLeafClass {
    pub name: String,
    /// `Texture`, as the ODF spells it.
    pub texture: String,
    /// `NumParticles` (NOT divided by the view count — LEAF's decision).
    pub particles: u32,
    pub min_size: f32,
    pub max_size: f32,
    pub radius: f32,
    pub height: f32,
    /// `0xcaaa3778`: the object's up axis × this at draw; template y ÷ it.
    pub scale: f32,
    /// `Offset`, added to every sprite.
    pub offset: [f32; 3],
    /// `0xeaaaa7a9`: the top metres that narrow toward the crown.
    pub taper: f32,
    /// `Seed`, wrapped to 32 bits; `None` = the PS2 draws one per session.
    pub seed: Option<u32>,
    pub layout: LeafLayout,
    /// `0xa8c0d41d`: a vine's straight-run length range.
    pub vine_run: [i32; 2],
    /// `0x45328dc6`: a vine's step, × the sprite size.
    pub vine_step: f32,
    pub darkness_min: f32,
    pub darkness_max: f32,
    pub alpha: f32,
    pub max_distance: f32,
    /// `NumParts`: 1 = the whole sheet turned by 90° per part, else quarters.
    pub num_parts: i32,
    /// `0xd4cbd35b` / `0x7cbb796c`: the sway's phase rate and amplitude.
    pub wind: [f32; 2],
}

/// A map's terrain foliage: the mesh table, the placed instances in a binary
/// sidecar ([`foliage_bin`]), and the path-block props.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestFoliage {
    /// `foliage.bin`, relative to the map dir: the instances FOLG places.
    pub instances: String,
    /// How many records it holds.
    pub count: u32,
    /// One row per `prp_` `Mesh()` entry, layers in order; a record's `mesh`
    /// indexes this.
    pub meshes: Vec<ManifestFoliageMesh>,
    /// The path block's props (re-foliage §9): world instances of their
    /// class, matrix as the PS2 builds it. Drawn like any instance; NOT in
    /// the collision world yet (queue FOLGC).
    pub path_props: Vec<ManifestInstance>,
    /// Every `grasspatch` class the map ships (re-foliage §7): what a
    /// `GrassPatch` mesh row names and what a world instance of the class
    /// draws. APPENDED; defaulted so a pre-FOLGG bake still loads.
    #[serde(default)]
    pub grass: Vec<ManifestGrassClass>,
}

/// A `grasspatch` class: its ODF properties as the PS2's setter stores them
/// (B1 `FUN_001adcd8`, defaults from the constructor `FUN_001ad820`;
/// analysis/re-foliage.md §7) plus the atlas the draw picks by `NumParts`.
/// The template the PS2 builds from these (`FUN_001ae120`) is the runtime's
/// (`game::viewer::grass`). Named fields carry the known property names; the
/// rest say their hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestGrassClass {
    pub name: String,
    /// `Texture`, as the ODF spells it (the texture table is case-blind).
    pub texture: String,
    /// `NumParticles` (NOT divided by the camera count — FOLG's decision).
    pub particles: u32,
    pub min_size: f32,
    pub max_size: f32,
    /// `0x3d5d0aef`: every sprite's base height.
    pub y_offset: f32,
    pub alpha: f32,
    /// `MaxDistance`: the draw distance of a patch placed as a world object.
    pub max_distance: f32,
    pub radius_fade_min: f32,
    pub radius_fade_max: f32,
    pub darkness_min: f32,
    pub darkness_max: f32,
    pub num_parts: i32,
    /// `0x35a4393b` (w, d): either > 0 places the sprites in that rectangle.
    pub rect: [f32; 2],
    /// `0x231fcc79`: a tall blade's rise range, × `MinSize`.
    pub raise: [f32; 2],
    /// `0xa9b14958`: a tall blade's size factor.
    pub tall_size: f32,
    /// `0xf6582b9c`: a tall blade's lean away from the camera.
    pub tall_lean: f32,
    /// `0xc75263e1`: a tall blade's darker ground quad, this far down
    /// (0 = none; no class on either disc sets it).
    pub shadow: f32,
    /// `0x97133eeb`: a normal sprite's random static lean, ±.
    pub lean: f32,
    /// `0x4b6d5171`: the part cycle's extra length.
    pub cycle: i32,
    /// `0xff7bb39a`: a normal sprite's width factor when `NumParts` is 3 or
    /// ≥ 5.
    pub thin: f32,
    /// `0x604db82e` present: tall blades do not sway.
    pub still: bool,
    /// The atlas the draw indexes by part (ELF .data, read at bake time):
    /// per part, four (u, v) corners — the sprite's top-left, top-right,
    /// bottom-left, bottom-right.
    pub atlas: Vec<[[f32; 2]; 4]>,
}

/// What a foliage mesh entry draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FoliageKind {
    /// A model (`File`), drawn by the world pipeline.
    Model,
    /// A `grasspatch` class's sprite cluster (queue FOLGG).
    GrassPatch,
    /// A `leafpatch` class (queue LEAF).
    LeafPatch,
}

/// One `prp_` `Mesh()` entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestFoliageMesh {
    /// Its layer, 0..4.
    pub layer: u8,
    pub kind: FoliageKind,
    /// The model name (`Model`) or class name (a patch), as the LVL spells
    /// it; `None` when it names nothing this LVL ships (the PS2 draws
    /// nothing there either).
    pub name: Option<String>,
    /// Fade distance d, metres: a model fades out over [d, 1.45·d] of
    /// horizontal distance; a grass patch over the last 20 m before d.
    pub distance: f32,
    /// Models nearer than this (horizontal) fade toward 0.1²; 0 = none.
    pub near_fade: f32,
    /// Uniform scale (models).
    pub scale: f32,
    /// Up = lerp(terrain normal, world up, stiffness).
    pub stiffness: f32,
    /// `0x26edc40c`: the model's collision mesh is solid on the PS2.
    pub collision: bool,
}

/// `foliage.bin`: the placed foliage instances, one definition for the bake
/// (writer) and the runtime (reader).
///
/// ```text
/// b"FOLG" u32 version (1) u32 count
/// count × { u16 mesh, u16 0, f32 x, f32 z, f32 yaw, u8 r, g, b, a }  (20 B)
/// ```
///
/// x and z are world metres, yaw radians (models only), rgba the PS2's
/// per-instance tint (255 = unity). Height and tilt are NOT stored: the
/// runtime takes them from the terrain it loads (re-foliage §6).
pub mod foliage_bin {
    /// One placed instance.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Record {
        pub mesh: u16,
        pub x: f32,
        pub z: f32,
        pub yaw: f32,
        pub rgba: [u8; 4],
    }

    const MAGIC: &[u8; 4] = b"FOLG";
    const VERSION: u32 = 1;
    const RECORD: usize = 20;

    pub fn write(records: &[Record]) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + records.len() * RECORD);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(records.len() as u32).to_le_bytes());
        for r in records {
            out.extend_from_slice(&r.mesh.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&r.x.to_le_bytes());
            out.extend_from_slice(&r.z.to_le_bytes());
            out.extend_from_slice(&r.yaw.to_le_bytes());
            out.extend_from_slice(&r.rgba);
        }
        out
    }

    /// `None` on a wrong magic, version or length.
    pub fn read(b: &[u8]) -> Option<Vec<Record>> {
        if b.len() < 12 || &b[..4] != MAGIC {
            return None;
        }
        let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let f32_at = |o: usize| f32::from_bits(u32_at(o));
        let n = u32_at(8) as usize;
        if u32_at(4) != VERSION || b.len() != 12 + n * RECORD {
            return None;
        }
        Some(
            (0..n)
                .map(|i| {
                    let o = 12 + i * RECORD;
                    Record {
                        mesh: u16::from_le_bytes([b[o], b[o + 1]]),
                        x: f32_at(o + 4),
                        z: f32_at(o + 8),
                        yaw: f32_at(o + 12),
                        rgba: b[o + 16..o + 20].try_into().unwrap(),
                    }
                })
                .collect(),
        )
    }
}

/// One world's authored AI data, from `mungefs::plan` and `mungefs::world`.
///
/// Present whenever the world authors ANY of the three — a planning graph, a
/// `Hint` or a `BARR` — because the three are authored independently: seven
/// BF2 maps keep hints in a `lvl_` mode layer that ships no `plan` of its own
/// (HOT1 77, MUS1 73, KAM1 34, MYG1 6, GEO1 4, FEL1 3, TAN1 2), and folding
/// those into the plan would lose them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestNav {
    /// The world's own planning graph: the last top-level `plan` for the base
    /// world, a mode layer's own `plan` for a layer. `None` on a world that
    /// ships none — at runtime such a layer falls back to the base graph, and
    /// no route sidecar is written for it.
    #[serde(default)]
    pub plan: Option<ManifestNavPlan>,
    #[serde(default)]
    pub hints: Vec<ManifestHint>,
    #[serde(default)]
    pub barriers: Vec<ManifestBarrier>,
}

/// One world's planning graph.
///
/// The big tables do not go in the RON — a single map's would be megabytes of
/// text — so they live in two binary sidecars under `<map>/nav/` whose paths
/// `routes` and `ManifestPlnp::records` name. Both are written by `bake::map`
/// and documented there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestNavPlan {
    pub hubs: Vec<ManifestNavHub>,
    pub arcs: Vec<ManifestNavArc>,
    /// `nav/routes_<layer>.bin`, the hub route tables.
    pub routes: String,
    /// The `plnp` patches that follow this world's `plan`, when it ships any
    /// (BF2 only, 20 chunks over 11 ground maps).
    #[serde(default)]
    pub plnp: Option<ManifestPlnp>,
}

/// One planning hub. `pos` is the world frame AS STORED (hub y is the
/// editor's pick height, not a floor — RE_LOG).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestNavHub {
    pub name: String,
    pub pos: [f32; 3],
    pub radius: f32,
    /// The hub's arc slots in order, empty (0xff) slots dropped. Slot k of the
    /// route tables indexes THIS list.
    pub arcs: Vec<u16>,
}

/// One arc between two hubs, walked in both directions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestNavArc {
    pub name: String,
    pub a: u16,
    pub b: u16,
    /// `AISizeType` mask: bit0 SOLDIER, 1 HOVER, 2 SMALL, 3 MEDIUM, 4 HUGE,
    /// 5 FLYER.
    pub type_mask: u32,
    /// BF2 only; 0 on BF1 arcs, whose record has no such field.
    pub flags: u32,
}

/// The `plnp` patch set: `groups` are the fnv1a hashes of the arc names each
/// blockable group holds, and `records` names the sidecar holding the
/// `1 << groups.len()` inflated XOR patches, indexed by the blocked-group
/// bitmask.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestPlnp {
    pub groups: Vec<Vec<u32>>,
    /// `nav/plnp_<layer>.bin`.
    pub records: String,
}

/// One `Hint` node. `xform` is the 3x3 rotation rows then the translation, AS
/// STORED — a hint's translation is NOT z-negated (unlike a barrier's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestHint {
    pub name: String,
    /// 0 Snipe, 1 Patrol, 2 Cover, 4 JetJump, 5 Minefield, 6 Land, 7/8
    /// BF2-only. Neither game handles 3 and no disc authors it.
    pub kind: u8,
    pub xform: [[f32; 3]; 4],
    /// The five PROP keys both games resolve, as AUTHORED (the loaders' own
    /// masking and folding is consumer behaviour — see `mungefs::world`).
    #[serde(default)]
    pub primary_stance: Option<u8>,
    #[serde(default)]
    pub secondary_stance: Option<u8>,
    #[serde(default)]
    pub mode: Option<u8>,
    #[serde(default)]
    pub radius: Option<f32>,
    #[serde(default)]
    pub command_post: Option<String>,
    /// Every PROP as stored: (fnv1a key hash, value string).
    #[serde(default)]
    pub props: Vec<(u32, String)>,
}

/// One `BARR` barrier: a 2-D oriented rectangle in XZ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestBarrier {
    pub name: String,
    /// The loader's WORLD position: the stored translation with z NEGATED
    /// (BF1 `FUN_00177828` / BF2 `FUN_00178ae8`, and the disc agrees).
    pub pos: [f32; 3],
    /// The 3x3 rotation rows as stored.
    pub rows: [[f32; 3]; 3],
    pub size: [f32; 3],
    /// The `AISizeType` mask this barrier stops.
    pub flags: u32,
}

/// One terrain cutter: its `CUTR INFO` bounding sphere `(r, cx, cy, cz)` and
/// its planes (`nx, ny, nz, d`; inside = every plane strictly < 0). See
/// `mungefs::terrain::Cutter`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestCutter {
    /// All zeroes on a manifest baked before the sphere was decoded — read as
    /// "no bound" by `core::collision::is_cut_by`.
    #[serde(default)]
    pub sphere: [f32; 4],
    pub planes: Vec<[f32; 4]>,
}

/// One geometry's collision: the `coll` mesh (baked to
/// `collision/<geometry>.glb`) and/or its `prim` primitives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestCollision {
    /// Geometry name as authored (matched case-insensitively at runtime).
    pub geometry: String,
    /// The `coll` chunk's NODE name (informational). Defaulted for bakes made
    /// before it was recorded.
    #[serde(default)]
    pub node: String,
    /// `collision/<geometry>.glb`, when this geometry ships a `coll` mesh.
    pub glb: Option<String>,
    /// Defaulted for bakes made before primitives were read.
    #[serde(default)]
    pub prims: Vec<ManifestPrim>,
}

/// One collision primitive in its model's space. `dims` is (radius, _, _) for a
/// sphere, (radius, FULL height, _) for a cylinder and the HALF extents for a
/// box — see `mungefs::coll` for how those conventions were measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestPrim {
    pub name: String,
    /// Defaulted for bakes made before the parent node was recorded.
    #[serde(default)]
    pub parent: String,
    /// "sphere" | "cylinder" | "box".
    pub kind: String,
    pub dims: [f32; 3],
    pub rotation: [[f32; 3]; 3],
    pub translation: [f32; 3],
    /// BF2's per-primitive layer mask as the ENGINE sees it — 0 = no MASK
    /// read = all five subsets; bits s 0x01, v 0x02, b 0x04, t 0x08, o 0x10
    /// (`mungefs::coll::Primitive::mask`, which also carries the loader's
    /// stale-byte quirk). Always 0 on BF1. Defaulted for bakes made before
    /// the byte was read.
    #[serde(default)]
    pub mask: u8,
    /// True iff this primitive authored its own `MASK` chunk rather than
    /// inheriting the previous one's. Informational (census only).
    #[serde(default)]
    pub mask_own: bool,
}

/// A map's water surface. The mesh is baked flat at `height`; the renderer
/// scrolls its UVs by `uv_velocity`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestWater {
    /// GLB filename beside the manifest.
    pub glb: String,
    /// Texture painting the surface; `None` when the map names one we did
    /// not bake (never happens on the twelve shipped water maps).
    #[serde(default)]
    pub texture: Option<String>,
    /// World Y of the plane. PROVEN — see `mungefs::water`.
    pub height: f32,
    /// UV units per second the renderer scrolls the surface by. Since
    /// 2026-09-09 this is the PS2's `fx__ Velocity / INFO5` on every map that
    /// authors a `Velocity` (all twelve); the pre-`fx__` value read `WATR ->
    /// LAYR`'s dead copy, which both ELFs drop. The SIGN is PROVISIONAL —
    /// see `mungefs::fx::WaterConfig::scroll_per_second`.
    #[serde(default)]
    pub uv_velocity: [f32; 2],
    /// `WATR -> LAYR`'s colour word. **DEAD DATA** — both ELFs read and drop
    /// it (`analysis/re-water.md` §1), so its byte order is moot and nothing
    /// may consume it. The PS2's tint is `config`'s diffuse LUT. Recorded
    /// because it is what the bytes hold.
    #[serde(default)]
    pub layer_color: [u8; 4],
    /// Informational: wet `WMAP` cells, `CPAT` patches, triangles.
    #[serde(default)]
    pub wet_cells: usize,
    #[serde(default)]
    pub patches: usize,
    #[serde(default)]
    pub triangles: usize,
    /// What the map's `fx__ Effect("Water")` authors. APPENDED after
    /// `triangles` so a manifest's existing RON text is unchanged up to this
    /// point; defaulted so a pre-`fx__` manifest still loads. `None` would
    /// mean a water map with no `Effect("Water")`, which neither disc ships.
    #[serde(default)]
    pub config: Option<ManifestWaterConfig>,
    #[serde(default)]
    pub wet: Option<ManifestWetMask>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestWetMask {
    pub grid: u32,
    pub cell_size: f32,
    pub half_extent: [f32; 2],
    pub nibbles: String,
}

impl ManifestWetMask {
    pub fn from_nibbles(grid: u32, cell_size: f32, half_extent: [f32; 2], cells: &[u8]) -> Self {
        Self {
            grid,
            cell_size,
            half_extent,
            nibbles: cells
                .iter()
                .map(|c| char::from_digit(u32::from(c & 0xf), 16).unwrap())
                .collect(),
        }
    }

    pub fn cells(&self) -> Vec<u8> {
        self.nibbles
            .chars()
            .map(|c| c.to_digit(16).map_or(0, |d| d as u8))
            .collect()
    }
}

/// The serde default of a water flag the PS2 resets to 1 (`specular_enable`,
/// `oscillation_enable`). The derived `Default` still gives `false` — that is
/// a test fixture's "unset", not a disc value.
fn flag_on() -> bool {
    true
}

/// The PS2's own water configuration, from the LVL's `fx__` chunk
/// (`mungefs::fx::WaterConfig`, `analysis/re-water.md` §5). `None` on a field
/// means the map authored no such key — never a substituted default.
///
/// The bake consumes `main_texture` and `metres_per_repeat` (both are already
/// applied to `glb`'s material and UVs) and records the rest for the runtime's
/// water look — tint, alpha and the speckle pass (`game::viewer::look`) and the
/// KAM1 ocean (`game::water_anim`). Colours are 0..255 RGBA, as the discs
/// store them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ManifestWaterConfig {
    /// `MainTexture` — what actually paints the surface. On both KAM1s this
    /// differs from `LAYR`'s name, which is the FOAM texture.
    #[serde(default)]
    pub main_texture: Option<String>,
    /// `FoamTexture`, blended over the base on an ocean map. Both KAM1s only.
    #[serde(default)]
    pub foam_texture: Option<String>,
    /// Metres of world per texture repeat, `cell / (Tile · INFO5)` — the
    /// value `glb`'s UVs were built with. `None` when the map authors no
    /// `Tile` (no disc map does).
    #[serde(default)]
    pub metres_per_repeat: Option<[f32; 2]>,
    /// `OceanEnable` — 1 on both KAM1s. Those maps draw a Tessendorf FFT
    /// surface on the PS2, and so does the port (`game::water_anim`).
    #[serde(default)]
    pub ocean: bool,
    /// `DisableLowRes` — BF1 KAS1, both YAV1s and BF2 DAG1. Those maps draw
    /// no far water at all (`re-water.md` §1).
    #[serde(default)]
    pub disable_low_res: bool,
    /// `SpecularEnable` — the byte BF2's flat-water renderer reads to gate its
    /// speckle layer and its far-water fog: 1 unless the map authors 0. The
    /// Water effect's level reset stores 1 (BF2 ELF 0x56027c) and no map on
    /// either disc authors the key (`analysis/re-water-flags.md` §1–§4
    /// `[O]`). BF1 has no gate, so the field means nothing there.
    #[serde(default = "flag_on")]
    pub specular_enable: bool,
    /// `OscillationEnable` — the byte that gates the flat water's 6 cm bob:
    /// 1 unless the map authors 0, which only BF2 FEL1 does. The same reset
    /// stores 1 (`re-water-flags.md` §7 `[O]`); BF1 has no gate.
    #[serde(default = "flag_on")]
    pub oscillation_enable: bool,
    /// The per-vertex diffuse LUT's ends, Border → Min → Max (`re-water.md`
    /// §3). W2's tint and alpha come from these.
    #[serde(default)]
    pub min_diffuse: Option<[f32; 4]>,
    #[serde(default)]
    pub max_diffuse: Option<[f32; 4]>,
    #[serde(default)]
    pub border_diffuse: Option<[f32; 4]>,
    #[serde(default)]
    pub specular_color: Option<[f32; 4]>,
    /// The additive animated highlight layer (W3): its frames in order, the
    /// rate, and the UV tiling/scroll/shift.
    #[serde(default)]
    pub speckle_textures: Vec<String>,
    #[serde(default)]
    pub speckle_fps: Option<f32>,
    #[serde(default)]
    pub speckle_tile: Option<[f32; 2]>,
    #[serde(default)]
    pub speckle_scroll: Option<[f32; 2]>,
    #[serde(default)]
    pub speckle_coord_shift: Option<[f32; 2]>,
    #[serde(default)]
    pub speckle_specular_color: Option<[f32; 4]>,
    #[serde(default)]
    pub speckle_ambient_color: Option<[f32; 4]>,
    /// `LightAzimAndElev` — (1, 0) on all twelve.
    #[serde(default)]
    pub light_azim_and_elev: Option<[f32; 2]>,
    /// The PS2 mesh's subdivisions per cell and its far-pass decimation. The
    /// port bakes two triangles per cell instead; `patch_divisions` sizes only
    /// the KAM1 ocean's runtime grid (`game::water_anim`), `lod_decimation`
    /// is recorded and not consumed.
    #[serde(default)]
    pub patch_divisions: Option<[f32; 2]>,
    #[serde(default)]
    pub lod_decimation: Option<f32>,
}

/// Shared-asset bake (BF1 COMMON/ENG/COMMON.LVL, BF2 INGAME.LVL): the com_*
/// classes + models + textures every map references (command posts, recharge
/// droids, turrets...). No world; the game merges it under each map's scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommonManifest {
    pub source: String,
    pub models: Vec<String>,
    pub textures: Vec<String>,
    pub classes: Vec<ManifestClass>,
    pub geometries: Vec<ManifestGeometry>,
    /// Collision for the shared props maps instance (command posts, crates,
    /// turrets): merged under the map's own by `Scene::merge_common`.
    /// Defaulted so a pre-collision common bake still loads.
    #[serde(default)]
    pub collision: Vec<ManifestCollision>,
    /// Classic-plus tier 2, as [`Manifest::texture_tier2`]: the common bake
    /// writes overrides too (BF2's INGAME.LVL against the PC's `ingame.lvl`).
    /// No `texture_tiers` twin here — which SOURCE filled the tier has never
    /// been recorded for the common bake. APPENDED after `collision`;
    /// defaulted so a single-tier common bake still loads.
    #[serde(default)]
    pub texture_tier2: Vec<(String, String)>,
}

/// One SIDE LVL's vehicles (`bake vehicle`, card VBAKE;
/// `analysis/re-vehicles.md` §1, §6, §10 item 3): every class whose BASE is a
/// vehicle label, its models (every LOD), its destruction chunks' models and
/// its collision, written to `side_<game>/vehicles/<side>/vehicles.ron` beside
/// `models/`, `collision/` and `textures/`. No world; `game::viewer::vehicles`
/// draws from it and `game::vehicle_pads` spawns from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehicleManifest {
    /// The side LVL's stem, lowercased (`imp`, `rep`, …).
    pub source: String,
    pub vehicles: Vec<ManifestVehicle>,
    /// The gmod rows of every geometry a vehicle or chunk names.
    pub geometries: Vec<ManifestGeometry>,
    /// Every `models/<name>.glb` written, in bake order.
    pub models: Vec<String>,
    /// Every `textures/<name>.ktx2` written (lowercase).
    pub textures: Vec<String>,
    /// The `coll` meshes and `prim` bodies of those geometries.
    pub collision: Vec<ManifestCollision>,
}

/// One vehicle class of a [`VehicleManifest`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestVehicle {
    /// Name, BASE, GeometryName and collision-layer lists, as a map's class.
    pub class: ManifestClass,
    /// The `ChunkGeometryName` of each `CHUNKSECTION`, in ODF order
    /// (re-vehicles §6: up to 9) — keys into `geometries`.
    pub chunks: Vec<String>,
    /// The geometry's `modl SPHR` sphere `[x, y, z, r]` in model space (card
    /// VDTH, `analysis/re-vehicle-death.md` §3): the centre a blast measures
    /// the class from, and the floor of its radius. `None` (not written)
    /// when the model has no SPHR, or on a bake from before VDTH.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sphere: Option<[f32; 4]>,
    /// The min y of the geometry's top LOD `modl INFO` vis box, in model
    /// space (card VREST, `analysis/re-vehicle-rest.md` §2): a flyer class's
    /// `LandedHeight` is minus this unless the ODF authors one after its
    /// `GeometryName`. `None` (not written) on a bake from before VREST.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vis_min_y: Option<f32>,
    /// A walker class's anim table and prim origins (card WALK); `None`
    /// (not written) for every other class, or a bake from before WALK.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walker: Option<ManifestWalker>,
}

/// What a walker's integrator reads from its bake (card WALK,
/// `analysis/re-walker.md` §5, §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestWalker {
    /// The class's `AnimationName` bank.
    pub bank: String,
    /// The anim table's 11 fixed entries (FUN_002663e0), by entry; `None`
    /// where the bank lacks the entry's clip.
    pub clips: Vec<Option<ManifestWalkerClip>>,
    /// Every collision primitive of the class's geometry at the bind pose:
    /// the leg pairs' foot prims and the `BuildingCollision` spheres.
    pub prims: Vec<ManifestPrimPoint>,
    /// Card WSKN: the drawn walker's skeleton and clips,
    /// `walkers/<skin>.glb` beside `vehicles.ron` — the geometry's LOD0
    /// model, whose GLB carries each vertex's bone as `JOINTS_0`; `None`
    /// when the bake found no skeleton.
    #[serde(default)]
    pub skin: Option<String>,
    /// Card WSKN: each `WalkerLegPair`'s two layer bones (the class entry's
    /// +0x28 / +0x30 masks, props 0x580f4d75 / 0x9eb9fc8e, re-walker §6):
    /// the `_leftup` / `_rightup` layers move these bones' subtrees.
    #[serde(default)]
    pub layer_bones: Vec<[String; 2]>,
}

/// One anim-table entry's clip (FUN_002a0c78, BF2's clock).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestWalkerClip {
    /// The bare clip name the bank was searched for.
    pub name: String,
    pub frames: u16,
    /// (frames − 1)/30 s.
    pub duration: f32,
    pub looping: bool,
    /// Track 0's root translation z at each frame (metres, before
    /// SkeletonRootScale).
    pub root_z: Vec<f32>,
}

/// A collision primitive's origin in model space at the skeleton's bind
/// pose, its parent bone and its bounding radius.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestPrimPoint {
    pub name: String,
    pub bone: String,
    pub pos: [f32; 3],
    pub radius: f32,
}

/// Just the identity fields of a manifest (cheap map discovery for the
/// shell): a `Manifest` deserialized through this reads only `map` and
/// `source_game` and ignores the rest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestHead {
    pub map: String,
    #[serde(default = "default_game")]
    pub source_game: String,
}

// --- lighting, fog and sky (baked by `bake::lighting`) ----------------------

/// Linear fog between `near` and `far` metres. Colour 0..1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ManifestFog {
    pub color: [f32; 3],
    pub near: f32,
    pub far: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestLighting {
    /// Where each value came from, for the artifact READMEs and for anyone
    /// wondering why a map looks the way it does: "lght", "sky_", "mixed" or
    /// "default". Defaulted (empty) for bakes made before it was recorded.
    #[serde(default)]
    pub source: String,
    /// Unit direction the sunlight TRAVELS (sun -> scene), engine Y-up world.
    pub sun_dir: [f32; 3],
    /// Linear, 0..1 — both games' authored colour through sRGB -> linear
    /// (BF2's `Light.Color` since queue LITEB, `bake::lighting`).
    pub sun_color: [f32; 3],
    /// Hemisphere ambient, 0..1. `ambient_ground` equals `ambient_sky` when
    /// the map authors only one ambient colour (every BF1 map).
    pub ambient_sky: [f32; 3],
    pub ambient_ground: [f32; 3],
    /// `SkyInfo.CharacterAmbientColor` / 255 where authored — the discs light
    /// soldiers with a separate, usually brighter ambient. The runtime takes
    /// it on BF1 maps only (`game::viewer::look`).
    #[serde(default)]
    pub character_ambient: Option<[f32; 3]>,
    #[serde(default)]
    pub fog: Option<ManifestFog>,
    /// `SkyInfo.NearSceneRange(a, b, c)` in metres, as authored. `c` is where
    /// the PS2's NEAR scene ends: the heightfield is drawn with a camera
    /// frustum whose far plane is `c`, and `b..c` is the fade band into the
    /// far-scene buffer (`analysis/re-fog.md` §2.5, `analysis/re-lowr.md`
    /// §2.2 [V]). Every shipped ground map authors it; `None` for a manifest
    /// baked before this field existed, or a map with no `sky_ SkyInfo`.
    #[serde(default)]
    pub near_scene_range: Option<[f32; 3]>,
    /// `SkyInfo.FarSceneRange` in metres, as authored — the distance the PS2
    /// culls FAR-SCENE draws at (world models by bounding sphere, the water's
    /// low-res pass, the `LOWR` sheet). `0.0` is authored and MEANS "no far
    /// layer at all" on nine of the 32 baked maps (`analysis/re-fog.md`
    /// §2.5/§6 [V] lists ten; BF2 TAT3 is not a baked ground map), so it is
    /// kept as `Some(0.0)` and never folded into `None`.
    #[serde(default)]
    pub far_scene_range: Option<f32>,
    /// The key light (`GlobalLights.Light1`) authors `Static()` — BF2 only;
    /// BF1 has no `lght`. A PRELIT model's light set skips every Static light
    /// (`analysis/re-model-lighting.md` §3 `[O]`); a normal model takes it.
    /// `false` for a manifest baked before queue LITEB.
    #[serde(default)]
    pub sun_static: bool,
    /// `GlobalLights.Light2`, when it names a directional (`Type(1)`) light:
    /// the PS2's second directional light on every model (§3 `[O]`: BF2
    /// COR1, END1, MUS1, NAB2, UTA1). `None` on BF1, on a BF2 map whose
    /// `Light2` names nothing, and on a manifest baked before queue LITEB.
    #[serde(default)]
    pub light2: Option<ManifestLight>,
    /// `DomeInfo`'s frame filter where `Filter` ≠ 0 and `Intensity` ≥ 1 —
    /// every BF1 map but HOT1, and BF2 TAT2 (`analysis/re-glow.md` §3
    /// `[O]`). `None` = no pass, and for a manifest baked before card BFLT.
    #[serde(default)]
    pub filter: Option<ManifestFilter>,
}

/// The PS2's DomeInfo frame filter, as authored: over the finished 3D frame,
/// before the HUD, per channel in display bytes,
/// `out = min(255, v + intensity/128 · max(0, low-pass(v) − threshold))`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ManifestFilter {
    /// `Threshold`, a display byte 0..255.
    pub threshold: f32,
    /// `Intensity`; the gain is `intensity / 128`.
    pub intensity: f32,
}

/// One directional light beside the key sun (`ManifestLighting::light2`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ManifestLight {
    /// Unit direction the light TRAVELS, engine Y-up world — the same
    /// reduction as `ManifestLighting::sun_dir`.
    pub dir: [f32; 3],
    /// Linear, 0..1 (the authored `Color`, sRGB -> linear).
    pub color: [f32; 3],
    /// `Static()` authored — see `ManifestLighting::sun_static`.
    pub is_static: bool,
}

/// The engine's pre-2026-08-31 hardcoded look, for a manifest baked before
/// `lighting` existed (mirrors `render::Lighting::default` and
/// `bake::lighting`'s `DEFAULT_*` constants). PROVISIONAL and ours — nothing
/// on either disc authors these.
impl Default for ManifestLighting {
    fn default() -> Self {
        Self {
            source: "default".into(),
            sun_dir: [-0.400_51, -0.801_02, -0.450_57],
            sun_color: [1.0, 1.0, 0.95],
            ambient_sky: [0.25, 0.25, 0.28],
            ambient_ground: [0.25, 0.25, 0.28],
            character_ambient: None,
            fog: None,
            near_scene_range: None,
            far_scene_range: None,
            sun_static: false,
            light2: None,
            filter: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestSkyModel {
    /// `modl` name; the bake checks it against the map's models.
    pub geometry: String,
    /// Vertical offset of the shell in metres (0 when unauthored).
    pub offset: f32,
    /// Fraction of camera motion the shell follows; 1.0 = fully camera-locked.
    pub movement_scale: f32,
    /// `DomeModel.Reflect`: the shell is also drawn mirrored under the eye,
    /// tinted by [`ManifestSky::reflection_color`]
    /// (`analysis/re-sky-reflect.md` §2, §5). Absent in pre-2026-09-29 bakes.
    #[serde(default)]
    pub reflect: bool,
}

/// One `sky_ SkyObject`: a flight of distant capital ships, drawn in the sky
/// around the map. The geometry lives in a SIDE LVL, not the map's own — see
/// `bake::map`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestSkyObject {
    /// `modl` name, copied into the map's own models by the bake.
    pub geometry: String,
    /// How many of this ship to place. `NumObjects`, 1..=8.
    pub count: u32,
    /// `Height` — an authored altitude RANGE in metres.
    pub height: [f32; 2],
    /// `Distance` — metres from the viewer.
    pub distance: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestSky {
    /// `DomeInfo.Texture` — the dome's own sheet, where authored.
    #[serde(default)]
    pub texture: Option<String>,
    /// `DomeModel` shells, outermost first as stored.
    pub models: Vec<ManifestSkyModel>,
    /// `DomeInfo.Ambient` / 255 — the port's flat tint on every shell. Neither
    /// ELF reads the key (hash 0x1c9640eb is in neither; `re-kamino.md` §1)
    /// and it bakes to white on all 32 maps, so it is inert; kept because
    /// dropping it would move every manifest for nothing (card KSKY). What
    /// the PS2 does tint a shell with is [`Manifest::lightning`].
    pub ambient: [f32; 3],
    /// `LowResTerrain.Texture` and `MaxDistance` as stored. The texture being
    /// authored is what gates the far `LOWR` sheet's draw
    /// ([`Manifest::lowres_glb`], `game::viewer::look`); the distance is the
    /// far end of the range the sheet is drawn over (the same function; 2000
    /// m where the map authors none).
    #[serde(default)]
    pub low_res_terrain: Option<String>,
    #[serde(default)]
    pub low_res_max_distance: Option<f32>,
    /// `SkyObject` flights whose geometry the bake resolved out of a SIDE
    /// LVL. Empty on the 23 maps that author none (and on any bake that
    /// could not reach the SIDE LVLs), and absent in pre-2026-08-31 bakes.
    #[serde(default)]
    pub objects: Vec<ManifestSkyObject>,
    /// `DomeInfo.TerrainEnable` as stored, on either game — the discs author
    /// it only as 0, on BF1 KAM1 and BES1. Only BF1 reads it: at 0 its
    /// terrain is not drawn at all (`analysis/re-terrain-enable.md` §2 `[O]`),
    /// which `game::viewer::scene::terrain_drawn` applies. Absent in
    /// pre-2026-09-28 bakes.
    #[serde(default)]
    pub terrain_enable: Option<f32>,
    /// `DomeInfo.ReflectionColor` as the tint the mirrored `reflect` shells
    /// are drawn with: the GS value `((c + 1) >> 1) / 128`, linearised like
    /// every other authored colour. `None` = the loader's default white
    /// (`analysis/re-sky-reflect.md` §1). Absent in pre-2026-09-29 bakes.
    #[serde(default)]
    pub reflection_color: Option<[f32; 3]>,
}

// ---------------------------------------------------------------------------
// `<map>/sounds/sounds.ron` — the per-map sound bank index.
// `bake::sounds` writes it, `game::sounds` reads it back. Same two rules as
// the manifest above: declaration order IS the wire format, and every
// `#[serde(default)]` keeps an older bake readable.
// ---------------------------------------------------------------------------

/// One `snd_` property script: the definition the game looks up by the
/// fnv1a_lowercase hash of a class-prop string (FireSound =
/// "all_weap_inf_rifle_fire").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundProp {
    pub name_hash: u32,
    pub kind: u32,
    pub gain: f32,
    pub pitch: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub rolloff: f32,
    pub priority: f32,
    pub looping: bool,
    /// (sample id hash, weight) — samples resolved through the bank index.
    pub samples: Vec<(u32, f32)>,
    /// Layered definitions: the sound property each `Layer` entry starts
    /// (the entries the PS2's parser keeps, analysis/re-weapon-sounds.md §1).
    pub layers: Vec<u32>,
    /// `soundstreamproperties`: the stream this plays from. The audio is NOT
    /// in this map's banks — it is in GLOBAL/CW/GCW.LVL, baked once by
    /// `bake global-sound` (analysis/re-sound-global.md §1.1/§4: about 45 %
    /// of the names a BF2 map resolves are streamed).
    ///
    /// The runtime carries this for the record only — a segment is resolved
    /// through the global index's `files`, NEVER by joining this hash to the
    /// segment id: 172 BF1 (58 BF2) ids appear in more than one stream and
    /// are baked once, under whichever stream came first
    /// (`engine/artifacts/global_sound/README.md`).
    ///
    /// Defaulted: keeps a pre-item-15 bake readable.
    #[serde(default)]
    pub stream: Option<u32>,
    /// (segment id hash, weight) into that stream — the key the OGGs under
    /// `audio/global_bf{1,2}/` are named by. Defaulted with `stream`.
    #[serde(default)]
    pub segments: Vec<(u32, f32)>,
    /// `mutedistance` where it is NOT `max_distance`: silent from here out
    /// (analysis/re-sound-distance.md). The three fields below are written
    /// only when set, so a property without them reads as before card SNDT.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mute_distance: Option<f32>,
    /// `rollindistance` when above 0: quieter the nearer inside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roll_in_distance: Option<f32>,
    /// `mode3d` 0 through the template chain: a 2-D sound, played at its
    /// gain at any distance.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flat: bool,
    /// Per `layers` entry: (delay s, deviation s) - the layer starts at
    /// delay + deviation x U(-1, 1) after the record (card WSND). Empty when
    /// every layer starts at once; else as long as `layers`. Like the fields
    /// below, written only when set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layer_delays: Vec<(f32, f32)>,
    /// `PitchDev` / `GainDev`: each start draws pitch (gain) +- this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch_dev: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain_dev: Option<f32>,
    /// `PlayInterval`, `PlayIntervalDev` (s): no start for interval +- dev
    /// after one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play_interval: Option<(f32, f32)>,
    /// `PlayProbability` below 1: the chance a start is taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play_probability: Option<f32>,
    /// The sub-level that defines it, by name hash (`kas2gcw`; card SNDE):
    /// the PS2 loads the one its mission names, and the same name has
    /// another era's stream and takes in the next one. 0 outside a
    /// sub-level (CORE.LVL) and in a bake made before the card.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub level: u32,
    /// The mixer bus it plays on, by name hash through the template chain
    /// (`soundfx`, `voiceover`, `ambience`, `battlechatter`, ...; card
    /// MIXH): the PS2 scales a voice by its bus's final gain
    /// (analysis/re-mix-headroom.md 1.4). 0 = none named, and in a bake
    /// made before the card.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub bus: u32,
    /// Per take of the list it plays (`samples` when it has any, else
    /// `segments`): the take's own (PlayInterval, PlayIntervalDev) - a
    /// drawn take is locked that long (B1 `FUN_0046b290`; card UCALL).
    /// Empty when no take is locked, and in a bake made before the card.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub take_locks: Vec<(f32, f32)>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(v: &u32) -> bool {
    *v == 0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundsRon {
    pub source: String,
    /// (bank name hash, sample count)
    pub banks: Vec<(u32, usize)>,
    /// sample id hash → relative WAV path under `sounds/`.
    pub samples: Vec<(u32, String)>,
    pub props: Vec<SoundProp>,
    /// Samples whose `frequency` field is outside 4–48 kHz and so cannot be a
    /// playback rate (~1.8 % of every disc bank; values cluster at 322–346 and
    /// 3000–3047). The record parse is NOT slipping on them — `frames ==
    /// size/16*28` holds for every record — so the disc really stores those
    /// values and what the PS2 does with them is unknown. Written as-is; see
    /// card SNDP.
    ///
    /// Defaulted: keeps a bake made before the census readable.
    #[serde(default)]
    pub odd_frequency: usize,
    /// Properties that play from a stream instead of a bank sample.
    /// Defaulted with `odd_frequency`.
    #[serde(default)]
    pub streamed: usize,
    /// The `soundparameterized` records (a vehicle's `EngineSound`),
    /// resolved (card VSND, `analysis/re-vehicle-sounds.md` §1-2). One per
    /// record per sub-level, in file order; written only when the LVL has
    /// one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub engines: Vec<EngineSound>,
    /// The LVL's `Music` records (card AMBM, `analysis/re-ambience-music.md`
    /// 3.7), one per name in file order; written only when the LVL has one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub music: Vec<MusicRon>,
}

/// A `Music` record: the stream property it plays (a `props` entry with
/// `stream` and `segments`) and the times of its state machine, in seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MusicRon {
    pub name_hash: u32,
    /// `SoundStream`: the name hash of the stream property.
    pub stream: u32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub min_playback: f32,
    pub min_inactive: f32,
    /// BF2's `MaxPlaybackTime`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_playback: Option<f32>,
    pub priority: f32,
}

/// A `soundparameterized` record: the looping layers of its layered record,
/// in that record's order, each with the graphs the record gives the layer
/// of its name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSound {
    pub name_hash: u32,
    /// The sub-level that defines it (`nab1cw`: a battle loads `<map><era>`,
    /// and the same name's graphs differ between sub-levels); 0 outside one.
    pub level: u32,
    pub layers: Vec<EngineLayer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineLayer {
    /// The sound property the layer loops.
    pub sound: u32,
    /// The layer starts `delay` +- `deviation` s after the record.
    pub delay: f32,
    pub deviation: f32,
    pub graphs: Vec<EngineGraph>,
}

/// `ParameterGraph(parameter, graph, target)` with the graph's points:
/// linear between them, flat outside.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineGraph {
    /// `speed`, `acceleration`, `health`, `turnontime`, `turnofftime`...
    /// (the engine component's table, B1 0x547e00); 8 hex digits for a
    /// hash that is none of them.
    pub parameter: String,
    /// `Gain`, `Pitch`... (the property field the graph multiplies).
    pub target: String,
    pub points: Vec<(f32, f32)>,
}

// ---------------------------------------------------------------------------
// `<map>/sounds/foley.ron` — the soldier foley table (card FOLY,
// `analysis/re-foley.md` §1). `bake::foley` writes it, `game::foley` reads it.
// ---------------------------------------------------------------------------

/// One `FoleyFXSoldier` record: a soldier family's sound properties on one
/// surface, by name hash; 0 = the record has no such slot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoleyRow {
    /// The hash of a soldier class's `FoleyFXClass` value.
    pub class: u32,
    pub walk_left: u32,
    pub walk_right: u32,
    pub run_left: u32,
    pub run_right: u32,
    pub jump: u32,
    pub land: u32,
    pub roll: u32,
    pub prone: u32,
    pub squat: u32,
    pub stand: u32,
    pub body_fall: u32,
}

/// One `FoleyFXGroup`: a surface (`terrain_foley`, `metal_foley`, ...).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoleyGroup {
    pub name: u32,
    pub rows: Vec<FoleyRow>,
}

/// One sub-level `<map><era>` (`nab1cw`) of the SOUND LVL.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoleyLevel {
    pub name: u32,
    pub groups: Vec<FoleyGroup>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoleyRon {
    pub levels: Vec<FoleyLevel>,
}

// ---------------------------------------------------------------------------
// `<map>/load/load.ron` — the loading-screen sequence.
// `bake::loadscreen` writes it, `game::assets::load_loadscreen` reads it.
// ---------------------------------------------------------------------------

/// How the game presents the textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LoadStyle {
    /// BF1: the `load_*_0..3` zoom steps play in order (~1 s each).
    #[default]
    Sequence,
    /// BF2: `<planet>_1..N` stills — the game shows one of them for the load
    /// (LOAD/COMMON.LVL's single `loadscreen` is the generic fallback).
    Pool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadScreen {
    /// Texture names in display order (KTX2 files under `load/`).
    pub sequence: Vec<String>,
    /// Extra textures not in the sequence — BF1's planet globe
    /// (`load_planet_tattoine`), which the original composites over the
    /// locale step. DELIBERATELY baked and not yet drawn: the load-screen
    /// layout pass (PLAN §Milestones, M5 polish - the old P10) is where it
    /// lands.
    pub extras: Vec<String>,
    /// Defaulted: keeps a bake made before BF2's still pools readable.
    #[serde(default)]
    pub style: LoadStyle,
}

// ---------------------------------------------------------------------------
// `classes/<game>_<lvl>.ron` — the gameplay classes of one LVL (entc / wpnc /
// ordc / expc). `bake::classes` writes it, `game::classdb` reads it.
// ---------------------------------------------------------------------------

/// `side_<game>/soldiers/<model>.skins.ron` (card USKN,
/// `analysis/re-soldier-skeleton-skins.md` 9): a soldier model's override
/// groups and the override textures its classes name. A class's
/// `OverrideTexture` replaces the textures listed under `override_texture`,
/// its `OverrideTexture2` those under `override_texture2`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SoldierSkins {
    /// Group (the segment's `MNAM`) -> the textures of the model's
    /// segments in it, lowercase.
    pub groups: std::collections::BTreeMap<String, Vec<String>>,
    /// Every override texture a class on this model names, lowercase,
    /// baked beside the model's own under `textures/`.
    pub overrides: Vec<String>,
}

/// One munged gameplay class: its name, the chunk it came from, its BASE and
/// its PROP list as stored. Keys stay FNV-1a hashes and values raw strings;
/// the lookups live with their callers (`bake::classes::ClassProps`,
/// `game::classdb::ClassDb`), since this crate is data only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassDef {
    pub name: String,
    /// Source chunk tag: "entc" | "wpnc" | "ordc" | "expc".
    ///
    /// Defaulted: keeps a classes.ron baked before the field existed readable
    /// (the reader's rule since 37a7f1e).
    #[serde(default)]
    pub kind: String,
    /// Parent class (BASE), e.g. "soldier", "hover", "cannon", "bolt".
    /// BF2 chains within a kind (rep_weap_inf_rifle -> com_weap_inf_rifle).
    pub base: String,
    /// Full property list as stored: (fnv1a key hash, raw value string).
    pub props: Vec<(u32, String)>,
}

// ---------------------------------------------------------------------------
// `effects.ron` — one LVL layer's `fx__` particle effects (card FXP1,
// `analysis/re-fx.md` §1–§4). `bake::effects` writes it beside a map's
// `manifest.ron`, a common bake's `common.ron`, and under
// `side_<game>/effects/<side>/`; the runtime (card FXP2) looks an effect up by
// hash map layer first, then the sides, then common. Every field is the
// parser's (`mungefs::particle`, which documents each); the values are the
// PS2's own, bit for bit (its RAM, `mungefs/tests/suite/fx_lvl.rs`).
// ---------------------------------------------------------------------------

/// One layer's effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxSet {
    /// `"bf1"` / `"bf2"`: BF1 reads no `FrameSize` (a fixed 4×4 atlas) and
    /// parses no `Looping`; BF2 reads both (re-fx §6.3, §8).
    pub game: String,
    /// The effects in first-seen order, one per NAME hash.
    pub effects: Vec<FxEffect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxEffect {
    /// `fnv1a_lowercase(name)` — the hash an ODF value names it by.
    pub hash: u32,
    /// The cracked name, where known (`bake/src/effect_names.txt`).
    pub name: Option<String>,
    /// The root `ParticleEmitter`s.
    pub emitters: Vec<FxEmitter>,
}

/// `(base, span)`: a value drawn as `base + span·rand()`; random iff span ≠ 0.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FxRange(pub f32, pub f32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FxBlend {
    Normal,
    Additive,
    Blur,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FxType {
    Particle,
    Streak,
    Emitter,
    Animated,
    Geometry,
    Billboard,
    Spark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FxShape {
    None,
    Spread,
    Vortex,
    Circle,
    Type4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FxPositionOp {
    None,
    Accelerate,
    Reach,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FxScalarOp {
    None,
    Move,
    Reach,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FxColorOp {
    None,
    Move,
    Reach,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FxPositionNode {
    pub lifetime: f32,
    pub op: FxPositionOp,
    pub value: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FxScalarNode {
    pub lifetime: f32,
    pub op: FxScalarOp,
    pub value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FxColorNode {
    pub lifetime: f32,
    pub op: FxColorOp,
    pub value: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxSpawner {
    pub shape: FxShape,
    pub shape_ranges: [FxRange; 9],
    pub offset: [FxRange; 3],
    pub position_scale: FxRange,
    pub velocity_scale: FxRange,
    pub inherit_velocity_factor: FxRange,
    pub size: FxRange,
    pub color: [FxRange; 4],
    /// The mode the particles' colour is in (Hue/Saturation/Value when set).
    pub hsv: bool,
    pub fade_in_time: f32,
    pub start_rotation: FxRange,
    pub rotation_velocity: FxRange,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxTransformer {
    pub lifetime: f32,
    pub position: Vec<FxPositionNode>,
    pub size: Vec<FxScalarNode>,
    pub rotation: Vec<FxScalarNode>,
    pub color: Vec<FxColorNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxGeometry {
    pub blend: FxBlend,
    pub kind: FxType,
    pub texture: Option<String>,
    pub model: Option<String>,
    pub child: Option<Box<FxEmitter>>,
    pub time_per_frame: f32,
    pub frame_size: f32,
    pub total_frames: f32,
    pub blur_value: f32,
    pub blur_res: i32,
    pub spark_length: f32,
    pub looping: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxEmitter {
    pub label: String,
    pub start_delay: FxRange,
    pub burst_delay: FxRange,
    pub burst_count: FxRange,
    pub max_particles: FxRange,
    pub color: [FxRange; 4],
    /// The emitter-level mode word: never read at runtime (re-fx §5.8).
    pub hsv: bool,
    pub size: FxRange,
    pub spawner: FxSpawner,
    pub transformer: FxTransformer,
    pub geometry: FxGeometry,
    pub min_lod_dist: f32,
    pub max_lod_dist: f32,
    pub bounding_radius: f32,
    pub no_register_step: bool,
    pub world_aligned: bool,
    pub sibling: Option<Box<FxEmitter>>,
    pub sound_name: Option<String>,
}

#[cfg(test)]
mod tests;
