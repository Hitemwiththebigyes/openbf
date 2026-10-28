use super::Sim;
use crate::unit::UnitHandle;
use crate::weapon::{Ability, DamageDesc};
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Buffs {
    pub offense: (f32, f32),
    pub defense: (f32, f32),
    pub regeneration: (f32, f32),
    pub poison: (f32, f32),
    pub poisoned_by: Option<UnitHandle>,
}

impl Default for Buffs {
    fn default() -> Self {
        Buffs {
            offense: (0.0, 1.0),
            defense: (0.0, 1.0),
            regeneration: (0.0, 0.0),
            poison: (0.0, 0.0),
            poisoned_by: None,
        }
    }
}

impl Buffs {
    pub(super) fn tick(&mut self, dt: f32) {
        for b in [
            &mut self.offense,
            &mut self.defense,
            &mut self.regeneration,
            &mut self.poison,
        ] {
            if b.0 > 0.0 && b.0 <= 300.0 {
                b.0 = (b.0 - dt).max(0.0);
            }
        }
    }

    pub fn timers(&self) -> [f32; 4] {
        [
            self.offense.0,
            self.defense.0,
            self.regeneration.0,
            self.poison.0,
        ]
    }

    pub fn active(&self) -> bool {
        self.timers().into_iter().any(|timer| timer > 0.0)
    }

    pub(super) fn apply_multipliers(&mut self, offense: (f32, f32), defense: (f32, f32)) {
        refresh(&mut self.offense, offense);
        refresh(&mut self.defense, defense);
    }

    pub(super) fn apply_ability(&mut self, ability: &Ability, owner: UnitHandle) {
        self.apply_multipliers(ability.offense, ability.defense);
        refresh(&mut self.regeneration, ability.regeneration);
        if refresh(&mut self.poison, ability.poison) {
            self.poisoned_by = Some(owner);
        }
    }

    pub fn dealt(&self) -> f32 {
        if self.offense.0 > 0.0 {
            self.offense.1
        } else {
            1.0
        }
    }

    pub fn taken(&self) -> f32 {
        if self.defense.0 > 0.0 {
            self.defense.1
        } else {
            1.0
        }
    }

    pub fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        for v in [
            self.offense.0,
            self.offense.1,
            self.defense.0,
            self.defense.1,
        ] {
            eat(&v.to_bits().to_le_bytes());
        }
        if self.regeneration.0 > 0.0 || self.poison.0 > 0.0 {
            eat(b"ability");
            for v in [
                self.regeneration.0,
                self.regeneration.1,
                self.poison.0,
                self.poison.1,
            ] {
                eat(&v.to_bits().to_le_bytes());
            }
            if self.poison.0 > 0.0 {
                if let Some(h) = self.poisoned_by {
                    eat(&h.index.to_le_bytes());
                    eat(&h.gen.to_le_bytes());
                }
            }
        }
    }
}

fn refresh(current: &mut (f32, f32), incoming: (f32, f32)) -> bool {
    if incoming.0 > current.0 {
        *current = incoming;
        true
    } else {
        false
    }
}

impl Sim {
    pub(super) fn tick_buffs(&mut self, dt: f32) {
        for i in 0..self.units.len() {
            if !self.units[i].alive {
                self.units[i].buffs = Buffs::default();
                continue;
            }
            self.units[i].buffs.tick(dt);
            let u = &mut self.units[i];
            if u.buffs.regeneration.0 > 0.0 {
                u.health.cur = (u.health.cur + u.buffs.regeneration.1 * dt).min(u.health.max);
            }
            if u.buffs.poison.0 <= 0.0 {
                u.buffs.poisoned_by = None;
                continue;
            }
            let attacker = u.buffs.poisoned_by;
            let damage = u.buffs.poison.1 * dt;
            let victim = self.handle(i);
            let scale = self.buff_scale(attacker, victim);
            let invincible = self.units[i].is_player && self.cheats.invincible;
            let u = &mut self.units[i];
            let damage = if invincible {
                0.0
            } else {
                u.health.take_damage(
                    &DamageDesc {
                        damage,
                        scales: [1.0; 7],
                        shield_scale: 1.0,
                    },
                    scale,
                    false,
                    1.0,
                )
            };
            if self.finish_unit_damage(victim, attacker, damage, 0.0, Vec3::ZERO) {
                self.units[i].buffs = Buffs::default();
            }
        }
    }
}
