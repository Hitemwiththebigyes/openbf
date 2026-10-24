//! Enter, drive, exit on a real map (card VSEAT, headless):
//! `cargo run -p game --release --example vehicle_ride -- [map] [class] [yaw°]`
//! (defaults `tat1 tat_hover_skiff`, yaw 0).
//!
//! The vehicle stands at the first pad naming the class (as `hover_drive`
//! puts it) with the class's hover numbers and `vehicle_pads::seat_setup`;
//! a player starts 12 m off its left rear. A scripted pad turns the soldier
//! toward the vehicle and walks until the use input would take it
//! (`Sim::enter_target`), presses ACTION, drives at full throttle — full
//! right stick while it is stuck — until the hover has covered 200 m, presses
//! ACTION again and stands for 1 s. The pads are recorded and replayed into
//! a fresh sim: the two hashes must agree. Printed: the walk, the seat, the
//! drive, where the soldier was placed (which of §5's 9 candidates, its
//! height over the ground probe).

use core::sim::{buttons, PadState, Sim};
use core::unit::{UnitHandle, UnitSpec};
use core::vehicle::{VehicleHandle, VehicleKind, VehicleSpec};
use game::classdb::ClassDb;
use game::vehicle_class::HoverClass;
use glam::Vec3;
use std::path::{Path, PathBuf};

#[path = "support/replay_check.rs"]
mod replay_check;

const DT: f32 = 1.0 / 30.0;
const DRIVE_M: f32 = 200.0;

fn world(dir: &Path, class: &str, yaw: f32) -> anyhow::Result<(Sim, UnitHandle, VehicleHandle)> {
    let (scene, load) = game::assets::load_battle_scene(dir, "con")?;
    if let Some((_, common)) = load.common {
        common?;
    }
    let mut sim = Sim::empty(1);
    sim.heightfield = scene.heightfield();
    scene.install_collision(&mut sim);
    let mut db = ClassDb::default();
    db.load_map(dir)?;
    // The game's shared classes: the seat weapons (card VTUR).
    for f in game::roster::class_files(&scene.manifest.source_game, &[]) {
        let p = game::assets::baked_root().join("classes").join(f);
        if p.exists() {
            db.load(&p)?;
        }
    }
    let p = HoverClass::from_db(&db, class)
        .ok_or_else(|| anyhow::anyhow!("{class}: not a hover class"))?
        .params();
    let ring = game::vehicle_class::seats(&db, class);
    sim.hover_params = vec![Some(p)];
    let mut setup = game::vehicle_pads::seat_setup(&db, class, &ring, &scene.manifest.source_game);
    let nodes = game::vehicle_pads::hardpoints(&scene.manifest, dir, &scene.manifest.source_game);
    let hp = nodes.get(class).cloned().unwrap_or_default();
    setup.guns = ring
        .iter()
        .map(|s| game::vehicle_pads::seat_gun(&mut sim, &db, s, &hp))
        .collect();
    sim.seat_setup = vec![Some(setup)];
    let inst = scene
        .manifest
        .instances
        .iter()
        .filter(|i| i.class == game::vehicle_pads::PAD_CLASS)
        .find(|i| {
            i.props
                .iter()
                .any(|(_, v)| v.trim().eq_ignore_ascii_case(class))
        })
        .ok_or_else(|| anyhow::anyhow!("no pad for {class}"))?;
    let at = Vec3::from(inst.translation);
    let g = sim.ground_under(at, false);
    let v = sim.spawn_vehicle(&VehicleSpec {
        kind: VehicleKind::Hover,
        class: 0,
        home_team: None,
        neutral_when_empty: true,
        spawn_pad: None,
        pos: Vec3::new(at.x, g, at.z),
        yaw: yaw.to_radians(),
        max_health: db.prop_f32(class, "MaxHealth").unwrap_or(1000.0),
        health_type: game::vehicle_class::health_type(&db, class),
        seats: ring.iter().map(|s| s.kind).collect(),
    });
    let rot = sim.vehicle(v).expect("alive").rot;
    let start = at + rot * Vec3::new(-8.0, 0.0, -9.0);
    let start = Vec3::new(
        start.x,
        sim.ground_under(start + Vec3::Y * 5.0, false),
        start.z,
    );
    let p = sim
        .spawn(&UnitSpec::player(0, start))
        .ok_or_else(|| anyhow::anyhow!("no player"))?;
    Ok((sim, p, v))
}

fn wrap(a: f32) -> f32 {
    let t = std::f32::consts::TAU;
    (a + std::f32::consts::PI).rem_euclid(t) - std::f32::consts::PI
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let map = args.first().map_or("tat1", String::as_str);
    let class = args.get(1).map_or("tat_hover_skiff", String::as_str);
    let yaw: f32 = args.get(2).map_or(Ok(0.0), |y| y.parse())?;
    let dir: PathBuf = game::assets::baked_root().join(map);

    let (mut sim, p, v) = world(&dir, class, yaw)?;
    for _ in 0..60 {
        sim.step(&[]); // the hover settles on its springs
    }
    let mut pads: Vec<PadState> = Vec::new();
    let mut step = |sim: &mut Sim, pad: PadState| {
        pads.push(pad);
        sim.step(&[pad]);
    };
    let q = PadState::quantize;

    // 1. Walk to it.
    let walk_from = sim.unit(p).expect("alive").soldier.pos;
    let mut ticks = 0;
    while sim.enter_target(p).is_none() {
        anyhow::ensure!(ticks < 600, "never in reach of the {class}");
        let s = &sim.unit(p).expect("alive").soldier;
        let d = sim.vehicle(v).expect("alive").pos - s.pos;
        let err = wrap(core::fmath::atan2(-d.x, -d.z) - s.yaw);
        let pad = PadState {
            rx: q(-3.0 * err),
            ly: if err.abs() < 0.5 { i16::MAX } else { 0 },
            ..PadState::default()
        };
        step(&mut sim, pad);
        ticks += 1;
    }
    let walked = sim.unit(p).expect("alive").soldier.pos.distance(walk_from);
    let press = PadState {
        buttons: buttons::ACTION,
        ..PadState::default()
    };
    step(&mut sim, press);
    let seat = sim.seat_of(p);
    println!(
        "walked {walked:.1} m in {:.2} s; ACTION -> {seat:?}",
        ticks as f32 * DT
    );
    anyhow::ensure!(seat.is_some(), "ACTION did not seat the player");

    // 2. Drive 200 m.
    let mut last = sim.vehicle(v).expect("alive").pos;
    let (mut path, mut t, mut stuck) = (0.0f32, 0u32, 0u32);
    while path < DRIVE_M {
        anyhow::ensure!(t < 60 * 30, "drove only {path:.1} m in 60 s");
        let speed = sim.vehicle(v).expect("alive").vel.length();
        stuck = if t > 90 && speed < 3.0 {
            45
        } else {
            stuck.saturating_sub(1)
        };
        let pad = PadState {
            ly: i16::MAX,
            rx: if stuck > 0 { i16::MAX } else { 0 },
            ..PadState::default()
        };
        step(&mut sim, pad);
        let now = sim.vehicle(v).expect("alive").pos;
        path += Vec3::new(now.x - last.x, 0.0, now.z - last.z).length();
        last = now;
        t += 1;
        anyhow::ensure!(
            sim.unit(p).expect("alive").soldier.pos == now,
            "the rider left its seat"
        );
    }
    let veh = sim.vehicle(v).expect("alive");
    println!(
        "drove {path:.1} m in {:.2} s, now at ({:.1}, {:.1}, {:.1}) at {:.1} m/s",
        t as f32 * DT,
        veh.pos.x,
        veh.pos.y,
        veh.pos.z,
        veh.vel.length()
    );

    // 2b. Card VTUR: switch through the gunner seats; in each, sweep the
    //     look right and fire for 1 s.
    let switch = PadState {
        buttons: buttons::SWITCH_WEAPON,
        ..PadState::default()
    };
    let n = sim.vehicle(v).expect("alive").seats.len();
    for _ in 1..n {
        step(&mut sim, PadState::default());
        step(&mut sim, switch);
        let Some((_, kind)) = sim.seat_of(p) else {
            anyhow::bail!("the switch unseated the player");
        };
        let mut shots = 0;
        let mut first = None;
        for i in 0..30 {
            sim.events.clear();
            step(
                &mut sim,
                PadState {
                    buttons: buttons::FIRE,
                    rx: if i < 15 { q(0.5) } else { 0 },
                    ..PadState::default()
                },
            );
            let fired: Vec<(Vec3, Vec3)> = sim
                .events
                .iter()
                .filter_map(|e| match e {
                    core::sim::Event::Shot { origin, dir, .. } => Some((*origin, *dir)),
                    _ => None,
                })
                .collect();
            shots += fired.len();
            if first.is_none() {
                // In the model frame of this same tick's transform.
                let veh = sim.vehicle(v).expect("alive");
                first = fired
                    .first()
                    .map(|&(o, d)| (veh.rot.inverse() * (o - veh.pos), veh.rot.inverse() * d));
            }
        }
        let veh = sim.vehicle(v).expect("alive");
        let seat = usize::from(sim.unit(p).expect("alive").mount.expect("seated").seat);
        let g = veh.guns.get(seat).and_then(|g| g.as_ref());
        match (first, g) {
            (Some((lo, ld)), Some(g)) => {
                println!(
                    "{kind:?}: {} shot(s) in 1 s; the first from model ({:.2}, {:.2}, {:.2}) along ({:.2}, {:.2}, {:.2}); look ({:.1}°, {:.1}°) aim ({:.1}°, {:.1}°)",
                    shots,
                    lo.x, lo.y, lo.z, ld.x, ld.y, ld.z,
                    g.look.0.to_degrees(), g.look.1.to_degrees(),
                    g.aim.0.to_degrees(), g.aim.1.to_degrees(),
                );
            }
            _ => println!("{kind:?}: no gun"),
        }
    }
    let veh = sim.vehicle(v).expect("alive");

    // 3. Exit, then stand for 1 s.
    let (vpos, vrot) = (veh.pos, veh.rot);
    step(&mut sim, PadState::default());
    step(&mut sim, press);
    let u = sim.unit(p).expect("alive");
    anyhow::ensure!(u.mount.is_none(), "ACTION did not exit");
    let placed = u.soldier.pos;
    let f = vrot * Vec3::Z;
    let fwd = Vec3::new(f.x, 0.0, f.z).normalize();
    let right = Vec3::new(fwd.z, 0.0, -fwd.x);
    let off = placed - vpos;
    let (x, z) = (off.dot(right), off.dot(fwd));
    let names = [
        "behind",
        "left",
        "right",
        "back-left",
        "back-right",
        "front-right",
        "front-left",
        "front",
        "centre",
    ];
    const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
    let cands = [
        (0.0, -1.0),
        (-1.0, 0.0),
        (1.0, 0.0),
        (-D, -D),
        (D, -D),
        (D, D),
        (-D, D),
        (0.0, 1.0),
        (0.0, 0.0),
    ];
    let rho = (x * x + z * z).sqrt();
    let k = cands
        .iter()
        .enumerate()
        .min_by(|a, b| {
            let da = (a.1 .0 * rho - x).powi(2) + (a.1 .1 * rho - z).powi(2);
            let db = (b.1 .0 * rho - x).powi(2) + (b.1 .1 * rho - z).powi(2);
            da.total_cmp(&db)
        })
        .map_or(8, |(i, _)| i);
    let ground = sim.ground_under(placed + Vec3::Y * 0.5, false);
    println!(
        "exit: placed ({:.2}, {:.2}, {:.2}), {rho:.2} m from the vehicle ({}), {:.3} m over the ground probe",
        placed.x,
        placed.y,
        placed.z,
        names[k],
        placed.y - ground
    );
    for _ in 0..30 {
        step(&mut sim, PadState::default());
    }
    let s = &sim.unit(p).expect("alive").soldier;
    println!(
        "1 s later: ({:.2}, {:.2}, {:.2}), grounded {}",
        s.pos.x, s.pos.y, s.pos.z, s.grounded
    );
    let hash = sim.hash();

    // 4. Replay the recorded pads into a fresh world.
    let (mut again, _, _) = world(&dir, class, yaw)?;
    for _ in 0..60 {
        again.step(&[]);
    }
    for pad in &pads {
        again.step(&[*pad]);
    }
    println!(
        "replay of {} pads: hash {hash:016x} / {:016x}: {}",
        pads.len(),
        again.hash(),
        if hash == again.hash() {
            "EQUAL"
        } else {
            "DIFFER"
        }
    );
    replay_check::check(hash, again.hash())?;
    Ok(())
}
