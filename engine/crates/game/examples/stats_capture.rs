//! Issue #50's pictures: the stats pages drawn headless with the numbers of
//! the PS2 reference frames (`engine/artifacts/lead_tmp/statsref/`), for laying
//! a 640×480 render beside them.
//!
//!   cargo run -p game --example stats_capture -- <page> <w> <h> <out.png>
//!
//! `<page>`: `team` (Kamino `kam1_cw/20_teamstats.png`), `team_dag`
//! (Dagobah `dag1_gcw/20_teamstats.png`), `personal` (a bot's page,
//! `kam1_cw/308_after_K.png`), `personal_me` (`dag1_gcw/300_after_K.png`),
//! `career` (`dag1_gcw/301_after_K.png`). The texts are BF2's table's
//! (`OPENBF_ASSETS` names a bake other than `engine/assets_baked`).

use anyhow::{anyhow, Result};
use game::viewer::{camera_from, GOLDEN_CAM};
use ui::stats::{Award, Page, StatRow, StatsScreens, TeamList};

fn row(name: &str, [points, kills, deaths]: [i32; 3]) -> StatRow {
    StatRow {
        name: name.into(),
        points,
        kills,
        deaths,
        ..Default::default()
    }
}

fn award(label: &str, winner: &str) -> Award {
    Award {
        label: label.into(),
        winner: winner.into(),
    }
}

/// The frames' lists, awards and pages.
fn pages(page: &str, base: StatsScreens) -> StatsScreens {
    let lists = |l: Vec<StatRow>, r: Vec<StatRow>, icons: [&str; 2]| {
        [
            TeamList {
                icon: icons[0].into(),
                rows: l,
            },
            TeamList {
                icon: icons[1].into(),
                rows: r,
            },
        ]
    };
    let personal = |v: [&str; 9]| -> Vec<(String, String)> {
        let labels = [
            "Player Points",
            "Hero Points",
            "Kills",
            "Deaths",
            "",
            "Favorite Class",
            "Favorite Weapon",
            "Head Shots",
            "Nemesis",
            "Bait",
        ];
        let mut values = v.iter();
        labels
            .iter()
            .map(|l| {
                let value = if l.is_empty() {
                    ""
                } else {
                    values.next().copied().unwrap_or("")
                };
                ((*l).to_owned(), value.to_owned())
            })
            .collect()
    };
    let me = |mut r: StatRow| {
        r.me = true;
        r
    };
    let (lists, awards) = if page == "team_dag" || page == "personal_me" || page == "career" {
        let mut player = me(row("PLAYER 1", [0, 0, 1]));
        player.personal = personal([
            "0",
            "0",
            "0",
            "1",
            "REBEL SOLDIER 0:36",
            "BLASTER RIFLE 0:36",
            "0",
            "KL 987 (1)",
            "",
        ]);
        (
            lists(
                vec![
                    row("THRAK FARELLE", [1, -1, 1]),
                    row("LAN OLESA", [0, 1, 1]),
                    row("BEN PULASTRA", [0, 0, 0]),
                    row("RAYCE FARELLE", [0, 0, 0]),
                    row("DORAN KROTAN", [0, 0, 0]),
                    row("BEN ULRAND", [0, 0, 1]),
                    player,
                    row("TIGHE BODALLA", [0, 0, 2]),
                    row("LOT KROTAN", [0, 0, 3]),
                    row("DAX OLESA", [0, 0, 3]),
                ],
                vec![
                    row("PG 134", [2, 0, 1]),
                    row("SU 697", [1, 1, 1]),
                    row("CX 512", [1, 1, 2]),
                    row("JM 221", [1, 1, 2]),
                    row("SF 021", [1, -1, 1]),
                    row("VH 961", [1, -1, 2]),
                    row("KG 077", [0, 0, 0]),
                    row("ZP 778", [0, 0, 1]),
                    row("JE 929", [-2, -1, 1]),
                    row("RP 493", [-2, -1, 2]),
                ],
                ["all_icon", "imp_icon"],
            ),
            [
                vec![
                    award("Tank Buster", ""),
                    award("Killing Spree", "DAX OLESA (2)"),
                    award("Camper", "RAYCE FARELLE (2:14)"),
                    award("Survivalist", "ARAN NOMANTE (1)"),
                ],
                vec![
                    award("Traitor", "KL 987 (2)"),
                    award("Public Enemy", "RP 493 (4)"),
                    award("Bantha Fodder", "LOT KROTAN (3)"),
                    award("Dead Eye", ""),
                ],
            ],
        )
    } else {
        let mut rho = row("RHO 571", [1, 1, 0]);
        rho.personal = personal([
            "1",
            "0",
            "1",
            "0",
            "HEAVY TROOPER 1:31",
            "LASER CANNONS 0:42",
            "0",
            "",
            "UNIT 892 (1)",
        ]);
        (
            lists(
                vec![
                    row("UNIT 464", [1, 1, 1]),
                    row("UNIT 273", [1, 1, 1]),
                    row("UNIT 032", [1, 1, 1]),
                    row("UNIT 397", [1, 1, 2]),
                    row("UNIT 466", [1, 1, 2]),
                    row("UNIT 253", [0, 0, 0]),
                    row("UNIT 538", [0, 0, 1]),
                    row("UNIT 790", [0, 0, 1]),
                    row("UNIT 754", [0, 0, 1]),
                    row("UNIT 503", [0, 0, 2]),
                ],
                vec![
                    row("OMEGA 370", [1, 2, 1]),
                    rho,
                    row("EPSILON 497", [1, 1, 1]),
                    row("GAMMA 935", [1, 1, 1]),
                    row("PSI 487", [1, 1, 2]),
                    me(row("PLAYER 1", [0, 0, 0])),
                    row("RHO 557", [0, 0, 0]),
                    row("ALPHA 332", [0, 0, 0]),
                    row("CHI 178", [0, 0, 0]),
                    row("UPSILON 941", [0, 0, 5]),
                ],
                ["cis_icon", "rep_icon"],
            ),
            [
                vec![
                    award("Tank Buster", ""),
                    award("Killing Spree", "UPSILON 941 (2)"),
                    award("Camper", "UNIT 503 (1:24)"),
                    award("Survivalist", "UNIT 464 (1)"),
                ],
                vec![
                    award("Traitor", "UNIT 538 (1)"),
                    award("Public Enemy", "UPSILON 941 (5)"),
                    award("Bantha Fodder", "UNIT 466 (2)"),
                    award("Dead Eye", ""),
                ],
            ],
        )
    };
    let mut s = StatsScreens::new(base.texts, lists, awards, base.career);
    match page {
        "personal" => {
            s.row = 1;
            s.page = Page::Personal;
        }
        "personal_me" => s.page = Page::Personal,
        "career" => s.page = Page::Career,
        _ => {}
    }
    s
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [page, w, h, out] = args.as_slice() else {
        return Err(anyhow!("usage: stats_capture <page> <w> <h> <out.png>"));
    };
    let (w, h): (u32, u32) = (w.parse()?, h.parse()?);
    let root = std::env::var_os("OPENBF_ASSETS")
        .map_or_else(game::assets::baked_root, std::path::PathBuf::from);
    // The table's texts, by a battle-less feed and the game's own builder.
    let feed = game::event_feed::EventFeed::new(
        game::event_feed::Texts::load(&root, "bf2"),
        ["rep", "cis"],
        &[],
    );
    let walk = game::walk::WalkSim::new(1, glam::Vec3::ZERO, None);
    let text = |key: &str, en: &str| {
        let t = match key.strip_prefix('#') {
            Some(hash) => feed.text_by_hash(u32::from_str_radix(hash, 16).unwrap_or(0), en),
            None => feed.screen_text(key, en),
        };
        t.trim().to_owned()
    };
    let unit = |_: usize| String::new();
    let names = game::round_stats::Names {
        unit: &unit,
        class: &|_, _| String::new(),
        weapon: &|_| String::new(),
        text: &text,
        sides: feed.sides(),
    };
    let career = game::profile::Career {
        deaths: 1,
        ..Default::default()
    };
    let base =
        game::round_stats::screens(&walk.sim, &Default::default(), walk.player, &career, &names);
    let st = pages(page, base);
    let mut r = render::Renderer::new_headless(w, h).map_err(|e| anyhow!("{e}"))?;
    let mut hud = game::hud_feed::HudView::for_renderer(&root, &r)?;
    let model = ui::hud::HudModel {
        round_end: Some(ui::hud::RoundEndModel {
            result: 0,
            stats: Some(st),
        }),
        ..Default::default()
    };
    hud.update(&model, 0.0, [w, h]);
    r.render_with_overlay(
        &camera_from(GOLDEN_CAM),
        &[],
        [0.0, 0.0, 0.0, 1.0],
        Some(&mut |dev, q, enc, view, size| hud.paint(dev, q, enc, view, size)),
    )
    .map_err(|e| anyhow!("render: {e}"))?;
    let (rgba, cw, ch) = r.capture_rgba().map_err(|e| anyhow!("{e}"))?;
    std::fs::write(
        out,
        render::encode_png_rgb(&rgba, cw, ch).map_err(|e| anyhow!("{e}"))?,
    )?;
    Ok(())
}
