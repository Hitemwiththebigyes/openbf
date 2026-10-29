//! Card TBLD (OURS, the maintainer 2026-10-28: "units with a fusion cutter when near a
//! broken turret should have a higher chance to go fix it as its next
//! objective, so turrets remain in play and get fixed. Make sure that
//! multiple engineers aren't given the objective of fixing the same turret at
//! the same time"). A bot on foot that carries a fusion cutter takes the
//! nearest broken building turret within [`FIX_REACH`] - destroyed, or
//! standing below MaxHealth and not held by the enemy - as its objective
//! ahead of its post, its sniper's spot and its fight's ring (a seen enemy
//! still comes first: the claim waits while it fights). It walks to the
//! turret, stands, turns its cutter on it and holds the trigger until the
//! turret is whole ([`serve`]).
//!
//! **One engineer a turret**: a claim ([`FixState`]) no other bot's pick takes;
//! two that pick on one tick pick blind of each other, and the later claim
//! (then the higher index) gives way at its next think. A claim ends when
//! the turret is whole, when the bot dies (a dead bot's claim is nobody's),
//! when it is re-tasked (a squad order, a vehicle, off the level) or after
//! [`FIX_GIVE_UP`] with no health gained (then that turret is not picked
//! again for [`RETRY`]).
//!
//! The PS2's builder (`analysis/re-build-zones.md` 3 [O][V]: BF2 engineers /
//! BF1 pilots, a search every 5 s within 75 m, one claim per team, the walk to
//! 1.6 m, the 20 s and 8 s give-ups) is the model; the reach, the weight
//! over the bot's other picks and the claim per turret (not per team) are
//! ours. Every constant is OURS and PROVISIONAL.

use super::state::AiState;
use crate::fmath;
use crate::sim::{buttons, PadState, Sim};
use crate::unit::{Unit, UnitHandle};
use crate::TICK_HZ;
use glam::Vec3;

/// How far a cutter carrier looks for a broken turret, m (straight).
pub const FIX_REACH: f32 = 50.0;
/// A claim with no health gained for this long is given up, ticks.
pub const FIX_GIVE_UP: u64 = 20 * TICK_HZ as u64;
/// A turret given up on is not picked again by that bot for this long.
pub const RETRY: u64 = 15 * TICK_HZ as u64;
/// The bot stands this far outside the turret's sphere, m.
const STAND_OFF: f32 = 1.0;

/// One bot's turret objective ([`super::fix`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixClaim {
    /// `Sim::building_turrets` index.
    pub turret: u16,
    /// The tick it was claimed.
    pub since: u64,
    /// The turret's health when last it rose, and that tick.
    pub best: f32,
    pub progress_at: u64,
}

/// The claim, the turret given up last and when, and no search before
/// `next`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FixState {
    pub cur: Option<FixClaim>,
    pub left: Option<(u16, u64)>,
    pub next: u64,
}

impl FixState {
    pub fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(b"FIX");
        if let Some(c) = self.cur {
            eat(&c.turret.to_le_bytes());
            eat(&c.since.to_le_bytes());
            eat(&c.best.to_bits().to_le_bytes());
            eat(&c.progress_at.to_le_bytes());
        }
        if let Some((t, at)) = self.left {
            eat(&t.to_le_bytes());
            eat(&at.to_le_bytes());
        }
        eat(&self.next.to_le_bytes());
    }
}

/// The cutter `u` carries: its class index and channel, `None` without one.
pub fn cutter(sim: &Sim, u: &Unit) -> Option<(usize, u8)> {
    let tool = |c: usize| {
        sim.weapon_classes
            .get(c)
            .is_some_and(|w| w.repair.is_some())
    };
    if tool(u.weapon_class) {
        return Some((u.weapon_class, 0));
    }
    u.held
        .iter()
        .find(|w| tool(w.class))
        .map(|w| (w.class, w.channel))
}

/// Turret `i`'s health now (down or standing) and whether `u` may fix it
/// (broken, the cutter gives its kind something, not held by the enemy).
fn need(sim: &Sim, u: &Unit, class: usize, i: usize) -> Option<f32> {
    let t = sim.building_turrets.get(i)?;
    let repair = sim.weapon_classes[class].repair?;
    if repair.health[t.repair_kind() as usize] <= 0.0 {
        return None;
    }
    if let Some(h) = t.down {
        return Some(h);
    }
    let v = sim.vehicle(t.handle).filter(|v| v.alive)?;
    if v.team.is_some_and(|team| team != u.team) || !t.broken(Some(v.health.cur)) {
        return None;
    }
    Some(v.health.cur)
}

/// Once a think, before the route is planned: keep, end or make the bot's
/// claim (kept while it fights, its give-up clock running).
pub(super) fn upkeep(sim: &Sim, me: UnitHandle, u: &Unit, st: &mut AiState) {
    let Some((class, _)) = cutter(sim, u) else {
        st.fix.cur = None;
        return;
    };
    let busy = u.mount.is_some()
        || st.lost.is_some()
        || st.cmd.is_some()
        || u.command.is_some_and(|c| !c.in_pool())
        || st.ride.is_some_and(|r| r.board.is_some());
    if busy {
        st.fix.cur = None;
        return;
    }
    if let Some(mut c) = st.fix.cur {
        let i = usize::from(c.turret);
        // Another bot's older claim (then the lower index) keeps the turret.
        let taken = sim.alive_units().any(|(h, o)| {
            h != me
                && o.ai.fix.cur.is_some_and(|x| {
                    x.turret == c.turret
                        && (x.since < c.since || x.since == c.since && h.index < me.index)
                })
        });
        match need(sim, u, class, i) {
            _ if taken => st.fix.cur = None,
            None => {
                st.fix.cur = None;
                st.fix.next = sim.tick;
            }
            Some(_) if sim.tick >= c.progress_at + FIX_GIVE_UP => {
                st.fix.cur = None;
                st.fix.left = Some((c.turret, sim.tick));
            }
            Some(h) => {
                if h > c.best {
                    c.best = h;
                    c.progress_at = sim.tick;
                }
                st.fix.cur = Some(c);
            }
        }
    }
    if st.fix.cur.is_some() || sim.tick < st.fix.next {
        return;
    }
    let pos = u.soldier.pos;
    let left = st
        .fix
        .left
        .filter(|(_, at)| sim.tick < at + RETRY)
        .map(|(t, _)| t);
    let mut best: Option<(f32, usize, f32)> = None;
    for (i, t) in sim.building_turrets.iter().enumerate() {
        if left == Some(i as u16) {
            continue;
        }
        let Some(h) = need(sim, u, class, i) else {
            continue;
        };
        let d2 = (t.sphere().0 - pos).length_squared();
        if d2 > FIX_REACH * FIX_REACH || best.is_some_and(|(b, _, _)| d2 >= b) {
            continue;
        }
        let claimed = sim
            .alive_units()
            .any(|(o, x)| o != me && x.ai.fix.cur.is_some_and(|c| usize::from(c.turret) == i));
        if !claimed {
            best = Some((d2, i, h));
        }
    }
    st.fix.cur = best.map(|(_, i, h)| FixClaim {
        turret: i as u16,
        since: sim.tick,
        best: h,
        progress_at: sim.tick,
    });
    if st.fix.cur.is_none() {
        st.fix.next = sim.tick + 5 * TICK_HZ as u64;
    }
}

/// Where the claimed turret's fixer stands: on the ground outside its
/// sphere, on the bot's side.
fn stand(sim: &Sim, u: &Unit, turret: u16) -> Option<(Vec3, Vec3, f32)> {
    let t = sim.building_turrets.get(usize::from(turret))?;
    let (centre, radius) = t.sphere();
    let mut away = u.soldier.pos - centre;
    away.y = 0.0;
    let away = away.try_normalize().unwrap_or(Vec3::Z);
    let mut p = centre + away * (radius + STAND_OFF);
    p.y = sim.spawn_ground(p);
    Some((p, centre, radius))
}

/// Is the bot fighting (an enemy seen now)? The claim then waits.
fn fighting(st: &AiState) -> bool {
    st.contacts.iter().flatten().any(|c| c.seen_now)
}

/// The point a fixer walks to this tick: its stand by the claimed turret,
/// `None` with no claim or while it fights.
pub(super) fn walk(sim: &Sim, me: UnitHandle, st: &AiState) -> Option<Vec3> {
    let c = st.fix.cur.filter(|_| !fighting(st))?;
    let u = sim.unit(me)?;
    stand(sim, u, c.turret).map(|(p, _, _)| p)
}

/// The walk to the turret failed: give it up for [`RETRY`].
pub(super) fn walk_failed(sim: &Sim, st: &mut AiState) {
    if let Some(c) = st.fix.cur.take() {
        st.fix.left = Some((c.turret, sim.tick));
    }
}

/// The fixer's pad once the turret is in its cutter's reach: stand, turn
/// the cutter on the turret's centre, bring the cutter up and hold its
/// trigger. `None` while it walks, fights or holds no claim.
pub(super) fn serve(sim: &Sim, me: UnitHandle, st: &AiState) -> Option<PadState> {
    let c = st.fix.cur.filter(|_| !fighting(st))?;
    let u = sim.unit(me)?;
    let (class, channel) = cutter(sim, u)?;
    let (_, centre, radius) = stand(sim, u, c.turret)?;
    let wc = &sim.weapon_classes[class];
    let eye = u.muzzle();
    let mut flat = centre - eye;
    flat.y = 0.0;
    if flat.length() > wc.lock_on_range + radius - 0.25 {
        return None;
    }
    let dir = (centre - eye).normalize_or_zero();
    let dt = crate::TICK_DT;
    let want_yaw = super::gunner_foot::yaw_toward(dir);
    let want_pitch = fmath::asin(dir.y.clamp(-1.0, 1.0));
    let e_yaw = (want_yaw - u.soldier.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    let mut pad = PadState {
        rx: PadState::quantize(-e_yaw / (u.consts.max_turn_speed * dt)),
        ry: PadState::quantize((want_pitch - u.soldier.pitch) / (u.consts.max_pitch_speed * dt)),
        ..Default::default()
    };
    // The cutter up: a switch press on alternate ticks (the press is an
    // edge) until its channel's current weapon is the cutter.
    let up = match channel {
        0 => u.weapon_class == class,
        _ => u
            .cur1
            .and_then(|k| u.held.get(usize::from(k)))
            .is_some_and(|w| w.class == class),
    };
    if !up {
        if sim.tick.is_multiple_of(2) {
            pad.buttons |= if channel == 0 {
                buttons::SWITCH_WEAPON
            } else {
                buttons::SWITCH_SECONDARY
            };
        }
        return Some(pad);
    }
    pad.buttons |= if channel == 0 {
        buttons::FIRE
    } else {
        buttons::SECONDARY_FIRE
    };
    Some(pad)
}
