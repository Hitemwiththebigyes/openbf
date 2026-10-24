//! Headless GPU test for the PS2 detail-map slot: a `<name>_dtl` companion
//! modulates the base by `detail.a * 2`, applied in GAMMA space (the GS
//! multiplies the stored texels), i.e. `pow(f, 2.2)` against our linearised
//! base. Ignored by default — needs a real adapter.
//!   scripts/gpu_tests.sh --render

use glam::Mat4;
use render::{Camera, DetailDesc, DrawCmd};

use crate::common;
use common::{headless, tex_desc, view_quad};

/// Linear value of one sRGB-encoded byte.
fn linear(b: u8) -> f64 {
    let c = f64::from(b) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn render_quad(detail: bool) -> (Vec<u8>, u32) {
    let mut r = headless();
    // Base: one dark grey texel (keeps the brightened half off the ceiling).
    let base: [u8; 4] = [64, 64, 64, 255];
    // Detail: two texels, alpha 64 (f = 0.50) then 192 (f = 1.51). RGB is 0,
    // exactly as the discs store them — only alpha is read.
    let dtl: [u8; 8] = [0, 0, 0, 64, 0, 0, 0, 192];
    let (base_mips, dtl_mips): ([&[u8]; 1], [&[u8]; 1]) = ([&base], [&dtl]);
    let mut desc = tex_desc(1, 1, &base_mips, "base");
    desc.detail = detail.then_some(DetailDesc {
        width: 2,
        height: 1,
        mips: &dtl_mips,
        scale: 1.0,
    });
    let tex = r.add_texture(&desc).expect("texture upload");

    // The view-spanning quad, UV 0..1 across x: the left half samples the
    // dark detail texel, the right half the bright one.
    let mesh = view_quad(&mut r, -5.0);
    let mut draw = DrawCmd::flat(mesh, [1.0, 1.0, 1.0, 1.0], Mat4::IDENTITY);
    draw.texture = Some(tex);
    r.render(&Camera::default(), &[draw], [0.0, 0.0, 0.0, 1.0])
        .expect("frame");
    let (rgba, w, _) = r.capture_rgba().expect("capture");
    (rgba, w)
}

#[test]
#[ignore = "requires a GPU adapter (run with --ignored on the NVIDIA box)"]
fn detail_map_modulates_in_gamma_space() {
    let (plain, w) = render_quad(false);
    let (with_detail, _) = render_quad(true);
    let at = |buf: &[u8], x: u32| linear(buf[((32 * w + x) * 4) as usize]);

    let (dark_ratio, bright_ratio) = (
        at(&with_detail, 16) / at(&plain, 16),
        at(&with_detail, 48) / at(&plain, 48),
    );
    // f = 2 * a/255 -> 0.502 and 1.506; gamma-space modulation squares that up
    // to 0.214 and 2.46. A linear-space multiply would land on 0.50 / 1.51,
    // which is what these bounds are here to exclude.
    assert!(
        (0.15..0.30).contains(&dark_ratio),
        "dark half ratio {dark_ratio:.3}, expected ~0.214 (pow(0.502, 2.2))"
    );
    assert!(
        (2.0..3.0).contains(&bright_ratio),
        "bright half ratio {bright_ratio:.3}, expected ~2.46 (pow(1.506, 2.2))"
    );

    // Without a detail map the two halves must be identical (the fallback
    // binding must never leak into the shader).
    assert!(
        (at(&plain, 16) - at(&plain, 48)).abs() < 1e-6,
        "control render should be flat across the quad"
    );
}
