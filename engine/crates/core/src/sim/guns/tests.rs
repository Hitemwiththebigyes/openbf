//! Seat guns on flat ground: a skiff-like hover whose pilot seat has no gun
//! and whose three gunner seats carry the BF1 rifle at the skiff's fire
//! points (the baked `hp_firel` / `hp_fire` / `hp_firer`, rounded) with
//! TURRET1..3's limits.

use crate::sim::tests::flat_ground;
use crate::sim::{buttons, Event, PadState, Sim};
use crate::unit::{UnitHandle, UnitSpec};
use crate::vehicle::gun::{Aimer, GunSetup, GunWeapon};
use crate::vehicle::hover::tests::skiff;
use crate::vehicle::{SeatKind, SeatSetup, VehicleHandle, VehicleKind, VehicleSpec};
use crate::weapon::{HealthType, WeaponClass};
use glam::Vec3;

const DEG: f32 = std::f32::consts::PI / 180.0;

fn gun(fire: [f32; 3], pivot: [f32; 3], yaw: [f32; 2], pilot: bool) -> GunSetup {
    GunSetup {
        weapons: vec![GunWeapon {
            class: 0,
            infinite: true,
            spare_clips: 0,
        }],
        fire_point: Vec3::from(fire),
        pivot: Vec3::from(pivot),
        eye: Vec3::new(0.0, 1.0, 2.0),
        aimer: Aimer {
            yaw: [yaw[0] * DEG, yaw[1] * DEG],
            pitch: [-45.0 * DEG, 45.0 * DEG],
            pitch_rate: 0.8,
            ..Aimer::default()
        },
        pilot,
        turn: None,
        framed: false,
    }
}

/// The skiff at the origin facing +Z (yaw 0), the player 6 m ahead of its
/// nose facing it (−Z). `pilot_gun` arms the pilot seat too.
fn world(pilot_gun: bool) -> (Sim, UnitHandle, VehicleHandle) {
    let mut s = Sim::empty(1);
    s.heightfield = Some(flat_ground());
    s.hover_params = vec![Some(skiff())];
    s.add_weapon_class(WeaponClass::bf1_rifle());
    s.seat_setup = vec![Some(SeatSetup {
        radius: 5.0,
        boardable: vec![true; 4],
        exit_drop: 10.0,
        ai_size: 1,
        guns: vec![
            pilot_gun.then(|| gun([0.0, 0.9, 4.2], [0.0, 0.8, 3.3], [0.0, 0.0], true)),
            Some(gun([2.3, 0.9, 0.0], [1.4, 0.8, 0.0], [65.0, 115.0], false)),
            Some(gun([0.0, 0.9, 4.2], [0.0, 0.8, 3.3], [-25.0, 25.0], false)),
            Some(gun(
                [-2.5, 0.9, 0.0],
                [-1.5, 0.8, 0.0],
                [-115.0, -65.0],
                false,
            )),
        ],
    })];
    let v = s.spawn_vehicle(&VehicleSpec {
        kind: VehicleKind::Hover,
        class: 0,
        home_team: None,
        neutral_when_empty: true,
        spawn_pad: None,
        pos: Vec3::new(0.0, 2.0, 0.0),
        yaw: 0.0,
        max_health: 1000.0,
        health_type: HealthType::Vehicle,
        seats: vec![
            SeatKind::Pilot,
            SeatKind::Gunner(0),
            SeatKind::Gunner(1),
            SeatKind::Gunner(2),
        ],
    });
    let p = s
        .spawn(&UnitSpec::player(0, Vec3::new(0.0, 0.0, 6.0)))
        .unwrap();
    (s, p, v)
}

fn pad(buttons: u32) -> PadState {
    PadState {
        buttons,
        ..PadState::default()
    }
}

/// Press and release `b`.
fn tap(s: &mut Sim, b: u32) {
    s.step(&[pad(b)]);
    s.step(&[pad(0)]);
}

fn seat(s: &Sim, p: UnitHandle) -> SeatKind {
    s.seat_of(p).expect("seated").1
}

#[test]
fn switch_walks_the_ring_to_the_next_free_seat() {
    let (mut s, p, _) = world(false);
    tap(&mut s, buttons::ACTION);
    assert_eq!(seat(&s, p), SeatKind::Pilot);
    for want in [
        SeatKind::Gunner(0),
        SeatKind::Gunner(1),
        SeatKind::Gunner(2),
        SeatKind::Pilot,
    ] {
        tap(&mut s, buttons::SWITCH_WEAPON);
        assert_eq!(seat(&s, p), want);
    }
    // A held button is one edge.
    s.step(&[pad(buttons::SWITCH_WEAPON)]);
    s.step(&[pad(buttons::SWITCH_WEAPON)]);
    assert_eq!(seat(&s, p), SeatKind::Gunner(0));
}

#[test]
fn nobody_human_aboard_leaves_the_guns_unmade() {
    let (mut s, _, v) = world(true);
    let h0 = s.hash();
    s.step(&[pad(0)]);
    assert!(s.vehicle(v).unwrap().guns.is_empty());
    let (mut t, _, _) = world(false);
    t.step(&[pad(0)]);
    assert_eq!(s.hash(), t.hash(), "setup alone does not reach the hash");
    assert_ne!(h0, s.hash());
}

#[test]
fn the_nose_gunner_fires_from_its_fire_point_along_its_aimer() {
    let (mut s, p, v) = world(false);
    tap(&mut s, buttons::ACTION);
    tap(&mut s, buttons::SWITCH_WEAPON);
    tap(&mut s, buttons::SWITCH_WEAPON);
    assert_eq!(seat(&s, p), SeatKind::Gunner(1));
    s.events.clear();
    s.step(&[pad(buttons::FIRE)]);
    let shot = s
        .events
        .iter()
        .find_map(|e| match e {
            Event::Shot {
                unit, origin, dir, ..
            } if *unit == p => Some((*origin, *dir)),
            _ => None,
        })
        .expect("a shot");
    let veh = s.vehicle(v).unwrap();
    let fp = veh.pos + veh.rot * Vec3::new(0.0, 0.9, 4.2);
    assert!(shot.0.distance(fp) < 0.2, "origin {:?} vs {fp:?}", shot.0);
    assert!(shot.1.z > 0.9, "straight ahead: {:?}", shot.1);
    assert!(!s.bolts.is_empty());
}

#[test]
fn the_aimer_follows_the_look_at_its_rate_and_stops_at_its_limit() {
    let (mut s, _, v) = world(false);
    tap(&mut s, buttons::ACTION);
    tap(&mut s, buttons::SWITCH_WEAPON);
    tap(&mut s, buttons::SWITCH_WEAPON);
    let right = PadState {
        rx: PadState::quantize(1.0),
        ..PadState::default()
    };
    s.step(&[right]);
    let g = s.vehicle(v).unwrap().guns[2].clone().unwrap();
    assert!(g.look.0 < 0.0, "stick right lowers yaw: {:?}", g.look);
    assert!(g.aim.0 <= 0.0 && g.aim.0 >= -2.0 * crate::TICK_DT - 1e-6);
    for _ in 0..60 {
        s.step(&[right]);
    }
    let g = s.vehicle(v).unwrap().guns[2].clone().unwrap();
    assert!((g.look.0 + 25.0 * DEG).abs() < 1e-5, "look clamped");
    assert!(
        (g.aim.0 + 25.0 * DEG).abs() < 1e-3,
        "aim clamped: {}",
        g.aim.0
    );
}

fn mouse(yaw: f32, pitch: f32) -> PadState {
    PadState {
        mx: PadState::quantize_look(f64::from(yaw)),
        my: PadState::quantize_look(f64::from(pitch)),
        buttons: buttons::MOUSE_AIM,
        ..PadState::default()
    }
}

#[test]
fn a_gunners_mouse_moves_the_look_by_its_angle_inside_the_limits() {
    let (mut s, p, v) = world(false);
    tap(&mut s, buttons::ACTION);
    tap(&mut s, buttons::SWITCH_WEAPON);
    tap(&mut s, buttons::SWITCH_WEAPON);
    let look = |s: &Sim| s.vehicle(v).unwrap().guns[2].clone().unwrap().look;
    let before = look(&s);
    let pad = mouse(0.3, 0.1);
    let preview = s.look_preview(p, &pad);
    s.step(&[pad]);
    let after = look(&s);
    let (dy, dp) = pad.look();
    assert!(
        (before.0 - dy - after.0).abs() < 1e-6,
        "{before:?} {after:?}"
    );
    assert!((before.1 + dp - after.1).abs() < 1e-6);
    assert_eq!(
        preview,
        Some(crate::sim::LookPreview::Seat {
            vehicle: v.index as usize,
            seat: 2,
            look: after,
            walker: false,
        })
    );
    s.step(&[mouse(-3.0, 3.0)]);
    assert_eq!(look(&s), (25.0 * DEG, 45.0 * DEG));
}

#[test]
fn a_turrets_mouse_look_wraps_and_holds_its_pitch_limits() {
    let (mut s, v) = dish_world();
    let p = s
        .spawn(&UnitSpec::player(0, Vec3::new(0.0, 0.0, 3.0)))
        .unwrap();
    assert!(s.enter_vehicle(p, v, 0));
    let look = |s: &Sim| s.vehicle(v).unwrap().guns[0].clone().unwrap().look;
    for _ in 0..5 {
        s.step(&[mouse(1.0, 0.0)]);
    }
    let l = look(&s);
    assert!(l.0.abs() <= std::f32::consts::PI, "wrapped {l:?}");
    assert!((crate::vehicle::gun::wrap(l.0 + 5.0)).abs() < 1e-4, "{l:?}");
    s.step(&[mouse(0.0, -2.0)]);
    assert!((look(&s).1 + 35.0 * DEG).abs() < 1e-5);
    s.step(&[mouse(0.0, 2.0)]);
    assert!((look(&s).1 - 10.0 * DEG).abs() < 1e-5);
}

#[test]
fn a_side_gun_rests_across_the_hull() {
    let (mut s, _, v) = world(false);
    tap(&mut s, buttons::ACTION);
    tap(&mut s, buttons::SWITCH_WEAPON);
    s.step(&[pad(buttons::FIRE)]);
    let g = s.vehicle(v).unwrap().guns[1].clone().unwrap();
    assert!((g.aim.0 - 90.0 * DEG).abs() < 1e-2, "{:?}", g.aim);
    let d = s.bolts.last().expect("a bolt").vel.normalize();
    assert!(d.x > 0.9, "the left gun fires out along +x: {d:?}");
}

#[test]
fn the_pilot_aims_straight_ahead() {
    let (mut s, p, v) = world(true);
    tap(&mut s, buttons::ACTION);
    assert_eq!(seat(&s, p), SeatKind::Pilot);
    let stick = PadState {
        rx: PadState::quantize(1.0),
        ry: PadState::quantize(1.0),
        ..PadState::default()
    };
    for _ in 0..10 {
        s.step(&[stick]);
    }
    let g = s.vehicle(v).unwrap().guns[0].clone().unwrap();
    assert_eq!(g.aim.0, 0.0);
    assert_eq!(g.look.0, 0.0);
    assert!(g.look.1 > 0.0, "the look stick pitches the aim");
}

#[test]
fn a_gunners_bolt_hits_the_soldier_it_aims_at() {
    let (mut s, p, v) = world(false);
    let enemy = s
        .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, 30.0)))
        .unwrap();
    tap(&mut s, buttons::ACTION);
    tap(&mut s, buttons::SWITCH_WEAPON);
    tap(&mut s, buttons::SWITCH_WEAPON);
    let hp = s.unit(enemy).unwrap().health.cur;
    // Aim down onto the soldier's chest, then fire.
    let down = PadState {
        ry: PadState::quantize(-1.0),
        ..PadState::default()
    };
    let veh = s.vehicle(v).unwrap();
    let eye = veh.pos + Vec3::new(0.0, 0.8, 3.3) + Vec3::new(0.0, 1.0, 2.0);
    let chest = Vec3::new(0.0, 1.0, 30.0);
    let want = ((chest - eye).y / (chest - eye).length()).asin();
    while s
        .vehicle(v)
        .unwrap()
        .guns
        .get(2)
        .and_then(|g| g.as_ref())
        .is_none_or(|g| g.look.1 > want)
    {
        s.step(&[down]);
    }
    for _ in 0..30 {
        s.step(&[pad(buttons::FIRE)]);
    }
    assert!(s.seat_of(p).is_some());
    assert!(
        s.unit(enemy).is_none_or(|u| u.health.cur < hp),
        "the enemy took the gunner's bolts"
    );
}

/// Card VTAI: a bot in the nose gunner's seat (a human pilots) turns its
/// look toward the enemy it sees and fires, every shot at a perceived
/// target; a wall between them means no shot at all.
#[test]
fn a_bot_gunner_aims_and_fires_at_what_it_sees() {
    let run = |wall: bool| {
        let (mut s, _, v) = world(false);
        if wall {
            s.collision.push_body(crate::collision::Body::new(
                glam::Mat4::from_translation(Vec3::new(0.0, 5.0, 15.0)),
                crate::collision::Shape::Box {
                    half: Vec3::new(20.0, 5.0, 0.5),
                },
            ));
        }
        tap(&mut s, buttons::ACTION);
        let b = s
            .spawn(&UnitSpec {
                controller: crate::unit::Controller::Ai,
                ..UnitSpec::trooper(0, Vec3::new(3.0, 0.0, 0.0))
            })
            .unwrap();
        assert!(s.enter_vehicle(b, v, 2));
        let enemy = s
            .spawn(&UnitSpec::trooper(1, Vec3::new(5.0, 0.0, 30.0)))
            .unwrap();
        let hp = s.unit(enemy).unwrap().health.cur;
        // The peak look yaw: once the enemy is down the look goes back to rest.
        let mut look = 0.0f32;
        for _ in 0..300 {
            s.step(&[pad(0)]);
            if let Some(Some(g)) = s.vehicle(v).unwrap().guns.get(2) {
                look = look.max(g.look.0);
            }
        }
        assert_eq!(s.nav_stats.unperceived_shots, 0);
        assert!(s.unit(b).unwrap().mount.is_some(), "the gunner stays");
        let hurt = s.unit(enemy).is_none_or(|u| u.health.cur < hp);
        (s.nav_stats.seat_shots, look, hurt)
    };
    let (shots, look, hurt) = run(false);
    // Card CLSE: a bot that rides with a human aims with no error (BF2,
    // `analysis/re-soldier-ai.md` 17.2): the enemy is down in 5 shots.
    assert!(shots >= 4, "{shots} seat shots");
    assert!(look > 5.0 * DEG, "the look turned toward +x: {look}");
    assert!(hurt, "the bolts reached the enemy");
    let (shots, _, hurt) = run(true);
    assert_eq!(shots, 0, "no shot through the wall");
    assert!(!hurt);
}

/// Card VTURL: a BF2 Hoth dish turret (`YawLimits -180 180`, `PitchLimits
/// -10 35`, speeds 1, `AimerYawLimits 0 0`, `AimerPitchLimits -15 40`) at
/// the origin facing +Z, its gunner seat armed with the rifle.
fn dish_world() -> (Sim, VehicleHandle) {
    use crate::vehicle::gun::{Slew, Turn};
    let mut s = Sim::empty(1);
    s.heightfield = Some(flat_ground());
    s.hover_params = vec![None];
    s.add_weapon_class(WeaponClass::bf1_rifle());
    let mut g = gun([0.0, 2.0, 2.0], [0.0, 2.0, 0.0], [0.0, 0.0], false);
    g.aimer.pitch = [-15.0 * DEG, 40.0 * DEG];
    g.turn = Some(Turn::from_class(
        Some([-180.0, 180.0]),
        Some([-10.0, 35.0]),
        (1.0, 1.0),
        (10.0, 10.0),
        Slew::Linear,
    ));
    g.framed = true;
    s.seat_setup = vec![Some(SeatSetup {
        radius: 3.0,
        boardable: vec![true],
        exit_drop: 15.0,
        ai_size: 1,
        guns: vec![Some(g)],
    })];
    let v = s.spawn_vehicle(&VehicleSpec {
        kind: VehicleKind::Turret,
        class: 0,
        home_team: None,
        neutral_when_empty: true,
        spawn_pad: None,
        pos: Vec3::ZERO,
        yaw: 0.0,
        max_health: 1000.0,
        health_type: HealthType::Building,
        seats: vec![SeatKind::Gunner(0)],
    });
    (s, v)
}

/// The traced fault (card VTURL): the look held at -180 and the aimer
/// walking back through 0. Now a held stick turns the dish through the
/// seam and on, at 1 rad/s, and the aimer (no yaw of its own) points where
/// the turret does on every tick.
#[test]
fn a_full_circle_turret_turns_past_180_and_its_gun_follows() {
    let (mut s, v) = dish_world();
    let p = s
        .spawn(&UnitSpec::player(0, Vec3::new(0.0, 0.0, 3.0)))
        .unwrap();
    assert!(s.enter_vehicle(p, v, 0));
    let mut prev: Option<(f32, f32)> = None;
    let mut turned = 0.0;
    let mut worst = 0.0f32;
    for _ in 0..240 {
        s.step(&[PadState {
            rx: PadState::quantize(1.0),
            ..PadState::default()
        }]);
        let g = s.vehicle(v).unwrap().guns[0].clone().unwrap();
        if let Some(l) = prev {
            turned += crate::vehicle::gun::wrap(g.look.0 - l.0);
        }
        prev = Some(g.look);
        worst = worst.max(crate::vehicle::gun::wrap(g.aim.0 - g.look.0).abs());
    }
    assert!(turned < -7.0, "turned {turned} rad in 8 s");
    assert!(
        worst < 1e-3,
        "the aimer left the turret's heading by {worst}"
    );
    // Stick down: the dish's nose goes DOWN to 35 degrees (the PS2's turret
    // pitch is + = nose down), up only to 10.
    for (ry, want) in [(-1.0, -35.0), (1.0, 10.0)] {
        for _ in 0..90 {
            s.step(&[PadState {
                ry: PadState::quantize(ry),
                ..PadState::default()
            }]);
        }
        let g = s.vehicle(v).unwrap().guns[0].clone().unwrap();
        assert!((g.look.1 - want * DEG).abs() < 1e-4, "{ry}: {:?}", g.look);
    }
}

/// Card TURR: BF2 Mygeeto's recoilless turret (`tur_bldg_recoilless_lg`):
/// the pivot `aimer_y` at the origin, `hp_fire1` 3.39 m over it and 1.28 m
/// to the side, `EyePointOffset 1.0 4.2 -8.3`, `AimerYawLimits 0 0`,
/// `AimerPitchLimits -30 20`, `PitchLimits -30 20`, turned at 1.5 rad/s;
/// at the origin facing +Z. `building`: the turret's own building as a map
/// body round the camera's eye (instance 7, the turret's), as the bake
/// builds it.
fn recoilless_gun() -> GunSetup {
    use crate::vehicle::gun::{Slew, Turn};
    let mut g = gun([1.281, 3.388, 3.372], [0.0, 0.0, 0.0], [0.0, 0.0], false);
    g.eye = Vec3::new(1.0, 4.2, -8.3);
    g.aimer.pitch = [-30.0 * DEG, 20.0 * DEG];
    g.aimer.pitch_rate = 1.0;
    g.turn = Some(Turn::from_class(
        Some([-180.0, 180.0]),
        Some([-30.0, 20.0]),
        (1.5, 1.0),
        (10.0, 10.0),
        Slew::Linear,
    ));
    g.framed = true;
    g
}

fn recoilless_building() -> crate::collision::Body {
    use crate::collision::{Body, Shape};
    let mut b = Body::new(
        glam::Mat4::from_translation(Vec3::new(0.0, 3.0, -5.0)),
        Shape::Box {
            half: Vec3::new(3.0, 3.0, 6.0),
        },
    );
    b.instance = Some(7);
    b
}

fn recoilless_world(building: bool) -> (Sim, VehicleHandle) {
    let (mut s, v) = dish_world();
    s.seat_setup[0].as_mut().unwrap().guns[0] = Some(recoilless_gun());
    if building {
        s.collision =
            crate::collision::CollisionWorld::new(vec![recoilless_building()], Vec::new());
        s.vehicle_mut(v).unwrap().instance = Some(7);
    }
    (s, v)
}

/// How far `p` is from the barrel's line (from the muzzle along the barrel).
fn off_the_barrel(s: &Sim, u: UnitHandle, p: Vec3) -> f32 {
    let l = s.seat_look(u).unwrap();
    let w = p - l.muzzle;
    (w - l.barrel * w.dot(l.barrel)).length()
}

/// Card TURR: the camera ray of a building turret passes its own building
/// (the PS2's excludes the turret's owner entity, B1 `FUN_00223850`
/// B1:138064, BF2 `FUN_0026cea0` B2:189959): a human looking level from an
/// eye inside his turret's building lays the barrel on the ray's far end,
/// 512 m out. The same building as no one's map body (instance 8) stops the
/// ray at the eye and the barrel runs to its limit. And the aimer lays the
/// BARREL's line on that point, not the pivot's (PS2 RAM, Mygeeto: 0.004 m
/// off the barrels' line, 3.5 m off the pivot's): it passes 0.281 m beside
/// it, the fire point's offset from the eye's line (1.281 against 1.0 m),
/// which an aimer with no yaw of its own keeps; none above or below.
#[test]
fn a_turrets_camera_ray_passes_its_own_building_and_its_barrel_meets_the_point() {
    for own in [true, false] {
        let (mut s, v) = recoilless_world(true);
        if !own {
            s.vehicle_mut(v).unwrap().instance = Some(8);
        }
        let p = s
            .spawn(&UnitSpec::player(0, Vec3::new(0.0, 0.0, 4.0)))
            .unwrap();
        assert!(s.enter_vehicle(p, v, 0));
        for _ in 0..90 {
            s.step(&[PadState::default()]);
        }
        let g = s.vehicle(v).unwrap().guns[0].clone().unwrap();
        assert_eq!(g.look, (0.0, 0.0), "the look rests level");
        let far = Vec3::new(1.0, 4.2, -8.3) + Vec3::Z * crate::vehicle::gun::AIM_RAY;
        let off = off_the_barrel(&s, p, far);
        if own {
            assert!(
                (off - 0.281).abs() < 0.01,
                "the barrel's line passes {off} m off the point"
            );
            assert!(g.aim.1.abs() < 1.0 * DEG, "aim {:?}", g.aim);
        } else {
            assert!(off > 10.0, "{off}");
            assert!(
                (g.aim.1 - 20.0 * DEG).abs() < 1e-3 || (g.aim.1 + 30.0 * DEG).abs() < 1e-3,
                "the aimer ran to a limit: {:?}",
                g.aim
            );
        }
    }
}

/// Card TURR: a building turret's barrel hangs off its aimer at its bind
/// offset (re-turret-draw 2.3, `lead_tmp/vturd/replay_aimer.py` A3): the
/// muzzle at rest IS the fire point, 1.28 m to the side - not turned onto
/// the pivot's axis by its heading, as a vehicle's side gun is.
#[test]
fn a_building_turrets_muzzle_rests_at_its_fire_point() {
    let g = recoilless_gun();
    assert!(g.muzzle((0.0, 0.0)).distance(g.fire_point) < 1e-5);
    let m = g.muzzle((90.0 * DEG, 0.0));
    assert!(m.distance(Vec3::new(3.372, 3.388, -1.281)) < 1e-3, "{m}");
}
