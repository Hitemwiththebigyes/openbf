use crate::sim::tests::flat_ground;
use crate::sim::{buttons, InputLog, LookPreview, PadState, Sim, LOOK_UNIT};
use crate::soldier::Stance;
use crate::unit::UnitSpec;
use glam::Vec3;

fn foot() -> Sim {
    let mut s = Sim::empty(7);
    s.heightfield = Some(flat_ground());
    s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
    s.units[0].soldier.pitch = 0.0;
    s.step(&[PadState::default()]);
    s
}

fn mouse(yaw: f64, pitch: f64) -> PadState {
    PadState {
        mx: PadState::quantize_look(yaw),
        my: PadState::quantize_look(pitch),
        buttons: buttons::MOUSE_AIM,
        ..Default::default()
    }
}

fn aim(s: &Sim) -> (f32, f32) {
    (s.units[0].soldier.yaw, s.units[0].soldier.pitch)
}

#[test]
fn mouse_aim_keeps_the_player_alive_until_explicit_respawn() {
    let mut s = foot();
    let player = s.handle(0);
    let pad = mouse(0.3, -0.1);
    s.step(&[pad]);
    assert!(s.unit(player).is_some());
    s.step(&[PadState {
        buttons: pad.buttons | buttons::RESPAWN,
        ..pad
    }]);
    assert!(s.unit(player).is_none());
}

#[test]
fn mouse_look_turns_by_its_angle_past_the_turn_rate() {
    let mut s = foot();
    let cap = s.units[0].consts.max_turn_speed * crate::TICK_DT;
    let pad = mouse(1.0, 0.25);
    let (yaw, pitch) = aim(&s);
    s.step(&[pad]);
    let (turned, raised) = (yaw - aim(&s).0, aim(&s).1 - pitch);
    assert_eq!(turned, pad.mx as f32 * LOOK_UNIT);
    assert_eq!(raised, pad.my as f32 * LOOK_UNIT);
    assert!(turned > 3.0 * cap, "{turned} against the stick's {cap}");
    s.step(&[mouse(-0.5, 0.0)]);
    assert!((aim(&s).0 - (yaw - 0.5)).abs() < 1e-5);
}

#[test]
fn mouse_pitch_holds_the_limits_and_the_prone_clamp() {
    let mut s = foot();
    let (lo, hi) = s.units[0].consts.pitch_limits;
    s.step(&[mouse(0.0, 4.0)]);
    assert_eq!(aim(&s).1, hi);
    s.step(&[mouse(0.0, -8.0)]);
    assert_eq!(aim(&s).1, lo);
    s.units[0].soldier.pitch = 0.0;
    s.units[0].soldier.stance = Stance::Prone;
    s.step(&[mouse(0.0, 2.0)]);
    assert_eq!(aim(&s).1, 0.4);
    s.step(&[mouse(0.0, -2.0)]);
    assert_eq!(aim(&s).1, -0.4);
}

#[test]
fn the_zoom_divides_mouse_look_by_its_magnification() {
    let mut s = foot();
    let zoom = PadState {
        buttons: buttons::ZOOM | buttons::MOUSE_AIM,
        ..Default::default()
    };
    s.step(&[zoom]);
    s.step(&[mouse(0.0, 0.0)]);
    let z = s.units[0].zoom.expect("zoomed").value;
    assert!(z > 1.0);
    let yaw = aim(&s).0;
    let pad = mouse(0.5, 0.0);
    s.step(&[pad]);
    let want = pad.mx as f32 * LOOK_UNIT / z;
    assert!(((yaw - aim(&s).0) - want).abs() < 1e-6, "{z}");
}

#[test]
fn the_preview_is_what_the_tick_does() {
    let mut s = foot();
    let h = s.handle(0);
    for pad in [mouse(0.3, -0.1), mouse(-2.0, 3.0), mouse(0.0, 0.0)] {
        let preview = s.look_preview(h, &pad);
        s.step(&[pad]);
        match preview {
            Some(LookPreview::Foot { unit, yaw, pitch }) => {
                assert_eq!(unit, 0);
                assert_eq!((yaw, pitch), aim(&s));
            }
            None => assert!(!pad.has_look()),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn a_log_without_mouse_fields_loads_and_writes_none() {
    let old = "(seed: 7, ticks: [(lx: 0, ly: 20000, rx: 3000, ry: -200, buttons: 1), \
               (lx: 5, ly: 0, rx: 0, ry: 0, buttons: 0)])";
    let log: InputLog = ron::from_str(old).unwrap();
    assert_eq!(log.tick_count(), 2);
    assert!(log.ticks.iter().all(|p| !p.has_look()));
    assert_eq!((log.ticks[0].rx, log.ticks[0].buttons), (3000, 1));
    let text = ron::to_string(&log).unwrap();
    assert!(!text.contains("mx") && !text.contains("my"), "{text}");
}

#[test]
fn a_mouse_log_replays_to_the_same_sim() {
    let log = InputLog {
        seed: 11,
        slots: 1,
        ticks: (0..240)
            .map(|i| PadState {
                ly: if i % 50 < 30 { 20000 } else { 0 },
                mx: ((i * 911) % 4000) - 2000,
                my: ((i * 433) % 1200) - 600,
                buttons: buttons::MOUSE_AIM | if i % 9 < 2 { buttons::FIRE } else { 0 },
                ..Default::default()
            })
            .collect(),
    };
    let text = ron::to_string(&log).unwrap();
    let back: InputLog = ron::from_str(&text).unwrap();
    assert_eq!(back.ticks, log.ticks);
    let run = |log: &InputLog| {
        let mut s = foot();
        for pads in log.frames() {
            s.step(pads);
        }
        (s.hash(), aim(&s))
    };
    let (a, b) = (run(&log), run(&back));
    assert_eq!(a, b);
    let still = InputLog {
        ticks: log
            .ticks
            .iter()
            .map(|p| PadState { mx: 0, my: 0, ..*p })
            .collect(),
        ..log.clone()
    };
    assert_ne!(run(&still).1, a.1, "the mouse turned the soldier");
}
