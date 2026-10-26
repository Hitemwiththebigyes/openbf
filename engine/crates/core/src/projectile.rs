//! Blaster bolts: simulated projectiles with ONE predictive raycast at spawn
//! (BF1 FUN_002762a0 spawn / FUN_002764c0 move+collide / FUN_00272e98
//! lifetime; BF2 FUN_00313600). Authentic consequences kept deliberately
//! (re-blaster-damage §5.1): the bolt hits the object the spawn ray found
//! at the precomputed time as long as that object is still alive — targets
//! cannot dodge after the shot, and anything entering the path later is
//! never hit. Flagged for a PCSX2 confirmation; a per-tick sweep would be
//! an openbf divergence. Complete bit-exact bolt replay serialization is
//! owned here by `Bolt::hash_into` (card HASH).

use crate::collision::{ray_heightfield, Capsule, CollisionWorld, HullFilter, Layers};
use crate::heightfield::Heightfield;
use crate::unit::UnitHandle;
use glam::Vec3;

/// Terrain march step for the spawn ray (meters).
pub const TERRAIN_RAY_STEP: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    None,
    /// The heightfield sheet (`collision::ray_heightfield`).
    Terrain,
    /// A `coll` mesh or `prim` primitive of the baked world
    /// (`collision::CollisionWorld::ray`) — a wall, a crate, a building.
    /// Like [`Hit::Terrain`] it carries no payload: the impact point is
    /// [`Bolt::end`], and nothing consumes the surface normal yet.
    World,
    /// Something that takes damage — see [`Target`].
    Target(Target),
}

/// What a bolt can damage: a soldier, or a vehicle's ONE hull pool (no
/// region — the PS2 has no damage regions, analysis/re-vehicles.md §6).
/// `Sim::resolve_bolt` deals a vehicle hit to the pool (VDMG); the spawn
/// ray picks a hull when its ordnance-subset bodies are the nearest hit
/// (card VHULL).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Unit(UnitHandle),
    Vehicle(crate::vehicle::VehicleHandle),
    Station(u16),
}

#[derive(Clone, Debug)]
pub struct Bolt {
    pub pos: Vec3,
    pub vel: Vec3,
    /// Seconds of flight left (+0x3c): LifeSpan, scaled by the hit fraction.
    pub remaining: f32,
    /// Seconds since spawn (+0x40) — BF2 damage falloff runs on this.
    pub age: f32,
    pub owner: UnitHandle,
    pub team: u8,
    pub weapon_class: usize,
    pub hit: Hit,
    /// Where the spawn ray said the bolt ends (impact or expiry point).
    pub end: Vec3,
    /// Was the shooter a human? Carried so `resolve_bolt` can tell a bot's
    /// bolt into a human from a human's own (D12's damage multiplier); the
    /// owner may be dead and recycled by the time the bolt lands.
    pub shooter_is_player: bool,
}

/// Who fired (carried by the bolt for damage attribution + friendly fire).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shooter {
    pub owner: UnitHandle,
    pub team: u8,
    pub weapon_class: usize,
    /// The firing unit's `is_player` at the press (D12).
    pub is_player: bool,
}

impl Bolt {
    /// Bit-exact replay state (card HASH). Hit variants have distinct tags;
    /// historical handles retain their generations after slot reuse.
    pub(crate) fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        for v in [
            self.pos.x,
            self.pos.y,
            self.pos.z,
            self.vel.x,
            self.vel.y,
            self.vel.z,
            self.remaining,
            self.age,
            self.end.x,
            self.end.y,
            self.end.z,
        ] {
            eat(&v.to_bits().to_le_bytes());
        }
        eat(&self.owner.index.to_le_bytes());
        eat(&self.owner.gen.to_le_bytes());
        eat(&[self.team]);
        eat(&(self.weapon_class as u64).to_le_bytes());
        eat(&[u8::from(self.shooter_is_player)]);
        match self.hit {
            Hit::None => eat(&[0]),
            Hit::Terrain => eat(&[1]),
            Hit::World => eat(&[2]),
            Hit::Target(Target::Unit(h)) => {
                eat(&[3]);
                eat(&h.index.to_le_bytes());
                eat(&h.gen.to_le_bytes());
            }
            Hit::Target(Target::Vehicle(h)) => {
                eat(&[4]);
                eat(&h.index.to_le_bytes());
                eat(&h.gen.to_le_bytes());
            }
            Hit::Target(Target::Station(s)) => {
                eat(&[5]);
                eat(&s.to_le_bytes());
            }
        }
    }

    /// Spawn with the predictive raycast: candidates = terrain + the given
    /// unit capsules (owner excluded by the caller). `speed` = ordnance
    /// Velocity, `life_span` = ordnance LifeSpan.
    ///
    /// Three candidates, nearest wins: the terrain CROSSING, the `world`
    /// models — the map's and the vehicles' hulls that `hulls` admits (a
    /// rider's bolt skips its own vehicle); a hull hit is
    /// [`Target::Vehicle`] — and the unit capsules.
    ///
    /// `world` carries both the map's bodies and its terrain cutters. The
    /// terrain ray is a crossing test that ignores a crossing inside a cutter
    /// (`core::collision`), so a bolt fired inside a cave or a cut building is
    /// not killed at t = 0 by the heightfield overhead; the model ray
    /// (`CollisionWorld::ray`) is what stops it at the cave's actual wall.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        origin: Vec3,
        dir: Vec3,
        speed: f32,
        life_span: f32,
        shooter: Shooter,
        hf: Option<&Heightfield>,
        world: &CollisionWorld,
        hulls: HullFilter,
        targets: impl Iterator<Item = (UnitHandle, Capsule)>,
    ) -> Bolt {
        let dir = dir.normalize();
        let total = speed * life_span;
        let mut best_t = total;
        let mut hit = Hit::None;
        if let Some(hf) = hf {
            if let Some(t) =
                ray_heightfield(hf, &world.cutters, origin, dir, total, TERRAIN_RAY_STEP)
            {
                if t < best_t {
                    best_t = t;
                    hit = Hit::Terrain;
                }
            }
        }
        // The PS2's bolt ray passes the trailing flag, so it tests only the
        // target model's ORDNANCE subset (analysis/re-collision-layers.md §4,
        // `FUN_002764C0`).
        if let Some(h) = world.ray_with(origin, dir, best_t, Layers::ORDNANCE, hulls) {
            if h.t < best_t {
                best_t = h.t;
                hit = match h.owner {
                    Some(v) => Hit::Target(Target::Vehicle(v)),
                    None => h
                        .instance
                        .and_then(|i| world.station(i))
                        .map_or(Hit::World, |s| Hit::Target(Target::Station(s))),
                };
            }
        }
        for (h, cap) in targets {
            if let Some(t) = cap.ray_hit(origin, dir, best_t) {
                if t < best_t {
                    best_t = t;
                    hit = Hit::Target(Target::Unit(h));
                }
            }
        }
        let remaining = if hit == Hit::None {
            life_span
        } else {
            life_span * (best_t / total)
        };
        Bolt {
            pos: origin,
            vel: dir * speed,
            remaining,
            age: 0.0,
            owner: shooter.owner,
            team: shooter.team,
            weapon_class: shooter.weapon_class,
            hit,
            end: origin + dir * best_t,
            shooter_is_player: shooter.is_player,
        }
    }

    /// One tick: `pos += vel × min(dt, remaining)`; returns true when the
    /// flight is over (impact or expiry) this tick.
    pub fn advance(&mut self, dt: f32) -> bool {
        let step = dt.min(self.remaining.max(0.0));
        self.pos += self.vel * step;
        self.age += dt;
        self.remaining -= dt;
        self.remaining <= 0.0
    }
}
