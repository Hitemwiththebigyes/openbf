//! Shared helpers for the game crate's integration tests.
//!
//! The five visual golden tests — `golden_tat1`, `golden_hud`, `golden_shell`,
//! `golden_spawn` and `golden_gallery` — all compare a captured RGBA frame
//! against a committed PNG under the SAME perceptual tolerances, and each one
//! carried its own copy of that loop. One copy lives here, so the tolerances
//! cannot drift between the single-frame goldens and the map/unit sheets.
//!
//! Cargo compiles this file into every test binary that declares `mod common;`,
//! so a helper only one of them uses looks dead to the rest — hence the
//! module-wide allow.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// A pixel differing by more than this in any of R/G/B counts as "strongly
/// different"; tolerates minor driver/rasterizer variance.
const STRONG_DIFF: u8 = 12;
/// Mean absolute per-channel error the frame must stay under.
const MEAN_LIMIT: f64 = 1.5;
/// Fraction of strongly-different pixels the frame must stay under.
const STRONG_FRAC_LIMIT: f64 = 0.01;

/// The committed golden PNGs (`engine/goldens`).
pub fn goldens_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../goldens"))
}

/// Pin this test process to the PS2 texture tier.
///
/// Every committed golden is the STRICT-FAITHFUL referee (the lead's decision
/// of 2026-09-08 night, when item 11 made `<name>.ktx2` the disc's own pixels
/// and put the PC override in `<name>.tier2.ktx2`). The runtime default is
/// `Pc`, so without this a golden would render whatever the bake machine's PC
/// install happened to provide — and would move on a machine without one.
/// Call it FIRST in every golden test, before anything loads a texture.
pub fn force_ps2_tier() {
    game::assets::set_texture_tier(game::assets::TextureTier::Ps2);
}

/// Write `rgba` as the golden at `golden` when the regeneration env var is
/// set, and say so; `false` means the caller should compare instead. This is
/// the path `scripts/goldens.sh` (without `--check`) drives.
pub fn wrote_update(golden: &Path, rgba: &[u8], w: u32, h: u32) -> bool {
    if std::env::var_os("UPDATE_GOLDENS").is_none() {
        return false;
    }
    if let Some(dir) = golden.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let png = render::encode_png(rgba, w, h).expect("png");
    std::fs::write(golden, png).expect("write golden");
    println!("golden written: {}", golden.display());
    true
}

/// Compare `rgba` with the PNG at `golden`; `None` when it matches. Returns
/// the failure text rather than panicking so a caller checking a whole set
/// (the gallery) can report every sheet that moved, not just the first.
pub fn compare_golden(golden: &Path, rgba: &[u8], w: u32, h: u32) -> Option<String> {
    let img = match image::open(golden) {
        Ok(i) => i.to_rgba8(),
        Err(e) => {
            return Some(format!(
                "{}: {e} (regenerate with scripts/goldens.sh)",
                golden.display()
            ))
        }
    };
    if (img.width(), img.height()) != (w, h) {
        return Some(format!(
            "{}: golden is {}x{}, render is {w}x{h}",
            golden.display(),
            img.width(),
            img.height()
        ));
    }
    let expected = usize::try_from(w).ok().and_then(|w| {
        usize::try_from(h)
            .ok()
            .and_then(|h| w.checked_mul(h)?.checked_mul(4))
    });
    let Some(expected) = expected else {
        return Some(format!(
            "{}: RGBA dimensions {w}x{h} overflow the buffer length",
            golden.display()
        ));
    };
    if rgba.len() != expected {
        return Some(format!(
            "{}: RGBA buffer has {} bytes, expected {expected} for {w}x{h}",
            golden.display(),
            rgba.len()
        ));
    }
    let g = img.as_raw();
    let (mut sum_abs, mut bad_px) = (0u64, 0u32);
    for (i, px) in rgba.as_chunks::<4>().0.iter().enumerate() {
        let mut worst = 0u8;
        for c in 0..3 {
            let d = px[c].abs_diff(g[i * 4 + c]);
            sum_abs += u64::from(d);
            worst = worst.max(d);
        }
        if worst > STRONG_DIFF {
            bad_px += 1;
        }
    }
    let mean = sum_abs as f64 / (f64::from(w) * f64::from(h) * 3.0);
    let bad_frac = f64::from(bad_px) / (f64::from(w) * f64::from(h));
    println!(
        "golden diff {}: mean abs {mean:.3}, strong-diff pixels {:.4}%",
        golden.display(),
        bad_frac * 100.0
    );
    (mean >= MEAN_LIMIT || bad_frac >= STRONG_FRAC_LIMIT).then(|| {
        format!(
            "{}: mean abs {mean:.3} (limit {MEAN_LIMIT}), {:.3}% of pixels differ strongly (limit {:.0}%)",
            golden.display(),
            bad_frac * 100.0,
            STRONG_FRAC_LIMIT * 100.0
        )
    })
}

/// The stems of every committed golden PNG directly under `dir`, sorted.
///
/// A test that owns a whole family of goldens uses this to prove it actually
/// rendered every one of them: a partial bake used to leave a family
/// unrendered and the test still passed, so `scripts/goldens.sh --check`
/// could exit 0 with 54 of the 90 goldens unchecked (bug T1).
pub fn committed_goldens(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .filter_map(|p| Some(p.file_stem()?.to_str()?.to_owned()))
        .collect();
    out.sort();
    out
}

/// [`wrote_update`] or [`compare_golden`] — what a test that checks a whole
/// SET wants: `None` when the golden was rewritten or matches.
pub fn check_golden(golden: &Path, rgba: &[u8], w: u32, h: u32) -> Option<String> {
    if wrote_update(golden, rgba, w, h) {
        return None;
    }
    compare_golden(golden, rgba, w, h)
}

/// [`check_golden`], panicking on a difference — what a test with exactly one
/// golden wants.
///
/// `golden_tat1` deliberately does NOT use this: `tat1_overview.png` is
/// written by the game binary's own `--capture-frame`, so that test must keep
/// comparing even during a regeneration run.
pub fn assert_golden(golden: &Path, rgba: &[u8], w: u32, h: u32) {
    if let Some(msg) = check_golden(golden, rgba, w, h) {
        panic!("{msg}");
    }
}
