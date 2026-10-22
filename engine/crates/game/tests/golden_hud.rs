//! Tier-2 visual golden: the HUD — BF2's own tree (`game::hud_feed`), with
//! two event lines in its message box, the statistics pop-up after a kill,
//! an enemy's name and health bar at the top (card HTGT),
//! BF1's squad order cross half a second after FOLLOW ME took two units
//! (card SQHUD), and the egui lines over it — painted over the TAT1 GOLDEN_CAM frame
//! with a fixed HudModel, perceptual-diffed against goldens/tat1_hud.png;
//! and the hit-direction chevrons (card HATK) against goldens/hud_ambush.png.
//!
//! Ignored by default — needs a GPU adapter AND baked assets_baked/tat1:
//!   cargo test -p game --test golden_hud -- --ignored
//! Regenerate deliberately (review the PNG first):
//!   UPDATE_GOLDENS=1 cargo test -p game --test golden_hud -- --ignored

mod common;

use game::viewer::{camera_from, upload_scene, GOLDEN_CAM};

fn model() -> ui::hud::HudModel {
    ui::hud::HudModel {
        team_names: ["all".into(), "imp".into()],
        tickets: [247, 239],
        bleed: [0.0; 2],
        posts: [3, 2],
        hp: 220.0,
        hp_max: 300.0,
        energy: 64.0,
        energy_max: 100.0,
        clip: 31,
        spare: 250,
        reloading: false,
        weapon: Some(ui::hud::HudWeapon {
            key: "all_weap_inf_rifle".into(),
            class: String::new(),
            icon: None,
            name: String::new(),
            total: Some(281),
            clip_fraction: 31.0 / 50.0,
        }),
        weapon2: None,
        // An enemy at 60 % under the reticle: the target readout (card HTGT).
        reticle: Some(-1),
        target: Some(ui::hud::HudTarget {
            id: 1,
            name: "Stormtrooper".into(),
            health_fraction: 0.6,
            relation: -1,
            team0: false,
            shield: None,
        }),
        reticle_y: game::walk::ps2_reticle_y("bf1"),
        dead: false,
        respawn_in: None,
        outcome: None,
        time_left: None,
        messages: Vec::new(),
        stats: Some(ui::hud::HudStats::default()),
        player_team: 0,
        map: None,
        squad: None,
        capture: None,
        ambush: Vec::new(),
        spawn: None,
    }
}

/// FOLLOW ME, two followers, half a second after the press.
fn cross() -> ui::hud_squad::SquadCross {
    ui::hud_squad::SquadCross {
        labels: ["MOVE OUT", "HOLD POSITION", "FOLLOW ME", "AT EASE"].map(str::to_owned),
        arm: ui::hud_squad::RIGHT,
        count: Some(2),
        timer: ui::hud_squad::SquadCross::timer_at(0.5).expect("shown"),
    }
}

/// The event lines of the golden: an enemy's capture, the player's death as
/// the PS2 printed it on HOT1 (`engine/artifacts/pcsx2/ref/evmsg/`).
fn messages() -> Vec<ui::hud::HudMessage> {
    use ui::hud::message_color::{ENEMY, OWN};
    [
        ("Empire captured the Homestead", ENEMY),
        ("Ben Pulastra killed Player 1", OWN),
    ]
    .into_iter()
    .map(|(text, color)| ui::hud::HudMessage {
        text: text.into(),
        color,
    })
    .collect()
}

#[test]
#[ignore = "requires a GPU adapter + baked assets_baked/tat1"]
fn tat1_hud_matches_golden() {
    common::force_ps2_tier();
    let baked = game::assets::baked_root().join("tat1");
    if !baked.join("manifest.ron").exists() {
        panic!("bake TAT1 first (see test header) — golden test can't skip silently");
    }
    let scene = game::assets::load_scene(&baked).expect("scene");
    let mut r = render::Renderer::new_headless(1280, 720).expect("headless renderer");
    let gpu = upload_scene(&mut r, &scene).expect("upload");
    let mut overlay = ui::overlay::EguiOverlay::headless(r.device(), render::HEADLESS_FORMAT);
    // The minimap as the golden camera sees the map: its posts as authored.
    let cam = camera_from(GOLDEN_CAM);
    let m = ui::hud::HudModel {
        map: Some(game::minimap::still(&scene, cam.position, cam.yaw)),
        squad: Some(cross()),
        ..model()
    };
    let mut hud = game::hud_feed::HudView::for_renderer(&game::assets::baked_root(), &r)
        .expect("the BF2 HUD bake (bake common)");
    // The first update settles the fades (the statistics pop-up of the
    // battle's start is gone again); then the lines arrive and a kill and
    // its point are scored, and the next frames admit the lines one by one.
    hud.update(&m, 0.0, [1280, 720]);
    let scored = ui::hud::HudModel {
        messages: messages(),
        stats: Some(ui::hud::HudStats {
            points: 1,
            kills: 1,
            ..Default::default()
        }),
        ..m.clone()
    };
    hud.update(&scored, 0.0, [1280, 720]);
    let m = ui::hud::HudModel {
        messages: Vec::new(),
        ..scored
    };
    // Three lines: the capture breaks at the box's edge.
    for _ in 0..3 {
        hud.update(&m, 0.6, [1280, 720]);
    }
    let paint = overlay.begin_headless([1280, 720], 1.0, |ctx| ui::hud::draw_hud(ctx, &m));
    r.render_with_overlay(
        &camera_from(GOLDEN_CAM),
        &gpu.draws,
        gpu.clear,
        Some(&mut |dev, q, enc, view, size| {
            hud.paint(dev, q, enc, view, size);
            overlay.paint(&paint, dev, q, enc, view, size)
        }),
    )
    .expect("render");
    let (rgba, w, h) = r.capture_rgba().expect("capture");

    common::assert_golden(&common::goldens_dir().join("tat1_hud.png"), &rgba, w, h);
}

/// Card HATK: the hit-direction chevrons over the HUD (`ui::hud_ambush`,
/// `analysis/re-hud-attacker.md`) - the right and bottom ones half a second
/// into their fade, the left and top ones just lit - perceptual-diffed
/// against goldens/hud_ambush.png. View it beside the PS2's frames,
/// `engine/artifacts/lead_tmp/hatk/shots/edges.png`.
#[test]
#[ignore = "requires a GPU adapter + baked assets_baked/tat1"]
fn tat1_hud_ambush_matches_golden() {
    use ui::hud_ambush::Edge;
    common::force_ps2_tier();
    let baked = game::assets::baked_root().join("tat1");
    if !baked.join("manifest.ron").exists() {
        panic!("bake TAT1 first (see test header) — golden test can't skip silently");
    }
    let scene = game::assets::load_scene(&baked).expect("scene");
    let mut r = render::Renderer::new_headless(1280, 720).expect("headless renderer");
    let gpu = upload_scene(&mut r, &scene).expect("upload");
    let m = model();
    let mut hud = game::hud_feed::HudView::for_renderer(&game::assets::baked_root(), &r)
        .expect("the BF2 HUD bake (bake common)");
    // The first update settles the fades; then two hits, half a second,
    // two more.
    hud.update(&m, 0.0, [1280, 720]);
    let hit = |edges: &[Edge]| ui::hud::HudModel {
        ambush: edges.to_vec(),
        ..m.clone()
    };
    hud.update(&hit(&[Edge::Right, Edge::Bottom]), 0.0, [1280, 720]);
    hud.update(&m, 0.5, [1280, 720]);
    hud.update(&hit(&[Edge::Left, Edge::Top]), 0.0, [1280, 720]);
    r.render_with_overlay(
        &camera_from(GOLDEN_CAM),
        &gpu.draws,
        gpu.clear,
        Some(&mut |dev, q, enc, view, size| hud.paint(dev, q, enc, view, size)),
    )
    .expect("render");
    let (rgba, w, h) = r.capture_rgba().expect("capture");

    common::assert_golden(&common::goldens_dir().join("hud_ambush.png"), &rgba, w, h);
}
