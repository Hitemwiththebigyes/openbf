//! What drives a unit, and the log that replays it: the pad ([`PadState`] and
//! its [`buttons`]), the input log ([`InputLog`]) and [`Sim::run_log`]; and
//! step 2 of the tick, BF1's aim assist, which bends a player slot's sticks
//! toward the unit or vehicle its acquisition picked ([`AimTarget`],
//! `acquire_magnet`, `apply_assist`, [`Sim::magnet_at`]). The replay, pad-slot and aim-assist tests live here.

use super::state::Sim;
use crate::aim_assist::{self, AssistEnv, Sticks};
use crate::collision::lead_point;
use crate::perception;
use crate::unit::UnitHandle;
use crate::vehicle::VehicleHandle;
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Button bits for [`PadState::buttons`], named for the locked control design
/// (PLAN §Movement & feel): one contextual ROLL_STANCE button (moving = roll,
/// stationary = stance cycle), BF2-style SPRINT, BF1 passive aim assist (no
/// lock-on button). Physical mapping lives in the `input` crate; the sim only
/// sees these bits.
pub mod buttons {
    pub const FIRE: u32 = 1 << 0;
    pub const JUMP: u32 = 1 << 1;
    /// Contextual: roll while moving, stance cycle while stationary.
    pub const ROLL_STANCE: u32 = 1 << 2;
    pub const SPRINT: u32 = 1 << 3;
    pub const ZOOM: u32 = 1 << 4;
    pub const RELOAD: u32 = 1 << 5;
    pub const ACTION: u32 = 1 << 6; // enter vehicle / interact
    pub const SWITCH_WEAPON: u32 = 1 << 7;
    pub const SECONDARY_FIRE: u32 = 1 << 8;
    /// Squad order FOLLOW ME (`core::squad`, card SQACK) — EDGE-TRIGGERED,
    /// read in `Sim::step` phase 0b.
    pub const SQUAD_FOLLOW: u32 = 1 << 9;
    /// Squad order HOLD POSITION.
    pub const SQUAD_HOLD: u32 = 1 << 10;
    /// Squad order AT EASE: releases the units this slot ordered.
    pub const SQUAD_AT_EASE: u32 = 1 << 11;
    /// Squad order MOVE OUT: the same release.
    pub const SQUAD_MOVE_OUT: u32 = 1 << 12;
    /// Spawn screen: next soldier class. Read ONLY while the player is dead
    /// (`Sim::step` phase 0a); an alive player's press does nothing.
    pub const SPAWN_CLASS_NEXT: u32 = 1 << 13;
    /// Spawn screen: previous soldier class.
    pub const SPAWN_CLASS_PREV: u32 = 1 << 14;
    /// Step channel 1's weapon slot (the PS2's "change secondary",
    /// `analysis/re-grenades.md` §2.4); [`SWITCH_WEAPON`] steps channel 0's.
    /// Both are edge-triggered, on foot (`Sim::step` phase 4).
    pub const SWITCH_SECONDARY: u32 = 1 << 15;
    /// Spawn map (card SPSEL): push the chosen POST toward screen-up on the
    /// map (world −z). Read ONLY while the player is dead (`Sim::step` phase
    /// 0a, `Sim::spawn_select`); the four arms combine into one push.
    pub const SPAWN_POST_UP: u32 = 1 << 16;
    /// Spawn map: push toward screen-down (world +z).
    pub const SPAWN_POST_DOWN: u32 = 1 << 17;
    /// Spawn map: push toward screen-left (world −x).
    pub const SPAWN_POST_LEFT: u32 = 1 << 18;
    /// Spawn map: push toward screen-right (world +x).
    pub const SPAWN_POST_RIGHT: u32 = 1 << 19;
    /// The spawn screens' accept (card SSELA; the PS2's X, re-spawn-flow §1):
    /// side -> spawn map -> class -> spawn. EDGE-TRIGGERED, read only while
    /// the slot is not on the field and `Sim::spawn_screens` is on.
    pub const SPAWN_ACCEPT: u32 = 1 << 20;
    /// The spawn screens' back (the PS2's Triangle): class -> spawn map;
    /// nothing on the spawn map (offline) or on the side screen.
    pub const SPAWN_BACK: u32 = 1 << 21;
    /// One-tick command from the confirmed pause-menu Respawn action (BUGX).
    /// Recorded/replayed with the other pad bits; no physical button maps to it.
    pub const RESPAWN: u32 = 1 << 22;
    /// The pad is aimed by the mouse (`mx`/`my`): aim assist leaves it alone.
    pub const MOUSE_AIM: u32 = 1 << 23;
}

pub const LOOK_UNIT: f32 = 1.0 / 65536.0;

fn zero(v: &i32) -> bool {
    *v == 0
}

/// One controller's state for one tick. Extend as input needs grow (axes are
/// quantized to i16 to keep the log bit-exact across platforms).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PadState {
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
    pub buttons: u32,
    #[serde(default, skip_serializing_if = "zero")]
    pub mx: i32,
    #[serde(default, skip_serializing_if = "zero")]
    pub my: i32,
}

impl PadState {
    pub fn pressed(&self, bit: u32) -> bool {
        self.buttons & bit != 0
    }

    /// Axis as f32 in [-1, 1] (i16::MIN maps to -1.0 exactly via the /32767
    /// convention with a final clamp).
    pub fn axis(v: i16) -> f32 {
        (f32::from(v) / 32767.0).clamp(-1.0, 1.0)
    }

    /// Quantize an f32 axis in [-1, 1] to the log-exact i16 form.
    pub fn quantize(v: f32) -> i16 {
        (v.clamp(-1.0, 1.0) * 32767.0) as i16
    }

    pub fn look(&self) -> (f32, f32) {
        (self.mx as f32 * LOOK_UNIT, self.my as f32 * LOOK_UNIT)
    }

    pub fn has_look(&self) -> bool {
        self.mx != 0 || self.my != 0
    }

    pub fn mouse_aim(&self) -> bool {
        self.pressed(buttons::MOUSE_AIM)
    }

    pub fn quantize_look(rad: f64) -> i32 {
        (rad / f64::from(LOOK_UNIT))
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
    }

    pub fn clear_look(&mut self) {
        self.mx = 0;
        self.my = 0;
    }
}

/// Deterministic input log: seed + per-tick pad states.
///
/// FLAT layout: `ticks` holds `slots` pads per tick back to back, so tick `t`
/// slot `s` is `ticks[t * slots + s]`. `slots` defaults to 1 on load, which is
/// exactly what the pre-P9 single-player logs on disk (no `slots` field) mean.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputLog {
    pub seed: u32,
    /// Player slots recorded per tick (>= 1).
    #[serde(default = "InputLog::one_slot")]
    pub slots: u8,
    pub ticks: Vec<PadState>,
}

impl Default for InputLog {
    fn default() -> Self {
        Self {
            seed: 0,
            slots: 1,
            ticks: Vec::new(),
        }
    }
}

impl InputLog {
    fn one_slot() -> u8 {
        1
    }

    /// Slots per tick, never 0 (a log claiming 0 slots is read as 1).
    pub fn slot_count(&self) -> usize {
        usize::from(self.slots).max(1)
    }

    /// Recorded ticks (a trailing partial tick is dropped).
    pub fn tick_count(&self) -> usize {
        self.ticks.len() / self.slot_count()
    }

    /// One slice of pads per tick, ready for [`Sim::step`].
    pub fn frames(&self) -> impl Iterator<Item = &[PadState]> {
        self.ticks.chunks_exact(self.slot_count())
    }
}

/// What a player's acquisition can pick (card HTGT, `analysis/re-target-hud.md`
/// §2 [O]): a soldier, or a vehicle or building turret — the PS2's candidates
/// are every collision object that is alive (or a building) with health.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AimTarget {
    Unit(UnitHandle),
    Vehicle(VehicleHandle),
}

impl AimTarget {
    /// The unit, when the target is one.
    pub fn unit(self) -> Option<UnitHandle> {
        match self {
            AimTarget::Unit(h) => Some(h),
            AimTarget::Vehicle(_) => None,
        }
    }

    /// The vehicle or turret, when the target is one.
    pub fn vehicle(self) -> Option<VehicleHandle> {
        match self {
            AimTarget::Vehicle(h) => Some(h),
            AimTarget::Unit(_) => None,
        }
    }
}

/// What the acquisition and the assist read of a candidate.
struct Candidate {
    /// The PS2's team nibble: `None` = team 0 (an empty vehicle, an unowned
    /// turret).
    team: Option<u8>,
    /// The object kind the weapon's `target_kind` tests ([`HealthType`]).
    kind: u32,
    /// The acquisition's bounds: centre and radius.
    ctr: Vec3,
    r: f32,
    /// Where the LOS ray aims.
    aim: Vec3,
    vel: Vec3,
    /// The assist's target radius (a soldier's capsule, a vehicle's bounds).
    assist_r: f32,
    /// The assist's target centre.
    assist_ctr: Vec3,
    /// A vehicle's hull and a turret's building, which the LOS ray passes.
    hull: Option<VehicleHandle>,
    building: Option<u32>,
}

/// The sticky target's cone widening while FIRE is held, π/4 (0x3f490fdb
/// @0x25f4a8, `analysis/re-aim-assist.md` §3.3).
const STICKY_WIDEN: f32 = std::f32::consts::FRAC_PI_4;

/// FUN_00241b98 in its 3-D mode (`re-aim-assist.md` §3.4): `t + 16·d²` for a
/// target of radius `r` centred `ctr`, seen from `pos` along the unit `dir`
/// — `t` its distance along the axis (0 to `maxd + r`), `d` its distance
/// off it, inside the cone of radius `r + t·slope` — else −1. Lower is
/// better.
pub(crate) fn cone_score(r: f32, slope: f32, maxd: f32, pos: Vec3, dir: Vec3, ctr: Vec3) -> f32 {
    let v = ctr - pos;
    let t = v.dot(dir);
    if t < 0.0 || t > maxd + r {
        return -1.0;
    }
    let d2 = (v - dir * t).length_squared();
    let rr = r + t * slope;
    if d2 > rr * rr {
        return -1.0;
    }
    t + 16.0 * d2
}

impl Sim {
    /// What the acquisition reads of `t`, `None` when it is gone. A
    /// vehicle's bounds are its `aim_sphere` (a building turret's building),
    /// else its blast sphere (card VDTH; PROVISIONAL: the PS2 takes the
    /// object's bounds box), centred in its frame.
    fn aim_candidate(&self, t: AimTarget) -> Option<Candidate> {
        match t {
            AimTarget::Unit(h) => {
                let u = self.unit(h)?;
                let (ctr, r) = u.aim_bounds();
                let cap = u.capsule();
                Some(Candidate {
                    team: Some(u.team),
                    kind: u.kind(),
                    ctr,
                    r,
                    aim: u.aim_point(),
                    vel: u.soldier.vel,
                    assist_r: cap.radius,
                    assist_ctr: cap.center(),
                    hull: None,
                    building: None,
                })
            }
            AimTarget::Vehicle(h) => {
                let v = self.vehicle(h)?;
                let (c, r) = match (v.aim_sphere, self.blast_setup.get(v.class as usize)) {
                    (Some(s), _) => s,
                    (None, Some(Some(b))) => (b.sphere_centre, b.sphere_radius),
                    _ => (Vec3::ZERO, 0.0),
                };
                let ctr = v.pos + v.rot * c;
                Some(Candidate {
                    team: v.team,
                    kind: v.health.health_type as u32,
                    ctr,
                    r,
                    aim: ctr,
                    vel: v.vel,
                    assist_r: r,
                    assist_ctr: ctr,
                    hull: Some(h),
                    building: v.instance,
                })
            }
        }
    }

    /// BF1's soldier acquisition, the PlayerController's FUN_0025e098
    /// (`analysis/re-aim-assist.md` §3.3, channel 0): keep the sticky
    /// magnet while it stays in the cone (widened by π/4 while FIRE is held)
    /// with a clear line; otherwise run the tournament over the units and
    /// vehicles in the query box — the lowest [`cone_score`] with a clear
    /// line wins, and once a filter-passing target is held only
    /// filter-passing candidates may replace it. The winner is the magnet
    /// only when it passes the weapon's Target* bits and lies within
    /// LockOnRange + r; it is the reticle's pick either way
    /// (`Sim::reticle_pick`), which the HUD's target readout shows
    /// (`analysis/re-target-hud.md` §2 [O], card HTGT).
    ///
    /// The candidates are alive units on foot and alive vehicles and
    /// building turrets but the shooter's own vehicle (BF2 B2:236430-236440:
    /// alive or a building; a rider is out of the collision world, so its
    /// vehicle stands for it). Port choices: units first in `alive_units()`
    /// order, then vehicles in index order (the PS2's collision-list order
    /// is [H]); the relation is same team = friend, team 0 = 0 (an empty
    /// vehicle, an unowned turret), else enemy (the PS2's default table for
    /// two armies); a destroyed turret is no candidate (the PS2 keeps a dead
    /// building; the port removes it); the query box is tested against each
    /// candidate's bounds sphere and has no 300 cap; the PS2's LockOffAngle
    /// hold (launchers only) and channel 1 are not ported.
    pub(super) fn acquire_magnet(&mut self, slot: usize, ph: UnitHandle, fire_held: bool) {
        let Some(p) = self.unit(ph) else {
            self.magnet[slot] = None;
            self.reticle_pick[slot] = None;
            return;
        };
        let c = &self.weapon_classes[p.weapon_class];
        let (er, lor, angle) = (c.extreme_range, c.lock_on_range, c.lock_on_angle);
        let (team_bits, kind_bits) = (c.target_team, c.target_kind);
        // The cone is centred on the reticle's ray (`re-aim-assist.md`
        // §10.5), not on the view's centre.
        let (pos, dir) = p.aim_ray(self.ground_rules, self.views());
        let own = p.mount.map(|m| m.vehicle);
        let own_building = own.and_then(|h| self.vehicle(h)).and_then(|v| v.instance);
        let rel = |c: &Candidate| -> i8 {
            match c.team {
                None => 0,
                Some(t) if t == p.team => 1,
                Some(_) => -1,
            }
        };
        let passes =
            |c: &Candidate| (team_bits >> (rel(c) + 1)) & 1 != 0 && (kind_bits >> c.kind) & 1 != 0;
        // The LOS ray stops r short of the aim point, on the shooter's side,
        // and passes the shooter's own vehicle and the target's hull.
        let clear = |c: &Candidate| {
            perception::los_seat(
                self,
                pos,
                c.aim + (pos - c.aim).normalize_or_zero() * c.r,
                crate::collision::HullFilter::Except([own, c.hull]),
                [own_building, c.building],
            )
        };
        let live = |t: AimTarget| match t {
            AimTarget::Unit(h) => self.unit(h).is_some_and(|u| u.mount.is_none()),
            AimTarget::Vehicle(h) => self.vehicle(h).is_some() && Some(h) != own,
        };

        let mut target = self.magnet[slot].filter(|&t| self.aim_candidate(t).is_some());
        let mut valid = target.is_some();
        let mut best = f32::MAX;
        if let Some((t, u)) = target.and_then(|t| Some((t, self.aim_candidate(t)?))) {
            let cone = angle + if fire_held { STICKY_WIDEN } else { 0.0 };
            let s = cone_score(u.r, cone, er, pos, dir, u.ctr);
            if s >= 0.0 && live(t) && clear(&u) {
                valid = passes(&u) && (u.ctr - pos).length() <= lor + u.r;
                best = s;
            } else {
                target = None;
                valid = false;
            }
        }
        if 0.0 < best {
            let mid = pos + dir * (er * 0.5);
            let units = self.alive_units().map(|(h, _)| AimTarget::Unit(h));
            let vehicles = self.alive_vehicles().map(|(h, _)| AimTarget::Vehicle(h));
            for t in units.chain(vehicles) {
                if t == AimTarget::Unit(ph) || !live(t) {
                    continue;
                }
                let Some(u) = self.aim_candidate(t) else {
                    continue;
                };
                let pass = passes(&u);
                if valid && !pass {
                    continue;
                }
                if (u.ctr - mid).abs().cmpgt(Vec3::splat(er + u.r)).any() || p.in_aim_box(u.ctr) {
                    continue;
                }
                let s = cone_score(u.r, angle, er, pos, dir, u.ctr);
                if 0.0 <= s && s < best && clear(&u) {
                    best = s;
                    target = Some(t);
                    valid = pass && (u.ctr - pos).length() <= lor + u.r;
                }
            }
        }
        let pick = target.and_then(|t| self.aim_candidate(t).map(|u| (t, rel(&u))));
        self.magnet[slot] = if valid { target } else { None };
        self.reticle_pick[slot] = pick;
    }

    /// Apply core::aim_assist to slot `slot`'s pad against magnet `m`.
    pub(super) fn apply_assist(
        &mut self,
        slot: usize,
        ph: UnitHandle,
        m: AimTarget,
        pad: &PadState,
        dt: f32,
    ) -> PadState {
        let (Some(p), Some(t)) = (self.unit(ph), self.aim_candidate(m)) else {
            return *pad;
        };
        // The pull steers the reticle's ray (card AIMA), not the chest's.
        let (_, lat, _) = p.aim_frame();
        let (eye, fwd) = p.aim_ray(self.ground_rules, self.views());
        let cap = p.capsule();
        let center = t.assist_ctr;
        let speed = self.weapon_classes[p.weapon_class].ordnance.velocity;
        // Ballistic lead (FUN_0026d3f0, flat-fire branch — bolts have no gravity).
        let lead = lead_point(center - eye, t.vel, speed).map(|rel| eye + rel);
        // Sign bridge: aim_assist yaw stick is POSITIVE-LEFT (BF1 f2
        // semantics); our rx is positive-right.
        let mut sticks = Sticks {
            move_mag: PadState::axis(pad.ly),
            yaw: -PadState::axis(pad.rx),
            pitch: PadState::axis(pad.ry),
        };
        let env = AssistEnv {
            self_pos: eye,
            self_vel: p.soldier.vel,
            self_radius: cap.radius,
            aim_fwd: fwd,
            lat,
            target_center: center,
            target_vel: t.vel,
            target_radius: t.assist_r,
            lead_point: lead,
            turn_rate: p.consts.max_turn_speed,
            // Zoomed, the assist's rate is divided as the turn is
            // (`re-zoom.md` §6).
            zoom_factor: p.zoom.map(|z| z.divisor),
            lock_on_range: self.weapon_classes[p.weapon_class].lock_on_range,
            fire_held: pad.pressed(buttons::FIRE),
            armed: self.assist_armed,
        };
        aim_assist::apply(
            dt,
            &mut sticks,
            &env,
            &mut self.assist[slot],
            self.assist_intensity,
        );
        PadState {
            rx: PadState::quantize(-sticks.yaw),
            ry: PadState::quantize(sticks.pitch),
            ..*pad
        }
    }

    /// The aim magnet of player slot `slot` (`None` when that slot has no
    /// target, or no such slot). The HUD reads slot 0.
    pub fn magnet_at(&self, slot: usize) -> Option<AimTarget> {
        self.magnet.get(slot).copied().flatten()
    }

    /// Run a whole input log from scratch; returns the final hash.
    pub fn run_log(log: &InputLog) -> u64 {
        let mut sim = Sim::new(log.seed);
        for pads in log.frames() {
            sim.step(pads);
        }
        sim.hash()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::{fire, flat_ground};
    use crate::unit::UnitSpec;
    use glam::Vec3;

    fn demo_log() -> InputLog {
        InputLog {
            seed: 0xC0FF_EE00,
            slots: 1,
            ticks: (0..300)
                .map(|i| PadState {
                    lx: ((i * 37) % 65536 - 32768) as i16,
                    ly: ((i * 91) % 65536 - 32768) as i16,
                    buttons: if i % 7 < 3 { buttons::FIRE } else { 0 },
                    ..Default::default()
                })
                .collect(),
        }
    }

    /// The pre-P9 logs on disk (`artifacts/p3_walk/*.ron`) carry no `slots`
    /// field: they load as one slot and replay to exactly the hash the same
    /// log with `slots: 1` gives.
    #[test]
    fn single_pad_log_without_slots_field_replays_identically() {
        #[derive(Serialize)]
        struct OldLog<'a> {
            seed: u32,
            ticks: &'a [PadState],
        }
        let log = demo_log();
        assert_eq!(log.slots, 1);
        let text = ron::to_string(&OldLog {
            seed: log.seed,
            ticks: &log.ticks,
        })
        .unwrap();
        assert!(!text.contains("slots"), "serialised without a slots field");
        let back: InputLog = ron::from_str(&text).unwrap();
        assert_eq!(back.slots, 1, "a missing slots field reads as one slot");
        assert_eq!(back.tick_count(), log.tick_count());
        assert_eq!(Sim::run_log(&back), Sim::run_log(&log));
    }

    /// Two occupied slots, one pad each: slot 0 walks forward (yaw 0 faces
    /// −Z), slot 1 walks backward, and neither sees the other's stick.
    #[test]
    fn two_slots_drive_two_units_independently() {
        let mut s = Sim::empty(4);
        s.heightfield = Some(flat_ground());
        let a = s
            .spawn(&UnitSpec::player_in_slot(0, 0, Vec3::ZERO))
            .unwrap();
        let b = s
            .spawn(&UnitSpec::player_in_slot(1, 0, Vec3::new(20.0, 0.0, 0.0)))
            .unwrap();
        assert_eq!(s.players, vec![Some(a), Some(b)], "one unit per slot");
        let fwd = PadState {
            ly: 32767,
            ..Default::default()
        };
        let back = PadState {
            ly: -32767,
            ..Default::default()
        };
        for _ in 0..60 {
            s.step(&[fwd, back]);
        }
        let (pa, pb) = (
            s.unit(a).unwrap().soldier.pos,
            s.unit(b).unwrap().soldier.pos,
        );
        assert!(pa.z < -5.0, "slot 0 walked forward: {pa}");
        assert!(pb.z > 2.0, "slot 1 walked backward: {pb}");
        // Swapping the pads swaps who goes which way.
        let mut s2 = Sim::empty(4);
        s2.heightfield = Some(flat_ground());
        let a2 = s2
            .spawn(&UnitSpec::player_in_slot(0, 0, Vec3::ZERO))
            .unwrap();
        let b2 = s2
            .spawn(&UnitSpec::player_in_slot(1, 0, Vec3::new(20.0, 0.0, 0.0)))
            .unwrap();
        for _ in 0..60 {
            s2.step(&[back, fwd]);
        }
        assert_eq!(s2.unit(a2).unwrap().soldier.pos.z, pb.z);
        assert_eq!(s2.unit(b2).unwrap().soldier.pos.z, pa.z);
    }

    /// A slot the pad slice does not reach idles — documented behaviour, not
    /// an error (a splitscreen player who has not joined; a net peer whose
    /// packet has not arrived).
    #[test]
    fn missing_pad_idles_the_slot() {
        let mut s = Sim::empty(4);
        s.heightfield = Some(flat_ground());
        let a = s
            .spawn(&UnitSpec::player_in_slot(0, 0, Vec3::ZERO))
            .unwrap();
        let b = s
            .spawn(&UnitSpec::player_in_slot(1, 0, Vec3::new(20.0, 0.0, 0.0)))
            .unwrap();
        let parked = s.unit(b).unwrap().soldier.pos;
        let fwd = PadState {
            ly: 32767,
            ..Default::default()
        };
        // One pad, two slots: slot 1 gets PadState::default().
        for _ in 0..60 {
            s.step(&[fwd]);
        }
        assert!(s.unit(a).unwrap().soldier.pos.z < -5.0, "slot 0 walked");
        assert_eq!(s.unit(b).unwrap().soldier.pos, parked, "slot 1 never moved");
        // An EMPTY slice idles every slot: the sim settles and then holds.
        for _ in 0..60 {
            s.step(&[]);
        }
        let held = s.unit(a).unwrap().soldier.pos;
        for _ in 0..30 {
            s.step(&[]);
        }
        assert_eq!(s.unit(a).unwrap().soldier.pos, held, "no pads: idle");
    }

    #[test]
    fn different_seed_different_hash() {
        let mut a = demo_log();
        let h1 = Sim::run_log(&a);
        a.seed ^= 1;
        assert_ne!(h1, Sim::run_log(&a));
    }

    #[test]
    fn input_log_ron_round_trip() {
        let log = demo_log();
        let s = ron::to_string(&log).unwrap();
        let back: InputLog = ron::from_str(&s).unwrap();
        assert_eq!(Sim::run_log(&log), Sim::run_log(&back));
    }

    /// A player at the origin facing −Z (the rifle: LockOnRange 64,
    /// ExtremeRange 120, LockOnAngle 1°) and the given soldiers, spawned in
    /// order after it.
    fn range(others: &[(u8, Vec3)]) -> (Sim, UnitHandle, Vec<UnitHandle>) {
        let mut s = Sim::empty(5);
        s.heightfield = Some(flat_ground());
        let p = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
        // A level aim: the spawn pitch's ray is in the floor 33 m out.
        s.units[p.index as usize].soldier.pitch = 0.0;
        let o = others
            .iter()
            .map(|&(team, pos)| s.spawn(&UnitSpec::trooper(team, pos)).unwrap())
            .collect();
        s.sync_slot_tables();
        (s, p, o)
    }

    /// The PS2 filters the magnet by team (`re-aim-assist.md` §3.3): a
    /// friend square on the crosshair is the reticle's pick (rel 1), never
    /// the magnet; an enemy there is both.
    #[test]
    fn a_friend_on_the_crosshair_is_picked_but_never_the_magnet() {
        let (mut s, p, o) = range(&[(0, Vec3::new(0.0, 0.0, -20.0))]);
        s.acquire_magnet(0, p, false);
        assert_eq!(s.magnet[0], None);
        assert_eq!(s.reticle_pick[0], Some((AimTarget::Unit(o[0]), 1)));
        let (mut s, p, o) = range(&[(1, Vec3::new(0.0, 0.0, -20.0))]);
        s.acquire_magnet(0, p, false);
        assert_eq!(s.magnet[0], Some(AimTarget::Unit(o[0])));
        assert_eq!(s.reticle_pick[0], Some((AimTarget::Unit(o[0]), -1)));
    }

    /// It is a cone, not a ray: an enemy 1 m off the axis at 20 m is inside
    /// r + t·slope (1.42 + 0.35) and becomes the magnet.
    #[test]
    fn an_enemy_just_off_the_ray_is_the_magnet() {
        let (mut s, p, o) = range(&[(1, Vec3::new(1.0, 0.0, -20.0))]);
        s.acquire_magnet(0, p, false);
        assert_eq!(s.magnet[0], Some(AimTarget::Unit(o[0])));
        // Well outside the cone: nothing.
        let (mut s, p, _) = range(&[(1, Vec3::new(3.0, 0.0, -20.0))]);
        s.acquire_magnet(0, p, false);
        assert_eq!((s.magnet[0], s.reticle_pick[0]), (None, None));
    }

    /// Past LockOnRange + r but inside ExtremeRange the enemy is picked
    /// (the reticle turns red) but is not the magnet.
    #[test]
    fn past_lock_on_range_is_picked_but_not_the_magnet() {
        let r = (crate::unit::AIM_BOX_MAX - crate::unit::AIM_BOX_MIN).length() * 0.5;
        let (mut s, p, o) = range(&[(1, Vec3::new(0.0, 0.0, -(64.0 + r + 1.0)))]);
        s.acquire_magnet(0, p, false);
        assert_eq!(s.magnet[0], None);
        assert_eq!(s.reticle_pick[0], Some((AimTarget::Unit(o[0]), -1)));
        // Past ExtremeRange + r: not even picked.
        let (mut s, p, _) = range(&[(1, Vec3::new(0.0, 0.0, -(120.0 + r + 1.0)))]);
        s.acquire_magnet(0, p, false);
        assert_eq!(s.reticle_pick[0], None);
    }

    /// The held magnet's cone widens by π/4 while FIRE is held: an enemy
    /// that steps 3 m off the axis at 20 m is kept while firing and dropped
    /// when not.
    #[test]
    fn the_sticky_magnet_holds_a_quarter_turn_wider_while_firing() {
        for (firing, kept) in [(true, true), (false, false)] {
            let (mut s, p, o) = range(&[(1, Vec3::new(0.0, 0.0, -20.0))]);
            s.acquire_magnet(0, p, false);
            assert_eq!(s.magnet[0], Some(AimTarget::Unit(o[0])));
            s.unit_mut(o[0]).unwrap().soldier.pos.x = 3.0;
            s.acquire_magnet(0, p, firing);
            assert_eq!(s.magnet[0].is_some(), kept, "firing {firing}");
        }
    }

    /// The tournament is order-dependent (§3.3): with nothing held, a
    /// better-scoring friend found FIRST takes the pick and the enemy behind
    /// cannot beat it — no magnet; found after a held enemy, the friend is
    /// skipped. The port's order is the unit table's.
    #[test]
    fn a_friend_found_first_shadows_the_enemy() {
        let friend = (0, Vec3::new(0.0, 0.0, -10.0));
        let enemy = (1, Vec3::new(0.5, 0.0, -20.0));
        let (mut s, p, o) = range(&[friend, enemy]);
        s.acquire_magnet(0, p, false);
        assert_eq!(s.magnet[0], None);
        assert_eq!(s.reticle_pick[0], Some((AimTarget::Unit(o[0]), 1)));
        let (mut s, p, o) = range(&[enemy, friend]);
        s.acquire_magnet(0, p, false);
        assert_eq!(s.magnet[0], Some(AimTarget::Unit(o[0])));
    }

    /// A vehicle 20 m down the crosshair, its blast sphere 2 m about a
    /// point 1 m up; `kind` Hover or a building Turret (no pilot seat).
    fn put_vehicle(s: &mut Sim, kind: crate::vehicle::VehicleKind) -> VehicleHandle {
        use crate::vehicle::{BlastSetup, SeatKind, VehicleKind, VehicleSpec};
        use crate::weapon::HealthType;
        let turret = kind == VehicleKind::Turret;
        let v = s.spawn_vehicle(&VehicleSpec {
            kind,
            class: 0,
            home_team: None,
            neutral_when_empty: true,
            spawn_pad: None,
            pos: Vec3::new(0.0, 0.0, -20.0),
            yaw: 0.0,
            max_health: 500.0,
            health_type: if turret {
                HealthType::Building
            } else {
                HealthType::Vehicle
            },
            seats: vec![if turret {
                SeatKind::Gunner(0)
            } else {
                SeatKind::Pilot
            }],
        });
        s.blast_setup = vec![Some(BlastSetup {
            explosion: None,
            sphere_centre: Vec3::new(0.0, 1.0, 0.0),
            sphere_radius: 2.0,
        })];
        v
    }

    /// Card HTGT (`re-target-hud.md` §2): a vehicle on the crosshair is the
    /// reticle's pick, team 0 while empty (relation 0) and an enemy once
    /// the other team holds it; it is the magnet only for a weapon whose
    /// TargetVehicle bit is set and that may take its relation.
    #[test]
    fn a_vehicle_on_the_crosshair_is_picked_and_magnets_by_the_weapons_bits() {
        use crate::vehicle::VehicleKind;
        let (mut s, p, _) = range(&[]);
        let v = put_vehicle(&mut s, VehicleKind::Hover);
        let w = s.unit(p).unwrap().weapon_class;
        s.weapon_classes[w].target_team = 1;
        s.weapon_classes[w].target_kind = 0x1f;
        s.acquire_magnet(0, p, false);
        assert_eq!(s.reticle_pick[0], Some((AimTarget::Vehicle(v), 0)));
        assert_eq!(
            s.magnet[0], None,
            "an enemy-only weapon: team 0 is no magnet"
        );
        s.vehicles[v.index as usize].team = Some(1);
        s.acquire_magnet(0, p, false);
        assert_eq!(s.reticle_pick[0], Some((AimTarget::Vehicle(v), -1)));
        assert_eq!(s.magnet[0], Some(AimTarget::Vehicle(v)));
        // BF1's rifles: TargetVehicle 0 - picked, never the magnet.
        s.weapon_classes[w].target_kind = 0x07;
        s.magnet[0] = None;
        s.acquire_magnet(0, p, false);
        assert_eq!(s.reticle_pick[0], Some((AimTarget::Vehicle(v), -1)));
        assert_eq!(s.magnet[0], None);
        // Its own team's: a friend.
        s.vehicles[v.index as usize].team = Some(0);
        s.acquire_magnet(0, p, false);
        assert_eq!(s.reticle_pick[0], Some((AimTarget::Vehicle(v), 1)));
    }

    /// A soldier in front of a tank scores lower and takes the pick; a
    /// destroyed vehicle is no candidate.
    #[test]
    fn a_soldier_before_a_vehicle_wins_and_a_wreck_is_no_target() {
        use crate::vehicle::VehicleKind;
        let (mut s, p, o) = range(&[(1, Vec3::new(0.0, 0.0, -10.0))]);
        let v = put_vehicle(&mut s, VehicleKind::Hover);
        s.acquire_magnet(0, p, false);
        assert_eq!(s.reticle_pick[0], Some((AimTarget::Unit(o[0]), -1)));
        s.units[o[0].index as usize].alive = false;
        s.acquire_magnet(0, p, false);
        assert_eq!(s.reticle_pick[0], Some((AimTarget::Vehicle(v), 0)));
        s.vehicles[v.index as usize].alive = false;
        s.acquire_magnet(0, p, false);
        assert_eq!(s.reticle_pick[0], None);
    }

    /// A building turret is picked through its own building's bodies (its
    /// centre sits inside them); another building in the way still blocks.
    #[test]
    fn a_turret_is_picked_through_its_own_building() {
        use crate::collision::{Body, Shape};
        use crate::vehicle::VehicleKind;
        let (mut s, p, _) = range(&[]);
        let v = put_vehicle(&mut s, VehicleKind::Turret);
        // Its building's bounds win over a blast sphere of radius 0.
        s.blast_setup[0].as_mut().unwrap().sphere_radius = 0.0;
        s.vehicles[v.index as usize].aim_sphere = Some((Vec3::new(0.0, 1.0, 0.0), 3.0));
        s.vehicles[v.index as usize].instance = Some(7);
        let mut shell = Body::new(
            glam::Mat4::from_translation(Vec3::new(0.0, 1.0, -20.0)),
            Shape::Box {
                half: Vec3::splat(3.0),
            },
        );
        shell.instance = Some(7);
        s.collision.push_body(shell);
        s.acquire_magnet(0, p, false);
        assert_eq!(s.reticle_pick[0], Some((AimTarget::Vehicle(v), 0)));
        s.vehicles[v.index as usize].instance = Some(8);
        s.acquire_magnet(0, p, false);
        assert_eq!(s.reticle_pick[0], None, "another building's shell blocks");
    }

    /// The shooter's own vehicle is never its pick.
    #[test]
    fn the_shooters_own_vehicle_is_not_picked() {
        use crate::vehicle::VehicleKind;
        let (mut s, p, _) = range(&[]);
        let v = put_vehicle(&mut s, VehicleKind::Hover);
        assert!(s.seat_unit(p, v, 0));
        s.acquire_magnet(0, p, false);
        assert_eq!(s.reticle_pick[0], None);
    }

    /// Acceptance 3 of card HTGT: the readout reads the picked vehicle's
    /// health every tick, so shots into its hull move it while the pick
    /// holds - no hit event in between.
    #[test]
    fn shots_into_a_picked_vehicle_drop_its_health_while_it_stays_picked() {
        use crate::collision::{HullPart, Layers, Shape};
        use crate::vehicle::VehicleKind;
        let (mut s, p, _) = range(&[]);
        let v = put_vehicle(&mut s, VehicleKind::Hover);
        s.vehicles[v.index as usize].team = Some(1);
        s.hull_parts = vec![Some(
            vec![HullPart::new(
                glam::Mat4::from_translation(Vec3::Y),
                Shape::Box {
                    half: Vec3::new(2.0, 1.0, 3.0),
                },
                Layers::ALL,
            )]
            .into(),
        )];
        s.sync_hulls();
        // The test rifle's bolt does nothing to a vehicle (scale 0, as
        // BF1's): give it a launcher's full vehicle scale.
        let w = s.unit(p).unwrap().weapon_class;
        s.weapon_classes[w].ordnance.scales[crate::weapon::HealthType::Vehicle as usize] = 1.0;
        let hull = &mut s.vehicles[v.index as usize].health;
        (hull.max, hull.cur) = (5000.0, 5000.0);
        let max = hull.max;
        let mut seen = Vec::new();
        for _ in 0..90 {
            s.step(&[fire()]);
            assert_eq!(
                s.reticle_pick[0].map(|(t, _)| t),
                Some(AimTarget::Vehicle(v))
            );
            seen.push(s.vehicles[v.index as usize].health.cur);
        }
        let last = *seen.last().unwrap();
        assert!(last < max, "hull {last} of {max}");
        assert!(seen.windows(2).all(|w| w[1] <= w[0]), "never rises");
        eprintln!("hull after 90 ticks of fire: {last} of {max}");
    }

    /// The assist pulls toward a vehicle magnet as toward a soldier.
    #[test]
    fn aim_assist_pulls_toward_a_vehicle_target() {
        use crate::vehicle::VehicleKind;
        let (mut s, p, _) = range(&[]);
        let v = put_vehicle(&mut s, VehicleKind::Hover);
        s.vehicles[v.index as usize].pos.x = 0.8;
        s.vehicles[v.index as usize].team = Some(1);
        let w = s.unit(p).unwrap().weapon_class;
        s.weapon_classes[w].target_kind = 0x1f;
        for _ in 0..30 {
            s.step(&[fire()]);
        }
        assert_eq!(s.magnet[0], Some(AimTarget::Vehicle(v)));
        assert!(
            s.unit(p).unwrap().soldier.yaw < -0.005,
            "yaw {}",
            s.unit(p).unwrap().soldier.yaw
        );
    }

    #[test]
    fn aim_assist_leaves_a_mouse_aimed_pad_alone() {
        use crate::vehicle::VehicleKind;
        let run = |mouse: bool| {
            let (mut s, p, _) = range(&[]);
            let v = put_vehicle(&mut s, VehicleKind::Hover);
            s.vehicles[v.index as usize].pos.x = 0.8;
            s.vehicles[v.index as usize].team = Some(1);
            let w = s.unit(p).unwrap().weapon_class;
            s.weapon_classes[w].target_kind = 0x1f;
            let mut pad = fire();
            if mouse {
                pad.buttons |= buttons::MOUSE_AIM;
            }
            for _ in 0..30 {
                s.step(&[pad]);
            }
            assert_eq!(s.magnet[0], Some(AimTarget::Vehicle(v)));
            s.unit(p).unwrap().soldier.yaw
        };
        assert!(run(false) < -0.005);
        assert_eq!(run(true), 0.0);
    }

    /// HTGTW: an off-pivot centre turns with the turret, rather than orbiting
    /// the unturned location. The radius is too small to cover both rays.
    #[test]
    fn htgtw_turned_turret_uses_the_rotated_sphere() {
        let (mut s, p, _) = range(&[]);
        let v = put_vehicle(&mut s, crate::vehicle::VehicleKind::Turret);
        let turret = &mut s.vehicles[v.index as usize];
        turret.pos = Vec3::new(4.0, 0.0, -16.0);
        turret.rot = glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        turret.aim_sphere = Some((Vec3::new(4.0, 1.0, 0.0), 0.25));
        let candidate = s.aim_candidate(AimTarget::Vehicle(v)).unwrap();
        // +X becomes -Z: (4,0,-16) + (0,1,-4).
        assert!(candidate.ctr.distance(Vec3::new(4.0, 1.0, -20.0)) < 1e-5);
        let eye = s.unit(p).unwrap().aim_ray(s.ground_rules, s.views()).0;
        let aim = |s: &mut Sim, point: Vec3| {
            let d = point - eye;
            let player = &mut s.units[p.index as usize];
            player.soldier.yaw = crate::fmath::atan2(-d.x, -d.z);
            player.soldier.pitch = crate::fmath::atan2(d.y, (d.x * d.x + d.z * d.z).sqrt());
            s.acquire_magnet(0, p, false);
        };
        aim(&mut s, Vec3::new(4.0, 1.0, -20.0));
        assert_eq!(s.reticle_pick[0], Some((AimTarget::Vehicle(v), 0)));
        aim(&mut s, Vec3::new(8.0, 1.0, -16.0));
        assert_eq!(s.reticle_pick[0], None);
    }

    #[test]
    fn htgtw_vehicle_only_weapon_excludes_building_turrets() {
        use crate::vehicle::VehicleKind;
        for (kind, vehicle_magnet) in [(VehicleKind::Hover, true), (VehicleKind::Turret, false)] {
            let (mut s, p, _) = range(&[]);
            let v = put_vehicle(&mut s, kind);
            s.vehicles[v.index as usize].team = Some(1);
            let w = s.unit(p).unwrap().weapon_class;
            s.weapon_classes[w].target_kind = 1 << 3;
            s.acquire_magnet(0, p, false);
            assert_eq!(s.reticle_pick[0], Some((AimTarget::Vehicle(v), -1)));
            assert_eq!(
                s.magnet[0],
                vehicle_magnet.then_some(AimTarget::Vehicle(v)),
                "{kind:?}"
            );
            // Invert the weapon's bits, retaining the same geometry and target.
            s.magnet[0] = None;
            s.weapon_classes[w].target_kind = 1 << 4;
            s.acquire_magnet(0, p, false);
            assert_eq!(
                s.magnet[0],
                (!vehicle_magnet).then_some(AimTarget::Vehicle(v)),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn htgtw_crossing_vehicle_assist_leads_in_its_travel_direction() {
        let (mut s, p, _) = range(&[]);
        let v = put_vehicle(&mut s, crate::vehicle::VehicleKind::Hover);
        // Centre exactly on the player's ray: stationary fire has no yaw
        // pull. With no movement stick, tracking cannot supply the lead.
        let (eye, fwd) = s.unit(p).unwrap().aim_ray(s.ground_rules, s.views());
        s.vehicles[v.index as usize].aim_sphere = Some((Vec3::ZERO, 0.5));
        s.vehicles[v.index as usize].pos = eye + fwd * 20.0;
        let w = s.unit(p).unwrap().weapon_class;
        s.weapon_classes[w].ordnance.velocity = 40.0;
        let mut outputs = Vec::new();
        for speed in [0.0, 4.0, -4.0] {
            s.vehicles[v.index as usize].vel = Vec3::X * speed;
            s.assist[0] = Default::default();
            let pad = s.apply_assist(0, p, AimTarget::Vehicle(v), &fire(), 1.0 / 30.0);
            outputs.push(pad.rx);
        }
        assert_eq!(outputs[0], 0);
        assert!(outputs[1] > 100, "rightward lead: {outputs:?}");
        assert!(outputs[2] < -100, "leftward lead: {outputs:?}");
        assert_eq!(outputs[1], -outputs[2]);
    }

    /// An enemy behind a wall fails the LOS ray and is not picked.
    #[test]
    fn an_enemy_behind_a_wall_is_not_picked() {
        use crate::collision::{Body, Shape};
        let (mut s, p, _) = range(&[(1, Vec3::new(0.0, 0.0, -20.0))]);
        s.collision.push_body(Body::new(
            glam::Mat4::from_translation(Vec3::new(0.0, 5.0, -10.0)),
            Shape::Box {
                half: Vec3::new(5.0, 5.0, 0.5),
            },
        ));
        s.acquire_magnet(0, p, false);
        assert_eq!((s.magnet[0], s.reticle_pick[0]), (None, None));
    }

    /// FUN_00241b98's two rejections and its score.
    #[test]
    fn cone_score_matches_the_ps2_formula() {
        let (pos, dir) = (Vec3::ZERO, Vec3::NEG_Z);
        assert_eq!(
            cone_score(1.0, 0.0, 10.0, pos, dir, Vec3::new(0.0, 0.0, 1.0)),
            -1.0
        );
        assert_eq!(
            cone_score(1.0, 0.0, 10.0, pos, dir, Vec3::new(0.0, 0.0, -11.5)),
            -1.0
        );
        assert_eq!(
            cone_score(1.0, 0.0, 10.0, pos, dir, Vec3::new(1.5, 0.0, -5.0)),
            -1.0
        );
        assert_eq!(
            cone_score(1.0, 0.1, 10.0, pos, dir, Vec3::new(1.5, 0.0, -5.0)),
            5.0 + 16.0 * 2.25
        );
    }

    /// Aim assist in the world: neutral stick + fire, target slightly right
    /// → yaw pulled negative (positive yaw = left).
    #[test]
    fn aim_assist_pulls_toward_the_unit_target() {
        let mut s = Sim::empty(11);
        let p = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
        s.spawn(&UnitSpec::trooper(1, Vec3::new(0.4, 0.0, -20.0)))
            .unwrap();
        for _ in 0..30 {
            s.step(&[fire()]);
        }
        assert!(s.magnet[0].is_some(), "magnet acquired in the cone");
        assert!(
            s.unit(p).unwrap().soldier.yaw < -0.005,
            "yaw {}",
            s.unit(p).unwrap().soldier.yaw
        );
    }
}
