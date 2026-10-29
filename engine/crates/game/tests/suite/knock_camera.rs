//! Issue #36: a soldier a blast throws near BF2 Coruscant's walls stays in
//! the chase camera's view (walls used to pull the camera in toward the look
//! point above him, which dropped him off the bottom of the screen) - or,
//! with the camera come into him, his body is hidden and the view is his
//! (`game::camera_collide::BODY_HIDE_DIST`, playtest 2026-10-28).
use core::sim::{buttons, PadState, Sim, SpawnStage};
use core::soldier::Stance;
use core::weapon::ExplosionClass;
use game::walk::{ClassVisual, TeamTemplate, WalkSim};
use glam::Vec3;

fn coruscant() -> Option<WalkSim> {
    let root = game::assets::baked_root();
    let map = root.join("bf2_cor1");
    if !map.join("manifest.ron").exists() {
        eprintln!("SKIP: knock camera needs baked BF2 Coruscant");
        return None;
    }
    let (scene, _) = game::assets::load_battle_scene(&map, "con").unwrap();
    let mission =
        game::mission::MissionDef::load(&root.parent().unwrap().join("missions/bf2_cor1_con.ron"))
            .unwrap();
    let (rosters, _) = game::roster::FactionRoster::pair(
        &root.join("classes"),
        "bf2",
        ["all", "imp"],
        Some(&mission),
    )
    .unwrap();
    let mut sim = Sim::empty(42);
    sim.ground_rules = core::soldier::GroundRules::Bf2;
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
    walk.camera_blockers = game::camera_collide::CameraBlockers::from_scene(&scene);
    let mut layout = game::layout::MapLayout::from_manifest(&scene.manifest);
    walk.sim.death_regions = layout.apply_mission(&mission).death_regions;
    walk.setup_conquest(&layout, [0, 0], true);
    assert!(walk.sim.open_spawn_screens(0, false));
    walk.sim.set_spawn_points(0, 12);
    for _ in 0..2 {
        walk.step_tick(&[PadState {
            buttons: buttons::SPAWN_ACCEPT,
            ..Default::default()
        }]);
        walk.step_tick(&[PadState::default()]);
    }
    for _ in 0..90 {
        walk.step_tick(&[PadState::default()]);
    }
    assert_eq!(walk.sim.spawn_stage[0], SpawnStage::Accepted);
    Some(walk)
}

/// The soldier's middle in the drawn camera's clip space: (x, y, w).
fn on_screen(walk: &WalkSim) -> (f32, f32, f32) {
    let cam = walk.camera();
    let mid = walk.soldier().pos + Vec3::Y * 1.0;
    let c = cam.view_proj(16.0 / 9.0) * mid.extend(1.0);
    (c.x / c.w, c.y / c.w, c.w)
}

#[test]
fn a_thrown_soldier_stays_in_view_near_coruscants_walls() {
    for k in 0..8 {
        let Some(mut walk) = coruscant() else {
            return;
        };
        let e = ExplosionClass {
            push: 10.0,
            push_outer: 6.5,
            ..Default::default()
        };
        let a = k as f32 * std::f32::consts::TAU / 8.0;
        let feet = walk.soldier().pos;
        let blast = feet + Vec3::new(a.sin(), 0.1, a.cos()) * 1.5;
        walk.sim.explode(&e, blast, None, Some(1));
        let (mut worst, mut behind, mut ticks) = (0.0f32, false, 0);
        let mut first_out = None;
        while ticks < 240 {
            walk.step_tick(&[PadState::default()]);
            ticks += 1;
            let (x, y, w) = on_screen(&walk);
            if walk.player_body_hidden() {
                continue;
            }
            let m = x.abs().max(y.abs());
            if (m > 1.0 || w <= 0.0) && first_out.is_none() {
                let s = walk.soldier();
                first_out = Some(format!(
                    "t {ticks} {:?} pos {:.1?} vel {:.1?} cam {:.1?} ndc ({x:.2}, {y:.2}) w {w:.2}",
                    s.stance,
                    s.pos,
                    s.vel,
                    walk.camera().position
                ));
            }
            worst = worst.max(m);
            behind |= w <= 0.0;
            if walk.soldier().stance == Stance::Stand && ticks > 30 {
                break;
            }
        }
        assert!(
            first_out.is_none() && !behind,
            "blast from direction {k}: worst {worst:.2}, first out {first_out:?}"
        );
    }
}
