use super::*;
use crate::ai::{think, Behaviour};
use crate::collision::{Body, Shape};
use crate::heightfield::Heightfield;
use crate::sim::{Event, ItemClass, PadState, Pickup};
use crate::unit::{Controller, UnitHandle, UnitSpec};
use glam::Mat4;

fn world(health: f32, ammo: u32) -> (Sim, UnitHandle) {
    let mut s = Sim::empty(19);
    s.heightfield = Some(Heightfield {
        size: 2,
        unit: 400.0,
        offset_x: 200.0,
        offset_z: 200.0,
        heights: vec![0.0; 4],
    });
    s.ground_rules = GroundRules::Bf2;
    s.spawn(&UnitSpec::player(0, Vec3::X * 10.0)).unwrap();
    let bot = s
        .spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(0, Vec3::ZERO)
        })
        .unwrap();
    s.add_item_class(ItemClass::basic("item", health, ammo));
    s.pickups.push(Pickup {
        class: 0,
        pos: Vec3::new(0.0, 0.0, -5.0),
        vel: Vec3::ZERO,
        yaw: 0.0,
        life: 30.0,
        landed: true,
    });
    (s, bot)
}

fn ask(s: &Sim, bot: UnitHandle, st: &mut AiState) -> Option<Vec3> {
    goal(s, s.unit(bot).unwrap(), st)
}

#[test]
fn bf2_bot_walks_to_needed_health_then_returns_and_is_deterministic() {
    let run = || {
        let (mut s, bot) = world(100.0, 0);
        s.unit_mut(bot).unwrap().health.cur -= 100.0;
        let origin = s.unit(bot).unwrap().soldier.pos;
        let mut taken = false;
        let mut detoured = false;
        let mut returned = false;
        let mut hashes = Vec::new();
        for _ in 0..150 {
            s.step(&[PadState::default()]);
            taken |= s
                .events
                .iter()
                .any(|e| matches!(e, Event::ItemTaken { unit, health: true, .. } if *unit == bot));
            let u = s.unit(bot).unwrap();
            detoured |= u.ai.behaviour == Behaviour::Pickup;
            returned |=
                taken && u.ai.items.detour.is_none() && u.soldier.pos.distance(origin) < 0.5;
            hashes.push(s.hash());
        }
        assert!(
            taken && detoured && returned,
            "taken={taken} detoured={detoured} returned={returned}"
        );
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn scans_are_two_seconds_apart_and_success_adds_four_seconds() {
    let (mut s, bot) = world(100.0, 0);
    let mut st = AiState::default();
    assert!(ask(&s, bot, &mut st).is_none());
    // Native 2 s / 4 s at 30 Hz: keep this oracle independent of PERIOD
    // and FOUND_DELAY, so changing either production constant fails here.
    assert_eq!(st.items.next_scan, 60);
    s.unit_mut(bot).unwrap().health.cur -= 100.0;
    s.tick = 59;
    assert!(ask(&s, bot, &mut st).is_none());
    s.tick = 60;
    assert_eq!(ask(&s, bot, &mut st), Some(Vec3::new(0.0, 0.0, -6.0)));
    assert_eq!(st.items.next_scan, 60 + 60 + 120);

    // End the detour to isolate the cooldown from movement/collection.
    st.items.detour = None;
    s.tick = 239;
    assert!(ask(&s, bot, &mut st).is_none());
    s.tick = 240;
    assert!(ask(&s, bot, &mut st).is_some());
    assert_eq!(st.items.next_scan, 420);
}

#[test]
fn a_bot_takes_ammo_but_never_detours_for_a_service_droid() {
    let (mut s, bot) = world(0.0, 1);
    s.unit_mut(bot).unwrap().weapon.spare = 0;
    let mut taken = false;
    for _ in 0..90 {
        s.step(&[PadState::default()]);
        taken |= s
            .events
            .iter()
            .any(|e| matches!(e, Event::ItemTaken { unit, ammo: true, .. } if *unit == bot));
    }
    assert!(taken);
    assert!(s.unit(bot).unwrap().weapon.spare > 0);
    let (mut s, bot) = world(100.0, 0);
    s.pickups.clear();
    s.unit_mut(bot).unwrap().health.cur = 1.0;
    s.add_station_class(crate::sim::StationClass::soldier("droid", 0.5, 15.0, 0));
    s.stations
        .push(crate::sim::Station::new(0, Vec3::new(0.0, 0.0, -5.0), 2.0));
    for _ in 0..90 {
        s.step(&[PadState::default()]);
    }
    assert!(s.unit(bot).unwrap().ai.items.detour.is_none());
    assert_eq!(s.unit(bot).unwrap().soldier.pos, Vec3::ZERO);
}

#[test]
fn bf1_full_units_energy_buffs_far_items_and_far_humans_never_detour() {
    for case in 0..7 {
        let (mut s, bot) = world(100.0, 0);
        if case != 1 {
            s.unit_mut(bot).unwrap().health.cur -= 100.0;
        }
        match case {
            0 => s.ground_rules = GroundRules::Bf1,
            1 => {}
            2 => {
                s.item_classes[0].health = 0.0;
                s.item_classes[0].energy = 150.0;
            }
            3 => {
                s.item_classes[0].health = 0.0;
                s.item_classes[0].offense = (12.0, 1.5);
            }
            4 => s.pickups[0].pos.z = -6.01,
            5 => s.units[0].soldier.pos.x = 20.01,
            _ => {
                s.unit_mut(bot).unwrap().mount = Some(crate::vehicle::Mount {
                    vehicle: crate::vehicle::VehicleHandle { index: 0, gen: 0 },
                    seat: 0,
                })
            }
        }
        let mut st = AiState::default();
        assert!(ask(&s, bot, &mut st).is_none(), "case {case}");
        if case == 0 {
            assert_eq!(st.items, ItemSeek::default());
        }
    }
}

#[test]
fn ammo_need_checks_reserve_caps_and_held_weapons() {
    let (mut s, bot) = world(0.0, 1);
    assert!(ask(&s, bot, &mut AiState::default()).is_none());
    s.unit_mut(bot).unwrap().weapon.spare = 0;
    assert!(ask(&s, bot, &mut AiState::default()).is_some());
    let held = crate::unit::HeldWeapon {
        slot: 1,
        channel: 0,
        class: 0,
        state: crate::weapon::WeaponState::with_clips(&s.weapon_classes[0], false, 3),
    };
    let u = s.unit_mut(bot).unwrap();
    u.weapon.spare = crate::weapon::SPARE_INFINITE;
    u.spec.slots = vec![crate::unit::SlotSpec {
        slot: 1,
        channel: 0,
        weapon_class: 0,
        clips: 3,
    }]
    .into();
    u.held.push(held);
    assert!(ask(&s, bot, &mut AiState::default()).is_none());
    s.unit_mut(bot).unwrap().held[0].state.spare = 0;
    assert!(ask(&s, bot, &mut AiState::default()).is_some());
}

#[test]
fn a_wall_blocks_the_detour_and_a_taken_item_returns_the_bot_to_its_origin() {
    let (mut s, bot) = world(100.0, 0);
    s.unit_mut(bot).unwrap().health.cur -= 100.0;
    s.collision.push_body(Body::new(
        Mat4::from_translation(Vec3::new(0.0, 1.0, -2.5)),
        Shape::Box {
            half: Vec3::new(2.0, 2.0, 0.1),
        },
    ));
    assert!(ask(&s, bot, &mut AiState::default()).is_none());
    let (mut s, bot) = world(100.0, 0);
    s.unit_mut(bot).unwrap().health.cur -= 100.0;
    let mut st = AiState::default();
    assert!(ask(&s, bot, &mut st).is_some());
    s.unit_mut(bot).unwrap().soldier.pos.z = -3.0;
    s.pickups.clear();
    assert_eq!(ask(&s, bot, &mut st), Some(Vec3::ZERO));
    s.unit_mut(bot).unwrap().soldier.pos = Vec3::ZERO;
    assert!(ask(&s, bot, &mut st).is_none());
    assert!(st.items.detour.is_none());
}

#[test]
fn a_blocked_point_past_one_item_does_not_hide_another_reachable_item() {
    let (mut s, bot) = world(100.0, 0);
    s.unit_mut(bot).unwrap().health.cur -= 100.0;
    let mut second = s.pickups[0];
    second.pos = Vec3::new(-4.0, 0.0, 0.0);
    s.pickups.push(second);
    s.collision.push_body(Body::new(
        Mat4::from_translation(Vec3::new(0.0, 1.0, -6.0)),
        Shape::Box {
            half: Vec3::new(0.2, 1.0, 0.1),
        },
    ));
    assert!(s.floor_line(Vec3::ZERO, s.pickups[0].pos));
    assert!(!s.floor_line(Vec3::ZERO, Vec3::new(0.0, 0.0, -6.0)));
    let mut st = AiState::default();
    assert_eq!(ask(&s, bot, &mut st), Some(Vec3::new(-5.0, 0.0, 0.0)));
    assert_eq!(st.items.detour.unwrap().item, second.pos);
}

#[test]
fn scans_and_each_detour_field_affect_the_hash() {
    let (mut s, bot) = world(100.0, 0);
    s.unit_mut(bot).unwrap().health.cur -= 100.0;
    let mut st = AiState::default();
    let base = |st: &AiState| {
        let mut bytes = Vec::new();
        st.hash_into(&mut |b| bytes.extend_from_slice(b));
        bytes
    };
    let idle = base(&st);
    let _ = think(&s, bot, &mut st);
    assert_ne!(idle, base(&st));
    let hash = base(&st);
    for case in 0..7 {
        let mut other = st;
        let d = other.items.detour.as_mut().unwrap();
        match case {
            0 => other.items.next_scan += 1,
            1 => d.origin.x += 1.0,
            2 => d.item.y += 1.0,
            3 => d.past.z += 1.0,
            4 => d.class += 1,
            5 => d.returning = !d.returning,
            _ => d.until += 1,
        }
        assert_ne!(hash, base(&other), "case {case}");
    }
}
