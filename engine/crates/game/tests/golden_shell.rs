//! Tier-2 visual golden: the instant-action shell painted headless (no map)
//! with a fixed ShellModel, diffed against goldens/shell.png.
//!   cargo test -p game --test golden_shell -- --ignored
//!   UPDATE_GOLDENS=1 cargo test -p game --test golden_shell -- --ignored

mod common;

fn model() -> ui::shell::ShellModel {
    ui::shell::ShellModel {
        maps: vec![("tat1".into(), "bf1".into()), ("tat2".into(), "bf2".into())],
        map_idx: 1,
        mode: "con".into(),
        teams: [2, 3],
        ai_per_team: 16,
        base_tickets: [150, 150],
        ticket_pct: 100,
        time_limit: 0,
        // START BATTLE (queue TLO put the time-limit row above it).
        cursor: 7,
        career_line: "maintainer — 12 rounds, 7 wins, 240 kills / 96 deaths".into(),
        mission_found: true,
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn shell_matches_golden() {
    common::force_ps2_tier();
    let mut r = render::Renderer::new_headless(1280, 720).expect("headless renderer");
    let mut overlay = ui::overlay::EguiOverlay::headless(r.device(), render::HEADLESS_FORMAT);
    let m = model();
    let paint = overlay.begin_headless([1280, 720], 1.0, |ctx| {
        ui::shell::draw_shell(ctx, &m);
    });
    r.render_with_overlay(
        &render::Camera::default(),
        &[],
        [0.05, 0.06, 0.10, 1.0],
        Some(&mut |dev, q, enc, view, size| overlay.paint(&paint, dev, q, enc, view, size)),
    )
    .expect("render");
    let (rgba, w, h) = r.capture_rgba().expect("capture");

    common::assert_golden(&common::goldens_dir().join("shell.png"), &rgba, w, h);
}

/// The BF2 PS2 shell's front screens (card BSHLA) as the HUD pass paints
/// them: each over a fixed frame of its movie (`shell_main` segment 0 frame
/// 300 for the title and the main menu, `shell_sub_left`'s first segment
/// frame 150 for Singleplayer and Options), the rows all revealed, the
/// prompt at its brightest. Needs the BF2 common bake with the shell.
/// `SHELL_PS2_OUT=<dir>`: also each at 640 x 480, the PS2 frames' size.
fn front_screens() -> Vec<(&'static str, ui::shell_ps2::ShellMenus)> {
    use ui::menu::MenuInput;
    use ui::shell_ps2::{Page, ShellMenus};
    let texts = game::event_feed::Texts::load(&game::assets::baked_root(), "bf2");
    let text = |key: &str, english: &str| {
        texts
            .bf2_text(key)
            .map_or_else(|| english.to_owned(), |t| t.trim_end().to_owned())
    };
    let base = ShellMenus::new(&text);
    let settle = |mut m: ShellMenus, page: Page| {
        m.page = page;
        // Five seconds on the page (every row shown), the prompt's clock at
        // the top of its swing.
        m.step(MenuInput::default(), false, 0.0);
        m.on_page = 5.0;
        m.clock = 0.0;
        m
    };
    vec![
        ("title", settle(base.clone(), Page::Title)),
        ("main", settle(base.clone(), Page::Main)),
        ("singleplayer", settle(base.clone(), Page::Singleplayer)),
        ("options", settle(base, Page::Options)),
    ]
}

fn front_screen(m: &ui::shell_ps2::ShellMenus, size: [u32; 2]) -> (Vec<u8>, u32, u32) {
    let assets = game::assets::baked_root();
    let movie = game::shell_movie::ShellMovie::load(&assets).expect("the shell bake (bake common)");
    let (seg, frame) = if m.movie() == ui::shell_ps2::MOVIE_MAIN {
        (0, 300)
    } else {
        (0, 150)
    };
    let (rgba, w, h) = movie.frame_now(m.movie(), seg, frame).expect("movie frame");
    let mut r = render::Renderer::new_headless(size[0], size[1]).expect("headless renderer");
    let mut view =
        game::hud_feed::HudView::for_renderer(&assets, &r).expect("the BF2 HUD bake (bake common)");
    view.set_movie_frame(rgba, w, h);
    view.update_shell(m, true, size);
    r.render_with_overlay(
        &render::Camera::default(),
        &[],
        [0.0, 0.0, 0.0, 1.0],
        Some(&mut |dev, q, enc, target, size| view.paint(dev, q, enc, target, size)),
    )
    .expect("render");
    r.capture_rgba().expect("capture")
}

#[test]
#[ignore = "requires a GPU adapter + the BF2 common bake"]
fn ps2_front_screens_match_golden() {
    common::force_ps2_tier();
    // A bake from before card BSHLA has no shell: land.py's branch gates
    // render the shared bake before its rebake, so this skips, saying so,
    // there; after the rebake it draws.
    if game::shell_movie::load_index(&game::assets::baked_root()).is_none() {
        eprintln!("skip: no shell in the BF2 common bake (rebake common)");
        return;
    }
    let ps2 = std::env::var_os("SHELL_PS2_OUT").map(std::path::PathBuf::from);
    let mut failures = Vec::new();
    for (name, m) in front_screens() {
        let (rgba, w, h) = front_screen(&m, [1280, 720]);
        let golden = common::goldens_dir().join(format!("shell_{name}.png"));
        if let Some(f) = common::check_golden(&golden, &rgba, w, h) {
            failures.push(f);
        }
        if let Some(dir) = &ps2 {
            let (rgba, w, h) = front_screen(&m, [640, 480]);
            image::RgbaImage::from_raw(w, h, rgba)
                .expect("frame")
                .save(dir.join(format!("shell_{name}_640.png")))
                .expect("write");
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
