//! Tier-1 tests for `tern -> WATR` against real disc data (gitignored; these
//! SKIP cleanly when it is absent). Expected values are the measurements in
//! the `mungefs::water` module header.

use super::support::{lvl, tern_of};
use mungefs::water::Water;

macro_rules! require_data {
    ($e:expr, $what:expr) => {
        match $e {
            Some(v) => v,
            None => {
                eprintln!("SKIP: extracted {} not present", $what);
                return;
            }
        }
    };
}

/// BF1 Kashyyyk Islands: the clearest water map on either disc — the whole
/// map is sea except one island, so the mask is unambiguous.
#[test]
fn bf1_kas1_water_grid_and_height() {
    let buf = require_data!(lvl("bf1/lvl/DATA/_LVL_PS2/KAS/KAS1.LVL"), "BF1 KAS1.LVL");
    let w = Water::parse(&tern_of(&buf))
        .unwrap()
        .expect("KAS1 has WATR");

    assert_eq!(w.info.grid, 64);
    assert_eq!(w.info.cell_size, 32.0);
    assert_eq!(w.info.half_extent, [1024.0, 1024.0]);
    // The relation that says half_extent really is a HALF-extent.
    assert_eq!(
        w.info.grid as f32 * w.info.cell_size,
        2.0 * w.info.half_extent[0]
    );
    assert_eq!(w.info.unknown_1, 2, "INFO[1] is 2 on all 14 water chunks");

    assert_eq!(w.cells.len(), 64 * 64);
    assert_eq!(w.wet_cells(), 4014, "98 % of Kashyyyk 1 is sea");
    assert_eq!(w.layer.height, -45.0);
    assert_eq!(w.layer.unknown, 10, "LAYR u32 is 10 on all 14");
    // KAS1 is one of the five that name no texture even though `kas1_water`
    // is in its `tex_` list.
    assert_eq!(w.layer.texture, "");
    assert_eq!(w.layer.uv_velocity, [0.005, -0.005]);
    assert_eq!(w.layer.uv_tile, [2.0, 2.0]);
    assert!(w.patches.is_empty(), "KAS1 ships a zero-length CPAT");
}

/// The nibble order and the row-major layout, checked the way they were
/// found: the terrain under a wet cell must lie below the water plane, and
/// the terrain under a dry cell must not.
#[test]
fn bf1_kas1_wet_cells_sit_over_low_ground() {
    let buf = require_data!(lvl("bf1/lvl/DATA/_LVL_PS2/KAS/KAS1.LVL"), "BF1 KAS1.LVL");
    let tern = tern_of(&buf);
    let w = Water::parse(&tern).unwrap().unwrap();
    let t = mungefs::terrain::Terrain::parse(&tern).unwrap();

    let n = t.info.grid_size as usize;
    let ground = |wx: f32, wz: f32| -> Option<f32> {
        let tx = ((wx + n as f32 / 2.0 * t.info.grid_unit) / t.info.grid_unit).round();
        let tz = ((wz + (n as f32 / 2.0 - 1.0) * t.info.grid_unit) / t.info.grid_unit).round();
        if tx < 0.0 || tz < 0.0 || tx >= n as f32 || tz >= n as f32 {
            return None;
        }
        Some(t.raw[tz as usize * n + tx as usize] as f32 * t.info.height_scale)
    };

    let (mut wet_below, mut wet, mut dry_below, mut dry) = (0, 0, 0, 0);
    let g = w.info.grid as usize;
    for z in 0..g {
        for x in 0..g {
            let o = w.cell_origin(x, z);
            let half = w.info.cell_size / 2.0;
            let Some(y) = ground(o[0] + half, o[1] + half) else {
                continue;
            };
            if w.is_wet(x, z) {
                wet += 1;
                wet_below += (y < w.layer.height) as u32;
            } else {
                dry += 1;
                dry_below += (y < w.layer.height) as u32;
            }
        }
    }
    let pct = |a: u32, b: u32| 100.0 * a as f32 / b as f32;
    assert!(
        pct(wet_below, wet) > 95.0,
        "wet cells over low ground: {:.1}%",
        pct(wet_below, wet)
    );
    assert!(
        pct(dry_below, dry) < 5.0,
        "dry cells over low ground: {:.1}%",
        pct(dry_below, dry)
    );
}

/// BF2 Death Star ships a `tern` that is NAME + WATR and nothing else, so the
/// heightfield parser refuses it while the water still has to come out. Its
/// single `CPAT` patch is also the smallest on either disc.
#[test]
fn bf2_dea1_water_survives_a_stub_tern() {
    let buf = require_data!(lvl("bf2/lvl/DATA/_LVL_PS2/DEA/DEA1.LVL"), "BF2 DEA1.LVL");
    let tern = tern_of(&buf);
    assert!(
        mungefs::terrain::Terrain::is_stub(&tern),
        "DEA1's tern is a heightfield stub"
    );

    let w = Water::parse(&tern).unwrap().expect("DEA1 has WATR");
    assert_eq!(w.layer.height, 41.75);
    assert_eq!(w.layer.texture, "dea1_water");
    assert_eq!(w.wet_cells(), 2);
    assert_eq!(w.patches.len(), 1);

    let p = &w.patches[0];
    assert_eq!(p.aabb.0, [-176.0, 41.75, -240.0]);
    assert_eq!(p.aabb.1, [-112.0, 42.75, -176.0]);
    assert_eq!(p.positions.len(), 14);
    assert_eq!(p.triangles.len(), 12);
    // The proof that `height` is the water plane: the patch BOX allows a 1 m
    // range and every vertex still lands exactly on the water height.
    for v in &p.positions {
        assert_eq!(v[1], w.layer.height, "patch vertex off the water plane");
    }
}

/// Every water map at once: the invariants that hold across all twelve.
#[test]
fn every_water_map_agrees_with_the_decoded_shape() {
    const MAPS: &[(&str, &str)] = &[
        ("bf1", "KAM/KAM1"),
        ("bf1", "KAS/KAS1"),
        ("bf1", "KAS/KAS2"),
        ("bf1", "NAB/NAB2"),
        ("bf1", "YAV/YAV1"),
        ("bf2", "DAG/DAG1"),
        ("bf2", "DEA/DEA1"),
        ("bf2", "FEL/FEL1"),
        ("bf2", "KAM/KAM1"),
        ("bf2", "KAS/KAS2"),
        ("bf2", "NAB/NAB2"),
        ("bf2", "YAV/YAV1"),
    ];
    let mut seen = 0;
    for (game, rel) in MAPS {
        let Some(buf) = lvl(&format!("{game}/lvl/DATA/_LVL_PS2/{rel}.LVL")) else {
            continue;
        };
        seen += 1;
        let w = Water::parse(&tern_of(&buf))
            .unwrap()
            .unwrap_or_else(|| panic!("{game} {rel} should carry WATR"));

        assert_eq!(w.info.unknown_1, 2, "{rel}");
        assert_eq!(w.layer.unknown, 10, "{rel}");
        assert_eq!(
            w.info.grid as f32 * w.info.cell_size,
            2.0 * w.info.half_extent[0],
            "{rel}: grid x cell must span 2 x half-extent"
        );
        assert_eq!(w.info.half_extent[0], w.info.half_extent[1], "{rel}");
        assert_eq!(w.cells.len(), (w.info.grid * w.info.grid) as usize, "{rel}");
        // Nibbles are only ever 0, 1 or 9 across all twelve maps.
        for c in &w.cells {
            assert!(matches!(c, 0 | 1 | 9), "{rel}: WMAP nibble {c}");
        }
        assert!(w.wet_cells() > 0, "{rel} has no wet cell");
        // uv_tile is always a matched pair of 1.0 or 2.0.
        assert_eq!(w.layer.uv_tile[0], w.layer.uv_tile[1], "{rel}");
        assert!(matches!(w.layer.uv_tile[0], 1.0 | 2.0), "{rel}");
        // Every CPAT vertex on every map sits on the water plane.
        for p in &w.patches {
            for v in &p.positions {
                assert_eq!(v[1], w.layer.height, "{rel}");
            }
        }
    }
    if seen == 0 {
        eprintln!("SKIP: no extracted map LVLs present");
    } else {
        assert_eq!(seen, 12, "all twelve water maps present and parsed");
    }
}

/// The negative half of the census: a map with no `WATR` returns `None`
/// rather than erroring, and `sky_ WaterInfo` is NOT this data — it is an
/// all-zero `Enabled(0)` default that appears mostly on maps with no water at
/// all (TAT1 is one of them).
#[test]
fn tat1_has_no_water() {
    let buf = require_data!(lvl("bf1/lvl/DATA/_LVL_PS2/TAT/TAT1.LVL"), "BF1 TAT1.LVL");
    assert_eq!(Water::parse(&tern_of(&buf)).unwrap(), None);
}
