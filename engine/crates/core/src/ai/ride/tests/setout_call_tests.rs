//! WGOUT actual-step first routed departure witnesses.
use super::*;

fn planner_setout_scene(kind: VehicleKind, phase: u64, case: u8) -> bool {
    use crate::sim::{Call, Event};
    let (mut s, p, vh) = pickup_world(kind);
    s.ground_rules = if case == 2 {
        GroundRules::Bf2
    } else {
        GroundRules::Bf1
    };
    if case != 3 {
        arm_gunner_seat(&mut s);
    }
    s.unit_mut(p).unwrap().ai.next_think = phase;
    if case == 1 {
        assert!(s.enter_vehicle(p, vh, 0));
    }
    let mut entry = s.unit(p).unwrap().mount.map(|_| 0);
    let mut start = None;
    let mut departed = None;
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for _ in 0..230 {
        let tick = s.tick;
        s.step(&[]);
        let u = s.unit(p).unwrap();
        if u.mount.is_some() {
            entry.get_or_insert(tick);
        }
        if let Some(w) = u.ai.ride.and_then(|r| r.pickup) {
            if w.started == Some(tick) {
                start = Some(tick);
                if case != 2
                    && crate::ai::unit_unit(p.index, tick, 0, crate::ai::SALT_NEED_GUNNER) < 0.3
                {
                    expected.push(tick);
                }
            }
        }
        if u.mount.is_some() && locomotion(&s, p) > 0.0 && departed.is_none() {
            departed = Some(tick);
            if case != 2
                && crate::ai::unit_unit(p.index, tick, 0, crate::ai::SALT_NEED_GUNNER) < 0.3
            {
                expected.push(tick);
            }
        }
        for e in &s.events {
            if matches!(e, Event::Call{unit,call:Call::NeedGunner} if *unit==p) {
                actual.push(tick);
            }
        }
    }
    assert!(
        entry.is_some() && start.is_some() && departed.is_some(),
        "reachable driver"
    );
    eprintln!("SETOUT {kind:?} phase{phase} case{case} entry{entry:?} wait{start:?} departure{departed:?} actual{actual:?} expected{expected:?}");
    actual == expected
}
#[test]
fn planner_setout_missing() {
    let mut faults = 0;
    for k in [VehicleKind::Hover, VehicleKind::Walker] {
        for phase in [0, 1, 2, 9, 10, 11, 20] {
            for case in [0, 1] {
                faults += usize::from(!planner_setout_scene(k, phase, case));
            }
        }
    }
    assert_eq!(
        faults, 0,
        "seated initial set-out needs its independent call attempt"
    );
}
#[test]
fn planner_setout_controls() {
    for k in [VehicleKind::Hover, VehicleKind::Walker] {
        for phase in [0, 10, 20] {
            for case in [2, 3] {
                assert!(planner_setout_scene(k, phase, case));
            }
        }
    }
}

use crate::sim::{Call, Event};
fn count(s: &Sim, h: UnitHandle, call: Call) -> usize {
    s.events
        .iter()
        .filter(|e| matches!(e, Event::Call { unit, call:c } if *unit == h && *c == call))
        .count()
}
pub(super) fn departure_scene(
    kind: VehicleKind,
    receiver_first: bool,
) -> (Sim, UnitHandle, UnitHandle, VehicleHandle) {
    let (mut s, first, vh) = pickup_world(kind);
    s.ground_rules = GroundRules::Bf1;
    arm_gunner_seat(&mut s);
    s.cps[0].def.pos = Vec3::new(0., 0., -90.);
    s.cps[0].def.capture = Region::sphere(s.cps[0].def.pos, 8.);
    let other = s
        .spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(0, Vec3::new(500., 0., 500.))
        })
        .unwrap();
    let (p, a) = if receiver_first {
        (other, first)
    } else {
        (first, other)
    };
    s.unit_mut(a).unwrap().soldier.pos = Vec3::new(500., 0., 500.);
    s.unit_mut(p).unwrap().soldier.pos = Vec3::new(12., 0., 0.);
    // Pick an entry time with a successful departure draw for either issuer.
    let seated = (0..30)
        .find(|&t| crate::ai::unit_unit(p.index, t + 121, 0, crate::ai::SALT_NEED_GUNNER) < 0.3)
        .unwrap();
    // Keep the prospective pilot human until public entry, without injecting clocks.
    s.unit_mut(p).unwrap().controller = Controller::Player { slot: 1 };
    for _ in 0..seated {
        s.step(&[]);
    }
    s.unit_mut(p).unwrap().controller = Controller::Ai;
    assert!(s.enter_vehicle(p, vh, 0));
    s.unit_mut(a).unwrap().ai.next_think = s.tick + 1;
    (s, p, a, vh)
}

#[test]
fn late_receiver_gets_departure_target_after_all_writebacks() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for first in [false, true] {
            let (mut s, p, a, vh) = departure_scene(kind, first);
            let departure = s.tick + 121;
            while s.tick < departure {
                s.step(&[]);
                assert_eq!(count(&s, a, Call::Yes), 0, "absent at wait-start");
                assert!(s.unit(a).unwrap().ai.pending_board.is_none());
            }
            s.unit_mut(a).unwrap().soldier.pos =
                s.unit(p).unwrap().soldier.pos + Vec3::new(0., 0., -20.);
            s.step(&[]);
            assert!(s.unit(p).unwrap().ai.ride.unwrap().pickup.unwrap().departed);
            assert_eq!(count(&s, p, Call::NeedGunner), 1, "departure draw succeeds");
            assert_eq!(count(&s, a, Call::Yes), 1, "issue-time membership");
            assert_eq!(s.unit(a).unwrap().ai.pending_board, Some((a, vh)));
            assert!(
                s.unit(a).unwrap().ai.ride.is_none_or(|r| r.board.is_none()),
                "no pre-think consumption"
            );
            let due = s.unit(a).unwrap().ai.next_think;
            while s.tick < due {
                s.step(&[]);
                assert!(s.unit(a).unwrap().ai.ride.is_none_or(|r| r.board.is_none()));
                assert_eq!(count(&s, p, Call::NeedGunner), 0, "no repeated departure");
            }
            s.step(&[]);
            assert_eq!(s.unit(a).unwrap().ai.ride.unwrap().board, Some(vh));
            assert_eq!(s.unit(a).unwrap().ai.ride.unwrap().since, due);
            assert!(s.unit(a).unwrap().ai.pending_board.is_none());
        }
    }
}

#[test]
fn completion_without_route_or_at_post_does_not_depart() {
    use crate::nav::{Hub, NavGraph, RouteTable};
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for failed_route in [false, true] {
            let (mut s, p, vh) = pickup_world(kind);
            s.ground_rules = GroundRules::Bf1;
            arm_gunner_seat(&mut s);
            assert!(s.enter_vehicle(p, vh, 0));
            if failed_route {
                s.cps[0].nav_hub = Some(1);
                s.nav = Some(
                    NavGraph::new(
                        vec![
                            Hub {
                                pos: Vec3::new(12., 0., 0.),
                                radius: 8.,
                                arcs: vec![],
                            },
                            Hub {
                                pos: Vec3::new(0., 0., -300.),
                                radius: 8.,
                                arcs: vec![],
                            },
                        ],
                        vec![],
                        RouteTable::empty(2),
                        vec![],
                        vec![],
                    )
                    .unwrap(),
                );
            } else {
                s.cps[0].def.pos = s.vehicle(vh).unwrap().pos;
                s.cps[0].def.capture = Region::sphere(s.cps[0].def.pos, 8.);
            }
            let mut calls = Vec::new();
            for _ in 0..160 {
                let tick = s.tick;
                s.step(&[]);
                if count(&s, p, Call::NeedGunner) > 0 {
                    calls.push(tick);
                }
                if let Some(w) = s.unit(p).unwrap().ai.ride.and_then(|r| r.pickup) {
                    assert!(!w.departed, "expiry alone cannot depart");
                }
            }
            assert!(
                calls.iter().all(|&t| t == 0),
                "only initial start may call: {calls:?}"
            );
        }
    }
}

#[test]
fn departure_tenure_resets_on_real_exit_reentry_and_slot_reuse() {
    for armed in [false, true] {
        for kind in [VehicleKind::Hover, VehicleKind::Walker] {
            let (mut s, p, vh) = pickup_world(kind);
            s.ground_rules = GroundRules::Bf1;
            if armed {
                arm_gunner_seat(&mut s);
            }
            assert!(s.enter_vehicle(p, vh, 0));
            for _ in 0..130 {
                s.step(&[]);
            }
            assert!(s.unit(p).unwrap().ai.ride.unwrap().pickup.unwrap().departed);
            s.exit_vehicle(p);
            assert!(s.unit(p).unwrap().ai.ride.unwrap().pickup.is_none());
            assert!(s.enter_vehicle(p, vh, 0));
            assert!(!s.unit(p).unwrap().ai.ride.unwrap().pickup.unwrap().departed);
            for _ in 0..130 {
                s.step(&[]);
            }
            assert!(s.unit(p).unwrap().ai.ride.unwrap().pickup.unwrap().departed);
            s.remove_vehicle(vh);
            assert!(s.unit(p).unwrap().ai.ride.unwrap().pickup.is_none());
            let replacement = s.spawn_vehicle(&VehicleSpec {
                kind,
                class: 0,
                home_team: None,
                neutral_when_empty: true,
                spawn_pad: None,
                pos: Vec3::new(12., 2., 0.),
                yaw: std::f32::consts::PI,
                max_health: 1000.,
                health_type: HealthType::Vehicle,
                seats: vec![SeatKind::Pilot, SeatKind::Gunner(0)],
            });
            assert_eq!(vh.index, replacement.index);
            assert_ne!(vh.gen, replacement.gen);
            assert!(s.enter_vehicle(p, replacement, 0));
            assert!(!s.unit(p).unwrap().ai.ride.unwrap().pickup.unwrap().departed);
            for _ in 0..130 {
                s.step(&[]);
            }
            assert!(s.unit(p).unwrap().ai.ride.unwrap().pickup.unwrap().departed);
            let human = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
            assert!(s.enter_vehicle(human, replacement, 0));
            assert!(s.unit(p).unwrap().ai.ride.and_then(|r| r.pickup).is_none());
            s.exit_vehicle(human);
            assert!(s.enter_vehicle(p, replacement, 0));
            assert!(!s.unit(p).unwrap().ai.ride.unwrap().pickup.unwrap().departed);
            for _ in 0..130 {
                s.step(&[]);
            }
            s.kill_environment(p, crate::sim::DeathCause::Fall);
            assert!(s.units[p.index as usize]
                .ai
                .ride
                .and_then(|r| r.pickup)
                .is_none());
            let fresh = s
                .spawn(&UnitSpec {
                    controller: Controller::Ai,
                    ..UnitSpec::trooper(0, Vec3::ZERO)
                })
                .unwrap();
            assert_eq!(p.index, fresh.index);
            assert_ne!(p.gen, fresh.gen);
            assert!(s.enter_vehicle(fresh, replacement, 0));
            assert!(
                !s.unit(fresh)
                    .unwrap()
                    .ai
                    .ride
                    .unwrap()
                    .pickup
                    .unwrap()
                    .departed
            );
        }
    }
}

#[test]
fn turning_toward_route_consumes_departure_before_translation() {
    let (mut s, p, vh) = pickup_world(VehicleKind::Hover);
    s.ground_rules = GroundRules::Bf1;
    arm_gunner_seat(&mut s);
    // A class without strafe turns toward the routed goal before translating.
    s.hover_params[0].as_mut().unwrap().strafe_speed = 0.;
    s.vehicle_mut(vh).unwrap().rot = glam::Quat::IDENTITY;
    assert!(s.enter_vehicle(p, vh, 0));
    while s.tick < 121 {
        s.step(&[]);
    }
    s.step(&[]);
    let ride = s.unit(p).unwrap().ai.ride.unwrap();
    let sticks = ride.hover.unwrap().sticks;
    assert!(ride.pickup.unwrap().departed);
    assert_eq!(sticks[0], 0., "route starts with turning, not translation");
    assert_ne!(sticks[2], 0.);
    assert_eq!(count(&s, p, Call::NeedGunner), 1);
    for _ in 0..40 {
        s.step(&[]);
        assert_eq!(count(&s, p, Call::NeedGunner), 0);
    }
}

#[test]
fn affected_bf1_and_unaffected_bf2_trace_controls() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for rules in [GroundRules::Bf1, GroundRules::Bf2] {
            let (mut s, p, vh) = pickup_world(kind);
            s.ground_rules = rules;
            // WGSEAT affects BF1 unarmed state/calls; BF2 armed stays a null control.
            if rules == GroundRules::Bf2 {
                arm_gunner_seat(&mut s);
            }
            assert!(s.enter_vehicle(p, vh, 0));
            let mut hashes = Vec::new();
            for _ in 0..230 {
                s.step(&[]);
                hashes.push(s.hash());
            }
            eprintln!(
                "CONTROL {kind:?} {rules:?} hashes={hashes:?} rng={:?} lcg={:?}",
                s.rng.state(),
                s.lcg.state()
            );
        }
    }
}

#[test]
fn claim_delayed_departure_and_actual_release_edge_have_one_attempt() {
    use crate::sim::buttons;
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for claim_delay in [false, true] {
            let (mut s, p, vh) = pickup_world(kind);
            s.ground_rules = GroundRules::Bf1;
            arm_gunner_seat(&mut s);
            assert!(s.enter_vehicle(p, vh, 0));
            let boarder = if claim_delay {
                Some(
                    s.spawn(&UnitSpec {
                        controller: Controller::Ai,
                        ..UnitSpec::trooper(0, Vec3::new(12., 0., 70.))
                    })
                    .unwrap(),
                )
            } else {
                None
            };
            let human = s
                .spawn(&UnitSpec::player(0, Vec3::new(20., 0., 0.)))
                .unwrap();
            let end = if claim_delay { 150 } else { 30 };
            while s.tick < end {
                s.step(&[PadState::default()]);
            }
            assert!(!s.unit(p).unwrap().ai.ride.unwrap().pickup.unwrap().complete);
            if let Some(b) = boarder {
                let r = s.unit(b).unwrap().ai.ride.unwrap();
                assert_eq!(r.board, Some(vh));
                assert!(r.maintenance.is_some());
                // A real death ends the maintained claim, without changing clocks.
                s.kill_environment(b, crate::sim::DeathCause::Fall);
            } else {
                s.unit_mut(human).unwrap().soldier.pos =
                    s.unit(p).unwrap().soldier.pos + Vec3::X * 8.;
            }
            let mut departed = None;
            for n in 0..80 {
                let tick = s.tick;
                s.step(&[PadState {
                    buttons: if n == 0 && !claim_delay {
                        buttons::SQUAD_MOVE_OUT
                    } else {
                        0
                    },
                    ..PadState::default()
                }]);
                let moving = s.unit(p).unwrap().mount.is_some() && locomotion(&s, p) > 0.;
                let first = moving && departed.is_none();
                if first {
                    departed = Some(tick);
                }
                let expected = first
                    && crate::ai::unit_unit(p.index, tick, 0, crate::ai::SALT_NEED_GUNNER) < 0.3;
                assert_eq!(
                    count(&s, p, Call::NeedGunner),
                    usize::from(expected),
                    "first departure only"
                );
            }
            let dep = departed.expect("release/dead claimant permits route");
            assert!(if claim_delay { dep >= 150 } else { dep < 121 });
            let w = s.unit(p).unwrap().ai.ride.unwrap().pickup.unwrap();
            assert!(w.departed);
            assert_eq!(w.seated, 0, "no second wait or restarted clocks");
            if !claim_delay {
                assert!(w.released.is_some(), "actual WPOKE consumed");
            }
        }
    }
}

#[test]
fn departure_uses_current_three_dimensional_members_and_claim_arbitration() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for first in [false, true] {
            for high in [false, true] {
                let (mut s, p, a, vh) = departure_scene(kind, first);
                let b = s
                    .spawn(&UnitSpec {
                        controller: Controller::Ai,
                        ..UnitSpec::trooper(0, Vec3::new(500., 0., 500.))
                    })
                    .unwrap();
                s.unit_mut(b).unwrap().ai.next_think = s.unit(a).unwrap().ai.next_think;
                let departure = s.tick + 121;
                while s.tick < departure {
                    s.step(&[]);
                }
                let origin = s.unit(p).unwrap().soldier.pos;
                s.unit_mut(a).unwrap().soldier.pos =
                    origin + Vec3::new(0., if high { 40. } else { 0. }, -20.);
                s.unit_mut(b).unwrap().soldier.pos = origin + Vec3::new(0., 0., -25.);
                s.step(&[]);
                assert_eq!(count(&s, p, Call::NeedGunner), 1);
                assert_eq!(count(&s, a, Call::Yes), usize::from(!high));
                assert_eq!(count(&s, b, Call::Yes), 1);
                assert_eq!(
                    s.unit(a).unwrap().ai.pending_board,
                    (!high).then_some((a, vh))
                );
                assert_eq!(s.unit(b).unwrap().ai.pending_board, Some((b, vh)));
                let due_a = s.unit(a).unwrap().ai.next_think;
                let due_b = s.unit(b).unwrap().ai.next_think;
                let (earlier, other, due) = if !high && (due_a, a.index) < (due_b, b.index) {
                    (a, b, due_a)
                } else {
                    (b, a, due_b)
                };
                while s.tick <= due_a.max(due_b) + 1 {
                    s.step(&[]);
                }
                assert_eq!(s.unit(earlier).unwrap().ai.ride.unwrap().board, Some(vh));
                assert_eq!(
                    s.unit(earlier).unwrap().ai.ride.unwrap().since,
                    due,
                    "original scheduled arbitration clock"
                );
                assert_eq!(
                    s.unit(other).unwrap().ai.ride.and_then(|r| r.board),
                    (!high).then_some(vh),
                    "two source slots include both on-foot answerers"
                );
            }
        }
    }
}
