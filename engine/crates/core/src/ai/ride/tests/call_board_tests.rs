//! GBOARD: natural issue/think/entry witnesses, controls and delivery invariants.
use super::*;
use crate::sim::{Call, Event};

fn scene(
    kind: VehicleKind,
    distance: f32,
    case: u8,
) -> (Sim, UnitHandle, UnitHandle, VehicleHandle) {
    let (mut s, pilot, vh) = pickup_world(kind);
    s.ground_rules = if case == 2 {
        GroundRules::Bf2
    } else {
        GroundRules::Bf1
    };
    if case != 3 {
        arm_gunner_seat(&mut s);
    }
    if case != 5 {
        s.cps[0].def.pos = Vec3::new(0.0, 0.0, -90.0);
        s.cps[0].def.capture = Region::sphere(s.cps[0].def.pos, 8.0);
    }
    // WGCAL: phase 9 boards at 9 and starts/calls at 10. Phase 10
    // boards at 10 but its start roll at 11 fails (formerly entry 0 failed).
    s.unit_mut(pilot).unwrap().ai.next_think = match case {
        1 => 10,
        5 => 0,
        _ => 9,
    };
    let receiver = s
        .spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(0, Vec3::new(12.0, 0.0, -distance))
        })
        .unwrap();
    (s, pilot, receiver, vh)
}

fn natural(kind: VehicleKind, distance: f32, case: u8) -> bool {
    let (mut s, pilot, receiver, vh) = scene(kind, distance, case);
    let (mut calls, mut yes) = (0, 0);
    let (mut issue, mut claim, mut entry, mut departure) = (None, None, None, None);
    let mut pilot_entry = None;
    let mut delivery_think = None;
    let mut since = None;
    let mut full = false;
    for _ in 0..150 {
        let tick = s.tick;
        let thinking = tick >= s.unit(receiver).unwrap().ai.next_think;
        s.step(&[]);
        for e in &s.events {
            if matches!(e, Event::Call{unit, call:Call::NeedGunner} if *unit==pilot) {
                calls += 1;
                issue = Some(tick);
                assert!(
                    s.unit(receiver)
                        .unwrap()
                        .ai
                        .ride
                        .is_none_or(|r| r.board.is_none()),
                    "no independent claim before call delivery"
                );
            }
            if matches!(e, Event::Call{unit, call:Call::Yes} if *unit==receiver) {
                yes += 1;
            }
        }
        if s.unit(pilot).unwrap().mount.is_some() {
            pilot_entry.get_or_insert(tick);
        }
        let u = s.unit(receiver).unwrap();
        if case == 0 && issue.is_some_and(|t| tick > t) && delivery_think.is_none() && thinking {
            delivery_think = Some(tick);
        }
        if let Some(r) = u.ai.ride.filter(|r| r.board == Some(vh)) {
            if claim.is_none() && case == 0 {
                assert_eq!(Some(tick), delivery_think, "claim at next scheduled think");
                assert!(issue.is_some_and(|t| tick > t));
            }
            claim.get_or_insert(tick);
            assert_eq!(
                *since.get_or_insert(r.since),
                r.since,
                "original claim clock"
            );
        }
        if u.mount.is_some_and(|m| m.vehicle == vh && m.seat == 1) {
            entry.get_or_insert(tick);
            full = true;
        }
        if full && locomotion(&s, pilot) > 0.0 {
            departure.get_or_insert(tick);
        }
    }
    assert_eq!(
        calls,
        usize::from(case == 0 || case == 4),
        "call setup {kind:?}/{case}"
    );
    assert_eq!(yes, usize::from(case == 0), "answer setup {kind:?}/{case}");
    assert_eq!(
        s.unit(pilot).unwrap().mount.unwrap().vehicle,
        vh,
        "natural pilot"
    );
    eprintln!("GBOARD {kind:?}/{distance}/{case}: vehicle={vh:?} pilot={pilot_entry:?} issue={issue:?} think={delivery_think:?} claim={claim:?} entry={entry:?} departure={departure:?}");
    if case == 0 || case == 5 {
        entry.is_some() && departure.is_some()
    } else {
        entry.is_none() && claim.is_none()
    }
}

#[test]
fn gboard_four_natural_calls_board_at_next_think() {
    let mut failures = 0;
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for distance in [20.0, 25.0] {
            failures += usize::from(!natural(kind, distance, 0));
        }
    }
    assert_eq!(
        failures, 0,
        "four supported call answerers must board and depart"
    );
}

#[test]
fn gboard_ten_call_and_autonomous_controls() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for case in 1..=5 {
            assert!(
                natural(kind, if case == 4 { 35.0 } else { 20.0 }, case),
                "{kind:?}/{case}"
            );
        }
    }
}

fn issued(kind: VehicleKind) -> (Sim, UnitHandle, UnitHandle, VehicleHandle) {
    let (mut s, p, a, vh) = scene(kind, 20.0, 0);
    while s.tick <= 10 {
        s.step(&[]);
    }
    assert_eq!(s.unit(a).unwrap().ai.pending_board, Some((a, vh)));
    assert!(s
        .events
        .iter()
        .any(|e| matches!(e, Event::Call{unit,call:Call::Yes} if *unit==a)));
    (s, p, a, vh)
}

#[test]
fn gboard_no_action_before_think_and_no_extra_calls_or_random_draws() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        let (mut s, _, a, vh) = issued(kind);
        let (mut control, _, ca, _) = issued(kind);
        control.unit_mut(ca).unwrap().ai.pending_board = None;
        let due = s.unit(a).unwrap().ai.next_think;
        assert!(due > s.tick, "nonzero delivery delay");
        while s.tick < due {
            s.step(&[]);
            control.step(&[]);
            let u = s.unit(a).unwrap();
            let c = control.unit(ca).unwrap();
            assert_eq!(u.ai.pending_board, Some((a, vh)));
            assert!(u.ai.ride.is_none_or(|r| r.board.is_none() && !r.use_req));
            assert_eq!(
                u.soldier.pos, c.soldier.pos,
                "pending goal cannot move before think"
            );
            assert_eq!(s.events, control.events, "no extra acknowledgement");
            assert_eq!(s.rng.state(), control.rng.state());
        }
        s.step(&[]);
        control.step(&[]);
        let u = s.unit(a).unwrap();
        assert!(u.ai.pending_board.is_none());
        assert_eq!(u.ai.ride.unwrap().board, Some(vh));
        assert_eq!(u.ai.ride.unwrap().since, due);
        assert_eq!(s.events, control.events);
        assert_eq!(s.rng.state(), control.rng.state());
    }
}

#[test]
fn gboard_explicit_target_ignores_search_distance_cooldown_and_nearer_decoy() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        let (mut s, _, a, vh) = issued(kind);
        let at = s.unit(a).unwrap().soldier.pos;
        let decoy = s.spawn_vehicle(&VehicleSpec {
            kind,
            class: 0,
            home_team: None,
            neutral_when_empty: true,
            spawn_pad: None,
            pos: at + Vec3::X * 3.0,
            yaw: 0.0,
            max_health: 1000.0,
            health_type: HealthType::Vehicle,
            seats: vec![SeatKind::Pilot, SeatKind::Gunner(0)],
        });
        let ai = &mut s.unit_mut(a).unwrap().ai;
        ai.ride = Some(RideState {
            block_until: 10000,
            ..Default::default()
        });
        let due = ai.next_think;
        while s.tick <= due {
            s.step(&[]);
        }
        assert_ne!(vh, decoy);
        assert_eq!(s.unit(a).unwrap().ai.ride.unwrap().board, Some(vh));
        assert_eq!(s.unit(a).unwrap().ai.ride.unwrap().block_until, 10000);
    }
}

#[test]
fn gboard_repeat_delivery_preserves_both_same_and_other_live_claims() {
    for other in [false, true] {
        let (mut s, _, a, vh) = issued(VehicleKind::Hover);
        let mut st = s.unit(a).unwrap().ai;
        st.ride = Some(RideState {
            board: Some(vh),
            since: 1,
            ..Default::default()
        });
        if other {
            let live = s.spawn_vehicle(&VehicleSpec {
                kind: VehicleKind::Hover,
                class: 0,
                home_team: None,
                neutral_when_empty: true,
                spawn_pad: None,
                pos: Vec3::new(24.0, 2.0, 0.0),
                yaw: 0.0,
                max_health: 1000.0,
                health_type: HealthType::Vehicle,
                seats: vec![SeatKind::Pilot, SeatKind::Gunner(0)],
            });
            assert!(
                claim_seat(&s, a, live).is_some(),
                "other target is live and boardable"
            );
            st.ride.as_mut().unwrap().board = Some(live);
        }
        let old = st.ride;
        let rng = s.rng.state();
        for _ in 0..3 {
            st.pending_board = Some((a, vh));
            deliver_call(&s, a, s.unit(a).unwrap(), &mut st);
            assert_eq!(
                st.ride, old,
                "delivery cannot replace claim or restart clock"
            );
            assert!(st.pending_board.is_none());
        }
        assert_eq!(s.rng.state(), rng);
        s.unit_mut(a).unwrap().ai = st;
    }
}

#[test]
fn gboard_delivery_rejects_invalid_target_or_receiver_once() {
    for case in [
        "target_gen",
        "target_index",
        "destroyed",
        "full",
        "unarmed",
        "no_post",
        "ineligible",
        "turret",
        "flyer",
        "enemy",
        "owner_gen",
        "owner_index",
        "player",
        "mounted",
        "dead",
        "ordered",
    ] {
        let (mut s, p, a, vh) = issued(VehicleKind::Hover);
        let mut st = s.unit(a).unwrap().ai;
        st.target_cp = Some(0);
        match case {
            "target_gen" => st.pending_board.as_mut().unwrap().1.gen += 1,
            "target_index" => st.pending_board.as_mut().unwrap().1.index += 1,
            "destroyed" => s.remove_vehicle(vh),
            "full" => {
                let h = s.spawn(&UnitSpec::trooper(0, Vec3::ZERO)).unwrap();
                assert!(s.seat_unit(h, vh, 1));
            }
            "unarmed" => s.seat_setup[0].as_mut().unwrap().guns.clear(),
            "no_post" => st.target_cp = None,
            "ineligible" => s.seat_setup[0].as_mut().unwrap().boardable.fill(false),
            "turret" => s.vehicle_mut(vh).unwrap().kind = VehicleKind::Turret,
            "flyer" => s.vehicle_mut(vh).unwrap().kind = VehicleKind::Flyer,
            "enemy" => s.vehicle_mut(vh).unwrap().team = Some(1),
            "owner_gen" => st.pending_board.as_mut().unwrap().0.gen += 1,
            "owner_index" => st.pending_board.as_mut().unwrap().0.index = p.index,
            "player" => s.unit_mut(a).unwrap().controller = Controller::Player { slot: 0 },
            "mounted" => {
                assert!(s.seat_unit(a, vh, 1));
            }
            "dead" => {
                s.units[a.index as usize].alive = false;
            }
            "ordered" => {
                s.unit_mut(a).unwrap().command = Some(crate::squad::Command {
                    kind: crate::squad::CommandKind::Hold(Vec3::ZERO),
                    issuer: 0,
                    issued_tick: s.tick,
                })
            }
            _ => unreachable!(),
        }
        let u = &s.units[a.index as usize];
        deliver_call(&s, a, u, &mut st);
        assert!(st.pending_board.is_none(), "{case}: consumed exactly once");
        assert!(
            st.ride.is_none_or(|r| r.board.is_none()),
            "{case}: rejected"
        );
    }
}

#[test]
fn gboard_admission_keeps_issue_tick_yes_list_and_3d_reach() {
    for case in [
        "near3d",
        "edge3d",
        "outside3d",
        "far3d",
        "enemy",
        "dead",
        "player",
        "mounted",
        "hold",
        "follow",
    ] {
        let (mut s, p, a, vh) = scene(VehicleKind::Hover, 20.0, 0);
        let human = s
            .spawn(&UnitSpec::player(0, Vec3::new(12.0, 0.0, -20.0)))
            .unwrap();
        match case {
            "near3d" => s.unit_mut(a).unwrap().soldier.pos.y = 10.0,
            "edge3d" => s.unit_mut(a).unwrap().soldier.pos = Vec3::new(0.0, 30.0, 0.0),
            "outside3d" => s.unit_mut(a).unwrap().soldier.pos = Vec3::new(0.0, 30.01, 0.0),
            "far3d" => s.unit_mut(a).unwrap().soldier.pos.y = 40.0,
            "enemy" => s.unit_mut(a).unwrap().team = 1,
            "dead" => s.kill_environment(a, crate::sim::DeathCause::VehicleDestroyed),
            "player" => s.unit_mut(a).unwrap().controller = Controller::Player { slot: 1 },
            "mounted" => {
                assert!(s.seat_unit(a, vh, 1));
            }
            "hold" => {
                s.issue_command(0, crate::squad::CommandKind::Hold(Vec3::ZERO));
            }
            "follow" => {
                s.issue_command(0, crate::squad::CommandKind::Follow(human));
            }
            _ => {}
        }
        // Isolate the existing call receiver predicate from falling/commander motion.
        s.tick = 10;
        let rng = s.rng.state();
        s.need_gunner_call(p, vh);
        let yes = s
            .events
            .iter()
            .any(|e| matches!(e,Event::Call{unit,call:Call::Yes} if *unit==a));
        let expected = matches!(case, "near3d" | "edge3d" | "hold" | "follow");
        assert_eq!(yes, expected, "{case}");
        assert_eq!(
            s.units[a.index as usize].ai.pending_board.is_some(),
            matches!(case, "near3d" | "edge3d"),
            "{case}"
        );
        assert_eq!(s.rng.state(), rng, "call must not add entropy draws");
    }
}

#[test]
fn gboard_intervening_order_and_receiver_or_target_lifecycle_discard_pending() {
    for case in ["order", "entry", "death", "reuse", "target"] {
        let (mut s, _, a, vh) = issued(VehicleKind::Hover);
        match case {
            "order" => {
                s.spawn(&UnitSpec::player(0, s.unit(a).unwrap().soldier.pos))
                    .unwrap();
                assert!(s.issue_command(0, crate::squad::CommandKind::Hold(Vec3::ZERO)) > 0);
                assert!(s.unit(a).unwrap().command.is_some());
            }
            "entry" => assert!(s.seat_unit(a, vh, 1)),
            "death" | "reuse" => {
                s.kill_environment(a, crate::sim::DeathCause::VehicleDestroyed);
                assert!(s.units[a.index as usize].ai.pending_board.is_none());
                if case == "reuse" {
                    let fresh = s
                        .spawn(&UnitSpec {
                            controller: Controller::Ai,
                            ..UnitSpec::trooper(0, Vec3::ZERO)
                        })
                        .unwrap();
                    assert_eq!(fresh.index, a.index);
                    assert_ne!(fresh.gen, a.gen);
                    assert!(s.unit(fresh).unwrap().ai.pending_board.is_none());
                }
            }
            "target" => s.remove_vehicle(vh),
            _ => unreachable!(),
        }
        assert!(
            s.units[a.index as usize].ai.pending_board.is_none(),
            "{case}"
        );
    }
}

#[test]
fn gboard_competing_receivers_use_original_claim_arbitration() {
    let (mut s, _, a, vh) = scene(VehicleKind::Hover, 25.0, 0);
    let b = s
        .spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(0, Vec3::new(12.0, 0.0, -25.0))
        })
        .unwrap();
    while s.tick <= 10 {
        s.step(&[]);
    }
    for h in [a, b] {
        assert_eq!(s.unit(h).unwrap().ai.pending_board, Some((h, vh)));
        s.unit_mut(h).unwrap().ai.next_think = 20;
    }
    while s.tick <= 20 {
        s.step(&[]);
    }
    for h in [a, b] {
        assert_eq!(s.unit(h).unwrap().ai.ride.unwrap().board, Some(vh));
    }
    s.step(&[]);
    assert_eq!(s.unit(a).unwrap().ai.ride.unwrap().board, Some(vh));
    assert!(
        s.unit(b).unwrap().ai.ride.unwrap().board.is_none(),
        "equal-since loser drops claim"
    );
    for _ in 0..130 {
        s.step(&[]);
    }
    assert_eq!(s.unit(a).unwrap().mount.unwrap().vehicle, vh);
    assert!(s.unit(b).unwrap().mount.is_none());
}

#[test]
fn gboard_pending_identity_is_hashed_field_by_field_and_absence_costs_nothing() {
    let (s, _, a, vh) = issued(VehicleKind::Hover);
    let mut st = s.unit(a).unwrap().ai;
    let bytes = |st: &AiState| {
        let mut b = vec![];
        st.hash_into(&mut |x| b.extend_from_slice(x));
        b
    };
    st.pending_board = None;
    let bare = bytes(&st);
    st.pending_board = Some((a, vh));
    let base = bytes(&st);
    assert_eq!(base.len(), bare.len() + 22);
    for field in 0..4 {
        let mut variant = st;
        let (owner, target) = variant.pending_board.as_mut().unwrap();
        match field {
            0 => owner.index += 1,
            1 => owner.gen += 1,
            2 => target.index += 1,
            _ => target.gen += 1,
        }
        assert_ne!(base, bytes(&variant), "pending identity field {field}");
    }
    st.pending_board = None;
    assert_eq!(bare, bytes(&st));
}

#[test]
fn gboard_actual_respawn_and_vehicle_slot_reuse_do_not_retarget_a_call() {
    let (mut s, _, a, vh) = issued(VehicleKind::Hover);
    // Actual timed respawn rebuilds the receiver, independently of slot reuse by spawn.
    s.cps[0].owner = Some(0);
    s.cps[0].def.spawn_nodes = vec![(Vec3::new(0.0, 0.0, -90.0), 0.0)];
    s.kill_environment(a, crate::sim::DeathCause::VehicleDestroyed);
    s.units[a.index as usize].respawn_at = Some(s.tick);
    s.units[a.index as usize].spawn_slot = None;
    s.step(&[]);
    let fresh = s.handle(a.index as usize);
    assert_ne!(a, fresh);
    assert!(s.unit(fresh).unwrap().ai.pending_board.is_none());
    let mut st = s.unit(fresh).unwrap().ai;
    st.pending_board = Some((a, vh));
    deliver_call(&s, fresh, s.unit(fresh).unwrap(), &mut st);
    assert!(st.ride.is_none_or(|r| r.board.is_none()));

    s.remove_vehicle(vh);
    let replacement = s.spawn_vehicle(&VehicleSpec {
        kind: VehicleKind::Hover,
        class: 0,
        home_team: None,
        neutral_when_empty: true,
        spawn_pad: None,
        pos: Vec3::ZERO,
        yaw: 0.0,
        max_health: 1000.0,
        health_type: HealthType::Vehicle,
        seats: vec![SeatKind::Pilot, SeatKind::Gunner(0)],
    });
    assert_eq!(replacement.index, vh.index);
    assert_ne!(replacement.gen, vh.gen);
    st.target_cp = Some(0);
    st.pending_board = Some((fresh, vh));
    deliver_call(&s, fresh, s.unit(fresh).unwrap(), &mut st);
    assert!(st.pending_board.is_none());
    assert!(
        st.ride.is_none_or(|r| r.board.is_none()),
        "stale target cannot alias replacement"
    );
}

#[test]
fn gboard_older_claim_wins_even_with_higher_unit_index() {
    let (mut s, _, a, vh) = scene(VehicleKind::Hover, 25.0, 0);
    let b = s
        .spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(0, Vec3::new(12.0, 0.0, -25.0))
        })
        .unwrap();
    while s.tick <= 10 {
        s.step(&[]);
    }
    s.unit_mut(a).unwrap().ai.next_think = 20;
    s.unit_mut(b).unwrap().ai.next_think = 11;
    while s.tick <= 20 {
        s.step(&[]);
    }
    assert!(a.index < b.index);
    assert_eq!(s.unit(b).unwrap().ai.ride.unwrap().since, 11);
    assert_eq!(s.unit(b).unwrap().ai.ride.unwrap().board, Some(vh));
    assert!(s.unit(a).unwrap().ai.ride.unwrap().board.is_none());
    for _ in 0..140 {
        s.step(&[]);
    }
    assert_eq!(s.unit(b).unwrap().mount.unwrap().vehicle, vh);
    assert!(s.unit(a).unwrap().mount.is_none());
}

#[test]
fn gboard_disperse_expiry_preserves_the_normal_pool_call_target() {
    let (mut s, _, a, vh) = scene(VehicleKind::Hover, 20.0, 0);
    s.spawn(&UnitSpec::player(0, s.unit(a).unwrap().soldier.pos))
        .unwrap();
    assert!(s.issue_command(0, crate::squad::CommandKind::Disperse(12)) > 0);
    assert!(s.unit(a).unwrap().command.unwrap().in_pool());
    while s.tick <= 10 {
        s.step(&[]);
    }
    assert_eq!(s.unit(a).unwrap().ai.pending_board, Some((a, vh)));
    let due = s.unit(a).unwrap().ai.next_think;
    assert!(due > 12);
    while s.tick <= 12 {
        s.step(&[]);
    }
    assert!(s.unit(a).unwrap().command.is_none());
    assert_eq!(
        s.unit(a).unwrap().ai.pending_board,
        Some((a, vh)),
        "pool expiry is not a new player order"
    );
    while s.tick <= due {
        s.step(&[]);
    }
    assert_eq!(s.unit(a).unwrap().ai.ride.unwrap().board, Some(vh));
}
