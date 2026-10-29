//! The MATCH LOG (card DPRX) — DEV SCAFFOLDING beside the black box: one
//! compact file per battle with everything the human did, for the WHOLE
//! round (the black box keeps 60 s). `scripts/human_profile.py` turns it into
//! the profile the human proxy is calibrated on.
//!
//! On with dev mode (on by default while the game is unfinished;
//! `OPENBF_DEV=0` turns it off), and `OPENBF_MATCHLOG=0` turns only it off.
//! The file is `<black box dir>/matches/<YYYYmmdd-HHMMSS>_<map>.jsonl`
//! (`blackbox::default_dir`: `engine/artifacts/playtest/matches/` when the
//! game runs from `engine/` or the repository root), written once at the
//! round's outcome and, if the battle ends before one, on quit / leaving it.
//! A headless round writes one for its human proxy (`round_to_outcome`'s
//! `MATCHLOG_DIR`).
//!
//! JSON lines. The first is the header object (`"kind": "header"`): map,
//! mission, era, factions, the subject's unit slot, team and kind (`human` /
//! `proxy`), difficulty, seed, build and checkout commits. A human's log
//! follows the player's unit (`Sim::players[0]`) through the side select and
//! every respawn, and its header's `unit` / `team` are the unit and team he
//! last spawned as (schema 2; schema 1 kept the team before the side select).
//! Then one array per row, the first item its kind, ticks at 30 Hz:
//! - `["spawn", tick, class, team]` (class: `names.classes[team]` index);
//! - `["shot", tick, weapon]` (a press, `Event::Shot`; weapon: `names.weapons`);
//! - `["hit", tick, shooter, victim, victim_gen, victim_team, damage,
//!   health_left, direct]` for a hit the subject deals or takes (direct 1 =
//!   a bolt, 0 = a blast);
//! - `["kill", tick, killer, victim, victim_team]` with the subject at one
//!   end; `["death", tick, cause]` an environmental death of his;
//! - `["seen", tick, enemy, enemy_gen]`: an enemy he SEES (within 60 m,
//!   40 deg of his aim, a clear line from his muzzle; tested every 3 ticks)
//!   after not seeing it for 5 s or ever - the start of an engagement;
//! - `["s", tick, alive, x, y, z, yaw, stance, side, fwd, lx, ly, seat]`
//!   every [`SAMPLE`] ticks: position, aim yaw, stance code
//!   (`unit_probe::stance_code`), the ground speed across and along his
//!   facing (m/s), the move stick (-32767..32767; 0 for a bot), -1 on foot
//!   else the vehicle slot.
//!
//! The last line is the footer (`"kind": "end"`): why it was written
//! (`outcome` / `quit`), the tick, the winner (null: none yet), both teams'
//! tickets, `cheats` = every cheat that was ON at any tick of the battle
//! (`Sim::cheats`, polled every tick; empty = a clean match), and the names
//! the rows index.
//!
//! The rows are fixed-size and kept in a vector reserved for a long match,
//! so a tick does not allocate; the file is encoded only when written.

use crate::walk::WalkSim;
use core::sim::{Cheats, Event, Sim};
use serde::Serialize;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Ticks between two samples (6 per second).
pub const SAMPLE: u64 = 5;
/// The sight test: within this far...
pub const SIGHT_M: f32 = crate::blackbox::session::SIGHT_M;
/// ...this close to the aim (half angle, degrees)...
pub const SIGHT_DEG: f32 = crate::blackbox::session::SIGHT_DEG;
/// ...and unseen this long before a sighting counts again (s).
pub const RESEEN_S: f32 = crate::blackbox::session::FORGET_S;
const SIGHT_EVERY: u64 = 3;
use crate::blackbox::session::SIGHT_COS;
/// Rows reserved up front: a 30-minute battle's samples and events.
const RESERVE: usize = 60_000;

/// Is the match log on: dev mode, and not `OPENBF_MATCHLOG=0`.
pub fn switch(dev: Option<&str>, matchlog: Option<&str>) -> bool {
    crate::dev::flag(dev) && matchlog != Some("0")
}

/// [`switch`], from the environment.
pub fn on() -> bool {
    switch(
        std::env::var("OPENBF_DEV").ok().as_deref(),
        std::env::var("OPENBF_MATCHLOG").ok().as_deref(),
    )
}

/// One row (see the module header).
#[derive(Debug, Clone, Copy)]
enum Row {
    Spawn(u64, u8, u8),
    Shot(u64, u32),
    Hit {
        tick: u64,
        shooter: u32,
        victim: u32,
        gen: u32,
        team: u8,
        damage: f32,
        left: f32,
        direct: bool,
    },
    Kill(u64, u32, u32, u8),
    Death(u64, u8),
    Seen(u64, u32, u32),
    Sample {
        tick: u64,
        alive: bool,
        pos: [f32; 3],
        yaw: f32,
        stance: u8,
        side: f32,
        fwd: f32,
        lx: i16,
        ly: i16,
        seat: i32,
    },
}

fn r2(v: f32) -> f64 {
    if v.is_finite() {
        (f64::from(v) * 100.0).round() / 100.0
    } else {
        0.0
    }
}

impl Row {
    fn json(&self) -> serde_json::Value {
        use serde_json::json;
        match *self {
            Row::Spawn(t, c, team) => json!(["spawn", t, c, team]),
            Row::Shot(t, w) => json!(["shot", t, w]),
            Row::Hit {
                tick,
                shooter,
                victim,
                gen,
                team,
                damage,
                left,
                direct,
            } => json!([
                "hit",
                tick,
                shooter,
                victim,
                gen,
                team,
                r2(damage),
                r2(left),
                u8::from(direct)
            ]),
            Row::Kill(t, k, v, team) => json!(["kill", t, k, v, team]),
            Row::Death(t, c) => json!(["death", t, c]),
            Row::Seen(t, e, g) => json!(["seen", t, e, g]),
            Row::Sample {
                tick,
                alive,
                pos,
                yaw,
                stance,
                side,
                fwd,
                lx,
                ly,
                seat,
            } => json!([
                "s",
                tick,
                u8::from(alive),
                r2(pos[0]),
                r2(pos[1]),
                r2(pos[2]),
                r2(yaw),
                stance,
                r2(side),
                r2(fwd),
                lx,
                ly,
                seat
            ]),
        }
    }
}

/// The header line.
#[derive(Debug, Clone, Serialize)]
pub struct Header {
    pub kind: &'static str,
    pub schema: u32,
    pub map: String,
    pub mission: String,
    pub era: String,
    pub source_game: String,
    pub factions: [String; 2],
    /// `human` (the windowed game) or `proxy` (a headless round).
    pub subject: String,
    pub unit: u32,
    pub team: u8,
    pub difficulty: String,
    pub seed: u32,
    pub build_commit: String,
    pub build_dirty: bool,
    pub commit: String,
    pub dirty: bool,
    /// Wall clock at the start, seconds since the epoch.
    pub started: u64,
    pub tick_hz: u32,
    /// `OPENBF_*` / `SWEEP_*` settings of the run.
    pub env: Vec<(String, String)>,
}

/// The recorder.
pub struct MatchLog {
    header: Header,
    names: crate::blackbox::Names,
    rows: Vec<Row>,
    /// Per enemy slot: (generation, last tick seen).
    seen: Vec<(u32, u64)>,
    /// Per cheat registry row (`Cheats::entries`): on at any tick.
    cheats_ever: Vec<bool>,
    alive: bool,
    gen: u32,
    /// A human's log: follow `Sim::players[0]` (the unit the player plays).
    follow_player: bool,
    path: PathBuf,
    written: bool,
}

/// `YYYYmmdd-HHMMSS` (UTC) of `secs` since the epoch.
pub fn stamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil from days (H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

impl MatchLog {
    /// A log of unit slot `unit` (`subject`: `human` / `proxy`) in `walk`'s
    /// battle, to be written into `dir`. A `human` log follows player slot
    /// 0's unit from then on. `meta` is the black box's run
    /// header (map, mission, difficulty, commits).
    pub fn start(
        meta: &crate::blackbox::Meta,
        walk: &WalkSim,
        unit: u32,
        subject: &str,
        era: &str,
        dir: &Path,
    ) -> MatchLog {
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let names = crate::blackbox::names_of(walk);
        let team = walk.sim.units.get(unit as usize).map_or(0, |u| u.team);
        let mut env = meta.env.clone();
        env.extend(
            std::env::vars().filter(|(k, _)| k.starts_with("SWEEP_") || k.starts_with("ARENA_")),
        );
        env.sort();
        let header = Header {
            kind: "header",
            schema: 2,
            map: meta.map.clone(),
            mission: meta.mission.clone(),
            era: era.to_owned(),
            source_game: meta.source_game.clone(),
            factions: names.factions.clone(),
            subject: subject.to_owned(),
            unit,
            team,
            difficulty: meta.difficulty.clone(),
            seed: meta.seed,
            build_commit: meta.build_commit.clone(),
            build_dirty: meta.build_dirty,
            commit: meta.commit.clone(),
            dirty: meta.dirty,
            started,
            tick_hz: core::TICK_HZ,
            env,
        };
        let path = dir.join(format!("{}_{}.jsonl", stamp(started), meta.map));
        MatchLog {
            header,
            names,
            rows: Vec::with_capacity(RESERVE),
            seen: Vec::with_capacity(256),
            cheats_ever: vec![false; Cheats::default().entries().len()],
            alive: false,
            gen: 0,
            follow_player: subject == "human",
            path,
            written: false,
        }
    }

    /// Write to `path` instead (a headless run's own name).
    pub fn set_path(&mut self, path: PathBuf) {
        self.path = path;
    }

    /// Where the file goes.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Record the tick the sim just stepped; `stick`: the human's move stick.
    /// Writes the file at the round's outcome (once).
    pub fn tick(&mut self, sim: &Sim, stick: (i16, i16)) {
        let t = sim.tick;
        self.follow(sim);
        let i = self.header.unit as usize;
        for (k, (_, _, on)) in sim.cheats.entries().iter().enumerate() {
            if let Some(e) = self.cheats_ever.get_mut(k) {
                *e |= *on;
            }
        }
        let Some(u) = sim.units.get(i) else { return };
        let me = i as u32;
        for ev in &sim.events {
            match *ev {
                Event::Shot {
                    unit, weapon_class, ..
                } if unit.index == me => self.rows.push(Row::Shot(t, weapon_class)),
                Event::Hit {
                    shooter,
                    victim,
                    damage,
                    age,
                    health_left,
                    ..
                } if shooter.index == me || victim.index == me => {
                    self.rows.push(Row::Hit {
                        tick: t,
                        shooter: shooter.index,
                        victim: victim.index,
                        gen: victim.gen,
                        team: sim.units.get(victim.index as usize).map_or(255, |v| v.team),
                        damage,
                        left: health_left,
                        direct: age >= 0.0,
                    });
                }
                Event::Kill { shooter, victim } if shooter.index == me || victim.index == me => {
                    let team = sim.units.get(victim.index as usize).map_or(255, |v| v.team);
                    self.rows
                        .push(Row::Kill(t, shooter.index, victim.index, team));
                }
                Event::EnvDeath { unit, cause } if unit.index == me => {
                    self.rows.push(Row::Death(t, cause as u8));
                }
                _ => {}
            }
        }
        // A life begins: the spawn row with its class and team, and the team
        // he plays is the header's.
        if u.alive && (!self.alive || u.gen != self.gen) {
            self.rows.push(Row::Spawn(t, u.spec.class, u.team));
            self.gen = u.gen;
            self.header.team = u.team;
        }
        self.alive = u.alive;
        if t.is_multiple_of(SAMPLE) {
            let v = u.soldier.vel;
            let (_, right, _) = u.aim_frame();
            let ahead = glam::Vec3::new(right.z, 0.0, -right.x);
            let flat = glam::Vec3::new(v.x, 0.0, v.z);
            let p = u.soldier.pos;
            self.rows.push(Row::Sample {
                tick: t,
                alive: u.alive,
                pos: [p.x, p.y, p.z],
                yaw: u.soldier.yaw,
                stance: crate::unit_probe::stance_code(u.soldier.stance),
                side: flat.dot(right),
                fwd: flat.dot(ahead),
                lx: stick.0,
                ly: stick.1,
                seat: u.mount.map_or(-1, |m| m.vehicle.index as i32),
            });
        }
        if u.alive && t.is_multiple_of(SIGHT_EVERY) {
            self.sight(sim, i, t);
        }
    }

    /// A human's log: the unit is the player's current one (`players[0]`),
    /// so a slot change starts a new life in the rows; with none (between
    /// rounds) the last one stays.
    fn follow(&mut self, sim: &Sim) {
        if !self.follow_player {
            return;
        }
        let Some(Some(h)) = sim.players.first() else {
            return;
        };
        if h.index != self.header.unit {
            self.header.unit = h.index;
            self.alive = false;
        }
    }

    /// The header's team before any spawn row: the subject unit's team now
    /// (the side select may have moved it).
    fn settle_team(&mut self, sim: &Sim) {
        let spawned = self.rows.iter().any(|r| matches!(r, Row::Spawn(..)));
        if !spawned {
            if let Some(u) = sim.units.get(self.header.unit as usize) {
                self.header.team = u.team;
            }
        }
    }

    /// The header as it will be written (the unit and team followed so far).
    pub fn header(&self) -> &Header {
        &self.header
    }

    fn sight(&mut self, sim: &Sim, i: usize, t: u64) {
        let u = &sim.units[i];
        let eye = u.muzzle();
        let (aim, _, _) = u.aim_frame();
        let cos = SIGHT_COS;
        let reseen = (RESEEN_S * core::TICK_HZ as f32) as u64;
        if self.seen.len() < sim.units.len() {
            self.seen.resize(sim.units.len(), (u32::MAX, 0));
        }
        for (h, o) in sim.alive_units() {
            if o.team == u.team || o.team > 1 {
                continue;
            }
            let c = o.capsule().center();
            let to = c - eye;
            let d = to.length();
            if !(1e-3..=SIGHT_M).contains(&d) || aim.dot(to / d) < cos {
                continue;
            }
            if !core::perception::los(sim, eye, c) {
                continue;
            }
            let s = &mut self.seen[h.index as usize];
            if s.0 != h.gen || t > s.1 + reseen {
                self.rows.push(Row::Seen(t, h.index, h.gen));
            }
            *s = (h.gen, t);
        }
    }

    /// Write the file (`reason`: `outcome` / `quit`), once. Returns where.
    pub fn write(&mut self, sim: &Sim, reason: &str) -> Option<PathBuf> {
        if self.written {
            return None;
        }
        self.written = true;
        self.follow(sim);
        self.settle_team(sim);
        let cheats: Vec<&str> = sim
            .cheats
            .entries()
            .iter()
            .zip(self.cheats_ever.iter().copied())
            .filter(|(_, on)| *on)
            .map(|((id, _, _), _)| *id)
            .collect();
        let tickets: Vec<u32> = sim.battle.teams.iter().map(|t| t.tickets()).collect();
        let footer = serde_json::json!({
            "kind": "end",
            "reason": reason,
            "tick": sim.tick,
            "winner": sim.outcome,
            "tickets": tickets,
            "cheats": cheats,
            "names": {
                "weapons": self.names.weapons,
                "classes": self.names.classes,
                "factions": self.names.factions,
            },
        });
        let mut out = Vec::with_capacity(self.rows.len() * 40 + 4096);
        let _ = serde_json::to_writer(&mut out, &self.header);
        out.push(b'\n');
        for r in &self.rows {
            let _ = serde_json::to_writer(&mut out, &r.json());
            out.push(b'\n');
        }
        let _ = serde_json::to_writer(&mut out, &footer);
        out.push(b'\n');
        if let Some(d) = self.path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        match std::fs::File::create(&self.path).and_then(|mut f| f.write_all(&out)) {
            Ok(()) => Some(self.path.clone()),
            Err(e) => {
                eprintln!("matchlog: {}: {e}", self.path.display());
                None
            }
        }
    }

    pub fn written(&self) -> bool {
        self.written
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_and_the_stamp() {
        assert!(switch(Some("1"), None));
        assert!(!switch(Some("1"), Some("0")));
        assert!(!switch(Some("0"), None));
        assert_eq!(stamp(0), "19700101-000000");
        assert_eq!(stamp(1_793_232_000), "20261029-000000");
    }

    /// A battle's rows: the spawn, a sample, the footer with a cheat that was
    /// on for one tick only, and the file reads back as JSON lines.
    #[test]
    fn a_match_writes_header_rows_and_footer() {
        let mut walk = WalkSim::new(7, glam::Vec3::ZERO, None);
        let meta = crate::blackbox::Meta {
            map: "test".into(),
            difficulty: "normal".into(),
            ..Default::default()
        };
        let dir = std::env::temp_dir().join(format!("openbf_matchlog_{}", std::process::id()));
        let mut log = MatchLog::start(&meta, &walk, walk.player.index, "human", "gcw", &dir);
        let sim = &mut walk.sim;
        sim.tick = 10;
        log.tick(sim, (0, 32767));
        sim.cheats.infinite_ammo = true;
        sim.tick = 11;
        log.tick(sim, (0, 0));
        sim.cheats.infinite_ammo = false;
        let path = log.write(sim, "quit").expect("written");
        assert!(log.write(sim, "quit").is_none(), "once");
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines[0]["kind"], "header");
        assert_eq!(lines[1][0], "spawn");
        assert_eq!(lines[2][0], "s");
        assert_eq!(lines[2][11], 32767);
        let end = lines.last().unwrap();
        assert_eq!(end["kind"], "end");
        assert_eq!(end["cheats"], serde_json::json!(["ammo"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A playtest (2026-10-28): the player took the other side at the side
    /// select, and the log kept the team he had before it ("all", 0 hits,
    /// "WON" though he lost). The log follows `players[0]` and records the
    /// team he played, through the side pick, a respawn and a slot change.
    #[test]
    fn a_human_log_follows_the_players_unit_and_the_team_he_played() {
        let mut walk = WalkSim::new(7, glam::Vec3::ZERO, None);
        let meta = crate::blackbox::Meta {
            map: "test".into(),
            ..Default::default()
        };
        let dir =
            std::env::temp_dir().join(format!("openbf_matchlog_follow_{}", std::process::id()));
        let p = walk.player.index as usize;
        assert_eq!(walk.sim.units[p].team, 0);
        let mut log = MatchLog::start(&meta, &walk, walk.player.index, "human", "gcw", &dir);
        assert_eq!(log.header().team, 0);
        // The side select moves him to team 1 before his first life.
        walk.sim.units[p].alive = false;
        walk.sim.tick = 1;
        log.tick(&walk.sim, (0, 0));
        walk.sim.units[p].team = 1;
        walk.sim.units[p].spec.team = 1;
        walk.sim.units[p].alive = true;
        walk.sim.tick = 2;
        log.tick(&walk.sim, (0, 0));
        assert_eq!(log.header().team, 1);
        // A respawn: a new generation in the same slot, a new spawn row.
        walk.sim.units[p].gen += 1;
        walk.sim.players[0] = Some(walk.sim.handle(p));
        walk.sim.tick = 3;
        log.tick(&walk.sim, (0, 0));
        // His unit moves to another slot (on team 1): the log goes with it,
        // and his hits from there are his.
        let other = walk
            .sim
            .spawn(&core::unit::UnitSpec::trooper(1, glam::Vec3::X * 5.0))
            .unwrap();
        walk.sim.players[0] = Some(other);
        let foe = walk
            .sim
            .spawn(&core::unit::UnitSpec::trooper(0, glam::Vec3::X * 9.0))
            .unwrap();
        walk.sim.tick = 4;
        walk.sim.events.push(Event::Hit {
            shooter: other,
            victim: foe,
            damage: 10.0,
            age: 0.1,
            health_left: 90.0,
            dir: glam::Vec3::X,
        });
        log.tick(&walk.sim, (0, 0));
        assert_eq!(log.header().unit, other.index);
        let path = log.write(&walk.sim, "quit").expect("written");
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines[0]["schema"], 2);
        assert_eq!(lines[0]["team"], 1);
        assert_eq!(lines[0]["unit"], other.index);
        let spawns: Vec<_> = lines.iter().filter(|l| l[0] == "spawn").collect();
        assert_eq!(spawns.len(), 3, "{spawns:?}");
        assert!(spawns.iter().all(|s| s[3] == 1), "{spawns:?}");
        let hits: Vec<_> = lines.iter().filter(|l| l[0] == "hit").collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0][2], other.index);
        assert_eq!(hits[0][5], 0, "the victim is on the other team");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A proxy's log stays on its own unit.
    #[test]
    fn a_proxy_log_keeps_its_unit() {
        let mut walk = WalkSim::new(7, glam::Vec3::ZERO, None);
        let bot = walk
            .sim
            .spawn(&core::unit::UnitSpec::trooper(1, glam::Vec3::X * 5.0))
            .unwrap();
        let meta = crate::blackbox::Meta::default();
        let dir = std::env::temp_dir();
        let mut log = MatchLog::start(&meta, &walk, bot.index, "proxy", "gcw", &dir);
        walk.sim.tick = 1;
        log.tick(&walk.sim, (0, 0));
        assert_eq!((log.header().unit, log.header().team), (bot.index, 1));
    }
}
