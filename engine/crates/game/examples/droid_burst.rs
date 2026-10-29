//! A BF2 battle droid's death drawn headless, before and after card DROIDDEATH
//! (`analysis/re-droid-death.md`): on BF2 Kamino (clone wars), a CIS soldier
//! is stood 6 m ahead of the player's chase camera and killed by a blast with
//! no push; `before_*` frames run with the class's chunk death switched off
//! (the corpse falls), `after_*` with its mark set (it bursts: the class's
//! `cis_sfx_droid_explosion`, no body).
//!
//!   droid_burst <out_dir>
//!
//! Frames: `<out_dir>/{before,after}_<tick>.png`, ticks after the kill.
use anyhow::{anyhow, Context, Result};
use core::sim::{buttons, Event, PadState, Sim, SpawnStage};
use core::unit::Controller;
use core::weapon::ExplosionClass;
use game::walk::{ClassVisual, TeamTemplate, WalkSim};
use glam::Vec3;
use render::Renderer;
use std::path::{Path, PathBuf};

const TICKS: [u64; 6] = [0, 2, 4, 8, 14, 30];

fn battle(root: &Path) -> Result<(WalkSim, game::assets::Scene)> {
    let map = root.join("bf2_kam1");
    let (scene, _) = game::assets::load_battle_scene(&map, "con")?;
    let mission = game::mission::MissionDef::load(
        &root
            .parent()
            .context("assets root")?
            .join("missions/bf2_kam1_con_cw.ron"),
    )?;
    let (rosters, _) = game::roster::FactionRoster::pair(
        &root.join("classes"),
        "bf2",
        [
            mission.teams[0].faction.as_str(),
            mission.teams[1].faction.as_str(),
        ],
        Some(&mission),
    )?;
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
    walk.sim.open_spawn_screens(0, false);
    walk.sim.set_spawn_points(0, 12);
    for _ in 0..2 {
        walk.step_tick(&[PadState {
            buttons: buttons::SPAWN_ACCEPT,
            ..Default::default()
        }]);
        walk.step_tick(&[PadState::default()]);
    }
    for _ in 0..60 {
        walk.step_tick(&[PadState::default()]);
    }
    anyhow::ensure!(walk.sim.spawn_stage[0] == SpawnStage::Accepted, "no spawn");
    Ok((walk, scene))
}

fn run(root: &Path, out: &Path, burst: bool) -> Result<()> {
    let (mut walk, scene) = battle(root)?;
    let mut r = Renderer::new_headless(960, 540).map_err(|e| anyhow!("renderer: {e}"))?;
    let mut gpu = game::viewer::upload_scene(&mut r, &scene)?;
    let mut vis = game::soldier_vis::SoldierVisuals::for_walk(&mut r, root, &scene, &walk);
    let mut fx = game::fx::Fx::for_walk(root, &scene.dir, "bf2", &walk);
    // A CIS soldier of the fill class (the super battle droid), held still
    // 6 m ahead of the player and facing him.
    let me = walk.soldier().clone();
    let fwd = Vec3::new(-me.yaw.sin(), 0.0, -me.yaw.cos());
    let at = me.pos + fwd * 6.0;
    let class = 0u8;
    let cis = walk
        .teams
        .iter()
        .position(|t| t.faction == "cis")
        .context("no CIS team")?;
    if !burst {
        walk.sim.classes[cis][usize::from(class)].chunk = None;
    }
    let spec = walk.sim.classes[cis][usize::from(class)].spec(
        cis as u8,
        at,
        me.yaw + std::f32::consts::PI,
        Controller::Idle,
        class,
        0,
    );
    let h = walk.sim.spawn(&spec).context("spawn the droid")?;
    let i = h.index as usize;
    walk.sim.units[i].chunk_mark = burst;
    for _ in 0..20 {
        walk.step_tick(&[PadState::default()]);
        walk.sim.units[i].soldier.pos = at;
        vis.update(&walk, core::TICK_DT);
    }
    let pos = walk.sim.units[i].soldier.pos;
    println!(
        "class {} at {pos:.2}, player at {:.2}",
        walk.teams[cis].class(class).name,
        walk.soldier().pos
    );
    let kill = ExplosionClass {
        damage: 2000.0,
        damage_outer: 0.5,
        ..Default::default()
    };
    walk.sim.explode(&kill, pos + Vec3::Y, None, Some(0));
    anyhow::ensure!(!walk.sim.units[i].alive, "the blast did not kill");
    let cam = walk.camera();
    let mut tick = 0;
    for t in TICKS {
        while tick < t {
            walk.step_tick(&[PadState::default()]);
            fx.ingest(&walk);
            if walk
                .sim
                .events
                .iter()
                .any(|e| matches!(e, Event::DroidBurst { .. }))
            {
                println!("DroidBurst at tick {}", tick + 1);
            }
            vis.update(&walk, core::TICK_DT);
            fx.update(core::TICK_DT);
            tick += 1;
        }
        r.time = (walk.sim.tick as f32 * core::TICK_DT) % render::TIME_WRAP;
        let mut draws = gpu.view_draws(&walk.sim, walk.player);
        let skinned = vis.draws(&walk);
        draws.extend(vis.weapon_draws(&walk));
        fx.submit(&mut r, &cam);
        gpu.animate_water_live(&mut r, cam.position)?;
        r.render_frame(&cam, &draws, &skinned, gpu.clear, None)
            .map_err(|e| anyhow!("render: {e}"))?;
        let (rgba, w, hgt) = r.capture_rgba().map_err(|e| anyhow!("capture: {e}"))?;
        let png = render::encode_png(&rgba, w, hgt).map_err(|e| anyhow!("png: {e}"))?;
        let name = format!("{}_{t:02}.png", if burst { "after" } else { "before" });
        std::fs::write(out.join(&name), png)?;
        println!(
            "{name}: body {} burst {} drawn soldiers {}",
            walk.sim.units[i].body,
            walk.sim.units[i].burst,
            skinned.len()
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    let out = PathBuf::from(std::env::args().nth(1).context("droid_burst <out_dir>")?);
    std::fs::create_dir_all(&out)?;
    let root = game::assets::baked_root();
    run(&root, &out, false)?;
    run(&root, &out, true)
}
