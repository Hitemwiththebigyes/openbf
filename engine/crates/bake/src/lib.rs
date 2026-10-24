//! Bake library: PS2 LVL → open formats (glTF/KTX2/RON). The `bake` binary is
//! a thin CLI over this.

pub mod adpcm;
pub mod alpha;
pub mod anim_sets;
pub mod beep;
pub mod bind_pose;
pub mod classes;
pub mod cloth;
pub mod door;
pub mod effects;
pub mod entity_light;
pub mod far_lowd;
pub mod foley;
pub mod foliage;
pub mod glb;
pub mod global_sound;
pub mod gltf_bin;
pub mod hardpoints;
pub mod hud;
pub mod ktx2w;
pub mod leafpatch;
pub mod lighting;
pub mod loadscreen;
pub mod lowres_glb;
pub mod map;
pub mod ogg;
pub mod shell;
pub mod side_classes;
pub mod skin_glb;
pub mod soldier;
pub mod sound_names;
pub mod sounds;
pub mod spec;
pub mod terrain_glb;
pub mod tier;
pub mod turret_rig;
pub mod vehicle;
pub mod walker;
pub mod water_glb;

pub mod detail;

/// Write a RON artifact the way every bake output is written: `to_string_pretty`
/// with `PrettyConfig::default()`. The seven call sites had this open-coded, and
/// the config is what makes the file byte-reproducible — one place to change it,
/// and one place a reader has to look to know what the shipped RON's shape is.
pub fn write_ron<T: serde::Serialize>(path: &std::path::Path, value: &T) -> anyhow::Result<()> {
    use anyhow::Context;
    let text = ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default())?;
    // A fresh tree has no output directories yet (card PACK: `bake classes` into an empty assets_baked/).
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
    }
    std::fs::write(path, text).with_context(|| path.display().to_string())?;
    Ok(())
}

/// Remove one owned optional output; absence is already the desired state.
pub(crate) fn remove_optional_file(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

/// The bake this working tree ships (`engine/assets_baked`), for the tests in
/// this crate that read it. TEST-ONLY and crate-private on purpose:
/// `game::assets::baked_root` is the same path by the same rule, and `bake` is
/// only a DEV-dependency of `game` (game/Cargo.toml:7-8), so the two cannot
/// share one definition without making `game`'s shipped binary link `bake`.
/// FOR THE LEAD: that dependency edge is the reason for the duplication.
#[cfg(test)]
pub(crate) const BAKED_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets_baked");
