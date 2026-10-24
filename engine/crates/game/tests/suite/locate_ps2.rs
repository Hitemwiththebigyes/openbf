//! Card LOC3: `NavGraph::locate` against what the PS2 itself stored for the
//! goal of every live path object in the save-state library (audit HUBM,
//! `tests/data/locate_ps2.txt`: 1,004 distinct objects, both games, soldier
//! and vehicle filters). Per object: the kind (in a hub / in a corridor /
//! neither) and, where the PS2 stored a destination, the hub itself or an
//! arc whose two ends are the PS2's destination and its other end.
//!
//! Data-gated: rows on a map with no bake are skipped and counted, and the
//! two maps the port does not bake at all (BF1 Mos Eisley, BF2 space
//! Kashyyyk, 54 rows) run only with `LOCATE_PS2_GRAPHS=<dir>` holding the
//! PS2's own graphs (`lead_tmp/loc3/tools/make_fixture.py` writes them).

use core::nav::{Arc, Hub, Locus, Mover, NavGraph, RouteTable};
use glam::Vec3;
use std::collections::HashMap;
use std::path::Path;

struct Row {
    map: String,
    state: String,
    filter: u32,
    goal: Vec3,
    result: u8,
    dest: Option<u16>,
    other: Option<u16>,
}

fn rows() -> Vec<Row> {
    let text = include_str!("../data/locate_ps2.txt");
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            assert_eq!(f.len(), 9, "row: {l}");
            let num = |i: usize| f[i].parse::<f32>().expect("a number");
            Row {
                map: f[0].to_owned(),
                state: f[1].to_owned(),
                filter: f[2].parse().expect("filter"),
                goal: Vec3::new(num(3), num(4), num(5)),
                result: f[6].parse().expect("result"),
                dest: f[7].parse().ok(),
                other: f[8].parse().ok(),
            }
        })
        .collect()
}

/// A baked map's conquest graph, as a battle loads it.
fn baked(key: &str) -> Option<NavGraph> {
    let dir = game::assets::baked_root().join(key);
    if !dir.join("manifest.ron").is_file() {
        return None;
    }
    let mut scene = game::assets::load_scene(&dir).ok()?;
    scene.apply_mode_layer("con");
    game::layout::nav_graph(&scene.manifest, &scene.dir)
}

/// The PS2's own graph of a save state, in `make_fixture.py`'s text form.
fn ram_graph(dir: &Path, state: &str) -> Option<NavGraph> {
    let text = std::fs::read_to_string(dir.join(format!("{state}.txt"))).ok()?;
    let (mut hubs, mut arcs, mut bf2) = (Vec::new(), Vec::new(), false);
    for l in text.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        let num = |i: usize| f[i].parse::<f32>().expect("a number");
        match f.first().copied() {
            Some("game") => bf2 = f[1] == "bf2",
            Some("hub") => hubs.push(Hub {
                pos: Vec3::new(num(1), num(2), num(3)),
                radius: num(4),
                arcs: f[5..].iter().map(|s| s.parse().expect("slot")).collect(),
            }),
            Some("arc") => arcs.push(Arc {
                a: f[1].parse().expect("a"),
                b: f[2].parse().expect("b"),
                type_mask: f[3].parse().expect("mask"),
                flags: f[4].parse().expect("flags"),
            }),
            _ => {}
        }
    }
    let n = hubs.len();
    let mut g = NavGraph::new(hubs, arcs, RouteTable::empty(n), Vec::new(), Vec::new())?;
    g.hub_discs = bf2;
    Some(g)
}

/// Does the port's answer hold what the PS2 stored?
fn agrees(g: &NavGraph, r: &Row) -> Result<(), String> {
    // The PS2's locate has no jet test: its mask is the type bit alone.
    let m = Mover::new(r.filter.trailing_zeros() as usize, true);
    // A filter no arc of the map carries: the PS2 locates nothing; the
    // port's nearest hub for it is its own and not under test.
    let typed = g.arcs.iter().any(|a| a.type_mask & r.filter != 0);
    let got = if typed { g.locate(r.goal, m) } else { None };
    let kind = match got {
        Some(Locus::Hub(_)) => 1,
        Some(Locus::Arc(..)) => 2,
        Some(Locus::Nearest(..)) | None => 0,
    };
    let fail = |why: &str| {
        Err(format!(
            "{} {} filter {} goal {:?}: PS2 result {} dest {:?} other {:?}, port {got:?} ({why})",
            r.map, r.state, r.filter, r.goal, r.result, r.dest, r.other
        ))
    };
    if kind != r.result {
        return fail("kind");
    }
    let Some(dest) = r.dest else {
        return Ok(());
    };
    match got {
        Some(Locus::Hub(h)) if h != dest => fail("hub"),
        Some(Locus::Arc(j, _) | Locus::Nearest(j, _)) => {
            let arc = g.arcs[j as usize];
            let ends = [arc.a, arc.b];
            let both = r
                .other
                .is_none_or(|o| ends == [dest, o] || ends == [o, dest]);
            if ends.contains(&dest) && both {
                Ok(())
            } else {
                fail("arc ends")
            }
        }
        _ => Ok(()),
    }
}

#[test]
fn locate_gives_what_the_ps2_stored_for_every_live_goal() {
    let rows = rows();
    assert_eq!(rows.len(), 1004, "the fixture's distinct objects");
    let extra = std::env::var_os("LOCATE_PS2_GRAPHS").map(std::path::PathBuf::from);
    let mut graphs: HashMap<String, Option<NavGraph>> = HashMap::new();
    let (mut ok, mut skipped, mut bad) = (0, 0, Vec::new());
    for r in &rows {
        let key = if r.map == "-" { &r.state } else { &r.map };
        let g = graphs.entry(key.clone()).or_insert_with(|| {
            if r.map == "-" {
                extra.as_deref().and_then(|d| ram_graph(d, &r.state))
            } else {
                baked(&r.map)
            }
        });
        let Some(g) = g else {
            skipped += 1;
            continue;
        };
        match agrees(g, r) {
            Ok(()) => ok += 1,
            Err(e) => bad.push(e),
        }
    }
    report_results(ok, skipped, rows.len(), &bad);
}

fn report_results(ok: usize, skipped: usize, total: usize, bad: &[String]) {
    println!(
        "locate vs the PS2's stored goals: {ok} of {} agree, {skipped} skipped (no graph), {} differ",
        total - skipped,
        bad.len()
    );
    if ok == 0 && bad.is_empty() {
        eprintln!("SKIP: no baked maps");
        return;
    }
    assert!(bad.is_empty(), "{} differ:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn locate_reporting_skips_only_when_no_rows_were_compared() {
    report_results(0, 1004, 1004, &[]);
    report_results(1, 1003, 1004, &[]);
    let all_bad = vec!["synthetic mismatch".to_owned(); 1004];
    assert!(std::panic::catch_unwind(|| report_results(0, 0, 1004, &all_bad)).is_err());
    let bad = vec!["synthetic mismatch".to_owned()];
    for (ok, skipped) in [(0, 1003), (1, 1002)] {
        assert!(
            std::panic::catch_unwind(|| report_results(ok, skipped, 1004, &bad)).is_err(),
            "available failing rows must fail even when none agree"
        );
    }
}
