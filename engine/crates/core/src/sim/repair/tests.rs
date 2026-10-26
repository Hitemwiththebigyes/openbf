use super::*;
use crate::collision::{Body, HullPart, Shape};
use crate::sim::{buttons, PadState, Station, StationClass};
use crate::unit::{SlotSpec, UnitHandle, UnitSpec};
use crate::weapon::{DamageDesc, OrdnanceClass, Repair};
use glam::Mat4;

fn cutter(bf2: bool) -> WeaponClass {
    let mut health = [0.0; 8];
    health[3] = if bf2 { 200.0 } else { 60.0 };
    health[4] = if bf2 { 100.0 } else { 60.0 };
    health[5] = if bf2 { 50.0 } else { 20.0 };
    health[6] = if bf2 { 50.0 } else { 30.0 };
    WeaponClass {
        name: "fusioncutter".into(),
        repair: Some(Repair { health }),
        target_team: 6,
        lock_on_range: if bf2 { 4.0 } else { 3.0 },
        lock_on_angle: if bf2 { 90.0 } else { 30.0 } * crate::weapon::DEG,
        shot_delay: [0.5; 2],
        heat_per_shot: 0.06,
        ..WeaponClass::default()
    }
}

fn fixture(bf2: bool, secondary: bool) -> (Sim, UnitHandle) {
    let mut sim = Sim::empty(5);
    sim.heightfield = Some(super::super::tests::flat_ground());
    sim.ground_rules = if bf2 {
        crate::soldier::GroundRules::Bf2
    } else {
        crate::soldier::GroundRules::Bf1
    };
    sim.weapon_classes.push(cutter(bf2));
    let mut spec = UnitSpec::player(0, Vec3::new(0.0, 0.0, 2.5));
    if secondary {
        spec.slots = vec![SlotSpec {
            slot: 3,
            channel: 1,
            weapon_class: 1,
            clips: 0,
        }]
        .into();
    } else {
        spec.weapon_class = 1;
    }
    let owner = sim.spawn(&spec).unwrap();
    let mut class = StationClass::soldier("medical", 0.5, 15.0, 0);
    class.sphere = Some([0.0, 1.0, 0.0, 1.2]);
    class.wreck_sphere = Some([0.0, 0.2, 0.0, 0.6]);
    sim.add_station_class(class);
    let mut s = Station::new(0, Vec3::ZERO, 5.0);
    s.instance = Some(7);
    s.sphere_centre = Vec3::Y;
    s.sphere_radius = 1.2;
    sim.stations.push(s);
    let mut body = Body::new(
        Mat4::from_translation(Vec3::Y),
        Shape::Cylinder {
            radius: 0.4,
            half_height: 1.0,
        },
    );
    body.instance = Some(7);
    sim.collision.push_body(body);
    sim.collision.bind_station(7, 0);
    sim.station_wreck_collision.push(vec![HullPart::new(
        Mat4::from_translation(Vec3::Y * 0.2),
        Shape::Box {
            half: Vec3::new(0.4, 0.2, 0.4),
        },
        Layers::ALL,
    )]);
    (sim, owner)
}

fn destroy(sim: &mut Sim) {
    let damage: DamageDesc = OrdnanceClass {
        max_damage: 500.0,
        ..Default::default()
    }
    .damage_desc(0.0, false);
    sim.damage_station(0, &damage, 1.0, None, None);
    assert!(sim.stations[0].destroyed);
}

fn fire(secondary: bool) -> PadState {
    PadState {
        buttons: if secondary {
            buttons::SECONDARY_FIRE
        } else {
            buttons::FIRE
        },
        ..Default::default()
    }
}

#[test]
fn both_games_repair_and_rebuild_through_their_weapon_channels_at_disc_rates() {
    for (bf2, secondary) in [(false, true), (true, false)] {
        let (mut sim, owner) = fixture(bf2, secondary);
        sim.stations[0].health.cur = 10.0;
        sim.step(&[fire(secondary)]);
        assert_eq!(sim.stations[0].health.cur, if bf2 { 80.0 } else { 70.0 });
        assert!(sim.bolts.is_empty() && sim.stickies.is_empty());
        for _ in 0..16 {
            sim.step(&[PadState::default()]);
        }
        destroy(&mut sim);
        sim.unit_mut(owner).unwrap().health.cur = 100.0;
        let mut shots = Vec::new();
        let mut health_before = 0.0;
        for tick in 0..100 {
            sim.step(&[fire(secondary)]);
            if sim.stations[0].health.cur != health_before {
                let delta = sim.stations[0].health.cur - health_before;
                assert_eq!(
                    delta,
                    if bf2 {
                        50.0_f32.min(80.0 - health_before)
                    } else {
                        20.0
                    }
                );
                health_before = sim.stations[0].health.cur;
                shots.push(tick);
            }
            if !sim.stations[0].destroyed {
                break;
            }
            assert_eq!(sim.unit(owner).unwrap().health.cur, 100.0);
            assert!(!sim.stations[0].serving);
        }
        assert_eq!(shots.len(), if bf2 { 2 } else { 4 });
        assert!(shots.windows(2).all(|w| w[1] - w[0] == 15));
        assert_eq!(sim.stations[0].health.cur, 80.0);
        assert!(!sim.stations[0].destroyed);
        assert_eq!(sim.collision.station(7), Some(0));
        assert_eq!(sim.stations[0].sphere_centre, Vec3::Y);
        assert_eq!(sim.stations[0].sphere_radius, 1.2);
        let active: Vec<_> = sim
            .collision
            .bodies()
            .iter()
            .filter(|b| b.instance == Some(7) && b.layers != Layers::NONE)
            .collect();
        assert_eq!(active.len(), 1);
        assert!(matches!(active[0].shape, Shape::Cylinder { .. }));
        for _ in 0..16 {
            sim.step(&[PadState::default()]);
        }
        assert!(sim.unit(owner).unwrap().health.cur > 100.0);
        destroy(&mut sim);
        sim.repair_station(0, 80.0);
        assert_eq!(sim.collision.station(7), Some(0));
        assert_eq!(sim.station_bodies[0].len(), 1);
        assert_eq!(sim.collision.bodies().len(), 1);
    }
}

#[test]
fn repair_acquisition_obeys_range_cone_team_and_world_occlusion() {
    for case in 0..5 {
        let (mut sim, owner) = fixture(true, false);
        sim.stations[0].health.cur = 1.0;
        match case {
            0 => sim.unit_mut(owner).unwrap().soldier.pos.z = 10.0,
            1 => sim.unit_mut(owner).unwrap().soldier.yaw = std::f32::consts::PI,
            2 => sim.stations[0].team = Some(1),
            3 => sim.collision.push_body(Body::new(
                Mat4::from_translation(Vec3::new(0.0, 1.0, 1.2)),
                Shape::Box {
                    half: Vec3::new(2.0, 2.0, 0.1),
                },
            )),
            _ => sim.unit_mut(owner).unwrap().soldier.energy = 0.0,
        }
        sim.step(&[fire(false)]);
        assert_eq!(sim.stations[0].health.cur, 1.0, "case {case}");
        assert!(!sim.events.iter().any(|e| matches!(e, Event::Repair { .. })));
    }
}

#[test]
fn partial_rebuilds_stay_wrecks_and_identical_inputs_keep_identical_hashes() {
    let run = || {
        let (mut sim, _) = fixture(true, false);
        destroy(&mut sim);
        let dead = sim.hash();
        sim.repair_station(0, 30.0);
        assert!(sim.stations[0].destroyed);
        assert_ne!(dead, sim.hash());
        sim.repair_station(0, f32::NAN);
        assert_eq!(sim.stations[0].health.cur, 30.0);
        let mut hashes = Vec::new();
        for _ in 0..100 {
            sim.step(&[fire(false)]);
            hashes.push(sim.hash());
        }
        assert!(!sim.stations[0].destroyed);
        hashes
    };
    assert_eq!(run(), run());
}
