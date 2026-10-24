//! BUGX synthetic scenes. These exercise runtime rules without claiming a
//! baked-map or PS2 reproduction. Suicide-menu and medic/ammo soldier caller
//! coverage remain separate prerequisites; stations are only a service control.
use core::cp::CpDef;
use core::heightfield::Heightfield;
use core::region::Region;
use core::sim::{buttons, DeathCause, Event, PadState, SpawnStage, Station, StationClass};
use core::unit::{Controller, UnitSpec};
use game::walk::WalkSim;
use glam::Vec3;

fn scene() -> WalkSim {
    let mut w = WalkSim::new(
        42,
        Vec3::ZERO,
        Some(Heightfield {
            size: 2,
            unit: 4000.0,
            offset_x: 2000.0,
            offset_z: 2000.0,
            heights: vec![0.0; 4],
        }),
    );
    let (capture_time, neutralize_time) = CpDef::class_times(false);
    w.sim.add_cp(
        CpDef {
            name: "home".into(),
            pos: Vec3::new(30.0, 0.0, 0.0),
            capture: Region::sphere(Vec3::new(30.0, 0.0, 0.0), 8.0),
            control: None,
            value_bleed: 0,
            value_atk: [0; 2],
            value_def: [0; 2],
            spawn_nodes: vec![(Vec3::new(30.0, 0.0, 0.0), 0.0)],
            capture_time,
            neutralize_time,
        },
        Some(0),
    );
    w
}

fn kill_with_blast(w: &mut WalkSim, h: core::unit::UnitHandle) {
    let pos = w.sim.unit(h).unwrap().soldier.pos + Vec3::Y;
    w.sim.explode(
        &core::weapon::ExplosionClass {
            damage: 20000.0,
            damage_inner: 2.0,
            damage_outer: 2.0,
            ..Default::default()
        },
        pos,
        None,
        None,
    );
    assert!(!w.sim.units[h.index as usize].alive);
    assert!(w.sim.units[h.index as usize].body, "visible corpse");
}

fn tap(w: &mut WalkSim, b: u32) {
    w.step_tick(&[PadState::default()]);
    w.step_tick(&[PadState {
        buttons: b,
        ..Default::default()
    }]);
}

#[test]
fn bugx_a_a_bot_drops_its_dead_target_and_stops_firing() {
    let mut w = scene();
    let target = w.player;
    let bot = w
        .sim
        .spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(1, Vec3::new(0.0, 0.0, 6.0))
        })
        .unwrap();
    // Establish a real acquired target and shots before the death.
    let mut shots = 0;
    for _ in 0..90 {
        w.sim.unit_mut(target).unwrap().health.cur = 10000.0;
        w.step_tick(&[PadState::default()]);
        shots += w
            .sim
            .events
            .iter()
            .filter(|e| matches!(e, Event::Shot { unit, .. } if *unit == bot))
            .count();
    }
    assert!(shots > 0, "live control must shoot");
    assert_eq!(w.sim.unit(bot).unwrap().ai.target, Some(target));
    kill_with_blast(&mut w, target);
    assert!(
        w.sim.units[target.index as usize].body,
        "corpse remains visible"
    );
    for _ in 0..30 {
        w.step_tick(&[PadState::default()]);
        assert!(
            !w.sim
                .events
                .iter()
                .any(|e| matches!(e, Event::Shot { unit, .. } if *unit == bot)),
            "shot after death"
        );
    }
    let ai = &w.sim.unit(bot).unwrap().ai;
    assert_ne!(ai.target, Some(target), "dead target retained");
    assert!(ai.contacts.iter().flatten().all(|c| c.handle != target));
}

#[test]
fn bugx_a_stations_do_not_heal_or_resupply_a_corpse() {
    for (health, clips) in [(25.0, 0), (0.0, 1)] {
        let mut w = scene();
        let h = w.player;
        let class = w
            .sim
            .add_station_class(StationClass::soldier("fixture", 0.1, health, clips));
        w.sim.stations.push(Station::new(class, Vec3::ZERO, 5.0));
        let u = w.sim.unit_mut(h).unwrap();
        u.health.cur = 100.0;
        u.weapon.spare = 0;
        for _ in 0..5 {
            w.step_tick(&[PadState::default()]);
        }
        let u = w.sim.unit(h).unwrap();
        assert!(
            if health > 0.0 {
                u.health.cur > 100.0
            } else {
                u.weapon.spare > 0
            },
            "live service control"
        );
        kill_with_blast(&mut w, h);
        let u = &mut w.sim.units[h.index as usize];
        u.weapon.spare = 0;
        let hp = u.health.cur;
        for _ in 0..30 {
            w.step_tick(&[PadState::default()]);
            let u = &w.sim.units[h.index as usize];
            assert_eq!(u.health.cur, hp, "corpse healed");
            assert_eq!(u.weapon.spare, 0, "corpse resupplied");
            assert!(!w.sim.stations[0].serving, "served only a corpse");
        }
    }
}

fn hud_model(dead: bool) -> ui::hud::HudModel {
    ui::hud::HudModel {
        team_names: ["one".into(), "two".into()],
        tickets: [250; 2],
        bleed: [0.0; 2],
        hp: if dead { 0.0 } else { 100.0 },
        hp_max: 100.0,
        energy: 100.0,
        energy_max: 100.0,
        weapon: None,
        weapon2: None,
        reticle: None,
        target: None,
        reticle_y: 0.5,
        dead,
        respawn_in: None,
        outcome: None,
        time_left: None,
        messages: vec![],
        stats: None,
        player_team: 0,
        map: None,
        squad: None,
        capture: None,
        ambush: vec![],
        spawn: None,
    }
}

#[test]
fn bugx_c_a_hit_before_death_has_no_chevron_at_respawn() {
    let mut w = scene();
    let layout = manifest::hud::HudLayout {
        name: "fixture".into(),
        nodes: vec![],
    };
    let mut hud = ui::hud_tree::Hud::new(&layout, &layout);
    let first = w.player;
    let mut model = hud_model(false);
    model.ambush.push(
        game::event_feed::ambush_edge(
            &w.sim,
            first,
            &Event::Hit {
                shooter: first,
                victim: first,
                damage: 1.0,
                age: 0.0,
                health_left: 99.0,
                dir: Vec3::X,
            },
        )
        .expect("direction lights an edge"),
    );
    ui::hud::feed(&mut hud, &model, &|_| false);
    assert!(hud.ambush.timers.iter().any(|t| *t > 0.0), "hit control");
    kill_with_blast(&mut w, first);
    for _ in 0..200 {
        w.step_tick(&[PadState::default()]);
        ui::hud::feed(
            &mut hud,
            &hud_model(!w.sim.units[first.index as usize].alive),
            &|_| false,
        );
        hud.advance(core::TICK_DT);
        if w.player != first {
            break;
        }
    }
    assert_ne!(w.player, first, "must reach a new life");
    assert!(w.player_on_foot(), "WalkSim follows the new generation");
    assert_eq!(hud.ambush.timers, [0.0; 4]);
}

#[test]
fn bugx_d_water_death_opens_spawn_map_then_respawns() {
    let mut w = scene();
    w.sim.spawn_screens = true;
    let first = w.player;
    // An authored death region below a water plane; water alone does not
    // implement drowning. The dry home node is outside this volume.
    w.sim.water_level = Some(2.0);
    w.sim.death_regions.push(Region::sphere(Vec3::ZERO, 3.0));
    w.step_tick(&[PadState::default()]);
    assert!(w.sim.events.iter().any(
        |e| matches!(e, Event::EnvDeath { unit, cause: DeathCause::DeathRegion } if *unit == first)
    ));
    assert!(!w.sim.units[first.index as usize].alive);
    assert!(w.sim.units[first.index as usize].soldier.pos.y < w.sim.water_level.unwrap());
    for _ in 0..125 {
        w.step_tick(&[PadState::default()]);
    }
    assert_eq!(w.sim.spawn_stage[0], SpawnStage::Map);
    assert_eq!(w.player, first, "waiting for accepts");
    tap(&mut w, buttons::SPAWN_ACCEPT);
    assert_eq!(w.sim.spawn_stage[0], SpawnStage::Class);
    tap(&mut w, buttons::SPAWN_ACCEPT);
    assert_eq!(w.sim.spawn_stage[0], SpawnStage::Accepted);
    assert_eq!(w.player.gen, first.gen + 1);
    assert!(w.player_on_foot());
    assert!((w.soldier().pos.x - 30.0).abs() < 0.01, "dry home spawn");
}

#[test]
fn bugx_a_pickups_do_not_heal_or_resupply_a_corpse() {
    for (health, clips) in [(50.0, 0), (0.0, 1)] {
        let mut w = scene();
        let h = w.player;
        let class =
            w.sim
                .add_item_class(core::sim::ItemClass::basic("fixture_item", health, clips));
        let life = w.sim.item_classes[usize::from(class)].life_span;
        let lay = |w: &mut WalkSim| {
            let pos = w.sim.units[h.index as usize].soldier.pos;
            w.sim.pickups.push(core::sim::Pickup {
                class,
                pos,
                vel: Vec3::ZERO,
                yaw: 0.0,
                life,
                landed: true,
            });
        };
        let u = w.sim.unit_mut(h).unwrap();
        u.health.cur = 100.0;
        u.weapon.spare = 0;
        lay(&mut w);
        w.step_tick(&[PadState::default()]);
        let u = w.sim.unit(h).unwrap();
        assert!(w.sim.pickups.is_empty(), "live control takes the item");
        assert!(
            if health > 0.0 {
                u.health.cur > 100.0
            } else {
                u.weapon.spare > 0
            },
            "live pickup control"
        );
        kill_with_blast(&mut w, h);
        w.sim.pickups.clear();
        let u = &mut w.sim.units[h.index as usize];
        u.weapon.spare = 0;
        let hp = u.health.cur;
        lay(&mut w);
        for _ in 0..30 {
            w.step_tick(&[PadState::default()]);
            let u = &w.sim.units[h.index as usize];
            assert!(!u.alive, "still the corpse");
            assert_eq!(u.health.cur, hp, "corpse healed by a pickup");
            assert_eq!(u.weapon.spare, 0, "corpse resupplied by a pickup");
        }
        assert_eq!(w.sim.pickups.len(), 1, "the corpse took the item");
    }
}
