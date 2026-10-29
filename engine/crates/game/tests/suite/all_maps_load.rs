//! Data-gated robustness test: every baked map directory loads.
//!
//! This is the guard for format work — a parser change that breaks one map
//! out of thirty is otherwise invisible until someone happens to open it.
//! Skips cleanly when nothing has been baked (`scripts/rebake.sh`).

use std::path::Path;

#[test]
fn every_baked_map_loads() {
    let dir = game::assets::baked_root();
    let maps = game::assets::discover_maps(&dir);
    if maps.is_empty() {
        eprintln!("SKIP: no baked maps under {}", dir.display());
        return;
    }
    let mut checked = 0;
    for (key, game_name, path) in &maps {
        let scene = game::assets::load_scene(path)
            .unwrap_or_else(|e| panic!("{key} ({game_name}) failed to load: {e:#}"));
        assert!(!scene.meshes.is_empty(), "{key}: no models");
        assert!(
            !scene.manifest.instances.is_empty(),
            "{key}: no world instances"
        );
        let drawable = scene
            .manifest
            .instances
            .iter()
            .filter(|i| scene.resolve_model(&i.class).is_some())
            .count();
        assert!(
            drawable > 0,
            "{key}: {} instances, none resolve to a model",
            scene.manifest.instances.len()
        );
        checked += 1;
    }
    println!("{checked} baked maps loaded");
}

/// Every baked map yields command posts for conquest. BF2 maps keep theirs in
/// a `lvl_` sublevel, and the layer that holds them is not reliably named for
/// the mode — Mygeeto's conquest world is the SECOND world of a layer the bake
/// names "myg1_turrets", and Felucia/Mustafar have no conquest sublevel at all
/// (their posts sit in the map's `_design` pass). This test is what keeps
/// `Scene::apply_mode_layer`'s matching honest.
#[test]
fn every_baked_map_has_command_posts_for_conquest() {
    let dir = game::assets::baked_root();
    let maps = game::assets::discover_maps(&dir);
    if maps.is_empty() {
        eprintln!("SKIP: no baked maps");
        return;
    }
    let mut checked = 0;
    for (key, _game, path) in &maps {
        let mut scene = game::assets::load_scene(path).unwrap_or_else(|e| panic!("{key}: {e:#}"));
        scene.apply_mode_layer("con");
        let layout = game::layout::MapLayout::from_manifest(&scene.manifest);
        assert!(
            !layout.cps.is_empty(),
            "{key}: conquest found no command posts"
        );
        checked += 1;
    }
    println!("{checked} maps have conquest command posts");
}

/// The bake infers each map's game from its models (`bake::map`), and the
/// terrain's vertex colour depends on it (`bake::terrain_glb::gs_vertex_byte`:
/// c5 << 3 on BF1, << 2 on BF2 — a wrong guess doubles or halves the ground).
/// Pin the inference to the disc every map was baked from: `<game>_<lvl>`
/// directories, and `scripts/rebake.sh`'s TAT1 = BF1, TAT2 = BF2.
#[test]
fn every_terrain_map_bakes_as_its_own_disc() {
    let maps = game::assets::discover_maps(&game::assets::baked_root());
    if maps.is_empty() {
        eprintln!("SKIP: no baked maps");
        return;
    }
    let mut checked = 0;
    for (key, game_name, _) in &maps {
        let want = match key.as_str() {
            "tat1" => "bf1",
            "tat2" => "bf2",
            k => k.split('_').next().unwrap_or(k),
        };
        assert_eq!(game_name, want, "{key}: baked as the wrong game");
        checked += 1;
    }
    assert!(checked >= 30, "only {checked} maps baked");
}

/// Every baked map with a mission file: the mission parses and names that map.
#[test]
fn baked_map_missions_parse() {
    let assets = game::assets::baked_root();
    let missions = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../missions"));
    let maps = game::assets::discover_maps(&assets);
    if maps.is_empty() {
        eprintln!("SKIP: no baked maps");
        return;
    }
    let mut found = 0;
    for (key, game_name, _) in &maps {
        let p = missions.join(format!("{key}_con.ron"));
        if !p.exists() {
            continue;
        }
        let m = game::mission::MissionDef::load(&p).unwrap_or_else(|e| panic!("{key}: {e:#}"));
        assert_eq!(&m.map, key, "mission {key} names a different map");
        assert_eq!(&m.source_game, game_name, "{key}: source game mismatch");
        found += 1;
    }
    assert!(found >= 2, "expected mission files for the baked maps");
    println!("{found} map missions parsed");
}

/// Every baked map loads its own planet's sound bank, and that bank carries
/// the infantry rifle sounds for the eras that planet actually hosts. The
/// discs ship one SOUND LVL per PLANET (RE_LOG "BF1 does NOT ship sound banks
/// per era"), so a map wired to the wrong planet shows up here as the wrong
/// set of sides.
///
/// The expected sets below are MEASURED over the shipped banks, and they line
/// up with each planet's era: Hoth and Endor are GCW-only (rebel + imperial
/// only), Geonosis and BF1 Kamino are Clone-Wars-only, everything else hosts
/// both. BF1 never names a `cis_weap_inf_rifle_fire` at all.
#[test]
fn every_baked_map_has_its_sound_bank() {
    let dir = game::assets::baked_root();
    let maps = game::assets::discover_maps(&dir);
    if maps.is_empty() || !dir.join("tat1/sounds/sounds.ron").exists() {
        eprintln!("SKIP: no baked maps with sounds (scripts/rebake.sh sounds)");
        return;
    }
    // Planets whose bank is era-restricted; everything else must carry the
    // full set for its game (BF1: all+imp+rep, BF2: all+imp+rep+cis).
    let restricted: &[(&str, &[&str])] = &[
        ("bf1_end1", &["all", "imp"]),
        ("bf1_hot1", &["all", "imp"]),
        ("bf1_geo1", &["rep"]),
        ("bf1_kam1", &["rep"]),
        ("bf2_end1", &["all", "imp"]),
        ("bf2_hot1", &["all", "imp"]),
        ("bf2_geo1", &["rep", "cis"]),
    ];
    let mut checked = 0;
    for (key, game_name, path) in &maps {
        let sl = game::sounds::SoundLib::load(path)
            .unwrap_or_else(|| panic!("{key}: no sound library loaded"));
        let have: Vec<&str> = ["all", "imp", "rep", "cis"]
            .into_iter()
            .filter(|s| sl.has(&format!("{s}_weap_inf_rifle_fire")))
            .collect();
        let want: Vec<&str> = match restricted.iter().find(|(k, _)| k == key) {
            Some((_, sides)) => sides.to_vec(),
            None if game_name == "bf1" => vec!["all", "imp", "rep"],
            None => vec!["all", "imp", "rep", "cis"],
        };
        assert_eq!(have, want, "{key} ({game_name}): wrong sides in sound bank");
        checked += 1;
    }
    assert_eq!(checked, maps.len());
    println!("{checked} maps have their planet's sound bank");
}

/// A mission with zero reinforcements must be rejected when it is loaded, not
/// crash later: the first spawn is what builds the player, so `tickets: 0`
/// used to panic at startup with "fresh battle has reinforcements".
#[test]
fn zero_ticket_mission_is_rejected() {
    let dir = std::env::temp_dir().join(format!("openbf-zero-tickets-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("zero_con.ron");
    std::fs::write(
        &p,
        r#"(
            map: "zero",
            mode: "con",
            source_game: "bf1",
            teams: [
                (faction: "all", units: 20, tickets: 0),
                (faction: "imp", units: 20, tickets: 250),
            ],
            ai_alive_cap: 16,
            spawn_delay: 10.0,
            spawn_interval: 0.25,
        )"#,
    )
    .unwrap();
    let err = game::mission::MissionDef::load(&p).unwrap_err();
    assert!(
        format!("{err:#}").contains("0 reinforcements"),
        "unexpected error: {err:#}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The flythrough orbits the map's OWN centre.
///
/// `capture_orbit` used to recover the centre and radius by inverting
/// `overview_camera`'s eye placement, which silently assumed a map's bounds
/// were centred on the world origin. They are not: BF2 TAN1's bounds sit
/// 330 m off it, and the reconstruction gave a centre 233 m away with a
/// radius 3.2x too large, so the camera swung a wide circle around empty
/// space. This pins the property the reconstruction broke — the point the
/// orbit looks at is the framing's own centre, on every map.
#[test]
fn every_map_orbits_its_own_centre() {
    let dir = game::assets::baked_root();
    let maps = game::assets::discover_maps(&dir);
    if maps.is_empty() {
        eprintln!("SKIP: no baked maps");
        return;
    }
    for (key, _game, path) in &maps {
        let scene = game::assets::load_scene(path).unwrap_or_else(|e| panic!("{key}: {e:#}"));
        let o =
            game::viewer::overview_bounds(&scene).unwrap_or_else(|| panic!("{key}: no framing"));
        // The framing's centre is inside the map's own instance spread.
        let mut inside = 0;
        for i in &scene.manifest.instances {
            let p = glam::Vec3::from(i.translation);
            if (p - o.center).length() <= o.radius {
                inside += 1;
            }
        }
        assert!(
            inside * 2 >= scene.manifest.instances.len(),
            "{key}: only {inside} of {} instances within the framing radius {:.0}",
            scene.manifest.instances.len(),
            o.radius
        );
        // Every orbit pose aims at that centre and stands off by the orbit
        // radius, not by a distance recovered from the world origin.
        for k in 0..8 {
            let a = std::f32::consts::TAU * k as f32 / 8.0;
            let eye = game::viewer::orbit_eye(o, a, 0.34);
            let cam = game::viewer::look_at(eye, o.center);
            let hit = eye + cam.forward() * (o.center - eye).length();
            assert!(
                (hit - o.center).length() < 1e-2 * o.radius.max(1.0),
                "{key}: orbit pose {k} aims at {hit}, centre is {}",
                o.center
            );
        }
    }
}

/// Every texture a drawn model asks for and the map actually baked must
/// RESOLVE, and it only does because the lookup is case-insensitive.
///
/// A `modl` segment spells its texture with whatever case the artist typed
/// and the `tex_` chunk that carries the pixels spells it another way — BF1
/// RhenVar 2 bakes `rhn_bldg_interior_02` and its walls ask for
/// `Rhn_bldg_interior_02`. The model draw path looked the name up EXACTLY
/// (the terrain and sky paths already lowercased), so those draws fell back
/// to flat grey: 1,306 of RhenVar 2's 1,462 draws, 3,752 across all 32 maps.
///
/// What this pins is the DATA half of that contract: `Scene::textures` is keyed
/// lowercase, every material resolves once lowercased, and there really are
/// materials whose spelling is not a key (the canary at the end). It does NOT
/// exercise the fold itself — that is `viewer::scene::get_texture`'s first line, which
/// needs a GPU — so a "simplification" there back to an exact-match lookup is
/// caught by the goldens, not here.
#[test]
fn every_baked_texture_a_model_asks_for_resolves() {
    let dir = game::assets::baked_root();
    let maps = game::assets::discover_maps(&dir);
    if maps.is_empty() {
        eprintln!("SKIP: no baked maps");
        return;
    }
    let mut needed_folding = 0;
    for (key, _game, path) in &maps {
        let scene = game::assets::load_scene(path).unwrap_or_else(|e| panic!("{key}: {e:#}"));
        for k in scene.textures.keys() {
            assert_eq!(*k, k.to_ascii_lowercase(), "{key}: mixed-case texture key");
        }
        let baked: Vec<String> = scene
            .manifest
            .textures
            .iter()
            .map(|n| n.to_ascii_lowercase())
            .collect();
        let mut unresolved = Vec::new();
        for inst in &scene.manifest.instances {
            let Some(model) = scene.resolve_model(&inst.class) else {
                continue;
            };
            for (_, _, mat, _, _) in &scene.meshes[model].submeshes {
                if mat.is_empty() {
                    continue;
                }
                let lower = mat.to_ascii_lowercase();
                if !baked.contains(&lower) {
                    continue; // the map never baked it — a gap, not this bug
                }
                if *mat != lower {
                    needed_folding += 1;
                }
                if !scene.textures.contains_key(&lower) {
                    unresolved.push(format!("{model} -> {mat}"));
                }
            }
        }
        unresolved.sort();
        unresolved.dedup();
        assert!(
            unresolved.is_empty(),
            "{key}: {} baked textures a model asks for do not resolve: {:?}",
            unresolved.len(),
            &unresolved[..unresolved.len().min(5)]
        );
    }
    assert!(
        needed_folding > 0,
        "no material needed case folding — this test can no longer catch the bug"
    );
    println!("{needed_folding} model materials resolve only by case folding");
}

/// Exactly one map's terrain names a texture nothing bakes, and it is BF1
/// Bespin Platforms.
///
/// `upload_scene` DOES NOT DRAW a terrain chunk whose layer texture does not
/// resolve, rather than painting it a colour we invented. That is only a safe
/// rule while the set it applies to is this small and this well understood:
/// BES1's `tern` names `LTEX("kam1_water")` and `DTEX("yav_decal")`, textures
/// from other maps' LVLs that BES1.LVL does not ship, so nothing — not even
/// the PS2 — can paint that sheet from this map's own data. If another map
/// joins the list, the right response is to find out why, not to let a whole
/// terrain quietly stop drawing.
#[test]
fn only_bespin_platforms_has_an_unresolvable_terrain_texture() {
    let dir = game::assets::baked_root();
    let maps = game::assets::discover_maps(&dir);
    if maps.is_empty() {
        eprintln!("SKIP: no baked maps");
        return;
    }
    let mut unresolved = Vec::new();
    for (key, _game, path) in &maps {
        let scene = game::assets::load_scene(path).unwrap_or_else(|e| panic!("{key}: {e:#}"));
        let Some(t) = &scene.terrain else { continue };
        for (_, _, mat, _, _) in &t.submeshes {
            if !scene.textures.contains_key(&mat.to_ascii_lowercase()) {
                unresolved.push(format!("{key}: {mat}"));
            }
        }
    }
    unresolved.sort();
    unresolved.dedup();
    assert_eq!(unresolved, vec!["bf1_bes1: kam1_water".to_string()]);
}

/// Every map clears to black, as the PS2 does (`analysis/re-fog.md` §9.2:
/// RGBAQ 0 at every BF1 clear site, the same packet in BF2) — never to its
/// fog colour. BF1 Kashyyyk Docks' fog is a live (255,0,0) the PS2 never
/// shows; clearing to it drew a pure red band below the horizon (card CLRB).
/// The fogged KAS2 and the unfogged UTA1 — two of the three maps whose dome
/// stops at the horizon — must both be examined.
#[test]
fn every_map_clears_to_black() {
    let dir = game::assets::baked_root();
    let maps = game::assets::discover_maps(&dir);
    if maps.is_empty() {
        eprintln!("SKIP: no baked maps");
        return;
    }
    let mut examined = Vec::new();
    for (key, _game, path) in &maps {
        let scene = game::assets::load_scene(path).unwrap_or_else(|e| panic!("{key}: {e:#}"));
        assert_eq!(
            game::viewer::clear_color(&scene),
            game::viewer::MAP_CLEAR,
            "{key}"
        );
        examined.push(key.clone());
    }
    for key in ["bf1_kas2", "bf2_uta1"] {
        assert!(
            examined.iter().any(|k| k == key),
            "{key} not examined: {examined:?}"
        );
    }
    assert_eq!(game::viewer::MAP_CLEAR, [0.0, 0.0, 0.0, 1.0]);
}

/// Every ground map's `Hint` and `BARR` totals, measured on the raw discs by
/// the RE census (`census_ai.py`, RE_LOG 2026-09-01 "`plan` / `plnp` / `Hint`
/// / `BARR`"): (baked map directory, hints, barriers) over EVERY `wrld` in the
/// LVL. The bake must keep all of them — the seven BF2 maps that author hints
/// in a mode layer with no `plan` of its own are exactly what
/// `ManifestNav::plan` being an Option is for.
const DISC_HINT_BARRIER_CENSUS: &[(&str, usize, usize)] = &[
    ("bf1_bes1", 176, 511),
    ("bf1_bes2", 31, 514),
    ("bf1_end1", 82, 717),
    ("bf1_geo1", 85, 214),
    ("bf1_hot1", 41, 179),
    ("bf1_kam1", 65, 301),
    ("bf1_kas1", 102, 691),
    ("bf1_kas2", 42, 585),
    ("bf1_nab1", 60, 83),
    ("bf1_nab2", 92, 374),
    ("bf1_rhn1", 81, 245),
    ("bf1_rhn2", 157, 191),
    ("bf1_yav1", 57, 739),
    ("bf1_yav2", 108, 268),
    ("bf2_cor1", 239, 328),
    ("bf2_dag1", 72, 151),
    ("bf2_dea1", 291, 251),
    ("bf2_end1", 82, 717),
    ("bf2_fel1", 154, 246),
    ("bf2_geo1", 83, 398),
    ("bf2_hot1", 402, 368),
    ("bf2_kam1", 237, 759),
    ("bf2_kas2", 219, 257),
    ("bf2_mus1", 153, 290),
    ("bf2_myg1", 242, 483),
    ("bf2_nab2", 103, 416),
    ("bf2_pol1", 227, 353),
    ("bf2_tan1", 223, 204),
    ("bf2_uta1", 193, 311),
    ("bf2_yav1", 973, 736),
    ("tat1", 110, 592),
    ("tat2", 316, 644),
];

/// Census over the authored AI data of every baked map: the `nav` block of the
/// base world and of every mode layer that has one, plus the two binary
/// sidecars beside them.
///
/// The invariants are the ones the RE census measured over all 34 ground maps
/// (RE_LOG 2026-09-01 "`plan` / `plnp` / `Hint` / `BARR`"): every arc endpoint
/// is a real hub, every one of a hub's arc slots is an arc that touches THAT
/// hub, no hub has more than four alternative rows for a unit type (both
/// loaders keep four pointers), every routed slot indexes an arc the hub
/// actually has, and every hint kind is one the games construct. The sim
/// routes on this data (`game::layout::nav_graph`, `core::nav`'s corner walk
/// over the barriers, `core::commander`'s hint picks), and this test is what
/// keeps the bake honest underneath them.
#[test]
fn every_baked_nav_block_is_self_consistent() {
    use manifest::{Manifest, ManifestNav};

    let dir = game::assets::baked_root();
    let maps = game::assets::discover_maps(&dir);
    if maps.is_empty() {
        eprintln!("SKIP: no baked maps under {}", dir.display());
        return;
    }
    let mut nav_blocks = 0usize;
    let mut censused = 0usize;
    let (mut hubs, mut arcs, mut hints, mut barriers, mut records) = (0usize, 0, 0, 0, 0);
    for (key, _game, path) in &maps {
        let text = std::fs::read_to_string(path.join("manifest.ron"))
            .unwrap_or_else(|e| panic!("{key}: manifest.ron: {e}"));
        let m: Manifest = ron::from_str(&text).unwrap_or_else(|e| panic!("{key}: {e}"));
        let mut blocks: Vec<(String, &ManifestNav)> = Vec::new();
        if let Some(n) = m.nav.as_ref() {
            blocks.push(("base".into(), n));
        }
        for l in &m.layers {
            if let Some(n) = l.nav.as_ref() {
                blocks.push((l.name.clone(), n));
            }
        }
        let mut map_hints = 0usize;
        let mut map_barriers = 0usize;
        for (which, nav) in blocks {
            let at = format!("{key}/{which}");
            for hint in &nav.hints {
                assert!(
                    matches!(hint.kind, 0 | 1 | 2 | 4 | 5 | 6 | 7 | 8),
                    "{at}: hint {} kind {}",
                    hint.name,
                    hint.kind
                );
            }
            // Card COVR: a donor map's hints (`bake::donor`) are not the
            // disc's, and only a DONORS target carries them.
            let donated = nav
                .hints
                .iter()
                .filter(|h| h.name.starts_with(bake::donor::PREFIX))
                .count();
            assert!(
                donated == 0 || bake::donor::DONORS.iter().any(|d| d.0 == key.as_str()),
                "{at}: {donated} donated hints on a map with no donor"
            );
            map_hints += nav.hints.len() - donated;
            map_barriers += nav.barriers.len();
            hints += nav.hints.len();
            barriers += nav.barriers.len();
            nav_blocks += 1;
            let Some(plan) = nav.plan.as_ref() else {
                // A world with hints but no graph of its own: nothing more to
                // check, and it must NOT have claimed a sidecar.
                assert!(
                    !nav.hints.is_empty() || !nav.barriers.is_empty(),
                    "{at}: a nav block with no plan, no hint and no barrier"
                );
                continue;
            };
            let nav = plan;
            let n_hubs = nav.hubs.len();
            for (i, a) in nav.arcs.iter().enumerate() {
                assert!(
                    (a.a as usize) < n_hubs && (a.b as usize) < n_hubs,
                    "{at}: arc {i} {}->{} outside {n_hubs} hubs",
                    a.a,
                    a.b
                );
            }
            for (h, hub) in nav.hubs.iter().enumerate() {
                for &s in &hub.arcs {
                    let arc = nav
                        .arcs
                        .get(s as usize)
                        .unwrap_or_else(|| panic!("{at}: hub {h} slot names arc {s}"));
                    assert!(
                        arc.a as usize == h || arc.b as usize == h,
                        "{at}: hub {h} slot arc {s} joins {}..{}",
                        arc.a,
                        arc.b
                    );
                }
            }
            // routes_<layer>.bin: u16 hubCount, u8 typeCount, then per type
            // per hub `u8 rows` and rows x hubCount entry bytes.
            let bin = std::fs::read(path.join(&nav.routes))
                .unwrap_or_else(|e| panic!("{at}: {}: {e}", nav.routes));
            assert!(bin.len() >= 3, "{at}: routes sidecar is {} B", bin.len());
            assert_eq!(
                u16::from_le_bytes([bin[0], bin[1]]) as usize,
                n_hubs,
                "{at}: routes hubCount"
            );
            assert_eq!(bin[2], 5, "{at}: routes typeCount");
            let mut p = 3usize;
            for t in 0..5 {
                for (h, hub) in nav.hubs.iter().enumerate() {
                    assert!(p < bin.len(), "{at}: routes truncated at type {t} hub {h}");
                    let rows = bin[p] as usize;
                    p += 1;
                    assert!(rows <= 4, "{at}: hub {h} type {t} has {rows} rows");
                    for _ in 0..rows {
                        assert!(p + n_hubs <= bin.len(), "{at}: routes row truncated");
                        for &e in &bin[p..p + n_hubs] {
                            if e != 0 {
                                let slot = (e & 7) as usize;
                                assert!(
                                    slot < hub.arcs.len(),
                                    "{at}: hub {h} type {t} routes slot {slot} of {}",
                                    hub.arcs.len()
                                );
                            }
                        }
                        p += n_hubs;
                    }
                }
            }
            assert_eq!(p, bin.len(), "{at}: routes sidecar has trailing bytes");

            // plnp_<layer>.bin: u16 recordCount, then u32 len + bytes each.
            if let Some(pp) = nav.plnp.as_ref() {
                let bin = std::fs::read(path.join(&pp.records))
                    .unwrap_or_else(|e| panic!("{at}: {}: {e}", pp.records));
                assert!(bin.len() >= 2);
                let count = u16::from_le_bytes([bin[0], bin[1]]) as usize;
                assert_eq!(count, 1usize << pp.groups.len(), "{at}: plnp recordCount");
                let mut p = 2usize;
                for i in 0..count {
                    assert!(p + 4 <= bin.len(), "{at}: plnp record {i} header");
                    let len = u32::from_le_bytes(bin[p..p + 4].try_into().unwrap()) as usize;
                    p += 4;
                    assert!(p + len <= bin.len(), "{at}: plnp record {i} body");
                    if i == 0 {
                        // Nothing blocked: the patch is hubCount zero bytes.
                        assert_eq!(len, n_hubs, "{at}: plnp record 0 length");
                        assert!(
                            bin[p..p + len].iter().all(|&b| b == 0),
                            "{at}: plnp record 0 is not all zero"
                        );
                    }
                    p += len;
                    records += 1;
                }
                assert_eq!(p, bin.len(), "{at}: plnp sidecar has trailing bytes");
            }

            hubs += n_hubs;
            arcs += nav.arcs.len();
        }

        // Nothing authored may be dropped: the totals over the base block and
        // every layer block equal what the discs hold.
        if let Some(&(_, want_h, want_b)) = DISC_HINT_BARRIER_CENSUS
            .iter()
            .find(|(m, _, _)| *m == key.as_str())
        {
            assert_eq!(map_hints, want_h, "{key}: Hint total vs the disc census");
            assert_eq!(map_barriers, want_b, "{key}: BARR total vs the disc census");
            censused += 1;
        }
    }
    println!(
        "{nav_blocks} nav blocks over {} maps: {hubs} hubs, {arcs} arcs, {hints} hints, \
         {barriers} barriers, {records} plnp records ({censused} maps checked against \
         the disc census)",
        maps.len()
    );
    assert!(nav_blocks > 0, "no baked map carries a nav block");
    assert_eq!(
        censused,
        DISC_HINT_BARRIER_CENSUS.len(),
        "every censused map must be baked"
    );
}
