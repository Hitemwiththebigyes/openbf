//! The health, ammo and vehicle droids (`powerupstation`, card PWST,
//! `analysis/re-powerup-station.md`). Step 5c' of the tick, after the items.
//!
//! - The reach (§3): the instance's `Radius` is a sphere at the station's
//!   origin, fixed at load (the idle turns do not move it); a unit is inside
//!   while its feet are strictly inside it, in 3-D.
//! - The pass (§4): each station keeps its own clock; once it reaches the
//!   class's `PowerupDelay` (the remainder kept, one pass a tick at most) the
//!   station gives ONCE to every unit inside the class's amount for that
//!   unit's health type, with the pickups' give rules (health to the cap,
//!   +N clips to every weapon to its cap; a station passes no dispenser
//!   skip). No team is read: friend and foe are served alike. Every
//!   station's clock starts at 0 with the battle (§4, measured).
//! - The state (§4, §6): a pass that served anyone leaves the station
//!   SERVING (the game layer's arm swing / bob); a pass that served nobody
//!   ends it. The animation itself is the game layer's.
//!
//! PROVISIONAL (ours): the living only (§12: a corpse inside the sphere is
//! not checked on the PS2); a rider is not in the world and is not served,
//! its vehicle is; a vehicle gets health only (a vehicle droid's +1 clip
//! waits for vehicle reserves, which the port's seat guns do not keep). The
//! gather's order is the port's slot order, units before vehicles; the
//! PS2's 64-entity cap is kept. BF2's hero refusal and regeneration test
//! (§4) wait for those units to exist; no station breaks yet (card PWSTD).

use super::pickups::give_ammo;
use super::state::{Event, Sim};
use crate::unit::UnitHandle;
use crate::vehicle::VehicleHandle;
use crate::weapon::{Health, HealthType};
use glam::Vec3;

/// The most entities one pass serves (the gather's list, B1 236313).
pub const GATHER_MAX: usize = 64;
/// A station class's `PowerupDelay` default (s; B1 `FUN_002aa5d8`, B2
/// `FUN_00368898`).
pub const DELAY_DEFAULT: f32 = 0.5;

/// One `powerupstation` class (§2.1), built by `game::roster` from the disc.
/// SETUP data like `Sim::item_classes`: not hashed.
#[derive(Clone, Debug, PartialEq)]
pub struct StationClass {
    pub name: String,
    /// `PowerupDelay` (s): the time between two passes.
    pub delay: f32,
    /// Per health type (person, animal, droid, vehicle, building): health
    /// given per pass (`PersonHealth` .. ; `SoldierHealth` = person and
    /// droid).
    pub health: [f32; 5],
    /// Per health type: clips added to every weapon per pass.
    pub ammo: [u32; 5],
    /// What the game layer draws and plays (the sim reads none of it):
    /// `GeometryName`, `PowerupSound` (at the station per unit served) and
    /// `AmbientSound`.
    pub geometry: String,
    pub powerup_sound: Option<String>,
    pub ambient_sound: Option<String>,
    /// The idle and serving animation's keys (§2.1, §6), read by the game
    /// layer only.
    pub anim: StationAnim,
}

/// A station class's animation keys (§2.1, §6; every default 0 / none,
/// B1 `FUN_002aa5d8`): the game layer's `viewer::stations` runs them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StationAnim {
    /// `IdleRotateSpeed` (rad/s): the idle turn about +Y; 0 = none.
    pub rotate_speed: f32,
    /// `IdleWaitTime` (s) between two idle turns.
    pub wait_time: f32,
    /// `IdleWobbleNode`: rocks about Z in a turn, bobs while serving.
    pub wobble_node: Option<String>,
    /// `IdleWobbleFactor`: the rock's amplitude (rad) and the bob's depth (m).
    pub wobble_factor: f32,
    /// `ActiveRotateNode` (the first 4): each swings about X while serving.
    pub rotate_nodes: Vec<String>,
    /// `ActiveSpinNode`: spins about Z while serving.
    pub spin_node: Option<String>,
}

impl StationClass {
    /// A station that gives soldiers (persons and droids) `health` and
    /// `ammo` every `delay` s.
    pub fn soldier(name: &str, delay: f32, health: f32, ammo: u32) -> Self {
        StationClass {
            name: name.to_owned(),
            delay,
            health: [health, 0.0, health, 0.0, 0.0],
            ammo: [ammo, 0, ammo, 0, 0],
            geometry: String::new(),
            powerup_sound: None,
            ambient_sound: None,
            anim: StationAnim::default(),
        }
    }
}

/// A placed station (`Sim::stations`). HASHED when there is one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Station {
    /// Index into `Sim::station_classes`.
    pub class: u8,
    /// The sphere's centre: the instance's origin.
    pub pos: Vec3,
    /// The instance's `Radius` (m).
    pub radius: f32,
    /// The station's clock (s), from 0 to the class's delay.
    pub timer: f32,
    /// The last pass served someone (§4: state 3, else 1).
    pub serving: bool,
}

impl Station {
    pub fn new(class: u8, pos: Vec3, radius: f32) -> Self {
        Station {
            class,
            pos,
            radius,
            timer: 0.0,
            serving: false,
        }
    }

    /// Strictly inside the sphere, in 3-D (§3).
    pub fn reaches(&self, feet: Vec3) -> bool {
        (feet - self.pos).length_squared() < self.radius * self.radius
    }

    pub fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(&[self.class, self.serving as u8]);
        for v in [self.pos.x, self.pos.y, self.pos.z, self.radius, self.timer] {
            eat(&v.to_bits().to_le_bytes());
        }
    }
}

/// The class table's slot for a health type (the PS2's flags & 7, §2.1);
/// the port's extra building kinds are buildings.
fn type_slot(t: HealthType) -> usize {
    (t as usize).min(4)
}

/// re-powerups §5.3 as the station calls it: a non-zero amount, and the
/// unit below its cap (or a negative amount), gives min(cur + amount, max).
pub(super) fn give_health(h: &mut Health, amount: f32) -> bool {
    if amount == 0.0 || (h.cur >= h.max && amount > 0.0) {
        return false;
    }
    let new = (h.cur + amount).min(h.max);
    let changed = new != h.cur;
    h.cur = new;
    changed
}

impl Sim {
    /// Register a station class (one entry per name) and return its index.
    pub fn add_station_class(&mut self, c: StationClass) -> u8 {
        if let Some(i) = self.station_classes.iter().position(|x| x.name == c.name) {
            return i as u8;
        }
        self.station_classes.push(c);
        (self.station_classes.len() - 1) as u8
    }

    /// Step 5c': every station's clock, and the passes that come due.
    pub(super) fn tick_stations(&mut self, dt: f32) {
        for si in 0..self.stations.len() {
            let s = self.stations[si];
            let Some(c) = self.station_classes.get(usize::from(s.class)) else {
                continue;
            };
            let delay = c.delay;
            let timer = s.timer + dt;
            if timer < delay {
                self.stations[si].timer = timer;
                continue;
            }
            self.stations[si].timer = timer - delay;
            let served = self.station_pass(si);
            self.stations[si].serving = served;
        }
    }

    /// One pass of station `si` (§4): gives to everyone inside; true when
    /// it gave to anyone.
    fn station_pass(&mut self, si: usize) -> bool {
        let s = self.stations[si];
        let c = self.station_classes[usize::from(s.class)].clone();
        let mut gathered = 0;
        let mut served = false;
        for ui in 0..self.units.len() {
            if gathered >= GATHER_MAX {
                return served;
            }
            let u = &self.units[ui];
            if !u.alive || u.mount.is_some() || !s.reaches(u.soldier.pos) {
                continue;
            }
            gathered += 1;
            let t = type_slot(u.health.health_type);
            let classes = &self.weapon_classes;
            let u = &mut self.units[ui];
            let health = give_health(&mut u.health, c.health[t]);
            let ammo = give_ammo(classes, u, c.ammo[t]);
            if health || ammo {
                served = true;
                let unit = UnitHandle {
                    index: ui as u32,
                    gen: u.gen,
                };
                self.events.push(Event::StationServed {
                    station: si as u16,
                    unit: Some(unit),
                    vehicle: None,
                    pos: s.pos,
                    health,
                    ammo,
                    energy: false,
                });
            }
        }
        for vi in 0..self.vehicles.len() {
            if gathered >= GATHER_MAX {
                break;
            }
            let v = &mut self.vehicles[vi];
            if !v.alive || !s.reaches(v.pos) {
                continue;
            }
            gathered += 1;
            let t = type_slot(v.health.health_type);
            if give_health(&mut v.health, c.health[t]) {
                served = true;
                let vehicle = VehicleHandle {
                    index: vi as u32,
                    gen: v.gen,
                };
                self.events.push(Event::StationServed {
                    station: si as u16,
                    unit: None,
                    vehicle: Some(vehicle),
                    pos: s.pos,
                    health: true,
                    ammo: false,
                    energy: false,
                });
            }
        }
        served
    }
}

#[cfg(test)]
mod tests;
