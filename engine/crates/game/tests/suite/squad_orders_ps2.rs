//! Card SQACK's PS2 scenes run on the port (`analysis/re-squad-orders.md`
//! section 6; the PS2 side is `lead_tmp/sqack/scenes/SUMMARY.txt`): the
//! player a CIS soldier on flat ground, N AI teammates on a 5 m ring, an
//! order, 10 s. Each scene prints its row; the asserts are the card's
//! acceptance list.
//!
//! The scenes script the rule's inputs (who stands where, what is pressed);
//! the walking, the thinking and the voice queue are the game's own
//! (`WalkSim::step_tick`, `Commander::frame`, `event_sounds::each`).

use core::sim::{buttons, Event, PadState, Sim};
use core::squad::CommandKind;
use core::unit::{Controller, UnitSpec};
use game::announce::{battle_lines, Commander};
use game::walk::{TeamTemplate, WalkSim};
use glam::Vec3;

const HZ: u64 = 30;
/// A take's length: the PS2's are 0.5-1.4 s.
const TAKE_SECS: f32 = 1.0;

struct Scene {
    walk: WalkSim,
    /// (tick, name) of every line the voice started.
    heard: Vec<(u64, String, Option<u8>)>,
    /// (tick, name) of every `chatter_tac` event sound (the player's line).
    said: Vec<(u64, String)>,
    /// (tick, kind, units) of every order, (tick, unit) of every take.
    orders: Vec<(u64, u8, u8)>,
    takes: Vec<(u64, u32)>,
}

/// The player at the origin, `ring` teammates 5 m round him and `far` more
/// at 35 m (inside MOVE OUT's 45 m, outside HOLD's 30 m).
fn scene(ring: usize, far: usize) -> Scene {
    let mut sim = Sim::empty(11);
    sim.heightfield = None;
    let hf = core::heightfield::Heightfield {
        size: 2,
        unit: 4000.0,
        offset_x: 2000.0,
        offset_z: 2000.0,
        heights: vec![0.0; 4],
    };
    let mut teams = [0, 1].map(|_| TeamTemplate::default_trooper());
    teams[0].faction = "cis".into();
    teams[1].faction = "rep".into();
    let mut walk = WalkSim::with_teams(sim, Some(hf), Vec3::ZERO, teams, None);
    let bot = |walk: &mut WalkSim, at: Vec3| {
        walk.sim
            .spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(0, at)
            })
            .expect("bot");
    };
    for k in 0..ring {
        let a = k as f32 / ring as f32 * std::f32::consts::TAU;
        bot(&mut walk, Vec3::new(5.0 * a.cos(), 0.0, 5.0 * a.sin()));
    }
    for k in 0..far {
        bot(&mut walk, Vec3::new(-35.0, 0.0, 3.0 * k as f32));
    }
    let lines = battle_lines(
        &Default::default(),
        ["cis", "rep"],
        ["cis", "rep"],
        [200, 200],
    );
    walk.voice = Some(Commander::new(
        lines,
        ["cis", "rep"],
        0,
        Vec::new(),
        Vec::new(),
    ));
    Scene {
        walk,
        heard: Vec::new(),
        said: Vec::new(),
        orders: Vec::new(),
        takes: Vec::new(),
    }
}

impl Scene {
    fn step(&mut self, pad: PadState) {
        self.walk.step_tick(&[pad]);
        let tick = self.walk.sim.tick;
        for e in &self.walk.sim.events {
            match e {
                Event::Command { kind, units, .. } => self.orders.push((tick, *kind, *units)),
                Event::OrderTaken { unit, .. } => self.takes.push((tick, unit.index)),
                _ => {}
            }
        }
        let ears = self.pos(0);
        let said = &mut self.said;
        game::event_sounds::each(&self.walk, ears, |name, _| {
            if name.contains("chatter_tac") {
                said.push((tick, name.to_owned()));
            }
        });
        let mut voice = self.walk.voice.take().expect("voice");
        let heard = &mut self.heard;
        voice.frame(&self.walk.sim, self.walk.player, 1.0 / HZ as f32, |l| {
            heard.push((tick, l.name.clone(), l.take));
            Some(TAKE_SECS)
        });
        self.walk.voice = Some(voice);
    }

    fn run(&mut self, ticks: u64, pad: PadState) {
        for _ in 0..ticks {
            self.step(pad);
        }
    }

    fn press(&mut self, bit: u32) -> u64 {
        self.step(PadState {
            buttons: bit,
            ..PadState::default()
        });
        self.walk.sim.tick
    }

    fn pos(&self, i: usize) -> Vec3 {
        self.walk.sim.units[i].soldier.pos
    }

    fn dist(&self, i: usize) -> f32 {
        (self.pos(i) - self.pos(0)).length()
    }

    fn with(&self, follow: bool) -> Vec<usize> {
        (0..self.walk.sim.units.len())
            .filter(|&i| match self.walk.sim.units[i].command.map(|c| c.kind) {
                Some(CommandKind::Follow(_)) => follow,
                Some(CommandKind::Hold(_)) => !follow,
                _ => false,
            })
            .collect()
    }

    /// Acceptance 5 for the order pressed at `at`: the player's line at the
    /// press tick, the first answer within 0.5 s on an idle voice, one to
    /// three answers, no take twice. Returns the answers' seconds.
    fn lines(&self, at: u64, line: &str, until: u64, idle: bool) -> Vec<f32> {
        let mine: Vec<&str> = self
            .said
            .iter()
            .filter(|(t, _)| *t == at)
            .map(|(_, n)| n.as_str())
            .collect();
        assert_eq!(mine, [line], "the player's line at the press tick");
        let answers: Vec<&(u64, String, Option<u8>)> = self
            .heard
            .iter()
            .filter(|(t, _, _)| *t >= at && *t < until)
            .collect();
        assert!(answers
            .iter()
            .all(|(_, n, _)| n == "ai_cis_inf_com_chatter_tac_yes"));
        let secs: Vec<f32> = answers
            .iter()
            .map(|(t, _, _)| (*t - at) as f32 / HZ as f32)
            .collect();
        assert!((1..=3).contains(&secs.len()), "answers {secs:?}");
        if idle {
            assert!(secs[0] <= 0.5, "first answer after {} s", secs[0]);
        }
        let mut takes: Vec<u8> = answers.iter().filter_map(|(_, _, k)| *k).collect();
        takes.sort_unstable();
        takes.dedup();
        assert_eq!(takes.len(), secs.len(), "a take twice inside its lock");
        secs
    }
}

const IDLE: PadState = PadState {
    lx: 0,
    ly: 0,
    rx: 0,
    ry: 0,
    buttons: 0,
    mx: 0,
    my: 0,
};
const WALK: PadState = PadState {
    lx: 0,
    ly: i16::MAX,
    rx: 0,
    ry: 0,
    buttons: 0,
    mx: 0,
    my: 0,
};

fn taken_within(s: &Scene, at: u64) -> (usize, f32) {
    let t: Vec<u64> = s
        .takes
        .iter()
        .filter(|(t, _)| *t >= at)
        .map(|x| x.0)
        .collect();
    let last = t.iter().max().map_or(0.0, |t| (*t - at) as f32 / HZ as f32);
    (t.len(), last)
}

/// PS2 F3: FOLLOW ME, 3 near, the player walks 4 s. 3 receivers, 2 followers;
/// trailing 5-12 m while he walks, 3.2 / 3.8 m from him at 10 s.
#[test]
fn f3_follow_three_near_then_walk() {
    let mut s = scene(3, 0);
    s.run(30, IDLE);
    let at = s.press(buttons::SQUAD_FOLLOW);
    assert_eq!(s.orders, [(at, 0, 3)], "receivers");
    s.run(59, IDLE);
    let followers = s.with(true);
    assert_eq!(followers.len(), 2, "two of three follow");
    let d2: Vec<f32> = followers.iter().map(|&i| s.dist(i)).collect();
    s.run(90, WALK);
    let d5: Vec<f32> = followers.iter().map(|&i| s.dist(i)).collect();
    let walked = s.pos(0).length();
    s.run(30, WALK);
    s.run(120, IDLE);
    let d10: Vec<f32> = followers.iter().map(|&i| s.dist(i)).collect();
    let (n, last) = taken_within(&s, at);
    let secs = s.lines(at, "cis_inf_com_chatter_tac_follow", at + 300, true);
    println!(
        "F3  receivers 3 (taken by {last:.2} s)  followers {followers:?}  dist +2 s {d2:.1?} \
         +5 s {d5:.1?} (leader walked {walked:.1} m) +10 s {d10:.1?}  answers {secs:.2?}"
    );
    assert_eq!(n, 3, "every receiver answers");
    assert!(last <= 0.5);
    assert!(walked > 10.0, "the leader walked");
    assert!(d5.iter().all(|d| (5.0..=15.0).contains(d)), "{d5:?}");
    assert!(d10.iter().all(|d| *d <= 4.3), "{d10:?}");
    assert_eq!(s.with(true).len(), 2, "still following");
}

/// PS2 F5: FOLLOW ME, 5 near, the player stands. 5 receivers, 2 followers at
/// 1.7 / 2.9 m 10 s later, 3 answers.
#[test]
fn f5_follow_five_near() {
    let mut s = scene(5, 0);
    s.run(30, IDLE);
    let at = s.press(buttons::SQUAD_FOLLOW);
    assert_eq!(s.orders, [(at, 0, 5)], "receivers");
    s.run(300, IDLE);
    let followers = s.with(true);
    let d10: Vec<f32> = followers.iter().map(|&i| s.dist(i)).collect();
    let others: Vec<f32> = (1..=5)
        .filter(|i| !followers.contains(i))
        .map(|i| (s.pos(i) - s.pos(0)).length())
        .collect();
    let (n, last) = taken_within(&s, at);
    let secs = s.lines(at, "cis_inf_com_chatter_tac_follow", at + 300, true);
    println!(
        "F5  receivers 5 (taken by {last:.2} s)  followers {followers:?}  dist +10 s {d10:.1?}  \
         the other three {others:.1?}  answers {secs:.2?}"
    );
    assert_eq!(n, 5);
    assert_eq!(followers.len(), 2, "two of five follow");
    assert!(d10.iter().all(|d| *d <= 4.3), "{d10:?}");
}

/// PS2 H3: HOLD POSITION, 3 near. Every receiver holds, 0.1-0.3 m of
/// displacement in 10 s; released 9.5-9.8 s after the player passes 20 m.
#[test]
fn h3_hold_three_near_then_walk_away() {
    let mut s = scene(3, 1);
    s.run(30, IDLE);
    let at = s.press(buttons::SQUAD_HOLD);
    assert_eq!(s.orders, [(at, 1, 3)], "receivers: the one at 35 m is out");
    let start: Vec<Vec3> = (1..=3).map(|i| s.pos(i)).collect();
    s.run(300, IDLE);
    assert_eq!(s.with(false), [1, 2, 3], "every receiver holds");
    let moved: Vec<f32> = (1..=3)
        .map(|i| (s.pos(i) - start[i - 1]).length())
        .collect();
    let secs = s.lines(at, "cis_inf_com_chatter_tac_hold", at + 300, true);
    // The player walks away; each holder is released 10 s after he passes
    // 20 m from IT.
    let (mut passed, mut released) = ([None; 3], [None; 3]);
    for _ in 0..900 {
        s.step(WALK);
        for i in 0..3 {
            if passed[i].is_none() && s.dist(i + 1) > 20.0 {
                passed[i] = Some(s.walk.sim.tick);
            }
            if released[i].is_none() && s.walk.sim.units[i + 1].command.is_none() {
                released[i] = Some(s.walk.sim.tick);
            }
        }
    }
    let after: Vec<f32> = (0..3)
        .map(|i| (released[i].expect("released") - passed[i].expect("passed")) as f32 / HZ as f32)
        .collect();
    let still: Vec<f32> = (1..=3)
        .map(|i| (s.pos(i) - start[i - 1]).length())
        .collect();
    println!(
        "H3  receivers 3  holders 3  displacement in 10 s {moved:.2?}  answers {secs:.2?}  \
         released {after:.1?} s after the leader passed 20 m (moved {still:.2?} m until then)"
    );
    assert!(moved.iter().all(|d| *d <= 0.3), "{moved:?}");
    assert!(after.iter().all(|t| (9.0..=11.0).contains(t)), "{after:?}");
}

/// PS2 M3: FOLLOW ME, then MOVE OUT 3.7 s later. 4 receivers within 45 m,
/// the two followers back in their own goal 0.07-0.1 s after the press.
/// PS2 A3: HOLD POSITION, then AT EASE: 4 receivers, 3 holders released.
#[test]
fn m3_and_a3_the_release_orders() {
    for (name, first, second, line, n_ordered) in [
        ("M3", buttons::SQUAD_FOLLOW, buttons::SQUAD_MOVE_OUT, 3u8, 2),
        ("A3", buttons::SQUAD_HOLD, buttons::SQUAD_AT_EASE, 2u8, 3),
    ] {
        let mut s = scene(3, 1);
        s.run(30, IDLE);
        s.press(first);
        s.run(110, IDLE);
        let ordered: Vec<usize> = (1..=4)
            .filter(|&i| s.walk.sim.units[i].command.is_some())
            .collect();
        assert_eq!(ordered.len(), n_ordered, "{name}");
        let at = s.press(second);
        assert_eq!(s.orders.last(), Some(&(at, line, 4)), "{name}: receivers");
        let mut free_at = None;
        for _ in 0..300 {
            if free_at.is_none()
                && ordered
                    .iter()
                    .all(|&i| s.walk.sim.units[i].command.is_none())
            {
                free_at = Some(s.walk.sim.tick);
            }
            s.step(IDLE);
        }
        let free = (free_at.expect("released") - at) as f32 / HZ as f32;
        let (n, last) = taken_within(&s, at);
        // The voice may still be saying the first order's answers.
        let secs = s.lines(at, "cis_inf_com_chatter_tac_moveout", at + 300, false);
        println!(
            "{name}  receivers 4 (taken by {last:.2} s)  ordered {ordered:?} free after {free:.2} s  \
             answers {secs:.2?}"
        );
        assert_eq!(n, 4, "{name}: every receiver within 45 m answers");
        assert!(free <= 0.5, "{name}: released after {free} s");
    }
}

/// PS2 N0: nobody in reach — the player's line still plays; and nothing is
/// drawn in the world or counted in the HUD line.
#[test]
fn n0_nobody_in_reach_and_no_marker() {
    let mut s = scene(0, 1);
    s.run(30, IDLE);
    let at = s.press(buttons::SQUAD_FOLLOW);
    assert_eq!(s.orders, [(at, 0, 0)]);
    assert_eq!(s.said, [(at, "cis_inf_com_chatter_tac_follow".to_owned())]);
    s.run(60, IDLE);
    assert!(s.heard.is_empty() && s.takes.is_empty());
    let at = s.press(buttons::SQUAD_MOVE_OUT);
    assert_eq!(s.orders.last(), Some(&(at, 3, 1)), "35 m is inside 45 m");
    assert!(!game::squad_vis::markers_on(false, None), "no world marker");
    println!("N0  FOLLOW ME: 0 receivers, the line plays; MOVE OUT: 1 receiver at 35 m");
}
