//! What an AI unit carries from one tick to the next, and how it is hashed:
//! [`AiState`] with its [`Route`] leg, the steering's last [`SteerPick`] and
//! the leg's [`RouteSource`] — plus [`NavTick`], what one think had to fall
//! back on (a diagnostic output, not hashed). Ride state includes the initial
//! pickup window, release time and vehicle identity (WAIT4/WPOKE), claim
//! freshness (CWAIT), first routed departure (WGOUT), and next-think call targets (GBOARD). The AI-hash test lives here.

use super::command::CmdState;
use super::{DETOUR_MAX, ERR_NO_TARGET};
use crate::unit::UnitHandle;
use glam::{Vec2, Vec3};

// Only for the field docs below, which link `think` and `stance_hold`.
#[cfg(doc)]
use super::{behave::think, draw::stance_hold};

/// What the steering picked this tick — ROUND_VERBOSE prints it, and it is
/// part of the AI hash because it is carried in [`AiState`] between ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SteerPick {
    /// The unit is not trying to move (holding, arrived, or nowhere to go).
    #[default]
    Idle,
    /// The probe ahead was clear: walk straight at the waypoint.
    Fwd,
    /// Blocked ahead: slide along the wall tangent.
    Slide,
    /// Card OBST: the way ahead is open and a small body stands further on:
    /// walk by it (`avoid::pick`). Not a blocked pick.
    Round,
    /// Blocked ahead and along the wall: the `k`-th fan candidate, signed by
    /// the side it turned to (`+/-1..4` = 30/60/90/120 deg).
    Fan(i8),
    /// Every candidate was blocked: back off along the reverse.
    Back,
    /// Every candidate, the reverse included, was a wall or a drop (card
    /// STFAL): stand, and let the stuck window re-plan.
    Ledge,
}

impl SteerPick {
    /// The two bytes that go into the hash (a tag and the fan's signed step).
    fn bytes(self) -> [u8; 2] {
        match self {
            SteerPick::Idle => [0, 0],
            SteerPick::Fwd => [1, 0],
            SteerPick::Slide => [2, 0],
            SteerPick::Back => [3, 0],
            SteerPick::Fan(k) => [4, k as u8],
            SteerPick::Ledge => [5, 0],
            SteerPick::Round => [6, 0],
        }
    }

    pub fn label(self) -> String {
        match self {
            SteerPick::Idle => "idle".to_owned(),
            SteerPick::Fwd => "fwd".to_owned(),
            SteerPick::Slide => "slide".to_owned(),
            SteerPick::Back => "back".to_owned(),
            SteerPick::Ledge => "ledge".to_owned(),
            SteerPick::Round => "round".to_owned(),
            SteerPick::Fan(k) => format!("fan {}{}", if k < 0 { "-" } else { "+" }, k.abs() * 30),
        }
    }
}

/// Where a route's next hop came from (ROUND_VERBOSE prints it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RouteSource {
    /// The authored route table — the PS2's own answer.
    #[default]
    Table,
    /// The port's breadth-first fallback, for entries the authors left empty.
    Bfs,
    /// Neither could answer: walk straight at the goal.
    Straight,
    /// Card FLNK: the next hub of the bot's flank route (`crate::nav::flank`).
    Flank,
}

impl RouteSource {
    pub fn tag(self) -> &'static str {
        match self {
            RouteSource::Table => "table",
            RouteSource::Bfs => "bfs",
            RouteSource::Straight => "straight",
            RouteSource::Flank => "flank",
        }
    }
}

/// The unit's current leg of its route. Every field is hashed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Route {
    /// The hub the goal (post capture centre, or the hunted enemy) sits on.
    pub dest_hub: u16,
    /// The hub this leg walks to. `next_hub == dest_hub` with `via_arc: None`
    /// is the LAST LEG: the goal itself (the post's capture volume).
    pub next_hub: u16,
    /// The arc this leg walks along — the chooser's `arrival_arc` at the next
    /// hub. `None` on the last leg and on a straight-line fallback.
    pub via_arc: Option<u16>,
    /// This leg's aim point: the unit's own drawn point of the next hub
    /// (`route::hub_point`, card HUBP — the hub's centre for a vehicle, an
    /// unresolved hub and a stuck unit's re-plan, `route::hub_waypoint`), or
    /// the goal itself on the last leg.
    pub waypoint: Vec3,
    /// Where this leg's LINE starts (XZ, card HUBP): the waypoint the unit
    /// reached in the hub it leaves — the PS2's leg runs from one drawn
    /// point to the next — or that hub's centre on a leg planned anywhere
    /// else (a spawn, a new objective, a stuck unit's re-plan).
    /// The arrival plane and the stuck re-plan measure against it, and so
    /// does `route::leg_point` until a detour corner is behind the unit:
    /// from there its line starts at that corner (the PS2's rule, card
    /// MOUTH: `route::seg_from`).
    pub from: Vec2,
    /// XZ distance to `waypoint` when the current progress window opened.
    pub ref_dist: f32,
    /// Ticks into the current progress window.
    pub stuck_ticks: u16,
    /// Ticks spent in windows that made no progress — reset by a window that
    /// does. The sweep's `stuck_max` is the largest this ever reaches.
    pub stuck_run: u16,
    /// Side-step state: `side * (ticks remaining + 1)`, so ±1 means "done,
    /// last side was ±" and 0 means "never side-stepped". The sign alternates
    /// on every trigger.
    pub sidestep: i8,
    pub source: RouteSource,
    /// This leg's CORNER WALK (PLAN "A.1 addendum"): the intermediate XZ
    /// waypoints [`crate::nav::NavGraph::detour`] found round the barriers the
    /// straight leg crosses. Only `detour[..detour_len]` is live, but the
    /// whole array is hashed so no stale slot can drift.
    pub detour: [Vec2; DETOUR_MAX],
    pub detour_len: u8,
    /// How many of them are already behind the unit.
    pub detour_at: u8,
}

impl Route {
    /// The last leg: walk at the goal itself, not at a hub.
    pub fn is_last(&self) -> bool {
        self.via_arc.is_none() && self.next_hub == self.dest_hub
    }

    /// The detour corner the unit is walking to, if the leg still has one.
    /// `None` means the leg's own waypoint is the target.
    pub fn corner(&self) -> Option<Vec2> {
        let (at, len) = (usize::from(self.detour_at), usize::from(self.detour_len));
        (at < len.min(DETOUR_MAX)).then(|| self.detour[at])
    }

    fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(&self.dest_hub.to_le_bytes());
        eat(&self.next_hub.to_le_bytes());
        eat(&self.via_arc.unwrap_or(u16::MAX).to_le_bytes());
        for v in [
            self.waypoint.x,
            self.waypoint.y,
            self.waypoint.z,
            self.ref_dist,
            self.from.x,
            self.from.y,
        ] {
            eat(&v.to_bits().to_le_bytes());
        }
        eat(&self.stuck_ticks.to_le_bytes());
        eat(&self.stuck_run.to_le_bytes());
        eat(&[self.sidestep as u8, self.source as u8]);
        for c in &self.detour {
            eat(&c.x.to_bits().to_le_bytes());
            eat(&c.y.to_bits().to_le_bytes());
        }
        eat(&[self.detour_len, self.detour_at]);
    }
}

/// What one `think` had to fall back on — folded into `Sim::nav_stats`, which
/// is a diagnostic OUTPUT and not hashed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NavTick {
    /// Hops answered by `route_bfs` because the table had no entry.
    pub bfs: u32,
    /// Hops neither the table nor BFS could answer.
    pub straight: u32,
    /// Card FLNK: hops taken from a flank route (on it or joining it).
    pub flank: u32,
    /// This unit's current run of ticks without progress.
    pub stuck_run: u16,
    /// Legs planned this tick whose straight line crossed a barrier and got a
    /// corner path instead.
    pub detoured: u32,
    /// 1 when the unit wanted to move and the way ahead was NOT clear.
    pub blocked: u32,
    /// Legs whose chosen next hub had no floor at the unit's own level and
    /// were rejected (Phase A.2, `hub_has_floor`).
    pub level_rejected: u32,
    /// Of those, the ones where EVERY alternative also failed and the unit
    /// fell back to the straight line.
    pub level_unrouted: u32,
    /// 1 when the forward wall SWEEP found a wall this tick.
    pub sweep_hits: u32,
    /// 1 when the commander handed this unit an objective the graph could not
    /// route to from where it stands (Phase B). The assignment is LEFT alone —
    /// the next plan sees the same unreachability and drops it — and this is
    /// the count that says how often that happens.
    pub unroutable: u32,
    /// This unit's current [`AiState::unreachable_dwell`].
    pub dwell: u16,
    /// Contacts this unit's perception held after this think (Phase C).
    pub contacts: u8,
    /// Records this think made by HEARING rather than sight.
    pub heard: u8,
    /// Card TEAM: records this think made or refreshed from what a friend
    /// near by sees (`perception::tell`).
    pub told: u8,
    /// 1 when this think ENDED a squad order (brief D6/D9): the leader is
    /// gone, or the follower has been beyond [`crate::squad::LEAVE_DIST`]
    /// of its leader — or the holder of its anchor — for
    /// [`crate::squad::LEAVE_TICKS`]. `think` takes `&Sim` and cannot
    /// clear the order itself, so it reports it and `Sim::step`'s write-back
    /// does it, in the same fixed order as every other write-back.
    pub released: u32,
    /// Card WAI2, a boarding claim this tick: [`NavTick::CLAIM_NEW`] made,
    /// [`NavTick::CLAIM_GIVEUP`] dropped on its clock, [`NavTick::CLAIM_LOST`]
    /// dropped because another bot has the vehicle; 0 otherwise.
    pub claim: u8,
    /// 1 when this unit walked its RETURN this tick (card BNDS): off the
    /// playable level and heading back for it.
    pub lost: u32,
    /// 1 when this think gave the unit up (card BNDS): off the level for
    /// [`crate::ai::LOST_RECYCLE`] with no way back. `Sim::step`'s write-back
    /// kills it ([`crate::sim::DeathCause::Lost`]), like every other
    /// write-back.
    pub recycle: u32,
    /// 1 when this think gave the unit up for making no net progress (card
    /// RCYC, [`StallState`]): the write-back kills it as
    /// [`crate::sim::DeathCause::Stalled`].
    pub stalled: u32,
    /// 1 when this think withheld a shot it was on target for, because a
    /// friend stood in the line of fire (card FFLN).
    pub line_held: u32,
    /// Card TPOS: this think asked the Tactician (1), and a candidate with
    /// cover from the target passed its filters (2) - B12's ceiling.
    pub tpos_query: u8,
    /// Card TPOS: what this think's Tactician did, one bit per event
    /// (`crate::ai::tactic::EV_*`: the query's answer, why a position was
    /// left, how a walk to one ended) - the fight-time split's counters.
    pub tpos_ev: u16,
    /// Card TPOS: this think's query's candidates with cover from the
    /// target, stage by stage (the sweep's `tfunnel` line: within the box,
    /// on the storey, kept by the cap, each filter's rejects, passed, the
    /// drop test's and the confirming ray's rejects, scored with no pick).
    pub tpos_funnel: [u8; 14],
    /// Card UCALL: 1 when this think's tracker TOOK a target (the first, or
    /// another in place of the held one) - BF1's `AcquiredTargetSound` call.
    pub acquired: u8,
    /// Card UCALL: 1 when this think's hearing turned the unit to a heard
    /// enemy - BF1's `HeardEnemySound` call.
    pub heard_turn: u8,
    /// Card UTALK: [`NavTick::SET_OUT`] when this tick the bot began walking
    /// to its post from anything else, [`NavTick::ARRIVED`] when its walk
    /// reached the post - the trooper agent's two points where BF1's AI says
    /// "follow me" / "move out" (re-squad-orders.md 5).
    pub trooper: u8,
    /// Card TURR: this think's turret business - 1..=5 the exit rule a
    /// gunner left its turret by ([`crate::ai::man`]), [`NavTick::MAN_UNREACHED`]
    /// a claimed turret its walk did not reach; 0 otherwise.
    pub man: u8,
}

/// What a bot carries between ticks. Card SPL11: the fields are grouped by
/// the layer that OWNS them (`analysis/bot-design.md` 3) — perception, the
/// behaviour, the navigator, the mover, the gunner. None is dropped yet; the
/// cards that replace a rule take its fields with it. [`AiState::hash_into`]
/// keeps its own, older order: the grouping moves no hash.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiState {
    // ── Perception: what the bot knows of the enemy (`crate::perception`).
    /// The enemy this unit is aiming at (Phase C decision 2/3): ALWAYS one of
    /// `contacts` — a bot may aim or fire only at a contact its perception
    /// holds, which is the rule that makes the top tiers fair. Was `engage`
    /// (the nearest visible enemy inside a 60 m constant) before Phase C.
    pub target: Option<UnitHandle>,
    /// The bounded contact memory this unit perceives the world through
    /// (`crate::perception`): six slots, oldest evicted, cleared 3 s after the
    /// last sighting or sound.
    pub contacts: [Option<crate::perception::Contact>; crate::perception::CONTACTS],
    /// Tick of the last target SWITCH — the 1.5 s minimum retarget interval
    /// of decision 3 measures from here.
    pub last_switch: u64,
    // ── Behaviour: the order it is under and what it is doing about it (`behave`).
    pub next_think: u64,
    pub target_cp: Option<u32>,
    /// The squad this unit is in (Phase B): the key of the objective it shares
    /// with at least one other unit of its team within
    /// [`crate::commander::SQUAD_RADIUS`], `None` when it is on its own.
    /// BOOKKEEPING ONLY — the sim never steers off it. Phase D's orders live
    /// on `Unit::command`/`CmdState` instead; this field's only readers are the
    /// sweep's dump and the tests.
    /// Written by the commander, hashed like the rest of the AI state.
    pub squad: Option<u32>,
    /// D6: what this unit is doing about its squad order, `None` for every
    /// unit that has no order — which is every unit in a round nobody
    /// commands. Hashed only when `Some` (see [`CmdState`]).
    pub cmd: Option<CmdState>,
    /// Card VAI: this bot's vehicle business ([`RideState`]), `None` for a
    /// bot that has none — every bot on a map without vehicles. Hashed only
    /// when `Some`, after `cmd`, so a vehicle-free sim's AI hash is the
    /// pre-VAI one.
    pub ride: Option<RideState>,
    /// GBOARD: one BF1 call target, (receiver identity, vehicle identity).
    /// Taken only on a scheduled think; absent state adds no hash bytes.
    pub pending_board: Option<(crate::unit::UnitHandle, crate::vehicle::VehicleHandle)>,
    /// Card BNDS: this bot is off the playable level ([`LostState`]); `None`
    /// on it. Hashed only when `Some`, after `ride`.
    pub lost: Option<LostState>,
    /// Card HOLD: the position this bot holds and fights from, or the hubs
    /// it owes before the next one ([`HoldState`]). Hashed only when `Some`,
    /// after `lost`.
    pub hold: Option<HoldState>,
    /// Card FGHT: what that position is in a fight ([`FightState`]). Hashed
    /// only when not the default, after `mv`.
    pub fight: FightState,
    /// Card TPOS: the Tactician's position ([`TposState`]). Hashed only when
    /// not the default, after `fight`.
    pub tpos: TposState,
    /// D13: the stance this unit wants to be in, as the CODE
    /// [`crate::commander::stance_of_code`] reads (1 Stand, 2 Crouch, 3 Prone)
    /// — **0 means "no wish"**, which is why the code and not the `Stance`
    /// discriminant is what is stored (`Stance::Stand`'s own discriminant is
    /// 0). Written every think from the defence spot's hint, or — for a unit
    /// with no objective and no route — from the nearest hint to where it
    /// stands. It is the DRAWN pick, not the authored value: the hint answers
    /// an allowed-stance MASK and [`stance_hold`] draws one of its bits once
    /// and keeps it while the mask allows. Hashed like the rest of the AI state.
    pub want_stance: u8,
    /// D11: no dodge before this tick (the 2 s cooldown, RE §4.1).
    pub dodge_until: u64,
    /// Consecutive ticks this unit has spent within
    /// [`crate::commander::DWELL_RADIUS`] of the post of an objective it has
    /// NOT completed — the sweep's `unreachable_dwell_max`. It is a
    /// diagnostic, but it is derived from position and carried between ticks,
    /// so like `Route::stuck_run` it is sim state and it is hashed.
    pub unreachable_dwell: u16,
    // ── Navigator: the route (`route`).
    /// The route being walked (Phase A). `None` = no graph, or nothing on it
    /// to walk to.
    pub route: Option<Route>,
    // ── Mover: the steering's memory, the progress books (`mover`, `steer`, `stall`).
    /// What the steering picked last tick (Phase A.1). Carried so
    /// ROUND_VERBOSE can report it for a unit that is stuck right now; hashed
    /// like every other piece of AI state.
    pub steer: SteerPick,
    /// Which way round the obstacle this unit is going: +1, -1, or 0 when
    /// there is nothing to go round.
    ///
    /// It has to be REMEMBERED. Recomputing the side every tick from the
    /// contact normal flips it whenever the approach is near head-on (the
    /// tangent is ill-conditioned exactly there), and a unit that alternates
    /// left and right at 7 m/s runs full speed and gets nowhere — which is
    /// what BF2 TAN1's eight jammed units were doing. The PS2 remembers its
    /// own choice the same way (`this+0x70`, the corner side, is picked ONCE
    /// per target; the contact side-step is held in soldier+0x980 while the
    /// push lasts — RE_LOG "Local obstacle avoidance").
    ///
    /// Card OBST: it is a COMMITMENT ([`MoverState::round`], `avoid::pick`) —
    /// kept over open ticks and over the progress window until the obstacle
    /// is left, changed at a dead end once. A FAILED progress window ends the
    /// commitment and writes the side-step's side here, so the PS2's "the
    /// side we did not use last time" starts the next one.
    pub slide: i8,
    /// DIAGNOSTIC ONLY (card BLKF) — never hashed, never read by a rule: what
    /// the steering was asked this tick, for the playtest black box. Default
    /// (kind 0) on a tick the bot did not walk.
    pub walk: WalkAsk,
    /// D11: a dodging unit with NO route strafes until this tick...
    pub strafe_until: u64,
    /// ...to this side (+1 right, −1 left; 0 when not strafing).
    pub strafe_side: i8,
    /// Card RCYC: the no-net-progress watch of a bot that has somewhere to
    /// go ([`StallState`]); `None` until it first wants to walk and after
    /// every arrival. Hashed only when `Some`, after `hold`.
    pub stall: Option<StallState>,
    /// Card MOVR: the mover's books ([`MoverState`]): the committed
    /// destination and the arrived latch, the held heading, the Recover
    /// ladder. Hashed only when not the default, after `stall`.
    pub mv: MoverState,
    /// DIAGNOSTIC ONLY (card MOVR) — never hashed, never read by a rule: what
    /// the bot is doing this tick ([`Behaviour`]).
    pub behaviour: Behaviour,
    // ── Gunner: the aim and the trigger (`gunner_foot`; a seat gun shares the bursts).
    /// The aim tracker's direction and its rate (brief D9, RE §4.2): the AI
    /// does not snap its pad at the target any more, it steers a unit vector
    /// toward the wanted one under an acceleration clamp and a damping, both
    /// from the skill vector. `Vec3::ZERO` means "not tracking" — the next
    /// tick with a target starts it at the unit's own facing.
    pub aim_dir: Vec3,
    /// The tracker's angular rate (D9). Zeroed by a slew and by a new target.
    pub aim_vel: Vec3,
    /// A BF1 round's tracked-position error radius in metres (brief D10; a
    /// BF2 round's error is the TARGET's `unit::AimBlock`, card CLSE2, and
    /// this stays at its start value): the aim point is the lead point
    /// offset by up to this far in a direction re-drawn once per think. It
    /// DECAYS toward the skill's floor while the target is tracked and GROWS
    /// with the target's movement, and it persists across targets.
    /// PROVISIONAL: the PS2's BF1 has no error at all (card CLSE2B).
    pub err: f32,
    /// The tracked position at the last think — the error grows with how far
    /// the target moved since (D10).
    pub err_pos: Vec3,
    /// The AI sniper distance table's yaw half-angle for this tick, already
    /// multiplied by the skill's `dispersion_mult` (brief D9, LEAD ADDENDUM
    /// 4b — `analysis/re-ai-sniper.md` §2). `None` = this unit's weapon fails
    /// [`crate::weapon::WeaponClass::ai_distance_table`] or it has no target
    /// this tick, and the weapon uses the class `YawSpread` as before. It
    /// reaches the weapon through [`crate::weapon::FireInput::ai_dispersion`]
    /// (`sim/step.rs`, the fire pass), which is where the PS2 keeps it too
    /// (`weapon+0x138`, rewritten by the AI every aim tick).
    pub dispersion: Option<f32>,
    /// D11: shots left in this burst. 0 means "between bursts" — a new length
    /// is drawn once [`AiState::pause_until`] has passed.
    pub burst: u8,
    /// D11: no FIRE press before this tick (the inter-burst pause).
    pub pause_until: u64,
    /// Card STTAP, BF2: a single-shot weapon's trigger gate (`ai+0xf0`) — set
    /// by a press and by an idle tick, cleared once [`AiState::tap_until`]
    /// has passed; a press needs it clear.
    pub tap_gate: bool,
    /// Card STTAP, BF2: the tick the gate may clear after (`ai+0xf4`).
    pub tap_until: u64,
    /// Card STTAP: a sniper's cooldown, `U(2, 3)` s after a press (BF2
    /// `ai+0xe8`, BF1 `+0x1b0`).
    pub snipe_until: u64,
    /// Card STTAP, BF1: the controller's trigger byte (`ai+0x48`), which a
    /// single-shot weapon runs 1 → 2 → −1 → −2 → 1.
    pub tap_byte: i8,
    /// D11: a charge weapon holds the trigger until this tick and fires on
    /// the release. 0 = not charging.
    pub charge_until: u64,
    /// Card GRAI: the weapon the bot fights with ([`super::arsenal`]).
    pub arm: ArmState,
    /// Card SNPR: a sniper's spot ([`SnipeBook`], `super::snipe`) — hashed
    /// only when it differs from the default (a bot that never held a
    /// scoped rifle), after `arm`.
    pub snipe: SnipeBook,
    /// Card TEAM: the role the team board gave the bot ([`RoleState`],
    /// `super::team::board`) — hashed only when it differs from the
    /// default, after `snipe`.
    pub role: RoleState,
    /// Card FLNK: where the bot's attack began and the flank route it walks
    /// ([`FlankState`], `super::team::board`) — hashed only when it differs
    /// from the default, after `role`.
    pub flank: FlankState,
}

/// Card FLNK (`analysis/bot-design.md` 6.2; OURS): the hub a bot's route to
/// the post it attacks was planned from, that post's hub, and the flank
/// route the team board gave it (`crate::nav::flank`; `plan_leg` walks its
/// hubs, then the authored route).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlankState {
    /// The hub the attack's route was planned from and the attacked post's
    /// hub; [`FlankState::NONE`] = none.
    pub from: u16,
    pub to: u16,
    /// The flank route walked (an index into `Sim::flank`), and the tick it
    /// was given.
    pub route: Option<u16>,
    pub since: u64,
}

impl FlankState {
    pub const NONE: u16 = u16::MAX;

    fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(&[12]);
        eat(&self.from.to_le_bytes());
        eat(&self.to.to_le_bytes());
        eat(&self.route.unwrap_or(u16::MAX).to_le_bytes());
        eat(&self.since.to_le_bytes());
    }
}

impl Default for FlankState {
    fn default() -> Self {
        FlankState {
            from: FlankState::NONE,
            to: FlankState::NONE,
            route: None,
            since: 0,
        }
    }
}

/// Card TEAM (`analysis/bot-design.md` 4.3; OURS): a role inside a post's
/// group, given and ended by the team board (`super::team::board`). Today
/// the one role is COVER: while a friend captures `post`, guard position
/// `at` (an index into the position table), facing `look`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RoleState {
    /// The post covered; `None` = no role (Assault, the default).
    pub post: Option<u8>,
    /// The tick the role was given.
    pub since: u64,
    /// The guard position.
    pub at: u32,
    /// The point it faces: the approach to the post.
    pub look: Vec3,
}

impl RoleState {
    fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(&[11, self.post.map_or(0, |p| p.wrapping_add(1))]);
        eat(&self.since.to_le_bytes());
        eat(&self.at.to_le_bytes());
        for v in [self.look.x, self.look.y, self.look.z] {
            eat(&v.to_bits().to_le_bytes());
        }
    }
}

/// Card SNPR (OURS): a sniper's spot with a long lane onto enemy ground,
/// where it walks and which it holds while the lane is busy
/// (`super::snipe`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SnipeState {
    /// The point stood on: a Snipe hint's, or a high hub's on its floor.
    pub spot: Vec3,
    /// The hint's allowed-stance mask (0 = none, a hub).
    pub mask: u8,
    /// It has reached the spot and holds it.
    pub arrived: bool,
    /// The last tick an enemy was seen from the spot (the arrival's at first).
    pub lane_at: u64,
    /// The lane's length when the spot was picked, m: its farthest enemy-side
    /// point in sight (a diagnostic the census reads; not a rule's input).
    pub lane: f32,
}

/// Card SNPR: the spot a sniper has ([`SnipeState`]), the one it left last
/// (not picked again next), and no pick before `next`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SnipeBook {
    pub cur: Option<SnipeState>,
    /// The spot left last; `None` = none.
    pub left: Option<Vec3>,
    pub next: u64,
}

impl SnipeBook {
    fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(&[
            10,
            u8::from(self.left.is_some()) | u8::from(self.cur.is_some()) << 1,
        ]);
        eat(&self.next.to_le_bytes());
        if let Some(l) = self.left {
            for v in [l.x, l.y, l.z] {
                eat(&v.to_bits().to_le_bytes());
            }
        }
        if let Some(c) = self.cur {
            eat(&[c.mask, u8::from(c.arrived)]);
            eat(&c.lane_at.to_le_bytes());
            for v in [c.spot.x, c.spot.y, c.spot.z, c.lane] {
                eat(&v.to_bits().to_le_bytes());
            }
        }
    }
}

/// Card GRAI: a bot's weapon choice between thinks (`super::arsenal`,
/// `analysis/re-grenades.md` 7) — hashed only when it differs from the
/// default (a bot that never picked anything but the weapon in its hands).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ArmState {
    /// The class slot the last think picked for the target; `None` = the
    /// weapon in the hands as it is (no target, or nothing usable).
    pub pick: Option<u8>,
    /// The picked weapon had the target inside its `MinRange..MaxRange`.
    pub in_range: bool,
    /// The target stood in a group of its own side until this tick (the
    /// PS2's 5 s stamp, 7.2): a grenade is forced at once.
    pub group_until: u64,
    /// No group test before this tick (BF1: once per 5 s; BF2: once a second).
    pub group_next: u64,
    /// The explosive claim (`re-soldier-ai.md` 4.3): this bot is the one of
    /// its team that may use a grenade on that soldier, until the tick.
    pub claim: Option<(UnitHandle, u64)>,
    /// Card HVYW: the target rides a vehicle and `pick` is a weapon that
    /// hurts it, not a grenade, with the hull in its band — the bot stands
    /// and fights it (`re-soldier-ai.md` 22).
    pub vehicle: bool,
    /// ... or no weapon of the bot's hurts that vehicle: it does not attack
    /// it at all — no shot, no fight (OURS: past 10 m; nearer it fires what
    /// it holds). Also a soldier for whom the bot has only the heavy weapon
    /// in its hands (OURS: no rocket at a lone man).
    pub spare: bool,
}

impl ArmState {
    fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(&[
            self.pick.map_or(0, |s| s.wrapping_add(1)),
            u8::from(self.in_range) | u8::from(self.vehicle) << 1 | u8::from(self.spare) << 2,
        ]);
        eat(&self.group_until.to_le_bytes());
        eat(&self.group_next.to_le_bytes());
        if let Some((h, until)) = self.claim {
            eat(&h.index.to_le_bytes());
            eat(&h.gen.to_le_bytes());
            eat(&until.to_le_bytes());
        }
    }
}

/// Card RCYC: the last resort's watch (`crate::ai::stall`). A bot that wants
/// to walk and makes no progress for [`crate::ai::RECYCLE_AFTER`] such ticks
/// is given up: [`crate::ai::RECYCLE_ROAM`] from `at` mid-round,
/// [`crate::ai::RECYCLE_NET`] beyond the box `lo..hi` in the endgame. `None` for a bot that has arrived
/// or has nowhere to go — hashed only when `Some`, after `hold`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StallState {
    /// Where the bot stood when the count last started.
    pub at: Vec3,
    /// The ground covered since the watch opened: the box round every
    /// position that was new ground (a point when it opens)...
    pub lo: Vec3,
    /// ...and its upper corner.
    pub hi: Vec3,
    /// Ticks it has wanted to walk since its last progress.
    pub ticks: u32,
    /// Card RLOOP: where the walk's failures began to count...
    pub pin: Vec3,
    /// ...and the failed progress windows since, never
    /// [`crate::ai::PIN_ROAM`] from `pin`.
    pub fails: u8,
    /// ...and where the bot stood when it was last sent on a floor walk
    /// ([`LostState::pinned`]): given up again near there, it is recycled.
    pub walked: Option<Vec3>,
}

impl StallState {
    fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        // Tag 5: after `HoldState`'s 4.
        eat(&[5]);
        for v in [
            self.at.x, self.at.y, self.at.z, self.lo.x, self.lo.y, self.lo.z, self.hi.x, self.hi.y,
            self.hi.z,
        ] {
            eat(&v.to_bits().to_le_bytes());
        }
        eat(&self.ticks.to_le_bytes());
        for v in [self.pin.x, self.pin.y, self.pin.z] {
            eat(&v.to_bits().to_le_bytes());
        }
        eat(&[self.fails]);
        if let Some(w) = self.walked {
            for v in [w.x, w.y, w.z] {
                eat(&v.to_bits().to_le_bytes());
            }
        }
    }
}

/// Card VAI: what a bot carries about boarding and leaving a vehicle. The
/// decisions are openbf's own (PLAN §AI; `crate::ai` `BOARD_*`, PROVISIONAL);
/// the entry itself is the PS2's instant Enter slot (re-vehicles §5).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RideState {
    /// WAIT4: once per ground pilot tenure; absent for other seats.
    pub pickup: Option<PickupWait>,
    /// VBAIL: 0..=100, once per successful AI hover/walker boarding.
    /// PROVISIONAL lifetime: the card asks per boarding; re-walker-ai §1
    /// records a PS2 controller-construction draw instead.
    pub bail_draw: Option<u8>,
    /// This ride has been counted among pilots exposed to <20% health.
    pub bail_seen: bool,
    /// The vehicle it is walking to, to take an admitted seat.
    pub board: Option<crate::vehicle::VehicleHandle>,
    /// WGORD: explicit boarding over a retained player Hold/Follow command.
    /// The command and its scratch remain in their ordinary, hashed fields.
    pub ordered_call: bool,
    /// WGPAS: exact receiver of an explicit BF1 ground call, only while claiming.
    pub called_board: Option<crate::unit::UnitHandle>,
    /// A terminated called approach must respect the existing boarding cooldown.
    pub call_blocked: bool,
    /// The tick that claim began (the give-up clock).
    pub since: u64,
    /// CWAIT: last valid ground-boarding think; independent of `since`.
    /// Cleared on claim release or seating, ignored without `board`.
    pub maintenance: Option<BoardMaintenance>,
    /// This tick's request for step 1c: enter `board` on foot, get out in a
    /// seat — the port's stand-in for the PS2's queued enter request (B1
    /// FUN_0023c418 → FUN_0023d418, re-vehicles §5).
    pub use_req: bool,
    /// No new claim before this tick (after a dismount or a failed entry).
    pub block_until: u64,
    /// Card WAI: at a walker's pilot seat, this tick's sticks (throttle p2,
    /// strafe p3) as the PS2's AI writes them — raw, for `Sim::ride` to hand
    /// to the walker past the pad (`walker_drive`). `None` anywhere else.
    pub sticks: Option<[f32; 2]>,
    /// Card VDROP: at a hover's pilot seat, this tick's sticks and the
    /// hover's driving state moved on (`hover_drive`), for `Sim::ride`.
    pub hover: Option<HoverOut>,
    /// Card TURR: the building turret closed to this bot until
    /// `closed_until` - one it left, or its walk did not reach (`man`).
    pub closed: Option<crate::vehicle::VehicleHandle>,
    pub closed_until: u64,
    /// Card TURR: in a building turret's seat, the gunner's clocks.
    pub man: Option<ManState>,
}

/// A ground boarding claim's maintenance and the unit generation owning it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardMaintenance {
    pub owner: UnitHandle,
    pub tick: u64,
}

/// Initial ground-driver window (`analysis/re-driver-wait.md` §§2,5,7).
/// Completion is permanent until the pilot leaves this vehicle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickupWait {
    pub vehicle: crate::vehicle::VehicleHandle,
    pub seated: u64,
    pub started: Option<u64>,
    /// BF1 release order time: effective expiry is two seconds before this.
    pub released: Option<u64>,
    pub complete: bool,
    /// WGOUT: first valid BF1 routed departure consumed its call attempt.
    pub departed: bool,
}

/// Card TURR: what a bot in a building turret's seat carries (`man`): the
/// ticks it sat down, last had a target the turret can hit and last had a
/// purpose there, and the point it looks at with no target seen (the
/// last known point of the enemy it can hit).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ManState {
    pub seated: u64,
    pub hittable: u64,
    pub purpose: u64,
    pub look: Option<Vec3>,
    /// The point this tick's aimer chases (`gunner::aim`'s aim point, else
    /// its idle point), which `Sim::seat_gun` takes in place of the camera
    /// ray's hit: on the PS2 an AI seat's aimer is handed its wanted
    /// direction by the AI's own aim (B1 `FUN_0026d748` calls `FUN_00236700`
    /// when the seat has no player, B1:191963-191967; re-turret-draw 2.3).
    pub aim: Option<Vec3>,
}

/// What a hover's driver hands `Sim::ride`: throttle, strafe and turn as
/// the PS2's AI writes them, raw, and the vehicle's [`HoverAi`] after this
/// tick.
///
/// [`HoverAi`]: crate::vehicle::hover::HoverAi
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoverOut {
    pub sticks: [f32; 3],
    pub ai: crate::vehicle::hover::HoverAi,
}

impl NavTick {
    pub const CLAIM_NEW: u8 = 1;
    pub const CLAIM_GIVEUP: u8 = 2;
    pub const CLAIM_LOST: u8 = 3;
    pub const SET_OUT: u8 = 1;
    pub const ARRIVED: u8 = 2;
    /// [`NavTick::man`]: a claimed turret the walk did not reach.
    pub const MAN_UNREACHED: u8 = 6;
}

/// Card BNDS: what a bot that is OFF the playable level carries
/// ([`crate::sim::Level`], `think`'s return walk). `None` for every bot on
/// the level — hashed only when `Some`, after `ride`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LostState {
    /// The tick the level test first read OFF in this spell.
    pub since: u64,
    /// The test read OFF on two thinks running: the return walk is on.
    pub confirmed: bool,
    /// How many return hubs have been given up on (a stuck window each).
    pub tries: u8,
    /// No jump at a low wall before this tick (0 = none taken yet).
    pub jump_after: u64,
    /// Ticks into the return walk's progress window...
    pub window: u16,
    /// ...and the XZ distance to the return point when it opened.
    pub ref_dist: f32,
    /// Card VMESH: the spell is a bot on foot SHUT INSIDE a map body
    /// (`CollisionWorld::enclosing`), not one off the level: it is given up
    /// after `SHUT_RECYCLE`, whatever it is doing.
    pub shut: bool,
    /// Card RETW: the floor walk back to the level (`Sim::return_path`),
    /// found when the spell is confirmed; `path.len` 0 = none found.
    pub path: crate::sim::ReturnPath,
    /// ...the point of it walked at now (`path.len` = spent or given up).
    pub path_at: u8,
    /// ...and how many times one was looked for in this spell.
    pub plans: u8,
    /// Card RLOOP: the spell is a bot ON the level given up at one place
    /// ([`crate::ai::PIN_FAILS`]), walking `path` to its leg's waypoint: it
    /// ends when the walk is spent and the route goes on; a window of it
    /// with no progress gives the bot up for good (`mover::run`).
    pub pinned: bool,
}

/// What a bot is DOING this tick (card MOVR; `analysis/bot-design.md` 4.4):
/// the link of `think`'s chain that runs, named. DIAGNOSTIC — never hashed,
/// never read by a rule: the sweep's unexplained-standing counter (B9) and
/// the black box read it. `None` is a bot with nowhere to be and nobody to
/// fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Behaviour {
    #[default]
    None,
    /// Walking its route to the order's point.
    Travel,
    /// Fighting a seen target inside its weapon's range — standing to shoot
    /// or circling (card FGHT) — or on a held position (card HOLD).
    Fight,
    /// Running from a grenade.
    Evade,
    /// Arrived at the post it attacks.
    Capture,
    /// Arrived at the post, or the spot, it defends.
    Guard,
    /// Walking to the vehicle it has claimed.
    Board,
    /// In a seat (`ride::drive`).
    Ride,
    /// Under a squad order (Follow / Hold area).
    Follow,
    /// The walk keeps failing, or the bot is off the level: the ladder of
    /// `behave::recover`, the return walk.
    Recover,
    /// Card SNPR: on its sniper's spot, held while its lane is busy
    /// (`super::snipe`; bot design 4.3's Overwatch role).
    Overwatch,
    /// Card TURR: in a building turret's seat (`man::seated`).
    Man,
}

impl Behaviour {
    /// The short name the sweep's rows and the black box print.
    pub fn tag(self) -> &'static str {
        match self {
            Behaviour::None => "none",
            Behaviour::Travel => "travel",
            Behaviour::Fight => "fight",
            Behaviour::Evade => "evade",
            Behaviour::Capture => "capture",
            Behaviour::Guard => "guard",
            Behaviour::Board => "board",
            Behaviour::Ride => "ride",
            Behaviour::Man => "man",
            Behaviour::Follow => "follow",
            Behaviour::Recover => "recover",
            Behaviour::Overwatch => "overwatch",
        }
    }

    /// A bot that stands in this behaviour has a reason to (B9's list:
    /// Guard, Overwatch, Capture, Man).
    pub fn stands(self) -> bool {
        matches!(
            self,
            Behaviour::Capture
                | Behaviour::Guard
                | Behaviour::Ride
                | Behaviour::Overwatch
                | Behaviour::Man
        )
    }
}

/// Card MOVR: what the mover answers for the tick's [`MoveRequest`]
/// (`analysis/bot-design.md` 4.5, 6.3: every walk has an outcome, and the
/// behaviour that asked handles all three).
///
/// [`MoveRequest`]: super::mover::MoveRequest
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Walk {
    /// Nothing was asked, or the bot stands by its own rule (an enemy in
    /// hold range).
    #[default]
    Idle,
    /// On its way.
    Following,
    /// On the request's destination: the stick is released until the
    /// destination moves ([`crate::ai::DEST_DEADBAND`]) or the request's
    /// kind changes.
    Arrived,
    /// A progress window closed with no progress this tick
    /// (`mover::stuck_window`).
    Failed,
}

/// Card MOVR: the mover's own books (`analysis/bot-design.md` 4.7) — the
/// destination it is committed to and whether it stands on it, the heading
/// it holds between its picks, and the Recover ladder's count. Hashed only
/// when it is not the default, after `stall`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MoverState {
    /// The destination the mover is committed to: a request whose
    /// destination is within [`crate::ai::DEST_DEADBAND`] of it (and of the
    /// same kind) is this one still.
    pub dest: Vec3,
    /// ...and the request's kind ([`WalkAsk::RETURN`] .. [`WalkAsk::LEG`]; 0
    /// = none yet).
    pub kind: u8,
    /// The bot has ARRIVED at `dest` and stands: the stick stays released
    /// until the request changes.
    pub arrived: bool,
    /// The Recover ladder (`behave::recover`): the walk's failures since it
    /// last made progress (0 = not recovering).
    pub recover: u8,
    /// The post given up, + 1 (0 = none): the commander draws again without
    /// it at its next plan and clears this.
    pub give_up: u32,
    /// The heading the steering chose on this bot's last pick (world XZ,
    /// unit; zero = none): held between picks.
    pub heading: Vec3,
    /// Card OBST: the obstacle the bot is going round ([`Round`]).
    pub round: Round,
}

/// Card OBST (`ai/avoid.rs`): the obstacle a bot is going round. The SIDE is
/// [`AiState::slide`]; this is what makes it a commitment — the side holds
/// until the bot is nearer its aim point than where it met the obstacle.
/// Hashed only when it is not the default.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Round {
    /// The nearest the bot has been to the aim point since the obstacle was
    /// met (XZ, m); 0 = no commitment (the side, if any, is only asked for).
    pub meet: f32,
    /// The aim point the commitment belongs to: another one ends it.
    pub aim: Vec3,
    /// The height of the feet where the obstacle was met (m): the way along
    /// it does not climb far over this and the aim point.
    pub y0: f32,
    /// The nearest any ended commitment got to `aim` (m; 0 = none): the
    /// leave rule measures against the whole walk at this aim point, so a
    /// penned bot does not "leave" its pen's wall once a lap.
    pub best: f32,
    /// Ticks spent going round since a FREE walk last got nearer its
    /// waypoint in a progress window: the allowance `avoid::ROUND_MAX` is of
    /// these — leaving an obstacle, a failed window or another aim point
    /// does not start it again (a penned bot leaves its pen's wall once a
    /// lap). Once SPENT it does not come back in that life (it is cleared
    /// only while under the allowance, `mover::run`), and the walk's
    /// failures count on to the post's give-up. MEASURED (card OBSR, 32
    /// seeds x 32 maps on the rim rule): starting it again when a free walk
    /// gets nearer its aim point than the walk has been moves no recycle
    /// count (206 -> 201) and costs BF1 Bespin 10 % more deaths under the
    /// floor (1,078 -> 1,189, t +2.5 over all maps) — not taken.
    pub ticks: u16,
    /// The side has been changed at a dead end, once.
    pub flipped: bool,
    /// The heading last walked along the obstacle (world XZ, unit; zero =
    /// none): where the way to the aim point is open and it is too soon to
    /// leave, the bot goes on along this.
    pub dir: Vec3,
    /// Where the bot stood when the current progress window opened: a bot
    /// going round is making progress when it has moved on from here.
    pub anchor: Vec3,
    /// Card OBSR: what last shut the way to the aim point was a rim barrier
    /// (`steer::StopKind::Barrier`) — an edge of the level, with no far side
    /// to come round to: the progress window's allowance is not for it.
    pub rim: bool,
    /// Card OBSV (`avoid`'s rule 5): the standing vehicle whose footprint
    /// the bot is passing (its slot + 1; 0 = none)...
    pub foot: u16,
    /// ...and by which of the two corners it shows (-1 / +1). Kept until the
    /// footprint is behind; it is no contact, so not [`Round::live`].
    pub foot_side: i8,
}

impl Round {
    /// Is a commitment live (a side chosen at a contact)?
    pub fn live(&self) -> bool {
        self.meet > 0.0
    }

    /// The commitment ends; the walk's books stay (the aim point and the
    /// nearest it got to it, the window's anchor, the allowance spent).
    pub fn end(&mut self) {
        let best = match (self.best > 0.0, self.meet > 0.0) {
            (true, true) => self.best.min(self.meet),
            (true, false) => self.best,
            _ => self.meet,
        };
        *self = Round {
            best,
            aim: self.aim,
            anchor: self.anchor,
            ticks: self.ticks,
            ..Round::default()
        };
    }
}

impl MoverState {
    fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        // Tag 6: after `StallState`'s 5.
        eat(&[6, self.kind, u8::from(self.arrived), self.recover]);
        for v in [
            self.dest.x,
            self.dest.y,
            self.dest.z,
            self.heading.x,
            self.heading.z,
        ] {
            eat(&v.to_bits().to_le_bytes());
        }
        eat(&self.give_up.to_le_bytes());
        // Tag 7: the commitment round an obstacle (card OBST).
        let r = &self.round;
        if *r != Round::default() {
            eat(&[7, u8::from(r.flipped) | (u8::from(r.rim) << 1)]);
            eat(&r.ticks.to_le_bytes());
            for v in [
                r.meet, r.best, r.y0, r.aim.x, r.aim.y, r.aim.z, r.anchor.x, r.anchor.y,
                r.anchor.z, r.dir.x, r.dir.z,
            ] {
                eat(&v.to_bits().to_le_bytes());
            }
            // (Card OBSV: only when a footprint is being passed.)
            if r.foot != 0 {
                eat(&[9, r.foot_side as u8]);
                eat(&r.foot.to_le_bytes());
            }
        }
    }
}

/// What the steering was asked on one tick ([`AiState::walk`]): the black
/// box's read-only hook into `think` (card BLKF). Not hashed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WalkAsk {
    /// Which link of `think`'s goal chain set the point walked at:
    /// 0 the bot did not walk, [`WalkAsk::RETURN`] .. [`WalkAsk::LEG`].
    pub kind: u8,
    /// The point walked at.
    pub goal: Vec3,
    /// The direction (XZ, unit) and the lowest floor handed to `avoid::pick`.
    pub dir: Vec3,
    pub low: f32,
}

impl WalkAsk {
    /// A lost bot's return point.
    pub const RETURN: u8 = 1;
    /// Out of a hazard.
    pub const FLEE: u8 = 2;
    /// A held position's goal.
    pub const HOLD: u8 = 3;
    /// The vehicle it boards.
    pub const BOARD: u8 = 4;
    /// The route's walk point or corner, an order's point, the post.
    pub const LEG: u8 = 5;
}

/// Card FGHT: what a fighting bot's position ([`HoldState`]) is —
/// `analysis/re-soldier-ai.md` 4.4, 4.5: the spot it STANDS to shoot from, or
/// the point of the ring round its target it walks to and pauses on. All
/// zero for a bot that is not fighting; hashed only then not.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FightState {
    /// [`FightState::STAND`] .. [`FightState::COVER`]; 0 = no fight position.
    pub mode: u8,
    /// The circle's pause lasts until this tick; a new ring point is drawn
    /// after it.
    pub pause_until: u64,
}

impl FightState {
    /// Standing to shoot on a spot of its own.
    pub const STAND: u8 = 1;
    /// On its way to a point of the ring round its target.
    pub const WALK: u8 = 2;
    /// Pausing on (or short of) that point.
    pub const PAUSE: u8 = 3;
    /// Card REACT: on its way to the position a hit sent it to — walked as
    /// a ring point, by a stander too.
    pub const MOVE: u8 = 4;
    /// Card TPOS: on its way to the position the Tactician gave it
    /// ([`TposState`]), walked as a ring point.
    pub const TAKE: u8 = 5;
    /// Card TPOS: at that position, in its hide / fire cycle.
    pub const COVER: u8 = 6;

    /// Is the bot on its way to a point (a ring's, a reaction's, a
    /// position's)?
    pub fn walks(&self) -> bool {
        matches!(self.mode, Self::WALK | Self::MOVE | Self::TAKE)
    }

    /// Is the bot on a Tactician's position, on its way or there?
    pub fn tactician(&self) -> bool {
        matches!(self.mode, Self::TAKE | Self::COVER)
    }

    fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        // Tag 8: after `MoverState`'s 6 and its round's 7.
        eat(&[8, self.mode]);
        eat(&self.pause_until.to_le_bytes());
    }
}

/// Card TPOS: the position of the map's table (`Sim::positions`) the
/// Tactician gave this bot (`ai::tactic`), its reservation and its hide /
/// fire cycle. Hashed only when not the default, after `fight`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TposState {
    /// The position held (a table index); `None` = none.
    pub held: Option<u32>,
    /// The tick it was granted (the write-back's tie rule, `tactic::grant`).
    pub at: u64,
    /// The reservation lasts to this tick: renewed every tick it is held
    /// (`tactic::keep`, card TPOSR),
    /// so a position is free [`crate::ai::tactic::FREE_AFTER`] after it is
    /// left (the PS2's hint rule).
    pub until: u64,
    /// The position held before, reserved to its own `prev_until`.
    pub prev: Option<u32>,
    pub prev_until: u64,
    /// The cycle: firing (else hidden), to `phase_until`.
    pub firing: bool,
    pub phase_until: u64,
    /// Another position is looked for at this tick (5-12 s after arriving).
    pub next_at: u64,
    /// Hits taken at it: two and it is compromised.
    pub hits: u8,
    /// A CORNER position's lean side (+1 / -1 across the line to the
    /// target; 0 = LOW cover or none).
    pub lean: i8,
    /// It had cover from the target when chosen.
    pub covered: bool,
    /// No query before this tick (a query that found nothing waits).
    pub retry_at: u64,
    /// The positions whose walks last failed, newest first: not asked for
    /// again (one remembered let a bot fail two in turn, the maintainer's Kamino
    /// scene).
    pub bad: [Option<u32>; 4],
    /// Where the bot stands for the position held: its point, or where the
    /// walk there ended within [`crate::ai::tactic::NEAR_ENOUGH`] of it (a
    /// point on a ledge's lip it cannot step onto, an open one a step off).
    pub stand: Vec3,
    /// On the way: the nearest (XZ m) the walk has been to the position, and
    /// the tick it last came [`crate::ai::tactic::GAIN`] nearer.
    pub best: f32,
    pub best_at: u64,
    /// The position's next re-score (the target round its cover, out of
    /// the band): every 5 s, the PS2's.
    pub rescore_at: u64,
}

impl TposState {
    fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        // Tag 10: after `FightState`'s 8 and the round's 9.
        eat(&[
            10,
            u8::from(self.firing) | u8::from(self.covered) << 1,
            self.hits,
            self.lean as u8,
        ]);
        for v in [self.held, self.prev].iter().chain(&self.bad) {
            eat(&v.unwrap_or(u32::MAX).to_le_bytes());
        }
        for t in [
            self.at,
            self.until,
            self.prev_until,
            self.phase_until,
            self.next_at,
            self.retry_at,
            self.rescore_at,
        ] {
            eat(&t.to_le_bytes());
        }
        for v in self.stand.to_array().into_iter().chain([self.best]) {
            eat(&v.to_bits().to_le_bytes());
        }
        eat(&self.best_at.to_le_bytes());
    }
}

/// Card HOLD: a bot HOLDING a position and fighting from it (`think`). It is
/// live while `sim.tick < until`; after that it only counts the hubs the bot
/// still has to walk before it may hold again (`skip`). `None` for a bot that
/// neither holds nor owes hubs — hashed only when `Some`, after `lost`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HoldState {
    /// The position taken: a Cover / Snipe hint, or a point in the hub.
    pub spot: Vec3,
    /// Current movement goal: its spot or a reaction/strafe step.
    pub goal: Vec3,
    /// The dwell ends at this tick.
    pub until: u64,
    /// Hubs to walk past before the next hold.
    pub skip: u8,
    /// Legacy shuffle deadline, retained in the hash for compatibility.
    pub shuffle_at: u64,
    /// The side the last strafe went (the next goes the other way).
    pub strafe_side: i8,
    /// The spot's allowed-stance mask (a hint's; 0 = none).
    pub stance_mask: u8,
    /// Where a holder with nobody in sight looks: toward the nearest enemy
    /// (re-read every think).
    pub look: Vec3,
    /// The last tick this bot perceived an enemy from its position: a hold
    /// that stays quiet for [`crate::ai::HOLD_QUIET`] is given up.
    pub last_contact: u64,
    /// Card JITR: `goal` is kept until this tick ([`crate::ai::HOLD_COMMIT`]
    /// after it was taken).
    pub commit_until: u64,
    /// Card JITR: ticks the steering has been turned off the straight line
    /// to `goal` since it was taken ([`crate::ai::HOLD_BLOCKED`]).
    pub blocked: u8,
    /// Card REACT: the tick the roll a hit asked for is pressed, toward
    /// `strafe_side` ([`crate::ai::ROLL_LEAD`] after the hit: a soldier
    /// standing still rolls on the spot); 0 = none.
    pub roll_at: u64,
}

impl HoldState {
    /// Card JITR: the ONE writer of a holder's goal once the position is
    /// taken — the strafe and the on-hit step both come through
    /// here. A step is refused inside the last one's commit time unless it is
    /// `forced` (the on-hit step, which has its own cooldown).
    pub fn step_to(&mut self, p: Vec3, tick: u64, forced: bool) -> bool {
        if !forced && tick < self.commit_until {
            return false;
        }
        self.goal = p;
        self.commit_until = tick + crate::ai::HOLD_COMMIT;
        self.blocked = 0;
        true
    }

    /// Card JITR: the goal cannot be walked straight at — the bot stands
    /// where it is (`pos`), and when that is not beside its spot any more,
    /// this is its spot.
    pub fn settle(&mut self, pos: Vec3) {
        self.goal = pos;
        self.blocked = 0;
        if crate::nav::xz_dist(pos, self.spot) > crate::ai::HOLD_SHUFFLE {
            self.spot = pos;
        }
    }

    fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        // Tag 4: after `LostState`'s 3.
        eat(&[
            4,
            self.skip,
            self.strafe_side as u8,
            self.stance_mask,
            self.blocked,
        ]);
        eat(&self.commit_until.to_le_bytes());
        for v in [
            self.spot.x,
            self.spot.y,
            self.spot.z,
            self.goal.x,
            self.goal.y,
            self.goal.z,
            self.look.x,
            self.look.y,
            self.look.z,
        ] {
            eat(&v.to_bits().to_le_bytes());
        }
        eat(&self.until.to_le_bytes());
        eat(&self.shuffle_at.to_le_bytes());
        eat(&self.last_contact.to_le_bytes());
        eat(&self.roll_at.to_le_bytes());
    }
}

impl LostState {
    fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        // Tag 3: after `CmdState`'s 1 and `RideState`'s 2.
        eat(&[3, u8::from(self.confirmed), self.tries]);
        eat(&self.since.to_le_bytes());
        eat(&self.jump_after.to_le_bytes());
        eat(&self.window.to_le_bytes());
        eat(&self.ref_dist.to_bits().to_le_bytes());
        // Eaten only when set: every spell before card VMESH hashes as it did.
        if self.shut {
            eat(&[1]);
        }
        if self.pinned {
            eat(&[5]);
        }
        // Card RETW: eaten once a walk was looked for.
        if self.plans > 0 {
            eat(&[4, self.plans, self.path.len, self.path_at]);
            for p in &self.path.pts[..usize::from(self.path.len)] {
                for v in p.to_array() {
                    eat(&v.to_bits().to_le_bytes());
                }
            }
        }
    }
}

impl RideState {
    /// Called only after successful entry into a ground pilot seat.
    pub(crate) fn begin_pickup(
        &mut self,
        vehicle: crate::vehicle::VehicleHandle,
        tick: u64,
        kind: crate::vehicle::VehicleKind,
        ai: crate::vehicle::hover::HoverAi,
    ) {
        // Step 1c precedes ride(): suppress the boarding foot pad immediately.
        if kind == crate::vehicle::VehicleKind::Walker {
            self.sticks = Some([0.0; 2]);
        } else {
            self.hover = Some(HoverOut {
                sticks: [0.0; 3],
                ai,
            });
        }
        self.maintenance = None;
        self.pickup = Some(PickupWait {
            vehicle,
            seated: tick,
            started: None,
            released: None,
            complete: false,
            departed: false,
        });
    }

    fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        // Tag 2: `CmdState`'s stream starts with 1, so the two stay apart.
        eat(&[2, u8::from(self.use_req)]);
        if let Some(draw) = self.bail_draw {
            eat(&[11, draw, u8::from(self.bail_seen)]);
        }
        if let Some(w) = self.pickup {
            eat(&[12, u8::from(w.complete)]);
            // Keep the old stream for idle/BF2 tenures. Departure changes the future.
            if w.departed {
                eat(&[15]);
            }
            eat(&w.vehicle.index.to_le_bytes());
            eat(&w.vehicle.gen.to_le_bytes());
            eat(&w.seated.to_le_bytes());
            if let Some(t) = w.released {
                eat(&[14]);
                eat(&t.to_le_bytes());
            }
            match w.started {
                Some(t) => {
                    eat(&[1]);
                    eat(&t.to_le_bytes());
                }
                None => eat(&[0]),
            }
        }
        if self.ordered_call {
            eat(&[16]);
        }
        if let Some(owner) = self.called_board {
            eat(&[17]);
            eat(&owner.index.to_le_bytes());
            eat(&owner.gen.to_le_bytes());
        }
        if self.call_blocked {
            eat(&[18]);
        }
        match self.board {
            Some(v) => {
                eat(&v.index.to_le_bytes());
                eat(&v.gen.to_le_bytes());
            }
            None => eat(&u32::MAX.to_le_bytes()),
        }
        if self.board.is_some() {
            if let Some(c) = self.maintenance {
                eat(&[13]);
                eat(&c.owner.index.to_le_bytes());
                eat(&c.owner.gen.to_le_bytes());
                eat(&c.tick.to_le_bytes());
            }
        }
        eat(&self.since.to_le_bytes());
        eat(&self.block_until.to_le_bytes());
        // Only a walker's driver adds to the stream (card WAI).
        if let Some(s) = self.sticks {
            eat(&[6]);
            for x in s {
                eat(&x.to_bits().to_le_bytes());
            }
        }
        // ...and a hover's (card VDROP); its `ai` is hashed on the vehicle.
        if let Some(h) = self.hover {
            eat(&[7]);
            for x in h.sticks {
                eat(&x.to_bits().to_le_bytes());
            }
        }
        // Card TURR: only a bot a turret was closed to, or one in its seat.
        if let Some(v) = self.closed {
            eat(&[8]);
            eat(&v.index.to_le_bytes());
            eat(&v.gen.to_le_bytes());
            eat(&self.closed_until.to_le_bytes());
        }
        if let Some(m) = self.man {
            eat(&[9]);
            for t in [m.seated, m.hittable, m.purpose] {
                eat(&t.to_le_bytes());
            }
            if let Some(p) = m.look {
                for v in [p.x, p.y, p.z] {
                    eat(&v.to_bits().to_le_bytes());
                }
            }
            if let Some(p) = m.aim {
                eat(&[10]);
                for v in [p.x, p.y, p.z] {
                    eat(&v.to_bits().to_le_bytes());
                }
            }
        }
    }
}

impl Default for AiState {
    /// Neutral state with tap_gate enabled and flank NONE sentinels. The
    /// tracked-position error starts at the PS2's no-target floor (LEAD
    /// ADDENDUM 4a): a unit that has never tracked anything is in exactly the state of one that has been target-less for
    /// a while, and its first target does not begin with a perfect aim point.
    fn default() -> Self {
        Self {
            target: None,
            contacts: [None; crate::perception::CONTACTS],
            last_switch: 0,
            next_think: 0,
            target_cp: None,
            squad: None,
            cmd: None,
            ride: None,
            pending_board: None,
            lost: None,
            hold: None,
            fight: FightState::default(),
            tpos: TposState::default(),
            want_stance: 0,
            dodge_until: 0,
            unreachable_dwell: 0,
            route: None,
            steer: SteerPick::default(),
            slide: 0,
            walk: WalkAsk::default(),
            strafe_until: 0,
            strafe_side: 0,
            stall: None,
            mv: MoverState::default(),
            behaviour: Behaviour::None,
            aim_dir: Vec3::ZERO,
            aim_vel: Vec3::ZERO,
            err: ERR_NO_TARGET,
            err_pos: Vec3::ZERO,
            dispersion: None,
            burst: 0,
            pause_until: 0,
            // As an idle tick leaves it (BF2 `FUN_002baf70`).
            tap_gate: true,
            tap_until: 0,
            snipe_until: 0,
            tap_byte: 0,
            charge_until: 0,
            arm: ArmState::default(),
            snipe: SnipeBook::default(),
            role: RoleState::default(),
            flank: FlankState::default(),
        }
    }
}

impl AiState {
    pub fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(&self.target_cp.unwrap_or(u32::MAX).to_le_bytes());
        match self.target {
            Some(e) => {
                eat(&[1]);
                eat(&e.index.to_le_bytes());
                eat(&e.gen.to_le_bytes());
            }
            None => eat(&[0]),
        }
        // Every slot, `None` a sentinel byte and never skipped: what a unit
        // remembers is state, and two units with the same target but
        // different memories will fight differently three seconds from now.
        for slot in &self.contacts {
            match slot {
                Some(c) => {
                    eat(&[1]);
                    c.hash_into(eat);
                }
                None => eat(&[0]),
            }
        }
        eat(&self.last_switch.to_le_bytes());
        match &self.route {
            Some(r) => {
                eat(&[1]);
                r.hash_into(eat);
            }
            None => eat(&[0]),
        }
        eat(&self.steer.bytes());
        eat(&[self.slide as u8]);
        match self.squad {
            Some(k) => {
                eat(&[1]);
                eat(&k.to_le_bytes());
            }
            None => eat(&[0]),
        }
        eat(&self.unreachable_dwell.to_le_bytes());
        eat(&[self.burst, self.strafe_side as u8, self.want_stance]);
        eat(&self.pause_until.to_le_bytes());
        eat(&self.charge_until.to_le_bytes());
        eat(&self.dodge_until.to_le_bytes());
        eat(&self.strafe_until.to_le_bytes());
        match self.dispersion {
            Some(d) => {
                eat(&[1]);
                eat(&d.to_bits().to_le_bytes());
            }
            None => eat(&[0]),
        }
        for v in [
            self.aim_dir.x,
            self.aim_dir.y,
            self.aim_dir.z,
            self.aim_vel.x,
            self.aim_vel.y,
            self.aim_vel.z,
            self.err,
            self.err_pos.x,
            self.err_pos.y,
            self.err_pos.z,
        ] {
            eat(&v.to_bits().to_le_bytes());
        }
        // D6, and the same rule as `Unit::command` (brief addendum 3): a unit
        // with no order eats NOTHING here, not even a 0 byte, so the AI hash of
        // a command-free sim is byte-for-byte the pre-Phase-D one and the three
        // replay gate values do not move. `CmdState::hash_into`'s leading 1 tag
        // identifies this first optional extension; later extensions follow.
        if let Some(c) = self.cmd {
            c.hash_into(eat);
        }
        if let Some(r) = self.ride {
            r.hash_into(eat);
        }
        if let Some((owner, target)) = self.pending_board {
            eat(b"GBOARD");
            for v in [owner.index, owner.gen, target.index, target.gen] {
                eat(&v.to_le_bytes());
            }
        }
        if let Some(l) = self.lost {
            l.hash_into(eat);
        }
        if let Some(h) = self.hold {
            h.hash_into(eat);
        }
        if let Some(s) = self.stall {
            s.hash_into(eat);
        }
        if self.mv != MoverState::default() {
            self.mv.hash_into(eat);
        }
        if self.fight != FightState::default() {
            self.fight.hash_into(eat);
        }
        if self.tpos != TposState::default() {
            self.tpos.hash_into(eat);
        }
        // Card STTAP: the trigger state, eaten once a single-shot weapon has
        // moved it — like the optional states above, a bot that never held
        // one costs the hash nothing.
        if (
            self.tap_gate,
            self.tap_byte,
            self.tap_until,
            self.snipe_until,
        ) != (true, 0, 0, 0)
        {
            eat(&[u8::from(self.tap_gate), self.tap_byte as u8]);
            eat(&self.tap_until.to_le_bytes());
            eat(&self.snipe_until.to_le_bytes());
        }
        if self.arm != ArmState::default() {
            self.arm.hash_into(eat);
        }
        if self.snipe != SnipeBook::default() {
            self.snipe.hash_into(eat);
        }
        if self.role != RoleState::default() {
            self.role.hash_into(eat);
        }
        if self.flank != FlankState::default() {
            self.flank.hash_into(eat);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::DETOUR_MAX;

    #[test]
    fn the_route_is_part_of_the_ai_hash() {
        let hash = |st: &AiState| {
            let mut h: u64 = 0;
            let mut eat = |b: &[u8]| {
                for &x in b {
                    h = (h ^ x as u64).wrapping_mul(0x1_0000_01B3);
                }
            };
            st.hash_into(&mut eat);
            h
        };
        let mut a = AiState::default();
        let base = hash(&a);
        a.route = Some(Route::default());
        assert_ne!(base, hash(&a), "having a route is hashed");
        for mutate in [
            (|r: &mut Route| r.dest_hub = 3) as fn(&mut Route),
            |r: &mut Route| r.next_hub = 4,
            |r: &mut Route| r.via_arc = Some(1),
            |r: &mut Route| r.waypoint = Vec3::new(1.0, 2.0, 3.0),
            |r: &mut Route| r.ref_dist = 9.0,
            |r: &mut Route| r.from = Vec2::new(5.0, 0.0),
            |r: &mut Route| r.from = Vec2::new(0.0, 5.0),
            |r: &mut Route| r.stuck_ticks = 5,
            |r: &mut Route| r.stuck_run = 6,
            |r: &mut Route| r.sidestep = -7,
            |r: &mut Route| r.source = RouteSource::Bfs,
            |r: &mut Route| r.detour[0] = Vec2::new(1.0, 2.0),
            |r: &mut Route| r.detour[DETOUR_MAX - 1] = Vec2::new(3.0, 4.0),
            |r: &mut Route| r.detour_len = 2,
            |r: &mut Route| r.detour_at = 1,
        ] {
            let mut b = AiState {
                route: Some(Route::default()),
                ..AiState::default()
            };
            let before = hash(&b);
            mutate(b.route.as_mut().unwrap());
            assert_ne!(before, hash(&b), "every Route field is hashed");
        }
        // ... and the two AiState fields Phase A.1 added.
        for mutate in [
            (|st: &mut AiState| st.steer = SteerPick::Slide) as fn(&mut AiState),
            |st: &mut AiState| st.steer = SteerPick::Fan(-3),
            |st: &mut AiState| st.slide = -1,
            // ... and the two Phase B added.
            |st: &mut AiState| st.squad = Some(0),
            |st: &mut AiState| st.unreachable_dwell = 1,
            // ... and D13's.
            |st: &mut AiState| st.want_stance = 3,
            // ... and every field Phase C added (D9's aim tracker, D10's
            // tracked-position error, D11's fire cycle). One assertion PER
            // FIELD and per float COMPONENT: a field left out of `hash_into`
            // makes two sims that have already diverged compare equal, which
            // is the one failure the replay gates cannot see (the replays have
            // no AI, so nothing else pins these eleven fields).
            |st: &mut AiState| st.aim_dir.x = 1.0,
            |st: &mut AiState| st.aim_dir.y = 1.0,
            |st: &mut AiState| st.aim_dir.z = 1.0,
            |st: &mut AiState| st.aim_vel.x = 1.0,
            |st: &mut AiState| st.aim_vel.y = 1.0,
            |st: &mut AiState| st.aim_vel.z = 1.0,
            // `err` starts at the no-target floor, not at 0.
            |st: &mut AiState| st.err = ERR_NO_TARGET + 1.0,
            |st: &mut AiState| st.err_pos.x = 1.0,
            |st: &mut AiState| st.err_pos.y = 1.0,
            |st: &mut AiState| st.err_pos.z = 1.0,
            |st: &mut AiState| st.dispersion = Some(0.125),
            |st: &mut AiState| st.burst = 1,
            |st: &mut AiState| st.pause_until = 1,
            |st: &mut AiState| st.charge_until = 1,
            // ... and card STTAP's trigger state.
            |st: &mut AiState| st.tap_gate = false,
            |st: &mut AiState| st.tap_until = 1,
            |st: &mut AiState| st.snipe_until = 1,
            |st: &mut AiState| st.tap_byte = 1,
            // ... and card GRAI's weapon choice.
            |st: &mut AiState| st.arm.pick = Some(0),
            |st: &mut AiState| st.arm.in_range = true,
            |st: &mut AiState| st.arm.vehicle = true,
            |st: &mut AiState| st.arm.spare = true,
            |st: &mut AiState| st.arm.group_until = 1,
            |st: &mut AiState| st.arm.group_next = 1,
            |st: &mut AiState| st.arm.claim = Some((UnitHandle { index: 1, gen: 0 }, 1)),
            |st: &mut AiState| st.dodge_until = 1,
            |st: &mut AiState| st.strafe_until = 1,
            |st: &mut AiState| st.strafe_side = -1,
            |st: &mut AiState| st.fight.mode = FightState::STAND,
            |st: &mut AiState| st.fight.pause_until = 1,
            // ... and card TEAM's role.
            |st: &mut AiState| st.role.post = Some(0),
            |st: &mut AiState| st.role.since = 1,
            |st: &mut AiState| st.role.at = 1,
            |st: &mut AiState| st.role.look.x = 1.0,
            |st: &mut AiState| st.role.look.y = 1.0,
            |st: &mut AiState| st.role.look.z = 1.0,
        ] {
            let mut b = AiState::default();
            let before = hash(&b);
            mutate(&mut b);
            assert_ne!(before, hash(&b), "every AiState field is hashed");
        }
        // Cards BNDS, HOLD and RCYC: the optional states eat nothing when
        // `None` and every field when `Some`.
        let with = |f: fn(&mut AiState)| {
            let mut b = AiState {
                lost: Some(LostState::default()),
                hold: Some(HoldState::default()),
                stall: Some(StallState::default()),
                ..AiState::default()
            };
            f(&mut b);
            hash(&b)
        };
        let both = with(|_| {});
        assert_ne!(both, hash(&AiState::default()), "having them is hashed");
        for mutate in [
            (|st: &mut AiState| st.lost.as_mut().unwrap().since = 1) as fn(&mut AiState),
            |st: &mut AiState| st.lost.as_mut().unwrap().confirmed = true,
            |st: &mut AiState| st.lost.as_mut().unwrap().tries = 1,
            |st: &mut AiState| st.lost.as_mut().unwrap().jump_after = 1,
            |st: &mut AiState| st.lost.as_mut().unwrap().window = 1,
            |st: &mut AiState| st.lost.as_mut().unwrap().ref_dist = 1.0,
            |st: &mut AiState| st.lost.as_mut().unwrap().shut = true,
            |st: &mut AiState| st.lost.as_mut().unwrap().pinned = true,
            |st: &mut AiState| st.lost.as_mut().unwrap().plans = 1,
            |st: &mut AiState| {
                let l = st.lost.as_mut().unwrap();
                l.plans = 1;
                l.path.len = 1;
                l.path.pts[0].z = 1.0;
            },
            |st: &mut AiState| st.hold.as_mut().unwrap().spot.x = 1.0,
            |st: &mut AiState| st.hold.as_mut().unwrap().goal.z = 1.0,
            |st: &mut AiState| st.hold.as_mut().unwrap().until = 1,
            |st: &mut AiState| st.hold.as_mut().unwrap().skip = 1,
            |st: &mut AiState| st.hold.as_mut().unwrap().shuffle_at = 1,
            |st: &mut AiState| st.hold.as_mut().unwrap().roll_at = 1,
            |st: &mut AiState| st.hold.as_mut().unwrap().strafe_side = 1,
            |st: &mut AiState| st.hold.as_mut().unwrap().stance_mask = 1,
            |st: &mut AiState| st.hold.as_mut().unwrap().look.y = 1.0,
            |st: &mut AiState| st.hold.as_mut().unwrap().last_contact = 1,
            |st: &mut AiState| st.stall.as_mut().unwrap().at.x = 1.0,
            |st: &mut AiState| st.stall.as_mut().unwrap().at.y = 1.0,
            |st: &mut AiState| st.stall.as_mut().unwrap().at.z = 1.0,
            |st: &mut AiState| st.stall.as_mut().unwrap().lo.x = 1.0,
            |st: &mut AiState| st.stall.as_mut().unwrap().lo.y = 1.0,
            |st: &mut AiState| st.stall.as_mut().unwrap().lo.z = 1.0,
            |st: &mut AiState| st.stall.as_mut().unwrap().hi.x = 1.0,
            |st: &mut AiState| st.stall.as_mut().unwrap().hi.y = 1.0,
            |st: &mut AiState| st.stall.as_mut().unwrap().hi.z = 1.0,
            |st: &mut AiState| st.stall.as_mut().unwrap().ticks = 1,
            |st: &mut AiState| st.stall.as_mut().unwrap().pin.x = 1.0,
            |st: &mut AiState| st.stall.as_mut().unwrap().pin.y = 1.0,
            |st: &mut AiState| st.stall.as_mut().unwrap().pin.z = 1.0,
            |st: &mut AiState| st.stall.as_mut().unwrap().fails = 1,
            |st: &mut AiState| st.stall.as_mut().unwrap().walked = Some(Vec3::ZERO),
            |st: &mut AiState| st.stall = None,
        ] {
            assert_ne!(
                both,
                with(mutate),
                "every LostState / HoldState / StallState field is hashed"
            );
        }
        // D10: the dispersion is hashed by VALUE, and `Some(0.0)` — a unit
        // whose distance table answers "no spread at all" — is not `None`.
        let di = |d: Option<f32>| {
            hash(&AiState {
                dispersion: d,
                ..AiState::default()
            })
        };
        assert_ne!(di(None), di(Some(0.0)));
        assert_ne!(di(Some(0.0)), di(Some(0.125)));
        // The four D11 tick stamps are hashed in DISTINCT positions: setting
        // any two of them to the same tick must not collide, which is what a
        // copy-pasted `eat(&self.pause_until…)` in place of one of the others
        // would do.
        let stamp: [fn(&mut AiState); 4] = [
            |st: &mut AiState| st.pause_until = 7,
            |st: &mut AiState| st.charge_until = 7,
            |st: &mut AiState| st.dodge_until = 7,
            |st: &mut AiState| st.strafe_until = 7,
        ];
        let stamped: Vec<u64> = stamp
            .iter()
            .map(|m| {
                let mut b = AiState::default();
                m(&mut b);
                hash(&b)
            })
            .collect();
        for i in 0..stamped.len() {
            for j in i + 1..stamped.len() {
                assert_ne!(
                    stamped[i], stamped[j],
                    "tick stamps {i} and {j} hash to the same word"
                );
            }
        }
        // D6: every `CmdState` field one at a time, and — the property the
        // replay gates rest on — an AiState with NO order eats NOTHING for it.
        // The no-order stream is a strict PREFIX of the ordered one, shorter by
        // exactly the 23 bytes `CmdState::hash_into` writes, which is only true
        // because all later optional extensions are absent in this fixture.
        let bytes = |st: &AiState| {
            let mut out: Vec<u8> = Vec::new();
            st.hash_into(&mut |b| out.extend_from_slice(b));
            out
        };
        let ordered = bytes(&AiState {
            cmd: Some(CmdState::default()),
            ..AiState::default()
        });
        let bare = bytes(&AiState::default());
        assert_eq!(ordered.len(), bare.len() + 1 + 12 + 2 + 8);
        assert_eq!(ordered[..bare.len()], bare[..], "the order is hashed LAST");
        assert_eq!(ordered[bare.len()], 1, "and behind a presence tag");
        let with = |c: CmdState| {
            hash(&AiState {
                cmd: Some(c),
                ..AiState::default()
            })
        };
        assert_ne!(hash(&AiState::default()), with(CmdState::default()));
        for mutate in [
            (|c: &mut CmdState| c.goal.x = 1.0) as fn(&mut CmdState),
            |c: &mut CmdState| c.goal.y = 1.0,
            |c: &mut CmdState| c.goal.z = 1.0,
            |c: &mut CmdState| c.far_ticks = 1,
            |c: &mut CmdState| c.roll = 1,
        ] {
            let mut c = CmdState::default();
            let before = with(c);
            mutate(&mut c);
            assert_ne!(before, with(c), "every CmdState field is hashed");
        }
        // Same argument for the three float TRIPLES: `aim_dir`, `aim_vel` and
        // `err_pos` are eaten one after another, so a duplicated component
        // would make two of them interchangeable.
        let triple: [fn(&mut AiState); 3] = [
            |st: &mut AiState| st.aim_dir = Vec3::new(1.0, 2.0, 3.0),
            |st: &mut AiState| st.aim_vel = Vec3::new(1.0, 2.0, 3.0),
            |st: &mut AiState| st.err_pos = Vec3::new(1.0, 2.0, 3.0),
        ];
        let tripled: Vec<u64> = triple
            .iter()
            .map(|m| {
                let mut b = AiState::default();
                m(&mut b);
                hash(&b)
            })
            .collect();
        for i in 0..tripled.len() {
            for j in i + 1..tripled.len() {
                assert_ne!(
                    tripled[i], tripled[j],
                    "float triples {i} and {j} hash to the same word"
                );
            }
        }
        // The fan's step is part of the hash, not just the tag.
        let f = |k| {
            hash(&AiState {
                steer: SteerPick::Fan(k),
                ..AiState::default()
            })
        };
        assert_ne!(f(1), f(2));
        assert_ne!(f(1), f(-1));
        // The squad id is hashed by VALUE, and `Some(0)` is not `None`.
        let sq = |k: Option<u32>| {
            hash(&AiState {
                squad: k,
                ..AiState::default()
            })
        };
        assert_ne!(sq(None), sq(Some(0)));
        assert_ne!(sq(Some(0)), sq(Some(1)));
    }
    #[test]
    fn every_pickup_field_changes_the_hash() {
        let mut r = RideState::default();
        r.begin_pickup(
            crate::vehicle::VehicleHandle { index: 2, gen: 3 },
            10,
            crate::vehicle::VehicleKind::Walker,
            Default::default(),
        );
        let bytes = |r: &RideState| {
            let mut out = vec![];
            r.hash_into(&mut |b| out.extend_from_slice(b));
            out
        };
        let base = bytes(&r);
        for mutate in [
            (|w: &mut PickupWait| w.vehicle.index += 1) as fn(&mut PickupWait),
            |w| w.vehicle.gen += 1,
            |w| w.seated += 1,
            |w| w.started = Some(12),
            |w| w.released = Some(12),
            |w| w.complete = true,
            |w| w.departed = true,
        ] {
            let mut other = r;
            mutate(other.pickup.as_mut().unwrap());
            assert_ne!(base, bytes(&other));
        }
        let mut a = r;
        a.pickup.as_mut().unwrap().started = Some(12);
        let mut b = a;
        b.pickup.as_mut().unwrap().started = Some(13);
        assert_ne!(bytes(&a), bytes(&b));
        a.pickup.as_mut().unwrap().released = Some(12);
        b = a;
        b.pickup.as_mut().unwrap().released = Some(13);
        assert_ne!(bytes(&a), bytes(&b));
        b.pickup = None;
        assert_ne!(base, bytes(&b));
    }
    #[test]
    fn cwait_each_maintenance_field_is_hashed_only_with_a_claim() {
        let bytes = |r: &RideState| {
            let mut out = vec![];
            r.hash_into(&mut |b| out.extend_from_slice(b));
            out
        };
        let mut r = RideState {
            board: Some(crate::vehicle::VehicleHandle { index: 2, gen: 3 }),
            maintenance: Some(BoardMaintenance {
                owner: UnitHandle { index: 4, gen: 5 },
                tick: 6,
            }),
            ..Default::default()
        };
        let baseline = bytes(&r);
        for change in 0..4 {
            let mut other = r;
            match change {
                0 => other.maintenance.as_mut().unwrap().owner.index += 1,
                1 => other.maintenance.as_mut().unwrap().owner.gen += 1,
                2 => other.maintenance.as_mut().unwrap().tick += 1,
                _ => other.maintenance = None,
            }
            assert_ne!(baseline, bytes(&other), "omitted field {change}");
        }
        r.board = None;
        let absent = bytes(&r);
        r.maintenance = None;
        assert_eq!(absent, bytes(&r), "inactive maintenance adds no bytes");
        assert_eq!(bytes(&RideState::default()), bytes(&r));
    }
}
