use super::{soldier_blast_sphere, Event, Sim};
use crate::weapon::Ability;
use glam::Vec3;

impl Sim {
    pub(crate) fn ability_targets<'a>(
        &'a self,
        i: usize,
        ability: &'a Ability,
    ) -> impl Iterator<Item = usize> + 'a {
        let owner = &self.units[i];
        let from = owner.muzzle();
        self.units
            .iter()
            .enumerate()
            .filter(move |(target, u)| {
                if *target == i || !u.alive || u.mount.is_some() {
                    return false;
                }
                let (height, radius) = soldier_blast_sphere(u.soldier.stance);
                let centre = u.soldier.pos + Vec3::Y * height;
                let reach = ability.radius + radius;
                from.distance_squared(centre) <= reach * reach
            })
            .take(128)
            .filter(move |(_, u)| {
                (if u.team == owner.team {
                    ability.friends
                } else {
                    ability.enemies
                }) && crate::perception::los(self, owner.soldier.pos, u.soldier.pos)
            })
            .map(|(target, _)| target)
    }

    pub(super) fn fire_ability(&mut self, i: usize, class: usize) {
        let ability = self.weapon_classes[class].ability.as_ref().unwrap();
        let owner = self.handle(i);
        let from = self.units[i].muzzle();
        self.events.push(Event::Ability {
            unit: owner,
            weapon_class: class as u32,
            pos: from,
        });
        let targets: Vec<_> = self.ability_targets(i, ability).collect();
        let affected = !targets.is_empty();
        for target in targets {
            self.units[target].buffs.apply_ability(ability, owner);
        }
        if affected && ability.self_on_hit && ability.friends {
            self.units[i].buffs.apply_ability(ability, owner);
        }
    }
}

#[cfg(test)]
mod tests;
