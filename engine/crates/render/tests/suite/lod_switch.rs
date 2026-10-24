//! Headless GPU tests of the far scene's model / LOWD switch (card LOD,
//! `render::LodSwitch`, `analysis/re-lod.md` §7): a geometry's model and its
//! LOWD are two draws over the same pixels, and the shader's `far_cull` gives
//! them ONE dithered threshold per pixel between the band's two ends — the
//! model keeps the fragments nearer than it, the LOWD those past it, so every
//! pixel shows exactly one of the two. The LOWD's own far end is its cull.
//!
//! The quad fills the view at z = -5 under the default camera (70° square
//! view), so every fragment is 5.0 .. 7.04 m from the eye.
//! Ignored by default — needs a real adapter (`scripts/gpu_tests.sh`).

use glam::Mat4;
use render::{Camera, DrawCmd, LightPath, LodSide, LodSwitch, Renderer};

use crate::common;
use common::{headless, view_quad, N};

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

/// A full-view flat quad on `side` of the band `[begin, end]`, culled at `far`.
fn draw(
    r: &mut Renderer,
    color: [f32; 4],
    side: LodSide,
    band: [f32; 2],
    far: Option<f32>,
) -> DrawCmd {
    let mesh = view_quad(r, -5.0);
    DrawCmd::flat(mesh, color, Mat4::IDENTITY)
        .with_light(LightPath::Unlit)
        .with_cull(far)
        .with_lod(Some(LodSwitch {
            fade_begin: band[0],
            fade_end: band[1],
            side,
        }))
}

/// Render `draws` over black and count the pixels that are red-, blue- and
/// still black.
fn counts(r: &mut Renderer, draws: &[DrawCmd]) -> (u32, u32, u32) {
    r.render(&Camera::default(), draws, [0.0, 0.0, 0.0, 1.0])
        .expect("frame");
    let (buf, _, _) = r.capture_rgba().expect("capture");
    let (mut red, mut blue, mut black) = (0, 0, 0);
    for p in buf.as_chunks::<4>().0 {
        match (p[0] > 128, p[2] > 128) {
            (true, false) => red += 1,
            (false, true) => blue += 1,
            (false, false) => black += 1,
            (true, true) => panic!("a pixel both red and blue: {p:?}"),
        }
    }
    (red, blue, black)
}

#[test]
#[ignore = "requires a GPU adapter (run with --ignored on the NVIDIA box)"]
fn model_and_lowd_split_every_pixel_by_one_threshold() {
    let mut r = headless();
    // A band whose dithered threshold falls among the fragments: 0..20 m,
    // so a pixel keeps the model where its dither u >= d / 20 (0.25..0.35).
    let band = [0.0, 20.0];
    let model = draw(&mut r, RED, LodSide::Model, band, None);
    let lowd = draw(&mut r, BLUE, LodSide::Lowd, band, Some(100.0));
    let (m, _, _) = counts(&mut r, &[model]);
    let (_, l, _) = counts(&mut r, &[lowd]);
    let all = N * N;
    assert!(
        m > 0 && l > 0,
        "both sides draw inside the band: model {m}, LOWD {l}"
    );
    assert!(
        m > l,
        "the threshold nearer the band's start keeps more model: {m} vs {l}"
    );
    assert_eq!(
        m + l,
        all,
        "the two sides never overlap and never leave a gap"
    );
    let (red, blue, black) = counts(&mut r, &[model, lowd]);
    assert_eq!(black, 0, "no pixel shows neither");
    assert_eq!(
        (red, blue),
        (m, l),
        "together each pixel keeps its own side"
    );
}

#[test]
#[ignore = "requires a GPU adapter (run with --ignored on the NVIDIA box)"]
fn nearer_than_the_band_the_model_past_it_the_lowd() {
    let mut r = headless();
    let all = N * N;
    // The band starts past every fragment: the model alone.
    let near = [10.0, 20.0];
    let m = draw(&mut r, RED, LodSide::Model, near, None);
    let l = draw(&mut r, BLUE, LodSide::Lowd, near, Some(100.0));
    assert_eq!(counts(&mut r, &[m, l]), (all, 0, 0));
    // The band ends nearer than every fragment: the LOWD alone.
    let past = [1.0, 2.0];
    let m = draw(&mut r, RED, LodSide::Model, past, None);
    let l = draw(&mut r, BLUE, LodSide::Lowd, past, Some(100.0));
    assert_eq!(counts(&mut r, &[m, l]), (0, all, 0));
    // The model's own cull nearer than the band still ends it.
    let m = draw(&mut r, RED, LodSide::Model, near, Some(3.0));
    assert_eq!(counts(&mut r, &[m]), (0, 0, all));
}

#[test]
#[ignore = "requires a GPU adapter (run with --ignored on the NVIDIA box)"]
fn no_lowd_past_far_scene_range() {
    let mut r = headless();
    let l = draw(&mut r, BLUE, LodSide::Lowd, [1.0, 2.0], Some(3.0));
    assert_eq!(counts(&mut r, &[l]), (0, 0, N * N));
}
