//! Card VTURD: a building turret drawn turned to its aim
//! (`analysis/re-turret-draw.md` 2 [O], `game::viewer::turret_pose`).
//!
//! 1. The three node formulas against the PS2's own RAM
//!    (`tests/data/turret_pose_ps2.txt`: a human in five turrets of both
//!    games, one row per game update, every third row kept and none
//!    filtered - `lead_tmp/vturd/tools/mk_fixture.py`). Each check prints
//!    how many rows fit and what the alternative rules fit.
//! 2. Data-gated, through the BAKED rigs and class db and the scene's own
//!    draws: with the player in one turret and a second gunner in another,
//!    the drawn aimer node looks along the seat's `GunState::aim` and the
//!    drawn turret node has the `look` yaw, each within a degree.

use game::viewer::turret_pose as tp;
use glam::{Mat4, Vec3};
use std::collections::HashMap;

/// A matrix from 12 numbers: the X, Y, Z rows of the PS2's, then its row 3.
fn m12(f: &[f32]) -> Mat4 {
    let v = |i: usize| Vec3::new(f[3 * i], f[3 * i + 1], f[3 * i + 2]);
    Mat4::from_cols(
        v(0).extend(0.0),
        v(1).extend(0.0),
        v(2).extend(0.0),
        v(3).extend(1.0),
    )
}

fn worst(a: &Mat4, b: &Mat4) -> f32 {
    (0..16)
        .map(|i| (a.to_cols_array()[i] - b.to_cols_array()[i]).abs())
        .fold(0.0, f32::max)
}

/// Rotation only: the three axes.
fn worst_rot(a: &Mat4, b: &Mat4) -> f32 {
    (0..12)
        .map(|i| (a.to_cols_array()[i] - b.to_cols_array()[i]).abs())
        .fold(0.0, f32::max)
}

struct Class {
    ytf: f32,
    ptf: f32,
    bind: Mat4,
    /// The aimer node's bind transform relative to its parent.
    aimer: Option<Mat4>,
    /// (recoil, bind offset from the parent, parent is the aimer node).
    barrels: Vec<(f32, Vec3, bool)>,
}

#[derive(Default)]
struct Tally {
    rows: usize,
    turret: usize,
    full_pitch: usize,
    aimer_rows: usize,
    aimer: usize,
    aimer_as_camera: usize,
    barrel_rows: usize,
    barrel: usize,
    barrel_no_slide: usize,
    slid: usize,
}

const TOL: f32 = 1e-3;

fn tally() -> HashMap<String, Tally> {
    let mut classes: HashMap<String, Class> = HashMap::new();
    let mut out: HashMap<String, Tally> = HashMap::new();
    for l in include_str!("../data/turret_pose_ps2.txt").lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        let Some(kind) = w.first().filter(|k| !k.starts_with('#')) else {
            continue;
        };
        let tag = w[1].to_owned();
        let skip = if *kind == "C" { 3 } else { 2 };
        let f: Vec<f32> = w[skip..]
            .iter()
            .map(|x| x.parse().expect("a number"))
            .collect();
        if *kind == "C" {
            let p = Vec3::new(f[2], f[3], f[4]);
            let u = Vec3::new(f[5], f[6], f[7]);
            let bind = Mat4::from_cols(
                Vec3::X.extend(0.0),
                u.extend(0.0),
                Vec3::Z.extend(0.0),
                p.extend(1.0),
            );
            let mut i = 9;
            let aimer = (f[8] == 1.0).then(|| {
                let q = glam::Quat::from_xyzw(f[i + 3], f[i + 4], f[i + 5], f[i + 6]);
                let m = Mat4::from_rotation_translation(q, Vec3::new(f[i], f[i + 1], f[i + 2]));
                i += 7;
                m
            });
            let nb = f[i] as usize;
            let barrels = (0..nb)
                .map(|b| {
                    let k = i + 1 + 5 * b;
                    (
                        f[k],
                        Vec3::new(f[k + 1], f[k + 2], f[k + 3]),
                        f[k + 4] == 1.0,
                    )
                })
                .collect();
            classes.insert(
                tag,
                Class {
                    ytf: f[0],
                    ptf: f[1],
                    bind,
                    aimer,
                    barrels,
                },
            );
            continue;
        }
        let c = &classes[&tag];
        let t = out.entry(tag).or_default();
        let (yaw, pitch) = (f[0], f[1]);
        let node = m12(&f[2..14]);
        t.rows += 1;
        // The fixture's pitch is the PS2's own (positive = nose down).
        let mine = tp::turret_node(&c.bind, yaw * c.ytf, pitch * c.ptf);
        t.turret += usize::from(worst(&mine, &node) < TOL);
        let full = tp::turret_node(&c.bind, yaw * c.ytf, pitch);
        t.full_pitch += usize::from(worst(&full, &node) < TOL);
        let Some(local) = &c.aimer else {
            continue;
        };
        let dir = Vec3::new(f[14], f[15], f[16]);
        let anode = m12(&f[17..29]);
        t.aimer_rows += 1;
        // The parent's matrix now is the PS2's own turret node of this row.
        let mine = tp::aimer_node(&node, local, dir);
        t.aimer += usize::from(worst(&mine, &anode) < TOL);
        // The alternative: the node as the turret's (camera's) aim basis.
        let cam = tp::turret_node(&c.bind, yaw, pitch);
        t.aimer_as_camera += usize::from(worst_rot(&cam, &anode) < TOL);
        for (b, (recoil, off, on_aimer)) in c.barrels.iter().enumerate() {
            let k = 29 + 13 * b;
            let (state, bm) = (f[k], m12(&f[k + 1..k + 13]));
            let parent = if *on_aimer { &anode } else { &node };
            let slide = if state > 0.0 { state * recoil } else { 0.0 };
            t.barrel_rows += 1;
            t.slid += usize::from(state > 0.0);
            t.barrel += usize::from(worst(&tp::barrel_node(parent, *off, slide), &bm) < TOL);
            t.barrel_no_slide += usize::from(worst(&tp::barrel_node(parent, *off, 0.0), &bm) < TOL);
        }
    }
    out
}

/// The turret node, the aimer node and the barrels equal the PS2's to 1e-3
/// on the rows kept; the alternatives the RE ruled out do not.
#[test]
fn the_node_formulas_fit_the_ps2s_own_rows() {
    let t = tally();
    let mut tags: Vec<&String> = t.keys().collect();
    tags.sort();
    for tag in &tags {
        let x = &t[*tag];
        println!(
            "{tag}: turret node {} of {} (full pitch on the node: {}); aimer node {} of {} \
             (as the camera's aim: {}); barrels {} of {} ({} slid; no slide: {})",
            x.turret,
            x.rows,
            x.full_pitch,
            x.aimer,
            x.aimer_rows,
            x.aimer_as_camera,
            x.barrel,
            x.barrel_rows,
            x.slid,
            x.barrel_no_slide
        );
    }
    assert_eq!(tags.len(), 5);
    let (rows, fit): (usize, usize) = t
        .values()
        .fold((0, 0), |a, x| (a.0 + x.rows, a.1 + x.turret));
    assert_eq!(rows, 613);
    // The BF2 sampler does not read a row atomically: 3 of its 1,080 rows
    // are torn (re-turret-draw 2.2); the BF1 captures fit on every row.
    assert!(fit >= rows - 3, "turret node: {fit} of {rows}");
    for tag in ["nab1_gt", "nab1_gty", "myg1_lg"] {
        assert_eq!(t[tag].turret, t[tag].rows, "{tag}");
    }
    // Every capture pitches, and no node tilts with it.
    for x in t.values() {
        assert!(x.full_pitch * 2 < x.rows, "{} of {}", x.full_pitch, x.rows);
    }
    for tag in ["nab1_gty", "myg1_lg"] {
        let x = &t[tag];
        assert!(x.aimer_rows > 100, "{tag}");
        assert_eq!(x.aimer, x.aimer_rows, "{tag}: the aimer node");
        // 0 at 5e-4 in the RE's replay; at this test's 1e-3 one Naboo row.
        assert!(
            x.aimer_as_camera <= 1,
            "{tag}: the camera's aim must not fit"
        );
    }
    let b = &t["myg1_lg"];
    assert_eq!(b.barrel_rows, 228);
    assert!(
        b.barrel >= b.barrel_rows - 1,
        "{} of {}",
        b.barrel,
        b.barrel_rows
    );
    assert!(
        b.slid > 0 && b.barrel_no_slide < b.barrel,
        "the slide is seen"
    );
}

/// One map's battle world as `examples/turret_ride.rs` builds it.
struct World {
    scene: game::assets::Scene,
    sim: core::sim::Sim,
    build: game::vehicle_pads::PadBuild,
}

fn world(map: &str, factions: [&str; 2]) -> Option<World> {
    let dir = game::assets::baked_root().join(map);
    let mut scene = game::assets::load_scene(&dir).ok()?;
    scene.apply_mode_layer("con");
    let common = dir
        .parent()?
        .join(format!("common_{}", scene.manifest.source_game));
    if common.join("common.ron").exists() {
        scene.merge_common(game::assets::load_common(&common).ok()?);
    }
    let mut sim = core::sim::Sim::empty(1);
    sim.heightfield = scene.heightfield();
    scene.install_collision(&mut sim);
    let classes = game::assets::baked_root().join("classes");
    let f = [factions[0].to_owned(), factions[1].to_owned()];
    let build = game::vehicle_pads::install(&mut sim, &scene.manifest, &dir, &classes, &f).ok()?;
    Some(World { scene, sim, build })
}

fn deg(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(std::f32::consts::TAU);
    d.min(std::f32::consts::TAU - d).to_degrees()
}

/// The drawn pose of the turret standing at `pos`, read back from the
/// scene's draws: (turret node yaw, aimer node yaw, aimer node pitch).
fn drawn(
    gpu: &game::viewer::SceneGpu,
    scene: &game::assets::Scene,
    seat: &game::vehicle_class::SeatSpec,
    pos: Vec3,
) -> (f32, f32, f32) {
    let td = gpu
        .turrets
        .iter()
        .find(|t| t.pos == pos)
        .expect("the turret's building is drawn with a rig");
    let rig = td.rig();
    let spec = tp::PoseSpec::of(rig, seat);
    let inst = &scene.manifest.instances[td.instance];
    let world = game::viewer::instance_matrix(inst.rotation, inst.translation);
    // A node's matrix now, from a part drawn on it: part = world · node · bind⁻¹.
    let node = |n: usize| {
        let k = rig
            .parts
            .iter()
            .position(|p| *p == Some(n as u32))
            .unwrap_or_else(|| panic!("{}: no part on node {}", td.class, rig.nodes[n].name));
        let m = td.part_model(&gpu.draws, k).expect("the part is drawn");
        world.inverse() * m * tp::bind_of(rig, n)
    };
    let t = node(spec.turret.expect("a turret node"));
    let a = node(spec.aimer.expect("an aimer node"));
    let (ay, ap) = core::vehicle::gun::angles(a.z_axis.truncate());
    (core::vehicle::gun::angles(t.z_axis.truncate()).0, ay, ap)
}

/// Data-gated (the bake and a GPU). On each map: the player seated in one
/// turret of `class` turns his look with the stick for a second; a bot sits
/// in another, its gun state then set off its rest by hand (the AI's aim is
/// not what is tested). The scene's draws, posed as a battle poses them,
/// show both guns where their seats aim.
#[test]
fn the_drawn_turret_follows_its_seats_gun_state() {
    let cases = [
        ("bf1_nab1", ["rep", "cis"], "com_weap_gunturret"),
        ("bf2_myg1", ["rep", "cis"], "tur_bldg_recoilless_lg"),
        ("bf2_hot1", ["all", "imp"], "tur_bldg_hoth_dishturret"),
        ("bf2_hot1", ["all", "imp"], "tur_bldg_hoth_lasermortar"),
    ];
    let mut ran = 0;
    for (map, factions, class) in cases {
        let Some(mut w) = world(map, factions) else {
            eprintln!("SKIP {map}: not baked");
            continue;
        };
        let rigged = w
            .scene
            .manifest
            .classes
            .iter()
            .any(|k| k.name.eq_ignore_ascii_case(class) && k.turret_rig.is_some());
        if !rigged {
            eprintln!("SKIP {map}: {class} has no rig in this bake (older than card VTURD)");
            continue;
        }
        let Some(ci) = w
            .build
            .classes
            .iter()
            .position(|c| c.eq_ignore_ascii_case(class))
        else {
            panic!(
                "{map}: no {class} turret spawned among {:?}",
                w.build.classes
            );
        };
        let turrets: Vec<core::vehicle::VehicleHandle> = w
            .sim
            .alive_vehicles()
            .filter(|(_, v)| v.class as usize == ci)
            .map(|(h, _)| h)
            .collect();
        assert!(turrets.len() >= 2, "{map}: {} {class}", turrets.len());
        let seat = w.sim.seat_setup[ci]
            .as_ref()
            .and_then(|s| s.guns.iter().position(Option::is_some))
            .expect("a gunner seat with a gun");
        let at = |w: &World, h| w.sim.vehicle(h).unwrap().pos;
        let player = w
            .sim
            .spawn(&core::unit::UnitSpec::player(
                0,
                at(&w, turrets[0]) + Vec3::Z * 3.0,
            ))
            .expect("a player");
        let other = w
            .sim
            .spawn(&core::unit::UnitSpec {
                controller: core::unit::Controller::Ai,
                ..core::unit::UnitSpec::trooper(1, at(&w, turrets[1]) + Vec3::Z * 3.0)
            })
            .expect("a bot");
        assert!(w.sim.seat_unit(player, turrets[0], seat as u8));
        assert!(w.sim.seat_unit(other, turrets[1], seat as u8));
        // The gun states are made on the first tick.
        w.sim.step(&[core::sim::PadState::default()]);
        let rest = w.sim.vehicle(turrets[0]).unwrap().guns[seat]
            .clone()
            .unwrap();
        for _ in 0..30 {
            let pad = core::sim::PadState {
                rx: core::sim::PadState::quantize(0.8),
                ry: core::sim::PadState::quantize(0.5),
                ..Default::default()
            };
            w.sim.step(&[pad]);
        }
        {
            // A bot with nothing to shoot at has not armed its seat yet:
            // give it the class's gun state, as its first aim tick would.
            let v = w.sim.vehicle_mut(turrets[1]).unwrap();
            if v.guns.is_empty() {
                v.guns = (0..v.seats.len())
                    .map(|s| (s == seat).then(|| rest.clone()))
                    .collect();
            }
            let g = v.guns[seat].as_mut().unwrap();
            g.look = (rest.look.0 + 1.2, rest.look.1 + 0.1);
            g.aim = (rest.aim.0 + 1.1, rest.aim.1 + 0.15);
        }
        // At rest the pose is the bind pose: how far the class's parts move
        // from where the bake put them with the gun at its rest.
        {
            let k = w.scene.resolve_class(class).expect("the class");
            let rig = k.turret_rig.as_ref().expect("its rig");
            let spec = tp::PoseSpec::of(rig, &w.build.seats[ci][seat]);
            // The gun's own rest, before the aimer's first step.
            let (look, _, aim) = w.sim.seat_setup[ci].as_ref().unwrap().guns[seat]
                .as_ref()
                .unwrap()
                .rest();
            let at_rest = (look, aim);
            let now = tp::pose(rig, &spec, look, aim, &[]);
            let off = tp::part_matrices(rig, &now)
                .iter()
                .flatten()
                .map(|m| worst(m, &Mat4::IDENTITY))
                .fold(0.0, f32::max);
            println!(
                "{map} {class}: at rest ({at_rest:?}) the parts are {off:.5} off the bind pose"
            );
            assert!(off < 1e-4, "{map} {class}: {off}");
        }
        let mut r = render::Renderer::new_headless(64, 64).expect("headless renderer");
        let mut gpu = game::viewer::upload_scene(&mut r, &w.scene).expect("upload");
        let all = gpu.turrets.len();
        let whole = gpu
            .turrets
            .iter()
            .filter(|t| t.rig().parts.iter().all(Option::is_some))
            .count();
        gpu.pose_turrets(&w.sim, &w.build.seats);
        let section = &w.build.seats[ci][seat];
        for (who, h) in [("the player's", turrets[0]), ("the bot's", turrets[1])] {
            let v = w.sim.vehicle(h).unwrap();
            let g = v.guns[seat].as_ref().unwrap();
            let turned = deg(g.look.0, rest.look.0);
            assert!(
                turned > 20.0,
                "{map} {who}: the look turned only {turned:.1} deg"
            );
            let (ty, ay, ap) = drawn(&gpu, &w.scene, section, v.pos);
            println!(
                "{map} {class} {who}: look ({:.1}, {:.1}) aim ({:.1}, {:.1}) deg; drawn turret \
                 node yaw {:.1}, aimer node ({:.1}, {:.1})",
                g.look.0.to_degrees(),
                g.look.1.to_degrees(),
                g.aim.0.to_degrees(),
                g.aim.1.to_degrees(),
                ty.to_degrees(),
                ay.to_degrees(),
                ap.to_degrees()
            );
            assert!(deg(ty, g.look.0) < 1.0, "{map} {who}: turret node yaw");
            assert!(deg(ay, g.aim.0) < 1.0, "{map} {who}: aimer node yaw");
            assert!(deg(ap, g.aim.1) < 1.0, "{map} {who}: aimer node pitch");
        }
        println!(
            "{map}: {all} armed buildings drawn with a rig, {whole} with every part on a node"
        );
        ran += 1;
    }
    println!("{ran} of {} cases ran", cases.len());
}

/// Card VTURL, data-gated (the bake): the player seated in a turret of each
/// class the PS2 rows hold, a stick held full for 2.5 s per axis. The look's
/// steady rate and its pitch limits against the PS2's own (the turret's
/// rate field and pitch per update in `lead_tmp/vturd/ps2/*_rows.bin`,
/// `lead_tmp/vturl/rates.py`): rates within 10 %, limits within 0.1 deg.
#[test]
fn the_look_turns_at_the_ps2s_rates_inside_its_limits() {
    // (map, factions, class, PS2 yaw rate, pitch rate (rad/s), the most
    // nose-UP and nose-DOWN pitch (deg) the rows reached).
    let cases = [
        (
            "bf1_nab1",
            ["rep", "cis"],
            "com_weap_gunturret",
            0.85,
            0.75,
            45.0,
            45.0,
        ),
        (
            "bf2_myg1",
            ["rep", "cis"],
            "tur_bldg_recoilless_lg",
            1.5,
            1.0,
            30.0,
            20.0,
        ),
        (
            "bf2_hot1",
            ["all", "imp"],
            "tur_bldg_hoth_dishturret",
            1.0,
            1.0,
            10.0,
            35.0,
        ),
    ];
    let mut ran = 0;
    for (map, factions, class, yaw_rate, pitch_rate, up, down) in cases {
        let Some(mut w) = world(map, factions) else {
            eprintln!("SKIP {map}: not baked");
            continue;
        };
        let ci = w
            .build
            .classes
            .iter()
            .position(|c| c.eq_ignore_ascii_case(class))
            .unwrap_or_else(|| panic!("{map}: no {class}"));
        let h = w
            .sim
            .alive_vehicles()
            .find(|(_, v)| v.class as usize == ci)
            .map(|(h, _)| h)
            .expect("a turret");
        let seat = w.sim.seat_setup[ci]
            .as_ref()
            .and_then(|s| s.guns.iter().position(Option::is_some))
            .expect("a gunner seat with a gun");
        let at = w.sim.vehicle(h).unwrap().pos;
        let player = w
            .sim
            .spawn(&core::unit::UnitSpec::player(0, at + Vec3::Z * 3.0))
            .expect("a player");
        assert!(w.sim.seat_unit(player, h, seat as u8));
        let gun = |w: &World| w.sim.vehicle(h).unwrap().guns[seat].clone().unwrap();
        // The most |rate| a held stick reaches (the PS2's rows: the rate
        // field's), and the pitch it is held at.
        let hold = |w: &mut World, rx: f32, ry: f32| {
            let mut most = (0.0f32, 0.0f32);
            for _ in 0..75 {
                w.sim.step(&[core::sim::PadState {
                    rx: core::sim::PadState::quantize(rx),
                    ry: core::sim::PadState::quantize(ry),
                    ..Default::default()
                }]);
                let g = gun(w);
                most = (most.0.max(g.rate.0.abs()), most.1.max(g.rate.1.abs()));
            }
            (most, gun(w).look.1.to_degrees())
        };
        let (yaw, _) = hold(&mut w, 1.0, 0.0);
        let (_, down_at) = hold(&mut w, 0.0, -1.0);
        let (pitch, up_at) = hold(&mut w, 0.0, 1.0);
        let got = (yaw.0, pitch.1, down_at, up_at);
        println!(
            "{map} {class}: yaw rate {:.3} rad/s (PS2 {yaw_rate}), pitch rate {:.3} (PS2 \
             {pitch_rate}), pitch {:.2} .. {:.2} deg (PS2 -{down} .. {up})",
            got.0, got.1, got.2, got.3
        );
        assert!((got.0 / yaw_rate - 1.0).abs() < 0.1, "{map}: yaw rate");
        assert!((got.1 / pitch_rate - 1.0).abs() < 0.1, "{map}: pitch rate");
        assert!((got.2 + down).abs() < 0.1, "{map}: nose down");
        assert!((got.3 - up).abs() < 0.1, "{map}: nose up");
        ran += 1;
    }
    println!("{ran} of {} cases ran", cases.len());
}
