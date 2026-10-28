use super::{in_hand, InHand, SoldierVisuals};
use crate::walk::WalkSim;
use core::unit::Unit;
use core::weapon::{FireState, WeaponClass, WeaponState};

const UNDERHAND_MAX: f32 = 0.4;
const BF1_SHOOT3_ABOVE: f32 = 0.7;

pub(super) fn drawn<'u>(u: &'u Unit, classes: &[WeaponClass]) -> (usize, u8, &'u WeaponState) {
    let hand = u.active_weapon();
    if classes.get(hand.0).is_some_and(|c| c.offhand_weapon) {
        (u.weapon_class, u.weapon_slot, &u.weapon)
    } else {
        hand
    }
}

pub(super) fn drawn_weapon(u: &Unit, classes: &[WeaponClass]) -> InHand {
    in_hand(classes, drawn(u, classes))
}

fn throw_variant(bf2: bool, strength: f32) -> u8 {
    if !bf2 && strength > BF1_SHOOT3_ABOVE {
        2
    } else {
        u8::from(strength > UNDERHAND_MAX)
    }
}

impl SoldierVisuals {
    pub(super) fn note_throws(&mut self, walk: &WalkSim) {
        self.sync_units(walk);
        for (i, (v, u)) in self.units.iter_mut().zip(&walk.sim.units).enumerate() {
            let (wc, _, state) = u.active_weapon();
            let class = &walk.sim.weapon_classes[wc];
            let free = u.alive && u.mount.is_none();
            let released = class.dispenses() && walk.sim.events.iter().any(|e| {
                matches!(e,
                    core::sim::Event::Throw { unit, weapon_class, .. }
                    if unit.index as usize == i && unit.gen == u.gen && *weapon_class as usize == wc
                )
            });
            let throwing =
                (free && class.thrown.is_some() && (state.state == FireState::Firing || released))
                    .then_some(wc);
            if throwing.is_some() && throwing != v.throwing {
                v.pending_throw = Some(if class.dispenses() {
                    0
                } else {
                    throw_variant(self.bf2, state.charge)
                });
                v.throw_windup = class.initial_salvo_delay;
            }
            v.throwing = throwing;
            if !free {
                v.pending_throw = None;
            }
        }
    }
}

#[cfg(test)]
mod tests;
