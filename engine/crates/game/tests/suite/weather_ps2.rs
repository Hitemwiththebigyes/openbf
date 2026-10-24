//! Card WTHR's referees: the precipitation rules against PS2 numbers
//! (`analysis/re-weather.md`).
//!
//! `data/weather/*.json` are three views exported out of PCSX2 save states by
//! `lead_tmp/wthr/tools/predict.py --export`: the camera, the Precipitation
//! object's fields, its particle array, the wind and the rain-shadow regions
//! as PS2 RAM held them, and the cells / alphas / streak the RE's model draws
//! from them — the model that was scored against those states' own frames
//! (re-weather.md §9). BF2 KAM1 cp1→cp6 is a view with rain shadows in sight
//! (27 cells, the 2.5 × walk); BF2 MUS1 cp0→cp5 a plain one (28 cells); BF2
//! KAM1 cp0→cp1 stands inside a hall (4 cells left, over the roof line —
//! the PS2 frame shows no rain and the port draws no streak pixel there).

use game::fx::emitter::Lcg;
use game::weather::{
    cell_walk, splash_box, splash_interval, streak_alpha, streak_rgb, streak_vector, Precip,
    Shadow, ShadowKind, View, Weather, Wind, TILE,
};
use glam::Vec3;
use manifest::ManifestRegion;
use serde_json::Value;

fn v3(v: &Value) -> Vec3 {
    Vec3::new(
        v[0].as_f64().unwrap() as f32,
        v[1].as_f64().unwrap() as f32,
        v[2].as_f64().unwrap() as f32,
    )
}

fn f(v: &Value) -> f32 {
    v.as_f64().unwrap() as f32
}

struct Vector {
    precip: Precip,
    view: View,
    wind: Vec3,
    shadows: Vec<Shadow>,
    expect: Value,
}

fn load(name: &str) -> Vector {
    let path = format!(
        "{}/tests/data/weather/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let j: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let (c, p) = (&j["camera"], &j["precipitation"]);
    let color: Vec<u8> = p["color"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_u64().unwrap() as u8)
        .collect();
    let precip = Precip {
        streaks: p["type"].as_u64().unwrap() == 1,
        texture: None,
        particle_size: f(&p["particle_size"]),
        range: f(&p["range"]),
        color: [color[0], color[1], color[2], color[3]],
        alpha: [0.0, 1.0],
        count: p["count"].as_u64().unwrap() as usize,
        velocity: f(&p["velocity"]),
        velocity_range: 1.0,
        streak_seconds: f(&p["streak_seconds"]),
        // Standing still, the camera's velocity is zero and the scales idle.
        cross_scale: 1.0,
        axial_scale: 1.0,
        rotation: 0.0,
        rotation_range: 0.0,
        ground_effect: None,
        ground_per_sec: 0.0,
        ground_spread: 0.0,
    };
    let view = View {
        pos: v3(&c["pos"]),
        x: v3(&c["x"]),
        y: v3(&c["y"]),
        z: v3(&c["z"]),
        near: f(&c["near"]),
        zoom: f(&c["zoom"]),
        tan_h: f(&c["tan_h"]),
        tan_v: f(&c["tan_v"]),
    };
    let shadows = j["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| Shadow {
            kind: match r["kind"].as_u64().unwrap() {
                5 => ShadowKind::AaBox,
                7 => ShadowKind::Cylinder(1),
                // The vector does not carry a rotated box's rows; none of
                // them is in view in either vector.
                2 => ShadowKind::Box([Vec3::X, Vec3::Y, Vec3::Z]),
                k => panic!("region kind {k}"),
            },
            center: v3(&r["pos"]),
            size: v3(&r["size"]),
            in_view: r["in_view"].as_u64().unwrap() != 0,
        })
        .collect();
    Vector {
        precip,
        view,
        wind: v3(&j["wind"]),
        shadows,
        expect: j["expect"].clone(),
    }
}

/// The port's walk against the vector's: (missing, extra, worst alpha gap).
fn compare(v: &Vector, tile_scale: f32, shadows: &[Shadow]) -> (usize, usize, f32) {
    let mut view = v.view;
    // A wrong tile, as a mutation: the same walk on a world scaled by it.
    view.pos /= tile_scale;
    let walk = cell_walk(&v.precip, &view, shadows);
    let want: Vec<([i32; 3], f32)> = v.expect["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let i = c["index"].as_array().unwrap();
            let i = [0, 1, 2].map(|k| i[k].as_i64().unwrap() as i32);
            (i, f(&c["alpha"]))
        })
        .collect();
    let mut worst = 0f32;
    let mut missing = 0;
    for (i, a) in &want {
        match walk.cells.iter().find(|(j, _)| j == i) {
            Some((_, b)) => worst = worst.max((a - b).abs()),
            None => missing += 1,
        }
    }
    let extra = walk
        .cells
        .iter()
        .filter(|(j, _)| !want.iter().any(|(i, _)| i == j))
        .count();
    (missing, extra, worst)
}

/// Acceptance 1: from the PS2's camera, regions and effect fields the port
/// draws the same cells with the same alphas, the same far / fade start /
/// dpx and the same streak.
#[test]
fn the_cell_walk_is_the_ps2_models_on_all_three_vectors() {
    for (name, cells) in [
        ("bf2_kam1_cp1_cp6_b000", 27),
        ("bf2_mus1_cp0_cp5_b000", 28),
        ("bf2_kam1_cp0_cp1_b000", 4),
    ] {
        let v = load(name);
        let walk = cell_walk(&v.precip, &v.view, &v.shadows);
        let e = &v.expect;
        assert!(
            (walk.far - f(&e["far"])).abs() < 1e-3,
            "{name} far {}",
            walk.far
        );
        assert!(
            (walk.fade_start - f(&e["fade_start"])).abs() < 1e-3,
            "{name} fade start {}",
            walk.fade_start
        );
        assert!(
            (walk.dpx - f(&e["dpx"])).abs() < 1e-3,
            "{name} dpx {}",
            walk.dpx
        );
        assert_eq!(walk.cells.len(), cells, "{name}");
        let (missing, extra, worst) = compare(&v, 1.0, &v.shadows);
        assert_eq!((missing, extra), (0, 0), "{name}: the cell set");
        assert!(worst < 1e-3, "{name}: worst alpha gap {worst}");
        let s = streak_vector(&v.precip, v.wind, Vec3::ZERO, &v.view);
        assert!(s.abs_diff_eq(v3(&e["streak"]), 1e-5), "{name} streak {s}");
        println!(
            "{name}: {} cells, worst alpha gap {worst:.2e}, far {} fade {} dpx {:.4} streak {s}",
            walk.cells.len(),
            walk.far,
            walk.fade_start,
            walk.dpx
        );
    }
}

/// The rain-shadow rule is what the Kamino vector tests: with its regions
/// taken out of view the walk is the plain one and the cell set is another;
/// and the port's own "in view" test flags the regions PS2 RAM had flagged.
#[test]
fn rain_shadows_shape_the_walk() {
    let v = load("bf2_kam1_cp1_cp6_b000");
    assert!(v.shadows.iter().any(|s| s.in_view));
    let dry: Vec<Shadow> = v
        .shadows
        .iter()
        .map(|s| Shadow {
            in_view: false,
            ..*s
        })
        .collect();
    let (missing, extra, _) = compare(&v, 1.0, &dry);
    assert!(missing + extra > 0, "no rain shadow: the same cells");
    let plain = cell_walk(&v.precip, &v.view, &dry);
    assert_eq!(plain.far, 12.5);
    // The same regions, flagged by the port's frustum test.
    let mut w = Weather::new(v.precip.clone(), Wind::still(), dry, 1);
    let walk = w.look(&v.view);
    let (ram, port): (Vec<bool>, Vec<bool>) = v
        .shadows
        .iter()
        .zip(&w.shadows)
        .map(|(a, b)| (a.in_view, b.in_view))
        .unzip();
    println!("in view, PS2 RAM: {ram:?}\nin view, port:    {port:?}");
    assert_eq!(walk.far, 31.25, "a shadow in view: 2.5 x Range");
    assert_eq!(ram, port, "the regions in view");
    let (missing, extra, worst) = {
        let mut flagged = v.shadows.clone();
        for (s, p) in flagged.iter_mut().zip(&port) {
            s.in_view = *p;
        }
        compare(&v, 1.0, &flagged)
    };
    assert_eq!((missing, extra), (0, 0));
    assert!(worst < 1e-3);
}

/// Inside Kamino's hall the cells round the camera are shut: what is left
/// is four dim cells over the roof line, off the screen. Without the regions the same view
/// would rain indoors.
#[test]
fn a_hall_is_dry() {
    let v = load("bf2_kam1_cp0_cp1_b000");
    let walk = cell_walk(&v.precip, &v.view, &v.shadows);
    assert_eq!(walk.cells.len(), 4);
    // One row of cells over the roof line, 9 m up, dim.
    for (i, a) in &walk.cells {
        let up = game::weather::cell_centre(*i).y - v.view.pos.y;
        assert!(up > 9.0 && *a < 0.31, "cell {i:?} {up} m up, alpha {a}");
    }
    let dry: Vec<Shadow> = v
        .shadows
        .iter()
        .map(|s| Shadow {
            in_view: false,
            ..*s
        })
        .collect();
    let open = cell_walk(&v.precip, &v.view, &dry);
    assert!(open.cells.len() > 20, "{} cells", open.cells.len());
    assert!(
        open.cells.iter().any(|(_, a)| *a > 0.9),
        "rain at the camera"
    );
}

/// A wrong tile is another picture: the vector's cells are 5 m cells.
#[test]
fn the_tile_is_five_metres() {
    assert_eq!(TILE, 5.0);
    let v = load("bf2_mus1_cp0_cp5_b000");
    let c = game::weather::cell_centre([-14, 12, -11]);
    assert_eq!(c, Vec3::new(-67.5, 62.5, -52.5));
    let (missing, extra, _) = compare(&v, 1.25, &v.shadows);
    assert!(missing + extra > 0);
}

/// §6's RAM row (`bf2_kam1_walk_cp0_cp1_b000`): mid-glide the wind is on
/// the straight line from `from` to `target` at the clock; past 1 it holds,
/// past 1.5 it draws a new target within the range of the base.
#[test]
fn the_wind_glides_holds_and_redraws() {
    let mut rng = Lcg(7);
    let base = Vec3::new(1.0, 0.0, 0.3);
    let mut w = Wind::new(base, 1.0, 0.1);
    w.from = Vec3::new(1.2584, 0.0, 0.6926);
    w.target = Vec3::new(1.1839, 0.0, 0.6062);
    let dt = 1.0 / 60.0;
    w.clock = 0.8667 - 0.1 * dt;
    w.update(dt, &mut rng);
    assert!(
        w.current.abs_diff_eq(Vec3::new(1.1938, 0.0, 0.6177), 1e-4),
        "{}",
        w.current
    );
    // A 10 s glide, then a 5 s hold on the target.
    w.clock = 1.2;
    w.update(dt, &mut rng);
    assert_eq!((w.current, w.from), (w.target, w.target));
    let held = w.target;
    w.clock = 1.5;
    w.update(dt, &mut rng);
    assert_eq!(w.clock, 0.0);
    assert_eq!(w.current, held, "the glide starts from the held vector");
    assert_ne!(w.target, held);
    // Along the base ± range, across at most R²/(R + L).
    let l = base.length();
    let d = w.target - base;
    let along = d.dot(base / l);
    let across = d.dot(Vec3::new(base.z, 0.0, -base.x) / l);
    assert!(along.abs() <= 1.0 + 1e-5 && across.abs() <= 1.0 / (1.0 + l) + 1e-5);
    // No wind stays none.
    let mut still = Wind::still();
    still.update(100.0, &mut rng);
    assert_eq!(still.current, Vec3::ZERO);
}

/// §3.4: a streak's colour is `Color` applied twice — Kamino's (216, 220,
/// 228) draws as (91, 94, 101) — and its alpha the particle's times the
/// cell's on the GS scale.
#[test]
fn a_streak_colour_is_color_applied_twice() {
    assert_eq!(streak_rgb([216, 220, 228, 255]), [91, 94, 101]);
    assert_eq!(streak_rgb([255, 255, 255, 255]), [128, 128, 128]);
    assert_eq!(streak_alpha(255, 1.0, 1.0), 1.0);
    assert_eq!(streak_alpha(255, 1.0, 0.0), 0.0);
    let a = streak_alpha(255, 0.85, 0.5);
    assert!((a - 0.425).abs() < 0.01, "{a}");
}

fn precip(size: f32) -> Precip {
    let mut p = load("bf2_mus1_cp0_cp5_b000").precip;
    p.particle_size = size;
    p
}

/// §3.4: a flake's edge is 2.83 × ⌊size·256⌋/256 — 3.3 cm for the snows,
/// 7.7 cm for Mygeeto's ash — and it takes no distance fade: every flake of
/// every drawn cell carries its own alpha.
#[test]
fn a_flake_has_the_ps2s_edge_and_no_distance_fade() {
    assert!((precip(0.015).flake_edge() - 0.0332).abs() < 1e-4);
    assert!((precip(0.03).flake_edge() - 0.0774).abs() < 1e-4);
    assert!((precip(0.025).flake_edge() - 0.0663).abs() < 1e-4);
    let v = load("bf2_mus1_cp0_cp5_b000");
    let mut w = Weather::new(v.precip.clone(), Wind::still(), v.shadows.clone(), 3);
    let walk = w.look(&v.view);
    assert!(walk.cells.iter().any(|(_, a)| *a < 0.5), "a faded cell");
    let own: Vec<u8> = w
        .particles
        .iter()
        .map(|q| (q.alpha * 255.0) as u8)
        .collect();
    let draws = w.draws(&v.view, Vec3::ZERO, None, 448.0);
    assert_eq!(draws.len(), walk.cells.len() * v.precip.count);
    for (i, d) in draws.iter().enumerate() {
        assert_eq!(d.color[3], own[i % own.len()], "flake {i}");
        assert!(!d.additive);
    }
}

/// §5: Kamino looking level — spread 8, 7 a second — drops ~23.5 splashes a
/// second into a 12.7 × 11 m footprint, and none lands under a rain shadow.
#[test]
fn splashes_fall_at_the_ps2s_rate_outside_the_shadows() {
    let rate = 1.0 / splash_interval(8.0, 7.0, 12.7, 11.0);
    assert!((rate - 23.5).abs() < 0.05, "{rate}");
    let view = View {
        pos: Vec3::new(0.0, 10.0, 0.0),
        x: Vec3::X,
        y: Vec3::Y,
        z: Vec3::Z,
        near: 0.5,
        zoom: 1.0,
        tan_h: 0.637,
        tan_v: 0.488,
    };
    let (lo, hi) = splash_box(8.0, &view);
    assert!((hi.x - lo.x - 12.694).abs() < 1e-3 && (hi.z - lo.z - 11.0).abs() < 1e-3);
    let mut p = load("bf2_kam1_cp1_cp6_b000").precip;
    p.ground_effect = Some("com_sfx_rainsplash".into());
    p.ground_per_sec = 7.0;
    p.ground_spread = 8.0;
    // A shadow over the footprint's far half (z < -5.5).
    let roof = Shadow {
        kind: ShadowKind::AaBox,
        center: Vec3::new(0.0, 0.0, -11.0),
        size: Vec3::new(50.0, 20.0, 4.5),
        in_view: true,
    };
    let mut w = Weather::new(p, Wind::still(), vec![roof], 9);
    let mut landed = Vec::new();
    for _ in 0..600 {
        landed.extend(w.splashes(1.0 / 60.0, &view, 1, &mut |top| {
            assert_eq!(top.y, 60.0, "the ray starts 50 m over the camera");
            Some(Vec3::new(top.x, 0.0, top.z))
        }));
    }
    assert!(!landed.is_empty());
    assert!(
        landed.iter().all(|h| h.z > -11.0 + 4.5 + 1.0 - 1e-4),
        "1 m clear"
    );
    // 10 s at 23.5 a second, about half of them under the roof.
    let tried = 23.5 * 10.0;
    let share = landed.len() as f32 / tried;
    assert!((0.38..0.62).contains(&share), "{} landed", landed.len());
    // Two views halve BF2's rate.
    let mut w2 = Weather::new(w.precip.clone(), Wind::still(), Vec::new(), 9);
    let n: usize = (0..600)
        .map(|_| w2.splashes(1.0 / 60.0, &view, 2, &mut |t| Some(t)).len())
        .sum();
    assert!((n as f32 - tried / 2.0).abs() < 3.0, "{n}");
}

fn region(shape: &str, rotation: [[f32; 3]; 3], size: [f32; 3]) -> ManifestRegion {
    ManifestRegion {
        shape: shape.into(),
        name: "rainshadow3".into(),
        rotation,
        translation: [10.0, 0.0, 0.0],
        size,
        props: Vec::new(),
    }
}

/// §4: which regions are rain shadows, their kinds, and the overlap tests of
/// kinds 5 and 7 and kind 2's centre test.
#[test]
fn rain_shadow_regions_and_their_overlap_tests() {
    let flip = [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]];
    let mut other = region("box", flip, [1.0; 3]);
    other.name = "cp1_capture".into();
    assert_eq!(Shadow::from_region(&other), None);
    let h = 2.5;
    // Kind 5: |Δ| < half + size on all three axes.
    let b = Shadow::from_region(&region("box", flip, [4.0, 2.0, 3.0])).unwrap();
    assert_eq!(b.kind, ShadowKind::AaBox);
    assert!(b.overlaps_cell(Vec3::new(16.4, 0.0, 0.0), h));
    assert!(!b.overlaps_cell(Vec3::new(16.5, 0.0, 0.0), h));
    assert!(b.overlaps_cell(Vec3::new(10.0, 4.4, 5.4), h));
    assert!(!b.overlaps_cell(Vec3::new(10.0, 4.6, 0.0), h));
    // A quarter turn about Y is still axis-aligned, its sizes swapped.
    let turned = [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];
    let t = Shadow::from_region(&region("box", turned, [4.0, 2.0, 3.0])).unwrap();
    assert_eq!(
        (t.kind, t.size),
        (ShadowKind::AaBox, Vec3::new(3.0, 2.0, 4.0))
    );
    // Kind 7: an upright cylinder, radius √(x² + z²), its yaw dropped
    // (BF2 KAM1's rainshadow10 is turned 1°).
    let yaw = [[-0.9998, 0.0, 0.02], [0.0, 1.0, 0.0], [-0.02, 0.0, -0.9998]];
    let c = Shadow::from_region(&region("cylinder", yaw, [3.0, 6.0, 4.0])).unwrap();
    assert_eq!(c.kind, ShadowKind::Cylinder(1));
    assert!((c.size.x - 5.0).abs() < 2e-3 && (c.size.y - 6.0).abs() < 1e-5);
    let c = Shadow {
        size: Vec3::new(5.0, 6.0, 5.0),
        ..c
    };
    assert!(
        c.overlaps_cell(Vec3::new(10.0, 8.4, 0.0), h),
        "over the axis"
    );
    assert!(!c.overlaps_cell(Vec3::new(10.0, 8.5, 0.0), h), "above it");
    assert!(
        !c.overlaps_cell(Vec3::new(17.5, 0.0, 0.0), h),
        "|dx| >= half + r"
    );
    // At the corner: (|dx| - half)² + (|dz| - half)² < r².
    assert!(
        c.overlaps_cell(Vec3::new(16.0, 0.0, 6.0), h),
        "3.5² + 3.5² < 25"
    );
    assert!(
        !c.overlaps_cell(Vec3::new(16.1, 0.0, 6.1), h),
        "3.6² + 3.6² > 25"
    );
    // Kind 2, a box turned 30° about Y: not an overlap test — each side's
    // projected half-size alone against the gap, so a cell a region-size
    // away from the centre is "dry" only when it is near the centre.
    let (s, co) = (0.5f32, 0.866_025_4f32);
    let tilt = [[co, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, co]];
    let r = Shadow::from_region(&region("box", tilt, [20.0, 10.0, 10.0])).unwrap();
    assert!(matches!(r.kind, ShadowKind::Box(_)));
    assert!(r.overlaps_cell(Vec3::new(10.0, 0.0, 0.0), h), "the centre");
    assert!(r.overlaps_cell(Vec3::new(13.0, 2.0, 0.0), h));
    // 8 m along the box's long axis is deep inside the box (half-length
    // 20), and the PS2 still calls it clear: the cell's projected half-size
    // (2.5·1.366) is under the gap.
    assert!(!r.overlaps_cell(Vec3::new(10.0, 0.0, 0.0) + Vec3::new(co, 0.0, -s) * 8.0, h));
    // A tilted cylinder never shades (kind 3).
    let lean = [[1.0, 0.0, 0.0], [0.0, 0.8, 0.6], [0.0, -0.6, 0.8]];
    let k3 = Shadow::from_region(&region("cylinder", lean, [3.0, 6.0, 4.0])).unwrap();
    assert_eq!(k3.kind, ShadowKind::Other);
    assert!(!k3.overlaps_cell(Vec3::new(10.0, 0.0, 0.0), h));
}

/// §2.2–§2.3: the setup's ranges, the fall's wrap in the unit cube, and the
/// half-second step toward a new extreme.
#[test]
fn particles_fall_wrap_and_reroll() {
    let mut p = load("bf2_kam1_cp1_cp6_b000").precip;
    p.alpha = [0.8, 1.0];
    p.velocity_range = 1.0;
    let mut w = Weather::new(
        p,
        Wind::new(Vec3::new(1.0, 0.0, 0.3), 0.0, 0.0),
        Vec::new(),
        5,
    );
    assert_eq!(w.particles.len(), 80);
    for q in &w.particles {
        assert!(q.pos.cmpge(Vec3::ZERO).all() && q.pos.cmplt(Vec3::ONE).all());
        assert!(q.offset.abs().max_element() <= 1.0);
        assert!((0.8..=1.0).contains(&q.alpha));
    }
    let before = w.particles.clone();
    let dt = 1.0 / 60.0;
    w.update(dt);
    for (a, b) in before.iter().zip(&w.particles) {
        // (wind.x, -8, wind.z) + its offset, over a 5 m cube, wrapped.
        let want = a.pos + (Vec3::new(1.0, -8.0, 0.3) + a.offset) * (dt / 5.0);
        assert!((want - want.floor()).abs_diff_eq(b.pos, 1e-6));
        assert_eq!(a.offset, b.offset, "no re-roll in the first half second");
    }
    // 0.4·t passes 0.2 at 0.5 s: one step of ~20 % toward (±1, ±1, ±1).
    let mut stepped = false;
    for _ in 0..31 {
        let was = w.particles[0].offset;
        w.update(dt);
        let now = w.particles[0].offset;
        if now != was {
            assert!(!stepped, "one step in the first 0.53 s");
            stepped = true;
            for i in 0..3 {
                let to = if now[i] > was[i] { 1.0 } else { -1.0 };
                let k = (now[i] - was[i]) / (to - was[i]);
                assert!((k - 0.2).abs() < 0.02, "factor {k}");
            }
        }
        assert!(w
            .particles
            .iter()
            .all(|q| q.pos.cmpge(Vec3::ZERO).all() && q.pos.cmplt(Vec3::ONE).all()));
    }
    assert!(stepped);
}

/// §3.1: the tail is where the drop was `StreakLength`/30 s ago as the
/// moving player sees it — Kamino standing still 0.46 m — and it carries the
/// head's alpha down to 0 in a strip the PS2's pixel wide.
#[test]
fn a_streak_runs_from_the_drop_to_where_it_was() {
    let v = load("bf2_kam1_cp1_cp6_b000");
    let s = streak_vector(&v.precip, Vec3::new(1.0, 0.0, 0.3), Vec3::ZERO, &v.view);
    assert!((s.length() - 0.46).abs() < 0.005, "{}", s.length());
    assert!(s.y > 0.0, "the tail is above the drop");
    // Walking with the rain's drift shortens the cross part to 0.2 of it.
    let mut p = v.precip.clone();
    p.cross_scale = 0.2;
    let run = v.view.x * 5.0;
    let s2 = streak_vector(&p, Vec3::ZERO, run, &v.view);
    assert!((s2.dot(v.view.x) - 5.0 * 0.2 * p.streak_seconds).abs() < 1e-5);
    // Longer than a tile: cut to one.
    p.streak_seconds = 1.0;
    assert!((streak_vector(&p, Vec3::ZERO, Vec3::ZERO, &v.view).length() - 5.0).abs() < 1e-5);
    let mut w = Weather::new(v.precip.clone(), Wind::still(), v.shadows.clone(), 2);
    let draws = w.draws(&v.view, Vec3::ZERO, None, 448.0);
    assert!(!draws.is_empty());
    for d in &draws {
        let render::ParticleShape::Streaks { lines, rgb, width } = &d.shape else {
            panic!("rain draws streaks");
        };
        assert_eq!(lines.len(), 80);
        assert_eq!(*rgb, [91.0 / 255.0, 94.0 / 255.0, 101.0 / 255.0]);
        // One pixel of 512 across the PS2's view, per metre of depth.
        assert!((width - 2.0 * 0.63707 / 512.0).abs() < 1e-6);
        assert!(d.texture.is_none() && !d.additive);
    }
}
