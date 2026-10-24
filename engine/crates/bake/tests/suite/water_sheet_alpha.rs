//! Queue item WA: the sheets a map's own `fx__ Effect("Water")` names keep
//! their disc alpha through the MAP bake, because every blended water layer
//! on both discs composites by that alpha — TEX0 `TCC = 1` / `TFX = 0`
//! MODULATE through the engine's one texture path, then `ALPHA 0x44`
//! source-over, so `As = At x Av >> 7` (`analysis/re-water-alpha.md` §0,
//! `[V]` but `[O]` UNAUDITED). Before, `bake::alpha`'s design D3 forced every
//! map sheet no segment names opaque and the port's `fs_water` multiply of
//! `base.a` was inert.
//!
//! The pair of maps IS the test. BF1 YAV1 and BF1 YAV2 both bake the SAME
//! sheet, `yav_water` (alpha 96 on every texel of the disc's 8-bpp CLUT), and
//! no segment of either map names it — but only YAV1 carries an
//! `Effect("Water")`, and only YAV1's copy keeps the alpha. That contrast is
//! the lead's NARROW scope decision (2026-09-17) in one assertion: the clause
//! frees the sheets the water config names and nothing else.
//!
//! Skips when the extracted disc data is absent.

fn bf1_lvl(rel: &str) -> Option<String> {
    let p = format!(
        "{}/../../../extracted/bf1/lvl/DATA/_LVL_PS2/{rel}.LVL",
        env!("CARGO_MANIFEST_DIR")
    );
    std::path::Path::new(&p).exists().then_some(p)
}

/// Level 0 of a baked KTX2, which is raw RGBA8 (`bake::tier::write_ktx2`).
fn level0(path: &std::path::Path) -> Vec<u8> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let reader = ktx2::Reader::new(&bytes).expect("ktx2");
    // The iterator is a named binding so it outlives the tail expression's
    // borrow of it.
    let mut levels = reader.levels();
    levels.next().expect("a level 0").data.to_vec()
}

fn alpha_range(rgba: &[u8]) -> (u8, u8) {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .fold((255, 0), |(lo, hi), px| (lo.min(px[3]), hi.max(px[3])))
}

fn bake_to_temp(lvl: &str, tag: &str) -> std::path::PathBuf {
    let out = std::env::temp_dir().join(format!(
        "openbf-bake-test-water-alpha-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&out);
    bake::map::bake_map(lvl, out.to_str().unwrap(), None).expect("bake ok");
    out
}

#[test]
fn only_the_maps_own_water_sheet_keeps_its_disc_alpha() {
    let (Some(yav1), Some(yav2)) = (bf1_lvl("YAV/YAV1"), bf1_lvl("YAV/YAV2")) else {
        eprintln!("SKIP: extracted BF1 YAV1/YAV2 not present");
        return;
    };

    // The disc, read straight out of YAV1.LVL: `yav_water` is 8-bpp paletted
    // and every one of its texels carries alpha 96
    // (`lead_tmp/re/water-alpha/disc_water_alpha.txt`).
    let buf = std::fs::read(&yav1).unwrap();
    let inner = mungefs::open(&buf).expect("ucfb");
    let disc = mungefs::chunks(inner)
        .flatten()
        .filter(|c| &c.tag == b"tex_")
        .filter_map(|c| mungefs::texture::Texture::parse(&c).ok())
        .find(|t| t.name.eq_ignore_ascii_case("yav_water"))
        .expect("BF1 YAV1 carries yav_water")
        .rgba8(0);
    assert_eq!(alpha_range(&disc), (96, 96), "the disc's own alpha");

    // YAV1 authors `Effect("Water") MainTexture("yav_water")`, so the baked
    // sheet is the disc's pixels, alpha included.
    let out1 = bake_to_temp(&yav1, "yav1");
    let baked1 = level0(&out1.join("textures/yav_water.ktx2"));
    assert_eq!(
        alpha_range(&baked1),
        (96, 96),
        "YAV1's water sheet keeps the disc's alpha"
    );
    assert_eq!(baked1, disc, "level 0 is the disc's pixels, alpha included");

    // YAV2 bakes the same sheet and carries no `Effect("Water")` at all, so
    // design D3 still forces it opaque. Same bytes but for the alpha channel.
    let out2 = bake_to_temp(&yav2, "yav2");
    let baked2 = level0(&out2.join("textures/yav_water.ktx2"));
    assert_eq!(
        alpha_range(&baked2),
        (255, 255),
        "YAV2 names no water, so its copy of the sheet is still opaque"
    );
    assert!(
        baked1
            .as_chunks::<4>()
            .0
            .iter()
            .zip(baked2.as_chunks::<4>().0)
            .all(|(a, b)| a[..3] == b[..3]),
        "the clause touches the alpha channel and nothing else"
    );

    let _ = std::fs::remove_dir_all(&out1);
    let _ = std::fs::remove_dir_all(&out2);
}
