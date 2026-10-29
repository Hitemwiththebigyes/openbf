//! What a fallen building turret looks like and collides as (card TBLD,
//! `core::sim::BuildingTurret`): the PS2's death rule over the class's ODF
//! (`analysis/re-build-zones.md` 1 [O][V], B2 `FUN_001e3638`
//! B2:93584-93660): a `DestroyedGeometryName` -> that wreck (state 1); else
//! an unbuilt model (the entc prop `manifest::BUILD_ZONE_PROP`,
//! `UnbuiltGeometryName`: `com_prop_buildzone`, the little square) -> that
//! pad (state 2); else hidden (state 1). The wreck and the pad are drawn
//! rigid at the instance (`analysis/re-turret-draw.md` 5 [O]).
//!
//! One named deviation (the maintainer 2026-10-28: "the normal looking turrets should
//! just show the little square on the ground"): a class with neither model
//! that is drawn as the common gun turret (`com_weap_gunturret`: BF2's
//! `tur_weap_built_gunturret` on Kamino, Coruscant and Naboo, DEA1's mortar)
//! falls to the same pad as its buildable twin `tur_bldg_laser`; the PS2
//! hides it.

use crate::assets::Scene;
use crate::classdb::ClassDb;
use crate::stations::Common;
use anyhow::Result;
use core::collision::HullPart;
use core::sim::TurretFall;
use manifest::Manifest;
use std::collections::HashMap;
use std::path::Path;

/// The common gun turret's geometry: a class drawn as it with no fallen
/// model of its own falls to [`PAD`] (the deviation above).
pub const GUN_TURRET: &str = "com_weap_gunturret";
/// The build pad every buildable turret on both discs names.
pub const PAD: &str = "com_prop_buildzone";

/// A building turret class's fallen look: the kind and its geometry (`None`
/// for [`TurretFall::Hidden`]).
pub fn fall_look(db: &ClassDb, class: &str) -> (TurretFall, Option<String>) {
    let name = |s: &str| Some(s.trim().to_owned()).filter(|s| !s.is_empty());
    if let Some(g) = db.prop(class, "DestroyedGeometryName").and_then(name) {
        return (TurretFall::Wreck, Some(g));
    }
    if let Some(g) = db
        .prop_hash(class, manifest::BUILD_ZONE_PROP)
        .and_then(name)
    {
        return (TurretFall::Pad, Some(g));
    }
    if db
        .prop(class, "GeometryName")
        .is_some_and(|g| g.trim().eq_ignore_ascii_case(GUN_TURRET))
    {
        return (TurretFall::Pad, Some(PAD.to_owned()));
    }
    (TurretFall::Hidden, None)
}

/// The bodies of `geometry`'s baked collision (the map's, else its game's
/// common bake) in model space, each in the subsets `class`'s collision
/// lists put it in; none when neither bake has it (the pad has no `coll`).
pub fn collision_parts(
    m: &Manifest,
    map_dir: &Path,
    common: &Common,
    geometry: &str,
    class: &str,
) -> Result<Vec<HullPart>> {
    let (common_dir, common) = (&common.0, &common.1);
    let entry = m
        .collision
        .iter()
        .find(|e| e.geometry.eq_ignore_ascii_case(geometry))
        .map(|e| (map_dir, e))
        .or_else(|| {
            common
                .as_ref()?
                .collision
                .iter()
                .find(|e| e.geometry.eq_ignore_ascii_case(geometry))
                .map(|e| (common_dir.as_path(), e))
        });
    let Some((dir, entry)) = entry else {
        return Ok(Vec::new());
    };
    let cg = crate::assets::load_collision_geom(dir, entry)?;
    let row = m
        .classes
        .iter()
        .chain(common.iter().flat_map(|m| &m.classes))
        .find(|k| k.name.eq_ignore_ascii_case(class));
    Ok(crate::assets::hull_parts(
        &cg,
        row.map_or(&[], |c| c.collision.as_slice()),
        &[],
    ))
}

/// Per placed building turret instance (manifest index) of `scene`, the
/// model its fallen look draws (`None`: hidden, or a model the bake lacks).
/// The viewer draws it in place of the turret while the sim says it is down
/// (`core::sim::Sim::building_down`).
pub fn fallen_models(scene: &Scene) -> Result<HashMap<usize, Option<String>>> {
    let mut out = HashMap::new();
    let root = scene.dir.parent().unwrap_or(&scene.dir);
    let mut db = ClassDb::default();
    db.load_map(&scene.dir)?;
    // The files a battle's turrets come from (`vehicle_pads::install`):
    // every side, the shared file and BF2's TUR side.
    let game = scene.manifest.source_game.to_ascii_lowercase();
    let tur = format!("{game}_tur.ron");
    let files = crate::roster::class_files(&game, &crate::roster::FACTION_ORDER);
    for file in files.iter().copied().chain([tur.as_str()]) {
        let path = root.join("classes").join(file);
        if pak::exists(&path) {
            db.load(&path)?;
        }
    }
    for (i, inst) in scene.manifest.instances.iter().enumerate() {
        let Some(name) = db.resolve(&inst.class).map(str::to_owned) else {
            continue;
        };
        if !crate::vehicle_class::is_turret(&db, &name) {
            continue;
        }
        let (_, geometry) = fall_look(&db, &name);
        let model = geometry.and_then(|g| scene.model_for_geometry(&g).map(str::to_owned));
        out.insert(i, model);
    }
    Ok(out)
}
