//! Card EVMSG end to end on the baked discs (`analysis/re-event-messages.md`):
//! a real conquest battle's events through `game::event_feed`, with the names
//! the discs' own text tables give.
//!
//! Skips without the baked maps; the name checks skip without the baked text
//! (`bake locl`, in the `common` group). `OPENBF_ASSETS` names another bake.

use game::event_feed::EventFeed;
use game::layout::MapLayout;
use game::mission::MissionDef;
use game::walk::WalkSim;
use std::path::PathBuf;

fn assets() -> PathBuf {
    std::env::var_os("OPENBF_ASSETS").map_or_else(game::assets::baked_root, PathBuf::from)
}

/// A conquest battle on the baked map `dir` under `mission`, the player on
/// team 0, and the map's source game.
fn battle(dir: &str, mission: &str) -> Option<(WalkSim, String)> {
    let Ok(mut scene) = game::assets::load_scene(&assets().join(dir)) else {
        eprintln!("SKIP: {dir} is not baked");
        return None;
    };
    scene.apply_mode_layer("con");
    let missions = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../missions"));
    let mission = MissionDef::load(&missions.join(mission)).expect("mission");
    let mut layout = MapLayout::from_manifest(&scene.manifest);
    layout.apply_mission(&mission);
    let mut sim = core::sim::Sim::empty(7);
    sim.battle = core::battle::Battle::new(mission.tickets());
    sim.omni_radius = core::sim::omni_radius_for(&scene.manifest.source_game);
    sim.ground_rules = core::soldier::GroundRules::for_game(&scene.manifest.source_game);
    sim.ai_alive_cap = mission.ai_alive_cap;
    let mut teams = [0, 1].map(|_| game::walk::TeamTemplate::default_trooper());
    for (t, def) in teams.iter_mut().zip(&mission.teams) {
        t.faction = def.faction.clone();
    }
    let nav = game::layout::nav_graph(&scene.manifest, &scene.dir);
    let mut walk = WalkSim::with_teams(sim, scene.heightfield(), glam::Vec3::ZERO, teams, nav);
    scene.install_collision(&mut walk.sim);
    walk.setup_conquest(&layout, [12, 12], true);
    Some((walk, scene.manifest.source_game.clone()))
}

fn has_text() -> bool {
    let table = assets().join("common_bf2/locl/english.ron");
    if !table.is_file() {
        eprintln!("SKIP: no baked text table at {}", table.display());
    }
    table.is_file()
}

/// The teams and the posts carry the discs' names: BF2 Hoth's "Echo Base",
/// and BF1 Kashyyyk's "PIER ONE" — the post the PS2 printed (§1) — in BF2's
/// case.
#[test]
fn a_battles_teams_and_posts_carry_the_discs_names() {
    if !has_text() {
        return;
    }
    if let Some((walk, game)) = battle("bf2_hot1", "bf2_hot1_con.ron") {
        let feed = EventFeed::for_walk(&assets(), &game, &walk);
        println!("bf2_hot1: {:?} {:?}", feed.team_names(), feed.post_names());
        assert_eq!(feed.team_names(), ["Empire", "Rebels"]);
        assert_eq!(feed.post_names().len(), walk.sim.cps.len());
        for want in ["Echo Base", "Bunker", "Outpost", "Forward Camp"] {
            assert!(feed.post_names().iter().any(|n| n == want), "{want}");
        }
        // The player and one soldier of each side, as the PS2 names them.
        let name = |team: u8| {
            let (i, _) = walk
                .sim
                .units
                .iter()
                .enumerate()
                .find(|(_, u)| u.team == team && !u.is_player)
                .expect("an AI");
            feed.unit_name(
                &walk.sim,
                core::unit::UnitHandle {
                    index: i as u32,
                    gen: 0,
                },
            )
        };
        assert_eq!(feed.unit_name(&walk.sim, walk.player), "Player 1");
        let (imp, all) = (name(0), name(1));
        println!("an Imperial {imp:?}, a Rebel {all:?}");
        assert_eq!(imp.len(), 6, "two letters and three digits: {imp:?}");
        assert_eq!(all.split(' ').count(), 2, "first and last name: {all:?}");
        assert!(all.chars().next().is_some_and(char::is_alphabetic));
    }
    if let Some((walk, game)) = battle("bf1_kas2", "bf1_kas2_con_cw.ron") {
        let feed = EventFeed::for_walk(&assets(), &game, &walk);
        println!("bf1_kas2: {:?} {:?}", feed.team_names(), feed.post_names());
        assert_eq!(feed.team_names(), ["CIS", "Republic"]);
        assert!(feed.post_names().iter().any(|n| n == "Pier One"));
    }
}

/// A minute of BF2 Hoth beside an idle player (the PS2 run of §1.1: 170 s
/// of AI battle made no line but the player's own death): every line is a
/// post line or about the player, however many AI die.
#[test]
fn a_battle_beside_an_idle_player_prints_only_post_lines_and_his_own_death() {
    let Some((mut walk, game)) = battle("bf2_hot1", "bf2_hot1_con.ron") else {
        return;
    };
    let mut feed = EventFeed::for_walk(&assets(), &game, &walk);
    let pad = core::sim::PadState::default();
    let (mut lines, mut deaths) = (Vec::new(), 0u32);
    for _ in 0..30 * 60 {
        walk.step_tick(std::slice::from_ref(&pad));
        feed.tick(&walk.sim, walk.player);
        deaths += walk
            .sim
            .events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    core::sim::Event::Kill { .. } | core::sim::Event::EnvDeath { .. }
                )
            })
            .count() as u32;
        lines.extend(feed.take());
    }
    println!("{deaths} deaths, {} lines:", lines.len());
    for l in &lines {
        println!("  {:?} {}", l.color, l.text);
    }
    assert!(deaths >= 5, "a battle was fought: {deaths} deaths");
    assert!(
        lines.len() < deaths as usize,
        "no kill feed: {} lines for {deaths} deaths",
        lines.len()
    );
    for l in &lines {
        let post = l.text.contains(" captured the ") || l.text.contains(" lost the ");
        let mine = l.text.contains("Player 1");
        assert!(
            post || mine,
            "a line the PS2's gate does not make: {}",
            l.text
        );
    }
    // The player's statistics moved with nothing he did not do.
    let st = feed.stats(&walk.sim, walk.player);
    println!("the idle player's statistics: {st:?}");
    assert_eq!(st.kills, 0);
}

/// The spawn screen's class names come from the discs' text (card SPSEL):
/// every class the six showcase missions field resolves `entity.<side>.<rest>`
/// in its own game's table — BF2 Hoth's Snowtrooper, BF1 Naboo's Rebels'
/// "Wookiee Smuggler" — never the roster's `UnitType` fallback.
#[test]
fn every_showcase_class_has_the_discs_name() {
    if !has_text() || !assets().join("common_bf1/locl/english.ron").is_file() {
        return;
    }
    let missions = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../missions"));
    for file in [
        "bf1_nab1_con.ron",
        "tat1_con.ron",
        "bf1_yav2_con.ron",
        "bf2_uta1_con.ron",
        "bf2_myg1_con.ron",
        "bf2_hot1_con.ron",
    ] {
        let m = MissionDef::load(&missions.join(file)).expect("mission");
        let texts = game::event_feed::Texts::load(&assets(), &m.source_game);
        for team in &m.teams {
            let names: Vec<String> = team
                .classes
                .iter()
                .map(|c| {
                    texts
                        .class_name(&c.0)
                        .unwrap_or_else(|| panic!("{file}: {} has no name", c.0))
                })
                .collect();
            println!("{file} {}: {}", team.faction, names.join(", "));
        }
    }
    let texts = game::event_feed::Texts::load(&assets(), "bf2");
    assert_eq!(
        texts.class_name("imp_inf_rifleman_snow").as_deref(),
        Some("Snowtrooper")
    );
    assert_eq!(
        texts.class_name("rep_inf_ep3_sniper").as_deref(),
        Some("Clone Sharpshooter")
    );
    let texts = game::event_feed::Texts::load(&assets(), "bf1");
    assert_eq!(
        texts.class_name("all_inf_smuggler").as_deref(),
        Some("Wookiee Smuggler")
    );
    assert_eq!(texts.class_name("nosuchside"), None);
}

/// Card SPSEL on a baked map (shares this suite's battle helper): BF2 Hoth's
/// player dies, an available D-pad direction takes the post the PS2's rule
/// gives from the post nearest his body, the pick has an index in the post
/// list, and he comes back on one of THAT post's nodes.
#[test]
fn a_dead_player_picks_a_post_on_hoth_and_spawns_there() {
    let Some((mut walk, game)) = battle("bf2_hot1", "bf2_hot1_con.ron") else {
        return;
    };
    let _ = game;
    let i = walk.player.index as usize;
    let team = walk.sim.units[i].team;
    let body = walk.sim.units[i].soldier.pos;
    walk.sim.units[i].alive = false;
    walk.sim.units[i].respawn_at = Some(walk.sim.tick + 30);
    walk.sim.battle.teams[usize::from(team)].alive_players -= 1;
    let posts = walk.sim.player_spawn_posts(team);
    assert!(posts.len() >= 2, "{posts:?}");
    let nearest = posts
        .iter()
        .min_by(|a, b| a.1.distance(body).total_cmp(&b.1.distance(body)))
        .unwrap();
    // The push that has a post ahead of the nearest one.
    let arms = [
        (core::sim::buttons::SPAWN_POST_RIGHT, glam::Vec2::X),
        (core::sim::buttons::SPAWN_POST_LEFT, -glam::Vec2::X),
        (core::sim::buttons::SPAWN_POST_DOWN, glam::Vec2::Y),
        (core::sim::buttons::SPAWN_POST_UP, -glam::Vec2::Y),
    ];
    let (bit, want) = arms
        .iter()
        .find_map(|&(bit, d)| {
            core::sim::Sim::pick_spawn_post(&posts, Some(nearest.0), nearest.1, d).map(|p| (bit, p))
        })
        .expect("some post lies some way");
    let press = core::sim::PadState {
        buttons: bit,
        ..Default::default()
    };
    walk.step_tick(std::slice::from_ref(&press));
    assert_eq!(walk.sim.spawn_post_selected(0), Some(want));
    let row = spawn_screen_row(&walk);
    assert!(
        row.is_some(),
        "the selected post has an index in the post list"
    );
    println!(
        "posts {posts:?}; pushed {bit:#x} from {:?} to {want:?}; post-list row {row:?}",
        nearest.0
    );
    let idle = core::sim::PadState::default();
    for _ in 0..600 {
        if walk.sim.units[i].alive {
            break;
        }
        walk.step_tick(std::slice::from_ref(&idle));
    }
    let me = &walk.sim.units[i];
    assert!(me.alive, "respawned");
    let nodes = walk.sim.spawn_post_nodes(team, want);
    let d = nodes
        .iter()
        .map(|(n, _)| glam::Vec2::new(n.x - me.soldier.pos.x, n.z - me.soldier.pos.z).length())
        .fold(f32::INFINITY, f32::min);
    println!(
        "spawned at {:?}, {d:.2} m from the nearest of the post's {} nodes",
        me.soldier.pos,
        nodes.len()
    );
    assert!(d < 1.0, "{d} m off post {want:?}'s nodes");
}

/// The equivalent selection-index calculation: the sim pick's index in
/// `player_spawn_posts`. This does not exercise `spawn_model` or the UI.
fn spawn_screen_row(walk: &WalkSim) -> Option<usize> {
    let team = walk.sim.units[walk.player.index as usize].team;
    let list = walk.sim.player_spawn_posts(team);
    walk.sim
        .spawn_post_selected(0)
        .and_then(|p| list.iter().position(|q| q.0 == p))
}
