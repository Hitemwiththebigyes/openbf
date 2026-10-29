//! A vehicle's engine loop (card VSND, `analysis/re-vehicle-sounds.md` `[O]`):
//! the class's `EngineSound` names a `soundparameterized` record - 1-4
//! looping layers, each with graphs that turn the vehicle's speed,
//! acceleration, health and its turn-on / turn-off timer into a gain factor
//! and a pitch factor. Presentation only: nothing here is sim state and
//! nothing is hashed.
//!
//! - [`graph`], [`factors`]: the record's arithmetic (§1.4, §2.2) - a graph
//!   is linear between its points and flat outside; every graph of a layer
//!   on one target multiplies.
//! - [`Instance`]: the PS2's engine component of one vehicle (§3, §4) - on
//!   while the pilot seat has a pilot, `TurnOffTime` of run-down after, the
//!   hover's filtered acceleration, the walker's `previous - speed / dt`.
//! - [`EngineSounds`]: one instance per vehicle of a battle, stepped once
//!   per sim tick; its [`Voice`]s are what `SoundLib::loop_voices` plays and
//!   what the `SOUND_LOG` referee lists.
//!
//! - [`EngineSounds::tick`] also runs a jet soldier's engine (card JETP2,
//!   `analysis/re-jetpack.md` §8, re-vehicle-sounds.md §3-§4): on in a jet
//!   stance, fed |v| in m/s and `previous - speed / dt` like the walker,
//!   with the class's `TurnOnSound` at the turn-on and `TurnOffSound` when
//!   the run-down ends ([`JetShot`]) - on every jet's end: a tap, empty
//!   fuel, a landing, a death.
//!
//! Not here (card VSN2): the vehicles' `TurnOnSound` / `TurnOffSound`, walker
//! steps, the turret whir, collision. Flyers and the droideka feed the same
//! component on the PS2 (§4) and are not fed here.

use crate::classdb::{fnv1a_lowercase, ClassDb};
use crate::walk::WalkSim;
use core::vehicle::{Vehicle, VehicleKind};
use glam::Vec3;
use manifest::{EngineLayer, EngineSound};
use std::collections::HashMap;

/// `TurnOffTime` from the component's constructor (B1 `FUN_00366cb8`).
pub const TURN_OFF_TIME: f32 = 3.0;
/// The hover's acceleration filter, per update (ELF 0x3c23d70a; the PS2 and
/// the port both run 30 updates a second).
pub const ACCELERATION_FILTER: f32 = 0.01;

/// A graph at `x` (B1 `FUN_0045fd50`): the first point's y before it, the
/// last point's y from it on, linear between two points. No points: 1.
pub fn graph(points: &[(f32, f32)], x: f32) -> f32 {
    let mut prev: Option<(f32, f32)> = None;
    for &(px, py) in points {
        if x < px {
            return match prev {
                None => py,
                Some((qx, qy)) => {
                    let t = (px - x) / (px - qx);
                    qy * t + py * (1.0 - t)
                }
            };
        }
        prev = Some((px, py));
    }
    prev.map_or(1.0, |p| p.1)
}

/// What the component sends in one update.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Inputs {
    pub speed: f32,
    pub acceleration: f32,
    /// Health / max health.
    pub health: f32,
    /// Seconds since the turn-on, or since the turn-off began.
    pub timer: f32,
    /// The run-down: `turnofftime` graphs read the timer and `turnontime`
    /// graphs 1.0; otherwise the other way round (B1 `FUN_00365bc0`).
    pub turning_off: bool,
}

/// One layer's (gain factor, pitch factor): the product of its graphs per
/// target (B1 `FUN_004610b8`). `proximity` is 1.0 for every class but
/// BF2's flyer (§5.2); a parameter no disc record uses, and a target other
/// than Gain and Pitch, multiply nothing here.
pub fn factors(layer: &EngineLayer, i: &Inputs) -> (f32, f32) {
    let (mut gain, mut pitch) = (1.0, 1.0);
    for g in &layer.graphs {
        let y = match g.parameter.as_str() {
            "speed" => graph(&g.points, i.speed),
            "acceleration" => graph(&g.points, i.acceleration),
            "health" => graph(&g.points, i.health),
            "proximity" => graph(&g.points, 1.0),
            "turnontime" if !i.turning_off => graph(&g.points, i.timer),
            "turnofftime" if i.turning_off => graph(&g.points, i.timer),
            _ => 1.0,
        };
        match g.target.as_str() {
            "Gain" => gain *= y,
            "Pitch" => pitch *= y,
            _ => {}
        }
    }
    (gain, pitch)
}

/// What a class feeds the component (§4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drive {
    /// |forward . velocity| / S, S = `ForwardSpeed` when the throttle is
    /// forward, else `ReverseSpeed`; the filtered acceleration.
    Hover { forward: f32, reverse: f32 },
    /// |velocity| in m/s; `previous - speed / dt`.
    Walker,
}

/// A vehicle class's engine keys.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineClass {
    /// `EngineSound`: the record's name.
    pub record: String,
    /// `TurnOffTime`, seconds of run-down after the pilot leaves.
    pub turn_off_time: f32,
}

impl EngineClass {
    /// `class`'s `EngineSound` and `TurnOffTime`, if it names a record.
    pub fn from_db(db: &ClassDb, class: &str) -> Option<EngineClass> {
        let record = db.prop(class, "EngineSound")?.trim().to_owned();
        if record.is_empty() {
            return None;
        }
        let time = db.prop(class, "TurnOffTime");
        Some(EngineClass {
            record,
            turn_off_time: time
                .and_then(|t| t.trim().parse().ok())
                .unwrap_or(TURN_OFF_TIME),
        })
    }
}

/// One update's view of the vehicle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Motion {
    /// The pilot seat has an occupant.
    pub piloted: bool,
    /// Velocity along the vehicle's forward axis, m/s.
    pub forward: f32,
    /// |velocity|, m/s.
    pub speed: f32,
    pub throttle: f32,
    /// Health / max health.
    pub health: f32,
}

impl Motion {
    pub fn of(v: &Vehicle) -> Motion {
        Motion {
            piloted: v.pilot_seat().is_some_and(|s| s.occupant.is_some()),
            forward: (v.rot * Vec3::Z).dot(v.vel),
            speed: v.vel.length(),
            throttle: v.input.throttle,
            health: if v.health.max > 0.0 {
                v.health.cur / v.health.max
            } else {
                1.0
            },
        }
    }
}

/// The PS2's engine instance of one vehicle (B1 `FUN_003659b0`: flags,
/// timer, previous speed) with the hover's own filtered acceleration and
/// the record player's layer starts (B1 `FUN_0045c700`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Instance {
    pub on: bool,
    pub turning_off: bool,
    pub timer: f32,
    previous: f32,
    acceleration: f32,
    /// What the last update sent.
    pub inputs: Inputs,
    /// Seconds since the play request, and each layer's voice playing.
    elapsed: f32,
    pub started: Vec<bool>,
}

impl Instance {
    /// One owner update and the sound update after it. `r` draws U(0, 1)
    /// for a layer's start deviation.
    pub fn update(
        &mut self,
        dt: f32,
        drive: Drive,
        turn_off_time: f32,
        m: &Motion,
        layers: &[EngineLayer],
        mut r: impl FnMut() -> f32,
    ) {
        self.started.resize(layers.len(), false);
        // No pilot: the run-down starts, and ends at `TurnOffTime` (B1
        // `FUN_00366318`). A pilot with the engine off or running down:
        // on, the timer from 0, and one update at rest (`FUN_003660b8`).
        if !m.piloted && self.on {
            if !self.turning_off {
                self.turning_off = true;
                self.timer = 0.0;
            }
            if turn_off_time <= self.timer {
                self.stop();
            }
        } else if m.piloted && (!self.on || self.turning_off) {
            if !self.on {
                self.elapsed = 0.0;
            }
            self.on = true;
            self.turning_off = false;
            self.timer = 0.0;
            self.previous = 0.0;
        }
        let (speed, acceleration) = match drive {
            Drive::Hover { forward, reverse } => {
                let s = if m.throttle > 0.0 { forward } else { reverse };
                let speed = (m.forward / if s == 0.0 { 1.0 } else { s }).abs();
                let a = (speed - self.previous) / dt;
                self.acceleration += (a - self.acceleration) * ACCELERATION_FILTER;
                (speed, self.acceleration)
            }
            // The original's operator-precedence slip, kept by BF2 (§4):
            // moving, it reads a gain graph at its left end.
            Drive::Walker => (m.speed, self.previous - m.speed / dt),
        };
        if !self.on {
            return;
        }
        self.inputs = Inputs {
            speed,
            acceleration,
            health: m.health,
            timer: self.timer,
            turning_off: self.turning_off,
        };
        self.previous = speed;
        self.timer += dt;
        // A layer starts when its delay +- deviation has passed, the
        // deviation drawn again each update.
        for (l, started) in layers.iter().zip(&mut self.started) {
            if !*started && l.delay + 2.0 * l.deviation * (r() - 0.5) <= self.elapsed {
                *started = true;
            }
        }
        self.elapsed += dt;
    }

    /// Off at once: every layer voice stops.
    pub fn stop(&mut self) {
        self.on = false;
        self.turning_off = false;
        self.started.iter_mut().for_each(|s| *s = false);
    }
}

/// One playing layer of one vehicle's engine, this tick.
#[derive(Clone, Debug, PartialEq)]
pub struct Voice {
    /// The vehicle's index in `Sim::vehicles`, or the soldier's in
    /// `Sim::units` for a jet ([`Voice::soldier`]).
    pub vehicle: u32,
    /// A jet soldier's engine (card JETP2), not a vehicle's.
    pub soldier: bool,
    pub layer: u8,
    /// The record's name and the layer's sound property.
    pub record: u32,
    pub sound: u32,
    pub pos: Vec3,
    pub gain_factor: f32,
    pub pitch_factor: f32,
    /// The `speed` the component sent (the referee's column).
    pub speed: f32,
}

/// The engines of a battle's vehicles.
#[derive(Default)]
pub struct EngineSounds {
    /// By vehicle index: the vehicle's generation and its instance.
    instances: HashMap<u32, (u32, Instance)>,
    /// By unit index: the soldier's generation and his jet's instance.
    jets: HashMap<u32, (u32, Instance)>,
    /// The jets' one-shots since the last [`EngineSounds::take_jet_shots`].
    shots: Vec<JetShot>,
    seed: u32,
    /// The battle's sound sub-levels, best first (`crate::foley::sublevels`).
    levels: Vec<u32>,
}

/// (pilot seats occupied, engines on, of them running down) over the
/// vehicles whose class has a record - the `SOUND_LOG` count row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Count {
    pub piloted: usize,
    pub on: usize,
    pub turning_off: usize,
}

impl EngineSounds {
    /// For a battle of `walk`'s teams on the map baked at `map_dir`.
    pub fn new(map_dir: &std::path::Path, walk: &WalkSim) -> EngineSounds {
        EngineSounds {
            levels: crate::foley::battle_levels(map_dir, walk),
            ..EngineSounds::default()
        }
    }

    /// Step every vehicle's engine by one sim tick; `record` looks an
    /// `EngineSound` up in the battle's sub-levels (`SoundLib::engine`). Fills `out` with the layers
    /// that play now.
    pub fn tick<'a>(
        &mut self,
        walk: &WalkSim,
        record: impl Fn(u32, &[u32]) -> Option<&'a EngineSound>,
        out: &mut Vec<Voice>,
    ) -> Count {
        let mut count = Count::default();
        let mut live = Vec::new();
        for (i, v) in walk.sim.vehicles.iter().enumerate() {
            let class = v.class as usize;
            let Some(ec) = walk.vehicle_engines.get(class).and_then(|e| e.as_ref()) else {
                continue;
            };
            let hash = fnv1a_lowercase(&ec.record);
            let drive = match v.kind {
                VehicleKind::Hover => match walk.sim.hover_params.get(class) {
                    Some(Some(p)) => Drive::Hover {
                        forward: p.forward_speed,
                        reverse: p.reverse_speed,
                    },
                    _ => continue,
                },
                VehicleKind::Walker => Drive::Walker,
                _ => continue,
            };
            let (Some(rec), true) = (record(hash, &self.levels), v.alive) else {
                continue;
            };
            let i = i as u32;
            live.push(i);
            let e = self.instances.entry(i).or_default();
            if e.0 != v.gen {
                *e = (v.gen, Instance::default());
            }
            let m = Motion::of(v);
            let seed = &mut self.seed;
            e.1.update(
                core::TICK_DT,
                drive,
                ec.turn_off_time,
                &m,
                &rec.layers,
                || {
                    *seed = seed.wrapping_mul(0x0019_660D).wrapping_add(0x3C6E_F35F);
                    (*seed >> 8) as f32 / 16_777_216.0
                },
            );
            count.piloted += usize::from(m.piloted);
            count.on += usize::from(e.1.on);
            count.turning_off += usize::from(e.1.on && e.1.turning_off);
            if !e.1.on {
                continue;
            }
            for (k, l) in rec
                .layers
                .iter()
                .enumerate()
                .filter(|(k, _)| e.1.started[*k])
            {
                let (gain_factor, pitch_factor) = factors(l, &e.1.inputs);
                out.push(Voice {
                    vehicle: i,
                    soldier: false,
                    layer: k as u8,
                    record: hash,
                    sound: l.sound,
                    pos: v.pos,
                    gain_factor,
                    pitch_factor,
                    speed: e.1.inputs.speed,
                });
            }
        }
        // A destroyed vehicle's engine stops at once.
        self.instances.retain(|i, _| live.contains(i));
        let soldiers = walk.sim.units.iter().enumerate().map(|(k, u)| JetMotion {
            unit: k as u32,
            gen: u.gen,
            jetting: u.alive && u.soldier.stance.jetting(),
            pos: u.soldier.pos,
            vel: u.soldier.vel,
            health: if u.health.max > 0.0 {
                u.health.cur / u.health.max
            } else {
                1.0
            },
            look: &walk.teams[usize::from(u.team).min(1)]
                .class(u.spec.class)
                .jet_look,
        });
        self.tick_jets(soldiers, &record, out);
        count
    }

    /// Step the jet soldiers' engines by one sim tick (card JETP2): the
    /// PS2's soldier calls of the engine component (BF1 `FUN_00209540`
    /// B1:118809-118823 on at the jet's start; `FUN_00205cc8`
    /// B1:117352-117376 the update, B1:117778-117783 the turn-off outside
    /// stance 7; BF2 `FUN_002476a0` B2:162611-162623, `FUN_00240bf8`
    /// B2:160196, `FUN_00239ad8` -> `FUN_0044ce98`). The loop's layers go
    /// to `out`; the turn-on / turning-off / turn-off one-shots to
    /// [`EngineSounds::take_jet_shots`].
    pub fn tick_jets<'a, 'b>(
        &mut self,
        soldiers: impl Iterator<Item = JetMotion<'b>>,
        record: &impl Fn(u32, &[u32]) -> Option<&'a EngineSound>,
        out: &mut Vec<Voice>,
    ) {
        let mut live = Vec::new();
        for s in soldiers {
            if !s.look.sounds() {
                continue;
            }
            let hash = s.look.engine.as_deref().map(fnv1a_lowercase);
            let rec = hash.and_then(|h| record(h, &self.levels));
            let layers = rec.map_or(&[][..], |r| &r.layers[..]);
            let e = self.jets.entry(s.unit).or_default();
            if e.0 != s.gen {
                *e = (s.gen, Instance::default());
            }
            live.push(s.unit);
            let (was_on, was_off) = (e.1.on, e.1.turning_off);
            let m = Motion {
                piloted: s.jetting,
                speed: s.vel.length(),
                health: s.health,
                ..Motion::default()
            };
            let seed = &mut self.seed;
            e.1.update(
                core::TICK_DT,
                Drive::Walker,
                s.look.turn_off_time,
                &m,
                layers,
                || {
                    *seed = seed.wrapping_mul(0x0019_660D).wrapping_add(0x3C6E_F35F);
                    (*seed >> 8) as f32 / 16_777_216.0
                },
            );
            let mut shot = |kind: JetShotKind, name: &Option<String>| {
                if let Some(n) = name {
                    self.shots.push(JetShot {
                        unit: s.unit,
                        kind,
                        name: n.clone(),
                        pos: s.pos,
                    });
                }
            };
            // On from off, or back on while running down (re-vehicle-sounds
            // §3.3): `TurnOnSound`.
            if e.1.on && !e.1.turning_off && (!was_on || was_off) {
                shot(JetShotKind::TurnOn, &s.look.turn_on);
            }
            // The run-down's start (§3.5): `TurningOffSound` (no jet class
            // authors one), then at `TurnOffTime` - 0.0 for every jet class,
            // so in the same update - `TurnOffSound` and the loop stops.
            if was_on && !was_off && (e.1.turning_off || !e.1.on) {
                shot(JetShotKind::TurningOff, &s.look.turning_off);
            }
            if was_on && !e.1.on {
                shot(JetShotKind::TurnOff, &s.look.turn_off);
            }
            let (Some(rec), Some(hash), true) = (rec, hash, e.1.on) else {
                continue;
            };
            for (k, l) in rec
                .layers
                .iter()
                .enumerate()
                .filter(|(k, _)| e.1.started[*k])
            {
                let (gain_factor, pitch_factor) = factors(l, &e.1.inputs);
                out.push(Voice {
                    vehicle: s.unit,
                    soldier: true,
                    layer: k as u8,
                    record: hash,
                    sound: l.sound,
                    pos: s.pos,
                    gain_factor,
                    pitch_factor,
                    speed: e.1.inputs.speed,
                });
            }
        }
        self.jets.retain(|i, _| live.contains(i));
    }

    /// The jets' one-shots since the last call, in order.
    pub fn take_jet_shots(&mut self) -> Vec<JetShot> {
        std::mem::take(&mut self.shots)
    }

    /// A soldier's jet engine, if it has one running (tests, the referee).
    pub fn jet(&self, unit: u32) -> Option<&Instance> {
        self.jets.get(&unit).map(|e| &e.1)
    }
}

/// One soldier's view for [`EngineSounds::tick_jets`].
#[derive(Clone, Copy, Debug)]
pub struct JetMotion<'a> {
    /// The unit's index in `Sim::units`, and its generation.
    pub unit: u32,
    pub gen: u32,
    /// Alive and in a jet stance (`Stance::jetting`).
    pub jetting: bool,
    pub pos: Vec3,
    pub vel: Vec3,
    /// Health / max health.
    pub health: f32,
    pub look: &'a crate::roster::JetLook,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JetShotKind {
    TurnOn,
    TurningOff,
    TurnOff,
}

/// A jet's one-shot: the sound property's name, at the soldier.
#[derive(Clone, Debug, PartialEq)]
pub struct JetShot {
    pub unit: u32,
    pub kind: JetShotKind,
    pub name: String,
    pub pos: Vec3,
}

#[cfg(test)]
mod tests;
