use super::{buttons, Event, PadState, Sim};
use crate::collision::{Capsule, HullFilter};
use crate::projectile::{Bolt, Shooter};
use crate::remote::{Droid, DroidClass, DroidEnd, StrikeRun, BOT_RANGE, BOT_RECALL, RADIUS};
use crate::unit::{Controller, UnitHandle};
use crate::weapon::{shot_direction, FireInput};
use glam::Vec3;

impl Sim {
    pub fn controlled_droid(&self, h: UnitHandle) -> Option<&Droid> {
        self.droids.iter().find(|d| d.owner == h && d.controlled)
    }

    pub fn droid_class(&self, d: &Droid) -> Option<&DroidClass> {
        self.weapon_classes[d.weapon_class]
            .remote
            .as_ref()
            .map(|r| &r.droid)
    }

    pub fn droid_of(&self, h: UnitHandle, weapon: usize) -> Option<&Droid> {
        self.droids
            .iter()
            .find(|d| d.owner == h && d.weapon_class == weapon)
    }

    pub(super) fn droid_input(
        &mut self,
        i: usize,
        slot: Option<usize>,
        weapon: usize,
        held: bool,
        blocked: bool,
    ) -> bool {
        let owner = self.handle(i);
        let droid = self
            .droids
            .iter()
            .position(|d| d.owner == owner && d.weapon_class == weapon);
        let state = match slot {
            Some(k) => &mut self.units[i].held[k].state,
            None => &mut self.units[i].weapon,
        };
        let fresh = held && state.remote_press == 0;
        state.remote_press = u8::from(held);
        let Some(k) = droid else {
            return false;
        };
        let take = self.weapon_classes[weapon]
            .remote
            .as_ref()
            .is_some_and(|r| r.take_control);
        if fresh && take && !blocked && !self.droids[k].controlled {
            let d = &mut self.droids[k];
            d.controlled = true;
            d.idle = 0.0;
            d.prev_buttons = self.units[i].soldier.prev_buttons() | buttons::SECONDARY_FIRE;
        }
        true
    }

    pub(super) fn launch_droid(&mut self, i: usize, weapon: usize, charge: f32) {
        let Some(r) = self.weapon_classes[weapon].remote.clone() else {
            return;
        };
        let u = &self.units[i];
        let owner = self.handle(i);
        let (aim, _, _) = u.aim_frame();
        let d = self.weapon_classes[weapon].thrown.and_then(|t| t.hand);
        let pos = u.launch_origin(d);
        let speed = r.launch_speed(charge);
        let (team, yaw, human, vel) = (u.team, u.soldier.yaw, u.is_player, u.soldier.vel);
        self.next_droid_id = self
            .next_droid_id
            .checked_add(1)
            .expect("droid identity exhausted");
        let mut droid = Droid::new(
            self.next_droid_id,
            owner,
            team,
            weapon,
            &r,
            pos,
            aim * speed + vel,
            yaw,
            human,
        );
        droid.prev_buttons = self.units[i].soldier.prev_buttons();
        self.droids.push(droid);
        self.events.push(Event::Throw {
            unit: owner,
            weapon_class: weapon as u32,
            origin: pos,
        });
    }

    pub(super) fn droid_pads(&mut self, unit_pads: &mut [PadState]) -> Vec<Option<PadState>> {
        (0..self.droids.len())
            .map(|k| self.droid_pad(k, unit_pads))
            .collect()
    }

    fn droid_pad(&mut self, k: usize, unit_pads: &mut [PadState]) -> Option<PadState> {
        let d = &self.droids[k];
        if !d.controlled {
            return None;
        }
        let i = d.owner.index as usize;
        if self.unit(d.owner).is_none_or(|u| u.mount.is_some()) {
            self.droids[k].controlled = false;
            return None;
        }
        let pad = if self.units[i].controller == Controller::Ai {
            self.bot_droid_pad(k)
        } else {
            unit_pads.get(i).copied().unwrap_or_default()
        };
        let d = &mut self.droids[k];
        let exit = pad.pressed(buttons::ACTION) && d.prev_buttons & buttons::ACTION == 0;
        let bot_done = self.units[i].controller == Controller::Ai && d.idle > BOT_RECALL;
        if let Some(p) = unit_pads.get_mut(i) {
            *p = PadState::default();
        }
        if exit || bot_done {
            d.controlled = false;
            let owner = d.owner;
            self.release(owner, pad.buttons);
            return None;
        }
        Some(pad)
    }

    fn bot_droid_pad(&self, k: usize) -> PadState {
        let d = &self.droids[k];
        let Some(r) = self.weapon_classes[d.weapon_class].remote.as_ref() else {
            return PadState::default();
        };
        let blaster = &self.weapon_classes[r.blaster_index];
        let eye = d.pos;
        let target = self
            .droid_enemies(d)
            .filter(|p| crate::perception::los(self, eye, *p))
            .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)));
        let Some(target) = target else {
            return PadState::default();
        };
        let to = target - eye;
        let dist = to.length();
        let want_yaw = crate::fmath::atan2(-to.x, -to.z);
        let want_pitch = crate::fmath::atan2(to.y, to.with_y(0.0).length());
        let yaw_err = crate::vehicle::gun::wrap(want_yaw - d.yaw);
        let pitch_err = want_pitch - d.pitch;
        let [_, optimal, max] = blaster.ai_range;
        let forward = if dist > optimal {
            1.0
        } else if dist < optimal * 0.5 {
            -0.5
        } else {
            0.0
        };
        let fire = yaw_err.abs() < 0.15 && dist <= max && d.prev_buttons & buttons::FIRE == 0;
        PadState {
            ly: PadState::quantize(forward),
            rx: PadState::quantize(-yaw_err * 2.0),
            ry: PadState::quantize(pitch_err * 2.0),
            buttons: if fire { buttons::FIRE } else { 0 },
            ..PadState::default()
        }
    }

    fn droid_enemies(&self, d: &Droid) -> impl Iterator<Item = Vec3> + '_ {
        let (team, eye) = (d.team, d.pos);
        self.alive_units()
            .filter(move |(_, u)| u.team != team && u.mount.is_none())
            .map(|(_, u)| u.soldier.pos + Vec3::Y)
            .filter(move |p| p.distance_squared(eye) <= BOT_RANGE * BOT_RANGE)
    }

    pub(super) fn tick_droids(&mut self, dt: f32, pads: &[Option<PadState>]) {
        self.tick_strikes(dt);
        let mut k = 0;
        let mut pad_at = 0;
        while k < self.droids.len() {
            let routed = pads.get(pad_at).copied().flatten();
            pad_at += 1;
            let mut d = self.droids[k].clone();
            let Some(r) = self.weapon_classes[d.weapon_class].remote.as_deref() else {
                self.droids.remove(k);
                continue;
            };
            let (c, blaster) = (r.droid.clone(), r.blaster_index);
            let charge_time = r.destruct.as_ref().map(|x| x.charge_time);
            let owner = self
                .unit(d.owner)
                .map(|u| (u.is_player, u.look_scale(), u.controller));
            let Some((is_player, look, controller)) = owner else {
                self.droids.remove(k);
                self.droid_gone(&d, DroidEnd::Recalled);
                continue;
            };
            let pad = routed.unwrap_or_default();
            if controller == Controller::Ai && d.controlled {
                d.idle = if self.droid_enemies(&d).next().is_some() {
                    0.0
                } else {
                    d.idle + dt
                };
            }
            d.yaw -= PadState::axis(pad.rx) * c.max_turn_speed * dt;
            d.pitch += PadState::axis(pad.ry) * c.max_turn_speed * dt;
            (d.yaw, d.pitch) = d.mouse_turned(&c, pad.look(), look);
            let (floor, normal) = self.ground_point_under_n(d.pos, d.pos.y);
            let ground = (floor > crate::sticky::WORLD_FLOOR).then_some((d.pos.y - floor, normal));
            d.fly(
                &c,
                dt,
                PadState::axis(pad.ly),
                PadState::axis(pad.lx),
                ground,
            );
            self.droid_walls(&mut d);
            let mut end = if d.alive() {
                d.drain(&c, dt)
            } else {
                Some(DroidEnd::Destroyed)
            };
            let press = pad.pressed(buttons::SECONDARY_FIRE)
                && d.prev_buttons & buttons::SECONDARY_FIRE == 0;
            if let Some(t) = d.charge {
                d.charge = Some(t + dt);
            } else if charge_time.is_some() && press && end.is_none() {
                d.charge = Some(0.0);
                self.events.push(Event::DroidCharge {
                    pos: d.pos,
                    weapon_class: d.weapon_class as u32,
                });
            }
            if d.charge.zip(charge_time).is_some_and(|(t, max)| t >= max) && end.is_none() {
                end = Some(DroidEnd::Destruct);
            }
            let fire = pad.pressed(buttons::FIRE) && end.is_none();
            let mut shots = Vec::new();
            d.blaster.update(
                dt,
                &self.weapon_classes[blaster],
                FireInput {
                    held: fire,
                    blocked: false,
                    infinite_ammo: false,
                    ai_dispersion: None,
                },
                &mut self.lcg,
                &mut shots,
            );
            if !shots.is_empty() {
                self.droid_shots(&d, blaster, is_player, &shots);
            }
            if let Some(p) = routed {
                d.prev_buttons = p.buttons;
            }
            if d.pos.y < crate::sticky::WORLD_FLOOR {
                end = Some(DroidEnd::Destroyed);
            }
            self.droids.remove(k);
            if let Some(end) = end {
                if end == DroidEnd::Destruct {
                    self.destruct(&d);
                }
                if d.controlled {
                    self.release(d.owner, d.prev_buttons);
                }
                self.droid_gone(&d, end);
                continue;
            }
            self.droids.insert(k, d);
            k += 1;
        }
    }

    fn release(&mut self, owner: UnitHandle, buttons: u32) {
        if let Some(u) = self.unit_mut(owner) {
            u.soldier.set_prev_buttons(buttons);
        }
    }

    fn free_bolt(&self, origin: Vec3, dir: Vec3, speed: f32, life: f32, who: Shooter) -> Bolt {
        Bolt::fire(
            origin,
            dir,
            speed,
            life,
            who,
            self.heightfield.as_ref(),
            self.water_plane(),
            &self.collision,
            HullFilter::All,
        )
        .shaped(&self.weapon_classes[who.weapon_class].ordnance)
    }

    fn droid_walls(&self, d: &mut Droid) {
        if let Some(hf) = &self.heightfield {
            let t = hf.height_at(d.pos.x, d.pos.z) + RADIUS;
            if d.pos.y < t && !self.collision.is_cut(d.pos) {
                d.pos.y = t;
                d.vel.y = d.vel.y.max(0.0);
            }
        }
        if self.collision.is_empty() {
            return;
        }
        let cap = Capsule {
            base: d.pos - Vec3::Y * RADIUS,
            height: 2.0 * RADIUS,
            radius: RADIUS,
        };
        let push = self.collision.push_out(&cap);
        let len = push.length();
        if len > 0.0 {
            let n = push / len;
            d.pos += push;
            let vn = d.vel.dot(n);
            if vn < 0.0 {
                d.vel -= n * vn;
            }
        }
    }

    fn droid_shots(
        &mut self,
        d: &Droid,
        blaster: usize,
        is_player: bool,
        shots: &[crate::weapon::ShotSpec],
    ) {
        let (fwd, right, up) = d.aim_frame();
        let origin = d.muzzle();
        let who = Shooter {
            owner: d.owner,
            team: d.team,
            weapon_class: blaster,
            is_player,
        };
        for spec in shots {
            let dir = shot_direction(
                &mut self.lcg,
                fwd,
                right,
                up,
                spec.pitch_spread,
                spec.yaw_spread,
                spec.tilt,
            );
            let blend = self.weapon_classes[blaster].ordnance.charged(spec.charge);
            let bolt = self.free_bolt(origin, dir, blend.velocity, blend.life_span, who);
            self.events.push(Event::Shot {
                unit: d.owner,
                weapon_class: blaster as u32,
                origin,
                dir,
            });
            self.bolts.push(bolt);
        }
    }

    fn destruct(&mut self, d: &Droid) {
        let Some(x) = self.weapon_classes[d.weapon_class]
            .remote
            .as_ref()
            .and_then(|r| r.destruct.as_ref())
        else {
            return;
        };
        let strike = x.salvo.as_ref().map(|s| StrikeRun {
            owner: d.owner,
            team: d.team,
            weapon_class: d.weapon_class,
            target: d.pos,
            left: s.count,
            timer: s.initial,
        });
        let burst = x.burst.clone();
        self.strikes.extend(strike);
        if let Some(e) = burst {
            self.explode(&e, d.pos, Some(d.owner), Some(d.team));
        }
    }

    fn droid_gone(&mut self, d: &Droid, end: DroidEnd) {
        self.events.push(Event::DroidGone {
            pos: d.pos,
            weapon_class: d.weapon_class as u32,
            end,
        });
        let e = self.droid_class(d).and_then(|c| c.explosion.clone());
        if let Some(e) = e {
            self.explode(&e, d.pos, Some(d.owner), Some(d.team));
        }
    }

    fn tick_strikes(&mut self, dt: f32) {
        let mut k = 0;
        while k < self.strikes.len() {
            self.strikes[k].timer -= dt;
            if self.strikes[k].timer > 0.0 {
                k += 1;
                continue;
            }
            let run = self.strikes[k].clone();
            let Some(r) = self.weapon_classes[run.weapon_class].remote.as_deref() else {
                self.strikes.remove(k);
                continue;
            };
            let Some(s) = r.destruct.as_ref().and_then(|x| x.salvo.as_ref()) else {
                self.strikes.remove(k);
                continue;
            };
            let draws = [
                self.lcg.unit(),
                self.lcg.unit(),
                self.lcg.unit(),
                self.lcg.unit(),
            ];
            let (origin, dir) = s.shot(run.target, draws);
            let (shell, delay) = (r.shell_index, s.delay);
            let ord = &self.weapon_classes[shell].ordnance;
            let who = Shooter {
                owner: run.owner,
                team: run.team,
                weapon_class: shell,
                is_player: self.unit(run.owner).is_some_and(|u| u.is_player),
            };
            let bolt = self.free_bolt(origin, dir, ord.velocity, ord.life_span, who);
            self.bolts.push(bolt);
            let run = &mut self.strikes[k];
            run.left -= 1;
            run.timer += delay;
            if run.left == 0 {
                self.strikes.remove(k);
            } else {
                k += 1;
            }
        }
    }

    pub(super) fn droid_damage(
        &self,
        d: &Droid,
        damage: f32,
        scales: &[f32; 7],
        team: Option<u8>,
    ) -> f32 {
        let ht = self
            .droid_class(d)
            .map_or(crate::weapon::HealthType::Droid, |c| c.health_type);
        let ff = if team == Some(d.team) {
            self.friendly_fire_scale
        } else {
            1.0
        };
        (damage * scales[ht as usize] * ff).max(0.0)
    }
}

#[cfg(test)]
mod tests;
