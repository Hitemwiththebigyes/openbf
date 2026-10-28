use super::*;

/// An ocean step that turns 2 water draws into 130 moves every later draw
/// by 128: the door parts follow, a speckle frame inside the old range
/// (the still plane's) is dropped, and an index before the range stays.
#[test]
fn door_and_speckle_indices_follow_an_ocean_splice() {
    let mut doors = vec![DoorDraw {
        instance: 7,
        model: Mat4::IDENTITY,
        parts: vec![(40..42, Vec::new()), (50..52, Vec::new())],
    }];
    let mut speckle = Some(SpeckleAnim {
        first: vec![3, 11, 60],
        frames: Vec::new(),
        fps: 30.0,
    });
    let inst = manifest::ManifestInstance {
        name: "t".into(),
        class: "t".into(),
        rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0; 3],
        props: Vec::new(),
    };
    let rig = manifest::ManifestTurretRig {
        nodes: Vec::new(),
        parts: Vec::new(),
    };
    let mut turrets = vec![super::super::turret_pose::TurretDraw::new(
        9,
        &inst,
        Mat4::IDENTITY,
        &rig,
        vec![(4..6, 0), (60..62, 1)],
        60..64,
    )];
    reindex_after_splice(
        &mut doors,
        &mut turrets,
        &mut [],
        &mut speckle,
        &(10..12),
        130,
    );
    assert_eq!(turrets[0].parts[0].0, 4..6);
    assert_eq!(turrets[0].parts[1].0, 188..190);
    assert_eq!(turrets[0].whole, 188..192);
    assert_eq!(doors[0].parts[0].0, 168..170);
    assert_eq!(doors[0].parts[1].0, 178..180);
    assert_eq!(speckle.as_ref().unwrap().first, vec![3, 188]);
    // A shrink moves them back; every speckle frame gone drops the layer.
    reindex_after_splice(
        &mut doors,
        &mut turrets,
        &mut [],
        &mut speckle,
        &(10..140),
        2,
    );
    assert_eq!(doors[0].parts[0].0, 40..42);
    speckle.as_mut().unwrap().first = vec![11];
    reindex_after_splice(
        &mut doors,
        &mut turrets,
        &mut [],
        &mut speckle,
        &(10..12),
        2,
    );
    assert!(speckle.is_none());
}

/// Only BF1's authored `TerrainEnable(0)` switches the terrain draw off.
#[test]
fn only_bf1_terrain_enable_zero_hides_the_terrain() {
    let sky = |e: Option<f32>| manifest::ManifestSky {
        texture: None,
        models: Vec::new(),
        ambient: [1.0; 3],
        low_res_terrain: None,
        low_res_max_distance: None,
        objects: Vec::new(),
        terrain_enable: e,
        reflection_color: None,
        stars: None,
    };
    assert!(!terrain_drawn("bf1", Some(&sky(Some(0.0)))));
    assert!(terrain_drawn("bf2", Some(&sky(Some(0.0)))));
    assert!(terrain_drawn("bf1", Some(&sky(Some(1.0)))));
    assert!(terrain_drawn("bf1", Some(&sky(None))));
    assert!(terrain_drawn("bf1", None));
}

/// Card KSKY: Kamino's `SkyDomeDarkenFactor(0.4)` rests dome shell 0 at
/// 255·(1 − 0.4) = 153 — the byte PS2 RAM holds — while shell 1
/// (`kam1_city`) stays white; a map without the effect darkens nothing.
#[test]
fn lightning_darkens_dome_shell_0_only() {
    let l = manifest::ManifestLightning {
        sky_dome_darken_factor: 0.4,
        sunlight_fade_factor: 0.1,
        color: [255.0; 3],
        brightness_min: 1.0,
        fade_time: 0.2,
        time_between_flashes: [3.0, 5.0],
        time_between_sub_flashes: [0.01, 0.5],
        num_sub_flashes: [2.0, 5.0],
        horizon_angle: [30.0, 60.0],
        sound_crack: None,
        sound_sub_crack: None,
    };
    assert_eq!(l.dome_byte(0.0), 153);
    // A flash at full brightness is white: the hook the weather card animates.
    assert_eq!(l.dome_byte(1.0), 255);
    assert_eq!(l.dome_tint(1.0), 1.0);
    // 153/255 through the sRGB curve.
    let k = ((0.6f32 + 0.055) / 1.055).powf(2.4);
    assert!((l.dome_tint(0.0) - k).abs() < 1e-6, "{}", l.dome_tint(0.0));
    let white = [1.0; 4];
    assert_eq!(sky_shell_tint(Some(&l), 0, white), [k, k, k, 1.0]);
    assert_eq!(sky_shell_tint(Some(&l), 1, white), white);
    assert_eq!(sky_shell_tint(None, 0, white), white);
}

/// A baked terrain mesh: `n x n` world-space grid vertices row-major, two
/// triangles per cell, one material — what `bake::terrain_glb` emits.
fn grid_mesh(n: usize) -> CpuMesh {
    let unit = 8.0f32;
    let half = n as f32 / 2.0 * unit;
    let mut positions = Vec::new();
    for z in 0..n {
        for x in 0..n {
            positions.push([x as f32 * unit - half, 0.0, z as f32 * unit - (half - unit)]);
        }
    }
    let mut indices = Vec::new();
    for z in 0..n - 1 {
        for x in 0..n - 1 {
            let a = (z * n + x) as u32;
            let (b, c, d) = (a + 1, a + n as u32, a + n as u32 + 1);
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let v = positions.len();
    CpuMesh {
        positions,
        normals: vec![[0.0, 1.0, 0.0]; v],
        uvs: vec![[0.0, 0.0]; v],
        colors: vec![[255; 4]; v],
        indices,
        submeshes: vec![(0, 0, "hoth_main_1".into(), render::DrawLayer::Opaque, 0)],
        spec: Vec::new(),
        light: Vec::new(),
        scroll: Vec::new(),
        sphere: None,
    }
}

/// The chunking the grid walk did before the bake could ship replacement
/// meshes — kept here as the reference the new dealer must reproduce.
fn grid_walk(n: usize) -> Vec<Vec<u32>> {
    let per_row = n.div_ceil(TERRAIN_CHUNK_CELLS).max(1);
    let mut out = Vec::new();
    for cz in 0..per_row {
        for cx in 0..per_row {
            let (x0, z0) = (cx * TERRAIN_CHUNK_CELLS, cz * TERRAIN_CHUNK_CELLS);
            let (x1, z1) = (
                (x0 + TERRAIN_CHUNK_CELLS).min(n - 1),
                (z0 + TERRAIN_CHUNK_CELLS).min(n - 1),
            );
            if x0 >= x1 || z0 >= z1 {
                continue;
            }
            let mut idx = Vec::new();
            for z in z0..z1 {
                for x in x0..x1 {
                    let a = (z * n + x) as u32;
                    let (b, c, d) = (a + 1, a + n as u32, a + n as u32 + 1);
                    idx.extend_from_slice(&[a, c, b, b, c, d]);
                }
            }
            out.push(idx);
        }
    }
    out
}

/// Terrain chunking is now driven by the BAKED index buffer, and on a
/// plain grid that reproduces the old grid walk chunk for chunk, index for
/// index — so no golden moves on the 19 terrains that ship no replacement
/// mesh.
#[test]
fn terrain_chunks_reproduce_the_grid_walk() {
    for n in [64usize, 128] {
        let cpu = grid_mesh(n);
        let got: Vec<Vec<u32>> = terrain_chunk_indices(&cpu)
            .expect("a grid mesh chunks")
            .into_iter()
            .filter(|c| !c.is_empty())
            .collect();
        assert_eq!(got, grid_walk(n), "n = {n}");
    }
}

/// A cutter-clipped patch: the bake drops that patch's quads and appends
/// the shipped replacement vertices after the grid
/// (analysis/re-terrain-cutters.md §0.3). The dropped quads must stay
/// dropped — the defect was drawing them anyway — and the replacement
/// triangle must be dealt into the chunk its world position is in.
#[test]
fn a_replaced_patch_loses_its_quads_and_keeps_the_shipped_triangle() {
    let n = 64usize;
    let mut cpu = grid_mesh(n);
    // The patch is cells 40..48 in x and z: chunk (1, 1) of the 2x2. Its
    // INTERIOR vertices are referenced by its quads and by nothing else,
    // so they are the ones that must disappear.
    let dropped: Vec<u32> = (41..48)
        .flat_map(|z: usize| (41..48).map(move |x| (z * 64 + x) as u32))
        .collect();
    let mut idx = Vec::new();
    for z in 0..n - 1 {
        for x in 0..n - 1 {
            if (40..48).contains(&x) && (40..48).contains(&z) {
                continue; // the patch the munge replaced
            }
            let a = (z * n + x) as u32;
            let (b, c, d) = (a + 1, a + n as u32, a + n as u32 + 1);
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let base = cpu.positions.len() as u32;
    for p in [
        [6.0f32, -3.0, 90.0],
        [20.0, -3.0, 90.0],
        [13.0, -3.0, 104.0],
    ] {
        cpu.positions.push(p);
        cpu.normals.push([0.0, 1.0, 0.0]);
        cpu.uvs.push([0.0, 0.0]);
        cpu.colors.push([255; 4]);
    }
    idx.extend_from_slice(&[base, base + 1, base + 2]);
    cpu.indices = idx;

    let chunks = terrain_chunk_indices(&cpu).expect("chunks");
    let all: Vec<u32> = chunks.concat();
    assert_eq!(all.len(), cpu.indices.len(), "every triangle is dealt once");
    assert!(
        !dropped.iter().any(|v| all.contains(v)),
        "a dropped quad came back"
    );
    // The three appended vertices sit at world (6..20, 90..104) = cells
    // (40..46, 42..46), so their triangle belongs to chunk (1, 1).
    assert!(
        chunks[3].ends_with(&[base, base + 1, base + 2]),
        "chunk (1,1)"
    );
}

/// The whole untextured policy, as three arms. See [`route_untextured`].
#[test]
fn untextured_routing_skips_the_disc_s_own_gaps_and_greys_only_our_misses() {
    // The disc named no texture at all: skipped, not painted.
    assert_eq!(route_untextured(None), Untextured::DiscUntextured);
    // An empty material name is the same thing — `upload_submeshes` maps
    // it to `None`, but the bake writes it as "" and either can arrive.
    assert_eq!(route_untextured(Some("")), Untextured::DiscUntextured);

    // A texture that is on neither disc: skipped, and counted apart.
    for name in DANGLING_ON_DISC {
        assert_eq!(route_untextured(Some(name)), Untextured::Dangling, "{name}");
    }
    // Matched case-insensitively — the discs spell these `yav_prop_ivy`,
    // `Cor1_bldg_flowers` and `noIcon`.
    assert_eq!(
        route_untextured(Some("Cor1_bldg_flowers")),
        Untextured::Dangling
    );
    assert_eq!(route_untextured(Some("noIcon")), Untextured::Dangling);

    // Anything else that fails to resolve is OUR bug: grey and counted.
    assert_eq!(
        route_untextured(Some("tat1_bldg_wall")),
        Untextured::LookupFailed
    );
    // Near-misses on the dangling list must NOT be swallowed by it.
    assert_eq!(
        route_untextured(Some("yav_prop_ivy2")),
        Untextured::LookupFailed
    );
    assert_eq!(route_untextured(Some("icon")), Untextured::LookupFailed);
}

/// The V address-mode rule over the shapes `artifacts/sky_wrap/dome_uv.txt`
/// actually holds — the four overshooting shells that made the horizon
/// stripe, the two base domes that genuinely tile in V, and the ordinary
/// [0, 1] shell.
#[test]
fn the_v_wrap_rule_over_the_shipped_dome_segments() {
    use render::WrapV::{ClampToEdge, Repeat};
    // Overshoot past v = 1: clamp, or the last rows wrap to the zenith.
    assert_eq!(sky_wrap_v(0.0020, 1.0010), ClampToEdge); // hoth_sky_dome
    assert_eq!(sky_wrap_v(0.0020, 1.0767), ClampToEdge); // mus1_sky_dome prim1
    assert_eq!(sky_wrap_v(0.0308, 1.1304), ClampToEdge); // BF1 NAB1 sky_water
                                                         // Genuinely tiling base domes: leave them repeating.
    assert_eq!(sky_wrap_v(-1.7080, 2.7080), Repeat); // BF2 POL1 pol1_skydome
    assert_eq!(sky_wrap_v(-0.5029, 1.5015), Repeat); // BF2 TAN1 tan1_sky_dome
                                                     // The ordinary case — and the one the bilinear footprint still
                                                     // wraps without the clamp.
    assert_eq!(sky_wrap_v(0.0, 1.0), ClampToEdge);
    // The tolerance is symmetric and exclusive at its own edges.
    assert_eq!(sky_wrap_v(-0.25, 1.25), ClampToEdge);
    assert_eq!(sky_wrap_v(-0.2501, 1.0), Repeat);
    assert_eq!(sky_wrap_v(0.0, 1.2501), Repeat);
}

/// The ocean foam's UV scale is the base repeat over the foam's (both KAM1s
/// 40 / 32 = 1.25), 1 on a bake without the foam field.
#[test]
fn foam_uv_scale_is_base_over_foam_repeat() {
    let c = manifest::ManifestWaterConfig {
        metres_per_repeat: Some([40.0, 40.0]),
        foam_metres_per_repeat: Some([32.0, 32.0]),
        ..Default::default()
    };
    assert_eq!(super::foam_uv_scale(Some(&c)), [1.25, 1.25]);
    let old = manifest::ManifestWaterConfig {
        foam_metres_per_repeat: None,
        ..c.clone()
    };
    assert_eq!(super::foam_uv_scale(Some(&old)), [1.0, 1.0]);
    assert_eq!(super::foam_uv_scale(None), [1.0, 1.0]);
}
