//! WGSEAT actual-step seat-independent ground call sources.
use super::*;
#[test]
fn wgseat_actual_callers_64_scenes() {
    use crate::sim::{Call, Event};
    let mut faults = 0;
    let mut controls = 0;
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for bf2 in [false, true] {
            for mode in 0..4 {
                // armed empty, armed full, unarmed pair, unarmed single
                for (natural, phase) in [(true, 9), (true, 10), (false, 10), (false, 11)] {
                    let (mut s, p, v) = pickup_world(kind);
                    s.ground_rules = if bf2 {
                        GroundRules::Bf2
                    } else {
                        GroundRules::Bf1
                    };
                    if mode < 2 {
                        arm_gunner_seat(&mut s);
                    }
                    if mode == 3 {
                        s.vehicle_mut(v).unwrap().seats.truncate(1);
                        s.seat_setup[0].as_mut().unwrap().guns.truncate(1);
                    }
                    s.unit_mut(p).unwrap().ai.next_think = phase;
                    if !natural {
                        s.unit_mut(p).unwrap().controller = Controller::Player { slot: 1 };
                        while s.tick < phase {
                            s.step(&[]);
                        }
                        s.unit_mut(p).unwrap().controller = Controller::Ai;
                        assert!(s.enter_vehicle(p, v, 0));
                    }
                    if mode == 1 {
                        let h = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
                        assert!(s.enter_vehicle(h, v, 1));
                    }
                    let a = s
                        .spawn(&UnitSpec {
                            controller: Controller::Ai,
                            ..UnitSpec::trooper(0, Vec3::new(12., 0., -20.))
                        })
                        .unwrap();
                    s.unit_mut(a).unwrap().ai.next_think = 500;
                    let mut entry = (!natural).then_some(phase);
                    let (mut start, mut departure) = (None, None);
                    let (mut actual, mut expected, mut yes) = (Vec::new(), Vec::new(), Vec::new());
                    let mut hashes = Vec::new();
                    for _ in 0..230 {
                        let t = s.tick;
                        s.step(&[]);
                        hashes.push(s.hash());
                        let u = s.unit(p).unwrap();
                        if let Some(m) = u.mount {
                            entry.get_or_insert(t);
                            assert_eq!(m.vehicle, v);
                            assert_eq!(m.seat, 0);
                        }
                        if u.ai
                            .ride
                            .and_then(|r| r.pickup)
                            .is_some_and(|w| w.started == Some(t))
                        {
                            start = Some(t);
                        }
                        let leaving =
                            u.mount.is_some() && locomotion(&s, p) > 0. && departure.is_none();
                        if leaving {
                            departure = Some(t);
                        }
                        if !bf2
                            && crate::ai::unit_unit(p.index, t, 0, crate::ai::SALT_NEED_GUNNER)
                                < 0.3
                        {
                            if start == Some(t) {
                                expected.push(t);
                            }
                            if leaving {
                                expected.push(t);
                            }
                        }
                        for e in &s.events {
                            if matches!(e,Event::Call{unit,call:Call::NeedGunner} if *unit==p) {
                                actual.push(t);
                            }
                            if matches!(e,Event::Call{unit,call:Call::Yes} if *unit==a) {
                                yes.push(t);
                            }
                        }
                    }
                    assert!(
                        departure.is_some(),
                        "reachable {kind:?} {bf2} {mode} {natural}"
                    );
                    assert_eq!(entry, Some(phase));
                    if !bf2 {
                        assert_eq!(start, Some(if natural { phase + 1 } else { phase }));
                        let route = if mode == 1 || mode == 3 {
                            start.unwrap()
                        } else {
                            phase + 121
                        };
                        assert_eq!(departure, Some(route));
                    }
                    eprintln!("WGSEAT_TRACE {kind:?} bf2{bf2} mode{mode} natural{natural} phase{phase} hashes={hashes:?} rng={} lcg={}",s.rng.state(),s.lcg.state());
                    let ok = actual == expected && yes == expected;
                    if !ok {
                        faults += 1;
                    } else {
                        controls += 1;
                    }
                    eprintln!("SEATCALL {kind:?} bf2{bf2} mode{mode} natural{natural} phase{phase} pilot{p:?} vehicle{v:?} receiver{a:?} entry{entry:?} start{start:?} depart{departure:?} actual{actual:?} yes{yes:?} expected{expected:?} ok{ok}");
                }
            }
        }
    }
    eprintln!("SEATCALL TOTAL faults{faults} controls{controls}");
    assert_eq!(
        faults, 0,
        "BF1 issue attempts must not depend on vehicle seat availability/weapon"
    );
}

#[test]
fn wgseat_unavailable_calls_defer_without_granting_a_seat() {
    use crate::sim::{Call, Event};
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for receiver_first in [false, true] {
            for mode in 0..3 {
                // full armed, nonboardable unarmed pair, single
                for delayed in [false, true] {
                    let (mut s, first, vh) = pickup_world(kind);
                    s.ground_rules = GroundRules::Bf1;
                    let other = s
                        .spawn(&UnitSpec {
                            controller: Controller::Ai,
                            ..UnitSpec::trooper(0, Vec3::new(12., 0., -20.))
                        })
                        .unwrap();
                    let (p, a) = if receiver_first {
                        (other, first)
                    } else {
                        (first, other)
                    };
                    s.unit_mut(a).unwrap().soldier.pos = Vec3::new(12., 0., -20.);
                    if mode == 0 {
                        arm_gunner_seat(&mut s);
                    }
                    if mode == 1 {
                        s.seat_setup[0].as_mut().unwrap().boardable[1] = false;
                    }
                    if mode == 2 {
                        s.vehicle_mut(vh).unwrap().seats.truncate(1);
                        s.seat_setup[0].as_mut().unwrap().guns.truncate(1);
                    }
                    let phase = (10..50)
                        .find(|&t| {
                            crate::ai::unit_unit(p.index, t, 0, crate::ai::SALT_NEED_GUNNER) < 0.3
                        })
                        .unwrap();
                    s.unit_mut(p).unwrap().controller = Controller::Player { slot: 1 };
                    s.unit_mut(a).unwrap().ai.next_think = 500;
                    while s.tick < phase {
                        s.step(&[]);
                    }
                    s.unit_mut(p).unwrap().controller = Controller::Ai;
                    s.unit_mut(p).unwrap().ai.next_think = phase;
                    assert!(s.enter_vehicle(p, vh, 0));
                    let occupant = if mode == 0 {
                        let h = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
                        assert!(s.enter_vehicle(h, vh, 1));
                        Some(h)
                    } else {
                        None
                    };
                    // A live player issuer keeps Hold valid through later thinks.
                    if occupant.is_none() {
                        s.spawn(&UnitSpec::player(0, Vec3::new(12., 0., -20.)))
                            .unwrap();
                    }
                    let extra = s
                        .spawn(&UnitSpec {
                            controller: Controller::Ai,
                            ..UnitSpec::trooper(0, Vec3::new(12., 0., -22.))
                        })
                        .unwrap();
                    let ordered = s
                        .spawn(&UnitSpec {
                            controller: Controller::Ai,
                            ..UnitSpec::trooper(0, Vec3::new(12., 0., -24.))
                        })
                        .unwrap();
                    let command = crate::squad::Command {
                        kind: crate::squad::CommandKind::Hold(Vec3::new(12., 0., -24.)),
                        issuer: 0,
                        issued_tick: s.tick,
                    };
                    s.unit_mut(ordered).unwrap().command = Some(command);
                    s.unit_mut(extra).unwrap().ai.next_think = phase + 7;
                    s.unit_mut(ordered).unwrap().ai.next_think = phase + 7;
                    s.unit_mut(a).unwrap().ai.next_think = if delayed { phase + 7 } else { phase };
                    s.step(&[]);
                    assert!(s
                        .events
                        .iter()
                        .any(|e| matches!(e,Event::Call{unit,call:Call::Yes} if *unit==a)));
                    assert_eq!(s.unit(a).unwrap().ai.pending_board, Some((a, vh)));
                    assert!(s.unit(a).unwrap().ai.ride.is_none_or(|r| r.board.is_none()));
                    assert_eq!(s.unit(extra).unwrap().ai.pending_board, Some((extra, vh)));
                    assert_eq!(
                        s.unit(ordered).unwrap().ai.pending_board,
                        Some((ordered, vh))
                    );
                    assert!(s
                        .events
                        .iter()
                        .any(|e| matches!(e,Event::Call{unit,call:Call::Yes} if *unit==ordered)));
                    let due = s.unit(a).unwrap().ai.next_think;
                    assert!(due >= s.tick);
                    while s.tick < due {
                        s.step(&[]);
                        assert_eq!(s.unit(a).unwrap().ai.pending_board, Some((a, vh)));
                    }
                    s.step(&[]);
                    let u = s.unit(a).unwrap();
                    assert!(u.ai.pending_board.is_none(), "consumed at scheduled think");
                    assert!(
                        u.ai.ride.is_none_or(|r| r.board.is_none()),
                        "admission refuses full or nonboardable secondary seats"
                    );
                    assert!(u.mount.is_none());
                    assert_eq!(
                        s.vehicle(vh).unwrap().seats.len(),
                        if mode == 2 { 1 } else { 2 }
                    );
                    if let Some(h) = occupant {
                        assert_eq!(s.vehicle(vh).unwrap().seats[1].occupant, Some(h));
                    }
                    assert_eq!(s.unit(p).unwrap().mount.unwrap().vehicle, vh);
                    assert_eq!(s.unit(ordered).unwrap().command, Some(command));
                    while s.tick <= phase + 18 {
                        s.step(&[]);
                    }
                    for h in [a, extra, ordered] {
                        assert!(s.unit(h).unwrap().mount.is_none());
                        assert!(s.unit(h).unwrap().ai.ride.is_none_or(|r| r.board.is_none()));
                    }
                    assert!(s.unit(extra).unwrap().ai.pending_board.is_none());
                    assert_eq!(s.unit(ordered).unwrap().command, Some(command));
                }
            }
        }
    }
}

// Separate principals discriminate publication eligibility from creation of
// the routed-departure source; the full matrix also checks independent draws.
#[test]
fn wgseat_wait_start_has_no_ground_gun_seat_gate() {
    use crate::sim::{Call, Event};
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        let (mut s, p, vh) = pickup_world(kind);
        s.ground_rules = GroundRules::Bf1;
        s.unit_mut(p).unwrap().ai.next_think = 9;
        while s.tick < 10 {
            s.step(&[]);
        }
        s.step(&[]);
        let u = s.unit(p).unwrap();
        assert_eq!(u.mount.unwrap().vehicle, vh);
        assert_eq!(u.ai.ride.unwrap().pickup.unwrap().started, Some(10));
        assert_eq!(
            s.events
                .iter()
                .filter(|e| matches!(e,Event::Call{unit,call:Call::NeedGunner} if *unit==p))
                .count(),
            1
        );
    }
}

#[test]
fn wgseat_routed_departure_has_no_secondary_armament_gate() {
    use crate::sim::{Call, Event};
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        let (mut s, p, vh) = pickup_world(kind);
        s.ground_rules = GroundRules::Bf1;
        s.unit_mut(p).unwrap().ai.next_think = 9;
        while s.tick < 130 {
            s.step(&[]);
        }
        s.step(&[]);
        let u = s.unit(p).unwrap();
        assert_eq!(u.mount.unwrap().vehicle, vh);
        assert!(u.ai.ride.unwrap().pickup.unwrap().departed);
        assert!(locomotion(&s, p) > 0.);
        assert_eq!(
            s.events
                .iter()
                .filter(|e| matches!(e,Event::Call{unit,call:Call::NeedGunner} if *unit==p))
                .count(),
            1
        );
        for _ in 0..80 {
            s.step(&[]);
            assert!(!s
                .events
                .iter()
                .any(|e| matches!(e,Event::Call{unit,call:Call::NeedGunner} if *unit==p)));
        }
    }
}

#[test]
fn wgseat_non_ground_keeps_free_armed_seat_gate() {
    use crate::sim::{Call, Event};
    for kind in [VehicleKind::Flyer, VehicleKind::Turret] {
        for mode in 0..3 {
            let (mut s, p, vh) = pickup_world(VehicleKind::Hover);
            s.ground_rules = GroundRules::Bf1;
            s.vehicle_mut(vh).unwrap().kind = kind;
            if mode != 0 {
                arm_gunner_seat(&mut s);
            }
            if mode == 2 {
                let h = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
                assert!(s.seat_unit(h, vh, 1));
            }
            s.tick = 10;
            s.need_gunner_call(p, vh);
            assert_eq!(
                s.events
                    .iter()
                    .filter(|e| matches!(e,Event::Call{unit,call:Call::NeedGunner} if *unit==p))
                    .count(),
                usize::from(mode == 1)
            );
        }
    }
}
