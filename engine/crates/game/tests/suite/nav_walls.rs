//! Card TGAP on the real bake (data-gated: skipped when the bake is absent):
//! the arcs no soldier can walk (`Sim::arc_unwalkable`) and the hubs he is
//! trapped in (`Sim::hub_trapped`), map by map.
//!
//! BF1 Tatooine's footpath arc 89 (hub 78 -> 77) meets a leaning slab between
//! two rocks: the PS2's soldier is stopped there walking west (0 of 4 walks,
//! `engine/artifacts/lead_tmp/tgap/`, 2026-10-09) and the port's both ways.
//! The floor walk calls it walkable (it tests floors, not walls); the wall
//! test walks it as a soldier round the walls and marks it shut, and the
//! footpath 75 .. 86 behind it, whose only other end is a leaf, is trapped -
//! so the bots that spawn at cp1's node (-377, 83) no longer walk into it and
//! stand at the slab until the 120 s recycle (tat1 seeds 9-40: 90 recycles
//! on main d896ca4, 87 of them there; 3 with the rule).
//!
//! The per-map table pins every map's count: a change to the floor walk, the
//! wall test or the bake that moves one has to be read before it lands
//! (`round_to_outcome`'s `NAV_FLOORS=1` lists them).
//!
//! Card TPOS: the position table built at load (`core::ai::tactic`) against
//! card BOTD's census on the six showcase maps (standable and covered points
//! within 5 %), and its build time on Mygeeto, in the thread's CPU seconds
//! (card TPOSA2; `round_to_outcome`'s `POSITIONS=1` lists the counts).

use core::sim::Sim;
use glam::Vec3;
use std::path::Path;

fn mission_for(key: &str) -> Option<game::mission::MissionDef> {
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../missions"));
    let path = dir.join(format!("{key}_con.ron"));
    path.exists()
        .then(|| game::mission::MissionDef::load(&path).ok())
        .flatten()
}

/// The map's sim with its posts in, as the sweep builds it before the
/// opening wave (`examples/spawn_node_census.rs`'s `build_sim`).
fn build_sim(path: &Path) -> Result<game::walk::WalkSim, String> {
    let mut scene = game::assets::load_scene(path).map_err(|e| format!("LOAD FAILED {e:#}"))?;
    scene.apply_mode_layer("con");
    if let Some(cd) = path
        .parent()
        .map(|p| p.join(format!("common_{}", scene.manifest.source_game)))
        .filter(|d| d.join("common.ron").exists())
    {
        if let Ok(c) = game::assets::load_common(&cd) {
            scene.merge_common(c);
        }
    }
    let key = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mission = mission_for(key);
    let mut layout = game::layout::MapLayout::from_manifest(&scene.manifest);
    let applied = mission.as_ref().map(|m| layout.apply_mission(m));
    let factions = mission
        .as_ref()
        .map(game::mission::MissionDef::factions)
        .unwrap_or_else(|| {
            game::roster::default_factions(&scene.manifest.source_game).map(str::to_owned)
        });
    let classes_dir = path
        .parent()
        .map(|p| p.join("classes"))
        .unwrap_or_else(|| std::path::PathBuf::from("classes"));
    let mut teams = factions.clone().map(|faction| game::walk::TeamTemplate {
        faction,
        ..game::walk::TeamTemplate::default_trooper()
    });
    if let Ok((rs, _)) = game::roster::FactionRoster::pair(
        &classes_dir,
        &scene.manifest.source_game,
        [factions[0].as_str(), factions[1].as_str()],
        mission.as_ref(),
    ) {
        for (t, r) in teams.iter_mut().zip(rs.iter()) {
            let f = r.fill_class();
            t.consts = f.consts;
            t.name = f.class.clone();
        }
    }
    let mut nav = game::layout::nav_graph(&scene.manifest, &scene.dir);
    if let Some(m) = &mission {
        m.apply_arc_blocks(nav.as_mut());
        m.apply_barriers(nav.as_mut());
    }
    let mut sim = Sim::empty(1);
    scene.install_water(&mut sim);
    sim.ground_rules = core::soldier::GroundRules::for_game(&scene.manifest.source_game);
    let mut w = game::walk::WalkSim::with_teams(sim, scene.heightfield(), Vec3::ZERO, teams, nav);
    scene.install_collision(&mut w.sim);
    if let Some(a) = &applied {
        w.sim.death_regions = a.death_regions.clone();
    }
    w.add_posts(&layout);
    Ok(w)
}

/// Per map: arc directions no soldier can walk, hubs he is trapped in.
const TABLE: &[(&str, usize, usize)] = &[
    ("bf1_bes1", 1, 0),
    ("bf1_bes2", 0, 0),
    ("bf1_end1", 4, 1),
    ("bf1_geo1", 0, 0),
    ("bf1_hot1", 0, 0),
    ("bf1_kam1", 0, 0),
    ("bf1_kas1", 0, 0),
    ("bf1_kas2", 4, 0),
    ("bf1_nab1", 0, 0),
    ("bf1_nab2", 0, 0),
    ("bf1_rhn1", 2, 0),
    ("bf1_rhn2", 0, 0),
    ("bf1_yav1", 0, 0),
    ("bf1_yav2", 0, 0),
    ("bf2_cor1", 0, 0),
    ("bf2_dag1", 0, 0),
    ("bf2_dea1", 6, 0),
    ("bf2_end1", 4, 1),
    ("bf2_fel1", 0, 0),
    ("bf2_geo1", 0, 0),
    ("bf2_hot1", 0, 0),
    ("bf2_kam1", 5, 1),
    ("bf2_kas2", 6, 0),
    ("bf2_mus1", 4, 0),
    ("bf2_myg1", 15, 0),
    ("bf2_nab2", 0, 0),
    ("bf2_pol1", 2, 0),
    ("bf2_tan1", 0, 0),
    ("bf2_uta1", 8, 0),
    ("bf2_yav1", 0, 0),
    ("tat1", 5, 12),
    ("tat2", 6, 2),
];

fn dead_dirs(sim: &Sim) -> usize {
    let Some(g) = sim.nav.as_ref() else { return 0 };
    g.arcs
        .iter()
        .enumerate()
        .map(|(i, a)| {
            usize::from(sim.arc_unwalkable(i as u16, a.a))
                + usize::from(sim.arc_unwalkable(i as u16, a.b))
        })
        .sum()
}

fn trapped(sim: &Sim) -> Vec<u16> {
    let n = sim.nav.as_ref().map_or(0, |g| g.hubs.len());
    (0..n as u16).filter(|&h| sim.hub_trapped(h)).collect()
}

#[test]
fn tatooines_footpath_is_shut_at_the_slab_and_trapped_behind_it() {
    let root = game::assets::baked_root();
    let dir = root.join("tat1");
    if !dir.join("manifest.ron").exists() || !root.join("classes").exists() {
        eprintln!("skipped: no tat1 bake");
        return;
    }
    let mut w = build_sim(&dir).expect("tat1");
    let sim = &w.sim;
    let g = sim.nav.as_ref().expect("graph");
    let arc = g.arcs[89];
    assert_eq!((arc.a, arc.b), (78, 77), "arc 89 is the footpath's 78 - 77");
    assert!(
        sim.arc_walkable(89, 78, core::ai::HUB_LEVEL_TOL),
        "the floor walk calls it walkable"
    );
    assert!(sim.arc_unwalkable(89, 78), "78 -> 77 is walled");
    assert!(sim.arc_unwalkable(89, 77), "77 -> 78 is walled");
    assert_eq!(trapped(sim), (75..=86).collect::<Vec<u16>>());
    // A soldier on cp1's node (-377.3, 83.0), nearest the footpath, starts
    // from no hub of it toward any other post (before: hub 85 -> 84 for all).
    let node = Vec3::new(-377.3, 20.7, 83.0);
    let feet = Vec3::new(node.x, w.sim.spawn_ground(node), node.z);
    let me = w.player;
    if let Some(u) = w.sim.unit_mut(me) {
        u.soldier.pos = feet;
        u.mount = None;
    }
    let dests: Vec<u16> = w
        .sim
        .cps
        .iter()
        .filter_map(|c| c.nav_hub)
        .filter(|&h| h != 46)
        .collect();
    assert_eq!(dests.len(), 5, "tat1's other five posts");
    for dest in dests {
        let [_, leg] = core::ai::first_leg(&w.sim, me, dest);
        let (start, route) = leg.expect("a first leg");
        assert!(
            !w.sim.hub_trapped(start) && !w.sim.hub_trapped(route.next_hub),
            "toward hub {dest}: start {start} next {}",
            route.next_hub
        );
    }
}

#[test]
fn every_maps_unwalkable_arcs_and_trapped_hubs_are_pinned() {
    let root = game::assets::baked_root();
    if !root.join("tat1").join("manifest.ron").exists() || !root.join("classes").exists() {
        eprintln!("skipped: no bake");
        return;
    }
    // A map's load is seconds: eight at a time.
    let bad: Vec<String> = std::thread::scope(|s| {
        let jobs: Vec<_> = TABLE
            .chunks(TABLE.len().div_ceil(8))
            .map(|chunk| {
                let root = &root;
                s.spawn(move || {
                    let mut bad = Vec::new();
                    for &(key, dead, trap) in chunk {
                        let dir = root.join(key);
                        if !dir.join("manifest.ron").exists() {
                            eprintln!("skipped: no {key} bake");
                            continue;
                        }
                        let w = build_sim(&dir).expect(key);
                        let got = (dead_dirs(&w.sim), trapped(&w.sim).len());
                        if got != (dead, trap) {
                            bad.push(format!(
                                "{key}: dead {} trapped {} (pinned {dead} {trap})",
                                got.0, got.1
                            ));
                        }
                    }
                    bad
                })
            })
            .collect();
        jobs.into_iter()
            .flat_map(|j| j.join().expect("a map"))
            .collect()
    });
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Card TPOS: (map, standable, covered) of card BOTD's census
/// (`lead_tmp/botd/out/cover_census.txt`, all four sources summed).
const CENSUS: &[(&str, u32, u32)] = &[
    ("bf1_nab1", 312, 79),
    ("bf1_yav2", 2381, 586),
    ("bf2_hot1", 1136, 220),
    ("bf2_myg1", 2810, 939),
    ("bf2_uta1", 1931, 667),
    ("tat1", 3953, 822),
];

#[test]
fn the_position_table_matches_the_census_on_the_showcase_maps() {
    let root = game::assets::baked_root();
    if !root.join("tat1").join("manifest.ron").exists() || !root.join("classes").exists() {
        eprintln!("skipped: no bake");
        return;
    }
    let bad: Vec<String> = std::thread::scope(|s| {
        let jobs: Vec<_> = CENSUS
            .iter()
            .map(|&(key, standable, covered)| {
                let root = &root;
                s.spawn(move || {
                    let dir = root.join(key);
                    if !dir.join("manifest.ron").exists() {
                        eprintln!("skipped: no {key} bake");
                        return None;
                    }
                    let w = build_sim(&dir).expect(key);
                    let t = &w.sim.positions;
                    let n = t.positions.len() as f32;
                    let c = t.positions.iter().filter(|p| p.covered()).count() as f32;
                    let off = |a: f32, b: u32| (a - b as f32).abs() > 0.05 * b as f32;
                    (off(n, standable) || off(c, covered)).then(|| {
                        format!("{key}: standable {n} covered {c} (census {standable} {covered})")
                    })
                })
            })
            .collect();
        jobs.into_iter()
            .filter_map(|j| j.join().expect("a map"))
            .collect()
    });
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// This thread's CPU seconds so far: utime + stime of
/// `/proc/thread-self/stat` (fields 14 and 15, in clock ticks of 1/100 s);
/// `None` where there is no such file.
fn thread_cpu() -> Option<f64> {
    let s = std::fs::read_to_string("/proc/thread-self/stat").ok()?;
    // After the command's closing parenthesis the state is field 3.
    let f: Vec<&str> = s.get(s.rfind(')')? + 2..)?.split_whitespace().collect();
    let tick = |k: usize| f.get(k)?.parse::<f64>().ok();
    Some((tick(11)? + tick(12)?) / 100.0)
}

/// Card TPOSA2: the build's CPU time, not the wall clock - ci runs the tests
/// side by side and the wall clock read 0.374 s under that load (0.056 s
/// alone).
#[test]
fn mygeetos_position_table_is_built_in_under_a_third_of_a_second() {
    let root = game::assets::baked_root();
    let dir = root.join("bf2_myg1");
    if !dir.join("manifest.ron").exists() || !root.join("classes").exists() {
        eprintln!("skipped: no bf2_myg1 bake");
        return;
    }
    let mut w = build_sim(&dir).expect("bf2_myg1");
    let (t0, c0) = (std::time::Instant::now(), thread_cpu());
    w.sim.build_positions();
    let secs = match (c0, thread_cpu()) {
        (Some(a), Some(b)) => b - a,
        _ => t0.elapsed().as_secs_f64(),
    };
    assert!(w.sim.positions.positions.len() > 2000, "a table");
    assert!(secs < 0.3, "built in {secs:.3} s of CPU");
}
