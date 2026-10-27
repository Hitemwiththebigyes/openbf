//! A combat unit: the P3 soldier plus health, its weapons (the one in its
//! hands and the class's other slots, card GREN), a team and a controller. Units live in `Sim::units` slots addressed by generation-
//! checked handles (the originals re-validate stale entity pointers by id —
//! e.g. bolt hit target `+0xb4` guard; we do the same with `gen`).

use crate::ai::AiState;
use crate::collision::Capsule;
use crate::fmath;
use crate::soldier::{GroundRules, Soldier, SoldierConstants, Stance};
use crate::weapon::{Health, HealthType, WeaponClass, WeaponState};
use glam::Vec3;

/// Where a bot's next life starts (card BSPN; re-command-walker-post §6
/// [O]): one of the sim's command posts by index, or a live command walker —
/// a post like any other to the PS2's draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnPost {
    Post(u16),
    Walker(crate::vehicle::VehicleHandle),
}

impl SpawnPost {
    /// A kind byte (1 post, 2 walker), then the index and generation.
    pub fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        let (kind, index, gen) = match *self {
            SpawnPost::Post(i) => (1u8, u32::from(i), 0),
            SpawnPost::Walker(h) => (2, h.index, h.gen),
        };
        eat(&[kind]);
        eat(&index.to_le_bytes());
        eat(&gen.to_le_bytes());
    }
}

/// Generation-checked unit reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UnitHandle {
    pub index: u32,
    pub gen: u32,
}

/// Who drives a unit's pad each tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Controller {
    /// Driven by the pad in player slot `slot` (`Sim::step`'s `pads[slot]`);
    /// a slot with no pad this tick idles.
    Player { slot: u8 },
    /// Stands still (dummy target).
    Idle,
    /// Dummy pacer: constant planar velocity, reversed every `period`
    /// ticks (the P3 aim-assist rig).
    Pacer { vel: Vec3, period: u32 },
    /// AI synthesizes pad input; state lives in `crate::ai`.
    Ai,
}

/// PROVISIONAL soldier collision radius (the BF1 aim-assist rig used 0.5;
/// the real prim radius comes from the soldier ODF / collision RE).
pub const SOLDIER_RADIUS: f32 = 0.5;

/// BF2 per-stance capsule heights (DAT_0061ed50: 1.9 / 1.6 / 0.55).
pub fn capsule_height(stance: Stance) -> f32 {
    match stance {
        // The tumble and get-up take the crouch's posture, a corpse the
        // prone's (card KNOCK, `re-knockback.md` §2 / §3: posture 1 / 2).
        Stance::Crouch | Stance::Tumble | Stance::GetUp => 1.6,
        Stance::Prone | Stance::Roll | Stance::Dead => 0.55,
        _ => 1.9,
    }
}

/// PROVISIONAL muzzle height: 75% of the capsule (the real fire point is
/// the weapon model's hp_fire hardpoint — needs skinned soldiers).
pub fn muzzle_height(stance: Stance) -> f32 {
    capsule_height(stance) * 0.75
}

/// The soldier's acquisition box, local to the feet (the model box the
/// PS2's aim acquisition reads, `analysis/re-aim-assist.md` §3.6: [V-RAM]
/// one BF1 soldier's box, which does not change with stance). PROVISIONAL
/// for every class: the RAM showed seven shapes (r 1.387 to 1.704) and the
/// port carries none per class — the modl vis box is not it (y max 2.0877,
/// z max 0.215 on the stormtrooper).
pub const AIM_BOX_MIN: Vec3 = Vec3::new(-0.929, -0.001, -0.196);
pub const AIM_BOX_MAX: Vec3 = Vec3::new(0.929, 2.099, 0.204);

/// The pitch every PS2 soldier spawns with (§10.3: 0xbd8f5c29, both games).
/// The port gives it to a human; a bot's is card FPNT's.
pub const SPAWN_PITCH: f32 = -0.07;

/// One camera section of a soldier class (`analysis/re-aim-assist.md`
/// §10.1): `EyePointOffset` = `TrackCenter` is `eye_y` above the feet, the
/// camera sits `offset` from it in its own frame (x right, y up, z back) and
/// looks `tilt` radians BELOW the aim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSection {
    pub eye_y: f32,
    pub offset: Vec3,
    pub tilt: f32,
}

/// In split screen the tilt is 0.4 of the authored one (§10.1, the tilt
/// getter B1 FUN_001b58c0 / B2 FUN_001ce9b8).
pub const SPLIT_TILT: f32 = 0.4;

/// A soldier's camera section by game, stance and zoom, [V-RAM] §10.1's
/// table (all player classes alike; the per-class exceptions - the BF1
/// snowtrooper's zoom - are card CAMS's). A zoomed soldier takes the
/// stance's ZOOM section: over the right shoulder, the same in both games
/// (`analysis/re-zoom.md` §5). Sprint is a section of its own on BF2; BF1 has no sprint, so
/// its sprint section is OURS (the maintainer, 2026-10-04, FEEL_NOTES): the stand
/// section moved back by BF2's own step, 0.8 m, so a sprint shows. Jump, roll
/// and fall keep the stand section (PROVISIONAL).
pub fn camera_section(rules: GroundRules, stance: Stance, zoomed: bool) -> CameraSection {
    let bf1 = rules == GroundRules::Bf1;
    if zoomed {
        let (eye_y, y) = match stance {
            Stance::Crouch => (1.3, 0.2),
            Stance::Prone => (0.5, 0.2),
            _ => (1.8, 0.05),
        };
        return CameraSection {
            eye_y,
            offset: Vec3::new(0.4, y, 2.8),
            tilt: f32::to_radians(3.5),
        };
    }
    let (eye_y, y, z, deg) = match stance {
        Stance::Crouch => (
            1.3,
            0.15,
            if bf1 { 3.5 } else { 3.0 },
            if bf1 { 5.0 } else { 10.0 },
        ),
        Stance::Prone => (0.5, 0.0, 3.0, 5.0),
        Stance::Sprint if !bf1 => (1.8, 0.0, 4.0, 10.0),
        Stance::Sprint => (1.8, 0.0, 4.3, 5.0),
        _ if bf1 => (1.8, 0.0, 3.5, 5.0),
        _ => (1.8, 0.0, 3.2, 10.0),
    };
    CameraSection {
        eye_y,
        offset: Vec3::new(0.0, y, z),
        tilt: f32::to_radians(deg),
    }
}

/// A human's zoom (`analysis/re-zoom.md` §3-§4): the PS2's zoom flag with
/// the weapon's zoom value and turn divisor at the step it is on, and
/// whether the weapon's zoom is the scope view (`ZoomFirstPerson`). Written
/// only by `Sim::tick_zoom` and the weapon switch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zoom {
    /// 0 = `ZoomMin`, 1 = `ZoomMax`.
    pub step: u8,
    pub value: f32,
    pub divisor: f32,
    pub scope: bool,
}

/// The scope view's eye stands this far behind the scope point along the aim
/// (§5: B1 `DAT_005178b4`; BF2 0.6 while zoomed).
pub fn scope_eye_back(rules: GroundRules) -> f32 {
    match rules {
        GroundRules::Bf1 => 0.3,
        GroundRules::Bf2 => 0.6,
    }
}

/// The fire point's two tables by stance (§10.2 step 2, the same numbers in
/// both games): T0 in the body's frame (x to the soldier's LEFT, y up, z
/// forward), T2 in the aim's (x left, y the aim's up, z along the aim).
fn fire_point_tables(stance: Stance) -> (Vec3, Vec3) {
    match stance {
        Stance::Crouch => (Vec3::new(-0.1, 1.2, 0.0), Vec3::new(-0.12, -0.05, 0.25)),
        Stance::Prone => (Vec3::new(0.06, 0.6, 0.4), Vec3::new(-0.12, -0.20, 0.15)),
        _ => (Vec3::new(0.06, 1.7, 0.0), Vec3::new(-0.12, -0.10, 0.25)),
    }
}

/// The camera the PS2 is about to draw, without its smoothing and collision
/// (§10.2 step 4): where it stands and its forward and up rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdealCamera {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    /// The track centre (= the eye point): what the camera's collision pulls
    /// toward.
    pub center: Vec3,
}

/// The point the acquisition's LOS ray aims at, above the feet by stance
/// (FUN_0020c680's table 0x586350: 1.4 / 0.6 / 0.2, `re-aim-assist.md`
/// §3.6). Roll takes prone's, as the capsule does (PROVISIONAL).
pub fn aim_point_height(stance: Stance) -> f32 {
    match stance {
        Stance::Crouch | Stance::Tumble | Stance::GetUp => 0.6,
        Stance::Prone | Stance::Roll | Stance::Dead => 0.2,
        _ => 1.4,
    }
}

/// One class weapon slot past the primary, as a unit is built with it
/// (card GREN, `analysis/re-grenades.md` §2.1): the PS2's slot index, its
/// channel (0 = the primary trigger's, 1 = the secondary's), the weapon and
/// its `WeaponAmmo` in clips (0 = infinite).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotSpec {
    pub slot: u8,
    pub channel: u8,
    /// Index into `Sim::weapon_classes`.
    pub weapon_class: usize,
    pub clips: u32,
}

/// A weapon a unit holds besides the one in its hands ([`Unit::held`]).
#[derive(Clone, Debug)]
pub struct HeldWeapon {
    pub slot: u8,
    pub channel: u8,
    /// Index into `Sim::weapon_classes`.
    pub class: usize,
    pub state: WeaponState,
}

#[derive(Clone, Debug)]
pub struct Unit {
    pub gen: u32,
    pub alive: bool,
    pub team: u8,
    pub is_player: bool,
    pub controller: Controller,
    pub soldier: Soldier,
    pub consts: SoldierConstants,
    pub health: Health,
    pub weapon: WeaponState,
    /// Index into `Sim::weapon_classes`.
    pub weapon_class: usize,
    /// The class slot `weapon` came from: channel 0's CURRENT slot (the
    /// PS2's cur[0], `re-grenades.md` §2.2).
    pub weapon_slot: u8,
    /// Every other slot the class gives it, each a live weapon (§2.2), in
    /// slot order. Switching channel 0 swaps one of these with `weapon`, so
    /// everything that reads `weapon` / `weapon_class` follows the switch.
    pub held: Vec<HeldWeapon>,
    /// Channel 1's current weapon: an index into `held` (the PS2's cur[1]).
    pub cur1: Option<u8>,
    /// The active channel (§2.3): 1 while the secondary trigger or its
    /// weapon's fire state has the soldier, else 0.
    pub active_channel: u8,
    /// The two switch buttons as they were last tick (bit 0 channel 0's,
    /// bit 1 channel 1's): the switch is an edge.
    pub switch_held: u8,
    /// The originals' integer AI skill 1..=20 (`core::skill`), 0 for a player
    /// or a dummy. Written at spawn/respawn from `Sim::difficulty`
    /// (PLAN "PHASE C DESIGN" decision 4: "every bot receives the tier's
    /// skill at spawn"). HASHED: a round of skill-8 bots is a different world
    /// from a round of skill-20 ones. The VECTOR is derived per think
    /// (`Skill::from_level`), never stored.
    pub skill: u8,
    /// The unit's own aim-error block (card CLSE2). HASHED.
    pub aim: AimBlock,
    /// Bookkeeping for traces/awards.
    pub kills: u32,
    pub deaths: u32,
    /// Minimal-AI scratch state (Controller::Ai).
    pub ai: AiState,
    /// Tick at which a dead unit may respawn (respawn timer, BF2 4.0 s).
    pub respawn_at: Option<u64>,
    /// Spawn-wave slot assigned at death (wave, slot) — see `SpawnWaves`.
    pub spawn_slot: Option<(u32, u32)>,
    /// The post a dead BOT will spawn at (card BSPN): drawn when its respawn
    /// timer runs out, kept until the spawn while the team may still spawn
    /// there (`Sim::keep_or_draw_spawn_post`). `None` alive, and for a human.
    pub spawn_post: Option<SpawnPost>,
    /// How to rebuild this unit on respawn.
    pub spec: UnitSpec,
    /// Soldier-class `CapturePosts` (re-cp-capture.md): soldiers/walkerdroids
    /// count toward captures, vehicles and BF1 heroes don't.
    pub capture_posts: bool,
    /// Card KNOCK (`analysis/re-knockback.md` §3): a dead unit whose body is
    /// still in the world - it flies, tumbles and slides
    /// ([`crate::soldier::Stance::Dead`]), blasts kick and push it, and it
    /// leaves after [`crate::soldier::CORPSE_SECS`] dead or at the respawn.
    /// A unit killed in a seat, or by the world, has none.
    pub body: bool,
    /// The kill's item drop waits for the dead state (B1 FUN_001ffec0 runs
    /// from the death block: a thrown soldier drops where he lands).
    pub drop_due: bool,
    /// The total push the WALL PASS applied to this unit this tick (`Sim::step`
    /// phase 3a, `CollisionWorld::push_out`), zero when it touched nothing.
    /// Horizontal by construction.
    ///
    /// This is the port's stand-in for the PS2's side-contact accumulator
    /// (`soldier+0x970`, RE_LOG "Local obstacle avoidance"): the steering
    /// reads it as "I am in contact with something on this side" even when the
    /// wall sweep ahead of me is clear, and takes the side-step's side from it
    /// (PLAN "PHASE A.2 DESIGN" decision 2). It is RECORDED, never re-applied
    /// — the push itself is already in `soldier.pos` — so no unit's motion,
    /// the player's least of all, depends on it.
    pub wall_push: Vec3,
    /// Tick this unit last FIRED, `u64::MAX` for never. Perception's hearing
    /// reads it (`crate::perception::scan`): a shot inside `HEAR_RADIUS`
    /// since the listener's last think puts the shooter in its memory with no
    /// line of sight at all. Written where `Event::Shot` is pushed
    /// (`Sim::step` phase 4) and HASHED — who has fired recently changes what
    /// everyone around them knows.
    pub last_shot_tick: u64,
    /// The squad order a player has on this unit (Phase D, `core::squad`):
    /// `None` for every unit that has none, which is every unit in a round
    /// nobody commands. HASHED only when `Some` — and NOTHING at all when it is
    /// `None`, not even a 0 byte (see `hash_into` below) — so a
    /// command-free sim hashes exactly as it did before Phase D — that is what
    /// keeps the replay gates' three values where they are. The only writers
    /// are `Sim::take_order` (phase 0b), `Sim::issue_command`, `Sim::clear_all`,
    /// `squad::on_unit_died`, phase 0b's Disperse expiry and the release in
    /// `Sim::step`'s AI write-back, all in `Sim::step`'s fixed order.
    pub command: Option<crate::squad::Command>,
    /// The order this unit was given and has not thought about yet (card
    /// SQACK): taken at its next think. HASHED only when `Some`.
    pub order: Option<crate::squad::Pending>,
    /// `(shooter, tick)` of the last bolt that HIT this unit, written next to
    /// `Event::Hit` in `Sim::resolve_bolt`. Being shot records the shooter at
    /// any range (design decision 2's hearing rule), which is what stops a
    /// unit standing still while it is killed from behind. Hashed.
    pub last_hit: Option<(UnitHandle, u64)>,
    /// The seat this unit rides in (card VENT, `crate::vehicle`), the unit's
    /// half of the two-sided occupancy; written only by `Sim::seat_unit` /
    /// `Sim::unseat_unit`. HASHED only when `Some`, like `command`, so an
    /// unmounted sim hashes as before.
    pub mount: Option<crate::vehicle::Mount>,
    /// BF2's timed offense / defense buffs (card PWUP, `sim/pickups.rs`).
    /// HASHED only while one runs.
    pub buffs: crate::sim::Buffs,
    /// The zoom of the weapon in hand, `None` when not zoomed: every bot,
    /// always. HASHED only when `Some`, so a round nobody zooms in hashes as
    /// it did.
    pub zoom: Option<Zoom>,
}

impl Unit {
    pub fn active_weapon(&self) -> (usize, u8, &WeaponState) {
        (self.active_channel == 1)
            .then(|| self.held.get(usize::from(self.cur1?)))
            .flatten()
            .map_or((self.weapon_class, self.weapon_slot, &self.weapon), |h| {
                (h.class, h.slot, &h.state)
            })
    }

    pub fn capsule(&self) -> Capsule {
        Capsule {
            base: self.soldier.pos,
            height: capsule_height(self.soldier.stance),
            radius: SOLDIER_RADIUS,
        }
    }

    /// Aim frame from yaw/pitch (render::Camera convention: yaw 0 = -Z,
    /// positive yaw left): (forward, right, up).
    pub fn aim_frame(&self) -> (Vec3, Vec3, Vec3) {
        let (sy, cy) = fmath::sin_cos(self.soldier.yaw);
        let (sp, cp) = fmath::sin_cos(self.soldier.pitch);
        let fwd = Vec3::new(-sy * cp, sp, -cy * cp);
        let right = Vec3::new(cy, 0.0, -sy);
        let up = right.cross(fwd).normalize();
        (fwd, right, up)
    }

    pub fn muzzle(&self) -> Vec3 {
        self.soldier.pos + Vec3::Y * muzzle_height(self.soldier.stance)
    }

    /// The acquisition's bounds (`re-aim-assist.md` §3.6): the box centre
    /// and half its diagonal. The box's 4 mm horizontal offset is dropped,
    /// so the centre is straight above the feet.
    pub fn aim_bounds(&self) -> (Vec3, f32) {
        let mid = (AIM_BOX_MIN + AIM_BOX_MAX) * 0.5;
        let r = (AIM_BOX_MAX - AIM_BOX_MIN).length() * 0.5;
        (self.soldier.pos + Vec3::Y * mid.y, r)
    }

    /// The camera section this unit takes now: the stance's, or its ZOOM
    /// section while zoomed.
    pub fn section(&self, rules: GroundRules) -> CameraSection {
        camera_section(rules, self.soldier.stance, self.zoom.is_some())
    }

    /// Is the view the scope's (zoomed on a `ZoomFirstPerson` weapon)?
    pub fn scoped(&self) -> bool {
        self.zoom.is_some_and(|z| z.scope)
    }

    pub fn look_scale(&self) -> f32 {
        self.zoom.map_or(1.0, |z| 1.0 / z.value.max(1.0))
    }

    /// The point the scope looks from and the scoped shot leaves (§10.2
    /// step 7): BF1 the fire point; BF2 the shoulder pivot plus the gun's
    /// reach along the aim, without the fire point's two sideways terms.
    pub fn scope_point(&self, rules: GroundRules) -> Vec3 {
        match rules {
            GroundRules::Bf1 => self.fire_point(),
            GroundRules::Bf2 => {
                let (t0, t2) = fire_point_tables(self.soldier.stance);
                let (d, right, _) = self.aim_frame();
                let ahead = Vec3::new(right.z, 0.0, -right.x);
                self.soldier.pos + Vec3::Y * t0.y + ahead * t0.z - right * t0.x + d * t2.z
            }
        }
    }

    /// The ideal chase camera (§10.2 step 4): the frame of the aim turned
    /// down by the section's tilt, standing at the track centre plus the
    /// section's offset in that frame. `views` is the number of views drawn
    /// (split screen takes [`SPLIT_TILT`] of the tilt). In the scope view
    /// (`re-zoom.md` §5) it is the eye [`scope_eye_back`] behind the scope
    /// point, looking along the aim with no tilt.
    pub fn ideal_camera(&self, rules: GroundRules, views: usize) -> IdealCamera {
        let (d, right, up) = self.aim_frame();
        if self.scoped() {
            let position = self.scope_point(rules) - d * scope_eye_back(rules);
            return IdealCamera {
                position,
                forward: d,
                up,
                center: position,
            };
        }
        let sec = self.section(rules);
        let tilt = sec.tilt * if views > 1 { SPLIT_TILT } else { 1.0 };
        let (st, ct) = fmath::sin_cos(tilt);
        let forward = d * ct - up * st;
        let cam_up = up * ct + d * st;
        let center = self.soldier.pos + Vec3::Y * sec.eye_y;
        IdealCamera {
            position: center + right * sec.offset.x + cam_up * sec.offset.y
                - forward * sec.offset.z,
            forward,
            up: cam_up,
            center,
        }
    }

    /// A human's aim ray (§10.2 step 4): it leaves the ideal camera's
    /// position along the aim, its origin A slid forward until it is abreast
    /// of the eye point. The reticle is where this ray goes on screen and the
    /// acquisition looks along it. In the scope view it leaves the scope
    /// point (step 7).
    pub fn aim_ray(&self, rules: GroundRules, views: usize) -> (Vec3, Vec3) {
        let d = self.aim_frame().0;
        if self.scoped() {
            return (self.scope_point(rules), d);
        }
        let cam = self.ideal_camera(rules, views);
        (cam.position + d * (cam.center - cam.position).dot(d), d)
    }

    /// The fire point F (§10.2 step 2): a shoulder pivot by stance with the
    /// gun ahead along the aim, below it and to the right. A table, not the
    /// weapon model's hardpoint. Only a human's shot leaves it today (bots
    /// keep [`Unit::muzzle`], card FPNT).
    pub fn fire_point(&self) -> Vec3 {
        let (t0, t2) = fire_point_tables(self.soldier.stance);
        let (d, right, up) = self.aim_frame();
        let ahead = Vec3::new(right.z, 0.0, -right.x);
        self.soldier.pos + Vec3::Y * t0.y + ahead * t0.z - right * (t0.x + t2.x)
            + up * t2.y
            + d * t2.z
    }

    /// Where the acquisition's LOS ray aims ([`aim_point_height`]).
    pub fn aim_point(&self) -> Vec3 {
        self.soldier.pos + Vec3::Y * aim_point_height(self.soldier.stance)
    }

    /// Is `p` inside this unit's acquisition box, taken in its model frame
    /// (the self-box skip, `re-aim-assist.md` §3.6)? The frame is the
    /// unit's heading: x along its right, z along its facing (the box is
    /// symmetric in z to 4 mm, so that sign hardly matters).
    pub fn in_aim_box(&self, p: Vec3) -> bool {
        let (sy, cy) = fmath::sin_cos(self.soldier.yaw);
        let d = p - self.soldier.pos;
        let local = Vec3::new(d.x * cy - d.z * sy, d.y, -d.x * sy - d.z * cy);
        local.cmpge(AIM_BOX_MIN).all() && local.cmple(AIM_BOX_MAX).all()
    }

    /// The PS2's object kind (`+0xac & 7`, `re-aim-assist.md` §3.6), the
    /// bit the weapon's `target_kind` tests. The PS2 copies it from the
    /// class (class+0x48); the port takes the unit's HealthType, which uses
    /// the same numbering (person 0, animal 1, droid 2 …) — [D], PROVISIONAL.
    pub fn kind(&self) -> u32 {
        self.health.health_type as u32
    }

    /// Feed into the sim hash.
    pub fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(&self.gen.to_le_bytes());
        eat(&[
            self.alive as u8,
            self.team,
            self.is_player as u8,
            self.skill,
        ]);
        self.soldier.hash_into(eat);
        eat(&self.health.cur.to_bits().to_le_bytes());
        self.weapon.hash_into(eat);
        self.ai.hash_into(eat);
        self.aim.hash_into(eat);
        eat(&self.respawn_at.unwrap_or(u64::MAX).to_le_bytes());
        let (w, s) = self.spawn_slot.unwrap_or((u32::MAX, u32::MAX));
        eat(&w.to_le_bytes());
        eat(&s.to_le_bytes());
        // Eaten only while a dead bot holds a choice, so a sim nobody has
        // died in keeps its hash (the pinned command-free format).
        if let Some(post) = self.spawn_post {
            post.hash_into(eat);
        }
        for v in [self.wall_push.x, self.wall_push.y, self.wall_push.z] {
            eat(&v.to_bits().to_le_bytes());
        }
        if self.body {
            eat(&[1, self.drop_due as u8]);
        }
        // Phase D: the squad order, and NOTHING AT ALL when there is none (not
        // even a 0 byte — brief addendum 3). That is what keeps a command-free
        // sim's hash byte-identical to the pre-Phase-D one, which is the whole
        // reason the three replay gate values do not move; the cost is that the
        // stream is only self-delimiting because `Command::hash_into` leads
        // with a 1 tag (`squad.rs`).
        if let Some(c) = self.command {
            c.hash_into(eat);
        }
        if let Some(p) = self.order {
            p.hash_into(eat);
        }
        eat(&self.last_shot_tick.to_le_bytes());
        match self.last_hit {
            Some((h, t)) => {
                eat(&[1]);
                eat(&h.index.to_le_bytes());
                eat(&h.gen.to_le_bytes());
                eat(&t.to_le_bytes());
            }
            None => eat(&[0]),
        }
        // Which CLASS this unit came out of its team's table as. A class
        // change is state: the same slot respawned as a sniper instead of a
        // trooper is a different world, and the weapon/health/constants it
        // carries are already hashed through `weapon`/`health`/`soldier`
        // only indirectly (two classes can share a rifle).
        eat(&[self.spec.class]);
        // The other slots (GREN): nothing at all for a unit that holds one
        // weapon, so a sim with no class slots hashes as it did.
        if !self.held.is_empty() {
            eat(&[
                self.held.len() as u8,
                self.weapon_slot,
                self.cur1.unwrap_or(u8::MAX),
                self.active_channel,
                self.switch_held,
            ]);
            for w in &self.held {
                eat(&[w.slot]);
                w.state.hash_into(eat);
            }
        }
        if let Some(m) = self.mount {
            m.hash_into(eat);
        }
        if self.buffs.active() {
            self.buffs.hash_into(eat);
        }
        if let Some(z) = self.zoom {
            eat(&[0x5a, z.step, z.scope as u8]);
            eat(&z.value.to_bits().to_le_bytes());
            eat(&z.divisor.to_bits().to_le_bytes());
        }
    }
}

/// One entry of a team's class table (`Sim::classes[team]`): everything the
/// sim needs to BUILD a unit of that class, plus the mission's per-class
/// counts. Built by the game layer from the ODF chain
/// (`game::roster::ClassRoster`); `core` never reads a class file.
///
/// SETUP data, like `Sim::weapon_classes`: written before the round starts,
/// never mutated by `step`, and NOT part of [`crate::sim::Sim::hash`] — what
/// each unit took out of it (`UnitSpec::class`) is.
#[derive(Clone, Debug)]
pub struct UnitClass {
    pub consts: SoldierConstants,
    pub max_health: f32,
    pub health_type: HealthType,
    /// Index into `Sim::weapon_classes` (the class's PRIMARY weapon).
    pub weapon_class: usize,
    /// Authored WeaponAmmo: total clips including a loaded clip; zero is unlimited.
    pub total_clips: u32,
    /// The primary's class slot and the class's other slots (GREN).
    pub weapon_slot: u8,
    pub slots: std::sync::Arc<[SlotSpec]>,
    pub jet: bool,
    pub capture_posts: bool,
    /// What a unit of this class drops when it dies (card PWUP).
    pub drops: crate::sim::DropTable,
    /// Mission `AddUnitClass` count / `SetupTeams` slot minimum — how many of
    /// this class the team fields before the fill class takes over
    /// (`Sim::ai_class`).
    pub min: u32,
    /// BF2 `SetupTeams` slot maximum; BF1 authors none.
    pub max: Option<u32>,
    /// The ODF's `PointsToUnlock` (BF2: every officer 8, the specials 12; BF1
    /// none = 0): the points a HUMAN needs to take this class, and with a
    /// cost the class's `max` caps the humans' pick too
    /// (`analysis/re-spawn-flow.md` §4.3, [`crate::sim::Sim::class_refusal`]).
    /// Bots are never refused.
    pub unlock_points: u32,
}

impl UnitClass {
    /// A spawn spec for this class. `class` is the class's own index in the
    /// team's table — the caller passes it because the table does not know
    /// where it lives.
    /// `skill` is the difficulty tier's base skill; it reaches the spec only
    /// on an `Ai` controller (a player and a dummy carry 0). `Sim::spawn`
    /// enforces the same rule on every other spawn path.
    pub fn spec(
        &self,
        team: u8,
        pos: Vec3,
        yaw: f32,
        controller: Controller,
        class: u8,
        skill: u8,
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
            total_clips: self.total_clips,
            weapon_slot: self.weapon_slot,
            slots: self.slots.clone(),
            capture_posts: self.capture_posts,
            jet: self.jet,
            drops: self.drops,
            class,
            skill: crate::skill_for(controller, skill),
        }
    }
}

/// The block every BF2 unit carries about ITSELF as a target, and the
/// direction its own aim error points (card CLSE2, `analysis/re-soldier-ai.md`
/// 18.1: B2 `FUN_002ae010` / `FUN_002ae7b8`). Every bot that shoots at the
/// unit takes the unit's `err` (18.6: knocked in on the running PS2); `dir`
/// is the unit's own, for the shots IT fires. `Sim::step` thinks it: a bot's
/// every 5th tick, a human's every tick. A BF1 round has no block (18.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimBlock {
    /// The error, metres before the radius's factors
    /// (`ai::gunner_foot::radius`): it decays to the skill's floor and grows
    /// with the unit's CHANGE of velocity, at most [`crate::ai::ERR_MAX`].
    pub err: f32,
    /// What growth is paid from: it fills at half the decay, at most
    /// [`AimBlock::BUDGET_MAX`], so the error can rise by no more than half
    /// of what it decays plus what was saved up.
    pub budget: f32,
    /// The unit's velocity at the last think.
    pub vel: Vec3,
    /// The axis the direction turns about; it walks (a U(-1, 1) cube is
    /// added every think and the sum normalised).
    pub axis: Vec3,
    /// The turn so far: 2 x dt radians about `axis` every think.
    pub quat: glam::Quat,
    /// `quat` applied to +Y: where this unit's error vector points. Zero
    /// until the first think.
    pub dir: Vec3,
}

impl Default for AimBlock {
    /// The PS2 constructor's values (B2:232960-232990): no error, an empty
    /// budget, the identity turn.
    fn default() -> Self {
        Self {
            err: 0.0,
            budget: 0.0,
            vel: Vec3::ZERO,
            axis: Vec3::ZERO,
            quat: glam::Quat::IDENTITY,
            dir: Vec3::ZERO,
        }
    }
}

impl AimBlock {
    /// The budget's cap (B2:233051-233056).
    pub const BUDGET_MAX: f32 = 5.0;
    /// A bot's block thinks every this many ticks (the PS2's every 5th
    /// update at its level of detail 0 / 1; the port has one level).
    pub const PERIOD: u64 = 5;

    /// One think `dt` seconds after the last: `vel` and `top` are the unit's
    /// velocity and MaxSpeed, `decay` (m/s) and `floor` (m) the skill's of
    /// who shoots at it, `jolt` the U(-1, 1) cube the axis walks by.
    pub fn think(&mut self, dt: f32, vel: Vec3, top: f32, decay: f32, floor: f32, jolt: Vec3) {
        let d = dt * decay;
        self.add(-d, floor);
        self.budget = (self.budget + 0.5 * d).clamp(0.0, Self::BUDGET_MAX);
        let delta = (self.vel - vel).length();
        self.vel = vel;
        self.add(crate::ai::ERR_GROWTH * delta / top.max(0.1), floor);
        self.axis = (self.axis + jolt).normalize_or_zero();
        let (s, c) = fmath::sin_cos(dt);
        let turn = glam::Quat::from_xyzw(self.axis.x * s, self.axis.y * s, self.axis.z * s, c);
        // OURS: normalised, so 60,000 ticks of products do not drift.
        self.quat = (self.quat * turn).normalize();
        self.dir = self.quat * Vec3::Y;
    }

    /// B2 `FUN_002ae7b8`: growth is cut to the budget and taken off it; then
    /// the floor, and the 2 m cap.
    fn add(&mut self, mut x: f32, floor: f32) {
        if x > 0.0 {
            x = x.min(self.budget);
            self.budget -= x;
        }
        self.err += x;
        self.err = if self.err < floor {
            floor
        } else {
            self.err.min(crate::ai::ERR_MAX)
        };
    }

    /// Feed into the sim hash.
    pub fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        let q = self.quat;
        for v in [
            self.err,
            self.budget,
            self.vel.x,
            self.vel.y,
            self.vel.z,
            self.axis.x,
            self.axis.y,
            self.axis.z,
            q.x,
            q.y,
            q.z,
            q.w,
        ] {
            eat(&v.to_bits().to_le_bytes());
        }
    }
}

/// How to build a unit (class constants resolved by the game layer).
#[derive(Clone, Debug)]
pub struct UnitSpec {
    pub team: u8,
    /// Player slot this unit belongs to (`None` = AI/dummy). The slot is what
    /// `Sim::players` indexes; [`UnitSpec::is_player`] is derived from it.
    pub player_slot: Option<u8>,
    pub controller: Controller,
    pub pos: Vec3,
    pub yaw: f32,
    pub consts: SoldierConstants,
    pub max_health: f32,
    pub health_type: HealthType,
    pub weapon_class: usize,
    /// Authored WeaponAmmo: total clips including a loaded clip; zero is unlimited.
    pub total_clips: u32,
    /// The primary's class slot (0 on every built-in spec).
    pub weapon_slot: u8,
    /// The class's other weapon slots (empty on every built-in spec).
    pub slots: std::sync::Arc<[SlotSpec]>,
    pub capture_posts: bool,
    /// Authored jet/jump-pack eligibility, loaded from `JetJump`. The planning
    /// graph keeps `jet_jump` arcs (`Arc::JET_JUMP`) off jetless routes (PLAN
    /// "PHASE A.2 DESIGN" decision 4); this flag does not implement jet flight.
    pub jet: bool,
    /// The class's drop table (card PWUP; empty on every built-in spec: a
    /// death spends no roll and drops nothing).
    pub drops: crate::sim::DropTable,
    /// The integer AI skill this unit spawns with (0 = not a bot). Filled by
    /// `Sim::spawn`/`Sim::respawn_slot` from `Sim::difficulty`; the default on
    /// every hand-built spec is 0.
    pub skill: u8,
    /// Index into the TEAM'S class table (`Sim::classes[team]`) this unit was
    /// built from. `0` on every built-in spec ([`UnitSpec::trooper`], the
    /// player constructors) — a sim with no class tables has one implied
    /// class per team and every unit is it.
    pub class: u8,
}

impl UnitSpec {
    /// Is this a PLAYER unit (tickets, cheats, the wall pass, the alive-AI
    /// cap)? Derived: a unit is a player exactly when it owns a slot.
    pub fn is_player(&self) -> bool {
        self.player_slot.is_some()
    }

    /// Dummy target / default trooper: BF1 all_inf_soldier health 300.
    pub fn trooper(team: u8, pos: Vec3) -> Self {
        Self {
            team,
            player_slot: None,
            controller: Controller::Idle,
            pos,
            yaw: 0.0,
            consts: SoldierConstants::default(),
            max_health: 300.0,
            health_type: HealthType::Person,
            weapon_class: 0,
            total_clips: 6,
            weapon_slot: 0,
            slots: std::sync::Arc::new([]),
            jet: false,
            capture_posts: true,
            drops: crate::sim::DropTable::default(),
            class: 0,
            skill: 0,
        }
    }

    /// A player driven by pad slot `slot`.
    pub fn player_in_slot(slot: u8, team: u8, pos: Vec3) -> Self {
        Self {
            player_slot: Some(slot),
            controller: Controller::Player { slot },
            ..Self::trooper(team, pos)
        }
    }

    /// The local player: pad slot 0.
    pub fn player(team: u8, pos: Vec3) -> Self {
        Self::player_in_slot(0, team, pos)
    }

    /// `classes` is `Sim::weapon_classes`.
    pub fn build(&self, gen: u32, classes: &[WeaponClass]) -> Unit {
        let class = &classes[self.weapon_class];
        let mut soldier = Soldier::new(self.pos);
        soldier.yaw = self.yaw;
        if self.is_player() {
            soldier.pitch = SPAWN_PITCH;
        }
        let mut held: Vec<HeldWeapon> = self
            .slots
            .iter()
            .map(|s| HeldWeapon {
                slot: s.slot,
                channel: s.channel,
                class: s.weapon_class,
                state: WeaponState::with_clips(&classes[s.weapon_class], self.is_player(), s.clips),
            })
            .collect();
        // Channel 1's current weapon is its first slot (§2.2); only the two
        // current weapons are selected.
        let cur1 = held.iter().position(|w| w.channel == 1);
        for (i, w) in held.iter_mut().enumerate() {
            w.state.active = Some(i) == cur1;
        }
        Unit {
            gen,
            alive: true,
            team: self.team,
            is_player: self.is_player(),
            controller: self.controller,
            soldier,
            consts: self.consts,
            health: Health::new(self.max_health, self.health_type),
            weapon: WeaponState::with_clips(class, self.is_player(), self.total_clips),
            weapon_class: self.weapon_class,
            weapon_slot: self.weapon_slot,
            held,
            cur1: cur1.map(|i| i as u8),
            active_channel: 0,
            switch_held: 0,
            skill: self.skill,
            aim: AimBlock::default(),
            kills: 0,
            deaths: 0,
            ai: AiState::default(),
            respawn_at: None,
            spawn_slot: None,
            spawn_post: None,
            spec: self.clone(),
            capture_posts: self.capture_posts,
            wall_push: Vec3::ZERO,
            body: false,
            drop_due: false,
            last_shot_tick: u64::MAX,
            last_hit: None,
            command: None,
            order: None,
            mount: None,
            buffs: crate::sim::Buffs::default(),
            zoom: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = AimBlock::PERIOD as f32 * crate::TICK_DT;
    /// Skill 17's decay (m/s) and floor (m), a soldier's MaxSpeed.
    const DECAY: f32 = 0.942_105_3;
    const FLOOR: f32 = 0.589_473_6;
    const TOP: f32 = 6.0;

    fn think(b: &mut AimBlock, vel: Vec3) {
        b.think(DT, vel, TOP, DECAY, FLOOR, Vec3::ZERO);
    }

    /// Card CLSE2 (`analysis/re-soldier-ai.md` 18.1): a unit's error decays
    /// to the floor of who shoots at it and its budget fills at HALF the
    /// decay, at most 5.
    #[test]
    fn a_units_error_decays_to_the_floor_and_saves_a_budget() {
        let mut b = AimBlock::default();
        think(&mut b, Vec3::ZERO);
        assert_eq!(
            b.err, FLOOR,
            "from the constructor's 0 straight to the floor"
        );
        assert!((b.budget - 0.5 * DT * DECAY).abs() < 1e-6);
        b.err = 2.0;
        for _ in 0..6 {
            think(&mut b, Vec3::ZERO);
        }
        assert!(
            (b.err - (2.0 - DECAY)).abs() < 1e-4,
            "a second's decay: {}",
            b.err
        );
        assert!((b.budget - 0.5 * 7.0 * DT * DECAY).abs() < 1e-5);
        for _ in 0..600 {
            think(&mut b, Vec3::ZERO);
        }
        assert_eq!((b.err, b.budget), (FLOOR, AimBlock::BUDGET_MAX));
    }

    /// The error grows with the unit's CHANGE of velocity between two thinks
    /// - a start 0.455, a reversal at full speed 0.91, a straight run nothing
    /// - and only as far as the budget pays.
    #[test]
    fn a_units_error_grows_with_its_change_of_velocity_out_of_the_budget() {
        let run = Vec3::new(TOP, 0.0, 0.0);
        let rich = || AimBlock {
            err: FLOOR,
            budget: 3.0,
            ..AimBlock::default()
        };
        // A start from rest: + 0.455, less the think's own decay.
        let mut b = rich();
        think(&mut b, run);
        assert!((b.err - (FLOOR + 0.455)).abs() < 1e-5, "{}", b.err);
        assert!((b.budget - (3.0 + 0.5 * DT * DECAY - 0.455)).abs() < 1e-5);
        // Running straight on adds nothing: it decays.
        let was = b.err;
        think(&mut b, run);
        assert!((b.err - (was - DT * DECAY)).abs() < 1e-5, "{}", b.err);
        // A reversal at full speed: + 0.91. Height counts (3-D).
        let was = b.err;
        think(&mut b, -run);
        assert!(
            (b.err - (was - DT * DECAY + 0.91)).abs() < 1e-5,
            "{}",
            b.err
        );
        let mut up = rich();
        think(&mut up, Vec3::new(0.0, TOP, 0.0));
        assert!((up.err - (FLOOR + 0.455)).abs() < 1e-5);
        // Never over 2 m.
        for k in 0..40 {
            b.budget = 3.0;
            think(&mut b, if k % 2 == 0 { run } else { -run });
        }
        assert_eq!(b.err, crate::ai::ERR_MAX);
        // An empty budget pays only what the think itself saved.
        let mut poor = AimBlock {
            err: FLOOR,
            ..AimBlock::default()
        };
        think(&mut poor, run);
        assert!(
            (poor.err - (FLOOR + 0.5 * DT * DECAY)).abs() < 1e-5,
            "{}",
            poor.err
        );
        assert_eq!(poor.budget, 0.0);
    }

    /// The direction tumbles: a unit vector that turns 2 x dt radians a
    /// think about a walking axis - it is not drawn anew.
    #[test]
    fn a_units_error_direction_tumbles() {
        let angle = |a: Vec3, b: Vec3| fmath::atan2(a.cross(b).length(), a.dot(b));
        let mut b = AimBlock::default();
        assert_eq!(b.dir, Vec3::ZERO);
        let mut last = Vec3::Y;
        let (mut most, mut least, mut far) = (0.0f32, 9.0f32, 0.0f32);
        for k in 0..600u64 {
            let draw = |s: u32| 2.0 * crate::ai::unit_unit(3, k, 1, s) - 1.0;
            b.think(
                DT,
                Vec3::ZERO,
                TOP,
                DECAY,
                FLOOR,
                Vec3::new(draw(1), draw(2), draw(3)),
            );
            assert!((b.dir.length() - 1.0).abs() < 1e-4, "unit length at {k}");
            assert!((b.axis.length() - 1.0).abs() < 1e-4);
            let turned = angle(b.dir, last);
            most = most.max(turned);
            least = least.min(turned);
            far = far.max(angle(b.dir, Vec3::Y));
            last = b.dir;
        }
        assert!(most <= 2.0 * DT + 1e-3, "at most 2 x dt a think: {most}");
        assert!(
            most > 1.5 * DT,
            "and that much about an axis across it: {most}"
        );
        assert!(least < DT, "less about an axis along it: {least}");
        assert!(
            far > 2.0,
            "over a minute and a half it goes all the way round: {far}"
        );
    }

    /// Every one of the twelve floats `hash_into` eats moves the hash (`dir`
    /// is not one: every think writes it from `quat`).
    #[test]
    fn the_aim_block_is_hashed_field_by_field() {
        let hash = |b: &AimBlock| {
            let mut out = Vec::new();
            b.hash_into(&mut |x: &[u8]| out.extend_from_slice(x));
            out
        };
        let base = AimBlock::default();
        let edits: [fn(&mut AimBlock); 12] = [
            |b| b.err = 1.0,
            |b| b.budget = 1.0,
            |b| b.vel.x = 1.0,
            |b| b.vel.y = 1.0,
            |b| b.vel.z = 1.0,
            |b| b.axis.x = 1.0,
            |b| b.axis.y = 1.0,
            |b| b.axis.z = 1.0,
            |b| b.quat.x = 1.0,
            |b| b.quat.y = 1.0,
            |b| b.quat.z = 1.0,
            |b| b.quat.w = 2.0,
        ];
        let mut seen = vec![hash(&base)];
        for e in edits {
            let mut b = base;
            e(&mut b);
            let h = hash(&b);
            assert!(!seen.contains(&h));
            seen.push(h);
        }
    }
}
