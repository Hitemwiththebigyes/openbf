use super::*;
use crate::remote::tests::Props;
use crate::TICK_DT;

pub(crate) const BF2_TURRET: &[(&str, &str)] = &[
    ("MaxHealth", "50.0"),
    ("Lifetime", "60.0"),
    ("PitchLimits", "-35 35"),
    ("YawLimits", "-180 180"),
    ("MaxTurnSpeed", "3.8"),
    ("MaxPitchSpeed", "3.8"),
    ("GeometryName", "com_weap_inf_dropturret"),
];

fn class() -> TurretClass {
    TurretClass::new(&Props(BF2_TURRET), WeaponClass::bf1_rifle())
}

fn turret(c: &TurretClass) -> Turret {
    Turret::new(
        1,
        UnitHandle { index: 0, gen: 0 },
        0,
        0,
        c,
        Vec3::new(0.0, 5.0, 0.0),
        Vec3::ZERO,
        0.0,
    )
}

#[test]
fn the_class_reads_the_disc_and_keeps_the_constructor_defaults() {
    let c = class();
    let b = c.body;
    assert_eq!((b.max_health, b.lifetime), (50.0, 60.0));
    assert_eq!((b.gravity, b.rebound, b.friction), (-9.8, 0.3, 0.5));
    assert_eq!((b.turn_speed, b.pitch_speed), (3.8, 3.8));
    assert!((b.pitch_limits[1] - 35.0 * DEG).abs() < 1e-6);
    assert!(b.wraps());
    assert_eq!(b.gains(), (1.5, 2.0));
    assert_eq!(c.health_type, HealthType::Building);
    assert_eq!(c.geometry.as_deref(), Some("com_weap_inf_dropturret"));
    let bare = TurretBody::from_props(&Props(&[]));
    assert_eq!(bare.lifetime, f32::MAX);
    assert_eq!((bare.turn_speed, bare.pitch_speed), (2.0, 0.5));
}

#[test]
fn it_ages_out_at_its_lifetime_and_dies_at_no_health() {
    let c = class();
    let mut t = turret(&c);
    let mut secs = 0.0;
    while t.grow_old(&c.body, TICK_DT).is_none() {
        secs += TICK_DT;
        assert!(secs < 61.0);
    }
    assert!((secs - 60.0).abs() < 0.05, "{secs}");
    let mut t = turret(&c);
    t.health = 0.0;
    assert_eq!(t.grow_old(&c.body, TICK_DT), Some(TurretEnd::Destroyed));
}

#[test]
fn landing_stands_it_on_the_surface_and_a_steep_wall_bounces_it() {
    let c = class();
    let mut t = turret(&c);
    t.yaw = 0.4;
    let n = Vec3::new(0.3, 1.0, 0.0).normalize();
    t.land(Vec3::new(1.0, 2.0, 3.0), n);
    assert!(t.landed);
    assert!(t.pos.abs_diff_eq(Vec3::new(1.0, 2.0, 3.0), 1e-5));
    assert_eq!((t.up, t.vel, t.base_yaw), (n, Vec3::ZERO, 0.4));
    let mut t = turret(&c);
    t.vel = Vec3::new(4.0, -1.0, 0.0);
    t.bounce(&c.body, Vec3::new(2.0, 1.0, 0.0), Vec3::NEG_X, 0.01);
    assert!(t.vel.x < 0.0 && t.vel.x.abs() < 4.0, "{:?}", t.vel);
    assert!(!t.landed);
}

#[test]
fn the_turn_follows_the_hover_stick_law_and_holds_the_pitch_limits() {
    let c = class();
    let mut t = turret(&c);
    let mut s = t.clone();
    t.turn(&c.body, TICK_DT, (3.0, 0.0));
    assert!((t.yaw - 3.8 * TICK_DT).abs() < 1e-6, "behind: full rate");
    s.turn(&c.body, TICK_DT, (0.2, 0.0));
    let k = crate::fmath::sin_cos(0.2).0 * 1.5;
    assert!((s.yaw - k * k * 3.8 * TICK_DT).abs() < 1e-6);
    for _ in 0..90 {
        t.turn(&c.body, TICK_DT, (-1.0, 1.2));
    }
    assert!((t.yaw + 1.0).abs() < 0.2, "{}", t.yaw);
    assert!((t.pitch - 35.0 * DEG).abs() < 1e-6);
    let p = t.centre() + t.aim_frame().0 * 10.0;
    assert!(t.on_target(p));
    assert!(!t.on_target(t.centre() - t.aim_frame().0 * 10.0));
}
