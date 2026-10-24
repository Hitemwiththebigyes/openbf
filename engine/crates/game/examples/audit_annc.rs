//! The commander's census (cards ANNC / ANNC2; written by the audit of ANNC
//! as an independent probe): a whole AI conquest round per mission, set up as `round_to_outcome` sets it up but with the
//! player left in slot 0 (the game's own shape), stepped tick by tick through
//! `Commander::frame` exactly as `main/windowed.rs` calls it. Logs every line
//! asked for and every line the voice starts, with the sim state beside it.
//!
//! audit_annc <max_ticks> <seed> <listener 0|1|keep> <swap 0|1> <mission.ron> ...
//! `swap 1` gives the two teams each other's faction (the shell's "any
//! pairing" choice) while the mission stays the same; `AUDIT_FACTIONS=imp,all`
//! seats any two armies (an era mix). `PS2ASK` / `PS2BLEED` are the probe's
//! own model of BF2's script bleed rate, kept apart from
//! `core::announcer::ScriptBleed` so the two can be compared
//! (`lead_tmp/audit-annc/cmp_bleed.py`). `AUDIT_OLD_FEED=1` feeds the port's
//! commander the sim's clamped rate on BF2 maps, as before card ANNC2.
//! `AUDIT_PICK=rep,cis` runs the round the SHELL starts for those two armies
//! (card ERAM): each `<map>_con.ron` argument names the map, the mission is
//! `game::mission::pick`'s and the listener is the first army's team in it.

use core::sim::{Event, PadState};
use game::announce::Commander;
use game::walk::WalkSim;
use std::path::{Path, PathBuf};

const DT: f32 = 1.0 / 30.0;

/// Resolve definitions for the armies actually seated, including swap/era mix.
fn battle_sounds(dir: &Path, w: &WalkSim, named: &[String]) -> Option<game::sounds::SoundLib> {
    let mut lib = game::sounds::SoundLib::load(dir)?;
    lib.for_battle(
        dir,
        [w.teams[0].faction.as_str(), w.teams[1].faction.as_str()],
        named,
    );
    Some(lib)
}

fn run(
    mission_path: &Path,
    max_ticks: u64,
    seed: u32,
    listener: Option<u8>,
    swap: bool,
    seats: Option<[String; 2]>,
) {
    let root = game::assets::baked_root();
    let mission = game::mission::MissionDef::load(mission_path).expect("mission");
    let dir = root.join(&mission.map);
    let key = mission_path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let mut scene = match game::assets::load_scene(&dir) {
        Ok(s) => s,
        Err(e) => {
            println!("{key} LOAD FAILED {e:#}");
            return;
        }
    };
    scene.apply_mode_layer("con");
    let common_dir = root.join(format!("common_{}", scene.manifest.source_game));
    if common_dir.join("common.ron").exists() {
        if let Ok(c) = game::assets::load_common(&common_dir) {
            scene.merge_common(c);
        }
    }
    let mut layout = game::layout::MapLayout::from_manifest(&scene.manifest);
    let applied = layout.apply_mission(&mission);
    let mut sim = core::sim::Sim::empty(seed);
    sim.omni_radius = core::sim::omni_radius_for(&scene.manifest.source_game);
    sim.water_level = scene.manifest.water.as_ref().map(|w| w.height);
    sim.ground_rules = core::soldier::GroundRules::for_game(&scene.manifest.source_game);
    sim.battle = core::battle::Battle::new(mission.tickets());
    mission.apply_death_drains(&mut sim.battle);
    sim.ai_alive_cap = mission.ai_alive_cap;
    let mut factions = mission.factions();
    if swap {
        factions.swap(0, 1);
    }
    if let Some(s) = seats {
        factions = s;
    }
    if let Ok(v) = std::env::var("AUDIT_FACTIONS") {
        let f: Vec<&str> = v.split(',').collect();
        factions = [f[0].to_owned(), f[1].to_owned()];
    }
    let classes_dir = root.join("classes");
    let rosters = game::roster::FactionRoster::pair(
        &classes_dir,
        &scene.manifest.source_game,
        [factions[0].as_str(), factions[1].as_str()],
        Some(&mission),
    );
    let mut tables: [Vec<core::unit::UnitClass>; 2] = [Vec::new(), Vec::new()];
    let teams = match &rosters {
        Ok((rs, _)) => {
            let mut names: Vec<(String, usize)> = Vec::new();
            let mut t = 0usize;
            rs.each_ref().map(|r| {
                for c in &r.classes {
                    let uc = c.register(&mut sim, &mut names);
                    tables[t].push(uc);
                }
                t += 1;
                game::walk::TeamTemplate::from_roster(r, &tables[t - 1][0], Vec::new())
            })
        }
        Err(e) => {
            println!("{key} rosters unavailable: {e:#} (built-in troopers)");
            let t = game::walk::TeamTemplate::default_trooper();
            factions.clone().map(|faction| game::walk::TeamTemplate {
                faction,
                ..t.clone()
            })
        }
    };
    let mut nav = game::layout::nav_graph(&scene.manifest, &scene.dir);
    mission.apply_arc_blocks(nav.as_mut());
    mission.apply_barriers(nav.as_mut());
    let mut w = WalkSim::with_teams(sim, scene.heightfield(), glam::Vec3::ZERO, teams, nav);
    w.sim.classes = tables;
    scene.install_collision(&mut w.sim);
    w.sim.death_regions = applied.death_regions.clone();
    let ai = [mission.initial_ai(0, true), mission.initial_ai(1, false)];
    w.setup_conquest(&layout, ai, true);
    let mut voice = Commander::for_battle(&mission, &layout, &w);
    // AUDIT_OLD_FEED=1: the feed before card ANNC2 (the sim's clamped rate on
    // BF2 maps too), for a before / after count on one sim.
    if std::env::var("AUDIT_OLD_FEED").is_ok_and(|v| v == "1") {
        voice.script_bleed = None;
    }
    if let Some(l) = listener {
        voice.announcer.set_listener(usize::from(l));
    }
    // The PS2 script's bleed rate (BF2 ObjectiveConquest, bytecode 7.0 / 7.3.0): set at Start and at every
    // finished capture / neutralisation to the unclamped rule; a timer of value 1.0 runs down at `rate` per
    // second; at its elapse `alive < reinforcements` keeps it going (+1.0), else SetBleedRate(team, 0) and stop.
    let mut voice_ps2 = Commander::for_battle(&mission, &layout, &w);
    if let Some(l) = listener {
        voice_ps2.announcer.set_listener(usize::from(l));
    }
    let mut ps2_rate = [0.0f32; 2];
    let mut ps2_timer: [Option<f32>; 2] = [None, None];
    let ps2_update =
        |cps: &[core::cp::CommandPost], rate: &mut [f32; 2], timer: &mut [Option<f32>; 2]| {
            for t in 0..2 {
                rate[t] = core::cp::bf2_bleed_rate(cps, t as u8);
                timer[t] = (rate[t] > 0.0).then_some(1.0);
            }
        };
    ps2_update(&w.sim.cps, &mut ps2_rate, &mut ps2_timer);
    let mut ps2_shown = ps2_rate;
    let mut n_ps2_asked = 0u32;
    let mut sl = battle_sounds(&dir, &w, &mission.sound_levels());
    let start_tickets = [0, 1].map(|t| w.sim.battle.teams[t].tickets());
    println!(
        "== {key} map {} seed {seed} factions {:?} (mission {:?}) ai {:?} tickets {:?} low_thr {:?} listener {} posts {} swap {swap}",
        mission.map,
        [w.teams[0].faction.clone(), w.teams[1].faction.clone()],
        mission.factions(),
        ai,
        start_tickets,
        voice.announcer.lines.low_threshold,
        voice.announcer.listener(),
        w.sim.cps.len(),
    );
    for (i, c) in w.sim.cps.iter().enumerate() {
        println!("   post {i} {} owner {:?}", c.def.name, c.owner);
    }
    let idle = [PadState::default()];
    let mut owners: Vec<Option<u8>> = w.sim.cps.iter().map(|c| c.owner).collect();
    let mut bleed = [0.0f32; 2];
    let mut t = 0u64;
    let (mut n_asked, mut n_started, mut n_events) = (0u32, 0u32, 0u32);
    let mut busy_until = 0.0f32;
    let mut max_pending = 0usize;
    let mut player_alive_last = true;
    // AUDIT_HOLD=<team>:<count>:<from secs>: hold a team's reinforcements at a small number (the live PS2 run's poke).
    let hold: Option<(usize, f32, f32)> = std::env::var("AUDIT_HOLD").ok().and_then(|v| {
        let p: Vec<f32> = v.split(':').filter_map(|x| x.parse().ok()).collect();
        (p.len() == 3).then(|| (p[0] as usize, p[1], p[2]))
    });
    while t < max_ticks && w.sim.outcome.is_none() {
        if let Some((team, n, from)) = hold {
            if t as f32 * DT >= from && w.sim.battle.teams[team].reinforcements != n {
                w.sim.battle.teams[team].reinforcements = n;
            }
        }
        w.step_tick(&idle);
        t += 1;
        let secs = t as f32 * DT;
        for e in &w.sim.events {
            match e {
                Event::Captured { cp, team, .. } => {
                    n_events += 1;
                    println!("{secs:8.2} EVENT captured post {cp} by team {team}");
                }
                Event::Neutralized { cp, by } => {
                    n_events += 1;
                    println!("{secs:8.2} EVENT neutralized post {cp} by team {by}");
                }
                _ => {}
            }
        }
        let mut changed = false;
        for (i, c) in w.sim.cps.iter().enumerate() {
            if c.owner != owners[i] {
                println!("{secs:8.2} OWNER post {i} {:?} -> {:?}", owners[i], c.owner);
                owners[i] = c.owner;
                changed = true;
            }
        }
        if changed {
            ps2_update(&w.sim.cps, &mut ps2_rate, &mut ps2_timer);
        }
        for t in 0..2 {
            if let Some(v) = ps2_timer[t] {
                let v = v - ps2_rate[t] * DT;
                if v <= 0.0 {
                    let team = &w.sim.battle.teams[t];
                    if team.alive() < team.tickets() {
                        ps2_timer[t] = Some(v + 1.0);
                    } else {
                        ps2_rate[t] = 0.0;
                        ps2_timer[t] = None;
                    }
                } else {
                    ps2_timer[t] = Some(v);
                }
            }
        }
        if ps2_rate != ps2_shown {
            println!(
                "{secs:8.2} PS2BLEED {ps2_shown:?} -> {ps2_rate:?} alive {:?} tickets {:?}",
                [0, 1].map(|x| w.sim.battle.teams[x].alive()),
                [0, 1].map(|x| w.sim.battle.teams[x].tickets())
            );
            ps2_shown = ps2_rate;
        }
        let b = [0, 1].map(|x| w.sim.battle.teams[x].bleed_per_sec);
        if b != bleed {
            println!(
                "{secs:8.2} BLEED {bleed:?} -> {b:?} alive {:?} tickets {:?}",
                [0, 1].map(|x| w.sim.battle.teams[x].alive()),
                [0, 1].map(|x| w.sim.battle.teams[x].tickets())
            );
            bleed = b;
        }
        let alive = w.sim.unit(w.player).is_some();
        if alive != player_alive_last {
            println!("{secs:8.2} PLAYER unit present: {alive}");
            player_alive_last = alive;
        }
        // The game's own listener rule unless one is forced.
        let forced = listener;
        let last_len = std::cell::Cell::new(0.0f32);
        let mut len_of = |name: &str| -> Option<f32> {
            let r = match sl.as_mut() {
                Some(s) => s.voice_take(name).map(|x| x.2),
                None => Some(2.0),
            };
            if let Some(x) = r {
                last_len.set(x);
            }
            r
        };
        let started = if let Some(l) = forced {
            // `frame` would reset the listener to the player's team: do its work by hand.
            let own: Vec<Option<u8>> = w.sim.cps.iter().map(|c| c.owner).collect();
            voice.announcer.set_listener(usize::from(l));
            voice.asked = voice.observe(
                DT,
                &own,
                [0, 1].map(|x| w.sim.battle.teams[x].bleed_per_sec),
                [0, 1].map(|x| w.sim.battle.teams[x].tickets()),
                true,
                w.sim.outcome.is_some(),
            );
            voice.queue.step(DT, |l| len_of(&l.name))
        } else {
            voice.frame(&w.sim, w.player, DT, |l| len_of(&l.name))
        };
        {
            // The same rules, fed the PS2 script's rate.
            let own: Vec<Option<u8>> = w.sim.cps.iter().map(|c| c.owner).collect();
            let lst = forced.map_or(voice.announcer.listener(), usize::from);
            voice_ps2.announcer.set_listener(lst);
            let spawned = forced.is_some() || w.sim.unit(w.player).is_some();
            let asked2 = voice_ps2.observe(
                DT,
                &own,
                ps2_rate,
                [0, 1].map(|x| w.sim.battle.teams[x].tickets()),
                spawned,
                w.sim.outcome.is_some(),
            );
            for l in &asked2 {
                n_ps2_asked += 1;
                println!(
                    "{secs:8.2} PS2ASK {:<47} tickets {:?} alive {:?} rate {:?}",
                    l.name,
                    [0, 1].map(|x| w.sim.battle.teams[x].tickets()),
                    [0, 1].map(|x| w.sim.battle.teams[x].alive()),
                    ps2_rate
                );
            }
            let _ = voice_ps2.queue.step(DT, |_| Some(2.0));
        }
        for l in &voice.asked {
            n_asked += 1;
            println!(
                "{secs:8.2} ASK   {:<48} tickets {:?} alive {:?} bleed {:?} pending {}",
                l.name,
                [0, 1].map(|x| w.sim.battle.teams[x].tickets()),
                [0, 1].map(|x| w.sim.battle.teams[x].alive()),
                bleed,
                voice.queue.pending().len()
            );
        }
        max_pending = max_pending.max(voice.queue.pending().len());
        if let Some(l) = started {
            n_started += 1;
            let len = last_len.get();
            let cut = secs + 1e-3 < busy_until;
            println!(
                "{secs:8.2} START {:<48} take {len:.2} s{}",
                l.name,
                if cut { "  ** CUT A LINE OFF **" } else { "" }
            );
            busy_until = secs + len;
        }
    }
    println!(
        "== {key} end: tick {t} ({:.1} min) outcome {:?} tickets {:?} events {n_events} asked {n_asked} started {n_started} max pending {max_pending} ps2asked {n_ps2_asked}",
        t as f32 * DT / 60.0,
        w.sim.outcome,
        [0, 1].map(|x| w.sim.battle.teams[x].tickets()),
    );
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let max_ticks: u64 = a[0].parse().expect("max_ticks");
    let seed: u32 = a[1].parse().expect("seed");
    let listener = a[2].parse::<u8>().ok();
    let swap = a[3] == "1";
    let missions = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../missions"));
    let picked = std::env::var("AUDIT_PICK").ok();
    for m in &a[4..] {
        let Some(armies) = picked.as_deref().and_then(|v| v.split_once(',')) else {
            run(&missions.join(m), max_ticks, seed, listener, swap, None);
            continue;
        };
        let map = m.strip_suffix("_con.ron").expect("<map>_con.ron");
        let armies = [armies.0, armies.1];
        let p = game::mission::pick(&missions, map, "con", armies).expect("a mission");
        // A cross-era pair keeps the armies as picked on the default mission.
        let seats = (!p.own).then(|| armies.map(str::to_owned));
        println!(
            "pick {map} {armies:?} -> {} (own {}) player team {}",
            p.path.file_name().unwrap().to_string_lossy(),
            p.own,
            p.player_team
        );
        run(&p.path, max_ticks, seed, Some(p.player_team), false, seats);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battle_sound_definitions_follow_named_levels_and_actual_armies() {
        let root = std::env::temp_dir().join(format!("openbf-annc-fixture-{}", std::process::id()));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let dir = root.join("bf1_kas2");
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        for seconds in [1u32, 2] {
            let data = vec![0u8; seconds as usize * 8000 * 2];
            let mut wav = Vec::new();
            wav.extend_from_slice(b"RIFF");
            wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&8000u32.to_le_bytes());
            wav.extend_from_slice(&16000u32.to_le_bytes());
            wav.extend_from_slice(&2u16.to_le_bytes());
            wav.extend_from_slice(&16u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
            wav.extend_from_slice(&data);
            std::fs::write(dir.join(format!("sounds/{seconds}.wav")), wav).unwrap();
        }
        let hash = game::classdb::fnv1a_lowercase;
        let mut props = Vec::new();
        for name in [
            "all_off_com_report_captured_commandpost",
            "rep_off_com_report_captured_commandpost",
        ] {
            for (level, sample) in [("kas2cw", 1), ("kas2gcw", 2)] {
                props.push(format!(
                    "(name_hash:{},kind:0,gain:1.0,pitch:1.0,min_distance:0.0,max_distance:100.0,rolloff:1.0,priority:1.0,looping:false,samples:[({sample},1.0)],layers:[],level:{})",
                    hash(name), hash(level)
                ));
            }
        }
        let bank = format!(
            "(source:\"fixture\",banks:[],samples:[(1,\"1.wav\"),(2,\"2.wav\")],props:[{}])",
            props.join(",")
        );
        std::fs::write(dir.join("sounds/sounds.ron"), bank).unwrap();
        let mut w = WalkSim::new(1, glam::Vec3::ZERO, None);
        // A GCW mission differs from the map-default CW guess; team reversal
        // must still use the GCW definition.
        w.teams[0].faction = "imp".into();
        w.teams[1].faction = "all".into();
        let named = ["kas2gcw".to_owned()];
        let mut sl = battle_sounds(&dir, &w, &named).unwrap();
        assert_eq!(
            sl.voice_take("all_off_com_report_captured_commandpost")
                .unwrap()
                .2,
            2.0
        );
        // A Republic army seated in that GCW mission needs its own era's
        // definition, even though the mission itself still names GCW.
        w.teams[0].faction = "rep".into();
        let mut sl = battle_sounds(&dir, &w, &named).unwrap();
        assert_eq!(
            sl.voice_take("rep_off_com_report_captured_commandpost")
                .unwrap()
                .2,
            1.0
        );
        assert_eq!(
            sl.voice_take("all_off_com_report_captured_commandpost")
                .unwrap()
                .2,
            2.0
        );
    }
}
