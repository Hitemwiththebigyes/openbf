//! BF2's PS2 Instant Action screen (`ifs_missionselect`) and its Session
//! Options (`ifs_instant_options_overview` / `ifs_instant_options`), the
//! model: the map list, the five states and every press, the playlist, the
//! options and the launch queue (`analysis/re-bf2-menus.md` §1, §2, §6, §7).
//! Drawn by the HUD pass in `crate::shell_ia`; the game builds the catalog
//! from its bake and plays the playlist it is handed.
//!
//! `[ours]` (§7): the map list holds both games' maps, each "<name> (bf1)" /
//! "<name> (bf2)"; rows, modes, eras and option groups the port cannot run
//! are dimmed (grey 110, never focused) - the PS2 has that style for button
//! stacks only, its list boxes have none; the era list ends with the port's
//! cross-era pairs.

use crate::menu::MenuInput;
use crate::shell_feel::MenuSound;

/// `ScriptCB_GetMaxMissionQueue()` = 128 (§1.3, B2 357683).
pub const MAX_MISSIONS: usize = 128;
/// Rows shown by the Planet, Mode and Playlist lists, and by the Era list
/// (`clamp(floor((403.2 - 240) / 17), 4, 16)` = 9, Era half that; §1.2).
pub const LIST_ROWS: usize = 9;
pub const ERA_ROWS: usize = 4;
/// L1 / R1 move a 9-row list 8 rows (menuref "Instant Action": 0 -> 8 ->
/// 16 -> 24 on the Planet list, `ia_list_page`).
pub const LIST_PAGE: usize = 8;
/// Rows shown on an options page (§2.2).
pub const OPTION_ROWS: usize = 5;
/// `iMaxBots`: AI units per team 0..16 (menuref §9.1.1 [M], `so_global`:
/// Right and R1 at 16 stay 16; the ELF image's 16, not the 25 of
/// `ScriptCB_GetNetGameDefaults`' non-network branch).
pub const MAX_BOTS: i32 = 16;
/// The modes the port's sim can run: Conquest only (`setup_conquest`;
/// §7 dims the rest).
pub const PORT_MODES: [Mode; 1] = [Mode::Con];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Game {
    Bf1,
    Bf2,
}

impl Game {
    pub fn tag(self) -> &'static str {
        match self {
            Game::Bf1 => "bf1",
            Game::Bf2 => "bf2",
        }
    }
}

/// `gMapModes` in screen order (con, ctf, 1flag, tdm, hunt, ass, eli; §6);
/// no map has tdm, so it is left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Con,
    Ctf,
    OneFlag,
    Hunt,
    Assault,
    Eli,
}

pub const MODES: [Mode; 6] = [
    Mode::Con,
    Mode::Ctf,
    Mode::OneFlag,
    Mode::Hunt,
    Mode::Assault,
    Mode::Eli,
];

impl Mode {
    /// The mission name's suffix (`hot1g_con`), the port's mission files'
    /// too (`bf2_hot1_con.ron`).
    pub fn subst(self) -> &'static str {
        match self {
            Mode::Con => "con",
            Mode::Ctf => "ctf",
            Mode::OneFlag => "1flag",
            Mode::Hunt => "hunt",
            Mode::Assault => "ass",
            Mode::Eli => "eli",
        }
    }

    /// Name and description keys with the disc's English; eli shows as
    /// Assault (`modename.name.assault`, §6).
    fn text(self) -> ((&'static str, &'static str), (&'static str, &'static str)) {
        match self {
            Mode::Con => (
                ("modename.name.con", "Conquest"),
                (
                    "modename.description.con",
                    "Conquest\r\nDefend your command posts while attempting to capture the enemy's.",
                ),
            ),
            Mode::Ctf => (
                ("modename.name.ctf", "2-flag CTF"),
                (
                    "modename.description.ctf",
                    "Capture The Flag (2 Flag)\r\nDefend your team flag while attempting to steal the enemy's.",
                ),
            ),
            Mode::OneFlag => (
                ("modename.name.1flag", "1-flag CTF"),
                (
                    "modename.description.1flag",
                    "Capture The Flag (1 Flag)\r\nBoth teams attempt to take a neutral flag and return it to the enemy base.",
                ),
            ),
            Mode::Hunt => (
                ("modename.name.hunt", "Hunt"),
                (
                    "modename.description.hunt",
                    "Hunt\r\nEliminate as many of the indigenous species as possible before time runs out.",
                ),
            ),
            Mode::Assault => (
                ("modename.name.assault", "Assault"),
                (
                    "modename.description.assault",
                    "Assault\r\nDestroy enemy critical systems (18 points), frigates (18 points), and fighters (3 points).",
                ),
            ),
            Mode::Eli => (
                ("modename.name.assault", "Assault"),
                (
                    "modename.description.elimination",
                    "Assault\r\nHeroes and villains clash sabers and exchange fire in the ultimate duel of the fates.",
                ),
            ),
        }
    }

    /// The row's icon (SHELL.LVL, §6; eli draws Assault's).
    pub fn icon(self) -> &'static str {
        match self {
            Mode::Con => "mode_icon_con",
            Mode::Ctf => "mode_icon_2ctf",
            Mode::OneFlag => "mode_icon_ctf",
            Mode::Hunt => "mode_icon_hunt",
            Mode::Assault | Mode::Eli => "mode_icon_ass",
        }
    }
}

/// `gMapEras` (§6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Era {
    Cw,
    Gcw,
}

pub const ERAS: [Era; 2] = [Era::Cw, Era::Gcw];

impl Era {
    fn index(self) -> usize {
        match self {
            Era::Cw => 0,
            Era::Gcw => 1,
        }
    }

    /// The era's two armies (its mission's faction pair).
    pub fn armies(self) -> [&'static str; 2] {
        match self {
            Era::Cw => ["rep", "cis"],
            Era::Gcw => ["all", "imp"],
        }
    }

    fn text(self) -> (&'static str, &'static str) {
        match self {
            Era::Cw => ("common.era.cw", "Clone Wars"),
            Era::Gcw => ("common.era.gcw", "Galactic Civil War"),
        }
    }

    /// The era row's `icon1` (§6).
    pub fn icon(self) -> &'static str {
        match self {
            Era::Cw => "cis_icon",
            Era::Gcw => "imp_icon",
        }
    }
}

/// `[ours]` The cross-era pairs after "All Eras" (§7; PLAN: era mix and
/// match), each run on the map's default mission.
pub const MIXED: [[&str; 2]; 4] = [
    ["all", "rep"],
    ["all", "cis"],
    ["imp", "rep"],
    ["imp", "cis"],
];

/// The BF2 IA maps (§6 "BF2 tables"): code, the Clone Wars modes, the
/// Galactic Civil War modes.
const BF2_MAPS: [(&str, &[Mode], &[Mode]); 24] = {
    use Mode::*;
    [
        ("cor1", &[Con, Ctf], &[Con, Ctf]),
        ("dag1", &[Con, Ctf], &[Con, Ctf]),
        ("dea1", &[Con, OneFlag], &[Con, OneFlag]),
        ("end1", &[], &[Con, Hunt, OneFlag]),
        ("fel1", &[Con, OneFlag], &[Con, OneFlag]),
        ("geo1", &[Con, Ctf, Hunt], &[]),
        ("hot1", &[], &[Con, OneFlag, Hunt]),
        ("tat3", &[Con, OneFlag], &[Con, OneFlag]),
        ("kam1", &[Con, OneFlag], &[Con, OneFlag]),
        ("kas2", &[Con, Hunt, Ctf], &[Con, Ctf]),
        ("tat2", &[Con, Ctf], &[Con, Ctf, Hunt, Eli]),
        ("mus1", &[Con, Ctf], &[Con, Ctf]),
        ("myg1", &[Con, Ctf], &[Con, Ctf]),
        ("nab2", &[Con, Ctf, Hunt], &[Con, Ctf]),
        ("pol1", &[Con, Ctf], &[Con, Ctf]),
        ("spa7", &[Assault, OneFlag], &[]),
        ("spa8", &[], &[Assault, OneFlag]),
        ("spa3", &[Assault, OneFlag], &[]),
        ("spa6", &[Assault, OneFlag], &[]),
        ("spa9", &[], &[Assault, OneFlag]),
        ("spa1", &[], &[Assault, OneFlag]),
        ("tan1", &[Con, OneFlag], &[Con, OneFlag]),
        ("uta1", &[Con, OneFlag], &[Con, OneFlag]),
        ("yav1", &[Con, OneFlag], &[Con, OneFlag]),
    ]
};

/// The BF1 IA maps (§6 "BF1 table"): code, name key, English, has a Clone
/// Wars mission, has a Galactic Civil War one. Conquest only.
const BF1_MAPS: [(&str, &str, &str, bool, bool); 16] = [
    (
        "bes1",
        "planets.bespin.mapname1",
        "BESPIN: PLATFORMS",
        true,
        true,
    ),
    (
        "bes2",
        "planets.bespin.mapname2",
        "BESPIN: CLOUD CITY",
        true,
        true,
    ),
    (
        "end1",
        "planets.endor.mapname1",
        "ENDOR: BUNKER",
        false,
        true,
    ),
    (
        "geo1",
        "planets.geonosis.mapname1",
        "GEONOSIS: SPIRE",
        true,
        false,
    ),
    (
        "hot1",
        "planets.hoth.mapname1",
        "HOTH: ECHO BASE",
        false,
        true,
    ),
    (
        "kam1",
        "planets.kamino.mapname1",
        "KAMINO: TIPOCA CITY",
        true,
        false,
    ),
    (
        "kas1",
        "planets.kashyyyk.mapname1",
        "KASHYYYK: ISLANDS",
        true,
        true,
    ),
    (
        "kas2",
        "planets.kashyyyk.mapname2",
        "KASHYYYK: DOCKS",
        true,
        true,
    ),
    (
        "nab1",
        "planets.naboo.mapname1",
        "NABOO: PLAINS",
        true,
        true,
    ),
    ("nab2", "planets.naboo.mapname2", "NABOO: THEED", true, true),
    (
        "rhn1",
        "planets.rhenvar.mapname1",
        "RHEN VAR: HARBOR",
        true,
        true,
    ),
    (
        "rhn2",
        "planets.rhenvar.mapname2",
        "RHEN VAR: CITADEL",
        true,
        true,
    ),
    (
        "tat1",
        "planets.tatooine.mapname1",
        "TATOOINE: DUNE SEA",
        true,
        true,
    ),
    (
        "tat2",
        "planets.tatooine.mapname2",
        "TATOOINE: MOS EISLEY",
        true,
        true,
    ),
    (
        "yav1",
        "planets.yavin.mapname1",
        "YAVIN 4: TEMPLE",
        true,
        true,
    ),
    (
        "yav2",
        "planets.yavin.mapname2",
        "YAVIN 4: ARENA",
        true,
        true,
    ),
];

/// One map of the merged list.
#[derive(Debug, Clone, PartialEq)]
pub struct MapEntry {
    /// The bake's directory (`bf2_hot1`, the legacy `tat2`), else
    /// `<game>_<code>` for a map that is not baked.
    pub key: String,
    pub game: Game,
    pub code: &'static str,
    /// "<name> (bf1)" / "<name> (bf2)" (§7).
    pub label: String,
    /// The info box's text: BF2's `mapname.description.<code>`; `[ours]`
    /// BF1 has none on its disc (§6), so its name stands alone.
    pub description: String,
    /// The modes per era (Clone Wars, Galactic Civil War), from the disc's
    /// tables.
    pub modes: [Vec<Mode>; 2],
    pub baked: bool,
    /// The faction pairs of the port's mission files, per mode.
    pub missions: Vec<(Mode, [String; 2])>,
}

impl MapEntry {
    fn has_era(&self, era: Era) -> bool {
        !self.modes[era.index()].is_empty()
    }

    /// The map's modes in `gMapModes` order (any era).
    fn all_modes(&self) -> Vec<Mode> {
        MODES
            .into_iter()
            .filter(|m| self.modes.iter().any(|e| e.contains(m)))
            .collect()
    }

    /// Whether the port can run `mode` with `armies` here: a baked map, a
    /// mode the sim runs, and a mission file - of that pair (either order)
    /// for an era, any of the mode for a cross-era pair (§7).
    pub fn runnable(&self, mode: Mode, side: Side) -> bool {
        if !self.baked || !PORT_MODES.contains(&mode) {
            return false;
        }
        let mut pairs = self.missions.iter().filter(|(m, _)| *m == mode);
        match side {
            Side::Era(era) => {
                self.modes[era.index()].contains(&mode)
                    && pairs.any(|(_, p)| {
                        let a = era.armies();
                        (p[0] == a[0] && p[1] == a[1]) || (p[0] == a[1] && p[1] == a[0])
                    })
            }
            Side::Mixed(_) => pairs.next().is_some(),
        }
    }
}

/// Build the merged list (§6 "Merged list", §7): every map of both games,
/// sorted by the case-folded label. `text(game, key)` is the game's English
/// table; `bakes` the baked maps as (directory, source game); `missions(key,
/// mode)` the faction pairs of that map's mission files for `mode`.
pub fn catalog(
    text: &dyn Fn(Game, &str) -> Option<String>,
    bakes: &[(String, String)],
    missions: &dyn Fn(&str, Mode) -> Vec<[String; 2]>,
) -> Vec<MapEntry> {
    let find = |game: Game, code: &str| {
        let prefixed = format!("{}_{code}", game.tag());
        bakes
            .iter()
            .find(|(k, g)| *k == prefixed || (k == code && g == game.tag()))
            .map(|(k, _)| k.clone())
    };
    let entry = |game: Game, code: &'static str, name: String, desc: String, modes| {
        let baked = find(game, code);
        let key = baked
            .clone()
            .unwrap_or_else(|| format!("{}_{code}", game.tag()));
        let missions = MODES
            .into_iter()
            .flat_map(|m| missions(&key, m).into_iter().map(move |p| (m, p)))
            .collect();
        MapEntry {
            label: format!("{name} ({})", game.tag()),
            key,
            game,
            code,
            description: desc,
            modes,
            baked: baked.is_some(),
            missions,
        }
    };
    let mut out = Vec::new();
    for (code, cw, gcw) in BF2_MAPS {
        let name = text(Game::Bf2, &format!("mapname.name.{code}"))
            .map_or_else(|| code.to_uppercase(), |t| t.trim().to_owned());
        let desc = text(Game::Bf2, &format!("mapname.description.{code}"))
            .map_or_else(|| name.clone(), |t| t.trim_end().to_owned());
        out.push(entry(
            Game::Bf2,
            code,
            name,
            desc,
            [cw.to_vec(), gcw.to_vec()],
        ));
    }
    for (code, key, english, cw, gcw) in BF1_MAPS {
        let name = text(Game::Bf1, key).map_or_else(|| english.to_owned(), |t| t.trim().to_owned());
        let modes = [cw, gcw].map(|on| if on { vec![Mode::Con] } else { Vec::new() });
        out.push(entry(Game::Bf1, code, name.clone(), name, modes));
    }
    out.sort_by_key(|m| m.label.to_lowercase());
    out
}

/// The Era list's pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Era(Era),
    /// `[ours]` an index into [`MIXED`].
    Mixed(usize),
}

impl Side {
    pub fn armies(self) -> [&'static str; 2] {
        match self {
            Side::Era(e) => e.armies(),
            Side::Mixed(i) => MIXED[i],
        }
    }
}

/// One playlist row (`gPickedMapList` minus "Remove All").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// An index into [`InstantAction::maps`].
    pub map: usize,
    pub mode: Mode,
    pub side: Side,
}

/// What LAUNCH hands the game, one per playlist row in order (§1.4, §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub key: String,
    pub game: Game,
    pub mode: Mode,
    pub armies: [String; 2],
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapPick {
    One(usize),
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModePick {
    One(Mode),
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidePick {
    One(Side),
    AllEras,
}

/// `ifs_ms_state` (§1.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Map,
    Mode,
    Era,
    Command,
    Playlist,
}

/// The command stack, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Launch,
    Add,
    Remove,
    Options,
}

pub const COMMANDS: [Cmd; 4] = [Cmd::Launch, Cmd::Add, Cmd::Remove, Cmd::Options];

impl Cmd {
    fn text(self) -> ((&'static str, &'static str), (&'static str, &'static str)) {
        match self {
            Cmd::Launch => (
                ("ifs.missionselect.buttons.text.launch", "Launch"),
                (
                    "ifs.missionselect.buttons.help.launch",
                    "Launch the current playlist.",
                ),
            ),
            Cmd::Add => (
                ("ifs.missionselect.buttons.text.add", "Add"),
                (
                    "ifs.missionselect.buttons.help.add",
                    "Add a map to the playlist.",
                ),
            ),
            Cmd::Remove => (
                ("ifs.missionselect.buttons.text.remove", "Remove"),
                (
                    "ifs.missionselect.buttons.help.remove",
                    "Remove a map from the playlist.",
                ),
            ),
            Cmd::Options => (
                ("ifs.missionselect.buttons.text.options", "Options"),
                (
                    "ifs.missionselect.buttons.help.options",
                    "Modify the session options.",
                ),
            ),
        }
    }
}

/// A list's focused row and its first shown row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ListCursor {
    pub at: usize,
    pub top: usize,
}

impl ListCursor {
    /// Move by `step` rows over `live` (each row's enabled flag), no wrap;
    /// `[ours]` dimmed rows are skipped (§7); `shown` rows are visible.
    fn step(&mut self, live: &[bool], step: isize, shown: usize) -> bool {
        let n = live.len() as isize;
        if n == 0 {
            return false;
        }
        let target = (self.at as isize + step).clamp(0, n - 1);
        let dir = step.signum();
        // The nearest live row from the target on, else back towards here.
        let mut pick = None;
        let mut k = target;
        while (0..n).contains(&k) && dir != 0 {
            if live[k as usize] {
                pick = Some(k);
                break;
            }
            k += dir;
        }
        if pick.is_none() {
            let mut k = target;
            while k != self.at as isize && dir != 0 {
                if live[k as usize] {
                    pick = Some(k);
                    break;
                }
                k -= dir;
            }
        }
        let moved = matches!(pick, Some(p) if p as usize != self.at);
        if let Some(p) = pick {
            self.at = p as usize;
        }
        self.show(n as usize, shown);
        moved
    }

    /// The first live row (or 0), shown.
    fn first(live: &[bool], shown: usize) -> ListCursor {
        let mut c = ListCursor {
            at: live.iter().position(|l| *l).unwrap_or(0),
            top: 0,
        };
        c.show(live.len(), shown);
        c
    }

    fn show(&mut self, n: usize, shown: usize) {
        if self.at < self.top {
            self.top = self.at;
        }
        if self.at >= self.top + shown {
            self.top = self.at + 1 - shown;
        }
        self.top = self.top.min(n.saturating_sub(shown));
    }
}

/// A list row as drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct ListRow {
    pub text: String,
    pub enabled: bool,
    /// The right-edge icon (a mode's, an era's) and the one left of it (a
    /// playlist row's era icon).
    pub icon: Option<&'static str>,
    pub icon2: Option<&'static str>,
}

/// The Session Options groups (§2.1; Host and Elimination are not reachable
/// in single player).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Global,
    Hero,
    Conquest,
    Ctf,
    Hunt,
    Assault,
}

pub const GROUPS: [Group; 6] = [
    Group::Global,
    Group::Hero,
    Group::Conquest,
    Group::Ctf,
    Group::Hunt,
    Group::Assault,
];

impl Group {
    /// The overview button and the page title.
    fn text(self) -> ((&'static str, &'static str), (&'static str, &'static str)) {
        match self {
            Group::Global => (
                ("ifs.instantoptions.buttons.global", "Global"),
                ("ifs.instantoptions.titles.global", "Global Options"),
            ),
            Group::Hero => (
                ("ifs.instantoptions.buttons.hero", "Hero"),
                ("ifs.instantoptions.titles.hero", "Hero Options"),
            ),
            Group::Conquest => (
                ("ifs.instantoptions.buttons.conquest", "Conquest"),
                ("ifs.instantoptions.titles.conquest", "Conquest Options"),
            ),
            Group::Ctf => (
                ("ifs.instantoptions.buttons.ctf", "CTF"),
                ("ifs.instantoptions.titles.ctf", "CTF Options"),
            ),
            Group::Hunt => (
                ("ifs.instantoptions.buttons.hunt", "Hunt"),
                ("ifs.instantoptions.titles.hunt", "Hunt Options"),
            ),
            Group::Assault => (
                ("ifs.instantoptions.buttons.assault", "Assault"),
                ("ifs.instantoptions.titles.assault", "Assault Options"),
            ),
        }
    }

    /// Whether the port honours the group's options: Global (AI per team,
    /// AI difficulty) and Conquest (AI per team, reinforcements, timer).
    /// Heroes, CTF, Hunt and Assault have no port mode: dimmed (§7); their
    /// values are still the PS2's.
    pub fn honoured(self) -> bool {
        matches!(self, Group::Global | Group::Conquest)
    }
}

/// One option row (`ifs_instant_options_tags`, §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opt {
    NumBots,
    AiDifficulty,
    HeroesOnOff,
    HeroUnlock,
    HeroUnlockVal,
    HeroAssign,
    HeroRespawn,
    ConNumBots,
    ConMult,
    ConTimer,
    CtfNumBots,
    CtfScore,
    CtfTimer,
    HunTimer,
    HunScore,
    AssNumBots,
    AssScore,
}

impl Opt {
    /// The help line key (`ifs.mp.createopts.helptext.<tag>`) and English.
    fn help(self) -> (&'static str, &'static str) {
        match self {
            Opt::NumBots => (
                "ifs.mp.createopts.helptext.numbots",
                "Set the number of AI units to fight on each team for all modes.",
            ),
            Opt::AiDifficulty => (
                "ifs.mp.createopts.helptext.aidifficulty",
                "Determine how tough the AI units will be for all modes.",
            ),
            Opt::HeroesOnOff => (
                "ifs.mp.createopts.helptext.heroes_onoff",
                "Determine whether or not heroes will appear for all maps on your playlist.",
            ),
            Opt::HeroUnlock => (
                "ifs.mp.createopts.helptext.hero_unlock_1",
                "Set the condition that will cause the hero to become available to a player.",
            ),
            Opt::HeroUnlockVal => (
                "ifs.mp.createopts.helptext.hero_unlock_2",
                "Based on the above condition, specify the details that will make the hero available to a player.",
            ),
            Opt::HeroAssign => (
                "ifs.mp.createopts.helptext.hero_assign",
                "Set the condition that will determine which player gets to play as the available hero.",
            ),
            Opt::HeroRespawn => (
                "ifs.mp.createopts.helptext.hero_respawn_val",
                "Determine the amount of time that will elapse before the hero becomes available after being defeated.",
            ),
            Opt::ConNumBots => (
                "ifs.mp.createopts.helptext.con_numbots",
                "Set the number of AI units per team for all Conquest matches in your session.",
            ),
            Opt::ConMult => (
                "ifs.mp.createopts.helptext.con_mult",
                "Adjust the percent of starting reinforcements per team for all Conquest maps on your playlist.",
            ),
            Opt::ConTimer => (
                "ifs.mp.createopts.helptext.con_timer",
                "Set the time limit for all Conquest matches in your session.",
            ),
            Opt::CtfNumBots => (
                "ifs.mp.createopts.helptext.ctf_numbots",
                "Set the number of AI units per team for all 1 and 2-flag CTF matches in your session.",
            ),
            Opt::CtfScore => (
                "ifs.mp.createopts.helptext.ctf_score",
                "Set the number of flags a team must capture to win for all Capture the Flag maps on your playlist.",
            ),
            Opt::CtfTimer => (
                "ifs.mp.createopts.helptext.ctf_timer",
                "Set the time limit for all 1 and 2-flag CTF matches in your session.",
            ),
            Opt::HunTimer => (
                "ifs.mp.createopts.helptext.hun_timer",
                "Set the time limit for all Hunt matches in your session.",
            ),
            Opt::HunScore => (
                "ifs.mp.createopts.helptext.hun_score",
                "Set the score limit for all the Hunt matches in your session.",
            ),
            Opt::AssNumBots => (
                "ifs.mp.createopts.helptext.ass_numbots",
                "Set the number of AI units per team for all space Assault matches in your session.",
            ),
            Opt::AssScore => (
                "ifs.mp.createopts.helptext.ass_score",
                "Set the score limit for all space Assault matches in your session.",
            ),
        }
    }
}

/// The session's option values (§2.3's fields and defaults).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionOptions {
    pub num_bots: i32,
    /// The AI tier, 1 Easy, 2 Normal, 3 Hard, 4 Elite (the profile's;
    /// [`DIFFICULTY_LEVELS`]). A named deviation: the PS2's `iDifficulty` is
    /// 1 Easy, 2 Normal, 3 Elite with only 2 <-> 3 reachable (the maintainer,
    /// 2026-10-28: the port's four tiers replace the PS2's three).
    pub difficulty: i32,
    pub heroes: bool,
    /// 3 Points or 5 Timer.
    pub hero_unlock: i32,
    pub hero_unlock_val: i32,
    /// 1 Best, 4 Worst, 7 Random.
    pub hero_assign: i32,
    /// 0 Always, 1..120 s, -1 Never.
    pub hero_respawn: i32,
    pub con_numbots: i32,
    pub con_mult: i32,
    pub con_timer: i32,
    pub ctf_numbots: i32,
    pub ctf_score: i32,
    pub ctf_timer: i32,
    pub hun_timer: i32,
    pub hun_score: i32,
    pub ass_numbots: i32,
    pub ass_score: i32,
}

impl Default for SessionOptions {
    /// The PS2's values (§2.3, menuref "Session Options" table [M]): 16 AI,
    /// Normal (ours: 2 = Normal, the port's default tier), 100 %, timers Off,
    /// CTF 5, Hunt 5 min / 50, Assault 180; Heroes Off, Unlock Points, first
    /// to 10 points, Best Player, Timer 90 s (`so_hero`,
    /// `sheets/hero_all_steps.png`).
    fn default() -> Self {
        SessionOptions {
            num_bots: 16,
            difficulty: 2,
            heroes: false,
            hero_unlock: 3,
            hero_unlock_val: 10,
            hero_assign: 1,
            hero_respawn: 90,
            con_numbots: 16,
            con_mult: 100,
            con_timer: 0,
            ctf_numbots: 16,
            ctf_score: 5,
            ctf_timer: 0,
            hun_timer: 5,
            hun_score: 50,
            ass_numbots: 16,
            ass_score: 180,
        }
    }
}

impl SessionOptions {
    /// The group's rows; Heroes Off hides the other hero rows (main.f8).
    pub fn rows(&self, g: Group) -> Vec<Opt> {
        match g {
            Group::Global => vec![Opt::NumBots, Opt::AiDifficulty],
            Group::Hero if !self.heroes => vec![Opt::HeroesOnOff],
            Group::Hero => vec![
                Opt::HeroesOnOff,
                Opt::HeroUnlock,
                Opt::HeroUnlockVal,
                Opt::HeroAssign,
                Opt::HeroRespawn,
            ],
            Group::Conquest => vec![Opt::ConNumBots, Opt::ConMult, Opt::ConTimer],
            Group::Ctf => vec![Opt::CtfNumBots, Opt::CtfScore, Opt::CtfTimer],
            Group::Hunt => vec![Opt::HunTimer, Opt::HunScore],
            Group::Assault => vec![Opt::AssNumBots, Opt::AssScore],
        }
    }

    /// One press on `o`: `dir` -1 / +1 (Left / Right), `mult` 1 or 10 (L1 /
    /// R1); `Clamp`, no wrap (main.f5, f6). The ranges are the PS2's
    /// (menuref "Session Options" table, `so_*` clips).
    pub fn adjust(&mut self, o: Opt, dir: i32, mult: i32) {
        let d = dir * mult;
        let clamp = |v: i32, lo: i32, hi: i32| v.clamp(lo, hi);
        match o {
            Opt::NumBots => {
                self.num_bots = clamp(self.num_bots + d, 0, MAX_BOTS);
                // A change copies into every mode's count (pc 12-29; menuref:
                // Global 10 -> Conquest, CTF and Assault read 10).
                self.con_numbots = self.num_bots;
                self.ctf_numbots = self.num_bots;
                self.ass_numbots = self.num_bots;
            }
            // `[ours]` the four tiers, a step per press, clamped like the
            // other rows (L1/R1 go to the ends). The PS2's row toggles
            // `5 - x`, Normal <-> Elite (a named deviation, the maintainer 2026-10-28).
            Opt::AiDifficulty => self.difficulty = clamp(self.difficulty + d, 1, DIFFICULTY_LEVELS),
            Opt::HeroesOnOff => self.heroes = !self.heroes,
            Opt::HeroUnlock => self.hero_unlock = if self.hero_unlock == 3 { 5 } else { 3 },
            Opt::HeroUnlockVal => self.hero_unlock_val = clamp(self.hero_unlock_val + d, 1, 120),
            // Best -> Worst -> Random, no wrap (menuref `so_hero_ranges`).
            Opt::HeroAssign => self.hero_assign = clamp(self.hero_assign + 3 * dir, 1, 7),
            // Always (0) .. 120, Right at 120 -> -1 Never, Right at Never
            // stays, Left from Never -> 120, Left at Always stays (menuref
            // `so_hero_ranges`).
            Opt::HeroRespawn => {
                self.hero_respawn = match (self.hero_respawn, dir > 0) {
                    (-1, true) => -1,
                    (-1, false) => 120,
                    (v, true) if v + d > 120 => -1,
                    (v, _) => (v + d).max(0),
                }
            }
            Opt::ConNumBots => self.con_numbots = clamp(self.con_numbots + d, 0, MAX_BOTS),
            Opt::ConMult => self.con_mult = clamp(self.con_mult + 10 * d, 10, 500),
            Opt::ConTimer => self.con_timer = clamp(self.con_timer + 5 * d, 0, 60),
            Opt::CtfNumBots => self.ctf_numbots = clamp(self.ctf_numbots + d, 0, MAX_BOTS),
            Opt::CtfScore => self.ctf_score = clamp(self.ctf_score + d, 1, 15),
            Opt::CtfTimer => self.ctf_timer = clamp(self.ctf_timer + 5 * d, 0, 60),
            Opt::HunTimer => self.hun_timer = clamp(self.hun_timer + 5 * d, 5, 60),
            Opt::HunScore => self.hun_score = clamp(self.hun_score + 10 * d, 10, 1000),
            Opt::AssNumBots => self.ass_numbots = clamp(self.ass_numbots + d, 0, MAX_BOTS),
            Opt::AssScore => self.ass_score = clamp(self.ass_score + 10 * d, 50, 1000),
        }
    }
}

/// A piece of an option row: text, or the `{OptionL}` / `{OptionR}` arrows
/// (drawn as COMMON's `optionl` / `optionr`, §2.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Left,
    Right,
}

/// `template`'s `%s` filled left to right, `%%` as `%`, cut at the arrow
/// tokens.
fn pieces(template: &str, args: &[&str]) -> Vec<Piece> {
    let mut filled = String::new();
    let mut args = args.iter();
    let mut rest = template;
    while let Some(at) = rest.find('%') {
        filled.push_str(&rest[..at]);
        let tail = &rest[at..];
        if let Some(after) = tail.strip_prefix("%s") {
            filled.push_str(args.next().copied().unwrap_or(""));
            rest = after;
        } else if let Some(after) = tail.strip_prefix("%%") {
            filled.push('%');
            rest = after;
        } else {
            filled.push('%');
            rest = &tail[1..];
        }
    }
    filled.push_str(rest);
    let mut out = Vec::new();
    let mut rest = filled.as_str();
    loop {
        let l = rest.find("{OptionL}");
        let r = rest.find("{OptionR}");
        let next = match (l, r) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let Some(at) = next else {
            if !rest.is_empty() {
                out.push(Piece::Text(rest.to_owned()));
            }
            return out;
        };
        if at > 0 {
            out.push(Piece::Text(rest[..at].to_owned()));
        }
        out.push(if Some(at) == l {
            Piece::Left
        } else {
            Piece::Right
        });
        rest = &rest[at + 9..];
    }
}

/// The page up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Missions,
    /// "Session Options": the focused button.
    Overview(usize),
    Group(Group, ListCursor),
}

/// A frame's presses: the menu edges plus L1 / R1.
#[derive(Debug, Clone, Copy, Default)]
pub struct IaInput {
    pub menu: MenuInput,
    pub l1: bool,
    pub r1: bool,
}

/// What a press asks the game to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IaCommand {
    /// Triangle in the command state: back to Singleplayer.
    Back,
    /// LAUNCH: the playlist, in order (`bRandomOrder` is MP-only, §1.4).
    Launch(Vec<Launch>),
    /// An options page was left (`push_prefs`): store what must outlive
    /// the session (the AI difficulty is the profile's).
    Prefs,
}

/// Every text the screens show, looked up once.
#[derive(Debug, Clone, PartialEq)]
pub struct IaTexts {
    pub planet: String,
    pub mode: String,
    pub era: String,
    pub playlist: String,
    pub remove_all: String,
    pub remove_all_desc: String,
    pub all_maps: String,
    pub all_maps_desc: String,
    pub all_modes: String,
    pub all_modes_desc: String,
    pub all_eras: String,
    pub list_full: String,
    pub back: String,
    pub select: String,
    pub session_options: String,
    commands: Vec<(String, String)>,
    modes: Vec<(String, String)>,
    eras: Vec<String>,
    groups: Vec<(String, String)>,
    /// `common.sides.<f>.name` for all, imp, rep, cis.
    sides: Vec<(&'static str, String)>,
    helps: Vec<(Opt, String)>,
    t: Vec<(&'static str, String)>,
}

/// The option rows' text keys and English.
const OPTION_TEXTS: [(&str, &str); 18] = [
    (
        "ifs.mp.createopts.numbots",
        "AI Units Per Team: {OptionL} %s {OptionR}",
    ),
    (
        "ifs.mp.createopts.difficulty",
        "AI Difficulty Level: {OptionL} %s {OptionR}",
    ),
    ("ifs.mp.createopts.heroes", "Heroes: {OptionL} %s {OptionR}"),
    (
        "ifs.mp.heroopts.herounlock3",
        "Unlock Hero: {OptionL} Points {OptionR}",
    ),
    (
        "ifs.mp.heroopts.herounlock5",
        "Unlock Hero: {OptionL} Timer {OptionR}",
    ),
    (
        "ifs.mp.heroopts.numpoints",
        "First to {OptionL} %s {OptionR} points",
    ),
    (
        "ifs.mp.heroopts.numseconds",
        "{OptionL} %s {OptionR} seconds",
    ),
    (
        "ifs.mp.heroopts.heroplayer1",
        "Assign To: {OptionL} Best Player {OptionR}",
    ),
    (
        "ifs.mp.heroopts.heroplayer4",
        "Assign To: {OptionL} Worst Player {OptionR}",
    ),
    (
        "ifs.mp.heroopts.heroplayer7",
        "Assign To: {OptionL} Random Player {OptionR}",
    ),
    (
        "ifs.mp.heroopts.respawntimer",
        "Timer: {OptionL} %s {OptionR}%s",
    ),
    (
        "ifs.mp.create.opt.reinforcements",
        "Reinforcements: {OptionL} %s%% {OptionR}",
    ),
    ("ifs.mp.create.opt.timer", "Timer: {OptionL} %s {OptionR}"),
    ("ifs.mp.create.opt.ctf", "CTF Score: {OptionL} %s {OptionR}"),
    (
        "ifs.mp.create.opt.scorelimit",
        "Score Limit: {OptionL} %s {OptionR}",
    ),
    ("ifs.mp.create.opt.off", "Off"),
    ("ifs.mp.heroopts.seconds", " seconds"),
    ("ifs.mp.heroopts.second", " second"),
];

/// Short texts: (key, English).
const SMALL_TEXTS: [(&str, &str); 8] = [
    ("common.on", "On"),
    ("common.off", "Off"),
    ("common.always", "Always"),
    ("common.never", "Never"),
    ("ifs.difficulty.easy", "Easy"),
    ("ifs.difficulty.medium", "Normal"),
    ("ifs.difficulty.hard", "Elite"),
    ("common.mp.options", "Options"),
];

/// The AI Difficulty row's levels, 1..=this: Easy, Normal, Hard, Elite
/// ([`SessionOptions::difficulty`]).
pub const DIFFICULTY_LEVELS: i32 = 4;

impl IaTexts {
    pub fn new(text: &dyn Fn(&str, &str) -> String) -> IaTexts {
        let t = |k: (&str, &str)| text(k.0, k.1);
        IaTexts {
            planet: t(("ifs.missionselect.selectplanet", "Planet")),
            mode: t(("ifs.missionselect.selectmode", "Mode")),
            era: t(("ifs.missionselect.selectera", "Era")),
            playlist: t(("ifs.missionselect.playlist", "Playlist")),
            remove_all: t(("mapname.name._remove_all_", "Remove All")),
            remove_all_desc: t((
                "mapname.description._remove_all_",
                "Remove all maps from the playlist.",
            )),
            all_maps: t(("mapname.name._all_maps_", "All Maps")),
            all_maps_desc: t((
                "mapname.description._all_maps_",
                "Add all maps to the playlist.",
            )),
            all_modes: t(("modename.name._all_modes_", "All Modes")),
            all_modes_desc: t((
                "modename.description._all_modes_",
                "Add all available game modes for this map.",
            )),
            all_eras: t(("common.era._all_eras_", "All Eras")),
            list_full: t(("ifs.missionselect.listfull", "Selected maplist is full.")),
            back: t(("common.back", "Back")),
            select: t(("common.select", "Select")),
            session_options: t(("ifs.instantoptions.title", "Session Options")),
            commands: COMMANDS
                .iter()
                .map(|c| {
                    let (a, b) = c.text();
                    (t(a), t(b))
                })
                .collect(),
            modes: MODES
                .iter()
                .map(|m| {
                    let (a, b) = m.text();
                    (t(a), t(b))
                })
                .collect(),
            eras: ERAS.iter().map(|e| t(e.text())).collect(),
            groups: GROUPS
                .iter()
                .map(|g| {
                    let (a, b) = g.text();
                    (t(a), t(b))
                })
                .collect(),
            sides: [
                ("all", "Rebels"),
                ("imp", "Empire"),
                ("rep", "Republic"),
                ("cis", "CIS"),
            ]
            .map(|(f, e)| (f, text(&format!("common.sides.{f}.name"), e)))
            .to_vec(),
            helps: [
                Opt::NumBots,
                Opt::AiDifficulty,
                Opt::HeroesOnOff,
                Opt::HeroUnlock,
                Opt::HeroUnlockVal,
                Opt::HeroAssign,
                Opt::HeroRespawn,
                Opt::ConNumBots,
                Opt::ConMult,
                Opt::ConTimer,
                Opt::CtfNumBots,
                Opt::CtfScore,
                Opt::CtfTimer,
                Opt::HunTimer,
                Opt::HunScore,
                Opt::AssNumBots,
                Opt::AssScore,
            ]
            .map(|o| (o, t(o.help())))
            .to_vec(),
            t: OPTION_TEXTS
                .iter()
                .chain(SMALL_TEXTS.iter())
                .map(|&(k, e)| (k, text(k, e)))
                .collect(),
        }
    }

    fn get(&self, key: &str) -> &str {
        self.t
            .iter()
            .find(|(k, _)| *k == key)
            .map_or("", |(_, v)| v.as_str())
    }

    pub fn command(&self, c: Cmd) -> (&str, &str) {
        let i = COMMANDS.iter().position(|x| *x == c).unwrap_or(0);
        let (a, b) = &self.commands[i];
        (a, b)
    }

    pub fn mode(&self, m: Mode) -> (&str, &str) {
        let i = MODES.iter().position(|x| *x == m).unwrap_or(0);
        let (a, b) = &self.modes[i];
        (a, b)
    }

    pub fn era(&self, e: Era) -> &str {
        &self.eras[e.index()]
    }

    pub fn group(&self, g: Group) -> (&str, &str) {
        let i = GROUPS.iter().position(|x| *x == g).unwrap_or(0);
        let (a, b) = &self.groups[i];
        (a, b)
    }

    fn side_name<'a>(&'a self, f: &'a str) -> &'a str {
        self.sides
            .iter()
            .find(|(k, _)| *k == f)
            .map_or(f, |(_, v)| v.as_str())
    }

    /// An Era row's text; `[ours]` a cross-era pair reads "<A> vs <B>".
    pub fn side(&self, s: Side) -> String {
        match s {
            Side::Era(e) => self.era(e).to_owned(),
            Side::Mixed(i) => format!(
                "{} vs {}",
                self.side_name(MIXED[i][0]),
                self.side_name(MIXED[i][1])
            ),
        }
    }

    pub fn help(&self, o: Opt) -> &str {
        self.helps
            .iter()
            .find(|(k, _)| *k == o)
            .map_or("", |(_, v)| v.as_str())
    }

    /// An option row as drawn: "Label: < value >" (main.f2).
    pub fn option(&self, o: Opt, v: &SessionOptions) -> Vec<Piece> {
        let n = |x: i32| x.to_string();
        let off = |x: i32| {
            if x == 0 {
                self.get("ifs.mp.create.opt.off").to_owned()
            } else {
                n(x)
            }
        };
        let num = self.get("ifs.mp.createopts.numbots");
        match o {
            Opt::NumBots => pieces(num, &[&n(v.num_bots)]),
            Opt::ConNumBots => pieces(num, &[&n(v.con_numbots)]),
            Opt::CtfNumBots => pieces(num, &[&n(v.ctf_numbots)]),
            Opt::AssNumBots => pieces(num, &[&n(v.ass_numbots)]),
            Opt::AiDifficulty => {
                // BF2's words where it has them (`ifs.difficulty.easy`
                // "Easy", `.medium` "Normal", `.hard` "Elite"); `[ours]` Hard,
                // English (BF2 has no "Hard": its Hard level shows "Elite").
                let name = match v.difficulty {
                    1 => self.get("ifs.difficulty.easy"),
                    2 => self.get("ifs.difficulty.medium"),
                    3 => "Hard",
                    _ => self.get("ifs.difficulty.hard"),
                };
                pieces(self.get("ifs.mp.createopts.difficulty"), &[name])
            }
            Opt::HeroesOnOff => pieces(
                self.get("ifs.mp.createopts.heroes"),
                &[self.get(if v.heroes { "common.on" } else { "common.off" })],
            ),
            Opt::HeroUnlock => pieces(
                self.get(if v.hero_unlock == 5 {
                    "ifs.mp.heroopts.herounlock5"
                } else {
                    "ifs.mp.heroopts.herounlock3"
                }),
                &[],
            ),
            Opt::HeroUnlockVal => pieces(
                self.get(if v.hero_unlock == 5 {
                    "ifs.mp.heroopts.numseconds"
                } else {
                    "ifs.mp.heroopts.numpoints"
                }),
                &[&n(v.hero_unlock_val)],
            ),
            Opt::HeroAssign => pieces(
                self.get(match v.hero_assign {
                    4 => "ifs.mp.heroopts.heroplayer4",
                    7 => "ifs.mp.heroopts.heroplayer7",
                    _ => "ifs.mp.heroopts.heroplayer1",
                }),
                &[],
            ),
            Opt::HeroRespawn => {
                let (value, unit) = match v.hero_respawn {
                    0 => (self.get("common.always").to_owned(), ""),
                    -1 => (self.get("common.never").to_owned(), ""),
                    1 => ("1".to_owned(), self.get("ifs.mp.heroopts.second")),
                    s => (n(s), self.get("ifs.mp.heroopts.seconds")),
                };
                pieces(self.get("ifs.mp.heroopts.respawntimer"), &[&value, unit])
            }
            Opt::ConMult => pieces(
                self.get("ifs.mp.create.opt.reinforcements"),
                &[&n(v.con_mult)],
            ),
            Opt::ConTimer => pieces(self.get("ifs.mp.create.opt.timer"), &[&off(v.con_timer)]),
            Opt::CtfTimer => pieces(self.get("ifs.mp.create.opt.timer"), &[&off(v.ctf_timer)]),
            Opt::HunTimer => pieces(self.get("ifs.mp.create.opt.timer"), &[&n(v.hun_timer)]),
            Opt::CtfScore => pieces(self.get("ifs.mp.create.opt.ctf"), &[&n(v.ctf_score)]),
            Opt::HunScore => pieces(self.get("ifs.mp.create.opt.scorelimit"), &[&n(v.hun_score)]),
            Opt::AssScore => pieces(self.get("ifs.mp.create.opt.scorelimit"), &[&n(v.ass_score)]),
        }
    }
}

/// The Instant Action screen and its Session Options.
#[derive(Debug, Clone, PartialEq)]
pub struct InstantAction {
    pub maps: Vec<MapEntry>,
    pub texts: IaTexts,
    pub state: State,
    /// The command stack's focus (kept while a list has it, f11).
    pub command: usize,
    pub map_list: ListCursor,
    pub mode_list: ListCursor,
    pub era_list: ListCursor,
    pub play_list: ListCursor,
    pub picked_map: MapPick,
    pub picked_mode: ModePick,
    pub playlist: Vec<Entry>,
    pub options: SessionOptions,
    pub page: Page,
    /// "Selected maplist is full." is up (`Popup_Ok`, §1.4).
    pub full: bool,
    /// Seconds on the current page (the drawing's clock).
    pub on_page: f32,
    /// The clicks the presses asked for, for the game to play
    /// ([`InstantAction::take_clicks`]).
    clicks: Vec<MenuSound>,
}

impl InstantAction {
    /// The screen's first Enter (§1.3): an empty playlist, the command
    /// state, focus on the first live button (ADD).
    pub fn new(maps: Vec<MapEntry>, texts: IaTexts, options: SessionOptions) -> InstantAction {
        let mut ia = InstantAction {
            maps,
            texts,
            state: State::Command,
            command: 0,
            map_list: ListCursor::default(),
            mode_list: ListCursor::default(),
            era_list: ListCursor::default(),
            play_list: ListCursor::default(),
            picked_map: MapPick::All,
            picked_mode: ModePick::All,
            playlist: Vec::new(),
            options,
            page: Page::Missions,
            full: false,
            on_page: 0.0,
            clicks: Vec::new(),
        };
        ia.enter();
        ia
    }

    /// Every later Enter: the command state, the playlist and the session
    /// options kept (f24; menuref §9.1.5).
    pub fn enter(&mut self) {
        self.state = State::Command;
        self.page = Page::Missions;
        self.full = false;
        self.on_page = 0.0;
        self.command = self.first_command();
        let live = self.live(&self.map_rows());
        self.map_list = ListCursor::first(&live, LIST_ROWS);
    }

    // ---- the lists -------------------------------------------------------

    fn live(&self, rows: &[ListRow]) -> Vec<bool> {
        rows.iter().map(|r| r.enabled).collect()
    }

    /// Whether `map` with `mode` and `side` would add anything.
    fn adds(&self, map: MapPick, mode: ModePick, side: SidePick) -> bool {
        !self.expand(map, mode, side).is_empty()
    }

    /// The Planet list: every map, then "All Maps".
    pub fn map_picks(&self) -> Vec<MapPick> {
        (0..self.maps.len())
            .map(MapPick::One)
            .chain(std::iter::once(MapPick::All))
            .collect()
    }

    pub fn map_rows(&self) -> Vec<ListRow> {
        self.map_picks()
            .into_iter()
            .map(|p| ListRow {
                text: match p {
                    MapPick::One(i) => self.maps[i].label.clone(),
                    MapPick::All => self.texts.all_maps.clone(),
                },
                enabled: self.adds(p, ModePick::All, SidePick::AllEras)
                    || self.mixed_adds(p, ModePick::All),
                icon: None,
                icon2: None,
            })
            .collect()
    }

    fn mixed_adds(&self, map: MapPick, mode: ModePick) -> bool {
        (0..MIXED.len()).any(|i| self.adds(map, mode, SidePick::One(Side::Mixed(i))))
    }

    /// The modes `map` has, in `gMapModes` order (All Maps: any map's).
    fn modes_of(&self, map: MapPick) -> Vec<Mode> {
        match map {
            MapPick::One(i) => self.maps[i].all_modes(),
            MapPick::All => MODES
                .into_iter()
                .filter(|m| self.maps.iter().any(|e| e.all_modes().contains(m)))
                .collect(),
        }
    }

    /// The Mode list: the map's modes, then "All Modes" when it has more
    /// than one (f2 pc 123-167).
    pub fn mode_picks(&self) -> Vec<ModePick> {
        let modes = self.modes_of(self.picked_map);
        let many = modes.len() > 1;
        modes
            .into_iter()
            .map(ModePick::One)
            .chain(many.then_some(ModePick::All))
            .collect()
    }

    pub fn mode_rows(&self) -> Vec<ListRow> {
        let picks = self.mode_picks();
        let live_modes = picks
            .iter()
            .filter(|p| matches!(p, ModePick::One(_)) && self.mode_live(**p))
            .count();
        picks
            .iter()
            .map(|&p| match p {
                ModePick::One(m) => ListRow {
                    text: self.texts.mode(m).0.to_owned(),
                    enabled: self.mode_live(p),
                    icon: Some(m.icon()),
                    icon2: None,
                },
                // §7: dimmed when only one mode is live.
                ModePick::All => ListRow {
                    text: self.texts.all_modes.clone(),
                    enabled: live_modes > 1,
                    icon: None,
                    icon2: None,
                },
            })
            .collect()
    }

    fn mode_live(&self, mode: ModePick) -> bool {
        self.adds(self.picked_map, mode, SidePick::AllEras)
            || self.mixed_adds(self.picked_map, mode)
    }

    /// The Era list: the eras that have the picked mode (any mode after All
    /// Modes) and "All Eras" when more than one (f3 pc 87-143). The `[ours]`
    /// cross-era pairs ([`MIXED`]) are not listed for now (owner's call,
    /// 2026-10-28: Clone Wars and Galactic Civil War only).
    pub fn side_picks(&self) -> Vec<SidePick> {
        let has = |e: Era| {
            let maps: Vec<&MapEntry> = match self.picked_map {
                MapPick::One(i) => vec![&self.maps[i]],
                MapPick::All => self.maps.iter().collect(),
            };
            maps.iter().any(|m| match self.picked_mode {
                ModePick::One(mode) => m.modes[e.index()].contains(&mode),
                ModePick::All => m.has_era(e),
            })
        };
        let eras: Vec<Era> = ERAS.into_iter().filter(|e| has(*e)).collect();
        let many = eras.len() > 1;
        eras.into_iter()
            .map(|e| SidePick::One(Side::Era(e)))
            .chain(many.then_some(SidePick::AllEras))
            .collect()
    }

    pub fn side_rows(&self) -> Vec<ListRow> {
        let picks = self.side_picks();
        let live = |p: SidePick| self.adds(self.picked_map, self.picked_mode, p);
        let live_eras = picks
            .iter()
            .filter(|p| matches!(p, SidePick::One(Side::Era(_))) && live(**p))
            .count();
        picks
            .iter()
            .map(|&p| match p {
                SidePick::One(s) => ListRow {
                    text: self.texts.side(s),
                    enabled: live(p),
                    icon: match s {
                        Side::Era(e) => Some(e.icon()),
                        Side::Mixed(_) => None,
                    },
                    icon2: None,
                },
                // `[ours]` like All Modes: dimmed with one live era.
                SidePick::AllEras => ListRow {
                    text: self.texts.all_eras.clone(),
                    enabled: live_eras > 1,
                    icon: None,
                    icon2: None,
                },
            })
            .collect()
    }

    /// The Playlist: "Remove All", then the rows (map name, mode icon, era
    /// icon left of it; f3).
    pub fn play_rows(&self) -> Vec<ListRow> {
        std::iter::once(ListRow {
            text: self.texts.remove_all.clone(),
            enabled: true,
            icon: None,
            icon2: None,
        })
        .chain(self.playlist.iter().map(|e| ListRow {
            text: self.maps[e.map].label.clone(),
            enabled: true,
            icon: Some(e.mode.icon()),
            icon2: match e.side {
                Side::Era(era) => Some(era.icon()),
                Side::Mixed(_) => None,
            },
        }))
        .collect()
    }

    /// The rows `fnAddMap1..4` would add (§1.4): maps in list order, eras
    /// outer, modes inner; `[ours]` only what the port can run.
    pub fn expand(&self, map: MapPick, mode: ModePick, side: SidePick) -> Vec<Entry> {
        let maps: Vec<usize> = match map {
            MapPick::One(i) => vec![i],
            MapPick::All => (0..self.maps.len()).collect(),
        };
        let sides: Vec<Side> = match side {
            SidePick::One(s) => vec![s],
            SidePick::AllEras => ERAS.into_iter().map(Side::Era).collect(),
        };
        let modes: Vec<Mode> = match mode {
            ModePick::One(m) => vec![m],
            ModePick::All => MODES.to_vec(),
        };
        let mut out = Vec::new();
        for &i in &maps {
            let m = &self.maps[i];
            for &s in &sides {
                if let Side::Era(e) = s {
                    if !m.has_era(e) {
                        continue;
                    }
                }
                for &mode in &modes {
                    if m.runnable(mode, s) {
                        out.push(Entry {
                            map: i,
                            mode,
                            side: s,
                        });
                    }
                }
            }
        }
        out
    }

    // ---- the command stack ----------------------------------------------

    /// f11: LAUNCH is live with a map in the playlist; in a list state all
    /// four are dimmed.
    pub fn command_live(&self, c: Cmd) -> bool {
        self.state == State::Command && (c != Cmd::Launch || !self.playlist.is_empty())
    }

    fn first_command(&self) -> usize {
        let launch = !self.playlist.is_empty();
        if launch {
            0
        } else {
            1
        }
    }

    fn focus_command(&mut self, c: Cmd) {
        self.command = COMMANDS.iter().position(|x| *x == c).unwrap_or(1);
    }

    /// The info box's text now (f9).
    pub fn info(&self) -> String {
        let at = |c: &ListCursor| c.at;
        match self.state {
            State::Command => self.texts.command(COMMANDS[self.command]).1.to_owned(),
            State::Map => match self.map_picks().get(at(&self.map_list)) {
                Some(MapPick::One(i)) => self.maps[*i].description.clone(),
                _ => self.texts.all_maps_desc.clone(),
            },
            State::Mode => match self.mode_picks().get(at(&self.mode_list)) {
                Some(ModePick::One(m)) => self.texts.mode(*m).1.to_owned(),
                _ => self.texts.all_modes_desc.clone(),
            },
            State::Era => match self.side_picks().get(at(&self.era_list)) {
                Some(SidePick::One(s)) => self.texts.side(*s),
                _ => self.texts.all_eras.clone(),
            },
            State::Playlist => match at(&self.play_list) {
                0 => String::new(),
                i => self
                    .playlist
                    .get(i - 1)
                    .map_or_else(String::new, |e| self.maps[e.map].label.clone()),
            },
        }
    }

    /// The overview's buttons and whether each is live.
    pub fn overview(&self) -> Vec<(Group, bool)> {
        GROUPS.iter().map(|g| (*g, g.honoured())).collect()
    }

    fn go(&mut self, page: Page) {
        self.page = page;
        self.on_page = 0.0;
    }

    fn click(&mut self, s: MenuSound) {
        self.clicks.push(s);
    }

    /// The clicks asked for since the last call, oldest first. The table is
    /// menuref's "Sound table": a stack move clicks select (the wrap too); a
    /// list move only when the cursor moves; a Session Options value step
    /// select always, even at a clamp; Cross accept, on "Remove All" error;
    /// Triangle one exit click (the command state's is the Singleplayer
    /// stack's re-Enter, not here); Cross on a group page nothing.
    pub fn take_clicks(&mut self) -> Vec<MenuSound> {
        std::mem::take(&mut self.clicks)
    }

    /// One frame: `dt` seconds and the presses.
    pub fn step(&mut self, input: IaInput, dt: f32) -> Option<IaCommand> {
        self.on_page += dt;
        let i = input.menu;
        match self.page {
            Page::Missions => {}
            Page::Overview(focus) => {
                let live: Vec<bool> = self.overview().iter().map(|(_, l)| *l).collect();
                let wrap = |step: isize| {
                    let n = live.len() as isize;
                    let mut k = focus as isize;
                    for _ in 0..n {
                        k = (k + step).rem_euclid(n);
                        if live[k as usize] {
                            return k as usize;
                        }
                    }
                    focus
                };
                if i.back {
                    self.click(MenuSound::Exit);
                    self.go(Page::Missions);
                } else if i.confirm && live[focus] {
                    self.click(MenuSound::Accept);
                    let g = GROUPS[focus];
                    let rows = vec![true; self.options.rows(g).len()];
                    self.go(Page::Group(g, ListCursor::first(&rows, OPTION_ROWS)));
                } else if i.up {
                    self.click(MenuSound::Select);
                    self.page = Page::Overview(wrap(-1));
                } else if i.down {
                    self.click(MenuSound::Select);
                    self.page = Page::Overview(wrap(1));
                }
                return None;
            }
            Page::Group(g, mut cur) => {
                if i.back {
                    self.click(MenuSound::Exit);
                    let at = GROUPS.iter().position(|x| *x == g).unwrap_or(0);
                    self.go(Page::Overview(at));
                    return Some(IaCommand::Prefs);
                }
                let rows = self.options.rows(g);
                let live = vec![true; rows.len()];
                let mut moved = false;
                if i.up {
                    moved |= cur.step(&live, -1, OPTION_ROWS);
                }
                if i.down {
                    moved |= cur.step(&live, 1, OPTION_ROWS);
                }
                if moved {
                    self.click(MenuSound::Select);
                }
                let (dir, mult) = match (i.left, i.right, input.l1, input.r1) {
                    (true, ..) => (-1, 1),
                    (_, true, ..) => (1, 1),
                    (_, _, true, _) => (-1, 10),
                    (_, _, _, true) => (1, 10),
                    _ => (0, 0),
                };
                if let (true, Some(o)) = (dir != 0, rows.get(cur.at)) {
                    // Select on every step, also at a clamp (menuref).
                    self.click(MenuSound::Select);
                    self.options.adjust(*o, dir, mult);
                    // The hero rows come and go with Heroes On / Off.
                    let n = self.options.rows(g).len();
                    cur.at = cur.at.min(n - 1);
                    cur.show(n, OPTION_ROWS);
                }
                self.page = Page::Group(g, cur);
                return None;
            }
        }
        if self.full {
            // "Selected maplist is full." closes with accept, Triangle too
            // (menuref `ia_listfull`).
            if i.confirm || i.back {
                self.click(MenuSound::Accept);
                self.full = false;
            }
            return None;
        }
        // A list press: Up / Down one row, L1 / R1 a page; whether the
        // cursor moved (a click only then, menuref).
        let moves = |c: &mut ListCursor, live: &[bool], shown: usize| {
            let mut moved = false;
            if i.up {
                moved |= c.step(live, -1, shown);
            }
            if i.down {
                moved |= c.step(live, 1, shown);
            }
            if input.l1 || input.r1 {
                let page = if shown == LIST_ROWS { LIST_PAGE } else { shown } as isize;
                moved |= c.step(live, if input.l1 { -page } else { page }, shown);
            }
            moved
        };
        match self.state {
            State::Command => {
                if i.back {
                    return Some(IaCommand::Back);
                }
                let live = COMMANDS.map(|c| self.command_live(c));
                let wrap = |from: usize, step: isize| {
                    let mut k = from as isize;
                    for _ in 0..COMMANDS.len() {
                        k = (k + step).rem_euclid(COMMANDS.len() as isize);
                        if live[k as usize] {
                            return k as usize;
                        }
                    }
                    from
                };
                if i.up {
                    self.command = wrap(self.command, -1);
                }
                if i.down {
                    self.command = wrap(self.command, 1);
                }
                if i.up || i.down {
                    self.click(MenuSound::Select);
                }
                if i.confirm && self.command_live(COMMANDS[self.command]) {
                    self.click(MenuSound::Accept);
                    match COMMANDS[self.command] {
                        Cmd::Launch => return Some(IaCommand::Launch(self.launch())),
                        Cmd::Add => {
                            self.state = State::Map;
                            let live = self.live(&self.map_rows());
                            if !live.get(self.map_list.at).copied().unwrap_or(false) {
                                self.map_list = ListCursor::first(&live, LIST_ROWS);
                            }
                        }
                        // The cursor on the last row (menuref §1.3,
                        // `ia_remove`).
                        Cmd::Remove => {
                            self.state = State::Playlist;
                            let n = self.playlist.len() + 1;
                            self.play_list.at = n - 1;
                            self.play_list.show(n, LIST_ROWS);
                        }
                        Cmd::Options => self.go(Page::Overview(0)),
                    }
                }
            }
            State::Map => {
                let live = self.live(&self.map_rows());
                if moves(&mut self.map_list, &live, LIST_ROWS) {
                    self.click(MenuSound::Select);
                }
                if i.back {
                    self.click(MenuSound::Exit);
                    self.state = State::Command;
                } else if i.confirm && live.get(self.map_list.at) == Some(&true) {
                    self.click(MenuSound::Accept);
                    self.picked_map = self.map_picks()[self.map_list.at];
                    self.state = State::Mode;
                    let live = self.live(&self.mode_rows());
                    self.mode_list = ListCursor::first(&live, LIST_ROWS);
                }
            }
            State::Mode => {
                let live = self.live(&self.mode_rows());
                if moves(&mut self.mode_list, &live, LIST_ROWS) {
                    self.click(MenuSound::Select);
                }
                if i.back {
                    self.click(MenuSound::Exit);
                    self.state = State::Map;
                } else if i.confirm && live.get(self.mode_list.at) == Some(&true) {
                    self.click(MenuSound::Accept);
                    self.picked_mode = self.mode_picks()[self.mode_list.at];
                    self.state = State::Era;
                    let live = self.live(&self.side_rows());
                    self.era_list = ListCursor::first(&live, ERA_ROWS);
                }
            }
            State::Era => {
                let live = self.live(&self.side_rows());
                if moves(&mut self.era_list, &live, ERA_ROWS) {
                    self.click(MenuSound::Select);
                }
                if i.back {
                    self.click(MenuSound::Exit);
                    self.state = State::Mode;
                } else if i.confirm && live.get(self.era_list.at) == Some(&true) {
                    // The full popup opens silently over this accept.
                    self.click(MenuSound::Accept);
                    let side = self.side_picks()[self.era_list.at];
                    self.add(self.picked_map, self.picked_mode, side);
                    self.state = State::Command;
                    self.focus_command(Cmd::Launch);
                }
            }
            State::Playlist => {
                let live = vec![true; self.playlist.len() + 1];
                if moves(&mut self.play_list, &live, LIST_ROWS) {
                    self.click(MenuSound::Select);
                }
                if i.back {
                    self.click(MenuSound::Exit);
                    self.state = State::Command;
                } else if i.confirm {
                    match self.play_list.at {
                        // Cross on "Remove All": the error click, the list
                        // cleared (menuref `ia_remove`).
                        0 => {
                            self.click(MenuSound::Error);
                            self.playlist.clear();
                        }
                        k => {
                            self.click(MenuSound::Accept);
                            self.playlist.remove(k - 1);
                        }
                    }
                    let n = self.playlist.len() + 1;
                    self.play_list.at = self.play_list.at.min(n - 1);
                    self.play_list.show(n, LIST_ROWS);
                    self.state = State::Command;
                    self.focus_command(if self.playlist.is_empty() {
                        Cmd::Add
                    } else {
                        Cmd::Launch
                    });
                }
            }
        }
        None
    }

    /// `fnAddMap` (§1.4): the rows appended until the list holds
    /// [`MAX_MISSIONS`]; one more opens "Selected maplist is full.".
    pub fn add(&mut self, map: MapPick, mode: ModePick, side: SidePick) {
        for e in self.expand(map, mode, side) {
            if self.playlist.len() >= MAX_MISSIONS {
                self.full = true;
                break;
            }
            self.playlist.push(e);
            let n = self.playlist.len() + 1;
            self.play_list.at = n - 1;
            self.play_list.show(n, LIST_ROWS);
        }
    }

    /// The playlist as LAUNCH hands it over, in order.
    pub fn launch(&self) -> Vec<Launch> {
        self.playlist
            .iter()
            .map(|e| {
                let m = &self.maps[e.map];
                Launch {
                    key: m.key.clone(),
                    game: m.game,
                    mode: e.mode,
                    armies: e.side.armies().map(str::to_owned),
                    label: m.label.clone(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
