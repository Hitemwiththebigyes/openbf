//! Blaster weapons: fire cycle, spread, ordnance damage — transcribed from the
//! decomp (spec: analysis/re-blaster-damage.md, lead-verified 2026-08-30 by
//! reading the bodies cited below). Everything here is data-driven from the
//! weapon (`wpnc`) and ordnance (`ordc`) classes; the BF1/BF2 rifle
//! constructors are byte-verified ODF values kept as test fixtures.
//!
//! Sources per item (BF1 SWBFRONT.ELF / BF2 SWBFRNT2.ELF):
//! - weapon state machine: BF1 weapon update (fn @ decomp line 208432, state
//!   at weapon+0xcc): idle + fire-input predicate → vtbl+0x70 start event +
//!   vtbl+0x74 trigger init (`FUN_00288c58`), firing state runs the cycle,
//!   `FUN_002891c0` = "cycle finished, back to idle" query;
//! - trigger init `FUN_00288c58` / BF2 `FUN_0032e758`: charge = accum /
//!   MaxPressedTime (1.0 if none), accum = 0, refire = ShotDelay (BF2:
//!   uniform(min,max)), salvoCounter = SalvoCount, salvoTimer =
//!   InitialSalvoDelay — the FIRST salvo comes from the trigger, not from
//!   the refire accumulator;
//! - fire cycle `FUN_00288d10` / BF2 `FUN_0032e8c0`: dt-chunk loop (eps 2^-15),
//!   refire branch (remainder-carrying accumulator; refuses TriggerSingle;
//!   gates = fire predicate, !reloading, !blocked), salvo branch (shots,
//!   ammo/heat, spread growth once per salvo, back-to-back salvos while the
//!   salvo timer stays ≤ 0, BF2 salvo delay uniform(min,max)); a salvo the
//!   refire branch starts with no dt left fires at the next update — even
//!   after trigger release: the only salvo cancel is weapon+0xc4 == 0, the
//!   weapon's ACTIVE (drawn) flag (set by select `FUN_00284148`, cleared by
//!   deselect `FUN_002841f0`), not the fire button;
//! - fire-input predicate `FUN_00289150`: overheated → false; charge weapon
//!   → fire on release (cmd == −1); automatic → held (cmd > 0); TriggerSingle
//!   → fresh press only (cmd == 1);
//! - heat (BF1 weapon update): overheated when heat ≥ 1.0, `heat -= dt ×
//!   HeatRecoverRate`, clears when heat < HeatThreshold;
//! - spread: ctor `FUN_00288858` (human init −SpreadThreshold, AI 0), per-tick
//!   recovery `FUN_00288930` / BF2 `FUN_0032e200`, getters `FUN_002889e8` /
//!   `FUN_00288c20` (max(s,0) + postureBase + axis spread), posture base
//!   `FUN_002889b0` / BF2 `0x32e438`;
//!   OURS (card SPRD): the floor sits [`WeaponClass::tight_hold`] lower, so
//!   held fire stays tight [`TIGHT_HOLD_SECS`] longer than the PS2's;
//! - shot direction `FUN_002892a8` / BF2 `FUN_0032f208`: triangular pitch then
//!   yaw offsets (4 LCG steps each), local dir (sinY·cosP, −sinP, cosY·cosP);
//! - ODF parsers: BF1 `FUN_002864f0`/`FUN_00289b48` (weapon), `FUN_002738a8`+
//!   `FUN_002774b0` (ordnance); BF2 `FUN_00328fb0`/`FUN_00330810`,
//!   `FUN_00310620`+`FUN_00314b98`;
//! - damage descriptor: BF1 `FUN_00272f88` (flat) / BF2 `FUN_00310178`
//!   (bolt-AGE falloff + team bonus); application: BF1 `FUN_001bc190` /
//!   BF2 `FUN_001dfb38` (HealthType scale, friendly-fire scale, shield phase;
//!   NO locational damage exists in either game).
//!
//! NEGATIVE RESULT carried into the design: the soldier `AimFactor*` /
//! `AimValue` ODF keys are consumed nowhere in either ELF — accuracy is
//! weapon-side only. Open (report §10): reload state machine (provisional
//! auto-reload below), charge-weapon accumulator semantics, BF2 wind-up
//! timers (rifles: 0), magnet-target velocity adjust at spawn.

use crate::fmath;
use crate::lcg::GameLcg;
use glam::Vec3;

/// Degrees→radians literal the ODF parsers multiply by (EE float).
pub const DEG: f32 = 0.017_453_294;
/// Fire-cycle loop epsilon, 2^-15 (both games).
pub const FIRE_EPS: f32 = 3.051_757_8e-5;

/// Key/value access to a class's PROP list (game layer implements it over
/// the baked ClassDb with BASE-chain inheritance). Uncracked BF2 keys are
/// looked up by hash.
pub trait PropSource {
    fn prop(&self, key: &str) -> Option<&str>;
    fn prop_hash(&self, hash: u32) -> Option<&str>;
}

pub(crate) fn pf(src: &dyn PropSource, key: &str) -> Option<f32> {
    src.prop(key)?.split_whitespace().next()?.parse().ok()
}

pub(crate) fn pf_hash(src: &dyn PropSource, hash: u32) -> Option<f32> {
    src.prop_hash(hash)?.trim().parse().ok()
}

pub(crate) fn pair(src: &dyn PropSource, key: &str) -> Option<[f32; 2]> {
    let s = src.prop(key)?;
    let mut it = s.split_whitespace().filter_map(|t| t.parse::<f32>().ok());
    let a = it.next()?;
    let b = it.next().unwrap_or(a);
    Some([a, b])
}

mod ability;
mod dispenser;
mod ordnance;
mod repair;
pub use ability::Ability;
pub use dispenser::Dispenser;
pub use ordnance::{
    blast_falloff, DamageDesc, ExplosionClass, Falloff, Health, HealthType, OrdBlend, OrdKind,
    OrdKnot, OrdnanceClass, EXPLOSION_OFFSET, HASH_FALLOFF_DURATION, HASH_FALLOFF_END_DAMAGE,
    HASH_FALLOFF_START, HASH_TEAM_BONUS_SCALE, SHELL_G,
};
pub use repair::Repair;

// ── Weapon class ──────────────────────────────────────────────────────────

/// OURS (card SPRD, the maintainer 2026-10-04: "have it stay tight a little while
/// longer"): held fire keeps its tight spread this many seconds longer than
/// the PS2's before the bloom starts, for the player and the bots alike. The
/// PS2's rule is 0.0 (the ODF values are untouched).
pub const TIGHT_HOLD_SECS: f32 = 1.5;
/// The same for a bot's weapon (its bursts are shorter than this, so a
/// burst no longer blooms at all); 0.0 = the PS2's.
pub const TIGHT_HOLD_SECS_AI: f32 = 1.5;
/// The hold's buffer is never more than this, nor than the class's own
/// SpreadLimit: a class with a near-zero ShotDelay (a launcher its reload
/// paces, BF1's commando pistol, the AT-AT's chin gun) would otherwise bank
/// a buffer no fire could use up.
pub const TIGHT_HOLD_CAP: f32 = 6.0 * DEG;

/// The zoom-spread slot keys, the third column of `posture_spread`:
/// `StandAimSpread`, `CrouchAimSpread`, `ProneAimSpread` (`re-zoom.md` §2;
/// no disc class authors one).
pub const HASH_ZOOM_SPREAD: [u32; 3] = [0x5BC0_137F, 0x67BF_C1D7, 0x7D9D_A8BD];

/// One charge stage of a [`WeaponClass`]: from `threshold` (the charge it
/// must exceed) the shot is `class`'s; `index` is its own entry in
/// `Sim::weapon_classes` (`usize::MAX` until registered), which its rounds
/// carry.
#[derive(Clone, Debug, PartialEq)]
pub struct ChargeStage {
    pub threshold: f32,
    pub class: Box<WeaponClass>,
    pub index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeaponClass {
    pub name: String,
    /// 0 = infinite (RoundsPerClip < 1).
    pub rounds_per_clip: u32,
    pub ammo_per_shot: u32,
    pub reload_time: f32,
    pub trigger_single: bool,
    /// `ZoomMax`, the scope's maximum magnification (BF2 class+0xb4 / BF1
    /// +0x7c). Used by human zoom and as the third gate of
    /// [`WeaponClass::ai_distance_table`]
    /// (`analysis/re-ai-sniper.md` §2). Class-ctor default 1.0 in both games
    /// (B2:319499 / B1:209461), and the loaders coerce a written 0 to 1.0.
    pub zoom_max: f32,
    /// `ZoomMin`, the first step of a human's zoom (`analysis/re-zoom.md`
    /// §2; default 1.0, a written 0 reads 1.0).
    pub zoom_min: f32,
    /// `ZoomRate`: non-zero makes the zoom a continuous one on the stick
    /// (turret and vehicle weapons only on the discs; the port has no
    /// continuous zoom yet, and a class with a rate takes no second step).
    pub zoom_rate: f32,
    /// `ZoomTurnDivisorMin` / `Max`: what the turn rates are divided by at
    /// each step. `ZoomMin` / `ZoomMax` are copied here when read (§2); no
    /// disc class authors the divisor keys.
    pub zoom_turn_div: [f32; 2],
    /// `ZoomFirstPerson`: a soldier's zoom on this weapon is the scope view.
    pub zoom_first_person: bool,
    /// `AIBubbleSizeMultiplier` (BF2 class +0x1e8, card CLSE2,
    /// `analysis/re-soldier-ai.md` 18.4): the factor on the error vector of
    /// a bot that fires this weapon - 1.25 the shotgun, 0.6 the sniper
    /// rifles. BF1 reads none of the three keys.
    pub ai_bubble_size: f32,
    /// `AIBubbleScaleDistDivider` (+0x1ec): the error vector grows with the
    /// range d, x (d / this + 0.5).
    pub ai_bubble_dist: f32,
    /// `AIBubbleScaleClamp` (+0x1f0): the most that range factor is.
    pub ai_bubble_clamp: f32,
    /// > 0 = charge weapon (fires on release).
    pub max_pressed_time: f32,
    pub lock_on_range: f32,
    /// The acquisition cone's slope, radians (C+0xd8: `LockOnAngle` × DEG, or
    /// `AutoAimSize` × 0.072; `analysis/re-aim-assist.md` §3.2). 0 = a cone
    /// only as wide as the target's radius.
    pub lock_on_angle: f32,
    /// The acquisition's reach (C+0xd0, `re-aim-assist.md` §3.2): the query
    /// box's half-side and the cone's length. See [`WeaponClass::from_props`]
    /// for the order-free rule the port uses.
    pub extreme_range: f32,
    /// The AI's range triple, m (card FGHT, `analysis/re-soldier-ai.md` 4.2,
    /// 4.5): `MinRange`, `OptimalRange`, `MaxRange`. A bot fights a target
    /// inside `max + 10`, stands to shoot inside the middle of the band and
    /// circles on a ring about `optimal`. The class defaults 0 / 30 / 60 are
    /// the note's ("defaults 30 / - / 60"); every shipped infantry weapon
    /// writes all three. Only the AI reads it.
    pub ai_range: [f32; 3],
    /// `BarrageMin` / `BarrageMax` (card CLSE4, `analysis/re-soldier-ai.md`
    /// 20.3; BF2 class +0x214 / +0x218, BF1 +0x1bc / +0x1c0): the bounds of
    /// a bot's burst with this weapon. The constructor's 3 / 6 in both
    /// games; on the discs only BF2's flyer weapons and its defence turret
    /// write them. Only the AI reads the three.
    pub barrage: [u8; 2],
    /// `BarrageDelay` (+0x21c / +0x1c4), s: the pause after a burst, before
    /// BF2's skill factor. The constructor's 2.0.
    pub barrage_delay: f32,
    /// Relations the magnet may take (C+0xf4): bit `rel + 1` for rel −1
    /// enemy / 0 neutral / 1 friend — TargetEnemy 1, TargetNeutral 2,
    /// TargetFriendly 4. Class-ctor default 1 (B1:209497).
    pub target_team: u32,
    /// Kinds the magnet may take (C+0xf8): bit `kind` — TargetPerson 1,
    /// Animal 2, Droid 4, Vehicle 8, Building 0x10. The ctor default
    /// 0xffffff9f has every kind the port has on.
    pub target_kind: u32,
    /// Kinds the AI may use this weapon on (card GRAI, BF2 class+0x124,
    /// `analysis/re-soldier-ai.md` 4.3): bit `kind`, as `target_kind`. The
    /// `AITarget*` keys (BF2 only on the discs); a class that writes none
    /// keeps its `Target*` bits — PROVISIONAL, the PS2's default of the AI
    /// word was not read.
    pub ai_target_kind: u32,
    /// Heat units per shot; 0 = never overheats.
    pub heat_per_shot: f32,
    pub heat_recover_rate: f32,
    pub heat_threshold: f32,
    /// [min, max] seconds; BF1 single value → both equal.
    pub shot_delay: [f32; 2],
    pub salvo_count: u32,
    pub shots_per_salvo: u32,
    pub salvo_delay: [f32; 2],
    pub initial_salvo_delay: f32,
    /// Radians.
    pub pitch_spread: f32,
    pub yaw_spread: f32,
    pub shot_tilts: Vec<(f32, f32)>,
    /// The charge stages after the root (issue #29, `analysis/re-weapon-charge.md`
    /// §1-§3: B1 `FUN_002864f0` / `FUN_00285108`, B2 `FUN_00328fb0` /
    /// `FUN_00327168`): each `NextCharge` clones the class as it stands, and
    /// the keys after it go to the clone. While the trigger is held the
    /// weapon steps to the next stage while pressed / MaxPressedTime is over
    /// that stage's threshold; the shot is built from the stage reached (its
    /// ordnance, ShotsPerSalvo, offsets, HeatPerShot). Empty: no stages.
    pub stages: Vec<ChargeStage>,
    /// [stand, crouch, prone] × [still, move, zoom], radians.
    pub posture_spread: [[f32; 3]; 3],
    pub spread_per_shot: f32,
    pub spread_recover_rate: f32,
    pub spread_threshold: f32,
    pub spread_limit: f32,
    pub ordnance: OrdnanceClass,
    /// Sound property names (FireSound / ReloadSound / FireEmptySound /
    /// WeaponChangeSound).
    pub fire_sound: Option<String>,
    pub fire_loop_sound: Option<String>,
    pub muzzle_effect: Option<String>,
    pub reload_sound: Option<String>,
    pub empty_sound: Option<String>,
    pub change_sound: Option<String>,
    /// The muzzle flash's look (card FXMF, `analysis/re-muzzle-flash.md`):
    /// render-only, replayed from the class chain by the class loader.
    /// `None` = none drawn (a sabre, a thrown weapon, a test class).
    pub flash: Option<crate::flash::MuzzleFlashLook>,
    /// The `grenade` weapon class's own keys (card GREN); `None` for every
    /// other weapon.
    pub thrown: Option<Thrown>,
    pub offhand_weapon: bool,
    pub repair: Option<Repair>,
    pub dispenser: Option<Dispenser>,
    pub mine: Option<Dispenser<crate::mine::MineClass>>,
    pub ability: Option<Ability>,
    pub remote: Option<Box<crate::remote::RemoteWeapon>>,
}

/// What the `grenade` weapon class adds (`analysis/re-grenades.md` §3.1; B1
/// `FUN_0028f1f0` / setter `FUN_0028f308`, B2 `FUN_00337fd0` /
/// `FUN_00338190`). Its cycle is the cannon's with one salvo: the press, the
/// `InitialSalvoDelay` wind-up, the throw, then `ShotDelay` — and an empty
/// clip reloads at once from the reserve (§3.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thrown {
    /// Added to a human's aim.y before the throw (ctor default 0.3).
    pub shot_elevate: f32,
    /// The weapon model is not drawn while its fire state runs (default 1).
    pub hide_on_fire: bool,
    /// BF2 throws from the hand, not the fire point: (up, forward) metres in
    /// the soldier's frame. PROVISIONAL constant — the PS2 samples the throw
    /// clip at the release (§3.3); RAM of the thermal's strong throw gave
    /// (0, 1.8183, 0.7433) from the soldier's position. `None` = BF1's fire
    /// point.
    pub hand: Option<(f32, f32)>,
}

impl Thrown {
    /// BF2's strong-throw hand offset ([`Thrown::hand`]) from the port's
    /// feet: the PS2's 1.8183 m up is from a soldier position that rests
    /// 0.02 m INTO the floor (the kept penetration, re-soldier-ground §1;
    /// feet y -0.02 on terrain y 0 in all 15 throws of re-grenades §13.1),
    /// the port's feet stand on it. With 1.8183 the grenade flew 2 cm high
    /// and its first contact came an update late in 5 of the 7 throws (card
    /// GRNP: rest 0.05-0.2 m and 1-3 updates off; 14 of 15 exact with this).
    pub const BF2_HAND: (f32, f32) = (1.8183 - 0.02, 0.7433);

    /// A BF1 map's thrown grenades take BF2's throw (the maintainer, 2026-10-04: the
    /// throw, distance, sticks and bounces are "the ps2 bf2 grenade physics";
    /// card GRNP, a named deviation from the BF1 disc). `false` puts BF1's
    /// own throw back ([`Thrown::throw_as_bf2`] is then never called).
    pub const BF1_THROWS_AS_BF2: bool = true;
    /// The BF2 thermal detonator's `InitialSalvoDelay` and its ordnance's
    /// `LifeSpan` (re-grenades §9; BF1's are 0.3 and 0.9).
    pub const BF2_WIND_UP: f32 = 0.51;
    pub const BF2_FUSE: f32 = 0.6;

    /// Turn a BF1 thrown weapon into one thrown as BF2's thermal detonator
    /// is: released from the hand after BF2's wind-up, at rest by BF2's
    /// slow-bounce count, BF2's fuse from the rest, sticking to what BF2's
    /// thermal sticks to (vehicles, buildings, unbuilt buildings) besides
    /// its own mask. Its speed, gravity and elevation are the discs' own
    /// (equal in both games); its damage, radius and ammo stay BF1's.
    pub fn throw_as_bf2(w: &mut WeaponClass) {
        use crate::sticky::stick;
        let Some(t) = w.thrown.as_mut() else {
            return;
        };
        t.hand = Some(Self::BF2_HAND);
        w.initial_salvo_delay = Self::BF2_WIND_UP;
        w.ordnance.life_span = Self::BF2_FUSE;
        if let Some(s) = w.ordnance.sticky.as_mut() {
            s.bf2 = true;
            s.stick_mask |= stick::VEHICLE | stick::BUILDING | stick::BUILDING_UNBUILT;
        }
    }

    /// The grenade class over a base weapon parsed by
    /// [`WeaponClass::from_props`]: its constructor defaults where the class
    /// authors nothing (`TriggerSingle` 1, `ReloadTime` 0.6, `ShotDelay`
    /// 0.125, `InitialSalvoDelay` 0.3 / BF2 0.5), then its own keys.
    pub fn apply(w: &mut WeaponClass, src: &dyn PropSource, bf2: bool) {
        if pf(src, "TriggerSingle").is_none() {
            w.trigger_single = true;
        }
        if pf(src, "ReloadTime").is_none() {
            w.reload_time = 0.6;
        }
        if pair(src, "ShotDelay").is_none() {
            w.shot_delay = [0.125, 0.125];
        }
        if pf(src, "InitialSalvoDelay").is_none() {
            w.initial_salvo_delay = if bf2 { 0.5 } else { 0.3 };
        }
        w.thrown = Some(Thrown {
            shot_elevate: pf(src, "ShotElevate").unwrap_or(0.3),
            hide_on_fire: pf(src, "HideOnFire").is_none_or(|v| v != 0.0),
            hand: bf2.then_some(Self::BF2_HAND),
        });
    }
}

impl Default for WeaponClass {
    /// WeaponCannonClass ctor defaults where traced (ShotsPerSalvo 1,
    /// SalvoCount 1); untraced fields use PROVISIONAL defaults (ours) — every
    /// shipped weapon sets them.
    fn default() -> Self {
        Self {
            name: String::new(),
            rounds_per_clip: 0,
            ammo_per_shot: 1,
            reload_time: 0.01,
            trigger_single: false,
            zoom_max: 1.0,
            zoom_min: 1.0,
            zoom_rate: 0.0,
            zoom_turn_div: [1.0, 1.0],
            zoom_first_person: false,
            ai_bubble_size: 1.0,
            ai_bubble_dist: 20.0,
            ai_bubble_clamp: 10.0,
            max_pressed_time: 0.0,
            lock_on_range: 0.0,
            lock_on_angle: 0.0,
            extreme_range: 0.0,
            ai_range: [0.0, 30.0, 60.0],
            barrage: [3, 6],
            barrage_delay: 2.0,
            target_team: 1,
            target_kind: 0x1f,
            ai_target_kind: 0x1f,
            heat_per_shot: 0.0,
            heat_recover_rate: 0.0,
            heat_threshold: 0.0,
            shot_delay: [0.2, 0.2],
            salvo_count: 1,
            shots_per_salvo: 1,
            salvo_delay: [0.0, 0.0],
            initial_salvo_delay: 0.0,
            pitch_spread: 0.0,
            yaw_spread: 0.0,
            shot_tilts: Vec::new(),
            stages: Vec::new(),
            posture_spread: [[0.0; 3]; 3],
            spread_per_shot: 0.0,
            spread_recover_rate: 0.0,
            spread_threshold: 0.0,
            spread_limit: 0.0,
            ordnance: OrdnanceClass::default(),
            fire_sound: None,
            fire_loop_sound: None,
            muzzle_effect: None,
            reload_sound: None,
            empty_sound: None,
            change_sound: None,
            flash: None,
            thrown: None,
            offhand_weapon: false,
            repair: None,
            dispenser: None,
            mine: None,
            ability: None,
            remote: None,
        }
    }
}

impl WeaponClass {
    /// The stage a charge reaches: the PS2 steps on while the charge is over
    /// the next stage's threshold (B2 `FUN_00327168`). 0 is the root.
    pub fn stage_for(&self, charge: f32) -> usize {
        self.stages
            .iter()
            .take_while(|s| charge > s.threshold)
            .count()
    }

    /// Stage `k`'s class (0, or past the last: the root / the last).
    pub fn stage(&self, k: usize) -> &WeaponClass {
        match k {
            0 => self,
            k => self.stages.get(k - 1).map_or(self, |s| &*s.class),
        }
    }

    /// Stage `k`'s entry in `Sim::weapon_classes` for a root registered at
    /// `root` (an unregistered stage: the root's).
    pub fn stage_index(&self, root: usize, k: usize) -> usize {
        match k {
            0 => root,
            k => self
                .stages
                .get(k - 1)
                .map(|s| s.index)
                .filter(|i| *i != usize::MAX)
                .unwrap_or(root),
        }
    }

    /// OURS (card SPRD, a named deviation; FEEL_NOTES): the extra free
    /// buffer under the spread state's floor, radians - what
    /// [`TIGHT_HOLD_SECS`] ([`TIGHT_HOLD_SECS_AI`] for a bot) of fire at the
    /// weapon's full cadence would grow (`mult` = the shooter's
    /// [`WeaponState::spread_mult`]), capped at the class's SpreadLimit and
    /// [`TIGHT_HOLD_CAP`]. 0 for a weapon whose spread never outruns its
    /// recovery.
    pub fn tight_hold(&self, human: bool, mult: f32) -> f32 {
        let mean = |d: [f32; 2]| 0.5 * (d[0] + d[1]);
        let salvos = self.salvo_count.max(1) as f32;
        let period = mean(self.shot_delay)
            + self.initial_salvo_delay
            + (salvos - 1.0) * mean(self.salvo_delay);
        if period <= 0.0 {
            return 0.0;
        }
        let net = salvos * mult * self.spread_per_shot / period - self.spread_recover_rate;
        let secs = if human {
            TIGHT_HOLD_SECS
        } else {
            TIGHT_HOLD_SECS_AI
        };
        (secs * net.max(0.0))
            .min(self.spread_limit)
            .min(TIGHT_HOLD_CAP)
    }

    /// BF1 `all_weap_inf_rifle` + `all_weap_inf_rifle_ord` (SIDE/ALL.LVL,
    /// byte-verified, report §8.1).
    pub fn bf1_rifle() -> Self {
        Self {
            name: "all_weap_inf_rifle".into(),
            rounds_per_clip: 50,
            ammo_per_shot: 1,
            reload_time: 1.5,
            trigger_single: false,
            // The DISC value (`re-ai-sniper.md` §3), not the class-ctor
            // default: this fixture is byte-verified against the ODF and
            // `fill_classes_keep_the_v0_fixtures` compares the two whole.
            // It changes nothing here — the rifle fails the table's other
            // two gates anyway. (LEAD ADDENDUM 4b says 1.0 for this
            // constructor; see the report's Deviations.)
            zoom_max: 2.5,
            zoom_min: 2.5,
            zoom_rate: 0.0,
            zoom_turn_div: [2.5, 2.5],
            zoom_first_person: false,
            ai_bubble_size: 1.0,
            ai_bubble_dist: 20.0,
            ai_bubble_clamp: 10.0,
            max_pressed_time: 0.0,
            lock_on_range: 64.0,
            // LockOnAngle 1.0; the bolt's reach 80 × 1.5 (OrdnanceName after
            // LockOnRange); TargetEnemy 1, TargetPerson/Animal/Droid 1,
            // TargetVehicle/Building 0 (every BF1 side's rifle, disc census).
            lock_on_angle: DEG,
            extreme_range: 120.0,
            ai_range: [0.0, 16.0, 48.0],
            barrage: [3, 6],
            barrage_delay: 2.0,
            target_team: 1,
            target_kind: 0x07,
            ai_target_kind: 0x07,
            heat_per_shot: 0.0,
            heat_recover_rate: 0.0,
            heat_threshold: 0.0,
            shot_delay: [0.2, 0.2],
            salvo_count: 1,
            shots_per_salvo: 1,
            salvo_delay: [0.0, 0.0],
            initial_salvo_delay: 0.0,
            pitch_spread: 0.25 * DEG,
            yaw_spread: 0.25 * DEG,
            shot_tilts: Vec::new(),
            stages: Vec::new(),
            posture_spread: [[0.0; 3]; 3],
            spread_per_shot: 1.1 * DEG,
            spread_recover_rate: 4.8 * DEG,
            spread_threshold: 1.6 * DEG,
            spread_limit: 6.0 * DEG,
            ordnance: OrdnanceClass {
                velocity: 80.0,
                life_span: 1.5,
                max_damage: 60.0,
                scales: [1.0, 1.0, 1.0, 0.0, 0.1, 0.1, 0.1],
                shield_scale: 1.0,
                falloff: None,
                team_bonus_scale: 0.0,
                ordnance_sound: Some("com_weap_inf_ord_hum_sm".into()),
                collision_sound: Some("imp_weap_ord_exp".into()),
                geometry: None,
                // re-bolt §2's BF1 rifle row.
                laser: Some(crate::laser::LaserLook {
                    texture: Some("redlaser_D".into()),
                    glow_color: [253, 43, 43, 100],
                    // Parsed from the disc; BF1 never reads it (§5.1).
                    light_color: [148, 25, 25, 255],
                    light_radius: 4.0,
                    ..crate::laser::LaserLook::rifle()
                }),
                // re-fx §7's census: every surface ord_explosion.
                sticky: None,
                explosion: None,
                kind: OrdKind::Other,
                gravity: 0.0,
                expire: None,
                knots: Vec::new(),
                min_damage: None,
                impact: Some(crate::impact::ImpactLook {
                    soft: Some("ord_explosion".into()),
                    rigid: Some("ord_explosion".into()),
                    building: Some("ord_explosion".into()),
                    terrain: Some("ord_explosion".into()),
                    water: Some("watersplash_ord".into()),
                    shield: Some("ord_explosion".into()),
                    ..Default::default()
                }),
            },
            fire_sound: Some("all_weap_inf_rifle_fire".into()),
            fire_loop_sound: None,
            muzzle_effect: None,
            reload_sound: Some("all_weap_inf_reload_med".into()),
            empty_sound: Some("com_weap_inf_ammo_empty".into()),
            change_sound: Some("all_weap_inf_equip_med".into()),
            // re-muzzle-flash §2's BF1 Alliance rifle row.
            thrown: None,
            offhand_weapon: false,
            repair: None,
            dispenser: None,
            mine: None,
            ability: None,
            remote: None,
            flash: Some(crate::flash::MuzzleFlashLook {
                radius: 0.6,
                length: 0.025,
                color: [255, 100, 100, 255],
                model: Some(crate::flash::BF1_MODEL.into()),
                ..crate::flash::MuzzleFlashLook::default()
            }),
        }
    }

    /// BF2 `rep_weap_inf_rifle` → `com_weap_inf_rifle` + `com_weap_inf_rifle_ord`
    /// (SIDE/REP.LVL, byte-verified, report §8.2).
    pub fn bf2_rifle() -> Self {
        Self {
            name: "com_weap_inf_rifle".into(),
            lock_on_range: 40.0,
            // The bolt's reach 160 × 0.8 (the BF1 rule, PROVISIONAL for BF2:
            // B2 FUN_00328fb0's OrdnanceName default is unread). The Target*
            // bits and LockOnAngle 1.0 are the disc's, as BF1's.
            extreme_range: 128.0,
            ai_range: [0.0, 24.0, 64.0],
            barrage: [3, 6],
            barrage_delay: 2.0,
            spread_per_shot: 1.2 * DEG,
            spread_limit: 5.0 * DEG,
            ordnance: OrdnanceClass {
                velocity: 160.0,
                life_span: 0.8,
                max_damage: 80.0,
                scales: [1.0, 1.0, 1.0, 0.1, 0.1, 0.1, 0.1],
                shield_scale: 1.0,
                falloff: Some(Falloff {
                    start_time: 0.4,
                    duration: 0.2,
                    end_damage: 50.0,
                }),
                team_bonus_scale: 0.25,
                ordnance_sound: Some("com_weap_inf_ord_hum_sm".into()),
                collision_sound: Some("imp_weap_ord_exp".into()),
                geometry: None,
                // re-bolt §2's BF2 Republic rifle row (red End inherited).
                laser: Some(crate::laser::LaserLook {
                    texture: Some("com_sfx_laser_blue".into()),
                    glow_color: [92, 136, 250, 100],
                    end_glow_color: [160, 43, 43, 100],
                    fade_light: true,
                    fade_start: 0.4,
                    fade_duration: 0.2,
                    light_color: [36, 53, 98, 255],
                    end_light_color: [94, 25, 25, 255],
                    light_radius: 4.0,
                    ..crate::laser::LaserLook::rifle()
                }),
                // re-fx §7's census: every surface com_sfx_ord_exp; BF2
                // never reads ImpactEffectShield, so the ctor's stands.
                sticky: None,
                explosion: None,
                kind: OrdKind::Other,
                gravity: 0.0,
                expire: None,
                knots: Vec::new(),
                min_damage: None,
                impact: Some(crate::impact::ImpactLook {
                    soft: Some("com_sfx_ord_exp".into()),
                    rigid: Some("com_sfx_ord_exp".into()),
                    building: Some("com_sfx_ord_exp".into()),
                    terrain: Some("com_sfx_ord_exp".into()),
                    water: Some("com_sfx_watersplash_ord".into()),
                    shield: Some("com_sfx_shieldimpact_sm".into()),
                    ..Default::default()
                }),
            },
            fire_sound: Some("rep_weap_inf_rifle_fire".into()),
            reload_sound: Some("com_weap_inf_reload_med".into()),
            empty_sound: Some("com_weap_inf_ammo_empty".into()),
            change_sound: Some("com_weap_inf_equip_med".into()),
            // re-muzzle-flash §2's BF2 Republic rifle row.
            flash: Some(crate::flash::MuzzleFlashLook {
                radius: 0.6,
                length: 0.025,
                color: [100, 100, 255, 255],
                model: Some(crate::flash::BF2_MODEL.into()),
                ..crate::flash::MuzzleFlashLook::default()
            }),
            ..Self::bf1_rifle()
        }
    }

    pub fn dispenses(&self) -> bool {
        self.dispenser.is_some() || self.mine.is_some() || self.remote.is_some()
    }

    /// Name of the ordnance class this weapon fires (`OrdnanceName` /
    /// legacy `Ordnance`).
    pub fn ordnance_name(src: &dyn PropSource) -> Option<String> {
        src.prop("OrdnanceName")
            .or_else(|| src.prop("Ordnance"))
            .map(|s| s.trim().to_owned())
    }

    /// Parse a `wpnc` PROP list (BF1 FUN_002864f0 + FUN_00289b48; BF2
    /// FUN_00328fb0 + FUN_00330810) with the traced transforms: deg→rad,
    /// ShotDelay/SalvoDelay pairs, min clamps, `MaxSpread` and
    /// `SpreadRecover` legacy aliases, RoundsPerClip < 1 → infinite.
    pub fn from_props(name: &str, src: &dyn PropSource, ordnance: OrdnanceClass) -> Self {
        let mut w = Self {
            name: name.to_owned(),
            ordnance,
            ..Self::default()
        };
        if let Some(v) = pf(src, "RoundsPerClip") {
            w.rounds_per_clip = if v < 1.0 { 0 } else { v as u32 };
        }
        if let Some(v) = pf(src, "AmmoPerShot") {
            w.ammo_per_shot = v.max(0.0) as u32;
        }
        if let Some(v) = pf(src, "ReloadTime") {
            w.reload_time = v.max(0.01);
        }
        if let Some(v) = pf(src, "TriggerSingle") {
            w.trigger_single = v != 0.0;
        }
        if let Some(v) = pf(src, "OffhandWeapon") {
            w.offhand_weapon = v != 0.0;
        }
        // The zoom keys (`analysis/re-zoom.md` §2): a written 0 reads 1.0
        // (B2:320352-320361, B1:209974-209983), and `ZoomMin` / `ZoomMax`
        // are copied to the turn divisors. The PS2 applies the keys in file
        // order, so a divisor key BEFORE its zoom key is overwritten there;
        // here the divisor keys always win (no disc class authors one).
        let one = |v: f32| if v == 0.0 { 1.0 } else { v };
        if let Some(v) = pf(src, "ZoomMin") {
            w.zoom_min = one(v);
            w.zoom_turn_div[0] = w.zoom_min;
        }
        if let Some(v) = pf(src, "ZoomMax") {
            w.zoom_max = one(v);
            w.zoom_turn_div[1] = w.zoom_max;
        }
        if let Some(v) = pf(src, "ZoomRate") {
            w.zoom_rate = v;
        }
        if let Some(v) = pf(src, "ZoomTurnDivisorMin") {
            w.zoom_turn_div[0] = one(v);
        }
        if let Some(v) = pf(src, "ZoomTurnDivisorMax") {
            w.zoom_turn_div[1] = one(v);
        }
        if let Some(v) = pf(src, "ZoomFirstPerson") {
            w.zoom_first_person = v != 0.0;
        }
        if let Some(v) = pf(src, "AIBubbleSizeMultiplier") {
            w.ai_bubble_size = v;
        }
        if let Some(v) = pf(src, "AIBubbleScaleDistDivider") {
            w.ai_bubble_dist = v;
        }
        if let Some(v) = pf(src, "AIBubbleScaleClamp") {
            w.ai_bubble_clamp = v;
        }
        if let Some(v) = pf(src, "MaxPressedTime") {
            w.max_pressed_time = v;
        }
        if let Some(v) = pf(src, "LockOnRange") {
            w.lock_on_range = v;
        }
        // The cone: both keys write C+0xd8, so which wins depends on the
        // ODF's order; no shipped class writes both.
        if let Some(v) = pf(src, "AutoAimSize") {
            w.lock_on_angle = v * 0.072;
        }
        if let Some(v) = pf(src, "LockOnAngle") {
            w.lock_on_angle = v * DEG;
        }
        // ExtremeRange (C+0xd0). On the PS2 the ExtremeRange and LockOnRange
        // setters clamp it up to LockOnRange, and OrdnanceName overwrites it
        // with the ordnance's reach, unclamped (B1:210249-210258,
        // 209842-209855). `PropSource` keeps no order, so the port's rule is
        // the shipped one: every BF1 blaster names its ordnance after
        // LockOnRange and writes no ExtremeRange, so a class with an
        // ordnance and no ExtremeRange takes the reach (Velocity × LifeSpan,
        // FUN_00277d30); only the mortars write ExtremeRange, and they have
        // no magnet (LockOnRange 0).
        w.extreme_range = match pf(src, "ExtremeRange") {
            Some(v) => v.max(w.lock_on_range),
            None if Self::ordnance_name(src).is_some() => {
                w.ordnance.velocity * w.ordnance.life_span
            }
            None => w.lock_on_range,
        };
        for (slot, key) in ["MinRange", "OptimalRange", "MaxRange"].iter().enumerate() {
            if let Some(v) = pf(src, key) {
                w.ai_range[slot] = v;
            }
        }
        for (slot, key) in ["BarrageMin", "BarrageMax"].iter().enumerate() {
            if let Some(v) = pf(src, key) {
                w.barrage[slot] = v.clamp(0.0, 255.0) as u8;
            }
        }
        if let Some(v) = pf(src, "BarrageDelay") {
            w.barrage_delay = v;
        }
        let flag = |key: &str| pf(src, key).map(|v| v != 0.0);
        let update_mask = |word: &mut u32, keys: &[&str]| {
            for (bit, key) in keys.iter().enumerate() {
                match flag(key) {
                    Some(true) => *word |= 1 << bit,
                    Some(false) => *word &= !(1 << bit),
                    None => {}
                }
            }
        };
        update_mask(
            &mut w.target_team,
            &["TargetEnemy", "TargetNeutral", "TargetFriendly"],
        );
        if let Some(all) = flag("TargetAll") {
            w.target_kind = if all { 0x1f } else { 0 };
        }
        update_mask(
            &mut w.target_kind,
            &[
                "TargetPerson",
                "TargetAnimal",
                "TargetDroid",
                "TargetVehicle",
                "TargetBuilding",
            ],
        );
        w.ai_target_kind = w.target_kind;
        update_mask(
            &mut w.ai_target_kind,
            &[
                "AITargetPerson",
                "AITargetAnimal",
                "AITargetDroid",
                "AITargetVehicle",
                "AITargetBuilding",
            ],
        );
        if let Some(v) = pf(src, "HeatPerShot") {
            w.heat_per_shot = v;
        }
        if let Some(v) = pf(src, "HeatRecoverRate") {
            w.heat_recover_rate = v;
        }
        if let Some(v) = pf(src, "HeatThreshold") {
            w.heat_threshold = v;
        }
        if let Some(p) = pair(src, "ShotDelay") {
            w.shot_delay = [p[0].max(0.01), p[1].max(p[0].max(0.01))];
        }
        if let Some(v) = pf(src, "SalvoCount") {
            w.salvo_count = v.max(0.0) as u32;
        }
        if let Some(v) = pf(src, "ShotsPerSalvo") {
            w.shots_per_salvo = v.max(0.0) as u32;
        }
        if let Some(p) = pair(src, "SalvoDelay") {
            w.salvo_delay = p;
        }
        if let Some(v) = pf(src, "InitialSalvoDelay") {
            w.initial_salvo_delay = v;
        }
        // Legacy MaxSpread: pitch = 0.025·v, yaw = 0.05·v (NOT deg→rad).
        if let Some(v) = pf(src, "MaxSpread") {
            w.pitch_spread = 0.025 * v;
            w.yaw_spread = 0.05 * v;
        }
        if let Some(v) = pf(src, "PitchSpread") {
            w.pitch_spread = v * DEG;
        }
        if let Some(v) = pf(src, "YawSpread") {
            w.yaw_spread = v * DEG;
        }
        for (p, stance) in ["Stand", "Crouch", "Prone"].iter().enumerate() {
            if let Some(v) = pf(src, &format!("{stance}StillSpread")) {
                w.posture_spread[p][0] = v * DEG;
            }
            if let Some(v) = pf(src, &format!("{stance}MoveSpread")) {
                w.posture_spread[p][1] = v * DEG;
            }
            if let Some(v) = src
                .prop_hash(HASH_ZOOM_SPREAD[p])
                .and_then(|s| s.trim().parse::<f32>().ok())
            {
                w.posture_spread[p][2] = v * DEG;
            }
        }
        if let Some(v) = pf(src, "SpreadPerShot").or_else(|| {
            src.prop_hash(0xEF49_35B0)
                .and_then(|s| s.trim().parse().ok())
        }) {
            w.spread_per_shot = v * DEG;
        }
        // Legacy SpreadRecover: rate = v × SpreadPerShot, limit = rate / ShotDelay.
        if let Some(v) = pf(src, "SpreadRecover") {
            w.spread_recover_rate = v * w.spread_per_shot;
            w.spread_limit = w.spread_recover_rate / w.shot_delay[0];
        }
        if let Some(v) = pf(src, "SpreadRecoverRate") {
            w.spread_recover_rate = v * DEG;
        }
        if let Some(v) = pf(src, "SpreadThreshold") {
            w.spread_threshold = v * DEG;
        }
        if let Some(v) = pf(src, "SpreadLimit") {
            w.spread_limit = v * DEG;
        }
        w.fire_sound = src.prop("FireSound").map(|s| s.trim().to_owned());
        w.fire_loop_sound = src.prop("FireLoopSound").map(|s| s.trim().to_owned());
        w.muzzle_effect = src.prop("MuzzleFlashEffect").map(|s| s.trim().to_owned());
        w.reload_sound = src.prop("ReloadSound").map(|s| s.trim().to_owned());
        w.empty_sound = src.prop("FireEmptySound").map(|s| s.trim().to_owned());
        w.change_sound = src.prop("WeaponChangeSound").map(|s| s.trim().to_owned());
        w
    }

    pub fn infinite_ammo(&self) -> bool {
        self.rounds_per_clip == 0
    }

    /// Can a human zoom this weapon at all (`re-zoom.md` §4 step 4: a class
    /// whose `ZoomMax` is 1.0 or less never)?
    pub fn zooms(&self) -> bool {
        self.zoom_max > 1.0
    }

    /// The zoom press (B1 `FUN_00283c80`, B2 `FUN_003259a0`, §3): from off,
    /// step 0 (`ZoomMin`); from step 0 on to step 1 (`ZoomMax`) when the two
    /// differ and the class has no `ZoomRate`; else off. `step` is the step
    /// the weapon is at, `None` when not zoomed.
    pub fn zoom_press(&self, step: Option<u8>) -> Option<u8> {
        match step {
            None => Some(0),
            Some(0) if self.zoom_min != self.zoom_max && self.zoom_rate == 0.0 => Some(1),
            Some(_) => None,
        }
    }

    /// The zoom value and the turn divisor at `step`.
    pub fn zoom_at(&self, step: u8) -> (f32, f32) {
        let k = usize::from(step.min(1));
        ([self.zoom_min, self.zoom_max][k], self.zoom_turn_div[k])
    }

    /// The CLASS half of the PS2's AI sniper-dispersion predicate
    /// (`GetYawSpread`, BF2 `FUN_0032e4b8` B2:323340-323346 / BF1
    /// `FUN_002889e8` B1:211248-211254 — `analysis/re-ai-sniper.md` §2
    /// conditions 1, 3 and 4). The other two conditions are the caller's: the
    /// shooter is an AI (fire point owner index < 0) and it has a current
    /// target — [`crate::ai::think`] answers both.
    ///
    /// `>=` is load-bearing: `imp_weap_inf_sniperrifle` ships `ZoomMax`
    /// exactly 6.0 (§3), and a `>` would drop the Scout Trooper out of the
    /// table. Note `yaw_spread` is written by `YawSpread` AND by the legacy
    /// `MaxSpread`, exactly as on the PS2.
    pub fn ai_distance_table(&self) -> bool {
        self.yaw_spread == 0.0 && self.trigger_single && self.zoom_max >= 6.0
    }
}

/// The AI sniper distance table: `(d² upper edge, yaw half-angle in radians)`
/// (B2:323373-323398 / B1:211274-211299, byte-identical constants in both
/// games — `analysis/re-ai-sniper.md` §2). It is a STEP function on the
/// SQUARED distance with strict `<` and no interpolation; the edges are
/// 20/40/60/80/100/120 m. Keeping d² is the PS2's own form and costs no sqrt.
pub const AI_SNIPER_DISPERSION: [(f32, f32); 6] = [
    (400.0, 0.0997),
    (1600.0, 0.0499),
    (3600.0, 0.0333),
    (6400.0, 0.025),
    (10000.0, 0.02),
    (14400.0, 0.0167),
];

/// Past the last band (d ≥ 120 m).
pub const AI_SNIPER_DISPERSION_FAR: f32 = 0.0143;

/// The table value (radians) for a squared shooter-to-target distance.
pub fn ai_sniper_yaw(dist_sq: f32) -> f32 {
    for (edge, v) in AI_SNIPER_DISPERSION {
        if dist_sq < edge {
            return v;
        }
    }
    AI_SNIPER_DISPERSION_FAR
}

// ── Weapon instance state ─────────────────────────────────────────────────

/// A shot the cycle asked the world to spawn (direction is rolled by the
/// world with [`shot_direction`] so the LCG order stays pitch-then-yaw per
/// shot, as in FUN_002892a8).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotSpec {
    pub tilt: (f32, f32),
    pub charge: f32,
    /// The charge stage that fired it ([`WeaponClass::stage`]).
    pub stage: u8,
    /// Half-angle magnitudes (radians) at spawn time.
    pub pitch_spread: f32,
    pub yaw_spread: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireState {
    Idle,
    Firing,
}

/// Per-tick inputs to [`WeaponState::update`].
#[derive(Clone, Copy, Debug, Default)]
pub struct FireInput {
    /// Fire button held this tick.
    pub held: bool,
    /// Soldier-side veto (rolls, dead, in-menu): no shots.
    pub blocked: bool,
    /// Dev cheat: never consume ammo / never reload.
    pub infinite_ammo: bool,
    /// The AI sniper distance table's yaw term for THIS tick, already scaled
    /// by the shooter's skill (`crate::ai::think` — brief D9). `None` for a
    /// human, for a bot with no target and for any class that fails
    /// [`WeaponClass::ai_distance_table`]; then the class `YawSpread` is used
    /// as it always was.
    pub ai_dispersion: Option<f32>,
}

/// [`WeaponState::spare`] of a slot whose `WeaponAmmo` is infinite.
pub const SPARE_INFINITE: u32 = u32::MAX;

#[derive(Clone, Debug)]
pub struct WeaponState {
    pub state: FireState,
    /// Refire accumulator (+0xd0 / BF2 +0xbc) and period (+0xd4 / +0xc0).
    pub refire_accum: f32,
    pub refire_time: f32,
    pub salvo_counter: u32,
    pub salvo_timer: f32,
    pub charge: f32,
    /// Signed spread state, radians (+0x150 / +0x130).
    pub spread: f32,
    pub spread_mult: f32,
    pub posture_base: f32,
    /// Rounds in the clip (BF2 int model; BF1's clip fraction is equivalent).
    pub clip: u32,
    /// Spare rounds outside the clip.
    pub spare: u32,
    /// > 0 while reloading (provisional state machine, report §10.1).
    pub reload_timer: f32,
    /// Heat in [0, 1]; overheated at ≥ 1.0 until it drops below HeatThreshold.
    pub heat: f32,
    pub overheated: bool,
    /// Per-weapon fire command (soldier-written): >0 held counter, −1 on the
    /// release edge, 0 idle.
    pub fire_cmd: i32,
    pub remote_press: u8,
    /// Human players get the −SpreadThreshold free-shot buffer; AI floor 0
    /// (both less the port's tight hold, `spread_floor`).
    pub human: bool,
    /// Last [`FireInput::ai_dispersion`] seen by [`WeaponState::update`] —
    /// the AI sniper table's contribution to [`WeaponState::yaw_spread`]. It
    /// lives on the instance because that is where the PS2 keeps it
    /// (`weapon+0x138` / `+0x158`, overwritten by the AI every aim tick and
    /// read by `GetYawSpread`; `analysis/re-ai-sniper.md` §2). NOT hashed:
    /// it is written from `AiState.dispersion`, which is, and rewritten
    /// before every use.
    pub ai_dispersion: Option<f32>,
    /// Weapon drawn (weapon+0xc4). Holstering cancels pending salvos and
    /// blocks triggering; held channel slots draw and holster their weapons.
    pub active: bool,
    pub shots_fired: u64,
    pub tilt_index: u32,
    /// The charge stage this trigger fires ([`WeaponClass::stage_for`]).
    pub stage: u8,
    /// The rounds this weapon was issued (the clip and the reserve at its
    /// making; [`SPARE_INFINITE`] = endless). Never written again: the PS2's
    /// cap of the reserve (B1 weapon+0x94, B2 ammo[2]), which the AI's
    /// "plenty of ammunition" asks (`re-grenades.md` 7.5). NOT hashed: a
    /// constant of the unit's class and slot.
    pub issued: u32,
}

impl WeaponState {
    /// One loaded clip plus `reserve_clips` spare clips. Zero means no reserve;
    /// use [`Self::with_clips`] for authored totals and an unlimited reserve.
    pub fn new(class: &WeaponClass, human: bool, reserve_clips: u32) -> Self {
        Self {
            state: FireState::Idle,
            refire_accum: 0.0,
            refire_time: class.shot_delay[0],
            salvo_counter: 0,
            salvo_timer: 0.0,
            charge: 0.0,
            stage: 0,
            spread: Self::spread_floor(class, human, 1.0),
            spread_mult: 1.0,
            posture_base: 0.0,
            clip: class.rounds_per_clip,
            spare: reserve_clips.saturating_mul(class.rounds_per_clip),
            reload_timer: 0.0,
            heat: 0.0,
            overheated: false,
            fire_cmd: 0,
            remote_press: 0,
            human,
            ai_dispersion: None,
            active: true,
            shots_fired: 0,
            tilt_index: 0,
            issued: class
                .rounds_per_clip
                .saturating_add(reserve_clips.saturating_mul(class.rounds_per_clip)),
        }
    }

    /// The PS2's ammo rule for a class slot (`re-grenades.md` §2.5, B1
    /// `FUN_002838d8`): `WeaponAmmo` clips = one loaded + (N − 1) in reserve;
    /// 0 (a written value < 1) = an endless reserve ([`SPARE_INFINITE`]).
    pub fn with_clips(class: &WeaponClass, human: bool, clips: u32) -> Self {
        let mut w = Self::new(class, human, clips.saturating_sub(1));
        if clips == 0 {
            w.spare = SPARE_INFINITE;
            w.issued = SPARE_INFINITE;
        }
        w
    }

    /// The rounds this weapon was issued; `None` = endless.
    pub fn issued(&self, c: &WeaponClass) -> Option<u32> {
        (!c.infinite_ammo() && self.issued != SPARE_INFINITE).then_some(self.issued)
    }

    /// Rounds the HUD counts: the clip plus the reserve (§2.5); `None` when
    /// either is endless.
    pub fn rounds(&self, c: &WeaponClass) -> Option<u32> {
        (!c.infinite_ammo() && self.spare != SPARE_INFINITE).then(|| self.clip + self.spare)
    }

    /// Nothing left to fire or load (the PS2's weapon state 8).
    pub fn out_of_ammo(&self, c: &WeaponClass) -> bool {
        !c.infinite_ammo() && self.clip < c.ammo_per_shot && self.spare == 0
    }

    /// Soldier-side fire command byte: held counter / release edge / idle.
    fn set_fire_input(&mut self, held: bool) {
        self.fire_cmd = if held {
            self.fire_cmd.max(0).saturating_add(1)
        } else if self.fire_cmd > 0 {
            -1
        } else {
            0
        };
    }

    /// The stage the trigger reached (0 the root; [`WeaponClass::stages`]).
    pub fn stage(&self) -> usize {
        usize::from(self.stage)
    }

    /// FUN_00289150.
    fn fire_predicate(&self, c: &WeaponClass) -> bool {
        if self.overheated {
            return false;
        }
        if c.max_pressed_time > 0.0 {
            self.fire_cmd == -1
        } else if !c.trigger_single {
            self.fire_cmd > 0
        } else {
            self.fire_cmd == 1
        }
    }

    pub fn reloading(&self) -> bool {
        self.reload_timer > 0.0
    }

    /// Effective half-angles at fire time (vtbl+0x44 / +0x40).
    pub fn pitch_spread(&self, c: &WeaponClass) -> f32 {
        self.spread.max(0.0) + self.posture_base + c.pitch_spread
    }
    /// `max(spreadState, 0) + postureBase + <the YawSpread slot>`
    /// (B2:323416 / B1:211316). The AI sniper table occupies that last slot
    /// when it applies — it REPLACES the class `YawSpread`, it does not
    /// replace `max(s, 0)`, which every branch of the PS2 getter adds
    /// (`analysis/re-ai-sniper.md` §2; the same document's §4.2 corrects
    /// `re-blaster-damage.md` §4.3, which said otherwise).
    pub fn yaw_spread(&self, c: &WeaponClass) -> f32 {
        self.spread.max(0.0) + self.posture_base + self.ai_dispersion.unwrap_or(c.yaw_spread)
    }

    /// SetPostureSpread (BF1 FUN_002889b0 / BF2 0x32e438), called by the
    /// soldier every tick: `stance` 0 stand / 1 crouch / 2 prone (>2 → 0),
    /// `move_mag` = max(|fwd|, |strafe|).
    pub fn set_posture(&mut self, c: &WeaponClass, stance: usize, move_mag: f32, zoomed: bool) {
        let p = if stance > 2 { 0 } else { stance };
        let base = if zoomed {
            c.posture_spread[p][2]
        } else {
            c.posture_spread[p][0]
        };
        self.posture_base = base + (c.posture_spread[p][1] - base) * move_mag;
    }

    /// Start a reload if one is possible (RELOAD button or empty clip).
    pub fn start_reload(&mut self, c: &WeaponClass) -> bool {
        if c.infinite_ammo()
            || self.reloading()
            || self.clip >= c.rounds_per_clip
            || self.spare == 0
        {
            return false;
        }
        if self.spare != SPARE_INFINITE {
            self.spare = self.spare.saturating_add(self.clip);
            self.clip = 0;
        }
        self.reload_timer = c.reload_time;
        true
    }

    /// Trigger init (BF1 FUN_00288c58 / BF2 FUN_0032e758).
    fn trigger(&mut self, c: &WeaponClass, lcg: &mut GameLcg) {
        self.charge = if c.max_pressed_time > 0.0 {
            self.refire_accum / c.max_pressed_time
        } else {
            1.0
        };
        self.stage = c.stage_for(self.charge).min(usize::from(u8::MAX)) as u8;
        self.refire_accum = 0.0;
        self.refire_time = lcg.uniform(c.shot_delay[0], c.shot_delay[1]);
        self.salvo_counter = c.salvo_count;
        self.salvo_timer = c.initial_salvo_delay;
    }

    /// FUN_002891c0: the firing state may return to idle.
    fn cycle_finished(&self, c: &WeaponClass, blocked: bool) -> bool {
        self.salvo_counter < 1
            && self.refire_time <= self.refire_accum
            && (c.trigger_single
                || !self.fire_predicate(c)
                || self.reloading()
                || (!c.infinite_ammo() && self.clip < c.ammo_per_shot)
                || blocked)
    }

    /// One tick of the weapon (state machine + fire cycle + heat + reload +
    /// spread recovery). Spawned shots are appended to `shots`.
    pub fn update(
        &mut self,
        dt: f32,
        c: &WeaponClass,
        input: FireInput,
        lcg: &mut GameLcg,
        shots: &mut Vec<ShotSpec>,
    ) {
        let FireInput {
            held: fire_held,
            blocked,
            infinite_ammo,
            ai_dispersion,
        } = input;
        // The PS2's AI writes `weapon+0x138` every aim tick, before anything
        // in the fire cycle can ask for a spread; ours arrives with the pad.
        self.ai_dispersion = ai_dispersion;
        self.set_fire_input(fire_held);

        // Heat (BF1 weapon update): overheat at 1.0, recover, clear below threshold.
        if !self.overheated {
            self.overheated = self.heat >= 1.0;
        }
        self.heat = (self.heat - dt * c.heat_recover_rate).max(0.0);
        if self.overheated {
            self.overheated = c.heat_threshold < self.heat;
        }

        // Reload timer (provisional machine): refill from spare on expiry.
        if self.reload_timer > 0.0 {
            self.reload_timer -= dt;
            if self.reload_timer <= 0.0 {
                self.reload_timer = 0.0;
                let take = (c.rounds_per_clip - self.clip.min(c.rounds_per_clip)).min(self.spare);
                self.clip += take;
                if self.spare != SPARE_INFINITE {
                    self.spare -= take;
                }
            }
        }
        if !infinite_ammo
            && !c.infinite_ammo()
            && self.active
            && self.state == FireState::Idle
            && self.clip < c.ammo_per_shot
            && !self.reloading()
        {
            self.start_reload(c);
        }

        // Charge enter (B1 cannon `FUN_00289050` 211479-211486, B2 twin;
        // `analysis/re-weapon-charge.md` §2): a fresh press of a charge weapon
        // starts the accumulator at 0 with MaxPressedTime its limit (in place
        // of the refire period); the idle tick below fills it while the
        // trigger is held, and the trigger reads charge = accum /
        // MaxPressedTime. The port used the refire period as the limit, so a
        // held charge stopped at ShotDelay / MaxPressedTime (issue #29).
        if c.max_pressed_time > 0.0 && self.state == FireState::Idle && self.fire_cmd == 1 {
            self.refire_accum = 0.0;
            self.refire_time = c.max_pressed_time;
        }

        // Idle → trigger (weapon update state 0: active && fire predicate).
        if self.state == FireState::Idle
            && self.active
            && !blocked
            && !self.reloading()
            && self.fire_predicate(c)
            // A grenade weapon with nothing in the hand does not wind up.
            && (c.thrown.is_none()
                || infinite_ammo
                || c.infinite_ammo()
                || self.clip >= c.ammo_per_shot)
        {
            self.trigger(c, lcg);
            self.state = FireState::Firing;
        }

        if self.state == FireState::Firing {
            // The stage the trigger reached fires: its ShotsPerSalvo, offsets
            // and heat; the ammunition is the weapon's (§3).
            let sc = c.stage(self.stage());
            let first = shots.len();
            self.fire_cycle(dt, sc, blocked, infinite_ammo, lcg, shots);
            for s in &mut shots[first..] {
                s.stage = self.stage;
            }
            if self.cycle_finished(sc, blocked) {
                self.state = FireState::Idle;
                self.stage = 0;
                if !infinite_ammo
                    && !c.infinite_ammo()
                    && self.active
                    && self.clip < c.ammo_per_shot
                {
                    self.start_reload(c);
                }
            }
        } else {
            // Idle: the accumulator still saturates toward the period so
            // cycle_finished/trigger semantics stay consistent.
            self.refire_accum = (self.refire_accum + dt).min(self.refire_time);
        }

        // Per-tick spread recovery (FUN_00288930 / FUN_0032e200).
        let floor = Self::spread_floor(c, self.human, self.spread_mult);
        self.spread = (self.spread - dt * c.spread_recover_rate).max(floor);
    }

    /// Where the spread state rests: the PS2's floor (a human's
    /// -SpreadThreshold, an AI's 0; ctor `FUN_00288858`) less the port's
    /// [`WeaponClass::tight_hold`].
    fn spread_floor(c: &WeaponClass, human: bool, mult: f32) -> f32 {
        let ps2 = if human { -c.spread_threshold } else { 0.0 };
        ps2 - c.tight_hold(human, mult)
    }

    /// FUN_00288d10 / FUN_0032e8c0 transcription.
    fn fire_cycle(
        &mut self,
        dt: f32,
        c: &WeaponClass,
        blocked: bool,
        infinite_ammo: bool,
        lcg: &mut GameLcg,
        shots: &mut Vec<ShotSpec>,
    ) {
        if !self.active {
            self.salvo_counter = 0;
        }
        let mut dt_left = dt;
        if dt_left <= FIRE_EPS {
            return;
        }
        loop {
            let take;
            if self.salvo_counter < 1 {
                // Refire branch: accumulate up to the period, then gate.
                let deficit = self.refire_time - self.refire_accum;
                take = if deficit < dt_left { deficit } else { dt_left };
                self.refire_accum += take;
                if self.refire_accum < self.refire_time
                    || c.trigger_single
                    || !self.fire_predicate(c)
                    || self.reloading()
                    || (!infinite_ammo && !c.infinite_ammo() && self.clip < c.ammo_per_shot)
                    || blocked
                {
                    break;
                }
                self.refire_accum -= self.refire_time;
                self.salvo_counter = c.salvo_count;
                self.salvo_timer = c.initial_salvo_delay;
            } else {
                if self.salvo_timer <= 0.0 {
                    loop {
                        let ammo_ok = |s: &Self| {
                            infinite_ammo || c.infinite_ammo() || s.clip >= c.ammo_per_shot
                        };
                        for _ in 0..c.shots_per_salvo {
                            if !ammo_ok(self) {
                                self.salvo_counter = 0;
                                break;
                            }
                            shots.push(ShotSpec {
                                tilt: c
                                    .shot_tilts
                                    .get(self.tilt_index as usize)
                                    .copied()
                                    .unwrap_or_default(),
                                charge: self.charge,
                                stage: 0,
                                pitch_spread: self.pitch_spread(c),
                                yaw_spread: self.yaw_spread(c),
                            });
                            if !infinite_ammo && !c.infinite_ammo() {
                                self.clip -= c.ammo_per_shot;
                            }
                            self.heat += c.heat_per_shot;
                            self.shots_fired += 1;
                            self.tilt_index =
                                (self.tilt_index + 1) % c.shot_tilts.len().max(1) as u32;
                        }
                        // Spread grows once per salvo, capped at SpreadLimit.
                        self.spread = (self.spread + self.spread_mult * c.spread_per_shot)
                            .min(c.spread_limit);
                        self.salvo_counter = self.salvo_counter.saturating_sub(1);
                        self.salvo_timer += lcg.uniform(c.salvo_delay[0], c.salvo_delay[1]);
                        if self.salvo_counter < 1 || 0.0 < self.salvo_timer {
                            break;
                        }
                    }
                }
                take = if self.salvo_timer < dt_left {
                    self.salvo_timer
                } else {
                    dt_left
                };
                self.salvo_timer -= take;
            }
            dt_left -= take;
            if dt_left <= FIRE_EPS {
                break;
            }
        }
    }

    /// Hash-feed for the sim state hash.
    pub fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        for v in [
            self.refire_accum,
            self.refire_time,
            self.salvo_timer,
            self.charge,
            self.spread,
            self.posture_base,
            self.reload_timer,
            self.heat,
        ] {
            eat(&v.to_bits().to_le_bytes());
        }
        // `stage` follows from `charge` (hashed above) and the class.
        eat(&self.salvo_counter.to_le_bytes());
        eat(&self.clip.to_le_bytes());
        eat(&self.spare.to_le_bytes());
        eat(&self.fire_cmd.to_le_bytes());
        if self.remote_press > 0 {
            eat(b"REMOTE");
            eat(&[self.remote_press]);
        }
        eat(&[self.state as u8, self.overheated as u8, self.human as u8]);
        if self.tilt_index != 0 {
            eat(b"tilt");
            eat(&self.tilt_index.to_le_bytes());
        }
    }
}

/// FUN_002892a8 steps 3–4: triangular pitch then yaw offsets (four LCG steps
/// each, pitch first), then the local direction (sinY·cosP, −sinP, cosY·cosP)
/// expressed in the muzzle frame (`right`, `up`, `fwd`). `tilt` = salvo tilt
/// pair (pitch, yaw) in radians, zero for rifles.
pub fn shot_direction(
    lcg: &mut GameLcg,
    fwd: Vec3,
    right: Vec3,
    up: Vec3,
    pitch_mag: f32,
    yaw_mag: f32,
    tilt: (f32, f32),
) -> Vec3 {
    let pitch = tilt.0 + lcg.triangular(pitch_mag);
    let yaw = tilt.1 + lcg.triangular(yaw_mag);
    let (sp, cp) = fmath::sin_cos(pitch);
    let (sy, cy) = fmath::sin_cos(yaw);
    (right * (sy * cp) + up * (-sp) + fwd * (cy * cp)).normalize()
}

#[cfg(test)]
mod tests;
