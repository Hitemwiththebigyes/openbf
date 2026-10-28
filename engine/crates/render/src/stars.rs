//! The sky's star field: `DomeInfo.Stars()` (issue #16, card STARS;
//! `analysis/re-stars.md`, [O]).
//!
//! Both games build the stars once, at load, with their own generator (B1
//! `FUN_002e6e50`, B2 `FUN_003bffd8`), and draw them every frame in the sky
//! pass, after the first dome shell and before the rest (B1 `FUN_002f2308`
//! → `FUN_002e7d78` → `FUN_002f33c8`; B2 `FUN_003c10a8` → `FUN_003ccb18`):
//! - `NumStars` points spread evenly over the upper hemisphere's area, in 36
//!   cells: 16 sectors of 22.5° for sin(elevation) 0..0.5, 16 for 0.5..sin 60°
//!   and 4 quarter caps above, each cell's count by a carried rounding of
//!   its share of the area;
//! - one Numerical Recipes LCG (×0x19660d + 0x3c6ef35f) seeded with
//!   `RandomSeed ^ 0xc71c2830`, eight steps per star: azimuth, elevation,
//!   brightness, tint ([`unit`] turns two steps into [0, 1));
//! - a brightness: `BrightStarPercent` of the stars at 1, the rest
//!   `x³ + AlphaMin·(1 − x³)`; a tint by `ColorSaturation` that takes red
//!   away from some stars and blue from others ([`star_rgb`]);
//! - each star a flat, untextured screen-facing square 1/256 of the
//!   PS2's screen width across, whatever the FOV ([`half_size`]), coloured
//!   by its bytes × the authored `Color`, ADDED to the sky (GS ALPHA 0x48:
//!   the draw state `0x100048`), never fogged;
//! - a twinkle where `TwinkleFactor` > 0: per star, an alpha dip that eases
//!   between two random targets `TwinkleFrequency` times a second, twice as
//!   deep at the horizon as at the zenith ([`twinkle_alpha`]);
//! - BF2's `EnableBottom`: every cell drawn again mirrored under the horizon.
//!
//! `StarTexture` is authored (`fx_star`); the draw (PRIM 0x48, no texture bit)
//! does not use it.

use crate::grass::GrassVertex;
use crate::{Camera, Renderer};
use glam::Vec3;

/// The authored field with the loaders' defaults, as `manifest::ManifestStars`
/// carries it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarField {
    pub count: u32,
    pub seed: u32,
    pub twinkle_factor: f32,
    pub twinkle_frequency: f32,
    pub color: [u8; 4],
    pub bright_fraction: f32,
    pub alpha_min: f32,
    pub color_saturation: f32,
    /// Also draw the field mirrored under the horizon (BF2 only).
    pub enable_bottom: bool,
}

/// One generated star: a unit direction from the eye and its colour bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Star {
    pub dir: Vec3,
    pub rgb: [u8; 3],
}

/// The field as built at load: the stars in cell order and where each of
/// the 36 cells starts (the twinkle's random streams restart per cell).
#[derive(Debug, Clone, Default)]
pub(crate) struct StarState {
    pub field: Option<StarField>,
    /// How many sky shells draw before the stars: the base dome's.
    pub slot: usize,
    pub stars: Vec<Star>,
    pub cells: Vec<std::ops::Range<usize>>,
    /// This frame's quads, reused across frames.
    pub vertices: Vec<GrassVertex>,
    pub vertex_buf: Option<(wgpu::Buffer, usize)>,
    pub instance_buf: Option<(wgpu::Buffer, usize)>,
}

const SEED_XOR: u32 = 0xc71c_2830;
/// sin 60°, the ELF's `0.8660254`: the top of the second band.
const SIN60: f32 = 0.866_025_4;
/// π/8 and π/2: the ELF's `0.3926991` and `1.5707964` are these f32s.
const SECTOR: f32 = std::f32::consts::FRAC_PI_8;
const QUARTER: f32 = std::f32::consts::FRAC_PI_2;

fn lcg(x: u32) -> u32 {
    x.wrapping_mul(0x0019_660d).wrapping_add(0x3c6e_f35f)
}

/// Two LCG steps as one float in [0, 1): the high halves of both words as a
/// 23-bit mantissa under exponent 0 (`(a >> 16 | b & 0xffff0000) >> 9`).
fn unit(a: u32, b: u32) -> f32 {
    f32::from_bits(((a >> 16) | (b & 0xffff_0000)) >> 9 | 0x3f80_0000) - 1.0
}

/// A star's colour bytes from its two random draws (B1 `FUN_002e7bb8`).
pub fn star_rgb(f: &StarField, p1: f32, p2: f32) -> [u8; 3] {
    let mut b = 1.0;
    let dim = 1.0 - f.bright_fraction;
    if p1 <= dim {
        let x = p1 / dim;
        let x = x * x * x;
        b = x + f.alpha_min * (1.0 - x);
    }
    let sat = f.color_saturation;
    let (mut r, mut g, mut bl) = (b, b, b);
    if p2 > 0.7 {
        r = b * (1.0 - sat * (p2 - 0.7) * 3.333_333_3);
        if p2 > 0.85 {
            let k = sat * (p2 - 0.85);
            g = b * (1.0 - (k + k) * 3.333_333_3);
        }
    }
    if p2 < 0.4 {
        bl = b * (1.0 - sat * (0.4 - p2) * 2.5);
        if p2 < 0.26668 {
            g *= 1.0 - sat * (0.26668 - p2) * 3.75;
        }
    }
    let byte = |v: f32| (v * 255.0) as i32 as u8;
    [byte(r), byte(g), byte(bl)]
}

/// Build the field (B1 `FUN_002e6e50`): the stars in cell order and each
/// cell's range.
pub(crate) fn generate(f: &StarField) -> (Vec<Star>, Vec<std::ops::Range<usize>>) {
    let n = f.count as f32;
    // Per band: its cells, each cell's share of NumStars, its sin(elevation)
    // range and its sector width.
    let bands = [
        (16usize, n * 0.03125, 0.0, 0.5, SECTOR),
        (16, n * ((SIN60 - 0.5) * 0.0625), 0.5, SIN60, SECTOR),
        (4, n * ((1.0 - SIN60) * 0.25), SIN60, 1.0, QUARTER),
    ];
    let mut x = f.seed ^ SEED_XOR;
    let mut acc = 0.0f32;
    let mut stars = Vec::with_capacity(f.count as usize + 36);
    let mut cells = Vec::with_capacity(36);
    for (cells_in_band, share, lo, hi, width) in bands {
        for cell in 0..cells_in_band {
            acc += share;
            let k = (acc + 0.5).floor();
            acc -= k;
            let start = stars.len();
            for _ in 0..k as u32 {
                let s1 = lcg(x);
                let s2 = lcg(s1);
                let s3 = lcg(s2);
                let s4 = lcg(s3);
                let e = lo + unit(s3, s4) * (hi - lo);
                let az = cell as f32 * width + unit(s1, s2) * width;
                let c = (1.0 - e * e).sqrt();
                let (sin, cos) = az.sin_cos();
                let s5 = lcg(s4);
                let s6 = lcg(s5);
                let s7 = lcg(s6);
                x = lcg(s7);
                stars.push(Star {
                    dir: Vec3::new(c * cos, e, c * sin),
                    rgb: star_rgb(f, unit(s5, s6), unit(s7, x)),
                });
            }
            cells.push(start..stars.len());
        }
    }
    (stars, cells)
}

/// The twinkle's alpha byte for each star of one cell at time `t` seconds
/// (B1 `FUN_002f33c8`): two LCG streams, seeded by the current and the next
/// step of `t · TwinkleFrequency`, one target per star from each, eased
/// linearly between them; the dip is halved at the zenith. 255 when the field
/// does not twinkle.
pub(crate) fn twinkle_alpha(f: &StarField, cell: &[Star], t: f32, out: &mut Vec<u8>) {
    out.clear();
    if f.twinkle_factor <= 0.0 {
        out.resize(cell.len(), 255);
        return;
    }
    let steps = t * f.twinkle_frequency;
    let k = steps as u32;
    let frac = steps - k as f32;
    let (mut a, mut b) = (k ^ SEED_XOR, k.wrapping_add(1) ^ SEED_XOR);
    for s in cell {
        let a1 = lcg(a);
        a = lcg(a1);
        let b1 = lcg(b);
        b = lcg(b1);
        let target = (1.0 - frac) * unit(a1, a) + frac * unit(b1, b);
        let v = 255.0 * (1.0 - f.twinkle_factor * target * (1.0 - s.dir.y * 0.5));
        out.push(v as i32 as u8);
    }
}

/// Half the side of a star's square at unit distance: the PS2 draws it
/// 1/256 of its 4:3 screen's width (`2.0 · w / (2 · near · 256)`, B1
/// `DAT_00518b28` = 2.0), which is 1/192 of the height; the port keeps that
/// height share, so a wider window does not grow the stars.
pub fn half_size(fov_y: f32) -> f32 {
    (fov_y * 0.5).tan() / 192.0
}

/// How far from the eye the squares are placed: inside every camera's far
/// plane; the sky pass draws with no depth, so only the direction shows.
const RADIUS: f32 = 10.0;

impl Renderer {
    /// Replace the map's star field (`None`: no stars). Builds the stars.
    /// They draw after the first `slot` sky shells: the base dome's own
    /// segments and its mirrored copy, so the dome cannot paint over them.
    pub fn set_stars(&mut self, field: Option<StarField>, slot: usize) {
        let (stars, cells) = field.as_ref().map(generate).unwrap_or_default();
        self.stars.field = field;
        self.stars.slot = slot;
        self.stars.stars = stars;
        self.stars.cells = cells;
    }
}

/// This frame's star quads into `out` (two triangles a star, plus the
/// mirrored copy under BF2's `EnableBottom`). The vertex colour is the GS
/// product of the star's bytes and the authored `Color`, gamma bytes as the
/// bolt shader takes them; alpha the twinkle × `Color`'s alpha.
pub(crate) fn build(state: &StarState, camera: &Camera, t: f32, out: &mut Vec<GrassVertex>) {
    out.clear();
    let Some(f) = state.field.as_ref() else {
        return;
    };
    let fwd = camera.forward();
    let right = camera.right();
    let up = right.cross(fwd).normalize();
    let h = half_size(camera.fov_y) * RADIUS;
    let (r, u) = (right * h, up * h);
    let c = f.color.map(|v| v as f32 / 255.0);
    let mut alpha = Vec::new();
    for range in &state.cells {
        let cell = &state.stars[range.clone()];
        twinkle_alpha(f, cell, t, &mut alpha);
        for (s, a) in cell.iter().zip(&alpha) {
            let color = [
                s.rgb[0] as f32 / 255.0 * c[0],
                s.rgb[1] as f32 / 255.0 * c[1],
                s.rgb[2] as f32 / 255.0 * c[2],
                *a as f32 / 255.0 * c[3],
            ];
            let mut quad = |dir: Vec3| {
                let p = camera.position + dir * RADIUS;
                let v = |q: Vec3| GrassVertex {
                    position: q.to_array(),
                    uv: [0.5, 0.5],
                    color,
                    up: 0.0,
                };
                let (a, b, cc, d) = (p - r - u, p + r - u, p + r + u, p - r + u);
                out.extend_from_slice(&[v(a), v(b), v(cc), v(a), v(cc), v(d)]);
            };
            quad(s.dir);
            if f.enable_bottom {
                quad(Vec3::new(s.dir.x, -s.dir.y, s.dir.z));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RHN2's authored block (BF1 RHN2.LVL `DomeInfo.Stars`).
    fn rhn2() -> StarField {
        StarField {
            count: 2000,
            seed: 1,
            twinkle_factor: 0.0,
            twinkle_frequency: 0.0,
            color: [255; 4],
            bright_fraction: 0.10,
            alpha_min: 10.0 * 0.003_921_569,
            color_saturation: 0.2,
            enable_bottom: false,
        }
    }

    #[test]
    fn the_field_has_its_count_in_36_cells_over_the_upper_hemisphere() {
        for count in [300, 1000, 2000] {
            let f = StarField { count, ..rhn2() };
            let (stars, cells) = generate(&f);
            assert_eq!(cells.len(), 36);
            // The carried rounding leaves at most one star over or under.
            assert!(
                (stars.len() as i64 - count as i64).abs() <= 1,
                "{count}: {}",
                stars.len()
            );
            for s in &stars {
                assert!((s.dir.length() - 1.0).abs() < 1e-5);
                assert!((0.0..=1.0).contains(&s.dir.y), "{}", s.dir);
            }
            // Even over the AREA: half the stars below 30°.
            let low = stars.iter().filter(|s| s.dir.y < 0.5).count() as f32;
            assert!((low / stars.len() as f32 - 0.5).abs() < 0.01);
        }
    }

    /// Each cell's stars stay in its sector and band.
    #[test]
    fn a_cell_holds_its_own_sector_and_band() {
        let (stars, cells) = generate(&rhn2());
        for (i, range) in cells.iter().enumerate() {
            let (lo, hi, width, sector) = match i {
                0..16 => (0.0, 0.5, SECTOR, i),
                16..32 => (0.5, SIN60, SECTOR, i - 16),
                _ => (SIN60, 1.0, QUARTER, i - 32),
            };
            for s in &stars[range.clone()] {
                assert!(s.dir.y >= lo - 1e-6 && s.dir.y <= hi + 1e-6);
                let az = s.dir.z.atan2(s.dir.x).rem_euclid(std::f32::consts::TAU);
                let a0 = sector as f32 * width;
                assert!(
                    az >= a0 - 1e-4 && az <= a0 + width + 1e-4,
                    "cell {i} az {az}"
                );
            }
        }
    }

    #[test]
    fn unit_reads_the_high_halves_of_two_steps() {
        assert_eq!(unit(0, 0), 0.0);
        assert_eq!(
            unit(0xffff_0000, 0xffff_0000),
            f32::from_bits(0x3fff_ffff) - 1.0
        );
        let x = lcg(1 ^ SEED_XOR);
        let v = unit(x, lcg(x));
        assert!((0.0..1.0).contains(&v));
    }

    /// The colour rule's corners (B1 `FUN_002e7bb8`).
    #[test]
    fn brightness_and_tint_follow_the_elf() {
        let f = rhn2();
        // Above 1 − BrightStarPercent: full brightness; a middle p2 untinted.
        assert_eq!(star_rgb(&f, 0.95, 0.5), [255, 255, 255]);
        // p1 = 0: the floor, AlphaMin.
        assert_eq!(star_rgb(&f, 0.0, 0.5), [10, 10, 10]);
        // p2 near 1 takes red and some green; near 0 blue and some green.
        let [r, g, b] = star_rgb(&f, 0.95, 0.9);
        assert!(r < g && g < b && b == 255, "{r} {g} {b}");
        let [r, g, b] = star_rgb(&f, 0.95, 0.1);
        assert!(r == 255 && b < g && g < r, "{r} {g} {b}");
    }

    #[test]
    fn no_twinkle_is_full_alpha_and_a_twinkle_dips_twice_as_deep_low() {
        let (stars, cells) = generate(&rhn2());
        let mut out = Vec::new();
        twinkle_alpha(&rhn2(), &stars[cells[0].clone()], 3.7, &mut out);
        assert!(out.iter().all(|a| *a == 255));

        let f = StarField {
            twinkle_factor: 0.5,
            twinkle_frequency: 0.1,
            ..rhn2()
        };
        let horizon = [Star {
            dir: Vec3::X,
            rgb: [255; 3],
        }];
        let zenith = [Star {
            dir: Vec3::Y,
            rgb: [255; 3],
        }];
        let (mut h, mut z) = (Vec::new(), Vec::new());
        twinkle_alpha(&f, &horizon, 12.0, &mut h);
        twinkle_alpha(&f, &zenith, 12.0, &mut z);
        let (dh, dz) = (255 - h[0] as i32, 255 - z[0] as i32);
        assert!(dh > 0 && (dh - 2 * dz).abs() <= 1, "{dh} {dz}");
        // The same instant gives the same alphas; it eases between steps.
        let mut again = Vec::new();
        twinkle_alpha(&f, &horizon, 12.0, &mut again);
        assert_eq!(h, again);
    }

    #[test]
    fn a_star_is_a_192th_of_the_height_and_bottom_doubles_the_quads() {
        let fov = 50f32.to_radians();
        assert!((half_size(fov) * 2.0 * 192.0 - 2.0 * (fov * 0.5).tan()).abs() < 1e-6);
        let field = StarField {
            count: 300,
            ..rhn2()
        };
        let (stars, cells) = generate(&field);
        let mut state = StarState {
            field: Some(field),
            stars,
            cells,
            ..Default::default()
        };
        let mut out = Vec::new();
        build(&state, &Camera::default(), 0.0, &mut out);
        assert_eq!(out.len(), state.stars.len() * 6);
        assert!(out.iter().all(|v| v.position[1] >= -0.1));
        state.field.as_mut().unwrap().enable_bottom = true;
        build(&state, &Camera::default(), 0.0, &mut out);
        assert_eq!(out.len(), state.stars.len() * 12);
        assert!(out.iter().any(|v| v.position[1] < -1.0));
    }
}
