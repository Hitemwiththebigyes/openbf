//! WGPAS/WGMUL: explicit secondary-seat admission, concurrent source budget and lifecycle.
use super::*;

// GBOARD: natural issue/think/entry witnesses, controls and delivery invariants.
#[test]
fn wgpas_actual_step_48_seat_matrix() {
    use crate::squad::CommandKind;
    let mut faults = 0;
    let mut passes = 0;
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for bf2 in [false, true] {
            for order in ["pool", "hold", "follow"] {
                for seats in 0..4 {
                    let (mut s, p, a, vh) = scene(kind, 20., if bf2 { 2 } else { 0 });
                    if seats == 1 || seats == 2 {
                        s.seat_setup[0].as_mut().unwrap().guns.clear();
                    }
                    if seats == 2 {
                        s.vehicle_mut(vh).unwrap().seats[1].kind = SeatKind::Passenger(0);
                    }
                    if seats == 3 {
                        let gun = s.seat_setup[0].as_mut().unwrap().guns[1].take();
                        s.seat_setup[0].as_mut().unwrap().guns.push(gun);
                        s.seat_setup[0].as_mut().unwrap().boardable.push(true);
                        s.vehicle_mut(vh).unwrap().seats.push(crate::vehicle::Seat {
                            kind: SeatKind::Gunner(1),
                            occupant: None,
                        });
                    }
                    assert!(crate::vehicle::is_ring(
                        &s.vehicle(vh)
                            .unwrap()
                            .seats
                            .iter()
                            .map(|x| x.kind)
                            .collect::<Vec<_>>()
                    ));
                    let human = s
                        .spawn(&UnitSpec::player(0, Vec3::new(12., 0., -40.)))
                        .unwrap();
                    match order {
                        "hold" => assert_eq!(s.issue_command(0, CommandKind::Hold(Vec3::ZERO)), 1),
                        "follow" => assert_eq!(s.issue_command(0, CommandKind::Follow(human)), 1),
                        _ => (),
                    }
                    let original = s.unit(a).unwrap().command;
                    let (
                        mut pilot_entry,
                        mut call,
                        mut yes,
                        mut pending,
                        mut claim,
                        mut entry,
                        mut due,
                    ) = (None, None, None, None, None, None, None);
                    for _ in 0..150 {
                        let tick = s.tick;
                        s.step(&[]);
                        if s.unit(p).unwrap().mount.is_some() {
                            pilot_entry.get_or_insert(tick);
                        }
                        for event in &s.events {
                            if matches!(event, Event::Call {unit, call:Call::NeedGunner} if *unit == p)
                                && call.is_none()
                            {
                                call = Some(tick);
                                due = Some(s.unit(a).unwrap().ai.next_think);
                            }
                            if matches!(event, Event::Call {unit, call:Call::Yes} if *unit == a) {
                                yes.get_or_insert(tick);
                            }
                        }
                        let u = s.unit(a).unwrap();
                        assert_eq!(u.command, original, "retain command every tick");
                        if u.ai.pending_board == Some((a, vh)) {
                            pending.get_or_insert(tick);
                        }
                        if u.ai.ride.is_some_and(|r| r.board == Some(vh)) {
                            claim.get_or_insert(tick);
                        }
                        if u.mount.is_some_and(|m| m.vehicle == vh && m.seat == 1) {
                            entry.get_or_insert(tick);
                        }
                    }
                    assert_eq!(pilot_entry, Some(9));
                    assert_eq!(s.unit(a).unwrap().command, original);
                    assert_eq!(call.is_some(), !bf2);
                    assert_eq!(yes.is_some(), !bf2);
                    let pass = if bf2 {
                        pending.is_none() && claim.is_none() && entry.is_none()
                    } else {
                        assert_eq!(call, Some(10));
                        assert_eq!(yes, Some(10));
                        assert_eq!(pending, Some(10));
                        assert_eq!(due, Some(19));
                        claim == due && entry.is_some()
                    };
                    passes += usize::from(pass);
                    faults += usize::from(!pass);
                    eprintln!("WGPAS {kind:?}/bf2{bf2}/{order}/seats{seats}: call{call:?} yes{yes:?} pending{pending:?} due{due:?} claim{claim:?} entry{entry:?} pass{pass}");
                }
            }
        }
    }
    eprintln!("WGPAS TOTAL faults={faults} passes={passes}");
    assert_eq!(
        faults, 0,
        "explicit BF1 call accepts existing boardable unarmed secondary seat"
    );
}

fn unarmed_claim(kind: VehicleKind) -> (Sim, UnitHandle, UnitHandle, VehicleHandle) {
    let (mut s, p, a, vh) = scene(kind, 20., 3);
    while s.tick <= 19 {
        s.step(&[]);
    }
    let r = s.unit(a).unwrap().ai.ride.unwrap();
    assert_eq!(r.board, Some(vh));
    assert_eq!(r.called_board, Some(a));
    assert!(!r.ordered_call);
    (s, p, a, vh)
}

#[test]
fn wgpas_no_call_cannot_inherit_unarmed_admission() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for case in [1, 2] {
            let (mut s, _, a, _) = scene(kind, 20., case);
            s.seat_setup[0].as_mut().unwrap().guns.clear();
            for _ in 0..150 {
                s.step(&[]);
                assert!(s.unit(a).unwrap().mount.is_none());
                assert!(s.unit(a).unwrap().ai.ride.is_none_or(|r| r.board.is_none()));
            }
        }
        let (mut s, _, a, vh) = unarmed_claim(kind);
        let r = s.unit(a).unwrap().ai.ride.unwrap();
        assert_eq!(
            claim_seat(&s, a, vh),
            None,
            "autonomous admission stays armed"
        );
        assert_eq!(claimed_seat(&s, a, vh, r), Some(1));
        let mut no_call = r;
        no_call.called_board = None;
        assert_eq!(claimed_seat(&s, a, vh, no_call), None);
        let mut reused = r;
        reused.called_board.as_mut().unwrap().gen += 1;
        assert_eq!(claimed_seat(&s, a, vh, reused), None);
        s.ground_rules = GroundRules::Bf2;
        assert_eq!(claimed_seat(&s, a, vh, r), None);
    }
}

#[test]
fn wgpas_hash_owner_cooldown_and_identical_continuation() {
    let bytes = |r: RideState| {
        let mut b = vec![];
        let st = AiState {
            ride: Some(r),
            ..AiState::default()
        };
        st.hash_into(&mut |x| b.extend_from_slice(x));
        b
    };
    let (s, _, a, _) = unarmed_claim(VehicleKind::Hover);
    let r = s.unit(a).unwrap().ai.ride.unwrap();
    for variant in 0..4 {
        let mut v = r;
        match variant {
            0 => v.called_board = None,
            1 => v.called_board.as_mut().unwrap().index += 1,
            2 => v.called_board.as_mut().unwrap().gen += 1,
            _ => v.call_blocked = true,
        }
        assert_ne!(bytes(r), bytes(v), "persistent future state {variant}");
    }
    let run = || {
        let (mut s, _, a, _) = unarmed_claim(VehicleKind::Hover);
        let mut hashes = vec![];
        for _ in 0..160 {
            s.step(&[]);
            hashes.push(s.hash());
        }
        assert!(s.unit(a).unwrap().mount.is_some());
        assert!(s.unit(a).unwrap().ai.ride.unwrap().called_board.is_none());
        s.exit_vehicle(a);
        assert!(s.unit(a).unwrap().ai.ride.unwrap().called_board.is_none());
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn wgpas_repeated_calls_keep_clocks_and_cannot_bypass_giveup() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        let (mut s, p, a, vh) = unarmed_claim(kind);
        let mut st = s.unit(a).unwrap().ai;
        let old = st.ride;
        for _ in 0..3 {
            st.pending_board = Some((a, vh));
            deliver_call(&s, a, s.unit(a).unwrap(), &mut st);
            assert_eq!(st.ride, old);
        }
        give_up(&s, &mut st, &mut NavTick::default());
        let ended = st.ride.unwrap();
        assert!(ended.called_board.is_none() && ended.call_blocked);
        st.pending_board = Some((a, vh));
        deliver_call(&s, a, s.unit(a).unwrap(), &mut st);
        assert_eq!(st.ride, Some(ended));
        assert!(st.pending_board.is_none());
        s.unit_mut(a).unwrap().ai = st;
        s.tick = (s.tick..s.tick + 100)
            .find(|&t| crate::ai::unit_unit(p.index, t, 0, crate::ai::SALT_NEED_GUNNER) < 0.3)
            .unwrap();
        s.need_gunner_call(p, vh);
        let mut st = s.unit(a).unwrap().ai;
        deliver_call(&s, a, s.unit(a).unwrap(), &mut st);
        assert!(st.ride.unwrap().board.is_none());
    }
}

#[test]
fn wgpas_unarmed_lifecycle_and_new_order_clear_provenance() {
    use crate::squad::CommandKind;
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for action in ["order", "death", "target", "eligibility", "entry", "giveup"] {
            let (mut s, _, a, vh) = unarmed_claim(kind);
            match action {
                "order" => {
                    s.spawn(&UnitSpec::player(0, Vec3::new(12., 0., -20.)))
                        .unwrap();
                    assert_eq!(s.issue_command(0, CommandKind::Hold(Vec3::ZERO)), 1);
                }
                "death" => s.kill_environment(a, crate::sim::DeathCause::VehicleDestroyed),
                "target" => s.remove_vehicle(vh),
                "eligibility" => {
                    s.seat_setup[0].as_mut().unwrap().boardable[1] = false;
                    s.step(&[]);
                }
                "entry" => {
                    assert!(s.enter_vehicle(a, vh, 1));
                    s.exit_vehicle(a);
                }
                "giveup" => {
                    let mut st = s.unit(a).unwrap().ai;
                    give_up(&s, &mut st, &mut NavTick::default());
                    s.unit_mut(a).unwrap().ai = st;
                }
                _ => unreachable!(),
            }
            assert!(
                s.units[a.index as usize]
                    .ai
                    .ride
                    .is_none_or(|r| r.called_board.is_none()),
                "{action}"
            );
        }
    }
}

#[test]
fn wgpas_unarmed_pickup_is_held_past_initial_wait() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        let (mut s, p, a, vh) = scene(kind, 29.5, 3);
        // Synthetic slow walker makes the approach outlast the initial wait.
        s.unit_mut(a).unwrap().consts.max_speed = 3.;
        s.unit_mut(a).unwrap().consts.max_strafe_speed = 3.;
        let at = s.unit(a).unwrap().soldier.pos;
        s.spawn(&UnitSpec::player(0, at - Vec3::Z * 20.)).unwrap();
        assert_eq!(s.issue_command(0, crate::squad::CommandKind::Hold(at)), 1);
        let mut entry = None;
        let mut held = 0;
        for _ in 0..240 {
            let t = s.tick;
            s.step(&[]);
            if s.unit(a).unwrap().mount.is_some() {
                entry.get_or_insert(t);
            }
            if t > 130 && entry.is_none() {
                assert_eq!(s.unit(a).unwrap().ai.ride.unwrap().board, Some(vh));
                assert_eq!(locomotion(&s, p), 0.0);
                assert!(!s.unit(p).unwrap().ai.ride.unwrap().pickup.unwrap().complete);
                held += 1;
            }
        }
        assert!(held > 0, "witness past four-second initial window");
        assert!(entry.is_some());
        assert!(locomotion(&s, p) > 0.);
    }
}

#[test]
fn wgpas_late_departure_unarmed_calls_in_both_index_orders() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for first in [false, true] {
            let (mut s, p, a, vh) = super::super::setout_call_tests::departure_scene(kind, first);
            s.seat_setup[0].as_mut().unwrap().guns.clear();
            let departure = s.tick + 121;
            while s.tick < departure {
                s.step(&[]);
            }
            s.unit_mut(a).unwrap().soldier.pos =
                s.unit(p).unwrap().soldier.pos + Vec3::new(0., 0., -20.);
            s.step(&[]);
            assert_eq!(s.unit(a).unwrap().ai.pending_board, Some((a, vh)));
            let due = s.unit(a).unwrap().ai.next_think;
            while s.tick <= due {
                s.step(&[]);
            }
            let r = s.unit(a).unwrap().ai.ride.unwrap();
            assert_eq!(r.called_board, Some(a));
            assert_eq!(r.board, Some(vh));
            assert_eq!(r.since, due);
            for _ in 0..180 {
                s.step(&[]);
            }
            assert!(s
                .unit(a)
                .unwrap()
                .mount
                .is_some_and(|m| m.vehicle == vh && m.seat == 1));
        }
    }
}

#[test]
fn wgpas_physical_entry_still_obeys_capacity_and_human_priority() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        let (mut s, _, a, vh) = unarmed_claim(kind);
        let human = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
        assert!(s.enter_vehicle(human, vh, 1));
        s.unit_mut(a).unwrap().soldier.pos = s.vehicle(vh).unwrap().pos;
        s.unit_mut(a).unwrap().ai.ride.as_mut().unwrap().use_req = true;
        s.ai_use_pass();
        assert!(s.unit(a).unwrap().mount.is_none());
        assert_eq!(s.vehicle(vh).unwrap().seats[1].occupant, Some(human));
        assert!(s.unit(a).unwrap().ai.ride.unwrap().called_board.is_none());
    }
}

#[test]
fn wgmul_actual_step_32_scene_capacity() {
    let mut faults = 0;
    let mut controls = 0;
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for bf2 in [false, true] {
            for extra in [false, true] {
                for two in [false, true] {
                    for armed in [false, true] {
                        let (mut s, p, a, vh) = scene(kind, 20., if bf2 { 2 } else { 0 });
                        if !armed {
                            s.seat_setup[0].as_mut().unwrap().guns.clear();
                        }
                        if extra {
                            let gun = s.seat_setup[0]
                                .as_ref()
                                .unwrap()
                                .guns
                                .get(1)
                                .cloned()
                                .flatten();
                            let setup = s.seat_setup[0].as_mut().unwrap();
                            setup.boardable.push(true);
                            setup.guns.resize(2, None);
                            setup.guns.push(gun);
                            s.vehicle_mut(vh).unwrap().seats.push(crate::vehicle::Seat {
                                kind: SeatKind::Gunner(1),
                                occupant: None,
                            });
                        }
                        let mut receivers = vec![a];
                        if two {
                            let b = s
                                .spawn(&UnitSpec {
                                    controller: Controller::Ai,
                                    ..UnitSpec::trooper(0, Vec3::new(16., 0., -20.))
                                })
                                .unwrap();
                            receivers.push(b);
                        }
                        assert!(crate::vehicle::is_ring(
                            &s.vehicle(vh)
                                .unwrap()
                                .seats
                                .iter()
                                .map(|x| x.kind)
                                .collect::<Vec<_>>()
                        ));
                        let mut peak_claims = 0;
                        let mut claim_ticks = vec![None; receivers.len()];
                        let mut entries = vec![None; receivers.len()];
                        let mut yes = vec![Vec::new(); receivers.len()];
                        let mut calls = Vec::new();
                        let mut due = vec![None; receivers.len()];
                        let mut pilot_entry = None;
                        for _ in 0..150 {
                            let t = s.tick;
                            s.step(&[]);
                            if s.unit(p).unwrap().mount.is_some() {
                                pilot_entry.get_or_insert(t);
                            }
                            for event in &s.events {
                                if matches!(event,Event::Call{unit,call:Call::NeedGunner} if *unit==p)
                                {
                                    calls.push(t);
                                    if calls.len() == 1 {
                                        for (j, &h) in receivers.iter().enumerate() {
                                            due[j] = Some(s.unit(h).unwrap().ai.next_think);
                                        }
                                    }
                                }
                                for (j, &h) in receivers.iter().enumerate() {
                                    if matches!(event,Event::Call{unit,call:Call::Yes} if *unit==h)
                                    {
                                        yes[j].push(t);
                                    }
                                }
                            }
                            let mut concurrent = 0;
                            for (j, &h) in receivers.iter().enumerate() {
                                let u = s.unit(h).unwrap();
                                if u.ai.ride.is_some_and(|r| r.board == Some(vh)) {
                                    claim_ticks[j].get_or_insert(t);
                                    concurrent += 1;
                                }
                                if let Some(m) = u.mount.filter(|m| m.vehicle == vh) {
                                    entries[j].get_or_insert((t, m.seat));
                                }
                            }
                            peak_claims = peak_claims.max(concurrent);
                        }
                        assert_eq!(pilot_entry, Some(9));
                        if !bf2 {
                            assert_eq!(calls.first(), Some(&10));
                            assert!(yes.iter().all(|rows| rows.first() == Some(&10)));
                        }
                        if !bf2 {
                            for (j, claim) in claim_ticks.iter().enumerate() {
                                assert_eq!(
                                    *claim, due[j],
                                    "called claim begins at actual scheduled think"
                                );
                            }
                            if two {
                                assert_eq!(due, vec![Some(19), Some(18)]);
                            }
                            if extra && two {
                                assert_ne!(
                                    entries[0].unwrap().1,
                                    entries[1].unwrap().1,
                                    "distinct physical seats"
                                );
                            }
                        }
                        let boarded = entries.iter().filter(|e| e.is_some()).count();
                        let expected = if bf2 {
                            0
                        } else {
                            receivers.len().min(if extra { 2 } else { 1 })
                        };
                        let pass =
                            boarded == expected && (bf2 || !extra || !two || peak_claims == 2);
                        faults += usize::from(!pass);
                        controls += usize::from(pass);
                        eprintln!("MULTI {kind:?}/bf2{bf2}/extra{extra}/two{two}/armed{armed}: calls{calls:?} yes{yes:?} due{due:?} claims{claim_ticks:?} peak{peak_claims} entries{entries:?} expected{expected} pass{pass}");
                    }
                }
            }
        }
    }
    eprintln!("MULTI TOTAL faults={faults} passes={controls}");
    assert_eq!(
        faults, 0,
        "each supported called receiver can approach a separate spare secondary seat"
    );
}

#[test]
fn wgmul_exact_source_budget_records_and_priority() {
    use crate::ai::state::BoardMaintenance;
    for seats in [2usize, 3] {
        for player in [false, true] {
            for ordinary in [
                "none",
                "live",
                "expired",
                "owner",
                "target",
                "dead",
                "enemy",
                "future",
                "duplicate",
            ] {
                for reverse in [false, true] {
                    for tie in [false, true] {
                        let (mut s, _, _, vh) = scene(VehicleKind::Hover, 25., 0);
                        if seats == 3 {
                            s.vehicle_mut(vh).unwrap().seats.push(crate::vehicle::Seat {
                                kind: SeatKind::Gunner(1),
                                occupant: None,
                            });
                            s.seat_setup[0].as_mut().unwrap().boardable.push(true);
                        }
                        if player {
                            let h = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
                            assert!(s.enter_vehicle(h, vh, 1));
                        }
                        s.tick = 100;
                        let mut candidates = Vec::new();
                        for i in 0..4 {
                            let h = s
                                .spawn(&UnitSpec {
                                    controller: Controller::Ai,
                                    ..UnitSpec::trooper(0, Vec3::new(12., 0., -25.))
                                })
                                .unwrap();
                            let rank = if reverse { 3 - i } else { i };
                            s.unit_mut(h).unwrap().ai.ride = Some(RideState {
                                board: Some(vh),
                                called_board: Some(h),
                                since: if tie { 90 } else { 90 + rank },
                                maintenance: Some(BoardMaintenance { owner: h, tick: 99 }),
                                ..Default::default()
                            });
                            candidates.push(h);
                        }
                        if ordinary != "none" {
                            let h = s
                                .spawn(&UnitSpec {
                                    controller: Controller::Ai,
                                    ..UnitSpec::trooper(0, Vec3::new(15., 0., -25.))
                                })
                                .unwrap();
                            let mut target = vh;
                            if ordinary == "target" {
                                target.gen += 1;
                            }
                            let mut owner = h;
                            if ordinary == "owner" {
                                owner.gen += 1;
                            }
                            if ordinary == "duplicate" {
                                owner = candidates[0];
                            }
                            let u = s.unit_mut(h).unwrap();
                            u.ai.ride = Some(RideState {
                                board: Some(target),
                                since: 1,
                                maintenance: Some(BoardMaintenance {
                                    owner,
                                    tick: match ordinary {
                                        "expired" => 10,
                                        "future" => 101,
                                        _ => 99,
                                    },
                                }),
                                ..Default::default()
                            });
                            if ordinary == "dead" {
                                u.alive = false;
                            }
                            if ordinary == "enemy" {
                                u.team = 1;
                            }
                        }
                        let mut priority = candidates.clone();
                        priority
                            .sort_by_key(|h| (s.unit(*h).unwrap().ai.ride.unwrap().since, h.index));
                        let budget = seats - usize::from(player) - usize::from(ordinary == "live");
                        let clocks: Vec<_> = candidates
                            .iter()
                            .map(|h| s.unit(*h).unwrap().ai.ride.unwrap().since)
                            .collect();
                        called_claim_pass(&mut s);
                        for (i, h) in candidates.iter().enumerate() {
                            let r = s.unit(*h).unwrap().ai.ride.unwrap();
                            assert_eq!(r.board==Some(vh),priority[..budget].contains(h),
                                "seats{seats}/player{player}/{ordinary}/reverse{reverse}/tie{tie} owner{h:?}");
                            assert_eq!(r.since, clocks[i]);
                            if r.board.is_some() {
                                assert_eq!(r.maintenance.unwrap().tick, 99);
                            }
                        }
                        let once: Vec<_> = candidates
                            .iter()
                            .map(|h| s.unit(*h).unwrap().ai.ride)
                            .collect();
                        called_claim_pass(&mut s);
                        assert_eq!(
                            once,
                            candidates
                                .iter()
                                .map(|h| s.unit(*h).unwrap().ai.ride)
                                .collect::<Vec<_>>(),
                            "idempotent, one record per owner"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn wgmul_ai_occupants_do_not_consume_claim_slots() {
    let (mut s, pilot, a, vh) = scene(VehicleKind::Hover, 25., 0);
    assert!(s.enter_vehicle(pilot, vh, 0));
    let b = s
        .spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(0, Vec3::new(12., 0., -25.))
        })
        .unwrap();
    s.tick = 10;
    for h in [a, b] {
        s.unit_mut(h).unwrap().ai.ride = Some(RideState {
            board: Some(vh),
            called_board: Some(h),
            since: 10,
            ..Default::default()
        });
    }
    called_claim_pass(&mut s);
    for h in [a, b] {
        assert_eq!(s.unit(h).unwrap().ai.ride.unwrap().board, Some(vh));
    }
}

#[test]
fn wgmul_actual_thinks_bound_capacity_plus_one_and_same_tick() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for seats in [2usize, 3] {
            for tie in [false, true] {
                let (mut s, _, a, vh) = scene(kind, 25., 0);
                if seats == 3 {
                    s.vehicle_mut(vh).unwrap().seats.push(crate::vehicle::Seat {
                        kind: SeatKind::Gunner(1),
                        occupant: None,
                    });
                    s.seat_setup[0].as_mut().unwrap().boardable.push(true);
                }
                let mut hs = vec![a];
                for i in 0..seats {
                    hs.push(
                        s.spawn(&UnitSpec {
                            controller: Controller::Ai,
                            ..UnitSpec::trooper(0, Vec3::new(12. + i as f32, 0., -25.))
                        })
                        .unwrap(),
                    );
                }
                while s.tick <= 10 {
                    s.step(&[]);
                }
                for h in &hs {
                    assert_eq!(s.unit(*h).unwrap().ai.pending_board, Some((*h, vh)));
                    if tie {
                        s.unit_mut(*h).unwrap().ai.next_think = 20;
                    }
                }
                let mut due: Vec<_> = hs
                    .iter()
                    .map(|h| (*h, s.unit(*h).unwrap().ai.next_think))
                    .collect();
                due.sort_by_key(|(h, t)| (*t, h.index));
                let last_due = due.iter().map(|(_, d)| *d).max().unwrap();
                eprintln!("WGMUL thinks {kind:?}/seats{seats}/tie{tie} due{due:?}");
                while s.tick <= last_due + 1 {
                    let t = s.tick;
                    s.step(&[]);
                    let claims: Vec<_> = hs
                        .iter()
                        .filter(|h| {
                            s.unit(**h)
                                .unwrap()
                                .ai
                                .ride
                                .is_some_and(|r| r.board == Some(vh))
                        })
                        .collect();
                    assert!(
                        claims.len() <= seats,
                        "no over-budget write-back: {kind:?}/{seats}/{tie}/{t}"
                    );
                    for (h, d) in &due[..seats] {
                        if t >= *d {
                            let r = s.unit(*h).unwrap().ai.ride.unwrap();
                            assert_eq!(r.board, Some(vh));
                            assert_eq!(r.since, *d);
                        }
                    }
                }
                assert!(s
                    .unit(due[seats].0)
                    .unwrap()
                    .ai
                    .ride
                    .is_none_or(|r| r.board.is_none()));
                let mut occupied = vec![false; seats];
                for _ in 0..180 {
                    s.step(&[]);
                    for h in &hs {
                        if let Some(m) = s.unit(*h).unwrap().mount {
                            assert_eq!(m.vehicle, vh);
                            occupied[m.seat as usize] = true;
                        }
                    }
                    let v = s.vehicle(vh).unwrap();
                    let riders: Vec<_> = v.seats.iter().filter_map(|seat| seat.occupant).collect();
                    let mut unique = riders.clone();
                    unique.sort_by_key(|h| (h.index, h.gen));
                    unique.dedup();
                    assert_eq!(riders.len(), unique.len());
                }
                assert_eq!(
                    occupied.iter().filter(|&&x| x).count(),
                    seats - 1,
                    "physical entry remains separately bounded"
                );
                assert!(
                    hs.iter().all(|h| s
                        .unit(*h)
                        .unwrap()
                        .ai
                        .ride
                        .is_none_or(|r| r.board.is_none())),
                    "surplus releases when full"
                );
            }
        }
    }
}

#[test]
fn wgmul_zero_budget_and_ai_vs_player_held_seats() {
    let (mut s, pilot, a, vh) = scene(VehicleKind::Hover, 25., 0);
    assert!(s.enter_vehicle(pilot, vh, 0));
    let bot = s
        .spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(0, Vec3::ZERO)
        })
        .unwrap();
    assert!(s.enter_vehicle(bot, vh, 1));
    assert_eq!(
        s.enter_seat(a, vh),
        None,
        "full AI occupancy still refuses CanEnter"
    );
    assert!(
        !s.enter_vehicle(a, vh, 1),
        "AI cannot physically steal an occupied seat"
    );
    s.tick = 10;
    let r = RideState {
        board: Some(vh),
        called_board: Some(a),
        since: 10,
        ..Default::default()
    };
    assert!(
        !claim_preempted(&s, a, s.unit(a).unwrap(), r, vh),
        "even full AI occupancy consumes no source slots; physical CanEnter is separate"
    );
    for seat in [0, 1] {
        let h = s.spawn(&UnitSpec::player(seat, Vec3::ZERO)).unwrap();
        assert!(s.enter_vehicle(h, vh, seat));
    }
    assert!(
        claim_preempted(&s, a, s.unit(a).unwrap(), r, vh),
        "two PlayerControllers leave zero source slots"
    );
    s.unit_mut(a).unwrap().ai.ride = Some(r);
    called_claim_pass(&mut s);
    assert!(s.unit(a).unwrap().ai.ride.unwrap().board.is_none());
}

#[test]
fn wgmul_departure_concurrent_receivers_in_both_issuer_orders() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        for first in [false, true] {
            let (mut s, p, a, vh) = super::super::setout_call_tests::departure_scene(kind, first);
            s.vehicle_mut(vh).unwrap().seats.push(crate::vehicle::Seat {
                kind: SeatKind::Gunner(1),
                occupant: None,
            });
            s.seat_setup[0].as_mut().unwrap().boardable.push(true);
            let b = s
                .spawn(&UnitSpec {
                    controller: Controller::Ai,
                    ..UnitSpec::trooper(0, Vec3::new(500., 0., 500.))
                })
                .unwrap();
            let departure = s.tick + 121;
            while s.tick < departure {
                s.step(&[]);
            }
            let at = s.unit(p).unwrap().soldier.pos;
            for (h, x) in [(a, -3.), (b, 3.)] {
                s.unit_mut(h).unwrap().soldier.pos = at + Vec3::new(x, 0., -20.);
            }
            s.step(&[]);
            let due = [
                (a, s.unit(a).unwrap().ai.next_think),
                (b, s.unit(b).unwrap().ai.next_think),
            ];
            for (h, _) in due {
                assert_eq!(s.unit(h).unwrap().ai.pending_board, Some((h, vh)));
            }
            while s.tick <= due.iter().map(|(_, t)| *t).max().unwrap() {
                s.step(&[]);
            }
            for (h, t) in due {
                let r = s.unit(h).unwrap().ai.ride.unwrap();
                assert_eq!(r.board, Some(vh));
                assert_eq!(r.since, t);
            }
            for _ in 0..180 {
                s.step(&[]);
            }
            let ma = s.unit(a).unwrap().mount.unwrap();
            let mb = s.unit(b).unwrap().mount.unwrap();
            assert_eq!((ma.vehicle, mb.vehicle), (vh, vh));
            assert_ne!(ma.seat, mb.seat);
        }
    }
}

#[test]
fn wgmul_human_takeover_shrinks_live_budget_without_restarting_survivors() {
    for kind in [VehicleKind::Hover, VehicleKind::Walker] {
        let (mut s, _, a, vh) = scene(kind, 25., 0);
        s.vehicle_mut(vh).unwrap().seats.push(crate::vehicle::Seat {
            kind: SeatKind::Gunner(1),
            occupant: None,
        });
        s.seat_setup[0].as_mut().unwrap().boardable.push(true);
        let mut hs = vec![a];
        for x in [14., 16.] {
            hs.push(
                s.spawn(&UnitSpec {
                    controller: Controller::Ai,
                    ..UnitSpec::trooper(0, Vec3::new(x, 0., -25.))
                })
                .unwrap(),
            );
        }
        while s.tick <= 10 {
            s.step(&[]);
        }
        let mut due: Vec<_> = hs
            .iter()
            .map(|h| (*h, s.unit(*h).unwrap().ai.next_think))
            .collect();
        due.sort_by_key(|(h, t)| (*t, h.index));
        while s.tick <= due.iter().map(|(_, t)| *t).max().unwrap() {
            s.step(&[]);
        }
        assert!(hs.iter().all(|h| s
            .unit(*h)
            .unwrap()
            .ai
            .ride
            .is_some_and(|r| r.board == Some(vh))));
        let human = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
        assert!(s.enter_vehicle(human, vh, 2));
        s.step(&[]);
        assert_eq!(s.vehicle(vh).unwrap().seats[2].occupant, Some(human));
        for (i, (h, t)) in due.iter().enumerate() {
            let r = s.unit(*h).unwrap().ai.ride.unwrap();
            assert_eq!(r.board, if i < 2 { Some(vh) } else { None });
            assert_eq!(r.since, *t, "unchanged arbitration and give-up clock");
        }
        for _ in 0..180 {
            s.step(&[]);
        }
        assert_eq!(s.vehicle(vh).unwrap().seats[2].occupant, Some(human));
        assert_eq!(
            hs.iter()
                .filter(|h| s.unit(**h).unwrap().mount.is_some())
                .count(),
            1
        );
        assert!(hs.iter().all(|h| s
            .unit(*h)
            .unwrap()
            .ai
            .ride
            .is_none_or(|r| r.board.is_none())));
    }
}
