//! Walk mode: the local player inside `core::Sim` on a baked map — fixed-tick
//! (A5 spine) + third-person chase camera from the games' CAMERASECTION data.
//! P4: the dummy targets are real units with health; R2 fires the soldier's
//! rifle (a class with a laser look draws its bolts as `bolt_vis` ribbons;
//! the rest are thin boxes).
//!
//! Soldiers render as skinned models (game::soldier_vis); a team with no
//! baked model falls back to a stance-height box. Heights = BF2 capsule
//! table DAT_0061ed50
//! {1.9 stand, 1.6 crouch, 0.55 prone} (core::unit::capsule_height).
//!
//! A player in a vehicle seat (card VSEAT) is hidden and the camera is the
//! seat's ([`vehicle_camera`]). A zoomed player (card ZOOM) is framed by the
//! PS2's eased projection scale; in the scope view his own body is not drawn.
//! A building turret's gunner zoomed with R3 (card VTURZ) looks from the eye
//! point along the look ([`turret_scope_camera`]) with the turret class's
//! `FirstPersonFOV`; his own building is left out of his view by the scene
//! (`SceneGpu::view_draws`).

use crate::camera_collide::CameraBlockers;
use crate::layout::{CommandPostDef, MapLayout};
use core::cp::CpDef;
use core::heightfield::Heightfield;
use core::region::Region;
use core::sim::{PadState, Sim};
use core::soldier::{Soldier, Stance};
use core::unit::{capsule_height, Controller, UnitHandle, UnitSpec};
use glam::{Mat4, Quat, Vec3};
use render::Camera;

/// Team tints (placeholder until faction colors come from data): team 0,
/// team 1, neutral.
pub const TEAM_COLORS: [[f32; 3]; 3] = [[0.95, 0.35, 0.15], [0.3, 0.9, 0.3], [0.9, 0.9, 0.9]];

/// How far the chase camera keeps from the terrain sheet, either side.
const CAMERA_TERRAIN_GAP: f32 = 0.3;

pub fn team_color(team: Option<u8>) -> [f32; 3] {
    match team {
        Some(0) => TEAM_COLORS[0],
        Some(1) => TEAM_COLORS[1],
        _ => TEAM_COLORS[2],
    }
}

/// The PS2 soldier camera's tan(fov_y / 2), third person, standing, zoom 1,
/// read from PS2 RAM on every `pcsx2/ref/states/` state (FINDINGS F5): BF1
/// cam+0x8c, BF2 cam+0x13c (offsets named by `analysis/re-sky-pass.md` §2/§4)
/// — 37.69° / 52.06° vertical. They are tan(FOV / 2) x 0.7666 of the
/// class's third-person FOV, 48° and 65° (`analysis/re-zoom.md` §5; card
/// CFOVM: vehicles may differ).
pub const PS2_TAN_HALF_FOV_Y_BF1: f32 = 0.34131;
pub const PS2_TAN_HALF_FOV_Y_BF2: f32 = 0.48838;

/// A soldier class's `ThirdPersonFOV` and `FirstPersonFOV` in degrees
/// (`re-zoom.md` §5): the zoomed view takes the second. No BF1 soldier
/// authors either (48° both); BF2's soldiers 65° / 70°.
pub fn ps2_soldier_fovs(rules: core::soldier::GroundRules) -> (f32, f32) {
    match rules {
        core::soldier::GroundRules::Bf1 => (48.0, 48.0),
        core::soldier::GroundRules::Bf2 => (65.0, 70.0),
    }
}

/// The PS2 camera eases its projection scale S and its FOV toward their
/// targets by exp(-25 dt) per update (`re-zoom.md` §5).
pub const ZOOM_EASE_RATE: f32 = 25.0;

/// The chase camera's vertical FOV (radians) for a manifest's `source_game`.
/// The port keeps the PS2's VERTICAL framing and widens the horizontal with
/// the window (Hor+, `render::Camera::fov_y`) — never the 4:3 horizontal.
pub fn ps2_fov_y(source_game: &str) -> f32 {
    let t = if source_game.eq_ignore_ascii_case("bf1") {
        PS2_TAN_HALF_FOV_Y_BF1
    } else {
        PS2_TAN_HALF_FOV_Y_BF2
    };
    2.0 * core::fmath::atan2(t, 1.0)
}

/// Where the PS2 draws the reticle with the standing camera at its ideal
/// place, as a fraction of the height from the top (`ui::hud::reticle_y`):
/// BF1 line 178.5 of 480, BF2 153.35 (`re-aim-assist.md` §10.4 [V-RAM]).
pub fn ps2_reticle_y(source_game: &str) -> f32 {
    let rules = core::soldier::GroundRules::for_game(source_game);
    let t = match rules {
        core::soldier::GroundRules::Bf1 => PS2_TAN_HALF_FOV_Y_BF1,
        core::soldier::GroundRules::Bf2 => PS2_TAN_HALF_FOV_Y_BF2,
    };
    ui::hud::reticle_y(
        core::unit::camera_section(rules, Stance::Stand, false).tilt,
        t,
    )
}

/// A seat's third-person camera (card VSEAT) as `(target, camera)`.
/// PROVISIONAL — the PS2's vehicle camera is untraced; this reads the seat's
/// props the way [`WalkSim::camera`] reads the soldier's CAMERASECTION: the
/// target is `TrackCenter` in the vehicle's HEADING frame (x right, y up, z
/// forward; default 0 2 0), the camera `TrackOffset.z` behind it along the
/// view, `.y` above and `.x` right (default 0 1 8), pitched down by
/// `TiltValue` degrees. The heading ignores the hover's pitch and roll so
/// the view does not rock with the springs. `ThirdPersonFOV` is not applied
/// (its unit is untraced, card CFOVM): the chase FOV `fov_y` is. A seat
/// with a gun (card VTUR) passes its `look` (yaw, pitch in the model frame,
/// `core::vehicle::gun`): the heading frame turns by the yaw and the view
/// pitches by the pitch, so the camera looks where the seat's aim ray goes.
pub fn vehicle_camera(
    v: &core::vehicle::Vehicle,
    seat: Option<&crate::vehicle_class::SeatSpec>,
    look: Option<(f32, f32)>,
    fov_y: f32,
) -> (Vec3, Camera) {
    let (look_yaw, look_pitch) = look.unwrap_or((0.0, 0.0));
    let f = v.rot * core::vehicle::gun::dir(look_yaw, 0.0);
    let fwd = Vec3::new(f.x, 0.0, f.z).try_normalize().unwrap_or(Vec3::Z);
    let right = Vec3::new(fwd.z, 0.0, -fwd.x);
    let c = seat.and_then(|s| s.track_center).unwrap_or([0.0, 2.0, 0.0]);
    let o = seat.and_then(|s| s.track_offset).unwrap_or([0.0, 1.0, 8.0]);
    let tilt = seat.and_then(|s| s.tilt_value).unwrap_or(0.0);
    let cam = Camera {
        yaw: core::fmath::atan2(-fwd.x, -fwd.z),
        pitch: look_pitch - tilt.to_radians(),
        fov_y,
        ..Camera::default()
    };
    let target = v.pos + right * c[0] + Vec3::Y * c[1] + fwd * c[2];
    let position = target - cam.forward() * o[2] + Vec3::Y * o[1] + cam.right() * o[0];
    (target, Camera { position, ..cam })
}

/// A zoomed building turret's view (card VTURZ, `analysis/re-turret-draw.md`
/// 4 [O]; B1 `FUN_00225988`, B2 `FUN_0026fdf8`): at the eye point
/// (`EyePointOffset` on the aim frame, `SeatLook::scope_eye`), looking along
/// the look, no pull-in and no floor (the PS2's look-along frame has
/// neither).
pub fn turret_scope_camera(look: &core::sim::SeatLook, fov_y: f32) -> Camera {
    let d = look.view;
    Camera {
        position: look.scope_eye,
        yaw: core::fmath::atan2(-d.x, -d.z),
        pitch: core::fmath::atan2(d.y, d.with_y(0.0).length()),
        fov_y,
        ..Camera::default()
    }
}

/// The FOV option's range (profile `Settings::fov_scale`, the pause menu):
/// a multiplier on the PS2 angle, 1.0 = the PS2's framing. The result is
/// held under [`MAX_FOV_Y`] so a BF2 map at the top of the range stays sane.
pub const FOV_SCALE_RANGE: (f32, f32) = (0.75, 2.0);
pub const MAX_FOV_Y: f32 = 100.0 * std::f32::consts::PI / 180.0;

/// A target box to draw: transform, magnet highlight, health fraction.
#[derive(Debug, Clone, Copy)]
pub struct TargetDraw {
    pub transform: Mat4,
    pub magnet: bool,
    pub health_frac: f32,
    pub team: u8,
}

/// What the HUD's weapon slot shows for a class's primary weapon (visual
/// only; `crate::roster::weapon_hud`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WeaponHud {
    /// What the PS2's weapon-change event carries (`analysis/re-hud.md` §3
    /// `[V][O]`): the weapon class's `HUDTag`, else its own `GeometryName`,
    /// else its name. `hudtransforms` turns it into a mesh.
    pub key: String,
    /// The weapon class's own name (`ui::hud::HudWeapon::class`).
    pub class: String,
    /// The class's `IconTexture` (BF1's HUD sheet), for a weapon with no BF2
    /// mesh to show.
    pub icon: Option<String>,
}

/// One class's VISUAL side, index-aligned with `Sim::classes[team]` (the
/// sim's own copy carries the constants; this carries what to draw and what
/// to call it).
#[derive(Debug, Clone)]
pub struct ClassVisual {
    /// ODF class name (`all_inf_marksman`).
    pub name: String,
    /// Display label for the HUD / spawn screen (`game::roster`'s rule).
    pub label: String,
    /// Soldier model, baked at `side_<game>/soldiers/<geometry>.glb`.
    pub geometry: Option<String>,
    /// Primary weapon model, baked at `side_<game>/weapons/<name>.glb`.
    pub weapon_geometry: Option<String>,
    /// The primary weapon in the HUD's weapon slot.
    pub weapon_hud: WeaponHud,
    /// The class's other slots in the HUD (card GREN): `(slot, what to
    /// show)` for each weapon the unit holds besides the primary.
    pub held_hud: Vec<(u8, WeaponHud)>,
    /// The model each of those slots is drawn as in the hand (card WHELD):
    /// `(slot, GeometryName)`, `None` = drawn as nothing. A slot not listed
    /// draws the primary's `weapon_geometry`.
    pub held_models: Vec<(u8, Option<String>)>,
    /// The hash of the class's `FoleyFXClass` (card FOLY, `crate::foley`):
    /// which row of the map's foley table its steps play; 0 = none.
    pub foley_class: u32,
    /// The class's `HurtSound` / `DeathSound` (card UCRY).
    pub cries: Cries,
    /// What picks the class's animation sets (card ANIML).
    pub anim: crate::animator::ClassAnim,
    /// The class's override textures (card USKN).
    pub skins: crate::roster::ClassSkins,
}

/// A soldier class's two cries (card UCRY, `analysis/re-unit-chatter.md`
/// 2.7, 3.6): the sound property names its `HurtSound` and `DeathSound`
/// give, through its base chain; None for a class without one (the Jedi).
/// And its three BF1 combat calls (card UCALL, 2.1-2.4): `AcquiredTargetSound`,
/// `FleeSound`, `HeardEnemySound`. The class's `HidingSound`,
/// `ApproachingTargetSound` and `PreparingForDamageSound` are never asked
/// on the PS2 (2.6) and are not read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cries {
    pub hurt: Option<String>,
    pub death: Option<String>,
    pub acquired: Option<String>,
    pub flee: Option<String>,
    pub heard: Option<String>,
    /// Card UTALK, the AI's own squad talk (re-squad-orders.md 2, 5): the
    /// class's AI-issuer lines `AISCFieldFollowSound`,
    /// `AISCFieldMoveOutSound`, `AISCDriverGetInSound` and the answer
    /// `AISCResponseYesSirSound`.
    pub follow: Option<String>,
    pub move_out: Option<String>,
    pub need_gunner: Option<String>,
    pub yes: Option<String>,
    /// Card UTALK (re-unit-chatter.md 2.8): `LowHealthSound`, asked whenever
    /// the unit's own line is over while it lives with health at or under
    /// its max x `LowHealthThreshold` (the class's; 0.3 where it sets none).
    pub low_health: Option<String>,
    pub low_threshold: f32,
    /// Card UVO2: BF2's `VOSound` lines, `VOUnitType` and `VORadius`
    /// (`crate::vo`); empty on BF1 classes.
    pub vo: std::sync::Arc<crate::vo::VoLines>,
}

impl Cries {
    /// The property a combat call asks.
    pub fn call(&self, call: core::sim::Call) -> Option<&str> {
        use core::sim::Call;
        match call {
            Call::Acquired => self.acquired.as_deref(),
            Call::Flee => self.flee.as_deref(),
            Call::Heard => self.heard.as_deref(),
            Call::Follow => self.follow.as_deref(),
            Call::MoveOut => self.move_out.as_deref(),
            Call::NeedGunner => self.need_gunner.as_deref(),
            Call::Yes => self.yes.as_deref(),
            Call::LowHealth => self.low_health.as_deref(),
            Call::Vo(_) => None,
        }
    }
}

/// The `LowHealthThreshold` a soldier class that sets none has (B1 class
/// +0x1e0c, the constructor's 0.3 at B1:121978; re-unit-chatter.md 2.8).
pub const LOW_HEALTH_THRESHOLD: f32 = 0.3;

impl ClassVisual {
    /// Base roster projection. Callers add the held HUD/model entries they use.
    pub fn from_roster(c: &crate::roster::ClassRoster) -> Self {
        Self {
            name: c.class.clone(),
            label: c.label.clone(),
            geometry: c.geometry.clone(),
            weapon_geometry: c.weapon_geometry.clone(),
            weapon_hud: c.weapon_hud.clone(),
            held_hud: Vec::new(),
            held_models: Vec::new(),
            foley_class: c.foley_class,
            cries: c.cries.clone(),
            anim: c.anim.clone(),
            skins: c.skins.clone(),
        }
    }

    /// What the HUD shows for the weapon of class slot `slot`: a held
    /// slot's own entry, else the primary's.
    pub fn hud_of(&self, slot: u8) -> &WeaponHud {
        self.held_hud
            .iter()
            .find(|(s, _)| *s == slot)
            .map_or(&self.weapon_hud, |(_, h)| h)
    }
}

/// Per-team unit template (from game::roster or the built-in defaults).
#[derive(Debug, Clone)]
pub struct TeamTemplate {
    /// Faction key for the HUD ("all" | "imp" | "rep" | "cis" | "team").
    pub faction: String,
    pub name: String,
    pub consts: core::soldier::SoldierConstants,
    pub max_health: f32,
    pub health_type: core::weapon::HealthType,
    /// Index into `Sim::weapon_classes`.
    pub weapon_class: usize,
    pub spare_clips: u32,
    /// The fill class's primary slot and its other weapon slots (card GREN),
    /// as its `Sim::classes` entry holds them: the template spawns the
    /// player's first unit.
    pub weapon_slot: u8,
    pub slots: std::sync::Arc<[core::unit::SlotSpec]>,
    /// The fill class's drop table (card PWUP).
    pub drops: core::sim::DropTable,
    /// Soldier model (class `GeometryName`), baked under
    /// `side_<game>/soldiers/<geometry>.glb`; `None` = marker box.
    pub geometry: Option<String>,
    /// Primary weapon model, baked under `side_<game>/weapons/<name>.glb`.
    pub weapon_geometry: Option<String>,
    /// Every class this team fields, index-aligned with `Sim::classes[team]`
    /// — `UnitSpec::class` indexes BOTH. Never empty: a template with no
    /// roster has one entry describing itself.
    pub classes: Vec<ClassVisual>,
}

impl TeamTemplate {
    /// Project roster metadata and its already-registered first class.
    /// Registration order and the visuals included remain the caller's policy.
    pub fn from_roster(
        roster: &crate::roster::FactionRoster,
        registered_fill: &core::unit::UnitClass,
        classes: Vec<ClassVisual>,
    ) -> Self {
        let c = roster.fill_class();
        Self {
            faction: roster.faction.clone(),
            name: c.class.clone(),
            consts: c.consts,
            max_health: c.max_health,
            health_type: c.health_type,
            weapon_class: registered_fill.weapon_class,
            weapon_slot: registered_fill.weapon_slot,
            slots: registered_fill.slots.clone(),
            drops: registered_fill.drops,
            spare_clips: c.spare_clips,
            geometry: c.geometry.clone(),
            weapon_geometry: c.weapon_geometry.clone(),
            classes,
        }
    }

    /// Built-in BF1 trooper with the BF1 rifle (Sim weapon class 0).
    pub fn default_trooper() -> Self {
        Self {
            faction: "team".into(),
            name: "trooper".into(),
            consts: core::soldier::SoldierConstants::default(),
            max_health: 300.0,
            health_type: core::weapon::HealthType::Person,
            weapon_class: 0,
            spare_clips: 6,
            weapon_slot: 0,
            slots: std::sync::Arc::new([]),
            drops: core::sim::DropTable::NONE,
            geometry: None,
            weapon_geometry: None,
            classes: vec![ClassVisual {
                name: "trooper".into(),
                label: "Trooper".into(),
                geometry: None,
                weapon_geometry: None,
                weapon_hud: WeaponHud::default(),
                held_hud: Vec::new(),
                held_models: Vec::new(),
                foley_class: 0,
                cries: Default::default(),
                anim: Default::default(),
                skins: Default::default(),
            }],
        }
    }

    /// The class at `index`, clamped (the tables are never empty).
    pub fn class(&self, index: u8) -> &ClassVisual {
        let i = usize::from(index).min(self.classes.len().saturating_sub(1));
        &self.classes[i]
    }

    pub fn spec(
        &self,
        team: u8,
        pos: Vec3,
        yaw: f32,
        controller: Controller,
        class: u8,
    ) -> UnitSpec {
        UnitSpec {
            team,
            player_slot: match controller {
                Controller::Player { slot } => Some(slot),
                _ => None,
            },
            controller,
            pos,
            yaw,
            consts: self.consts,
            max_health: self.max_health,
            health_type: self.health_type,
            weapon_class: self.weapon_class,
            spare_clips: self.spare_clips,
            weapon_slot: self.weapon_slot,
            slots: self.slots.clone(),
            capture_posts: true,
            drops: self.drops,
            class,
            // `Sim::spawn` overwrites this from `Sim::difficulty` for every
            // `Controller::Ai` unit (D2) — the template does not know the tier.
            skill: 0,
            // No class in the game layer builds a jet trooper yet (PLAN
            // "PHASE A.2 DESIGN" decision 4): the flag exists so the planning
            // graph can keep `jet_jump` arcs off a jetless soldier.
            jet: false,
        }
    }
}

/// Which of the five per-faction value rows a team's faction key reads
/// (`game::layout::PROP_VALUE_ATK`'s order: Alliance, Empire, Republic, CIS,
/// Locals). The roster's own keys are the three-letter ones (`all`/`imp`/
/// `rep`/`cis`, `game::roster::default_factions`); anything else — the
/// built-in `default_trooper`'s "team", a mod faction, a mission that names
/// something we do not know — is LOCALS, which is what the disc's own
/// "everyone else" row is for.
pub fn faction_row(faction: &str) -> usize {
    match faction.to_ascii_lowercase().as_str() {
        "all" | "alliance" => 0,
        "imp" | "empire" => 1,
        "rep" | "republic" => 2,
        "cis" => 3,
        _ => 4,
    }
}

pub struct WalkSim {
    pub sim: Sim,
    /// The local player's unit — player SLOT 0 (`Sim::players[0]`).
    pub player: UnitHandle,
    pub teams: [TeamTemplate; 2],
    /// World boxes the chase camera must not enter (visual only — never read
    /// by the sim, so it cannot affect determinism). Set by the caller once
    /// the scene is loaded; empty = terrain-only camera flooring.
    pub camera_blockers: CameraBlockers,
    /// Each vehicle class's `GeometryName`, by `Vehicle::class` (visual
    /// only, like `camera_blockers`): set with the pads
    /// (`crate::vehicle_pads::PadBuild::geometry`), drawn by
    /// `crate::viewer::VehicleVisuals`.
    pub vehicle_geometry: Vec<Option<String>>,
    /// Each vehicle class's seat ring with its camera props, by
    /// `Vehicle::class` (visual only): set with the pads
    /// (`crate::vehicle_pads::PadBuild::seats`), read by [`vehicle_camera`].
    pub vehicle_seats: Vec<Vec<crate::vehicle_class::SeatSpec>>,
    /// Each vehicle class's seats' rider hangs, parallel to `vehicle_seats`
    /// (visual only, card VRIDE): read by `crate::soldier_vis`.
    pub vehicle_riders: Vec<Vec<crate::vehicle_class::RiderHang>>,
    /// Each vehicle class's engine sound keys, by `Vehicle::class` (sound
    /// only, card VSND): set with the pads, read by
    /// `crate::engine_sound::EngineSounds`.
    pub vehicle_engines: Vec<Option<crate::engine_sound::EngineClass>>,
    /// Each vehicle class's `Label`, by `Vehicle::class` (display only): set
    /// with the pads (`crate::vehicle_pads::PadBuild::labels`); the spawn
    /// screen names a command walker's post by it (card CWSP).
    pub vehicle_labels: Vec<Option<String>>,
    /// Each vehicle class's ODF name, by `Vehicle::class` (display only):
    /// set with the pads (`crate::vehicle_pads::PadBuild::classes`); the
    /// HUD's target readout names a picked vehicle by it (card HTGT).
    pub vehicle_classes: Vec<String>,
    /// The chase camera's PS2 vertical FOV (radians; visual only — the sim
    /// never reads it). BF1's by default; `make_walk` sets the map's game's
    /// with [`ps2_fov_y`].
    pub fov_y_ps2: f32,
    /// The camera's projection scale S: 1.0 unzoomed, eased toward the zoom
    /// of the player's weapon each tick ([`WalkSim::step_tick`]). Visual
    /// only, like the FOV.
    pub zoom_s: f32,
    /// How far the FOV has eased from the class's third-person FOV (0) to
    /// its first-person one (1), the zoomed view's.
    pub zoom_fov_mix: f32,
    /// The player's FOV option (profile `Settings::fov_scale`), clamped to
    /// [`FOV_SCALE_RANGE`]; 1.0 = the PS2's framing.
    pub fov_scale: f32,
    /// The commander's voice-overs (card ANNC; presentation only — the sim
    /// never reads it). None without a mission: the lines are the script's.
    pub voice: Option<crate::announce::Commander>,
    /// The battle's music (card AMBM; presentation only). None without a
    /// mission: the Musics are the script's.
    pub music: Option<crate::music::BattleMusic>,
    /// The sound sub-levels the mission's script loads (card SNDE,
    /// `MissionDef::sound_levels`; presentation only): the battle's sound
    /// library, foley table and engine records are theirs
    /// (`crate::foley::battle_levels`). Empty without a mission.
    pub sound_levels: Vec<String>,
    /// Each command post's name key, `level.<map>.<label>`, by `Sim::cps`
    /// index (card EVMSG; presentation only): what the post is called in the
    /// disc's text table (`crate::event_feed`).
    pub post_keys: Vec<String>,
    /// The minimap's fixed part (card BMAP; presentation only): the map
    /// square, the image and the classes' icons. `None` draws no minimap.
    pub minimap: Option<crate::minimap::MapStatic>,
    /// The spawn screens' level-start view (card SSELB; presentation only):
    /// the mission's camera shots, cut on each screen change, until the
    /// player's first spawn. `None` = the battle's own view.
    pub backdrop: Option<crate::spawn_backdrop::Backdrop>,
    /// The drawn frame's blend between the last two ticks (card RINT;
    /// presentation only - the sim never reads it): [`WalkSim::advance_with`]
    /// keeps the pose each tick starts from.
    pub blend: crate::render_blend::Blend,
    accum: f32,
}

/// The one map [`WalkSim::rescue_spawn`] must not move a grounded player on
/// (`MapLayout::map`, the disc's name — the baked directory is `tat1` too).
///
/// `main.rs`'s `--spawn` default `(-140, 230)` was picked on this map in P3,
/// and the two recorded input logs the replay referees drive
/// (`engine/artifacts/p3_walk/{walk,aim}_demo.ron` through
/// `scripts/sim_gates.sh`) were recorded walking from it with no `--spawn`
/// flag: the logs are pad frames, not positions, so a different start replays
/// them into different terrain and the referee stops measuring what it was
/// built to measure. On TAT1 the default is also the only map's own start it
/// IS, so keeping it costs nothing else.
const HISTORIC_WALK_START_MAP: &str = "tat1";

/// A command post's spawn nodes as `(position, yaw)`, with the fallback
/// [`WalkSim::setup_conquest`] has always used when the post names no path or
/// the path is empty: one node 3 m off the post's +x/+z corner. Never empty,
/// so index 0 is always a node.
fn cp_spawn_nodes(layout: &MapLayout, cp: &CommandPostDef) -> Vec<(Vec3, f32)> {
    cp.spawn_path
        .as_deref()
        .and_then(|n| layout.path(n))
        .map(|p| p.nodes.iter().map(|n| (n.pos, n.yaw)).collect())
        .filter(|v: &Vec<(Vec3, f32)>| !v.is_empty())
        .unwrap_or_else(|| vec![(cp.pos + Vec3::new(3.0, 0.0, 3.0), 0.0)])
}

impl WalkSim {
    /// `seed` feeds both sim RNGs (spread); replays carry it in the log.
    /// Default templates: BF1 troopers with the BF1 rifle.
    pub fn new(seed: u32, spawn: Vec3, hf: Option<Heightfield>) -> Self {
        let sim = Sim::empty(seed);
        let t = TeamTemplate::default_trooper();
        Self::with_teams(sim, hf, spawn, [t.clone(), t], None)
    }

    /// Build on a prepared sim (weapon classes registered) with per-team
    /// templates; the player spawns as a team-0 unit at `spawn`
    /// ([`WalkSim::with_teams_seat`] seats it on either team).
    ///
    /// `nav` is the map's authored planning graph
    /// (`game::layout::nav_graph`), an INPUT like the heightfield. It is set
    /// BEFORE any command post is added, because `Sim::add_cp` resolves each
    /// post's planning hub as it goes in. `None` = the AI walks straight at
    /// its post, as it did before Phase A.
    pub fn with_teams(
        sim: Sim,
        hf: Option<Heightfield>,
        spawn: Vec3,
        teams: [TeamTemplate; 2],
        nav: Option<core::nav::NavGraph>,
    ) -> Self {
        Self::with_teams_seat(sim, hf, spawn, teams, nav, 0)
    }

    /// [`WalkSim::with_teams`] with the player a unit of `player_team` (0 or
    /// 1): the team its army has in the mission picked (card ERAM).
    pub fn with_teams_seat(
        mut sim: Sim,
        hf: Option<Heightfield>,
        spawn: Vec3,
        teams: [TeamTemplate; 2],
        nav: Option<core::nav::NavGraph>,
        player_team: u8,
    ) -> Self {
        let seat = player_team.min(1);
        sim.heightfield = hf;
        sim.nav = nav;
        // A caller's spawn is an (x, z) with y = 0 unless it pinned one; the
        // ground probe starts from the authored y, so lift it onto the
        // terrain first — under a heightfield more than TERRAIN_BURY above
        // y = 0 the player would otherwise be placed at NO_GROUND.
        let spawn = match &sim.heightfield {
            Some(h) => Vec3::new(spawn.x, spawn.y.max(h.height_at(spawn.x, spawn.z)), spawn.z),
            None => spawn,
        };
        let player = sim
            .spawn(&teams[usize::from(seat)].spec(
                seat,
                spawn,
                0.0,
                Controller::Player { slot: 0 },
                0,
            ))
            .expect("fresh battle has reinforcements");
        Self {
            sim,
            player,
            camera_blockers: CameraBlockers::default(),
            vehicle_geometry: Vec::new(),
            vehicle_seats: Vec::new(),
            vehicle_riders: Vec::new(),
            vehicle_engines: Vec::new(),
            vehicle_labels: Vec::new(),
            vehicle_classes: Vec::new(),
            fov_y_ps2: ps2_fov_y("bf1"),
            zoom_s: 1.0,
            zoom_fov_mix: 0.0,
            fov_scale: 1.0,
            voice: None,
            music: None,
            sound_levels: Vec::new(),
            post_keys: Vec::new(),
            minimap: None,
            backdrop: None,
            teams,
            blend: crate::render_blend::Blend::default(),
            accum: 0.0,
        }
    }

    /// Drop a line of enemy troopers ahead of the spawn (one pacing
    /// sideways, reversing every 3 s), feet on the terrain.
    pub fn spawn_dummy_targets(&mut self) {
        let base = self.soldier().pos;
        for (dx, dz, vx) in [
            (-4.0, -18.0, 0.0),
            (5.0, -25.0, 0.0),
            (0.0, -35.0, 2.0), // pacer
        ] {
            let controller = if vx != 0.0 {
                Controller::Pacer {
                    vel: Vec3::new(vx, 0.0, 0.0),
                    period: 3 * core::TICK_HZ,
                }
            } else {
                Controller::Idle
            };
            self.sim.spawn(&UnitSpec {
                controller,
                ..UnitSpec::trooper(1, base + Vec3::new(dx, 0.0, dz))
            });
        }
    }

    /// Conquest setup from the baked layout: the posts
    /// ([`WalkSim::add_posts`]), then the opening wave
    /// ([`WalkSim::opening_wave`]). A battle with vehicle pads calls the two
    /// itself with `vehicle_pads::install` between them, so the wave also
    /// spawns at the command walkers its pads build (card CWSP).
    pub fn setup_conquest(
        &mut self,
        layout: &MapLayout,
        ai_per_team: [u32; 2],
        relocate_player: bool,
    ) {
        self.add_posts(layout);
        self.opening_wave(ai_per_team, relocate_player);
    }

    /// The layout's command posts into the sim (owner = mission Team − 1,
    /// spawn nodes from their SpawnPath, class timings by major / minor).
    pub fn add_posts(&mut self, layout: &MapLayout) {
        for cp in &layout.cps {
            let nodes = cp_spawn_nodes(layout, cp);
            let (capture_time, neutralize_time) = CpDef::class_times(cp.major);
            let capture = cp.capture.unwrap_or_else(|| Region::sphere(cp.pos, 8.0));
            // The post's five per-faction rows collapse to the two the sim
            // needs, HERE, because `core` has no faction notion: team index t
            // takes the row of `self.teams[t].faction` (PLAN "Phase B
            // addendum").
            let rows = [
                faction_row(&self.teams[0].faction),
                faction_row(&self.teams[1].faction),
            ];
            let def = CpDef {
                name: cp.name.clone(),
                pos: cp.pos,
                capture,
                control: cp.control,
                value_bleed: cp.value_bleed,
                value_atk: [cp.value_atk[rows[0]], cp.value_atk[rows[1]]],
                value_def: [cp.value_def[rows[0]], cp.value_def[rows[1]]],
                spawn_nodes: nodes,
                capture_time,
                neutralize_time,
            };
            let owner = match cp.team {
                1 => Some(0),
                2 => Some(1),
                _ => None,
            };
            self.sim.add_cp(def, owner);
            if let Some(post) = self.sim.cps.last_mut() {
                post.ai_spawn_weight = cp.ai_spawn_weight;
            }
            self.post_keys
                .push(format!("level.{}.{}", layout.map, cp.label));
        }
        // Card TPOS: the position table stands on the floors walked from
        // the posts just added.
        self.sim.build_positions();
    }

    /// The opening wave: `ai_per_team` AI spawned round-robin over their
    /// team's spawn points — its posts' nodes, then the points of the command
    /// walkers its pads are about to build (`Sim::pad_walker_spawn_points`;
    /// none unless the pads are installed) — and the player relocated to a
    /// friendly post unless the caller pinned a spawn.
    pub fn opening_wave(&mut self, ai_per_team: [u32; 2], relocate_player: bool) {
        if relocate_player {
            let seat = self.sim.units[self.player.index as usize].team;
            if let Some(&(pos, yaw)) = self.sim.spawn_points(seat).first() {
                let g = self.sim.ground_under(pos, false);
                let u = &mut self.sim.units[self.player.index as usize];
                u.soldier.pos = Vec3::new(pos.x, g, pos.z);
                u.soldier.yaw = yaw;
            }
        }
        for team in 0..2u8 {
            let mut points = self.sim.spawn_points(team);
            points.extend(self.sim.pad_walker_spawn_points(team));
            if points.is_empty() {
                continue;
            }
            // With a class table the AI come out of `Sim::spawn_ai` (the
            // mission's mix, `Sim::ai_class`); without one — every test and
            // every caller that never built a roster — they are the team
            // template, exactly as before.
            let has_classes = !self.sim.classes[team as usize].is_empty();
            for k in 0..ai_per_team[team as usize] as usize {
                let (pos, yaw) = points[k % points.len()];
                if has_classes {
                    self.sim.spawn_ai(team, pos, yaw);
                } else {
                    let spec = self.teams[team as usize].spec(team, pos, yaw, Controller::Ai, 0);
                    self.sim.spawn(&spec);
                }
            }
        }
    }

    /// Where the player starts on the plain `game --map` path (no
    /// `--mission`, no `--ai`, no `--spawn`): the LOWEST-INDEX friendly
    /// command post's own spawn node, on every map but one.
    ///
    /// Nothing calls [`WalkSim::setup_conquest`] on that path, so the player
    /// keeps the CLI's `--spawn` default — a single hard-coded point
    /// (`main.rs`'s `-140, 230`) picked on TAT1 in P3, which means nothing on
    /// the other 31 maps. On seven BF2 maps it is off the walkable surface
    /// entirely and the player starts ~300 m under the world on
    /// `Sim::spawn`'s [`core::collision::NO_GROUND`] sentinel (cor1, dea1,
    /// kam1, mus1, myg1, tan1, uta1 — measured); on the 24 that do have ground
    /// there it is an arbitrary spot in the terrain. Both are the same defect
    /// (HANDOFF §Standing backlog 7; queue items W and WS), so both take the
    /// same answer: start where the map says a friendly soldier comes out.
    ///
    /// Lowest index because the layout's post order is the manifest's, which
    /// is the bake's, so the choice is the same on every run; friendly =
    /// mission team 1, the same 1 → sim team 0 mapping
    /// [`WalkSim::setup_conquest`] uses, because that is where the player
    /// would come out once a mission existed. The post's spawn node, its yaw
    /// and its ground probe are `setup_conquest`'s own ([`cp_spawn_nodes`] +
    /// `ground_under`), so a later `--ai` run puts the player in the same
    /// place. A post whose own spawn node has no ground is skipped, and a map
    /// with no friendly post at all is left alone — a groundless player stays
    /// on the sentinel rather than being teleported somewhere unauthored.
    /// That is one map on the two discs: BF2 Hoth authors all 16 of its posts
    /// as team 2 (measured), because on that map ownership is the MISSION's to
    /// re-author (`MapLayout::apply_mission`) and this path has no mission.
    ///
    /// **The one exception is [`HISTORIC_WALK_START_MAP`]**: there a player
    /// who already has ground keeps the default point, because that point is
    /// the map's own historic walk start and the recorded input logs replay
    /// from it (see the constant). A groundless player is still moved, on that
    /// map as on any other.
    ///
    /// Returns the post ONLY when the authored point had NO GROUND — the case
    /// the caller announces, in its own words, since before this rule existed.
    /// An ordinary post start announces itself here, so that every line
    /// printed says what actually happened.
    pub fn rescue_spawn(&mut self, layout: &MapLayout) -> Option<(String, f32)> {
        let grounded =
            self.sim.ground_under(self.soldier().pos, false) > core::collision::NO_GROUND;
        if grounded && layout.map == HISTORIC_WALK_START_MAP {
            return None;
        }
        let (post, y) = self.move_to_friendly_post(layout)?;
        if grounded {
            println!("spawn: no mission — starting at command post {post} (y {y:.2})");
            return None;
        }
        Some((post, y))
    }

    /// Move the player to the lowest-index friendly command post whose own
    /// spawn node has ground, and return that post's name and the y they
    /// landed on. `None` = no friendly post has ground; the player is left
    /// exactly where they were. See [`WalkSim::rescue_spawn`] for why this
    /// post and not another.
    fn move_to_friendly_post(&mut self, layout: &MapLayout) -> Option<(String, f32)> {
        let own = self.sim.units[self.player.index as usize].team + 1;
        for cp in layout.cps.iter().filter(|c| c.team == own) {
            let (pos, yaw) = cp_spawn_nodes(layout, cp)[0];
            let g = self.sim.ground_under(pos, false);
            if g <= core::collision::NO_GROUND {
                continue;
            }
            let u = &mut self.sim.units[self.player.index as usize];
            u.soldier.pos = Vec3::new(pos.x, g, pos.z);
            u.soldier.yaw = yaw;
            return Some((cp.name.clone(), g));
        }
        None
    }

    /// The player's soldier (even when dead — the slot persists).
    pub fn soldier(&self) -> &Soldier {
        &self.sim.units[self.player.index as usize].soldier
    }

    /// The player's team, dead or alive (the slot keeps it between lives).
    pub fn player_team(&self) -> u8 {
        self.sim.units[self.player.index as usize].team
    }

    /// Is the player on foot (alive and in no seat)? A rider is not drawn.
    pub fn player_on_foot(&self) -> bool {
        self.sim
            .unit(self.player)
            .is_some_and(|u| u.mount.is_none())
    }

    /// The magnet target when it is a unit (draw order of `target_draws`;
    /// a vehicle magnet draws no unit marker). Slot 0 — what the HUD draws.
    pub fn magnet(&self) -> Option<UnitHandle> {
        self.sim.magnet_at(0).and_then(core::sim::AimTarget::unit)
    }

    /// Advance the fixed-tick sim by a real frame's dt (A5: 30 Hz, accumulator
    /// capped at 0.1 s). Returns ticks stepped.
    pub fn advance(&mut self, real_dt: f32, pads: &[PadState]) -> u32 {
        self.advance_with(real_dt, pads, |_| {})
    }

    /// [`WalkSim::advance`], calling `each` after every tick it steps.
    /// `Sim::events` holds ONE tick's events, until the next step: whatever
    /// turns them into something seen or heard (effects, event sounds, bolt
    /// remnants, the message box) reads them here — once per tick, whether
    /// the frame steps none (every other frame at 60 Hz), one or several.
    pub fn advance_with(
        &mut self,
        real_dt: f32,
        pads: &[PadState],
        mut each: impl FnMut(&WalkSim),
    ) -> u32 {
        self.accum = (self.accum + real_dt).min(core::MAX_FRAME_ACCUM_SECS);
        let mut ticks = 0;
        while self.accum >= core::TICK_DT {
            let mut blend = std::mem::take(&mut self.blend);
            blend.before_tick(self);
            self.blend = blend;
            self.step_tick(pads);
            each(self);
            self.accum -= core::TICK_DT;
            ticks += 1;
        }
        ticks
    }

    /// The accumulator's left-over after the last frame's ticks (s): how far
    /// the frame is into the next tick (`render_blend::fraction`).
    pub fn accum(&self) -> f32 {
        self.accum
    }

    /// Write the pose blended between the last two ticks in, for the frame's
    /// draw lists (card RINT, `render_blend::Blend::apply`, at the
    /// accumulator's fraction); [`WalkSim::blend_out`] must follow before a
    /// tick or anything but drawing reads the walk.
    pub fn blend_in(&mut self) {
        let t = crate::render_blend::fraction(self);
        let mut blend = std::mem::take(&mut self.blend);
        blend.apply(self, t);
        self.blend = blend;
    }

    /// The last tick's own pose back after [`WalkSim::blend_in`] (a no-op
    /// without one).
    pub fn blend_out(&mut self) {
        let mut blend = std::mem::take(&mut self.blend);
        blend.restore(self);
        self.blend = blend;
    }

    /// One fixed tick (also the headless/replay entry — no accumulator).
    /// `pads` is indexed by player slot; slot 0 is [`WalkSim::player`].
    pub fn step_tick(&mut self, pads: &[PadState]) {
        self.sim.step(pads);
        // A respawn gives the player's slot a new generation
        // (`Sim::respawn_slot`): follow it, or every `sim.unit(self.player)`
        // reads None from the first death on (zoom, sprint latch, scope body,
        // music, commander, black box).
        if let Some(Some(h)) = self.sim.players.first() {
            self.player = *h;
        }
        self.follow_backdrop();
        // The camera's zoom follows the player's (`re-zoom.md` §5): S and
        // the FOV ease toward their targets, and rest exactly once there.
        // A building turret's zoom is the seat's (card VTURZ); the same ease
        // (PROVISIONAL for a turret: its projection is not read).
        let zoom = self
            .sim
            .unit(self.player)
            .and_then(|u| u.zoom)
            .or_else(|| self.sim.seat_zoom(self.player));
        let k = core::fmath::exp(-ZOOM_EASE_RATE * core::TICK_DT);
        let ease = |v: f32, to: f32| {
            let v = to + (v - to) * k;
            if (v - to).abs() < 1e-4 {
                to
            } else {
                v
            }
        };
        self.zoom_s = ease(self.zoom_s, zoom.map_or(1.0, |z| z.value));
        self.zoom_fov_mix = ease(self.zoom_fov_mix, if zoom.is_some() { 1.0 } else { 0.0 });
        // The squad's answers to this tick's orders join the voice queue;
        // the units' combat calls wait for the sound bank's gates
        // (`Commander::gate_calls`).
        if let Some(v) = self.voice.as_mut() {
            v.orders(&self.sim, self.player);
            v.calls(&self.sim, &self.teams);
        }
    }

    /// Third-person chase camera: the PS2's ideal camera (`core::unit`
    /// `ideal_camera`, `analysis/re-aim-assist.md` §10.2 step 4) - the track
    /// centre above the soldier plus the stance's track offset in a frame
    /// that looks the section's tilt BELOW the aim - then kept out of the
    /// terrain and (PROVISIONAL) out of world geometry. No smoothing (BF1's
    /// AimTension 30 and the MoveTension 15 are card CAMS's; the ZOOM
    /// sections are taken at once on the PS2 too). Zoomed on a scope weapon
    /// it is the scope's eye, looking along the aim. Framed at
    /// [`WalkSim::fov_y`].
    pub fn camera(&self) -> Camera {
        if let Some(b) = &self.backdrop {
            return b.camera();
        }
        if let Some(cam) = self.seat_camera() {
            return cam;
        }
        let me = &self.sim.units[self.player.index as usize];
        let ideal = me.ideal_camera(self.sim.ground_rules, self.sim.views());
        Camera {
            position: self.settle_camera(ideal.center, ideal.position),
            yaw: me.soldier.yaw,
            pitch: core::fmath::atan2(ideal.forward.y, ideal.forward.with_y(0.0).length()),
            fov_y: self.fov_y(),
            ..Camera::default()
        }
    }

    /// The level start's backdrop follows slot 0's spawn screen and ends
    /// with the player's first spawn (card SSELB): after that a spawn screen
    /// stands over the place he died.
    pub fn follow_backdrop(&mut self) {
        let alive = self.sim.units[self.player.index as usize].alive;
        if alive || !self.sim.spawn_screens {
            self.backdrop = None;
        }
        if let Some(b) = self.backdrop.as_mut() {
            b.follow(self.sim.spawn_stage.first().copied().unwrap_or_default());
        }
    }

    /// Where the reticle sits, as a fraction of the window's height from the
    /// top (§10.4): the point 1024 m along the player's aim ray through the
    /// drawn camera. With the camera at its ideal place that is tan(tilt) /
    /// tan(fov_y / 2) of the half height above the centre, at any aspect.
    /// A rider's stays on the centre (the seat cameras are PROVISIONAL).
    pub fn reticle_y(&self) -> f32 {
        let me = &self.sim.units[self.player.index as usize];
        if me.mount.is_some() {
            return 0.5;
        }
        let (a, d) = me.aim_ray(self.sim.ground_rules, self.sim.views());
        let clip = self.camera().view_proj(1.0) * (a + d * 1024.0).extend(1.0);
        0.5 - 0.5 * clip.y / clip.w
    }

    /// The player's seat camera, when the player rides (card VSEAT). A
    /// building turret's gunner is in third person always, the camera on the
    /// gun's look; zoomed (card VTURZ) it is [`turret_scope_camera`].
    fn seat_camera(&self) -> Option<Camera> {
        if self.sim.seat_zoom(self.player).is_some() {
            let look = self.sim.seat_look(self.player)?;
            return Some(turret_scope_camera(&look, self.fov_y()));
        }
        let m = self.sim.unit(self.player)?.mount?;
        let v = self.sim.vehicle(m.vehicle)?;
        let seat = self
            .vehicle_seats
            .get(v.class as usize)
            .and_then(|r| r.get(usize::from(m.seat)));
        let gun_look = v
            .guns
            .get(usize::from(m.seat))
            .and_then(|g| g.as_ref())
            .map(|g| g.look);
        // A walker's pilot looks along the walker's aim (card WALK), armed
        // seat or not.
        let walker_pilot = v.kind == core::vehicle::VehicleKind::Walker && m.seat == 0;
        let look = if walker_pilot {
            Some((v.walker.aim_yaw, v.walker.aim_pitch))
        } else {
            gun_look
        };
        let (target, cam) = vehicle_camera(v, seat, look, self.fov_y());
        Some(Camera {
            position: self.settle_camera(target, cam.position),
            ..cam
        })
    }

    /// Pull a camera at `position` looking at `target` in out of the world
    /// boxes, then in short of the terrain sheet where the track crosses it
    /// (the sheet is a surface, met from above or below; a cutter's hole is
    /// none), then keep it [`CAMERA_TERRAIN_GAP`] off the sheet on the side
    /// it is on. It never moves through the sheet: BF2 GEO1's hangar is 31 m
    /// under uncut terrain, and its entrance deck lies in a cutter's hole
    /// beside the hill, where a floor alone lifted the camera onto the hill.
    fn settle_camera(&self, target: Vec3, mut position: Vec3) -> Vec3 {
        // Buildings: pull the camera in along the track line before flooring,
        // so a wall behind the player shortens the track instead of putting
        // the camera inside the room next door.
        if !self.camera_blockers.is_empty() {
            position = self.camera_blockers.pull_in(target, position);
        }
        if let Some(h) = &self.sim.heightfield {
            let track = position - target;
            let len = track.length();
            if len > 1e-3 {
                let dir = track / len;
                let cutters = &self.sim.collision.cutters;
                if let Some(t) =
                    core::collision::ray_heightfield(h, cutters, target, dir, len, 0.25)
                {
                    position = target + dir * (t - CAMERA_TERRAIN_GAP).max(0.0);
                }
            }
            let y = h.height_at(position.x, position.z);
            if !self
                .sim
                .collision
                .is_cut(Vec3::new(position.x, y, position.z))
            {
                position.y = if position.y >= y {
                    position.y.max(y + CAMERA_TERRAIN_GAP)
                } else {
                    position.y.min(y - CAMERA_TERRAIN_GAP)
                };
            }
        }
        position
    }

    /// The chase camera's vertical FOV: the PS2's, times the player's option.
    /// The PS2's is 2 atan(tan(FOV / 2) x 0.7666 / S) (`re-zoom.md` §5):
    /// [`WalkSim::fov_y_ps2`] holds it for the third-person FOV at S = 1.
    pub fn fov_y(&self) -> f32 {
        let k = self.fov_scale.clamp(FOV_SCALE_RANGE.0, FOV_SCALE_RANGE.1);
        let mut fov = self.fov_y_ps2;
        if self.zoom_s != 1.0 || self.zoom_fov_mix != 0.0 {
            let (third, first) = self.zoom_fovs();
            let half = |rad: f32| {
                let (s, c) = core::fmath::sin_cos(rad * 0.5);
                s / c
            };
            let eased = half((third + (first - third) * self.zoom_fov_mix).to_radians())
                / half(third.to_radians());
            fov = 2.0 * core::fmath::atan2(half(fov) * eased, self.zoom_s);
        }
        (fov * k).min(MAX_FOV_Y)
    }

    /// The (third-person, zoomed) FOVs in degrees the zoom eases between:
    /// the soldier class's, or in a seat the seat camera's unzoomed one and
    /// the turret class's `FirstPersonFOV` (card VTURZ, re-turret-draw 4: a
    /// zoomed turret's camera takes the class's +0x1bc, 48 when not
    /// authored - `tur_bldg_recoilless_lg`'s 12.98 deg at zoom 3 against
    /// 13.0-13.5 fitted to the PS2's frame, `lead_tmp/vturz/fit_fov.py`).
    fn zoom_fovs(&self) -> (f32, f32) {
        let soldier = ps2_soldier_fovs(self.sim.ground_rules);
        let Some(m) = self.sim.unit(self.player).and_then(|u| u.mount) else {
            return soldier;
        };
        let first = self
            .sim
            .vehicle(m.vehicle)
            .and_then(|v| self.vehicle_seats.get(v.class as usize))
            .and_then(|r| r.get(usize::from(m.seat)))
            .and_then(|s| s.first_person_fov)
            .unwrap_or(48.0);
        (soldier.0, first)
    }

    /// Is the player's own body left out of the draw (the scope view,
    /// `re-zoom.md` §5)?
    pub fn player_body_hidden(&self) -> bool {
        self.sim.unit(self.player).is_some_and(|u| u.scoped())
    }

    /// Placeholder soldier visual: a stance-height box standing on the ground
    /// at the soldier's position, facing their yaw.
    pub fn marker_transform(&self) -> Mat4 {
        let s = self.soldier();
        let h = capsule_height(s.stance);
        Mat4::from_translation(s.pos + Vec3::Y * (h * 0.5))
            * Mat4::from_rotation_y(s.yaw)
            * Mat4::from_scale(Vec3::new(0.6, h, 0.6))
    }

    /// Alive non-player units on foot as stance-height boxes.
    pub fn target_draws(&self) -> Vec<TargetDraw> {
        self.sim
            .alive_units()
            .filter(|(h, u)| *h != self.player && u.mount.is_none())
            .map(|(h, u)| {
                let hgt = capsule_height(u.soldier.stance);
                TargetDraw {
                    transform: Mat4::from_translation(u.soldier.pos + Vec3::Y * (hgt * 0.5))
                        * Mat4::from_rotation_y(u.soldier.yaw)
                        * Mat4::from_scale(Vec3::new(0.7, hgt, 0.7)),
                    magnet: self.magnet() == Some(h),
                    health_frac: (u.health.cur / u.health.max).clamp(0.0, 1.0),
                    team: u.team,
                }
            })
            .collect()
    }

    /// In-flight bolts of a class with no laser look (not `bolt` / `laser`:
    /// a missile, a shell) as thin placeholder boxes
    /// along their velocity. A laser class's bolts are drawn as the PS2's
    /// ribbons by [`crate::bolt_vis`]. Returns (transform, team).
    pub fn bolt_draws(&self) -> Vec<(Mat4, u8)> {
        const LEN: f32 = 1.2;
        const THICK: f32 = 0.08;
        self.sim
            .bolts
            .iter()
            .filter(|b| {
                self.sim
                    .weapon_classes
                    .get(b.weapon_class)
                    .is_none_or(|w| w.ordnance.laser.is_none())
            })
            .map(|b| {
                let dir = b.vel.normalize_or_zero();
                let rot = Quat::from_rotation_arc(Vec3::Z, dir);
                (
                    Mat4::from_translation(b.pos - dir * (LEN * 0.5))
                        * Mat4::from_quat(rot)
                        * Mat4::from_scale(Vec3::new(THICK, THICK, LEN)),
                    b.team,
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    /// The five value rows, and the "everything else is Locals" rule that the
    /// built-in trooper (faction "team") falls into.
    #[test]
    fn faction_rows_cover_the_four_mains_and_default_to_locals() {
        assert_eq!(super::faction_row("all"), 0);
        assert_eq!(super::faction_row("imp"), 1);
        assert_eq!(super::faction_row("rep"), 2);
        assert_eq!(super::faction_row("cis"), 3);
        assert_eq!(super::faction_row("CIS"), 3, "case-insensitive");
        assert_eq!(super::faction_row("alliance"), 0);
        assert_eq!(super::faction_row("team"), 4, "the built-in trooper");
        assert_eq!(super::faction_row("wookiee"), 4);
    }

    use super::*;
    use core::sim::buttons;

    /// The PS2's measured vertical FOVs (FINDINGS F5), and the option scales
    /// the chase camera's angle within its range and cap.
    #[test]
    fn chase_camera_takes_the_ps2_vertical_fov() {
        let deg = |r: f32| r.to_degrees();
        assert!((deg(ps2_fov_y("bf1")) - 37.69).abs() < 0.01);
        assert!((deg(ps2_fov_y("BF2")) - 52.06).abs() < 0.01);
        let mut w = WalkSim::new(1, Vec3::ZERO, Some(flat_ground()));
        w.fov_y_ps2 = ps2_fov_y("bf2");
        assert_eq!(w.camera().fov_y, ps2_fov_y("bf2"));
        w.fov_scale = 1.5;
        assert!((w.camera().fov_y - 1.5 * ps2_fov_y("bf2")).abs() < 1e-6);
        w.fov_scale = 9.0;
        assert_eq!(w.camera().fov_y, MAX_FOV_Y);
        w.fov_scale = 0.0;
        assert!((w.camera().fov_y - FOV_SCALE_RANGE.0 * ps2_fov_y("bf2")).abs() < 1e-6);
    }

    /// Card ZOOM acceptance 4, the camera's half (`analysis/re-zoom.md` §5):
    /// S after n ticks of a zoom z is z + (1 - z) exp(-25 n dt) - the PS2's
    /// rows 1.848, 2.217, 2.377 for the rifle - the settled vertical FOV is
    /// 15.55 deg in BF1 and 24.24 deg in BF2 (the base FOV eased 65 -> 70
    /// deg), the camera stands at the ZOOM section, the reticle stays on the
    /// aim ray, and the zoom out comes back to the plain FOV exactly.
    #[test]
    fn the_zoom_eases_the_projection_as_the_ps2_does() {
        use core::sim::buttons;
        use core::soldier::GroundRules;
        let pad = |buttons| PadState {
            buttons,
            ..Default::default()
        };
        for (game, rules, settled, first) in [
            ("bf1", GroundRules::Bf1, 15.55, 20.92),
            ("bf2", GroundRules::Bf2, 24.24, 31.16),
        ] {
            let mut w = WalkSim::new(1, Vec3::ZERO, Some(flat_ground()));
            w.sim.ground_rules = rules;
            w.fov_y_ps2 = ps2_fov_y(game);
            let plain = w.camera();
            w.step_tick(&[pad(buttons::ZOOM)]);
            let cam = w.camera();
            // The section is taken at once: 0.4 m to the right, 3.5 deg
            // under the aim.
            let me = &w.sim.units[w.player.index as usize];
            assert!(((cam.position - me.soldier.pos).dot(cam.right()) - 0.4).abs() < 1e-3);
            assert!(((me.soldier.pitch - cam.pitch).to_degrees() - 3.5).abs() < 0.01);
            let mut rows = vec![w.zoom_s];
            let first_fov = cam.fov_y.to_degrees();
            for n in 2..=40 {
                w.step_tick(&[pad(0)]);
                if n <= 4 {
                    rows.push(w.zoom_s);
                }
            }
            let fov = w.camera().fov_y.to_degrees();
            println!(
                "{game}: plain {:.2} deg, S {rows:.4?}, first zoomed tick {first_fov:.2} deg, settled {fov:.2} deg, reticle line {:.1} of 480",
                plain.fov_y.to_degrees(),
                w.reticle_y() * 480.0
            );
            for (i, s) in rows.iter().enumerate() {
                let n = (i + 1) as f32;
                let want = 2.5 + (1.0 - 2.5) * (-25.0 * n * core::TICK_DT).exp();
                assert!(
                    (s - want).abs() < 1e-4,
                    "{game} tick {n}: S {s} want {want}"
                );
            }
            // The PS2's own rows (BF2's 1 / 30 s update; BF1's 33.36 ms
            // clock reads 1.8486, 2.2171, 2.3772).
            for (s, ps2) in rows.iter().zip([1.8481, 2.2167]) {
                assert!((s - ps2).abs() < 2e-3);
            }
            assert_eq!(w.zoom_s, 2.5, "S rests at the zoom");
            assert!((first_fov - first).abs() < 0.02, "{game}: {first_fov}");
            assert!((fov - settled).abs() < 0.01, "{game}: {fov}");
            // The reticle is where the aim ray goes (§5 [D]: tan(3.5 deg) x
            // S / tan of the half height above the centre).
            let t = (w.camera().fov_y * 0.5).tan();
            let want = 0.5 - 0.5 * 3.5f32.to_radians().tan() / t;
            assert!((w.reticle_y() - want).abs() < 2e-3);
            // Out again: the plain camera, to the bit.
            w.step_tick(&[pad(buttons::ZOOM)]);
            for _ in 0..40 {
                w.step_tick(&[pad(0)]);
            }
            assert_eq!(w.camera().fov_y, plain.fov_y);
            assert_eq!(w.camera().position, plain.position);
        }
    }

    /// Card AIMA: the chase camera looks the section's tilt below the aim
    /// and the reticle sits where the aim ray goes on screen - line 178.5 of
    /// 480 on a BF1 map, 153.35 on a BF2 map (`re-aim-assist.md` §10.4), at
    /// every pitch; a wider FOV option moves it toward the centre.
    #[test]
    fn the_reticle_sits_where_the_aim_ray_goes_on_screen() {
        use core::soldier::GroundRules;
        for (game, rules, line, cam_deg) in [
            ("bf1", GroundRules::Bf1, 178.5, -9.011),
            ("bf2", GroundRules::Bf2, 153.35, -14.011),
        ] {
            let mut w = WalkSim::new(1, Vec3::ZERO, Some(flat_ground()));
            w.sim.ground_rules = rules;
            w.fov_y_ps2 = ps2_fov_y(game);
            let cam = w.camera();
            let y = w.reticle_y() * 480.0;
            println!(
                "{game}: camera pitch {:.3} deg, {:.3} m up, reticle line {y:.2} of 480",
                cam.pitch.to_degrees(),
                cam.position.y
            );
            assert!((cam.pitch.to_degrees() - cam_deg).abs() < 0.01);
            assert!((y - line).abs() < 0.5, "{game}: line {y}");
            assert!((ps2_reticle_y(game) * 480.0 - line).abs() < 0.5);
            for pitch in [-0.3, 0.2, 0.6] {
                w.sim.units[w.player.index as usize].soldier.pitch = pitch;
                let y = w.reticle_y() * 480.0;
                assert!((y - line).abs() < 0.5, "{game} pitch {pitch}: line {y}");
            }
            w.fov_scale = 1.3;
            assert!(w.reticle_y() * 480.0 > line + 5.0);
        }
    }

    /// Flat ground at y = 0 over ±2 km. A sim with NO heightfield now floors at
    /// `core::collision::NO_GROUND` (−300) instead of 0, so a test about
    /// standing on flat ground has to say so.
    fn flat_ground() -> Heightfield {
        Heightfield {
            size: 2,
            unit: 4000.0,
            offset_x: 2000.0,
            offset_z: 2000.0,
            heights: vec![0.0; 4],
        }
    }

    #[test]
    fn soldier_follows_flat_ground_and_camera_tracks() {
        let mut w = WalkSim::new(1, Vec3::ZERO, Some(flat_ground()));
        let fwd = PadState {
            ly: 32767,
            ..Default::default()
        };
        for _ in 0..90 {
            w.step_tick(&[fwd]);
        }
        assert!(
            w.soldier().pos.z < -10.0,
            "walked forward: {}",
            w.soldier().pos
        );
        assert_eq!(w.soldier().pos.y, 0.0);
        let cam = w.camera();
        assert!(cam.position.z > w.soldier().pos.z, "camera behind soldier");
        // The camera looks the section's tilt below the aim, from above the
        // track centre (card AIMA).
        assert!((1.8..2.8).contains(&cam.position.y), "{}", cam.position);
        assert!(cam.pitch < w.soldier().pitch - 0.08);
    }

    /// The terrain floors a camera filming a soldier above it and caps one
    /// filming a soldier under it (a cave or a hangar under uncut terrain,
    /// BF2 GEO1's hangar post): it never lifts the camera through the sheet.
    #[test]
    fn camera_stays_on_the_soldiers_side_of_the_terrain() {
        let w = WalkSim::new(1, Vec3::ZERO, Some(flat_ground()));
        let open = w.settle_camera(Vec3::new(0.0, 1.5, 0.0), Vec3::new(0.0, 2.5, 4.0));
        assert_eq!(
            open,
            Vec3::new(0.0, 2.5, 4.0),
            "nothing in the way: untouched"
        );
        let above = w.settle_camera(Vec3::new(0.0, 1.5, 0.0), Vec3::new(0.0, -2.0, 4.0));
        assert!(
            above.y >= CAMERA_TERRAIN_GAP && above.z < 4.0,
            "pulled in and floored: {above}"
        );
        let cave = w.settle_camera(Vec3::new(0.0, -30.0, 0.0), Vec3::new(0.0, -28.0, 4.0));
        assert_eq!(cave.y, -28.0, "under the sheet it stays where it was");
        let roof = w.settle_camera(Vec3::new(0.0, -2.0, 0.0), Vec3::new(0.0, 1.0, 4.0));
        assert!(
            roof.y <= -CAMERA_TERRAIN_GAP,
            "and never rises through it: {roof}"
        );
    }

    #[test]
    fn accumulator_steps_at_thirty_hz() {
        let mut w = WalkSim::new(1, Vec3::ZERO, None);
        let pad = PadState::default();
        assert_eq!(w.advance(1.0 / 60.0, &[pad]), 0); // half a tick: none yet
        assert_eq!(w.advance(1.0 / 60.0, &[pad]), 1); // second half: one tick
        let capped = w.advance(1.0, &[pad]);
        assert!(
            (2..=3).contains(&capped),
            "capped at MAX_FRAME_ACCUM_SECS (float rounding): {capped}"
        );
    }

    #[test]
    fn marker_height_follows_stance() {
        let mut w = WalkSim::new(1, Vec3::ZERO, None);
        let press = PadState {
            buttons: buttons::ROLL_STANCE,
            ..Default::default()
        };
        let idle = PadState::default();
        for _ in 0..20 {
            w.step_tick(&[idle]);
        }
        w.step_tick(&[press]);
        assert_eq!(w.soldier().stance, Stance::Crouch);
        assert_eq!(capsule_height(w.soldier().stance), 1.6);
    }

    /// Aim assist on flat ground: a target 20 m ahead, slightly right of the
    /// aim; holding fire with a neutral stick pulls yaw toward it — and the
    /// rifle kills it.
    #[test]
    fn fire_magnetism_pulls_yaw_and_shots_kill_the_target() {
        let mut w = WalkSim::new(1, Vec3::ZERO, None);
        let t = w
            .sim
            .spawn(&UnitSpec::trooper(1, Vec3::new(0.4, 0.0, -20.0)))
            .unwrap();
        let fire = PadState {
            buttons: buttons::FIRE,
            ..Default::default()
        };
        for _ in 0..30 {
            w.step_tick(&[fire]);
        }
        assert_eq!(w.magnet(), Some(t), "target acquired");
        // Target is to the RIGHT (+X); positive yaw turns left, so the pull
        // drives yaw negative.
        assert!(
            w.soldier().yaw < -0.005,
            "yaw pulled toward target: {}",
            w.soldier().yaw
        );
        assert!(w.target_draws()[0].health_frac < 1.0, "hits landed");
        for _ in 0..60 {
            w.step_tick(&[fire]);
        }
        assert!(w.sim.unit(t).is_none(), "five rifle hits kill a trooper");
        assert!(w.target_draws().is_empty());
        assert_eq!(w.sim.battle.teams[1].tickets(), 249);

        // Intensity 0 = no pull.
        let mut w0 = WalkSim::new(1, Vec3::ZERO, None);
        w0.sim.assist_intensity = 0.0;
        w0.sim
            .spawn(&UnitSpec::trooper(1, Vec3::new(0.4, 0.0, -20.0)))
            .unwrap();
        for _ in 0..30 {
            w0.step_tick(&[fire]);
        }
        assert_eq!(w0.soldier().yaw, 0.0, "slider at 0 leaves aim untouched");
    }

    /// Out-of-range or off-cone targets never become magnets
    /// (`re-aim-assist.md` §3.3): the one at 80 m is past LockOnRange + r
    /// (64 + 1.42) but inside ExtremeRange (120), so it is the reticle's
    /// pick and not the magnet. The dummy rig spawns three enemies and the
    /// pacer moves.
    #[test]
    fn acquisition_respects_cone_and_range() {
        let mut w = WalkSim::new(1, Vec3::ZERO, None);
        let far = w
            .sim
            .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -80.0)))
            .unwrap();
        w.sim
            .spawn(&UnitSpec::trooper(1, Vec3::new(15.0, 0.0, -10.0))) // far off the cone
            .unwrap();
        // A level aim: the spawn pitch's ray is in the floor 33 m out.
        w.sim.units[w.player.index as usize].soldier.pitch = 0.0;
        w.step_tick(&[PadState::default()]);
        assert_eq!(w.magnet(), None);
        assert_eq!(
            w.sim.reticle_pick[0],
            Some((core::sim::AimTarget::Unit(far), -1))
        );

        let mut r = WalkSim::new(1, Vec3::ZERO, None);
        r.spawn_dummy_targets();
        assert_eq!(r.target_draws().len(), 3);
        let before = r.sim.units[3].soldier.pos;
        for _ in 0..30 {
            r.step_tick(&[PadState::default()]);
        }
        assert!(
            (r.sim.units[3].soldier.pos - before).length() > 1.5,
            "pacer moved"
        );
        assert!(r.bolt_draws().is_empty());
    }

    /// A 40 m box with its top at y = 6 around (100, 100) and nothing at the
    /// origin — a stand-in for a BF2 map with no heightfield, where the only
    /// ground is model collision (`Scene::collision_world`).
    use crate::layout::{SpawnNode, SpawnPath};

    fn island_world() -> core::collision::CollisionWorld {
        use core::collision::{Body, CollisionWorld, Shape};
        CollisionWorld::new(
            vec![Body::new(
                Mat4::from_translation(Vec3::new(100.0, 5.0, 100.0)),
                Shape::Box {
                    half: Vec3::new(20.0, 1.0, 20.0),
                },
            )],
            Vec::new(),
        )
    }

    fn post(name: &str, team: u8, pos: Vec3, spawn_path: Option<&str>) -> CommandPostDef {
        CommandPostDef {
            name: name.to_string(),
            label: String::new(),
            class: "com_bldg_controlzone".to_string(),
            team,
            pos,
            capture: None,
            control: None,
            spawn_path: spawn_path.map(str::to_owned),
            value_bleed: crate::layout::VALUE_DEFAULT,
            ai_spawn_weight: core::cp::AI_SPAWN_WEIGHT_DEFAULT,
            value_atk: [crate::layout::VALUE_DEFAULT; 5],
            value_def: [crate::layout::VALUE_DEFAULT; 5],
            major: false,
            vo: Default::default(),
        }
    }

    /// The layout the rescue is judged on: an ENEMY post over the island
    /// (wrong team), a FRIENDLY post over the void (right team, no ground)
    /// and a friendly post over the island with a real spawn path — in that
    /// order, so passing means the rescue skipped the first two for the
    /// stated reasons and not by luck of ordering.
    fn rescue_layout() -> MapLayout {
        MapLayout {
            cps: vec![
                post("cp_enemy", 2, Vec3::new(100.0, 6.0, 100.0), None),
                post("cp_void", 1, Vec3::new(500.0, 0.0, 500.0), None),
                post(
                    "cp_home",
                    1,
                    Vec3::new(90.0, 6.0, 90.0),
                    Some("cp_home_spawn"),
                ),
            ],
            paths: vec![SpawnPath {
                name: "cp_home_spawn".to_string(),
                nodes: vec![SpawnNode {
                    pos: Vec3::new(110.0, 6.0, 110.0),
                    yaw: 1.25,
                }],
            }],
            regions: Vec::new(),
            // Any map but TAT1: the rule under test applies everywhere else.
            map: "kamino1".to_string(),
        }
    }

    /// Card ERAM: a player seated on team 1 (its army's own team in the
    /// mission picked) is a team-1 unit and starts at team 1's post, where
    /// the team-0 player of the same battle starts at team 0's.
    #[test]
    fn a_player_seated_on_team_1_starts_at_its_own_post() {
        let start = |seat: u8| {
            let t = TeamTemplate::default_trooper();
            let mut w = WalkSim::with_teams_seat(
                Sim::empty(1),
                None,
                Vec3::ZERO,
                [t.clone(), t],
                None,
                seat,
            );
            w.sim.collision = island_world();
            w.setup_conquest(&rescue_layout(), [0, 0], true);
            let u = &w.sim.units[w.player.index as usize];
            (u.team, u.soldier.pos)
        };
        // `cp_enemy` (team 2) has no spawn path: its one node is beside it.
        let own = Vec3::new(103.0, 6.0, 103.0);
        assert_eq!(start(1), (1, own));
        let (team, pos) = start(0);
        assert_eq!(team, 0);
        assert_ne!(pos, own);
    }

    /// `game --map` with no mission, no `--ai` and no `--spawn`: the player
    /// starts on nothing, and the rescue moves them to the lowest-index
    /// FRIENDLY post whose own spawn node has ground — node position, node
    /// yaw, probed y.
    #[test]
    fn rescue_spawn_moves_a_groundless_player_to_the_first_friendly_post() {
        let mut w = WalkSim::new(1, Vec3::ZERO, None);
        w.sim.collision = island_world();
        assert_eq!(
            w.sim.ground_under(w.soldier().pos, false),
            core::collision::NO_GROUND,
            "the authored spawn has nothing under it"
        );

        let moved = w.rescue_spawn(&rescue_layout());

        assert_eq!(moved, Some(("cp_home".to_string(), 6.0)));
        assert_eq!(w.soldier().pos, Vec3::new(110.0, 6.0, 110.0));
        assert_eq!(w.soldier().yaw, 1.25);
    }

    /// A player standing on the island: `game --map` gave them ground, and
    /// they STILL start at the post — the half queue item WS widened (24 maps
    /// had ground under the default point and started at an arbitrary spot in
    /// the terrain anyway). The move is silent — it returns `None`, because
    /// the caller's line is the no-ground rescue's — and the two posts before
    /// `cp_home` are skipped for their own reasons, not by ordering luck.
    #[test]
    fn a_grounded_player_starts_at_the_first_friendly_post_too() {
        let mut w = WalkSim::new(1, Vec3::new(100.0, 0.0, 100.0), None);
        w.sim.collision = island_world();
        // Standing on the island's top face (`Sim::spawn` ran before the
        // collision world was there, so put them where it would have).
        w.sim.units[w.player.index as usize].soldier.pos = Vec3::new(100.0, 6.0, 100.0);
        assert!(
            w.sim.ground_under(w.soldier().pos, false) > core::collision::NO_GROUND,
            "the authored spawn has ground under it"
        );

        assert_eq!(w.rescue_spawn(&rescue_layout()), None, "moved silently");

        assert_eq!(w.soldier().pos, Vec3::new(110.0, 6.0, 110.0));
        assert_eq!(w.soldier().yaw, 1.25);
    }

    /// The two ways the rule must do nothing: the historic walk start (TAT1,
    /// where the replay referees run) with ground under it, and a map whose
    /// friendly posts are all over the void.
    #[test]
    fn rescue_spawn_leaves_tat1_and_a_hopeless_map_alone() {
        // Byte for byte the test above, except the map's name.
        let mut w = WalkSim::new(1, Vec3::new(100.0, 0.0, 100.0), None);
        w.sim.collision = island_world();
        w.sim.units[w.player.index as usize].soldier.pos = Vec3::new(100.0, 6.0, 100.0);
        let mut tat1 = rescue_layout();
        tat1.map = super::HISTORIC_WALK_START_MAP.to_string();
        assert_eq!(w.rescue_spawn(&tat1), None, "the historic start stands");
        assert_eq!(
            w.soldier().pos,
            Vec3::new(100.0, 6.0, 100.0),
            "not moved to the post"
        );

        let mut v = WalkSim::new(1, Vec3::ZERO, None);
        v.sim.collision = island_world();
        let mut layout = rescue_layout();
        layout.cps.retain(|c| c.name != "cp_home");
        // `Sim::spawn` already dropped it to the sentinel, and with no
        // rescuable post it stays there — a broken map stays broken rather
        // than being teleported somewhere unauthored.
        let stranded = v.soldier().pos;
        assert_eq!(stranded, Vec3::new(0.0, core::collision::NO_GROUND, 0.0));
        assert_eq!(v.rescue_spawn(&layout), None, "no friendly post has ground");
        assert_eq!(v.soldier().pos, stranded, "left where it was");
    }

    /// The player's handle follows his slot through a respawn (its
    /// generation goes up): before cleanse 16 it kept the first life's, so
    /// every `sim.unit(walk.player)` read None from the first death on.
    #[test]
    fn the_player_handle_follows_a_respawn() {
        use core::cp::CpDef;
        use core::region::Region;
        let mut walk = WalkSim::new(42, Vec3::ZERO, None);
        let (capture_time, neutralize_time) = CpDef::class_times(false);
        walk.sim.add_cp(
            CpDef {
                name: "cp".into(),
                pos: Vec3::ZERO,
                capture: Region::sphere(Vec3::ZERO, 8.0),
                control: None,
                value_bleed: 0,
                value_atk: [0, 0],
                value_def: [0, 0],
                spawn_nodes: vec![(Vec3::new(0.0, 0.0, 3.0), 0.0)],
                capture_time,
                neutralize_time,
            },
            Some(0),
        );
        let first = walk.player;
        assert!(walk.player_on_foot());
        walk.sim
            .kill_environment(first, core::sim::DeathCause::Fall);
        let mut ticks = 0;
        while !walk.sim.units[first.index as usize].alive {
            walk.step_tick(&[PadState::default()]);
            ticks += 1;
            assert!(ticks < 3000, "the player never respawned");
        }
        assert_eq!(walk.player.index, first.index);
        assert_eq!(walk.player.gen, first.gen + 1);
        assert_eq!(Some(walk.player), walk.sim.players[0]);
        assert!(walk.player_on_foot(), "the new life is found");
        assert_eq!(walk.player_team(), 0);
    }

    /// Card SSELB: behind the level start's spawn screens the view is the
    /// mission's camera shot; a screen change cuts to another shot, a move on
    /// the same screen does not; the first spawn ends the backdrop for good.
    #[test]
    fn the_level_start_view_is_the_camera_shot_until_the_spawn() {
        use crate::spawn_backdrop::{Backdrop, CameraShot};
        let mut w = WalkSim::new(1, Vec3::ZERO, Some(flat_ground()));
        let chase = w.camera();
        assert!(w.sim.open_spawn_screens(0, true));
        let shots: Vec<CameraShot> = (0..4)
            .map(|i| CameraShot::from_call([1.0, 0.0, 0.0, 0.0, 10.0 * i as f32, 50.0, 0.0]))
            .collect();
        w.backdrop = Backdrop::new(shots, 3);
        w.follow_backdrop();
        assert_eq!(w.camera().position, Vec3::new(0.0, 50.0, 0.0), "shot 0");
        let pad = |b: u32| PadState {
            buttons: b,
            ..PadState::default()
        };
        // Down moves the highlight: the same screen, the same shot.
        w.step_tick(&[pad(buttons::SPAWN_POST_DOWN)]);
        w.step_tick(&[pad(0)]);
        assert_eq!(w.sim.side_choice[0], core::sim::SideChoice::Team2);
        assert_eq!(w.camera().position.x, 0.0);
        // X takes the side: the map screen, another shot.
        w.step_tick(&[pad(buttons::SPAWN_ACCEPT)]);
        w.step_tick(&[pad(0)]);
        assert_eq!(w.sim.spawn_stage[0], core::sim::SpawnStage::Map);
        assert_ne!(w.camera().position.x, 0.0);
        assert_eq!(w.camera().position.y, 50.0);
        // On the field: the backdrop is gone and the chase camera is back.
        let i = w.player.index as usize;
        w.sim.units[i].alive = true;
        w.follow_backdrop();
        assert!(w.backdrop.is_none());
        assert_eq!(w.camera().position, chase.position);
    }
}
