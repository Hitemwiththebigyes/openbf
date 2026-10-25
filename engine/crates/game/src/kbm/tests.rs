use super::*;
use crate::walk::WalkSim;
use core::sim::InputLog;
use glam::Vec3;

fn grabbed() -> Kbm {
    Kbm {
        grabbed: true,
        ..Kbm::default()
    }
}

#[test]
fn every_bit_the_pad_presses_has_a_key_or_button() {
    let pad = input::Mapping::ps2_default()
        .table
        .iter()
        .fold(0, |a, (_, b)| a | b);
    let pad = pad
        | buttons::SPAWN_POST_UP
        | buttons::SPAWN_POST_DOWN
        | buttons::SPAWN_POST_LEFT
        | buttons::SPAWN_POST_RIGHT;
    let kbm = Bindings::pc_default()
        .table
        .iter()
        .fold(0, |a, (_, b)| a | b.bits);
    assert_eq!(pad & !kbm, 0, "unbound: {:#x}", pad & !kbm);
}

#[test]
fn a_controller_alone_passes_through_untouched() {
    let mut k = Kbm::default();
    let pad = PadState {
        lx: -1200,
        ly: 30000,
        rx: 500,
        ry: -9000,
        buttons: buttons::FIRE | buttons::ZOOM,
        ..PadState::default()
    };
    assert_eq!(k.pad(pad), pad);
    assert_eq!(k.pad(PadState::default()), PadState::default());
}

#[test]
fn wasd_walks_at_full_stick_and_a_diagonal_is_unit_length() {
    let mut k = Kbm::default();
    k.press(Trigger::Key(KeyCode::KeyW), true);
    let p = k.pad(PadState::default());
    assert_eq!((p.lx, p.ly), (0, i16::MAX));
    assert!(p.pressed(buttons::SPAWN_POST_UP));
    k.press(Trigger::Key(KeyCode::KeyA), true);
    let p = k.pad(PadState::default());
    let (x, y) = (PadState::axis(p.lx), PadState::axis(p.ly));
    assert!((x * x + y * y - 1.0).abs() < 1e-3 && x < 0.0 && y > 0.0);
    k.press(Trigger::Key(KeyCode::KeyD), true);
    let p = k.pad(PadState::default());
    assert_eq!(p.lx, 0, "left and right cancel");
    k.press(Trigger::Key(KeyCode::KeyA), false);
    k.press(Trigger::Key(KeyCode::KeyD), false);
    let stick = PadState {
        lx: -20000,
        ly: -5000,
        ..PadState::default()
    };
    let p = k.pad(stick);
    assert_eq!((p.lx, p.ly), (-20000, i16::MAX), "the stronger push wins");
}

#[test]
fn a_tap_shorter_than_a_tick_reaches_the_next_tick() {
    let mut k = Kbm::default();
    k.tap(Trigger::Key(KeyCode::KeyE));
    k.wheel(-1.0);
    let p = k.pad(PadState::default());
    assert!(p.pressed(buttons::ACTION) && p.pressed(buttons::SWITCH_WEAPON));
    k.consumed(&p, 0);
    assert!(k.pad(PadState::default()).pressed(buttons::ACTION));
    k.consumed(&p, 1);
    assert_eq!(k.pad(PadState::default()).buttons, 0);
    k.press(Trigger::Mouse(MouseButton::Left), true);
    k.consumed(&p, 2);
    assert!(k.pad(PadState::default()).pressed(buttons::FIRE), "held");
}

#[test]
fn motion_counts_only_while_grabbed_and_follows_sensitivity_and_invert() {
    let mut k = Kbm::default();
    k.motion(100.0, 100.0);
    assert!(!k.pad(PadState::default()).has_look());
    let mut k = grabbed();
    k.sensitivity = 2.0;
    k.motion(100.0, -50.0);
    let p = k.pad(PadState::default());
    let (yaw, pitch) = p.look();
    assert!((f64::from(yaw) - 100.0 * 2.0 * RAD_PER_COUNT).abs() < 1e-4);
    assert!(pitch > 0.0, "the mouse pushed away looks up");
    assert!(p.mouse_aim());
    k.consumed(&p, 1);
    assert!(!k.pad(PadState::default()).has_look());
    k.invert_y = true;
    k.motion(0.0, -50.0);
    assert!(k.pad(PadState::default()).look().1 < 0.0);
}

#[test]
fn a_deflected_right_stick_takes_the_aim_back_from_the_mouse() {
    let mut k = grabbed();
    k.motion(3.0, 0.0);
    assert!(k.pad(PadState::default()).mouse_aim());
    let stick = PadState {
        rx: 8000,
        ..PadState::default()
    };
    let p = k.pad(stick);
    assert!(!p.mouse_aim());
    k.consumed(&p, 1);
    assert!(!k.pad(PadState::default()).mouse_aim());
}

#[test]
fn a_frames_motion_reaches_one_tick_and_the_record_replays() {
    let mut k = grabbed();
    let mut walk = WalkSim::new(5, Vec3::ZERO, None);
    let mut replay = WalkSim::new(5, Vec3::ZERO, None);
    let start = walk.sim.units[walk.player.index as usize].soldier.yaw;
    let mut log = InputLog::default();
    let mut moved = 0.0f64;
    let dt = 1.0 / 144.0;
    for f in 0..432 {
        let dx = f64::from((f * 37) % 23) - 6.0;
        k.motion(dx, 0.0);
        moved += dx * RAD_PER_COUNT;
        if f % 50 < 20 {
            k.press(Trigger::Key(KeyCode::KeyW), true);
        } else {
            k.press(Trigger::Key(KeyCode::KeyW), false);
        }
        let pad = k.pad(PadState::default());
        let mut first = Some(pad);
        let ticks = walk.advance_with(dt, &[pad], |_| {
            let p = first.take().unwrap_or(PadState {
                mx: 0,
                my: 0,
                ..pad
            });
            log.ticks.push(p);
        });
        k.consumed(&pad, ticks);
    }
    let yaw = walk.sim.units[walk.player.index as usize].soldier.yaw;
    let left = k.pad(PadState::default()).look().0;
    let drawn = f64::from(start - yaw) + f64::from(left);
    assert!(
        (drawn - moved).abs() < 1e-3,
        "turned {drawn}, moved {moved}"
    );
    for pads in log.frames() {
        replay.step_tick(pads);
    }
    assert_eq!(replay.sim.hash(), walk.sim.hash());
}

#[test]
fn a_steady_mouse_turns_the_blended_view_evenly_through_the_ticks() {
    let mut k = grabbed();
    let mut walk = WalkSim::new(5, Vec3::ZERO, None);
    walk.blend.on = true;
    for _ in 0..60 {
        walk.step_tick(&[PadState::default()]);
    }
    k.press(Trigger::Key(KeyCode::KeyW), true);
    let step = 10.0 * RAD_PER_COUNT as f32;
    let mut last: Option<f32> = None;
    let mut ticked = 0;
    for _ in 0..60 {
        k.motion(10.0, 0.0);
        let pad = k.pad(PadState::default());
        let ticks = walk.advance_with(1.0 / 144.0, &[pad], |_| {});
        k.consumed(&pad, ticks);
        ticked += ticks;
        crate::render_blend::blend_in_looking(&mut walk, &k.look());
        let yaw = walk.camera().yaw;
        walk.blend_out();
        if let Some(l) = last {
            assert!(
                (l - yaw - step).abs() < 1e-4,
                "turned {} after {ticks} ticks",
                l - yaw
            );
        }
        last = Some(yaw);
    }
    assert!(ticked >= 10);
}
