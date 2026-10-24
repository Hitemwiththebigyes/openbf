//! Bake one map LVL to an output directory:
//!   models/<name>.glb, textures/<name>.ktx2, manifest.ron, terrain_<name>.glb,
//!   water.glb (map bake, when the map ships `WATR`),
//!   collision/<geometry>.glb (one `coll` chunk each, bare triangle soup) and
//!   the manifest's `collision` + `terrain_cutters` sections,
//!   nav/routes_<layer>.bin + nav/plnp_<layer>.bin with the manifest's `nav`
//!   sections (one per world that authors a plan, a Hint or a BARR),
//!   lowres_<name>.glb (the `LOWR` far-LOD sheet, when the map ships one —
//!   bake::lowres_glb),
//!   foliage.bin + the manifest's `foliage` (the terrain foliage the map's
//!   `tern -> FOLG` and `prp_` place, and the `prp_` path props —
//!   bake::foliage),
//!   common.ron (the `bake common` variant)
//! Terrain is emitted as terrain_<name>.glb through bake::terrain_glb (heights
//! per analysis/re-terrain-heights.md, textured with LTEX layer 0), and its
//! texture layers above 0 as terrain_layers_<name>.glb (card TALPH); a stub
//! `tern` with no heightfield is skipped.

use anyhow::{anyhow, Context, Result};
use std::path::Path;

/// The manifest schema lives in the shared `manifest` crate so the bake and
/// the runtime cannot drift: ONE definition, `Serialize` for this side and
/// `Deserialize` for `game::assets`. Re-exported under their old paths.
///
/// Field order there IS the RON wire format — see that crate.
pub use manifest::{
    CommonManifest, Manifest, ManifestBarrier, ManifestClass, ManifestCollision, ManifestCutter,
    ManifestGeometry, ManifestHint, ManifestInstance, ManifestLayer, ManifestNav, ManifestNavArc,
    ManifestNavHub, ManifestNavPlan, ManifestPath, ManifestPlnp, ManifestPrim, ManifestRegion,
    ManifestWater,
};

/// One world's planning graph, and the two nav sidecars that hold its bulk.
///
/// The route tables are far too big for RON (a single BF1 map's would be
/// megabytes of text), so they go to `nav/routes_<layer>.bin`:
///
/// ```text
/// u16 hubCount; u8 typeCount (5)
/// for t in 0..5, for hub in 0..hubCount:  u8 rows; rows x hubCount bytes
/// ```
///
/// and the inflated `plnp` patches to `nav/plnp_<layer>.bin`:
///
/// ```text
/// u16 recordCount; per record: u32 len; len bytes
/// ```
///
/// `key` is `base` for the base world and the layer's name for a BF2 mode
/// layer. Layer names are NOT unique on the discs (BF2 HOT1 ships two
/// `hoth_objective` layers, NAB2 and YAV1 two `*commondesign` ones — and in
/// each case BOTH carry a plan), so the caller passes a key already made
/// unique; see `unique_nav_key`. A world with NO plan gets no sidecar at all.
fn nav_plan_entry(
    out: &Path,
    key: &str,
    plan: &mungefs::plan::Plan,
    plnp: Option<&mungefs::plan::Plnp>,
) -> Result<ManifestNavPlan> {
    let dir = out.join("nav");
    std::fs::create_dir_all(&dir)?;

    // routes_<key>.bin
    let hub_count = plan.routes.hub_count;
    let mut routes_bin = Vec::new();
    routes_bin.extend((hub_count as u16).to_le_bytes());
    routes_bin.push(5u8);
    for t in 0..5 {
        for h in 0..hub_count {
            let rows = plan.routes.hub_rows(t, h);
            // rowCount is a u8 on the disc (BF2) or at most four unpacked
            // alternatives (BF1), so this never truncates — but say so.
            let n = u8::try_from(rows.len())
                .map_err(|_| anyhow!("nav {key}: hub {h} type {t} has {} rows", rows.len()))?;
            routes_bin.push(n);
            for r in rows {
                routes_bin.extend_from_slice(r);
            }
        }
    }
    let routes_file = format!("routes_{key}.bin");
    std::fs::write(dir.join(&routes_file), routes_bin)?;

    // plnp_<key>.bin
    let plnp = match plnp {
        Some(pp) => {
            let mut bin = Vec::new();
            bin.extend((pp.records.len() as u16).to_le_bytes());
            for r in &pp.records {
                bin.extend((r.len() as u32).to_le_bytes());
                bin.extend_from_slice(r);
            }
            let file = format!("plnp_{key}.bin");
            std::fs::write(dir.join(&file), bin)?;
            Some(ManifestPlnp {
                groups: pp.groups.clone(),
                records: format!("nav/{file}"),
            })
        }
        None => None,
    };

    Ok(ManifestNavPlan {
        hubs: plan
            .hubs
            .iter()
            .map(|h| ManifestNavHub {
                name: h.name.clone(),
                pos: h.pos,
                radius: h.radius,
                // Slots are prefix-packed; 0xff means "no arc here".
                arcs: h
                    .slots
                    .iter()
                    .filter(|&&s| s != 0xff)
                    .map(|&s| u16::from(s))
                    .collect(),
            })
            .collect(),
        arcs: plan
            .arcs
            .iter()
            .map(|a| ManifestNavArc {
                name: a.name.clone(),
                a: a.a,
                b: a.b,
                type_mask: a.type_mask,
                flags: a.flags,
            })
            .collect(),
        routes: format!("nav/{routes_file}"),
        plnp,
    })
}

/// One world's whole nav block: its graph (when it has one) and the `Hint` /
/// `BARR` nodes of the `wrld`s it owns. `None` only when the world authors
/// none of the three — the three are authored independently, and seven BF2
/// maps keep hints in a mode layer that ships no `plan`.
fn nav_block(
    plan: Option<ManifestNavPlan>,
    worlds: &[&mungefs::world::World],
) -> Option<ManifestNav> {
    let hints: Vec<ManifestHint> = worlds
        .iter()
        .flat_map(|w| w.hints.iter())
        .map(|h| ManifestHint {
            name: h.name.clone(),
            kind: h.kind,
            // Rows then translation, AS STORED: a hint's z is NOT flipped.
            xform: [
                h.transform.rotation[0],
                h.transform.rotation[1],
                h.transform.rotation[2],
                h.transform.translation,
            ],
            primary_stance: h.primary_stance,
            secondary_stance: h.secondary_stance,
            mode: h.mode,
            radius: h.radius,
            command_post: h.command_post.clone(),
            props: h
                .props
                .iter()
                .map(|p| (p.key_hash, p.value.clone()))
                .collect(),
        })
        .collect();
    let barriers: Vec<ManifestBarrier> = worlds
        .iter()
        .flat_map(|w| w.barriers.iter())
        .map(|b| ManifestBarrier {
            name: b.name.clone(),
            // The loaders NEGATE z here (BF1 FUN_00177828 / BF2
            // FUN_00178ae8) and the disc only fits that way.
            pos: [
                b.transform.translation[0],
                b.transform.translation[1],
                -b.transform.translation[2],
            ],
            rows: b.transform.rotation,
            size: b.size,
            flags: b.flags,
        })
        .collect();
    if plan.is_none() && hints.is_empty() && barriers.is_empty() {
        return None;
    }
    Some(ManifestNav {
        plan,
        hints,
        barriers,
    })
}

/// A filesystem-safe, unique key for a nav sidecar pair. Layer names repeat on
/// the discs, so a repeat gets `_2`, `_3`, ... in layer order.
fn unique_nav_key(name: &str, used: &mut Vec<String>) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = if safe.is_empty() {
        "layer".to_string()
    } else {
        safe
    };
    let mut key = safe.clone();
    let mut n = 1;
    while used.iter().any(|u| u == &key) {
        n += 1;
        key = format!("{safe}_{n}");
    }
    used.push(key.clone());
    key
}

fn region_entry(r: &mungefs::world::Region) -> ManifestRegion {
    ManifestRegion {
        shape: r.shape.clone(),
        name: r.name.clone(),
        rotation: r.transform.rotation,
        translation: r.transform.translation,
        size: r.size,
        props: r
            .props
            .iter()
            .map(|p| (p.key_hash, p.value.clone()))
            .collect(),
    }
}

fn instance_entry(i: &mungefs::world::Instance) -> ManifestInstance {
    ManifestInstance {
        class: i.class.clone(),
        name: i.name.clone(),
        rotation: i.transform.rotation,
        translation: i.transform.translation,
        props: i
            .props
            .iter()
            .map(|p| (p.key_hash, p.value.clone()))
            .collect(),
    }
}

/// Copy the named `modl`s — and the textures they reference — out of the SIDE
/// LVLs beside this map, into the map's own bake.
///
/// `sky_ SkyObject` names distant capital ships (`imp_fly_destroyer_DOME`,
/// `rep_fly_assault_DOME`, ...) and **their geometry is not in the map's own
/// LVL**. That was VERIFIED rather than assumed: all 17 distinct names the
/// 20 shipped `SkyObject` blocks reference were searched for across every
/// `modl` in every LVL on both discs. On every GROUND map they resolve to a
/// SIDE LVL — `SIDE/ALL.LVL`, `CIS.LVL`, `IMP.LVL`, `REP.LVL` — and only the
/// space maps (SPA2-SPA9, not baked) carry theirs locally. Two names resolve
/// nowhere at all: BF2 COR1 authors an EMPTY geometry, and BF2 KAS2 authors
/// `rcis_fly_gunship_dome`, which exists in no LVL on either disc.
///
/// The SIDE directory is derived from the map's own path
/// (`.../_LVL_PS2/TAT/TAT1.LVL` -> `.../_LVL_PS2/SIDE`) rather than passed
/// in, so no caller has to change. A bake that cannot reach it simply gets
/// no sky objects.
fn pull_side_models(
    lvl_path: &str,
    wanted: &[String],
    out: &Path,
    all_textures: &mut Vec<mungefs::texture::Texture>,
    usage: &mut crate::alpha::AlphaUsage,
) -> Result<Vec<String>> {
    let Some(side_dir) = Path::new(lvl_path)
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("SIDE"))
        .filter(|p| p.is_dir())
    else {
        eprintln!("sky objects: no SIDE directory beside {lvl_path}");
        return Ok(Vec::new());
    };

    let mut still_wanted: Vec<String> = wanted.to_vec();
    let mut found = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&side_dir)?.flatten().collect();
    entries.sort_by_key(|e| e.path()); // deterministic across filesystems
    for entry in entries {
        if still_wanted.is_empty() {
            break;
        }
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lvl"))
        {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let Ok(inner) = mungefs::open(&bytes) else {
            continue;
        };
        // SIDE LVLs nest their content inside `lvl_` sublevel wrappers, so a
        // top-level `chunks` walk finds ZERO models in them —
        // `content_chunks` is the one that descends (same call
        // `bake::soldier` makes).
        let Ok(side_chunks) = mungefs::content_chunks(inner) else {
            continue;
        };
        // A texture is only worth decoding if one of the models we take
        // actually uses it, so walk models first and textures second.
        let mut taken_here: Vec<mungefs::model::Model> = Vec::new();
        for c in &side_chunks {
            if &c.tag != b"modl" {
                continue;
            }
            let Ok(m) = mungefs::model::Model::parse(c) else {
                continue;
            };
            if still_wanted.iter().any(|w| w.eq_ignore_ascii_case(&m.name)) {
                still_wanted.retain(|w| !w.eq_ignore_ascii_case(&m.name));
                taken_here.push(m);
            }
        }
        if taken_here.is_empty() {
            continue;
        }
        let needed: Vec<String> = taken_here
            .iter()
            .flat_map(|m| m.segments.iter())
            .flat_map(|s| s.textures.iter())
            .map(|(_, t)| t.to_ascii_lowercase())
            .collect();
        for c in &side_chunks {
            if &c.tag != b"tex_" {
                continue;
            }
            if let Ok(t) = mungefs::texture::Texture::parse(c) {
                let lower = t.name.to_ascii_lowercase();
                let wanted_tex =
                    needed.contains(&lower) || needed.iter().any(|n| lower == format!("{n}_dtl"));
                if wanted_tex && !all_textures.iter().any(|e| e.name == t.name) {
                    all_textures.push(t);
                }
            }
        }
        // The verdicts for these models are taken on the textures we just
        // pulled in beside them, so the table is built here, after the push.
        let table = crate::alpha::AlphaTable::new(all_textures);
        for m in taken_here {
            let alphas = table.model(&m);
            usage.record_model(&m, &alphas);
            let glb = crate::glb::model_to_glb(&m, &alphas, false)?;
            std::fs::write(out.join("models").join(format!("{}.glb", m.name)), glb)?;
            found.push(m.name);
        }
    }
    if !still_wanted.is_empty() {
        eprintln!(
            "sky objects: no modl for {still_wanted:?} in {}",
            side_dir.display()
        );
    }
    Ok(found)
}

/// One `lvl_` mode-layer wrapper: its worlds, and the AI graph it carries (a
/// wrapper's `plan`, and the `plnp` that follows it inside the same wrapper).
struct ModeLayer {
    group: Vec<mungefs::world::World>,
    plan: Option<mungefs::plan::Plan>,
    plnp: Option<mungefs::plan::Plnp>,
}

/// One `entc` -> the manifest's class row: the render-geometry link, and the
/// collision-layer properties in PROP ORDER (analysis/re-collision-layers.md
/// §2 — a property replaces its own layer's subset with the bodies named so
/// far, so the order is the wire format, not a set). Both the map bake and
/// `bake_common` go through here.
pub(crate) fn class_from_entc(e: &mungefs::entity::EntityClass) -> ManifestClass {
    ManifestClass {
        name: e.name.clone(),
        base: e.base.clone(),
        geometry: e.geometry_name().map(str::to_owned),
        collision: e.collision_layers(),
        // Filled after the walk by `crate::door::attach`, which needs the
        // model's `skel` and the clip's `zaa_` bank.
        door: None,
        // Filled after the walk by `crate::hardpoints::attach`.
        hardpoints: Vec::new(),
        // Filled after the walk by `crate::leafpatch::attach`.
        attach: Vec::new(),
        build_zone: e
            .props
            .iter()
            .find(|p| p.key_hash == manifest::BUILD_ZONE_PROP)
            .map(|p| p.value.clone()),
        // Filled after the walk by `crate::entity_light::attach`.
        lights: Vec::new(),
        // Filled after the walk by `crate::turret_rig::TurretRigs::attach`.
        turret_rig: None,
    }
}

fn bail_no_world(lvl_path: &str) -> Result<()> {
    Err(anyhow!("no wrld chunk in {lvl_path}"))
}

/// Which bake `bake_content` is doing. It decides these things: whether a
/// `wrld` chunk is mandatory (a map's is; a common bake's is synthesized),
/// whether a texture NO segment names keeps its disc alpha (queue item SM's
/// clause — `crate::alpha::keep_alpha`'s `unnamed` input, scoped to the
/// common bakes), and whether the sheets the LVL's own `fx__
/// Effect("Water")` names keep theirs (queue item WA's clause —
/// `keep_alpha`'s `water_named` input, scoped to the map bakes) — and a
/// fourth: only a map bake writes its world models' per-vertex lighting
/// (`crate::glb::model_to_glb`'s `vertex_light`, queue LITEB).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Content {
    /// `COMMON.LVL` / `INGAME.LVL`: the shared models, the HUD sheets.
    Common,
    /// One map LVL.
    Map,
}

pub fn bake_common(lvl_path: &str, outdir: &str, pc_lvl: Option<&str>) -> Result<CommonManifest> {
    let m = bake_content(lvl_path, outdir, Content::Common, pc_lvl)?;
    let cm = CommonManifest {
        source: m.map,
        models: m.models,
        textures: m.textures,
        classes: m.classes,
        geometries: m.geometries,
        collision: m.collision,
        texture_tier2: m.texture_tier2,
    };
    crate::write_ron(&Path::new(outdir).join("common.ron"), &cm)?;
    // BF2's HUD layouts, fonts and extra sheets (card BHUD); a no-op for an
    // LVL with no `hud_` chunk.
    if let Some(h) = crate::hud::bake_hud(lvl_path, outdir)? {
        eprintln!(
            "hud: {} layouts, {} fonts, {} sheets from COMMON/CORE",
            h.layouts.len(),
            h.fonts.len(),
            h.textures.len()
        );
    }
    Ok(cm)
}

/// Bake one map. `pc_lvl` is the matching PC-format LVL, if the local PC
/// install is present: eligible larger textures become optional tier-2
/// siblings beside the PS2 base (see `crate::tier`). `None` bakes PS2-only,
/// as every BF1 map does.
///
/// Beside the manifest it writes [`MAP_CLASSES_FILE`]: the map LVL's own
/// entc/wpnc/ordc/expc PROPS in the side-classes format (`crate::classes`).
/// `ManifestClass` keeps only a class's geometry and collision, and a
/// map-local vehicle (TAT1's `tat_hover_skiff`, BES1's cloud car) has its
/// drive and seat numbers nowhere else (analysis/re-vehicles.md §10 item 2).
/// Always the PS2 LVL's — the PC tier is textures only.
pub fn bake_map(lvl_path: &str, outdir: &str, pc_lvl: Option<&str>) -> Result<Manifest> {
    let m = bake_content(lvl_path, outdir, Content::Map, pc_lvl)?;
    crate::write_ron(&Path::new(outdir).join("manifest.ron"), &m)?;
    let classes = Path::new(outdir).join(MAP_CLASSES_FILE);
    crate::classes::bake_classes(lvl_path, &classes.to_string_lossy())?;
    Ok(m)
}

/// A map's own classes, beside its `manifest.ron` (`game::classdb`'s
/// `ClassDb::load_map` reads it).
pub const MAP_CLASSES_FILE: &str = "classes.ron";

/// Bake everything in an LVL. `kind` says which bake this is: a map's `wrld`
/// chunk is mandatory, a common bake's is synthesized, only a common bake
/// keeps the disc alpha of a texture no segment names (item SM), and only a
/// map bake keeps the disc alpha of the sheets its `Effect("Water")` names
/// (item WA).
/// The `lvl_` mode-layer name contest, written out four times before this
/// (`tex_`, `modl`, `entc`, `gmod`): a layer copy of a name the top level
/// already baked is dropped, counted and announced. `taken` is evaluated by
/// the caller so the lookup still only happens for a layer chunk.
fn layer_loses(
    tag: &'static str,
    offset: usize,
    name: &str,
    taken: bool,
    skips: &mut std::collections::BTreeMap<&'static str, usize>,
) -> bool {
    if !taken {
        return false;
    }
    eprintln!("lvl_ layer {tag} 0x{offset:X}: {name} already baked — first occurrence wins");
    *skips.entry(tag).or_default() += 1;
    true
}

/// The manifest collision row for `name`, created empty on first sight.
/// `coll` (the mesh) and `prim` (the boxes) each fill in their own half and
/// may arrive in either order, so both go through here.
pub(crate) fn collision_slot(
    name: &str,
    collision: &mut Vec<ManifestCollision>,
    index: &mut std::collections::HashMap<String, usize>,
) -> usize {
    *index.entry(name.to_ascii_lowercase()).or_insert_with(|| {
        collision.push(ManifestCollision {
            geometry: name.to_string(),
            node: String::new(),
            glb: None,
            prims: Vec::new(),
        });
        collision.len() - 1
    })
}

/// Write one `coll` mesh as `collision/<name>.glb` (bare triangle soup) and
/// return the manifest's path to it — `None` for a mesh with no polygons.
pub(crate) fn write_coll(
    m: &mungefs::coll::CollisionMesh,
    out: &Path,
    offset: usize,
) -> Result<Option<String>> {
    let tris = m.triangles();
    if tris.is_empty() {
        eprintln!("coll 0x{offset:X}: {} has no polygons", m.name);
        return Ok(None);
    }
    for (leaves, n) in m.repaired() {
        eprintln!(
            "coll 0x{offset:X}: {} leaves {leaves:?} are a signed fan: {n} triangles for the region they sum to",
            m.name
        );
    }
    let file = format!("{}.glb", m.name);
    let glb = crate::glb::triangles_to_glb(&m.name, &m.vertices, &tris)?;
    std::fs::write(out.join("collision").join(&file), glb)?;
    Ok(Some(format!("collision/{file}")))
}

/// A `prim` chunk's bodies as manifest rows, and how many of an undecoded
/// kind were dropped.
pub(crate) fn manifest_prims(p: &mungefs::coll::Primitives) -> (Vec<ManifestPrim>, usize) {
    let mut prims = Vec::new();
    let mut unknown = 0usize;
    for prim in &p.prims {
        let (kind, dims) = match prim.kind {
            mungefs::coll::PrimKind::Sphere { radius } => ("sphere", [radius, 0.0, 0.0]),
            mungefs::coll::PrimKind::Cylinder { radius, height } => {
                ("cylinder", [radius, height, 0.0])
            }
            mungefs::coll::PrimKind::Box { half } => ("box", half),
            mungefs::coll::PrimKind::Unknown(_) => {
                unknown += 1;
                continue;
            }
        };
        prims.push(ManifestPrim {
            name: prim.name.clone(),
            parent: prim.parent.clone(),
            kind: kind.to_owned(),
            dims,
            rotation: prim.rotation,
            translation: prim.translation,
            // BF2's layer mask, already carrying the loader's stale-byte
            // quirk (mungefs::coll::Primitive::mask).
            mask: prim.mask,
            mask_own: prim.mask_own,
        });
    }
    (prims, unknown)
}

/// One `alpha:` census line — sorted, space separated, silent when empty.
/// `sentence` takes the count because each of the four lines puts it in a
/// different place.
fn alpha_report(mut names: Vec<String>, sentence: impl FnOnce(usize) -> String) {
    if names.is_empty() {
        return;
    }
    names.sort();
    eprintln!("alpha: {}: {}", sentence(names.len()), names.join(" "));
}

fn bake_content(
    lvl_path: &str,
    outdir: &str,
    kind: Content,
    pc_lvl: Option<&str>,
) -> Result<Manifest> {
    let pc = pc_lvl
        .map(crate::tier::PcTextures::load)
        .transpose()
        .with_context(|| format!("PC override LVL for {lvl_path}"))?;
    let buf = std::fs::read(lvl_path).with_context(|| lvl_path.to_string())?;
    let inner = mungefs::open(&buf).map_err(|e| anyhow!("{e}"))?;
    let out = Path::new(outdir);
    std::fs::create_dir_all(out.join("models"))?;
    std::fs::create_dir_all(out.join("textures"))?;
    std::fs::create_dir_all(out.join("collision"))?;

    let mut models: Vec<String> = Vec::new();
    // Every baked `modl`, by lowercase name, for the far extension
    // (`crate::far_lowd`) once the gmods are known.
    let mut parsed_models: std::collections::HashMap<String, mungefs::model::Model> =
        std::collections::HashMap::new();
    let mut bf2_models = 0usize;
    let mut all_textures: Vec<mungefs::texture::Texture> = Vec::new();
    // ALPHA RULE v2 (crate::alpha): what the DRAWN segments of this bake ask
    // of each texture's alpha. Replaces the old per-texture OR of the
    // material flag word, which was lossy — see the module doc there.
    let mut alpha_usage = crate::alpha::AlphaUsage::default();
    // The PRE-v2 per-texture rule, kept for ONE purpose: design D7's sky
    // carve-out (`crate::alpha::keep_alpha`). The OR of the material flag word
    // over every segment of every TOP-LEVEL `modl` in this LVL — deliberately
    // NOT including the sky-object models pulled from SIDE LVLs, because the
    // old rule did not either, and D7 has to reproduce it exactly.
    let mut tex_flags: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    // The same OR over the models a BF2 `lvl_` MODE LAYER contributes (design
    // D1). Design D3 folds it into the D7 answer — a layer draw is a draw —
    // but keeps it apart so the bake can say when doing so would have moved a
    // sky texture's alpha away from what D7 reproduced.
    let mut layer_tex_flags: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    // Queue DSHN: the sheets a specular-pass segment of any model here draws
    // (`crate::spec::pass_textures`) keep their disc alpha — the pass's gloss
    // weight (the BF1/BF2 KAM1 decks, DEA1's rooms, common's probe droid…).
    let mut pass_named: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Card FXMF: the muzzle flash model's sheet keeps its disc alpha — it is
    // the flame's whole shape (a white CLUT, mean alpha 38/255), and the
    // flash draws it ADDITIVELY by that alpha (`analysis/re-muzzle-flash.md`
    // §4.3; BF1 MTRL 0x84 and BF2 0x10044 would otherwise be rule 2's
    // OPAQUE).
    let mut flash_named: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut textures = Vec::new();
    let mut transparent: Vec<String> = Vec::new();
    // One entry per texture that HAS a classic-plus override: which source
    // filled tier 2, and the file it went to (`<name>.tier2.ktx2`, beside the
    // PS2 base). See `crate::tier`.
    let mut texture_tiers: Vec<(String, String)> = Vec::new();
    let mut texture_tier2: Vec<(String, String)> = Vec::new();
    let mut classes: Vec<ManifestClass> = Vec::new();
    let mut geometries: Vec<ManifestGeometry> = Vec::new();
    // Worlds: unwrapped (base + always-on design/sound/collision layers) and
    // lvl_-wrapped per-mode layers (BF2), grouped by wrapper.
    let mut worlds: Vec<mungefs::world::World> = Vec::new();
    let mut mode_layers: Vec<ModeLayer> = Vec::new();
    // The base world's AI graph: the LAST top-level `plan`, with the `plnp`
    // that follows it. BF2 frees the previous graph on every 'plan' chunk, so
    // last-loaded wins (KAM1 and MUS1 ship three top-level chunks each);
    // BF1 ships exactly one.
    let mut base_plan: Option<mungefs::plan::Plan> = None;
    let mut base_plnp: Option<mungefs::plan::Plnp> = None;
    let mut terrain_glb = None;
    let mut terrain_parsed: Option<mungefs::terrain::Terrain> = None;
    // The far-LOD sheet is written AFTER the walk, not here: its texture and
    // its UV scale come from `sky_`, and `tern` precedes `sky_` in every one
    // of the 32 shipped LVLs (re-lowr.md §4).
    let mut lowres: Option<(String, mungefs::terrain::LowResSheet, f32)> = None;
    let mut water: Option<mungefs::water::Water> = None;
    let mut water_cfg: Option<mungefs::fx::WaterConfig> = None;
    let mut paths = Vec::new();
    let mut sky_cfg: Option<mungefs::sky::Sky> = None;
    let mut lights_cfg: Option<mungefs::light::Lights> = None;
    // Collision, merged by LOWERCASE geometry name. The Vec keeps first-seen
    // chunk order (the sim visits bodies in manifest order and must not depend
    // on a hash); the map is a lookup index only, never iterated.
    let mut collision: Vec<ManifestCollision> = Vec::new();
    let mut collision_index: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut terrain_cutters: Vec<ManifestCutter> = Vec::new();
    // The terrain foliage (`crate::foliage`): the map's `prp_` file and its
    // terrain's FOLG layer mask, placed once the game is known.
    let mut foliage_file: Option<mungefs::foliage::FoliageFile> = None;
    let mut folg_mask: Option<mungefs::foliage::FolgMask> = None;
    let mut grass_props: Vec<(String, Vec<(u32, String)>)> = Vec::new();
    // Card LEAF (`crate::leafpatch`): every `leafpatch` class's PROPs, and
    // every class's attach PROPs in walk order for the attach replay.
    let mut leaf_props: Vec<(String, Vec<(u32, String)>)> = Vec::new();
    // Card BPNT (`crate::entity_light`): every `Light` class, and the
    // authored omnis and spots of every non-stub `lght`.
    let mut entity_lights: Vec<crate::entity_light::LightClass> = Vec::new();
    let mut point_lights: Vec<manifest::ManifestPointLight> = Vec::new();
    let mut attach_props: Vec<(String, Vec<(u32, String)>)> = Vec::new();

    // PRE-EXPANSION (design D1). BF2 map LVLs wrap each game mode's layer in a
    // `lvl_` whose single child is a hashed sub-level container (A2 §1), and
    // that container holds not only the layer's `wrld`/`plan`/`plnp` but the
    // layer's OWN content: its `entc`, `gmod`, `modl`, `skel`, `coll`, `prim`,
    // `tex_`, `expc`. Walking only the top level therefore missed every prop a
    // mode layer introduces — the Hoth bridges, Polis Massa's 15 classes,
    // Coruscant's 11 — so the manifest had instances naming classes that had no
    // row, no GLB and no collision. The walk below runs over the top-level
    // chunks in file order followed by every wrapper's content in wrapper
    // order, which is exactly the flattening `mungefs::content_chunks` does for
    // the SIDE bake, with the layer bookkeeping kept.
    //
    // A layer's `wrld`/`plan`/`plnp` are consumed HERE, into `ModeLayer`, and
    // never reach the walk (a layer `plan` must not become the base graph). A
    // layer `lght` is dropped: BF2 ships one per sublevel, only the first
    // carries content, and the walk's `lght` arm has already taken it from the
    // top level.
    let side_buf = match kind {
        Content::Map => crate::side_classes::side_classes_lvl(lvl_path)
            .map(|p| std::fs::read(&p).with_context(|| p.display().to_string()))
            .transpose()?,
        Content::Common => None,
    };
    let mut expanded: Vec<mungefs::Chunk<'_>> = Vec::new();
    let mut layer_chunks: Vec<mungefs::Chunk<'_>> = Vec::new();
    let mut wrappers = 0usize;
    for c in mungefs::chunks(inner) {
        let c = c.map_err(|e| anyhow!("{e}"))?;
        if &c.tag != b"lvl_" {
            expanded.push(c);
            continue;
        }
        wrappers += 1;
        // Sub-level wrappers carry hashed tags; a hash can happen to be
        // printable ASCII (TAT2's hunt layer = 0x2A635340 "@Sc*"), so never
        // use tag_is_ascii here — just look at what is inside.
        for sub in c.children().flatten() {
            if &sub.tag == b"wrld" {
                // Content directly under `lvl_` rather than under a hashed
                // container: an unwrapped world, handled by the walk's own arm
                // (no map on either disc ships one, but the shape is legal).
                layer_chunks.push(sub);
                continue;
            }
            let mut group = Vec::new();
            let mut plan = None;
            let mut plnp = None;
            for item in sub.children().flatten() {
                match &item.tag {
                    b"wrld" => group.push(
                        mungefs::world::World::parse(&item)
                            .map_err(|e| anyhow!("wrld 0x{:X}: {e}", item.offset))?,
                    ),
                    b"plan" => {
                        plan = Some(
                            mungefs::plan::Plan::parse(&item)
                                .map_err(|e| anyhow!("plan 0x{:X}: {e}", item.offset))?,
                        );
                        plnp = None;
                    }
                    b"plnp" => {
                        if plan.is_some() {
                            plnp = Some(
                                mungefs::plan::Plnp::parse(&item)
                                    .map_err(|e| anyhow!("plnp 0x{:X}: {e}", item.offset))?,
                            );
                        }
                    }
                    b"lght" => {}
                    _ => layer_chunks.push(item),
                }
            }
            if !group.is_empty() {
                mode_layers.push(ModeLayer { group, plan, plnp });
            }
        }
    }
    // Card VTUR2: the building-turret classes a world places from BF2's
    // SIDE/TUR.LVL join the walk as layer content (losing every by-name
    // contest to the map's own), so their models, collision, skel nodes and
    // manifest rows bake like the map's. Their PROPs go to the side-classes
    // file `bf2_tur.ron` (`bake classes`), not to this map's classes.ron.
    if let Some(side) = side_buf.as_deref() {
        let mut placed = std::collections::BTreeSet::new();
        let mut defined = std::collections::HashSet::new();
        for c in expanded.iter().chain(&layer_chunks) {
            match &c.tag {
                b"wrld" => {
                    if let Ok(w) = mungefs::world::World::parse(c) {
                        placed.extend(w.instances.iter().map(|i| i.class.to_ascii_lowercase()));
                    }
                }
                b"entc" => {
                    if let Ok(e) = mungefs::entity::EntityClass::parse(c) {
                        defined.insert(e.name.to_ascii_lowercase());
                    }
                }
                _ => {}
            }
        }
        for w in mode_layers.iter().flat_map(|l| &l.group) {
            placed.extend(w.instances.iter().map(|i| i.class.to_ascii_lowercase()));
        }
        let (pulled, taken) = crate::side_classes::pull_side_classes(side, &placed, &defined)?;
        if !taken.is_empty() {
            eprintln!(
                "side classes: {} chunk(s) from SIDE/TUR.LVL for {}",
                pulled.len(),
                taken.join(" ")
            );
        }
        layer_chunks.extend(pulled);
    }
    let top_level = expanded.len();
    expanded.extend(layer_chunks);
    if expanded.len() > top_level {
        eprintln!(
            "lvl_ layers: {wrappers} wrapper(s) contributed {} content chunk(s)",
            expanded.len() - top_level
        );
    }
    // Everything at index >= `top_level` came out of a mode layer, and loses
    // every by-name contest against content the top level already baked
    // (design D2: first wins, and the top level goes first).
    let from_layer = |i: usize| i >= top_level;
    // Per-tag counts of layer content that lost such a contest, for the report.
    let mut layer_skips: std::collections::BTreeMap<&'static str, usize> = Default::default();

    // Textures FIRST, in their own pass: ALPHA RULE v2 judges each drawn
    // segment against the alpha of the texture it names, so every `tex_` must
    // be decoded before the first `modl` is written. Detail companions are
    // paired with their base after the walk (crate::detail), so nothing is
    // written here.
    for (i, c) in expanded.iter().enumerate() {
        if &c.tag != b"tex_" {
            continue;
        }
        match mungefs::texture::Texture::parse(c) {
            Ok(t) => {
                let dup = from_layer(i)
                    && all_textures
                        .iter()
                        .any(|e| e.name.eq_ignore_ascii_case(&t.name));
                if layer_loses("tex_", c.offset, &t.name, dup, &mut layer_skips) {
                    continue;
                }
                all_textures.push(t);
            }
            Err(e) => eprintln!("skipping tex_ 0x{:X}: {e}", c.offset),
        }
    }
    let alpha_table = crate::alpha::AlphaTable::new(&all_textures);
    // Card TBIND (`crate::bind_pose`): a model with a `skel` is written in
    // its bind pose, so a turret's head and gun sit on their bones.
    let bind_pose = crate::bind_pose::BindPose::of(&expanded);
    let mut posed: Vec<String> = Vec::new();
    // Card VTURD (`crate::turret_rig`): an armed building's model is written
    // as one primitive per (segment, node), so its gun can turn.
    let turret_rigs = crate::turret_rig::TurretRigs::of(&expanded);
    // Card FXP1 (`crate::effects`): this layer's `fx__` particle effects,
    // collected before the texture write because the sheets they name are
    // drawn blended by their alpha (re-fx §6.7) and so keep it.
    let fx = crate::effects::Collected::from_chunks(expanded.iter())?;
    let fx_named = fx.textures();
    // Card GLOW: BF2's `Effect("HDR")`, read before the models because it
    // decides how their self-lit segments are written (`crate::glb::
    // glow_path`); the first `fx__` that carries one. Only an enabled one
    // is kept — a disabled HDR draws nothing differently.
    let mut hdr = None;
    for c in expanded.iter().filter(|c| &c.tag == b"fx__") {
        if let Some(h) = mungefs::fx::HdrConfig::parse(c)
            .map_err(|e| anyhow!("fx__ 0x{:X} Effect(\"HDR\"): {e}", c.offset))?
        {
            hdr = Some(h);
            break;
        }
    }
    let hdr = hdr.filter(|h| h.enable && kind == Content::Map);
    if let Some(h) = hdr {
        eprintln!(
            "hdr: on ({} bloom passes, weight {}, threshold {})",
            h.num_bloom_passes, h.max_total_weight, h.glow_threshold
        );
    }
    // Card KSKY: `Effect("Lightning")`, the first `fx__` that carries one.
    // Only an enabled one is kept (BF1 KAM1, BF1 YAV2, BF2 KAM1) — a disabled
    // effect has no tick and darkens nothing, so the other maps' manifests
    // do not move. Its `LightningBolt` is parsed by `mungefs::fx` and not
    // carried: nothing draws a bolt yet (card WTHL).
    let mut lightning = None;
    for c in expanded.iter().filter(|c| &c.tag == b"fx__") {
        if let Some(l) = mungefs::fx::LightningConfig::parse(c)
            .map_err(|e| anyhow!("fx__ 0x{:X} Effect(\"Lightning\"): {e}", c.offset))?
        {
            lightning = Some(l);
            break;
        }
    }
    let lightning = lightning
        .filter(|l| l.enable && kind == Content::Map)
        .map(|l| {
            eprintln!(
                "lightning: on (sky dome darken {}, dome byte {:?})",
                l.sky_dome_darken_factor,
                l.resting_dome_colour()
            );
            manifest::ManifestLightning {
                sky_dome_darken_factor: l.sky_dome_darken_factor,
                sunlight_fade_factor: l.sunlight_fade_factor,
                color: l.color,
                brightness_min: l.brightness_min,
                fade_time: l.fade_time,
                time_between_flashes: l.time_between_flashes,
                time_between_sub_flashes: l.time_between_sub_flashes,
                num_sub_flashes: l.num_sub_flashes,
                horizon_angle: l.horizon_angle,
                sound_crack: l.sound_crack,
                sound_sub_crack: l.sound_sub_crack,
            }
        });
    // Card WTHR: `Effect("Precipitation")` and `Effect("Wind")`, the first
    // `fx__` that carries each; kept only when enabled (re-weather.md §1:
    // nine maps and four), so every other map's manifest does not move.
    let mut precipitation = None;
    let mut wind = None;
    for c in expanded.iter().filter(|c| &c.tag == b"fx__") {
        if precipitation.is_none() {
            precipitation = mungefs::fx::PrecipitationConfig::parse(c)
                .map_err(|e| anyhow!("fx__ 0x{:X} Effect(\"Precipitation\"): {e}", c.offset))?;
        }
        if wind.is_none() {
            wind = mungefs::fx::WindConfig::parse(c)
                .map_err(|e| anyhow!("fx__ 0x{:X} Effect(\"Wind\"): {e}", c.offset))?;
        }
    }
    let precipitation = precipitation
        .filter(|p| p.enable && kind == Content::Map)
        .map(|p| {
            eprintln!(
                "precipitation: on ({}, {} particles, range {} m)",
                if p.streaks { "streaks" } else { "quads" },
                p.particle_density,
                p.range
            );
            manifest::ManifestPrecipitation {
                streaks: p.streaks,
                texture: p.texture,
                particle_size: p.particle_size,
                range: p.range,
                color: p.color,
                alpha_min_max: p.alpha_min_max,
                particle_density: p.particle_density,
                particle_density_range: p.particle_density_range,
                velocity: p.velocity,
                velocity_range: p.velocity_range,
                streak_length: p.streak_length,
                camera_cross_velocity_scale: p.camera_cross_velocity_scale,
                camera_axial_velocity_scale: p.camera_axial_velocity_scale,
                rotation: p.rotation,
                rotation_range: p.rotation_range,
                ground_effect: p.ground_effect,
                ground_effects_per_sec: p.ground_effects_per_sec,
                ground_effect_spread: p.ground_effect_spread,
            }
        });
    // A flake's sheet is drawn blended by its alpha (re-weather.md §3.4:
    // MODULATE with texture alpha), so it keeps it.
    let precip_named: Option<String> = precipitation
        .as_ref()
        .and_then(|p| p.texture.as_ref().map(|t| t.to_ascii_lowercase()));
    let wind = wind
        .filter(|w| w.enable && kind == Content::Map)
        .map(|w| manifest::ManifestWind {
            velocity: w.velocity,
            velocity_range: w.velocity_range,
            velocity_change_rate: w.velocity_change_rate,
        });
    // The sheets a self-lit segment draws keep their alpha: the glow mask.
    let mut glow_named: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (i, c) in expanded.iter().enumerate() {
        match &c.tag {
            b"sky_" if sky_cfg.is_none() => {
                sky_cfg = Some(
                    mungefs::sky::Sky::parse(c)
                        .map_err(|e| anyhow!("sky_ 0x{:X}: {e}", c.offset))?,
                );
            }
            // The water's real config: a map carries 3-9 `fx__` chunks and
            // at most one of them holds the `Effect("Water")` (only the 12
            // water maps hold any), so this scans until one parses.
            b"fx__" if water_cfg.is_none() => {
                water_cfg = mungefs::fx::WaterConfig::parse(c)
                    .map_err(|e| anyhow!("fx__ 0x{:X} Effect(\"Water\"): {e}", c.offset))?;
            }
            // BF2 ships one `lght` per mode sublevel and only the first
            // carries content; the stubs are 12 bytes (NAME only).
            b"lght" if c.payload.len() > 12 => {
                let l = mungefs::light::Lights::parse(c)
                    .map_err(|e| anyhow!("lght 0x{:X}: {e}", c.offset))?;
                // Card BPNT: every non-stub chunk's omnis and spots are
                // local lights (DEA1, FEL1 and UTA1 ship three chunks).
                point_lights.extend(crate::lighting::point_lights_from(&l));
                if lights_cfg.is_none() {
                    lights_cfg = Some(l);
                }
            }
            b"tex_" => {}
            b"modl" => {
                let mut m = mungefs::model::Model::parse(c)
                    .map_err(|e| anyhow!("modl 0x{:X}: {e}", c.offset))?;
                let dup = from_layer(i) && models.iter().any(|n| n.eq_ignore_ascii_case(&m.name));
                if layer_loses("modl", c.offset, &m.name, dup, &mut layer_skips) {
                    continue;
                }
                match bind_pose.pose(&m) {
                    Ok(Some(p)) => {
                        posed.push(p.name.clone());
                        m = p;
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("bind pose: {e} — {} stays as stored", m.name),
                }
                match turret_rigs.split(&m) {
                    Ok(Some(p)) => m = p,
                    Ok(None) => {}
                    Err(e) => eprintln!("turret rig: {e} — written whole"),
                }
                let flags = if from_layer(i) {
                    &mut layer_tex_flags
                } else {
                    &mut tex_flags
                };
                for seg in &m.segments {
                    for (_, t) in &seg.textures {
                        *flags.entry(t.to_ascii_lowercase()).or_default() |= seg.material_flags;
                    }
                }
                pass_named.extend(crate::spec::pass_textures([&m]));
                flash_named.extend(crate::alpha::flash_textures(&m));
                glow_named.extend(crate::alpha::glow_textures(&m, hdr.is_some()));
                let alphas = alpha_table.model(&m);
                alpha_usage.record_model(&m, &alphas);
                let glb = if kind == Content::Map {
                    crate::glb::world_model_to_glb(&m, &alphas, hdr.is_some())?
                } else if m.name.to_ascii_lowercase().starts_with("hud_") {
                    // A HUD mesh keeps its vertex colours (card BHUD2).
                    crate::glb::hud_model_to_glb(&m, &alphas)?
                } else {
                    crate::glb::model_to_glb(&m, &alphas, false)?
                };
                std::fs::write(out.join("models").join(format!("{}.glb", m.name)), glb)?;
                if m.is_bf2() {
                    bf2_models += 1;
                }
                models.push(m.name.clone());
                parsed_models.insert(m.name.to_ascii_lowercase(), m);
            }
            b"tern" => {
                // WATR first, and independently of the heightfield: BF2 DEA1
                // and BF2 KAM1 carry a STUB tern whose only content is the
                // water, so it is the one ground plane those maps have.
                if water.is_none() {
                    water = mungefs::water::Water::parse(c)
                        .map_err(|e| anyhow!("tern 0x{:X} WATR: {e}", c.offset))?;
                }
                if mungefs::terrain::Terrain::is_stub(c) {
                    // BF2 model-built maps (COR1/DEA1/MYG1/TAN1/UTA1) carry a
                    // NAME-only tern: no heightfield to bake.
                    eprintln!("tern 0x{:X}: stub (no heightfield)", c.offset);
                } else {
                    let mut t = mungefs::terrain::Terrain::parse(c)
                        .map_err(|e| anyhow!("tern 0x{:X}: {e}", c.offset))?;
                    if terrain_cutters.is_empty() {
                        terrain_cutters = t
                            .cutters
                            .iter()
                            .map(|k| ManifestCutter {
                                sphere: k.sphere,
                                planes: k.planes.clone(),
                            })
                            .collect();
                    }
                    if let Some(sheet) = t.lowres.take() {
                        let half = t.info.grid_unit * t.info.grid_size as f32 / 2.0;
                        lowres = Some((t.name.clone(), sheet, half));
                    }
                    // Written after the loop: its vertex colour is per game
                    // (`terrain_glb::gs_vertex_byte`), and the game is known
                    // only once every model has been seen.
                    terrain_glb = Some(format!("terrain_{}.glb", t.name));
                    // At the payload's own size here; `crate::foliage`
                    // checks it against the game's grid.
                    folg_mask = mungefs::foliage::FolgMask::payload_dim(c)
                        .and_then(|d| mungefs::foliage::FolgMask::parse(c, d));
                    terrain_parsed = Some(t);
                }
            }
            b"coll" => {
                let m = mungefs::coll::CollisionMesh::parse(c)
                    .map_err(|e| anyhow!("coll 0x{:X}: {e}", c.offset))?;
                let slot = collision_slot(&m.name, &mut collision, &mut collision_index);
                if collision[slot].glb.is_some() {
                    eprintln!(
                        "coll 0x{:X}: duplicate collision mesh for {} — first occurrence wins",
                        c.offset, m.name
                    );
                    continue;
                }
                collision[slot].node = m.node.clone();
                collision[slot].glb = write_coll(&m, out, c.offset)?;
            }
            b"prim" => {
                let p = mungefs::coll::Primitives::parse(c)
                    .map_err(|e| anyhow!("prim 0x{:X}: {e}", c.offset))?;
                let slot = collision_slot(&p.model, &mut collision, &mut collision_index);
                if !collision[slot].prims.is_empty() {
                    eprintln!(
                        "prim 0x{:X}: duplicate primitives for {} — first occurrence wins",
                        c.offset, p.model
                    );
                    continue;
                }
                let (mut prims, unknown) = manifest_prims(&p);
                // STDOR / RKHULT: door and map-local vehicle prims rest at bind.
                bind_pose.rest_prims(&p.model, &mut prims);
                collision[slot].prims = prims;
                if unknown > 0 {
                    eprintln!(
                        "prim 0x{:X}: {} dropped {unknown} primitive(s) of an undecoded kind",
                        c.offset, p.model
                    );
                }
            }
            b"entc" => {
                let e = mungefs::entity::EntityClass::parse(c)
                    .map_err(|e| anyhow!("entc 0x{:X}: {e}", c.offset))?;
                // `resolve_class` takes the FIRST row of a name anyway, so a
                // layer class that repeats a top-level one is dropped rather
                // than left as an unreachable duplicate.
                let dup =
                    from_layer(i) && classes.iter().any(|k| k.name.eq_ignore_ascii_case(&e.name));
                if layer_loses("entc", c.offset, &e.name, dup, &mut layer_skips) {
                    continue;
                }
                // A grass patch class keeps its PROPs for the foliage bake
                // (`crate::foliage::grass_class`); BF2 FEL1 spells the BASE
                // `Grasspatch`.
                if e.base.eq_ignore_ascii_case("grasspatch") {
                    grass_props.push((
                        e.name.clone(),
                        e.props
                            .iter()
                            .map(|p| (p.key_hash, p.value.clone()))
                            .collect(),
                    ));
                }
                if crate::entity_light::is_light(&e.base) {
                    let p: Vec<(u32, String)> = e
                        .props
                        .iter()
                        .filter(|p| crate::entity_light::is_light_prop(p.key_hash))
                        .map(|p| (p.key_hash, p.value.clone()))
                        .collect();
                    entity_lights.push(crate::entity_light::light_class(&e.name, &p));
                }
                if crate::leafpatch::is_leafpatch(&e.base) {
                    leaf_props.push((
                        e.name.clone(),
                        e.props
                            .iter()
                            .map(|p| (p.key_hash, p.value.clone()))
                            .collect(),
                    ));
                }
                attach_props.push((
                    e.name.clone(),
                    e.props
                        .iter()
                        .filter(|p| crate::leafpatch::is_attach_prop(p.key_hash))
                        .map(|p| (p.key_hash, p.value.clone()))
                        .collect(),
                ));
                classes.push(class_from_entc(&e));
            }
            b"gmod" => {
                let g = mungefs::entity::GeomModel::parse(c)
                    .map_err(|e| anyhow!("gmod 0x{:X}: {e}", c.offset))?;
                // Keyed by `name`: that is what `Scene::resolve_model` looks a
                // class's GeometryName up by, so first-by-name is exactly the
                // row the runtime would have used.
                let dup = from_layer(i)
                    && geometries
                        .iter()
                        .any(|k| k.name.eq_ignore_ascii_case(&g.name));
                if layer_loses("gmod", c.offset, &g.name, dup, &mut layer_skips) {
                    continue;
                }
                let Some(best) = g.best_model() else {
                    continue; // no LOD entries — nothing drawable
                };
                geometries.push(ManifestGeometry {
                    model: best.to_owned(),
                    lods: g
                        .lods
                        .iter()
                        .map(|l| (l.tier.clone(), l.model.clone()))
                        .collect(),
                    name: g.name,
                    far: None,
                });
            }
            b"prp_" if foliage_file.is_none() => {
                foliage_file = Some(
                    mungefs::foliage::FoliageFile::parse(c)
                        .map_err(|e| anyhow!("prp_ 0x{:X}: {e}", c.offset))?,
                );
            }
            b"PATH" => {
                for p in mungefs::path::parse_all(c)
                    .map_err(|e| anyhow!("PATH 0x{:X}: {e}", c.offset))?
                {
                    paths.push(ManifestPath {
                        name: p.name,
                        nodes: p.nodes.iter().map(|n| (n.pos, n.rot)).collect(),
                        props: p.props,
                    });
                }
            }
            b"path" => {
                // BF2: top-level config-format path files.
                if mungefs::path::is_config_path(c) {
                    for p in mungefs::path::parse_config_paths(c)
                        .map_err(|e| anyhow!("path 0x{:X}: {e}", c.offset))?
                    {
                        paths.push(ManifestPath {
                            name: p.name,
                            nodes: p.nodes.iter().map(|n| (n.pos, n.rot)).collect(),
                            props: p.props,
                        });
                    }
                }
            }
            b"plan" => {
                base_plan = Some(
                    mungefs::plan::Plan::parse(c)
                        .map_err(|e| anyhow!("plan 0x{:X}: {e}", c.offset))?,
                );
                base_plnp = None; // a plnp belongs to the plan BEFORE it
            }
            b"plnp" if base_plan.is_some() => {
                base_plnp = Some(
                    mungefs::plan::Plnp::parse(c)
                        .map_err(|e| anyhow!("plnp 0x{:X}: {e}", c.offset))?,
                );
            }
            b"wrld" => {
                worlds.push(
                    mungefs::world::World::parse(c)
                        .map_err(|e| anyhow!("wrld 0x{:X}: {e}", c.offset))?,
                );
            }
            _ => {}
        }
    }
    if !posed.is_empty() {
        eprintln!(
            "bind pose: {} model(s) posed ({})",
            posed.len(),
            posed.join(" ")
        );
    }
    // The far scene's LOWD extension (`crate::far_lowd`, card UTAS): what a
    // geometry's LOWD draws beyond its LOD0, as one more model.
    for g in &mut geometries {
        let tier = |t: &str| {
            g.lods
                .iter()
                .find(|(k, _)| k == t)
                .and_then(|(_, m)| parsed_models.get(&m.to_ascii_lowercase()))
        };
        let (Some(lod0), Some(lowd)) = (tier("LOD0"), tier("LOWD")) else {
            continue;
        };
        if let Some(ext) = crate::far_lowd::extension(lod0, lowd) {
            let alphas = alpha_table.model(&ext);
            let glb = if kind == Content::Map {
                crate::glb::world_model_to_glb(&ext, &alphas, hdr.is_some())?
            } else {
                crate::glb::model_to_glb(&ext, &alphas, false)?
            };
            std::fs::write(out.join("models").join(format!("{}.glb", ext.name)), glb)?;
            eprintln!(
                "far_lowd: {} draws {} of {}'s triangles past NearSceneRange[1] as {}",
                g.name, ext.face_count, lowd.name, ext.name
            );
            models.push(ext.name.clone());
            g.far = Some(ext.name);
        }
    }
    if !layer_skips.is_empty() {
        eprintln!(
            "lvl_ layers: {} chunk(s) skipped for a name the top level already baked ({})",
            layer_skips.values().sum::<usize>(),
            layer_skips
                .iter()
                .map(|(t, n)| format!("{t} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // BF2 doors (queue DR2): each `door`-based class gets its rig — the
    // trigger spheres and the `open` clip's motion per prim (crate::door).
    for (name, rig) in crate::door::rigs(&expanded) {
        match rig {
            Ok(d) => {
                if let Some(k) = classes
                    .iter_mut()
                    .find(|k| k.name.eq_ignore_ascii_case(&name))
                {
                    k.door = Some(d);
                }
            }
            Err(e) => eprintln!("door {name}: no rig ({e}) — it stays shut"),
        }
    }

    // Card VTUR: a vehicle's or armed building's skel nodes (fire points).
    crate::hardpoints::attach(&expanded, classes.iter_mut());
    // Card VTURD: an armed building's node rig over its written model.
    for line in turret_rigs.attach(&parsed_models, classes.iter_mut()) {
        eprintln!("turret rig: {line}");
    }
    drop(parsed_models);

    // Textures: write each one, plus a `<base>.dtl.ktx2` companion for every
    // detailCompressed base (crate::detail). A companion consumed that way is
    // dropped from the plain texture list — it is a modulation layer, not a
    // material of its own — but an orphan `_dtl` (no flagged base) still bakes
    // normally.
    let tex_dir = out.join("textures");
    // `sky_ SkyObject` names capital ships that do NOT live in the map's own
    // LVL — VERIFIED by searching every LVL on both discs for all 17 shipped
    // names: on every GROUND map they are in a SIDE LVL (ALL/CIS/IMP/REP),
    // and only the space maps carry theirs locally. Pull them in so the
    // manifest's `have_model` check below can see them.
    if let Some(sky) = sky_cfg.as_ref() {
        let wanted: Vec<String> = sky
            .objects
            .iter()
            .map(|o| o.geometry.clone())
            .filter(|g| !g.is_empty() && !models.iter().any(|m| m.eq_ignore_ascii_case(g)))
            .collect();
        if !wanted.is_empty() {
            let pulled =
                pull_side_models(lvl_path, &wanted, out, &mut all_textures, &mut alpha_usage)?;
            models.extend(pulled);
        }
    }

    // The manifest's sky, resolved HERE rather than beside the rest of the
    // manifest: design D7 needs to know which models and which texture the sky
    // path uses before the KTX2s are written, and `models` is final by now
    // (the walk is over and `pull_side_models` has run).
    let sky = crate::lighting::sky_from(sky_cfg.as_ref(), |g| {
        models.iter().find(|m| m.eq_ignore_ascii_case(g)).cloned()
    });

    // The far-LOD sheet, now that `sky_` has been walked: its texture is
    // `DomeInfo.LowResTerrain.Texture` and its UV scale that scope's
    // `TextureScale` (`bake::lowres_glb`). Written whenever the `tern` SHIPS
    // one, including on BF2 DAG1/NAB2, whose sheet the disc never draws
    // because they author no `LowResTerrain` scope at all — the manifest says
    // that with `sky.low_res_terrain: None`, and the bake stays a
    // transcription of what the disc holds.
    let lrt = sky_cfg
        .as_ref()
        .and_then(|s| s.dome.as_ref())
        .and_then(|d| d.low_res_terrain.as_ref());
    let lowres_glb = match lowres {
        Some((ref name, ref sheet, half)) => crate::lowres_glb::lowres_to_glb(
            name,
            sheet,
            half,
            lrt.and_then(|l| l.texture.as_deref()),
            lrt.and_then(|l| l.texture_scale)
                .unwrap_or(crate::lowres_glb::DEFAULT_TEXTURE_SCALE),
        )?
        .map(|glb| -> Result<String> {
            let fname = format!("lowres_{name}.glb");
            std::fs::write(out.join(&fname), glb)?;
            Ok(fname)
        })
        .transpose()?,
        None => None,
    };
    // D7: the sky path is out of scope for ALPHA RULE v2. `sky_models` are the
    // DOME shells (`ManifestSky.models`); `sky_textures` is every texture their
    // drawn segments name, plus the dome's own `DomeInfo.Texture`.
    let sky_models: std::collections::HashSet<String> = sky
        .as_ref()
        .map(|s| {
            s.models
                .iter()
                .map(|m| m.geometry.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    let mut sky_textures: std::collections::HashSet<String> = sky_models
        .iter()
        .flat_map(|m| alpha_usage.textures_of(m).map(str::to_owned))
        .collect();
    if let Some(t) = sky.as_ref().and_then(|s| s.texture.as_ref()) {
        sky_textures.insert(t.to_ascii_lowercase());
    }

    // Textures whose tier-2 override carries the PS2 alpha channel instead of
    // its own, because a draw depends on that alpha (design D3). Reported by
    // the bake so the count stays visible.
    let mut pc_alpha_substitutions: Vec<String> = Vec::new();
    // D7 report: sky textures whose KTX2 alpha state the carve-out changes
    // relative to D3 alone, and textures BOTH a sky shell and a world model
    // draw (where the two rules can disagree).
    let mut sky_overrides: Vec<String> = Vec::new();
    let mut sky_and_world: Vec<String> = Vec::new();
    // D3's carve-out check: sky textures whose KTX2 alpha state moved because
    // the mode layers' models joined the pre-v2 OR. Expected EMPTY on every
    // map — if it is not, design D3 says to take the layer models back out.
    let mut layer_flag_moves: Vec<String> = Vec::new();
    // Queue item SM: textures NO segment names whose disc alpha is not all
    // 255 — the ones `keep_alpha`'s `unnamed` clause keeps an alpha for that
    // D3 alone would have forced opaque. Reported so the count stays visible.
    let mut unnamed_kept: Vec<String> = Vec::new();
    // Queue item WA: the lower-cased names THIS map's `fx__ Effect("Water")`
    // gives its water layers — `MainTexture` (all twelve wet maps author one)
    // and `FoamTexture` (both KAM1s, nobody else). Those sheets keep their
    // disc alpha; every other sheet in the bake is unaffected. Empty on the
    // 22 dry maps, which author no `Effect("Water")` at all, and asked only
    // of a MAP bake so the clause names the bake it applies to, exactly as
    // `unnamed` does for the common ones.
    //
    // The config's OWN names, not `water_glb::resolve_texture`'s answer: the
    // fallbacks that function carries (`LAYR`, then the map's single
    // `*_water`) are guesses for data neither disc ships, and a guess must
    // not decide what a baked file contains. All twelve shipped maps author a
    // `MainTexture`, so the two agree on every map that exists.
    let water_named_textures: std::collections::BTreeSet<String> = match (kind, &water_cfg) {
        (Content::Map, Some(c)) => [c.main_texture.as_deref(), c.foam_texture.as_deref()]
            .into_iter()
            .flatten()
            .map(|n| n.to_ascii_lowercase())
            .collect(),
        _ => Default::default(),
    };
    // Queue CLN5 (2026-09-25): the drawn `LOWR` sheet's texture keeps its
    // disc alpha too — the PS2 alpha-tests that draw at AREF 0x10
    // (`analysis/re-lowr.md` §2.2 [V]) and BF1 KAS1's and KAS2's sheets carry
    // real alpha (`crate::alpha::keep_alpha`, `consumer_named`). Only a sheet
    // the map both SHIPS and names in a `LowResTerrain` scope: BF2 DAG1/NAB2's
    // undrawn ones name nothing.
    let sheet_named: Option<String> = match (kind, &lowres) {
        (Content::Map, Some(_)) => lrt
            .and_then(|l| l.texture.as_deref())
            .map(|n| n.to_ascii_lowercase()),
        _ => None,
    };
    // Queue FOLGG: the sheets this map's `grasspatch` classes name
    // (`Texture`, default "grass" — `crate::foliage::grass_class`) keep their
    // disc alpha too: every grass sprite is a cutout composited by it
    // (analysis/re-foliage.md §7; the sprites are rectangles without it). No
    // model names these sheets, so without the clause they bake opaque.
    // Card LEAF: so do the `leafpatch` classes' sheets — the PS2 blends every
    // leaf sprite by its texel alpha (analysis/re-leafpatch.md §6.5).
    let grass_named: std::collections::BTreeSet<String> = match kind {
        Content::Map => grass_props
            .iter()
            .map(|(name, props)| {
                crate::foliage::grass_class(name, props)
                    .texture
                    .to_ascii_lowercase()
            })
            .chain(leaf_props.iter().map(|(name, props)| {
                crate::leafpatch::leaf_class(name, props)
                    .texture
                    .to_ascii_lowercase()
            }))
            .collect(),
        _ => Default::default(),
    };
    let mut water_alpha_kept: Vec<String> = Vec::new();
    let mut consumed: Vec<String> = Vec::new();
    for t in &all_textures {
        // Keep the existing companion writes/consumption order. Absent
        // companions are removed only after their base is successfully emitted.
        if crate::detail::companion(t, &all_textures).is_none() {
            continue;
        }
        if let Some(name) = crate::detail::write_detail_for(&tex_dir, t, &all_textures)? {
            consumed.push(name);
        }
    }
    for t in &all_textures {
        if consumed.contains(&t.name.to_ascii_lowercase()) {
            continue;
        }
        let lower = t.name.to_ascii_lowercase();
        // ALPHA RULE v2, design D3: the baked file keeps its alpha iff at
        // least one DRAW in this bake was judged MASK or BLEND. A texture a
        // segment names but no such draw needs bakes opaque.
        //
        // A texture NO segment names at all keeps its disc alpha (queue item
        // SM): it is not a world texture, and the PS2 composites the HUD
        // sheets among them by exactly that alpha (`analysis/re-hud-icons.md`
        // §2 [V]) — see `crate::alpha::keep_alpha`.
        //
        // Design D7: unless the SKY path uses it, in which case the pre-v2
        // answer stands — the sky pipeline composites by alpha and never reads
        // the material, so the rule must not reach it.
        let is_sky = sky_textures.contains(&lower);
        let top_or_flags = tex_flags.get(&lower).copied();
        let layer_or_flags = layer_tex_flags.get(&lower).copied();
        // No segment of any top-level or mode-layer model names it — and, the
        // lead's scope decision for item SM (2026-09-17), only in a COMMON
        // bake. The evidence for the clause is the HUD's (every HUD bitmap is
        // composited by exactly this alpha, `analysis/re-hud-icons.md` §2
        // [V]) and the HUD sheets live in COMMON.LVL / INGAME.LVL. A map's
        // water, terrain and effect sheets are unnamed too, but the water
        // pass composites by texture alpha and what the PS2's water layer
        // does with it was unread VU1 (`analysis/re-water.md` §8, HOLD 21), so
        // those kept design D3's answer until WA measured the WATER half (the
        // clause below); terrain sheets still keep it, and the particle
        // effects' sheets keep their alpha by `fx_named` (card FXP1).
        let unnamed = kind == Content::Common && top_or_flags.is_none() && layer_or_flags.is_none();
        // Queue item WA (2026-09-17): the sheets THIS map's own `fx__
        // Effect("Water")` names keep their disc alpha, because every blended
        // water layer on both discs composites by it — TEX0 TCC=1 / TFX=0
        // MODULATE through the engine's one texture path, then ALPHA 0x44
        // source-over, so `As = At x Av >> 7`
        // (`analysis/re-water-alpha.md` §0, [V] but `[O]` UNAUDITED). This is
        // the call site that carries the name set; it is deliberately NARROW
        // (the lead's scope decision, same doc) — `MainTexture` and
        // `FoamTexture` and nothing else, so a map's terrain, foliage and
        // mist sheets keep design D3's answer.
        let water_named = water_named_textures.contains(&lower);
        let consumer_named = water_named
            || sheet_named.as_deref() == Some(lower.as_str())
            || grass_named.contains(&lower)
            || pass_named.contains(&lower)
            || flash_named.contains(&lower)
            || glow_named.contains(&lower)
            || fx_named.contains(&lower)
            || precip_named.as_deref() == Some(lower.as_str());
        // The pre-v2 OR of the material flag word over every model that draws
        // this texture: top-level ones, and since design D3 the mode layers'
        // too. A texture NO model names keeps the old `unwrap_or` default.
        let old_or_flags = match (top_or_flags, layer_or_flags) {
            (None, None) => crate::detail::ALPHA_IS_MEANINGFUL,
            (a, b) => a.unwrap_or(0) | b.unwrap_or(0),
        };
        let world_needs = alpha_usage.needs_alpha_outside(&lower, &sky_models);
        let needs_alpha =
            crate::alpha::keep_alpha(is_sky, old_or_flags, world_needs, unnamed, consumer_named);
        if is_sky {
            // Design D3's carve-out check: would leaving the layer models out
            // of the pre-v2 OR — D7's exact input — have answered differently?
            let top_only = top_or_flags.unwrap_or(crate::detail::ALPHA_IS_MEANINGFUL);
            if crate::alpha::keep_alpha(true, top_only, world_needs, unnamed, consumer_named)
                != needs_alpha
            {
                layer_flag_moves.push(t.name.clone());
            }
            if needs_alpha != alpha_usage.needs_alpha(&lower) {
                sky_overrides.push(format!(
                    "{}({})",
                    t.name,
                    if needs_alpha { "keeps alpha" } else { "opaque" }
                ));
            }
            if alpha_usage
                .models_using(&lower)
                .any(|m| !sky_models.contains(m))
            {
                sky_and_world.push(t.name.clone());
            }
        }
        // Both tiers, side by side (PLAN §Classic-plus assets, the maintainer
        // 2026-09-03): the base file is ALWAYS the PS2 disc's pixels — the sim
        // reads only that — and the classic-plus override, where the chain has
        // one, is written beside it as `<name>.tier2.ktx2`. The renderer picks
        // between them at runtime.
        //
        // The alpha state is the PS2 texture's on BOTH files: force it opaque
        // where no draw wants alpha, and where a draw does want it, give the
        // override the PS2 mask (the PC file's own alpha is a different
        // channel — gloss/spec — on several of the shipped sheets, so a MASK
        // draw would cut out along a mask nobody authored and the tier flip
        // would change the silhouette).
        let mut r = crate::tier::Resolved::from_ps2(t);
        let mut tier2 = crate::tier::tier2(t, pc.as_ref());
        let carries_alpha = || {
            r.mips
                .iter()
                .any(|m| m.as_chunks::<4>().0.iter().any(|px| px[3] < 255))
        };
        if !is_sky && unnamed && carries_alpha() {
            unnamed_kept.push(t.name.clone());
        }
        // Queue item WA, reported so the count stays visible exactly as SM's
        // does: the map's own water sheets that carry real alpha on the disc.
        // Expected 1 per water map, the KAM1s included: they name 2, but the
        // BASE sheet `kam1_water_2` is alpha-255 everywhere and so never
        // appears (the build's `maps` bake printed 12 lines, one per map).
        if water_named && carries_alpha() {
            water_alpha_kept.push(t.name.clone());
        }
        if !needs_alpha {
            r.make_opaque();
            if let Some(o) = tier2.as_mut() {
                o.make_opaque();
            }
        } else if let Some(o) = tier2.as_mut() {
            o.take_alpha_from(&r);
            pc_alpha_substitutions.push(t.name.clone());
        }
        r.write_pair(&tex_dir, &t.name, tier2.as_ref())?;
        if crate::detail::companion(t, &all_textures).is_none() {
            crate::remove_optional_file(&tex_dir.join(crate::detail::detail_file(&lower)))?;
        }
        if let Some(o) = &tier2 {
            let file = format!("{}.tier2.ktx2", t.name);
            texture_tiers.push((lower.clone(), o.tier.as_str().to_owned()));
            texture_tier2.push((lower.clone(), file));
        }
        // DERIVED, and read by nothing at runtime (design D4): the textures at
        // least one draw composites.
        if alpha_usage.blended(&lower) {
            transparent.push(t.name.clone());
        }
        textures.push(t.name.clone());
    }
    alpha_report(layer_flag_moves, |n| {
        format!(
            "D3 CARVE-OUT MOVED — {n} sky texture(s) change alpha state because the \
             lvl_ layers' models joined the pre-v2 OR"
        )
    });
    alpha_report(sky_overrides, |n| {
        format!("D7 sky carve-out changed the KTX2 alpha state of {n} texture(s)")
    });
    alpha_report(sky_and_world, |n| {
        format!("{n} texture(s) drawn by BOTH a sky shell and a world model")
    });
    alpha_report(pc_alpha_substitutions, |n| {
        format!("tier 2 took the PS2 alpha on {n} texture(s) whose alpha the base keeps")
    });
    alpha_report(unnamed_kept, |n| {
        format!("{n} alpha-bearing texture(s) no segment names keep their disc alpha")
    });
    alpha_report(water_alpha_kept, |n| {
        format!("{n} water sheet(s) this map's Effect(\"Water\") names keep their disc alpha")
    });

    // Water, after the texture list is final: resolving which texture paints
    // it needs the baked names (4 of the 12 maps name none in `LAYR`).
    let water = match water {
        Some(w) if crate::water_glb::triangle_count(&w) > 0 => {
            let cfg = water_cfg.as_ref();
            let texture = crate::water_glb::resolve_texture(&w, cfg, &textures);
            let glb = crate::water_glb::water_to_glb(&w, cfg, "water", texture.as_deref())?
                .expect("triangle_count > 0");
            std::fs::write(out.join("water.glb"), glb)?;
            Some(ManifestWater {
                glb: "water.glb".into(),
                texture,
                height: w.layer.height,
                // The PS2's scroll where the map authors a `Velocity` (all
                // twelve do); `LAYR`'s dead copy only for data neither disc
                // ships.
                uv_velocity: cfg
                    .and_then(|c| c.scroll_per_second(w.info.unknown_5))
                    .unwrap_or(w.layer.uv_velocity),
                layer_color: w.layer.color,
                wet_cells: w.wet_cells(),
                patches: w.patches.len(),
                triangles: crate::water_glb::triangle_count(&w),
                config: cfg.map(|c| crate::water_glb::manifest_config(c, &w)),
            })
        }
        _ => None,
    };

    // Base world = the one carrying the terrain name (else the most
    // populated); every other unwrapped world merges into it.
    if worlds.is_empty() {
        if kind == Content::Map {
            bail_no_world(lvl_path)?;
        }
        let stem = Path::new(lvl_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "common".into());
        worlds.push(mungefs::world::World {
            name: stem,
            terrain_name: None,
            sky_name: None,
            regions: Vec::new(),
            instances: Vec::new(),
            hints: Vec::new(),
            barriers: Vec::new(),
        });
    }
    let base_idx = worlds
        .iter()
        .position(|w| w.terrain_name.is_some())
        .unwrap_or_else(|| {
            worlds
                .iter()
                .enumerate()
                .max_by_key(|(_, w)| w.instances.len())
                .map(|(i, _)| i)
                .unwrap()
        });
    let w = worlds.remove(base_idx);
    let mut regions: Vec<ManifestRegion> = w.regions.iter().map(region_entry).collect();
    let mut instances: Vec<ManifestInstance> = w.instances.iter().map(instance_entry).collect();
    for extra in &worlds {
        regions.extend(extra.regions.iter().map(region_entry));
        instances.extend(extra.instances.iter().map(instance_entry));
    }
    // The AI data: the base world's own block, then one per mode layer that
    // ships a `plan` of its own. A layer without one gets None and the
    // runtime falls back to the base.
    // Sidecar keys are claimed only by a world that HAS a plan, so a layer
    // without one neither writes nor reserves a file name.
    let mut nav_keys: Vec<String> = vec!["base".to_string()];
    let base_worlds: Vec<&mungefs::world::World> =
        std::iter::once(&w).chain(worlds.iter()).collect();
    let base_graph = base_plan
        .as_ref()
        .map(|p| nav_plan_entry(out, "base", p, base_plnp.as_ref()))
        .transpose()?;
    let nav = nav_block(base_graph, &base_worlds);
    let mut layers: Vec<ManifestLayer> = Vec::new();
    for ml in &mode_layers {
        let group = &ml.group;
        let graph = match ml.plan.as_ref() {
            Some(plan) => {
                let key = unique_nav_key(&group[0].name, &mut nav_keys);
                Some(nav_plan_entry(out, &key, plan, ml.plnp.as_ref())?)
            }
            None => None,
        };
        let refs: Vec<&mungefs::world::World> = group.iter().collect();
        let nav = nav_block(graph, &refs);
        layers.push(ManifestLayer {
            name: group[0].name.clone(),
            worlds: group.iter().map(|g| g.name.clone()).collect(),
            regions: group
                .iter()
                .flat_map(|g| g.regions.iter().map(region_entry))
                .collect(),
            instances: group
                .iter()
                .flat_map(|g| g.instances.iter().map(instance_entry))
                .collect(),
            nav,
        });
    }
    let source_game = if bf2_models > 0 || (models.is_empty() && !layers.is_empty()) {
        "bf2"
    } else {
        "bf1"
    }
    .to_string();
    let mut terrain_layers_glb = None;
    if let (Some(t), Some(fname)) = (&terrain_parsed, &terrain_glb) {
        let bf2 = source_game == "bf2";
        let glb = crate::terrain_glb::terrain_to_glb(t, bf2)?;
        std::fs::write(out.join(fname), glb)?;
        if let Some(glb) = crate::terrain_glb::terrain_layers_to_glb(t, bf2)? {
            let fname = format!("terrain_layers_{}.glb", t.name);
            std::fs::write(out.join(&fname), glb)?;
            terrain_layers_glb = Some(fname);
        }
    }
    let lighting =
        crate::lighting::lighting_from(sky_cfg.as_ref(), lights_cfg.as_ref(), source_game == "bf2");
    let foliage = if foliage_file.is_none() && grass_props.is_empty() {
        None
    } else {
        let f = crate::foliage::bake(
            out,
            &crate::foliage::Inputs {
                lvl_path,
                game: &source_game,
                terrain: terrain_parsed.as_ref(),
                mask: folg_mask.as_ref(),
                file: foliage_file.as_ref(),
                models: &models,
                classes: &classes,
                paths: &paths,
                grass: &grass_props,
            },
        )?;
        if let Some(f) = &f {
            eprintln!(
                "foliage: {} instances over {} meshes, {} path props, {} grass classes",
                f.count,
                f.meshes.len(),
                f.path_props.len(),
                f.grass.len()
            );
        }
        f
    };
    // Card LEAF: the leaf classes, and what each prop class hangs on its
    // model's hardpoints (cap 30 on BF1, 16 on BF2).
    let leaf: Vec<manifest::ManifestLeafClass> = leaf_props
        .iter()
        .map(|(name, props)| crate::leafpatch::leaf_class(name, props))
        .collect();
    if !leaf.is_empty() {
        let cap = if source_game == "bf2" { 16 } else { 30 };
        crate::leafpatch::attach(
            &attach_props,
            &leaf,
            cap,
            &crate::hardpoints::Skeletons::of(&expanded),
            &mut classes,
        );
        eprintln!(
            "leaf: {} classes, {} attachments on {} classes",
            leaf.len(),
            classes.iter().map(|k| k.attach.len()).sum::<usize>(),
            classes.iter().filter(|k| !k.attach.is_empty()).count()
        );
    }
    // Card BPNT: the EntityLights each prop class hangs on its hardpoints.
    // BF2 only: BF1's sole point source is its explosions' flash (§11).
    if !entity_lights.is_empty() && source_game == "bf2" {
        crate::entity_light::attach(
            &attach_props,
            &entity_lights,
            &crate::hardpoints::Skeletons::of(&expanded),
            crate::entity_light::Miss::of(&source_game),
            &mut classes,
        );
    }
    if !(point_lights.is_empty() && entity_lights.is_empty()) {
        eprintln!(
            "lights: {} authored, {} on {} classes",
            point_lights.len(),
            classes.iter().map(|k| k.lights.len()).sum::<usize>(),
            classes.iter().filter(|k| !k.lights.is_empty()).count()
        );
    }
    {
        let lower = |v: &[String]| v.iter().map(|n| n.to_ascii_lowercase()).collect::<Vec<_>>();
        fx.write(out, &source_game, &lower(&textures), &lower(&models))?;
    }
    let manifest = Manifest {
        map: w.name.clone(),
        source_game,
        terrain_name: w.terrain_name.clone(),
        terrain_glb,
        sky_name: w.sky_name.clone(),
        models,
        textures,
        transparent,
        texture_tiers,
        classes,
        geometries,
        regions,
        instances,
        paths,
        layers,
        lighting,
        sky,
        water,
        collision,
        terrain_cutters,
        nav,
        texture_tier2,
        lowres_glb,
        foliage,
        terrain_layers_glb,
        leaf,
        hdr: hdr.map(|h| manifest::ManifestHdr {
            num_bloom_passes: h.num_bloom_passes,
            max_total_weight: h.max_total_weight,
            glow_threshold: h.glow_threshold,
        }),
        point_lights,
        lightning,
        precipitation,
        wind,
    };
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mungefs::world::Prop;

    /// BF1 NAB2.LVL entc `nab2_bldg_b_19` @0x3E99B4 (and BF2 NAB2.LVL
    /// @0x3AAC90, identical): four collision lines in this order. The class
    /// row must carry them verbatim, lowercased, in order — soldiers and
    /// bolts see only the mesh, vehicles only the two boxes
    /// (analysis/re-collision-layers.md §0.4).
    #[test]
    fn a_class_carries_its_collision_lines_in_prop_order() {
        let h = mungefs::fnv1a_lowercase;
        let e = mungefs::entity::EntityClass {
            name: "nab2_bldg_b_19".into(),
            base: "prop".into(),
            props: vec![
                Prop {
                    key_hash: h("GeometryName"),
                    value: "nab2_bldg_b_19".into(),
                },
                Prop {
                    key_hash: h("VehicleCollision"),
                    value: "p_cube".into(),
                },
                Prop {
                    key_hash: h("VehicleCollision"),
                    value: "p_cube1".into(),
                },
                Prop {
                    key_hash: h("SoldierCollision"),
                    value: "CollisionMesh".into(),
                },
                Prop {
                    key_hash: h("OrdnanceCollision"),
                    value: "CollisionMesh".into(),
                },
                // Not a collision key: ignored, as every other prop is.
                Prop {
                    key_hash: h("FoleyFXGroup"),
                    value: "stone".into(),
                },
            ],
        };
        let c = class_from_entc(&e);
        assert_eq!(c.name, "nab2_bldg_b_19");
        assert_eq!(c.base, "prop");
        assert_eq!(c.geometry.as_deref(), Some("nab2_bldg_b_19"));
        assert_eq!(
            c.collision,
            vec![
                ("vehicle".to_string(), "p_cube".to_string()),
                ("vehicle".to_string(), "p_cube1".to_string()),
                ("soldier".to_string(), "collisionmesh".to_string()),
                ("ordnance".to_string(), "collisionmesh".to_string()),
            ]
        );
    }

    /// A class with no collision property gets an EMPTY vector, which is the
    /// engine's default: every body in all five subsets (§0.3). KAM1's
    /// `kam_bldg_mushplatform1` is the case that matters — its
    /// `p_collision_floor` IS the platform floor.
    #[test]
    fn a_class_without_collision_props_carries_nothing() {
        let e = mungefs::entity::EntityClass {
            name: "kam_bldg_mushplatform1".into(),
            base: "prop".into(),
            props: vec![Prop {
                key_hash: mungefs::fnv1a_lowercase("GeometryName"),
                value: "kam_bldg_mushplatform1".into(),
            }],
        };
        assert!(class_from_entc(&e).collision.is_empty());
    }

    // ---- design D1: a `lvl_` mode layer's own content is baked -------------
    //
    // The LVLs below are built in memory in the disc's own container format
    // (`[tag][u32 size][payload]`, padded to 4 — analysis/a2-bake-formats.md
    // §1) so the walk can be exercised without a disc: `ucfb { ... lvl_ {
    // <hashed sub-level> { entc, gmod, modl, wrld } } }`, which is the shape
    // BF2 HOT1 ships four of.

    fn chunk(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::from(*tag);
        v.extend((payload.len() as u32).to_le_bytes());
        v.extend_from_slice(payload);
        while !v.len().is_multiple_of(4) {
            v.push(0);
        }
        v
    }

    fn zstr(s: &str) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    }

    /// One `modl` with a single one-vertex segment. `seed` goes into the
    /// vertex so two models of the SAME NAME can be told apart by their GLB.
    fn synth_modl(name: &str, seed: u16) -> Vec<u8> {
        let mut segm = chunk(b"INFO", &[1, 0, 0, 0, 1, 0, 0, 0]);
        segm.extend(chunk(b"MTRL", &1u32.to_le_bytes()));
        segm.extend(chunk(b"RTYP", b"Normal\0"));
        segm.extend(chunk(b"TNAM", &{
            let mut t = 0u32.to_le_bytes().to_vec();
            t.extend(zstr("atex"));
            t
        }));
        let mut posi = Vec::new();
        for _ in 0..3 {
            posi.extend(seed.to_le_bytes());
        }
        segm.extend(chunk(b"POSI", &posi));
        segm.extend(chunk(b"STRP", &[0u8; 2]));

        let mut info = vec![0u8; 68];
        // A unit vertex box, so the dequantized position is finite.
        for (i, v) in [-1.0f32, -1.0, -1.0, 1.0, 1.0, 1.0].iter().enumerate() {
            info[12 + i * 4..16 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        let mut p = chunk(b"NAME", &zstr(name));
        p.extend(chunk(b"INFO", &info));
        p.extend(chunk(b"segm", &segm));
        chunk(b"modl", &p)
    }

    fn synth_gmod(name: &str, model: &str) -> Vec<u8> {
        let mut lod = zstr(model);
        while !lod.len().is_multiple_of(4) {
            lod.push(0);
        }
        lod.extend(1.0f32.to_le_bytes());
        let mut p = chunk(b"NAME", &zstr(name));
        p.extend(chunk(b"LOD0", &lod));
        chunk(b"gmod", &p)
    }

    fn synth_entc(name: &str, geometry: &str) -> Vec<u8> {
        let mut prop = mungefs::fnv1a_lowercase("GeometryName")
            .to_le_bytes()
            .to_vec();
        prop.extend(zstr(geometry));
        let mut p = chunk(b"BASE", &zstr("prop"));
        p.extend(chunk(b"TYPE", &zstr(name)));
        p.extend(chunk(b"PROP", &prop));
        chunk(b"entc", &p)
    }

    /// A `wrld` with one instance of `class`, so the world is non-empty.
    fn synth_wrld(name: &str, class: &str) -> Vec<u8> {
        let mut info = chunk(b"TYPE", &zstr(class));
        info.extend(chunk(b"NAME", &zstr("inst0")));
        info.extend(chunk(b"XFRM", &[0u8; 48]));
        let inst = chunk(b"inst", &chunk(b"INFO", &info));
        let mut p = chunk(b"NAME", &zstr(name));
        p.extend(inst);
        chunk(b"wrld", &p)
    }

    /// `lvl_ { <fnv(name)> { body } }` — the wrapper BF2 puts a mode layer in.
    fn synth_lvl_wrapper(name: &str, body: &[u8]) -> Vec<u8> {
        let tag = mungefs::fnv1a_lowercase(name).to_le_bytes();
        chunk(b"lvl_", &chunk(&tag, body))
    }

    fn synth_ucfb(body: &[u8]) -> Vec<u8> {
        let mut v = Vec::from(*b"ucfb");
        v.extend((body.len() as u32).to_le_bytes());
        v.extend_from_slice(body);
        v
    }

    /// Per-process scratch dir for a synthetic bake — two `cargo test` runs
    /// at once must not bake into the same directory (queue item 3).
    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("{tag}-{}", std::process::id()))
    }

    fn bake_synthetic(lvl: &[u8], tag: &str) -> Manifest {
        let out = scratch_dir(tag);
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        let path = out.join("synth.lvl");
        std::fs::write(&path, lvl).unwrap();
        bake_map(path.to_str().unwrap(), out.to_str().unwrap(), None).expect("bake")
    }

    /// D1: a layer's `entc` / `gmod` / `modl` are baked exactly like top-level
    /// content — BOTH GLBs land, the class and geometry rows exist — while its
    /// `wrld` still arrives as a mode layer and NOT as a base world. Before
    /// this change the walk took only `wrld`/`plan`/`plnp` out of a wrapper,
    /// so `b`'s three chunks were silently dropped and the layer placed an
    /// instance of a class with no row (BF2 HOT1's bridges).
    #[test]
    fn a_mode_layers_own_content_is_baked_like_top_level_content() {
        let mut body = synth_modl("a", 100);
        body.extend(synth_wrld("base", "a_class"));
        let mut layer = synth_entc("b_class", "b");
        layer.extend(synth_gmod("b", "b"));
        layer.extend(synth_modl("b", 200));
        layer.extend(synth_wrld("layer_con", "b_class"));
        body.extend(synth_lvl_wrapper("layer_con", &layer));
        let out = scratch_dir("openbf-bake-test-layer-content");
        let m = bake_synthetic(&synth_ucfb(&body), "openbf-bake-test-layer-content");

        assert_eq!(m.models, vec!["a".to_string(), "b".to_string()]);
        assert!(out.join("models/a.glb").is_file());
        assert!(out.join("models/b.glb").is_file());
        assert_eq!(m.classes.len(), 1);
        assert_eq!(m.classes[0].name, "b_class");
        assert_eq!(m.classes[0].geometry.as_deref(), Some("b"));
        assert_eq!(m.geometries.len(), 1);
        assert_eq!(m.geometries[0].name, "b");
        assert_eq!(m.geometries[0].model, "b");
        // The world inside the wrapper is still a MODE LAYER, not a base world.
        assert_eq!(m.map, "base");
        assert_eq!(m.layers.len(), 1);
        assert_eq!(m.layers[0].name, "layer_con");
        assert_eq!(m.layers[0].instances.len(), 1);
        let _ = std::fs::remove_dir_all(&out);
    }

    /// D2: first wins, by name. A layer `modl` that repeats a top-level name
    /// is skipped, and the GLB on disk is byte-for-byte the one the same LVL
    /// bakes WITHOUT the layer — the property that lets BF1 and TAT2, which
    /// have no layer content to add, come out of this change unchanged.
    #[test]
    fn a_layer_model_never_overwrites_the_top_level_one_of_the_same_name() {
        let mut plain = synth_modl("a", 100);
        plain.extend(synth_wrld("base", "a_class"));
        let m0 = bake_synthetic(&synth_ucfb(&plain), "openbf-bake-test-layer-dup-none");
        let glb0 =
            std::fs::read(scratch_dir("openbf-bake-test-layer-dup-none").join("models/a.glb"))
                .unwrap();

        let mut layer = synth_modl("a", 900); // same name, different geometry
        layer.extend(synth_wrld("layer_con", "a_class"));
        let mut with_layer = plain.clone();
        with_layer.extend(synth_lvl_wrapper("layer_con", &layer));
        let m1 = bake_synthetic(&synth_ucfb(&with_layer), "openbf-bake-test-layer-dup");
        let glb1 =
            std::fs::read(scratch_dir("openbf-bake-test-layer-dup").join("models/a.glb")).unwrap();

        assert_eq!(
            m1.models,
            vec!["a".to_string()],
            "the layer copy is skipped"
        );
        assert_eq!(m0.models, m1.models);
        assert_eq!(glb0, glb1, "the TOP-LEVEL model is the one on disk");
        // ...and the layer itself still arrives.
        assert_eq!(m1.layers.len(), 1);
        let _ = std::fs::remove_dir_all(scratch_dir("openbf-bake-test-layer-dup-none"));
        let _ = std::fs::remove_dir_all(scratch_dir("openbf-bake-test-layer-dup"));
    }

    /// Card VTUR2: a class the map's worlds place but do not define comes
    /// from `SIDE/TUR.LVL` beside the map's directory — its whole sublevel,
    /// model included — while a TUR sublevel nobody places stays out, and a
    /// class the map defines itself keeps the map's geometry.
    #[test]
    fn a_placed_turret_class_is_pulled_from_the_side_tur_lvl() {
        let root = scratch_dir("openbf-bake-test-tur-pull");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("MAP")).unwrap();
        std::fs::create_dir_all(root.join("SIDE")).unwrap();
        let out = root.join("out");

        let mut tur = Vec::new();
        for (class, model, seed) in [
            ("tur_placed", "tp", 1),
            ("tur_unplaced", "tu", 2),
            ("own_class", "side_own", 3),
        ] {
            let mut sub = synth_modl(model, seed);
            sub.extend(synth_entc(class, model));
            tur.extend(synth_lvl_wrapper(class, &sub));
        }
        std::fs::write(root.join("SIDE/TUR.LVL"), synth_ucfb(&tur)).unwrap();

        let mut body = synth_entc("own_class", "own");
        body.extend(synth_modl("own", 4));
        body.extend(synth_wrld("base", "own_class"));
        body.extend(synth_lvl_wrapper(
            "layer_con",
            &synth_wrld("layer_con", "TUR_Placed"),
        ));
        let map = root.join("MAP/map.lvl");
        std::fs::write(&map, synth_ucfb(&body)).unwrap();
        let m = bake_map(map.to_str().unwrap(), out.to_str().unwrap(), None).expect("bake");

        assert_eq!(m.models, vec!["own".to_string(), "tp".to_string()]);
        assert!(out.join("models/tp.glb").is_file());
        assert!(!out.join("models/tu.glb").exists());
        let geometry = |n: &str| {
            m.classes
                .iter()
                .find(|k| k.name == n)
                .map(|k| k.geometry.clone())
        };
        assert_eq!(geometry("tur_placed"), Some(Some("tp".into())));
        assert_eq!(geometry("own_class"), Some(Some("own".into())));
        assert_eq!(geometry("tur_unplaced"), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
