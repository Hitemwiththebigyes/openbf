//! A walker's class numbers (card WALK, `analysis/re-walker.md` §11 [O]): the
//! ctor defaults (FUN_00263160), the setter's conversions (FUN_00263f68),
//! the BASE chain and the `WalkerSection` routing — with a section open the
//! turret setter FUN_00270770 gets first refusal ([`TURRET_CLAIMS`]), and
//! what it does not claim (the legs, speeds, turn rates) still lands on the
//! walker (B2:182865-182869).
//!
//! BASE chains: scalars inherit child-over-parent, as the hover's; a def that
//! opens a `WalkerLegPair` replaces the pairs (PROVISIONAL: every shipped
//! walker authors its own; the derive FUN_002636b0's copy of the pairs is
//! untraced). K+0xe18 bit 7 is neither set by the ctor nor copied by the
//! derive, and nothing in the locomotion reads it: not carried.
//!
//! Not carried (unused by the built integrator, `core::vehicle::walker`):
//! JumpHeight and the two jump hashes (§7: no walker authors them), the
//! death clip's K+0xe04, MaxTerrainAngle (no walker reader), the water
//! damage, the two hashes that fall into WalkerWidth's read (0x4aee3e6b,
//! 0xc33a9f04; no shipped walker authors a number there). MaxTurnSpeed and
//! MaxSpeed also scale the AI's steering (card WAI, `core::ai::walker_drive`).
//!
//! **BF1-disc walkers (card WALKB1, re-walker §13.9):** PLAN locks BF2's
//! handling, and BF2's code with BF1's data is neither game (a walking AT-ST
//! turns 3.4°/s), so a class read from a BF1 db takes BF2's body values:
//! [`bf1_body`]'s rows are set over its own after the chain. Everything else
//! stays BF1's. `OPENBF_BF1_WALKER_PACE=1` is the other way: BF1's own gait
//! under BF2's handling, no boost ([`WalkerClass::gait`]).
//!
//! A `commandwalker` class also carries its post's spawn points (card CWSP,
//! `analysis/re-command-walker-post.md` §4 [O]; the post's class setter B2
//! FUN_0035a3a0 / B1 FUN_002a1138): `SpawnPointCount` allocates the lists
//! and each `SpawnPointLocation "x y z angle"` appends a point. A plain
//! `walker` has no post component and reads neither key.

use super::{float, floats, h, scoped, Scope};
use crate::classdb::ClassDb;
use core::vehicle::walker::{flags, BuildingSphere, Clip, LegPair, WalkerParams, ANIM_COUNT};
use glam::Vec3;

/// Engine bases that construct the walker (re-vehicles §2).
pub const WALKER_BASES: [&str; 2] = ["walker", "commandwalker"];

/// The 30 hashes the MountedTurret setter FUN_00270770 claims inside a
/// `WalkerSection` (`lead_tmp/vwlk/turret_claims_b2.txt`, re-walker §11):
/// YawLimits, PitchLimits, the weapons, the turret node and sounds among
/// them — so a section's limits are the section's, not the walker's.
pub const TURRET_CLAIMS: [u32; 30] = [
    0x8822cb3c, 0xc13068b6, 0x9d39f68a, 0xb371fc9a, 0xb69a7cb4, 0x9455a446, 0x8e5da6e8, 0x9b0ca8c3,
    0xde721cb4, 0xd6c12c3b, 0xd0329e80, 0xdc19bd13, 0xf2aa4a90, 0xe9d0fb69, 0xf84aa204, 0xfbf47dba,
    0x3403b139, 0x1503f425, 0x1fa43b99, 0x16472328, 0x28f016d2, 0x2c3e8078, 0x0ebb87c8, 0x5d967ebc,
    0x6f332041, 0x6363d774, 0x8585bb8a, 0x865061fc, 0x4ed7a4d0, 0x4c39154d,
];

/// The three angle props' ceiling (0x3fc70bd8 = 89.1°, B2:183688-183702).
const ANGLE_CAP: f32 = 1.555_088_4;

/// One `WalkerLegPair`: its foot collision prims (left, right; hashes
/// 0x21322adb / 0xb62deb24, FUN_002a1468 entry +0x8 / +0xc).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegPairNames {
    pub feet: [Option<String>; 2],
}

/// A walker class, raw as the setter leaves K (angles in radians, the
/// tangent props as tangents, TurnThreshold as sin(x/2)).
#[derive(Clone, Debug, PartialEq)]
pub struct WalkerClass {
    motion: WalkerParams,
    /// `Some(rate)`: MaxSpeed is the class's BF1 gait, the walk clip's
    /// z·rate·30/N (re-walker §13.6; `rate` = TEMP_AnimationSpeed), taken
    /// from the bake in [`WalkerClass::params`] — the kaadu always, every
    /// BF1-disc walker under `OPENBF_BF1_WALKER_PACE=1`.
    pub gait: Option<f32>,
    /// `AnimationName`: the bank the anim table's clips come from.
    pub animation: Option<String>,
    pub leg_pairs: Vec<LegPairNames>,
    /// `BuildingCollision` prims, in PROP order.
    pub building: Vec<String>,
    /// Is the engine base `commandwalker` — does the class own a post?
    pub command: bool,
    /// The post's `SpawnPointCount` (0 until authored).
    pub spawn_point_count: usize,
    /// The post's `SpawnPointLocation`s: (x, y, z) in the walker's frame and
    /// the angle about up in radians (`angle × 0.017453294`), in PROP order.
    pub spawn_points: Vec<(Vec3, f32)>,
}

impl Default for WalkerClass {
    /// FUN_00263160 (re-walker §11), in the setter's units.
    fn default() -> Self {
        Self {
            motion: WalkerParams::default(),
            gait: None,
            animation: None,
            leg_pairs: Vec::new(),
            building: Vec::new(),
            command: false,
            spawn_point_count: 0,
            spawn_points: Vec::new(),
        }
    }
}

/// The post setter's degrees-to-radians factor (B2 FUN_0035a3a0).
const SPAWN_ANGLE_RAD: f32 = 0.017_453_294;

/// tan(min(|deg|·π/180, 89.1°)); 0 stays 0.
fn tan_prop(deg: f32) -> f32 {
    if deg == 0.0 {
        0.0
    } else {
        let (s, c) =
            core::fmath::sin_cos((deg.abs() * std::f32::consts::PI / 180.0).min(ANGLE_CAP));
        s / c
    }
}

/// strtol(value) ≠ 0: the leading integer.
fn strtol_nonzero(v: &str) -> bool {
    let t = v.trim_start();
    let end = t
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+'))))
        .map_or(t.len(), |(i, _)| i);
    t[..end].parse::<i64>().is_ok_and(|x| x != 0)
}

/// 0x…-spelled hash or prop name → the setter's key.
fn key(k: &str) -> u32 {
    k.strip_prefix("0x")
        .and_then(|x| u32::from_str_radix(x, 16).ok())
        .unwrap_or_else(|| h(k))
}

/// Card WALKB1 (re-walker §13.9; per class and prop, BF1 against BF2:
/// `lead_tmp/walkb1/body_diff_out.txt`): the body props a BF1-disc walker
/// takes from BF2's class of the same name, as that class authors them,
/// and its BF1 gait's rate (TEMP_AnimationSpeed, §13.6). `None`: not one
/// of the eight. The tests pin the rows to the baked BF2 classes.
/// - e0c (0xafb0caf4) stays 0 on the spider, AT-AT and AT-TE (BF2: 88°,
///   67.5°, 88°): it feeds only §4.5 C, which a human never reaches, and
///   with it their bots crab 5° off the goal; BF1's converged.
/// - The kaadu has no fielded BF2 form: what BF2's tauntaun changed for
///   handling, at its own gait, with no boost.
fn bf1_body(class: &str) -> Option<(Vec<(&'static str, &'static str)>, f32)> {
    // Flag bits 1, 3, 4 and bit 2 cleared: every fielded BF2 walker.
    let mut v = vec![
        ("0xe57fbb1d", "1"),
        ("0xba804df3", "0"),
        ("0x8ebd30be", "1"),
        ("0x3f164e6b", "1"),
        ("0xea47dd5c", "87"),
        ("0xbed8917f", "89"),
    ];
    // Bit 0 and e0c: the AT-STs and the mounts.
    let strafer = [("0x55bc9af9", "1"), ("0xafb0caf4", "88")];
    let mount = [
        ("StoppedTurnSpeed", "0.5"),
        ("ForwardTurnSpeed", "0.8"),
        ("YawLimits", "-90 90"),
        ("PitchLimits", "-35 40"),
    ];
    let mut rate = 1.0;
    match class.to_ascii_lowercase().as_str() {
        "imp_walk_atst" | "imp_walk_atst_snow" | "imp_walk_atst_jungle" => {
            v.extend(strafer);
            v.extend([
                ("BoostSpeed", "10"),
                ("ForwardTurnSpeed", "0.5"),
                ("BoostTurnSpeed", "0.3"),
                ("TurnThreshold", "0.6"),
                ("YawLimits", "-90 90"),
                ("EnergyBar", "80"),
                ("EnergyAutoRestore", "9"),
                ("EnergyBoostDrain", "20"),
            ]);
        }
        "all_walk_tauntaun" => {
            v.extend(strafer);
            v.extend(mount);
            v.extend([
                ("MaxSpeed", "20"),
                ("BoostSpeed", "30"),
                ("BoostTurnSpeed", "0.3"),
                ("EnergyBar", "100"),
                ("EnergyAutoRestore", "20"),
                ("EnergyBoostDrain", "10"),
            ]);
        }
        "gun_walk_kaadu" => {
            v.extend(strafer);
            v.extend(mount);
            // Without a bake; `params` reads the clip (z·30/N = 19.544).
            v.extend([("MaxSpeed", "19.544"), ("BoostSpeed", "0")]);
        }
        "cis_walk_spider" => v.extend([
            ("StoppedTurnSpeed", "0.3"),
            ("ForwardTurnSpeed", "0.2"),
            ("YawLimits", "-90 90"),
            ("PitchLimits", "-20 20"),
            ("LegRayHitLength", "10"),
        ]),
        "imp_walk_atat" => {
            rate = 1.5;
            v.extend([
                ("PitchLimits", "-35 15"),
                ("0x7fdeedf1", "1"),
                ("0xea47dd5c", "50"),
                ("0xbed8917f", "78"),
            ]);
        }
        "rep_walk_atte" => {
            rate = 1.5;
            v.extend([
                ("MaxTurnSpeed", "1"),
                ("StoppedTurnSpeed", "0.13"),
                ("ForwardTurnSpeed", "0.13"),
                ("YawLimits", "-20 20"),
            ]);
        }
        _ => return None,
    }
    Some((v, rate))
}

/// `OPENBF_BF1_WALKER_PACE=1`: BF1-disc walkers keep BF1's pace under
/// BF2's handling (re-walker §13.9, "the other way"). Default off.
fn bf1_pace() -> bool {
    std::env::var("OPENBF_BF1_WALKER_PACE").is_ok_and(|v| v == "1")
}

impl WalkerClass {
    /// `class`'s walker numbers, or `None` if the db lacks it or its BASE
    /// chain does not end at a [`WALKER_BASES`] engine class. A BF1 db's
    /// walker (and BF2's own unfielded kaadu) takes [`bf1_body`]'s rows.
    pub fn from_db(db: &ClassDb, class: &str) -> Option<Self> {
        let mut c = Self::authored(db, class)?;
        let kaadu = class.eq_ignore_ascii_case("gun_walk_kaadu");
        if !db.is_bf2() || kaadu {
            c.take_bf2_body(class, bf1_pace());
        }
        Some(c)
    }

    /// [`bf1_body`]'s rows set over the class's own; with `pace`, BF1's
    /// gait and no boost.
    fn take_bf2_body(&mut self, class: &str, pace: bool) {
        let Some((rows, rate)) = bf1_body(class) else {
            return;
        };
        for (k, v) in rows {
            self.set(key(k), v);
        }
        if pace {
            self.motion.boost_speed = 0.0;
        }
        if pace || class.eq_ignore_ascii_case("gun_walk_kaadu") {
            self.gait = Some(rate);
        }
    }

    /// The class as its own chain authors it.
    fn authored(db: &ClassDb, class: &str) -> Option<Self> {
        let base = db.engine_base(class)?;
        if !WALKER_BASES.contains(&base) {
            return None;
        }
        let mut c = Self {
            command: base.eq_ignore_ascii_case("commandwalker"),
            ..Self::default()
        };
        for def in db.chain(class).into_iter().rev() {
            let mut own_pairs = false;
            for (scope, k, v) in scoped(def) {
                if scope != Scope::Body {
                    continue;
                }
                if k == h("WalkerLegPair") && !own_pairs {
                    c.leg_pairs.clear();
                    own_pairs = true;
                }
                c.set(k, v);
            }
        }
        Some(c)
    }

    fn set(&mut self, k: u32, v: &str) {
        let scalars: [(&str, &mut f32); 14] = [
            ("MaxSpeed", &mut self.motion.max_speed),
            ("MaxTurnSpeed", &mut self.motion.max_turn_speed),
            ("BoostSpeed", &mut self.motion.boost_speed),
            ("StoppedTurnSpeed", &mut self.motion.stopped_turn_speed),
            ("ForwardTurnSpeed", &mut self.motion.forward_turn_speed),
            ("BoostTurnSpeed", &mut self.motion.boost_turn_speed),
            ("MaxYawSpeed", &mut self.motion.max_yaw_speed),
            ("MaxPitchSpeed", &mut self.motion.max_pitch_speed),
            ("EnergyBar", &mut self.motion.energy_max),
            ("EnergyAutoRestore", &mut self.motion.energy_auto_restore),
            ("EnergyBoostDrain", &mut self.motion.energy_boost_drain),
            ("SkeletonRootScale", &mut self.motion.root_scale),
            ("LegRayHitLength", &mut self.motion.leg_ray),
            ("WalkerWidth", &mut self.motion.width),
        ];
        for (name, slot) in scalars {
            if k == h(name) {
                if let Some(x) = float(v) {
                    *slot = x;
                }
                return;
            }
        }
        let tangents: [(u32, &mut f32); 4] = [
            (0xea47_dd5c, &mut self.motion.e08),
            (0xafb0_caf4, &mut self.motion.e0c),
            (0xbed8_917f, &mut self.motion.e10),
            (0xd128_c668, &mut self.motion.e14),
        ];
        for (key, slot) in tangents {
            if k == key {
                if let Some(x) = float(v) {
                    // K+0xe14 is a plain float (B2:183830).
                    *slot = if key == 0xd128_c668 { x } else { tan_prop(x) };
                }
                return;
            }
        }
        let bits: [(u32, u32); 6] = [
            (0x55bc_9af9, flags::AI),
            (0xe57f_bb1d, flags::AIM_WINDOW),
            (0xba80_4df3, flags::NO_CAMERA_TURN),
            (0x8ebd_30be, flags::CAMERA_TURN),
            (0x3f16_4e6b, flags::REVERSE_STEER),
            (0x7fde_edf1, flags::DAMAGE_BUILDINGS),
        ];
        for (key, bit) in bits {
            if k == key {
                if strtol_nonzero(v) {
                    self.motion.flags |= bit;
                } else {
                    self.motion.flags &= !bit;
                }
                return;
            }
        }
        if k == h("TurnThreshold") {
            if let Some(x) = float(v) {
                self.motion.turn_threshold = core::fmath::sin_cos(x * 0.5).0;
            }
        } else if k == h("YawLimits") {
            if let Some([lo, hi]) = floats::<2>(v) {
                self.motion.yaw_limits = [lo.to_radians(), hi.to_radians()];
            }
        } else if k == h("PitchLimits") {
            if let Some([lo, hi]) = floats::<2>(v) {
                self.motion.pitch_limits = [lo.to_radians(), hi.to_radians()];
            }
        } else if k == 0x6558_7105 {
            self.motion.roll = strtol_nonzero(v);
        } else if k == h("AnimationName") {
            self.animation = Some(v.trim().to_owned());
        } else if k == h("WalkerLegPair") {
            self.leg_pairs.push(LegPairNames::default());
        } else if k == 0x2132_2adb || k == 0xb62d_eb24 {
            let side = usize::from(k == 0xb62d_eb24);
            if let Some(p) = self.leg_pairs.last_mut() {
                p.feet[side] = Some(v.trim().to_owned());
            }
        } else if k == h("BuildingCollision") {
            self.building.push(v.trim().to_owned());
        } else if self.command && k == h("SpawnPointCount") {
            // "%i": the two lists are allocated afresh at this size.
            self.spawn_point_count = v.trim().parse::<usize>().unwrap_or(0);
            self.spawn_points.clear();
        } else if self.command && k == h("SpawnPointLocation") {
            if let Some([x, y, z, a]) = floats::<4>(v) {
                if self.spawn_points.len() < self.spawn_point_count {
                    self.spawn_points
                        .push((Vec3::new(x, y, z), a * SPAWN_ANGLE_RAD));
                }
            }
        }
    }

    /// The sim's copy (`core::vehicle::walker`), with the class's baked
    /// anim table and prim origins (`manifest::ManifestWalker`); without a
    /// bake the walker has no clips (it stands) and no legs (it falls).
    pub fn params(&self, bake: Option<&manifest::ManifestWalker>) -> WalkerParams {
        let prim =
            |n: &str| bake.and_then(|b| b.prims.iter().find(|p| p.name.eq_ignore_ascii_case(n)));
        let leg_pairs: Vec<LegPair> = self
            .leg_pairs
            .iter()
            .filter_map(|lp| {
                let l = prim(lp.feet[0].as_deref()?)?;
                let r = prim(lp.feet[1].as_deref()?)?;
                Some(LegPair {
                    feet: [Vec3::from(l.pos), Vec3::from(r.pos)],
                })
            })
            .collect();
        let foot_bones: Vec<&str> = self
            .leg_pairs
            .iter()
            .flat_map(|lp| lp.feet.iter().flatten())
            .filter_map(|n| prim(n))
            .map(|p| p.bone.as_str())
            .collect();
        let building = self
            .building
            .iter()
            .filter_map(|n| prim(n))
            .map(|p| BuildingSphere {
                pos: Vec3::from(p.pos),
                radius: p.radius,
                foot: foot_bones.iter().any(|b| b.eq_ignore_ascii_case(&p.bone)),
            })
            .collect();
        let clips = (0..ANIM_COUNT)
            .map(|i| {
                let c = bake?.clips.get(i)?.as_ref()?;
                Some(Clip {
                    duration: c.duration,
                    looping: c.looping,
                    root_z: c.root_z.clone(),
                })
            })
            .collect();
        WalkerParams {
            max_speed: self
                .gait
                .zip(bake.and_then(bf1_gait))
                .map_or(self.motion.max_speed, |(rate, g)| g * rate),
            leg_pairs,
            clips,
            building,
            spawn_points: self.spawn_points.clone(),
            ..self.motion.clone()
        }
    }
}

/// A baked walk clip's BF1 gait at rate 1: z·30/N (re-walker §13.6), z the
/// root's travel over the clip's N frames.
fn bf1_gait(bake: &manifest::ManifestWalker) -> Option<f32> {
    let c = bake.clips.get(core::vehicle::walker::ANIM_WALK)?.as_ref()?;
    let z = c.root_z.last()? - c.root_z.first()?;
    (c.frames > 0).then(|| z * 30.0 / f32::from(c.frames))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hashes_are_the_prop_names() {
        assert_eq!(h("WalkerWidth"), 0x0b9b_b9c9);
        assert_eq!(h("MaxTurnSpeed"), 0x5433_f589);
        assert_eq!(h("YawLimits"), 0x2c3e_8078);
        assert_eq!(h("PitchLimits"), 0x3403_b139);
        assert_eq!(h("WeaponName"), 0xfbf4_7dba);
        assert_eq!(h("SpawnPointCount"), 0x3257_9f4d);
        assert_eq!(h("SpawnPointLocation"), 0xa067_d145);
    }

    #[test]
    fn the_setter_converts_angles_and_flags() {
        assert!((tan_prop(87.0) - 19.0812).abs() < 1e-3);
        assert!((tan_prop(88.0) - 28.6364).abs() < 1e-3);
        assert!((tan_prop(89.0) - 57.2904).abs() < 1e-2);
        assert_eq!(tan_prop(0.0), 0.0);
        assert!((tan_prop(-120.0) - ANGLE_CAP.tan()).abs() < 1e-3);
        assert!(strtol_nonzero("1") && strtol_nonzero(" -2x"));
        assert!(!strtol_nonzero("0") && !strtol_nonzero("0.9") && !strtol_nonzero("x"));
    }

    fn census_db() -> Option<ClassDb> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets_baked/classes/");
        let mut db = ClassDb::default();
        for f in ["bf2_imp.ron", "bf2_rep.ron", "bf2_all.ron"] {
            let p = std::path::Path::new(dir).join(f);
            if !p.exists() {
                return None;
            }
            db.load(&p).ok()?;
        }
        Some(db)
    }

    /// re-walker §11's census (and §12's RAM for the conversions): BF2's
    /// AT-ST, one-man AT-ST and tauntaun.
    #[test]
    fn the_census_walkers_read_as_the_ps2_holds_them() {
        let Some(db) = census_db() else {
            eprintln!("SKIP: BF2 class RONs not baked");
            return;
        };
        let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
        let deg = |d: f32| d.to_radians();
        let at = WalkerClass::from_db(&db, "imp_walk_atst").expect("AT-ST");
        assert_eq!((at.motion.max_speed, at.motion.boost_speed), (5.0, 10.0));
        assert_eq!(
            (
                at.motion.stopped_turn_speed,
                at.motion.forward_turn_speed,
                at.motion.boost_turn_speed
            ),
            (0.5, 0.5, 0.3)
        );
        assert!(
            close(at.motion.turn_threshold, 0.29552),
            "{}",
            at.motion.turn_threshold
        );
        assert!(
            close(at.motion.e08, 19.0812)
                && close(at.motion.e0c, 28.6364)
                && (at.motion.e10 - 57.29).abs() < 0.01
        );
        let want = flags::AI | flags::AIM_WINDOW | flags::CAMERA_TURN | flags::REVERSE_STEER;
        assert_eq!(at.motion.flags, want, "bits 0 1 3 4, bit 2 cleared");
        assert!(
            close(at.motion.yaw_limits[0], deg(-90.0)) && close(at.motion.yaw_limits[1], deg(90.0))
        );
        assert!(
            close(at.motion.pitch_limits[0], deg(-35.0))
                && close(at.motion.pitch_limits[1], deg(40.0))
        );
        assert_eq!(
            (at.motion.max_yaw_speed, at.motion.max_pitch_speed),
            (0.7, 0.4)
        );
        assert_eq!(
            (
                at.motion.energy_max,
                at.motion.energy_auto_restore,
                at.motion.energy_boost_drain
            ),
            (80.0, 9.0, 20.0)
        );
        assert_eq!(at.motion.leg_ray, 5.0);
        assert_eq!(at.animation.as_deref(), Some("imp_walk_atst"));
        assert_eq!(at.leg_pairs.len(), 1);
        assert_eq!(
            at.leg_pairs[0].feet,
            [Some("p_l_foot".to_string()), Some("p_r_foot".to_string())]
        );
        assert_eq!(at.building, ["p_sphere_lfoot", "p_sphere_rfoot"]);
        // TURRET1's `YawLimits = 0 0` is the turret's (the section routing).
        let one = WalkerClass::from_db(&db, "rep_walk_oneman_atst").expect("one-man");
        assert_eq!((one.motion.max_speed, one.motion.boost_speed), (5.0, 10.0));
        assert_eq!(
            (
                one.motion.stopped_turn_speed,
                one.motion.forward_turn_speed,
                one.motion.boost_turn_speed
            ),
            (0.8, 0.6, 0.4)
        );
        assert_eq!(one.motion.max_yaw_speed, 2.0);
        assert!(close(one.motion.yaw_limits[1], deg(90.0)));
        let tt = WalkerClass::from_db(&db, "all_walk_tauntaun").expect("tauntaun");
        assert_eq!((tt.motion.max_speed, tt.motion.boost_speed), (20.0, 30.0));
        assert_eq!(
            (
                tt.motion.stopped_turn_speed,
                tt.motion.forward_turn_speed,
                tt.motion.boost_turn_speed
            ),
            (0.5, 0.8, 0.3)
        );
        assert!(
            close(tt.motion.turn_threshold, 0.47943),
            "{}",
            tt.motion.turn_threshold
        );
        assert_eq!(
            (
                tt.motion.energy_max,
                tt.motion.energy_auto_restore,
                tt.motion.energy_boost_drain
            ),
            (100.0, 20.0, 10.0)
        );
        assert_eq!(tt.motion.leg_ray, 2.5, "the ctor's");
        assert_eq!(tt.leg_pairs.len(), 1);
        assert!(WalkerClass::from_db(&db, "imp_hover_speederbike").is_none());
        // Card WAI: MaxTurnSpeed, the AI's steering rate (PS2 RAM K+0x75c,
        // re-walker-ai §12: 1.5 / 2.0 / 0.6).
        assert_eq!(
            (at.motion.max_turn_speed, tt.motion.max_turn_speed),
            (1.5, 2.0)
        );
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets_baked/classes/");
        let mut cis = ClassDb::default();
        if cis
            .load(&std::path::Path::new(dir).join("bf2_cis.ron"))
            .is_ok()
        {
            let sp = WalkerClass::from_db(&cis, "cis_walk_spider").expect("spider");
            assert_eq!((sp.motion.max_speed, sp.motion.max_turn_speed), (3.0, 0.6));
            assert_eq!(
                (sp.motion.stopped_turn_speed, sp.motion.forward_turn_speed),
                (0.3, 0.2)
            );
            assert!(close(sp.motion.e08, 19.0812) && close(sp.motion.e0c, 28.6364));
            assert_eq!(
                sp.motion.flags & flags::AI,
                0,
                "the spider takes the plain branch"
            );
            assert!(!sp.command && sp.spawn_points.is_empty());
        }
        assert!(!at.command && at.spawn_points.is_empty(), "a plain walker");
    }

    /// What card WALKB1 calls the body: every number the integrator and
    /// the AI's steering read.
    fn body(c: &WalkerClass) -> Vec<(&'static str, f32)> {
        vec![
            ("MaxSpeed", c.motion.max_speed),
            ("BoostSpeed", c.motion.boost_speed),
            ("StoppedTurnSpeed", c.motion.stopped_turn_speed),
            ("ForwardTurnSpeed", c.motion.forward_turn_speed),
            ("BoostTurnSpeed", c.motion.boost_turn_speed),
            ("MaxTurnSpeed", c.motion.max_turn_speed),
            ("TurnThreshold", c.motion.turn_threshold),
            ("YawLimits lo", c.motion.yaw_limits[0]),
            ("YawLimits hi", c.motion.yaw_limits[1]),
            ("PitchLimits lo", c.motion.pitch_limits[0]),
            ("PitchLimits hi", c.motion.pitch_limits[1]),
            ("MaxYawSpeed", c.motion.max_yaw_speed),
            ("MaxPitchSpeed", c.motion.max_pitch_speed),
            ("e08", c.motion.e08),
            ("e0c", c.motion.e0c),
            ("e10", c.motion.e10),
            ("e14", c.motion.e14),
            ("flags", c.motion.flags as f32),
            ("EnergyBar", c.motion.energy_max),
            ("EnergyAutoRestore", c.motion.energy_auto_restore),
            ("EnergyBoostDrain", c.motion.energy_boost_drain),
            ("LegRayHitLength", c.motion.leg_ray),
            ("roll", f32::from(u8::from(c.motion.roll))),
            ("WalkerWidth", c.motion.width),
        ]
    }

    fn game_db(game: &str) -> Option<ClassDb> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets_baked/classes/");
        let mut db = ClassDb::default();
        for side in ["all", "imp", "rep", "cis", "gun"] {
            let p = std::path::Path::new(dir).join(format!("{game}_{side}.ron"));
            if p.exists() {
                db.load(&p).ok()?;
            } else if side != "gun" {
                return None;
            }
        }
        Some(db)
    }

    const SHARED: [&str; 7] = [
        "imp_walk_atst",
        "imp_walk_atst_snow",
        "imp_walk_atst_jungle",
        "all_walk_tauntaun",
        "cis_walk_spider",
        "imp_walk_atat",
        "rep_walk_atte",
    ];
    const NO_E0C: [&str; 3] = ["cis_walk_spider", "imp_walk_atat", "rep_walk_atte"];

    /// Card WALKB1 (re-walker §13.9), pinned to the baked class RONs: on a
    /// BF1 map the seven shared walkers hold BF2's body (e0c 0 on the three
    /// without flag bit 0), none of the eight keeps flag bit 2, and the
    /// kaadu has the tauntaun's flags at its own clip's z·30/N.
    #[test]
    fn a_bf1_map_walker_holds_bf2s_body() {
        let (Some(b1), Some(b2)) = (game_db("bf1"), game_db("bf2")) else {
            eprintln!("SKIP: class RONs not baked");
            return;
        };
        assert!(!b1.is_bf2() && b2.is_bf2());
        for class in SHARED {
            let ours = WalkerClass::from_db(&b1, class).expect(class);
            let theirs = WalkerClass::from_db(&b2, class).expect(class);
            assert_ne!(
                body(&WalkerClass::authored(&b1, class).expect(class)),
                body(&theirs),
                "{class}: BF1's own body is not BF2's"
            );
            for ((name, a), (_, b)) in body(&ours).into_iter().zip(body(&theirs)) {
                if name == "e0c" && NO_E0C.contains(&class) {
                    assert!(b > 2.0, "{class}: BF2 authors e0c");
                    assert_eq!(a, 0.0, "{class}: e0c stays 0");
                } else {
                    assert_eq!(a, b, "{class} {name}: BF1 map {a}, BF2 {b}");
                }
            }
            assert_eq!(ours.gait, None, "{class}");
            assert_eq!(
                ours.motion.flags & flags::NO_CAMERA_TURN,
                0,
                "{class}: bit 2"
            );
            // The other way: BF1's pace, no boost, the handling unchanged.
            let mut pace = WalkerClass::authored(&b1, class).expect(class);
            pace.take_bf2_body(class, true);
            let rate = if ["imp_walk_atat", "rep_walk_atte"].contains(&class) {
                1.5
            } else {
                1.0
            };
            assert_eq!(
                (pace.gait, pace.motion.boost_speed),
                (Some(rate), 0.0),
                "{class}"
            );
            assert_eq!(
                pace.motion.forward_turn_speed,
                theirs.motion.forward_turn_speed
            );
        }
        let tt = WalkerClass::from_db(&b2, "all_walk_tauntaun").expect("tauntaun");
        let kaadu = WalkerClass::from_db(&b1, "gun_walk_kaadu").expect("kaadu");
        assert_eq!(
            kaadu.motion.flags, tt.motion.flags,
            "the kaadu's flags are the tauntaun's"
        );
        assert_eq!(kaadu.motion.flags & flags::NO_CAMERA_TURN, 0);
        assert_eq!(
            (
                kaadu.motion.stopped_turn_speed,
                kaadu.motion.forward_turn_speed
            ),
            (tt.motion.stopped_turn_speed, tt.motion.forward_turn_speed)
        );
        assert_eq!(
            (kaadu.motion.e08, kaadu.motion.e0c, kaadu.motion.e10),
            (tt.motion.e08, tt.motion.e0c, tt.motion.e10)
        );
        assert_eq!(
            (kaadu.motion.yaw_limits, kaadu.motion.pitch_limits),
            (tt.motion.yaw_limits, tt.motion.pitch_limits)
        );
        assert_eq!((kaadu.motion.boost_speed, kaadu.gait), (0.0, Some(1.0)));
        // BF2's own unfielded kaadu takes the same rows.
        if let Some(k2) = WalkerClass::from_db(&b2, "gun_walk_kaadu") {
            assert_eq!(body(&k2), body(&kaadu));
        }
        // Its MaxSpeed is its walk clip's z·30/N, and the table's number
        // (the walker without a bake) agrees with it.
        let ron = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets_baked/side_bf1/vehicles/gun/vehicles.ron"
        );
        let Ok(text) = pak::read_to_string(ron) else {
            eprintln!("SKIP: the BF1 gun side is not baked");
            return;
        };
        let vm: manifest::VehicleManifest = ron::from_str(&text).expect("vehicles.ron");
        let bake = vm
            .vehicles
            .into_iter()
            .find(|v| v.class.name.eq_ignore_ascii_case("gun_walk_kaadu"))
            .and_then(|v| v.walker)
            .expect("the kaadu's walker bake");
        let clip = bake.clips[core::vehicle::walker::ANIM_WALK]
            .as_ref()
            .expect("walk");
        let z = clip.root_z[clip.root_z.len() - 1] - clip.root_z[0];
        let want = z * 30.0 / f32::from(clip.frames);
        let p = kaadu.params(Some(&bake));
        assert!(
            (p.max_speed - want).abs() < 1e-3,
            "{} vs {want}",
            p.max_speed
        );
        assert!((p.max_speed - 19.544).abs() < 5e-3, "{}", p.max_speed);
        assert!(
            (kaadu.motion.max_speed - want).abs() < 5e-3,
            "the table's fallback"
        );
    }

    /// Card CWSP (re-command-walker-post §1): the two command walkers' six
    /// spawn points, both games.
    #[test]
    fn the_command_walkers_carry_their_six_spawn_points() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets_baked/classes/");
        let deg = |d: f32| d * SPAWN_ANGLE_RAD;
        for game in ["bf1", "bf2"] {
            let mut db = ClassDb::default();
            let mut ok = true;
            for side in ["imp", "rep"] {
                let p = std::path::Path::new(dir).join(format!("{game}_{side}.ron"));
                ok &= p.exists() && db.load(&p).is_ok();
            }
            if !ok {
                eprintln!("SKIP: {game} class RONs not baked");
                continue;
            }
            let atat = WalkerClass::from_db(&db, "imp_walk_atat").expect("AT-AT");
            assert!(atat.command);
            let want: Vec<(Vec3, f32)> = [
                (8.0, 0.0, 5.0),
                (8.0, 4.0, 5.0),
                (8.0, -4.0, 5.0),
                (-8.0, 0.0, 355.0),
                (-8.0, 4.0, 355.0),
                (-8.0, -4.0, 355.0),
            ]
            .into_iter()
            .map(|(x, z, a)| (Vec3::new(x, 0.0, z), deg(a)))
            .collect();
            assert_eq!(atat.spawn_points, want, "{game} AT-AT");
            assert_eq!(atat.params(None).spawn_points, want);
            let atte = WalkerClass::from_db(&db, "rep_walk_atte").expect("AT-TE");
            let want: Vec<(Vec3, f32)> = [
                (-6.0, 4.0),
                (6.0, 4.0),
                (-7.0, 0.0),
                (7.0, 0.0),
                (-6.0, -4.0),
                (6.0, -4.0),
            ]
            .into_iter()
            .map(|(x, z)| (Vec3::new(x, 0.0, z), 0.0))
            .collect();
            assert_eq!(atte.spawn_points, want, "{game} AT-TE");
        }
    }
}
