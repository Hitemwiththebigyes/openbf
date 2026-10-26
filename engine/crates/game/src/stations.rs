use crate::assets::Scene;
use crate::classdb::ClassDb;
use anyhow::Result;
use core::sim::Sim;
use manifest::{CommonManifest, Manifest};
use std::collections::HashMap;
use std::path::Path;

pub fn install_collision(sim: &mut Sim, m: &Manifest, map_dir: &Path, db: &ClassDb) -> Result<()> {
    if sim.stations.is_empty() {
        return Ok(());
    }
    let root = map_dir.parent().unwrap_or(map_dir);
    let common_dir = root.join(format!("common_{}", m.source_game));
    let path = common_dir.join("common.ron");
    let common: Option<CommonManifest> = if pak::exists(&path) {
        Some(ron::from_str(&pak::read_to_string(&path)?)?)
    } else {
        None
    };
    let sphere = |geometry: &str, class: &str| -> Result<Option<[f32; 4]>> {
        let entry = m
            .geometries
            .iter()
            .find(|g| {
                g.name.eq_ignore_ascii_case(geometry)
                    && pak::exists(map_dir.join("models").join(format!("{}.glb", g.model)))
            })
            .map(|g| (map_dir, g))
            .or_else(|| {
                common
                    .as_ref()?
                    .geometries
                    .iter()
                    .find(|g| g.name.eq_ignore_ascii_case(geometry))
                    .map(|g| (common_dir.as_path(), g))
            });
        let Some((dir, g)) = entry else {
            return Ok(None);
        };
        let mesh = crate::assets::load_glb(&dir.join("models").join(format!("{}.glb", g.model)))?;
        Ok(mesh.sphere.map(|s| {
            let ([x, y, z], r) = crate::vehicle_class::blast_sphere(db, class, Some(s));
            [x, y, z, r]
        }))
    };
    for c in &mut sim.station_classes {
        c.sphere = sphere(&c.geometry, &c.name)?;
        c.wreck_sphere = match &c.destroyed_geometry {
            Some(g) => sphere(g, &c.name)?,
            None => None,
        };
    }
    sim.station_bodies.clear();
    sim.station_live_collision.clear();
    for si in 0..sim.stations.len() {
        let s = &mut sim.stations[si];
        let c = &sim.station_classes[usize::from(s.class)];
        if let Some([x, y, z, r]) = c.sphere {
            s.sphere_centre = s.pose().transform_point3(glam::Vec3::new(x, y, z));
            s.sphere_radius = r;
        }
        sim.prepare_station_collision(si);
    }
    sim.station_wreck_collision = sim
        .station_classes
        .iter()
        .map(|c| {
            let Some(geometry) = &c.destroyed_geometry else {
                return Ok(Vec::new());
            };
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
            let class = m
                .classes
                .iter()
                .chain(common.iter().flat_map(|m| &m.classes))
                .find(|k| k.name.eq_ignore_ascii_case(&c.name));
            Ok(crate::assets::hull_parts(
                &cg,
                class.map_or(&[], |c| c.collision.as_slice()),
            ))
        })
        .collect::<Result<_>>()?;
    Ok(())
}

pub fn wreck_models(scene: &Scene) -> Result<HashMap<usize, String>> {
    let mut out = HashMap::new();
    if !scene
        .manifest
        .classes
        .iter()
        .any(|c| c.base.eq_ignore_ascii_case("powerupstation"))
    {
        return Ok(out);
    }
    let root = scene.dir.parent().unwrap_or(&scene.dir);
    let mut db = ClassDb::default();
    db.load_map(&scene.dir)?;
    for file in crate::roster::class_files(&scene.manifest.source_game, &[]) {
        let path = root.join("classes").join(file);
        if pak::exists(&path) {
            db.load(&path)?;
        }
    }
    for (i, inst) in scene.manifest.instances.iter().enumerate() {
        if db.engine_base(&inst.class) != Some("powerupstation") {
            continue;
        }
        if let Some(model) = db
            .prop(&inst.class, "DestroyedGeometryName")
            .and_then(|g| scene.model_for_geometry(g.trim()))
        {
            out.insert(i, model.to_owned());
        }
    }
    Ok(out)
}
