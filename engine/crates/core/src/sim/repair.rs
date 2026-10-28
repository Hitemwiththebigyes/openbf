use super::{Event, Sim};
use crate::collision::{HullFilter, Layers};
use crate::projectile::Target;
use crate::unit::Unit;
use crate::weapon::WeaponClass;
use glam::Vec3;

impl Sim {
    pub(super) fn tick_repair_weapon(
        &mut self,
        i: usize,
        held_slot: Option<usize>,
        held: bool,
        blocked: bool,
        dt: f32,
    ) -> bool {
        let u = &self.units[i];
        let class = held_slot.map_or(u.weapon_class, |k| u.held[k].class);
        let wc = &self.weapon_classes[class];
        if wc.repair.is_none() {
            return false;
        }
        let target = self.repair_target(i, class);
        let u = &mut self.units[i];
        let state = match held_slot {
            Some(k) => &mut u.held[k].state,
            None => &mut u.weapon,
        };
        if state.repair_tick(wc, held && target.is_some(), blocked, dt) {
            if let Some(target) = target {
                self.fire_repair(i, class, target);
            }
        }
        true
    }

    fn repair_candidate(
        &self,
        u: &Unit,
        wc: &WeaponClass,
        sphere: (Vec3, f32),
        owner: (Option<u8>, Option<u32>, HullFilter),
    ) -> Option<f32> {
        let (centre, radius) = sphere;
        let (team, instance, hulls) = owner;
        let relation = match team {
            Some(t) if t == u.team => 2,
            Some(_) => 0,
            None => 1,
        };
        if wc.target_team & (1 << relation) == 0 {
            return None;
        }
        let from = u.muzzle();
        let delta = centre - from;
        let dist = delta.length();
        if dist > wc.lock_on_range + radius || dist <= 0.0 {
            return None;
        }
        let dir = delta / dist;
        let score = super::input::cone_score(
            radius,
            wc.lock_on_angle,
            wc.lock_on_range,
            from,
            u.aim_frame().0,
            centre,
        );
        if score < 0.0 {
            return None;
        }
        if !crate::perception::los_terrain(self, from, centre)
            || self
                .collision
                .ray_past(
                    from,
                    dir,
                    (dist - 0.01).max(0.0),
                    Layers::ORDNANCE,
                    hulls,
                    [instance, None],
                )
                .is_some()
        {
            return None;
        }
        Some(score)
    }

    fn repair_target(&self, i: usize, class: usize) -> Option<Target> {
        let u = &self.units[i];
        let wc = &self.weapon_classes[class];
        let repair = wc.repair?;
        let mut best: Option<(f32, Target)> = None;
        for (si, s) in self.stations.iter().enumerate() {
            let kind = s.repair_kind() as usize;
            if repair.health[kind] <= 0.0 || s.health.cur >= s.health.max {
                continue;
            }
            if let Some(score) = self.repair_candidate(
                u,
                wc,
                (s.sphere_centre, s.sphere_radius),
                (s.team, s.instance, HullFilter::All),
            ) {
                if best.is_none_or(|(old, _)| score < old) {
                    best = Some((score, Target::Station(si as u16)));
                }
            }
        }
        for (vh, v) in self.alive_vehicles() {
            if v.team.is_some_and(|t| t != u.team)
                || repair.health[v.health.health_type as usize] <= 0.0
                || v.health.cur >= v.health.max
            {
                continue;
            }
            let (centre, radius) = v.aim_sphere.unwrap_or((Vec3::ZERO, 0.0));
            let centre = v.pos + v.rot * centre;
            if let Some(score) = self.repair_candidate(
                u,
                wc,
                (centre, radius),
                (v.team, v.instance, HullFilter::except(Some(vh))),
            ) {
                if best.is_none_or(|(old, _)| score < old) {
                    best = Some((score, Target::Vehicle(vh)));
                }
            }
        }
        best.map(|(_, target)| target)
    }

    fn fire_repair(&mut self, i: usize, class: usize, target: Target) {
        let repair = self.weapon_classes[class].repair.unwrap();
        let cost = self.weapon_classes[class].heat_per_shot * self.units[i].consts.energy_bar;
        if !(self.units[i].is_player && self.cheats.infinite_stamina) {
            if self.units[i].soldier.energy < cost {
                return;
            }
            self.units[i].soldier.energy -= cost;
        }
        let (pos, amount) = match target {
            Target::Station(si) => {
                let s = &self.stations[usize::from(si)];
                let kind = s.repair_kind() as usize;
                let before = s.health.cur;
                let pos = s.sphere_centre;
                self.repair_station(si, repair.health[kind]);
                (pos, self.stations[usize::from(si)].health.cur - before)
            }
            Target::Vehicle(vh) => {
                let Some(v) = self.vehicle_mut(vh) else {
                    return;
                };
                let before = v.health.cur;
                v.health.cur =
                    (v.health.cur + repair.health[v.health.health_type as usize]).min(v.health.max);
                (v.pos, v.health.cur - before)
            }
            Target::Unit(_) | Target::Mine(_) | Target::Droid(_) | Target::Turret(_) => return,
        };
        if amount > 0.0 {
            self.events.push(Event::Repair {
                unit: self.handle(i),
                weapon_class: class as u32,
                target,
                pos,
                amount,
            });
        }
    }

    pub fn repair_station(&mut self, station: u16, amount: f32) {
        let Some(s) = self.stations.get_mut(usize::from(station)) else {
            return;
        };
        if amount <= 0.0 || !amount.is_finite() {
            return;
        }
        s.health.cur = (s.health.cur + amount).min(s.health.max);
        if !s.destroyed || s.health.cur < s.health.max {
            return;
        }
        s.destroyed = false;
        s.serving = false;
        let pose = s.pose();
        let c = &self.station_classes[usize::from(s.class)];
        if let Some([x, y, z, r]) = c.sphere {
            s.sphere_centre = pose.transform_point3(Vec3::new(x, y, z));
            s.sphere_radius = r;
        }
        if let Some(instance) = s.instance {
            let parts = self
                .station_live_collision
                .get(usize::from(station))
                .map_or(&[][..], Vec::as_slice);
            self.collision.replace_instance(instance, parts, pose);
            self.collision.bind_station(instance, station);
            let bodies = self.collision.prepare_rotation(instance, pose);
            self.station_bodies
                .resize_with(self.stations.len(), Vec::new);
            self.station_bodies[usize::from(station)] = bodies;
        }
    }
}

#[cfg(test)]
mod tests;
