use super::{Frame, Fx};
use core::projectile::Bolt;
use core::sim::{Event, Sim};
use core::sticky::Sticky;
use glam::{Mat3, Vec3};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Round {
    Bolt(crate::bolt_vis::Key, u64),
    Sticky(u64),
    Guided(u64),
}

#[derive(Debug, Default)]
pub(super) struct Rounds {
    tick: Option<u64>,
    stickies: Vec<(Sticky, u64)>,
    bolts: Vec<(Bolt, u64)>,
    next: u64,
    spawned: HashSet<Round>,
}

impl Rounds {
    /// Match each survivor once using its next age and position, so collinear shots
    /// and identical pellets retain separate visual identities without sim state.
    fn bolt_ids(&mut self, sim: &Sim) -> Vec<u64> {
        let mut previous: HashMap<_, VecDeque<u64>> = HashMap::new();
        if self.tick.and_then(|t| t.checked_add(1)) == Some(sim.tick) {
            for (b, id) in &self.bolts {
                let pos = b.pos + b.vel * core::TICK_DT.min(b.remaining.max(0.0));
                previous
                    .entry((
                        crate::bolt_vis::key(b),
                        (b.age + core::TICK_DT).to_bits(),
                        pos.to_array().map(f32::to_bits),
                    ))
                    .or_default()
                    .push_back(*id);
            }
        }
        let ids: Vec<_> = sim
            .bolts
            .iter()
            .map(|b| {
                previous
                    .get_mut(&(
                        crate::bolt_vis::key(b),
                        b.age.to_bits(),
                        b.pos.to_array().map(f32::to_bits),
                    ))
                    .and_then(VecDeque::pop_front)
                    .unwrap_or_else(|| {
                        let id = self.next;
                        self.next += 1;
                        id
                    })
            })
            .collect();
        self.bolts = sim.bolts.iter().cloned().zip(ids.iter().copied()).collect();
        ids
    }

    fn sticky_ids(&mut self, sim: &Sim) -> Vec<u64> {
        let thrown = sim
            .events
            .iter()
            .filter(|e| match e {
                // Packs and placed items are no stickies: only a grenade's
                // throw adds one to `sim.stickies`.
                Event::Throw { weapon_class, .. } => {
                    !sim.weapon_classes[*weapon_class as usize].dispenses()
                }
                _ => false,
            })
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
        let bolt_ids = self.rounds.bolt_ids(sim);
        for (b, id) in sim.bolts.iter().zip(bolt_ids) {
            let r = Round::Bolt(crate::bolt_vis::key(b), id);
            now.insert(r, (b.pos, b.vel, trail(b.weapon_class)));
        }
        let ids = self.rounds.sticky_ids(sim);
        for (g, id) in sim.stickies.iter().zip(ids) {
            now.insert(Round::Sticky(id), (g.pos, g.vel, trail(g.weapon_class)));
        }
        for r in &sim.guided {
            let trail = sim.guided_class(r).and_then(|c| c.trail.as_deref());
            now.insert(Round::Guided(r.id), (r.pos, r.vel, trail));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::Library;
    use core::{projectile::Hit, unit::UnitHandle, weapon::WeaponClass};

    /// A thrown item that is no sticky (a pack, a placed mine) must not
    /// re-number the grenades already in flight.
    fn keeps_grenade_ids_when_throwing(class: WeaponClass) {
        let mut sim = Sim::empty(1);
        let item = sim.add_weapon_class(class);
        let owner = UnitHandle { index: 0, gen: 0 };
        sim.stickies.push(Sticky::new(
            Vec3::ZERO,
            Vec3::NEG_Z,
            &core::sticky::StickyClass::new(false),
            30.0,
            owner,
            0,
            0,
        ));
        let mut rounds = Rounds::default();
        let before = rounds.sticky_ids(&sim);
        rounds.tick = Some(sim.tick);
        sim.tick += 1;
        sim.events.push(Event::Throw {
            unit: owner,
            weapon_class: item as u32,
            origin: Vec3::ZERO,
        });
        assert_eq!(rounds.sticky_ids(&sim), before);
    }

    #[test]
    fn dispensing_a_pack_keeps_an_existing_grenades_trail_identity() {
        keeps_grenade_ids_when_throwing(WeaponClass {
            dispenser: Some(core::weapon::Dispenser {
                item: core::sim::ItemClass::basic("dual", 100.0, 2),
                velocity: 10.0,
                strength: [0.3, 0.4],
                max_items: 4,
            }),
            ..Default::default()
        });
    }

    #[test]
    fn placing_a_mine_keeps_an_existing_grenades_trail_identity() {
        struct NoProps;
        impl core::weapon::PropSource for NoProps {
            fn prop(&self, _: &str) -> Option<&str> {
                None
            }
            fn prop_hash(&self, _: u32) -> Option<&str> {
                None
            }
        }
        keeps_grenade_ids_when_throwing(WeaponClass {
            mine: Some(core::weapon::Dispenser {
                item: core::mine::MineClass::from_props(&NoProps, false),
                velocity: 10.0,
                strength: [0.3, 0.4],
                max_items: 4,
            }),
            ..Default::default()
        });
    }

    #[test]
    fn collinear_rounds_keep_separate_trails_and_retire_independently() {
        for age in [0.0, 0.1] {
            let root = crate::assets::baked_root();
            let common = root.join("common_bf1");
            if !common.join("effects.ron").exists() {
                eprintln!("SKIP: common_bf1 effects are not baked");
                return;
            }
            let mut fx = Fx::with_library(Library::load(&root, &common, "bf1", &[]));
            let mut sim = Sim::empty(1);
            let mut w = WeaponClass::bf1_rifle();
            w.ordnance.impact.as_mut().unwrap().trail = Some(crate::fx::BF1_IMPACT.into());
            let wc = sim.add_weapon_class(w);
            let first = Bolt {
                pos: Vec3::ZERO,
                vel: Vec3::NEG_Z * 10.0,
                remaining: 5.0,
                age: 0.0,
                owner: UnitHandle { index: 0, gen: 0 },
                team: 0,
                weapon_class: wc,
                hit: Hit::None,
                end: Vec3::NEG_Z * 50.0,
                shooter_is_player: true,
                hulls: core::collision::HullFilter::All,
                hit_mult: 1.0,
                gravity: 0.0,
                charge: 1.0,
            };
            let mut second = first.clone();
            second.age = age;
            second.pos = Vec3::NEG_Z;
            assert_eq!(crate::bolt_vis::key(&first), crate::bolt_vis::key(&second));
            sim.bolts = vec![first, second];
            fx.follow_rounds(&sim);
            let ids: Vec<_> = fx.live.iter().filter_map(|e| e.round).collect();
            assert_eq!(ids.len(), 2);
            assert_ne!(ids[0], ids[1]);
            for b in &mut sim.bolts {
                b.advance(core::TICK_DT);
            }
            sim.tick += 1;
            fx.follow_rounds(&sim);
            assert_eq!(fx.live.len(), 2, "survivors must not restart their trails");
            sim.bolts.remove(0);
            sim.bolts[0].advance(core::TICK_DT);
            sim.tick += 1;
            fx.follow_rounds(&sim);
            let attached: Vec<_> = fx.live.iter().filter(|e| e.round.is_some()).collect();
            assert_eq!(attached.len(), 1);
            assert_eq!(attached[0].round, Some(ids[1]));
            assert_eq!(attached[0].frame.pos, sim.bolts[0].pos);
        }
    }
}
