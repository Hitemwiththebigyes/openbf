//! Cards UNLK1 / UNLK3 on real BF1 maps: Kamino's jet trooper and Yavin's
//! dark trooper and Wookiee smuggler take BF2's 12-point cost, refuse the
//! class accept at 11 points and spawn at 12; career kills halve the jet
//! trooper's cost at 50 and unlock it for good at 100. The droideka's lock is
//! in the table too, but the port does not field it yet (card DEKF).

use core::sim::{buttons, ClassRefusal, PadState, Sim, SpawnStage};
use game::profile::Career;
use game::walk::{ClassVisual, TeamTemplate, WalkSim};
use glam::Vec3;

fn bf1_match(name: &str, sides: [&str; 2]) -> Option<WalkSim> {
    let root = game::assets::baked_root();
    let map = root.join(name);
    let missions = root
        .parent()
        .unwrap()
        .join(format!("missions/{name}_con.ron"));
    if !map.join("manifest.ron").exists() || !missions.exists() {
        eprintln!("SKIP: needs baked {name} and its mission");
        return None;
    }
    let (scene, _) = game::assets::load_battle_scene(&map, "con").unwrap();
    let mission = game::mission::MissionDef::load(&missions).unwrap();
    let (rosters, _) =
        game::roster::FactionRoster::pair(&root.join("classes"), "bf1", sides, Some(&mission))
            .unwrap();
    let mut sim = Sim::empty(42);
    sim.omni_radius = core::sim::omni_radius_for("bf1");
    for (team, definition) in mission.teams.iter().enumerate() {
        sim.battle.teams[team] = core::battle::TeamState::new(definition.tickets);
    }
    let mut names = Vec::new();
    let mut team = 0;
    let templates = rosters.map(|roster| {
        let table: Vec<_> = roster
            .classes
            .iter()
            .map(|class| class.register(&mut sim, &mut names))
            .collect();
        let visuals = roster
            .classes
            .iter()
            .map(ClassVisual::from_roster)
            .collect();
        let template = TeamTemplate::from_roster(&roster, &table[0], visuals);
        sim.classes[team] = table;
        team += 1;
        template
    });
    let nav = game::layout::nav_graph(&scene.manifest, &scene.dir);
    let mut walk = WalkSim::with_teams(sim, scene.heightfield(), Vec3::ZERO, templates, nav);
    scene.install_collision(&mut walk.sim);
    let layout = game::layout::MapLayout::from_manifest(&scene.manifest);
    walk.setup_conquest(&layout, [0, 0], true);
    assert!(walk.sim.open_spawn_screens(0, true));
    Some(walk)
}

fn tap(walk: &mut WalkSim, b: u32) {
    walk.step_tick(&[PadState {
        buttons: b,
        ..Default::default()
    }]);
    walk.step_tick(&[PadState::default()]);
}

/// Side `team`, then the spawn map's accept, then class `name` on the
/// carousel; its index.
fn to_class(walk: &mut WalkSim, team: usize, name: &str) -> usize {
    assert_eq!(walk.sim.spawn_stage[0], SpawnStage::Side);
    if team == 1 {
        tap(walk, buttons::SPAWN_POST_DOWN);
    }
    tap(walk, buttons::SPAWN_ACCEPT);
    tap(walk, buttons::SPAWN_ACCEPT);
    assert_eq!(walk.sim.spawn_stage[0], SpawnStage::Class);
    let class = walk.teams[team]
        .classes
        .iter()
        .position(|c| c.name == name)
        .unwrap_or_else(|| panic!("{name} is not fielded"));
    for _ in 0..walk.teams[team].classes.len() {
        if usize::from(walk.sim.spawn_choice[0]) == class {
            break;
        }
        tap(walk, buttons::SPAWN_CLASS_NEXT);
        while walk.sim.class_carousel[0].slide > 0 {
            walk.step_tick(&[PadState::default()]);
        }
    }
    assert_eq!(usize::from(walk.sim.spawn_choice[0]), class);
    class
}

#[test]
fn bf1_specials_refuse_below_12_points_and_spawn_at_12() {
    for (map, sides, team, name) in [
        ("bf1_kam1", ["cis", "rep"], 1, "rep_inf_jet_trooper"),
        ("bf1_yav1", ["imp", "all"], 0, "imp_inf_dark_trooper"),
        ("bf1_yav1", ["imp", "all"], 1, "all_inf_smuggler"),
    ] {
        let Some(mut walk) = bf1_match(map, sides) else {
            return;
        };
        // The other classes stay free.
        for (c, v) in walk.sim.classes[team].iter().zip(&walk.teams[team].classes) {
            let want = if v.name == name { 12 } else { 0 };
            assert_eq!(c.unlock_points, want, "{}", v.name);
        }
        let class = to_class(&mut walk, team, name);
        game::unlocks::apply_costs(&mut walk, &Career::default());
        walk.sim.set_spawn_points(0, 11);
        assert_eq!(
            walk.sim.class_refusal(0, team as u8, class),
            Some(ClassRefusal::Points(1))
        );
        tap(&mut walk, buttons::SPAWN_ACCEPT);
        assert_eq!(walk.sim.spawn_stage[0], SpawnStage::Class, "{name}");
        walk.sim.set_spawn_points(0, 12);
        tap(&mut walk, buttons::SPAWN_ACCEPT);
        assert_eq!(walk.sim.spawn_stage[0], SpawnStage::Accepted, "{name}");
        let me = walk.sim.unit(walk.player).unwrap();
        let got = (usize::from(me.team), usize::from(me.spec.class));
        assert_eq!(got, (team, class), "{name}");
    }
}

/// BF2 jet trooper kills count for BF1's jet trooper too (one counter).
#[test]
fn career_kills_earn_the_jet_trooper_down() {
    let Some(mut walk) = bf1_match("bf1_kam1", ["cis", "rep"]) else {
        return;
    };
    let class = to_class(&mut walk, 1, "rep_inf_jet_trooper");
    let mut career = Career::default();
    career
        .class_kills
        .insert("rep_inf_ep3_jettrooper".into(), 40);
    career.class_kills.insert("rep_inf_jet_trooper".into(), 10);
    game::unlocks::apply_costs(&mut walk, &career);
    walk.sim.set_spawn_points(0, 5);
    assert_eq!(
        walk.sim.class_refusal(0, 1, class),
        Some(ClassRefusal::Points(1)),
        "50 kills: half"
    );
    career.class_kills.insert("rep_inf_jet_trooper".into(), 60);
    game::unlocks::apply_costs(&mut walk, &career);
    walk.sim.set_spawn_points(0, 0);
    tap(&mut walk, buttons::SPAWN_ACCEPT);
    assert_eq!(walk.sim.spawn_stage[0], SpawnStage::Accepted, "100: free");
    let me = walk.sim.unit(walk.player).unwrap();
    assert_eq!(usize::from(me.spec.class), class);
}

/// Card CHEAT2: the `all` cheat makes a 12-point class free; the player
/// spawns it with 0 points and no career.
#[test]
fn the_all_cheat_unlocks_a_12_point_class_at_0_points() {
    let Some(mut walk) = bf1_match("bf1_yav1", ["imp", "all"]) else {
        return;
    };
    let class = to_class(&mut walk, 0, "imp_inf_dark_trooper");
    game::unlocks::set_player_gate(&mut walk, 0, &Career::default());
    assert_eq!(
        walk.sim.class_refusal(0, 0, class),
        Some(ClassRefusal::Points(12)),
        "locked without the cheat"
    );
    walk.sim.cheats.all_classes = true;
    game::unlocks::set_player_gate(&mut walk, 0, &Career::default());
    assert_eq!(walk.sim.spawn_cost(0, 0, class), 0);
    assert_eq!(walk.sim.class_refusal(0, 0, class), None);
    tap(&mut walk, buttons::SPAWN_ACCEPT);
    assert_eq!(walk.sim.spawn_stage[0], SpawnStage::Accepted);
    let me = walk.sim.unit(walk.player).unwrap();
    assert_eq!(usize::from(me.spec.class), class);
}
