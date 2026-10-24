//! Queue item SM: BF1 COMMON.LVL's `squad_cmd_highlight` is a cutout on the
//! disc — its shape IS its CLUT alpha (`analysis/re-hud-icons.md` §2 [V]:
//! 1,594 of its 4,096 texels at alpha 0) — and the common bake keeps that
//! alpha now, because no model in the LVL names the sheet
//! (`bake::alpha::keep_alpha`'s `unnamed` clause). Before, the bake forced it
//! opaque and the squad marker drew a solid square. Skips when the extracted
//! disc data is absent.

fn common_bf1() -> Option<String> {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../extracted/bf1/lvl/DATA/_LVL_PS2/COMMON/ENG/COMMON.LVL"
    );
    std::path::Path::new(p).exists().then(|| p.to_string())
}

fn alpha_zero(rgba: &[u8]) -> usize {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[3] == 0)
        .count()
}

#[test]
fn squad_cmd_highlight_keeps_its_disc_alpha_through_the_common_bake() {
    let Some(lvl) = common_bf1() else {
        eprintln!("SKIP: extracted BF1 COMMON.LVL not present");
        return;
    };

    // The disc: the `tex_` decoded straight out of the LVL.
    let buf = std::fs::read(&lvl).unwrap();
    let inner = mungefs::open(&buf).expect("ucfb");
    let t = mungefs::chunks(inner)
        .flatten()
        .filter(|c| &c.tag == b"tex_")
        .filter_map(|c| mungefs::texture::Texture::parse(&c).ok())
        .find(|t| t.name.eq_ignore_ascii_case("squad_cmd_highlight"))
        .expect("BF1 COMMON.LVL carries squad_cmd_highlight");
    assert_eq!((t.info.width, t.info.height), (64, 64));
    let disc = t.rgba8(0);
    assert_eq!(alpha_zero(&disc), 1_594, "re-hud-icons.md §2's count");

    // The bake.
    let out =
        std::env::temp_dir().join(format!("openbf-bake-test-hud-alpha-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    bake::map::bake_common(&lvl, out.to_str().unwrap(), None).expect("bake ok");
    let bytes = std::fs::read(out.join("textures/squad_cmd_highlight.ktx2")).unwrap();
    let reader = ktx2::Reader::new(&bytes).expect("ktx2");
    let level0 = reader.levels().next().expect("a level 0").data;
    assert_eq!(
        alpha_zero(level0),
        1_594,
        "the baked sheet keeps the disc's alpha-0 texels"
    );
    assert_eq!(
        level0,
        &disc[..],
        "level 0 is the disc's pixels, alpha included"
    );
    let _ = std::fs::remove_dir_all(&out);
}
