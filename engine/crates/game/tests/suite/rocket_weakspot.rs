//! Hitmark (the maintainer 2026-10-28, `analysis/re-target-hud.md` §12): the human
//! player's rocket launcher on a vehicle's weak spot - a hull prim its class
//! authors a `HitLocation` x4 for (`analysis/re-hitzones.md` §3: the hits
//! BF2's demolition award counts) - raises `Event::CriticalHit` and lights
//! the reticle's critical-hit mark; the same rocket on the hull elsewhere,
//! a rifle on the weak spot, or a bot's rocket on it does not.
//!
//! BF2 Geonosis, both sides: each pad vehicle whose hull carries a x4 part
//! (the Republic's AT-TE and fighter tank, the CIS hailfire and spider
//! walker), shot from a spot with a clear line at the part by a real
//! rocketeer / rifleman of the other side with his real weapon's round. BF1
//! has no such zone (its only `HitLocation`s, the hailfire's and the
//! Republic gunship's, are x0), so no BF1 vehicle lights the mark.
//!
//! The round is laid from the soldier's fire point straight at the part
//! (`Bolt::fire`, the class's own ordnance), not through the trigger: the
//! launcher's aim pipeline (reticle ray, fire delay) is not what is tested,
//! and a bot aims by its own think.

use core::collision::{HullFilter, Layers};
use core::projectile::{Bolt, Shooter, Target};
use core::sim::{Event, PadState, Sim};
use core::unit::{Controller, UnitHandle};
use core::vehicle::VehicleHandle;
use glam::Vec3;

struct World {
    sim: Sim,
    db: game::classdb::ClassDb,
    names: Vec<(String, usize)>,
    classes: Vec<String>,
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
    for t in &mut sim.battle.teams {
        t.reinforcements = 200.0;
    }
    // No posts here: every pad makes its own side's class (the first team it
    // has one for) on the first tick, empty and of no side.
    for p in &mut sim.vehicle_pads {
        p.team = p.team.filter(|&t| p.def.class_for(Some(t)).is_some());
        p.team = p
            .team
            .or_else(|| (0..2).find(|&t| p.def.class_for(Some(t)).is_some()));
    }
    sim.step(&[]);
    Some(World {
        sim,
        db,
        names: Vec::new(),
        classes: build.classes.clone(),
    })
}

fn spawn(w: &mut World, class: &str, team: u8, pos: Vec3, controller: Controller) -> UnitHandle {
    let roster = game::roster::ClassRoster::build(&w.db, class, 0, None).unwrap();
    let c = roster.register(&mut w.sim, &mut w.names);
    w.sim
        .spawn(&c.spec(team, pos, 0.0, controller, 0, 0))
        .unwrap()
}

/// The centres of `vh`'s hull parts in the ORDNANCE subset that take
/// damage, weak spots (factor over 1) or plain (factor 1).
fn parts(w: &World, vh: VehicleHandle, weak: bool) -> Vec<Vec3> {
    w.sim
        .collision
        .hull(vh)
        .iter()
        .filter(|b| b.layers.contains(Layers::ORDNANCE) && b.hit_mult > 0.0)
        .filter(|b| (b.hit_mult > 1.0) == weak)
        .map(|b| b.to_world.transform_point3(Vec3::ZERO))
        .collect()
}

/// Stand `u` on a spot from which a line out of his fire point meets `vh`'s
/// hull first at one of `aims`, on a weak spot (`weak`) or not: the fire
/// point and the direction, `None` when no spot near has one.
fn stand_in_line(
    w: &mut World,
    u: UnitHandle,
    vh: VehicleHandle,
    aims: &[Vec3],
    weak: bool,
) -> Option<(Vec3, Vec3)> {
    let floor = w.sim.vehicle(vh)?.pos.y;
    for &aim in aims {
        for dist in [12.0f32, 18.0, 25.0] {
            for k in 0..32 {
                let a = k as f32 * std::f32::consts::TAU / 32.0;
                let mut p = aim + Vec3::new(a.cos(), 0.0, a.sin()) * dist;
                p.y = w.sim.spawn_ground(p);
                if (p.y - floor).abs() > 4.0 {
                    continue;
                }
                let s = w.sim.unit_mut(u).unwrap();
                s.soldier.pos = p;
                let d = aim - p;
                s.soldier.yaw = (-d.x).atan2(-d.z);
                let from = w.sim.unit(u).unwrap().fire_point();
                let dir = (aim - from).normalize();
                let Some(h) = w.sim.collision.ray_with(
                    from,
                    dir,
                    (aim - from).length() + 1.0,
                    Layers::ORDNANCE,
                    HullFilter::All,
                ) else {
                    continue;
                };
                if h.owner == Some(vh)
                    && (h.hit_mult > 1.0) == weak
                    && h.hit_mult > 0.0
                    && core::perception::los_terrain(&w.sim, from, from + dir * h.t)
                {
                    return Some((from, dir));
                }
            }
        }
    }
    None
}

/// What one round did: the vehicle's drop, a critical-hit event on it, and
/// the HUD feed's mark for the human viewer.
struct Shot {
    drop: f32,
    critical: bool,
    mark: Option<[u8; 3]>,
}

/// `shooter`'s round of `weapon` (`None`: the one in his hand) from `from` along `dir`, followed for 2 s,
/// the feed watching `viewer`.
fn lay_round(
    w: &mut World,
    shooter: UnitHandle,
    viewer: UnitHandle,
    vh: VehicleHandle,
    weapon: Option<&str>,
    (from, dir): (Vec3, Vec3),
) -> Shot {
    let u = w.sim.unit(shooter).unwrap();
    let wc = weapon.map_or(u.weapon_class, |name| {
        w.sim
            .weapon_classes
            .iter()
            .position(|c| c.name == name)
            .unwrap_or_else(|| panic!("{name} not loaded"))
    });
    let who = Shooter {
        owner: shooter,
        team: u.team,
        weapon_class: wc,
        is_player: u.is_player,
    };
    let mut feed =
        game::event_feed::EventFeed::new(game::event_feed::Texts::builtin(), ["rep", "cis"], &[]);
    let o = &w.sim.weapon_classes[wc].ordnance;
    let b = Bolt::fire(
        from,
        dir,
        o.velocity,
        o.life_span,
        who,
        w.sim.heightfield.as_ref(),
        w.sim.water_plane(),
        &w.sim.collision,
        HullFilter::All,
    );
    w.sim.bolts.push(b);
    let before = w.sim.vehicle(vh).unwrap().health.cur;
    let (mut critical, mut mark) = (false, None);
    for _ in 0..60 {
        w.sim.step(&[PadState::default()]);
        feed.tick(&w.sim, viewer);
        mark = mark.or(feed.hit_critical(w.sim.tick));
        critical |= w.sim.events.iter().any(|e| {
            matches!(e, Event::CriticalHit { shooter: s, victim, .. }
                if *s == shooter && *victim == Target::Vehicle(vh))
        });
    }
    let after = w.sim.vehicle(vh).map_or(0.0, |v| v.health.cur.max(0.0));
    Shot {
        drop: before - after,
        critical,
        mark,
    }
}

/// The side's rocketeer with his launcher, and its rifleman.
fn kit(side: &str) -> [(String, Option<String>); 2] {
    let class = |role: &str| match side {
        "rep" => format!("rep_inf_ep2_{role}"),
        _ => format!("{side}_inf_{role}"),
    };
    [
        (
            class("rocketeer"),
            Some(format!("{side}_weap_inf_rocket_launcher")),
        ),
        // His primary, the side's blaster rifle (a droid's own name).
        (class("rifleman"), None),
    ]
}

#[test]
fn a_rocket_on_a_vehicle_weak_spot_lights_the_critical_mark() {
    let factions = ["rep", "cis"];
    let Some(probe) = world("bf2_geo1", factions) else {
        return;
    };
    let mut cases: Vec<(String, VehicleHandle)> = Vec::new();
    for (vh, v) in probe.sim.alive_vehicles() {
        let c = probe.classes[v.class as usize].clone();
        if !parts(&probe, vh, true).is_empty() && !cases.iter().any(|(k, _)| *k == c) {
            cases.push((c, vh));
        }
    }
    let names: Vec<&str> = cases.iter().map(|(c, _)| c.as_str()).collect();
    eprintln!("bf2_geo1 classes with a weak spot: {names:?}");
    assert!(names.contains(&"rep_walk_atte"), "{names:?}");
    assert!(names.contains(&"cis_tread_hailfire"), "{names:?}");
    let mut lit = Vec::new();
    for (class, vh) in &cases {
        // The vehicle's side by its class prefix; the shooter is the other.
        let home = usize::from(class.starts_with("cis"));
        let team = (1 - home) as u8;
        let [(rocketeer, rocket), (rifleman, rifle)] = kit(factions[1 - home]);
        // Each round in a fresh world: the vehicle at full health.
        let one = |weak: bool, class_name: &str, weapon: Option<&str>, human: bool| {
            let mut w = world("bf2_geo1", factions).unwrap();
            let far = Vec3::new(5000.0, 0.0, 5000.0);
            let (me, viewer) = if human {
                let me = spawn(
                    &mut w,
                    class_name,
                    team,
                    far,
                    Controller::Player { slot: 0 },
                );
                (me, me)
            } else {
                // The human is elsewhere; the round is a bot's.
                let viewer = spawn(
                    &mut w,
                    class_name,
                    team,
                    far,
                    Controller::Player { slot: 0 },
                );
                (spawn(&mut w, class_name, team, far, Controller::Ai), viewer)
            };
            let aims = parts(&w, *vh, weak);
            let line = stand_in_line(&mut w, me, *vh, &aims, weak)?;
            let s = lay_round(&mut w, me, viewer, *vh, weapon, line);
            let held = &w.sim.weapon_classes[w.sim.unit(me).unwrap().weapon_class].name;
            Some((s, held.clone()))
        };
        let Some((hit, rocket)) = one(true, &rocketeer, rocket.as_deref(), true) else {
            eprintln!("{class}: no clear line at its weak spot");
            continue;
        };
        eprintln!(
            "{class} <- {rocket} on the weak spot: drop {}, event {}, mark {:?}",
            hit.drop, hit.critical, hit.mark
        );
        assert!(hit.drop > 0.0, "{class}: the rocket did damage");
        assert!(hit.critical, "{class}: the critical-hit event");
        // White on an empty vehicle of no side, red on one its pad gives
        // the other side (the AT-TE, a command walker, keeps its team).
        assert!(
            [ui::hud::target_color::NONE, ui::hud::target_color::ENEMY]
                .map(Some)
                .contains(&hit.mark),
            "{class}: the mark {:?}",
            hit.mark
        );
        lit.push(class.clone());

        if let Some((s, _)) = one(false, &rocketeer, Some(&rocket), true) {
            eprintln!("{class} <- {rocket} on the hull: drop {}", s.drop);
            assert!(s.drop > 0.0, "{class}: the hull rocket did damage");
            assert!(!s.critical, "{class}: the hull: no critical hit");
            assert_eq!(s.mark, None, "{class}: the hull: no mark");
        }
        let (s, rifle) = one(true, &rifleman, rifle.as_deref(), true).expect("the same line");
        eprintln!(
            "{class} <- {rifle} on the weak spot: drop {}, event {}",
            s.drop, s.critical
        );
        assert_eq!(
            s.critical,
            s.drop > 0.0,
            "{class}: the rifle's x4 that took health is a critical hit"
        );
        assert_eq!(s.mark, None, "{class}: but a rifle lights no mark");
        let (s, _) = one(true, &rocketeer, Some(&rocket), false).expect("the same line");
        eprintln!(
            "{class} <- a bot's {rocket} on the weak spot: drop {}, event {}",
            s.drop, s.critical
        );
        assert!(s.drop > 0.0, "{class}: the bot's rocket did damage");
        assert!(!s.critical, "{class}: a bot on Normal: no x4");
        assert_eq!(s.mark, None, "{class}: a bot's rocket: no mark");
    }
    eprintln!("lit: {lit:?}");
    assert!(lit.len() >= 2, "{lit:?} lit the mark");
}

/// Which weapons are rocket launchers (`WeaponClass::rocket_launcher`), on
/// both games' real classes: BF2 by `ScoreForMedalsType` 2 (the four sides'
/// infantry launchers, not the EMP launcher; the award / hero guided ones
/// carry the 2 too, but `weapon_from_db` builds no class for them - their
/// chain reaches `com_weap_null`), BF1 by name (the four sides' infantry launchers); never a
/// rifle, a sniper rifle, a wrist rocket or a mortar.
#[test]
fn the_rocket_launchers_of_both_games() {
    let root = game::assets::baked_root().join("classes");
    if !root.join("bf2_rep.ron").exists() {
        eprintln!("SKIP: no class bake");
        return;
    }
    let sides = ["rep", "cis", "all", "imp"];
    let check = |game: &str, yes: &[&str], no: &[&str]| {
        let db = game::roster::load_classdb(&root, game, &sides).unwrap();
        for (names, want) in [(yes, true), (no, false)] {
            for n in names {
                let w = game::roster::weapon_from_db(&db, n)
                    .unwrap_or_else(|e| panic!("{game} {n}: {e}"));
                assert_eq!(
                    w.rocket_launcher(),
                    want,
                    "{game} {n} ({:?})",
                    w.medals_type
                );
            }
        }
    };
    check(
        "bf2",
        &[
            "rep_weap_inf_rocket_launcher",
            "cis_weap_inf_rocket_launcher",
            "all_weap_inf_rocket_launcher",
            "imp_weap_inf_rocket_launcher",
        ],
        &[
            "rep_weap_inf_emp_launcher",
            "rep_weap_inf_rifle",
            "rep_weap_inf_sniper_rifle",
            "cis_weap_inf_wrist_rocket",
            "imp_weap_inf_mortar_launcher",
        ],
    );
    check(
        "bf1",
        &[
            "rep_weap_inf_rocket_launcher",
            "rep_weap_inf_torpedo_launcher",
            "cis_weap_inf_torpedo_launcher",
            "all_weap_inf_torpedo_launcher",
            "imp_weap_inf_torpedo_launcher",
        ],
        &[
            "rep_weap_inf_rifle",
            "cis_weap_inf_wrist_rocket",
            "imp_weap_inf_mortar_launcher",
            "all_weap_inf_mortar_launcher",
        ],
    );
}
