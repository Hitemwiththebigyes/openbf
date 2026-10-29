//! Every building turret on BF1 Yavin 4: Arena can be entered (playtest
//! 2026-10-28: "one of the turrets ... it wouldn't let me get in it at all",
//! from any distance or angle). The map's six Minigun Turrets
//! (`com_weap_gunturret`) are neutral; a player of either side walks up to
//! each from every level side and enters on the use press. The originals
//! refuse only a turret of the other side (manned by its gunner) or a held
//! seat - the port lets a player take a seat from a bot of his own side.
//!
//! The playtest's turret had been DESTROYED minutes before (black box
//! `prev3.json`, seed 54, build 86ee8b4e: `vehicle_lost` of the turret at
//! (-31.29, -52.9, 14.97) at tick 2605; the player stood 4.4 m from it at
//! ticks 9390-9480, facing it). Since building turrets take damage
//! (874e9de1) a fallen one was removed for good while its building still
//! stood as before. The PS2 builds a fallen armed building again at full
//! health after its `RespawnTime` (`analysis/re-build-zones.md` 0.3); the
//! port builds it only with a fusion cutter (the maintainer 2026-10-28, card TBLD,
//! `core::sim::BuildingTurret`).

use core::sim::{buttons, PadState, Sim};
use core::unit::{Controller, UnitHandle};
use core::vehicle::{VehicleHandle, VehicleKind};
use glam::Vec3;

const MAP: &str = "bf1_yav2";
const CLASSES: [&str; 2] = ["imp_inf_storm_trooper", "all_inf_soldier"];

struct World {
    sim: Sim,
    db: game::classdb::ClassDb,
    names: Vec<(String, usize)>,
    /// One player per side, team 0 and team 1 (slots 0 and 1).
    me: [UnitHandle; 2],
}

fn world() -> Option<World> {
    let root = game::assets::baked_root();
    let dir = root.join(MAP);
    if !dir.join("manifest.ron").exists() {
        eprintln!("SKIP: {MAP} is not baked");
        return None;
    }
    let mut scene = game::assets::load_scene(&dir).expect("load scene");
    scene.apply_mode_layer("con");
    scene.merge_common(
        game::assets::load_common(&root.join(format!("common_{}", scene.manifest.source_game)))
            .expect("common assets"),
    );
    let mut sim = Sim::empty(42);
    sim.ground_rules = core::soldier::GroundRules::for_game(&scene.manifest.source_game);
    sim.heightfield = scene.heightfield();
    scene.install_collision(&mut sim);
    let factions = ["imp", "all"];
    let f = factions.map(str::to_owned);
    game::vehicle_pads::install(&mut sim, &scene.manifest, &dir, &root.join("classes"), &f)
        .expect("install");
    let db = game::roster::load_classdb(&root.join("classes"), "bf1", &factions).unwrap();
    for t in &mut sim.battle.teams {
        t.reinforcements = 200.0;
    }
    sim.cheats.invincible = true;
    let mut w = World {
        sim,
        db,
        names: Vec::new(),
        me: [UnitHandle { index: 0, gen: 0 }; 2],
    };
    for team in 0..2u8 {
        w.me[usize::from(team)] = spawn(
            &mut w,
            CLASSES[usize::from(team)],
            team,
            away(usize::from(team)),
            Controller::Player { slot: team },
        );
    }
    Some(w)
}

fn spawn(w: &mut World, class: &str, team: u8, pos: Vec3, controller: Controller) -> UnitHandle {
    let roster = game::roster::ClassRoster::build(&w.db, class, 0, None).unwrap();
    let c = roster.register(&mut w.sim, &mut w.names);
    w.sim
        .spawn(&c.spec(team, pos, 0.0, controller, 0, 0))
        .unwrap()
}

/// Where the player not under test waits, out of the way.
fn away(team: usize) -> Vec3 {
    Vec3::new(0.0, -53.7, -20.0 + 4.0 * team as f32)
}

fn idle() -> [PadState; 2] {
    [PadState::default(), PadState::default()]
}

fn turrets(w: &World) -> Vec<VehicleHandle> {
    w.sim
        .alive_vehicles()
        .filter(|(_, v)| v.kind == VehicleKind::Turret)
        .map(|(h, _)| h)
        .collect()
}

/// Turn player `team` to look at `at`.
fn face(w: &mut World, team: usize, at: Vec3) {
    let u = w.sim.unit(w.me[team]).unwrap();
    let d = at - u.muzzle();
    let s = w.sim.unit_mut(w.me[team]).unwrap();
    s.soldier.yaw = (-d.x).atan2(-d.z);
    s.soldier.pitch = (d.y / d.length()).asin();
}

/// Press and release use; true when player `team` sits in `vh` after it.
fn press_use(w: &mut World, team: usize, vh: VehicleHandle) -> bool {
    let mut pads = idle();
    pads[team].buttons = buttons::ACTION;
    w.sim.step(&pads);
    let inside = w
        .sim
        .unit(w.me[team])
        .unwrap()
        .mount
        .is_some_and(|m| m.vehicle == vh);
    w.sim.step(&idle());
    inside
}

/// Player `team` starts 8 m out from `pos` at angle `k` of 16 (on its
/// level; `None` where that is a drop), walks at it for 2.5 s facing it, and
/// presses use: did he get into `vh`? He is put back on foot after.
fn walk_in(w: &mut World, vh: VehicleHandle, pos: Vec3, team: usize, k: usize) -> Option<bool> {
    let a = k as f32 * std::f32::consts::TAU / 16.0;
    let mut start = pos + Vec3::new(a.cos(), 0.0, a.sin()) * 8.0;
    start.y = w.sim.spawn_ground(start);
    if (start.y - pos.y).abs() > 1.5 {
        return None;
    }
    for t in 0..2 {
        let s = w.sim.unit_mut(w.me[t]).unwrap();
        s.soldier.pos = if t == team { start } else { away(t) };
        s.soldier.vel = Vec3::ZERO;
    }
    for _ in 0..6 {
        w.sim.step(&idle());
    }
    let mut walk = idle();
    walk[team].ly = 32767;
    for _ in 0..75 {
        face(w, team, pos);
        w.sim.step(&walk);
    }
    for _ in 0..3 {
        w.sim.step(&idle());
    }
    face(w, team, pos);
    let inside = press_use(w, team, vh);
    if inside {
        press_use(w, team, vh);
        assert!(w.sim.unit(w.me[team]).unwrap().mount.is_none());
    }
    Some(inside)
}

#[test]
fn every_yav2_turret_is_entered_by_either_side_from_its_door() {
    let Some(mut w) = world() else {
        return;
    };
    let all = turrets(&w);
    assert_eq!(all.len(), 6, "{MAP}: six Minigun Turrets");
    for (i, &vh) in all.iter().enumerate() {
        let v = w.sim.vehicle(vh).unwrap();
        let pos = v.pos;
        assert_eq!((v.team, v.home_team), (None, None), "turret {i}: neutral");
        for team in 0..2 {
            let tries: Vec<Option<bool>> =
                (0..16).map(|k| walk_in(&mut w, vh, pos, team, k)).collect();
            let level = tries.iter().flatten().count();
            let entered = tries.iter().flatten().filter(|&&x| x).count();
            eprintln!("{MAP} turret {i} team {team}: {entered} of {level} level approaches");
            // Every turret has a walkable side, and the walk from it ends at
            // the gun, where the press boards it (four of the six have a
            // neighbour that stops two of the sixteen walks short).
            assert!(level >= 8, "turret {i}: {level} level approaches");
            assert!(
                entered + 2 >= level,
                "turret {i} team {team}: {entered} of {level}: {tries:?}"
            );
        }
    }
}

#[test]
fn a_destroyed_yav2_turret_stays_down_until_a_cutter_builds_it() {
    let Some(mut w) = world() else {
        return;
    };
    let vh = turrets(&w)[0];
    let pos = w.sim.vehicle(vh).unwrap().pos;
    // An imperial bot mans it: a rebel player is refused (the turret is the
    // Empire's while he sits in it), an imperial one takes his seat.
    let bot = spawn(&mut w, CLASSES[0], 0, pos, Controller::Ai);
    assert!(w.sim.seat_unit(bot, vh, 0));
    assert_eq!(w.sim.vehicle(vh).unwrap().team, Some(0));
    assert_eq!(w.sim.enter_seat(w.me[1], vh), None, "an enemy's turret");
    assert_eq!(w.sim.enter_seat(w.me[0], vh), Some(0), "a team-mate's seat");
    assert!(w.sim.enter_vehicle(w.me[0], vh, 0));
    assert!(w.sim.unit(bot).unwrap().mount.is_none());
    w.sim.exit_vehicle(w.me[0]);
    // Destroyed: nothing to enter, and nothing comes back by itself (the maintainer
    // 2026-10-28: only a fusion cutter fixes it; the PS2 regenerates it over
    // its RespawnTime). The bot (a stormtrooper) carries no cutter.
    w.sim.destroy_vehicle(vh, None);
    assert!(w.sim.vehicle(vh).is_none());
    for team in 0..2 {
        assert_eq!(walk_in(&mut w, vh, pos, team, 8), Some(false));
    }
    let fell = w.sim.tick;
    while w.sim.tick < fell + 300 * 30 {
        w.sim.step(&idle());
    }
    assert!(w
        .sim
        .alive_vehicles()
        .all(|(_, v)| v.kind != VehicleKind::Turret || v.pos != pos));
    assert!(w.sim.building_down(w.sim.building_turrets[0].instance));
    // A cutter's last shot builds it: neutral, empty, full, on its building.
    let max = w.sim.building_turrets[0].spec.max_health;
    w.sim.repair_building(0, max);
    let (rebuilt, v) = w
        .sim
        .alive_vehicles()
        .find(|(_, v)| v.kind == VehicleKind::Turret && v.pos == pos)
        .expect("built");
    assert_eq!(v.health.cur, max);
    assert_eq!((v.team, v.seats[0].occupant), (None, None));
    let inst = w.sim.building_turrets[0].instance;
    assert_eq!(v.instance, Some(inst), "on its building");
    assert_eq!(w.sim.collision.turret(inst), Some(rebuilt));
    for team in 0..2 {
        assert_eq!(
            walk_in(&mut w, rebuilt, pos, team, 8),
            Some(true),
            "team {team}"
        );
    }
}
