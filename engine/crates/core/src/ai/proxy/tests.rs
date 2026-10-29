use super::*;
use crate::unit::{Controller, UnitHandle, UnitSpec};
use glam::Vec3;

fn bot(sim: &mut Sim, team: u8, at: Vec3) -> UnitHandle {
    sim.spawn(&UnitSpec {
        controller: Controller::Ai,
        ..UnitSpec::trooper(team, at)
    })
    .expect("bot")
}

fn field(d: Difficulty) -> (Sim, UnitHandle, UnitHandle) {
    let (mut sim, _) = crate::ai::tests::l_sim();
    sim.units.clear();
    sim.players.clear();
    sim.difficulty = d;
    let a = bot(&mut sim, 0, Vec3::ZERO);
    let b = bot(&mut sim, 1, Vec3::new(0.0, 0.0, -15.0));
    (sim, a, b)
}

/// An unset flag leaves the sim's hash as it was; setting it moves it.
#[test]
fn the_flag_is_hashed_only_when_set() {
    let (mut sim, a, _) = field(Difficulty::Normal);
    let before = sim.hash();
    assert!(sim.set_proxy(a, None));
    assert_eq!(sim.hash(), before, "None is no byte");
    assert!(sim.set_proxy(a, Some(Strength::Good.profile())));
    assert_ne!(sim.hash(), before);
}

/// A proxy is a human to the human-only rules: the TOKN tokens (the enemy
/// bot without one is barred), the DTEAM count, `is_human`; and a bot to the
/// cheats (`is_player` stays false).
#[test]
fn a_proxy_is_human_to_the_human_rules() {
    let (mut sim, a, b) = field(Difficulty::Normal);
    assert_eq!(humans_on(&sim, 0), 0);
    sim.unit_mut(b).unwrap().ai.target = Some(a);
    assert!(!crate::ai::tokn::bars(&sim, b, a), "a bot is not governed");
    sim.set_proxy(a, Some(Strength::Good.profile()));
    let u = sim.unit(a).unwrap();
    assert!(u.is_human() && !u.is_player);
    assert_eq!((humans_on(&sim, 0), humans_on(&sim, 1)), (1, 0));
    assert!(crate::ai::tokn::bars(&sim, b, a), "governed, no token yet");
    crate::ai::tokn::deal(&mut sim);
    assert!(
        crate::ai::tokn::holds(&sim, b, a),
        "the lone shooter holds one"
    );
}

/// The proxy's profile is the same on every tier: its skill has no Elite
/// cap, its reaction and view are its own.
#[test]
fn the_profile_ignores_the_tier() {
    let p = Strength::Good.profile();
    for d in Difficulty::ALL {
        let (mut sim, a, _) = field(d);
        sim.set_proxy(a, Some(p));
        let u = sim.unit(a).unwrap();
        assert_eq!(sim.skill_of(u), Skill::from_level(p.skill), "{d:?}");
        let t = sim.unit_tier(u);
        assert_eq!(t.reaction_ticks, p.reaction_ticks);
        assert_eq!(t.react, p.react);
        assert!((t.view_mult - p.view_mult * sim.mission_ai.view_mult).abs() < 1e-6);
    }
    let (sim, a, _) = field(Difficulty::Elite);
    assert_eq!(sim.skill_of(sim.unit(a).unwrap()).err_cap, 0.5, "a bot's");
}

/// A proxy that dies comes back a proxy.
#[test]
fn the_flag_survives_a_respawn() {
    let (mut sim, a, _) = field(Difficulty::Normal);
    sim.set_proxy(a, Some(Strength::Average.profile()));
    let (capture_time, neutralize_time) = crate::cp::CpDef::class_times(false);
    sim.add_cp(
        crate::cp::CpDef {
            name: "home".into(),
            pos: Vec3::ZERO,
            capture: crate::region::Region::sphere(Vec3::ZERO, 8.0),
            control: None,
            value_bleed: 10,
            value_atk: [100, 100],
            value_def: [100, 100],
            spawn_nodes: vec![(Vec3::new(2.0, 0.0, 2.0), 0.0)],
            capture_time,
            neutralize_time,
        },
        Some(0),
    );
    sim.kill_environment(a, crate::sim::DeathCause::Fall);
    let i = a.index as usize;
    sim.respawn_slot(i);
    assert!(sim.units[i].alive && sim.units[i].gen > a.gen, "respawned");
    assert_eq!(sim.units[i].proxy_human, Some(Strength::Average.profile()));
}

/// The targets: the maintainer's table, each tier's band below the one
/// before it, and the verdict reads the band.
#[test]
fn the_targets_are_the_maintainers_and_fall_by_tier() {
    assert_eq!(target(Difficulty::Easy), (0.90, 1.0));
    assert_eq!(target(Difficulty::Normal), (0.65, 0.80));
    assert_eq!(target(Difficulty::Hard), (0.45, 0.60));
    assert_eq!(target(Difficulty::Elite), (0.25, 0.40));
    for w in TARGETS.windows(2) {
        assert!(w[1].1 < w[0].1 && w[1].2 <= w[0].2);
    }
    assert!(passes(Difficulty::Normal, 23, 32));
    assert!(!passes(Difficulty::Normal, 27, 32));
    assert!(passes(Difficulty::Easy, 32, 32));
    assert!(!passes(Difficulty::Elite, 0, 0));
    assert_eq!(Strength::parse("AVG"), Some(Strength::Average));
    assert_eq!(Strength::parse("good").map(Strength::label), Some("good"));
}
