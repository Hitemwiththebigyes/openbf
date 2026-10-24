//! A building turret entered, fired and left on a real map (card VTURB,
//! headless): `cargo run -p game --release --example turret_ride -- [map]
//! [turret#] [faction1 faction2]` (defaults `bf1_nab1 0 rep cis`).
//!
//! The map's vehicles come from `vehicle_pads::install`, as a battle builds
//! them: its pads and every armed-building instance as a `Turret`. A player
//! starts 10 m in front of turret `#` (manifest order) — or behind, left or
//! right, the first with ground near the turret's base — walks toward it
//! until the use input would take it (`Sim::enter_target`), presses ACTION,
//! sweeps the look right while holding FIRE for 1 s, presses ACTION again
//! and stands for 1 s. The pads are recorded and replayed into a fresh sim:
//! the two hashes must agree. Printed: the turret census, the walk, the
//! seat, the shots (the first one's origin in the model frame beside the
//! baked fire point), the turret's position before and after (it never
//! moves), and where the soldier was placed.

use core::sim::{buttons, PadState, Sim};
use core::unit::{UnitHandle, UnitSpec};
use core::vehicle::{VehicleHandle, VehicleKind};
use glam::Vec3;
use std::path::{Path, PathBuf};

#[path = "support/replay_check.rs"]
mod replay_check;

const DT: f32 = 1.0 / 30.0;

fn world(
    dir: &Path,
    pick: usize,
    factions: &[String; 2],
    print: bool,
) -> anyhow::Result<(Sim, UnitHandle, VehicleHandle)> {
    let (scene, load) = game::assets::load_battle_scene(dir, "con")?;
    if let Some((_, common)) = load.common {
        common?;
    }
    let mut sim = Sim::empty(1);
    sim.heightfield = scene.heightfield();
    scene.install_collision(&mut sim);
    let classes = game::assets::baked_root().join("classes");
    let b = game::vehicle_pads::install(&mut sim, &scene.manifest, dir, &classes, factions)?;
    let turrets: Vec<VehicleHandle> = sim
        .alive_vehicles()
        .filter(|(_, v)| v.kind == VehicleKind::Turret)
        .map(|(h, _)| h)
        .collect();
    if print {
        println!(
            "{}: {} pads, {} turrets ({} classes in the table)",
            dir.display(),
            b.pads.len(),
            turrets.len(),
            b.classes.len()
        );
        for (i, t) in b.turrets.iter().enumerate() {
            let c = &b.classes[t.spec.class as usize];
            let setup = sim.seat_setup[t.spec.class as usize].as_ref();
            let guns = setup.map_or(0, |s| s.guns.iter().flatten().count());
            println!(
                "  #{i} {c} at ({:.1}, {:.1}, {:.1}) yaw {:.1}°, seats {:?}, guns {guns}, hull {} {:?}",
                t.spec.pos.x,
                t.spec.pos.y,
                t.spec.pos.z,
                t.spec.yaw.to_degrees(),
                t.spec.seats,
                t.spec.max_health,
                t.spec.health_type,
            );
        }
    }
    let v = *turrets
        .get(pick)
        .ok_or_else(|| anyhow::anyhow!("no turret #{pick}"))?;
    let veh = sim.vehicle(v).expect("alive");
    let f = veh.rot * Vec3::Z;
    let fwd = Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::Z);
    // 10 m in front, else behind, left or right: the first whose ground is
    // within 3 m of the turret's base (BES1's AAA guns face off the edge).
    let right = Vec3::new(fwd.z, 0.0, -fwd.x);
    let start = [fwd, -fwd, -right, right]
        .iter()
        .map(|d| {
            let s = veh.pos + *d * 10.0;
            Vec3::new(s.x, sim.ground_under(s + Vec3::Y * 5.0, false), s.z)
        })
        .find(|s| (s.y - veh.pos.y).abs() < 3.0)
        .unwrap_or(veh.pos + fwd * 10.0);
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
    let map = args.first().map_or("bf1_nab1", String::as_str);
    let pick: usize = args.get(1).map_or(Ok(0), |s| s.parse())?;
    let factions: [String; 2] = [
        args.get(2).cloned().unwrap_or_else(|| "rep".into()),
        args.get(3).cloned().unwrap_or_else(|| "cis".into()),
    ];
    let dir: PathBuf = game::assets::baked_root().join(map);

    let (mut sim, p, v) = world(&dir, pick, &factions, true)?;
    let at = sim.vehicle(v).expect("alive").pos;
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
        let s = &sim.unit(p).expect("alive").soldier;
        if ticks % 60 == 0 || ticks >= 600 {
            let d = s.pos - at;
            println!(
                "  t {:.1} s: soldier ({:.1}, {:.1}, {:.1}), {:.2} m from the origin (x-z), dy {:.2}",
                ticks as f32 * DT,
                s.pos.x,
                s.pos.y,
                s.pos.z,
                Vec3::new(d.x, 0.0, d.z).length(),
                d.y
            );
        }
        anyhow::ensure!(ticks < 600, "never in reach of turret #{pick}");
        let d = at - s.pos;
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

    // 2. Sweep the look right and fire for 1 s.
    let (mut shots, mut first) = (0, None);
    let mut ends: Vec<(f32, bool)> = Vec::new();
    for i in 0..90 {
        sim.events.clear();
        step(
            &mut sim,
            PadState {
                buttons: if i < 30 { buttons::FIRE } else { 0 },
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
        ends.extend(sim.events.iter().filter_map(|e| match e {
            core::sim::Event::BoltExpired { pos, struck } => Some((pos.distance(at), *struck)),
            _ => None,
        }));
        if first.is_none() {
            let veh = sim.vehicle(v).expect("alive");
            first = fired
                .first()
                .map(|&(o, d)| (veh.rot.inverse() * (o - veh.pos), veh.rot.inverse() * d));
        }
    }
    let veh = sim.vehicle(v).expect("alive");
    let seat = usize::from(sim.unit(p).expect("alive").mount.expect("seated").seat);
    let setup = sim.seat_setup[veh.class as usize].as_ref().expect("setup");
    let fp = setup
        .guns
        .get(seat)
        .and_then(|g| g.as_ref())
        .map(|g| g.fire_point);
    let g = veh.guns.get(seat).and_then(|g| g.as_ref());
    match (first, g) {
        (Some((lo, ld)), Some(g)) => println!(
            "{} shot(s) in 1 s; the first from model ({:.2}, {:.2}, {:.2}) along ({:.2}, {:.2}, {:.2}), fire point {fp:?}; look ({:.1}°, {:.1}°) aim ({:.1}°, {:.1}°)",
            shots, lo.x, lo.y, lo.z, ld.x, ld.y, ld.z,
            g.look.0.to_degrees(), g.look.1.to_degrees(),
            g.aim.0.to_degrees(), g.aim.1.to_degrees(),
        ),
        _ => println!("{shots} shot(s); no gun state"),
    }
    anyhow::ensure!(shots > 0, "FIRE fired nothing");
    let ends: Vec<String> = ends
        .iter()
        .map(|(d, s)| format!("{d:.1} m{}", if *s { " struck" } else { "" }))
        .collect();
    println!(
        "the bolts ended (from the turret's origin): {}",
        ends.join(", ")
    );
    let moved = veh.pos.distance(at);
    println!("the turret moved {moved:.4} m");
    anyhow::ensure!(moved == 0.0, "a turret never moves");

    // Why the exit's ring passes or fails: each candidate's path from the
    // centre against the SOLDIER-layer bodies, and the terrain under it.
    if std::env::var("TURRET_RIDE_WHY").is_ok() {
        let veh = sim.vehicle(v).expect("alive");
        let r = setup.radius + core::sim::EXIT_RING;
        let f = veh.rot * Vec3::Z;
        let fwd = Vec3::new(f.x, 0.0, f.z).normalize();
        let right = Vec3::new(fwd.z, 0.0, -fwd.x);
        let hf = sim.heightfield.as_ref().map(|h| h.height_at(at.x, at.z));
        println!(
            "exit ring rho {r:.2}; terrain at the centre {hf:?}, centre y {:.2}",
            at.y
        );
        for (x, z) in [(0.0f32, -1.0f32), (-1.0, 0.0), (1.0, 0.0), (0.0, 1.0)] {
            let c = at + right * (x * r) + fwd * (z * r) + Vec3::Y;
            let d = c - at;
            let hit = sim.collision.ray(
                at,
                d.normalize(),
                d.length(),
                core::collision::Layers::SOLDIER,
            );
            println!(
                "  ({x:+.0}, {z:+.0}): body hit at {:?} of {:.2} m",
                hit.map(|h| h.0),
                d.length()
            );
        }
    }

    // 3. Exit, then stand for 1 s.
    step(&mut sim, PadState::default());
    step(&mut sim, press);
    let u = sim.unit(p).expect("alive");
    anyhow::ensure!(u.mount.is_none(), "ACTION did not exit");
    let placed = u.soldier.pos;
    let off = placed - at;
    let ground = sim.ground_under(placed + Vec3::Y * 0.5, false);
    println!(
        "exit: placed ({:.2}, {:.2}, {:.2}), {:.2} m from the turret's origin (x-z), {:.3} m over the ground probe",
        placed.x,
        placed.y,
        placed.z,
        Vec3::new(off.x, 0.0, off.z).length(),
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
    // Walk out: 2 s of full stick, in each of the four directions.
    for (name, lx, ly) in [
        ("forward", 0, i16::MAX),
        ("back", 0, -i16::MAX),
        ("left", -i16::MAX, 0),
        ("right", i16::MAX, 0),
    ] {
        let from = sim.unit(p).expect("alive").soldier.pos;
        for _ in 0..60 {
            step(
                &mut sim,
                PadState {
                    lx,
                    ly,
                    ..PadState::default()
                },
            );
        }
        let to = sim.unit(p).expect("alive").soldier.pos;
        println!(
            "walk {name} 2 s: {:.2} m, now {:.2} m from the origin (x-z)",
            to.distance(from),
            Vec3::new(to.x - at.x, 0.0, to.z - at.z).length()
        );
    }
    let hash = sim.hash();

    // 4. Replay the recorded pads into a fresh world.
    let (mut again, _, _) = world(&dir, pick, &factions, false)?;
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
