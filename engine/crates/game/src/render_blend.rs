//! Render interpolation (card RINT): the sim steps at 30 Hz, the windowed
//! game draws at the display's rate, and a frame between two ticks draws the
//! world blended between them - `lerp(prev, cur, accum / TICK_DT)`, where
//! `accum` is what [`WalkSim::advance_with`]'s accumulator holds after the
//! frame's ticks. Without it a 60 / 144 Hz frame repeats the last tick's
//! picture and motion steps.
//!
//! What is blended (the drawn state the frame's builders read): each unit's
//! feet, yaw and pitch (the chase camera hangs off them, so the camera blends
//! too), each vehicle's place and turn, its guns' looks and a walker's aim,
//! and the camera's zoom ease (`zoom_s`, `zoom_fov_mix`). Not blended: a
//! walker's posed legs and the door / station animations (they step with
//! their ticks), the bolts (their own blend, `BoltVis::draws`), effects and
//! the soldiers' animation clocks (already advanced by the frame's dt).
//!
//! A unit or vehicle snaps - drawn where the tick left it, no blend - when
//! its slot's generation or alive flag changed (a respawn, a death, a new
//! vehicle), a unit's seat changed (boarding, leaving), or it moved farther
//! in the tick than [`UNIT_SNAP_M`] / [`VEHICLE_SNAP_M`] (a teleport).
//!
//! The blend is presentation only. [`Blend::apply`] writes the blended pose
//! into the sim for the frame's draw lists and [`Blend::restore`] writes the
//! tick's own values back, bit for bit, before anything else reads the sim
//! (the tick, the black box, a menu action): no tick ever sees a blended
//! value, so the sim hash is the same with or without it.

use crate::walk::WalkSim;
use core::sim::{LookPreview, PadState};
use glam::{Quat, Vec3};

/// A unit that moved farther than this in one tick is drawn where it is
/// (150 m/s: no soldier runs or flies that fast; a respawn or a rescue
/// spawn does). PROVISIONAL.
pub const UNIT_SNAP_M: f32 = 5.0;
/// The same for a vehicle (300 m/s). PROVISIONAL.
pub const VEHICLE_SNAP_M: f32 = 10.0;

/// One unit's drawn pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitPose {
    pub gen: u32,
    pub alive: bool,
    /// The vehicle and seat it rides (`Unit::mount`), packed; None on foot.
    pub seat: Option<(u32, u8)>,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

/// One vehicle's drawn pose.
#[derive(Debug, Clone, PartialEq)]
pub struct VehiclePose {
    pub gen: u32,
    pub alive: bool,
    pub pos: Vec3,
    pub rot: Quat,
    /// `GunState::look` per seat (None = an unarmed seat).
    pub looks: Vec<Option<(f32, f32)>>,
    /// `WalkerState::aim_yaw` / `aim_pitch`.
    pub aim: (f32, f32),
}

/// Everything the blend reads and writes, as one tick left it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pose {
    pub units: Vec<UnitPose>,
    pub vehicles: Vec<VehiclePose>,
    pub zoom_s: f32,
    pub zoom_fov_mix: f32,
}

impl Pose {
    /// The walk's drawn state now.
    pub fn capture(walk: &WalkSim) -> Self {
        let mut p = Pose::default();
        p.capture_into(walk);
        p
    }

    /// [`Pose::capture`] into this one's buffers (no allocation when warm,
    /// but a vehicle's looks).
    pub fn capture_into(&mut self, walk: &WalkSim) {
        self.units.clear();
        self.units.extend(walk.sim.units.iter().map(|u| UnitPose {
            gen: u.gen,
            alive: u.alive,
            seat: u.mount.map(|m| (m.vehicle.index, m.seat)),
            pos: u.soldier.pos,
            yaw: u.soldier.yaw,
            pitch: u.soldier.pitch,
        }));
        self.vehicles.clear();
        self.vehicles
            .extend(walk.sim.vehicles.iter().map(|v| VehiclePose {
                gen: v.gen,
                alive: v.alive,
                pos: v.pos,
                rot: v.rot,
                looks: v.guns.iter().map(|g| g.as_ref().map(|g| g.look)).collect(),
                aim: (v.walker.aim_yaw, v.walker.aim_pitch),
            }));
        self.zoom_s = walk.zoom_s;
        self.zoom_fov_mix = walk.zoom_fov_mix;
    }

    /// Write this pose into the walk (slots past either length untouched).
    pub fn write(&self, walk: &mut WalkSim) {
        for (u, p) in walk.sim.units.iter_mut().zip(&self.units) {
            u.soldier.pos = p.pos;
            u.soldier.yaw = p.yaw;
            u.soldier.pitch = p.pitch;
        }
        for (v, p) in walk.sim.vehicles.iter_mut().zip(&self.vehicles) {
            v.pos = p.pos;
            v.rot = p.rot;
            for (g, l) in v.guns.iter_mut().zip(&p.looks) {
                if let (Some(g), Some(l)) = (g.as_mut(), l) {
                    g.look = *l;
                }
            }
            (v.walker.aim_yaw, v.walker.aim_pitch) = p.aim;
        }
        walk.zoom_s = self.zoom_s;
        walk.zoom_fov_mix = self.zoom_fov_mix;
    }
}

/// `a` toward `b` by `t`; exactly `b` when the two are equal (a still
/// turret's place is matched by equality: `soldier_vis` seat hangs).
fn mix(a: f32, b: f32, t: f32) -> f32 {
    match a == b {
        true => b,
        false => a + (b - a) * t,
    }
}

fn mix3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    Vec3::new(mix(a.x, b.x, t), mix(a.y, b.y, t), mix(a.z, b.z, t))
}

/// `a` toward `b` by `t` the short way round (radians, either sign);
/// exactly `b` when the two are equal.
pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    if a == b {
        return b;
    }
    let tau = std::f32::consts::TAU;
    let mut d = (b - a) % tau;
    if d > std::f32::consts::PI {
        d -= tau;
    } else if d < -std::f32::consts::PI {
        d += tau;
    }
    a + d * t
}

fn lerp_look(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (lerp_angle(a.0, b.0, t), mix(a.1, b.1, t))
}

/// The pose drawn at `t` (0 = `prev`, 1 = `cur`) of the way from one tick to
/// the next; each unit or vehicle that snaps is `cur`'s.
pub fn blend(prev: &Pose, cur: &Pose, t: f32) -> Pose {
    let mut out = cur.clone();
    for (o, p) in out.units.iter_mut().zip(&prev.units) {
        let c = *o;
        let same = p.gen == c.gen && p.alive == c.alive && p.seat == c.seat;
        if !same || p.pos.distance_squared(c.pos) > UNIT_SNAP_M * UNIT_SNAP_M {
            continue;
        }
        o.pos = mix3(p.pos, c.pos, t);
        o.yaw = lerp_angle(p.yaw, c.yaw, t);
        o.pitch = mix(p.pitch, c.pitch, t);
    }
    for (o, p) in out.vehicles.iter_mut().zip(&prev.vehicles) {
        let same = p.gen == o.gen && p.alive == o.alive && p.looks.len() == o.looks.len();
        if !same || p.pos.distance_squared(o.pos) > VEHICLE_SNAP_M * VEHICLE_SNAP_M {
            continue;
        }
        o.pos = mix3(p.pos, o.pos, t);
        if p.rot != o.rot {
            o.rot = p.rot.slerp(o.rot, t);
        }
        for (l, pl) in o.looks.iter_mut().zip(&p.looks) {
            if let (Some(l), Some(pl)) = (l.as_mut(), pl) {
                *l = lerp_look(*pl, *l, t);
            }
        }
        o.aim = lerp_look(p.aim, o.aim, t);
    }
    out.zoom_s = mix(prev.zoom_s, cur.zoom_s, t);
    out.zoom_fov_mix = mix(prev.zoom_fov_mix, cur.zoom_fov_mix, t);
    out
}

/// The blend's state: the pose before the last tick, and the tick's own
/// values while a blended pose is written in.
#[derive(Debug, Default)]
pub struct Blend {
    /// The pose before the newest tick (None before the first tick).
    prev: Option<Pose>,
    /// Spare buffer for the next `prev`.
    spare: Pose,
    /// The tick's own pose while [`Blend::apply`] has a blend written in.
    saved: Option<Pose>,
    /// Whether it blends: off by default (headless runs draw ticks), on in
    /// the windowed game unless `OPENBF_RENDER_BLEND=0` ([`Blend::from_env`]).
    pub on: bool,
}

impl Blend {
    /// The windowed game's blend: on unless `OPENBF_RENDER_BLEND=0`.
    pub fn from_env() -> Self {
        Blend {
            on: std::env::var("OPENBF_RENDER_BLEND").map_or(true, |v| v != "0"),
            ..Default::default()
        }
    }

    /// Before a tick steps: keep the pose it starts from.
    pub fn before_tick(&mut self, walk: &WalkSim) {
        debug_assert!(self.saved.is_none(), "a tick stepped under a blend");
        if !self.on {
            return;
        }
        self.spare.capture_into(walk);
        match self.prev.as_mut() {
            Some(p) => std::mem::swap(p, &mut self.spare),
            None => self.prev = Some(std::mem::take(&mut self.spare)),
        }
    }

    /// A new battle (or the walk was swapped): nothing to blend from.
    pub fn reset(&mut self) {
        debug_assert!(self.saved.is_none());
        self.prev = None;
    }

    /// Write the pose `t` of the way from the last tick's start to its end
    /// into the walk, keeping the tick's own; [`Blend::restore`] must follow
    /// before anything but drawing reads the walk. Returns whether it wrote.
    pub fn apply(&mut self, walk: &mut WalkSim, t: f32) -> bool {
        if self.saved.is_some() || !self.on {
            return false;
        }
        let Some(prev) = self.prev.as_ref() else {
            return false;
        };
        let cur = Pose::capture(walk);
        blend(prev, &cur, t.clamp(0.0, 1.0)).write(walk);
        self.saved = Some(cur);
        true
    }

    /// Write the tick's own pose back (a no-op when none is blended in).
    pub fn restore(&mut self, walk: &mut WalkSim) {
        if let Some(cur) = self.saved.take() {
            cur.write(walk);
        }
    }

    /// Whether a blended pose is written in now.
    pub fn applied(&self) -> bool {
        self.saved.is_some()
    }

    pub fn preview(&mut self, walk: &mut WalkSim, look: LookPreview) {
        if self.saved.is_none() {
            self.saved = Some(Pose::capture(walk));
        }
        match look {
            LookPreview::Foot { unit, yaw, pitch } => {
                if let Some(u) = walk.sim.units.get_mut(unit) {
                    (u.soldier.yaw, u.soldier.pitch) = (yaw, pitch);
                }
            }
            LookPreview::Seat {
                vehicle,
                seat,
                mut look,
                walker,
            } => {
                let tick = self.saved.as_ref().and_then(|p| p.vehicles.get(vehicle));
                if let (true, Some(t), Some(v)) = (walker, tick, walk.sim.vehicles.get(vehicle)) {
                    let f = (v.rot.inverse() * t.rot) * Vec3::Z;
                    look.0 += core::fmath::atan2(f.x, f.z);
                }
                if let Some(v) = walk.sim.vehicles.get_mut(vehicle) {
                    if let Some(Some(g)) = v.guns.get_mut(seat) {
                        g.look = look;
                    }
                    if walker {
                        (v.walker.aim_yaw, v.walker.aim_pitch) = look;
                    }
                }
            }
        }
    }
}

pub fn blend_in_looking(walk: &mut WalkSim, look: &PadState) {
    let preview = walk.sim.look_preview(walk.player, look);
    walk.blend_in();
    if let Some(p) = preview {
        let mut blend = std::mem::take(&mut walk.blend);
        blend.preview(walk, p);
        walk.blend = blend;
    }
}

/// How far the frame is between the last tick and the next (0..1): the
/// accumulator's left-over over one tick.
pub fn fraction(walk: &WalkSim) -> f32 {
    (walk.accum() / core::TICK_DT).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk() -> WalkSim {
        let mut w = WalkSim::new(1, Vec3::ZERO, None);
        w.blend.on = true;
        w
    }

    fn me(w: &mut WalkSim) -> &mut core::soldier::Soldier {
        let i = w.player.index as usize;
        &mut w.sim.units[i].soldier
    }

    /// Half a tick on, a unit is drawn at the midpoint of its two ticks and
    /// turned halfway (the short way across +-pi); the tick's own pose comes
    /// back bit for bit.
    #[test]
    fn half_a_tick_draws_the_midpoint() {
        let mut w = walk();
        me(&mut w).pos = Vec3::new(1.0, 0.0, 2.0);
        me(&mut w).yaw = 3.0;
        let mut b = Blend {
            on: true,
            ..Default::default()
        };
        b.before_tick(&w);
        me(&mut w).pos = Vec3::new(2.0, 0.5, 4.0);
        me(&mut w).yaw = -3.0;
        assert!(b.apply(&mut w, 0.5));
        assert!(me(&mut w).pos.abs_diff_eq(Vec3::new(1.5, 0.25, 3.0), 1e-6));
        let half = std::f32::consts::PI;
        assert!(
            (me(&mut w).yaw.abs() - half).abs() < 1e-4,
            "{}",
            me(&mut w).yaw
        );
        b.restore(&mut w);
        assert_eq!(me(&mut w).pos, Vec3::new(2.0, 0.5, 4.0));
        assert_eq!(me(&mut w).yaw, -3.0);
        assert!(!b.applied());
    }

    /// A respawned unit (a new generation in its slot) and one that moved
    /// farther than a run in the tick are drawn where the tick left them.
    #[test]
    fn a_respawn_or_a_teleport_snaps() {
        let mut w = walk();
        let i = w.player.index as usize;
        me(&mut w).pos = Vec3::ZERO;
        let mut b = Blend {
            on: true,
            ..Default::default()
        };
        b.before_tick(&w);
        w.sim.units[i].gen += 1;
        me(&mut w).pos = Vec3::new(1.0, 0.0, 0.0);
        b.apply(&mut w, 0.5);
        assert_eq!(me(&mut w).pos, Vec3::new(1.0, 0.0, 0.0), "respawn");
        b.restore(&mut w);

        let mut b = Blend {
            on: true,
            ..Default::default()
        };
        b.before_tick(&w);
        me(&mut w).pos = Vec3::new(1.0 + UNIT_SNAP_M + 0.5, 0.0, 0.0);
        b.apply(&mut w, 0.5);
        assert_eq!(me(&mut w).pos.x, 1.0 + UNIT_SNAP_M + 0.5, "teleport");
        b.restore(&mut w);
    }

    #[test]
    fn the_pending_mouse_look_is_drawn_where_the_tick_lands() {
        for on in [true, false] {
            let mut w = walk();
            w.blend.on = on;
            w.advance(1.0 / 30.0, &[PadState::default()]);
            let look = PadState {
                mx: PadState::quantize_look(0.2),
                my: PadState::quantize_look(-0.05),
                buttons: core::sim::buttons::MOUSE_AIM,
                ..PadState::default()
            };
            let tick = (me(&mut w).yaw, me(&mut w).pitch);
            blend_in_looking(&mut w, &look);
            let drawn = (me(&mut w).yaw, me(&mut w).pitch);
            let cam = w.camera();
            w.blend_out();
            assert_eq!((me(&mut w).yaw, me(&mut w).pitch), tick, "restored");
            assert_ne!(drawn, tick);
            assert_eq!(cam.yaw, drawn.0);
            w.step_tick(&[look]);
            assert_eq!((me(&mut w).yaw, me(&mut w).pitch), drawn, "blend {on}");
        }
    }

    #[test]
    fn a_walkers_mouse_aim_keeps_its_heading_while_the_legs_turn_the_hull() {
        use core::vehicle::{gun::dir, hover::quat_y, SeatKind, VehicleKind, VehicleSpec};
        let mut w = walk();
        let i = w
            .sim
            .spawn_vehicle(&VehicleSpec {
                kind: VehicleKind::Walker,
                class: 0,
                home_team: None,
                neutral_when_empty: false,
                spawn_pad: None,
                pos: Vec3::ZERO,
                yaw: 0.0,
                max_health: 1.0,
                health_type: core::weapon::HealthType::Vehicle,
                seats: vec![SeatKind::Pilot],
            })
            .index as usize;
        w.sim.vehicles[i].walker.aim_yaw = 0.3;
        let mut b = Blend {
            on: true,
            ..Default::default()
        };
        b.before_tick(&w);
        let v = &mut w.sim.vehicles[i];
        v.rot = (v.rot * quat_y(-0.1)).normalize();
        v.walker.aim_yaw += 0.1;
        let aim = v.walker.aim_yaw;
        let heading = v.rot * dir(aim, 0.0);
        let tick_rot = v.rot;
        assert!(b.apply(&mut w, 0.25));
        b.preview(
            &mut w,
            LookPreview::Seat {
                vehicle: i,
                seat: 0,
                look: (aim, 0.0),
                walker: true,
            },
        );
        let v = &w.sim.vehicles[i];
        assert!(v.rot.angle_between(tick_rot) > 0.05);
        let drawn = v.rot * dir(v.walker.aim_yaw, 0.0);
        assert!(drawn.angle_between(heading) < 1e-5, "{drawn} {heading}");
        b.restore(&mut w);
        assert_eq!(w.sim.vehicles[i].walker.aim_yaw, aim);
    }

    /// The sim never reads the blend: a walk drawn blended at 144 Hz (the
    /// pose written in and back every frame) steps to the same sim hash as
    /// one that never blends, and its frames do move between ticks.
    #[test]
    fn the_blend_never_reaches_the_sim() {
        let pad = PadState {
            ly: -20000,
            rx: 9000,
            ..Default::default()
        };
        let mut plain = WalkSim::new(1, Vec3::ZERO, None);
        let mut drawn = walk();
        let dt = 1.0 / 144.0;
        let mut moved_between_ticks = 0;
        let mut last = None;
        for _ in 0..288 {
            plain.advance(dt, &[pad]);
            let ticks = drawn.advance(dt, &[pad]);
            drawn.blend_in();
            let cam = drawn.camera().position;
            if ticks == 0 && last.is_some_and(|l: Vec3| l != cam) {
                moved_between_ticks += 1;
            }
            last = Some(cam);
            drawn.blend_out();
        }
        assert_eq!(plain.sim.hash(), drawn.sim.hash());
        assert_eq!(plain.camera().position, drawn.camera().position);
        assert!(moved_between_ticks > 100, "{moved_between_ticks}");
    }
}
