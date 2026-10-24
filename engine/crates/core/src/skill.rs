//! Difficulty tiers and the per-unit SKILL VECTOR (PLAN Log "PHASE C DESIGN —
//! perception, the skill vector and the four tiers", decisions 1 and 4).
//!
//! The originals keep an integer skill 1..=20 per unit and read every accuracy
//! parameter off curves in `t = (s − 1)/19` (`analysis/re-ai-difficulty.md`
//! §4.1, §7). We keep that scale, so "old Elite" is a NUMBER — the PS2's
//! non-human-team Elite base is enemy skill 17 (RE §2.1) — and our four tiers
//! set every bot's base skill on top of it.
//!
//! # The tier table — PROVISIONAL, the lead's, DO NOT TUNE
//!
//! | tier | base skill | t | view mult | reaction s (ticks) | pressure ×human | human damage × | err cap | bleed handicap |
//! |---|---|---|---|---|---|---|---|---|
//! | Easy | 8 | 0.368 | 0.8 | 0.5 (15) | 1.0 | 1.0 | 1.83 m | — |
//! | Normal | 17 | 0.842 | 1.0 | 0.3 (9) | 1.0 | 1.0 | 1.36 m | — |
//! | Hard | 20 | 1.0 | 1.25 | 0.2 (6) | 1.3 | 1.15 | 1.2 m | — |
//! | Elite | 20 | 1.0 | 1.5 | 0.15 (5) | 1.6 | 1.3 | **0.5 m** | ×1.25 |
//!
//! Card HOLD adds how quickly a bot runs from a grenade, as a chance per
//! landing (ours, PROVISIONAL — the maintainer's to tune; its shuffle and strafe went
//! with card TPOS):
//!
//! | tier | grenade (per landing) | grenade delay s |
//! |---|---|---|
//! | Easy | 30 % | 1.1–2.1 |
//! | Normal | 60 % | 0.6–1.3 |
//! | Hard | 85 % | 0–0.8 |
//! | Elite | 95 % | 0–0.8 |
//!
//! Card REACT adds what a bot on a position (a hub's, a fight's) does when
//! it is HIT (on a Tactician's position, card TPOS, the "moves" share
//! leaves it at once and two hits leave it) — one draw per hit, 1.5 s apart at least (ours, PROVISIONAL —
//! The maintainer's to tune; the PS2's attacker ignores the hit):
//!
//! | tier | stays | strafes 2.5 m | rolls aside | moves to a new position |
//! |---|---|---|---|---|
//! | Easy | 65 % | 20 % | 5 % | 10 % |
//! | Normal | 35 % | 30 % | 15 % | 20 % |
//! | Hard | 20 % | 30 % | 20 % | 30 % |
//! | Elite | 10 % | 30 % | 25 % | 35 % |
//!
//! Only the SKILL SCALE and the curves below are the RE's. Every column of
//! that table is the lead's design placeholder: the base skills, the view
//! multipliers, the reaction floor (the PS2 has **no** reaction timer at all —
//! RE §4.3: its "reaction feel" is the 5-frame think cadence plus the error
//! floor), the pressure weight, the human damage multiplier (see
//! [`Tier::human_damage`]) and the bleed handicap. They are measured by the
//! arena and moved by the LEAD, never by a build agent chasing an acceptance.
//!
//! The PS2's own per-level bases, for scale (RE §2.1): BF2 non-human teams
//! 4 / 14 / 17 at Easy / Medium / Hard, human-team teammates 6 / 11 / 11.

/// The four host difficulty tiers (PLAN §Saves: host authority), ordered
/// easiest first (the arena's head-to-head takes the lower of two).
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
    Elite,
}

impl Difficulty {
    /// Every tier, in table order — the arena's row order.
    pub const ALL: [Difficulty; 4] = [
        Difficulty::Easy,
        Difficulty::Normal,
        Difficulty::Hard,
        Difficulty::Elite,
    ];

    /// One byte for [`crate::sim::Sim::hash`].
    pub fn bytes(self) -> [u8; 1] {
        [match self {
            Difficulty::Easy => 0,
            Difficulty::Normal => 1,
            Difficulty::Hard => 2,
            Difficulty::Elite => 3,
        }]
    }

    pub fn label(self) -> &'static str {
        match self {
            Difficulty::Easy => "easy",
            Difficulty::Normal => "normal",
            Difficulty::Hard => "hard",
            Difficulty::Elite => "elite",
        }
    }

    /// `easy|normal|hard|elite`, case-insensitive; `None` for anything else.
    pub fn parse(s: &str) -> Option<Difficulty> {
        match s.trim().to_ascii_lowercase().as_str() {
            "easy" => Some(Difficulty::Easy),
            "normal" => Some(Difficulty::Normal),
            "hard" => Some(Difficulty::Hard),
            "elite" => Some(Difficulty::Elite),
            _ => None,
        }
    }

    /// The integer skill EVERY bot of every team takes at spawn (symmetric
    /// competence — PLAN "AI decision": the tiers do not hand one team a
    /// better bot than the other; the human-facing levers are the pressure
    /// weight, the damage multiplier and the team handicap).
    pub fn base_skill(self) -> u8 {
        match self {
            Difficulty::Easy => 8,
            Difficulty::Normal => 17,
            Difficulty::Hard | Difficulty::Elite => 20,
        }
    }

    /// The per-tier columns of the table above.
    pub fn tier(self) -> Tier {
        match self {
            Difficulty::Easy => Tier {
                view_mult: 0.8,
                reaction_ticks: 15,
                pressure: 1.0,
                human_damage: 1.0,
                err_cap: None,
                bleed_handicap: 1.0,
                react: [0.20, 0.05, 0.10],
                grenade: 0.30,
                grenade_delay: (33, 63),
                tell: false,
                cover: false,
                flank: false,
            },
            Difficulty::Normal => Tier {
                view_mult: 1.0,
                reaction_ticks: 9,
                pressure: 1.0,
                human_damage: 1.0,
                err_cap: None,
                bleed_handicap: 1.0,
                react: [0.30, 0.15, 0.20],
                grenade: 0.60,
                grenade_delay: (18, 39),
                tell: true,
                cover: false,
                flank: true,
            },
            Difficulty::Hard => Tier {
                view_mult: 1.25,
                reaction_ticks: 6,
                pressure: 1.3,
                human_damage: 1.15,
                err_cap: None,
                bleed_handicap: 1.0,
                react: [0.30, 0.20, 0.30],
                grenade: 0.85,
                grenade_delay: (0, 24),
                tell: true,
                cover: true,
                flank: true,
            },
            Difficulty::Elite => Tier {
                view_mult: 1.5,
                reaction_ticks: 5,
                pressure: 1.6,
                human_damage: 1.3,
                // The Elite-only extra of design decision 1: a tighter
                // tracked-position error cap than any skill level yields.
                err_cap: Some(0.5),
                bleed_handicap: 1.25,
                react: [0.30, 0.25, 0.35],
                grenade: 0.95,
                grenade_delay: (0, 24),
                tell: true,
                cover: true,
                flank: true,
            },
        }
    }

    /// The skill vector a bot of this tier carries: [`Skill::from_level`] at
    /// [`Difficulty::base_skill`], with the tier's own `err_cap` override
    /// applied (Elite).
    pub fn skill(self) -> Skill {
        self.skill_at(self.base_skill())
    }

    /// The vector a bot of integer skill `s` carries UNDER THIS TIER: the
    /// level curve ([`Skill::from_level`]) with the tier's own overrides
    /// applied — today only Elite's tighter error cap. One place, so the
    /// think and `Difficulty::skill` cannot drift.
    pub fn skill_at(self, s: u8) -> Skill {
        let mut k = Skill::from_level(s);
        if let Some(cap) = self.tier().err_cap {
            k.err_cap = cap;
        }
        k
    }
}

/// The per-tier columns that are NOT a function of the integer skill. All
/// PROVISIONAL (lead placeholders, no RE) — see the module header.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tier {
    /// Multiplies the PS2's view-distance shape `(20·bound_radius + 30)`
    /// (RE §5.1). PROVISIONAL.
    pub view_mult: f32,
    /// Ticks between a contact's FIRST SIGHTING and the tick it may be fired
    /// at. OURS: the PS2 has no reaction timer (RE §4.3). PROVISIONAL.
    pub reaction_ticks: u64,
    /// Target-choice weight on a HUMAN target (per human, not per team —
    /// PLAN "AI decision"). PROVISIONAL.
    pub pressure: f32,
    /// Damage multiplier on a bot's bolt into a HUMAN. **Entirely ours**: the
    /// PS2's only level-3 damage lever is `HitLocation`, the ×4 vehicle
    /// critical-hit prims an Elite AI attacker may score (RE §9.2), which is
    /// not a buff against humans and waits for P7 vehicles. PROVISIONAL.
    pub human_damage: f32,
    /// Elite's tighter tracked-position error cap; `None` = the skill's own.
    pub err_cap: Option<f32>,
    /// Ticket-bleed multiplier against a team that HAS a human when the
    /// opposing team has none (the team handicap, a host option). 1.0 = none.
    /// PROVISIONAL.
    pub bleed_handicap: f32,
    /// (Card TPOS: card HOLD's `shuffle` and `strafe` chances went with the
    /// shuffle and the strafe on a burst: a fighter's cycle and its next
    /// position are the Tactician's, `crate::ai::tactic`.)
    /// Card REACT (the maintainer, 2026-10-07): what a bot on a position does about a
    /// HIT, one draw per hit outside the cooldown — the chances to STRAFE,
    /// to ROLL aside and to MOVE to a new position; the rest it stays
    /// (`crate::ai` `react`). OURS and PROVISIONAL; a named deviation, the
    /// PS2's attacker ignores the hit.
    pub react: [f32; 3],
    /// ...when a grenade lands within its warning radius, to run clear of it.
    pub grenade: f32,
    /// The grenade reaction's delay in ticks, drawn uniformly in this range.
    /// The SHAPE is the PS2's (re-grenades.md §7.4 — BF1: U(1.1, 2.1) s under
    /// difficulty 14, U(0.6, 1.3) s under 17, else U(0, 0.8) s); which tier
    /// takes which range is ours.
    pub grenade_delay: (u64, u64),
    /// Card TEAM (`analysis/bot-design.md` 4.1, section 7): an enemy a
    /// friend within `perception::TELL_RADIUS` sees is passed on as heard -
    /// aimed at, never fired at unseen. OURS: the PS2 has no team sense.
    /// The design's column: Easy no, the rest yes. A told record keeps no
    /// bot on a position (`ai::behave`): before that rule a told enemy held
    /// a bot standing at its fight spot while its friends saw him - BF2
    /// Coruscant's foot time in 20 s spells within 2 m 11.7 -> 21.1 % (8
    /// seeds), with it 9.4 %.
    pub tell: bool,
    /// Card TEAM (`analysis/bot-design.md` 4.3, section 7): while a friend
    /// captures a post, up to two others inside its region take guard
    /// positions facing the approach (the board's Cover role,
    /// `ai::team::board`). OURS. The design's column: Hard and Elite.
    pub cover: bool,
    /// Card FLNK (`analysis/bot-design.md` 6.2, section 7): a share of a
    /// post's attackers walks the pair's flank route (the board's Flank
    /// role, `ai::team::board`). OURS. The design's column: all but Easy.
    pub flank: bool,
}

/// Everything the AI reads off its integer skill, computed once per think.
///
/// Every curve here is the RE's (`analysis/re-ai-difficulty.md` §4.1 —
/// `FUN_002a2c80..0x2a32e8`, the BF2 skill-parameter getters; §4.4 for the
/// burst/pause/charge shapes), evaluated at `t = (s − 1)/19` over the
/// originals' 1..=20 scale. `lerp(a, b)` below means `a + (b − a)·t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skill {
    /// The integer level this vector came from (clamped 1..=20).
    pub level: u8,
    /// `(level − 1)/19`.
    pub t: f32,
    /// `weapon+0x134` (BF1 `+0x154`): the weapon's own spread growth per shot
    /// is multiplied by this. `lerp(3, 1)`, RE §4.1 / §4.2 step 2.
    pub spread_mult: f32,
    /// `weapon+0x138` (BF1 `+0x158`): scales the AI distance-dispersion table
    /// in `GetYawSpread`. `lerp(2, 0)`, RE §4.1.
    pub dispersion_mult: f32,
    /// Aim-direction acceleration clamp, CUBIC in `t` (RE §4.1
    /// `FUN_002a2eb0` — `lerp(0.8, 14.0)` over `t³`).
    pub aim_accel: f32,
    /// Aim-direction damping, `lerp(0.8, 2.4)` (RE §4.1 `FUN_002a2ee0`).
    pub aim_damping: f32,
    /// Tracked-position error decay, m/s (RE §4.1 `FUN_002a2dc0`,
    /// `lerp(0.1, 1.1)`; the RE's sign is negative because the PS2 ADDS it).
    pub err_decay: f32,
    /// Tracked-position error FLOOR, m: `max(0, lerp(1.6, 0.4))`, a FLOAT
    /// and continuous in skill — 1.6 m at skill 1, 0.968 at 11, 0.4 at 20
    /// (`analysis/re-ai-sniper.md` §1, lead-verified 2026-09-08:
    /// `FUN_002a2df8` has NO integer conversion, Ghidra's
    /// `(int)f * (uint)(0.0 < f)` is the EE `max.s` at 0x2a2e28, and
    /// `FUN_002ae7b8` consumes it as a float).
    pub err_floor: f32,
    /// Tracked-position error CAP, m: `max(0, lerp(2.2, 1.2))`, likewise a
    /// float — 2.2 m at skill 1, 1.36 at 17, 1.2 at 20 (same source,
    /// `FUN_002a2e38`/`FUN_002a2e78`). A tier may override it
    /// ([`Tier::err_cap`]). The flyer cap `max(0, lerp(4.0, 1.2))` has no
    /// input in the port (no flyer targets).
    pub err_cap: f32,
    /// Burst-length fraction: `n = max(2, min + U(0, max − min)·t)`
    /// (RE §4.1 `FUN_002a2c90`, §4.4 `FUN_002bac98`).
    pub burst_t: f32,
    /// Inter-burst pause multiplier `1.5 − 0.8·t` — the RE's
    /// `(1.6 − lerp(0.1, 0.9)) × weaponclass+0x21c` (§4.1 `FUN_002a2cb8`).
    pub pause_mult: f32,
    /// A single-shot weapon's re-arm scale: the trigger comes up for
    /// `U(0.2, 2.0) × tap_x` seconds after each press, `tap_x = max(0, 1 −
    /// lerp(0.1, 0.9))` (BF2 `FUN_002baf70` over `FUN_002a2cf0`,
    /// `analysis/re-round-stalls.md` §2 C4; a real `max.s` at 0x2bb018).
    pub tap_x: f32,
    /// Charge weapons release at charge `U(0.15, charge_hi) × MaxPressedTime`
    /// (RE §4.1 `FUN_002a2d28`/`FUN_002a2d40`, §4.4).
    pub charge_hi: f32,
    /// Chance to use the grenade class against the current target (RE §4.1
    /// `FUN_002a2d68`): BF2's roll inside the grenade gate — the bot's number
    /// plus floor(10 t), mod 100, at most floor(100 x this) (card GRAI,
    /// `ai::arsenal::grenade_forced`). BF1's gate has no skill term.
    pub grenade_chance: f32,
    /// Chance to dodge on being hit, `lerp(0, 0.2)`, cooldown 2 s (RE §4.1
    /// `FUN_002a3018`; the dodge is a movement re-plan, NOT a roll — §9.4).
    pub dodge_chance: f32,
}

impl Skill {
    /// The vector at integer skill `s` (clamped to the originals' 1..=20).
    pub fn from_level(s: u8) -> Skill {
        let level = s.clamp(1, 20);
        let t = f32::from(level - 1) / 19.0;
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        Skill {
            level,
            t,
            spread_mult: lerp(3.0, 1.0),
            dispersion_mult: lerp(2.0, 0.0),
            // RE §4.1: the acceleration getter is the one CUBIC curve.
            aim_accel: 0.8 + 13.2 * t * t * t,
            aim_damping: lerp(0.8, 2.4),
            err_decay: lerp(0.1, 1.1),
            // FLOATS, clamped at zero, never truncated: HOLD 19(g) was
            // ruled by the lead on 2026-09-08 (`analysis/re-ai-sniper.md` §1,
            // brief_phase_c LEAD ADDENDUM 4a) — the `(int)` in the decompiler
            // output is the EE's `max.s` against zero, and the consumer takes
            // a float. Commit 1 of this phase shipped `as i32 as f32` and the
            // "1 m below skill 12, then 0" rows with it; both are wrong.
            err_floor: lerp(1.6, 0.4).max(0.0),
            err_cap: lerp(2.2, 1.2).max(0.0),
            burst_t: t,
            pause_mult: 1.5 - 0.8 * t,
            tap_x: (1.0 - lerp(0.1, 0.9)).max(0.0),
            charge_hi: t,
            grenade_chance: t,
            dodge_chance: lerp(0.0, 0.2),
        }
    }

    /// The aim tracker's `(A, B)` = (acceleration clamp, damping), HALVED for
    /// a weapon that is not a `WeaponCannon` (RE §4.2 step 4,
    /// `DAT_00699ec8`).
    ///
    /// The foot gunner currently passes `true` for every weapon class: its
    /// tracker does not yet classify non-cannon weapons. The non-cannon
    /// scaling remains available and tested.
    pub fn tracker_for(self, is_cannon: bool) -> (f32, f32) {
        if is_cannon {
            (self.aim_accel, self.aim_damping)
        } else {
            (self.aim_accel * 0.5, self.aim_damping * 0.5)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn the_four_tiers_yield_the_designs_vector() {
        // The design's expected values (PLAN "PHASE C DESIGN", D1's table),
        // recomputed from the RE's own formulas. The ONE cell that moved is
        // Easy's aim accel: the design prints 1.459, the cubic gives
        // 0.8 + 13.2·(7/19)³ = 1.4601 — reported, not tuned.
        let rows = [
            //  tier                 spread   disp    accel   damp    decay  floor cap   pause  dodge
            (
                Difficulty::Easy,
                2.263,
                1.263,
                1.4601,
                1.389,
                0.468,
                1.157895,
                1.831579,
                1.205,
                0.074,
            ),
            (
                Difficulty::Normal,
                1.316,
                0.316,
                8.683,
                2.147,
                0.942,
                0.589474,
                1.357895,
                0.826,
                0.168,
            ),
            (
                Difficulty::Hard,
                1.0,
                0.0,
                14.0,
                2.4,
                1.1,
                0.4,
                1.2,
                0.7,
                0.2,
            ),
            (
                Difficulty::Elite,
                1.0,
                0.0,
                14.0,
                2.4,
                1.1,
                0.4,
                // The tier's own Elite extra overrides the skill's 1.2 m cap
                // (decision 1, PROVISIONAL and the lead's — the RE's cap at
                // skill 20 is 1.2 m and there is no 0.5 anywhere in the PS2
                // path, ADDENDUM 4a).
                0.5,
                0.7,
                0.2,
            ),
        ];
        for (d, spread, disp, accel, damp, decay, floor, cap, pause, dodge) in rows {
            let s = d.skill();
            assert!(
                close(s.spread_mult, spread),
                "{d:?} spread {}",
                s.spread_mult
            );
            assert!(
                close(s.dispersion_mult, disp),
                "{d:?} disp {}",
                s.dispersion_mult
            );
            assert!(close(s.aim_accel, accel), "{d:?} accel {}", s.aim_accel);
            assert!(
                close(s.aim_damping, damp),
                "{d:?} damping {}",
                s.aim_damping
            );
            assert!(close(s.err_decay, decay), "{d:?} decay {}", s.err_decay);
            assert!(close(s.err_floor, floor), "{d:?} floor {}", s.err_floor);
            assert!(close(s.err_cap, cap), "{d:?} cap {}", s.err_cap);
            assert!(close(s.pause_mult, pause), "{d:?} pause {}", s.pause_mult);
            assert!(
                close(s.dodge_chance, dodge),
                "{d:?} dodge {}",
                s.dodge_chance
            );
        }
    }

    #[test]
    fn the_curves_match_the_res_per_level_table() {
        // analysis/re-ai-difficulty.md §4.1, the [D] per-level table, with
        // the error floor and cap as `analysis/re-ai-sniper.md` §1 rules them
        // (LEAD ADDENDUM 4a): floats, `max(0, lerp(...))`, so skill 11 is
        // checked on EVERY column now — the truncation that made the RE
        // table's 1 m at skill 11 unreachable is gone.
        let rows = [
            //  s   spread  disp   accel  damp   decay  cap
            (4u8, 2.68, 1.68, 0.85, 1.05, 0.26, 2.042105),
            (11, 1.95, 0.95, 2.72, 1.64, 0.63, 1.673684),
            (14, 1.63, 0.63, 5.03, 1.89, 0.78, 1.515789),
            (17, 1.32, 0.32, 8.68, 2.15, 0.94, 1.357895),
            (20, 1.00, 0.00, 14.0, 2.40, 1.10, 1.2),
        ];
        for (s, spread, disp, accel, damp, decay, cap) in rows {
            let k = Skill::from_level(s);
            // The RE's table is printed to 2-3 significant figures.
            assert!(
                (k.spread_mult - spread).abs() < 5e-3,
                "s{s} spread {}",
                k.spread_mult
            );
            assert!(
                (k.dispersion_mult - disp).abs() < 5e-3,
                "s{s} disp {}",
                k.dispersion_mult
            );
            assert!(
                (k.aim_accel - accel).abs() < 5e-3,
                "s{s} accel {}",
                k.aim_accel
            );
            assert!(
                (k.aim_damping - damp).abs() < 5e-3,
                "s{s} damping {}",
                k.aim_damping
            );
            assert!(
                (k.err_decay - decay).abs() < 5e-3,
                "s{s} decay {}",
                k.err_decay
            );
            assert!((k.err_cap - cap).abs() < 1e-6, "s{s} cap {}", k.err_cap);
        }
        // ADDENDUM 4a's pins, to 1e-6 (IEEE round-to-nearest against the
        // EE's round-toward-zero differ by <= 2.4e-7): the tier levels the
        // PS2 itself uses, from `re-ai-sniper.md` §1's full 0..20 table.
        let pins = [
            (4u8, 1.410_526_3, 2.042_105_2),
            (6, 1.284_210_6, 1.936_842_2),
            (11, 0.968_421, 1.673_684_2),
            (14, 0.778_947_4, 1.515_789_5),
            (17, 0.589_473_6, 1.357_894_8),
            (20, 0.4, 1.2),
        ];
        for (s, floor, cap) in pins {
            let k = Skill::from_level(s);
            assert!(
                (k.err_floor - floor).abs() < 1e-6,
                "s{s} floor {}",
                k.err_floor
            );
            assert!((k.err_cap - cap).abs() < 1e-6, "s{s} cap {}", k.err_cap);
        }
        // Both curves are continuous and monotonically falling in skill, and
        // the floor never reaches 0 inside 1..=20 (the `max(0, ...)` clamp is
        // dead code for a soldier, and is kept because the PS2 has it).
        for s in 2..=20u8 {
            let (a, b) = (Skill::from_level(s - 1), Skill::from_level(s));
            assert!(b.err_floor < a.err_floor && b.err_cap < a.err_cap);
            assert!(b.err_floor > 0.0, "floor at skill {s}");
        }
        assert_eq!(Skill::from_level(1).err_floor, 1.6);
        assert_eq!(Skill::from_level(1).err_cap, 2.2);
    }

    #[test]
    fn the_skill_scale_is_clamped_to_the_originals_range() {
        assert_eq!(Skill::from_level(0).level, 1);
        assert_eq!(Skill::from_level(200).level, 20);
        assert_eq!(Skill::from_level(1).t, 0.0);
        assert_eq!(Skill::from_level(20).t, 1.0);
        // Skill 0 (a player, a dummy) is never a bot's: it clamps to the
        // worst bot rather than dividing by zero anywhere downstream.
        assert!(Skill::from_level(0).spread_mult > 2.9);
    }

    #[test]
    fn a_non_cannon_weapon_halves_the_tracker() {
        let s = Difficulty::Elite.skill();
        assert_eq!(s.tracker_for(true), (s.aim_accel, s.aim_damping));
        let (a, b) = s.tracker_for(false);
        assert!(close(a, s.aim_accel * 0.5) && close(b, s.aim_damping * 0.5));
    }

    #[test]
    fn difficulty_parses_labels_case_insensitively_and_round_trips() {
        for d in Difficulty::ALL {
            assert_eq!(Difficulty::parse(d.label()), Some(d));
            assert_eq!(Difficulty::parse(&d.label().to_uppercase()), Some(d));
            assert_eq!(Difficulty::parse(&format!("  {}  ", d.label())), Some(d));
        }
        assert_eq!(Difficulty::parse("medium"), None);
        assert_eq!(Difficulty::parse(""), None);
        assert_eq!(Difficulty::default(), Difficulty::Normal);
        // The hash byte is distinct per tier.
        let mut seen: Vec<u8> = Difficulty::ALL.iter().map(|d| d.bytes()[0]).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 4);
    }

    #[test]
    fn the_tier_columns_are_the_designs_table() {
        let want = [
            (Difficulty::Easy, 0.8, 15u64, 1.0, 1.0, 1.0),
            (Difficulty::Normal, 1.0, 9, 1.0, 1.0, 1.0),
            (Difficulty::Hard, 1.25, 6, 1.3, 1.15, 1.0),
            (Difficulty::Elite, 1.5, 5, 1.6, 1.3, 1.25),
        ];
        for (d, view, react, press, dmg, bleed) in want {
            let t = d.tier();
            assert_eq!((t.view_mult, t.reaction_ticks), (view, react), "{d:?}");
            assert_eq!(
                (t.pressure, t.human_damage, t.bleed_handicap),
                (press, dmg, bleed)
            );
        }
        assert_eq!(Difficulty::Elite.tier().err_cap, Some(0.5));
        assert_eq!(Difficulty::Hard.tier().err_cap, None);
        // Base skills: symmetric competence, every team the same.
        let bases: Vec<u8> = Difficulty::ALL.iter().map(|d| d.base_skill()).collect();
        assert_eq!(bases, vec![8, 17, 20, 20]);
    }
}
