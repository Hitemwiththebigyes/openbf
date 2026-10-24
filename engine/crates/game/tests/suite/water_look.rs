//! Data-gated: the twelve baked water maps get the look the disc authors
//! (queue item W2; the rule is `game::viewer::water_look`, its RE is
//! `analysis/re-water.md` §1/§3 [V] and the per-map census is §5).
//!
//! The unit tests beside the rule prove the ARITHMETIC on synthetic numbers.
//! This is the other half — that the numbers actually reaching it out of the
//! bake are the disc's, per map: which maps fog their water and from where,
//! how far the surface reaches, how translucent it is and its colour. A bake change that
//! silently dropped `fx__`'s LUT or its `DisableLowRes` would leave the unit
//! tests green and only fail here. Where the water reaches past its fog
//! crossover, the baked scene fog must begin no earlier than the crossover,
//! or the far water steps into fog at the seam. BF2 FEL1 fogs from 0 m but
//! has no far layer (`FarSceneRange(0)`, water ends at the crossover), so
//! it has no seam (`analysis/re-fog.md` §2, `re-water-flags.md` §5).
//!
//! Skips cleanly when the maps are not baked (`scripts/rebake.sh`).

use game::assets::Manifest;
use game::viewer::water_look;

/// The manifest alone — this test needs no meshes, and `load_scene` over
/// thirty-two maps would read a gigabyte of GLB to answer four numbers.
fn manifest_of(dir: &std::path::Path) -> Manifest {
    let txt = std::fs::read_to_string(dir.join("manifest.ron"))
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    ron::from_str(&txt).unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
}

/// `(map, fog_begin, far, alpha, foam)` — the four numbers per water map,
/// read off the baked manifests on 2026-09-09 and derived by the ELF's rule:
///
/// * `fog_begin` = `NearSceneRange.c` on a map that draws a far pass of
///   water, and `None` everywhere else — BF2's FLAT sub-pass is gated on
///   `SpecularEnable`, which no disc map authors but the level reset sets to 1
///   (`analysis/re-water-flags.md`, `[O]`; queue WSE, 2026-09-25), an ocean's
///   far pass is fogged in both games (`analysis/re-kamino.md` §2 `[O]`, card
///   KSEA), and a `DisableLowRes` map has no far pass to fog;
/// * `far` = `0.9 · FarSceneRange` (`0.8` for an ocean), floored at
///   `NearSceneRange.c`, or exactly `c` on a `DisableLowRes` map;
/// * `alpha` = `mean(Min.a, Max.a) / 255`, or 1 on an ocean map (its base
///   record is opaque);
/// * `foam` = the map authors `OceanEnable` and a `FoamTexture`.
const EXPECTED: &[(&str, Option<f32>, f32, f32, bool)] = &[
    ("bf1_kam1", Some(90.0), 640.0, 1.0, true),
    ("bf1_kas1", None, 180.0, 80.0 / 255.0, false),
    ("bf1_kas2", Some(100.0), 1350.0, 188.0 / 255.0, false),
    ("bf1_nab2", Some(150.0), 2700.0, 128.0 / 255.0, false),
    ("bf1_yav1", None, 100.0, 1.0, false),
    ("bf2_dag1", None, 60.0, 1.0, false),
    ("bf2_dea1", Some(180.0), 180.0, 150.0 / 255.0, false),
    ("bf2_fel1", Some(52.0), 52.0, 188.0 / 255.0, false),
    ("bf2_kam1", Some(90.0), 640.0, 1.0, true),
    ("bf2_kas2", Some(100.0), 4500.0, 1.0, false),
    ("bf2_nab2", Some(150.0), 150.0, 128.0 / 255.0, false),
    ("bf2_yav1", None, 100.0, 1.0, false),
];

#[test]
fn every_water_map_takes_the_look_its_disc_authors() {
    let root = game::assets::baked_root();
    let maps = game::assets::discover_maps(&root);
    if maps.is_empty() {
        eprintln!("SKIP: no baked maps under {}", root.display());
        return;
    }
    let mut seen = 0;
    for (key, _, path) in &maps {
        let manifest = manifest_of(path);
        let Some(water) = manifest.water.as_ref() else {
            assert!(
                !EXPECTED.iter().any(|e| e.0 == key.as_str()),
                "{key} ships a WATR and the bake dropped it"
            );
            continue;
        };
        let row = EXPECTED
            .iter()
            .find(|e| e.0 == key.as_str())
            .unwrap_or_else(|| panic!("{key} has water and is not in the census"));
        let look = water_look(&manifest.source_game, &manifest.lighting, water);
        if let (Some(fog), Some(begin), Some(far)) =
            (manifest.lighting.fog.as_ref(), look.fog_begin, look.far)
        {
            assert!(
                far <= begin || fog.near >= begin,
                "{key}: scene fog starts at {} before water fog crossover {begin} \
                 (water reaches {far})",
                fog.near
            );
        }
        assert_eq!(look.fog_begin, row.1, "{key}: water fog crossover");
        assert!(
            look.far.is_some_and(|f| (f - row.2).abs() < 1e-3),
            "{key}: water reaches {:?}, expected {}",
            look.far,
            row.2
        );
        assert!(
            (look.tint[3] - row.3).abs() < 1e-6,
            "{key}: blend weight {}, expected {}",
            look.tint[3],
            row.3
        );
        assert_eq!(look.foam_tint.is_some(), row.4, "{key}: ocean foam layer");
        // The colour is the LUT's mean, `MaxDiffuse` / 255 in GS terms, on
        // flat water, and unity on an ocean (`re-water-colour.md` §1.5/§1.9).
        let max = water.config.as_ref().and_then(|c| c.max_diffuse);
        let want = match (row.4, max) {
            (false, Some(m)) => [m[0] / 255.0, m[1] / 255.0, m[2] / 255.0],
            _ => [1.0; 3],
        };
        assert_eq!(look.tint[..3], want, "{key}: tint");
        // BF1's ocean is the unity draw; BF2's is lit, with its foam at the
        // fitted weight and a loss of colour from FogRange's near distance
        // (card KSEAL). No flat water is lit.
        let lit = row.4 && manifest.source_game != "bf1";
        if row.4 {
            let foam = look.foam_tint.expect("checked above");
            assert_eq!(foam[..3], [1.0; 3], "{key}: foam colour at unity");
            assert_eq!(foam[3] < 1.0, lit, "{key}: foam weight {}", foam[3]);
        }
        assert_eq!(look.sun > 0.0, lit, "{key}: sun weight {}", look.sun);
        assert_eq!(look.dim.is_some(), lit, "{key}: far loss {:?}", look.dim);
        seen += 1;
    }
    assert_eq!(seen, EXPECTED.len(), "the disc ships twelve water maps");

    // BF2 KAS2 is one of the two maps with their own LUT (30/30/30 to
    // 70/70/70, a grey where the others are green-blue): its mean is
    // MaxDiffuse, 70/255. Spot-checked against the literal so a bake that
    // lost the LUT cannot pass by agreeing with itself above.
    let path = root.join("bf2_kas2");
    if path.join("manifest.ron").exists() {
        let m = manifest_of(&path);
        let look = water_look(
            &m.source_game,
            &m.lighting,
            m.water.as_ref().expect("bf2_kas2 water"),
        );
        for c in 0..3 {
            assert!(
                (look.tint[c] - 70.0 / 255.0).abs() < 1e-6,
                "bf2_kas2 tint {:?}",
                look.tint
            );
        }
    }
}
