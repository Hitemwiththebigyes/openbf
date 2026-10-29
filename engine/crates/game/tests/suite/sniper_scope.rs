//! The sniper scope (playtest 2026-10-28, BF2 Kamino, the Republic jet
//! trooper's sniper rifle): "the scope just zoomed in, it is supposed to
//! have its own HUD"; "the bullets in scope came from the side or did not
//! show at all, a bot would just die"; "in scope it did not show the
//! health of other bots".
//!
//! 1. Zoomed on a `ZoomFirstPerson` weapon the HUD draws the scope's
//!    `weapon_scope` overlay (`ui::hud_scope`, `analysis/re-zoom.md` §8),
//!    BF1's own picture on a BF1 map; the weapon panel is off
//!    (`weaponsDisable`), the reticle and the target readout stay; zoomed
//!    out it is all as before.
//! 2. A scoped shot at a bot is a direct hit (its age is not
//!    `core::sim::BLAST_AGE`) and its drawn bolt runs from the scope point,
//!    the shot's own origin on the camera's axis, to the hit on the bot,
//!    whether it strikes in its first update (inside 66 m) or later.
//!
//! Real classes and maps from the bake; each case skips without them.
//! `scope_pictures` (ignored: a GPU) renders the scoped view to
//! `engine/artifacts/lead_tmp/scope/`.

use core::sim::{buttons, Event, PadState};
use core::unit::{Controller, UnitHandle};
use glam::Vec3;

struct Scoped {
    scene: game::assets::Scene,
    walk: game::walk::WalkSim,
    enemy: UnitHandle,
    game: &'static str,
    sides: [&'static str; 2],
}

/// `class` (a sniper) on `map`, an idle enemy `dist` m away on open ground
/// level with him: the first lane near a command post with a clear line
/// between their heads. `None` without the bake.
fn scoped(
    map: &str,
    game: &'static str,
    class: &str,
    sides: [&'static str; 2],
    dist: f32,
) -> Option<Scoped> {
    let root = game::assets::baked_root();
    let mut scene = game::assets::load_scene(&root.join(map)).ok()?;
    scene.apply_mode_layer("con");
    let db = game::roster::load_classdb(&root.join("classes"), game, &sides).ok()?;
    let roster = game::roster::ClassRoster::build(&db, class, 1, None).ok()?;
    let mut sim = core::sim::Sim::empty(42);
    sim.ground_rules = core::soldier::GroundRules::for_game(game);
    scene.install_collision(&mut sim);
    let uc = roster.register(&mut sim, &mut Vec::new());
    let mut visual = game::walk::ClassVisual::from_roster(&roster);
    visual.held_hud = roster
        .held
        .iter()
        .map(|h| (h.slot, h.hud.clone()))
        .collect();
    let faction = game::roster::FactionRoster {
        faction: sides[0].into(),
        classes: vec![roster.clone()],
    };
    let t = game::walk::TeamTemplate::from_roster(&faction, &uc, vec![visual]);
    let mut enemy_t = t.clone();
    enemy_t.faction = sides[1].into();
    let layout = game::layout::MapLayout::from_manifest(&scene.manifest);
    let at = layout.cps.first().map_or(Vec3::ZERO, |c| c.pos);
    let mut walk =
        game::walk::WalkSim::with_teams(sim, scene.heightfield(), at, [t, enemy_t.clone()], None);
    walk.fov_y_ps2 = game::walk::ps2_fov_y(game);
    let pi = walk.player.index as usize;
    let h = walk
        .sim
        .spawn(&enemy_t.spec(1, at, 0.0, Controller::Idle, 0))
        .unwrap();
    let settle = |walk: &mut game::walk::WalkSim| {
        for _ in 0..40 {
            walk.step_tick(&[PadState::default()]);
        }
    };
    let tries = layout.cps.iter().flat_map(|c| {
        [Vec3::new(8.0, 1.0, 8.0), Vec3::new(-8.0, 1.0, -8.0)]
            .into_iter()
            .flat_map(move |off| (0..16).map(move |k| (c.pos + off, k)))
    });
    for (base, k) in tries {
        let a = k as f32 * std::f32::consts::FRAC_PI_8;
        let dir = Vec3::new(a.sin(), 0.0, a.cos());
        let u = walk.sim.unit_mut(h).unwrap();
        u.soldier.pos = base + dir * dist + Vec3::Y * 0.5;
        u.soldier.vel = Vec3::ZERO;
        settle(&mut walk);
        let e = walk.sim.unit(h).unwrap().soldier.pos;
        let p = &mut walk.sim.units[pi].soldier;
        p.pos = e - dir * dist + Vec3::Y * 0.5;
        p.vel = Vec3::ZERO;
        settle(&mut walk);
        let p = walk.sim.units[pi].soldier.pos;
        let clear = core::perception::los_seat(
            &walk.sim,
            p + Vec3::Y * 1.7,
            e + Vec3::Y * 1.4,
            core::collision::HullFilter::All,
            [None, None],
        );
        if (e.y - p.y).abs() < 1.0 && clear {
            let mut s = Scoped {
                scene,
                walk,
                enemy: h,
                game,
                sides,
            };
            aim(&mut s);
            return Some(s);
        }
    }
    panic!("{map}: no open lane of {dist} m near a post");
}

/// Turn the player's aim onto the enemy's chest.
fn aim(s: &mut Scoped) {
    let w = &mut s.walk;
    let Some(e) = w.sim.unit(s.enemy).map(|u| u.soldier.pos + Vec3::Y * 1.2) else {
        return;
    };
    let pi = w.player.index as usize;
    // Twice: the scope point moves with the aim.
    for _ in 0..2 {
        let o = w.sim.units[pi].aim_ray(w.sim.ground_rules, 1).0;
        let d = e - o;
        let me = &mut w.sim.units[pi].soldier;
        me.pitch = d.y.atan2(Vec3::new(d.x, 0.0, d.z).length());
        me.yaw = (-d.x).atan2(-d.z);
    }
}

/// One tick with `buttons` held, aimed at the enemy first.
fn tick(s: &mut Scoped, buttons: u32) {
    aim(s);
    s.walk.step_tick(&[PadState {
        buttons,
        ..Default::default()
    }]);
}

/// Press ZOOM once and let the camera's zoom ease.
fn zoom(s: &mut Scoped) {
    tick(s, buttons::ZOOM);
    for _ in 0..15 {
        tick(s, 0);
    }
}

/// The HUD model as the battle builds it (`main/battle.rs` `hud_model`).
fn model(s: &Scoped, feed: &game::event_feed::EventFeed) -> ui::hud::HudModel {
    let w = &s.walk;
    let me = &w.sim.units[w.player.index as usize];
    let [weapon, weapon2] = game::hud_feed::weapon_rows(w);
    ui::hud::HudModel {
        team_names: s.sides.map(String::from),
        tickets: [150, 150],
        hp: me.health.cur,
        hp_max: me.health.max,
        energy: me.soldier.energy,
        energy_max: me.consts.energy_bar,
        weapon,
        weapon2,
        reticle: Some(
            w.sim
                .reticle_pick
                .first()
                .copied()
                .flatten()
                .map_or(0, |(_, rel)| rel),
        ),
        target: feed.target(&w.sim, 0),
        reticle_y: w.reticle_y(),
        stats: Some(ui::hud::HudStats::default()),
        player_team: me.team,
        scope: game::hud_feed::scope_view(w),
        ..Default::default()
    }
}

fn hud_view() -> Option<game::hud_feed::HudView> {
    let root = game::assets::baked_root();
    if !root.join("common_bf2/hud/1playerhud.ron").is_file() {
        return None;
    }
    game::hud_feed::HudView::load(&root, render::HEADLESS_FORMAT, None).ok()
}

const SIZE: [u32; 2] = [1280, 720];

/// What the HUD draws: the scope's sheet, the weapon panel's ammo count,
/// the reticle, the readout's name and bar.
fn drawn(v: &mut game::hud_feed::HudView, m: &ui::hud::HudModel, sheet: &str) -> [bool; 5] {
    for _ in 0..3 {
        v.update(m, 0.5, SIZE);
    }
    [
        v.drawn_sheets().any(|t| t == sheet),
        v.bounds("player1weapon1ammo").is_some(),
        v.bounds("player1reticule_shape").is_some()
            || v.bounds("player1reticule_outline").is_some(),
        v.bounds("player1targetinfo_classNameNEW").is_some(),
        v.bounds("player1targethealth_fill").is_some(),
    ]
}

/// Case 1, on BF2 and BF1 maps.
#[test]
fn the_scope_overlay_is_up_while_zoomed_and_the_readout_stays() {
    for (map, game, class, sides, sheet) in [
        (
            "bf2_kam1",
            "bf2",
            "rep_inf_ep2_jettrooper_sniper",
            ["rep", "cis"],
            "weapon_scope",
        ),
        (
            "bf1_tat2",
            "bf1",
            "all_inf_marksman",
            ["all", "imp"],
            "common_bf1/weapon_scope",
        ),
    ] {
        let (Some(mut s), Some(mut v)) = (scoped(map, game, class, sides, 40.0), hud_view()) else {
            eprintln!("skip {map}: no bake");
            continue;
        };
        let root = game::assets::baked_root();
        let feed = game::event_feed::EventFeed::for_walk(&root, game, &s.walk);
        for _ in 0..5 {
            tick(&mut s, 0);
        }
        assert_eq!(game::hud_feed::scope_view(&s.walk), None, "{map}: unzoomed");
        let m = model(&s, &feed);
        assert!(m.target.is_some(), "{map}: the reticle picks the enemy");
        assert_eq!(
            drawn(&mut v, &m, sheet),
            [false, true, true, true, true],
            "{map} unzoomed: no scope, panel, reticle, readout name and bar"
        );

        // Both zoom steps: the scope, no panel, the reticle and readout.
        for step in [0, 1] {
            zoom(&mut s);
            let me = s.walk.sim.unit(s.walk.player).unwrap();
            assert!(
                me.scoped() && me.zoom.unwrap().step == step,
                "{map}: step {step}"
            );
            let m = model(&s, &feed);
            assert_eq!(
                m.scope.as_ref().map(|v| v.sheet.as_str()),
                Some(sheet),
                "{map} step {step}"
            );
            assert!(m.target.is_some(), "{map} step {step}: the pick in scope");
            assert_eq!(
                drawn(&mut v, &m, sheet),
                [true, false, true, true, true],
                "{map} step {step}: scope, no panel, reticle, readout name and bar"
            );
        }
        // Zoomed out: as before.
        zoom(&mut s);
        assert_eq!(
            game::hud_feed::scope_view(&s.walk),
            None,
            "{map}: zoomed out"
        );
        let m = model(&s, &feed);
        assert_eq!(
            drawn(&mut v, &m, sheet),
            [false, true, true, true, true],
            "{map} zoomed out"
        );
    }
}

/// Case 2: fire scoped at the bot; the hit and the drawn line.
fn scoped_shot(map: &str, game: &'static str, class: &str, sides: [&'static str; 2], dist: f32) {
    let Some(mut s) = scoped(map, game, class, sides, dist) else {
        eprintln!("skip {map}: no bake");
        return;
    };
    let root = game::assets::baked_root();
    let mut bolts = game::bolt_vis::BoltVis::new(&root, game);
    zoom(&mut s);
    assert!(s.walk.sim.unit(s.walk.player).unwrap().scoped(), "{map}");
    let mut shot = None;
    let mut hit = None;
    let mut impact = None;
    for k in 0..10 {
        tick(&mut s, if k == 0 { buttons::FIRE } else { 0 });
        bolts.track(&s.walk);
        for e in &s.walk.sim.events {
            match *e {
                Event::Shot { origin, dir, .. } => shot = Some((origin, dir)),
                Event::Hit { victim, age, .. } if victim == s.enemy => hit = Some((age, k)),
                Event::Impact {
                    pos,
                    surface: core::impact::Surface::Soldier,
                    ..
                } => impact = Some(pos),
                _ => {}
            }
        }
        if hit.is_some() {
            break;
        }
    }
    let (origin, dir) = shot.expect("the rifle fired");
    let (age, at) = hit.expect("the shot hit the bot");
    let impact = impact.expect("the impact on the bot");
    let me = s.walk.sim.unit(s.walk.player).unwrap();
    let scope_point = me.scope_point(s.walk.sim.ground_rules);
    assert!(
        origin.distance(scope_point) < 1e-3,
        "{map}: the shot leaves the scope point"
    );
    assert!(
        age >= 0.0 && age != core::sim::BLAST_AGE,
        "{map}: a direct hit, age {age}"
    );
    println!("{map} at {dist} m: struck at age {age:.3} s on tick {at}");
    // The drawn line: from the shot's origin to the impact on the bot.
    let lines = bolts.lines();
    assert!(
        lines
            .iter()
            .any(|&(a, b)| a.distance(origin) < 1e-3 && b.distance(impact) < 1e-3),
        "{map}: a line from {origin} to {impact}, drawn {lines:?}"
    );
    // On the camera's axis: the eye looks down the line.
    let cam = s.walk.camera();
    let off = (origin - cam.position).cross(dir).length();
    assert!(off < 1e-2, "{map}: the line is {off} m off the eye");
}

/// A first-update hit (2000 m/s: inside 66.7 m), on Kamino.
#[test]
fn a_scoped_shot_inside_one_update_is_direct_and_drawn_from_the_scope() {
    scoped_shot(
        "bf2_kam1",
        "bf2",
        "rep_inf_ep2_jettrooper_sniper",
        ["rep", "cis"],
        40.0,
    );
}

/// A hit on the second update, and BF1's marksman.
#[test]
fn a_longer_scoped_shot_is_drawn_to_the_bot_too() {
    scoped_shot(
        "bf2_nab2",
        "bf2",
        "rep_inf_ep2_jettrooper_sniper",
        ["rep", "cis"],
        80.0,
    );
    scoped_shot("bf1_tat2", "bf1", "all_inf_marksman", ["all", "imp"], 40.0);
}

/// One sim tick for the pictures: the soldiers' poses and the bolts follow.
fn step(
    s: &mut Scoped,
    vis: &mut game::soldier_vis::SoldierVisuals,
    bolts: &mut game::bolt_vis::BoltVis,
    b: u32,
) {
    tick(s, b);
    vis.update(&s.walk, core::TICK_DT);
    bolts.set_fire_points(vis.fire_points(&s.walk));
    bolts.update(&s.walk, core::TICK_DT);
}

/// The scoped view as the window draws it: the scene, the soldiers, the
/// bolts and the HUD, before and after the shot, at both zoom steps, on
/// BF2 Kamino and BF1 Tatooine; PNGs in `engine/artifacts/lead_tmp/scope/`.
#[test]
#[ignore = "requires a GPU adapter + the baked maps"]
fn scope_pictures() {
    let root = game::assets::baked_root();
    let out = std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../artifacts/lead_tmp/scope"
    ));
    std::fs::create_dir_all(&out).unwrap();
    for (map, game, class, sides, dist) in [
        (
            "bf2_kam1",
            "bf2",
            "rep_inf_ep2_jettrooper_sniper",
            ["rep", "cis"],
            40.0,
        ),
        ("bf1_tat2", "bf1", "all_inf_marksman", ["all", "imp"], 40.0),
    ] {
        let mut s = scoped(map, game, class, sides, dist).expect("bake and lane");
        let mut r = render::Renderer::new_headless(SIZE[0], SIZE[1]).expect("headless renderer");
        let gpu = game::viewer::upload_scene(&mut r, &s.scene).expect("upload");
        let mut hud = game::hud_feed::HudView::for_renderer(&root, &r).expect("the BF2 HUD bake");
        let mut vis = game::soldier_vis::SoldierVisuals::for_walk(&mut r, &root, &s.scene, &s.walk);
        let mut bolts = game::bolt_vis::BoltVis::new(&root, s.game);
        let feed = game::event_feed::EventFeed::for_walk(&root, s.game, &s.walk);
        let mut picture = |s: &Scoped,
                           r: &mut render::Renderer,
                           vis: &game::soldier_vis::SoldierVisuals,
                           bolts: &mut game::bolt_vis::BoltVis,
                           name: &str| {
            let m = model(s, &feed);
            hud.update(&m, 0.0, SIZE);
            hud.update(&m, 1.0, SIZE);
            let cam = s.walk.camera();
            let mut draws = gpu.draws.clone();
            draws.extend(vis.weapon_draws(&s.walk));
            let skinned = vis.draws(&s.walk);
            bolts.submit(r, &s.walk, cam.position, &[]);
            r.render_frame(
                &cam,
                &draws,
                &skinned,
                gpu.clear,
                Some(&mut |dev, q, enc, view, size| hud.paint(dev, q, enc, view, size)),
            )
            .expect("render");
            let (rgba, w, h) = r.capture_rgba().expect("capture");
            let png = render::encode_png_rgb(&rgba, w, h).expect("png");
            let path = out.join(format!("{map}_{name}.png"));
            std::fs::write(&path, png).unwrap();
            println!("{} (bolt lines {:?})", path.display(), bolts.lines());
        };
        for _ in 0..10 {
            step(&mut s, &mut vis, &mut bolts, 0);
        }
        picture(&s, &mut r, &vis, &mut bolts, "0_unzoomed");
        step(&mut s, &mut vis, &mut bolts, buttons::ZOOM);
        for _ in 0..15 {
            step(&mut s, &mut vis, &mut bolts, 0);
        }
        picture(&s, &mut r, &vis, &mut bolts, "1_scope_x2");
        step(&mut s, &mut vis, &mut bolts, buttons::FIRE);
        picture(&s, &mut r, &vis, &mut bolts, "2_scope_x2_shot");
        for _ in 0..6 {
            step(&mut s, &mut vis, &mut bolts, 0);
        }
        picture(&s, &mut r, &vis, &mut bolts, "3_scope_x2_after_0.2s");
        step(&mut s, &mut vis, &mut bolts, buttons::ZOOM);
        for _ in 0..15 {
            step(&mut s, &mut vis, &mut bolts, 0);
        }
        picture(&s, &mut r, &vis, &mut bolts, "4_scope_x8");
    }
}
