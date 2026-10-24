//! Phase B: what every AI unit is FOR — the PS2's Conquest goal
//! (`analysis/re-conquest-goal.md`, audited PARTLY 2026-10-02 - its banner and
//! section 10; its section 8 is this module's build list). One [`TeamPlan`] per team holds each unit's target —
//! attack that post, hold this one, or go hunt — and `core::ai`'s follower
//! obeys it instead of Phase A's "walk at the nearest post that is not mine".
//!
//! The PS2 has NO team planner. Each bot draws ONE post that is not its
//! team's, at random by the post's authored value, and keeps it; nothing in
//! the draw looks at distance, enemies or crowding (doc sections 2, 3, 6).
//! [`draw_post`] is that draw; [`plan`] is the goal's 1 s tick, which draws
//! only for a unit that has no target or whose target is done:
//!
//! | a unit draws when | BF2 | BF1 |
//! |---|---|---|
//! | it has no target (a new life) | yes | yes |
//! | its target is its own team's | when `value_def` is 0 | at once (and when an own target is lost) |
//! | 5 s inside an own target's capture region | yes | yes, excluding that post |
//! | no route to the target | yes | PORT GUARD |
//! | a post lost to neutral | — | up to five units within 60 m retake it |
//!
//! OURS, each PROVISIONAL:
//!
//! - the 5 s clock restarts when the unit leaves the region or has a fight
//!   target: the PS2's capturers stay until an order interrupts the capture
//!   order and leave 5 s after it resumes, and the port's fight is not an
//!   order (doc section 8, item 2c);
//! - "the team can spawn at it" is read as "the team owns it";
//! - [`Objective::Hunt`] when a unit can route to no post that is left to
//!   take, and instead
//!   of holding a post once a side is down to its last reinforcements
//!   ([`crate::ai::HOLD_ENDGAME`]) — the port's endgame (card STEND); the PS2
//!   draws an own post there;
//! - the reachability drop on a BF1 map (BF1's draw has none), and the
//!   [`DWELL_GIVE_UP`] report, which re-draws without the post given up on;
//! - card DEFS, a NAMED DEVIATION (the maintainer, INBOX 2026-10-12: the PS2's
//!   capturers all hold the post, and the port's all stood on its one Cover
//!   hint): a post keeps at most a quarter of the team's living units as
//!   defenders, the nearest ([`DEFEND_SHARE`]); one once it has been quiet
//!   for 5 s and none after 20 s ([`defend_cap`], [`post_threatened`]); a
//!   defender over the cap draws again at once; and an authored hint is ONE
//!   defender's ([`TeamPlan::spots`], [`pick_spot`]);
//! - a vehicle's ban flags are not ported; the PS2's driver-shares-its-draw
//!   rule is: a passenger takes its AI driver's target.
//!
//! Properties that are not negotiable, because the replay spine rests on them:
//!
//! - it is SIM STATE, hashed with everything else ([`crate::sim::Sim::hash`]);
//! - it runs at a fixed cadence — team `t` plans when `tick` reaches its
//!   `next_plan_tick`, staggered so the two teams never plan on the same tick;
//! - teams in index order, units in index order, posts in index order, and the
//!   only randomness is a per-unit HASH ([`crate::ai::route_rand`]'s shape) —
//!   never the shared LCG, which `think` may not touch either.

use crate::collision::STEP_HEIGHT;
use crate::nav::Mover;
use crate::sim::Sim;
use crate::soldier::Stance;
use glam::Vec3;
use std::collections::BTreeSet;

/// Ticks between plans for one team (30 = 1 s): the PS2 goal manager's tick
/// (BF2 `FUN_002e7cf8`). Team `t` plans on ticks `t, t + 30, t + 60, ...`, so
/// the two teams never plan on the same tick and the pass costs at most one
/// team's worth of work per tick.
pub const PLAN_PERIOD: u64 = 30;
/// Plans (seconds) a unit spends inside its target's capture region before the
/// at-post helper's one test: 5.0 s (BF2 `FUN_002d5c30`, BF1 `FUN_00255fd0`).
pub const AT_POST_PLANS: u8 = 5;
/// [`TeamPlan::at_post`] once that test has run for this stay.
pub const AT_POST_TESTED: u8 = u8::MAX;
/// BF1's retake call (`FUN_00262090`): a post lost to neutral takes up to
/// this many AI units of the team ...
pub const RETAKE_UNITS: u32 = 5;
/// ... within this many metres of it.
pub const RETAKE_RADIUS: f32 = 60.0;
/// Defenders inside their post's capture volume take a Cover/Snipe hint within
/// this many metres OF THE POST. PROVISIONAL.
pub const HINT_RADIUS: f32 = 25.0;
/// Only a hint on the CENTRE's own level is offered ([`hint_on_level`]):
/// `false` restores the pre-HDL rule, where a hint 8 m up a wall was as good
/// as one on the floor (POL1's cp6con defenders, queue STK). PROVISIONAL: a
/// port routing rule — how the PS2 picks a defender's hint is not in
/// analysis/.
pub const HINT_SAME_LEVEL: bool = true;
/// Units on the same objective within this of each other are one squad.
/// PROVISIONAL.
pub const SQUAD_RADIUS: f32 = 20.0;
/// A unit within this of the post of an objective it has not completed is
/// DWELLING on it (`unreachable_dwell_max`). PROVISIONAL.
pub const DWELL_RADIUS: f32 = 10.0;
/// After this many ticks (10 s) of dwelling, the unit REPORTS the objective as
/// undoable and draws again without it. PROVISIONAL, ours.
///
/// MEASURED, the shape the pin takes: BF1 KAS2 units 5-15 spent 39,000-56,000
/// ticks directly UNDER cp1, 22 m below a post the graph says is four hops
/// away, and BF1 KAM1's units the same under cp1 and cp5. The route is fine;
/// the last leg is a cliff.
pub const DWELL_GIVE_UP: u16 = 300;

/// Card DEFS, rule 2 (OURS, a named deviation: the maintainer, INBOX 2026-10-12): at
/// most this share of the team's living units defends one post -
/// `max(1, round(share x alive))`, the nearest to the post first; a defender
/// over the cap draws again at once with the post excluded. The PS2 has no
/// cap: every unit that targeted a post holds it once it is taken (5 of 17
/// on one Cover hint on BF2 Kamino, 4-8 a metre apart on BF2 Hoth). This
/// and the six constants of rules 1, 3 and 4 below are PROVISIONAL.
pub const DEFEND_SHARE: f32 = 0.25;
/// Rule 3: a post is QUIET when it has had no threat ([`post_threatened`])
/// and no capture for this many ticks (5 s); the cap of a quiet post is
/// [`DEFEND_QUIET_CAP`], and the share's again from the plan a threat is
/// seen. OURS.
pub const DEFEND_QUIET_FALL: u64 = 150;
pub const DEFEND_QUIET_CAP: usize = 1;
/// Rule 4: after this many ticks (20 s) without a threat or a capture the
/// cap is 0 - the last defender draws again too, wherever it stands (the
/// PS2's 5 s test runs only inside the capture region, and a Cover hint is
/// mostly outside it). OURS.
pub const DEFEND_QUIET_LEAVE: u64 = 600;
/// Rule 3's distance: an enemy within this fraction of the distance between
/// the map's two farthest posts threatens a post ([`threat_radius`]), and
/// never less than [`THREAT_RADIUS_MIN`]. OURS.
pub const THREAT_FRAC: f32 = 0.2;
pub const THREAT_RADIUS_MIN: f32 = 30.0;
/// Rule 1: two defenders' spots are at least this far apart
/// ([`crate::ai::HOLD_SPACING`]'s value, the hub hold's): one defender a
/// hint, and a defender with no hint left walks to its own point of the
/// capture region. OURS.
pub const DEFEND_SPACING: f32 = 2.0;

/// `Hint::kind` values a defender will stand at (0 Snipe, 2 Cover — the
/// authored kinds `core::nav::Hint` documents).
pub const HINT_SNIPE: u8 = 0;
pub const HINT_COVER: u8 = 2;

/// What a unit is FOR.
///
/// `Escort(UnitHandle)` (Phase D, squad commands) and `Board(VehicleHandle)`
/// (Phase E, vehicles) are the two the design leaves the enum open for; they
/// are NOT built here, because nothing in the sim can carry them out yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Objective {
    /// Take command post `cp` (an index into `Sim::cps`).
    Attack(u32),
    /// Hold command post `cp`, which my team owns: the same kept target as
    /// `Attack(cp)`, relabelled by [`plan`] while the team owns the post.
    Defend(u32),
    /// Nothing is left to take: go at the nearest enemy.
    Hunt,
}

impl Objective {
    /// The five bytes that go into the hash: a tag and the post index.
    pub fn bytes(self) -> [u8; 5] {
        let (tag, cp) = match self {
            Objective::Attack(c) => (1u8, c),
            Objective::Defend(c) => (2, c),
            Objective::Hunt => (3, 0),
        };
        let b = cp.to_le_bytes();
        [tag, b[0], b[1], b[2], b[3]]
    }

    /// A single number naming this objective — the SQUAD id.
    pub fn key(self) -> u32 {
        match self {
            Objective::Attack(c) => c,
            Objective::Defend(c) => 0x4000_0000 | c,
            Objective::Hunt => 0x8000_0000,
        }
    }

    pub fn label(self) -> String {
        match self {
            Objective::Attack(c) => format!("Attack cp{c}"),
            Objective::Defend(c) => format!("Defend cp{c}"),
            Objective::Hunt => "Hunt".to_owned(),
        }
    }

    /// The post this objective is about, if it is about one.
    pub fn cp(self) -> Option<u32> {
        match self {
            Objective::Attack(c) | Objective::Defend(c) => Some(c),
            Objective::Hunt => None,
        }
    }
}

/// One team's plan: what every unit SLOT is for, and when to think again.
///
/// `assignment` is indexed by unit slot exactly like `Sim::units`, so a slot
/// that is dead, on the other team, or not AI holds `None` — as does a unit
/// that spawned since the last plan, which is what makes it fall back to the
/// Phase A rule until the goal's next tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TeamPlan {
    pub assignment: Vec<Option<Objective>>,
    /// Per slot, the generation of the unit `assignment` was made for: a
    /// respawned slot holds no target. A slot past the end counts as current.
    pub gens: Vec<u32>,
    /// Per slot, the plans it has spent inside its target's capture region
    /// with nobody to fight, up to [`AT_POST_PLANS`]; then [`AT_POST_TESTED`].
    pub at_post: Vec<u8>,
    /// Every post's owner at the last plan: BF1's owner-change events.
    pub owners: Vec<Option<u8>>,
    /// Card DEFS: per slot, a defender's own hint and its allowed-stance
    /// mask; no two defenders of a team within [`DEFEND_SPACING`]. `None`: no
    /// defender, or no hint left for it.
    pub spots: Vec<Option<(Vec3, u8)>>,
    /// Card DEFS: per post, the last tick it was threatened
    /// ([`post_threatened`]) or became this team's.
    pub threat: Vec<u64>,
    pub next_plan_tick: u64,
}

impl TeamPlan {
    /// The plan for a fresh team: team `t` first plans on tick `t`.
    pub fn new(team: u8) -> TeamPlan {
        TeamPlan {
            next_plan_tick: u64::from(team),
            ..TeamPlan::default()
        }
    }

    pub fn of(&self, unit_index: u32) -> Option<Objective> {
        self.assignment.get(unit_index as usize).copied().flatten()
    }

    /// Card DEFS: the spot unit `unit_index` defends from, if it has one.
    pub fn spot(&self, unit_index: u32) -> Option<(Vec3, u8)> {
        self.spots.get(unit_index as usize).copied().flatten()
    }

    pub fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(&self.next_plan_tick.to_le_bytes());
        eat(&(self.assignment.len() as u32).to_le_bytes());
        for a in &self.assignment {
            // Length-prefixed, and `None` is a sentinel byte rather than a
            // skip: "unit 3 has no objective" and "there is no unit 3" are
            // different states and must not hash alike.
            match a {
                Some(o) => {
                    eat(&[1]);
                    eat(&o.bytes());
                }
                None => eat(&[0]),
            }
        }
        eat(&(self.gens.len() as u32).to_le_bytes());
        for g in &self.gens {
            eat(&g.to_le_bytes());
        }
        eat(&(self.at_post.len() as u32).to_le_bytes());
        eat(&self.at_post);
        eat(&(self.owners.len() as u32).to_le_bytes());
        for o in &self.owners {
            eat(&[o.unwrap_or(u8::MAX)]);
        }
        eat(&(self.spots.len() as u32).to_le_bytes());
        for s in &self.spots {
            match s {
                Some((p, m)) => {
                    eat(&[1, *m]);
                    for v in p.to_array() {
                        eat(&v.to_bits().to_le_bytes());
                    }
                }
                None => eat(&[0]),
            }
        }
        eat(&(self.threat.len() as u32).to_le_bytes());
        for t in &self.threat {
            eat(&t.to_le_bytes());
        }
    }
}

/// Which game's Conquest rules a map runs (doc sections 2-3 against 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalRules {
    Bf1,
    Bf2,
}

impl GoalRules {
    /// The sim's game: [`Sim::ground_rules`] is the flag every sim builder
    /// sets from the manifest's `source_game`.
    pub fn of(sim: &Sim) -> GoalRules {
        match sim.ground_rules {
            crate::soldier::GroundRules::Bf1 => GoalRules::Bf1,
            crate::soldier::GroundRules::Bf2 => GoalRules::Bf2,
        }
    }
}

/// One post as [`draw_post`] sees it, for the drawing unit's team.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawPost {
    /// The unit's team owns it.
    pub friendly: bool,
    /// Nobody owns it.
    pub neutral: bool,
    /// `CpDef::value_atk` / `value_def` for the unit's team.
    pub atk: i32,
    pub def: i32,
    /// The unit can route to it and it is not excluded.
    pub open: bool,
}

/// The Conquest goal's target draw (BF2 `FUN_002e8318`, BF1 `FUN_002618b8`):
/// the post index, `None` when there is nothing to draw. `r` is one uniform
/// number in `[0, 1)`.
///
/// Candidates are the open posts NOT friendly to the team. BF2 drops a value
/// `<= 0` and weighs `Value_ATK` (hostile) or the mean of ATK and DEF
/// (neutral); BF1 weighs `Value_ATK` for the PS2's team 1 (port team 0) and
/// `Value_DEF` for its team 2, whoever owns the post. The draw is the first
/// candidate, in post order, whose running weight passes `r x` the sum. With
/// no candidate BF2 draws among its own posts by `Value_DEF`; then both games
/// draw uniformly over all (open) posts — BF2 only when there are two.
pub fn draw_post(rules: GoalRules, team: u8, posts: &[DrawPost], r: f32) -> Option<u32> {
    let weight = |p: &DrawPost| -> f32 {
        let w = match rules {
            GoalRules::Bf2 if p.friendly => p.def as f32,
            GoalRules::Bf2 if p.neutral => (p.atk + p.def) as f32 * 0.5,
            GoalRules::Bf2 => p.atk as f32,
            GoalRules::Bf1 if team == 0 => p.atk as f32,
            GoalRules::Bf1 => p.def as f32,
        };
        w.max(0.0)
    };
    let pass = |friendly: bool| -> Option<u32> {
        let ok = |p: &DrawPost| p.open && p.friendly == friendly && weight(p) > 0.0;
        let sum: f32 = posts.iter().filter(|p| ok(p)).map(weight).sum();
        let (mut run, mut last) = (0.0f32, None);
        for (i, p) in posts.iter().enumerate().filter(|(_, p)| ok(p)) {
            run += weight(p);
            last = Some(i as u32);
            if r * sum < run {
                break;
            }
        }
        last
    };
    if let Some(c) = pass(false) {
        return Some(c);
    }
    if rules == GoalRules::Bf2 {
        if let Some(c) = pass(true) {
            return Some(c);
        }
        if posts.len() < 2 {
            return None;
        }
    }
    let n = posts.iter().filter(|p| p.open).count();
    let k = ((r * n as f32) as usize).min(n.saturating_sub(1));
    posts
        .iter()
        .enumerate()
        .filter(|(_, p)| p.open)
        .nth(k)
        .map(|(i, _)| i as u32)
}

/// The draw's uniform number in `[0, 1)` for one unit at one plan: a hash,
/// not the shared LCG, so the plan is a pure function of sim state
/// ([`crate::ai::unit_rand`]'s FNV shape; the top 24 bits, so 1.0 is never
/// reached).
fn roll(unit_index: u32, tick: u64, gen: u32) -> f32 {
    let mut h = crate::ai::route_rand(unit_index, tick, gen);
    // FNV's last multiply leaves the low byte's change in the low bits only.
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h >> 8) as f32 / 16_777_216.0
}

/// Everything one plan needs to know about one command post, gathered once so
/// the per-unit loop is arithmetic only.
struct PostInfo {
    /// Where this post's hop-distance row starts in `plan`'s flat field
    /// (`hub_count` entries, `u16::MAX` = never); `None` when the post has no
    /// hub or the map has no graph.
    dist: Option<usize>,
    friendly: bool,
    neutral: bool,
    atk: i32,
    def: i32,
}

/// The post player `slot` stands NEAREST in XZ, if that player is alive.
///
/// Brief D5(b) keeps a dispersed bot off "the player's current objective", and
/// a player has no plan assignment to read — so the post they are standing
/// nearest is the LEAD'S PROVISIONAL reading of "current" (report it as the
/// reading taken; a radius or the post they last captured are the alternatives
/// nobody has decided between). Ties keep the LOWER post index, the authored
/// order, so no float equality decides anything; a map with no posts, a slot
/// with no player and a DEAD player all answer `None` — no exclusion at all.
fn issuer_post(sim: &Sim, slot: u8) -> Option<u32> {
    let h = sim.players.get(usize::from(slot)).copied().flatten()?;
    let from = sim.unit(h)?.soldier.pos;
    let mut best: Option<(f32, u32)> = None;
    for (ci, c) in sim.cps.iter().enumerate() {
        let d = crate::nav::xz_dist(c.def.pos, from);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, ci as u32));
        }
    }
    best.map(|(_, ci)| ci)
}

/// The Conquest goal's tick for team `team`. Called by
/// [`crate::sim::Sim::step`] phase 1, BEFORE the think loop, so the units
/// think against the plan made this same tick.
pub fn plan(sim: &mut Sim, team: u8) {
    let t = usize::from(team).min(1);
    let rules = GoalRules::of(sim);
    sim.commander[t].next_plan_tick = sim.tick + PLAN_PERIOD;
    sim.nav_stats.plans += 1;
    let old = std::mem::take(&mut sim.commander[t]);

    // --- the team's units, in index order, with the hub each starts from ---
    let mut units: Vec<(usize, u32, Vec3, Option<u16>)> = Vec::new();
    for i in 0..sim.units.len() {
        let u = &sim.units[i];
        // A Follow or Hold bot is the PLAYER'S while the order stands (brief
        // D5), so it is not in the pool at all: `assignment[i]` stays `None`,
        // the squad bookkeeping below skips it, and no plan can take the order
        // off it. A DISPERSE bot is in the pool — handing them back is what
        // Disperse IS — and comes back with the spread rule further down.
        if !u.alive
            || u.team != team
            || u.controller != crate::unit::Controller::Ai
            || u.command.is_some_and(|c| !c.in_pool())
        {
            continue;
        }
        let (pos, gen) = (u.soldier.pos, u.gen);
        let hub = sim
            .nav
            .as_ref()
            .and_then(|g| crate::ai::start_hub(sim, g, sim.handle(i)));
        units.push((i, gen, pos, hub));
    }

    // --- the posts ---
    let mover = Mover::SOLDIER;
    // One jetless hop-distance field for every post, end to end, with one
    // scratch queue for every BFS: the commander's shared infantry policy.
    let hub_count = sim.nav.as_ref().map_or(0, |g| g.hubs.len());
    let mut hop_field: Vec<u16> = Vec::with_capacity(hub_count * sim.cps.len());
    let mut bfs_queue: Vec<u16> = Vec::new();
    let mut posts: Vec<PostInfo> = Vec::with_capacity(sim.cps.len());
    for c in &sim.cps {
        let dist = match (sim.nav.as_ref(), c.nav_hub) {
            (Some(g), Some(h)) => {
                let base = hop_field.len();
                g.hop_distances_into(h, mover, &mut hop_field, &mut bfs_queue);
                Some(base)
            }
            _ => None,
        };
        posts.push(PostInfo {
            dist,
            friendly: c.owner == Some(team),
            neutral: c.owner.is_none(),
            atk: c.def.value_atk[t],
            def: c.def.value_def[t],
        });
    }
    // Unreachable from where the unit stands. No graph (or no hub for the
    // post, or for the unit): every post is open and the follower walks
    // straight at it, exactly as before Phase A.
    let reach = |hub: Option<u16>, p: &PostInfo| -> bool {
        match (hub, p.dist) {
            (Some(h), Some(base)) if usize::from(h) < hub_count => {
                hop_field[base + usize::from(h)] != u16::MAX
            }
            _ => true,
        }
    };
    // And the brain's endgame, a side down to its last reinforcements: only
    // kills end the round, so nobody holds a post — a unit whose target is
    // its team's draws again, and hunts when the draw is an own post
    // (MEASURED: BF2 POL1 seed 3 stood 39,227 ticks at 1 v 1 without it).
    let endgame = sim
        .battle
        .teams
        .iter()
        .any(|t| t.tickets() <= crate::ai::HOLD_ENDGAME);

    // --- card DEFS: each own post's cap of defenders, and who is over it ---
    let radius = threat_radius(sim);
    let alive = sim.alive_units().filter(|(_, u)| u.team == team).count();
    let mut threat: Vec<u64> = Vec::with_capacity(posts.len());
    let mut cap: Vec<usize> = Vec::with_capacity(posts.len());
    for (ci, p) in posts.iter().enumerate() {
        let taken = p.friendly && old.owners.get(ci).is_some_and(|o| *o != Some(team));
        let last = if taken || (p.friendly && post_threatened(sim, team, ci as u32, radius)) {
            sim.tick
        } else {
            old.threat.get(ci).copied().unwrap_or(0)
        };
        threat.push(last);
        cap.push(defend_cap(alive, sim.tick.saturating_sub(last)));
    }
    // The defenders a post keeps are the nearest to it (ties: the lower
    // slot); `held` counts them, and a draw that lands on a full post is
    // made again without it.
    let mut over: Vec<bool> = vec![false; sim.units.len()];
    let mut held: Vec<usize> = vec![0; posts.len()];
    for (ci, p) in posts.iter().enumerate() {
        if !p.friendly {
            continue;
        }
        let at = sim.cps[ci].def.pos;
        let mut holders: Vec<(f32, usize)> = units
            .iter()
            .filter(|&&(i, gen, _, _)| {
                old.gens.get(i).is_none_or(|g| *g == gen)
                    && old.of(i as u32).and_then(Objective::cp) == Some(ci as u32)
            })
            .map(|&(i, _, pos, _)| (crate::nav::xz_dist(pos, at), i))
            .collect();
        holders.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for &(_, i) in holders.iter().skip(cap[ci]) {
            over[i] = true;
        }
        held[ci] = holders.len().min(cap[ci]);
    }

    let mut assignment: Vec<Option<Objective>> = vec![None; sim.units.len()];
    let mut gens: Vec<u32> = vec![0; sim.units.len()];
    let mut at_post: Vec<u8> = vec![0; sim.units.len()];
    let mut changes = 0u64;
    // Hoisted: refilled per draw instead of allocated per draw.
    let mut offered: Vec<DrawPost> = Vec::with_capacity(posts.len());
    // The Disperse rule (brief D5), one entry per ISSUER that has a dispersed
    // unit in this plan: the post excluded for its bots (b) and the posts its
    // bots hold (c). Built lazily and left EMPTY by a plan with no dispersed
    // unit, which is every plan of a command-free round.
    let mut disperse: Vec<(u8, Option<u32>, BTreeSet<u32>)> = Vec::new();
    for &(i, gen, pos, hub) in &units {
        let (command, fighting, dwell) = {
            let u = &sim.units[i];
            (u.command, u.ai.target.is_some(), u.ai.unreachable_dwell)
        };
        // Card MOVR: the post this unit's walk gave up (`behave::recover`),
        // read once.
        let gave_up = std::mem::take(&mut sim.units[i].ai.mv.give_up).checked_sub(1);
        gens[i] = gen;
        let kept = old
            .of(i as u32)
            .filter(|_| old.gens.get(i).is_none_or(|g| *g == gen));
        // The order this unit is under, if it is a DISPERSE still in force.
        // Follow and Hold never reach here (they are out of the pool above),
        // and `Sim::step` phase 0b has already dropped every EXPIRED Disperse —
        // the `tick < until` test is what makes a directly-called `plan` agree
        // with a stepped one.
        let dispersing = command.filter(|c| match c.kind {
            crate::squad::CommandKind::Disperse(until) => sim.tick < until,
            _ => false,
        });
        let spread =
            dispersing.map(
                |c| match disperse.iter().position(|(s, _, _)| *s == c.issuer) {
                    Some(k) => k,
                    None => {
                        disperse.push((c.issuer, issuer_post(sim, c.issuer), BTreeSet::new()));
                        disperse.len() - 1
                    }
                },
            );
        // (a) The FIRST plan after the order draws afresh: the point of
        // Disperse is a new spread, not the target it already had.
        let fresh_order = dispersing.is_some_and(|c| c.issued_tick + PLAN_PERIOD > sim.tick);

        // --- does it draw? ---
        let target = kept
            .and_then(Objective::cp)
            .filter(|&c| (c as usize) < posts.len());
        let mut clock = 0u8;
        let mut exclude: Option<u32> = None;
        let draw = match target {
            None => true,
            Some(_) if fresh_order => true,
            Some(c) => {
                let p = &posts[c as usize];
                // The at-post helper's 5 s test, once per stay.
                let inside = sim.cps[c as usize].def.capture.contains(pos) && !fighting;
                clock = match old.at_post.get(i).copied().unwrap_or(0) {
                    _ if !inside => 0,
                    AT_POST_TESTED => AT_POST_TESTED,
                    n => n + 1,
                };
                let tested = clock != AT_POST_TESTED && clock >= AT_POST_PLANS;
                if tested {
                    clock = AT_POST_TESTED;
                }
                if dwell >= DWELL_GIVE_UP || gave_up == Some(c) {
                    // It has stood beside the post for 10 s without arriving,
                    // or its walk there has failed twice (card MOVR).
                    exclude = Some(c);
                    true
                } else if !reach(hub, p) || (endgame && p.friendly) {
                    true
                } else if p.friendly && over[i] {
                    // Card DEFS: over the post's cap of defenders.
                    exclude = Some(c);
                    true
                } else if tested && p.friendly {
                    if rules == GoalRules::Bf1 {
                        exclude = Some(c);
                    }
                    true
                } else {
                    match rules {
                        GoalRules::Bf2 => p.friendly && p.def == 0,
                        // The owner-change event: a target that became the
                        // team's, or an own one that stopped being it.
                        GoalRules::Bf1 => p.friendly != matches!(kept, Some(Objective::Defend(_))),
                    }
                }
            }
        };

        let chosen = if draw {
            clock = 0;
            offered.clear();
            offered.extend(posts.iter().enumerate().map(|(ci, p)| DrawPost {
                friendly: p.friendly,
                neutral: p.neutral,
                atk: p.atk,
                def: p.def,
                open: reach(hub, p) && exclude != Some(ci as u32),
            }));
            let r = roll(i as u32, sim.tick, gen);
            let mut got = None;
            // Nothing this unit can route to is left to take: it hunts (the
            // port's endgame). Rare since card HUBM put every post and unit
            // on a hub its mover has an arc at (TAT1: 42.5 -> 0.6 % of on-foot
            // unit-seconds hunting); kept for a unit on a graph that joins no
            // post left to take.
            if posts.iter().any(|p| !p.friendly && reach(hub, p)) {
                // (b) and (c): the spread. The post the ISSUER stands nearest
                // is not drawn (they are already there), nor is any post
                // another dispersed bot of the SAME issuer holds.
                if let Some(k) = spread {
                    let (_, banned, taken) = &disperse[k];
                    let mut spread_offer = offered.clone();
                    for (ci, o) in spread_offer.iter_mut().enumerate() {
                        let ci = ci as u32;
                        o.open &= Some(ci) != *banned && !taken.contains(&ci);
                    }
                    got = draw_post(rules, team, &spread_offer, r);
                    if got.is_none() {
                        // (d) The exclusions asked for more distinct posts
                        // than the map has: they are dropped FOR THIS UNIT
                        // and the counter says how often that happened.
                        sim.nav_stats.disperse_fallbacks += 1;
                    }
                }
                if got.is_none() {
                    got = draw_post(rules, team, &offered, r);
                }
                // Card DEFS: an own post that has its defenders is not drawn.
                let full =
                    |c: u32| posts[c as usize].friendly && held[c as usize] >= cap[c as usize];
                if !endgame && got.is_some_and(full) {
                    for (ci, o) in offered.iter_mut().enumerate() {
                        o.open &= !(posts[ci].friendly && held[ci] >= cap[ci]);
                    }
                    got = draw_post(rules, team, &offered, r).filter(|&c| !full(c));
                }
                if let Some(c) = got.filter(|&c| posts[c as usize].friendly) {
                    held[c as usize] += 1;
                }
            }
            match got {
                Some(c) if posts[c as usize].friendly && endgame => Objective::Hunt,
                Some(c) if posts[c as usize].friendly => Objective::Defend(c),
                Some(c) => Objective::Attack(c),
                None => Objective::Hunt,
            }
        } else {
            // Kept, and named by who owns it now.
            match target {
                Some(c) if posts[c as usize].friendly => Objective::Defend(c),
                Some(c) => Objective::Attack(c),
                None => Objective::Hunt,
            }
        };
        if kept.map(Objective::cp) != Some(chosen.cp()) {
            changes += 1;
            // A new target starts a fresh dwell window, so giving up on one
            // does not immediately give up on the next.
            sim.units[i].ai.unreachable_dwell = 0;
        }
        if let (Some(k), Some(c)) = (spread, chosen.cp()) {
            disperse[k].2.insert(c);
        }
        assignment[i] = Some(chosen);
        at_post[i] = clock;
    }

    // --- a passenger takes its AI driver's target (BF2 `FUN_002e80b0`) ---
    for &(i, _, _, _) in &units {
        let Some(m) = sim.units[i].mount else {
            continue;
        };
        let driver = sim
            .vehicle(m.vehicle)
            .and_then(|v| v.pilot_seat())
            .and_then(|s| s.occupant)
            .filter(|d| d.index as usize != i)
            .and_then(|d| assignment.get(d.index as usize).copied().flatten())
            .filter(|o| o.cp().is_some());
        if let Some(o) = driver {
            if assignment[i].map(Objective::cp) != Some(o.cp()) {
                assignment[i] = Some(o);
                at_post[i] = 0;
            }
        }
    }

    // --- BF1's retake call: a post the team lost to neutral ---
    if rules == GoalRules::Bf1 {
        for (ci, p) in posts.iter().enumerate() {
            if !p.neutral || old.owners.get(ci).copied().flatten() != Some(team) {
                continue;
            }
            let at = sim.cps[ci].def.pos;
            let mut sent = 0;
            for &(i, _, pos, hub) in &units {
                if sent >= RETAKE_UNITS {
                    break;
                }
                let flying = sim.units[i]
                    .mount
                    .and_then(|m| sim.vehicle(m.vehicle))
                    .is_some_and(|v| v.kind == crate::vehicle::VehicleKind::Flyer);
                if flying || (pos - at).length() > RETAKE_RADIUS || !reach(hub, p) {
                    continue;
                }
                sent += 1;
                if assignment[i].and_then(Objective::cp) != Some(ci as u32) {
                    changes += 1;
                    assignment[i] = Some(Objective::Attack(ci as u32));
                    at_post[i] = 0;
                    sim.units[i].ai.unreachable_dwell = 0;
                }
            }
        }
    }
    sim.nav_stats.assignment_changes += changes;
    for a in &assignment {
        if let Some(Objective::Attack(c)) = a {
            if *c < 64 {
                sim.nav_stats.attack_used[t] |= 1u64 << c;
            }
        }
    }

    // --- squads: same objective, within SQUAD_RADIUS of another such unit ---
    for &(i, _, pos, _) in &units {
        let Some(o) = assignment[i] else {
            sim.units[i].ai.squad = None;
            continue;
        };
        let together = units.iter().any(|&(j, _, p, _)| {
            j != i && assignment[j] == Some(o) && (p - pos).length() <= SQUAD_RADIUS
        });
        sim.units[i].ai.squad = together.then(|| o.key());
    }
    for (u, a) in sim.units.iter_mut().zip(assignment.iter()) {
        if u.team == team && a.is_none() {
            u.ai.squad = None;
        }
    }

    // --- card DEFS: one spot a defender, kept while it defends the post ---
    let mut spots: Vec<Option<(Vec3, u8)>> = vec![None; sim.units.len()];
    let defends = |i: usize| match assignment[i] {
        Some(Objective::Defend(c)) if sim.units[i].mount.is_none() => Some(c),
        _ => None,
    };
    let mut taken: Vec<Vec3> = Vec::new();
    for &(i, gen, _, _) in &units {
        let kept = old.gens.get(i).is_none_or(|g| *g == gen)
            && defends(i).is_some()
            && old.of(i as u32) == assignment[i];
        if let Some(s) = old.spot(i as u32).filter(|_| kept) {
            spots[i] = Some(s);
            taken.push(s.0);
        }
    }
    for &(i, _, pos, _) in &units {
        let Some(c) = defends(i).filter(|_| spots[i].is_none()) else {
            continue;
        };
        spots[i] = pick_spot(sim, c, i, pos, &taken);
        taken.extend(spots[i].map(|s| s.0));
    }

    let owners = sim.cps.iter().map(|c| c.owner).collect();
    sim.commander[t] = TeamPlan {
        assignment,
        gens,
        at_post,
        owners,
        spots,
        threat,
        next_plan_tick: old.next_plan_tick,
    };
}

/// Card DEFS, rules 2-4: how many defenders a post keeps, of a team with
/// `alive` living units, `quiet` ticks after its last threat or capture.
pub fn defend_cap(alive: usize, quiet: u64) -> usize {
    if quiet >= DEFEND_QUIET_LEAVE {
        0
    } else if quiet >= DEFEND_QUIET_FALL {
        DEFEND_QUIET_CAP
    } else {
        ((DEFEND_SHARE * alive as f32).round() as usize).max(1)
    }
}

/// Card DEFS, rule 3: the distance inside which an enemy threatens a post -
/// [`THREAT_FRAC`] of the distance between the map's two farthest posts, at
/// least [`THREAT_RADIUS_MIN`].
pub fn threat_radius(sim: &Sim) -> f32 {
    let mut far = 0.0f32;
    for (i, a) in sim.cps.iter().enumerate() {
        for b in &sim.cps[i + 1..] {
            far = far.max(crate::nav::xz_dist(a.def.pos, b.def.pos));
        }
    }
    (THREAT_FRAC * far).max(THREAT_RADIUS_MIN)
}

/// Card DEFS, rule 3 (and card TURR's defence clause): is post `cp` under
/// threat for `team` - a living enemy within `radius` of it, or a living unit
/// of the team within `radius` of it with a target? Like the hunt, the first
/// is not bounded by what anybody sees.
pub fn post_threatened(sim: &Sim, team: u8, cp: u32, radius: f32) -> bool {
    let Some(post) = sim.cps.get(cp as usize) else {
        return false;
    };
    let at = post.def.pos;
    sim.alive_units().any(|(_, u)| {
        (u.soldier.pos - at).length_squared() <= radius * radius
            && (u.team != team || u.ai.target.is_some())
    })
}

/// Card DEFS, rule 1: the spot unit slot `i`, at `from`, defends post `cp`
/// from - the nearest authored Cover / Snipe hint of the post
/// ([`nearest_hint`]) that is not within [`DEFEND_SPACING`] of a `taken`
/// spot. With no hint left it has none: its walk ends at its own drawn point
/// of the capture region, as an attacker's does (card PARR's point: on the
/// post's floor, one a soldier can stand on), where no stance is asked and
/// the PS2's 5 s test sends it on.
fn pick_spot(sim: &Sim, cp: u32, i: usize, from: Vec3, taken: &[Vec3]) -> Option<(Vec3, u8)> {
    let post = sim.cps.get(cp as usize)?;
    let free = |p: Vec3| {
        taken
            .iter()
            .all(|t| crate::nav::xz_dist(*t, p) >= DEFEND_SPACING)
    };
    let scoped = sim.weapon_classes[sim.units[i].weapon_class].ai_distance_table();
    nearest_hint(sim, post.def.pos, HINT_RADIUS, from, scoped, &free)
        .map(|h| (h.pos, h.stance_mask))
}

/// Allowed-stance mask bit: the unit may STAND at this hint.
pub const MASK_STAND: u8 = 1 << 0;
/// Allowed-stance mask bit: it may CROUCH.
pub const MASK_CROUCH: u8 = 1 << 1;
/// Allowed-stance mask bit: it may go PRONE (BF1 only — see [`stance_mask`]).
pub const MASK_PRONE: u8 = 1 << 2;

/// The ALLOWED-STANCE MASK a hint's authored `PrimaryStance` asks for:
/// [`MASK_STAND`] | [`MASK_CROUCH`] | [`MASK_PRONE`].
///
/// `PrimaryStance` is a BITMASK, not an enum (RE lead-verified 2026-09-08
/// evening: `analysis/RE_LOG.md`, `engine/artifacts/lead_tmp/s15/re/primary_stance.md`
/// §1-5 and its answer table). The PS2 draws ONE of the set bits uniformly when
/// the unit arrives at the hint — BF2 `FUN_002deed0` / BF1 `FUN_0023f600`:
/// `do r = rand(0,2); while (((mask >> r) & 1) == 0)` — and HOLDS it, so
/// `PrimaryStance 3` is "stand or crouch", NOT prone. What reaches that picker
/// differs per game, which is why this takes `source_game` the way
/// [`crate::sim::omni_radius_for`] does:
///
/// - **BF2** masks the authored value `& 3` in the setter (`FUN_002defe0`
///   267212-267219) and stores nothing when the result is 0, leaving the hint
///   ctor's default of stand (`FUN_002deb88` 266934, `| 0x500`). BF2 AI
///   therefore never prones, three times over: the setter drops bit 2, the
///   Cover tick folds a prone pick back to stand (257788-257790), and the
///   soldier's go-prone slot is a stub (`FUN_00246ec8`).
/// - **BF1** stores the raw byte whenever it is non-zero (`FUN_0023f648`
///   157262-157265), so 4-7 are the only values that can produce prone, and
///   only on BF1.
///
/// A hint that authors NO `PrimaryStance` asks for stand on both games (the
/// ctor default), so the answer is never "no wish" — a hint always wants
/// something. PORT GUARD, not from the disc: a value whose low three bits are
/// all zero (8, 16, ...) would spin the PS2's rejection sampler forever; no
/// disc authors one (the phase-C census has 1-7 only) and this answers stand.
pub fn stance_mask(primary_stance: Option<u8>, source_game: &str) -> u8 {
    let bits = match primary_stance {
        Some(v) if source_game.eq_ignore_ascii_case("bf1") => v & 7,
        Some(v) => v & 3,
        None => 0,
    };
    if bits == 0 {
        MASK_STAND
    } else {
        bits
    }
}

/// The allowed PEEK SIDES a hint's authored `SecondaryStance` carries: bit 0 =
/// the hint's −right, bit 1 = its +right (the authored bits 3 and 4 — BF2
/// stores them at bits 12-13 of the hint word, `(v & 0x18) << 9`
/// `FUN_002defe0` 267199-267208; BF1 at +0x7e as `(v & 0x18) >> 3`).
///
/// CARRIED ONLY: the Cover peek state that consumes it is Phase D work and
/// nothing reads this yet. `SecondaryStance`'s low bits are a stance mask like
/// the primary one ([`stance_mask`] reads them the same way).
pub fn peek_sides(secondary_stance: Option<u8>) -> u8 {
    (secondary_stance.unwrap_or(0) >> 3) & 3
}

/// The `Stance` a stance CODE means; `None` for the 0 sentinel ("no wish") and
/// for anything a mask cannot produce. The code is the mask bit's INDEX plus
/// one, so 1/2/3 are always stand/crouch/prone whatever the authored value was.
pub fn stance_of_code(code: u8) -> Option<Stance> {
    match code {
        1 => Some(Stance::Stand),
        2 => Some(Stance::Crouch),
        3 => Some(Stance::Prone),
        _ => None,
    }
}

/// The mask bit a stance CODE occupies, and 0 for the sentinel: the bridge
/// between `AiState::want_stance` (a code) and a hint's mask.
pub fn stance_bit(code: u8) -> u8 {
    match code {
        1..=3 => 1 << (code - 1),
        _ => 0,
    }
}

/// The inverse of [`stance_of_code`]: the code that asks for `s`, and 0 for
/// a stance no `PrimaryStance` can author. `AiState::want_stance` stores the
/// CODE rather than the `Stance` because 0 there has to mean "no wish", and
/// `Stance::Stand`'s own discriminant is 0.
pub fn stance_code(s: Stance) -> u8 {
    match s {
        Stance::Stand => 1,
        Stance::Crouch => 2,
        Stance::Prone => 3,
        _ => 0,
    }
}

/// The nearest authored Cover or Snipe hint within [`HINT_RADIUS`] of
/// `centre`, measured from `from`.
///
/// `scoped` is the unit's own "my weapon has a scope"
/// ([`crate::weapon::WeaponClass::ai_distance_table`], D9's predicate and the
/// only scope test the PS2 AI has): a unit without one is not offered a Snipe
/// hint at all, so the marksman posts stay for the marksmen (brief D13).
fn nearest_hint<'a>(
    sim: &'a Sim,
    centre: Vec3,
    radius: f32,
    from: Vec3,
    scoped: bool,
    free: &dyn Fn(Vec3) -> bool,
) -> Option<&'a crate::nav::Hint> {
    let g = sim.nav.as_ref()?;
    let mut best: Option<(f32, &crate::nav::Hint)> = None;
    for h in &g.hints {
        let usable = match h.kind {
            HINT_COVER => true,
            HINT_SNIPE => scoped,
            _ => false,
        };
        if !usable {
            continue;
        }
        if (h.pos - centre).length() > radius || !free(h.pos) {
            continue;
        }
        if HINT_SAME_LEVEL && !hint_on_level(sim, centre.y, h.pos) {
            continue;
        }
        let d = (h.pos - from).length();
        // Strictly less: ties keep the earlier hint (hints are in authored
        // order), so no float equality decides anything.
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, h));
        }
    }
    best.map(|(_, h)| h)
}

/// Does the hint at `hint` stand on the level of a centre at height
/// `centre_y`? The floor the hint stands on (`ground_under` from the hint
/// itself) must be the floor found under the hint's XZ from the centre's
/// height (the [`crate::ai::route`] `hub_has_floor` idiom), within
/// [`STEP_HEIGHT`].
///
/// A hint on a ledge or walkway ABOVE the centre fails — the probe from the
/// centre's height finds the floor under it — and a unit sent at it walks a
/// straight last leg into the wall (queue STK: POL1's `hintnode153`, 8.5 m
/// above cp6con, three defenders stuck in a crate stack all round). A hint
/// BELOW the centre with no floor over it passes when a bot can drop to it —
/// no deeper than the AI's own ledge rule ([`crate::ai::LEDGE_DROP`], card
/// STLVL: BF2 DEA1's cp2 at 52 m has a hint 6 m under its gallery, and its
/// defenders stood on the rim over it until they were recycled).
fn hint_on_level(sim: &Sim, centre_y: f32, hint: Vec3) -> bool {
    let own = sim.ground_under(hint, false);
    let from_centre = sim.ground_under(Vec3::new(hint.x, centre_y, hint.z), false);
    (own - from_centre).abs() <= STEP_HEIGHT && own >= centre_y - crate::ai::LEDGE_DROP
}

/// Where a LONE unit defending `cp` would stand, and in what stance (since
/// card DEFS a defender's own spot is the plan's, [`TeamPlan::spot`], drawn
/// by [`pick_spot`] with every other defender's spot taken; this is the same
/// hint rule with nothing taken): the nearest authored Cover or Snipe
/// hint within [`HINT_RADIUS`] of the POST, or `None` when no hint qualifies.
/// The capture-region fallback belongs to [`pick_spot`].
///
/// Brief D13: the hint's ALLOWED-STANCE MASK comes back with it (never 0 — a
/// hint with no authored `PrimaryStance` asks for stand; see [`stance_mask`]),
/// and the caller draws one of its bits ([`crate::ai::stance_hold`]). A Snipe
/// hint is only offered to a `scoped` unit. `Hint::command_post` — the disc's
/// own "this hint belongs to that post" — is still unread: the design says a
/// radius, and a radius is what this uses.
pub fn defend_spot(sim: &Sim, cp: u32, from: Vec3, scoped: bool) -> Option<(Vec3, u8)> {
    let post = sim.cps.get(cp as usize)?;
    hint_spot(sim, post.def.pos, HINT_RADIUS, from, scoped)
}

/// [`defend_spot`]'s body, over ANY centre and radius (brief D8): the nearest
/// authored Cover or Snipe hint within `radius` of `centre`, measured from
/// `from`, with its allowed-stance mask.
///
/// Defend asks it about a POST at [`HINT_RADIUS`]; a squad `Hold` asks it about
/// the bot's ANCHOR at 10 m (the earlier port's hold order), and a holder that finds
/// one stands there in the stance the hint asks for, exactly as a defender
/// does. One body, so the two cannot drift — and so the sweep, which only ever
/// walks the Defend path, is the referee for both.
pub fn hint_spot(
    sim: &Sim,
    centre: Vec3,
    radius: f32,
    from: Vec3,
    scoped: bool,
) -> Option<(Vec3, u8)> {
    let h = nearest_hint(sim, centre, radius, from, scoped, &|_| true)?;
    Some((h.pos, h.stance_mask))
}

/// The allowed-stance mask an IDLE unit takes from where it stands (brief
/// D13): the same hint rule as [`defend_spot`], centred on the unit itself
/// rather than on a post, and **0 when no hint is in range** (no wish). It
/// answers a mask only — an idle unit is not sent walking to a hint, it uses
/// the one it is already standing in.
pub fn hint_stance(sim: &Sim, from: Vec3, scoped: bool) -> u8 {
    nearest_hint(sim, from, HINT_RADIUS, from, scoped, &|_| true).map_or(0, |h| h.stance_mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cp::CpDef;
    use crate::heightfield::Heightfield;
    use crate::nav::{Arc, Hub, NavGraph, RouteTable};
    use crate::region::Region;
    use crate::squad::{Command, CommandKind};
    use crate::unit::{Controller, UnitSpec};

    /// Three hubs in a line (0 - 1 - 2), soldier arcs, no authored route table
    /// (the BFS answers every hop) — enough for a hop-distance field with a
    /// near post and a far one.
    fn line_graph() -> NavGraph {
        let hub = |x: f32| Hub {
            pos: Vec3::new(x, 0.0, 0.0),
            radius: 2.0,
            arcs: Vec::new(),
        };
        let mut hubs = vec![hub(0.0), hub(40.0), hub(80.0)];
        hubs[0].arcs = vec![0];
        hubs[1].arcs = vec![0, 1];
        hubs[2].arcs = vec![1];
        let arcs = vec![
            Arc {
                a: 0,
                b: 1,
                type_mask: 1,
                flags: 0,
            },
            Arc {
                a: 1,
                b: 2,
                type_mask: 1,
                flags: 0,
            },
        ];
        NavGraph::new(hubs, arcs, RouteTable::empty(3), Vec::new(), Vec::new()).expect("graph")
    }

    fn post(name: &str, x: f32, value: i32) -> CpDef {
        CpDef {
            name: name.into(),
            pos: Vec3::new(x, 0.0, 0.0),
            capture: Region::sphere(Vec3::new(x, 0.0, 0.0), 6.0),
            control: None,
            value_bleed: 100,
            value_atk: [value, value],
            value_def: [value, value],
            spawn_nodes: vec![],
            capture_time: 10.0,
            neutralize_time: 12.0,
        }
    }

    /// A flat world with the line graph, `owners` posts at the three hubs and
    /// four AI of team 0 standing at hub 1.
    fn world(owners: &[Option<u8>]) -> Sim {
        let mut sim = Sim::empty(7);
        sim.heightfield = Some(Heightfield {
            size: 2,
            unit: 400.0,
            offset_x: 200.0,
            offset_z: 200.0,
            heights: vec![0.0; 4],
        });
        sim.nav = Some(line_graph());
        for (i, o) in owners.iter().enumerate() {
            sim.add_cp(post(&format!("cp{i}"), i as f32 * 40.0, 10), *o);
        }
        for k in 0..4 {
            sim.spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(0, Vec3::new(40.0, 0.0, k as f32 * 2.0))
            })
            .expect("spawned");
        }
        // One enemy, far enough away (300 m) that no post is CONTESTED, so
        // the tests choose when to contest one — and near enough to hub 2 that
        // HUNT always has somewhere to go.
        sim.spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(1, Vec3::new(80.0, 0.0, 300.0))
        })
        .expect("spawned");
        sim
    }

    /// Twelve more AI of team 0, 100 m and more from every post: a team of
    /// sixteen, whose share of defenders a post is four (card DEFS).
    fn crowd(sim: &mut Sim) {
        for k in 0..12 {
            sim.spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(0, Vec3::new(0.0, 0.0, 100.0 + 2.0 * k as f32))
            })
            .expect("spawned");
        }
    }

    fn assignments(sim: &Sim, team: u8) -> Vec<Option<Objective>> {
        sim.commander[usize::from(team)].assignment.clone()
    }

    /// The Disperse-bias world (brief D5): `values.len()` ENEMY posts at
    /// `x = 40 i` (so every one of them is an Attack objective and Hunt is
    /// never offered), `bots` AI of team 0 standing at hub 1, and the ISSUER —
    /// a player of team 0 in slot 0 — standing with them at x = 40, which puts
    /// every bot inside [`crate::squad::ORDER_REACH`] and makes **cp1** the post
    /// D5(b) keeps the squad off. Bots are indices `0..bots`, the player is
    /// `bots`, the enemy `bots + 1`.
    fn disperse_world(values: &[i32], bots: usize) -> Sim {
        let mut sim = Sim::empty(7);
        sim.heightfield = Some(Heightfield {
            size: 2,
            unit: 400.0,
            offset_x: 200.0,
            offset_z: 200.0,
            heights: vec![0.0; 4],
        });
        sim.nav = Some(line_graph());
        for (i, v) in values.iter().enumerate() {
            sim.add_cp(post(&format!("cp{i}"), i as f32 * 40.0, *v), Some(1));
        }
        for k in 0..bots {
            sim.spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(0, Vec3::new(40.0, 0.0, k as f32 * 2.0))
            })
            .expect("bot");
        }
        sim.spawn(&UnitSpec::player_in_slot(0, 0, Vec3::new(40.0, 0.0, 0.0)))
            .expect("issuer");
        // One enemy 300 m off, as in `world`: it contests nothing, and Hunt
        // has somewhere to go if it is ever offered.
        sim.spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(1, Vec3::new(80.0, 0.0, 300.0))
        })
        .expect("enemy");
        sim
    }

    /// What the AI of team 0 were assigned, in index order, unwrapped.
    fn objectives(sim: &Sim, bots: usize) -> Vec<Objective> {
        assignments(sim, 0)[..bots]
            .iter()
            .map(|o| o.expect("every bot in the pool gets an objective"))
            .collect()
    }

    /// A hint at `x` of `kind` whose allowed-stance mask is `mask` (what the
    /// loader has already read out of the authored `PrimaryStance` — see
    /// [`stance_mask`]).
    fn hint(name: &str, kind: u8, x: f32, mask: u8) -> crate::nav::Hint {
        crate::nav::Hint {
            name: name.into(),
            kind,
            pos: Vec3::new(x, 0.0, 0.0),
            rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            radius: None,
            mode: None,
            stance_mask: mask,
            secondary_stance_mask: MASK_STAND,
            peek_sides: 0,
            command_post: None,
        }
    }

    /// The RE's ANSWER TABLE for `PrimaryStance`, both games, verbatim
    /// (`engine/artifacts/lead_tmp/s15/re/primary_stance.md`, lead-verified
    /// 2026-09-08 evening; `analysis/RE_LOG.md` 2026-09-08 evening). The field
    /// is a BITMASK: 3 is stand-or-crouch and NOT prone, BF2 masks `& 3` so it
    /// never prones at all, and BF1's 4-7 are the only values that can.
    #[test]
    fn the_primary_stance_mask_is_the_res_answer_table() {
        // v -> (BF2 mask, BF1 mask), one row per row of the RE's table.
        let table: [(Option<u8>, u8, u8); 8] = [
            (Some(1), MASK_STAND, MASK_STAND),
            (Some(2), MASK_CROUCH, MASK_CROUCH),
            // NOT prone on either game — the old 1/2/3 enum reading's error.
            (Some(3), MASK_STAND | MASK_CROUCH, MASK_STAND | MASK_CROUCH),
            // BF2: `4 & 3 == 0` -> not stored -> the ctor default, stand.
            (Some(4), MASK_STAND, MASK_PRONE),
            (Some(5), MASK_STAND, MASK_STAND | MASK_PRONE),
            (Some(6), MASK_CROUCH, MASK_CROUCH | MASK_PRONE),
            (
                Some(7),
                MASK_STAND | MASK_CROUCH,
                MASK_STAND | MASK_CROUCH | MASK_PRONE,
            ),
            // Absent: both ctors default to stand, so a hint always wants one.
            (None, MASK_STAND, MASK_STAND),
        ];
        for (v, bf2, bf1) in table {
            assert_eq!(stance_mask(v, "bf2"), bf2, "BF2 PrimaryStance {v:?}");
            assert_eq!(stance_mask(v, "bf1"), bf1, "BF1 PrimaryStance {v:?}");
            // The name comes from the manifest and its case is not ours.
            assert_eq!(stance_mask(v, "BF1"), bf1, "case-insensitive");
        }
        // BF2 can never be handed a prone bit; every mask is non-zero.
        for v in 0..=255u8 {
            assert_eq!(stance_mask(Some(v), "bf2") & MASK_PRONE, 0, "BF2 {v} prone");
            assert_ne!(stance_mask(Some(v), "bf2"), 0);
            assert_ne!(stance_mask(Some(v), "bf1"), 0);
            // PORT GUARD (no disc authors it): a value with no low bit set at
            // all would spin the PS2 picker, and answers stand here.
            if v & 7 == 0 {
                assert_eq!(stance_mask(Some(v), "bf1"), MASK_STAND, "guard {v}");
            }
        }
        // The peek sides are the authored bits 3 and 4, and nothing else.
        assert_eq!(peek_sides(None), 0);
        assert_eq!(peek_sides(Some(3)), 0);
        assert_eq!(peek_sides(Some(8 | 3)), 1);
        assert_eq!(peek_sides(Some(16 | 2)), 2);
        assert_eq!(peek_sides(Some(24 | 1)), 3);
        assert_eq!(peek_sides(Some(0xE0)), 0, "bits above 4 are not sides");
    }

    /// The stance CODE (what `AiState::want_stance` carries) round-trips, and
    /// the 0 sentinel and the stances no hint can author stay outside it.
    #[test]
    fn the_stance_code_is_the_mask_bit_index_plus_one() {
        assert_eq!(stance_of_code(1), Some(Stance::Stand));
        assert_eq!(stance_of_code(2), Some(Stance::Crouch));
        assert_eq!(stance_of_code(3), Some(Stance::Prone));
        for c in [0u8, 4, 5, 255] {
            assert_eq!(stance_of_code(c), None, "code {c} means nothing");
            assert_eq!(stance_bit(c), 0);
        }
        for c in [1u8, 2, 3] {
            assert_eq!(stance_code(stance_of_code(c).expect("mapped")), c);
        }
        // A code's bit is the mask bit the loader would have set for it.
        assert_eq!(stance_bit(1), MASK_STAND);
        assert_eq!(stance_bit(2), MASK_CROUCH);
        assert_eq!(stance_bit(3), MASK_PRONE);
        // The code of a `Stance` no hint can author must be 0 — the sentinel
        // `AiState::want_stance` reads as "no wish".
        for s in [Stance::Sprint, Stance::Jump, Stance::Roll, Stance::Fall] {
            assert_eq!(stance_code(s), 0);
        }
    }

    /// D13(a): a Snipe hint is only offered to a unit whose weapon passes the
    /// AI distance table; a Cover hint is offered to everyone. D13(b): the
    /// spot comes back with the hint's allowed-stance MASK.
    #[test]
    fn a_snipe_hint_is_for_scoped_units_only_and_the_spot_carries_its_stance() {
        let mut sim = world(&[Some(0), None, None]);
        // cp0 is at x = 0. A Snipe hint 3 m from it (prone: a BF1 hint
        // authoring 4) and a Cover hint 9 m from it (crouch): the Snipe one is
        // nearer to both the post and the unit, so it wins whenever it is on
        // offer at all.
        sim.nav.as_mut().expect("graph").hints = vec![
            hint("snipe", HINT_SNIPE, 3.0, MASK_PRONE),
            hint("cover", HINT_COVER, 9.0, MASK_CROUCH),
        ];
        let from = Vec3::ZERO;
        assert_eq!(
            defend_spot(&sim, 0, from, true),
            Some((Vec3::new(3.0, 0.0, 0.0), MASK_PRONE)),
            "a scoped unit takes the nearer Snipe hint and its stance"
        );
        assert_eq!(
            defend_spot(&sim, 0, from, false),
            Some((Vec3::new(9.0, 0.0, 0.0), MASK_CROUCH)),
            "an unscoped unit does not see the Snipe hint at all"
        );
        // A hint outside HINT_RADIUS of the POST is not a spot, however near
        // the unit stands to it.
        sim.nav.as_mut().expect("graph").hints = vec![hint("far", HINT_COVER, 26.0, MASK_STAND)];
        assert_eq!(defend_spot(&sim, 0, Vec3::new(26.0, 0.0, 0.0), true), None);
        // A mask with two bits comes back whole — the DRAW is the caller's
        // (`ai::stance_hold`), not the spot's.
        sim.nav.as_mut().expect("graph").hints =
            vec![hint("odd", HINT_COVER, 5.0, MASK_STAND | MASK_CROUCH)];
        assert_eq!(
            defend_spot(&sim, 0, from, true),
            Some((Vec3::new(5.0, 0.0, 0.0), MASK_STAND | MASK_CROUCH))
        );
        // `hint_stance` is the same rule centred on the UNIT: 40 m from cp0 is
        // outside the post's radius, but the unit is standing on the hint.
        sim.nav.as_mut().expect("graph").hints = vec![hint("here", HINT_COVER, 40.0, MASK_PRONE)];
        assert_eq!(defend_spot(&sim, 0, Vec3::new(40.0, 0.0, 0.0), true), None);
        assert_eq!(
            hint_stance(&sim, Vec3::new(40.0, 0.0, 0.0), true),
            MASK_PRONE
        );
        // No hint in range is the one "no wish" answer.
        assert_eq!(hint_stance(&sim, Vec3::new(80.0, 0.0, 0.0), true), 0);
    }

    /// The plan is a pure function of sim state: two identical sims stepped
    /// the same way produce the same assignment, and the same hash.
    #[test]
    fn the_plan_is_deterministic_and_replay_exact() {
        let run = || {
            let mut sim = world(&[Some(1), None, Some(1)]);
            for _ in 0..90 {
                sim.step(&[crate::sim::PadState::default()]);
            }
            (assignments(&sim, 0), sim.hash())
        };
        let (a, ha) = run();
        let (b, hb) = run();
        assert_eq!(a, b);
        assert_eq!(ha, hb);
        assert!(a.iter().any(|o| o.is_some()), "somebody got an objective");
        // The plan really is part of the hash: perturbing it changes it.
        let mut sim = world(&[Some(1), None, Some(1)]);
        sim.step(&[crate::sim::PadState::default()]);
        let before = sim.hash();
        sim.commander[0].assignment[1] = Some(Objective::Hunt);
        assert_ne!(before, sim.hash(), "the plan is in the sim hash");
    }

    /// D5: a Follow or Hold bot is the PLAYER'S — it leaves the commander's
    /// pool the moment the order lands, stays out of every plan while it
    /// stands, and comes back at the first plan after `clear_all`.
    #[test]
    fn a_followed_or_holding_bot_is_out_of_the_plan_until_it_is_cleared() {
        for follow in [true, false] {
            let mut sim = disperse_world(&[10, 10, 10], 4);
            plan(&mut sim, 0);
            assert!(
                assignments(&sim, 0)[..4].iter().all(|o| o.is_some()),
                "every bot starts in the pool"
            );
            let h = sim.players[0].expect("the issuer");
            let kind = if follow {
                CommandKind::Follow(h)
            } else {
                CommandKind::Hold(Vec3::ZERO)
            };
            // A FOLLOW is taken by two (card SQACK), a HOLD by everyone.
            let n = if follow { crate::squad::FOLLOW_CAP } else { 4 };
            assert_eq!(usize::from(sim.issue_command(0, kind)), n);
            plan(&mut sim, 0);
            assert!(
                assignments(&sim, 0)[..n].iter().all(|o| o.is_none()),
                "out of the pool while the order stands ({kind:?})"
            );
            assert!(
                sim.units[..n].iter().all(|u| u.ai.squad.is_none()),
                "and out of the squad bookkeeping"
            );
            assert_eq!(usize::from(sim.clear_all(0)), n);
            plan(&mut sim, 0);
            assert!(
                assignments(&sim, 0)[..4].iter().all(|o| o.is_some()),
                "back in the pool after clear_all ({kind:?})"
            );
        }
    }

    /// D5(b)+(c): four dispersed bots of one issuer take four DIFFERENT
    /// posts and none of them the post their issuer stands on. The control —
    /// the same world, no order — is measured beside it, because "they
    /// spread" only means something if they would not have: over thirty
    /// ticks' draws the dispersed four spread every time and the control
    /// does not.
    #[test]
    fn four_dispersed_bots_take_four_distinct_objectives_none_the_issuers() {
        let mut control_spread = 0;
        for tick in 0..30 {
            let mut ctl = disperse_world(&[10, 10, 10, 10, 10], 4);
            ctl.tick = tick;
            plan(&mut ctl, 0);
            let control = objectives(&ctl, 4);
            let control_keys: BTreeSet<u32> = control.iter().map(|o| o.key()).collect();
            control_spread +=
                u32::from(control_keys.len() == 4 && !control.iter().any(|o| o.cp() == Some(1)));

            let mut sim = disperse_world(&[10, 10, 10, 10, 10], 4);
            sim.tick = tick;
            assert_eq!(sim.issue_command(0, CommandKind::Disperse(900)), 4);
            plan(&mut sim, 0);
            let got = objectives(&sim, 4);
            let keys: BTreeSet<u32> = got.iter().map(|o| o.key()).collect();
            assert_eq!(keys.len(), 4, "four distinct objectives: {got:?}");
            assert!(
                !got.iter().any(|o| o.cp() == Some(1)),
                "none of them the issuer's own post cp1: {got:?}"
            );
            assert_eq!(
                sim.nav_stats.disperse_fallbacks, 0,
                "nothing had to fall back"
            );
        }
        assert!(
            control_spread < 10,
            "the control does not spread by itself: {control_spread} of 30"
        );
    }

    /// D5(c): the post an earlier dispersed bot of the SAME issuer holds is
    /// not drawn by the next one, even when it is worth ten times the other.
    #[test]
    fn an_earlier_dispersed_bot_takes_its_objective_off_the_table() {
        for tick in 0..30 {
            let mut sim = disperse_world(&[10, 10, 100], 4);
            sim.tick = tick;
            assert_eq!(sim.issue_command(0, CommandKind::Disperse(900)), 4);
            plan(&mut sim, 0);
            let got = objectives(&sim, 4);
            let first: BTreeSet<Option<u32>> = got[..2].iter().map(|o| o.cp()).collect();
            assert_eq!(
                first,
                BTreeSet::from([Some(0), Some(2)]),
                "the first two take one post each: {got:?}"
            );
            // Two posts on offer and four bots: the last two run out and fall
            // back, which is (d) again and is what the counter is for.
            assert_eq!(sim.nav_stats.disperse_fallbacks, 2, "{got:?}");
        }
    }

    /// D5(d): the exclusions can ask for more distinct posts than the map
    /// has. On a FOUR-post map with the issuer's post excluded only three are
    /// left, so the fourth bot drops them — and the counter says so, once.
    #[test]
    fn a_dispersed_bot_falls_back_when_the_objectives_run_out() {
        let mut sim = disperse_world(&[10, 10, 10, 10], 4);
        assert_eq!(sim.issue_command(0, CommandKind::Disperse(900)), 4);
        plan(&mut sim, 0);
        let got = objectives(&sim, 4);
        assert_eq!(
            sim.nav_stats.disperse_fallbacks, 1,
            "exactly one bot ran out: {got:?}"
        );
        let first: BTreeSet<Option<u32>> = got[..3].iter().map(|o| o.cp()).collect();
        assert_eq!(
            first,
            BTreeSet::from([Some(0), Some(2), Some(3)]),
            "three spread; the fourth falls back onto any post: {got:?}"
        );
    }

    /// D5: the rule lives exactly as long as the order does. A `Disperse` whose
    /// tick has passed plans EXACTLY as no order at all (phase 0b clears it
    /// first in a stepped sim; a directly-called `plan` proves the guard).
    #[test]
    fn an_expired_disperse_plans_as_if_there_were_no_order() {
        let mut sim = disperse_world(&[10, 10, 10, 10, 10], 4);
        assert_eq!(sim.issue_command(0, CommandKind::Disperse(900)), 4);
        sim.tick = 900;
        plan(&mut sim, 0);

        let mut ctl = disperse_world(&[10, 10, 10, 10, 10], 4);
        ctl.tick = 900;
        plan(&mut ctl, 0);

        assert_eq!(
            assignments(&sim, 0),
            assignments(&ctl, 0),
            "expired == no order"
        );
        assert_eq!(sim.nav_stats.disperse_fallbacks, 0);
    }

    /// D5(a): the FIRST plan after the order draws afresh, so a bot cannot
    /// keep the target it already had just because it had it; every plan
    /// after that keeps it like any other.
    ///
    /// The pair differs ONLY in the order's `issued_tick` — the same world,
    /// the same tick, so the same draw.
    #[test]
    fn the_first_plan_after_a_disperse_draws_afresh() {
        let held = Objective::Attack(0);
        let run = |tick: u64, age: u64| -> Objective {
            let mut sim = disperse_world(&[10, 10, 10], 1);
            assert_eq!(sim.issue_command(0, CommandKind::Disperse(100_000)), 1);
            sim.units[0].command = Some(Command {
                kind: CommandKind::Disperse(100_000),
                issuer: 0,
                issued_tick: tick - age,
            });
            sim.commander[0].assignment = vec![Some(held); sim.units.len()];
            sim.tick = tick;
            plan(&mut sim, 0);
            objectives(&sim, 1)[0]
        };
        let (mut fresh_kept, mut stale_kept) = (0, 0);
        for t in 100..130 {
            // age 0: issued this very tick. age PLAN_PERIOD: the plan after the
            // first one (`issued_tick + PLAN_PERIOD > tick` is false at equal).
            let (fresh, stale) = (run(t, 0), run(t, PLAN_PERIOD));
            fresh_kept += u32::from(fresh == held);
            stale_kept += u32::from(stale == held);
        }
        assert_eq!(stale_kept, 30, "a standing order keeps its target");
        // cp1 is the issuer's: the fresh draw is cp0 or cp2, evenly.
        assert!(
            (5..=25).contains(&fresh_kept),
            "the fresh order draws again: kept {fresh_kept} of 30"
        );
    }

    /// Queue HDL: a hint on a ledge above the post is not a defend spot, even
    /// when it is the nearer one — the defender would walk its straight last
    /// leg into the wall under it (POL1's `hintnode153`, 8.5 m above cp6con).
    /// The same-floor hint is taken instead; with none on the floor this
    /// optional hint query returns `None`.
    #[test]
    fn a_hint_above_the_posts_level_is_not_a_defend_spot() {
        use crate::collision::{Body, CollisionWorld, Shape};
        use glam::Mat4;
        let mut sim = world(&[Some(0), None, None]);
        // A ledge 3 m from cp0 (x = 0), its top at y 8.
        sim.collision = CollisionWorld::new(
            vec![Body::new(
                Mat4::from_translation(Vec3::new(3.0, 7.5, 0.0)),
                Shape::Box {
                    half: Vec3::new(2.0, 0.5, 2.0),
                },
            )],
            Vec::new(),
        );
        let mut up = hint("up", HINT_COVER, 3.0, MASK_CROUCH);
        up.pos.y = 8.0;
        sim.nav.as_mut().expect("graph").hints =
            vec![up, hint("floor", HINT_COVER, 9.0, MASK_STAND)];
        let from = Vec3::ZERO;
        assert_eq!(
            defend_spot(&sim, 0, from, false),
            Some((Vec3::new(9.0, 0.0, 0.0), MASK_STAND)),
            "the nearer hint is 8 m up: the same-floor one is the spot"
        );
        // A hint just off the floor (its own floor is still the post's) is on
        // the level.
        sim.nav.as_mut().expect("graph").hints[1].pos.y = 0.5;
        sim.nav.as_mut().expect("graph").hints[1].pos.x = 2.0;
        sim.nav.as_mut().expect("graph").hints[1].pos.z = 5.0;
        assert_eq!(
            defend_spot(&sim, 0, from, false),
            Some((Vec3::new(2.0, 0.5, 5.0), MASK_STAND))
        );
        // Only the ledge hint: the optional hint query returns no spot.
        sim.nav.as_mut().expect("graph").hints.truncate(1);
        assert_eq!(defend_spot(&sim, 0, from, false), None);
    }

    /// Card STLVL: a hint UNDER the post's level with nothing over it is a
    /// spot only when a bot can drop to it — BF2 DEA1's cp2 has one 6 m
    /// under its gallery, past the AI's ledge rule.
    #[test]
    fn a_hint_a_drop_too_deep_under_the_post_is_not_a_defend_spot() {
        let sim = world(&[Some(0), None, None]);
        let on_ground = Vec3::new(9.0, 0.0, 0.0);
        assert!(hint_on_level(&sim, 0.0, on_ground), "the post's own floor");
        assert!(hint_on_level(&sim, 3.0, on_ground), "a 3 m drop");
        assert!(
            !hint_on_level(&sim, crate::ai::LEDGE_DROP + 2.0, on_ground),
            "a 6 m drop"
        );
    }

    /// D8: [`hint_spot`] is [`defend_spot`]'s body over any centre and radius —
    /// a squad `Hold` asks it about an ANCHOR at
    /// 10 m, where a defender asks about a post at
    /// [`HINT_RADIUS`], and the radius is the only difference.
    #[test]
    fn the_hint_spot_is_the_defend_spot_over_any_centre_and_radius() {
        let mut sim = world(&[Some(1), None, Some(1)]);
        let g = sim.nav.as_mut().expect("graph");
        // Cover at x = 49 and x = 51, an anchor at x = 40: 9 m and 11 m off.
        g.hints = vec![
            hint("near", HINT_COVER, 49.0, MASK_CROUCH),
            hint("far", HINT_COVER, 51.0, MASK_STAND),
        ];
        let anchor = Vec3::new(40.0, 0.0, 0.0);
        let hold = 10.0;
        assert_eq!(
            hint_spot(&sim, anchor, hold, anchor, false),
            Some((Vec3::new(49.0, 0.0, 0.0), MASK_CROUCH)),
            "the one inside 10 m"
        );
        // Only the far one is authored: nothing is in range of the anchor, and
        // the SAME query at the post radius finds it.
        sim.nav.as_mut().expect("graph").hints.remove(0);
        assert_eq!(
            hint_spot(&sim, anchor, hold, anchor, false),
            None,
            "11 m off"
        );
        assert_eq!(
            hint_spot(&sim, anchor, HINT_RADIUS, anchor, false),
            Some((Vec3::new(51.0, 0.0, 0.0), MASK_STAND)),
            "in range of a post"
        );
        // And Defend still goes through it: the post at x = 40 is cp1.
        assert_eq!(
            defend_spot(&sim, 1, anchor, false),
            hint_spot(&sim, sim.cps[1].def.pos, HINT_RADIUS, anchor, false),
            "defend_spot IS hint_spot at the post"
        );
    }

    fn dp(owner: Option<u8>, team: u8, atk: i32, def: i32) -> DrawPost {
        DrawPost {
            friendly: owner == Some(team),
            neutral: owner.is_none(),
            atk,
            def,
            open: true,
        }
    }

    /// `n` draws of `posts` with the plan's own per-unit number, as the share
    /// (%) of each post.
    fn shares(rules: GoalRules, team: u8, posts: &[DrawPost], n: u32) -> Vec<f32> {
        let mut hits = vec![0u32; posts.len()];
        for k in 0..n {
            // 32 unit slots over consecutive plans, as a round draws them.
            let r = roll(k % 32, u64::from(k / 32) * PLAN_PERIOD + 1, 1 + k / 640);
            assert!((0.0..1.0).contains(&r), "{r}");
            let c = draw_post(rules, team, posts, r).expect("a post");
            hits[c as usize] += 1;
        }
        hits.iter().map(|&h| 100.0 * h as f32 / n as f32).collect()
    }

    /// The draw on Polis Massa's start (`analysis/re-conquest-goal.md` section
    /// 8's test vectors): port team 0 (the PS2's team 1) owns cp6con 20,
    /// cp4con 10 and cp1con 10, team 1 cp5con 20 and cp3con 10, cp2con (20)
    /// is neutral. Team 1's candidates weigh 20 : 20 : 10 : 10 (PS2: 21 / 27
    /// / 17 / 14 of 79 draws), team 0's 20 : 20 : 10 (13 / 17 / 8 of 38), and
    /// an own post is never drawn while a candidate exists.
    #[test]
    fn the_draw_is_by_value_over_the_posts_that_are_not_the_teams() {
        // cp1con, cp2con, cp3con, cp4con, cp5con, cp6con.
        let owners = [Some(0), None, Some(1), Some(0), Some(1), Some(0)];
        let values = [10, 20, 10, 10, 20, 20];
        let posts = |team: u8| -> Vec<DrawPost> {
            owners
                .iter()
                .zip(values)
                .map(|(o, v)| dp(*o, team, v, v))
                .collect()
        };
        let close = |got: &[f32], want: &[f32]| {
            for (g, w) in got.iter().zip(want) {
                assert!((g - w).abs() <= 3.0, "{got:?} against {want:?}");
            }
        };
        let third = 100.0 / 3.0;
        let s = shares(GoalRules::Bf2, 1, &posts(1), 10_000);
        close(&s, &[third / 2.0, third, 0.0, third / 2.0, 0.0, third]);
        assert_eq!((s[2], s[4]), (0.0, 0.0), "never an own post: {s:?}");
        let s = shares(GoalRules::Bf2, 0, &posts(0), 10_000);
        close(&s, &[0.0, 40.0, 20.0, 0.0, 40.0, 0.0]);
        assert_eq!((s[0], s[3], s[5]), (0.0, 0.0, 0.0), "{s:?}");
    }

    /// BF2 weighs a hostile post by ATK and a neutral one by the mean of ATK
    /// and DEF, and drops a value of 0 and a post the unit cannot reach.
    #[test]
    fn the_bf2_draw_weighs_by_owner_and_drops_zero_and_unreachable_posts() {
        let mut posts = vec![
            dp(Some(1), 0, 30, 0),
            dp(None, 0, 30, 10),
            dp(Some(1), 0, 0, 50),
            dp(Some(1), 0, 40, 0),
        ];
        posts[3].open = false;
        let s = shares(GoalRules::Bf2, 0, &posts, 10_000);
        for (g, w) in s.iter().zip([60.0, 40.0, 0.0, 0.0]) {
            assert!((g - w).abs() <= 3.0, "{s:?}");
        }
        assert_eq!((s[2], s[3]), (0.0, 0.0), "{s:?}");
    }

    /// BF1 (`FUN_002617f8`): the PS2's team 1 (port team 0) weighs every
    /// candidate by `Value_ATK`, its team 2 by `Value_DEF`, whoever owns the
    /// post — a neutral post is no mean.
    #[test]
    fn the_bf1_draw_weighs_team_1_by_atk_and_team_2_by_def() {
        // (ATK, DEF): 30 / 10 and 10 / 30, one hostile and one neutral.
        let posts = |team: u8| vec![dp(Some(1 - team), team, 30, 10), dp(None, team, 10, 30)];
        let s = shares(GoalRules::Bf1, 0, &posts(0), 10_000);
        assert!((s[0] - 75.0).abs() <= 3.0, "team 0 by ATK: {s:?}");
        let s = shares(GoalRules::Bf1, 1, &posts(1), 10_000);
        assert!((s[0] - 25.0).abs() <= 3.0, "team 1 by DEF: {s:?}");
        // BF2 on the same posts: 30 against the mean 20.
        let s = shares(GoalRules::Bf2, 1, &posts(1), 10_000);
        assert!((s[0] - 60.0).abs() <= 3.0, "{s:?}");
    }

    /// With no candidate BF2 draws among its own posts by `Value_DEF`, then
    /// uniformly (never on a one-post map); BF1 goes straight to uniform.
    #[test]
    fn with_nothing_to_attack_the_draw_falls_back() {
        let own = vec![dp(Some(0), 0, 10, 30), dp(Some(0), 0, 10, 10)];
        let s = shares(GoalRules::Bf2, 0, &own, 10_000);
        assert!((s[0] - 75.0).abs() <= 3.0, "own posts by DEF: {s:?}");
        let s = shares(GoalRules::Bf1, 0, &own, 10_000);
        assert!((s[0] - 50.0).abs() <= 3.0, "BF1 uniform: {s:?}");
        let zero = vec![dp(Some(0), 0, 10, 0), dp(Some(0), 0, 10, 0)];
        let s = shares(GoalRules::Bf2, 0, &zero, 10_000);
        assert!((s[0] - 50.0).abs() <= 3.0, "then uniform: {s:?}");
        assert_eq!(draw_post(GoalRules::Bf2, 0, &zero[..1], 0.5), None);
        assert_eq!(draw_post(GoalRules::Bf1, 0, &[], 0.5), None);
    }

    /// Plan team 0 `n` times, a second apart, without stepping the sim.
    fn plans(sim: &mut Sim, n: u32) {
        for _ in 0..n {
            sim.tick += PLAN_PERIOD;
            plan(sim, 0);
        }
    }

    /// A unit keeps the post it drew: sixty plans with enemies about, posts
    /// to choose from and nothing done change no target.
    #[test]
    fn a_target_is_kept_from_plan_to_plan() {
        let mut sim = world(&[Some(1), Some(1), Some(1)]);
        // Off every post, so nobody is "at" its target.
        for (k, u) in sim.units.iter_mut().enumerate() {
            u.soldier.pos = Vec3::new(20.0, 0.0, 30.0 + k as f32);
        }
        plan(&mut sim, 0);
        let first = assignments(&sim, 0);
        let changes = sim.nav_stats.assignment_changes;
        assert_eq!(changes, 4, "one draw per unit");
        // An enemy walks onto one of the targets: nothing in the plan looks.
        let n = sim.units.len() - 1;
        sim.units[n].soldier.pos = Vec3::new(0.0, 0.0, 1.0);
        plans(&mut sim, 60);
        assert_eq!(assignments(&sim, 0), first);
        assert_eq!(sim.nav_stats.assignment_changes, changes);
    }

    /// A respawned slot holds no target: the new life draws.
    #[test]
    fn a_new_life_draws_again() {
        let mut sim = world(&[Some(1), Some(1), Some(1)]);
        let held = vec![Some(Objective::Attack(0)); sim.units.len()];
        let gens: Vec<u32> = sim.units.iter().map(|u| u.gen).collect();
        sim.commander[0].assignment = held.clone();
        sim.commander[0].gens = gens.clone();
        plans(&mut sim, 1);
        assert_eq!(assignments(&sim, 0)[..4], held[..4], "the same lives keep");
        sim.commander[0].gens = gens.iter().map(|g| g + 1).collect();
        plans(&mut sim, 1);
        assert_ne!(assignments(&sim, 0)[..4], held[..4], "new lives draw");
        assert_eq!(sim.nav_stats.assignment_changes, 4);
    }

    /// BF2: a captured target is held (`Defend`) by whoever targeted it. A
    /// unit inside its capture region leaves 5 s after arriving; one with a
    /// fight on its hands, or outside the region, stays; and a post worth 0
    /// to defend sends its holders on at once.
    #[test]
    fn bf2_holds_a_captured_target_until_five_seconds_at_the_post() {
        let fixture = |def: i32| {
            let mut sim = world(&[Some(1), Some(1), Some(1)]);
            crowd(&mut sim);
            sim.cps[1].def.value_def = [def, def];
            let mut held = vec![Some(Objective::Attack(1)); 5];
            held.resize(sim.units.len(), Some(Objective::Attack(0)));
            sim.commander[0].assignment = held;
            // Units 0-2 stand on cp1 (x = 40); unit 3 is on its way.
            sim.units[3].soldier.pos = Vec3::new(20.0, 0.0, 30.0);
            sim.cps[1].owner = Some(0);
            sim
        };
        let mut sim = fixture(10);
        // Unit 2 has an enemy to fight.
        let enemy = sim.handle(4);
        sim.units[2].ai.target = Some(enemy);
        plans(&mut sim, 4);
        assert_eq!(
            assignments(&sim, 0)[..4],
            [Some(Objective::Defend(1)); 4],
            "held, and relabelled"
        );
        assert_eq!(
            sim.nav_stats.assignment_changes, 0,
            "a relabel is no change"
        );
        plans(&mut sim, 1);
        let a = assignments(&sim, 0);
        assert!(
            a[..2]
                .iter()
                .all(|o| matches!(o, Some(Objective::Attack(0 | 2)))),
            "5 s at the post: on to an enemy post: {a:?}"
        );
        assert_eq!(a[2..4], [Some(Objective::Defend(1)); 2], "{a:?}");
        // The fight over, unit 2's five seconds start.
        sim.units[2].ai.target = None;
        plans(&mut sim, 4);
        assert_eq!(assignments(&sim, 0)[2], Some(Objective::Defend(1)));
        plans(&mut sim, 1);
        assert!(matches!(
            assignments(&sim, 0)[2],
            Some(Objective::Attack(0 | 2))
        ));

        // Value_DEF 0: every holder draws at the next plan, wherever it is.
        let mut sim = fixture(0);
        plans(&mut sim, 1);
        let a = assignments(&sim, 0);
        assert!(
            a[..4]
                .iter()
                .all(|o| matches!(o, Some(Objective::Attack(0 | 2)))),
            "{a:?}"
        );
    }

    /// The 5 s test runs once per stay: a unit that arrived more than 5 s
    /// before the capture stays on as the post's defender (card DEFS: while
    /// the post is threatened - here by an enemy 25 m off).
    #[test]
    fn a_capturer_that_passed_its_test_stays() {
        let mut sim = world(&[Some(1), Some(1), Some(1)]);
        enemy_near_cp1(&mut sim, 25.0);
        sim.commander[0].assignment = vec![Some(Objective::Attack(1)); 5];
        plans(&mut sim, 8);
        assert_eq!(assignments(&sim, 0)[0], Some(Objective::Attack(1)));
        assert_eq!(sim.commander[0].at_post[0], AT_POST_TESTED);
        sim.cps[1].owner = Some(0);
        plans(&mut sim, 30);
        assert_eq!(assignments(&sim, 0)[0], Some(Objective::Defend(1)));
        // Away from the post and back: a new stay, a new test.
        sim.units[0].soldier.pos = Vec3::new(20.0, 0.0, 30.0);
        plans(&mut sim, 1);
        sim.units[0].soldier.pos = Vec3::new(40.0, 0.0, 0.0);
        plans(&mut sim, 5);
        assert!(matches!(
            assignments(&sim, 0)[0],
            Some(Objective::Attack(0 | 2))
        ));
    }

    /// BF1 (`FUN_00261f88`): when a target becomes the team's, every unit
    /// holding it draws at once — nobody stays as a defender.
    #[test]
    fn bf1_redraws_every_holder_when_the_target_is_taken() {
        let mut sim = world(&[Some(1), Some(1), Some(1)]);
        sim.ground_rules = crate::soldier::GroundRules::Bf1;
        sim.commander[0].assignment = vec![Some(Objective::Attack(1)); 5];
        sim.units[3].soldier.pos = Vec3::new(20.0, 0.0, 30.0);
        plans(&mut sim, 3);
        assert_eq!(assignments(&sim, 0)[..4], [Some(Objective::Attack(1)); 4]);
        sim.cps[1].owner = Some(0);
        plans(&mut sim, 1);
        let a = assignments(&sim, 0);
        assert!(
            a[..4]
                .iter()
                .all(|o| matches!(o, Some(Objective::Attack(0 | 2)))),
            "{a:?}"
        );
    }

    /// BF1's retake call (`FUN_00262090`): a post the team lost to neutral
    /// takes the team's AI within 60 m — five of them at most.
    #[test]
    fn bf1_sends_up_to_five_nearby_units_to_a_post_lost_to_neutral() {
        let mut sim = world(&[Some(1), Some(0), Some(1)]);
        sim.ground_rules = crate::soldier::GroundRules::Bf1;
        // Three more of team 0 beside the first three; unit 3 is 100 m off.
        for k in 0..3 {
            sim.spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(0, Vec3::new(42.0, 0.0, k as f32 * 2.0))
            })
            .expect("spawned");
        }
        sim.units[3].soldier.pos = Vec3::new(40.0, 0.0, 100.0);
        sim.commander[0].assignment = vec![Some(Objective::Attack(0)); 8];
        plans(&mut sim, 1);
        assert!(assignments(&sim, 0)
            .iter()
            .all(|o| *o != Some(Objective::Attack(1))));
        sim.cps[1].owner = None;
        plans(&mut sim, 1);
        let a = assignments(&sim, 0);
        let sent: Vec<usize> = (0..a.len())
            .filter(|&i| a[i] == Some(Objective::Attack(1)))
            .collect();
        assert_eq!(sent, vec![0, 1, 2, 5, 6], "five, in index order: {a:?}");
        // The call is the event's: the next plan sends nobody new.
        plans(&mut sim, 1);
        assert_eq!(assignments(&sim, 0), a);
    }

    /// BF1's at-post helper (`FUN_00255fd0`): 5 s inside an own target's
    /// region the unit draws again WITHOUT that post. Here the only other
    /// post is worth 0, so the draw is the uniform fallback over the open
    /// posts: with the exclusion every unit in the region leaves, without
    /// it some stay.
    #[test]
    fn bf1_excludes_the_post_it_stood_five_seconds_on() {
        let mut sim = world(&[Some(1), Some(0)]);
        // Card DEFS: a team of sixteen and a threatened post, so the cap
        // (four) takes none of the four away.
        crowd(&mut sim);
        enemy_near_cp1(&mut sim, 25.0);
        sim.ground_rules = crate::soldier::GroundRules::Bf1;
        sim.cps[0].def.value_atk = [0, 0];
        sim.commander[0].assignment = vec![Some(Objective::Defend(1)); 5];
        plans(&mut sim, 4);
        assert_eq!(assignments(&sim, 0)[..4], [Some(Objective::Defend(1)); 4]);
        plans(&mut sim, 1);
        let a = assignments(&sim, 0);
        assert_eq!(a[..3], [Some(Objective::Attack(0)); 3], "{a:?}");
        assert_eq!(a[3], Some(Objective::Defend(1)), "on the region's rim");
    }

    /// A passenger takes its AI driver's target (BF2 `FUN_002e80b0`), and
    /// keeps its own when the pilot seat is empty.
    #[test]
    fn a_passenger_takes_its_drivers_target() {
        use crate::vehicle::{SeatKind, VehicleKind, VehicleSpec};
        use crate::weapon::HealthType;
        let mut sim = world(&[Some(1), Some(1), Some(1)]);
        for (k, u) in sim.units.iter_mut().enumerate() {
            u.soldier.pos = Vec3::new(20.0, 0.0, 30.0 + k as f32);
        }
        let v = sim.spawn_vehicle(&VehicleSpec {
            kind: VehicleKind::Hover,
            class: 0,
            home_team: Some(0),
            neutral_when_empty: true,
            spawn_pad: None,
            pos: Vec3::new(20.0, 1.0, 30.0),
            yaw: 0.0,
            max_health: 900.0,
            health_type: HealthType::Vehicle,
            seats: vec![SeatKind::Pilot, SeatKind::Passenger(0)],
        });
        let (driver, rider) = (sim.handle(0), sim.handle(1));
        assert!(sim.seat_unit(rider, v, 1));
        let mut held = vec![Some(Objective::Attack(0)); 5];
        held[1] = Some(Objective::Attack(2));
        sim.commander[0].assignment = held.clone();
        plans(&mut sim, 1);
        assert_eq!(assignments(&sim, 0)[..4], held[..4], "no driver: kept");
        assert!(sim.seat_unit(driver, v, 0));
        plans(&mut sim, 1);
        let a = assignments(&sim, 0);
        assert_eq!(a[..4], [Some(Objective::Attack(0)); 4], "{a:?}");
    }

    /// Two graphs that do not join: hubs 0 - 1 and 2 - 3, a post at 0, 1 and
    /// 2 and the four AI of team 0 at hub 1.
    fn split_world(owners: [Option<u8>; 3]) -> Sim {
        let mut sim = world(&[]);
        let hub = |x: f32, arc: u16| Hub {
            pos: Vec3::new(x, 0.0, 0.0),
            radius: 2.0,
            arcs: vec![arc],
        };
        let arc = |a: u16, b: u16| Arc {
            a,
            b,
            type_mask: 1,
            flags: 0,
        };
        sim.nav = NavGraph::new(
            vec![hub(0.0, 0), hub(40.0, 0), hub(80.0, 1), hub(120.0, 1)],
            vec![arc(0, 1), arc(2, 3)],
            RouteTable::empty(4),
            Vec::new(),
            Vec::new(),
        );
        for (i, o) in owners.iter().enumerate() {
            sim.add_cp(post(&format!("cp{i}"), i as f32 * 40.0, 10), *o);
        }
        for (k, u) in sim.units.iter_mut().enumerate() {
            u.soldier.pos = Vec3::new(40.0, 0.0, 10.0 + k as f32);
        }
        sim
    }

    /// The reach drop: a post the unit has no route to is not offered.
    #[test]
    fn a_post_with_no_route_to_it_is_not_drawn() {
        let mut sim = split_world([Some(1), Some(0), Some(1)]);
        // Eight first draws per unit, each with another random number.
        for _ in 0..8 {
            sim.commander[0].assignment.clear();
            plans(&mut sim, 1);
            let a = assignments(&sim, 0);
            assert_eq!(a[..4], [Some(Objective::Attack(0)); 4], "{a:?}");
        }
    }

    /// No route to the target it holds: the unit draws again at the next
    /// plan (BF2's draw tests the route; a PORT GUARD on a BF1 map).
    #[test]
    fn a_target_with_no_route_to_it_is_redrawn() {
        let mut sim = split_world([Some(1), Some(0), Some(1)]);
        sim.commander[0].assignment = vec![Some(Objective::Attack(2)); 5];
        plans(&mut sim, 1);
        let a = assignments(&sim, 0);
        assert_eq!(a[..4], [Some(Objective::Attack(0)); 4], "{a:?}");
        assert_eq!(sim.nav_stats.assignment_changes, 4);
    }

    /// In the endgame nobody holds a post: a holder draws an enemy post, or
    /// hunts when there is none it may take.
    #[test]
    fn in_the_endgame_nobody_holds_a_post() {
        let mut sim = world(&[Some(0), Some(0), Some(1)]);
        crowd(&mut sim);
        sim.cps[2].def.value_atk = [0, 0];
        sim.commander[0].assignment = vec![Some(Objective::Defend(1)); 5];
        plans(&mut sim, 3);
        assert_eq!(assignments(&sim, 0)[..4], [Some(Objective::Defend(1)); 4]);
        sim.battle = crate::battle::Battle::new([crate::ai::HOLD_ENDGAME, 100]);
        plans(&mut sim, 1);
        assert_eq!(assignments(&sim, 0)[..4], [Some(Objective::Hunt); 4]);
        sim.cps[2].def.value_atk = [10, 10];
        plans(&mut sim, 1);
        assert_eq!(assignments(&sim, 0)[..4], [Some(Objective::Attack(2)); 4]);
    }

    /// With nothing left to take the team HUNTS (the port's endgame), and
    /// nobody hunts while a post is still takeable.
    #[test]
    fn hunt_when_there_is_nothing_to_attack() {
        let mut sim = world(&[Some(0), Some(0), Some(0)]);
        plan(&mut sim, 0);
        let a = assignments(&sim, 0);
        assert_eq!(a[..4], [Some(Objective::Hunt); 4], "{a:?}");
        let mut sim = world(&[Some(1), Some(0), Some(0)]);
        plan(&mut sim, 0);
        let b = assignments(&sim, 0);
        assert_eq!(b[..4], [Some(Objective::Attack(0)); 4], "{b:?}");
    }

    /// Units on the same objective standing together are a squad; one on its
    /// own is not.
    #[test]
    fn units_together_on_one_objective_form_a_squad() {
        let mut sim = world(&[Some(1), None, Some(1)]);
        // Unit 3 is sent far away from the other three.
        sim.units[3].soldier.pos = Vec3::new(40.0, 0.0, 300.0);
        plan(&mut sim, 0);
        let a = assignments(&sim, 0);
        let squads: Vec<Option<u32>> = sim.units.iter().map(|u| u.ai.squad).collect();
        for i in 0..3 {
            if a[i] == a[(i + 1) % 3] {
                assert_eq!(
                    squads[i],
                    a[i].map(Objective::key),
                    "unit {i} shares its objective with a neighbour"
                );
            }
        }
        assert_eq!(squads[3], None, "the lone unit has no squad");
    }

    #[test]
    fn objective_bytes_and_keys_are_distinct() {
        let all = [
            Objective::Attack(0),
            Objective::Attack(1),
            Objective::Defend(0),
            Objective::Defend(1),
            Objective::Hunt,
        ];
        let mut keys: Vec<u32> = all.iter().map(|o| o.key()).collect();
        let mut bytes: Vec<[u8; 5]> = all.iter().map(|o| o.bytes()).collect();
        keys.sort_unstable();
        keys.dedup();
        bytes.sort_unstable();
        bytes.dedup();
        assert_eq!(keys.len(), all.len());
        assert_eq!(bytes.len(), all.len());
        assert_eq!(Objective::Attack(3).cp(), Some(3));
        assert_eq!(Objective::Hunt.cp(), None);
        assert_eq!(Objective::Defend(2).label(), "Defend cp2");
    }

    #[test]
    fn the_draws_number_is_uniform_and_varies_per_unit_and_plan() {
        let mut bins = [0u32; 10];
        for u in 0..32u32 {
            for p in 0..100u64 {
                let x = roll(u, p * PLAN_PERIOD, 1);
                assert!((0.0..1.0).contains(&x), "{x}");
                bins[(x * 10.0) as usize] += 1;
            }
        }
        assert!(
            bins.iter().all(|&b| (250..=390).contains(&b)),
            "3,200 numbers over ten bins: {bins:?}"
        );
        // Deterministic.
        assert_eq!(roll(3, 90, 1), roll(3, 90, 1));
        assert_ne!(roll(3, 90, 1), roll(3, 120, 1));
        assert_ne!(roll(3, 90, 1), roll(4, 90, 1));
        assert_ne!(roll(3, 90, 1), roll(3, 90, 2));
    }

    /// Card DEFS's scene: [`world`] with cp1 the enemy's and sixteen AI of
    /// team 0 - slots 0-3 are `world`'s (at x = 40, z = 0 / 2 / 4 / 6), slots
    /// 5-8 stand 11-14 m from cp1 (outside its 6 m region, so the PS2's 5 s
    /// at-post test never runs for them) and slots 9-16 far off at cp0 - the
    /// eight near cp1 all targeting it. One plan is made, then cp1 is TAKEN:
    /// the next plan sees the capture.
    fn defs_world() -> Sim {
        let mut sim = world(&[Some(1), Some(1), Some(1)]);
        for k in 0..12 {
            let at = match k {
                0..=3 => Vec3::new(40.0, 0.0, -11.0 - k as f32),
                _ => Vec3::new(0.0, 0.0, 20.0 + 2.0 * k as f32),
            };
            sim.spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(0, at)
            })
            .expect("spawned");
        }
        let mut a = vec![Some(Objective::Attack(0)); sim.units.len()];
        for i in [0, 1, 2, 3, 5, 6, 7, 8] {
            a[i] = Some(Objective::Attack(1));
        }
        sim.commander[0].assignment = a;
        sim.commander[0].gens = sim.units.iter().map(|u| u.gen).collect();
        plans(&mut sim, 1);
        sim.cps[1].owner = Some(0);
        sim
    }

    fn defenders(sim: &Sim, cp: u32) -> Vec<usize> {
        (0..sim.units.len())
            .filter(|&i| sim.commander[0].of(i as u32) == Some(Objective::Defend(cp)))
            .collect()
    }

    /// An enemy that stands `d` metres from cp1 and thinks nothing.
    fn enemy_near_cp1(sim: &mut Sim, d: f32) {
        sim.spawn(&UnitSpec::trooper(1, Vec3::new(40.0 + d, 0.0, 0.0)))
            .expect("enemy");
    }

    /// Card DEFS, rule 2: of the eight that targeted the post, a quarter of
    /// the team's sixteen living units hold it - the four NEAREST - and the
    /// other four have another post at once.
    #[test]
    fn a_taken_post_keeps_a_quarter_of_the_team_and_the_rest_go_on_at_once() {
        let mut sim = defs_world();
        // Slots 0 and 1 stand 20 m off: the nearest four are 2, 3, 5 and 6.
        sim.units[0].soldier.pos = Vec3::new(40.0, 0.0, 20.0);
        sim.units[1].soldier.pos = Vec3::new(40.0, 0.0, 21.0);
        plans(&mut sim, 1);
        assert_eq!(defend_cap(16, 0), 4);
        assert_eq!(defenders(&sim, 1), [2, 3, 5, 6], "the four nearest hold it");
        for i in [0, 1, 7, 8] {
            assert!(
                matches!(sim.commander[0].of(i), Some(Objective::Attack(0 | 2))),
                "unit {i} is over the cap: {:?}",
                sim.commander[0].of(i)
            );
        }
        // The share is of the LIVING team, and never under one defender.
        assert_eq!(defend_cap(17, 0), 4);
        assert_eq!(defend_cap(22, 0), 6);
        assert_eq!(defend_cap(1, 0), 1);
        // With eight of the team dead the cap is two.
        let mut sim = defs_world();
        for i in 9..17 {
            sim.units[i].alive = false;
        }
        plans(&mut sim, 1);
        assert_eq!(defenders(&sim, 1), [0, 1]);
        // A DRAW that lands on an own post with its defenders is made again
        // without it: here the enemy's post is worth nothing, so BF2 draws
        // among its own - four take cp1 and the rest go at cp0 after all.
        let mut sim = world(&[Some(1), Some(0)]);
        crowd(&mut sim);
        sim.cps[0].def.value_atk = [0, 0];
        plans(&mut sim, 1);
        assert_eq!(defenders(&sim, 1), [0, 1, 2, 3]);
        for i in 5..17 {
            assert_eq!(sim.commander[0].of(i), Some(Objective::Attack(0)));
        }
    }

    /// Card DEFS, rule 1: of four defenders two have the post's two Cover
    /// hints, one each with its stance, kept from plan to plan, and the other
    /// two have none; stepped, the two walk to their hints and crouch, the
    /// others to a point of the capture region each, standing.
    #[test]
    fn every_defender_has_its_own_spot_and_only_a_hint_asks_for_a_crouch() {
        let mut sim = defs_world();
        sim.nav.as_mut().expect("graph").hints = vec![
            hint("a", HINT_COVER, 49.0, MASK_CROUCH),
            hint("b", HINT_COVER, 31.0, MASK_CROUCH),
        ];
        // Out of the region, so nobody's 5 s test runs while they walk.
        for i in 0..4 {
            sim.units[i].soldier.pos = Vec3::new(40.0, 0.0, 6.5 + i as f32);
        }
        plans(&mut sim, 1);
        let ds = defenders(&sim, 1);
        assert_eq!(ds, [0, 1, 2, 3]);
        let spots: Vec<(Vec3, u8)> = (0..sim.units.len())
            .filter_map(|i| sim.commander[0].spot(i as u32))
            .collect();
        assert_eq!(spots.len(), 2, "one defender a hint: {spots:?}");
        assert!(crate::nav::xz_dist(spots[0].0, spots[1].0) >= DEFEND_SPACING);
        assert_eq!(
            spots.iter().map(|s| s.1).collect::<Vec<_>>(),
            [MASK_CROUCH; 2],
            "the hints' own stance"
        );
        // A hint is one defender's: a third asking has none left.
        let taken: Vec<Vec3> = spots.iter().map(|s| s.0).collect();
        assert_eq!(
            pick_spot(&sim, 1, 2, sim.units[2].soldier.pos, &taken),
            None
        );
        assert!(pick_spot(&sim, 1, 2, sim.units[2].soldier.pos, &taken[..1]).is_some());
        // Kept: moved so that each stands nearer the OTHER's hint, each still
        // has its own at the next plan.
        let holders: Vec<usize> = ds
            .iter()
            .copied()
            .filter(|&i| sim.commander[0].spot(i as u32).is_some())
            .collect();
        let before: Vec<_> = holders
            .iter()
            .map(|&i| sim.commander[0].spot(i as u32))
            .collect();
        let home: Vec<Vec3> = holders.iter().map(|&i| sim.units[i].soldier.pos).collect();
        for (k, &i) in holders.iter().enumerate() {
            let other = before[1 - k].expect("a spot").0;
            sim.units[i].soldier.pos = Vec3::new(other.x, 0.0, 5.0);
        }
        plans(&mut sim, 1);
        let after: Vec<_> = holders
            .iter()
            .map(|&i| sim.commander[0].spot(i as u32))
            .collect();
        assert_eq!(after, before, "a defender keeps its spot");
        for (k, &i) in holders.iter().enumerate() {
            sim.units[i].soldier.pos = home[k];
        }
        sim.commander[0].threat[1] = sim.tick;
        // Stepped (4 s, inside the capture's own 5 s): the two on a hint
        // stand on it, kept, and crouch; the two without walk into the
        // capture region and stand; no two of the four are in one place.
        for _ in 0..120 {
            sim.step(&[crate::sim::PadState::default()]);
        }
        assert_eq!(defenders(&sim, 1), ds);
        for &i in &ds {
            let u = &sim.units[i];
            match sim.commander[0].spot(i as u32) {
                Some(s) => {
                    assert!(spots.contains(&s), "kept");
                    assert!(
                        crate::nav::xz_dist(u.soldier.pos, s.0) < 1.0,
                        "unit {i} at {:?}, its spot {:?}",
                        u.soldier.pos,
                        s.0
                    );
                    assert_eq!(u.soldier.stance, Stance::Crouch, "unit {i}");
                }
                None => {
                    assert!(sim.cps[1].def.capture.contains(u.soldier.pos), "unit {i}");
                    assert_eq!(
                        u.soldier.stance,
                        Stance::Stand,
                        "unit {i}: no hint, no crouch"
                    );
                }
            }
            for &j in ds.iter().filter(|&&j| j > i) {
                let d = crate::nav::xz_dist(u.soldier.pos, sim.units[j].soldier.pos);
                assert!(d >= 1.5, "units {i} and {j} are {d} m apart");
            }
        }
    }

    /// Card DEFS, rule 3: five seconds without a threat or a capture and the
    /// post keeps ONE defender; an enemy inside the threat radius - a fifth
    /// of the distance between the map's two farthest posts - keeps the
    /// share, and brings it back when it comes.
    #[test]
    fn a_quiet_post_keeps_one_defender_and_a_threatened_one_its_share() {
        // Quiet: the 300 m enemy threatens nothing.
        let mut sim = defs_world();
        for i in 0..4 {
            sim.units[i].soldier.pos = Vec3::new(40.0, 0.0, 6.5 + i as f32);
        }
        plans(&mut sim, 5);
        assert_eq!(defenders(&sim, 1), [0, 1, 2, 3], "4 s after the capture");
        plans(&mut sim, 1);
        assert_eq!(defenders(&sim, 1), [0], "5 s: one, the nearest");
        assert_eq!(defend_cap(16, DEFEND_QUIET_FALL - 1), 4);
        assert_eq!(defend_cap(16, DEFEND_QUIET_FALL), DEFEND_QUIET_CAP);

        // Threatened: an enemy 25 m from the post (the radius here is the
        // floor, 30 m: the posts span 80 m).
        let mut sim = defs_world();
        for i in 0..4 {
            sim.units[i].soldier.pos = Vec3::new(40.0, 0.0, 6.5 + i as f32);
        }
        assert_eq!(threat_radius(&sim), THREAT_RADIUS_MIN);
        enemy_near_cp1(&mut sim, 25.0);
        plans(&mut sim, 40);
        assert_eq!(defenders(&sim, 1), [0, 1, 2, 3], "held while threatened");
        assert_eq!(sim.commander[0].threat[1], sim.tick);

        // The radius is the map's: at 35 m the same enemy is no threat on
        // this map, and is one where the farthest posts are 400 m apart.
        let mut sim = defs_world();
        enemy_near_cp1(&mut sim, 35.0);
        let r = threat_radius(&sim);
        assert!(!post_threatened(&sim, 0, 1, r));
        sim.cps[2].def.pos.x = 400.0;
        let r = threat_radius(&sim);
        assert_eq!(r, THREAT_FRAC * 400.0);
        assert!(post_threatened(&sim, 0, 1, r));
        // A team-mate there with a target is a threat too; one without is not.
        let mut sim = defs_world();
        assert!(!post_threatened(&sim, 0, 1, 30.0));
        let enemy = sim.handle(4);
        sim.units[0].ai.target = Some(enemy);
        assert!(post_threatened(&sim, 0, 1, 30.0));
        assert!(!post_threatened(&sim, 0, 0, 30.0), "not cp0, 40 m off");
    }

    /// Card DEFS, rule 4: twenty seconds quiet and the last defender draws
    /// another post too, though it never stood in the capture region (the
    /// PS2's 5 s test did not run for it) - and not while the post is
    /// threatened.
    #[test]
    fn the_last_defender_of_a_quiet_post_goes_on_after_twenty_seconds() {
        let fixture = || {
            let mut sim = defs_world();
            for i in 0..4 {
                sim.units[i].soldier.pos = Vec3::new(40.0, 0.0, 6.5 + i as f32);
            }
            sim
        };
        let mut sim = fixture();
        plans(&mut sim, 20);
        assert_eq!(defenders(&sim, 1), [0], "19 s after the capture");
        assert_eq!(sim.commander[0].at_post[0], 0, "never in the region");
        plans(&mut sim, 1);
        assert_eq!(defenders(&sim, 1), [] as [usize; 0], "20 s: nobody");
        assert!(matches!(
            sim.commander[0].of(0),
            Some(Objective::Attack(0 | 2))
        ));
        assert_eq!(defend_cap(16, DEFEND_QUIET_LEAVE - 1), DEFEND_QUIET_CAP);
        assert_eq!(defend_cap(16, DEFEND_QUIET_LEAVE), 0);
        // Threatened all the while: the four are still there.
        let mut sim = fixture();
        enemy_near_cp1(&mut sim, 25.0);
        plans(&mut sim, 30);
        assert_eq!(defenders(&sim, 1), [0, 1, 2, 3]);
    }

    /// The hash must separate "no unit there" from "a unit with no objective",
    /// and must move for every field.
    #[test]
    fn the_plan_hash_covers_every_field() {
        let h = |p: &TeamPlan| {
            let mut acc: u64 = 0xCBF2_9CE4_8422_2325;
            p.hash_into(&mut |b: &[u8]| {
                for &x in b {
                    acc ^= u64::from(x);
                    acc = acc.wrapping_mul(0x1_0000_01B3);
                }
            });
            acc
        };
        let base = TeamPlan {
            assignment: vec![Some(Objective::Attack(1)), None],
            gens: vec![1, 0],
            at_post: vec![2, 0],
            owners: vec![Some(0), None],
            spots: vec![Some((Vec3::new(1.0, 2.0, 3.0), MASK_CROUCH)), None],
            threat: vec![90, 0],
            next_plan_tick: 30,
        };
        let mut tick = base.clone();
        tick.next_plan_tick = 60;
        let mut shorter = base.clone();
        shorter.assignment.pop();
        let mut other = base.clone();
        other.assignment[0] = Some(Objective::Defend(1));
        let mut none = base.clone();
        none.assignment[0] = None;
        let mut swapped = base.clone();
        swapped.assignment.swap(0, 1);
        let mut gen = base.clone();
        gen.gens[0] = 2;
        let mut clock = base.clone();
        clock.at_post[0] = AT_POST_TESTED;
        let mut owner = base.clone();
        owner.owners[1] = Some(1);
        let mut spot = base.clone();
        spot.spots[0] = Some((Vec3::new(1.0, 2.0, 3.5), MASK_CROUCH));
        let mut mask = base.clone();
        mask.spots[0] = Some((Vec3::new(1.0, 2.0, 3.0), 0));
        let mut no_spot = base.clone();
        no_spot.spots[0] = None;
        let mut threat = base.clone();
        threat.threat[1] = 90;
        for v in [
            &tick, &shorter, &other, &none, &swapped, &gen, &clock, &owner, &spot, &mask, &no_spot,
            &threat,
        ] {
            assert_ne!(h(&base), h(v), "{v:?}");
        }
    }
}
