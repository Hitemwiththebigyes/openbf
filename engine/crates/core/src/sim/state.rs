//! What the sim IS, and how it is hashed: the [`Sim`] struct with its
//! counters ([`NavStats`]), cheats ([`Cheats`]) and per-tick output
//! ([`Event`], [`DeathCause`]) and the fall rule ([`LETHAL_LANDING_SPEED`]);
//! the constructors ([`Sim::empty`], [`Sim::new`]); the queries — a unit by
//! handle, the ground under a point ([`Sim::ground_under`]), the terrain ray;
//! [`omni_radius_for`]; and [`Sim::hash`], what every replay gate compares.
//! The hash tests live here, the 655566562afc1358 `--sim-hash 100` literal among them.

use super::spawn::SpawnWaves;
use crate::aim_assist::AssistState;
use crate::battle::Battle;
use crate::collision::{
    CollisionWorld, FOOT_RADIUS, FOOT_RING, FOOT_RING_DIRS, NO_GROUND, STEP_HEIGHT, TERRAIN_BURY,
};
use crate::cp::CommandPost;
use crate::heightfield::Heightfield;
use crate::lcg::GameLcg;
use crate::projectile::Bolt;
use crate::region::Region;
use crate::rng::SimRng;
use crate::unit::{Unit, UnitHandle, UnitSpec};
use crate::weapon::WeaponClass;
use glam::Vec3;

/// PROVISIONAL (ours): a soldier that lands at or beyond this DOWNWARD speed
/// dies — 25 m/s, about a 17 m drop at the soldier's g = 18
/// (`core::soldier::GRAVITY`). Both games deal landing damage by a law the
/// port does not have yet — (approach − CollisionThreshold)² × CollisionScale
/// per class (`analysis/re-soldier-ground.md` §4; cards SLAND, DTHR) — so this
/// is a single lethal-speed cut-off, not a damage model. It is also what makes
/// [`crate::collision::NO_GROUND`] mean what it says — nothing catches you.
pub const LETHAL_LANDING_SPEED: f32 = 25.0;

/// [`Sim::outcome`] when the time limit runs out on equal reinforcements:
/// BF2's `ObjectiveConquest:GameOptionsTimeLimitUp` calls
/// `MissionVictory({1,2})` — BOTH teams win (`analysis/re-time-limit.md` §1
/// hop 9 [V][O]). 2 is no team, so it hashes apart from either win and from
/// "undecided" (255).
pub const OUTCOME_DRAW: u8 = 2;

/// Session cheats (PLAN §Cheats): the originals' invincibility + infinite
/// ammo, plus openbf's infinite stamina / jet fuel / all-awards. Toggled from
/// the pause menu (ui::menu) or `--cheat`; reset on restart; apply to PLAYER
/// units only (MP: host-only, later).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cheats {
    pub invincible: bool,
    pub infinite_ammo: bool,
    /// Sprint energy never drains.
    pub infinite_stamina: bool,
    /// Jet/jump-pack fuel never drains (no jetpacks yet — registered for the
    /// registry's sake).
    pub infinite_jet_fuel: bool,
    /// All awards granted this session (awards system lands at P8).
    pub all_awards: bool,
}

impl Cheats {
    /// Registry view: (id, label, enabled) in menu order.
    pub fn entries(&self) -> [(&'static str, &'static str, bool); 5] {
        [
            ("invincible", "Invincibility (PS2: sos)", self.invincible),
            ("ammo", "Infinite ammo (PS2: ammo)", self.infinite_ammo),
            (
                "stamina",
                "Infinite stamina (openbf)",
                self.infinite_stamina,
            ),
            ("jet", "Infinite jet fuel (openbf)", self.infinite_jet_fuel),
            ("awards", "All awards (openbf, session)", self.all_awards),
        ]
    }

    pub fn toggle(&mut self, id: &str) {
        match id {
            "invincible" => self.invincible = !self.invincible,
            "ammo" => self.infinite_ammo = !self.infinite_ammo,
            "stamina" => self.infinite_stamina = !self.infinite_stamina,
            "jet" => self.infinite_jet_fuel = !self.infinite_jet_fuel,
            "awards" => self.all_awards = !self.all_awards,
            _ => {}
        }
    }
}

/// Things that happened during a tick (drained by the game layer for HUD,
/// audio, traces and Tier-4 artifacts).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Shot {
        unit: UnitHandle,
        /// The weapon class that fired: the unit's own on foot, the seat
        /// gun's for a rider (card VSND - its `FireSound` is the gun's).
        weapon_class: u32,
        origin: Vec3,
        dir: Vec3,
    },
    Hit {
        shooter: UnitHandle,
        victim: UnitHandle,
        /// The health the hit took (after the shield and the scales; 0 for
        /// a hit the shield swallowed).
        damage: f32,
        /// Bolt age at impact (seconds) — distance = age × velocity.
        age: f32,
        /// The victim's health after the hit (0 or less: it killed). The hurt
        /// cry's 2 % test reads it (card UCRY, `analysis/re-unit-chatter.md`
        /// 2.7: B1 `FUN_001bc190` 64387).
        health_left: f32,
        /// The way the shot travelled, unit length (card HATK: the HUD's
        /// hit-direction chevrons, `analysis/re-hud-attacker.md` §3): a
        /// bolt's velocity; a blast's victim centre less the blast centre
        /// (PROVISIONAL, §10), zero when they coincide.
        dir: Vec3,
    },
    Kill {
        shooter: UnitHandle,
        victim: UnitHandle,
    },
    BoltExpired {
        pos: Vec3,
        /// The bolt STRUCK something at `pos` — the terrain sheet, a world
        /// model or a hull — rather than running out of life in the air. The fx puff and
        /// the impact sound key off this, and neither cares which it was.
        struck: bool,
    },
    /// A bolt struck `surface` at `pos` (card FXP3): the fx layer spawns the
    /// ordnance's impact effect and its explosion's (`analysis/re-fx.md` §7,
    /// B1 FUN_00276810). Alongside [`Event::BoltExpired`] or [`Event::Hit`],
    /// never instead; the sim never reads it. `dir` is the bolt's unit
    /// direction, `weapon_class` indexes [`Sim::weapon_classes`], and a
    /// struck hull names its `vehicle` (the effect rides it).
    Impact {
        pos: Vec3,
        dir: Vec3,
        weapon_class: u32,
        surface: crate::impact::Surface,
        vehicle: Option<crate::vehicle::VehicleHandle>,
    },
    /// AI shed by the no-overspawn rule when a player took the last slots
    /// (or bleed dropped the pool below the live count).
    Despawn {
        unit: UnitHandle,
    },
    Respawn {
        unit: UnitHandle,
    },
    /// A reload started (ReloadSound hook).
    Reload {
        unit: UnitHandle,
    },
    /// `unit` selected the weapon `weapon_class` with a switch input (its
    /// `WeaponChangeSound`, re-weapon-sounds.md §7.1).
    WeaponSelect {
        unit: UnitHandle,
        weapon_class: u32,
    },
    /// A human pressed the trigger on a weapon that is reloading, overheated
    /// or out of ammunition (its `FireEmptySound`, §7.3).
    FireEmpty {
        unit: UnitHandle,
    },
    /// A grenade left `unit`'s hand (card GREN): the weapon's FireSound and
    /// the drawn throw. `weapon_class` indexes [`Sim::weapon_classes`].
    Throw {
        unit: UnitHandle,
        weapon_class: u32,
        origin: Vec3,
    },
    /// A thrown grenade touched something and bounced, stuck or came to rest
    /// (its `CollisionOtherSound`); `water` for the water's surface.
    StickyContact {
        pos: Vec3,
        weapon_class: u32,
        water: bool,
    },
    /// A thrown grenade's fuse ran out at `pos`: its `ExplosionExpire` class
    /// went off there (the damage is already dealt, [`Sim::explode`]).
    StickyBlast {
        pos: Vec3,
        weapon_class: u32,
    },
    /// A player slot gave a squad order (card SQACK). `kind` is
    /// [`crate::squad::Order`]'s byte (0 follow, 1 hold, 2 at ease, 3 move
    /// out); `units` is how many bots are in its reach — pushed even when
    /// that is 0: the player's line plays whether or not anyone hears it.
    Command {
        slot: u8,
        kind: u8,
        units: u8,
    },
    /// `unit` took slot `slot`'s order at its think and answers "yes" —
    /// whatever the order then makes it do. `did`: the order changed what
    /// the unit does (it now follows or holds, or was released by it) — the
    /// HUD's count (card SQHUD).
    OrderTaken {
        unit: crate::unit::UnitHandle,
        slot: u8,
        did: bool,
    },
    /// Card UCALL: a bot asks for one of BF1's combat calls (its class's
    /// `AcquiredTargetSound`, `FleeSound`, `HeardEnemySound`;
    /// re-unit-chatter.md 2.2-2.4). On a BF2 map only the tracker's take
    /// and the grenade warning, which ask BF2's `VOSound` lines (card UVO2).
    Call {
        unit: crate::unit::UnitHandle,
        call: Call,
    },
    Captured {
        cp: u32,
        team: u8,
    },
    Neutralized {
        cp: u32,
        by: u8,
    },
    /// Owner units recovered a post being neutralized.
    Saved {
        cp: u32,
        team: u8,
    },
    /// BF2 rule: tickets hit 0 (or domination timer expired) → round over.
    TeamDefeated {
        team: u8,
    },
    /// The round's time limit ran out ([`Sim::time_limit`]): `winner` is the
    /// team with more reinforcements, or [`OUTCOME_DRAW`] when they are equal.
    TimeUp {
        winner: u8,
    },
    /// The world killed a unit: a mission death region, or a lethal landing.
    /// See [`Sim::kill_environment`].
    EnvDeath {
        unit: UnitHandle,
        cause: DeathCause,
    },
    /// Damage reached a vehicle's hull pool (card VDMG): a bolt (`shooter`
    /// set) or a collision (`None`). `damage` is what left the pool.
    VehicleHit {
        shooter: Option<UnitHandle>,
        vehicle: crate::vehicle::VehicleHandle,
        damage: f32,
        /// The way the bolt or blast travelled, as [`Event::Hit`]'s `dir`;
        /// `None` for a collision.
        dir: Option<Vec3>,
    },
    /// A vehicle's hull pool ran out and it is gone this tick, no wreck
    /// (re-vehicles §6, [`Sim::destroy_vehicle`]). The game layer reads the
    /// class's `ExplosionName` and chunks from `class`; `pos` / `vel` are the
    /// hull's at death (the chunks inherit `vel`).
    VehicleDestroyed {
        vehicle: crate::vehicle::VehicleHandle,
        class: u32,
        pos: Vec3,
        vel: Vec3,
        killer: Option<UnitHandle>,
    },
    /// A dead soldier dropped an item (card PWUP); `item` indexes
    /// [`Sim::item_classes`].
    ItemDropped {
        item: u8,
        pos: Vec3,
    },
    /// An item came to rest (BF1's `CollisionOtherSound`).
    ItemLanded {
        item: u8,
        pos: Vec3,
    },
    /// `unit` took the item at `pos`: its `PowerupSound`, and the taker's
    /// gain effect for what it gave (`re-powerups.md` §5.6).
    ItemTaken {
        unit: UnitHandle,
        item: u8,
        pos: Vec3,
        health: bool,
        ammo: bool,
        energy: bool,
    },
    /// A station served a unit or a vehicle (card PWST): its `PowerupSound`
    /// at `pos` (the station's), and the unit's gain effect for what it
    /// gave. `station` indexes [`Sim::stations`].
    StationServed {
        station: u16,
        unit: Option<UnitHandle>,
        vehicle: Option<crate::vehicle::VehicleHandle>,
        pos: Vec3,
        health: bool,
        ammo: bool,
        /// BF2 energy: no disc station class gives any (§2.2), always
        /// false today.
        energy: bool,
    },
    /// A door's trigger was entered: its `OpenSound` plays (queue DRV,
    /// [`crate::door::Door::edge`]). `door` indexes [`Sim::doors`].
    DoorOpened {
        door: u32,
    },
    /// A door's trigger emptied: its `CloseSound` plays.
    DoorClosed {
        door: u32,
    },
}

/// Card HOLD: a live explosive lying on the ground — a landed grenade — that
/// bots within its `radius` get warned of and run clear of (`crate::ai`
/// `think`; the PS2's grenade warning, re-grenades.md §7.4: every AI unit
/// within 7.5 m after a difficulty-ranked delay). Nothing in the port throws
/// one yet (card GREN); [`Sim::add_hazard`] is the hook its landing calls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hazard {
    pub pos: Vec3,
    /// The warning radius (m).
    pub radius: f32,
    /// The tick it landed: the reaction delay counts from here.
    pub since: u64,
    /// The tick it goes off or is gone: dropped from the list then.
    pub until: u64,
    /// Who threw it (card GRAI): its thrower runs from it at once.
    pub owner: Option<UnitHandle>,
}

/// The PS2's grenade-warning radius (m; re-grenades.md §7.4).
pub const HAZARD_RADIUS: f32 = 7.5;

/// One of BF1's combat calls ([`Event::Call`]); on a BF2 map only
/// [`Call::Acquired`] and [`Call::Flee`] are issued, the triggers of BF2's
/// AcquiredTarget and Grenade lines (card UVO2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    /// The tracker took a target ("there he is").
    Acquired,
    /// The first AI soldier within [`HAZARD_RADIUS`] of a live grenade
    /// ("grenade!").
    Flee,
    /// Hearing turned the unit to a heard enemy.
    Heard,
    /// Card UTALK: a trooper sets out for a post it is not at - "follow me"
    /// (its class's `AISCFieldFollowSound`; re-squad-orders.md 5).
    Follow,
    /// Card UTALK: a trooper has reached its post - "move out" (its class's
    /// `AISCFieldMoveOutSound`).
    MoveOut,
    /// Card UTALK: an AI that took a vehicle's pilot seat with a free gun
    /// seat calls for a gunner (its class's `AISCDriverGetInSound`).
    NeedGunner,
    /// Card UTALK: an AI soldier on foot within [`ORDER_REACH`] of a
    /// [`Call::NeedGunner`] answers "yes" (its class's
    /// `AISCResponseYesSirSound`).
    ///
    /// [`ORDER_REACH`]: crate::squad::ORDER_REACH
    Yes,
    /// Card UTALK: the class's `LowHealthSound`. Never in the sim's events:
    /// its handle is the sound side's (`game::announce::Commander::calls`).
    LowHealth,
    /// Card UVO2: a BF2 `VOSound` line for the event of this index in the
    /// PS2's table (`game::vo::EVENTS`). Never in the sim's events: the
    /// sound side picks it from them (`game::vo::Vo::tick`).
    Vo(u8),
}

/// Card UTALK: the chance a trooper's "follow me" is issued as it sets out
/// (B1 `FUN_0024b7a0` 165905, `0x3e99999a`; re-squad-orders.md 5), and a
/// seated AI's need-gunner call (`FUN_0024d020` 167039). The AI's "move
/// out" is issued with chance 1 (`FUN_0024baf0` 166190).
pub const FOLLOW_CHANCE: f32 = 0.3;
pub const NEED_GUNNER_CHANCE: f32 = 0.3;

/// The grenade warning's reaction delay at the PS2's default enemy
/// difficulty (s): uniform in (0.6, 1.3) - B1 `FUN_00236be8` 150870-150939,
/// the difficulty word's default 14 (`FUN_0023a540` 153290) takes the
/// "under 17" pair (re-unit-chatter.md 2.3). PROVISIONAL for the port's
/// other difficulties.
pub const GRENADE_WARN_DELAY: (f32, f32) = (0.6, 1.3);

/// What the world killed a unit with ([`Event::EnvDeath`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeathCause {
    /// Inside one of the mission's `AddDeathRegion` volumes.
    DeathRegion,
    /// Landed at or beyond [`LETHAL_LANDING_SPEED`].
    Fall,
    /// Riding a vehicle destroyed by no attacker (a crash into the world,
    /// [`Sim::destroy_vehicle`]); a destroyer's riders are his kills.
    VehicleDestroyed,
    /// An explosion with no attacker (an empty vehicle's death blast,
    /// [`Sim::explode`]).
    Blast,
    /// Below the map's kill floor ([`Sim::kill_floor`]).
    KillFloor,
    /// A hard landing under the map ([`Sim::lands_under_the_map`]).
    UnderMap,
    /// A bot off the playable level that found no way back
    /// (`crate::ai` `LOST_RECYCLE`): the last resort.
    Lost,
    /// A bot on the level with somewhere to go that made no net progress
    /// for `crate::ai` `RECYCLE_AFTER` (card RCYC): the last resort, and
    /// every use is a defect to close.
    Stalled,
}

impl DeathCause {
    /// Does the `invincible` cheat spare the player from this? The PS2's
    /// death regions and its world floor kill past the cheat (the kill's
    /// third argument is 0 — `analysis/re-kill-floor.md`), and so does a
    /// destroyed vehicle's (re-vehicle-death 10.1 item 4); its landing damage
    /// and blasts are ordinary damage, which the cheat stops.
    pub fn cheat_spares(self) -> bool {
        matches!(self, DeathCause::Fall | DeathCause::Blast)
    }
}

pub struct Sim {
    pub tick: u64,
    /// The round's difficulty tier (PLAN §Saves: host authority). SIM STATE —
    /// it decides every bot's skill at spawn and the two human layers, so it
    /// is HASHED (one byte, right after `tick`). Default `Normal`.
    pub difficulty: crate::skill::Difficulty,
    /// Card ARVS: a tier per team (0 and 1) over [`Sim::difficulty`], for the
    /// arena's head-to-head (`arena ... vs hard:elite`). Every per-unit tier
    /// read goes through [`Sim::difficulty_of`]; `None` (the default, and
    /// every battle the game runs) is `difficulty`. SIM STATE: hashed after
    /// `difficulty`, but only when one is set, so an unset pair keeps every
    /// hash the sim had before.
    pub team_difficulty: [Option<crate::skill::Difficulty>; 2],
    /// Radius inside which a target is inside the AI's view cone whatever the
    /// angle: BF2's 10 m omni (RE §9.1). **BF1 maps set 0.0** — BF1's cone
    /// test has no omni branch. An INPUT like `nav`, set by whoever builds the
    /// sim from `scene.manifest.source_game`; NOT hashed.
    pub omni_radius: f32,
    /// Which game's ground rules the soldiers move by (card STHOP). An INPUT
    /// like `omni_radius`, set from the manifest's `source_game` by the same
    /// sites ([`crate::soldier::GroundRules::for_game`]); BF2's by default.
    pub ground_rules: crate::soldier::GroundRules,
    /// Host option: apply the tier's `human_damage` multiplier to a bot's
    /// bolt into a human (D12). An INPUT — constant for a round; what it
    /// gates reaches the hash through health and tickets.
    pub human_damage_on: bool,
    /// Host option: apply the Elite team handicap (bleed ×1.25 against a team
    /// with a human whose opponent has none, D12). An INPUT, like
    /// `human_damage_on`.
    pub team_handicap_on: bool,
    pub rng: SimRng,
    pub lcg: GameLcg,
    pub heightfield: Option<Heightfield>,
    /// The map's baked model collision (`coll` meshes, `prim` primitives,
    /// terrain cutters). Empty = heightfield-only, as before. An INPUT like the
    /// heightfield and not part of `hash` — except that [`Sim::doors`] MOVE
    /// their bodies in it, which is why the doors' state is hashed — and the
    /// live vehicles' hulls ride in it, placed from their hashed poses
    /// (`Sim::sync_hulls`, card VHULL). Callers set both after `Scene::apply_mode_layer`
    /// (`game::assets::Scene::install_collision`).
    pub collision: CollisionWorld,
    /// BF2's self-opening doors (queue DR2, `crate::door`), each moving
    /// bodies of `collision`. Their `open` parameters are SIM STATE and
    /// hashed — when there are any, so a map with no door (every BF1 map, the
    /// fixtures) hashes exactly as before the doors existed.
    pub doors: Vec<crate::door::Door>,
    pub weapon_classes: Vec<WeaponClass>,
    /// Per-team soldier CLASS tables, index-aligned with what
    /// `UnitSpec::class` names (`game::roster::FactionRoster` fills them from
    /// the mission's mix, else the faction's default list). SETUP data like
    /// `weapon_classes`: set before anyone spawns, never mutated by `step`,
    /// and NOT hashed — a table is an input, what the units took out of it is
    /// the state. Empty (the `Sim::empty` default) = no class system: every
    /// spawn comes from the caller's own `UnitSpec`, exactly as before.
    pub classes: [Vec<crate::unit::UnitClass>; 2],
    /// Per player SLOT, the class index that slot's NEXT respawn takes
    /// (`buttons::SPAWN_CLASS_NEXT`/`PREV` while dead). Parallel to
    /// `players`, grown by `sync_slot_tables`, and HASHED — a player who has
    /// picked the sniper is in a different state from one who has not.
    pub spawn_choice: Vec<u8>,
    /// Per player slot, last tick's spawn-select bits, for edge detection. A
    /// dead soldier does not step, so `Soldier::prev_buttons` cannot serve.
    /// Hashed beside `spawn_choice`.
    pub prev_spawn_bits: Vec<u32>,
    /// Per player slot, the POST the spawn map has selected for that slot's
    /// next respawn (card SPSEL; `buttons::SPAWN_POST_*` while dead, the PS2's
    /// pick `Sim::pick_spawn_post`). `None` = never picked: the respawn draws
    /// a node from every post's, as before the spawn map reached the sim.
    /// Hashed only once some slot holds a pick, so a sim whose players never
    /// touched the spawn map keeps its hash.
    pub spawn_post_choice: Vec<Option<crate::unit::SpawnPost>>,
    /// Card SSELA: the human slots pass through the spawn screens (the PS2's
    /// spawn display, re-spawn-flow §1): a slot is on the field only once it
    /// accepts a class, and a dead one comes back only through the spawn map
    /// and the class accept ([`Sim::open_spawn_screens`] turns it on, the
    /// game binary's battles call it). Off (the default: every headless
    /// referee, the golden scenes, `--spawn`) = auto-accept: a dead human
    /// comes back once its 4.0 s timer has run out. An INPUT for the round;
    /// it reaches the hash through the per-slot block below.
    pub spawn_screens: bool,
    /// Card SSELA: per player slot, which spawn screen is up
    /// ([`super::spawn::SpawnStage`]); `Accepted` while the slot is on the
    /// field or waiting out its respawn timer. Parallel to `players`.
    pub spawn_stage: Vec<super::spawn::SpawnStage>,
    /// Card SSELA: per player slot, the side screen's highlighted row (team
    /// 1, team 2, Auto Assign). Parallel to `players`. This and
    /// `spawn_stage` are hashed together, only while `spawn_screens` is on
    /// or a slot holds a non-default value, so every sim that never opens the
    /// screens keeps its hash.
    pub side_choice: Vec<super::spawn::SideChoice>,
    /// Card SSELB: per player slot, the class carousel's input state (the
    /// PS2's `SD+0x25` arm and `SD+0x33` slide, `FUN_00399dd8`): a held
    /// step fires once, and a step while the line-up slides is dropped.
    /// Parallel to `players`; hashed only while some slot's is not idle.
    pub class_carousel: Vec<super::spawn::Carousel>,
    /// Card SSELB: per player slot, the human's points (the PS2's stats+0x1fc,
    /// `analysis/re-spawn-flow.md` §4.3), which unlock a class on the class
    /// screen ([`Sim::class_refusal`]). An INPUT the game writes from the
    /// slot's event feed before each tick (the feed scores the points); not
    /// hashed, like the pads. Missing = 0.
    pub spawn_points: Vec<i32>,
    /// How many AI this team has EVER spawned — the cursor `ai_class` walks
    /// the class table with. Hashed (it decides the next AI's class).
    pub ai_spawned: [u32; 2],
    /// Unit slots; dead slots are reused with a bumped generation.
    pub units: Vec<Unit>,
    /// Vehicle slots (`crate::vehicle`, card VENT), reused like `units`.
    /// Hashed after the units — and NOTHING at all when empty, so a
    /// vehicle-free sim hashes exactly as before.
    pub vehicles: Vec<crate::vehicle::Vehicle>,
    /// The vehicle spawn pads (`crate::vehicle_pad`, card VPAD), ticked after
    /// the posts. Hashed last — and NOTHING at all when empty.
    pub vehicle_pads: Vec<crate::vehicle_pad::VehiclePad>,
    /// Per vehicle CLASS (`Vehicle::class` indexes it), the hover numbers —
    /// `None` for a class nothing integrates (walkers, flyers, turrets).
    /// SETUP data like `weapon_classes`, NOT hashed. A vehicle whose class
    /// has none, or is past the end, stands still (`Sim::tick_hovers`).
    pub hover_params: Vec<Option<crate::vehicle::hover::HoverParams>>,
    /// Per vehicle CLASS, the walker numbers (card WALK) — `None` for a
    /// class that is not a walker. SETUP data, NOT hashed. A walker whose
    /// class has none stands still (`Sim::tick_walkers`).
    pub walker_params: Vec<Option<crate::vehicle::walker::WalkerParams>>,
    /// Per vehicle CLASS, what the use input and the exit read (card VSEAT,
    /// `crate::vehicle::SeatSetup`); SETUP data, NOT hashed. A class past
    /// the end, or `None`, cannot be entered.
    pub seat_setup: Vec<Option<crate::vehicle::SeatSetup>>,
    /// Per vehicle CLASS, its death explosion and its blast sphere (card
    /// VDTH, `crate::vehicle::BlastSetup`); SETUP data, NOT hashed.
    pub blast_setup: Vec<Option<crate::vehicle::BlastSetup>>,
    /// Per vehicle CLASS, its hull's bodies in model space (card VHULL,
    /// `crate::collision::HullPart`) — `None` for a class with no collision
    /// the port builds, or a building turret, whose building is the world's
    /// own body. SETUP data, NOT hashed: a live vehicle's hull in
    /// [`Sim::collision`] follows its hashed pose (`Sim::sync_hulls`).
    pub hull_parts: Vec<Option<std::sync::Arc<[crate::collision::HullPart]>>>,
    pub bolts: Vec<Bolt>,
    /// The thrown grenades in flight or lying armed (card GREN,
    /// [`crate::sticky`]), in throw order. HASHED when there is one.
    pub stickies: Vec<crate::sticky::Sticky>,
    /// The `powerupitem` classes (card PWUP, `sim/pickups.rs`). SETUP data
    /// like `weapon_classes`, NOT hashed.
    pub item_classes: Vec<super::pickups::ItemClass>,
    /// The `PowerupItem` pool's capacity: a death drops nothing while this
    /// many items lie on the field (25; BF1 missions set 25 to 60). An INPUT.
    pub item_pool: usize,
    /// The items on the field, in drop order. HASHED when there is one.
    pub pickups: Vec<super::pickups::Pickup>,
    /// The `powerupstation` classes (card PWST, `sim/stations.rs`). SETUP
    /// data like `item_classes`, NOT hashed.
    pub station_classes: Vec<super::stations::StationClass>,
    /// The placed health / ammo / vehicle droids, in manifest order. HASHED
    /// when there is one.
    pub stations: Vec<super::stations::Station>,
    pub battle: Battle,
    pub cheats: Cheats,
    /// BF1 DAT_00517508 / BF2 DAT_005fcbe0: the profile's friendly-fire
    /// percent x 0.01, 100 in every instant-action battle state read (326
    /// states, both games; re-blaster-damage.md section 6.3). A bot's shot
    /// into a human of its own team is dropped before this scale
    /// (`resolve_bolt`, `blast_unit`).
    pub friendly_fire_scale: f32,
    /// PLAN intensity slider; 1.0 = PS2-authentic.
    pub assist_intensity: f32,
    /// Term-3 gate (SP Auto Aim option); default on.
    pub assist_armed: bool,
    /// Aim-assist state per player slot (parallel to `players`).
    pub assist: Vec<AssistState>,
    /// Aim magnet per player slot (parallel to `players`): a unit, or a
    /// vehicle / building turret (card HTGT).
    pub magnet: Vec<Option<super::input::AimTarget>>,
    /// What each player slot's crosshair picks, with its relation (−1
    /// enemy, 0 team 0, 1 friend): the PS2's K+0x30 / K+0x40, which the HUD
    /// reticle colours by (`analysis/re-aim-assist.md` §3.7, card RETC) and
    /// the target readout shows (card HTGT). Written by `acquire_magnet`;
    /// NOT hashed while nothing in the sim reads it.
    pub reticle_pick: Vec<Option<(super::input::AimTarget, i8)>>,
    pub events: Vec<Event>,
    /// The unit driven by each pad passed to [`Sim::step`], indexed by player
    /// slot: slot `s` is driven by `pads[s]`. A slot whose entry is `None` has
    /// no unit; a slot past the end of the `pads` slice idles that tick.
    pub players: Vec<Option<UnitHandle>>,
    pub cps: Vec<CommandPost>,
    /// Zero-CP domination timer: (dominating team, seconds).
    pub domination: Option<(u8, f32)>,
    /// Winner once the round is decided (BF2: a team at 0 tickets loses
    /// immediately; domination timer at 20 s; the time limit, where one is
    /// set, gives it on tickets — [`OUTCOME_DRAW`] when they are equal). The
    /// sim keeps stepping.
    pub outcome: Option<u8>,
    /// The conquest time limit in seconds (the Instant Action `con_timer`
    /// option, queue TLO; `analysis/re-time-limit.md` §1 [V][O]). `None` =
    /// Off, the disc's power-on value and the default: no limit ends the
    /// round. Counted on the sim clock (`tick`). An INPUT like
    /// `team_handicap_on` — constant for a round, NOT hashed; what it decides
    /// reaches the hash through `outcome`.
    pub time_limit: Option<u32>,
    /// Death → respawn delay (BF2 FUN_0034f650 / BF1 FUN_00297440: 4.0 s).
    pub respawn_delay: f32,
    /// Per-team spawn waves (teams 0/1).
    pub waves: [SpawnWaves; 2],
    /// BF2 alive-AI cap per team (NumBots DAT_006004fc default 16; the
    /// PLAN unit-count slider raises it).
    pub ai_alive_cap: u32,
    /// Volumes that kill any living unit standing in them — the mission's
    /// `AddDeathRegion` list, resolved by
    /// `game::layout::MapLayout::apply_mission`. An INPUT like the
    /// heightfield: set once and not part of [`Sim::hash`].
    pub death_regions: Vec<Region>,
    /// The map's water level, when it has water (the manifest's `water`
    /// height). An INPUT like `omni_radius`, set by whoever builds the sim;
    /// NOT hashed. Card HOLD reads it: a bot takes no hold position on a
    /// floor under water.
    pub water_level: Option<f32>,
    /// Card HOLD: the grenades lying on the ground ([`Hazard`]). SIM STATE —
    /// hashed, when there are any, so a round with none hashes as before.
    pub hazards: Vec<Hazard>,
    /// The height under which every soldier dies (`sim/bounds.rs`):
    /// [`crate::sim::KILL_FLOOR_DROP`] under the lowest floor the map's graph
    /// and posts stand on. DERIVED by [`Sim::resolve_nav_floors`] from inputs,
    /// so not hashed. `None` = no graph and no post, no floor.
    pub kill_floor: Option<f32>,
    /// Card LLEG: the posts' spawn nodes that lie inside a solid body, each
    /// with the floor a unit spawned there stands on
    /// (`Sim::resolve_spawn_seats`). DERIVED from inputs, so not hashed.
    pub spawn_seats: Vec<(Vec3, f32)>,
    /// Per hub of `nav`, the floor a soldier walks there (NaN = the walk from
    /// the posts never reached it). DERIVED like `kill_floor`; empty = off.
    pub nav_floors: Vec<f32>,
    /// Per arc of `nav`: bit 0 = a soldier cannot WALK it from `a` to `b`,
    /// bit 1 = from `b` to `a` (`Sim::arc_walkable` — open air on the line,
    /// or a far end the walk does not arrive at). DERIVED with `nav_floors`.
    pub nav_unwalkable: Vec<u8>,
    /// Per hub of `nav`: a soldier there has no walkable way back to the
    /// main body of its part of the graph (`Sim::hub_trapped`, card TGAP).
    /// DERIVED with `nav_floors`; empty = none.
    pub nav_trapped: Vec<bool>,
    /// Card TPOS: the map's position table (`ai::tactic`): the places a
    /// soldier might stand to fight and their cover. DERIVED by
    /// [`Sim::build_positions`] from inputs, so not hashed; empty = none.
    pub positions: crate::ai::tactic::PositionTable,
    /// Card TEAM: the unit grid, live only inside the think loop
    /// (`ai::team`). DERIVED, not hashed.
    pub unit_grid: crate::ai::team::UnitGrid,
    /// The map's authored planning graph (`core::nav`), when it ships one.
    /// An INPUT like the heightfield and the collision world: immutable during
    /// the round and NOT part of [`Sim::hash`]. `None` = the AI walks straight
    /// at its post, as it did before Phase A. Set by
    /// `game::walk::WalkSim::with_teams` from `game::layout::nav_graph`.
    pub nav: Option<crate::nav::NavGraph>,
    /// Card FLNK: the flank table of `nav` between the posts' hubs
    /// (`nav::flank`), with this match's shares. DERIVED from `nav`, the
    /// posts and the seed ([`Sim::build_flank`]), so not hashed; empty =
    /// none.
    pub flank: crate::nav::FlankTable,
    /// Navigation diagnostics for the round (ROUND_VERBOSE / the sweep). An
    /// OUTPUT counter like `events`: deterministic, but not hashed.
    pub nav_stats: NavStats,
    /// Card HITCH: wall-clock ms the last [`Sim::step`] spent on the AI (the
    /// commander's plan, the turret offers, the team board and every bot's
    /// think) - the windowed game's frame recorder reads it. A measurement
    /// of the machine: never read by the sim, not hashed.
    pub ai_wall_ms: f32,
    /// Phase B: the two teams' objective plans (`core::commander`). SIM STATE
    /// — hashed, replay-exact, re-made one team per tick.
    pub commander: [crate::commander::TeamPlan; 2],
}

/// What the follower had to fall back on this round — the numbers the sweep
/// prints per map (PLAN Phase A design, acceptance).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NavStats {
    /// Hops the authored table had no entry for, answered by `route_bfs`.
    pub bfs_fallbacks: u64,
    /// Hops neither the table nor BFS could answer: the unit walked straight
    /// at its post.
    pub straight_fallbacks: u64,
    /// Longest run of ticks any unit spent making no progress toward its
    /// waypoint.
    pub stuck_max: u16,
    /// Legs whose straight line crossed a barrier and got a corner path
    /// instead (Phase A.1, `core::nav::NavGraph::detour`).
    pub detoured_legs: u64,
    /// Unit-ticks in which a unit wanted to move and the way ahead of it was
    /// NOT clear — the cost, and the size, of the steering layer's work.
    pub blocked_ticks: u64,
    /// Legs whose chosen next hub had NO FLOOR at the unit's own level and
    /// were rejected for an alternative (Phase A.2, `core::ai::hub_has_floor`).
    pub level_rejected: u64,
    /// Of those, the ones where every alternative failed too and the unit fell
    /// back to the straight line — the number that says a map's graph and its
    /// geometry disagree.
    pub level_unrouted: u64,
    /// Unit-ticks in which the forward wall SWEEP found a wall.
    pub sweep_hits: u64,
    /// Phase B: plan passes run (both teams together).
    pub plans: u64,
    /// Assignments a plan CHANGED (a unit given a different objective).
    pub assignment_changes: u64,
    /// Per team, a bit per command-post index the team ever ATTACKED — the
    /// "distinct attack objectives used over the round" the acceptance asks
    /// for. Posts past index 63 do not fit and are not counted (no disc map
    /// has more than 16).
    pub attack_used: [u64; 2],
    /// Ticks a unit was handed an objective the graph could not route to.
    pub unroutable_objectives: u64,
    /// The longest `AiState::unreachable_dwell` any unit reached.
    pub unreachable_dwell_max: u16,
    /// Phase C: shots fired by AI units over the round.
    pub ai_shots: u64,
    /// Of those, shots whose target the shooter's perception did NOT hold —
    /// the fairness rule of design decision 2. It must stay 0: a non-zero
    /// count is a bug, and `debug_assert` catches it in the tests.
    pub unperceived_shots: u64,
    /// The most contacts any unit's memory held at once.
    pub contacts_max: u8,
    /// Records made by HEARING (a shot inside `HEAR_RADIUS`, or being hit).
    pub heard_contacts: u64,
    /// Card TEAM: contact records made or refreshed from a friend's sight.
    pub told_contacts: u64,
    /// Card TEAM: Cover roles the team board gave, and AI ticks spent with
    /// one / on a sniper's spot (Overwatch) - the role counts per map.
    pub cover_given: u64,
    pub cover_ticks: u64,
    pub overwatch_ticks: u64,
    /// Card FLNK: Flank roles the team board gave, and AI ticks spent with
    /// one.
    pub flank_given: u64,
    pub flank_ticks: u64,
    /// ...and how they ended: the last leg into the post, a death, another
    /// objective, anything else (a seat, an order); and the stalled bots
    /// recycled while they held one.
    pub flank_ends: [u64; 4],
    pub flank_stalled: u64,
    /// ...and the legs the thinks planned along a flank route.
    pub flank_legs: u64,
    /// AI shots emitted by a fire cycle that was already running when the
    /// trigger came up — the weapon's, not the AI's (see `unperceived_shots`).
    pub carry_shots: u64,
    /// Card RKSLF: ticks a bot's exploding shot was dropped at its release
    /// because its blast would land on its own side
    /// (`ai::release_blasts_own_side`).
    pub blast_holds: u64,
    /// Card KNOCK: soldiers and corpses a blast's push reached
    /// (`Sim::kick_and_push`).
    pub knocks: u64,
    /// Bolts fired by an AI unit that HIT somebody — the numerator of the
    /// sweep's `hit_rate`, against `ai_shots`.
    pub ai_hits: u64,
    /// Card FFLN: kills credited to a unit (a bolt's or a blast's), and of
    /// those the ones whose victim was on the killer's own team (not the
    /// killer himself: `self_kills`). OUTPUT only.
    pub unit_kills: u64,
    pub team_kills: u64,
    pub self_kills: u64,
    /// Card FFLN: bolts that hit a unit, and of those a unit of the
    /// shooter's own team. OUTPUT only.
    pub bolt_hits: u64,
    pub friend_hits: u64,
    /// Card FFLN: bot-ticks a shot was withheld because a friend stood in
    /// the line of fire ([`crate::ai::NavTick::line_held`]). OUTPUT only.
    pub line_held: u64,
    /// Card TPOS: the Tactician's queries, and those in which a candidate
    /// with cover from the target passed its filters (B12's ceiling,
    /// [`crate::ai::NavTick::tpos_query`]). OUTPUT only.
    pub tpos_queries: u64,
    pub tpos_ceiling: u64,
    /// ...each `crate::ai::tactic::EV_*` bit's thinks, and (`[16]`) the
    /// queries with a covered candidate that gave no covered pick (every
    /// confirming ray shut). OUTPUT only.
    pub tpos_ev: [u64; 17],
    /// ...and `NavTick::tpos_funnel` summed. OUTPUT only.
    pub tpos_funnel: [u64; 14],
    /// Phase D: squad commands ISSUED (one per accepted edge, whatever the
    /// number of bots that took it — `Event::Command` carries that). OUTPUT
    /// only, never hashed: the sweep's player is idle, so the acceptance reads
    /// `commands 0` on all 32 maps.
    pub commands: u64,
    /// Followers RELEASED by the distance rule (brief D6). OUTPUT only.
    pub releases: u64,
    /// Card VAI: bots that entered a vehicle's seat. OUTPUT only.
    pub ai_boards: u64,
    /// Card VAI: bots that got out of a vehicle. OUTPUT only.
    pub ai_dismounts: u64,
    /// Card VAI: bot-ticks spent in a seat. OUTPUT only.
    pub ai_drive_ticks: u64,
    /// Card VTAI: bots that entered a gunner seat (of `ai_boards`). OUTPUT only.
    pub ai_gunners: u64,
    /// Card VTAI: bolts bots fired from seat guns (of `ai_shots`). OUTPUT only.
    pub seat_shots: u64,
    /// Card WAI: bots that entered a WALKER's pilot seat (of `ai_boards`),
    /// and the centimetres walkers covered under a bot (XZ, summed per
    /// tick). OUTPUT only.
    pub ai_walker_rides: u64,
    pub ai_walker_cm: u64,
    /// Card WAI2: boarding claims bots made, those dropped on the give-up
    /// clock (a claimer that never reached its vehicle) and those lost to
    /// another bot (`ai::ride::board_goal`). OUTPUT only.
    pub ai_claims: u64,
    pub ai_claim_giveups: u64,
    pub ai_claim_lost: u64,
    /// Card TURR: turret offers made (`ai::man::offer_pass`), bots that sat
    /// down in a building turret, and per [`crate::ai::NavTick::man`] code
    /// the gunners that left by exit rule 1-5 ([1..=5]) and the claims whose
    /// walk did not reach the turret ([6]). OUTPUT only.
    pub man_offers: u64,
    /// ...of those, made on the defence clause alone. OUTPUT only.
    pub man_defence_offers: u64,
    pub man_boards: u64,
    pub man_codes: [u64; 7],
    /// Disperse plans that had to drop their exclusions because the map did not
    /// offer enough distinct objectives (brief D5(d)). OUTPUT only — a fact
    /// about a two-post map, not something to tune away.
    pub disperse_fallbacks: u64,
}

/// BF2's 10 m view-cone omni radius, or 0 for a BF1 map (RE §9.1: BF1's cone
/// test `FUN_002357d0` has NO omni branch; BF2's `FUN_002a7230` does). Every
/// site that builds a `Sim` from a baked scene sets `Sim::omni_radius` from
/// this and the manifest's `source_game`.
pub fn omni_radius_for(source_game: &str) -> f32 {
    if source_game.eq_ignore_ascii_case("bf1") {
        0.0
    } else {
        10.0
    }
}

impl Sim {
    /// The tier of team `team`'s units: its [`Sim::team_difficulty`] override,
    /// else the round's [`Sim::difficulty`] (a team past 1 has no override).
    pub fn difficulty_of(&self, team: u8) -> crate::skill::Difficulty {
        self.team_difficulty
            .get(usize::from(team))
            .copied()
            .flatten()
            .unwrap_or(self.difficulty)
    }

    /// The tier of whoever shoots AT a unit of team `team`: the other side's
    /// for teams 0 and 1, the round's for any other team.
    pub fn enemy_difficulty_of(&self, team: u8) -> crate::skill::Difficulty {
        match team {
            0 => self.difficulty_of(1),
            1 => self.difficulty_of(0),
            _ => self.difficulty,
        }
    }

    /// Empty world on flat ground: BF1 rifle as weapon class 0, tat1r ticket
    /// pools (250/250). Spawn units with [`Sim::spawn`].
    pub fn empty(seed: u32) -> Sim {
        Sim {
            tick: 0,
            difficulty: crate::skill::Difficulty::default(),
            team_difficulty: [None; 2],
            // BF2's omni; a BF1 map's builder sets 0.0 (RE §9.1).
            omni_radius: 10.0,
            ground_rules: crate::soldier::GroundRules::Bf2,
            human_damage_on: true,
            team_handicap_on: true,
            rng: SimRng::new(seed),
            // Independent generator; decorrelated seed so the two streams
            // don't start on the same word.
            lcg: GameLcg::new(seed ^ 0xA5A5_5A5A),
            heightfield: None,
            collision: CollisionWorld::default(),
            doors: Vec::new(),
            weapon_classes: vec![WeaponClass::bf1_rifle()],
            classes: [Vec::new(), Vec::new()],
            spawn_choice: Vec::new(),
            spawn_post_choice: Vec::new(),
            prev_spawn_bits: Vec::new(),
            spawn_screens: false,
            spawn_stage: Vec::new(),
            side_choice: Vec::new(),
            class_carousel: Vec::new(),
            spawn_points: Vec::new(),
            ai_spawned: [0, 0],
            units: Vec::new(),
            vehicles: Vec::new(),
            vehicle_pads: Vec::new(),
            hover_params: Vec::new(),
            walker_params: Vec::new(),
            seat_setup: Vec::new(),
            blast_setup: Vec::new(),
            hull_parts: Vec::new(),
            bolts: Vec::new(),
            stickies: Vec::new(),
            item_classes: Vec::new(),
            item_pool: super::pickups::ITEM_POOL_DEFAULT,
            pickups: Vec::new(),
            station_classes: Vec::new(),
            stations: Vec::new(),
            battle: Battle::new([250, 250]),
            cheats: Cheats::default(),
            friendly_fire_scale: 1.0,
            assist_intensity: 1.0,
            assist_armed: true,
            assist: Vec::new(),
            magnet: Vec::new(),
            reticle_pick: Vec::new(),
            events: Vec::new(),
            players: Vec::new(),
            cps: Vec::new(),
            domination: None,
            outcome: None,
            time_limit: None,
            respawn_delay: 4.0,
            waves: [SpawnWaves::mission_default(), SpawnWaves::mission_default()],
            ai_alive_cap: 16,
            death_regions: Vec::new(),
            water_level: None,
            hazards: Vec::new(),
            kill_floor: None,
            nav_floors: Vec::new(),
            spawn_seats: Vec::new(),
            nav_unwalkable: Vec::new(),
            nav_trapped: Vec::new(),
            positions: Default::default(),
            unit_grid: Default::default(),
            nav: None,
            flank: Default::default(),
            nav_stats: NavStats::default(),
            ai_wall_ms: 0.0,
            commander: [
                crate::commander::TeamPlan::new(0),
                crate::commander::TeamPlan::new(1),
            ],
        }
    }

    /// The P3 default scene: one player at the origin on flat ground (what
    /// `--sim-hash` / `--replay` run).
    pub fn new(seed: u32) -> Sim {
        let mut s = Sim::empty(seed);
        s.spawn(&UnitSpec::player(0, Vec3::ZERO));
        s
    }

    /// A grenade has landed at `pos` and goes off `fuse` ticks from now: bots
    /// within [`HAZARD_RADIUS`] are warned of it (card HOLD).
    pub fn add_hazard(&mut self, pos: Vec3, fuse: u64) {
        self.add_hazard_by(pos, fuse, None);
    }

    /// [`Sim::add_hazard`] for a grenade `owner` threw.
    pub fn add_hazard_by(&mut self, pos: Vec3, fuse: u64, owner: Option<UnitHandle>) {
        self.hazards.push(Hazard {
            pos,
            radius: HAZARD_RADIUS,
            since: self.tick,
            until: self.tick + fuse,
            owner,
        });
    }

    pub fn add_weapon_class(&mut self, wc: WeaponClass) -> usize {
        self.weapon_classes.push(wc);
        self.weapon_classes.len() - 1
    }

    pub fn unit(&self, h: UnitHandle) -> Option<&Unit> {
        self.units
            .get(h.index as usize)
            .filter(|u| u.gen == h.gen && u.alive)
    }

    pub fn unit_mut(&mut self, h: UnitHandle) -> Option<&mut Unit> {
        self.units
            .get_mut(h.index as usize)
            .filter(|u| u.gen == h.gen && u.alive)
    }

    pub fn handle(&self, index: usize) -> UnitHandle {
        UnitHandle {
            index: index as u32,
            gen: self.units[index].gen,
        }
    }

    /// Alive units with their handles.
    pub fn alive_units(&self) -> impl Iterator<Item = (UnitHandle, &Unit)> {
        self.units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive)
            .map(|(i, u)| {
                (
                    UnitHandle {
                        index: i as u32,
                        gen: u.gen,
                    },
                    u,
                )
            })
    }

    /// Ground height under `feet`: the highest walkable model surface between
    /// the probe and the terrain, else the terrain itself.
    ///
    /// The probe starts `STEP_HEIGHT` above the feet (the step a soldier climbs
    /// in one tick) and runs down to the terrain height; a model surface is
    /// taken only if it is inside that span, so a platform more than a step
    /// above the feet is ignored and one below the terrain never wins. With
    /// `rising` (the soldier is moving up) the probe starts AT the feet, so a
    /// jump does not mantle onto a ledge it has not cleared.
    ///
    /// A point inside a terrain cutter has NO terrain under it — the PS2's own
    /// height query returns its −300.0 sentinel there
    /// (analysis/re-terrain-heights.md, FUN_00473db0) — so the model floor of a
    /// cave or hangar becomes the only ground, and [`NO_GROUND`] the floor when
    /// there is not one.
    ///
    /// The terrain is also a ONE-SIDED sheet: feet more than [`TERRAIN_BURY`]
    /// below it are underground (a base or tunnel under an UNCUT heightfield —
    /// BF2 Polis Massa's `cp5con`/`cp6con`) and the terrain does not apply at
    /// all, so the model floor under them is the ground.
    ///
    /// PROVISIONAL: `STEP_HEIGHT`, `TERRAIN_BURY`, `WALKABLE_MIN_NY` and using
    /// −300.0 as a no-terrain floor are all ours (`core::collision`).
    pub fn ground_under(&self, feet: Vec3, rising: bool) -> f32 {
        self.ground_under_with(feet, rising, crate::collision::HullFilter::All)
    }

    /// [`Sim::ground_under`] standing on only the vehicle hulls `hulls`
    /// admits (a pad's spawn ray sees none, card VHULL).
    pub fn ground_under_with(
        &self,
        feet: Vec3,
        rising: bool,
        hulls: crate::collision::HullFilter,
    ) -> f32 {
        let probe = feet.y + if rising { 0.0 } else { STEP_HEIGHT };
        self.floor_under(feet, probe, hulls, true).0
    }

    /// [`Sim::ground_under`] as ONE ray, with no foot ring: what is under
    /// exactly this point. The AI's ledge probe asks this (`ai::steer`'s
    /// `floor_ahead`): a step is refused when the point ahead has no floor,
    /// though the foot would still hold the edge behind it.
    pub fn ground_point_under(&self, feet: Vec3, rising: bool) -> f32 {
        let probe = feet.y + if rising { 0.0 } else { STEP_HEIGHT };
        self.floor_under(feet, probe, crate::collision::HullFilter::All, false)
            .0
    }

    /// [`Sim::ground_under`] with the floor's normal: the model ray's, or the
    /// heightfield triangle's; up when there is no ground. The soldier's
    /// floor contact (card STHOP, `Soldier::resolve_ground`), asked AFTER the
    /// move: `from_y` is the feet's height before it, and the probe starts no
    /// lower, so a fall of more than a step in one tick (18 m/s, a 9 m drop)
    /// still finds the floor it crossed.
    pub fn ground_under_n(&self, feet: Vec3, rising: bool, from_y: f32) -> (f32, Vec3) {
        let probe = if rising {
            feet.y
        } else {
            (feet.y + STEP_HEIGHT).max(from_y)
        };
        self.floor_under(feet, probe, crate::collision::HullFilter::All, true)
    }

    /// [`Sim::ground_under_n`] as ONE ray, with no foot ring
    /// ([`Sim::ground_point_under`] with the floor's normal).
    pub fn ground_point_under_n(&self, feet: Vec3, from_y: f32) -> (f32, Vec3) {
        let probe = (feet.y + STEP_HEIGHT).max(from_y);
        self.floor_under(feet, probe, crate::collision::HullFilter::All, false)
    }

    /// [`Sim::ground_under_n`] with the probe only
    /// [`crate::collision::AIR_STEP`] over the feet: what a soldier coming
    /// DOWN in the air asks when the whole step found a face he cannot stand
    /// on (card JARC, `Sim::step` 3a').
    pub fn ground_under_falling(&self, feet: Vec3, from_y: f32) -> (f32, Vec3) {
        let probe = (feet.y + crate::collision::AIR_STEP).max(from_y);
        self.floor_under(feet, probe, crate::collision::HullFilter::All, true)
    }

    /// The floor under the point `p` for ONE down ray from no lower than
    /// `from_y`: where a falling item lands (card PWUP).
    pub(super) fn floor_point_under(&self, p: Vec3, from_y: f32) -> f32 {
        let probe = (p.y + STEP_HEIGHT).max(from_y);
        self.floor_under(p, probe, crate::collision::HullFilter::All, false)
            .0
    }

    /// The floor under the point `p` for ONE down ray from `from_y`, blind
    /// to the vehicles' hulls: a pad's spawn ray (card VREST).
    pub(super) fn floor_point_under_static(&self, p: Vec3, from_y: f32) -> f32 {
        self.floor_under(p, from_y, crate::collision::HullFilter::Static, false)
            .0
    }

    /// The floor under `feet` for a down ray starting at height `probe`;
    /// `ring` = the soldier's foot (the ring of rays), not a bare point.
    fn floor_under(
        &self,
        feet: Vec3,
        probe: f32,
        hulls: crate::collision::HullFilter,
        ring: bool,
    ) -> (f32, Vec3) {
        let mut terrain = self
            .heightfield
            .as_ref()
            .map(|h| h.height_at(feet.x, feet.z));
        if terrain.is_some() && self.collision.is_cut(feet) {
            terrain = None;
        }
        // Buried: the sheet is above the feet by more than a bank climb.
        if terrain.is_some_and(|t| t > feet.y + TERRAIN_BURY) {
            terrain = None;
        }
        let floor = terrain.unwrap_or(NO_GROUND);
        if probe > floor {
            // y >= floor by construction of the ray length.
            let down = |x: f32, z: f32| {
                self.collision
                    .ray_down_with(Vec3::new(x, probe, z), probe - floor, hulls)
            };
            let centre = down(feet.x, feet.z);
            // The PS2's foot is a sphere, not a point (`coll_body_lower`,
            // r 0.57 — re-soldier-ground.md §1.2): it bridges the slits
            // between collision floor pieces that a point falls through
            // (KAM1's cloning rooms: the PS2 soldier stands at 69.272 on a
            // slit, the point foot found the hidden disc 7.9 m down — card
            // GRND, re-kamino.md §4.3). When the centre ray finds no floor
            // within a step under the feet, a ring of rays inside the
            // sphere's reach looks for one, each hit lowered by how far the
            // sphere's underside is over the feet at that radius.
            // The terrain within a step is a floor too: no ring on open ground.
            let near = |h: &(f32, Vec3)| h.0 >= feet.y - STEP_HEIGHT;
            let terrain_near = terrain.is_some_and(|t| t >= feet.y - STEP_HEIGHT);
            if ring && !centre.as_ref().is_some_and(near) && !terrain_near {
                let lift = FOOT_RADIUS - (FOOT_RADIUS * FOOT_RADIUS - FOOT_RING * FOOT_RING).sqrt();
                let mut best = centre;
                for (dx, dz) in FOOT_RING_DIRS {
                    if let Some((y, n)) = down(feet.x + dx * FOOT_RING, feet.z + dz * FOOT_RING) {
                        let y = y - lift;
                        if best.is_none_or(|(b, _)| y > b) {
                            best = Some((y, n));
                        }
                    }
                }
                if let Some(hit) = best {
                    return hit;
                }
            } else if let Some(hit) = centre {
                // The same foot on a short STEEP face (card RAIL). The centre
                // ray stands the soldier on the bevel of a kerb or a rail — a
                // floor to the PS2 too (n.y > 0.6), but one the stick has no
                // hold on (under n.y 0.7 / 0.75, `GroundRules::climb_min_ny`),
                // so the point foot had to climb the whole face on momentum:
                // BF2 Kamino's rim rail (0.5 m up a 53° bevel) let a soldier
                // over at 7 m/s head on and held him at 6.5 m/s, at 45°, and
                // from a stand — the maintainer's "invisible wall". The PS2's foot
                // sphere (r 0.57) rides the bevel's top edge instead. So when
                // the floor under the centre gives no hold, a ring ray that
                // finds one that does, higher and within a step of the feet,
                // is the floor: the top of the kerb, less the sphere's sag.
                if ring && hit.1.y < self.ground_rules.climb_min_ny() && near(&hit) {
                    let lift =
                        FOOT_RADIUS - (FOOT_RADIUS * FOOT_RADIUS - FOOT_RING * FOOT_RING).sqrt();
                    // The highest such surface wins; the feet never go
                    // under the face the centre found.
                    let mut best: Option<(f32, Vec3)> = None;
                    for (dx, dz) in FOOT_RING_DIRS {
                        if let Some((y, n)) = down(feet.x + dx * FOOT_RING, feet.z + dz * FOOT_RING)
                        {
                            if n.y >= self.ground_rules.climb_min_ny()
                                && y > hit.0
                                && best.is_none_or(|(b, _)| y > b)
                            {
                                best = Some((y, n));
                            }
                        }
                    }
                    return match best {
                        Some((y, n)) => ((y - lift).max(hit.0), n),
                        None => hit,
                    };
                }
                return hit;
            }
        }
        match (terrain, self.heightfield.as_ref()) {
            (Some(t), Some(h)) => (t, Vec3::from(h.normal_at(feet.x, feet.z))),
            _ => (floor, Vec3::Y),
        }
    }

    /// The map's terrain ray: heightfield + cutters bundled, so every caller
    /// (AI line of sight, bolt spawn rays, the round diagnostics) crosses the
    /// same sheet. `None` when the map has no heightfield, or when the ray
    /// never crosses the surface within `max_t`. See
    /// [`crate::collision::ray_heightfield`].
    pub fn terrain_ray(&self, origin: Vec3, dir: Vec3, max_t: f32, step: f32) -> Option<f32> {
        let hf = self.heightfield.as_ref()?;
        crate::collision::ray_heightfield(hf, &self.collision.cutters, origin, dir, max_t, step)
    }

    /// FNV-1a-64 over the sim state, bit-exact.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for &b in bytes {
                h ^= b as u64;
                h = h.wrapping_mul(0x1_0000_01B3);
            }
        };
        eat(&self.tick.to_le_bytes());
        eat(&self.difficulty.bytes());
        if self.team_difficulty.iter().any(Option::is_some) {
            for d in self.team_difficulty {
                eat(&d.map_or([0xff], crate::skill::Difficulty::bytes));
            }
        }
        eat(&self.rng.state().to_le_bytes());
        eat(&self.lcg.state().to_le_bytes());
        for u in &self.units {
            u.hash_into(&mut eat);
        }
        // The vehicles (VENT): nothing at all when there are none, like the
        // doors, so the replay values stay put until a vehicle exists.
        if !self.vehicles.is_empty() {
            eat(&(self.vehicles.len() as u32).to_le_bytes());
            for v in &self.vehicles {
                v.hash_into(&mut eat);
            }
        }
        for b in &self.bolts {
            for v in [b.pos.x, b.pos.y, b.pos.z, b.remaining, b.age] {
                eat(&v.to_bits().to_le_bytes());
            }
        }
        for g in &self.stickies {
            g.hash_into(&mut eat);
        }
        for t in &self.battle.teams {
            eat(&t.reinforcements.to_bits().to_le_bytes());
            eat(&t.alive_players.to_le_bytes());
            eat(&t.alive_ai.to_le_bytes());
        }
        for w in &self.waves {
            eat(&w.timer.to_bits().to_le_bytes());
            eat(&w.wave.to_le_bytes());
            eat(&w.next_slot.to_le_bytes());
        }
        // The two commander plans (Phase B), each length-prefixed with `None`
        // as its own sentinel byte.
        for p in &self.commander {
            p.hash_into(&mut eat);
        }
        // The per-slot magnet table and the player slot table, both
        // length-prefixed with `None` as (u32::MAX, u32::MAX): a magnet held
        // by slot 0 and the same magnet held by slot 1 are different states
        // and must not hash alike (they did while the magnets were a
        // flattened list of `Some`s).
        let handle_or_none =
            |h: Option<UnitHandle>| h.map_or((u32::MAX, u32::MAX), |h| (h.index, h.gen));
        // A vehicle magnet (card HTGT) carries a marker byte first, so a
        // unit magnet hashes as it did before vehicles could be one.
        eat(&(self.magnet.len() as u32).to_le_bytes());
        for m in &self.magnet {
            let (i, g) = match *m {
                Some(super::input::AimTarget::Vehicle(h)) => {
                    eat(&[0x56]);
                    (h.index, h.gen)
                }
                m => handle_or_none(m.and_then(super::input::AimTarget::unit)),
            };
            eat(&i.to_le_bytes());
            eat(&g.to_le_bytes());
        }
        eat(&(self.players.len() as u32).to_le_bytes());
        for p in &self.players {
            let (i, g) = handle_or_none(*p);
            eat(&i.to_le_bytes());
            eat(&g.to_le_bytes());
        }
        // The spawn-select state, both per-slot tables length-prefixed like
        // the two above (D9: every new piece of STATE is hashed; the class
        // TABLES themselves are setup data and are not).
        eat(&(self.spawn_choice.len() as u32).to_le_bytes());
        for c in &self.spawn_choice {
            eat(&[*c]);
        }
        eat(&(self.prev_spawn_bits.len() as u32).to_le_bytes());
        for b in &self.prev_spawn_bits {
            eat(&b.to_le_bytes());
        }
        // The spawn map's post picks: NOTHING while no slot has one (card
        // SPSEL; the pinned `--sim-hash 100` and every idle-player gate keep
        // their values), else a tag, the length and each slot's pick.
        if self.spawn_post_choice.iter().any(Option::is_some) {
            eat(b"SPSEL");
            eat(&(self.spawn_post_choice.len() as u32).to_le_bytes());
            for c in &self.spawn_post_choice {
                match c {
                    Some(p) => p.hash_into(&mut eat),
                    None => eat(&[0]),
                }
            }
        }
        // The spawn screens (card SSELA): NOTHING while they are off and every
        // slot holds the defaults, else a tag, the switch and each slot's
        // stage and side row.
        if self.spawn_screens
            || self
                .spawn_stage
                .iter()
                .any(|&s| s != super::spawn::SpawnStage::Accepted)
            || self
                .side_choice
                .iter()
                .any(|&c| c != super::spawn::SideChoice::Team1)
        {
            eat(b"SSELA");
            eat(&[u8::from(self.spawn_screens)]);
            eat(&(self.spawn_stage.len() as u32).to_le_bytes());
            for (st, side) in self.spawn_stage.iter().zip(&self.side_choice) {
                eat(&[*st as u8, *side as u8]);
            }
        }
        // The class carousel (card SSELB): NOTHING while every slot's is idle.
        if self
            .class_carousel
            .iter()
            .any(|c| *c != super::spawn::Carousel::default())
        {
            eat(b"SSELB");
            eat(&(self.class_carousel.len() as u32).to_le_bytes());
            for c in &self.class_carousel {
                eat(&[u8::from(c.armed), c.slide]);
            }
        }
        // The AI fill cursor: which class the next AI of each team takes.
        for n in &self.ai_spawned {
            eat(&n.to_le_bytes());
        }
        for c in &self.cps {
            c.hash_into(&mut eat);
        }
        if let Some((t, secs)) = self.domination {
            eat(&[t]);
            eat(&secs.to_bits().to_le_bytes());
        }
        eat(&[self.outcome.map_or(255, |w| w)]);
        // The doors (DR2): nothing at all when there are none.
        if !self.doors.is_empty() {
            eat(&(self.doors.len() as u32).to_le_bytes());
            for d in &self.doors {
                eat(&d.open.to_bits().to_le_bytes());
            }
        }
        // The grenades on the ground (HOLD): nothing when there are none.
        if !self.hazards.is_empty() {
            eat(&(self.hazards.len() as u32).to_le_bytes());
            for z in &self.hazards {
                for v in [z.pos.x, z.pos.y, z.pos.z, z.radius] {
                    eat(&v.to_bits().to_le_bytes());
                }
                eat(&z.since.to_le_bytes());
                eat(&z.until.to_le_bytes());
                if let Some(o) = z.owner {
                    eat(&o.index.to_le_bytes());
                }
            }
        }
        // The vehicle pads (VPAD): nothing at all when there are none.
        if !self.vehicle_pads.is_empty() {
            eat(&(self.vehicle_pads.len() as u32).to_le_bytes());
            for p in &self.vehicle_pads {
                p.hash_into(&mut eat);
            }
        }
        // The items on the field (PWUP): nothing at all when there are none.
        if !self.pickups.is_empty() {
            eat(&(self.pickups.len() as u32).to_le_bytes());
            for p in &self.pickups {
                p.hash_into(&mut eat);
            }
        }
        // The stations (PWST): nothing at all when there are none.
        if !self.stations.is_empty() {
            eat(&(self.stations.len() as u32).to_le_bytes());
            for s in &self.stations {
                s.hash_into(&mut eat);
            }
        }
        h
    }

    /// Card MOUTH2: is `body` a leaf of a door that is OPENING — a living
    /// unit stands in one of its triggers, so its next tick slides it on, and
    /// its clip is not at its end yet (`crate::ai::steer`'s `stop_ahead`
    /// asks, of the body its sweep met)? A door that is fully open, or one
    /// with nobody in its triggers, is whatever its bodies are. The triggers
    /// are asked of the units as they stand now, [`Self::tick_doors`]' own
    /// test: the answer is a function of hashed state (the sound latch is
    /// not).
    pub fn door_opening(&self, body: &crate::collision::Body) -> bool {
        let bodies = self.collision.bodies();
        self.doors.iter().any(|d| {
            d.bodies
                .iter()
                .any(|k| bodies.get(k.body).is_some_and(|b| std::ptr::eq(b, body)))
                && self.opening(d)
        })
    }

    /// [`Self::door_opening`]'s test of one door: its clip not at its end
    /// and a living unit in one of its triggers.
    fn opening(&self, d: &crate::door::Door) -> bool {
        d.open < 1.0 && d.occupied(self.units.iter().filter(|u| u.alive).map(|u| u.soldier.pos))
    }

    /// Card DOORS: does the vertical line through `p` pass through the box
    /// of a leaf of a door that is OPENING ([`Self::door_opening`]'s test)?
    /// `crate::ai::steer`'s `stop_ahead` asks it of the point its
    /// steep-floor probe comes down at: a face found there is the leaf's,
    /// and the leaf is leaving. The box is the leaf's as it stands this tick
    /// (a rolled leaf's box is wider than the leaf; the line through it still
    /// meets the leaf wherever the box is as tall as the leaf's own span).
    pub fn opening_leaf_over(&self, p: Vec3) -> bool {
        let bodies = self.collision.bodies();
        self.doors.iter().any(|d| {
            d.bodies.iter().any(|k| {
                bodies.get(k.body).is_some_and(|b| {
                    (b.aabb.min.x..=b.aabb.max.x).contains(&p.x)
                        && (b.aabb.min.z..=b.aabb.max.z).contains(&p.z)
                })
            }) && self.opening(d)
        })
    }

    /// One tick of every door (`crate::door`): occupied by any LIVE unit's
    /// feet, the parameter moves, a door that moved moves its bodies, and a
    /// trigger edge pushes its sound event (not hashed, like every event).
    pub(crate) fn tick_doors(&mut self, dt: f32) {
        if self.doors.is_empty() {
            return;
        }
        let feet: Vec<Vec3> = self
            .units
            .iter()
            .filter(|u| u.alive)
            .map(|u| u.soldier.pos)
            .collect();
        for (i, d) in self.doors.iter_mut().enumerate() {
            let occupied = d.occupied(feet.iter().copied());
            if d.advance(occupied, dt) {
                for k in 0..d.bodies.len() {
                    self.collision.move_body(d.bodies[k].body, d.pose(k));
                }
            }
            // The sound latch (DRV): an output, like every event.
            let door = i as u32;
            match d.edge(occupied) {
                Some(crate::door::Edge::Opened) => self.events.push(Event::DoorOpened { door }),
                Some(crate::door::Edge::Closed) => self.events.push(Event::DoorClosed { door }),
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::flat_ground;
    use crate::sim::PadState;
    use crate::unit::Controller;

    /// DR2: a unit standing in a door's trigger opens it through
    /// `Sim::step` — the parameter reaches 1.0 in the clip's (frames−1)/30 s
    /// and the door's body rides to the last frame — and nobody in range
    /// leaves it shut. The door's parameter is in the hash; its sound latch
    /// and the `DoorOpened` event (DRV) are not.
    #[test]
    fn a_unit_in_the_trigger_opens_the_door_and_the_door_is_hashed() {
        use crate::collision::{Aabb, Body, CollisionWorld, Shape};
        use crate::door::{Door, DoorBody, Trigger};
        use glam::{Mat4, Quat};
        let make = |unit_at: Vec3| {
            let mut s = Sim::empty(4);
            s.heightfield = Some(flat_ground());
            let shut = Vec3::new(0.0, 2.0, -3.0);
            let body = Body::new(
                Mat4::from_translation(shut),
                Shape::Box {
                    half: Vec3::new(1.0, 2.0, 0.2),
                },
            );
            let mut env = Aabb::empty();
            env.add(Vec3::new(-1.0, 0.0, -3.2));
            env.add(Vec3::new(1.0, 7.0, -2.8));
            s.collision = CollisionWorld::with_movers(vec![body], Vec::new(), vec![(0, env)]);
            s.doors = vec![Door {
                triggers: vec![Trigger {
                    center: Vec3::new(0.0, 0.0, -3.0),
                    radius: 4.0,
                }],
                frames: 31,
                bodies: vec![DoorBody {
                    body: 0,
                    frames: (0..31)
                        .map(|f| (Quat::IDENTITY, shut + Vec3::Y * (3.0 * f as f32 / 30.0)))
                        .collect(),
                }],
                ..Door::default()
            }];
            s.spawn(&UnitSpec::player_in_slot(0, 0, unit_at)).unwrap();
            s
        };
        let mut near = make(Vec3::new(0.0, 0.0, 0.0));
        let mut far = make(Vec3::new(0.0, 0.0, 20.0));
        let opened = |s: &Sim| {
            s.events
                .iter()
                .filter(|e| matches!(e, Event::DoorOpened { door: 0 }))
                .count()
        };
        for i in 0..30 {
            near.step(&[PadState::default()]);
            far.step(&[PadState::default()]);
            // DRV: the open sound's event on the first occupied tick only.
            assert_eq!(opened(&near), usize::from(i == 0), "tick {i}");
            assert_eq!(opened(&far), 0);
        }
        assert_eq!(near.doors[0].open, 1.0, "opened in 30 ticks = 1.00 s");
        assert!(
            near.doors[0].latch && !far.doors[0].latch,
            "only the occupied door latched"
        );
        assert_eq!(far.doors[0].open, 0.0, "nobody in range: shut");
        let y = |s: &Sim| s.collision.bodies()[0].to_world.w_axis.y;
        assert!((y(&near) - 5.0).abs() < 1e-5, "the box rode the clip");
        assert_eq!(y(&far), 2.0);
        // Same everything but the door's parameter: a different hash.
        let mut shut = make(Vec3::new(0.0, 0.0, 0.0));
        let open = make(Vec3::new(0.0, 0.0, 0.0));
        shut.doors[0].open = 0.5;
        assert_ne!(shut.hash(), open.hash());
        // The sound latch is output-side: not hashed.
        let mut latched = make(Vec3::new(0.0, 0.0, 0.0));
        latched.doors[0].latch = true;
        assert_eq!(latched.hash(), open.hash());
        // And no door at all hashes exactly as before doors existed: the
        // field eats nothing when it is empty.
        let mut none = make(Vec3::ZERO);
        let before = none.hash();
        none.doors.clear();
        assert_ne!(none.hash(), before, "a door list is hashed");
        let mut bare = Sim::empty(4);
        bare.heightfield = Some(flat_ground());
        bare.spawn(&UnitSpec::player_in_slot(0, 0, Vec3::ZERO))
            .unwrap();
        assert_eq!(bare.hash(), none.hash(), "an empty list eats nothing");
    }

    /// Card MOUTH2: a door is opening while a living unit stands in one of
    /// its triggers and its clip is not at its end — and only its own leaves
    /// answer.
    #[test]
    fn a_door_is_opening_while_a_unit_is_in_its_trigger_and_its_clip_runs() {
        use crate::collision::{Body, CollisionWorld, Shape};
        use crate::door::{Door, DoorBody, Trigger};
        use glam::{Mat4, Quat};
        let make = |unit_at: Vec3| {
            let mut s = Sim::empty(4);
            s.heightfield = Some(flat_ground());
            let slab = |x: f32| {
                Body::new(
                    Mat4::from_translation(Vec3::new(x, 2.0, -3.0)),
                    Shape::Box {
                        half: Vec3::new(1.0, 2.0, 0.2),
                    },
                )
            };
            s.collision = CollisionWorld::new(vec![slab(0.0), slab(10.0)], Vec::new());
            s.doors = vec![Door {
                triggers: vec![Trigger {
                    center: Vec3::new(0.0, 0.0, -3.0),
                    radius: 4.0,
                }],
                frames: 31,
                bodies: vec![DoorBody {
                    body: 0,
                    frames: vec![(Quat::IDENTITY, Vec3::new(0.0, 2.0, -3.0)); 31],
                }],
                ..Door::default()
            }];
            s.spawn(&UnitSpec::player_in_slot(0, 0, unit_at)).unwrap();
            s
        };
        let leaf = |s: &Sim| s.door_opening(&s.collision.bodies()[0]);
        let mut s = make(Vec3::new(0.0, 0.0, 0.5));
        assert!(leaf(&s), "shut, and a unit stands in the trigger");
        assert!(
            !s.door_opening(&s.collision.bodies()[1]),
            "a body that is no leaf of it"
        );
        s.doors[0].open = 0.97;
        assert!(leaf(&s), "still sliding");
        s.doors[0].open = 1.0;
        assert!(!leaf(&s), "the clip is at its end: what is left is a wall");
        s.doors[0].open = 0.5;
        s.units[0].alive = false;
        assert!(!leaf(&s), "a dead unit holds no door");
        assert!(
            !leaf(&make(Vec3::new(0.0, 0.0, 1.5))),
            "nobody within the trigger's 4 m: a wall"
        );

        // Card DOORS: the line through a point, against the leaf's box (x
        // -1 .. 1, z -3.2 .. -2.8) - at any height, and only while the door
        // is opening; the slab at x 10 is no door's.
        let mut s = make(Vec3::new(0.0, 0.0, 0.5));
        let over = |s: &Sim, x: f32, y: f32, z: f32| s.opening_leaf_over(Vec3::new(x, y, z));
        assert!(over(&s, 0.9, 0.0, -3.1) && over(&s, -0.9, 9.0, -2.9));
        assert!(!over(&s, 1.1, 1.0, -3.0), "beside the leaf");
        assert!(!over(&s, 0.0, 1.0, -2.7), "in front of it");
        assert!(!over(&s, 10.0, 1.0, -3.0), "a body that is no leaf");
        s.doors[0].open = 1.0;
        assert!(!over(&s, 0.0, 1.0, -3.0), "the clip is at its end");
        s.doors[0].open = 0.5;
        s.units[0].alive = false;
        assert!(!over(&s, 0.0, 1.0, -3.0), "a dead unit holds no door");
        assert!(
            !over(&make(Vec3::new(0.0, 0.0, 1.5)), 0.0, 1.0, -3.0),
            "nobody in the trigger"
        );
    }

    /// A magnet held by slot 0 and the same magnet held by slot 1 are
    /// different states: both slot tables are length-prefixed in the hash.
    #[test]
    fn magnet_slot_is_part_of_the_hash() {
        let mut a = Sim::empty(4);
        let mut b = Sim::empty(4);
        for s in [&mut a, &mut b] {
            s.heightfield = Some(flat_ground());
            s.spawn(&UnitSpec::player_in_slot(0, 0, Vec3::ZERO))
                .unwrap();
            s.spawn(&UnitSpec::player_in_slot(1, 0, Vec3::new(20.0, 0.0, 0.0)))
                .unwrap();
        }
        let t = a
            .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -30.0)))
            .unwrap();
        let tb = b
            .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -30.0)))
            .unwrap();
        assert_eq!(t, tb);
        assert_eq!(a.hash(), b.hash());
        let t = crate::sim::AimTarget::Unit(t);
        a.magnet[0] = Some(t);
        b.magnet[1] = Some(t);
        assert_ne!(a.hash(), b.hash(), "which slot holds the magnet is hashed");
        a.magnet[0] = None;
        a.magnet[1] = Some(t);
        assert_eq!(a.hash(), b.hash());
    }

    /// Phase D D1: the squad order is per-unit hashed state — and a unit with
    /// NO order costs the hash nothing, which is the property the whole phase
    /// leans on (brief addendum 3).
    #[test]
    fn a_squad_command_is_part_of_the_hash_and_no_command_costs_nothing() {
        use crate::squad::{Command, CommandKind};
        // The 655566562afc1358 literal: `Sim::new(42)` stepped 100 idle ticks, which is
        // exactly what `game --sim-hash 100` runs. Measured at HEAD before this
        // commit (`phase-d` 1aa9651, and the same value at 96814de) and pinned
        // HERE so that adding a field to `Unit` can never move it unnoticed:
        // `Unit::hash_into` eats nothing at all for a unit with no command, so
        // a command-free sim hashes byte-for-byte as it did before Phase D.
        // If this fails, the hash FORMAT moved and every replay gate value with
        // it — that is a lead decision (PLAN B1), not a test to update.
        // Card STHOP (2026-09-30) moved it on purpose, 17e1b946ff0e821c ->
        // ba980b4058ec387f: `Soldier::hash_into` now eats the ground normal
        // and the airtime, state the next tick's motion depends on.
        // Card CONT (2026-10-02) moved it on purpose, ba980b4058ec387f ->
        // 4bf3989459aaec77: `TeamPlan::hash_into` now eats each slot's
        // generation and at-post clock and the posts' owners at the last
        // plan, the state a kept target's next draw depends on.
        // Card JITR (2026-10-02) moved it on purpose, 4bf3989459aaec77 ->
        // dc6414f430a49ed7: `AiState::hash_into` now eats the steering's
        // remembered slide side and the tick that memory ends.
        // Card AIMA (2026-10-02) moved it on purpose, dc6414f430a49ed7 ->
        // f5046a190fcf7eda: the state, not the format - a human spawns at
        // the PS2's pitch -0.07 (with a pitch of 0 the old value returns).
        // Card JGATE (2026-10-05) moved it on purpose, f5046a190fcf7eda ->
        // e20eb63fc9d796e6: `Soldier::hash_into` now eats the stand timer
        // (`stand_age`), which gates BF1's jump and its fall.
        // Card SPRD (2026-10-06) moved it on purpose, e20eb63fc9d796e6 ->
        // 76799f150370b936: the state, not the format - a weapon's spread
        // rests `tight_hold` under the PS2's floor (with both
        // `TIGHT_HOLD_SECS` at 0 the old value returns).
        // Card OBST (2026-10-06) moved it on purpose, 76799f150370b936 ->
        // 655566562afc1358: `AiState::hash_into` no longer eats the slide
        // memory card JITR added (`slide_last`, `slide_keep`: deleted, the
        // side is a commitment in `MoverState::round` now).
        // Card CLSE2 (2026-10-12) moved it on purpose, 655566562afc1358 ->
        // 9f5f669ed7142854: the format - a unit's hash eats its `AimBlock` (12
        // floats: BF2's aim error is the target's own block).
        // Card DEFS (2026-10-12) moved it on purpose, 9f5f669ed7142854 ->
        // 301ed5aeda7aae76: the format - a team's plan hashes its defenders'
        // spots and each post's last threat (`TeamPlan::spots`, `threat`).
        // Card SKIRT (2026-10-15) moved it on purpose, 301ed5aeda7aae76 ->
        // 5affffe147952dc0: the format - a soldier's hash eats what is left of
        // the floor overlap and whether last tick kept the flatter normal.
        // Card KNOCK (2026-10-21) moved it on purpose, 5affffe147952dc0 ->
        // f9f37400e0f3171e: the format - a soldier's hash eats its landing
        // draw and the get-up's fire lock (`knock_draw`, `fire_lock`).
        let mut sim = Sim::new(42);
        for _ in 0..100 {
            sim.step(&[PadState::default()]);
        }
        assert_eq!(
            format!("{:016x}", sim.hash()),
            "f9f37400e0f3171e",
            "the command-free hash format moved"
        );

        let mut a = Sim::empty(4);
        let mut b = Sim::empty(4);
        for s in [&mut a, &mut b] {
            s.heightfield = Some(flat_ground());
            s.spawn(&UnitSpec::player_in_slot(0, 0, Vec3::ZERO))
                .unwrap();
            s.spawn(&UnitSpec::trooper(0, Vec3::new(5.0, 0.0, 0.0)))
                .unwrap();
        }
        assert_eq!(a.hash(), b.hash());
        let leader = a.players[0].expect("player");
        let cmd = |kind, issuer, issued_tick| {
            Some(Command {
                kind,
                issuer,
                issued_tick,
            })
        };
        a.units[1].command = cmd(CommandKind::Follow(leader), 0, 0);
        assert_ne!(a.hash(), b.hash(), "having an order is hashed");
        // ... and every field of it: the kind, the payload, the slot, the tick.
        b.units[1].command = cmd(CommandKind::Hold(Vec3::ZERO), 0, 0);
        assert_ne!(a.hash(), b.hash(), "the kind is hashed");
        b.units[1].command = cmd(CommandKind::Follow(leader), 1, 0);
        assert_ne!(a.hash(), b.hash(), "the issuer slot is hashed");
        b.units[1].command = cmd(CommandKind::Follow(leader), 0, 7);
        assert_ne!(a.hash(), b.hash(), "the issue tick is hashed");
        b.units[1].command = cmd(CommandKind::Follow(leader), 0, 0);
        assert_eq!(a.hash(), b.hash());
        // WHICH unit carries the order is hashed too (the order is not a
        // sim-wide fact): move it to the player and the sims differ.
        b.units[1].command = None;
        b.units[0].command = cmd(CommandKind::Follow(leader), 0, 0);
        assert_ne!(a.hash(), b.hash(), "whose order it is, is hashed");
    }

    #[test]
    fn the_difficulty_and_every_bots_skill_are_part_of_the_hash() {
        // Phase C D2: `Sim::difficulty` is SIM STATE (one byte after `tick`)
        // and `Unit.skill` is per unit. Two sims that differ only in the tier
        // must not hash alike, and the skill must reach the unit at spawn.
        let build = |d: crate::skill::Difficulty| {
            let mut s = Sim::empty(4);
            s.difficulty = d;
            s.heightfield = Some(flat_ground());
            let p = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
            let mut ai = UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -30.0));
            ai.controller = Controller::Ai;
            let a = s.spawn(&ai).unwrap();
            (s, p, a)
        };
        let (norm, p, a) = build(crate::skill::Difficulty::Normal);
        let (elite, _, _) = build(crate::skill::Difficulty::Elite);
        assert_ne!(norm.hash(), elite.hash(), "the tier byte is hashed");
        // Every bot receives the tier's skill; a player keeps 0.
        assert_eq!(norm.unit(a).unwrap().skill, 17);
        assert_eq!(elite.unit(a).unwrap().skill, 20);
        assert_eq!(norm.unit(p).unwrap().skill, 0, "a player is not a bot");
        // The spec carries it too, so a rebuild starts from the same place.
        assert_eq!(norm.unit(a).unwrap().spec.skill, 17);
        // `Unit.skill` alone moves the hash (same tier byte, hand-edited).
        let mut hacked = build(crate::skill::Difficulty::Normal).0;
        hacked.units[a.index as usize].skill = 3;
        assert_ne!(hacked.hash(), norm.hash(), "Unit.skill is hashed");
    }

    #[test]
    fn a_team_tier_reaches_that_teams_bots_and_only_then_moves_the_hash() {
        // Card ARVS: `team_difficulty` unset is the sim it was (same hash);
        // set, each team's bots take their own tier's skill at spawn.
        use crate::skill::Difficulty as D;
        let build = |td: [Option<D>; 2]| {
            let mut s = Sim::empty(4);
            s.team_difficulty = td;
            s.heightfield = Some(flat_ground());
            let mut hs = Vec::new();
            for team in [0u8, 1] {
                let mut ai = UnitSpec::trooper(team, Vec3::new(f32::from(team) * 5.0, 0.0, 0.0));
                ai.controller = Controller::Ai;
                hs.push(s.spawn(&ai).unwrap());
            }
            (s, hs)
        };
        let (plain, _) = build([None; 2]);
        let base = {
            let mut s = Sim::empty(4);
            s.heightfield = Some(flat_ground());
            for team in [0u8, 1] {
                let mut ai = UnitSpec::trooper(team, Vec3::new(f32::from(team) * 5.0, 0.0, 0.0));
                ai.controller = Controller::Ai;
                s.spawn(&ai).unwrap();
            }
            s
        };
        assert_eq!(plain.hash(), base.hash(), "no override: the old hash");
        let (split, hs) = build([Some(D::Easy), Some(D::Elite)]);
        assert_ne!(split.hash(), plain.hash(), "an override is hashed");
        assert_eq!(split.unit(hs[0]).unwrap().skill, 8, "team 0 at Easy");
        assert_eq!(split.unit(hs[1]).unwrap().skill, 20, "team 1 at Elite");
        assert_eq!(split.difficulty_of(0), D::Easy);
        assert_eq!(
            split.difficulty_of(2),
            D::Normal,
            "a third team: the round's"
        );
        assert_eq!(split.enemy_difficulty_of(0), D::Elite);
        assert_eq!(split.enemy_difficulty_of(1), D::Easy);
    }

    #[test]
    fn a_bf1_map_has_no_view_cone_omni() {
        // RE §9.1: BF1's cone test has no 10 m omni branch, BF2's does.
        assert_eq!(crate::sim::omni_radius_for("bf1"), 0.0);
        assert_eq!(crate::sim::omni_radius_for("BF1"), 0.0);
        assert_eq!(crate::sim::omni_radius_for("bf2"), 10.0);
        // `omni_radius` is an INPUT, like `nav`: it must NOT move the hash.
        let mut a = Sim::empty(4);
        let mut b = Sim::empty(4);
        a.omni_radius = 0.0;
        b.omni_radius = 10.0;
        assert_eq!(a.hash(), b.hash(), "omni_radius is an input, not state");
        // So are the two host-option gates.
        a.human_damage_on = false;
        a.team_handicap_on = false;
        assert_eq!(a.hash(), b.hash(), "the host options are inputs");
        assert_eq!(Sim::empty(1).difficulty, crate::skill::Difficulty::Normal);
        assert_eq!(Sim::empty(1).omni_radius, 10.0);
    }

    /// The wall pass's recorded push is part of the sim hash: the AI reads it
    /// as its contact cue next tick, so a build that lost it would replay
    /// differently and nothing else would say so (PLAN "PHASE A.2 DESIGN"
    /// decision 2).
    #[test]
    fn wall_push_is_part_of_the_hash() {
        let mut a = Sim::empty(4);
        let mut b = Sim::empty(4);
        for s in [&mut a, &mut b] {
            s.heightfield = Some(flat_ground());
            s.spawn(&UnitSpec::trooper(0, Vec3::ZERO)).unwrap();
        }
        assert_eq!(a.hash(), b.hash());
        a.units[0].wall_push = Vec3::new(0.25, 0.0, 0.0);
        assert_ne!(a.hash(), b.hash(), "the contact accumulator is hashed");
        b.units[0].wall_push = Vec3::new(0.25, 0.0, 0.0);
        assert_eq!(a.hash(), b.hash());
        // ... and the wall pass is what writes it: a unit walked into a wall
        // is pushed out AND records the push, with the push equal to the
        // correction its position actually took.
        let mut s = Sim::empty(4);
        s.heightfield = Some(flat_ground());
        s.collision = CollisionWorld::new(
            vec![crate::collision::Body::new(
                glam::Mat4::from_translation(Vec3::new(0.2, 1.0, 0.0)),
                crate::collision::Shape::Box {
                    half: Vec3::new(0.5, 5.0, 5.0),
                },
            )],
            Vec::new(),
        );
        let h = s.spawn(&UnitSpec::trooper(0, Vec3::ZERO)).unwrap();
        let before = s.unit(h).unwrap().soldier.pos;
        s.step(&[PadState::default()]);
        let u = s.unit(h).unwrap();
        assert!(u.wall_push.length() > 0.0, "pushed out of the slab");
        assert_eq!(u.wall_push.y, 0.0, "the push is horizontal");
        let moved = u.soldier.pos - before;
        assert!(
            (Vec3::new(moved.x, 0.0, moved.z) - u.wall_push).length() < 1e-5,
            "records exactly the push it applied: {moved} vs {}",
            u.wall_push
        );
    }

    /// A box body is real ground: a Pacer walks its top at a constant y and
    /// drops to the no-ground sentinel the moment it steps off the edge.
    #[test]
    fn unit_stands_on_a_box_and_walks_off_its_edge() {
        use crate::collision::{Body, CollisionWorld, Shape, NO_GROUND};
        let mut s = Sim::empty(5);
        // Box spanning x, z in (−5, 5) with its top at y = 6.
        s.collision = CollisionWorld::new(
            vec![Body::new(
                glam::Mat4::from_translation(Vec3::new(0.0, 5.0, 0.0)),
                Shape::Box {
                    half: Vec3::new(5.0, 1.0, 5.0),
                },
            )],
            Vec::new(),
        );
        let h = s
            .spawn(&UnitSpec {
                controller: Controller::Pacer {
                    vel: Vec3::new(2.0, 0.0, 0.0),
                    period: 0,
                },
                ..UnitSpec::trooper(1, Vec3::new(0.0, 6.0, 0.0))
            })
            .unwrap();
        assert_eq!(
            s.units[h.index as usize].soldier.pos.y, 6.0,
            "spawned on it"
        );
        for _ in 0..60 {
            s.step(&[PadState::default()]);
            let p = s.units[h.index as usize].soldier.pos;
            assert!((p.y - 6.0).abs() < 1e-4, "still on the box at {p}");
        }
        // 2 m/s for 60 ticks = 4 m; keep walking until it is past x = 5.
        for _ in 0..30 {
            s.step(&[PadState::default()]);
        }
        let p = s.units[h.index as usize].soldier.pos;
        assert!(p.x > 5.0, "walked off the edge: {p}");
        assert_eq!(p.y, NO_GROUND, "nothing under it any more: {p}");
    }
}
