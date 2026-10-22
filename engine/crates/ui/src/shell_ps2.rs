//! The BF2 PS2 shell's front screens (card BSHLA, `analysis/re-bf2-shell.md`
//! §2-§4, §9): Press START, the Main Menu, Singleplayer and Options, each
//! the PS2's vertical-button stack over the background movie, painted by the
//! HUD pass (`crate::hud_draw`) in the shell's 512 × 448 screen.
//!
//! - **Title** (`ifs_start`): `logo` at the top, the `shell_main` movie and
//!   "Press START Button To Begin" pulsing; only Start advances (§2 row 3;
//!   the keyboard's Enter is the port's Start). The legal / logo screens and
//!   the profile flow are skipped (§9.1, §9.3).
//! - **Main Menu** (`ifs_main`): logo + stack 10 units lower than the
//!   others: Singleplayer, **Multiplayer in its slot but disabled** (the
//!   PS2's own dimmed style: label grey 110, focus skips it, §3.3 - the port
//!   has no online play), Splitscreen disabled (until card SSCR), Options.
//! - **Singleplayer** (`ifs_sp`, left-justified, `shell_sub_left`): Rise of
//!   the Empire and Galactic Conquest disabled (no port mode yet), Instant
//!   Action live; Training and Space Overview hidden (§9.5).
//! - **Options** (`ifs_opt_top`, left-justified, `shell_sub_left`): Game
//!   Options and Audio Options open the port's settings page, Controls is
//!   disabled until a remap page exists; Online Options, View Trailer,
//!   Credits and Career Stats hidden (§9.7).
//!
//! The stack's geometry is the frames' [M] rule (§3.2), not the Lua's
//! left-justify formula (which does not reproduce them, §3.2 [H]): labels
//! 1.087 × the font's advances across, the bar 1.087 × the widest label +
//! [`BAR_PAD`], centred stacks about x [`CENTRE_X`], left-justified ones from
//! x [`LEFT_X`]; rows `font height + 6` ([`crate::spawn_draw::ROW_H`]) every
//! `+ 4` ([`crate::spawn_draw::ROW_PITCH`]); the stack's middle
//! [`CENTRE_LIFT`] above its container's y (0.6 of the screen, + 10 on the
//! main menu). Focus moves wrap last <-> first and skip disabled rows (§3.2,
//! §3.3). Each row appears [`ROW_REVEAL`] s after the one above it
//! (`flashy`, §3.2).
//!
//! `[ours]` Hor+: the movie, the logo and centred stacks keep the 4:3 box at
//! the window's centre (the movie pillarboxed, never stretched: §9.9), a
//! left-justified stack keeps its offset from the box's left edge, the help
//! lines their offsets from the window's edges.

use crate::hud_draw::{run_width, Ctx};
use crate::hud_tree::HAlign;
use crate::menu::MenuInput;
use crate::spawn_draw::{
    BUTTON_FONT, LABEL_STRETCH, ROW_H, ROW_PITCH, SELECTED_TEXT, TITLE_TEXT, UNSELECTED_TEXT,
};
use manifest::hud::HudFont;

/// `gDefaultButtonScreenRelativeY` 0.6 of the 448-line screen (§3.1 [V]).
pub const CONTAINER_Y: f32 = 0.6 * 448.0;
/// The main menu's container sits 10 units lower (§3.2 [V]).
pub const MAIN_Y_ADD: f32 = 10.0;
/// The stack's middle stands this far above its container's y [M]: title
/// tops at 206 / 178 / 143.5 px of 480 on the main menu / Singleplayer /
/// Options frames (4, 5, 7 rows) - one offset fits all three to 0.2 units.
pub const CENTRE_LIFT: f32 = 8.7;
/// A centred stack's middle across [M] (main menu bar 143.75..501.25 px of
/// 640 = 115..401 units).
pub const CENTRE_X: f32 = 258.0;
/// A left-justified stack's bar left edge [M] (43 px on both the
/// Singleplayer and the Options frame: the safe area's 25.6 + 8.8).
pub const LEFT_X: f32 = 34.4;
/// The bar's outer width beyond 1.087 × its widest label's advances [M]
/// (§3.2: main menu, Singleplayer and Options agree within 0.9 units).
pub const BAR_PAD: f32 = 134.2;
/// A dimmed button's label grey (`ShowHideVerticalButtons` f7 pc 266-289
/// [V]: 110 instead of 255).
pub const DIMMED_GREY: f32 = 110.0;
/// Seconds between one row's appearance and the next's (`flashy`, §3.2 [V]).
pub const ROW_REVEAL: f32 = 0.1;
/// The logo: 512 × 256, centred, 38 units above the top of the safe area
/// (22.4, §3.5): §3.4 [V] says "above the top edge", and the frames put it
/// from the safe area's [M] (ink rows 36-147 px of 480 on
/// `title_pressstart.png`; from the screen's edge they land 24 px higher).
pub const LOGO: [f32; 4] = [0.0, 22.4 - 38.0, 512.0, 22.4 - 38.0 + 256.0];
/// The prompt's run top [M] (`title_pressstart.png`: its ink rows 427-441
/// px, a row's ink starts 4.5 units below its run's top), centred.
pub const PROMPT_TOP: f32 = 394.0;
/// The prompt's alpha swings between these at [`PROMPT_RATE`] a second
/// (`ifs_start` main.f2 pc 45-64 [V]).
pub const PROMPT_ALPHA: [f32; 2] = [0.4, 1.0];
pub const PROMPT_RATE: f32 = 0.5;
/// The texture name the movie's current frame is drawn from.
pub const MOVIE_SHEET: &str = "#shell_movie";

/// The movie properties (`manifest::shell`).
pub const MOVIE_MAIN: &str = "shell_main";
pub const MOVIE_SUB: &str = "shell_sub_left";
/// The music every front screen plays (`ifs_start` main.f2 sets
/// `shell_soundtrack`, §2 [V]).
pub const MUSIC: &str = "shell_soundtrack";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Title,
    Main,
    Singleplayer,
    Options,
}

/// Every button the port shows; the label is the disc's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowId {
    Singleplayer,
    Multiplayer,
    Splitscreen,
    Options,
    RiseOfTheEmpire,
    GalacticConquest,
    InstantAction,
    GameOptions,
    AudioOptions,
    Controls,
}

impl RowId {
    /// The disc's text key and its English fallback (`re-bf2-shell` §2,
    /// `bshl/keys_resolved.tsv`).
    pub fn key(self) -> (&'static str, &'static str) {
        match self {
            RowId::Singleplayer => ("ifs.main.sp", "Singleplayer"),
            RowId::Multiplayer => ("ifs.main.mp", "Multiplayer"),
            RowId::Splitscreen => ("ifs.main.split", "Splitscreen"),
            RowId::Options => ("ifs.main.options", "Options"),
            RowId::RiseOfTheEmpire => ("ifs.sp.riseempire", "Rise of the Empire"),
            RowId::GalacticConquest => ("ifs.sp.meta", "Galactic Conquest"),
            RowId::InstantAction => ("ifs.sp.ia", "Instant Action"),
            RowId::GameOptions => ("ifs.GameOpt.title", "Game Options"),
            RowId::AudioOptions => ("ifs.SoundOpt.title", "Audio Options"),
            RowId::Controls => ("ifs.controls.Vehicle_Unit", "Controls"),
        }
    }
}

/// The other texts the screens show: (key, English).
pub const TEXT_PROMPT: (&str, &str) = ("ifs.start.press_start", "Press START Button To Begin");
pub const TEXT_MAIN: (&str, &str) = ("ifs.main.title", "Main Menu");
pub const TEXT_BACK: (&str, &str) = ("common.back", "Back");
pub const TEXT_SELECT: (&str, &str) = ("common.select", "Select");

#[derive(Debug, Clone, PartialEq)]
pub struct StackRow {
    pub id: RowId,
    pub label: String,
    /// `false` = the PS2's `bDimmed`: drawn grey, never focused.
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stack {
    pub title: String,
    pub rows: Vec<StackRow>,
    pub focus: usize,
    pub left: bool,
    /// The container's extra y (the main menu's 10).
    pub y_add: f32,
}

impl Stack {
    /// The next enabled row from `focus` going `step` (+1 down, -1 up),
    /// wrapping (`FollowButtonLink`, §3.3); `focus` when none other is.
    pub fn follow(&self, step: isize) -> usize {
        let n = self.rows.len() as isize;
        let mut k = self.focus as isize;
        for _ in 0..n {
            k = (k + step).rem_euclid(n);
            if self.rows[k as usize].enabled {
                return k as usize;
            }
        }
        self.focus
    }

    /// The first enabled row (`TagOfFirst` skips dimmed ones).
    pub fn first(&self) -> usize {
        self.rows.iter().position(|r| r.enabled).unwrap_or(0)
    }
}

/// What a step asks the game to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellCommand {
    /// Singleplayer → Instant Action: today's IA setup (until card BSHLB).
    InstantAction,
    /// Options → Game Options / Audio Options: the port's settings page.
    Settings,
    /// Back on the title screen (`[ours]`: the PS2 has no way out).
    Quit,
}

/// The shell's screens and where each one's focus is.
#[derive(Debug, Clone, PartialEq)]
pub struct ShellMenus {
    pub page: Page,
    pub main: Stack,
    pub singleplayer: Stack,
    pub options: Stack,
    pub prompt: String,
    pub back: String,
    pub select: String,
    /// Seconds since the page was entered (the reveal) and since the shell
    /// started (the prompt's pulse).
    pub on_page: f32,
    pub clock: f32,
    /// A page of the port's own is up over the screen (the Instant Action
    /// setup, the settings): only the movie draws.
    pub covered: bool,
}

impl ShellMenus {
    /// The four screens, their texts looked up by `text(key, English)`.
    pub fn new(text: &dyn Fn(&str, &str) -> String) -> ShellMenus {
        let row = |id: RowId, enabled: bool| {
            let (k, e) = id.key();
            StackRow {
                id,
                label: text(k, e),
                enabled,
            }
        };
        let stack = |title: String, rows: Vec<StackRow>, left: bool, y_add: f32| {
            let mut s = Stack {
                title,
                rows,
                focus: 0,
                left,
                y_add,
            };
            s.focus = s.first();
            s
        };
        let (k, e) = RowId::Singleplayer.key();
        let sp_title = text(k, e);
        let (k, e) = RowId::Options.key();
        let opt_title = text(k, e);
        ShellMenus {
            page: Page::Title,
            main: stack(
                text(TEXT_MAIN.0, TEXT_MAIN.1),
                vec![
                    row(RowId::Singleplayer, true),
                    row(RowId::Multiplayer, false),
                    row(RowId::Splitscreen, false),
                    row(RowId::Options, true),
                ],
                false,
                MAIN_Y_ADD,
            ),
            singleplayer: stack(
                sp_title,
                vec![
                    row(RowId::RiseOfTheEmpire, false),
                    row(RowId::GalacticConquest, false),
                    row(RowId::InstantAction, true),
                ],
                true,
                0.0,
            ),
            options: stack(
                opt_title,
                vec![
                    row(RowId::GameOptions, true),
                    row(RowId::AudioOptions, true),
                    row(RowId::Controls, false),
                ],
                true,
                0.0,
            ),
            prompt: text(TEXT_PROMPT.0, TEXT_PROMPT.1),
            back: text(TEXT_BACK.0, TEXT_BACK.1),
            select: text(TEXT_SELECT.0, TEXT_SELECT.1),
            on_page: 0.0,
            clock: 0.0,
            covered: false,
        }
    }

    pub fn stack(&self) -> Option<&Stack> {
        match self.page {
            Page::Title => None,
            Page::Main => Some(&self.main),
            Page::Singleplayer => Some(&self.singleplayer),
            Page::Options => Some(&self.options),
        }
    }

    fn stack_mut(&mut self) -> Option<&mut Stack> {
        match self.page {
            Page::Title => None,
            Page::Main => Some(&mut self.main),
            Page::Singleplayer => Some(&mut self.singleplayer),
            Page::Options => Some(&mut self.options),
        }
    }

    fn go(&mut self, page: Page) {
        self.page = page;
        self.on_page = 0.0;
    }

    /// One frame: `dt` seconds, the menu edges and Start's.
    pub fn step(&mut self, input: MenuInput, start: bool, dt: f32) -> Option<ShellCommand> {
        self.clock += dt;
        self.on_page += dt;
        if self.page == Page::Title {
            if start {
                self.go(Page::Main);
            } else if input.back {
                return Some(ShellCommand::Quit);
            }
            return None;
        }
        if input.back {
            let to = if self.page == Page::Main {
                Page::Title
            } else {
                Page::Main
            };
            self.go(to);
            return None;
        }
        let s = self.stack_mut()?;
        if input.up {
            s.focus = s.follow(-1);
        }
        if input.down {
            s.focus = s.follow(1);
        }
        if !input.confirm {
            return None;
        }
        let row = s.rows.get(s.focus).filter(|r| r.enabled)?.id;
        match row {
            RowId::Singleplayer => self.go(Page::Singleplayer),
            RowId::Options => self.go(Page::Options),
            RowId::InstantAction => return Some(ShellCommand::InstantAction),
            RowId::GameOptions | RowId::AudioOptions => return Some(ShellCommand::Settings),
            _ => {}
        }
        None
    }

    /// The movie property the page plays (§4).
    pub fn movie(&self) -> &'static str {
        match self.page {
            Page::Title | Page::Main => MOVIE_MAIN,
            Page::Singleplayer | Page::Options => MOVIE_SUB,
        }
    }

    /// The prompt's alpha now: a triangle between [`PROMPT_ALPHA`] at
    /// [`PROMPT_RATE`] a second, starting at the top.
    pub fn prompt_alpha(&self) -> f32 {
        let [lo, hi] = PROMPT_ALPHA;
        let span = hi - lo;
        let t = (self.clock * PROMPT_RATE).rem_euclid(2.0 * span);
        if t < span {
            hi - t
        } else {
            lo + (t - span)
        }
    }
}

/// A stack's boxes in shell units: the title bar and one per row.
#[derive(Debug, Clone, PartialEq)]
pub struct StackLayout {
    pub title: [f32; 4],
    pub rows: Vec<[f32; 4]>,
}

/// Lay a stack out (the [M] rule of the module doc).
pub fn stack_layout(font: &HudFont, stack: &Stack) -> StackLayout {
    let widest = std::iter::once(stack.title.as_str())
        .chain(stack.rows.iter().map(|r| r.label.as_str()))
        .map(|t| run_width(font, t, LABEL_STRETCH, false))
        .fold(0.0, f32::max);
    let w = widest + BAR_PAD;
    let x0 = if stack.left {
        LEFT_X
    } else {
        CENTRE_X - w * 0.5
    };
    let bars = stack.rows.len() + 1;
    let height = bars as f32 * ROW_PITCH - (ROW_PITCH - ROW_H);
    let top = CONTAINER_Y + stack.y_add - CENTRE_LIFT - height * 0.5;
    let bar = |k: usize| {
        let y = top + ROW_PITCH * k as f32;
        [x0, y, x0 + w, y + ROW_H]
    };
    StackLayout {
        title: bar(0),
        rows: (1..bars).map(bar).collect(),
    }
}

impl Ctx<'_> {
    /// The shell's page for a `size` window: the movie (its frame is the
    /// [`MOVIE_SHEET`] texture; `movie` false draws none), then the logo
    /// or the stack and the help lines.
    pub(crate) fn shell_page(&mut self, m: &ShellMenus, movie: bool, size: [f32; 2]) {
        use crate::spawn_draw::{screen_at, Anchor};
        let centre = screen_at(size, Anchor::Centre);
        let full = [0.0, 0.0, 1.0, 1.0];
        if movie {
            self.sheet(&centre, [0.0, 0.0, 512.0, 448.0], full, MOVIE_SHEET, 1.0);
        }
        if m.covered {
            return;
        }
        if matches!(m.page, Page::Title | Page::Main) {
            self.sheet(&centre, LOGO, full, "logo", 1.0);
        }
        let Some(stack) = m.stack() else {
            let (rgb, _) = UNSELECTED_TEXT;
            let at = (256.0, PROMPT_TOP);
            let style = (rgb, m.prompt_alpha());
            self.label(&centre, BUTTON_FONT, &m.prompt, at, HAlign::Center, style);
            return;
        };
        let Some(font) = self.art.fonts.get(BUTTON_FONT) else {
            return;
        };
        let lay = stack_layout(font, stack);
        let lh = f32::from(font.line_height);
        let s = if stack.left {
            screen_at(size, Anchor::Left)
        } else {
            centre
        };
        // `[ours]` the reveal counts the title as row 0.
        let shown = |k: usize| m.on_page >= ROW_REVEAL * k as f32;
        let title_pieces = [
            "bf2_buttons_topleft",
            "bf2_buttons_title_center",
            "bf2_buttons_topright",
        ];
        self.bar(&s, lay.title, title_pieces);
        let mid = |r: [f32; 4]| ((r[0] + r[2]) * 0.5, r[1] + (ROW_H - lh) * 0.5);
        self.label(
            &s,
            BUTTON_FONT,
            &stack.title,
            mid(lay.title),
            HAlign::Center,
            TITLE_TEXT,
        );
        for (k, (row, r)) in stack.rows.iter().zip(&lay.rows).enumerate() {
            if !shown(k + 1) {
                break;
            }
            let pieces = if k + 1 == stack.rows.len() {
                [
                    "bf2_buttons_botleft",
                    "bf2_buttons_items_center",
                    "bf2_buttons_botright",
                ]
            } else {
                [
                    "bf2_buttons_midleft",
                    "bf2_buttons_items_center",
                    "bf2_buttons_midright",
                ]
            };
            self.bar(&s, *r, pieces);
            let style = if !row.enabled {
                ([DIMMED_GREY; 3], UNSELECTED_TEXT.1)
            } else if k == stack.focus {
                SELECTED_TEXT
            } else {
                UNSELECTED_TEXT
            };
            self.label(&s, BUTTON_FONT, &row.label, mid(*r), HAlign::Center, style);
        }
        self.help(size, false, "btnb", &m.back);
        self.help(size, true, "btna", &m.select);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menus() -> ShellMenus {
        ShellMenus::new(&|_, e| e.to_owned())
    }

    fn press(f: impl Fn(&mut MenuInput)) -> MenuInput {
        let mut i = MenuInput::default();
        f(&mut i);
        i
    }

    /// Acceptance 2 of card BSHLA: Multiplayer sits in its PS2 slot,
    /// disabled, and focus skips it both ways (§3.3: the PS2 skips its
    /// dimmed Splitscreen the same way).
    #[test]
    fn main_menu_skips_the_disabled_multiplayer() {
        let mut m = menus();
        assert_eq!(m.step(MenuInput::default(), true, 0.1), None);
        assert_eq!(m.page, Page::Main);
        let ids: Vec<_> = m.main.rows.iter().map(|r| r.id).collect();
        assert_eq!(
            ids,
            [
                RowId::Singleplayer,
                RowId::Multiplayer,
                RowId::Splitscreen,
                RowId::Options
            ]
        );
        let mp = &m.main.rows[1];
        assert!(!mp.enabled, "Multiplayer is present and disabled");
        assert_eq!(m.main.focus, 0);
        m.step(press(|i| i.down = true), false, 0.1);
        assert_eq!(
            m.main.rows[m.main.focus].id,
            RowId::Options,
            "Down skips it"
        );
        m.step(press(|i| i.up = true), false, 0.1);
        assert_eq!(m.main.rows[m.main.focus].id, RowId::Singleplayer, "and Up");
        // Wrapping: Up from Singleplayer lands on Options, Down from Options
        // on Singleplayer.
        m.step(press(|i| i.up = true), false, 0.1);
        assert_eq!(m.main.rows[m.main.focus].id, RowId::Options);
        m.step(press(|i| i.down = true), false, 0.1);
        assert_eq!(m.main.rows[m.main.focus].id, RowId::Singleplayer);
        // A confirm never lands on a disabled row.
        m.main.focus = 1;
        assert_eq!(m.step(press(|i| i.confirm = true), false, 0.1), None);
        assert_eq!(m.page, Page::Main);
    }

    /// The main menu as the HUD pass lays it out: the dimmed rows' labels
    /// are drawn grey 110 (§3.3), every glyph of both of them.
    #[test]
    fn the_disabled_rows_are_drawn_dimmed() {
        use crate::hud_draw::HudArt;
        use crate::hud_tree::Hud;
        use manifest::hud::{HudFont, HudGlyph, HudLayout};
        let font = HudFont {
            name: BUTTON_FONT.into(),
            line_height: 22,
            pages: vec!["page0".into()],
            glyphs: (32u8..127)
                .map(|code| HudGlyph {
                    code: u16::from(code),
                    page: 0,
                    advance: 10,
                    x0: 0,
                    x1: 10,
                    y0: 2,
                    y1: 14,
                    uv: [0.0, 0.1, 0.0, 0.1],
                })
                .collect(),
        };
        let mut art = HudArt::default();
        art.fonts.insert(BUTTON_FONT.into(), font);
        let empty = HudLayout {
            name: "t".into(),
            nodes: Vec::new(),
        };
        let mut hud = Hud::new(&empty, &empty);
        let mut m = menus();
        m.step(MenuInput::default(), true, 5.0);
        m.step(MenuInput::default(), false, 5.0);
        hud.shell = Some((m, false));
        let f = crate::hud_draw::draw(&hud, &art, [640.0, 480.0]);
        let grey = DIMMED_GREY / 255.0;
        let dimmed = f
            .verts
            .iter()
            .filter(|v| v.color[..3].iter().all(|c| (c - grey).abs() < 1e-4))
            .count();
        let glyphs = "MultiplayerSplitscreen".len();
        assert_eq!(dimmed, 4 * glyphs, "Multiplayer and Splitscreen dimmed");
        let sheets: Vec<&str> = f.batches.iter().map(|b| b.texture.as_str()).collect();
        for p in [
            "logo",
            "bf2_buttons_topleft",
            "bf2_buttons_botright",
            "btna",
            "btnb",
        ] {
            assert!(sheets.contains(&p), "{p}: {sheets:?}");
        }
    }

    #[test]
    fn the_flow_reaches_instant_action_and_back() {
        let mut m = menus();
        let confirm = press(|i| i.confirm = true);
        // Only Start leaves the title: Cross does nothing there.
        assert_eq!(m.step(confirm, false, 0.1), None);
        assert_eq!(m.page, Page::Title);
        m.step(MenuInput::default(), true, 0.1);
        assert_eq!(m.step(confirm, false, 0.1), None);
        assert_eq!(m.page, Page::Singleplayer);
        assert_eq!(m.movie(), MOVIE_SUB);
        // The first focus skips the two disabled campaigns.
        assert_eq!(
            m.singleplayer.rows[m.singleplayer.focus].id,
            RowId::InstantAction
        );
        assert_eq!(
            m.step(confirm, false, 0.1),
            Some(ShellCommand::InstantAction)
        );
        m.step(press(|i| i.back = true), false, 0.1);
        assert_eq!(m.page, Page::Main);
        m.step(press(|i| i.down = true), false, 0.1);
        m.step(confirm, false, 0.1);
        assert_eq!(m.page, Page::Options);
        assert_eq!(m.step(confirm, false, 0.1), Some(ShellCommand::Settings));
        m.step(press(|i| i.back = true), false, 0.1);
        m.step(press(|i| i.back = true), false, 0.1);
        assert_eq!(m.page, Page::Title);
        assert_eq!(m.movie(), MOVIE_MAIN);
        assert_eq!(
            m.step(press(|i| i.back = true), false, 0.1),
            Some(ShellCommand::Quit)
        );
    }

    #[test]
    fn the_prompt_pulses_between_its_alphas() {
        let mut m = menus();
        let mut seen = (f32::MAX, f32::MIN);
        for _ in 0..100 {
            m.step(MenuInput::default(), false, 0.05);
            let a = m.prompt_alpha();
            seen = (seen.0.min(a), seen.1.max(a));
        }
        assert!(
            (seen.0 - 0.4).abs() < 0.03 && (seen.1 - 1.0).abs() < 0.03,
            "{seen:?}"
        );
    }
}
