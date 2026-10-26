//! WGCAL actual boarding and public-entry wait-start timing witnesses.
use super::*;
use crate::sim::{Call, Event};
fn scene(kind: VehicleKind, phase: u64, mode: u8) -> bool {
    let (mut s, p, v) = pickup_world(kind);
    s.ground_rules = if mode == 2 {
        GroundRules::Bf2
    } else {
        GroundRules::Bf1
    };
    if mode != 3 {
        arm_gunner_seat(&mut s);
    }
    s.cps[0].def.pos = Vec3::new(0., 0., -90.);
    s.cps[0].def.capture = Region::sphere(s.cps[0].def.pos, 8.);
    s.unit_mut(p).unwrap().ai.next_think = phase;
    let receiver = s
        .spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(0, Vec3::new(12., 0., -20.))
        })
        .unwrap();
    if mode == 1 || mode == 4 {
        assert!(s.enter_vehicle(p, v, 0));
    }
    if mode == 4 {
        let h = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
        assert!(s.enter_vehicle(h, v, 1));
    }
    let (mut actual, mut expected, mut answers, mut expected_answers) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut entry, mut start) = (s.unit(p).unwrap().mount.map(|_| s.tick), None);
    for _ in 0..90 {
        let tick = s.tick;
        let was_departed = s
            .unit(p)
            .unwrap()
            .ai
            .ride
            .and_then(|r| r.pickup)
            .is_some_and(|w| w.departed);
        s.step(&[]);
        let u = s.unit(p).unwrap();
        if u.mount.is_some() {
            entry.get_or_insert(tick);
        }
        if u.ai
            .ride
            .and_then(|r| r.pickup)
            .is_some_and(|w| w.started == Some(tick))
        {
            start = Some(tick);
            if mode != 2
                && crate::ai::unit_unit(p.index, tick, 0, crate::ai::SALT_NEED_GUNNER) < 0.3
            {
                expected.push(tick);
            }
        }
        if mode != 2
            && !was_departed
            && u.ai.ride.and_then(|r| r.pickup).is_some_and(|w| w.departed)
            && crate::ai::unit_unit(p.index, tick, 0, crate::ai::SALT_NEED_GUNNER) < 0.3
        {
            expected.push(tick);
        }
        let recipient = s.unit(receiver).unwrap();
        if recipient.mount.is_none()
            && recipient.soldier.pos.distance_squared(u.soldier.pos) <= 30. * 30.
        {
            expected_answers.extend(expected.iter().filter(|&&t| t == tick).copied());
        }
        for e in &s.events {
            if matches!(e,Event::Call{unit,call:Call::NeedGunner} if *unit==p) {
                actual.push(tick);
            }
            if matches!(e,Event::Call{unit,call:Call::Yes} if *unit==receiver) {
                answers.push(tick);
            }
        }
    }
    assert!(entry.is_some(), "natural pilot");
    eprintln!("WAITCALL {kind:?} phase{phase} mode{mode} entry{entry:?} start{start:?} actual{actual:?} expected{expected:?} answers{answers:?}");
    actual == expected && answers == expected_answers
}
#[test]
fn wgcal_wait_calls_at_start() {
    let mut failures = 0;
    for k in [VehicleKind::Hover, VehicleKind::Walker] {
        for phase in [0, 1, 2, 9, 10, 11, 20] {
            for mode in [0, 1] {
                failures += usize::from(!scene(k, phase, mode));
            }
        }
    }
    assert_eq!(
        failures, 0,
        "initial call must use wait-start moment, not seat entry"
    );
}
#[test]
fn wgcal_wait_call_controls() {
    for k in [VehicleKind::Hover, VehicleKind::Walker] {
        for phase in [0, 10, 20] {
            for mode in [2, 3, 4] {
                assert!(scene(k, phase, mode));
            }
        }
    }
}

// Keep callers and receivers in either index order. A public entry isolates
// delayed first thinks from approach/claim policy, without injecting a wait.
fn public_scene(
    kind: VehicleKind,
    receiver_first: bool,
    phase: u64,
) -> (Sim, UnitHandle, UnitHandle, VehicleHandle) {
    let (mut s, first, vh) = pickup_world(kind);
    s.ground_rules = GroundRules::Bf1;
    arm_gunner_seat(&mut s);
    s.cps[0].def.pos = Vec3::new(0.0, 0.0, -90.0);
    s.cps[0].def.capture = Region::sphere(s.cps[0].def.pos, 8.0);
    let other = s
        .spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(0, Vec3::new(12.0, 0.0, -20.0))
        })
        .unwrap();
    let (p, a) = if receiver_first {
        (other, first)
    } else {
        (first, other)
    };
    s.unit_mut(a).unwrap().soldier.pos = Vec3::new(12.0, 0.0, -20.0);
    s.unit_mut(p).unwrap().ai.next_think = phase;
    s.unit_mut(a).unwrap().ai.next_think = phase; // same-tick scratch write-back
    assert!(s.enter_vehicle(p, vh, 0));
    (s, p, a, vh)
}

fn calls(s: &Sim, h: UnitHandle, call: Call) -> usize {
    s.events
        .iter()
        .filter(|e| matches!(e, Event::Call{unit, call:c} if *unit==h && *c==call))
        .count()
}

#[test]
fn wgcal_index_order_deferred_delivery_and_full_seat_departure() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for first in [false, true] {
            // Pick a successful existing salted roll separately for each issuer.
            let (probe, p, _, _) = public_scene(kind, first, 0);
            let phase = (9..50)
                .find(|&t| {
                    t % 10 <= 7
                        && crate::ai::unit_unit(p.index, t, 0, crate::ai::SALT_NEED_GUNNER) < 0.3
                })
                .unwrap();
            drop(probe);
            let (mut s, p, a, vh) = public_scene(kind, first, phase);
            let mut total = 0;
            for _ in 0..=phase {
                let tick = s.tick;
                s.step(&[]);
                total += calls(&s, p, Call::NeedGunner);
                assert_eq!(calls(&s, p, Call::NeedGunner), usize::from(tick == phase));
                assert_eq!(calls(&s, a, Call::Yes), usize::from(tick == phase));
            }
            assert_eq!(total, 1);
            assert_eq!(
                s.unit(p).unwrap().ai.ride.unwrap().pickup.unwrap().started,
                Some(phase)
            );
            assert_eq!(
                s.unit(a).unwrap().ai.pending_board,
                Some((a, vh)),
                "all write-backs precede publication"
            );
            assert!(
                s.unit(a).unwrap().ai.ride.is_none_or(|r| r.board.is_none()),
                "no immediate consumption"
            );
            let due = s.unit(a).unwrap().ai.next_think;
            assert!(due > s.tick);
            while s.tick < due {
                s.step(&[]);
                assert_eq!(calls(&s, p, Call::NeedGunner), 0, "no reroll");
                assert_eq!(s.unit(a).unwrap().ai.pending_board, Some((a, vh)));
                assert!(s.unit(a).unwrap().ai.ride.is_none_or(|r| r.board.is_none()));
                assert!(s.unit(a).unwrap().mount.is_none());
            }
            s.step(&[]);
            assert_eq!(s.unit(a).unwrap().ai.pending_board, None);
            assert_eq!(s.unit(a).unwrap().ai.ride.unwrap().board, Some(vh));
            let mut boarded = false;
            let mut departed = false;
            for _ in 0..160 {
                s.step(&[]);
                assert_eq!(
                    calls(&s, p, Call::NeedGunner),
                    0,
                    "no duplicate later think"
                );
                if s.unit(a)
                    .unwrap()
                    .mount
                    .is_some_and(|m| m.vehicle == vh && m.seat == 1)
                {
                    boarded = true;
                }
                if boarded && locomotion(&s, p) > 0.0 {
                    departed = true;
                }
            }
            assert!(
                boarded && departed,
                "Yes -> next-think claim -> boarding -> departure {kind:?}/{first}"
            );
        }
    }
}

#[test]
fn wgcal_issue_time_membership_is_three_dimensional() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for first in [false, true] {
            for entering in [false, true] {
                let (mut s, p, a, vh) = public_scene(kind, first, 0);
                let phase = (10..50)
                    .find(|&t| {
                        crate::ai::unit_unit(p.index, t, 0, crate::ai::SALT_NEED_GUNNER) < 0.3
                    })
                    .unwrap();
                s.unit_mut(p).unwrap().ai.next_think = phase;
                s.unit_mut(a).unwrap().ai.next_think = phase;
                let origin = s.unit(p).unwrap().soldier.pos;
                s.unit_mut(a).unwrap().soldier.pos =
                    origin + Vec3::new(0.0, if entering { 40.0 } else { 10.0 }, -20.0);
                while s.tick < phase {
                    s.step(&[]);
                    assert_eq!(calls(&s, p, Call::NeedGunner), 0);
                }
                // Movement between entry and start changes this issue's list.
                let at = s.unit(p).unwrap().soldier.pos;
                s.unit_mut(a).unwrap().soldier.pos =
                    at + Vec3::new(0.0, if entering { 10.0 } else { 40.0 }, -20.0);
                s.step(&[]);
                assert_eq!(calls(&s, p, Call::NeedGunner), 1);
                assert_eq!(calls(&s, a, Call::Yes), usize::from(entering));
                assert_eq!(
                    s.unit(a).unwrap().ai.pending_board,
                    entering.then_some((a, vh))
                );
            }
        }
    }
}

#[test]
fn wgcal_failed_roll_and_delayed_think_never_retry() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for phase in [11, 200] {
            let (mut s, p, _, _) = public_scene(kind, false, phase);
            assert!(crate::ai::unit_unit(p.index, phase, 0, crate::ai::SALT_NEED_GUNNER) >= 0.3);
            let rng = s.rng.state();
            let lcg = s.lcg.state();
            let (mut started, mut completed) = (false, false);
            for _ in 0..260 {
                let tick = s.tick;
                s.step(&[]);
                if let Some(w) = s.unit(p).unwrap().ai.ride.and_then(|r| r.pickup) {
                    assert_eq!(w.seated, 0);
                    if w.started == Some(tick) {
                        started = true;
                        assert_eq!(tick, phase);
                    }
                    completed |= w.complete;
                }
                // With no first think/goal, expiry may request exit in this
                // same update; seat cleanup then removes the completed wait.
                completed |= tick > 120 && s.unit(p).unwrap().mount.is_none();
                assert_eq!(
                    calls(&s, p, Call::NeedGunner),
                    usize::from(phase == 11 && tick == 121),
                    "failed initial roll stays spent; WGOUT departure is independent"
                );
            }
            assert_eq!(started, phase <= 120, "expired wait cannot start late");
            assert!(completed, "{kind:?} phase{phase}: wait ended");
            assert_eq!(s.rng.state(), rng);
            assert_eq!(s.lcg.state(), lcg);
        }
    }
}

fn observe_tenure(s: &mut Sim, p: UnitHandle, vh: VehicleHandle) {
    let seated = s.tick;
    let mut seen = false;
    for _ in 0..40 {
        let tick = s.tick;
        s.step(&[]);
        let w = s.unit(p).unwrap().ai.ride.unwrap().pickup.unwrap();
        assert_eq!(w.vehicle, vh);
        assert_eq!(w.seated, seated, "tenure clock must not restart");
        let transition = w.started == Some(tick);
        let success =
            transition && crate::ai::unit_unit(p.index, tick, 0, crate::ai::SALT_NEED_GUNNER) < 0.3;
        assert_eq!(
            calls(s, p, Call::NeedGunner),
            usize::from(success),
            "exactly one attempt at this tenure's start"
        );
        if transition {
            assert!(!seen);
            seen = true;
        }
    }
    assert!(seen, "natural scheduled think must start new tenure");
}

#[test]
fn wgcal_exit_reentry_handover_and_vehicle_slot_reuse() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        let (mut s, p, a, mut vh) = public_scene(kind, false, 10);
        // Keep the free gun while testing tenure transitions; no board injection.
        s.unit_mut(a).unwrap().controller = Controller::Player { slot: 1 };
        observe_tenure(&mut s, p, vh);
        s.exit_vehicle(p);
        assert!(s.unit(p).unwrap().ai.ride.unwrap().pickup.is_none());
        assert!(s.enter_vehicle(p, vh, 0));
        observe_tenure(&mut s, p, vh);
        let human = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
        assert!(s.enter_vehicle(human, vh, 0));
        assert!(s.unit(p).unwrap().ai.ride.unwrap().pickup.is_none());
        s.exit_vehicle(human);
        assert!(s.enter_vehicle(p, vh, 0));
        observe_tenure(&mut s, p, vh);
        let old = vh;
        s.remove_vehicle(vh);
        assert!(s.unit(p).unwrap().ai.ride.unwrap().pickup.is_none());
        vh = s.spawn_vehicle(&VehicleSpec {
            kind,
            class: 0,
            home_team: None,
            neutral_when_empty: true,
            spawn_pad: None,
            pos: Vec3::new(12.0, 2.0, 0.0),
            yaw: 0.0,
            max_health: 1000.0,
            health_type: HealthType::Vehicle,
            seats: vec![SeatKind::Pilot, SeatKind::Gunner(0)],
        });
        assert_eq!(old.index, vh.index);
        assert_ne!(old.gen, vh.gen);
        assert!(s.enter_vehicle(p, vh, 0));
        observe_tenure(&mut s, p, vh);
    }
}

#[test]
fn wgcal_bailout_preempts_start_and_player_orders_keep_priority() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        let (mut s, p, _, vh) = public_scene(kind, false, 10);
        s.vehicle_mut(vh).unwrap().health.cur = 100.0;
        s.unit_mut(p).unwrap().ai.ride.as_mut().unwrap().bail_draw = Some(0);
        for _ in 0..11 {
            s.step(&[]);
            assert_eq!(calls(&s, p, Call::NeedGunner), 0);
        }
        assert!(s.unit(p).unwrap().mount.is_none());
        let (mut s, p, a, vh) = public_scene(kind, false, 10);
        let human = s
            .spawn(&UnitSpec::player(0, Vec3::new(12.0, 0.0, -20.0)))
            .unwrap();
        assert!(s.issue_command(0, crate::squad::CommandKind::Follow(human)) > 0);
        for _ in 0..11 {
            s.step(&[]);
        }
        assert_eq!(calls(&s, p, Call::NeedGunner), 1);
        assert_eq!(
            calls(&s, a, Call::Yes),
            1,
            "issue-time acknowledgement retained"
        );
        assert_eq!(
            s.unit(a).unwrap().ai.pending_board,
            Some((a, vh)),
            "WGORD retains the earlier player goal behind explicit boarding"
        );
        assert!(s.unit(a).unwrap().command.is_some());
    }
}

#[test]
fn wgcal_foot_bf2_controls_and_affected_unarmed_traces() {
    for case in 0..3 {
        let (mut s, p, vh) = pickup_world(VehicleKind::Hover);
        match case {
            0 => s.remove_vehicle(vh),
            1 => {
                s.ground_rules = GroundRules::Bf2;
                arm_gunner_seat(&mut s);
            }
            _ => s.ground_rules = GroundRules::Bf1,
        }
        s.unit_mut(p).unwrap().ai.next_think = 9;
        let mut trace = 0u64;
        for _ in 0..180 {
            s.step(&[]);
            trace = trace.rotate_left(7) ^ s.hash();
        }
        eprintln!(
            "WGCAL_NULL {case}: {trace:016x} rng={} lcg={}",
            s.rng.state(),
            s.lcg.state()
        );
    }
}

#[test]
fn wgcal_non_ground_entry_pass_keeps_its_existing_call() {
    // Dispatch control only: retain the old non-ground entry pass, using its
    // ordinary queued-use request. Ground timing scenes above use natural or
    // public boarding and never manufacture a request/claim/transition.
    for kind in [VehicleKind::Flyer, VehicleKind::Turret] {
        let (mut s, p, vh) = pickup_world(VehicleKind::Hover);
        s.ground_rules = GroundRules::Bf1;
        arm_gunner_seat(&mut s);
        s.vehicle_mut(vh).unwrap().kind = kind;
        if kind == VehicleKind::Turret {
            let setup = s.seat_setup[0].as_mut().unwrap();
            setup.guns[0] = setup.guns[1].clone();
        }
        s.unit_mut(p).unwrap().soldier.pos = Vec3::new(12.0, 0.0, 0.0);
        while s.tick < 10 {
            s.step(&[]);
        }
        if s.unit(p).unwrap().mount.is_some() {
            s.exit_vehicle(p);
        }
        s.unit_mut(p).unwrap().soldier.pos = Vec3::new(12.0, 0.0, 0.0);
        s.unit_mut(p).unwrap().ai.ride = Some(RideState {
            board: Some(vh),
            use_req: true,
            ..Default::default()
        });
        s.events.clear();
        s.ai_use_pass();
        assert!(s
            .unit(p)
            .unwrap()
            .mount
            .is_some_and(|m| m.vehicle == vh && m.seat == 0));
        assert_eq!(
            calls(&s, p, Call::NeedGunner),
            1,
            "non-ground entry call unchanged"
        );
    }
}
