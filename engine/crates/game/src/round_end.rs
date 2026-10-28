//! The end of a round (issue #33, `analysis/re-round-end.md` §2-§3): one flow
//! on every map, BF1's. When the round is decided the view goes to a random
//! camera shot of the mission and the victory / defeat banner stands over it
//! (5 s at least, 7 s at most, not past the music's end: BF1 §2.1); then the
//! stats screens, flipped through with accept (the PS2's X) - team stats,
//! personal stats, awards - and left with back (Triangle) on the team stats,
//! accept on the last, or 30 s with no input (§3, both games); leaving goes
//! back to the shell.

use crate::spawn_backdrop::{CameraShot, ShotPicker};

/// The banner's shortest and longest stay (BF1 `FUN_002ae6c8`, §2.1).
pub const BANNER_MIN_SECS: f32 = 5.0;
pub const BANNER_MAX_SECS: f32 = 7.0;
/// A stats screen with no input this long quits by itself (§3, both games).
pub const STATS_IDLE_SECS: f32 = 30.0;

/// Where the end of the round stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The banner over the camera shot.
    Banner,
    /// One of the stats screens.
    Stats(Screen),
    /// Left: back to the shell.
    Done,
}

/// BF1's stats screens, in order (§3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Team,
    Personal,
    Awards,
}

/// How the round went for the viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Result {
    Victory,
    Defeat,
    /// The time limit ran out on equal reinforcements: both teams win.
    Draw,
}

#[derive(Clone, Debug)]
pub struct RoundEnd {
    pub result: Result,
    pub phase: Phase,
    /// The camera shot the view stands on; None when the mission has none
    /// (the battle's view stays).
    pub shot: Option<CameraShot>,
    banner_secs: f32,
    /// Seconds in the current phase, and since the last input on a stats
    /// screen.
    pub secs: f32,
    idle: f32,
}

impl RoundEnd {
    /// The round just ended: draw a shot from `shots` (the picker's own draw,
    /// seeded) and stand the banner for the music's length within 5-7 s
    /// (7 s with no music).
    pub fn start(result: Result, shots: &[CameraShot], seed: u32, music_secs: Option<f32>) -> Self {
        let shot = (!shots.is_empty()).then(|| {
            let mut p = ShotPicker::new(seed);
            shots[p.next(shots.len()).min(shots.len() - 1)]
        });
        Self {
            result,
            phase: Phase::Banner,
            shot,
            banner_secs: music_secs
                .unwrap_or(BANNER_MAX_SECS)
                .clamp(BANNER_MIN_SECS, BANNER_MAX_SECS),
            secs: 0.0,
            idle: 0.0,
        }
    }

    /// One frame: `accept` / `back` are this frame's presses.
    pub fn step(&mut self, dt: f32, accept: bool, back: bool) {
        self.secs += dt;
        let next = match self.phase {
            Phase::Banner => (self.secs >= self.banner_secs).then_some(Phase::Stats(Screen::Team)),
            Phase::Stats(s) => {
                self.idle = if accept || back { 0.0 } else { self.idle + dt };
                if self.idle >= STATS_IDLE_SECS {
                    Some(Phase::Done)
                } else if accept {
                    Some(match s {
                        Screen::Team => Phase::Stats(Screen::Personal),
                        Screen::Personal => Phase::Stats(Screen::Awards),
                        Screen::Awards => Phase::Done,
                    })
                } else if back {
                    Some(match s {
                        Screen::Team => Phase::Done,
                        Screen::Personal => Phase::Stats(Screen::Team),
                        Screen::Awards => Phase::Stats(Screen::Personal),
                    })
                } else {
                    None
                }
            }
            Phase::Done => None,
        };
        if let Some(p) = next {
            self.phase = p;
            self.secs = 0.0;
            self.idle = 0.0;
        }
    }

    pub fn done(&self) -> bool {
        self.phase == Phase::Done
    }
}

#[cfg(test)]
mod flow_tests {
    use super::*;

    fn shots() -> Vec<CameraShot> {
        (0..4)
            .map(|i| CameraShot::from_call([1.0, 0.0, 0.0, 0.0, i as f32 * 10.0, 5.0, 0.0]))
            .collect()
    }

    /// The banner stands for the music within 5-7 s (7 with none), then the
    /// team stats; accept flips to personal, awards, and out; back steps
    /// back, and quits from the team stats.
    #[test]
    fn the_banner_then_the_stats_screens_then_out() {
        for (music, want) in [
            (None, 7.0),
            (Some(3.0), 5.0),
            (Some(6.2), 6.2),
            (Some(20.0), 7.0),
        ] {
            let mut r = RoundEnd::start(Result::Victory, &shots(), 7, music);
            let mut t = 0.0f32;
            while r.phase == Phase::Banner {
                r.step(1.0 / 30.0, false, false);
                t += 1.0 / 30.0;
            }
            assert!((t - want).abs() < 0.05, "music {music:?}: {t}");
            assert_eq!(r.phase, Phase::Stats(Screen::Team));
        }
        let mut r = RoundEnd::start(Result::Defeat, &shots(), 7, Some(5.0));
        r.step(5.1, false, false);
        r.step(0.1, true, false);
        assert_eq!(r.phase, Phase::Stats(Screen::Personal));
        r.step(0.1, false, true);
        assert_eq!(r.phase, Phase::Stats(Screen::Team));
        r.step(0.1, true, false);
        r.step(0.1, true, false);
        assert_eq!(r.phase, Phase::Stats(Screen::Awards));
        r.step(0.1, true, false);
        assert!(r.done());
        let mut r = RoundEnd::start(Result::Draw, &shots(), 7, None);
        r.step(7.1, false, false);
        r.step(0.1, false, true);
        assert!(r.done(), "back on the team stats quits");
    }

    /// 30 s with no input on a stats screen quits; any press restarts the
    /// count. A mission with no shots keeps the battle's view.
    #[test]
    fn an_idle_stats_screen_quits_after_30_s() {
        let mut r = RoundEnd::start(Result::Victory, &shots(), 1, None);
        assert!(r.shot.is_some());
        r.step(7.1, false, false);
        for _ in 0..(29 * 30) {
            r.step(1.0 / 30.0, false, false);
        }
        assert!(!r.done());
        r.step(0.0, false, false);
        r.step(1.0 / 30.0, true, false);
        assert_eq!(
            r.phase,
            Phase::Stats(Screen::Personal),
            "a press restarts it"
        );
        for _ in 0..(31 * 30) {
            r.step(1.0 / 30.0, false, false);
        }
        assert!(r.done());
        assert!(RoundEnd::start(Result::Victory, &[], 1, None)
            .shot
            .is_none());
    }
}

#[cfg(test)]
mod tests {
    use crate::round_end::{Phase, Result, Screen};
    use crate::spawn_backdrop::CameraShot;
    use crate::walk::WalkSim;

    /// Issue #33 in the walk: nothing before the round is decided; then the
    /// view stands on a mission shot and the viewer's result is drawn, the
    /// battle's own camera again with no shots; the stats screens follow and
    /// a run of accepts leaves.
    #[test]
    fn the_walk_ends_its_round_on_a_mission_shot() {
        let mut w = WalkSim::new(7, glam::Vec3::ZERO, None);
        let shot = CameraShot::from_call([1.0, 0.0, 0.0, 0.0, 40.0, 25.0, -10.0]);
        w.end_shots = vec![shot];
        assert!(!w.follow_round_end(0.1, false, false, None));
        assert!(w.round_end.is_none(), "not before the round is decided");
        let me = w.sim.units[w.player.index as usize].team;
        w.sim.outcome = Some(me);
        w.follow_round_end(1.0 / 30.0, false, false, None);
        let r = w.round_end.as_ref().unwrap();
        assert_eq!((r.result, r.phase), (Result::Victory, Phase::Banner));
        assert_eq!(w.camera().position, shot.camera().position);
        w.follow_round_end(7.1, false, false, None);
        assert_eq!(
            w.round_end.as_ref().unwrap().phase,
            Phase::Stats(Screen::Team)
        );
        let mut done = false;
        for _ in 0..3 {
            done = w.follow_round_end(0.1, true, false, None);
        }
        assert!(done, "accept through team, personal, awards: out");
        let mut w = WalkSim::new(7, glam::Vec3::ZERO, None);
        w.sim.outcome = Some(1 - me.min(1));
        let before = w.camera().position;
        w.follow_round_end(0.1, false, false, None);
        assert_eq!(w.round_end.as_ref().unwrap().result, Result::Defeat);
        assert_eq!(w.camera().position, before, "no shots: the battle's view");
    }
}
