use super::*;

/// BF2's English map names (§6), the rest from the tables' own fallbacks.
fn text(game: Game, key: &str) -> Option<String> {
    let names = [
        ("cor1", "Coruscant"),
        ("dag1", "Dagobah"),
        ("dea1", "Death Star"),
        ("end1", "Endor"),
        ("fel1", "Felucia"),
        ("geo1", "Geonosis"),
        ("hot1", "Hoth"),
        ("tat3", "Jabba's Palace"),
        ("kam1", "Kamino"),
        ("kas2", "Kashyyyk"),
        ("tat2", "Mos Eisley"),
        ("mus1", "Mustafar"),
        ("myg1", "Mygeeto"),
        ("nab2", "Naboo"),
        ("pol1", "Polis Massa"),
        ("spa7", "Space Felucia"),
        ("spa8", "Space Hoth"),
        ("spa3", "Space Kashyyyk"),
        ("spa6", "Space Mygeeto"),
        ("spa9", "Space Tatooine"),
        ("spa1", "Space Yavin"),
        ("tan1", "Tantive IV"),
        ("uta1", "Utapau"),
        ("yav1", "Yavin 4"),
    ];
    let code = key.strip_prefix("mapname.name.")?;
    (game == Game::Bf2)
        .then(|| names.iter().find(|(c, _)| *c == code))
        .flatten()
        .map(|(_, n)| (*n).to_owned())
}

/// The main checkout's bake (§6 "What the port already bakes"): every map
/// but the six space maps, BF1 Dune Sea and BF2 Mos Eisley in their legacy
/// directories; conquest missions for every era a map has.
fn maps() -> Vec<MapEntry> {
    let mut bakes: Vec<(String, String)> = Vec::new();
    for (code, ..) in BF1_MAPS {
        let key = if code == "tat1" {
            "tat1".to_owned()
        } else {
            format!("bf1_{code}")
        };
        bakes.push((key, "bf1".into()));
    }
    for (code, ..) in BF2_MAPS {
        if code.starts_with("spa") {
            continue;
        }
        let key = if code == "tat2" {
            "tat2".to_owned()
        } else {
            format!("bf2_{code}")
        };
        bakes.push((key, "bf2".into()));
    }
    let eras = |key: &str| -> Vec<Era> {
        let bf2 = BF2_MAPS
            .iter()
            .find(|(c, ..)| key == format!("bf2_{c}") || (key == *c && *c == "tat2"));
        if let Some((_, cw, gcw)) = bf2 {
            let mut v = Vec::new();
            if cw.contains(&Mode::Con) {
                v.push(Era::Cw);
            }
            if gcw.contains(&Mode::Con) {
                v.push(Era::Gcw);
            }
            return v;
        }
        let (_, _, _, cw, gcw) = BF1_MAPS
            .iter()
            .find(|(c, ..)| key == format!("bf1_{c}") || (key == *c && *c == "tat1"))
            .copied()
            .unwrap();
        [(cw, Era::Cw), (gcw, Era::Gcw)]
            .into_iter()
            .filter(|(on, _)| *on)
            .map(|(_, e)| e)
            .collect()
    };
    let missions = |key: &str, mode: Mode| -> Vec<[String; 2]> {
        if mode != Mode::Con {
            return Vec::new();
        }
        eras(key)
            .into_iter()
            // Missions name the defender first as often as not.
            .map(|e| {
                let [a, b] = e.armies();
                [b.to_owned(), a.to_owned()]
            })
            .collect()
    };
    catalog(&text, &bakes, &missions)
}

fn ia() -> InstantAction {
    InstantAction::new(
        maps(),
        IaTexts::new(&|_, e| e.to_owned()),
        SessionOptions::default(),
    )
}

fn press(f: impl Fn(&mut IaInput)) -> IaInput {
    let mut i = IaInput::default();
    f(&mut i);
    i
}

fn cross() -> IaInput {
    press(|i| i.menu.confirm = true)
}

fn triangle() -> IaInput {
    press(|i| i.menu.back = true)
}

fn down() -> IaInput {
    press(|i| i.menu.down = true)
}

fn up() -> IaInput {
    press(|i| i.menu.up = true)
}

/// §6 "Merged list": the 40 rows in the case-folded order, each with its
/// game, then All Maps.
#[test]
fn the_merged_list_is_both_games_case_folded() {
    let m = maps();
    let labels: Vec<&str> = m.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "BESPIN: CLOUD CITY (bf1)",
            "BESPIN: PLATFORMS (bf1)",
            "Coruscant (bf2)",
            "Dagobah (bf2)",
            "Death Star (bf2)",
            "Endor (bf2)",
            "ENDOR: BUNKER (bf1)",
            "Felucia (bf2)",
            "Geonosis (bf2)",
            "GEONOSIS: SPIRE (bf1)",
            "Hoth (bf2)",
            "HOTH: ECHO BASE (bf1)",
            "Jabba's Palace (bf2)",
            "Kamino (bf2)",
            "KAMINO: TIPOCA CITY (bf1)",
            "Kashyyyk (bf2)",
            "KASHYYYK: DOCKS (bf1)",
            "KASHYYYK: ISLANDS (bf1)",
            "Mos Eisley (bf2)",
            "Mustafar (bf2)",
            "Mygeeto (bf2)",
            "Naboo (bf2)",
            "NABOO: PLAINS (bf1)",
            "NABOO: THEED (bf1)",
            "Polis Massa (bf2)",
            "RHEN VAR: CITADEL (bf1)",
            "RHEN VAR: HARBOR (bf1)",
            "Space Felucia (bf2)",
            "Space Hoth (bf2)",
            "Space Kashyyyk (bf2)",
            "Space Mygeeto (bf2)",
            "Space Tatooine (bf2)",
            "Space Yavin (bf2)",
            "Tantive IV (bf2)",
            "TATOOINE: DUNE SEA (bf1)",
            "TATOOINE: MOS EISLEY (bf1)",
            "Utapau (bf2)",
            "Yavin 4 (bf2)",
            "YAVIN 4: ARENA (bf1)",
            "YAVIN 4: TEMPLE (bf1)",
        ]
    );
    // Keys: the bake's directories, the legacy two unprefixed; a map that
    // is not baked keeps `<game>_<code>`.
    let key = |l: &str| m.iter().find(|e| e.label == l).unwrap().key.as_str();
    assert_eq!(key("Hoth (bf2)"), "bf2_hot1");
    assert_eq!(key("HOTH: ECHO BASE (bf1)"), "bf1_hot1");
    assert_eq!(key("Mos Eisley (bf2)"), "tat2");
    assert_eq!(key("TATOOINE: MOS EISLEY (bf1)"), "bf1_tat2");
    assert_eq!(key("TATOOINE: DUNE SEA (bf1)"), "tat1");
    assert_eq!(key("Space Yavin (bf2)"), "bf2_spa1");
    let rows = ia().map_rows();
    assert_eq!(rows.len(), 41);
    assert_eq!(rows[40].text, "All Maps");
}

/// §7: the unbaked space maps are dimmed and the cursor skips them both
/// ways; a mode the port does not run is dimmed, and so is All Modes with
/// only Conquest live.
#[test]
fn dimmed_rows_are_skipped_by_the_cursor() {
    let mut m = ia();
    let rows = m.map_rows();
    let dimmed: Vec<&str> = rows
        .iter()
        .filter(|r| !r.enabled)
        .map(|r| r.text.as_str())
        .collect();
    assert_eq!(dimmed.len(), 6, "{dimmed:?}");
    assert!(dimmed.iter().all(|t| t.starts_with("Space ")));
    m.step(cross(), 0.0); // ADD
    assert_eq!(m.state, State::Map);
    let polis = rows
        .iter()
        .position(|r| r.text == "Polis Massa (bf2)")
        .unwrap();
    m.map_list = ListCursor {
        at: polis,
        top: polis - 4,
    };
    m.step(down(), 0.0);
    assert_eq!(rows[m.map_list.at].text, "RHEN VAR: CITADEL (bf1)");
    m.step(down(), 0.0);
    m.step(down(), 0.0);
    assert_eq!(rows[m.map_list.at].text, "Tantive IV (bf2)", "six skipped");
    m.step(up(), 0.0);
    assert_eq!(rows[m.map_list.at].text, "RHEN VAR: HARBOR (bf1)");
    // Hoth (bf2): Conquest live, 1-flag CTF and Hunt dimmed, All Modes too.
    let hoth = rows.iter().position(|r| r.text == "Hoth (bf2)").unwrap();
    m.map_list.at = hoth;
    m.step(cross(), 0.0);
    assert_eq!(m.state, State::Mode);
    let modes: Vec<(String, bool)> = m
        .mode_rows()
        .into_iter()
        .map(|r| (r.text, r.enabled))
        .collect();
    assert_eq!(
        modes,
        [
            ("Conquest".into(), true),
            ("1-flag CTF".into(), false),
            ("Hunt".into(), false),
            ("All Modes".into(), false),
        ]
    );
    m.step(down(), 0.0);
    assert_eq!(m.mode_list.at, 0, "nothing live below Conquest");
}

/// §1.3: every press of the five states, the dim rule and the focus.
#[test]
fn the_states_and_their_presses() {
    let mut m = ia();
    assert_eq!(m.state, State::Command);
    assert_eq!(COMMANDS[m.command], Cmd::Add, "LAUNCH is dimmed: ADD first");
    assert!(!m.command_live(Cmd::Launch));
    assert!(
        m.command_live(Cmd::Remove),
        "REMOVE is live on an empty list"
    );
    assert_eq!(m.info(), "Add a map to the playlist.");
    // Up wraps past the dimmed LAUNCH to OPTIONS.
    m.step(up(), 0.0);
    assert_eq!(COMMANDS[m.command], Cmd::Options);
    m.step(down(), 0.0);
    assert_eq!(COMMANDS[m.command], Cmd::Add);
    // ADD -> Planet; Triangle back; ADD again.
    m.step(cross(), 0.0);
    assert_eq!(m.state, State::Map);
    assert!(COMMANDS.iter().all(|c| !m.command_live(*c)), "all dimmed");
    assert_eq!(COMMANDS[m.command], Cmd::Add, "ADD keeps its focus");
    assert_eq!(m.info(), "BESPIN: CLOUD CITY", "a BF1 map: its name alone");
    m.step(triangle(), 0.0);
    assert_eq!(m.state, State::Command);
    m.step(cross(), 0.0);
    // Planet -> Mode -> Era, Triangle walks back one at a time.
    let hoth = m
        .map_rows()
        .iter()
        .position(|r| r.text == "Hoth (bf2)")
        .unwrap();
    m.map_list.at = hoth;
    m.step(cross(), 0.0);
    assert_eq!(m.state, State::Mode);
    assert!(m.info().starts_with("Conquest"));
    m.step(cross(), 0.0);
    assert_eq!(m.state, State::Era);
    let eras: Vec<String> = m.side_rows().into_iter().map(|r| r.text).collect();
    assert_eq!(eras[0], "Galactic Civil War", "Hoth has no Clone Wars");
    assert_eq!(eras.len(), 1, "no All Eras for one era, no cross-era pairs");
    assert_eq!(m.info(), "Galactic Civil War");
    m.step(triangle(), 0.0);
    assert_eq!(m.state, State::Mode);
    m.step(triangle(), 0.0);
    assert_eq!(m.state, State::Map);
    m.step(cross(), 0.0);
    m.step(cross(), 0.0);
    // Cross on the era adds the row, back to the stack on LAUNCH.
    m.step(cross(), 0.0);
    assert_eq!(m.state, State::Command);
    assert_eq!(COMMANDS[m.command], Cmd::Launch);
    assert_eq!(m.playlist.len(), 1);
    assert_eq!(m.play_rows()[1].text, "Hoth (bf2)");
    assert_eq!(m.play_rows()[1].icon, Some("mode_icon_con"));
    assert_eq!(m.play_rows()[1].icon2, Some("imp_icon"));
    // REMOVE -> the playlist on "Remove All" (info empty), Down, Cross
    // deletes Hoth; with only Remove All left the focus is ADD.
    m.step(down(), 0.0);
    m.step(down(), 0.0);
    assert_eq!(COMMANDS[m.command], Cmd::Remove);
    m.step(cross(), 0.0);
    assert_eq!(m.state, State::Playlist);
    m.play_list.at = 0;
    assert_eq!(m.info(), "");
    m.step(down(), 0.0);
    assert_eq!(m.info(), "Hoth (bf2)");
    m.step(cross(), 0.0);
    assert!(m.playlist.is_empty());
    assert_eq!(m.state, State::Command);
    assert_eq!(COMMANDS[m.command], Cmd::Add);
    // Triangle in the command state leaves for Singleplayer; Cross on a
    // dimmed LAUNCH does nothing.
    m.command = 0;
    assert_eq!(m.step(cross(), 0.0), None);
    assert_eq!(m.step(triangle(), 0.0), Some(IaCommand::Back));
}

#[test]
fn remove_all_clears_and_removing_keeps_the_rest() {
    let mut m = ia();
    for i in [2, 3, 4] {
        m.add(
            MapPick::One(i),
            ModePick::One(Mode::Con),
            SidePick::One(Side::Era(Era::Cw)),
        );
    }
    assert_eq!(m.playlist.len(), 3);
    m.focus_command(Cmd::Remove);
    m.step(cross(), 0.0);
    m.play_list.at = 2;
    m.step(cross(), 0.0);
    let left: Vec<usize> = m.playlist.iter().map(|e| e.map).collect();
    assert_eq!(left, [2, 4]);
    assert_eq!(COMMANDS[m.command], Cmd::Launch);
    m.focus_command(Cmd::Remove);
    m.step(cross(), 0.0);
    m.play_list.at = 0;
    m.step(cross(), 0.0);
    assert!(m.playlist.is_empty(), "Remove All");
}

/// §1.4: All Maps / All Modes / All Eras add what the port runs, eras
/// outer and modes inner; the list stops at 128 and says so.
#[test]
fn all_maps_modes_eras_and_the_cap() {
    let mut m = ia();
    let hoth = m
        .maps
        .iter()
        .position(|e| e.label == "Kashyyyk (bf2)")
        .unwrap();
    m.add(MapPick::One(hoth), ModePick::All, SidePick::AllEras);
    let sides: Vec<Side> = m.playlist.iter().map(|e| e.side).collect();
    assert_eq!(sides, [Side::Era(Era::Cw), Side::Era(Era::Gcw)]);
    assert!(m.playlist.iter().all(|e| e.mode == Mode::Con));
    m.playlist.clear();
    m.add(MapPick::All, ModePick::All, SidePick::AllEras);
    // 34 baked maps; geo1, hot1, end1 (both games), kam1 (bf1) have one era.
    let one_era = ["Endor (bf2)", "ENDOR: BUNKER (bf1)", "Geonosis (bf2)"];
    assert!(one_era.iter().all(|l| m.maps.iter().any(|e| e.label == *l)));
    let expected: usize = m
        .maps
        .iter()
        .filter(|e| e.baked)
        .map(|e| ERAS.iter().filter(|x| e.has_era(**x)).count())
        .sum();
    assert_eq!(m.playlist.len(), expected);
    assert!(!m.full);
    // The order: map by map in list order, Clone Wars before Galactic
    // Civil War.
    assert_eq!(m.maps[m.playlist[0].map].label, "BESPIN: CLOUD CITY (bf1)");
    assert_eq!(m.playlist[0].side, Side::Era(Era::Cw));
    assert_eq!(m.playlist[1].side, Side::Era(Era::Gcw));
    // Twice more: 128 rows, then the popup; Cross closes it.
    m.add(MapPick::All, ModePick::All, SidePick::AllEras);
    m.add(MapPick::All, ModePick::All, SidePick::AllEras);
    assert_eq!(m.playlist.len(), MAX_MISSIONS);
    assert!(m.full);
    assert_eq!(m.texts.list_full, "Selected maplist is full.");
    m.step(cross(), 0.0);
    assert!(!m.full);
}

/// §2.3: every option's default, step, L1/R1 ×10 and clamp.
#[test]
fn option_ranges_steps_and_defaults() {
    let mut o = SessionOptions::default();
    assert_eq!(
        (
            o.num_bots,
            o.difficulty,
            o.con_numbots,
            o.con_mult,
            o.con_timer
        ),
        (16, 2, 16, 100, 0)
    );
    assert_eq!(
        (o.ctf_score, o.ctf_timer, o.hun_timer, o.hun_score),
        (5, 0, 5, 50)
    );
    assert_eq!(o.ass_score, 180);
    o.adjust(Opt::NumBots, 1, 10);
    assert_eq!(o.num_bots, MAX_BOTS);
    assert_eq!(
        (o.con_numbots, o.ctf_numbots, o.ass_numbots),
        (MAX_BOTS, MAX_BOTS, MAX_BOTS)
    );
    o.adjust(Opt::NumBots, -1, 10);
    o.adjust(Opt::NumBots, -1, 10);
    o.adjust(Opt::NumBots, -1, 10);
    assert_eq!(o.num_bots, 0);
    o.adjust(Opt::AiDifficulty, 1, 1);
    assert_eq!(o.difficulty, 3);
    o.adjust(Opt::AiDifficulty, -1, 10);
    assert_eq!(o.difficulty, 1, "clamped at Easy");
    o.adjust(Opt::AiDifficulty, 1, 10);
    assert_eq!(o.difficulty, DIFFICULTY_LEVELS, "clamped at Elite");
    o.adjust(Opt::ConMult, 1, 1);
    assert_eq!(o.con_mult, 110);
    o.adjust(Opt::ConMult, 1, 10);
    assert_eq!(o.con_mult, 210);
    for _ in 0..5 {
        o.adjust(Opt::ConMult, 1, 10);
    }
    assert_eq!(o.con_mult, 500);
    for _ in 0..6 {
        o.adjust(Opt::ConMult, -1, 10);
    }
    assert_eq!(o.con_mult, 10);
    o.adjust(Opt::ConTimer, 1, 1);
    assert_eq!(o.con_timer, 5);
    o.adjust(Opt::ConTimer, 1, 10);
    assert_eq!(o.con_timer, 55);
    o.adjust(Opt::ConTimer, 1, 1);
    o.adjust(Opt::ConTimer, 1, 1);
    assert_eq!(o.con_timer, 60);
    o.adjust(Opt::HunTimer, -1, 10);
    assert_eq!(o.hun_timer, 5, "Hunt's timer has no Off");
    o.adjust(Opt::HunScore, 1, 10);
    assert_eq!(o.hun_score, 150);
    o.adjust(Opt::AssScore, -1, 10);
    assert_eq!(o.ass_score, 80);
    o.adjust(Opt::AssScore, -1, 10);
    assert_eq!(o.ass_score, 50);
    o.adjust(Opt::CtfScore, 1, 10);
    assert_eq!(o.ctf_score, 15);
    o.adjust(Opt::HeroAssign, 1, 1);
    assert_eq!(o.hero_assign, 4);
    o.adjust(Opt::HeroAssign, 1, 10);
    assert_eq!(o.hero_assign, 7);
    o.hero_respawn = 120;
    o.adjust(Opt::HeroRespawn, 1, 1);
    assert_eq!(o.hero_respawn, -1, "121 wraps to Never");
    o.adjust(Opt::HeroUnlock, 1, 1);
    assert_eq!(o.hero_unlock, 5);
    assert_eq!(o.rows(Group::Hero), [Opt::HeroesOnOff], "Heroes Off");
    o.adjust(Opt::HeroesOnOff, 1, 1);
    assert_eq!(o.rows(Group::Hero).len(), 5);
}

/// menuref §9.1.1-2: AI units per team 0..16 on every page that has the
/// row; Global copies into Conquest, CTF and Assault.
#[test]
fn ai_per_team_is_capped_at_16() {
    assert_eq!(MAX_BOTS, 16);
    let mut o = SessionOptions::default();
    for opt in [
        Opt::NumBots,
        Opt::ConNumBots,
        Opt::CtfNumBots,
        Opt::AssNumBots,
    ] {
        for _ in 0..3 {
            o.adjust(opt, 1, 1);
        }
        o.adjust(opt, 1, 10);
        let v = [o.num_bots, o.con_numbots, o.ctf_numbots, o.ass_numbots];
        assert!(v.iter().all(|n| *n == 16), "{opt:?} {v:?}");
    }
    o.adjust(Opt::ConNumBots, -1, 10);
    o.adjust(Opt::ConNumBots, -1, 10);
    assert_eq!(o.con_numbots, 0, "minimum 0");
    o.num_bots = 16;
    o.adjust(Opt::NumBots, -1, 1);
    o.adjust(Opt::NumBots, -1, 1);
    o.adjust(Opt::NumBots, -1, 1);
    o.adjust(Opt::NumBots, -1, 1);
    o.adjust(Opt::NumBots, -1, 1);
    o.adjust(Opt::NumBots, -1, 1);
    assert_eq!(
        (o.num_bots, o.con_numbots, o.ctf_numbots, o.ass_numbots),
        (10, 10, 10, 10),
        "Global 10 -> all three read 10"
    );
}

/// menuref "Session Options" Hero rows (`so_hero`, `so_hero_ranges`,
/// `sheets/hero_all_steps.png`): the defaults and every range.
#[test]
fn hero_defaults_and_ranges() {
    let t = IaTexts::new(&|_, e| e.to_owned());
    let flat = |p: Vec<Piece>| -> String {
        p.into_iter()
            .map(|x| match x {
                Piece::Text(s) => s,
                Piece::Left => "<".into(),
                Piece::Right => ">".into(),
            })
            .collect()
    };
    let mut o = SessionOptions::default();
    assert!(!o.heroes, "Heroes Off");
    assert_eq!(o.rows(Group::Hero), [Opt::HeroesOnOff], "the rest hidden");
    assert_eq!(flat(t.option(Opt::HeroesOnOff, &o)), "Heroes: < Off >");
    o.adjust(Opt::HeroesOnOff, 1, 1);
    let rows: Vec<String> = o
        .rows(Group::Hero)
        .into_iter()
        .map(|r| flat(t.option(r, &o)))
        .collect();
    assert_eq!(
        rows,
        [
            "Heroes: < On >",
            "Unlock Hero: < Points >",
            "First to < 10 > points",
            "Assign To: < Best Player >",
            "Timer: < 90 > seconds",
        ]
    );
    // Unlock: Points <-> Timer only (Right, Right, Left = Timer, Points,
    // Timer); one shared value.
    let mut seen = Vec::new();
    for dir in [1, 1, -1] {
        o.adjust(Opt::HeroUnlock, dir, 1);
        seen.push(o.hero_unlock);
    }
    assert_eq!(seen, [5, 3, 5]);
    assert_eq!(flat(t.option(Opt::HeroUnlockVal, &o)), "< 10 > seconds");
    // The value: 1..120, L1/R1 10 steps, clamped.
    for _ in 0..12 {
        o.adjust(Opt::HeroUnlockVal, 1, 10);
    }
    assert_eq!(o.hero_unlock_val, 120);
    o.adjust(Opt::HeroUnlockVal, 1, 1);
    assert_eq!(o.hero_unlock_val, 120);
    for _ in 0..13 {
        o.adjust(Opt::HeroUnlockVal, -1, 10);
    }
    assert_eq!(o.hero_unlock_val, 1);
    // Assign: Best -> Worst -> Random, no wrap; Left Random -> Worst.
    let mut seen = Vec::new();
    for dir in [1, 1, 1, -1] {
        o.adjust(Opt::HeroAssign, dir, 1);
        seen.push(o.hero_assign);
    }
    assert_eq!(seen, [4, 7, 7, 4]);
    // Timer: 90, Always (0) .. 120, then Never; Never stays, Left -> 120.
    assert_eq!(o.hero_respawn, 90);
    for _ in 0..10 {
        o.adjust(Opt::HeroRespawn, -1, 10);
    }
    assert_eq!(o.hero_respawn, 0);
    assert_eq!(flat(t.option(Opt::HeroRespawn, &o)), "Timer: < Always >");
    o.adjust(Opt::HeroRespawn, -1, 1);
    assert_eq!(o.hero_respawn, 0, "Left at Always stays");
    o.hero_respawn = 120;
    o.adjust(Opt::HeroRespawn, 1, 1);
    assert_eq!(o.hero_respawn, -1);
    assert_eq!(flat(t.option(Opt::HeroRespawn, &o)), "Timer: < Never >");
    o.adjust(Opt::HeroRespawn, 1, 1);
    assert_eq!(o.hero_respawn, -1, "Right at Never stays");
    o.adjust(Opt::HeroRespawn, -1, 1);
    assert_eq!(o.hero_respawn, 120);
}

/// menuref "Session Options": every value press clicks select, even at a
/// clamp; Up/Down select, silent at the ends; Cross nothing; Triangle one
/// exit. Overview: moves select, Cross accept.
#[test]
fn session_options_clicks() {
    use crate::shell_feel::MenuSound as S;
    let mut m = ia();
    m.focus_command(Cmd::Options);
    m.step(cross(), 0.0);
    assert_eq!(m.take_clicks(), [S::Accept]);
    m.step(down(), 0.0);
    m.step(press(|i| i.menu.left = true), 0.0);
    assert_eq!(
        m.take_clicks(),
        [S::Select],
        "Left on the overview: nothing"
    );
    m.step(up(), 0.0);
    m.step(cross(), 0.0);
    assert_eq!(m.take_clicks(), [S::Select, S::Accept]);
    // Global: AI at 16, Right and R1 clamp but click.
    m.step(press(|i| i.menu.right = true), 0.0);
    m.step(press(|i| i.r1 = true), 0.0);
    assert_eq!(m.options.num_bots, 16);
    assert_eq!(m.take_clicks(), [S::Select, S::Select]);
    m.step(up(), 0.0);
    assert!(m.take_clicks().is_empty(), "silent at the first row");
    m.step(down(), 0.0);
    m.step(down(), 0.0);
    assert_eq!(m.take_clicks(), [S::Select], "silent at the last row");
    m.step(cross(), 0.0);
    assert!(m.take_clicks().is_empty(), "Cross does nothing");
    m.step(triangle(), 0.0);
    m.step(triangle(), 0.0);
    assert_eq!(m.take_clicks(), [S::Exit, S::Exit]);
}

/// menuref "Instant Action" and the sound table: stack moves click (wrap
/// too), Left/Right/L1/R1 nothing in the command state; a list click only
/// when the cursor moves; Triangle back from the stack is the
/// Singleplayer's re-Enter (no click here); Cross on "Remove All" error.
#[test]
fn instant_action_clicks() {
    use crate::shell_feel::MenuSound as S;
    let mut m = ia();
    m.step(up(), 0.0);
    m.step(down(), 0.0);
    let presses: [fn(&mut IaInput); 4] = [
        |i: &mut IaInput| i.menu.left = true,
        |i: &mut IaInput| i.menu.right = true,
        |i: &mut IaInput| i.l1 = true,
        |i: &mut IaInput| i.r1 = true,
    ];
    for f in presses {
        m.step(press(f), 0.0);
    }
    assert_eq!(m.take_clicks(), [S::Select, S::Select]);
    m.step(cross(), 0.0);
    assert_eq!(m.take_clicks(), [S::Accept]);
    m.step(up(), 0.0);
    assert!(m.take_clicks().is_empty(), "Up on row 0: silent");
    m.step(down(), 0.0);
    m.step(press(|i| i.r1 = true), 0.0);
    assert_eq!(m.take_clicks(), [S::Select, S::Select]);
    m.step(triangle(), 0.0);
    assert_eq!(m.take_clicks(), [S::Exit]);
    assert_eq!(m.step(triangle(), 0.0), Some(IaCommand::Back));
    assert!(m.take_clicks().is_empty());
    m.add(
        MapPick::One(2),
        ModePick::One(Mode::Con),
        SidePick::One(Side::Era(Era::Cw)),
    );
    m.focus_command(Cmd::Remove);
    m.step(cross(), 0.0);
    m.play_list.at = 0;
    m.take_clicks();
    m.step(cross(), 0.0);
    assert_eq!(m.take_clicks(), [S::Error]);
    assert!(m.playlist.is_empty());
    // The full popup: closed by Triangle with accept.
    m.full = true;
    m.step(triangle(), 0.0);
    assert_eq!(m.take_clicks(), [S::Accept]);
    assert!(!m.full);
}

/// menuref §9.1.11 (`ia_list_scroll`, `ia_list_page`): on the PS2's 25
/// Planet rows Down moves to the 9th visible row, then scrolls one row per
/// press; R1 / L1 page 8 rows (0 -> 8 -> 16 -> 24), not moving at the end.
/// REMOVE puts the cursor on the playlist's last row.
#[test]
fn lists_scroll_one_row_and_page_eight() {
    let live = [true; 25];
    let mut c = ListCursor::default();
    for k in 1..LIST_ROWS {
        assert!(c.step(&live, 1, LIST_ROWS));
        assert_eq!((c.at, c.top), (k, 0));
    }
    c.step(&live, 1, LIST_ROWS);
    assert_eq!((c.at, c.top), (LIST_ROWS, 1));
    c.step(&live, 1, LIST_ROWS);
    assert_eq!((c.at, c.top), (LIST_ROWS + 1, 2));
    let page = LIST_PAGE as isize;
    let mut c = ListCursor::default();
    let mut at = Vec::new();
    for _ in 0..4 {
        let moved = c.step(&live, page, LIST_ROWS);
        at.push((c.at, moved));
    }
    assert_eq!(at, [(8, true), (16, true), (24, true), (24, false)]);
    assert_eq!(c.top, 25 - LIST_ROWS);
    let mut back = Vec::new();
    for _ in 0..3 {
        c.step(&live, -page, LIST_ROWS);
        back.push(c.at);
    }
    assert_eq!(back, [16, 8, 0]);
    // The same through the screen's R1.
    let mut m = ia();
    m.step(cross(), 0.0);
    m.step(press(|i| i.r1 = true), 0.0);
    assert_eq!(m.map_list.at, LIST_PAGE);
    // The playlist: entered on its last row.
    let mut m = ia();
    for i in [2, 3, 4] {
        m.add(
            MapPick::One(i),
            ModePick::One(Mode::Con),
            SidePick::One(Side::Era(Era::Cw)),
        );
    }
    m.play_list = ListCursor::default();
    m.focus_command(Cmd::Remove);
    m.step(cross(), 0.0);
    assert_eq!(m.play_list.at, 3);
}

/// The maintainer, 2026-10-28 (a named deviation from the PS2's Normal <-> Elite
/// toggle): the AI Difficulty row offers exactly the port's four tiers, in
/// order, a step per press from the lowest to the highest.
#[test]
fn the_ai_difficulty_row_offers_exactly_four_tiers() {
    let t = IaTexts::new(&|_, e| e.to_owned());
    let mut o = SessionOptions {
        difficulty: 1,
        ..SessionOptions::default()
    };
    let name = |o: &SessionOptions| -> String {
        t.option(Opt::AiDifficulty, o)
            .into_iter()
            .map(|x| match x {
                Piece::Text(s) => s,
                Piece::Left => "<".into(),
                Piece::Right => ">".into(),
            })
            .collect()
    };
    let mut seen = vec![name(&o)];
    for _ in 0..6 {
        o.adjust(Opt::AiDifficulty, 1, 1);
        let n = name(&o);
        if seen.last() != Some(&n) {
            seen.push(n);
        }
    }
    assert_eq!(
        seen,
        [
            "AI Difficulty Level: < Easy >",
            "AI Difficulty Level: < Normal >",
            "AI Difficulty Level: < Hard >",
            "AI Difficulty Level: < Elite >",
        ]
    );
    assert_eq!(DIFFICULTY_LEVELS, 4);
}

/// menuref §9.1.5: the session options outlive leaving the screen.
#[test]
fn options_persist_across_leaving_instant_action() {
    let mut m = ia();
    m.options.adjust(Opt::NumBots, -1, 10);
    m.options.adjust(Opt::ConMult, 1, 5);
    let kept = m.options;
    assert_eq!(m.step(triangle(), 0.0), Some(IaCommand::Back));
    m.enter();
    assert_eq!(m.options, kept);
}

#[test]
fn option_rows_read_as_the_disc_writes_them() {
    let t = IaTexts::new(&|_, e| e.to_owned());
    let o = SessionOptions::default();
    let flat = |p: Vec<Piece>| -> String {
        p.into_iter()
            .map(|x| match x {
                Piece::Text(s) => s,
                Piece::Left => "<".into(),
                Piece::Right => ">".into(),
            })
            .collect()
    };
    assert_eq!(
        flat(t.option(Opt::NumBots, &o)),
        "AI Units Per Team: < 16 >"
    );
    assert_eq!(
        flat(t.option(Opt::AiDifficulty, &o)),
        "AI Difficulty Level: < Normal >"
    );
    assert_eq!(flat(t.option(Opt::ConMult, &o)), "Reinforcements: < 100% >");
    assert_eq!(flat(t.option(Opt::ConTimer, &o)), "Timer: < Off >");
    let mut o2 = o;
    o2.hero_respawn = 1;
    assert_eq!(flat(t.option(Opt::HeroRespawn, &o2)), "Timer: < 1 > second");
    assert_eq!(
        t.help(Opt::ConNumBots),
        "Set the number of AI units per team for all Conquest matches in your session."
    );
}

/// §2.1-2.2: OPTIONS opens the overview (Hero, CTF, Hunt, Assault dimmed
/// and skipped), a group page adjusts with Left/Right and L1/R1, Triangle
/// walks back and asks for the prefs to be kept.
#[test]
fn session_options_pages() {
    let mut m = ia();
    m.focus_command(Cmd::Options);
    m.step(cross(), 0.0);
    assert_eq!(m.page, Page::Overview(0));
    let live: Vec<bool> = m.overview().iter().map(|(_, l)| *l).collect();
    assert_eq!(live, [true, false, true, false, false, false]);
    m.step(down(), 0.0);
    assert_eq!(m.page, Page::Overview(2), "Hero skipped");
    m.step(down(), 0.0);
    assert_eq!(m.page, Page::Overview(0), "wraps past the dimmed three");
    m.step(up(), 0.0);
    assert_eq!(m.page, Page::Overview(2));
    m.step(cross(), 0.0);
    assert!(matches!(m.page, Page::Group(Group::Conquest, _)));
    m.step(down(), 0.0);
    m.step(press(|i| i.menu.right = true), 0.0);
    m.step(press(|i| i.r1 = true), 0.0);
    assert_eq!(m.options.con_mult, 210);
    m.step(press(|i| i.l1 = true), 0.0);
    m.step(press(|i| i.menu.left = true), 0.0);
    assert_eq!(m.options.con_mult, 100);
    m.step(cross(), 0.0);
    assert!(matches!(m.page, Page::Group(..)), "Cross does nothing");
    assert_eq!(m.step(triangle(), 0.0), Some(IaCommand::Prefs));
    assert_eq!(m.page, Page::Overview(2));
    m.step(triangle(), 0.0);
    assert_eq!(m.page, Page::Missions);
    assert_eq!(m.state, State::Command);
}

/// §1.4, §7: LAUNCH hands the whole playlist over in order, map, mode and
/// armies per row; L1 / R1 page the lists.
#[test]
fn launch_hands_over_the_playlist_in_order() {
    let mut m = ia();
    let at = |l: &str| m.maps.iter().position(|e| e.label == l).unwrap();
    let (hoth, kam) = (at("Hoth (bf2)"), at("KAMINO: TIPOCA CITY (bf1)"));
    m.add(
        MapPick::One(hoth),
        ModePick::One(Mode::Con),
        SidePick::One(Side::Era(Era::Gcw)),
    );
    m.add(
        MapPick::One(kam),
        ModePick::One(Mode::Con),
        SidePick::One(Side::Mixed(1)),
    );
    m.focus_command(Cmd::Launch);
    let Some(IaCommand::Launch(q)) = m.step(cross(), 0.0) else {
        panic!("no launch");
    };
    assert_eq!(q.len(), 2);
    assert_eq!(q[0].key, "bf2_hot1");
    assert_eq!(q[0].armies, ["all".to_owned(), "imp".to_owned()]);
    assert_eq!(q[1].key, "bf1_kam1");
    assert_eq!(q[1].game, Game::Bf1);
    assert_eq!(q[1].armies, ["all".to_owned(), "cis".to_owned()]);
    assert!(q.iter().all(|e| e.mode == Mode::Con));
    // Paging: R1 moves a page down, the view follows.
    let mut m = ia();
    m.step(cross(), 0.0);
    m.step(press(|i| i.r1 = true), 0.0);
    m.step(press(|i| i.r1 = true), 0.0);
    assert_eq!(m.map_list.at, 2 * LIST_PAGE);
    assert_eq!(m.map_list.top, 2 * LIST_PAGE + 1 - LIST_ROWS);
    m.step(press(|i| i.l1 = true), 0.0);
    m.step(press(|i| i.l1 = true), 0.0);
    assert_eq!(m.map_list.at, 0);
}
