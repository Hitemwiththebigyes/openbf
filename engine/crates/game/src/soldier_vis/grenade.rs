use super::{in_hand, InHand, SoldierVisuals};
use crate::walk::WalkSim;
use core::unit::Unit;
use core::weapon::{FireState, WeaponClass, WeaponState};

const UNDERHAND_MAX: f32 = 0.4;
const BF1_SHOOT3_ABOVE: f32 = 0.7;

/// The weapon drawn in the hand and whose set the animator takes: an
/// offhand weapon leaves the primary there (BF2). `bf2_throw`: the soldier
/// throws BF2's throw on a BF1 map ([`crate::animator::Rig::throws_offhand`],
/// card GTHRW), so a thrown grenade is offhand too (a dispenser keeps BF1's
/// drop).
pub(super) fn drawn<'u>(
    u: &'u Unit,
    classes: &[WeaponClass],
    bf2_throw: bool,
) -> (usize, u8, &'u WeaponState) {
    let hand = u.active_weapon();
    let offhand = classes
        .get(hand.0)
        .is_some_and(|c| c.offhand_weapon || (bf2_throw && c.thrown.is_some() && !c.dispenses()));
    if offhand {
        (u.weapon_class, u.weapon_slot, &u.weapon)
    } else {
        hand
    }
}

fn drawn_weapon(u: &Unit, classes: &[WeaponClass], bf2_throw: bool) -> InHand {
    in_hand(classes, drawn(u, classes, bf2_throw))
}

fn throw_variant(bf2: bool, strength: f32) -> u8 {
    if !bf2 && strength > BF1_SHOOT3_ABOVE {
        2
    } else {
        u8::from(strength > UNDERHAND_MAX)
    }
}

impl SoldierVisuals {
    /// Does unit `u`'s rig throw BF2's throw on a BF1 map
    /// ([`crate::animator::Rig::throws_offhand`])?
    pub(super) fn bf2_throw(&self, u: &Unit) -> bool {
        self.asset_for(u.team, u.spec.class)
            .and_then(|sa| sa.rig.as_ref())
            .is_some_and(|r| r.throws_offhand())
    }

    /// The weapon drawn in unit `u`'s hand ([`drawn_weapon`]).
    pub(super) fn hand_of(&self, u: &Unit, classes: &[WeaponClass]) -> InHand {
        drawn_weapon(u, classes, self.bf2_throw(u))
    }

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
