//! The water's motion (queue item OC; `game::water_anim`, whose header holds
//! every citation): the ocean's spectrum and transform, the flat-water bob,
//! the lattice the ocean's patch is instanced over, and the ocean's foam
//! (queue OCF). CPU only, except
//! `both_kam1_bakes_lay_out_as_one_patch_per_160_m_cell`, which reads the two
//! baked KAM1 water meshes and skips when they are not baked.

use std::f32::consts::TAU;

use game::water_anim::{
    bob, cells_in_range, flat_lift, foam_alpha, foam_patch, lattice_cells, oscillates,
    shared_uv_origin, Complex, Foam, Lattice, Ocean, Spectrum, BOB_AMPLITUDE, BOB_RATE, FOAM_RATE,
    FOAM_WARMUP, OCEAN_SEED,
};
use glam::Vec3;

/// Both KAM1s: a 16×16 grid (`PatchDivisions`) over one 160 m cell.
fn kam1() -> Ocean {
    Ocean::new(16, 160.0, &Spectrum::KAM1, OCEAN_SEED)
}

#[test]
fn the_bob_is_six_centimetres_every_3_333_seconds() {
    let period = TAU / BOB_RATE;
    assert!((period - 3.3333).abs() < 1e-3, "period {period}");
    assert_eq!(bob(0.0), 0.0, "t = 0 is the still plane");
    assert!((bob(period / 4.0) - BOB_AMPLITUDE).abs() < 1e-6);
    assert!((bob(0.75 * period) + BOB_AMPLITUDE).abs() < 1e-6);
    for i in 0..200 {
        let t = i as f32 * 0.137;
        assert!(
            (bob(t + period) - bob(t)).abs() < 1e-5,
            "not periodic at t = {t}"
        );
        assert!(bob(t).abs() <= BOB_AMPLITUDE + 1e-7, "over 6 cm at t = {t}");
    }
}

/// `OscillationEnable(0)` — BF2 FEL1 — holds the plane still; the reset's 1
/// and a manifest with no config bob (`re-water-flags.md` §7 `[O]`).
#[test]
fn an_authored_oscillation_enable_zero_stills_the_bob() {
    let t = TAU / BOB_RATE / 4.0;
    assert!(bob(t) != 0.0);
    let mut c = manifest::ManifestWaterConfig {
        oscillation_enable: true,
        ..Default::default()
    };
    assert!(oscillates(Some(&c)) && oscillates(None));
    assert_eq!(flat_lift(oscillates(Some(&c)), t), bob(t));
    c.oscillation_enable = false;
    assert!(!oscillates(Some(&c)));
    assert_eq!(flat_lift(oscillates(Some(&c)), t), 0.0);
}

#[test]
fn phillips_is_zero_at_rest_and_across_the_wind() {
    let s = Spectrum::KAM1;
    // L = 0.1019 · 25² (re-water.md §3).
    assert!(
        (s.largest_wave() - 63.6875).abs() < 1e-3,
        "{}",
        s.largest_wave()
    );
    assert_eq!(s.phillips([0.0, 0.0]), 0.0);
    let w = (0.2f32 * 0.2 + 1.0).sqrt();
    let k = 0.1;
    let along = s.phillips([k * 0.2 / w, k / w]);
    let across = s.phillips([k / w, -k * 0.2 / w]);
    assert!(along > 0.0);
    assert!(
        across <= 1e-10 * along,
        "cos² leaves {across} across the wind"
    );
    // cos² does not tell a wave from its reverse.
    assert_eq!(s.phillips([0.03, 0.05]), s.phillips([-0.03, -0.05]));
}

#[test]
fn the_spectrum_is_hermitian_so_the_sea_is_real() {
    let o = kam1();
    let n = o.n();
    for &t in &[0.0, 0.5, 1.0, 7.25, 123.0] {
        let s = o.spectrum(t);
        let big = s
            .iter()
            .map(|c| c.re.abs().max(c.im.abs()))
            .fold(0.0, f32::max);
        for iz in 0..n {
            for ix in 0..n {
                let a = s[iz * n + ix];
                let b = s[((n - iz) % n) * n + (n - ix) % n].conj();
                assert!(
                    (a.re - b.re).abs() <= 1e-6 * big && (a.im - b.im).abs() <= 1e-6 * big,
                    "h(k) != conj h(-k) at ({ix},{iz}), t = {t}: {a:?} vs {b:?}"
                );
            }
        }
        let f = o.field(t);
        let re = f.iter().map(|c| c.re.abs()).fold(0.0, f32::max);
        let im = f.iter().map(|c| c.im.abs()).fold(0.0, f32::max);
        assert!(re > 0.0 && im <= 1e-4 * re, "t = {t}: |re| {re}, |im| {im}");
    }
}

/// One ±k pair of unit amplitude must come out as `2·cos(k·x)` — which pins
/// the transform, the ABSENCE of a `1/NM` normalisation (`re-water.md` §3a:
/// the PS2's divide is gated off for the water) and the `−n/2` index offset.
#[test]
fn the_transform_turns_one_mode_into_its_cosine() {
    let o = kam1();
    let n = o.n();
    let half = (n / 2) as isize;
    let at = |mx: isize, mz: isize| ((mz + half) as usize) * n + (mx + half) as usize;
    let mut s = vec![Complex::ZERO; n * n];
    s[at(1, 2)] = Complex::new(1.0, 0.0);
    s[at(-1, -2)] = Complex::new(1.0, 0.0);
    let f = o.inverse(&s);
    for z in 0..n {
        for x in 0..n {
            let want = 2.0 * (TAU * (x as f32 + 2.0 * z as f32) / n as f32).cos();
            let got = f[z * n + x];
            // 1e-5 of an amplitude that is now 2, not 2/256: the f32 error of
            // a 256-term sum scales with the sum.
            assert!(
                (got.re - want).abs() < 1e-5 && got.im.abs() < 1e-5,
                "({x},{z}): {got:?}, want {want}"
            );
        }
    }
}

#[test]
fn the_sea_moves_and_is_the_same_sea_every_run() {
    let (a, b) = (kam1(), kam1());
    assert_eq!(a.heights(0.5), b.heights(0.5), "one seed, one sea");
    let (h0, h1) = (a.heights(0.0), a.heights(0.5));
    let moved = h0
        .iter()
        .zip(&h1)
        .map(|(p, q)| (p - q).abs())
        .fold(0.0, f32::max);
    assert!(moved > 0.0, "the field did not change in 0.5 s");
    let rms = a.rms(0.0);
    assert!(rms > 0.0 && rms.is_finite(), "rms {rms}");
    let other = Ocean::new(16, 160.0, &Spectrum::KAM1, OCEAN_SEED + 1);
    assert_ne!(other.heights(0.0), h0, "the seed does not reach the sea");
}

#[test]
fn the_patch_tiles_without_a_seam() {
    let o = kam1();
    let n = o.n();
    let v = o.patch_vertices(0.75, [0.0, 0.0], [40.0, 40.0]);
    assert_eq!(v.len(), (n + 1) * (n + 1));
    for i in 0..=n {
        let (row0, rown) = (v[i * (n + 1)], v[i * (n + 1) + n]);
        assert_eq!(
            rown.position[1], row0.position[1],
            "column n != column 0 at row {i}"
        );
        assert_eq!(rown.normal, row0.normal);
        assert_eq!(
            v[n * (n + 1) + i].position[1],
            v[i].position[1],
            "row n != row 0"
        );
    }
    let last = v[(n + 1) * (n + 1) - 1];
    assert_eq!([last.position[0], last.position[2]], [160.0, 160.0]);
    assert_eq!(last.uv, [4.0, 4.0], "Tile(4, 4): four repeats per cell");
    let idx = o.patch_indices(1);
    assert_eq!(idx.len(), n * n * 6);
    assert!(idx.iter().all(|&i| (i as usize) < v.len()));
    assert_eq!(
        &idx[..6],
        &[0, 17, 1, 1, 17, 18],
        "the bake's [a, c, b, b, c, d]"
    );
}

#[test]
fn lattice_cells_reads_the_bakes_two_triangles_per_cell() {
    // A 3×3 sheet of 160 m cells at y −8, laid out as `bake::water_glb`
    // does, with cells (0, 0) and (2, 1) wet.
    let mut pos = Vec::new();
    for z in 0..4 {
        for x in 0..4 {
            pos.push([x as f32 * 160.0 - 240.0, -8.0, z as f32 * 160.0 - 240.0]);
        }
    }
    let cell = |x: u32, z: u32| {
        let a = z * 4 + x;
        [a, a + 4, a + 1, a + 1, a + 4, a + 5]
    };
    let idx: Vec<u32> = [cell(0, 0), cell(2, 1)].concat();
    let l = lattice_cells(&pos, &idx).expect("a plain lattice");
    assert_eq!((l.cell, l.height), (160.0, -8.0));
    assert_eq!(l.origins, vec![[-240.0, -240.0], [80.0, -80.0]]);
    let mut soup = idx.clone();
    soup.extend_from_slice(&[0, 5, 10, 0, 5, 10]);
    assert_eq!(
        lattice_cells(&pos, &soup),
        None,
        "a CPAT-like triangle is not a cell"
    );
    assert_eq!(lattice_cells(&pos, &idx[..3]), None);
    assert_eq!(shared_uv_origin(&l.origins, [40.0, 40.0]), Some([0.0, 0.0]));
    assert_eq!(
        shared_uv_origin(&l.origins, [48.0, 48.0]),
        None,
        "160/48 is not whole: the cells start at different texels"
    );
}

/// Data-gated: both KAM1 bakes are the lattice the ocean needs — 1,618 wet
/// cells of 160 m at `LAYR.height` −8, every one starting on a texture
/// repeat. Skips when the map is not baked.
#[test]
fn both_kam1_bakes_lay_out_as_one_patch_per_160_m_cell() {
    let root = game::assets::baked_root();
    for map in ["bf1_kam1", "bf2_kam1"] {
        let glb = root.join(map).join("water.glb");
        if !glb.exists() {
            eprintln!("SKIP {map}: {} is not baked", glb.display());
            continue;
        }
        let mesh = game::assets::load_glb(&glb).unwrap_or_else(|e| panic!("{map}: {e:#}"));
        let l = lattice_cells(&mesh.positions, &mesh.indices)
            .unwrap_or_else(|| panic!("{map}: the water mesh is not a plain lattice"));
        assert_eq!(
            (l.origins.len(), l.cell, l.height),
            (1618, 160.0, -8.0),
            "{map}"
        );
        assert!(
            shared_uv_origin(&l.origins, [40.0, 40.0]).is_some(),
            "{map}"
        );
    }
}

/// A decimated patch (`LODDecimation(2)` on both KAM1s) is the full patch at
/// every other point — exactly, so the two meet where they share a vertex.
///
/// **Nothing draws the decimated patch any more** (queue OCL,
/// `analysis/re-water.md` §3b): the disc builds one every frame and never
/// issues it, so the port stopped building one too, and `Ocean::patch`'s
/// `every` now has no caller outside these tests. This is kept as what it
/// always measured — a property of the SAMPLER, which stays true — and as
/// the reason the far patch could be deleted without moving a vertex the
/// near patch had.
#[test]
fn the_far_patch_is_the_near_patch_at_every_other_point() {
    let o = kam1();
    let n = o.n();
    let h = o.heights(2.5);
    let near = o.patch(&h, 1, [0.0, 0.0], [40.0, 40.0]);
    let far = o.patch(&h, 2, [0.0, 0.0], [40.0, 40.0]);
    let m = n / 2;
    assert_eq!(far.len(), (m + 1) * (m + 1));
    for gz in 0..=m {
        for gx in 0..=m {
            let (f, q) = (far[gz * (m + 1) + gx], near[2 * gz * (n + 1) + 2 * gx]);
            assert_eq!((f.position, f.normal, f.uv), (q.position, q.normal, q.uv));
        }
    }
    assert_eq!(o.patch_indices(2).len(), m * m * 6);
}

/// `Ocean::bound` is the triangle inequality's ceiling, so no sample may ever
/// exceed it.
#[test]
fn no_wave_ever_exceeds_the_bound() {
    let o = kam1();
    let b = o.bound();
    let mut worst = 0.0f32;
    for i in 0..400 {
        let t = i as f64 * 0.731;
        worst = o.heights(t).iter().fold(worst, |w, v| w.max(v.abs()));
    }
    assert!(
        worst > 0.0 && worst <= b,
        "max |h| {worst} over the bound {b}"
    );
}

/// The radix-2 FFT against the plain DFT on this sea's own spectrum.
#[test]
fn the_fft_is_the_dft() {
    let o = kam1();
    let n = o.n();
    let s = o.spectrum(3.0);
    let got = o.inverse(&s);
    let half = (n / 2) as f32;
    for z in 0..n {
        for x in 0..n {
            let mut acc = Complex::ZERO;
            for kz in 0..n {
                for kx in 0..n {
                    let a = TAU * ((kx as f32 - half) * x as f32 + (kz as f32 - half) * z as f32)
                        / n as f32;
                    acc = acc + s[kz * n + kx] * Complex::cis(a);
                }
            }
            // Relative: these are metres now, not centimetres, and a
            // 256-term f32 sum carries its own last-place error.
            let (want, g) = (acc, got[z * n + x]);
            let tol = 1e-5 * want.re.hypot(want.im).max(1.0);
            assert!(
                (g.re - want.re).abs() < tol && (g.im - want.im).abs() < tol,
                "({x},{z}): fft {g:?}, dft {want:?}"
            );
        }
    }
}

/// The scale itself: dropping `Ocean::inverse`'s `1/n²` is the whole of queue
/// OCB, and it multiplies every height by `n² = 256`. This test samples 200
/// frames; over 2000 frames of KAM1's own sea the numbers are rms median
/// 4.3295 m, peak-to-trough median 19.1866 m, worst sample 17.1606 m (OCB's
/// build report, re-measured by its landing) — against `re-water.md` §3a's
/// independent model of the disc's pipeline (4.145 m and 19.23 m) and the
/// 0.0162 m / 0.0751 m the normalised transform gave. The bands are wide
/// enough for another `OCEAN_SEED` and far too tight for a stray `1/n²` to
/// slip back in.
#[test]
fn kam1s_sea_is_metres_not_centimetres() {
    let o = kam1();
    let (mut rms, mut range) = (Vec::new(), Vec::new());
    for i in 0..200 {
        let t = i as f64 * 1.37;
        let h = o.heights(t);
        let lo = h.iter().copied().fold(f32::MAX, f32::min);
        let hi = h.iter().copied().fold(f32::MIN, f32::max);
        rms.push(o.rms(t));
        range.push(hi - lo);
    }
    let median = |v: &mut Vec<f32>| {
        v.sort_by(f32::total_cmp);
        v[v.len() / 2]
    };
    let (r, p) = (median(&mut rms), median(&mut range));
    assert!((2.0..8.0).contains(&r), "rms median {r} m is not a sea");
    assert!(
        (10.0..35.0).contains(&p),
        "peak-to-trough median {p} m is not a sea"
    );
}

/// The sea ends at the far pass, on a tile boundary (card KSEA,
/// `analysis/re-kamino.md` §2.1): from the PCSX2 pose — eye z = −119 looking
/// down −z — `FarSceneRange` 800 (reach 640 m) draws the 160 m row ending at
/// z = −800 (681 m out) and not the next, and 400 (reach 320 m) ends at the
/// row whose far side is z = −480 (361 m out). Every drawn cell is whole.
#[test]
fn the_ocean_draws_whole_cells_that_reach_into_the_far_range() {
    let origins = (-10..10)
        .flat_map(|z| (-10..10).map(move |x| [x as f32 * 160.0, z as f32 * 160.0]))
        .collect();
    let lattice = Lattice {
        cell: 160.0,
        height: -8.0,
        origins,
    };
    let eye = Vec3::new(21.86, 72.24, -118.94);
    let far_row = |reach: f32| {
        cells_in_range(&lattice, eye, reach)
            .filter(|c| c[0] == 0.0)
            .map(|c| c[1])
            .fold(f32::MAX, f32::min)
    };
    assert_eq!(far_row(640.0), -800.0, "FarSceneRange 800");
    assert_eq!(far_row(320.0), -480.0, "FarSceneRange 400");
    // Radial: the same reach behind the eye and to its sides.
    let n = cells_in_range(&lattice, eye, 640.0).count();
    assert!((60..=90).contains(&n), "{n} cells inside 640 m");
    assert_eq!(cells_in_range(&lattice, eye, 0.0).count(), 0);
}

/// `n²` zero heights with `v` at `(x, z)`.
fn spike(n: usize, x: usize, z: usize, v: f32) -> Vec<f32> {
    let mut h = vec![0.0; n * n];
    h[z * n + x] = v;
    h
}

/// The disc's rule on a synthetic crest (`FUN_00477cd0` 539941–539985): +6 a
/// step while the crest stands, saturating at 255 on the 43rd, then −4 a step
/// on a flat field, reaching 0 on the 64th — and nothing else ever foams.
#[test]
fn the_foam_byte_climbs_on_a_crest_and_decays_on_a_flat_field() {
    let n = 8;
    let crest = spike(n, 3, 5, 4.0);
    let at = |f: &Foam| f.bytes()[5 * (n + 1) + 3];
    let mut f = Foam::new(n);
    assert_eq!(f.bytes().len(), (n + 1) * (n + 1));
    assert!(f.bytes().iter().all(|&b| b == 0), "ALL of it starts at 0");
    for i in 1..=42u32 {
        f.step(&crest);
        assert_eq!(u32::from(at(&f)), 6 * i, "climb step {i}");
    }
    f.step(&crest);
    assert_eq!(at(&f), 255, "252 + 6 saturates");
    f.step(&crest);
    assert_eq!(at(&f), 255);
    let foamed = f.bytes().iter().filter(|&&b| b != 0).count();
    assert_eq!(foamed, 1, "only the crest foams, not its slopes");
    let flat = vec![0.0; n * n];
    for i in 1..=63u32 {
        f.step(&flat);
        assert_eq!(u32::from(at(&f)), 255 - 4 * i, "decay step {i}");
    }
    f.step(&flat);
    assert_eq!(at(&f), 0, "3 − 4 floors at 0");
    assert!(f.bytes().iter().all(|&b| b == 0));
}

/// The test is strict and one-sided, and one axis is enough: a crest of
/// exactly 3.0 does not foam and a hair over does, a trough never does, and
/// a ridge that is flat along x foams on its crest across z.
#[test]
fn the_crest_test_is_strict_and_ignores_troughs() {
    let n = 8;
    let one_step = |h: &[f32]| {
        let mut f = Foam::new(n);
        f.step(h);
        f.bytes()[2 * (n + 1) + 2]
    };
    assert_eq!(
        one_step(&spike(n, 2, 2, 1.5)),
        0,
        "(1.5 − 0) − (0 − 1.5) = 3.0 exactly"
    );
    assert_eq!(one_step(&spike(n, 2, 2, 1.501)), 6);
    assert_eq!(one_step(&spike(n, 2, 2, -100.0)), 0, "a trough");
    let mut ridge = vec![0.0; n * n];
    ridge[2 * n..3 * n].fill(4.0);
    assert_eq!(one_step(&ridge), 6);
}

/// The map is `(n+1)²` points read through the grid's periodic indices, like
/// the disc's `(N+1)·(M+1)` bytes: a crest on the corner foams all four
/// corners, and on KAM1's sea row and column `n` always equal row and column
/// 0 — so cells that share an edge show the same foam along it.
#[test]
fn the_foam_wraps_like_the_patch() {
    let n = 8;
    let mut f = Foam::new(n);
    f.step(&spike(n, 0, 0, 4.0));
    let foamed: Vec<usize> = (0..f.bytes().len())
        .filter(|&i| f.bytes()[i] != 0)
        .collect();
    assert_eq!(foamed, [0, n, n * (n + 1), n * (n + 1) + n]);

    let o = kam1();
    let n = o.n();
    let mut f = Foam::new(n);
    for s in 0..300 {
        f.step(&o.heights(s as f64 / f64::from(FOAM_RATE)));
        let b = f.bytes();
        for i in 0..=n {
            assert_eq!(
                b[i * (n + 1) + n],
                b[i * (n + 1)],
                "column n, row {i}, step {s}"
            );
            assert_eq!(b[n * (n + 1) + i], b[i], "row n, column {i}, step {s}");
        }
    }
}

/// The disc's `(b + 1) >> 1` on the GS's 0..0x80 (`FUN_0048a6c8` 550727),
/// carried to the port's 0..255: the ends are exact, the halving's rounding
/// shows, and it never runs backwards.
#[test]
fn foam_alpha_is_the_gs_half_byte_on_the_ports_scale() {
    assert_eq!(foam_alpha(0), 0);
    assert_eq!(foam_alpha(255), 255, "0x80 is 1.0");
    assert_eq!(foam_alpha(1), 2, "(1 + 1) >> 1 is 1 of 128");
    assert_eq!(foam_alpha(2), 2, "(2 + 1) >> 1 is 1 of 128 too");
    assert_eq!(foam_alpha(6), 6, "one step of foam, 3 of 128");
    for b in 0..=255u8 {
        let gs = f64::from((u32::from(b) + 1) >> 1);
        assert_eq!(
            f64::from(foam_alpha(b)),
            (gs * 255.0 / 128.0).round(),
            "byte {b}"
        );
        if b > 0 {
            assert!(foam_alpha(b) >= foam_alpha(b - 1), "byte {b}");
        }
    }
}

/// The foam layer is the patch with the foam in its alpha: every vertex's
/// position, normal, UV and rgb exactly the base's, so the layers meet at one
/// depth (the water pipeline's `LessEqual`), and the base stays opaque.
#[test]
fn the_foam_patch_is_the_patch_with_the_foam_in_its_alpha() {
    let o = kam1();
    let mut f = Foam::new(o.n());
    let t = 12.5;
    f.advance(&o, t);
    assert!(
        f.bytes().iter().any(|&b| b > 0) && f.bytes().contains(&0),
        "a settled sea has foam and clear water"
    );
    let base = o.patch(&o.heights(t), 1, [0.25, 0.5], [40.0, 40.0]);
    let foam = foam_patch(&base, &f);
    assert_eq!(foam.len(), base.len());
    for ((b, v), &byte) in base.iter().zip(&foam).zip(f.bytes()) {
        assert_eq!(b.color, [255; 4], "the base is opaque");
        assert_eq!((v.position, v.normal, v.uv), (b.position, b.normal, b.uv));
        assert_eq!(v.color, [255, 255, 255, foam_alpha(byte)]);
    }
}

/// The foam steps on a 30 Hz grid of the water clock, each step at its own
/// `k / 30` s, so how often the game calls `advance` changes nothing: 60 and
/// 144 frames a second reach the same bytes. A cold start warms up, a repeat
/// of the same `t` takes no step, and a clock that runs backwards, jumps, or
/// goes non-finite cannot wedge it.
#[test]
fn the_foam_is_the_same_at_any_frame_rate() {
    let o = kam1();
    let run = |hz: u32| {
        let mut f = Foam::new(o.n());
        let steps: u32 = (0..=2 * hz)
            .map(|i| f.advance(&o, i as f64 / hz as f64))
            .sum();
        (f, steps)
    };
    let (a, sa) = run(60);
    let (b, sb) = run(144);
    assert_eq!(a, b, "60 and 144 frames a second disagree");
    assert_eq!(
        (sa, sb),
        (FOAM_WARMUP + 60, FOAM_WARMUP + 60),
        "2 s is 60 steps"
    );

    let mut f = Foam::new(o.n());
    assert_eq!(f.advance(&o, 1.0), FOAM_WARMUP, "a cold start warms up");
    assert_eq!(f.advance(&o, 1.0), 0, "the same t again");
    assert_eq!(f.advance(&o, 1.5), 15, "half a second");
    assert_eq!(f.advance(&o, 1.0), FOAM_WARMUP, "backwards restarts");
    assert_eq!(f.advance(&o, f64::NAN), 0);
    assert_eq!(f.advance(&o, f64::INFINITY), 0);
    let past = 2.0 + FOAM_WARMUP as f64 / f64::from(FOAM_RATE);
    assert_eq!(
        f.advance(&o, past),
        FOAM_WARMUP,
        "a jump past the warm-up restarts"
    );
}

/// A cold start's [`FOAM_WARMUP`] steps forget the zero start: the foam is a
/// long run's byte for byte (the constant was chosen on 1,200 such times over
/// 200 s; seven are re-checked here), so a capture at any `t` shows the foam
/// the game would show there.
#[test]
fn the_warmup_forgets_its_zero_start() {
    let o = kam1();
    let heights = |s: i64| o.heights(s as f64 / f64::from(FOAM_RATE));
    let mut long = Foam::new(o.n());
    for s in -800..=900i64 {
        long.step(&heights(s));
        if s >= 0 && s % 150 == 0 {
            let mut cold = Foam::new(o.n());
            for c in s - i64::from(FOAM_WARMUP) + 1..=s {
                cold.step(&heights(c));
            }
            assert_eq!(cold.bytes(), long.bytes(), "step {s}");
        }
    }
}

/// At KAM1's metre scale the crest test fires on about an eighth of the points
/// each step — `re-water.md` §3a measured 13.3 % (8.6–17.2 %) on its model of
/// the disc's pipeline, this sea a median 12.8 % — and the settled foam covers
/// a minority of the sea but never none of it. Under the old `1/n²` the test
/// fired on no point at all (§3a); that is the failure these bands catch.
#[test]
fn the_crest_test_fires_on_about_an_eighth_of_kam1s_points() {
    let o = kam1();
    let mut f = Foam::new(o.n());
    let points = f.bytes().len() as f32;
    let (mut fired, mut covered) = (Vec::new(), Vec::new());
    for s in -200..600i64 {
        let before = f.clone();
        f.step(&o.heights(s as f64 / f64::from(FOAM_RATE)));
        if s >= 0 {
            // A byte that rose fired, and so did one that held at 255.
            let up = (f.bytes().iter().zip(before.bytes()))
                .filter(|(a, b)| a > b || (**a == 255 && **b == 255))
                .count();
            fired.push(up as f32 / points);
            covered.push(f.bytes().iter().filter(|&&b| b > 0).count() as f32 / points);
        }
    }
    let median = |v: &mut Vec<f32>| {
        v.sort_by(f32::total_cmp);
        v[v.len() / 2]
    };
    assert!(
        covered.iter().all(|&c| c > 0.0),
        "a step with no foam at all"
    );
    let (fire, cover) = (median(&mut fired), median(&mut covered));
    assert!(
        (0.08..0.20).contains(&fire),
        "the test fires on {fire} of the points"
    );
    assert!(
        (0.1..0.6).contains(&cover),
        "the foam covers {cover} of the sea"
    );
}

#[test]
fn every_ocean_cell_routes_foam_to_its_own_mesh() {
    use glam::Mat4;
    use render::{DrawCmd, MeshHandle};
    let base = DrawCmd::flat(MeshHandle::default(), [0.2, 0.4, 0.6, 1.0], Mat4::IDENTITY)
        .with_cull(Some(640.0));
    let foam = DrawCmd {
        color: [1.0, 1.0, 1.0, 0.5],
        ..base
    };
    let cells = [[-160.0, 0.0], [0.0, 160.0]];
    let draws = game::water_anim::cell_draws(
        cells,
        -8.0,
        &[base, foam],
        &[false, true],
        ["base", "foam"],
        |mesh, draw| (mesh, draw),
    );
    assert_eq!(draws.len(), 4);
    for (cell, pair) in cells.iter().zip(draws.as_chunks::<2>().0) {
        assert_eq!((pair[0].0, pair[1].0), ("base", "foam"));
        for ((_, draw), template) in pair.iter().zip([base, foam]) {
            assert_eq!(
                draw.model,
                Mat4::from_translation(Vec3::new(cell[0], -8.0, cell[1]))
            );
            assert_eq!(draw.color, template.color);
            assert_eq!(draw.cull, None);
        }
    }
}

#[test]
fn ocean_and_foam_continue_across_the_material_clock_wrap() {
    let o = kam1();
    let wrap = f64::from(render::TIME_WRAP);
    for t in [wrap, 2.0 * wrap, 1_000_000.0, 100_000_000.0] {
        let before = o.heights(t - 1.0 / 30.0);
        let after = o.heights(t);
        let delta = before
            .iter()
            .zip(&after)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(delta > 0.0 && delta < 1.0, "wave step at {t}: {delta} m");
        let mut foam = Foam::new(o.n());
        foam.advance(&o, t - 0.02);
        assert_eq!(foam.advance(&o, t + 0.02), 1, "foam restarted at {t}");
    }
}
