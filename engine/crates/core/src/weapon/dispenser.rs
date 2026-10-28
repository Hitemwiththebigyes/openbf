use super::{pair, pf, PropSource, Thrown, WeaponClass, WeaponState};
use crate::sim::ItemClass;

#[derive(Clone, Debug, PartialEq)]
pub struct Dispenser<I = ItemClass> {
    pub item: I,
    pub velocity: f32,
    pub strength: [f32; 2],
    pub max_items: u32,
}

impl Dispenser {
    pub const BF2_HAND: (f32, f32) = (1.148, 0.721);
}

impl<I> Dispenser<I> {
    pub fn new(w: &mut WeaponClass, src: &dyn PropSource, bf2: bool, item: I) -> Self {
        Thrown::apply(w, src, bf2);
        w.shot_delay = pair(src, "ShotDelay")
            .unwrap_or([0.125; 2])
            .map(|v| v.max(0.0));
        let thrown = w.thrown.as_mut().unwrap();
        thrown.shot_elevate = 0.0;
        thrown.hand = bf2.then_some(Dispenser::BF2_HAND);
        w.flash = None;
        Self {
            item,
            velocity: pf(src, "Velocity").unwrap_or(10.0),
            strength: [
                pf(src, "MinStrength").unwrap_or(0.3),
                pf(src, "MaxStrength").unwrap_or(1.0),
            ],
            max_items: pf(src, "MaxItems").unwrap_or(0.0).max(0.0) as u32,
        }
    }

    pub fn speed(&self, charge: f32) -> f32 {
        (self.strength[0] + (self.strength[1] - self.strength[0]) * charge) * self.velocity
    }
}

impl WeaponState {
    pub(crate) fn refund_dispense(&mut self, c: &WeaponClass) {
        self.clip = self
            .clip
            .saturating_add(c.ammo_per_shot)
            .min(c.rounds_per_clip);
        self.reload_timer = 0.0;
        self.shots_fired = self.shots_fired.saturating_sub(1);
    }
}
