//! Every BF2 `CLTH` chunk on the disc parses to its last byte (card CLOTH,
//! `analysis/re-cloth.md` 1). Needs the extracted BF2 disc (gitignored);
//! SKIPS cleanly when absent.

use super::support::walk_lvls;
use mungefs::cloth::{find_cloths, Cloth};
use std::path::Path;

#[test]
fn all_58_disc_chunks_parse() {
    let root = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../extracted/bf2/lvl"
    ));
    let mut files = Vec::new();
    walk_lvls(root, &mut files);
    if files.is_empty() {
        eprintln!("SKIP: extracted BF2 LVLs not present");
        return;
    }
    files.sort();
    let mut chunks = 0;
    let mut kept: Vec<(String, Cloth)> = Vec::new();
    for f in &files {
        let buf = std::fs::read(f).unwrap();
        let Ok(inner) = mungefs::open(&buf) else {
            continue;
        };
        let all = mungefs::content_chunks(inner).unwrap();
        for c in all.iter().filter(|c| &c.tag == b"CLTH") {
            let cloth = Cloth::parse(c).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
            chunks += 1;
            let file = f.file_name().unwrap().to_string_lossy().into_owned();
            kept.push((file, cloth));
        }
        // find_cloths keeps one per name.
        let one = find_cloths(inner).unwrap();
        let mut names: Vec<_> = one.iter().map(|c| c.name.to_ascii_lowercase()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), one.len(), "{}", f.display());
    }
    assert_eq!(chunks, 58);
    // (name, file, vertices, pinned, triangles, shapes) - the RE pass's
    // `lead_tmp/cloth/clth_all.txt`, read by an independent Python reader.
    for (name, file, n, pinned, tris, shapes) in [
        ("imp_inf_snowtrooper_skirt", "IMP.LVL", 54, 18, 80, 4),
        (
            "imp_inf_darthvader_cape",
            "IMP.LVL",
            143,
            26,
            236,
            usize::MAX,
        ),
        ("rep_inf_arctrooper_cape", "REP.LVL", 55, 16, 80, usize::MAX),
        (
            "rep_inf_ep3sniper_cape",
            "REP.LVL",
            47,
            18,
            usize::MAX,
            usize::MAX,
        ),
        ("all_inf_leia_skirt", "ALL.LVL", 230, 136, 400, usize::MAX),
        (
            "com_icon_neutral_carried_cloth",
            "INGAME.LVL",
            221,
            15,
            384,
            usize::MAX,
        ),
    ] {
        let (_, c) = kept
            .iter()
            .find(|(f, c)| f == file && c.name == name)
            .unwrap_or_else(|| panic!("{name} not in {file}"));
        assert_eq!(c.positions.len(), n, "{name}");
        assert_eq!(c.uvs.len(), n, "{name}");
        assert_eq!(c.pinned, pinned, "{name}");
        if tris != usize::MAX {
            assert_eq!(c.triangles.len(), tris, "{name}");
        }
        if shapes != usize::MAX {
            assert_eq!(c.shapes.len(), shapes, "{name}");
        }
    }
    let skirt = &kept
        .iter()
        .find(|(_, c)| c.name == "imp_inf_snowtrooper_skirt")
        .unwrap()
        .1;
    assert_eq!(skirt.parent_bone, "bone_root");
    assert_eq!(skirt.texture, "imp_inf_snowtrooper");
    assert!(skirt.pin_bones.iter().all(|b| b == "bone_pelvis"));
    assert!(skirt.shapes.iter().all(|s| s.kind == 1));
}
