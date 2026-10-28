use super::*;
use crate::TICK_DT;

pub(crate) struct Props(pub &'static [(&'static str, &'static str)]);

impl PropSource for Props {
    fn prop(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| *v)
    }
    fn prop_hash(&self, hash: u32) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| u32::from_str_radix(k.trim_start_matches("0x"), 16) == Ok(hash))
            .map(|(_, v)| *v)
    }
}

pub(crate) const BF1_DROID: &[(&str, &str)] = &[
    ("MaxHealth", "40.0"),
    ("AddHealth", "-1.0"),
    ("SetAltitude", "1.75"),
    ("GravityScale", "1.0//2"),
    ("LiftSpring", "10.0"),
    ("LiftDamp", "3.0"),
    ("acceleration", "1.0"),
    ("LevelSpring", "5.0"),
    ("LevelDamp", "3.0"),
    ("EyePointOffset", "0.0 0.5 0.0"),
    ("TrackOffset", "0.0 -0.25 1.5"),
    ("PitchLimits", "-50.0 50.0"),
];

pub(crate) const BF2_DROID: &[(&str, &str)] = &[
    ("MaxHealth", "100.0"),
    ("AddHealth", "-1.0"),
    ("EnergyAutoRestore", "-1.7"),
    ("SetAltitude", "2.0"),
    ("acceleration", "25.0"),
    ("TrackOffset", "0.0 -0.25 1.8"),
];

fn hover(c: &DroidClass, d: &mut Droid, forward: f32, ticks: usize) {
    for _ in 0..ticks {
        let ground = Some((d.pos.y, Vec3::Y));
        d.fly(c, TICK_DT, forward, 0.0, ground);
    }
}

fn droid(c: &DroidClass, y: f32) -> Droid {
    let r = RemoteWeapon::new(&Props(&[]), c.clone(), WeaponClass::bf1_rifle());
    Droid::new(
        1,
        UnitHandle { index: 0, gen: 0 },
        0,
        0,
        &r,
        Vec3::Y * y,
        Vec3::ZERO,
        0.0,
        true,
    )
}

#[test]
fn bf1_reads_only_the_misspelled_acceleration() {
    let bf1 = DroidClass::from_props(&Props(BF1_DROID), false);
    assert_eq!(bf1.acceleration, 70.0);
    assert_eq!(bf1.gravity_scale, 1.0);
    assert!((bf1.pitch_limits[1] - 50.0 * DEG).abs() < 1e-6);
    assert_eq!(bf1.track_offset, Vec3::new(0.0, -0.25, 1.5));
    assert_eq!(bf1.energy_restore, 0.0);
    assert_eq!(bf1.lifetime(), 40.0);
    let bf2 = DroidClass::from_props(&Props(BF2_DROID), true);
    assert_eq!(bf2.acceleration, 25.0);
    assert!((bf2.lifetime() - 100.0 / 1.7).abs() < 1e-3);
    assert_eq!(bf2.max_speed, 9.0);
    assert_eq!(bf2.max_strafe_speed, 7.0);
}

#[test]
fn droid_settles_just_over_its_set_altitude() {
    for (props, bf2, want) in [(BF1_DROID, false, 1.75), (BF2_DROID, true, 2.0)] {
        let c = DroidClass::from_props(&Props(props), bf2);
        let mut d = droid(&c, 0.5);
        hover(&c, &mut d, 0.0, 600);
        let lift = c.lift_damp * GRAVITY * TICK_DT / (c.lift_spring + GRAVITY / (9.0 * want));
        assert!(
            (d.pos.y - want - lift).abs() < 0.005,
            "bf2={bf2}: {}",
            d.pos.y
        );
        assert!(d.vel.length() < 0.01, "{:?}", d.vel);
    }
}

#[test]
fn stick_drives_the_droid_to_max_speed_along_its_heading() {
    let c = DroidClass::from_props(&Props(BF1_DROID), false);
    let mut d = droid(&c, 1.75);
    hover(&c, &mut d, 1.0, 60);
    assert!((d.vel.z + c.max_speed).abs() < 0.05, "{:?}", d.vel);
    assert!(d.vel.x.abs() < 1e-4);
    d.yaw = std::f32::consts::FRAC_PI_2;
    hover(&c, &mut d, 1.0, 60);
    assert!((d.vel.x + c.max_speed).abs() < 0.05, "{:?}", d.vel);
}

#[test]
fn droid_out_of_reach_of_the_ground_falls_and_cannot_steer() {
    let c = DroidClass::from_props(&Props(BF1_DROID), false);
    let mut d = droid(&c, 100.0);
    d.fly(&c, TICK_DT, 1.0, 1.0, Some((100.0, Vec3::Y)));
    assert!((d.vel.y + GRAVITY * TICK_DT).abs() < 1e-5);
    assert_eq!(d.vel.with_y(0.0), Vec3::ZERO);
}

#[test]
fn drain_ends_the_droid_when_its_health_or_energy_runs_out() {
    for (props, bf2) in [(BF1_DROID, false), (BF2_DROID, true)] {
        let c = DroidClass::from_props(&Props(props), bf2);
        let mut d = droid(&c, 1.0);
        let ticks = (c.lifetime() / TICK_DT).ceil() as usize;
        for _ in 0..ticks - 2 {
            assert_eq!(d.drain(&c, TICK_DT), None);
        }
        let mut end = None;
        for _ in 0..4 {
            end = end.or(d.drain(&c, TICK_DT));
        }
        assert_eq!(end, Some(DroidEnd::Expired), "bf2={bf2}");
    }
}

#[test]
fn salvo_rounds_come_down_on_the_target_from_the_authored_angle() {
    let src = Props(&[
        ("0x8339fb35", "70.0"),
        ("0xecdb9d1d", "600.0"),
        ("SalvoCount", "4"),
        ("SalvoDelay", "0.75"),
        ("0xebd81a5e", "2.5"),
    ]);
    let s = Salvo::from_props(&src, WeaponClass::default());
    assert_eq!((s.count, s.delay, s.spread), (4, 0.75, 2.5));
    let target = Vec3::new(10.0, 0.0, -5.0);
    let (origin, dir) = s.shot(target, [0.5; 4]);
    assert!((origin + dir * 600.0 - target).length() < 1e-2);
    assert!((origin.y - 600.0 * (70.0 * DEG).sin()).abs() < 0.1);
    let (spread, _) = s.shot(target, [1.0, 0.0, 0.5, 0.5]);
    assert!(((spread - origin).length() - 2.5).abs() < 1e-3);
}

#[test]
fn salvo_frame_and_droid_rotation_match_glam() {
    let s = Salvo::from_props(
        &Props(&[("0x8339fb35", "70.0"), ("0x3186fbfe", "30.0")]),
        WeaponClass::default(),
    );
    let want = Mat3::from_rotation_x(-s.pitch) * Mat3::from_rotation_y(s.yaw);
    assert!(s.frame().abs_diff_eq(want, 1e-6));
    let c = DroidClass::from_props(&Props(BF1_DROID), false);
    let mut d = droid(&c, 1.0);
    d.yaw = 0.7;
    d.up = Vec3::new(0.2, 1.0, -0.1).normalize();
    let want = Quat::from_rotation_arc(Vec3::Y, d.up) * Quat::from_rotation_y(d.yaw);
    assert!(d.rotation().abs_diff_eq(want, 1e-5));
}
