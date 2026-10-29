//! Card DPRX (OURS, a measuring tool): the HUMAN PROXY - a bot the sim treats
//! as a human player for every human-only rule, so the sweep and the arena
//! can measure difficulty the way a human meets it, without one playing.
//!
//! A proxy is an ordinary AI unit (`Controller::Ai`, the bot brain drives
//! it) with [`crate::unit::Unit::proxy_human`] set. Everywhere a rule asks
//! "is this a human?" about the unit as a TARGET or as a team member it asks
//! [`crate::unit::Unit::is_human`], which counts the proxy:
//! - target choice: the tier's pressure weight on a human target
//!   (`perception::choose_target_with`);
//! - card TOKN's hit tokens (`ai::tokn`): only a token holder's shots at him
//!   may hit, the rest aim near misses;
//! - the aim error against him: BF2's whole error against a human, 0.3 of it
//!   against a bot (`gunner_foot::error_vector`), and his error block updated
//!   every tick (`Sim::aim_blocks`);
//! - the D12 human-damage multiplier on a bot's hit into him, and a bot's
//!   friendly fire not hurting him (`resolve_bolt`, `blast_unit`);
//! - the bleed handicap's "team with a human" (`Sim::apply_team_handicap`);
//! - card DTEAM's per-team human count: [`humans_on`].
//!
//! What stays a BOT's: the cheats (they test `is_player` only, so a proxy
//! never gets them), spawning (waves, the alive-AI cap, the class draw),
//! seats, and every rule about the unit as a SHOOTER (a bot's headshot gate,
//! a bot's spread multiplier, the AI aim error). It gets no damage buff: its
//! edge over a bot is only its fixed [`Profile`].
//!
//! The profile is a fixed "human" row, the same on every tier (a human does
//! not get better when the host picks Elite): its aim skill, its reaction,
//! what it does when hit, how readily it runs from a grenade, how far it
//! sees and whether it strafes (circles) instead of standing. Two strengths,
//! [`Strength::Average`] and [`Strength::Good`]; the numbers are PROVISIONAL
//! until card DBASE calibrates "good" against the maintainer's own match
//! (`scripts/human_profile.py`, `analysis/difficulty-proposal.md` §5).

use crate::sim::Sim;
use crate::skill::{Difficulty, Skill, Tier};
use crate::unit::Unit;

/// The two proxy strengths of `analysis/difficulty-proposal.md` §5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strength {
    /// "P-avg": a decent player.
    Average,
    /// "P-good": a competent player; the difficulty targets are his.
    Good,
}

impl Strength {
    pub fn label(self) -> &'static str {
        match self {
            Strength::Average => "average",
            Strength::Good => "good",
        }
    }

    /// `average|avg|good`, case-insensitive.
    pub fn parse(s: &str) -> Option<Strength> {
        match s.trim().to_ascii_lowercase().as_str() {
            "average" | "avg" => Some(Strength::Average),
            "good" => Some(Strength::Good),
            _ => None,
        }
    }

    /// This strength's row. PROVISIONAL (ours, the proposal's defaults with
    /// the damage multipliers dropped: a proxy must be fair).
    pub fn profile(self) -> Profile {
        match self {
            Strength::Average => Profile {
                skill: 17,
                reaction_ticks: 9,
                react: [0.30, 0.15, 0.20],
                grenade: 0.60,
                view_mult: 1.25,
                circles: false,
            },
            Strength::Good => Profile {
                skill: 20,
                reaction_ticks: 6,
                react: [0.35, 0.25, 0.30],
                grenade: 0.95,
                view_mult: 1.5,
                circles: true,
            },
        }
    }
}

/// A proxy's fixed "human" row. Every field replaces the tier's own for this
/// unit only. PROVISIONAL.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    /// The aim skill (1..=20, the originals' scale): spread, dispersion, the
    /// aim tracker, the error cap, burst and pause ([`Skill::from_level`]),
    /// WITHOUT a tier's override (Elite's 0.5 m cap is a bot's extra).
    pub skill: u8,
    /// Ticks from first seeing an enemy to the first shot at it.
    pub reaction_ticks: u64,
    /// When hit: the chances to strafe, roll, move to a new position (the
    /// rest it stays) - `Tier::react`.
    pub react: [f32; 3],
    /// The chance to run clear of a grenade that lands near.
    pub grenade: f32,
    /// The view distance multiplier (x the map's AI view multiplier).
    pub view_mult: f32,
    /// Fights on the move (the circling style: walks a ring round its
    /// target) instead of the bot's own drawn style; a scoped weapon still
    /// stands.
    pub circles: bool,
}

impl Profile {
    /// `tier` with this profile's columns in place of the tier's.
    pub fn over(&self, mut tier: Tier, map_view: f32) -> Tier {
        tier.reaction_ticks = self.reaction_ticks;
        tier.react = self.react;
        tier.grenade = self.grenade;
        tier.view_mult = self.view_mult * map_view;
        tier
    }

    /// The skill vector: the level's curves, no tier override.
    pub fn skill(&self) -> Skill {
        Skill::from_level(self.skill)
    }

    /// Feed into the sim hash (only a proxy's unit eats it).
    pub fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(&[0xD7, self.skill, u8::from(self.circles)]);
        eat(&self.reaction_ticks.to_le_bytes());
        for v in self
            .react
            .iter()
            .chain([self.grenade, self.view_mult].iter())
        {
            eat(&v.to_bits().to_le_bytes());
        }
    }
}

/// Card DTEAM's hook: how many humans team `team` has, proxies included -
/// each player slot's unit on the team, plus every proxy unit on it (alive or
/// waiting to respawn: it keeps the flag through its deaths).
pub fn humans_on(sim: &Sim, team: u8) -> u8 {
    let players = sim
        .players
        .iter()
        .flatten()
        .filter(|h| {
            sim.units
                .get(h.index as usize)
                .is_some_and(|u| u.team == team && u.proxy_human.is_none())
        })
        .count();
    let proxies = sim
        .units
        .iter()
        .filter(|u| u.team == team && u.proxy_human.is_some())
        .count();
    u8::try_from(players + proxies).unwrap_or(u8::MAX)
}

impl Sim {
    /// The tier columns unit `u`'s brain works to: its team's
    /// ([`Sim::ai_tier`]), or a proxy's own profile over them.
    pub fn unit_tier(&self, u: &Unit) -> Tier {
        let tier = self.ai_tier(u.team);
        match &u.proxy_human {
            Some(p) => p.over(tier, self.mission_ai.view_mult),
            None => tier,
        }
    }

    /// Unit `u`'s skill vector: its integer skill under its team's tier, or
    /// a proxy's own level with no tier override.
    pub fn skill_of(&self, u: &Unit) -> Skill {
        match &u.proxy_human {
            Some(p) => p.skill(),
            None => self.difficulty_of(u.team).skill_at(u.skill),
        }
    }

    /// Make the unit `h` a proxy (`None`: an ordinary bot again). It keeps
    /// the flag through its respawns. Returns whether `h` was a live AI.
    pub fn set_proxy(&mut self, h: crate::unit::UnitHandle, profile: Option<Profile>) -> bool {
        match self.unit_mut(h) {
            Some(u) if u.controller == crate::unit::Controller::Ai && !u.is_player => {
                u.proxy_human = profile;
                true
            }
            _ => false,
        }
    }
}

/// Card DPRX's difficulty targets: the proxy team's win rate with a GOOD
/// proxy, averaged over both sides of each map (the maintainer, 2026-10-28;
/// PROVISIONAL until he confirms them after DBASE). `(tier, low, high)` as
/// fractions; `high` 1.0 = no ceiling.
pub const TARGETS: [(Difficulty, f32, f32); 4] = [
    (Difficulty::Easy, 0.90, 1.0),
    (Difficulty::Normal, 0.65, 0.80),
    (Difficulty::Hard, 0.45, 0.60),
    (Difficulty::Elite, 0.25, 0.40),
];

/// The target band of tier `d`.
pub fn target(d: Difficulty) -> (f32, f32) {
    TARGETS
        .iter()
        .find(|(t, _, _)| *t == d)
        .map_or((0.0, 1.0), |&(_, lo, hi)| (lo, hi))
}

/// PASS when `wins` of `rounds` is inside tier `d`'s band.
pub fn passes(d: Difficulty, wins: u32, rounds: u32) -> bool {
    if rounds == 0 {
        return false;
    }
    let (lo, hi) = target(d);
    let r = wins as f32 / rounds as f32;
    r >= lo && r <= hi
}

#[cfg(test)]
mod tests;
