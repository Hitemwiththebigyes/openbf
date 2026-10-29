use super::{buttons, Event, PadState, Sim};
use crate::collision::{HullFilter, Layers};
use crate::guided::{GuidedClass, GuidedEnd, GuidedRocket, COLLISION_RADIUS_PROVISIONAL as RADIUS};
use crate::unit::{Controller, UnitHandle};
use crate::vehicle::hover::{quat_x, quat_y};
use glam::Vec3;

impl Sim {
    pub fn guided_class(&self, r: &GuidedRocket) -> Option<&GuidedClass> {
        self.weapon_classes[r.weapon_class]
            .guided
            .as_ref()
            .map(|g| &g.class)
    }

    pub fn controlled_guided(&self, owner: UnitHandle) -> Option<&GuidedRocket> {
        self.guided
            .iter()
            .find(|r| r.owner == owner && r.pending_end.is_none())
    }

    pub(super) fn guided_input(&mut self, i: usize, slot: Option<usize>, held: bool) -> bool {
        let u = &self.units[i];
        let class = slot.map_or(u.weapon_class, |k| u.held[k].class);
        let wc = &self.weapon_classes[class];
        if wc.guided.is_none() {
            return false;
        }
        let locked = wc.award_locked(self.cheats.all_awards);
        let has = self
            .guided
            .iter()
            .any(|r| r.owner == self.handle(i) && r.weapon_class == class);
        let u = &mut self.units[i];
        let state = match slot {
            Some(k) => &mut u.held[k].state,
            None => &mut u.weapon,
        };
        let release = state.remote_press == 2 && held;
        if !held {
            state.remote_press = 0;
        }
        locked || has || release
    }

    pub(super) fn launch_guided(&mut self, i: usize, weapon_class: usize) {
        let Some(c) = self.weapon_classes[weapon_class].guided.as_ref() else {
            return;
        };
        let u = &self.units[i];
        let owner = self.handle(i);
        if self.weapon_classes[weapon_class].award_locked(self.cheats.all_awards)
            || self.guided.iter().any(|r| r.owner == owner)
        {
            return;
        }
        let (fwd, right, up) = u.aim_frame();
        let offset = c.spawn_offset;
        let origin = u.muzzle();
        let pos = u.soldier.pos + right * offset.x + up * offset.y + fwd * offset.z;
        self.next_guided_id = self
            .next_guided_id
            .checked_add(1)
            .expect("guided identity exhausted");
        self.guided.push(GuidedRocket {
            id: self.next_guided_id,
            owner,
            team: u.team,
            weapon_class,
            pos,
            vel: fwd * c.class.speeds[1],
            rot: quat_y(u.soldier.yaw) * quat_x(u.soldier.pitch),
            pitch_rate: 0.0,
            turn_rate: 0.0,
            bank: 0.0,
            health: c.class.health,
            energy: c.class.energy,
            age: 0.0,
            boosted: false,
            prev_buttons: u.soldier.prev_buttons() | buttons::FIRE,
            pending_end: None,
        });
        let last = self.guided.len() - 1;
        if let Some(at) = self.guided_contact(&self.guided[last], origin) {
            self.guided[last].pos = at;
            self.guided[last].pending_end = Some(GuidedEnd::Impact);
        }
        self.events.push(Event::Shot {
            unit: owner,
            weapon_class: weapon_class as u32,
            origin,
            dir: fwd,
        });
    }

    pub(super) fn guided_pads(&mut self, pads: &mut [PadState]) -> Vec<PadState> {
        let mut routed = Vec::with_capacity(self.guided.len());
        for k in 0..self.guided.len() {
            let r = &self.guided[k];
            let owner = r.owner;
            let Some(u) = self.unit(owner).filter(|u| u.mount.is_none()) else {
                self.guided[k].pending_end = Some(GuidedEnd::OwnerGone);
                routed.push(PadState::default());
                continue;
            };
            let pad = if u.controller == Controller::Ai {
                self.bot_guided_pad(k)
            } else {
                pads.get(owner.index as usize).copied().unwrap_or_default()
            };
            self.take_remote_pad(owner, pads);
            let r = &mut self.guided[k];
            if pad.buttons & !r.prev_buttons & buttons::ACTION != 0 {
                r.pending_end = Some(GuidedEnd::Returned);
            } else if pad.buttons & !r.prev_buttons & buttons::SECONDARY_FIRE != 0 {
                r.pending_end = Some(GuidedEnd::Destroyed);
            }
            r.prev_buttons = pad.buttons;
            routed.push(pad);
        }
        routed
    }

    fn bot_guided_pad(&self, k: usize) -> PadState {
        let r = &self.guided[k];
        let target = self.unit(r.owner).and_then(|u| u.ai.target).and_then(|h| {
            self.unit(h)
                .filter(|e| e.cloak.visible())
                .map(|u| u.aim_point())
        });
        let Some(target) = target.filter(|p| crate::perception::los(self, r.pos, *p)) else {
            return PadState::default();
        };
        let local = r.rot.inverse() * (target - r.pos).normalize_or_zero();
        PadState {
            rx: PadState::quantize(crate::fmath::atan2(local.x, -local.z) * 2.0),
            ry: PadState::quantize(crate::fmath::asin(local.y.clamp(-1.0, 1.0)) * 2.0),
            ..Default::default()
        }
    }

    pub(super) fn tick_guided(&mut self, pads: &[PadState], dt: f32) {
        let count = self.guided.len();
        for k in 0..count {
            let mut r = self.guided[k].clone();
            let c = self.guided_class(&r).unwrap();
            let owner_gone = self.unit(r.owner).is_none_or(|u| u.mount.is_some());
            if owner_gone {
                r.pending_end = Some(GuidedEnd::OwnerGone);
            } else if self
                .unit(r.owner)
                .is_some_and(|u| u.weapon_class != r.weapon_class)
            {
                r.pending_end = Some(GuidedEnd::Returned);
            }
            if r.health <= 0.0 && r.pending_end.is_none() {
                r.pending_end = Some(GuidedEnd::Destroyed);
            }
            if r.pending_end.is_none() {
                let previous = r.pos;
                r.fly(c, pads.get(k).unwrap_or(&PadState::default()), dt);
                if let Some(pos) = self.guided_contact(&r, previous) {
                    r.pos = pos;
                    r.pending_end = Some(GuidedEnd::Impact);
                } else if r.energy <= 0.0 {
                    r.pending_end = Some(
                        if r.boosted && r.vel.length() >= c.boost_speed * c.critical_speed {
                            GuidedEnd::CriticalExpiry
                        } else {
                            GuidedEnd::Expired
                        },
                    );
                } else if r.pos.y < -5000.0 || self.death_regions.iter().any(|v| v.contains(r.pos))
                {
                    r.pending_end = Some(GuidedEnd::Destroyed);
                }
            }
            self.guided[k] = r;
        }
        let mut k = 0;
        while k < self.guided.len() {
            let Some(end) = self.guided[k].pending_end else {
                k += 1;
                continue;
            };
            let r = self.guided.remove(k);
            let explosion = self.weapon_classes[r.weapon_class]
                .guided
                .as_ref()
                .and_then(|c| c.explosion(end))
                .cloned();
            if let Some(e) = explosion {
                self.explode(&e, r.pos, Some(r.owner), Some(r.team));
            }
            self.events.push(Event::GuidedGone {
                pos: r.pos,
                weapon_class: r.weapon_class as u32,
                end,
            });
            self.release_remote(r.owner, r.prev_buttons);
            if let Some(u) = self.unit_mut(r.owner) {
                if u.weapon_class == r.weapon_class {
                    u.weapon.remote_press = 2;
                }
                for h in &mut u.held {
                    if h.class == r.weapon_class {
                        h.state.remote_press = 2;
                    }
                }
            }
        }
    }

    fn guided_contact(&self, r: &GuidedRocket, from: Vec3) -> Option<Vec3> {
        let delta = r.pos - from;
        let len = delta.length();
        if len <= 0.0 {
            return None;
        }
        let dir = delta / len;
        let mut nearest = None;
        let mut offer = |distance: f32| {
            if distance <= len && nearest.is_none_or(|t| distance < t) {
                nearest = Some(distance);
            }
        };
        if let Some(hf) = &self.heightfield {
            if let Some(hit) = crate::collision::ray_heightfield(
                hf,
                &self.collision.cutters,
                from,
                dir,
                len,
                crate::projectile::TERRAIN_RAY_STEP,
            ) {
                if !self.collision.is_cut(from + dir * hit) {
                    offer(hit);
                }
            }
        }
        if let Some(hit) =
            self.collision
                .ray_with(from, dir, len, Layers::ORDNANCE, HullFilter::All)
        {
            offer(hit.t);
        }
        if let Some(t) = self
            .water_plane()
            .and_then(|water| water.crossing(from, dir))
        {
            offer(t);
        }
        for (_, u) in self
            .alive_units()
            .filter(|(h, u)| *h != r.owner && u.mount.is_none())
        {
            if let Some((t, _)) = u.bolt_prims().hit(from, dir, len) {
                offer(t);
            }
        }
        for d in self.droids.iter().filter(|d| d.owner != r.owner) {
            if let Some(t) =
                crate::unit::ray_sphere(from, dir, d.pos, crate::remote::RADIUS + RADIUS)
            {
                offer(t);
            }
        }
        for turret in self.turrets.iter().filter(|t| t.health > 0.0) {
            if let Some(t) = crate::unit::ray_sphere(
                from,
                dir,
                turret.centre(),
                crate::autoturret::RADIUS + RADIUS,
            ) {
                offer(t);
            }
        }
        nearest.map(|t| from + dir * t)
    }
}

#[cfg(test)]
mod tests;
