//! Bake a SIDE LVL's vehicles (card VBAKE; `analysis/re-vehicles.md` §1, §6,
//! §10 item 3) to `<outdir>/models/<modl>.glb`, `<outdir>/collision/<coll>.glb`,
//! `<outdir>/textures/<tex>.ktx2` and `<outdir>/vehicles.ron`
//! ([`manifest::VehicleManifest`]).
//!
//! A vehicle is an `entc` whose BASE is one of the ELF's vehicle labels
//! ([`VEHICLE_BASES`], re-vehicles §2). Each one brings its `GeometryName`'s
//! gmod (every LOD: LOD0 and LOWD), the gmod of each `CHUNKSECTION`'s
//! `ChunkGeometryName` (§6), the `coll` mesh and `prim` bodies of those
//! geometries, and the class's collision-layer lists in PROP order. A class
//! whose geometry the LVL does not ship (the BF2 ALL `_ord` missiles, which
//! the side LVL files under a `flyer` BASE; BF2 REP's `rep_fly_jedifighter_sc`)
//! is skipped with a line on stderr.
//!
//! **Every model is written RIGID, in its skeleton's bind pose.** Most side
//! vehicle models are bone-space (the speeder bikes, tanks, walkers, most
//! fighters); their vertices are moved to model space through the model's own
//! `skel` (`mungefs::model::to_model_space`, the soldier bake's step). The
//! rest (the hovernaut, the K-wing, the MTT, the Hailfire, the TIE bomber, the
//! troop transport) keep their body as stored and take each `BNAM` segment
//! through its bone's bind matrix (card VBIND, `crate::bind_pose::pose_bnam`).
//! The collision prims rest at the same bind pose: each `XFRM` is in its
//! `PRNT` bone's frame (card RKHUL, [`crate::bind_pose::prims_at_bind`]).
//! The result goes through the world models' writer and ALPHA RULE v2
//! (`crate::glb::model_to_glb`, `crate::alpha`), which draws any `MTRL` word —
//! including the 0x02 alpha test `rep_walk_atte` carries and the 0x04 + 0x80
//! words BF2's tri-fighter and ARC-170 carry, which the skinned soldier path
//! refuses (`crate::soldier::segment_blends`). A walker's top-LOD model also
//! carries each vertex's bone as `JOINTS_0`, and its skeleton and clips go
//! to `walkers/<model>.glb` (card WSKN, `crate::walker::skin_glb`): the
//! runtime poses it on the CPU and draws it through the same world path, so
//! the disc's alpha test on `rep_walk_atte` stays the world's cutout (ALPHA
//! RULE v2) rather than a skinned pass it would need. Every other vehicle's
//! skeleton is not baked (a turret's aim moves bones: card VTUR's). The textures take the side bake's writer
//! (`crate::soldier::bake_textures_flagged`), told per texture whether a drawn
//! segment was judged MASK or BLEND (design D3's rule for the map bake).

use anyhow::{anyhow, Context, Result};
use manifest::{ManifestCollision, ManifestGeometry, ManifestVehicle, VehicleManifest};
use mungefs::model::Model;
use std::collections::HashMap;
use std::path::Path;

/// The ClassLabels whose constructor builds a vehicle entity (re-vehicles §2:
/// `treaded` IS a hover on both discs). `armedbuilding` and the turrets are
/// seat-only buildings (§8), not baked here.
pub const VEHICLE_BASES: [&str; 8] = [
    "hover",
    "treaded",
    "walker",
    "flyer",
    "carrier",
    "commandhover",
    "commandwalker",
    "commandflyer",
];

/// fnv1a("ChunkGeometryName"): one per `CHUNKSECTION` (re-vehicles §6).
const PROP_CHUNK_GEOMETRY: u32 = 0xDA32_8455;
/// fnv1a("AnimationName"): a walker's clip bank (re-walker §5).
const PROP_ANIMATION_NAME: u32 = 0x9855_7A5E;

/// Is `base` (an entc BASE) a vehicle label?
pub fn is_vehicle_base(base: &str) -> bool {
    VEHICLE_BASES.iter().any(|b| b.eq_ignore_ascii_case(base))
}

/// Everything in the LVL the bake can look up by lowercase name; first
/// occurrence wins, as it does in every other bake.
struct Index {
    classes: Vec<mungefs::entity::EntityClass>,
    gmods: HashMap<String, mungefs::entity::GeomModel>,
    models: HashMap<String, Model>,
    skels: HashMap<String, mungefs::skel::Skeleton>,
    colls: HashMap<String, (mungefs::coll::CollisionMesh, usize)>,
    prims: HashMap<String, (mungefs::coll::Primitives, usize)>,
}

fn index(chunks: &[mungefs::Chunk<'_>]) -> Result<Index> {
    let mut ix = Index {
        classes: Vec::new(),
        gmods: HashMap::new(),
        models: HashMap::new(),
        skels: HashMap::new(),
        colls: HashMap::new(),
        prims: HashMap::new(),
    };
    for c in chunks {
        let at = |e| anyhow!("{} 0x{:X}: {e}", String::from_utf8_lossy(&c.tag), c.offset);
        match &c.tag {
            b"entc" => {
                let e = mungefs::entity::EntityClass::parse(c).map_err(at)?;
                if !ix
                    .classes
                    .iter()
                    .any(|k| k.name.eq_ignore_ascii_case(&e.name))
                {
                    ix.classes.push(e);
                }
            }
            b"gmod" => {
                let g = mungefs::entity::GeomModel::parse(c).map_err(at)?;
                ix.gmods.entry(g.name.to_ascii_lowercase()).or_insert(g);
            }
            b"modl" => {
                let m = Model::parse(c).map_err(at)?;
                ix.models.entry(m.name.to_ascii_lowercase()).or_insert(m);
            }
            b"skel" => {
                let s = mungefs::skel::parse_skel(c).map_err(at)?;
                ix.skels.entry(s.name.to_ascii_lowercase()).or_insert(s);
            }
            b"coll" => {
                let m = mungefs::coll::CollisionMesh::parse(c).map_err(at)?;
                ix.colls
                    .entry(m.name.to_ascii_lowercase())
                    .or_insert((m, c.offset));
            }
            b"prim" => {
                let p = mungefs::coll::Primitives::parse(c).map_err(at)?;
                ix.prims
                    .entry(p.model.to_ascii_lowercase())
                    .or_insert((p, c.offset));
            }
            _ => {}
        }
    }
    Ok(ix)
}

impl Index {
    /// A geometry's LOD rows: its gmod's, else the bare modl of that name
    /// (a chunk may name a modl directly). `None` when the LVL has neither.
    fn lods(&self, geometry: &str) -> Option<Vec<(String, String)>> {
        let key = geometry.to_ascii_lowercase();
        let rows: Vec<(String, String)> = match self.gmods.get(&key) {
            Some(g) => g
                .lods
                .iter()
                .map(|l| (l.tier.clone(), l.model.clone()))
                .collect(),
            None => vec![("LOD0".to_owned(), geometry.to_owned())],
        };
        let rows: Vec<_> = rows
            .into_iter()
            .filter(|(_, m)| self.models.contains_key(&m.to_ascii_lowercase()))
            .collect();
        (!rows.is_empty()).then_some(rows)
    }

    /// A walker geometry's `prim` bodies and skeleton (card WALK): named as
    /// the geometry, else as one of its LOD models.
    fn walker_rig(
        &self,
        geometry: &str,
    ) -> (
        Option<&mungefs::coll::Primitives>,
        Option<&mungefs::skel::Skeleton>,
    ) {
        let mut names = vec![geometry.to_ascii_lowercase()];
        if let Some(lods) = self.lods(geometry) {
            names.extend(lods.into_iter().map(|(_, m)| m.to_ascii_lowercase()));
        }
        let prims = names.iter().find_map(|n| self.prims.get(n)).map(|(p, _)| p);
        let skel = names.iter().find_map(|n| self.skels.get(n));
        (prims, skel)
    }

    /// The model in its skeleton's bind pose — the skeleton named as the
    /// model, else as its geometry: a bone-space model's every vertex through
    /// its bone's bind matrix, and any other model's `BNAM` segments through
    /// theirs (`crate::bind_pose::pose_bnam`, the map bake's rule, PROVISIONAL
    /// [D] — card VBIND: the hovernaut's barrels, the trooptrans's landing
    /// gear and the Hailfire's wheels are stored in their bones' frames). A
    /// model with no skeleton, or bound only to identity bones, is as stored.
    ///
    /// Also each segment's per-vertex skeleton bones (empty for a model
    /// that is not bone-space): a walker's `JOINTS_0` (card WSKN).
    fn model_space(&self, name: &str, geometry: &str) -> Result<(Model, Vec<Vec<usize>>)> {
        let m = &self.models[&name.to_ascii_lowercase()];
        let mut out = m.clone();
        if !m.bone_space {
            if let Some(skel) = self.skel_of(name, geometry) {
                if let Some(p) = crate::bind_pose::pose_bnam(m, skel).map_err(|e| anyhow!(e))? {
                    out = p;
                }
            }
            return Ok((out, Vec::new()));
        }
        let skel = self
            .skel_of(name, geometry)
            .ok_or_else(|| anyhow!("{name}: bone-space model with no skel"))?;
        let mut bones = Vec::new();
        for seg in out.segments.iter_mut() {
            let (p, n, b) =
                mungefs::model::to_model_space(m, seg, skel).map_err(|e| anyhow!("{name}: {e}"))?;
            seg.positions = p;
            seg.normals = n;
            bones.push(b);
        }
        out.bone_space = false;
        Ok((out, bones))
    }

    /// A geometry's `prim` bodies in model space at the bind pose (card
    /// RKHUL): each through its `PRNT` bone's bind matrix, by the skeleton
    /// named as the prims' model, else as the geometry. Also the count of
    /// undecoded prims dropped.
    fn rest_prims(
        &self,
        p: &mungefs::coll::Primitives,
        geometry: &str,
    ) -> (Vec<manifest::ManifestPrim>, usize) {
        let (mut prims, unknown) = crate::map::manifest_prims(p);
        if let Some(skel) = self.skel_of(&p.model, geometry) {
            crate::bind_pose::prims_at_bind(skel, &mut prims);
        }
        (prims, unknown)
    }

    /// The skeleton a model is posed by: named as the model, else as its
    /// geometry.
    fn skel_of(&self, name: &str, geometry: &str) -> Option<&mungefs::skel::Skeleton> {
        self.skels
            .get(&name.to_ascii_lowercase())
            .or_else(|| self.skels.get(&geometry.to_ascii_lowercase()))
    }
}

/// A geometry's top LOD model (LOD0, else LOD1, LOD2, LOWD, else its first).
fn top_lod(ix: &Index, geometry: &str) -> Option<String> {
    let lods = ix.lods(geometry)?;
    let top = ["LOD0", "LOD1", "LOD2", "LOWD"]
        .iter()
        .find_map(|t| lods.iter().find(|(tier, _)| tier == t))
        .unwrap_or(&lods[0]);
    Some(top.1.clone())
}

/// Bake every vehicle class of one SIDE LVL into `outdir`. `pc_lvl` is the
/// PC install's matching side LVL: its larger textures are written beside the
/// PS2 ones as `<name>.tier2.ktx2` (`crate::tier`).
pub fn bake_vehicles(
    lvl_path: &str,
    outdir: &str,
    pc_lvl: Option<&str>,
) -> Result<VehicleManifest> {
    let pc = pc_lvl.map(crate::tier::PcTextures::load).transpose()?;
    let buf = std::fs::read(lvl_path).with_context(|| lvl_path.to_string())?;
    let inner = mungefs::open(&buf).map_err(|e| anyhow!("{e}"))?;
    let chunks = mungefs::content_chunks(inner).map_err(|e| anyhow!("{e}"))?;
    let ix = index(&chunks)?;
    let out = Path::new(outdir);
    for d in ["models", "collision", "textures", "walkers"] {
        std::fs::create_dir_all(out.join(d))?;
    }

    // Card WALK: the walkers' clip banks.
    let banks = mungefs::anim::find_banks(inner).unwrap_or_default();

    // Classes first: which geometries, in first-named order.
    let mut vehicles = Vec::new();
    let mut geometries: Vec<ManifestGeometry> = Vec::new();
    // Card WSKN: the models whose skin is written (lowercase).
    let mut skinned: Vec<String> = Vec::new();
    for e in ix.classes.iter().filter(|e| is_vehicle_base(&e.base)) {
        let Some(geometry) = e.geometry_name() else {
            eprintln!("vehicle {}: no GeometryName — skipped", e.name);
            continue;
        };
        if ix.lods(geometry).is_none() {
            eprintln!(
                "vehicle {}: geometry {geometry} not in this LVL — skipped",
                e.name
            );
            continue;
        }
        let chunk_names: Vec<String> = e
            .props
            .iter()
            .filter(|p| p.key_hash == PROP_CHUNK_GEOMETRY)
            .map(|p| p.value.clone())
            .collect();
        for g in std::iter::once(geometry).chain(chunk_names.iter().map(String::as_str)) {
            if geometries.iter().any(|k| k.name.eq_ignore_ascii_case(g)) {
                continue;
            }
            let Some(lods) = ix.lods(g) else {
                eprintln!("vehicle {}: chunk geometry {g} not in this LVL", e.name);
                continue;
            };
            let best = ["LOD0", "LOD1", "LOD2", "LOWD"]
                .iter()
                .find_map(|t| lods.iter().find(|(tier, _)| tier == t))
                .unwrap_or(&lods[0])
                .1
                .clone();
            geometries.push(ManifestGeometry {
                name: g.to_owned(),
                model: best,
                lods,
                far: None,
            });
        }
        // The class's model is its geometry's top LOD (as the chunk loop
        // above picks it); its SPHR is what the PS2 measures the class by.
        // Its `INFO` vis box's floor is what a flyer class's LandedHeight is
        // read from (card VREST, re-vehicle-rest §2).
        let top = top_lod(&ix, geometry).map(|top| &ix.models[&top.to_ascii_lowercase()]);
        let sphere = top.and_then(|m| m.sphere);
        let vis_min_y = top.map(|m| m.vis_box.min[1]);
        // Card WALK: a walker's anim table and prim origins; card WSKN: its
        // top LOD's skin (the skeleton and the bank's clips), once per model.
        let walker = if crate::walker::is_walker_base(&e.base) {
            let bank = e
                .props
                .iter()
                .find(|p| p.key_hash == PROP_ANIMATION_NAME)
                .map(|p| p.value.clone())
                .unwrap_or_default();
            let (prims, skel) = ix.walker_rig(geometry);
            let top = top_lod(&ix, geometry);
            let skin = match top
                .as_deref()
                .and_then(|t| Some((t, ix.skel_of(t, geometry)?)))
            {
                Some((t, sk)) if ix.models[&t.to_ascii_lowercase()].bone_space => {
                    let key = t.to_ascii_lowercase();
                    if !skinned.contains(&key) {
                        let b = banks.iter().find(|b| b.name.eq_ignore_ascii_case(&bank));
                        let glb = crate::walker::skin_glb(sk, b)
                            .with_context(|| format!("walker skin {t}"))?;
                        std::fs::write(out.join("walkers").join(format!("{t}.glb")), glb)?;
                        skinned.push(key);
                    }
                    Some(t.to_owned())
                }
                _ => None,
            };
            Some(crate::walker::bake(e, &bank, &banks, prims, skel, skin))
        } else {
            None
        };
        vehicles.push(ManifestVehicle {
            class: crate::map::class_from_entc(e),
            chunks: chunk_names,
            sphere,
            vis_min_y,
            walker,
        });
    }

    // Card VTUR: each class's skel nodes (the seats' fire points).
    crate::hardpoints::attach(&chunks, vehicles.iter_mut().map(|v| &mut v.class));

    // Models: every LOD of every geometry, once each.
    let mut textures_decoded: Vec<mungefs::texture::Texture> = Vec::new();
    let mut to_write: Vec<Model> = Vec::new();
    let mut joints: Vec<Vec<Vec<usize>>> = Vec::new();
    for g in &geometries {
        for (_, name) in &g.lods {
            if to_write.iter().any(|m| m.name.eq_ignore_ascii_case(name)) {
                continue;
            }
            let (m, bones) = ix.model_space(name, &g.name)?;
            to_write.push(m);
            joints.push(bones);
        }
    }
    let wanted: Vec<String> = to_write
        .iter()
        .flat_map(|m| m.segments.iter())
        .filter_map(crate::alpha::segment_texture)
        .map(str::to_ascii_lowercase)
        .collect();
    for c in chunks.iter().filter(|c| &c.tag == b"tex_") {
        if let Ok(t) = mungefs::texture::Texture::parse(c) {
            let lower = t.name.to_ascii_lowercase();
            if wanted.contains(&lower)
                && !textures_decoded
                    .iter()
                    .any(|k| k.name.eq_ignore_ascii_case(&t.name))
            {
                textures_decoded.push(t);
            }
        }
    }
    let table = crate::alpha::AlphaTable::new(&textures_decoded);
    let mut usage = crate::alpha::AlphaUsage::default();
    let mut models = Vec::new();
    for (m, bones) in to_write.iter().zip(&joints) {
        let alphas = table.model(m);
        usage.record_model(m, &alphas);
        // A walker's skinned model carries its vertices' bones (card WSKN).
        let skin = skinned.contains(&m.name.to_ascii_lowercase());
        let glb =
            crate::glb::model_to_glb_joints(m, &alphas, false, skin.then_some(bones.as_slice()))?;
        std::fs::write(out.join("models").join(format!("{}.glb", m.name)), glb)?;
        models.push(m.name.clone());
    }

    // Textures: alpha kept iff a drawn segment is MASK or BLEND (design D3),
    // or a specular-pass segment draws it — the pass's gloss weight (queue
    // DSHN, `crate::spec::pass_textures`).
    let pass = crate::spec::pass_textures(&to_write);
    let flags: HashMap<String, u32> = wanted
        .iter()
        .map(|t| {
            let keep = usage.needs_alpha(t) || pass.contains(t);
            (
                t.clone(),
                if keep {
                    crate::detail::ALPHA_IS_MEANINGFUL
                } else {
                    0
                },
            )
        })
        .collect();
    let textures = crate::soldier::bake_textures_flagged(inner, &wanted, &flags, out, pc.as_ref())?;

    // Collision: the coll mesh and prim bodies named as a geometry or any of
    // its LOD models, in geometry order. A prim's `XFRM` is in its `PRNT`
    // bone's frame (card RKHUL): it is written in model space at the bind
    // pose, as the drawn model is. [D] BF2's spider walker: through their
    // bones its four foot spheres (r 2.5) sit at y 2.50, on the ground under
    // the drawn feet, and its body sphere 6.19 m up; as stored they all lay
    // within 2.9 m of the origin, and rockets passed through the body.
    let mut collision: Vec<ManifestCollision> = Vec::new();
    let mut collision_index = HashMap::new();
    for g in &geometries {
        let names = std::iter::once(g.name.as_str()).chain(g.lods.iter().map(|(_, m)| m.as_str()));
        for n in names {
            let key = n.to_ascii_lowercase();
            if let Some((m, offset)) = ix.colls.get(&key) {
                let slot =
                    crate::map::collision_slot(&m.name, &mut collision, &mut collision_index);
                if collision[slot].glb.is_none() {
                    collision[slot].node = m.node.clone();
                    collision[slot].glb = crate::map::write_coll(m, out, *offset)?;
                }
            }
            if let Some((p, offset)) = ix.prims.get(&key) {
                let slot =
                    crate::map::collision_slot(&p.model, &mut collision, &mut collision_index);
                if collision[slot].prims.is_empty() {
                    let (prims, unknown) = ix.rest_prims(p, &g.name);
                    if unknown > 0 {
                        eprintln!(
                            "prim 0x{offset:X}: {} dropped {unknown} primitive(s) of an undecoded kind",
                            p.model
                        );
                    }
                    collision[slot].prims = prims;
                }
            }
        }
    }

    let manifest = VehicleManifest {
        source: Path::new(lvl_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default(),
        vehicles,
        geometries,
        models,
        textures,
        collision,
    };
    crate::write_ron(&out.join("vehicles.ron"), &manifest)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_geometry_hash_is_the_property_name() {
        assert_eq!(
            mungefs::fnv1a_lowercase("ChunkGeometryName"),
            PROP_CHUNK_GEOMETRY
        );
    }

    #[test]
    fn animation_name_hash_is_the_property_name() {
        assert_eq!(
            mungefs::fnv1a_lowercase("AnimationName"),
            PROP_ANIMATION_NAME
        );
    }

    /// One primitive-like range of a posed model's `bone` segment(s).
    fn bounds(m: &Model, bone: &str) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for s in m
            .segments
            .iter()
            .filter(|s| s.bone.as_deref() == Some(bone))
        {
            for p in &s.positions {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        (lo, hi)
    }

    fn side(game: &str, lvl: &str) -> Option<Vec<u8>> {
        let p = format!(
            "{}/../../../extracted/{game}/lvl/DATA/_LVL_PS2/SIDE/{lvl}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(p).ok()
    }

    /// Data-gated (card VBIND): a vehicle that is not bone-space wears its
    /// `BNAM` parts on their bones. BF1's hovernaut front barrel, stored at
    /// the origin along +z, sits at its bone 5.55 m up; the Hailfire's two
    /// wheels, stored flat (thin in y) at the origin, stand upright (thin in
    /// x) 3.6 m either side; the hovernaut's unbound hull stays as stored.
    #[test]
    fn a_model_space_vehicle_wears_its_parts_on_their_bones() {
        let (Some(all), Some(cis)) = (side("bf1", "ALL.LVL"), side("bf1", "CIS.LVL")) else {
            eprintln!("SKIP: BF1 SIDE LVLs not present");
            return;
        };
        let chunks = mungefs::content_chunks(mungefs::open(&all).unwrap()).unwrap();
        let ix = index(&chunks).unwrap();
        let stored = &ix.models["all_hover_hovernaut"];
        let (m, _) = ix
            .model_space("all_hover_hovernaut", "all_hover_hovernaut")
            .unwrap();
        let (lo, hi) = bounds(&m, "turret_front_barrel");
        assert!(lo[1] > 5.3 && hi[1] < 5.8, "barrel y {lo:?}..{hi:?}");
        for (a, b) in stored.segments.iter().zip(&m.segments) {
            if a.bone.is_none() {
                assert_eq!(a.positions, b.positions);
            }
        }

        let chunks = mungefs::content_chunks(mungefs::open(&cis).unwrap()).unwrap();
        let ix = index(&chunks).unwrap();
        let (m, _) = ix
            .model_space("cis_tread_hailfire", "cis_tread_hailfire")
            .unwrap();
        for (bone, sign) in [("wheel_left", 1.0f32), ("wheel_right", -1.0)] {
            let (lo, hi) = bounds(&m, bone);
            let mid = (lo[0] + hi[0]) * 0.5 * sign;
            assert!((2.5..4.5).contains(&mid), "{bone} x {lo:?}..{hi:?}");
            assert!(
                hi[0] - lo[0] < hi[1] - lo[1] && hi[1] - lo[1] > 7.0,
                "{bone} upright {lo:?}..{hi:?}"
            );
        }
    }

    #[test]
    fn treaded_is_a_vehicle_and_a_turret_is_not() {
        assert!(is_vehicle_base("Treaded"));
        assert!(is_vehicle_base("commandflyer"));
        assert!(!is_vehicle_base("armedbuilding"));
        assert!(!is_vehicle_base("prop"));
    }

    /// Data-gated (card RKHUL): BF2's spider walker's collision prims rest
    /// on its drawn model at the bind pose. The four foot spheres (r 2.5)
    /// touch the ground the drawn feet stand on, the body sphere sits 6.2 m
    /// up inside the drawn body, and every prim's centre is inside the drawn
    /// model's box - as stored they all lay within 2.9 m of the origin.
    #[test]
    fn a_walkers_prims_rest_on_its_drawn_model() {
        let Some(cis) = side("bf2", "CIS.LVL") else {
            eprintln!("SKIP: BF2 CIS.LVL not present");
            return;
        };
        let chunks = mungefs::content_chunks(mungefs::open(&cis).unwrap()).unwrap();
        let ix = index(&chunks).unwrap();
        let geom = "cis_walk_spider";
        let top = top_lod(&ix, geom).unwrap();
        let (m, _) = ix.model_space(&top, geom).unwrap();
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in m.segments.iter().flat_map(|s| &s.positions) {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let (stored, _) = &ix.prims[geom];
        let (prims, _) = ix.rest_prims(stored, geom);
        assert_eq!(prims.len(), 17);
        for p in &prims {
            let t = p.translation;
            assert!(
                (0..3).all(|k| t[k] > lo[k] - 0.01 && t[k] < hi[k] + 0.01),
                "{} at {t:?} outside the drawn {lo:?}..{hi:?}",
                p.name
            );
        }
        let at = |n: &str| prims.iter().find(|p| p.name == n).unwrap().translation;
        for foot in ["p_foot1", "p_foot2", "p_foot3", "p_foot4"] {
            let t = at(foot);
            assert!(
                (t[1] - 2.5 - lo[1]).abs() < 0.3,
                "{foot} y {} drawn floor {}",
                t[1],
                lo[1]
            );
            assert!(t[0].abs() > 6.5, "{foot} x {}", t[0]);
        }
        assert!((at("p_main")[1] - 6.19).abs() < 0.05);
        // Control: the stored XFRMs are the ones the hull missed by.
        let raw = crate::map::manifest_prims(stored).0;
        let main = raw.iter().find(|p| p.name == "p_main").unwrap();
        assert!(main.translation[1].abs() < 0.01);
    }
}
