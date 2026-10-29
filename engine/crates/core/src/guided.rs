use crate::sim::PadState;
use crate::unit::UnitHandle;
use crate::vehicle::hover::{quat_x, quat_y, quat_z};
use crate::weapon::{pf, pf_hash, ExplosionClass, HealthType, PropSource};
use glam::{Quat, Vec3};

pub const COLLISION_RADIUS_PROVISIONAL: f32 = 0.2;

#[derive(Clone, Debug, PartialEq)]
pub struct GuidedClass {
    pub acceleration: f32,
    pub speeds: [f32; 3],
    pub boost_speed: f32,
    pub boost_acceleration: f32,
    pub pitch_rate: f32,
    pub turn_rate: f32,
    pub pitch_filter: f32,
    pub turn_filter: f32,
    pub pitch_damp: f32,
    pub turn_damp: f32,
    pub bank_angle: f32,
    pub bank_filter: f32,
    pub energy: f32,
    pub energy_restore: f32,
    pub boost_drain: f32,
    pub health: f32,
    pub health_type: HealthType,
    pub explosion: Option<ExplosionClass>,
    pub noncritical: Option<ExplosionClass>,
    pub critical_speed: f32,
    pub track_center: Vec3,
    pub track_offset: Vec3,
    pub engine_sound: Option<String>,
    pub death_sound: Option<String>,
    pub trail: Option<String>,
}

fn vector(src: &dyn PropSource, hash: u32, default: Vec3) -> Vec3 {
    let Some(value) = src.prop_hash(hash) else {
        return default;
    };
    let mut values = value
        .split_whitespace()
        .filter_map(|v| v.parse::<f32>().ok());
    match (values.next(), values.next(), values.next()) {
        (Some(x), Some(y), Some(z)) => Vec3::new(x, y, z),
        _ => default,
    }
}

impl GuidedClass {
    pub fn from_props(src: &dyn PropSource) -> Self {
        let f = |key, default| pf(src, key).unwrap_or(default);
        Self {
            acceleration: f("Acceleraton", 0.0),
            speeds: [f("MinSpeed", 0.0), f("MidSpeed", 0.0), f("MaxSpeed", 0.0)],
            boost_speed: f("BoostSpeed", 0.0),
            boost_acceleration: f("BoostAcceleration", 0.0),
            pitch_rate: f("PitchRate", 0.0),
            turn_rate: f("TurnRate", 0.0),
            pitch_filter: f("PitchFilter", 0.0),
            turn_filter: f("TurnFilter", 0.0),
            pitch_damp: pf_hash(src, 0x53d5_cdc8).unwrap_or(0.0),
            turn_damp: pf_hash(src, 0x1795_170d).unwrap_or(0.0),
            bank_angle: f("BankAngle", 0.0),
            bank_filter: f("BankFilter", 0.0),
            energy: f("EnergyBar", 100.0),
            energy_restore: f("EnergyAutoRestore", 0.0),
            boost_drain: f("EnergyBoostDrain", 0.0),
            health: f("MaxHealth", 1.0),
            health_type: src
                .prop("HealthType")
                .and_then(HealthType::parse)
                .unwrap_or(HealthType::Droid),
            explosion: None,
            noncritical: None,
            critical_speed: pf_hash(src, 0xbb30_82af).unwrap_or(0.8),
            track_center: vector(src, 0xe85d_5895, Vec3::ZERO),
            track_offset: vector(src, 0xfd3d_9507, Vec3::ZERO),
            engine_sound: src.prop("EngineSound").map(str::to_owned),
            death_sound: src.prop("DeathSound").map(str::to_owned),
            trail: src.prop_hash(0x0894_d756).map(str::to_owned),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GuidedWeapon {
    pub class: GuidedClass,
    pub spawn_offset: Vec3,
    pub requires_award: bool,
}

impl GuidedWeapon {
    pub fn new(src: &dyn PropSource, class: GuidedClass) -> Self {
        Self {
            class,
            spawn_offset: vector(src, 0x389c_8e46, Vec3::ZERO),
            requires_award: pf(src, "MedalsTypeToUnlock").is_some_and(|v| v != -1.0),
        }
    }

    pub fn explosion(&self, end: GuidedEnd) -> Option<&ExplosionClass> {
        if matches!(
            end,
            GuidedEnd::Impact | GuidedEnd::Destroyed | GuidedEnd::CriticalExpiry
        ) {
            self.class.explosion.as_ref()
        } else {
            self.class.noncritical.as_ref()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuidedEnd {
    Impact,
    Destroyed,
    Expired,
    CriticalExpiry,
    Returned,
    OwnerGone,
}

#[derive(Clone, Debug)]
pub struct GuidedRocket {
    pub id: u64,
    pub owner: UnitHandle,
    pub team: u8,
    pub weapon_class: usize,
    pub pos: Vec3,
    pub vel: Vec3,
    pub rot: Quat,
    pub pitch_rate: f32,
    pub turn_rate: f32,
    pub bank: f32,
    pub health: f32,
    pub energy: f32,
    pub age: f32,
    pub boosted: bool,
    pub prev_buttons: u32,
    pub pending_end: Option<GuidedEnd>,
}

impl GuidedRocket {
    pub fn forward(&self) -> Vec3 {
        self.rot * -Vec3::Z
    }

    pub fn yaw_pitch(&self) -> (f32, f32) {
        let f = self.forward();
        (
            crate::fmath::atan2(-f.x, -f.z),
            crate::fmath::asin(f.y.clamp(-1.0, 1.0)),
        )
    }

    pub fn steer(&mut self, c: &GuidedClass, pad: &PadState, dt: f32) {
        let look = pad.look();
        let turn = (PadState::axis(pad.rx) + look.0 / crate::sim::MOUSE_STEER).clamp(-1.0, 1.0);
        let pitch = (PadState::axis(pad.ry) + look.1 / crate::sim::MOUSE_STEER).clamp(-1.0, 1.0);
        let slew = |value: f32, target: f32, accel: f32, damp: f32| {
            let rate = if value * target > 0.0 && target.abs() > value.abs() {
                accel
            } else {
                damp
            };
            value + (target - value).clamp(-rate * dt, rate * dt)
        };
        self.pitch_rate = slew(
            self.pitch_rate,
            c.pitch_rate * pitch,
            c.pitch_filter,
            c.pitch_damp,
        );
        self.turn_rate = slew(
            self.turn_rate,
            -c.turn_rate * turn,
            c.turn_filter,
            c.turn_damp,
        );
        self.rot =
            (self.rot * quat_y(self.turn_rate * dt) * quat_x(self.pitch_rate * dt)).normalize();
        self.bank +=
            (-c.bank_angle * turn - self.bank).clamp(-c.bank_filter * dt, c.bank_filter * dt);
    }

    pub fn drawn_rotation(&self) -> Quat {
        self.rot * quat_z(self.bank)
    }

    pub fn fly(&mut self, c: &GuidedClass, pad: &PadState, dt: f32) {
        self.steer(c, pad, dt);
        self.boosted = pad.pressed(crate::sim::buttons::SPRINT) && self.energy > 0.0;
        let throttle = PadState::axis(pad.ly);
        let [min, mid, max] = c.speeds;
        let target = if self.boosted {
            c.boost_speed
        } else if throttle >= 0.0 {
            mid + throttle * (max - mid)
        } else {
            mid + throttle * (mid - min)
        };
        let acceleration = if self.boosted {
            c.boost_acceleration
        } else {
            c.acceleration
        };
        let fwd = self.forward();
        let speed = self.vel.dot(fwd);
        let next = speed + (target - speed).clamp(-3.0 * acceleration * dt, acceleration * dt);
        self.vel = fwd * next;
        self.pos += self.vel * dt;
        self.energy = (self.energy
            + (c.energy_restore - if self.boosted { c.boost_drain } else { 0.0 }) * dt)
            .clamp(0.0, c.energy);
        self.age += dt;
    }

    pub(crate) fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(&self.id.to_le_bytes());
        eat(&self.owner.index.to_le_bytes());
        eat(&self.owner.gen.to_le_bytes());
        eat(&[
            self.team,
            self.boosted as u8,
            self.pending_end.map_or(255, |e| e as u8),
        ]);
        eat(&(self.weapon_class as u64).to_le_bytes());
        for value in self
            .pos
            .to_array()
            .into_iter()
            .chain(self.vel.to_array())
            .chain(self.rot.to_array())
            .chain([
                self.pitch_rate,
                self.turn_rate,
                self.bank,
                self.health,
                self.energy,
                self.age,
            ])
        {
            eat(&value.to_bits().to_le_bytes());
        }
        eat(&self.prev_buttons.to_le_bytes());
    }
}
