//! A building turret takes the ordnance's damage (playtest 2026-10-28, BF2
//! Kamino: "turrets don't lose their health", a rocket at a turret left its
//! gunner alive). `analysis/re-turret-collision.md` [O]: a round or blast
//! that meets the armed building damages the building only (3.1), through
//! the ordnance's scale for its HealthType (building, B1 FUN_001c08b0); its
//! blast measures the building past its own bodies (3.4); at 0 every turret
//! seat's gunner is killed with the building's last hit (4.1, 4.2). Before the
//! fix the building's bodies were plain map bodies: a round stopped on them
//! as on a wall (`Hit::World`) and the pool never fell.
//!
//! Each case is a real map's turret class with a bot of the other side in its
//! seat, shot from 15 m by a human of the real shooting class with the real
//! weapon: the turret's drop is the round's direct damage at its age plus its
//! blast's on the building's sphere, by the ordnance's building scales; then,
//! at 1 health, the next round destroys it and kills the gunner (a kill for
//! the shooter); the building's bodies are a plain wall after.

use core::collision::{HullFilter, Layers};
use core::projectile::{Hit, Target};
use core::sim::{buttons, Event, PadState, Sim};
use core::unit::{Controller, UnitHandle};
use core::vehicle::{VehicleHandle, VehicleKind};
use core::weapon::{blast_falloff, HealthType, EXPLOSION_OFFSET};
use glam::Vec3;

struct World {
    sim: Sim,
    classes: Vec<String>,
    db: game::classdb::ClassDb,
    names: Vec<(String, usize)>,
}

fn world(map: &str, factions: [&str; 2]) -> Option<World> {
    let root = game::assets::baked_root();
    let dir = root.join(map);
    if !dir.join("manifest.ron").exists() {
        eprintln!("SKIP: {map} is not baked");
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
    let f = [factions[0].to_owned(), factions[1].to_owned()];
    let build =
        game::vehicle_pads::install(&mut sim, &scene.manifest, &dir, &root.join("classes"), &f)
            .expect("install");
    let game = scene.manifest.source_game.to_ascii_lowercase();
    let db = game::roster::load_classdb(&root.join("classes"), &game, &factions).unwrap();
    // A battle under way: no side in its endgame (the gunner's exit rule 4).
    for t in &mut sim.battle.teams {
        t.reinforcements = 200.0;
    }
    Some(World {
        sim,
        classes: build.classes.clone(),
        db,
        names: Vec::new(),
    })
}

fn turrets(w: &World, class: &str) -> Vec<VehicleHandle> {
    w.sim
        .alive_vehicles()
        .filter(|(_, v)| {
            v.kind == VehicleKind::Turret && w.classes[v.class as usize].eq_ignore_ascii_case(class)
        })
        .map(|(h, _)| h)
        .collect()
}

fn spawn(w: &mut World, class: &str, team: u8, pos: Vec3, controller: Controller) -> UnitHandle {
    let roster = game::roster::ClassRoster::build(&w.db, class, 0, None).unwrap();
    let c = roster.register(&mut w.sim, &mut w.names);
    w.sim
        .spawn(&c.spec(team, pos, 0.0, controller, 0, 0))
        .unwrap()
}

/// A spot `dist` m from the turret on the ground near its level, with a clear
/// line from head height to `aim` past everything but the turret's own
/// building, and the point on that line where it first meets the building
/// (a wall, not a gap that shows the gunner).
fn firing_spot(w: &World, vh: VehicleHandle, aim: Vec3, dist: f32) -> Option<(Vec3, Vec3)> {
    let v = w.sim.vehicle(vh)?;
    let inst = v.instance;
    (0..32).find_map(|k| {
        let a = k as f32 * std::f32::consts::TAU / 32.0;
        let mut p = v.pos + Vec3::new(a.cos(), 0.0, a.sin()) * dist;
        p.y = w.sim.spawn_ground(p);
        if (p.y - v.pos.y).abs() > 3.0 {
            return None;
        }
        let from = p + Vec3::Y * 1.5;
        let d = aim - from;
        let clear = w
            .sim
            .collision
            .ray_past(
                from,
                d.normalize(),
                d.length(),
                Layers::ORDNANCE,
                HullFilter::All,
                [inst, None],
            )
            .is_none()
            && core::perception::los_terrain(&w.sim, from, aim);
        if !clear {
            return None;
        }
        let dir = d.normalize();
        let h = w.sim.collision.ray_with(
            from,
            dir,
            d.length() + 2.0,
            Layers::ORDNANCE,
            HullFilter::Static,
        )?;
        // Its near side: short of the turret's centre.
        (h.instance == inst && h.t < d.length() - 0.3).then(|| (p, from + dir * (h.t + 0.05)))
    })
}

/// The weapon in hand, through the ordinary switch input.
fn select(w: &mut World, u: UnitHandle, weapon: &str) {
    for _ in 0..12 {
        if w.sim.weapon_classes[w.sim.unit(u).unwrap().weapon_class].name == weapon {
            return;
        }
        w.sim.step(&[PadState {
            buttons: buttons::SWITCH_WEAPON,
            ..Default::default()
        }]);
        w.sim.step(&[PadState::default()]);
    }
    panic!("{weapon} not in hand");
}

/// Lay the human's aim so his round (from the fire point toward what the
/// reticle's ray meets, `Sim::aim_hit`) flies at `at`.
fn aim_at(w: &mut World, u: UnitHandle, at: Vec3) {
    let delta = at - w.sim.unit(u).unwrap().fire_point();
    let s = w.sim.unit_mut(u).unwrap();
    s.soldier.yaw = (-delta.x).atan2(-delta.z);
    s.soldier.pitch = (delta.y / delta.length()).asin();
    let yaw_of = |d: Vec3| (-d.x).atan2(-d.z);
    for _ in 0..12 {
        let fp = w.sim.unit(u).unwrap().fire_point();
        let shot = (w.sim.aim_hit(u.index as usize) - fp).normalize();
        let want = (at - fp).normalize();
        let s = w.sim.unit_mut(u).unwrap();
        s.soldier.pitch += want.y.asin() - shot.y.asin();
        let pi = std::f32::consts::PI;
        s.soldier.yaw += (yaw_of(want) - yaw_of(shot) + pi).rem_euclid(2.0 * pi) - pi;
    }
}

/// What one shot did: the turret's drop, the drop the ordnance's numbers
/// give, the gunner's health after, and the kill events.
struct Shot {
    drop: f32,
    expected: f32,
    direct: f32,
    hit_turret: bool,
    destroyed: bool,
    kills: Vec<(UnitHandle, UnitHandle)>,
}

/// Fire one round of the weapon in `me`'s hand at `at` and follow it for 3 s.
fn fire(w: &mut World, me: UnitHandle, vh: VehicleHandle, at: Vec3) -> Shot {
    // Settle the reload; a gunner's blast can knock the shooter down, and he
    // fires standing.
    for k in 0..600 {
        w.sim.step(&[PadState::default()]);
        if k >= 90 && !w.sim.unit(me).unwrap().soldier.no_fire() {
            break;
        }
    }
    aim_at(w, me, at);
    let before = w.sim.vehicle(vh).unwrap().health.cur;
    let v = w.sim.vehicle(vh).unwrap().clone();
    let (centre, radius) = w.sim.vehicle_blast_sphere(&v);
    let (mut age, mut hit, mut charge) = (0.0, Hit::None, 1.0);
    let mut impact = None;
    let mut wc = None;
    let mut shot = false;
    let mut out = Shot {
        drop: 0.0,
        expected: 0.0,
        direct: 0.0,
        hit_turret: false,
        destroyed: false,
        kills: Vec::new(),
    };
    for k in 0..90 {
        let pad = if !shot && k < 30 {
            PadState {
                buttons: buttons::FIRE | buttons::MOUSE_AIM,
                ..Default::default()
            }
        } else {
            PadState::default()
        };
        w.sim.step(&[pad]);
        if let Some(b) = w.sim.bolts.iter().find(|b| b.owner == me) {
            (age, hit, charge) = (b.age, b.hit, b.charge);
            wc = Some(b.weapon_class);
        }
        for e in &w.sim.events {
            match *e {
                Event::Impact {
                    pos,
                    dir,
                    weapon_class,
                    ..
                } if impact.is_none() => {
                    impact = Some((pos, dir));
                    wc = Some(weapon_class as usize);
                }
                Event::Kill { shooter, victim } => out.kills.push((shooter, victim)),
                Event::Shot { unit, .. } if unit == me => shot = true,
                Event::VehicleDestroyed { vehicle, .. } if vehicle == vh => out.destroyed = true,
                _ => {}
            }
        }
    }
    let after = w.sim.vehicle(vh).map_or(0.0, |v| v.health.cur.max(0.0));
    out.drop = before - after;
    out.hit_turret = hit == Hit::Target(Target::Vehicle(vh));
    let o = &w.sim.weapon_classes[wc.expect("a round")].ordnance;
    let b = HealthType::Building as usize;
    if out.hit_turret {
        out.direct = o.damage_desc_charged(age, false, charge).damage * o.scales[b];
        out.expected = out.direct;
        if let (Some(e), Some((pos, dir))) = (&o.explosion, impact) {
            let blast = pos - dir * EXPLOSION_OFFSET;
            let f = blast_falloff(
                (centre - blast).length(),
                radius,
                e.damage_inner,
                e.damage_outer,
            );
            out.expected += e.damage * e.scales[b] * f;
        }
        out.expected = out.expected.min(before);
    }
    out
}

/// One map's case: the turret class, the gunner's class (the turret's side),
/// the shooting class and its weapon; the drop the first round must make at
/// full health (`None`: only the formula).
struct Case<'a> {
    map: &'a str,
    factions: [&'a str; 2],
    turret: &'a str,
    gunner: &'a str,
    shooter: &'a str,
    weapon: &'a str,
    first: Option<f32>,
}

fn run(c: &Case) {
    let Some(mut w) = world(c.map, c.factions) else {
        return;
    };
    let all = turrets(&w, c.turret);
    assert!(!all.is_empty(), "{}: no {}", c.map, c.turret);
    // The first of the class with a clear 15 m line at its building's wall,
    // low, below a seated gunner.
    let low = |v: &core::vehicle::Vehicle| {
        let (sc, _) = v.aim_sphere.unwrap();
        v.pos + v.rot * Vec3::new(0.0, sc.y.min(0.7), 0.0)
    };
    let (vh, spot, wall) = all
        .iter()
        .find_map(|&vh| {
            let at = low(w.sim.vehicle(vh)?);
            firing_spot(&w, vh, at, 15.0).map(|(s, wl)| (vh, s, wl))
        })
        .unwrap_or_else(|| panic!("{}: no clear spot at any {}", c.map, c.turret));
    let v = w.sim.vehicle(vh).unwrap().clone();
    assert_eq!(v.health.health_type, HealthType::Building, "{}", c.turret);
    let inst = v.instance.expect("a building turret has its building");
    assert_eq!(w.sim.collision.turret(inst), Some(vh));
    let team = v.team.or(v.home_team).unwrap_or(0);
    let gunner = spawn(&mut w, c.gunner, team, v.pos, Controller::Ai);
    assert!(w.sim.seat_unit(gunner, vh, 0), "{}: seat", c.map);
    // Team-mates far off, so the gunner's side is not in its endgame.
    for k in 0..4 {
        spawn(
            &mut w,
            c.gunner,
            team,
            v.pos + Vec3::new(400.0 + 3.0 * k as f32, 300.0, 0.0),
            Controller::Ai,
        );
    }
    let me = spawn(
        &mut w,
        c.shooter,
        1 - team,
        spot,
        Controller::Player { slot: 0 },
    );
    w.sim.cheats.invincible = true;
    w.sim.cheats.infinite_ammo = true;
    w.sim.cheats.no_reload = true;
    select(&mut w, me, c.weapon);

    // 1. At full health: the drop is the ordnance's, by the building scales.
    let s = fire(&mut w, me, vh, wall);
    eprintln!(
        "{} {} <- {}: drop {} (direct {}, expected {}), destroyed {}",
        c.map, c.turret, c.weapon, s.drop, s.direct, s.expected, s.destroyed
    );
    assert!(
        s.hit_turret,
        "{}: the round struck the turret's building",
        c.map
    );
    assert!(s.drop > 0.0, "{}: the turret's health fell", c.map);
    assert!(
        (s.drop - s.expected).abs() < 1e-3,
        "{}: drop {} expected {}",
        c.map,
        s.drop,
        s.expected
    );
    if let Some(first) = c.first {
        assert!(
            (s.drop - first).abs() < 0.01,
            "{}: {} vs {first}",
            c.map,
            s.drop
        );
    }
    // 2. The round that empties it destroys it and kills its gunner.
    if !s.destroyed {
        assert_eq!(
            w.sim.unit(gunner).and_then(|g| g.mount).map(|m| m.vehicle),
            Some(vh),
            "{}: the gunner is still seated",
            c.map
        );
        // One health left of one: a bot gunner leaves a turret under 30 %
        // of its health at his next think (`ai::man`'s exit rule 3, ours),
        // so the pool is shrunk whole, not emptied.
        let h = &mut w.sim.vehicle_mut(vh).unwrap().health;
        (h.cur, h.max) = (1.0, 1.0);
        let s = fire(&mut w, me, vh, wall);
        assert!(s.destroyed, "{}: destroyed at 0", c.map);
        assert!(s.kills.contains(&(me, gunner)), "{}: {:?}", c.map, s.kills);
    } else {
        assert!(s.kills.contains(&(me, gunner)), "{}: {:?}", c.map, s.kills);
    }
    assert!(w.sim.vehicle(vh).is_none());
    assert!(
        w.sim.unit(gunner).is_none_or(|g| !g.alive),
        "{}: the gunner died",
        c.map
    );
    // The building is a plain wall again: a round stops on it, harmlessly.
    assert_eq!(w.sim.collision.turret(inst), None);
}

/// The playtest's turret: BF2 Kamino's Minigun Turret (`tur_weap_built_gunturret`,
/// 800 health) under the jet trooper's EMP launcher: 200 rising to 400 over
/// 0.5 s of flight (253.33 at 4 updates), building scale 1, plus its blast's
/// 400 at building scale 0.25 = 100: 353.33. The playtest's 31 EMP rounds
/// would have destroyed three such turrets.
#[test]
fn kamino_turret_emp_launcher() {
    run(&Case {
        map: "bf2_kam1",
        factions: ["cis", "rep"],
        turret: "tur_weap_built_gunturret",
        gunner: "cis_inf_rifleman",
        shooter: "rep_inf_ep3_jettrooper",
        weapon: "rep_weap_inf_emp_launcher",
        first: Some(253.333_34 + 100.0),
    });
}

/// The rocket launcher: 500 rising to 1500 (766.67 at 4 updates) and the same
/// 100 of blast - one rocket from 15 m destroys the turret and kills its
/// gunner (the playtest: "the bot in the turret didn't die").
#[test]
fn kamino_turret_rocket_launcher() {
    run(&Case {
        map: "bf2_kam1",
        factions: ["cis", "rep"],
        turret: "tur_weap_built_gunturret",
        gunner: "cis_inf_rifleman",
        shooter: "rep_inf_ep3_rocketeer",
        weapon: "rep_weap_inf_rocket_launcher",
        first: Some(800.0),
    });
}

/// The rifle: 80 at building scale 0.1 = 8 a bolt.
#[test]
fn kamino_turret_rifle() {
    run(&Case {
        map: "bf2_kam1",
        factions: ["cis", "rep"],
        turret: "tur_weap_built_gunturret",
        gunner: "cis_inf_rifleman",
        shooter: "rep_inf_ep3_rifleman",
        weapon: "rep_weap_inf_rifle",
        first: Some(8.0),
    });
}

/// Not Kamino only: BF2 Hoth's dish turret and BF1's gun turret (Kamino,
/// Naboo) and Geonosis cannon, each under a rocket launcher and a rifle.
#[test]
fn other_maps_building_turrets() {
    let cases = [
        (
            "bf2_hot1",
            ["all", "imp"],
            "tur_bldg_hoth_dishturret",
            "imp_inf_rifleman",
            "all_inf_rocketeer",
            "all_weap_inf_rocket_launcher",
        ),
        (
            "bf2_hot1",
            ["all", "imp"],
            "tur_bldg_hoth_dishturret",
            "imp_inf_rifleman",
            "all_inf_rifleman",
            "all_weap_inf_rifle",
        ),
        (
            "bf1_kam1",
            ["rep", "cis"],
            "com_weap_gunturret",
            "cis_inf_battledroid",
            "rep_inf_jet_trooper",
            "rep_weap_inf_rocket_launcher",
        ),
        (
            "bf1_nab1",
            ["rep", "cis"],
            "com_weap_gunturret",
            "cis_inf_battledroid",
            "rep_inf_clone_trooper",
            "rep_weap_inf_rifle",
        ),
        (
            "bf1_geo1",
            ["rep", "cis"],
            "geo_bldg_geoturret",
            "cis_inf_battledroid",
            "rep_inf_arc_trooper",
            "rep_weap_inf_torpedo_launcher",
        ),
    ];
    for (map, factions, turret, gunner, shooter, weapon) in cases {
        run(&Case {
            map,
            factions,
            turret,
            gunner,
            shooter,
            weapon,
            first: None,
        });
    }
}

/// A turret's own gunner's rounds still leave his building as before: a
/// human gunner turning and pitching his Kamino gun through its arc for 4 s,
/// firing, never strikes his own turret.
#[test]
fn a_gunners_own_rounds_spare_his_turret() {
    let Some(mut w) = world("bf2_kam1", ["cis", "rep"]) else {
        return;
    };
    let vh = turrets(&w, "tur_weap_built_gunturret")[0];
    let pos = w.sim.vehicle(vh).unwrap().pos;
    let p = spawn(
        &mut w,
        "cis_inf_rifleman",
        0,
        pos,
        Controller::Player { slot: 0 },
    );
    assert!(w.sim.seat_unit(p, vh, 0));
    let mut shots = 0;
    for k in 0..120 {
        let pad = PadState {
            buttons: if k % 2 == 0 { buttons::FIRE } else { 0 },
            rx: 20000,
            ry: if k < 60 { -15000 } else { 15000 },
            ..Default::default()
        };
        w.sim.step(&[pad]);
        shots += w
            .sim
            .events
            .iter()
            .filter(|e| matches!(e, Event::Shot { unit, .. } if *unit == p))
            .count();
        assert!(
            !w.sim.events.iter().any(|e| matches!(e,
                Event::VehicleHit { vehicle, .. } if *vehicle == vh)),
            "the gunner's round struck his own turret"
        );
    }
    assert!(shots > 0, "the gun fired");
    assert_eq!(w.sim.vehicle(vh).unwrap().health.cur, 800.0);
}
