use super::{emitter::Frame, Fx, Gain};

use core::sim::{Event, Sim};

const BUFF_EFFECTS: [u32; 4] = [0xc17a_b11a, 0x28b9_f3ac, 0xbb3e_84e9, 0xcaa1_4018];

impl Fx {
    pub(super) fn follow_buffs(&mut self, sim: &Sim) {
        for effect in &mut self.live {
            let Some((h, kind)) = effect.buff else {
                continue;
            };
            let owner = sim.unit(h).filter(|u| u.buffs.timers()[kind] > 0.0);
            if let Some(u) = owner {
                effect.follow_unit(u);
            } else {
                effect.stop();
                effect.buff = None;
            }
        }
        for (h, u) in sim.alive_units() {
            for (kind, (timer, hash)) in u.buffs.timers().into_iter().zip(BUFF_EFFECTS).enumerate()
            {
                if timer <= 0.0 || self.live.iter().any(|e| e.buff == Some((h, kind))) {
                    continue;
                }
                let frame = Frame {
                    vel: u.soldier.vel,
                    ..Frame::at(u.soldier.pos)
                };
                if self.spawn_hash(hash, frame) {
                    self.live.last_mut().unwrap().buff = Some((h, kind));
                }
            }
        }
        for event in &sim.events {
            let Event::Ability {
                unit, weapon_class, ..
            } = event
            else {
                continue;
            };
            let Some(u) = sim.unit(*unit) else { continue };
            let Some(name) = sim
                .weapon_classes
                .get(*weapon_class as usize)
                .and_then(|w| w.ability.as_ref())
                .and_then(|a| a.activation_effect.as_deref())
            else {
                continue;
            };
            if self.spawn(name, Frame::at(u.soldier.pos)) {
                self.live.last_mut().unwrap().unit = Some((*unit, Gain::Activation));
            }
        }
    }
}
