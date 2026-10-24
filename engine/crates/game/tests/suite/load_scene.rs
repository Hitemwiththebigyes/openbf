//! Tier-1: baked TAT1 loads back into CPU scene structs (data-gated).

#[test]
fn baked_tat1_loads() {
    let lvl = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../extracted/bf1/lvl/DATA/_LVL_PS2/TAT/TAT1.LVL"
    );
    if !std::path::Path::new(lvl).exists() {
        eprintln!("SKIP: extracted TAT1.LVL not present");
        return;
    }
    let out = std::env::temp_dir().join(format!("openbf-scene-load-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    bake::map::bake_map(lvl, out.to_str().unwrap(), None).expect("bake");
    let scene = game::assets::load_scene(&out).expect("scene loads");
    assert_eq!(scene.manifest.map, "tat1");
    // 51 from the map's own LVL + the 4 sky-object capital ships pulled in
    // from the SIDE LVLs (bake::map::pull_side_models).
    assert_eq!(scene.meshes.len(), 51 + 4);
    // 45 materials + tat_skiff01's detail companion, which loads into that
    // texture's `detail` slot rather than as a texture of its own, + the 4
    // the sky objects brought with them.
    assert_eq!(scene.textures.len(), 45 + 4);
    // Every sky-object flight resolves to a mesh that actually loaded --
    // the case-sensitivity bug this test would not otherwise catch.
    let sky = scene.manifest.sky.as_ref().expect("TAT1 authors a sky");
    for o in &sky.objects {
        assert!(
            scene.meshes.contains_key(&o.geometry),
            "sky object {} has no mesh",
            o.geometry
        );
    }
    for m in &sky.models {
        assert!(
            scene.meshes.contains_key(&m.geometry),
            "sky dome {} has no mesh",
            m.geometry
        );
    }
    assert!(
        scene.textures["tat_skiff01"].detail.is_some(),
        "tat_skiff01 is detailCompressed on the disc"
    );
    assert!(scene.textures["tat1_rocks1"].detail.is_none());
    let terrain = scene.terrain.as_ref().expect("terrain mesh");
    // The sim reads the row-major grid at the HEAD of the mesh — TAT1's is
    // 256x256 — and `bake::terrain_glb` appends the five cutter-clipped
    // patches' shipped replacement meshes after it: 460 vertices and 690
    // triangles in place of 5*64 quads (analysis/re-terrain-cutters.md §0.5).
    assert_eq!(
        game::assets::terrain_grid_size(&terrain.positions),
        Some(256)
    );
    assert_eq!(terrain.positions.len(), 256 * 256 + 460);
    assert_eq!(terrain.indices.len(), (255 * 255 - 5 * 64) * 6 + 690 * 3);
    // entc/gmod chain: TAT1 carries 29 entity classes and 34 geometry tables.
    assert_eq!(scene.manifest.classes.len(), 29);
    assert_eq!(scene.manifest.geometries.len(), 34);
    // Landmarks resolve through class -> GeometryName -> gmod LOD0 -> modl.
    assert_eq!(
        scene.resolve_model("tat1_bldg_crawler"),
        Some("tat1_bldg_crawler"),
        "sandcrawler"
    );
    assert_eq!(
        scene.resolve_model("tat1_bldg_sarlaccpit"),
        Some("tat1_bldg_sarlaccpit")
    );
    // Class name != model name: the barrier class maps across the tat_/tat1_ gap.
    assert_eq!(
        scene.resolve_model("tat_prop_tuskbarrier"),
        Some("tat1_prop_tuskbarrier")
    );
    // Non-render classes (ambient sound emitter) resolve to nothing.
    assert_eq!(scene.resolve_model("com_snd_amb_static"), None);
    let drawable = scene
        .manifest
        .instances
        .iter()
        .filter(|i| scene.resolve_model(&i.class).is_some())
        .count();
    // 137/249: the rest are dust effect emitters (no geometry) and com_*
    // classes defined in COMMON.LVL, not the map LVL (turrets, recharge
    // droids, CP markers). This map scene has not merged the common bake.
    assert_eq!(drawable, 137, "drawable instance count");

    // P4 layout: 13 Zero Editor paths; 6 command posts, each with a capture
    // region and a spawn path of >= 9 nodes; starting owners per the wrld
    // Team props (cp10/cp4 = 1, cp7/cp2 = 2); ValueBleed 10 on all six.
    assert_eq!(scene.manifest.paths.len(), 13);
    let layout = game::layout::MapLayout::from_manifest(&scene.manifest);
    assert_eq!(
        layout.cps.len(),
        6,
        "{:?}",
        layout.cps.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
    for cp in &layout.cps {
        assert!(cp.capture.is_some(), "{} capture region", cp.name);
        assert!(cp.control.is_some(), "{} control region", cp.name);
        let path = layout.path(cp.spawn_path.as_deref().unwrap()).unwrap();
        assert!(
            path.nodes.len() >= 9,
            "{} spawn path {} nodes",
            cp.name,
            path.nodes.len()
        );
        assert_eq!(cp.value_bleed, 10);
        // The CP marker stands inside its own capture region.
        assert!(
            cp.capture.unwrap().contains(cp.pos + glam::Vec3::Y * 0.5),
            "{} inside capture",
            cp.name
        );
    }
    let team = |n: &str| layout.cps.iter().find(|c| c.name == n).unwrap().team;
    assert_eq!(
        (team("cp10"), team("cp7"), team("cp4"), team("cp2")),
        (1, 2, 1, 2)
    );
    assert!(layout.cps.iter().filter(|c| c.major).count() == 2);

    // Heightfield round-trip: grid vertices reproduce exactly; interpolated
    // sample near the RE_LOG worked example (patch 17 cell (5,3), world
    // (-856,-864), y 29.7) lands close to the vertex height.
    let hf = scene.heightfield().expect("terrain heightfield");
    assert_eq!(hf.size, 256);
    let terrain2 = scene.terrain.as_ref().unwrap();
    for &i in &[0usize, 255, 256, 65535, 128 * 256 + 128] {
        let p = terrain2.positions[i];
        let got = hf.height_at(p[0], p[2]);
        assert!(
            (got - p[1]).abs() < 1e-4,
            "vertex {i}: height_at({}, {}) = {got}, want {}",
            p[0],
            p[2],
            p[1]
        );
    }
    let y = hf.height_at(-856.0, -864.0);
    assert!((y - 29.7).abs() < 1.0, "RE_LOG worked example: got {y}");

    let mut resolved = 0;
    for mesh in scene.meshes.values() {
        for (_, _, mat, _, _) in &mesh.submeshes {
            if !mat.is_empty() && scene.textures.contains_key(mat) {
                resolved += 1;
            }
        }
    }
    assert!(
        resolved > 50,
        "material->texture links resolve, got {resolved}"
    );
    let _ = std::fs::remove_dir_all(&out);
}

/// Data-gated: BF2 rifleman inherits sprint/energy constants through the
/// two-level BASE chain (SIDE/ALL.LVL -> INGAME.LVL commons).
#[test]
fn bf2_class_db_inheritance() {
    let all = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../extracted/bf2/lvl/DATA/_LVL_PS2/SIDE/ALL.LVL"
    );
    let ingame = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../extracted/bf2/lvl/DATA/_LVL_PS2/INGAME.LVL"
    );
    if !std::path::Path::new(all).exists() || !std::path::Path::new(ingame).exists() {
        eprintln!("SKIP: extracted BF2 LVLs not present");
        return;
    }
    let dir = std::env::temp_dir().join(format!("openbf-classdb-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let all_ron = dir.join("all.ron");
    let ingame_ron = dir.join("ingame.ron");
    bake::classes::bake_classes(all, all_ron.to_str().unwrap()).expect("bake ALL");
    bake::classes::bake_classes(ingame, ingame_ron.to_str().unwrap()).expect("bake INGAME");

    let mut db = game::classdb::ClassDb::default();
    db.load(&all_ron).expect("load ALL");
    db.load(&ingame_ron).expect("load INGAME");

    // RE_LOG §BF2 soldier defaults: values live on com_inf_default, reached
    // from the rifleman through all_inf_default.
    let c = "all_inf_default_rifleman";
    assert_eq!(db.prop_f32(c, "MaxSpeed"), Some(7.0));
    assert_eq!(db.prop_f32(c, "EnergyBar"), Some(100.0));
    assert_eq!(db.prop_f32(c, "EnergyDrainSprint"), Some(20.0));
    assert_eq!(db.prop_f32(c, "SprintAccelerateTime"), Some(0.35));
    assert_eq!(db.engine_base(c), Some("soldier"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// BF2 TAT2: the conquest layer merges into the scene; the layout yields the
/// six conquest posts with regions (some live in the design layer) and
/// config-format spawn paths in engine space (nodes near their posts).
#[test]
fn baked_tat2_conquest_layout() {
    let lvl = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../extracted/bf2/lvl/DATA/_LVL_PS2/TAT/TAT2.LVL"
    );
    if !std::path::Path::new(lvl).exists() {
        eprintln!("SKIP: extracted TAT2.LVL not present");
        return;
    }
    let out = std::env::temp_dir().join(format!(
        "openbf-scene-load-test-tat2-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&out);
    bake::map::bake_map(lvl, out.to_str().unwrap(), None).expect("bake");
    let mut scene = game::assets::load_scene(&out).expect("scene loads");
    assert_eq!(scene.manifest.map, "tat2");
    let hf = scene.heightfield().expect("terrain heightfield");
    assert_eq!(hf.size, 64);
    let before = scene.manifest.instances.len();
    assert_eq!(scene.apply_mode_layer("con").as_deref(), Some("tat2_con"));
    assert_eq!(scene.manifest.instances.len(), before + 21);
    assert_eq!(scene.apply_mode_layer("con"), None, "merged once");
    let layout = game::layout::MapLayout::from_manifest(&scene.manifest);
    let mut names: Vec<&str> = layout.cps.iter().map(|c| c.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["cp1", "cp2", "cp3", "cp6", "cp7", "cp8"]);
    for cp in &layout.cps {
        assert!(cp.capture.is_some(), "{} capture region", cp.name);
        assert_eq!(cp.value_bleed, 10, "{} ValueBleed_<faction>", cp.name);
        let path = layout
            .path(cp.spawn_path.as_deref().unwrap())
            .unwrap_or_else(|| panic!("{} path", cp.name));
        assert!(!path.nodes.is_empty());
        for n in &path.nodes {
            let d = (n.pos - cp.pos).length();
            assert!(d < 60.0, "{}: spawn node {} m from the post", cp.name, d);
        }
    }
    let team = |n: &str| layout.cps.iter().find(|c| c.name == n).unwrap().team;
    assert_eq!((team("cp1"), team("cp7")), (1, 2));
    // Case-insensitive geometry chain: class GeometryName "tat2_bldg_wall_40M"
    // → lowercase gmod/modl.
    assert!(
        scene.resolve_model("tat2_bldg_wall_40m").is_some(),
        "wall segments resolve"
    );
    let drawable = scene
        .manifest
        .instances
        .iter()
        .filter(|i| scene.resolve_model(&i.class).is_some())
        .count();
    assert!(drawable > 650, "drawable TAT2 instances: {drawable}");
    let _ = std::fs::remove_dir_all(&out);
}

/// Shared assets: COMMON.LVL bakes without a world and, merged under TAT1,
/// makes the command posts / recharge droids / turrets drawable.
#[test]
fn common_assets_merge_makes_cps_drawable() {
    let lvl = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../extracted/bf1/lvl/DATA/_LVL_PS2/TAT/TAT1.LVL"
    );
    let common = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../extracted/bf1/lvl/DATA/_LVL_PS2/COMMON/ENG/COMMON.LVL"
    );
    if !std::path::Path::new(lvl).exists() || !std::path::Path::new(common).exists() {
        eprintln!("SKIP: extracted TAT1/COMMON not present");
        return;
    }
    let out = std::env::temp_dir().join(format!("openbf-scene-common-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    bake::map::bake_map(lvl, out.join("tat1").to_str().unwrap(), None).expect("bake map");
    let cm = bake::map::bake_common(common, out.join("common_bf1").to_str().unwrap(), None)
        .expect("bake common");
    assert!(
        cm.models.len() >= 40 && cm.textures.len() >= 100,
        "{} models / {} textures",
        cm.models.len(),
        cm.textures.len()
    );
    assert!(cm.classes.iter().any(|c| c.name == "com_bldg_controlzone"));
    let mut scene = game::assets::load_scene(&out.join("tat1")).expect("scene");
    let before = scene
        .manifest
        .instances
        .iter()
        .filter(|i| scene.resolve_model(&i.class).is_some())
        .count();
    assert_eq!(before, 137);
    let c = game::assets::load_common(&out.join("common_bf1")).expect("load common");
    let added = scene.merge_common(c);
    assert!(added >= 40, "added {added}");
    let after = scene
        .manifest
        .instances
        .iter()
        .filter(|i| scene.resolve_model(&i.class).is_some())
        .count();
    assert!(after >= before + 12, "drawable {before} -> {after}");
    assert!(scene.resolve_model("com_bldg_controlzone").is_some());
    assert!(scene.resolve_model("com_item_healthrecharge").is_some());
    let _ = std::fs::remove_dir_all(&out);
}
