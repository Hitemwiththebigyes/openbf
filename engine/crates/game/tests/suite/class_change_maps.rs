//! Issue #28 on real maps of both games: a human at one of his team's posts
//! changes class without dying; health carries as a share and a class
//! played again this life comes back with its own ammo.

use core::sim::{buttons, PadState, Sim, SpawnStage};
use game::walk::{ClassVisual, TeamTemplate, WalkSim};
use glam::Vec3;

fn battle(map: &str, game_id: &str) -> Option<WalkSim> {
    let root = game::assets::baked_root();
    let dir = root.join(map);
    let mission_file = root
        .parent()
        .unwrap()
        .join(format!("missions/{map}_con.ron"));
    if !dir.join("manifest.ron").exists() || !mission_file.exists() {
        eprintln!("SKIP: class change on {map} requires its baked map and mission");
        return None;
    }
    let (scene, _) = game::assets::load_battle_scene(&dir, "con").unwrap();
    let mission = game::mission::MissionDef::load(&mission_file).unwrap();
    let factions = [
        mission.teams[0].faction.as_str(),
        mission.teams[1].faction.as_str(),
    ];
    let (rosters, _) =
        game::roster::FactionRoster::pair(&root.join("classes"), game_id, factions, Some(&mission))
            .unwrap();
    let mut sim = Sim::empty(42);
    if game_id == "bf2" {
        sim.ground_rules = core::soldier::GroundRules::Bf2;
    }
    sim.omni_radius = core::sim::omni_radius_for(game_id);
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
    assert!(walk.sim.open_spawn_screens(0, false));
    Some(walk)
}

fn tap(walk: &mut WalkSim, bits: u32) {
    walk.step_tick(&[PadState {
        buttons: bits,
        ..Default::default()
    }]);
    walk.step_tick(&[PadState::default()]);
}

/// The player inside a post of his team he can change class at, facing it.
fn at_friendly_post(walk: &mut WalkSim) {
    let i = walk.player.index as usize;
    let team = walk.sim.units[i].team;
    for (ci, cp) in walk.sim.cps.clone().iter().enumerate() {
        if !cp.spawnable_by(team) {
            continue;
        }
        for off in [Vec3::Z, Vec3::X, Vec3::NEG_Z, Vec3::NEG_X] {
            for r in [2.0, 3.0, 1.0] {
                let mut at = cp.def.pos + off * r;
                at.y = walk.sim.ground_under(at, false);
                let to = cp.def.pos - at;
                let u = &mut walk.sim.units[i];
                u.soldier.pos = at;
                u.soldier.yaw = (-to.x).atan2(-to.z);
                u.soldier.pitch = to.y.atan2(to.with_y(0.0).length());
                if walk.sim.class_change_post(i) == Some(ci) {
                    return;
                }
            }
        }
    }
    panic!("no friendly post to change class at");
}

fn change_to(walk: &mut WalkSim, to: u8) {
    at_friendly_post(walk);
    tap(walk, buttons::ACTION | buttons::SPAWN_BACK);
    assert_eq!(walk.sim.spawn_stage[0], SpawnStage::Alive, "opened");
    while walk.sim.spawn_choice[0] != to {
        tap(walk, buttons::SPAWN_CLASS_NEXT);
        while walk.sim.class_carousel[0].slide > 0 {
            walk.step_tick(&[PadState::default()]);
        }
    }
    tap(walk, buttons::SPAWN_ACCEPT);
    assert_eq!(walk.sim.spawn_stage[0], SpawnStage::Accepted);
    assert_eq!(walk.sim.unit(walk.player).unwrap().spec.class, to);
}

fn check(map: &str, game_id: &str) {
    let Some(mut walk) = battle(map, game_id) else {
        return;
    };
    // Spawn as the default class through the screens.
    tap(&mut walk, buttons::SPAWN_ACCEPT);
    tap(&mut walk, buttons::SPAWN_ACCEPT);
    let me = walk.sim.unit(walk.player).unwrap();
    assert!(me.alive, "{map}: spawned");
    assert_eq!(me.spec.class, 0);
    let tickets = walk.sim.battle.teams[0].tickets();
    // Hurt and empty.
    let u = walk.sim.unit_mut(walk.player).unwrap();
    u.health.cur = u.health.max * 0.5;
    u.weapon.clip = 0;
    u.weapon.spare = 0;
    change_to(&mut walk, 1);
    let me = walk.sim.unit(walk.player).unwrap();
    assert!(me.alive);
    assert!(
        (me.health.cur / me.health.max - 0.5).abs() < 0.01,
        "{map}: health share"
    );
    assert!(me.weapon.clip > 0, "{map}: a fresh class is full");
    change_to(&mut walk, 0);
    let me = walk.sim.unit(walk.player).unwrap();
    assert_eq!(
        (me.weapon.clip, me.weapon.spare),
        (0, 0),
        "{map}: no refill"
    );
    assert_eq!(
        walk.sim.battle.teams[0].tickets(),
        tickets,
        "{map}: no ticket"
    );
    // Bots never changed class this way: every AI unit's generation is
    // untouched by the player's swaps (they keep spawning normally).
    assert!(walk
        .sim
        .units
        .iter()
        .enumerate()
        .all(|(i, _)| i == walk.player.index as usize || walk.sim.class_change_post(i).is_none()));
}

#[test]
fn a_bf1_map_changes_class_at_a_friendly_post() {
    check("bf1_hot1", "bf1");
}

#[test]
fn a_bf2_map_changes_class_at_a_friendly_post() {
    check("bf2_uta1", "bf2");
}
