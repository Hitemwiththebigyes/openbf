//! Card DPRX: the black box's MATCH-LONG tally of one subject - the human, or
//! in a headless round the sweep's human proxy (`core::ai::proxy`). The rings
//! keep the last 60 s; this keeps counts over the whole battle, so a
//! calibration match ("play one match, then `scripts/human_profile.py`")
//! has its K/D, accuracy, reaction, lives and movement in the exit snapshot.
//!
//! One definition for both subjects, so a human's numbers and a proxy's
//! compare:
//! - kills: `Event::Kill` by the subject's slot of an enemy; deaths: a kill
//!   of the subject or an environmental death (a suicide too);
//! - shots: `Event::Shot` by the subject (a press, as the event counts it);
//!   hits: `Event::Hit` by the subject on an enemy unit, one per victim per
//!   tick (a shotgun's pellets in one tick are one hit);
//! - lives: from the tick the subject is alive with a new generation to the
//!   tick it is not (`open` while it is alive at the snapshot);
//! - movement, on foot and alive only: the ground path (a step over 3 m is a
//!   teleport, not walked), ticks moving (over 0.5 m/s), STRAFING (sideways
//!   speed over 1 m/s and over the forward speed, facing by the soldier's own
//!   yaw), crouched, prone; each with how many times it began (a strafe after
//!   0.3 s without one);
//! - sight to hit ([`Session::engage`]): every 3 ticks each enemy within
//!   [`SIGHT_M`] inside [`SIGHT_DEG`] of the subject's aim with a clear line
//!   from its muzzle is SEEN; an enemy first seen opens an engagement, the
//!   subject's first hit on it closes it with the seconds between, and one
//!   unseen for [`FORGET_S`] without a hit is dropped (counted). It measures
//!   reaction plus aim plus travel time, the same way for a human and a bot.
//!
//! It only reads the sim and is in no hash.

use core::sim::{Event, Sim};
use core::soldier::Stance;
use serde::{Deserialize, Serialize};

/// An enemy farther than this is not counted as seen (m).
pub const SIGHT_M: f32 = 60.0;
/// ...nor one further off the aim than this (degrees, half angle).
pub const SIGHT_DEG: f32 = 40.0;
/// Its cosine (a constant: no float transcendental in game code).
pub const SIGHT_COS: f32 = 0.766_044_4;
/// An engagement with no sight of its enemy for this long is dropped (s).
pub const FORGET_S: f32 = 5.0;
/// Ticks between two sight tests.
const SIGHT_EVERY: u64 = 3;

/// The tally, as the snapshot writes it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Session {
    /// `human`, `proxy` or `none` (nobody to follow yet).
    pub subject: String,
    /// The subject's unit slot and team.
    pub unit: i32,
    pub team: u8,
    /// The first and last tick tallied.
    pub from: u64,
    pub to: u64,
    pub kills: u32,
    pub deaths: u32,
    pub shots: u32,
    pub hits: u32,
    /// Each life: [spawn tick, end tick]; the last one runs to `to` when
    /// `open`.
    pub lives: Vec<[u64; 2]>,
    pub open: bool,
    /// Ticks alive, and of those on foot.
    pub alive_ticks: u64,
    pub foot_ticks: u64,
    /// The ground path walked on foot (m).
    pub dist_m: f32,
    pub moving_ticks: u64,
    pub strafe_ticks: u64,
    pub strafe_starts: u32,
    pub crouch_ticks: u64,
    pub crouch_starts: u32,
    pub prone_ticks: u64,
    pub prone_starts: u32,
    /// Seconds from first sight to first hit, per engagement that got one.
    pub engage: Vec<f32>,
    /// Engagements dropped without a hit.
    pub engage_dropped: u32,
}

/// One open engagement: the enemy's slot and generation, first and last
/// seen.
#[derive(Debug, Clone, Copy)]
struct Open {
    index: u32,
    gen: u32,
    first: u64,
    last: u64,
}

/// The tally and its working state.
#[derive(Debug, Clone, Default)]
pub struct Tally {
    pub out: Session,
    gen: u32,
    life_open: bool,
    last_pos: Option<glam::Vec3>,
    last_strafe: Option<u64>,
    was_crouch: bool,
    was_prone: bool,
    open: Vec<Open>,
    hit_seen: Vec<(u32, u64)>,
}

impl Tally {
    /// Follow unit slot `subject` (`kind`: `human` / `proxy`) for the tick
    /// the sim just stepped. A new subject starts the tally over.
    pub fn tick(&mut self, sim: &Sim, subject: Option<(usize, &str)>) {
        let t = sim.tick;
        let Some((i, kind)) = subject else {
            return;
        };
        if self.out.unit != i as i32 || self.out.subject != kind {
            *self = Tally::default();
            self.out.subject = kind.to_owned();
            self.out.unit = i as i32;
            self.out.from = t;
        }
        let Some(u) = sim.units.get(i) else { return };
        self.out.team = u.team;
        self.out.to = t;
        let hz = core::TICK_HZ as f32;
        // Events first: they belong to this tick.
        self.hit_seen.retain(|&(_, at)| at == t);
        for ev in &sim.events {
            match *ev {
                Event::Shot { unit, .. } if unit.index as usize == i => self.out.shots += 1,
                Event::Hit {
                    shooter, victim, ..
                } if shooter.index as usize == i => {
                    let enemy = sim
                        .units
                        .get(victim.index as usize)
                        .is_some_and(|v| v.team != u.team);
                    if enemy && !self.hit_seen.contains(&(victim.index, t)) {
                        self.hit_seen.push((victim.index, t));
                        self.out.hits += 1;
                        if let Some(k) = self
                            .open
                            .iter()
                            .position(|o| o.index == victim.index && o.gen == victim.gen)
                        {
                            let o = self.open.swap_remove(k);
                            self.out.engage.push((t - o.first) as f32 / hz);
                        }
                    }
                }
                Event::Kill { shooter, victim } => {
                    if victim.index as usize == i {
                        self.out.deaths += 1;
                    } else if shooter.index as usize == i
                        && sim
                            .units
                            .get(victim.index as usize)
                            .is_some_and(|v| v.team != u.team)
                    {
                        self.out.kills += 1;
                    }
                }
                Event::EnvDeath { unit, .. } if unit.index as usize == i => self.out.deaths += 1,
                _ => {}
            }
        }
        // Lives.
        let alive = u.alive;
        if self.life_open && (!alive || u.gen != self.gen) {
            if let Some(l) = self.out.lives.last_mut() {
                l[1] = t;
            }
            self.life_open = false;
            self.last_pos = None;
        }
        if alive && !self.life_open {
            self.life_open = true;
            self.gen = u.gen;
            self.out.lives.push([t, t]);
        }
        self.out.open = self.life_open;
        if self.life_open {
            if let Some(l) = self.out.lives.last_mut() {
                l[1] = t;
            }
        }
        if !alive {
            self.was_crouch = false;
            self.was_prone = false;
            return;
        }
        self.out.alive_ticks += 1;
        if u.mount.is_some() {
            self.last_pos = None;
            return;
        }
        self.out.foot_ticks += 1;
        let p = u.soldier.pos;
        if let Some(q) = self.last_pos {
            let d = glam::Vec2::new(p.x - q.x, p.z - q.z).length();
            if d < 3.0 {
                self.out.dist_m += d;
            }
        }
        self.last_pos = Some(p);
        let v = u.soldier.vel;
        let (_, right, _) = u.aim_frame();
        let fwd = glam::Vec3::new(-right.z, 0.0, right.x);
        let flat = glam::Vec3::new(v.x, 0.0, v.z);
        let (side, ahead) = (flat.dot(right).abs(), flat.dot(fwd).abs());
        if flat.length() > 0.5 {
            self.out.moving_ticks += 1;
        }
        if side > 1.0 && side > ahead {
            self.out.strafe_ticks += 1;
            if self.last_strafe.is_none_or(|l| t > l + 9) {
                self.out.strafe_starts += 1;
            }
            self.last_strafe = Some(t);
        }
        let (crouch, prone) = (
            u.soldier.stance == Stance::Crouch,
            u.soldier.stance == Stance::Prone,
        );
        self.out.crouch_ticks += u64::from(crouch);
        self.out.prone_ticks += u64::from(prone);
        self.out.crouch_starts += u32::from(crouch && !self.was_crouch);
        self.out.prone_starts += u32::from(prone && !self.was_prone);
        (self.was_crouch, self.was_prone) = (crouch, prone);
        // Sight.
        if t.is_multiple_of(SIGHT_EVERY) {
            self.sight(sim, i, t);
        }
    }

    fn sight(&mut self, sim: &Sim, i: usize, t: u64) {
        let u = &sim.units[i];
        let eye = u.muzzle();
        let (aim, _, _) = u.aim_frame();
        let cos = SIGHT_COS;
        for (h, o) in sim.alive_units() {
            if o.team == u.team || o.team > 1 {
                continue;
            }
            let c = o.capsule().center();
            let to = c - eye;
            let d = to.length();
            if !(1e-3..=SIGHT_M).contains(&d) || aim.dot(to / d) < cos {
                continue;
            }
            if !core::perception::los(sim, eye, c) {
                continue;
            }
            match self
                .open
                .iter_mut()
                .find(|x| x.index == h.index && x.gen == h.gen)
            {
                Some(x) => x.last = t,
                None => self.open.push(Open {
                    index: h.index,
                    gen: h.gen,
                    first: t,
                    last: t,
                }),
            }
        }
        let forget = (FORGET_S * core::TICK_HZ as f32) as u64;
        let before = self.open.len();
        self.open.retain(|x| {
            t <= x.last + forget
                && sim
                    .units
                    .get(x.index as usize)
                    .is_some_and(|o| o.alive && o.gen == x.gen)
        });
        self.out.engage_dropped += (before - self.open.len()) as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sim::PadState;
    use core::unit::{Controller, UnitSpec};
    use glam::Vec3;

    /// A proxy that walks, shoots and is killed: its life, its walk and its
    /// death are tallied; a new subject starts over.
    #[test]
    fn a_life_a_walk_and_a_death_are_tallied() {
        let mut sim = Sim::new(7);
        sim.units.clear();
        let me = sim
            .spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(0, Vec3::ZERO)
            })
            .unwrap();
        let mut tally = Tally::default();
        for k in 0..30 {
            sim.units[me.index as usize].soldier.pos = Vec3::new(0.1 * k as f32, 0.0, 0.0);
            sim.tick += 1;
            sim.events.clear();
            tally.tick(&sim, Some((me.index as usize, "proxy")));
        }
        assert_eq!(tally.out.lives.len(), 1);
        assert!(tally.out.open);
        assert!(
            (tally.out.dist_m - 2.9).abs() < 1e-3,
            "{}",
            tally.out.dist_m
        );
        sim.kill_environment(me, core::sim::DeathCause::Fall);
        sim.tick += 1;
        tally.tick(&sim, Some((me.index as usize, "proxy")));
        assert_eq!(tally.out.deaths, 1);
        assert!(!tally.out.open);
        assert_eq!(tally.out.lives[0][1], sim.tick);
        assert_eq!(tally.out.alive_ticks, 30);
        sim.step(&[PadState::default()]);
        tally.tick(&sim, Some((me.index as usize, "human")));
        assert_eq!((tally.out.deaths, tally.out.subject.as_str()), (0, "human"));
    }
}
