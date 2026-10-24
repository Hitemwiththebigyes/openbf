//! Effects: the PS2's particle effects (card FXP2, `analysis/re-fx.md`
//! §5–§6, [O]) and the PROVISIONAL placeholder quads still standing in for
//! what nothing spawns yet.
//!
//! **Particle effects.** [`Library`] holds the baked `effects.ron` layers
//! (card FXP1) — the map's, its two sides', the common one — and looks an
//! effect up by `fnv1a_lowercase(name)` in that order, as the PS2's one
//! effect table resolves a name (re-fx §1); textures resolve the same way.
//! [`Fx::spawn`] starts one at a frame; each [`LiveEffect`] runs the
//! emitters (`emitter.rs`, §5) and [`Fx::submit`] hands the renderer this
//! frame's particles (`draw.rs`, §6), which it sorts far to near among the
//! laser bolts (`render::particles`).
//!
//! **What spawns them** (card FXP3, re-fx §7 [O]). A struck bolt
//! (`Event::Impact`) spawns its ordnance's `ImpactEffect*` for the surface
//! ([`core::impact::ImpactLook::effect`]) at the hit with world axes — riding
//! the hull when it struck a vehicle — then its `ExplosionName` class's
//! effect ([`Fx::explode`]); a destroyed vehicle (`VehicleDestroyed`)
//! detonates its class's explosion effect. A name the running game's
//! effect table does not hold spawns nothing (§7: BF2's inherited BF1
//! names). The PS2 draws an explosion's effect only within 60 m of the
//! camera (B1 FUN_002701a0) — a PS2 budget the port drops (FEEL_NOTES).
//! Still to come: water (`ImpactEffectWater`, `WaterEffect` — the port's
//! bolts do not stop at water), `TrailEffect`, and the vehicles' own
//! Damage / Chunk / Thrust / Wake / Stomp effects.
//!
//! The muzzle flash is not a particle effect (§7): it is a model the held
//! weapon draws (card FXMF, `crate::soldier_vis`), handed to
//! [`Fx::submit_with`] to share the particles' far-to-near list.
//!
//! **Placeholders** (PROVISIONAL, chosen to read clearly, nothing claims to
//! match the PS2): with no effect data baked at all, a struck bolt, a unit
//! hit and a destroyed vehicle draw coloured camera-facing quads.
//!
//! Visual only — driven by sim events, never feeding back into the sim.

pub mod draw;
pub mod emitter;

use crate::walk::WalkSim;
use core::impact::{ExplosionLook, Surface};
use core::sim::Sim;
use core::vehicle::VehicleHandle;
use emitter::{Era, Frame, Instance, Lcg, Variant};
use glam::{Mat4, Vec3};
use manifest::{FxEmitter, FxSet};
use render::{Camera, DrawCmd, MeshHandle, ParticleDraw, Renderer, TextureHandle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The rifle impact every showcase bolt spawns (re-fx §7).
pub const BF1_IMPACT: &str = "ord_explosion";
pub const BF2_IMPACT: &str = "com_sfx_ord_exp";

// How far an ordnance's explosion sits off the hit (the sim's blast sits
// there too).
use core::weapon::EXPLOSION_OFFSET;

/// PROVISIONAL sizes (m) and lifetimes (s).
const IMPACT_SIZE: f32 = 0.45;
const IMPACT_LIFE: f32 = 0.22;
const HIT_SIZE: f32 = 0.34;
const HIT_LIFE: f32 = 0.18;
/// A destroyed vehicle's blast (card VDTH): a slow fireball, until card FXC
/// reads the class's own `Effect`. Its size grows with the vehicle's blast
/// sphere (`Sim::blast_setup`), floored at [`BLAST_MIN_SIZE`].
const BLAST_LIFE: f32 = 0.9;
const BLAST_MIN_SIZE: f32 = 3.0;
/// How far up a hit unit's capsule the spark sits.
const HIT_HEIGHT: f32 = 1.1;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// Bolt struck the terrain or a world model (`BoltExpired { struck }`).
    Impact,
    /// Bolt hit a unit.
    Hit,
    /// A vehicle was destroyed (`VehicleDestroyed`); the payload is the
    /// fireball's full size (m).
    Blast(f32),
}

#[derive(Debug, Clone, Copy)]
struct Puff {
    pos: Vec3,
    kind: Kind,
    team: u8,
    age: f32,
}

impl Puff {
    fn life(&self) -> f32 {
        match self.kind {
            Kind::Impact => IMPACT_LIFE,
            Kind::Hit => HIT_LIFE,
            Kind::Blast(_) => BLAST_LIFE,
        }
    }

    /// (size, colour) at this age. The colour's ALPHA is the fade: these draw
    /// through the renderer's blend pass, so an effect can thin out to nothing
    /// instead of shrinking and dimming (which is what it had to do when the
    /// renderer only alpha-tested). Colours stay at full brightness for most
    /// of the life and the alpha does the work — a flash that dims and stays
    /// opaque reads as a grey card in front of the scene.
    fn look(&self) -> (f32, [f32; 4]) {
        let t = (self.age / self.life()).clamp(0.0, 1.0);
        let bolt = if self.team == 0 {
            [1.0, 0.45, 0.15]
        } else {
            [0.35, 1.0, 0.35]
        };
        match self.kind {
            // Impact: expands and cools toward the bolt colour.
            Kind::Impact => (
                IMPACT_SIZE * (0.6 + 0.9 * t),
                [
                    bolt[0] * 0.6 + 0.4,
                    bolt[1] * 0.6 + 0.3,
                    bolt[2] * 0.6 + 0.25,
                    1.0 - t * t,
                ],
            ),
            // Unit hit: a quick warm spark.
            Kind::Hit => (HIT_SIZE * (0.7 + 0.7 * t), [1.0, 0.5, 0.2, 1.0 - t * t]),
            // Blast: swells fast, cools from white-yellow to deep orange.
            Kind::Blast(size) => (
                size * (0.35 + 0.65 * (1.0 - (1.0 - t) * (1.0 - t))),
                [1.0, 0.85 - 0.5 * t, 0.5 - 0.4 * t, 1.0 - t * t],
            ),
        }
    }
}

/// One baked effect, ready to run: its emitters with every nested
/// sibling template flattened in (a sibling is a second instance of the
/// same effect, re-fx §2.1 +0x214) and, per emitter, BF1's 20 variants.
#[derive(Debug)]
pub struct EffectDef {
    pub hash: u32,
    pub name: Option<String>,
    pub emitters: Vec<FxEmitter>,
    /// Empty for BF2 (every particle its own seed).
    caches: Vec<Vec<Variant>>,
}

fn flatten(e: &FxEmitter, out: &mut Vec<FxEmitter>) {
    let mut own = e.clone();
    let sibling = own.sibling.take();
    out.push(own);
    if let Some(s) = sibling {
        flatten(&s, out);
    }
}

/// The baked effects of one map, in lookup order (see the module doc).
#[derive(Debug)]
pub struct Library {
    pub era: Era,
    effects: HashMap<u32, Arc<EffectDef>>,
    /// The layers' `textures/` dirs, map first.
    texture_dirs: Vec<PathBuf>,
}

impl Library {
    /// `sets` highest layer first: an effect in two layers is the first's.
    pub fn from_sets(era: Era, sets: Vec<FxSet>, rng: &mut Lcg) -> Library {
        let mut effects = HashMap::new();
        for set in sets {
            for e in set.effects {
                effects.entry(e.hash).or_insert_with(|| {
                    let mut emitters = Vec::new();
                    for em in &e.emitters {
                        flatten(em, &mut emitters);
                    }
                    // S+0x128: a per-spawner random set at construction.
                    let caches = match era {
                        Era::Bf1 => emitters
                            .iter()
                            .map(|em| emitter::variant_cache(&em.spawner, rng.step()))
                            .collect(),
                        Era::Bf2 => Vec::new(),
                    };
                    Arc::new(EffectDef {
                        hash: e.hash,
                        name: e.name.clone(),
                        emitters,
                        caches,
                    })
                });
            }
        }
        Library {
            era,
            effects,
            texture_dirs: Vec::new(),
        }
    }

    /// The layers under `assets_dir` for the map baked at `map_dir`, whose
    /// teams are `factions` ("rep", "cis", …; a side with no effects dir is
    /// skipped). A layer that is missing or unreadable is reported and
    /// skipped.
    pub fn load(assets_dir: &Path, map_dir: &Path, game: &str, factions: &[&str]) -> Library {
        let era = if game.eq_ignore_ascii_case("bf2") {
            Era::Bf2
        } else {
            Era::Bf1
        };
        let game = game.to_ascii_lowercase();
        let mut dirs = vec![map_dir.to_path_buf()];
        for f in factions {
            dirs.push(
                assets_dir
                    .join(format!("side_{game}"))
                    .join("effects")
                    .join(f.to_ascii_lowercase()),
            );
        }
        dirs.push(assets_dir.join(format!("common_{game}")));
        let mut sets = Vec::new();
        for d in &dirs {
            let path = d.join("effects.ron");
            if !pak::exists(&path) {
                continue;
            }
            match pak::read_to_string(&path)
                .map_err(anyhow::Error::from)
                .and_then(|t| ron::from_str::<FxSet>(&t).map_err(anyhow::Error::from))
            {
                Ok(set) => sets.push(set),
                Err(e) => eprintln!("effects {}: {e:#}", path.display()),
            }
        }
        let mut lib = Library::from_sets(era, sets, &mut Lcg(0x5eed_f00d));
        lib.texture_dirs = dirs.iter().map(|d| d.join("textures")).collect();
        lib
    }

    pub fn get(&self, name: &str) -> Option<&Arc<EffectDef>> {
        self.effects.get(&crate::classdb::fnv1a_lowercase(name))
    }

    pub fn len(&self) -> usize {
        self.effects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }
}

/// One spawned effect: its clock, its frame and one instance per emitter.
#[derive(Debug)]
pub struct LiveEffect {
    def: Arc<EffectDef>,
    /// Seconds since the spawn (the PS2's E+0xfc).
    pub t: f32,
    pub frame: Frame,
    instances: Vec<Instance>,
    stopped: bool,
    /// The vehicle it rides and its offset in the hull's frame: the PS2
    /// spawns an impact in the struck object's frame (B1 FUN_00276810
    /// B1:198040–). PROVISIONAL: only the origin follows (world axes kept).
    ride: Option<(VehicleHandle, Vec3)>,
}

impl LiveEffect {
    pub fn new(def: Arc<EffectDef>, era: Era, frame: Frame, rng: &mut Lcg) -> LiveEffect {
        let instances = def
            .emitters
            .iter()
            .map(|em| Instance::new(em, era, rng.step(), rng.step()))
            .collect();
        let mut fx = LiveEffect {
            def,
            t: 0.0,
            frame,
            instances,
            stopped: false,
            ride: None,
        };
        fx.advance(0.0);
        fx
    }

    /// Advance the clock `dt` seconds.
    pub fn advance(&mut self, dt: f32) {
        self.t += dt;
        for (inst, em) in self.instances.iter_mut().zip(&self.def.emitters) {
            inst.update(em, self.t, self.frame, self.stopped);
        }
    }

    /// No more bursts; the effect ends once its particles have.
    pub fn stop(&mut self) {
        self.stopped = true;
    }

    pub fn is_dead(&self) -> bool {
        self.instances.iter().all(Instance::is_dead)
    }

    /// Particles emitted so far, per emitter.
    pub fn emitted(&self) -> Vec<u32> {
        self.instances.iter().map(Instance::emitted).collect()
    }

    /// Every emitter's live particles, unthinned (`keep` all).
    pub fn particles(&self, era: Era) -> Vec<Vec<emitter::Particle>> {
        self.instances
            .iter()
            .enumerate()
            .map(|(i, inst)| {
                let em = &self.def.emitters[i];
                let cache = self.def.caches.get(i).map_or(&[][..], Vec::as_slice);
                inst.particles(em, era, self.t, self.frame, cache, |_| true)
            })
            .collect()
    }

    /// This effect's draws for the view at `eye` looking along `forward`.
    pub fn draws(
        &self,
        era: Era,
        eye: Vec3,
        forward: Vec3,
        texture: &mut dyn FnMut(&str) -> Option<TextureHandle>,
        out: &mut Vec<ParticleDraw>,
    ) {
        let d = (self.frame.pos - eye).dot(forward);
        for (i, inst) in self.instances.iter().enumerate() {
            if inst.is_dead() {
                continue;
            }
            let em = &self.def.emitters[i];
            let terms = draw::view_terms(d, em.bounding_radius);
            if terms.fade <= 0.0 {
                continue;
            }
            let cache = self.def.caches.get(i).map_or(&[][..], Vec::as_slice);
            let ps = inst.particles(em, era, self.t, self.frame, cache, |k| terms.keep(k));
            if ps.is_empty() {
                continue;
            }
            let tex = em
                .geometry
                .texture
                .as_deref()
                .and_then(|n| texture(&n.to_ascii_lowercase()));
            draw::emitter_draws(em, era, &ps, terms, self.frame.pos, eye, forward, tex, out);
        }
    }
}

/// Effect sheet `name` (lowercase) from the first of `dirs` that holds it,
/// uploaded once into `cache`; None (once, with a line) when none does.
fn sheet(
    r: &mut Renderer,
    dirs: &[std::path::PathBuf],
    cache: &mut HashMap<String, Option<TextureHandle>>,
    name: &str,
) -> Option<TextureHandle> {
    if let Some(t) = cache.get(name) {
        return *t;
    }
    let t = dirs.iter().find_map(|d| {
        crate::viewer::upload_texture_file(r, d, name).unwrap_or_else(|e| {
            eprintln!("effect texture {name}: {e:#}");
            None
        })
    });
    if t.is_none() {
        eprintln!("effect texture {name}: in no layer (drawn as nothing)");
    }
    cache.insert(name.to_owned(), t);
    t
}

/// The game's effects: the particle effects running and the placeholder
/// puffs. `ingest` once per sim tick, `update` once per rendered frame.
#[derive(Default)]
pub struct Fx {
    puffs: Vec<Puff>,
    library: Option<Library>,
    live: Vec<LiveEffect>,
    rng: Option<Lcg>,
    textures: HashMap<String, Option<TextureHandle>>,
    /// The `SoundName` of each effect an explosion spawned since the last
    /// [`Fx::take_sounds`], and where (card WSND).
    sounds: Vec<(String, Vec3)>,
}

impl std::fmt::Debug for Fx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fx")
            .field("puffs", &self.puffs.len())
            .field("effects", &self.library.as_ref().map_or(0, Library::len))
            .field("live", &self.live.len())
            .finish()
    }
}

impl Fx {
    /// Effects over `library` (no placeholder puffs yet).
    pub fn with_library(library: Library) -> Fx {
        Fx {
            library: Some(library),
            ..Fx::default()
        }
    }

    /// The map's effect layers (see [`Library::load`]).
    pub fn load(assets_dir: &Path, map_dir: &Path, game: &str, factions: &[&str]) -> Fx {
        Fx::with_library(Library::load(assets_dir, map_dir, game, factions))
    }

    /// The effect layers of the map at `map_dir` for `walk`'s two sides.
    pub fn for_walk(assets_dir: &Path, map_dir: &Path, game: &str, walk: &WalkSim) -> Fx {
        let factions = [
            walk.teams[0].faction.as_str(),
            walk.teams[1].faction.as_str(),
        ];
        Fx::load(assets_dir, map_dir, game, &factions)
    }

    pub fn library(&self) -> Option<&Library> {
        self.library.as_ref()
    }

    /// Start the effect `name` at `frame`; false when no layer holds it.
    pub fn spawn(&mut self, name: &str, frame: Frame) -> bool {
        let Some(lib) = &self.library else {
            return false;
        };
        let Some(def) = lib.get(name) else {
            return false;
        };
        let rng = self.rng.get_or_insert(Lcg(0x0bf2_5eed));
        self.live
            .push(LiveEffect::new(def.clone(), lib.era, frame, rng));
        true
    }

    /// [`Fx::spawn`] riding vehicle `h` of `sim` (its origin follows the hull).
    fn spawn_riding(&mut self, name: &str, pos: Vec3, h: VehicleHandle, sim: &Sim) -> bool {
        if !self.spawn(name, Frame::at(pos)) {
            return false;
        }
        if let Some(v) = sim.vehicle(h) {
            let local = v.rot.inverse() * (pos - v.pos);
            if let Some(e) = self.live.last_mut() {
                e.ride = Some((h, local));
            }
        }
        true
    }

    /// Detonate an explosion class's look at `pos` (B1 FUN_002701a0
    /// B1:193756–193807): its `Effect` — BF2 one of its rows at random (B2
    /// FUN_0030c8f8). Water (`WaterEffect`) waits for the port's water line.
    /// False when nothing spawned.
    pub fn explode(&mut self, look: &ExplosionLook, pos: Vec3) -> bool {
        let pick = match look.effects.len() {
            0 => return false,
            1 => 0,
            n => {
                let rng = self.rng.get_or_insert(Lcg(0x0bf2_5eed));
                ((rng.float() * n as f32) as usize).min(n - 1)
            }
        };
        let name = &look.effects[pick];
        if !self.spawn(name, Frame::at(pos)) {
            return false;
        }
        // The effect's own sound (`"<name> defer"`, re-weapon-sounds.md §3.1,
        // §4.2). The PS2's explosion function plays it for a blast out of
        // every camera's range; PROVISIONAL [H]: inside the range the
        // effect is taken to play it itself, so it starts either way.
        let sound = self.live.last().and_then(|e| {
            let mut next = e.def.emitters.iter().collect::<Vec<_>>();
            while let Some(em) = next.pop() {
                // Most emitters carry an empty name.
                let name = em.sound_name.as_deref();
                if let Some(s) = name.and_then(|s| s.split_whitespace().next()) {
                    return Some(s.to_owned());
                }
                next.extend(em.sibling.as_deref());
                next.extend(em.geometry.child.as_deref());
            }
            None
        });
        if let Some(s) = sound {
            self.sounds.push((s, pos));
        }
        true
    }

    /// The sounds the explosions' effects started since the last call, for
    /// the tick's sound pass (`crate::event_sounds`).
    pub fn take_sounds(&mut self) -> Vec<(String, Vec3)> {
        std::mem::take(&mut self.sounds)
    }

    /// Is any effect data baked? Without it the placeholder quads stand in.
    fn has_effects(&self) -> bool {
        self.library.as_ref().is_some_and(|l| !l.is_empty())
    }

    /// The running particle effects.
    pub fn live(&self) -> &[LiveEffect] {
        &self.live
    }

    /// Upload every sheet the library's effects can draw (card HITCH): at the
    /// battle's load, so no effect's first spawn reads a file and makes a
    /// texture inside a frame. Returns how many were wanted.
    pub fn preload(&mut self, r: &mut Renderer) -> usize {
        let Some(lib) = &self.library else {
            return 0;
        };
        let mut names: Vec<String> = Vec::new();
        for def in lib.effects.values() {
            let mut next: Vec<&FxEmitter> = def.emitters.iter().collect();
            while let Some(em) = next.pop() {
                if let Some(n) = em.geometry.texture.as_deref() {
                    let n = n.to_ascii_lowercase();
                    if !names.contains(&n) {
                        names.push(n);
                    }
                }
                next.extend(em.sibling.as_deref());
                next.extend(em.geometry.child.as_deref());
            }
        }
        names.sort();
        for n in &names {
            sheet(r, &lib.texture_dirs, &mut self.textures, n);
        }
        names.len()
    }

    /// The rifle impact of this library's game (see the module doc).
    fn impact_name(&self) -> &'static str {
        match self.library.as_ref().map(|l| l.era) {
            Some(Era::Bf2) => BF2_IMPACT,
            _ => BF1_IMPACT,
        }
    }

    /// Hand the renderer this frame's particles for the view `camera`,
    /// uploading any sheet not yet loaded.
    pub fn submit(&mut self, r: &mut Renderer, camera: &Camera) {
        self.submit_with(r, camera, Vec::new());
    }

    /// [`Fx::submit`] with `extra` draws sorted into the same list — the
    /// muzzle flashes (`SoldierVisuals::flash_draws`).
    pub fn submit_with(&mut self, r: &mut Renderer, camera: &Camera, extra: Vec<ParticleDraw>) {
        let mut out = extra;
        if let Some(lib) = &self.library {
            let cache = &mut self.textures;
            let dirs = &lib.texture_dirs;
            let mut texture = |name: &str| sheet(r, dirs, cache, name);
            for e in &self.live {
                e.draws(
                    lib.era,
                    camera.position,
                    camera.forward(),
                    &mut texture,
                    &mut out,
                );
            }
        }
        r.set_particles(out);
    }

    /// Read this tick's sim events into effects.
    pub fn ingest(&mut self, walk: &WalkSim) {
        self.ingest_sim(&walk.sim);
    }

    /// [`Fx::ingest`] over the sim itself.
    pub fn ingest_sim(&mut self, sim: &Sim) {
        use core::sim::Event;
        // Riding effects follow their hull; a hull gone leaves them where
        // they are.
        for e in &mut self.live {
            if let Some((h, local)) = e.ride {
                match sim.vehicle(h) {
                    Some(v) => {
                        e.frame.pos = v.pos + v.rot * local;
                        e.frame.vel = v.vel;
                    }
                    None => e.ride = None,
                }
            }
        }
        let baked = self.has_effects();
        for e in &sim.events {
            match e {
                Event::Impact {
                    pos,
                    dir,
                    weapon_class,
                    surface,
                    vehicle,
                } if baked => {
                    let look = sim
                        .weapon_classes
                        .get(*weapon_class as usize)
                        .and_then(|w| w.ordnance.impact.as_ref());
                    // No look: a fixture weapon built without the class db
                    // — the showcase rifle's impact on every surface.
                    let name = match look {
                        Some(l) => l.effect(*surface),
                        None => Some(self.impact_name()),
                    };
                    if let Some(name) = name {
                        match vehicle {
                            Some(h) if *surface == Surface::Vehicle => {
                                self.spawn_riding(name, *pos, *h, sim);
                            }
                            _ => {
                                self.spawn(name, Frame::at(*pos));
                            }
                        }
                    }
                    if let Some(x) = look.and_then(|l| l.explosion.as_ref()) {
                        self.explode(x, *pos - *dir * EXPLOSION_OFFSET);
                    }
                }
                Event::BoltExpired { pos, struck } if !baked => {
                    // Air expiry gets no puff: nothing was struck.
                    if *struck {
                        self.puffs.push(Puff {
                            // Lifted off the surface or half the quad is
                            // buried in the ground it just hit.
                            pos: *pos + Vec3::Y * 0.12,
                            kind: Kind::Impact,
                            team: 0,
                            age: 0.0,
                        });
                    }
                }
                Event::Hit { victim, .. } if !baked => {
                    if let Some(v) = sim.units.get(victim.index as usize) {
                        self.puffs.push(Puff {
                            pos: v.soldier.pos + Vec3::Y * HIT_HEIGHT,
                            kind: Kind::Hit,
                            team: v.team,
                            age: 0.0,
                        });
                    }
                }
                // A thrown grenade's fuse ran out (GREN): its explosion
                // class's effect where it lay.
                Event::StickyBlast { pos, weapon_class } => {
                    let look = sim
                        .weapon_classes
                        .get(*weapon_class as usize)
                        .and_then(|w| w.ordnance.sticky.as_ref())
                        .and_then(|s| s.explosion.as_ref())
                        .map(|x| &x.look);
                    match look {
                        Some(look) if baked => {
                            self.explode(look, *pos);
                        }
                        _ => self.puffs.push(Puff {
                            pos: *pos + Vec3::Y * 0.5,
                            kind: Kind::Blast(BLAST_MIN_SIZE),
                            team: 0,
                            age: 0.0,
                        }),
                    }
                }
                // An item was taken (PWUP, re-powerups §5.6): the taker's
                // `HealthGainEffect` / `AmmoGainEffect` / BF2's
                // `EnergyGainEffect` — the class defaults, which no disc
                // class overrides. At the taker (PROVISIONAL: the PS2's
                // follows it, one at a time per unit).
                // A station's give runs the same effects (PWST,
                // re-powerup-station §5.1); a vehicle served shows none.
                Event::ItemTaken {
                    unit,
                    health,
                    ammo,
                    energy,
                    ..
                }
                | Event::StationServed {
                    unit: Some(unit),
                    health,
                    ammo,
                    energy,
                    ..
                } => {
                    if let Some(u) = sim.units.get(unit.index as usize) {
                        let f = Frame::at(u.soldier.pos);
                        for (gave, bf1, bf2) in [
                            (*health, "Powerup_blue", "com_sfx_powerup_blue"),
                            (*ammo, "Powerup_red", "com_sfx_powerup_red"),
                            (*energy, "", "com_sfx_powerup_green"),
                        ] {
                            if gave && !self.spawn(bf2, f) {
                                self.spawn(bf1, f);
                            }
                        }
                    }
                }
                Event::VehicleDestroyed { class, pos, .. } => {
                    let setup = sim
                        .blast_setup
                        .get(*class as usize)
                        .and_then(|b| b.as_ref());
                    if baked {
                        // Where the sim detonates it (`Sim::destroy_vehicle`).
                        if let Some(x) = setup.and_then(|b| b.explosion.as_ref()) {
                            self.explode(&x.look, *pos);
                        }
                        continue;
                    }
                    let w = setup.map_or(0.0, |b| b.sphere_radius);
                    self.puffs.push(Puff {
                        pos: *pos + Vec3::Y * (w * 0.5),
                        kind: Kind::Blast((w * 1.5).max(BLAST_MIN_SIZE)),
                        team: 0,
                        age: 0.0,
                    });
                }
                _ => {}
            }
        }
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.puffs {
            p.age += dt;
        }
        self.puffs.retain(|p| p.age < p.life());
        for e in &mut self.live {
            e.advance(dt);
        }
        self.live.retain(|e| !e.is_dead());
    }

    /// Live placeholder puffs plus running particle effects.
    pub fn len(&self) -> usize {
        self.puffs.len() + self.live.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        self.puffs.clear();
        self.live.clear();
        self.sounds.clear();
    }

    /// Camera-facing quads for the live effects. `quad` is the unit quad from
    /// `viewer::upload_billboard_quad` (its normals face the sun, so the
    /// lambert term is constant and an effect does not dim as you turn);
    /// `sprite` is `viewer::upload_fx_sprite`, a soft round falloff. Both are
    /// optional-free: pass what you uploaded.
    pub fn draws(
        &self,
        quad: MeshHandle,
        sprite: Option<TextureHandle>,
        camera: &Camera,
    ) -> Vec<DrawCmd> {
        self.puffs
            .iter()
            .map(|p| {
                let (model, color) = billboard(p, camera);
                DrawCmd {
                    mesh: quad,
                    texture: sprite,
                    color,
                    model,
                    // Composited, not alpha-tested: an effect is light, not
                    // a cutout. The blend pass sorts these back-to-front
                    // with the rest of the transparent draws and writes no
                    // depth, so overlapping puffs add up instead of one
                    // punching a hole in the others.
                    layer: render::DrawLayer::Blend,
                    // An effect puff is a near-scene draw and takes no
                    // far-scene cull; the map's cull is applied to the
                    // world, the terrain and the water (`viewer::far_cull`).
                    cull: None,
                    // and no near edge either — that is the `LOWR` sheet's
                    // alone (`viewer::lowres_range`).
                    cull_near: None,
                    lod: None,
                    light: render::LightPath::Lit,
                    fade: None,
                    lit: None,
                }
            })
            .collect()
    }
}

/// Transform + colour for one puff, facing `camera`.
fn billboard(p: &Puff, camera: &Camera) -> (Mat4, [f32; 4]) {
    let right = camera.right();
    let up = right.cross(camera.forward()).normalize_or_zero();
    let (size, color) = p.look();
    // Third column = the billboard's own normal (toward the camera), so the
    // matrix is not singular and the winding stays as authored.
    let basis = glam::Mat3::from_cols(right * size, up * size, -camera.forward() * size);
    (
        Mat4::from_translation(p.pos) * Mat4::from_mat3(basis),
        color,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn puff(kind: Kind) -> Puff {
        Puff {
            pos: Vec3::ZERO,
            kind,
            team: 0,
            age: 0.0,
        }
    }

    #[test]
    fn puffs_expire_after_their_lifetime() {
        let mut fx = Fx {
            puffs: vec![puff(Kind::Hit), puff(Kind::Impact)],
            ..Fx::default()
        };
        fx.update(HIT_LIFE + 1e-3);
        assert_eq!(fx.len(), 1, "the hit spark is the short one");
        fx.update(IMPACT_LIFE);
        assert!(fx.is_empty());
    }

    /// Effects fade by ALPHA now that they are composited, not by dimming
    /// their colour — a flash that dims and stays opaque reads as a grey card.
    #[test]
    fn effects_change_size_and_fade_out_by_alpha() {
        let mut m = puff(Kind::Hit);
        let (_, c0) = m.look();
        assert_eq!(c0[3], 1.0, "born fully opaque");
        m.age = HIT_LIFE;
        let (_, c1) = m.look();
        assert_eq!(c1[3], 0.0, "and is gone by the end of its life");
        assert_eq!(c1[..3], c0[..3], "colour does not dim; alpha does the work");

        let mut i = puff(Kind::Impact);
        let (s0, a0) = i.look();
        i.age = IMPACT_LIFE;
        let (s1, a1) = i.look();
        assert!(s1 > s0, "the impact expands");
        assert!(a1[3] < a0[3]);
    }

    /// Every effect goes through the blend pass. Alpha-testing them at 0.5
    /// would delete the soft rim and stop them fading at all.
    #[test]
    fn effects_are_composited_not_alpha_tested() {
        let fx = Fx {
            puffs: vec![puff(Kind::Impact), puff(Kind::Hit), puff(Kind::Blast(4.0))],
            ..Fx::default()
        };
        let draws = fx.draws(render::MeshHandle::default(), None, &Camera::default());
        assert_eq!(draws.len(), 3);
        assert!(draws.iter().all(|d| d.layer == render::DrawLayer::Blend));
    }

    #[test]
    fn quads_face_the_camera() {
        let p = puff(Kind::Hit);
        // Camera looking down -Z: the quad's x axis is world +X, y axis +Y.
        let (m, _) = billboard(&p, &Camera::default());
        assert!(m.x_axis.truncate().normalize().abs_diff_eq(Vec3::X, 1e-4));
        assert!(m.y_axis.truncate().normalize().abs_diff_eq(Vec3::Y, 1e-4));

        // Yawed 90 deg: it turns with the camera.
        let cam = Camera {
            yaw: std::f32::consts::FRAC_PI_2,
            ..Camera::default()
        };
        let (m, _) = billboard(&p, &cam);
        let x = m.x_axis.truncate().normalize();
        assert!(x.abs_diff_eq(Vec3::NEG_Z, 1e-4), "{x:?}");
    }

    /// The BF1 common layer, when this tree has a bake.
    fn bf1_common() -> Option<Library> {
        let dir = crate::assets::baked_root().join("common_bf1");
        if !dir.join("effects.ron").exists() {
            eprintln!("no baked common_bf1/effects.ron: skipped");
            return None;
        }
        let lib = Library::load(&crate::assets::baked_root(), &dir, "bf1", &[]);
        assert!(!lib.is_empty());
        Some(lib)
    }

    /// Card FXP3: an impact spawns its ordnance's effect for the surface,
    /// then its explosion's; a name no layer holds spawns nothing, and air
    /// expiry (no `Impact`) spawns nothing at all.
    #[test]
    fn impacts_spawn_the_surface_effect_then_the_explosion() {
        use core::sim::Event;
        use core::weapon::WeaponClass;
        let Some(lib) = bf1_common() else { return };
        let mut fx = Fx::with_library(lib);
        let mut sim = Sim::empty(1);
        let mut w = WeaponClass::bf1_rifle();
        let l = w.ordnance.impact.as_mut().unwrap();
        l.building = Some("no_such_effect".into());
        l.explosion = Some(ExplosionLook {
            effects: vec!["small_explosion".into()],
            water_effect: None,
            sound: None,
        });
        let wc = sim.add_weapon_class(w) as u32;
        let impact = |surface| Event::Impact {
            pos: Vec3::ZERO,
            dir: Vec3::NEG_Z,
            weapon_class: wc,
            surface,
            vehicle: None,
        };
        let names = |fx: &Fx| -> Vec<String> {
            fx.live
                .iter()
                .map(|e| e.def.name.clone().unwrap_or_default())
                .collect()
        };
        sim.events = vec![impact(Surface::Terrain)];
        fx.ingest_sim(&sim);
        assert_eq!(names(&fx), [BF1_IMPACT, "small_explosion"]);
        assert_eq!(
            fx.live[1].frame.pos,
            Vec3::Z * EXPLOSION_OFFSET,
            "backed off along the bolt"
        );
        // No SoundName on `small_explosion`: nothing queued (card WSND).
        assert!(fx.take_sounds().is_empty());
        // `Explosion` carries "com_weap_obj_med_exp defer" (`med_explosion`,
        // the AAT's death blast, carries none).
        let med = ExplosionLook {
            effects: vec!["Explosion".into()],
            ..ExplosionLook::default()
        };
        assert!(fx.explode(&med, Vec3::X));
        assert_eq!(
            fx.take_sounds(),
            [("com_weap_obj_med_exp".to_owned(), Vec3::X)]
        );
        assert!(fx.take_sounds().is_empty());
        fx.clear();
        sim.events = vec![impact(Surface::Building)];
        fx.ingest_sim(&sim);
        assert_eq!(names(&fx), ["small_explosion"]);
        fx.clear();
        sim.events = vec![Event::BoltExpired {
            pos: Vec3::ZERO,
            struck: false,
        }];
        fx.ingest_sim(&sim);
        assert!(fx.is_empty());
    }

    /// A synthetic emitter: MaxParticles is a LIFETIME budget — bursts of
    /// 3 every 0.1 s stop after 7 (the last clamped to 1), and the instance
    /// dies once the last burst outlives the Transformer's 0.25 s.
    #[test]
    fn bursts_spend_a_lifetime_budget() {
        let Some(lib) = bf1_common() else { return };
        let mut em = lib.get(BF1_IMPACT).unwrap().emitters[0].clone();
        em.start_delay = manifest::FxRange(0.05, 0.0);
        em.burst_delay = manifest::FxRange(0.1, 0.0);
        em.burst_count = manifest::FxRange(2.6, 0.0);
        em.max_particles = manifest::FxRange(7.0, 0.0);
        em.transformer.lifetime = 0.25;
        let mut inst = Instance::new(&em, Era::Bf1, 1, 0);
        let f = Frame::at(Vec3::ZERO);
        inst.update(&em, 0.04, f, false);
        assert_eq!(inst.emitted(), 0, "StartDelay 0.05");
        inst.update(&em, 0.06, f, false);
        assert_eq!(inst.emitted(), 3, "(int)(2.6 + 0.5)");
        inst.update(&em, 0.26, f, false);
        assert_eq!(inst.emitted(), 7, "3 + 3 + 1: clamped to the budget");
        assert!(!inst.is_dead());
        inst.update(&em, 0.25 + 0.25 + 1e-3, f, false);
        assert!(inst.is_dead(), "the last burst (t = 0.25) retired");
    }

    /// `ord_explosion` (re-fx §1): Sparks emits 2 ANIMATED particles and its
    /// sibling Flare 1 PARTICLE, all at t = 0; the Flare is gone after its
    /// 0.1 s, the effect after the Sparks' 0.2 s.
    #[test]
    fn ord_explosion_spawns_two_plus_one_and_dies_after_0_2_s() {
        let Some(lib) = bf1_common() else { return };
        let def = lib.get(BF1_IMPACT).unwrap();
        assert_eq!(
            def.emitters
                .iter()
                .map(|e| e.label.as_str())
                .collect::<Vec<_>>(),
            ["Sparks", "Flare"]
        );
        let mut fx = Fx::with_library(lib);
        assert!(fx.spawn(BF1_IMPACT, Frame::at(Vec3::new(1.0, 2.0, 3.0))));
        let e = &fx.live()[0];
        assert_eq!(e.emitted(), [2, 1]);
        let ps = e.particles(Era::Bf1);
        assert_eq!((ps[0].len(), ps[1].len()), (2, 1));
        // Sparks: no velocity, so they sit at the hit; born at alpha 0.
        assert!(ps[0].iter().all(|p| p.pos == Vec3::new(1.0, 2.0, 3.0)));
        assert!(ps[0].iter().all(|p| p.frame == Some(0) && p.rgba[3] == 0.0));
        fx.update(0.15);
        let ps = fx.live()[0].particles(Era::Bf1);
        assert_eq!(
            (ps[0].len(), ps[1].len()),
            (2, 0),
            "the Flare's 0.1 s is over"
        );
        // 0.15 s of 0.015 s frames: frame 10; the colour well into orange.
        assert!(ps[0].iter().all(|p| p.frame == Some(10)));
        fx.update(0.049);
        assert_eq!(fx.live().len(), 1);
        fx.update(0.002);
        assert!(fx.live().is_empty(), "dead after 0.2 s");
    }

    /// BF1's Sparks draw from 20 variants: every Sparks particle's size is
    /// one of its emitter's 20 cached sizes (0.3 + 0.3·rand, ×1000 in an
    /// i16), and the square is drawn √2·size′ wide at sizeMul 1.2 up close.
    #[test]
    fn sparks_come_from_the_variant_cache_and_draw_at_the_size_multiplier() {
        let Some(lib) = bf1_common() else { return };
        let def = lib.get(BF1_IMPACT).unwrap().clone();
        let sizes: Vec<f32> = def.caches[0].iter().map(|v| v.size).collect();
        assert_eq!(sizes.len(), 20);
        assert!(sizes.iter().all(|s| (0.3..0.6).contains(s)));
        let mut rng = Lcg(3);
        let e = LiveEffect::new(
            def,
            Era::Bf1,
            Frame::at(Vec3::new(0.0, 0.0, -10.0)),
            &mut rng,
        );
        let ps = e.particles(Era::Bf1);
        assert!(ps[0].iter().all(|p| sizes.contains(&p.size)));
        let mut out = Vec::new();
        e.draws(Era::Bf1, Vec3::ZERO, Vec3::NEG_Z, &mut |_| None, &mut out);
        assert_eq!(out.len(), 3);
        match &out[0].shape {
            render::ParticleShape::Sprite { half, .. } => {
                let want = ps[0][0].size * 1.2 * std::f32::consts::FRAC_1_SQRT_2;
                assert!((half - want).abs() < 1e-6, "{half} {want}");
            }
            s => panic!("{s:?}"),
        }
        assert!(out.iter().all(|d| d.additive));
    }
}
