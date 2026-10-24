//! PS2 detail maps (`<name>_dtl`).
//!
//! A `tex_` whose INFO has `detailCompressed` set is a low-resolution base
//! that the PS2 multiplies x2 against a sibling texture named `<name>_dtl`
//! (audit A2 §4, unmunge `resolve_detail_compression`). On the discs every
//! such pair is 1:1 and the companion is exactly 4x the base in each
//! dimension, carrying its signal in ALPHA — see RE_LOG "BF1 detail
//! textures".
//!
//! The bake writes the companion as `<base>.dtl.ktx2` (a name no PS2 texture
//! can collide with — source names are `[a-z0-9_]`). Its presence next to
//! `<base>.ktx2` IS the detail flag, so nothing downstream has to re-derive
//! it from the source LVL.

use anyhow::{Context, Result};
use mungefs::texture::Texture;
use std::path::Path;

/// File name of the baked detail companion for `base`.
pub fn detail_file(base: &str) -> String {
    format!("{base}.dtl.ktx2")
}

/// Source `tex_` name of the detail companion for `base`.
pub fn detail_source_name(base: &str) -> String {
    format!("{base}_dtl")
}

/// Material flag bits that make a texture's ALPHA meaningful: 0x02 = cutout
/// ("hardedged"), 0x04 = transparent. A texture referenced only by materials
/// with neither bit is drawn with its alpha ignored on the PS2, so the bake
/// stores it opaque — otherwise the port's alpha test deletes it. MEASURED
/// over all 10,060 segment/texture pairs on the two discs: with 0x02 set the
/// texture really is a cutout 72-77 % of the time, with neither bit set 6 %.
/// The super battle droid was the tell — every texel of its texture has alpha
/// below the test, so the whole model vanished. See RE_LOG.
pub const ALPHA_IS_MEANINGFUL: u32 = CUTOUT | TRANSPARENT;

/// `segm MTRL` bit 0x02: the alpha channel is a hard-edged CUTOUT mask.
pub const CUTOUT: u32 = 0x02;

/// `segm MTRL` bit 0x04: the material is TRANSPARENT and must be composited,
/// not alpha-tested. 50 baked textures on the discs (water, glass, shadows,
/// godrays, holograms, HUD hints) have no texel above 0.5 — under any alpha
/// test the port runs — so alpha-testing them deletes the surface outright.
pub const TRANSPARENT: u32 = 0x04;

/// The WORLD alpha-test threshold as an 8-bit disc alpha: the first alpha
/// `fs_mask` keeps. It is the PS2's own test in disc terms — the derivation
/// is on `MASK_CUTOFF` in `render/src/shader.wgsl`. Skinned draws are never
/// alpha-tested (the disc tests no side model, `crate::soldier::segment_blends`),
/// so this constant does not describe them.
pub const ALPHA_TEST: u8 = 193;

/// Write `<dir>/<name>.ktx2` from a decoded texture (all mips).
///
/// This used to be a two-line wrapper over a `write_ktx2_opts(…, opaque: bool)`
/// that nothing ever passed `true` to: forcing every texel opaque is
/// `tier::Resolved::make_opaque`'s job now, before the pixels get here, and
/// both callers (`bake::map`, `bake::soldier`) go through `Resolved`.
pub fn write_ktx2(dir: &Path, file: &str, t: &Texture) -> Result<()> {
    let mips: Vec<Vec<u8>> = (0..t.mips.len()).map(|i| t.rgba8(i)).collect();
    let bytes = crate::ktx2w::write_rgba8(t.info.width, t.info.height, &mips);
    std::fs::write(dir.join(file), bytes).with_context(|| file.to_owned())?;
    Ok(())
}

/// Write the detail companion of `base` if the LVL carries one, returning the
/// source name it consumed (so the caller can keep it out of the plain
/// texture list). `all` is every decoded texture in the LVL, keyed however
/// the caller names them — lookup is by lowercased name. Removes an obsolete
/// companion when none is currently present.
pub fn write_detail_for(dir: &Path, base: &Texture, all: &[Texture]) -> Result<Option<String>> {
    let file = detail_file(&base.name.to_ascii_lowercase());
    let Some(d) = companion(base, all) else {
        crate::remove_optional_file(&dir.join(file))?;
        return Ok(None);
    };
    write_ktx2(dir, &file, d)?;
    Ok(Some(d.name.to_ascii_lowercase()))
}

/// Current companion, if the base is detail-compressed and the LVL has it.
pub(crate) fn companion<'a>(base: &Texture, all: &'a [Texture]) -> Option<&'a Texture> {
    if !base.info.detail_compressed {
        return None;
    }
    let want = detail_source_name(&base.name.to_ascii_lowercase());
    all.iter().find(|t| t.name.to_ascii_lowercase() == want)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_detail_state_replaces_or_removes_only_its_companion() {
        let dir =
            std::env::temp_dir().join(format!("openbf-detail-lifecycle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let texture = |name: &str| Texture {
            name: name.into(),
            info: mungefs::texture::TextureInfo {
                width: 1,
                height: 1,
                bpp: 32,
                detail_compressed: true,
                mip_distance: 0.0,
                mip_count: 1,
            },
            palette: Vec::new(),
            mips: vec![vec![11, 22, 33, 44]],
        };
        let mut base = texture("Base");
        let mut detail = texture("BASE_DTL");
        detail.info.detail_compressed = false;
        let all = [detail];
        let path = dir.join("base.dtl.ktx2");
        let unrelated = dir.join("other.dtl.ktx2");
        std::fs::write(&unrelated, b"keep").unwrap();
        write_ktx2(&dir, "expected.ktx2", &all[0]).unwrap();
        assert_eq!(
            write_detail_for(&dir, &base, &all).unwrap().as_deref(),
            Some("base_dtl")
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            std::fs::read(dir.join("expected.ktx2")).unwrap()
        );
        assert!(write_detail_for(&dir, &base, &[]).unwrap().is_none()); // flagged, missing
        assert!(!path.exists());
        std::fs::write(&path, b"stale").unwrap();
        base.info.detail_compressed = false;
        assert!(write_detail_for(&dir, &base, &all).unwrap().is_none()); // unflagged, present
        assert!(!path.exists());
        assert!(write_detail_for(&dir, &base, &all).unwrap().is_none()); // already absent
        assert_eq!(std::fs::read(unrelated).unwrap(), b"keep");
        std::fs::create_dir(&path).unwrap();
        assert!(write_detail_for(&dir, &base, &all).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn names_are_derived_from_the_base() {
        assert_eq!(detail_file("all_inf_trooper"), "all_inf_trooper.dtl.ktx2");
        assert_eq!(detail_source_name("all_inf_trooper"), "all_inf_trooper_dtl");
    }

    /// The bake's 8-bit alpha-test threshold and the shader's float one are
    /// the same number in two languages, and both are the PS2's test in disc
    /// terms. If the shader moves its cutoff, the bake keeps deciding
    /// "alpha-testing deletes this texture" against a test the renderer no
    /// longer runs — so pin all three to each other here, for every alpha.
    #[test]
    fn alpha_test_threshold_matches_the_shader() {
        const SHADER: &str = include_str!("../../render/src/shader.wgsl");
        let cutoff: f32 = SHADER
            .lines()
            .find_map(|l| l.trim().strip_prefix("const MASK_CUTOFF: f32 = "))
            .and_then(|v| v.strip_suffix(';'))
            .expect("shader.wgsl declares `const MASK_CUTOFF: f32 = <x>;`")
            .parse()
            .expect("MASK_CUTOFF is a float literal");
        assert!(
            SHADER.contains("base.a < MASK_CUTOFF"),
            "fs_mask no longer tests against MASK_CUTOFF; update ALPHA_TEST to match"
        );
        for a in 0..=255u8 {
            // The GS: the CLUT alpha scaled at upload, modulated by the opaque
            // vertex alpha 0x7f, tested GEQUAL against AREF 0x60.
            let at = (u32::from(a) * 0x81) >> 8;
            let ps2_keeps = ((at * 0x7f) >> 7) >= 0x60;
            let shader_keeps = f32::from(a) / 255.0 >= cutoff;
            assert_eq!(shader_keeps, ps2_keeps, "disc alpha {a}: shader vs PS2");
            assert_eq!(
                a >= ALPHA_TEST,
                ps2_keeps,
                "disc alpha {a}: ALPHA_TEST vs PS2"
            );
        }
        assert_eq!(ALPHA_TEST, 193);
    }
}
