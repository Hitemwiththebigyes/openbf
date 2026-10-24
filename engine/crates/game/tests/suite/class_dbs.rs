//! Shared baked class discovery for the rider and gunner census referees.

/// Every baked class db of `game`: each map's own classes first, then the
/// shared side files, as `vehicle_pads::install` stacks them; with the map.
pub(super) fn class_dbs(game: &str) -> Vec<(String, game::classdb::ClassDb)> {
    let root = game::assets::baked_root();
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir(&root) else {
        return out;
    };
    let mut maps: Vec<_> = dir
        .flatten()
        .filter(|e| e.path().join("manifest.ron").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    maps.sort();
    let mut sides: Vec<_> = std::fs::read_dir(root.join("classes"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(&format!("{game}_")))
        })
        .collect();
    sides.sort();
    for m in maps {
        let text = std::fs::read_to_string(root.join(&m).join("manifest.ron")).unwrap();
        let src = text
            .find("source_game: \"")
            .map(|i| &text[i + 14..i + 17])
            .unwrap_or("");
        if src != game {
            continue;
        }
        let mut db = game::classdb::ClassDb::default();
        let _ = db.load_map(&root.join(&m));
        for s in &sides {
            db.load(s).unwrap();
        }
        out.push((m, db));
    }
    out
}
