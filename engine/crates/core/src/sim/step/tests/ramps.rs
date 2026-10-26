use super::*;
use crate::collision::{trimesh, Body, Shape};
use crate::soldier::Stance;
use glam::Mat4;

#[test]
fn running_jumps_clear_ramps_at_every_phase() {
    for rules in [GroundRules::Bf1, GroundRules::Bf2] {
        for deg in [-40.0, -20.0, 0.0, 10.0, 15.5, 20.0, 30.0, 40.0] {
            for sprint in [false, true] {
                for jump_tick in 30..60 {
                    for max_speed in [6.5, 9.0] {
                        check_ramp_jump(rules, deg, sprint, jump_tick, max_speed);
                    }
                }
            }
        }
    }
}

fn check_ramp_jump(rules: GroundRules, deg: f32, sprint: bool, jump_tick: u32, max_speed: f32) {
    let context = format!("{rules:?} {deg} deg speed {max_speed} sprint {sprint} tick {jump_tick}");
    let (sn, cs) = crate::fmath::sin_cos(deg.to_radians());
    let grade = sn / cs;
    let mut s = Sim::empty(3);
    s.ground_rules = rules;
    let positions = [
        [-100.0, 50.0 - 100.0 * grade, 100.0],
        [100.0, 50.0 - 100.0 * grade, 100.0],
        [100.0, 50.0 + 100.0 * grade, -100.0],
        [-100.0, 50.0 + 100.0 * grade, -100.0],
    ];
    s.collision = CollisionWorld::new(
        vec![Body::new(
            Mat4::IDENTITY,
            Shape::Mesh(trimesh(&positions, &[0, 1, 2, 0, 2, 3]).unwrap()),
        )],
        Vec::new(),
    );
    let h = s
        .spawn(&UnitSpec::player_in_slot(0, 0, Vec3::new(0.0, 50.0, 0.0)))
        .unwrap();
    s.unit_mut(h).unwrap().consts.max_speed = max_speed;
    for _ in 0..2 {
        s.step(&[PadState::default()]);
    }
    let mut pad = PadState {
        ly: i16::MAX,
        buttons: if sprint { buttons::SPRINT } else { 0 },
        ..PadState::default()
    };
    for _ in 0..jump_tick {
        s.step(&[pad]);
    }
    let before = s.unit(h).unwrap().soldier.clone();
    assert!(before.grounded, "{context}: run-up {before:?}");
    pad.buttons |= buttons::JUMP;
    s.step(&[pad]);
    let after = &s.unit(h).unwrap().soldier;
    assert_eq!(
        after.stance,
        Stance::Jump,
        "{context}: before {before:?}, after {after:?}"
    );
    pad.buttons &= !buttons::JUMP;
    for _ in 0..5 {
        s.step(&[pad]);
    }
    let after = &s.unit(h).unwrap().soldier;
    let clearance = after.pos.y - (50.0 - after.pos.z * grade);
    assert!(
        clearance > 0.5,
        "{context}: clearance {clearance}, before {before:?}, after {after:?}"
    );
}
