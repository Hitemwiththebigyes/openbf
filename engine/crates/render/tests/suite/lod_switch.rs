//! Headless GPU tests of the far scene's model / LOWD switch (card LOD,
//! `render::LodSwitch`, `analysis/re-lod.md` §7): a geometry's model and its
//! LOWD are two draws over the same pixels, and the frame draws exactly one of
//! them, the whole instance at once, by its anchor's distance from the eye.
//! No pixel shows a mix of the two (the old dithered band did).
//!
//! The quad fills the view at z = -5 under the default camera (70° square
//! view), so its anchor at (0, 0, -5) is 5 m from the eye.
//! Ignored by default — needs a real adapter (`scripts/gpu_tests.sh`).

use glam::{Mat4, Vec3};
use render::{Camera, DrawCmd, LightPath, LodSide, LodSwitch, Renderer};

use crate::common;
use common::{headless, view_quad, N};

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

/// A full-view flat quad on `side` of a switch at `at` metres, culled at `far`.
fn draw(r: &mut Renderer, color: [f32; 4], side: LodSide, at: f32, far: Option<f32>) -> DrawCmd {
    let mesh = view_quad(r, -5.0);
    DrawCmd::flat(mesh, color, Mat4::IDENTITY)
        .with_light(LightPath::Unlit)
        .with_cull(far)
        .with_lod(Some(LodSwitch {
            at,
            anchor: Vec3::new(0.0, 0.0, -5.0),
            radius: 0.0,
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
fn the_whole_instance_is_its_model_nearer_than_the_switch_and_its_lowd_past_it() {
    let mut r = headless();
    let all = N * N;
    // Anchor 5 m away, switch at 6 m: the model alone, every pixel.
    let m = draw(&mut r, RED, LodSide::Model, 6.0, None);
    let l = draw(&mut r, BLUE, LodSide::Lowd, 6.0, Some(100.0));
    assert_eq!(counts(&mut r, &[m, l]), (all, 0, 0));
    // Switch at 4 m: the LOWD alone, even though the quad's far corners are
    // past 6 m and its centre nearer — no split across the instance.
    let m = draw(&mut r, RED, LodSide::Model, 4.0, None);
    let l = draw(&mut r, BLUE, LodSide::Lowd, 4.0, Some(100.0));
    assert_eq!(counts(&mut r, &[m, l]), (0, all, 0));
    // The model's own cull still ends it.
    let m = draw(&mut r, RED, LodSide::Model, 6.0, Some(3.0));
    assert_eq!(counts(&mut r, &[m]), (0, 0, all));
}

#[test]
#[ignore = "requires a GPU adapter (run with --ignored on the NVIDIA box)"]
fn no_lowd_past_its_cull() {
    let mut r = headless();
    let l = draw(&mut r, BLUE, LodSide::Lowd, 1.0, Some(3.0));
    assert_eq!(counts(&mut r, &[l]), (0, 0, N * N));
}
