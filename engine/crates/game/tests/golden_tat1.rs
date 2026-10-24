//! Tier-2 visual golden: headless-render baked TAT1 at the shared GOLDEN_CAM
//! pose and perceptual-diff against the committed golden PNG.
//!
//! Ignored by default — needs a GPU adapter AND locally baked TAT1 + common_bf1:
//!   cargo run -p bake -- map ../extracted/.../TAT/TAT1.LVL assets_baked/tat1
//!   cargo test -p game --test golden_tat1 -- --ignored --nocapture
//! Regenerate the golden deliberately (visual change reviewed first):
//!   cargo run -p game -- --map assets_baked/tat1 --headless \
//!     --capture-frame goldens/tat1_overview.png

mod common;

use game::viewer::{camera_from, upload_scene, GOLDEN_CAM};

#[test]
#[ignore = "requires a GPU adapter + baked assets_baked/tat1"]
fn tat1_overview_matches_golden() {
    common::force_ps2_tier();
    let baked = game::assets::baked_root().join("tat1");
    let golden = common::goldens_dir().join("tat1_overview.png");
    if !baked.join("manifest.ron").exists() {
        panic!("bake TAT1 first (see test header) — golden test can't skip silently");
    }

    // Share the binary's loader: conquest layer plus the source game's
    // COMMON models (posts, droids and turrets).
    let (scene, load) = game::assets::load_battle_scene(&baked, "con").expect("scene");
    let (_, common) = load
        .common
        .expect("bake common_bf1 before comparing the golden");
    common.expect("common_bf1 must load; a partial scene cannot certify the golden");
    let mut r = render::Renderer::new_headless(1280, 720).expect("headless renderer");
    let gpu = upload_scene(&mut r, &scene).expect("upload");
    r.render(&camera_from(GOLDEN_CAM), &gpu.draws, gpu.clear)
        .expect("render");
    let (rgba, w, h) = r.capture_rgba().expect("capture");

    // Deliberately NOT common::assert_golden: this golden is written by the
    // game binary's own --capture-frame path, so the test must compare even
    // during a regeneration run.
    if let Some(msg) = common::compare_golden(&golden, &rgba, w, h) {
        panic!("{msg}");
    }
}
