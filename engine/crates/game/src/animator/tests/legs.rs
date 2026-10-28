use super::*;
use std::f32::consts::TAU;

#[test]
fn shots_restart_the_lower_player_only_when_it_plays_the_action() {
    for rig in [rig_bf1(), rig_bf2()] {
        for speed in [0.0, 6.5] {
            let mut r = Run::new(rig.clone());
            let i = input(speed);
            r.steps(&i, 10);
            r.shoot(&i);
            r.steps(&i, 3);
            let before = r.a.lower.secs;
            r.shoot(&i);
            assert!((r.a.upper.secs - DT).abs() < 1e-6);
            let expected = if speed == 0.0 { DT } else { before + DT };
            assert!(
                (r.a.lower.secs - expected).abs() < 1e-6,
                "{:?} speed {speed}: {} expected {expected}",
                r.rig.game,
                r.a.lower.secs
            );
        }
    }
}

fn with_crouch(mut rig: Rig) -> Rig {
    if let Some(sets) = &mut rig.sets.bf1 {
        sets.base[1] = sets.base[0].clone();
        sets.w[1] = sets.w[0].clone();
        sets.u[1] = sets.u[0].clone();
    }
    for family in &mut rig.sets.bf2 {
        for slots in family.sets.values_mut() {
            for m in 0..bf2::MOVES.len() {
                for layer in 0..2 {
                    slots[2 * bf2::move_pair(1, m) + layer] =
                        slots[2 * bf2::move_pair(0, m) + layer];
                }
            }
            for action in [0, 5] {
                for layer in 0..2 {
                    slots[2 * bf2::action_pair(1, action) + layer] =
                        slots[2 * bf2::action_pair(0, action) + layer];
                }
            }
        }
    }
    Rig::new(rig.sets, &skel(), &clips())
}

#[test]
fn sustained_fire_lets_stationary_leg_turns_finish() {
    for rig in [rig_bf1(), rig_bf2()].map(with_crouch) {
        for (s, stance) in [(0, Stance::Stand), (1, Stance::Crouch)] {
            for yaw in [-PI + 0.01, PI - 0.01] {
                for fps in [30, 60, 144] {
                    let mut r = Run::new(rig.clone());
                    let dt = 1.0 / fps as f32;
                    let mut i = AnimInput {
                        stance,
                        ..input(0.0)
                    };
                    r.a.step(&r.rig, &r.clips, &r.skel, &i, dt);
                    i.yaw = yaw;
                    for frame in 0..fps * 5 {
                        i.fired = frame % (fps / 10) == 0;
                        r.a.step(&r.rig, &r.clips, &r.skel, &i, dt);
                        if frame >= fps {
                            assert_eq!(
                                r.a.body(),
                                Some(Body::Feet(s, Move::Idle)),
                                "{:?} {stance:?} yaw {yaw} fps {fps} frame {frame}",
                                r.rig.game
                            );
                            assert!(r.a.twist().abs() < 0.4, "{}", r.a.twist());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn fast_mouse_turns_keep_the_waist_within_the_original_aim_limit() {
    for rig in [rig_bf1(), rig_bf2()].map(with_crouch) {
        for stance in [Stance::Stand, Stance::Crouch] {
            for fps in [30, 60, 144] {
                let dt = 1.0 / fps as f32;
                let mut r = Run::new(rig.clone());
                let mut i = AnimInput {
                    stance,
                    mouse_aim: true,
                    alert: true,
                    ..input(0.0)
                };
                r.a.step(&r.rig, &r.clips, &r.skel, &i, dt);
                for frame in 0..fps * 3 {
                    i.yaw += [PI - 0.01, -2.4, 3.0 * TAU + 1.2, -3.0 * TAU - 1.4]
                        [(frame / (fps / 5)) % 4];
                    i.fired = frame % (fps / 10) == 0;
                    r.a.step(&r.rig, &r.clips, &r.skel, &i, dt);
                    let waist = r.a.aim().turn(r.a.twist()).0;
                    assert!(
                        waist.abs() <= aim::YAW_LIMIT + 1e-5,
                        "{:?} {stance:?} fps {fps} frame {frame}: waist {waist}",
                        r.rig.game
                    );
                    assert!((waist + r.a.twist()).abs() < 1e-5);
                }
                for frame in 0..fps * 2 {
                    i.fired = frame % (fps / 10) == 0;
                    r.a.step(&r.rig, &r.clips, &r.skel, &i, dt);
                }
                assert!(matches!(r.a.body(), Some(Body::Feet(_, Move::Idle))));
                let settled = r.a.twist();
                r.steps(&i, fps);
                assert_eq!(r.a.twist(), settled);
            }
        }
    }
}
