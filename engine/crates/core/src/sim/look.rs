use super::input::PadState;
use super::state::Sim;
use crate::unit::UnitHandle;
use crate::vehicle::gun::GunSetup;
use crate::vehicle::walker::{WalkerParams, WalkerState};
use crate::vehicle::{Vehicle, VehicleKind};

pub const MOUSE_STEER: f32 = 0.03;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LookPreview {
    Foot {
        unit: usize,
        yaw: f32,
        pitch: f32,
    },
    Seat {
        vehicle: usize,
        seat: usize,
        look: (f32, f32),
        walker: bool,
    },
}

pub(super) fn seat_look(
    g: &GunSetup,
    look: (f32, f32),
    d: (f32, f32),
    zoom: Option<f32>,
) -> (f32, f32) {
    let k = zoom.map_or(1.0, |z| 1.0 / z.max(1.0));
    let (dy, dp) = (-d.0 * k, d.1 * k);
    if let Some(t) = g.turn {
        return t.hold(look.0 + dy, look.1 + dp);
    }
    let a = g.aimer;
    let yaw = if g.pilot {
        0.0
    } else {
        (look.0 + dy).clamp(a.yaw[0], a.yaw[1])
    };
    (yaw, (look.1 + dp).clamp(a.pitch[0], a.pitch[1]))
}

pub(super) fn walker_aim(w: &WalkerState, p: &WalkerParams, d: (f32, f32)) -> (f32, f32) {
    let legs = w.leg_angle();
    let (lo, hi) = (p.yaw_limits[0] + legs, p.yaw_limits[1] + legs);
    let to = w.aim_yaw - d.0;
    let yaw = if to > w.aim_yaw && hi > 0.0 {
        to.min(w.aim_yaw.max(hi))
    } else if to < w.aim_yaw && lo < 0.0 {
        to.max(w.aim_yaw.min(lo))
    } else {
        to
    };
    let pitch = (w.aim_pitch + d.1).clamp(p.pitch_limits[0], p.pitch_limits[1]);
    (yaw, pitch)
}

pub(super) fn mouse_drive(
    v: &mut Vehicle,
    pad: &PadState,
    full: bool,
    walker: Option<&WalkerParams>,
) -> bool {
    if !pad.has_look() {
        return full;
    }
    let (dy, dp) = pad.look();
    if v.kind == VehicleKind::Walker {
        (v.walker.aim_yaw, v.walker.aim_pitch) = match walker {
            Some(p) => walker_aim(&v.walker, p, (dy, dp)),
            None => (v.walker.aim_yaw - dy, v.walker.aim_pitch + dp),
        };
        return full;
    }
    let turn = (v.input.turn + dy / MOUSE_STEER).clamp(-1.0, 1.0);
    v.input.turn = turn;
    full || turn * turn + v.input.pitch * v.input.pitch >= 0.9
}

impl Sim {
    pub fn look_preview(&self, h: UnitHandle, pad: &PadState) -> Option<LookPreview> {
        if !pad.has_look() && !pad.mouse_aim() {
            return None;
        }
        let u = self.unit(h).filter(|u| u.alive)?;
        let Some(m) = u.mount else {
            if u.soldier.look_frozen() {
                return None;
            }
            let (yaw, pitch) = u
                .soldier
                .mouse_turned(pad.look(), u.look_scale(), &u.consts);
            return Some(LookPreview::Foot {
                unit: h.index as usize,
                yaw,
                pitch,
            });
        };
        let g = self.seat_gun_setup(h)?;
        let vehicle = m.vehicle.index as usize;
        let seat = usize::from(m.seat);
        let v = self.vehicles.get(vehicle)?;
        if g.pilot && v.kind == VehicleKind::Walker {
            let p = self.walker_params.get(v.class as usize)?.as_ref()?;
            return Some(LookPreview::Seat {
                vehicle,
                seat,
                look: walker_aim(&v.walker, p, pad.look()),
                walker: true,
            });
        }
        let st = v.guns.get(seat)?.as_ref()?;
        Some(LookPreview::Seat {
            vehicle,
            seat,
            look: seat_look(&g, st.look, pad.look(), st.zoom),
            walker: false,
        })
    }
}

#[cfg(test)]
mod tests;
