//! Player profile: settings + career stats, autosaved everywhere (PLAN §Saves,
//! sessions & host authority: "each user profile autosaves continuously").
//! v0 = one RON file; awards/unlock progression and per-mode stats grow in
//! here (P8). Cheats are deliberately NOT persisted (session-scoped by design).
//!
//! Location: `$OPENBF_PROFILE` (tests/dev override) else `profile.ron` in
//! [`data_dir`], `openbf_data/` beside the game file (the maintainer, 2026-10-21: on every
//! OS, nothing in %APPDATA% or ~/.config).
//! Saves are atomic (write `.tmp`, rename).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    /// BF1 aim-assist intensity slider (1.0 = PS2-authentic).
    pub assist_intensity: f32,
    /// Difficulty tier for the next round (PLAN §Saves: host authority). The
    /// `--difficulty` CLI flag WINS over this for the session it is given in.
    #[serde(default)]
    pub difficulty: core::skill::Difficulty,
    /// Aim assist on/off. **THE MAINTAINER'S RULE (2026-09-03): no difficulty tier ever
    /// turns this off.** The PS2 BF2's difficulty screen clears `bAutoAim`,
    /// `bAimAssist` and `bStickyReticule` when Elite is picked
    /// (`analysis/re-ai-difficulty.md` §9.3) — that is HISTORY here, not
    /// behaviour: the port's assist is the player's own switch at every tier.
    #[serde(default = "yes")]
    pub aim_assist: bool,
    /// Host option: the tier's human damage multiplier (D12).
    #[serde(default = "yes")]
    pub human_damage_on: bool,
    /// Host option: the Elite team handicap (D12).
    #[serde(default = "yes")]
    pub team_handicap_on: bool,
    /// FOV option: a multiplier on the PS2 chase camera's vertical FOV (1.0 =
    /// the PS2's framing; `game::walk::FOV_SCALE_RANGE`). Hor+ either way.
    #[serde(default = "one")]
    pub fov_scale: f32,
    /// The music volume (card MUSV): a factor on the disc record's own gain,
    /// `game::music::VOLUME_RANGE`; 0 = no music.
    #[serde(default = "one")]
    pub music_volume: f32,
    #[serde(default = "one")]
    pub mouse_sensitivity: f32,
    #[serde(default)]
    pub invert_y: bool,
    /// Alive-AI cap override (None = the mission's authentic cap).
    pub ai_alive_cap: Option<u32>,
    pub last_map: Option<String>,
    pub last_mode: Option<String>,
    pub last_teams: Option<[String; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            assist_intensity: 1.0,
            difficulty: core::skill::Difficulty::default(),
            aim_assist: true,
            human_damage_on: true,
            team_handicap_on: true,
            fov_scale: 1.0,
            music_volume: 1.0,
            mouse_sensitivity: 1.0,
            invert_y: false,
            ai_alive_cap: None,
            last_map: None,
            last_mode: None,
            last_teams: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Career {
    pub rounds: u32,
    pub wins: u32,
    pub kills: u32,
    pub deaths: u32,
    pub shots: u64,
    pub hits: u64,
    pub captures: u32,
}

/// Where the game keeps what it writes (the profile, the playtest snapshots of
/// a packed build): `openbf_data/` beside the running game file, on every OS.
pub fn data_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.join("openbf_data")))
        .unwrap_or_else(|| PathBuf::from("openbf_data"))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub name: String,
    pub settings: Settings,
    pub career: Career,
}

impl Profile {
    pub fn path() -> PathBuf {
        if let Some(p) = std::env::var_os("OPENBF_PROFILE") {
            return PathBuf::from(p);
        }
        data_dir().join("profile.ron")
    }

    /// Load the profile, or a fresh default one (also when the file is
    /// unreadable — a corrupt profile must never block play; it is backed up).
    pub fn load_or_default() -> Profile {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(txt) => match ron::from_str::<Profile>(&txt) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!(
                        "profile {} unreadable ({e}); starting fresh",
                        path.display()
                    );
                    let _ = std::fs::rename(&path, path.with_extension("ron.bad"));
                    Profile::fresh()
                }
            },
            Err(_) => Profile::fresh(),
        }
    }

    fn fresh() -> Profile {
        Profile {
            name: std::env::var("USER").unwrap_or_else(|_| "player".into()),
            ..Default::default()
        }
    }

    /// Atomic save.
    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
        }
        let txt = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())?;
        let tmp = path.with_extension("ron.tmp");
        std::fs::write(&tmp, txt).with_context(|| tmp.display().to_string())?;
        std::fs::rename(&tmp, &path).with_context(|| path.display().to_string())?;
        Ok(())
    }

    /// Fold a finished (or abandoned) round's player stats into the career.
    pub fn record_round(
        &mut self,
        won: Option<bool>,
        kills: u32,
        deaths: u32,
        shots: u64,
        hits: u64,
    ) {
        self.career.rounds += 1;
        if won == Some(true) {
            self.career.wins += 1;
        }
        self.career.kills += kills;
        self.career.deaths += deaths;
        self.career.shots += shots;
        self.career.hits += hits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_profile_reads_the_mouse_defaults() {
        let s: Settings = ron::from_str("(assist_intensity: 0.5, fov_scale: 1.1)").unwrap();
        assert_eq!((s.mouse_sensitivity, s.invert_y), (1.0, false));
        assert_eq!(s.assist_intensity, 0.5);
    }

    #[test]
    fn round_trips_and_survives_corruption() {
        let dir = std::env::temp_dir().join(format!("openbf-profile-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("profile.ron");
        // SAFETY: test-only env override, single-threaded use of this var.
        unsafe { std::env::set_var("OPENBF_PROFILE", &path) };
        assert_eq!(Profile::path(), path);
        let mut p = Profile::load_or_default();
        assert_eq!(p.career, Career::default());
        p.settings.assist_intensity = 0.7;
        p.settings.last_map = Some("tat2".into());
        p.record_round(Some(true), 12, 3, 240, 90);
        p.save().expect("save");
        let back = Profile::load_or_default();
        assert_eq!(back, p);
        assert_eq!(back.career.wins, 1);
        // Corrupt file → fresh profile, original kept as .bad.
        std::fs::write(&path, "(not ron").unwrap();
        let fresh = Profile::load_or_default();
        assert_eq!(fresh.career, Career::default());
        assert!(path.with_extension("ron.bad").exists());
        unsafe { std::env::remove_var("OPENBF_PROFILE") };
        let _ = std::fs::remove_dir_all(&dir);
    }
}
