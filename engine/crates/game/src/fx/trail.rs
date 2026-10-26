use super::{Frame, Fx};
use core::sim::{Event, Sim};
use core::sticky::Sticky;
use glam::{Mat3, Vec3};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Round {
    Bolt(crate::bolt_vis::Key),
    Sticky(u64),
}

#[derive(Debug, Default)]
pub(super) struct Rounds {
    tick: Option<u64>,
    stickies: Vec<(Sticky, u64)>,
    next: u64,
    spawned: HashSet<Round>,
}

impl Rounds {
    fn sticky_ids(&mut self, sim: &Sim) -> Vec<u64> {
        let thrown = sim
            .events
            .iter()
            .filter(|e| matches!(e, Event::Throw { .. }))
            .count();
        let kept: Vec<&(Sticky, u64)> = if self.tick.map(|t| t + 1) == Some(sim.tick) {
            self.stickies
                .iter()
                .filter(|(g, _)| !g.clone().burn(core::TICK_DT))
                .collect()
        } else {
            Vec::new()
        };
        let aligned = kept.len() + thrown == sim.stickies.len()
            && kept
                .iter()
                .zip(&sim.stickies)
                .all(|((p, _), g)| p.owner == g.owner && p.weapon_class == g.weapon_class);
        let mut ids: Vec<u64> = if aligned {
            kept.iter().map(|(_, id)| *id).collect()
        } else {
            Vec::new()
        };
        while ids.len() < sim.stickies.len() {
            ids.push(self.next);
            self.next += 1;
        }
        self.stickies = sim
            .stickies
            .iter()
            .cloned()
            .zip(ids.iter().copied())
            .collect();
        ids
    }
}

fn along(vel: Vec3) -> Mat3 {
    let z = vel.try_normalize().unwrap_or(Vec3::Z);
    let x = Vec3::Y.cross(z).try_normalize().unwrap_or(Vec3::X);
    Mat3::from_cols(x, z.cross(x), z)
}

impl Fx {
    pub(super) fn follow_rounds(&mut self, sim: &Sim) {
        if self.rounds.tick == Some(sim.tick) {
            return;
        }
        let trail = |wc: usize| {
            sim.weapon_classes
                .get(wc)
                .and_then(|w| w.ordnance.impact.as_ref())
                .and_then(|l| l.trail.as_deref())
        };
        let mut now: HashMap<Round, (Vec3, Vec3, Option<&str>)> = HashMap::new();
        for b in &sim.bolts {
            let r = Round::Bolt(crate::bolt_vis::key(b));
            now.insert(r, (b.pos, b.vel, trail(b.weapon_class)));
        }
        let ids = self.rounds.sticky_ids(sim);
        for (g, id) in sim.stickies.iter().zip(ids) {
            now.insert(Round::Sticky(id), (g.pos, g.vel, trail(g.weapon_class)));
        }
        self.rounds.tick = Some(sim.tick);
        for e in &mut self.live {
            let Some(r) = e.round else { continue };
            match now.get(&r) {
                Some(&(pos, vel, _)) => {
                    e.frame.pos = pos;
                    e.frame.vel = vel;
                }
                None => {
                    e.stop();
                    e.round = None;
                }
            }
        }
        self.rounds.spawned.retain(|r| now.contains_key(r));
        let mut fresh: Vec<_> = now
            .into_iter()
            .filter(|(r, _)| !self.rounds.spawned.contains(r))
            .filter_map(|(r, (pos, vel, name))| name.map(|n| (r, pos, vel, n)))
            .collect();
        fresh.sort_by_key(|(r, ..)| *r);
        for (r, pos, vel, name) in fresh {
            self.rounds.spawned.insert(r);
            let frame = Frame {
                rot: along(vel),
                pos,
                vel,
            };
            if self.spawn(name, frame) {
                if let Some(e) = self.live.last_mut() {
                    e.round = Some(r);
                }
            }
        }
    }
}
