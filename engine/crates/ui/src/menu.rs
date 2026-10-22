//! Pause menu (BF2-style, controller-navigated): resume, the session cheat
//! registry (PLAN §Cheats), the settings (aim-assist intensity slider,
//! unit-count/alive-AI cap slider, the FOV option, the music volume), quit. Pure egui over a
//! [`MenuModel`]; the game applies the returned [`MenuAction`]s.
//!
//! Navigation: D-pad up/down moves the cursor, Cross activates/toggles,
//! left/right adjusts sliders, Circle or Start resumes. The pointer works too
//! (egui), but the game is controller-only by design.

use egui::{Align2, Color32, RichText, Vec2};

#[derive(Debug, Clone, Default)]
pub struct MenuModel {
    /// Cheat registry rows: (id, label, enabled).
    pub cheats: Vec<(&'static str, &'static str, bool)>,
    pub assist_intensity: f32,
    pub ai_alive_cap: u32,
    /// The FOV option (a multiplier on the PS2 camera's angle) and the
    /// vertical FOV it gives, in degrees, for the label.
    pub fov_scale: f32,
    pub fov_deg: f32,
    /// The music volume, 0..1 (1 = the disc record's own gain, 0 = off).
    pub music_volume: f32,
    pub cursor: usize,
    /// Round result line, if the round is over.
    pub outcome_line: Option<String>,
    /// The shell's settings page (card BSHLA: Options → Game / Audio
    /// Options): titled by `title`, its first row "Back", no Quit row.
    pub in_shell: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuAction {
    Resume,
    ToggleCheat(usize),
    SetAssist(f32),
    SetAiCap(u32),
    SetFov(f32),
    SetMusic(f32),
    Quit,
}

/// Controller edges for one frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct MenuInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    pub back: bool,
}

/// Rows in cursor order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Row {
    Resume,
    Cheat(usize),
    Assist,
    AiCap,
    Fov,
    Music,
    Quit,
}

pub fn rows(m: &MenuModel) -> Vec<Row> {
    let mut v = vec![Row::Resume];
    v.extend((0..m.cheats.len()).map(Row::Cheat));
    v.push(Row::Assist);
    v.push(Row::AiCap);
    v.push(Row::Fov);
    v.push(Row::Music);
    if m.in_shell.is_none() {
        v.push(Row::Quit);
    }
    v
}

/// Apply controller input to the cursor/values; returns actions to perform
/// and the new cursor.
pub fn navigate(m: &MenuModel, input: MenuInput) -> (usize, Vec<MenuAction>) {
    let rows = rows(m);
    let n = rows.len();
    let mut cursor = m.cursor.min(n - 1);
    let mut actions = Vec::new();
    if input.back {
        actions.push(MenuAction::Resume);
        return (cursor, actions);
    }
    if input.up {
        cursor = (cursor + n - 1) % n;
    }
    if input.down {
        cursor = (cursor + 1) % n;
    }
    let step = |dir: f32| dir;
    match rows[cursor] {
        Row::Resume if input.confirm => actions.push(MenuAction::Resume),
        Row::Cheat(i) if input.confirm => actions.push(MenuAction::ToggleCheat(i)),
        Row::Assist if input.left || input.right => {
            let d = if input.right { step(0.1) } else { step(-0.1) };
            actions.push(MenuAction::SetAssist(
                ((m.assist_intensity + d) * 10.0).round() / 10.0,
            ));
        }
        Row::AiCap if input.left || input.right => {
            let cap = if input.right {
                m.ai_alive_cap.saturating_add(2).min(64)
            } else {
                m.ai_alive_cap.saturating_sub(2).max(2)
            };
            actions.push(MenuAction::SetAiCap(cap));
        }
        Row::Fov if input.left || input.right => {
            let d = if input.right { 0.05 } else { -0.05 };
            actions.push(MenuAction::SetFov(
                ((m.fov_scale + d) * 20.0).round() / 20.0,
            ));
        }
        Row::Music if input.left || input.right => {
            let d = if input.right { 0.1 } else { -0.1 };
            actions.push(MenuAction::SetMusic(
                ((m.music_volume + d) * 10.0).round().clamp(0.0, 10.0) / 10.0,
            ));
        }
        Row::Quit if input.confirm => actions.push(MenuAction::Quit),
        _ => {}
    }
    (cursor, actions)
}

fn row_label(ui: &mut egui::Ui, selected: bool, text: String, value: Option<String>) -> bool {
    let bg = if selected {
        Color32::from_rgba_premultiplied(60, 90, 140, 200)
    } else {
        Color32::TRANSPARENT
    };
    let mut clicked = false;
    egui::Frame::new()
        .fill(bg)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .corner_radius(3.0)
        .show(ui, |ui| {
            ui.set_min_width(360.0);
            ui.horizontal(|ui| {
                let r = ui.label(RichText::new(text).size(18.0).color(if selected {
                    Color32::WHITE
                } else {
                    Color32::LIGHT_GRAY
                }));
                clicked |= r.clicked();
                if let Some(v) = value {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(v).size(18.0).color(Color32::WHITE).strong());
                    });
                }
            });
        });
    clicked
}

/// Draw the menu; pointer clicks produce actions too.
pub fn draw_menu(ctx: &egui::Context, m: &MenuModel) -> Vec<MenuAction> {
    let mut actions = Vec::new();
    egui::Area::new(egui::Id::new("pause_menu"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_black_alpha(200))
                .inner_margin(egui::Margin::same(18))
                .corner_radius(6.0)
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(m.in_shell.as_deref().unwrap_or("PAUSED"))
                                .size(28.0)
                                .color(Color32::WHITE)
                                .strong(),
                        );
                        if let Some(o) = &m.outcome_line {
                            ui.label(RichText::new(o).size(16.0).color(Color32::LIGHT_GRAY));
                        }
                    });
                    ui.add_space(8.0);
                    let rows = rows(m);
                    for (i, row) in rows.iter().enumerate() {
                        let sel = i == m.cursor;
                        let clicked = match row {
                            Row::Resume => {
                                let back = if m.in_shell.is_some() {
                                    "Back"
                                } else {
                                    "Resume"
                                };
                                row_label(ui, sel, back.into(), None)
                            }
                            Row::Cheat(k) => {
                                let (_, label, on) = m.cheats[*k];
                                row_label(
                                    ui,
                                    sel,
                                    format!("Cheat: {label}"),
                                    Some(if on { "ON".into() } else { "off".into() }),
                                )
                            }
                            Row::Assist => row_label(
                                ui,
                                sel,
                                "Aim assist intensity  ◀ ▶".into(),
                                Some(format!("{:.1}", m.assist_intensity)),
                            ),
                            Row::AiCap => row_label(
                                ui,
                                sel,
                                "Alive AI per team  ◀ ▶".into(),
                                Some(format!("{}", m.ai_alive_cap)),
                            ),
                            Row::Fov => row_label(
                                ui,
                                sel,
                                "Field of view  ◀ ▶".into(),
                                Some(format!("{:.2}× ({:.0}°)", m.fov_scale, m.fov_deg)),
                            ),
                            Row::Music => row_label(
                                ui,
                                sel,
                                "Music volume  ◀ ▶".into(),
                                Some(if m.music_volume <= 0.0 {
                                    "off".into()
                                } else {
                                    format!("{:.0}%", m.music_volume * 100.0)
                                }),
                            ),
                            Row::Quit => row_label(ui, sel, "Quit to desktop".into(), None),
                        };
                        if clicked {
                            match row {
                                Row::Resume => actions.push(MenuAction::Resume),
                                Row::Cheat(k) => actions.push(MenuAction::ToggleCheat(*k)),
                                Row::Quit => actions.push(MenuAction::Quit),
                                _ => {}
                            }
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("D-pad move · ✕ select · ◀▶ adjust · ○/Start resume")
                            .size(12.0)
                            .color(Color32::GRAY),
                    );
                });
        });
    actions
}

/// PS2 BF2 pause-menu cheat codes: Morse code typed on the D-pad while paused
/// (engine `FUN_0038c1d8`; tables at 0x5ff100/0x5ff220 — lead-verified against
/// the raw ELF 2026-08-30, see `analysis/re-cheat-codes.md`). Up = dot,
/// Down = dash, Left = end of letter, Right = submit word. Rising edges only,
/// one symbol per frame, priority Up > Down > Left > Right. The letter buffer
/// wraps at 6 symbols (`% 6`), the word at 32 chars (`& 0x1f`); nothing times
/// out and nothing resets on resume or level change — a half-typed word
/// survives until the next submit. Unrecognised letters are dropped,
/// unrecognised words reset silently. BF1 has no such system.
#[derive(Debug, Clone)]
pub struct MorseCheats {
    letter: [u8; 8],
    mlen: usize,
    word: [u8; 33],
    wlen: usize,
}

impl Default for MorseCheats {
    fn default() -> Self {
        Self::new()
    }
}

/// ITU Morse for a–z, 0–9 in the engine's table order (0x5ff100).
pub const MORSE: [(u8, &str); 36] = [
    (b'a', ".-"),
    (b'b', "-..."),
    (b'c', "-.-."),
    (b'd', "-.."),
    (b'e', "."),
    (b'f', "..-."),
    (b'g', "--."),
    (b'h', "...."),
    (b'i', ".."),
    (b'j', ".---"),
    (b'k', "-.-"),
    (b'l', ".-.."),
    (b'm', "--"),
    (b'n', "-."),
    (b'o', "---"),
    (b'p', ".--."),
    (b'q', "--.-"),
    (b'r', ".-."),
    (b's', "..."),
    (b't', "-"),
    (b'u', "..-"),
    (b'v', "...-"),
    (b'w', ".--"),
    (b'x', "-..-"),
    (b'y', "-.--"),
    (b'z', "--.."),
    (b'0', "-----"),
    (b'1', ".----"),
    (b'2', "..---"),
    (b'3', "...--"),
    (b'4', "....-"),
    (b'5', "....."),
    (b'6', "-...."),
    (b'7', "--..."),
    (b'8', "---.."),
    (b'9', "----."),
];

/// The engine's 7-entry `{handler, hash}` table (0x5ff220) with the words
/// recovered by hashing: FNV-1a-32 over `byte | 0x20`.
pub const CHEAT_WORDS: [(&str, u32); 7] = [
    ("alias", 0x7d26_8157),
    ("sos", 0xcb36_196c),
    ("ammo", 0x5b9b_0daf),
    ("80s", 0xfe9e_d9d2),
    ("at", 0x5725_1588),
    ("hud", 0xe28d_61c6),
    ("slow", 0x6d95_42b8),
];

/// FNV-1a-32 with the engine's `| 0x20` fold (`FUN_00539308`).
pub fn fnv1a_lower(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5u32, |h, &b| {
        (h ^ u32::from(b | 0x20)).wrapping_mul(0x0100_0193)
    })
}

fn cstr(buf: &[u8]) -> &[u8] {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    &buf[..end]
}

impl MorseCheats {
    pub fn new() -> Self {
        Self {
            letter: [0; 8],
            mlen: 0,
            word: [0; 33],
            wlen: 0,
        }
    }

    /// Feed one frame of D-pad rising edges; returns the recognised word when
    /// Right submits a known one.
    pub fn feed(&mut self, input: MenuInput) -> Option<&'static str> {
        let mut hit = None;
        if input.up {
            self.push(b'.');
        } else if input.down {
            self.push(b'-');
        } else if input.left {
            // FUN_0038c1c0: terminate + reset, then strncmp(code, morse, 6)
            // over the 36 entries; the first match appends its letter.
            self.letter[self.mlen] = 0;
            self.mlen = 0;
            let code = cstr(&self.letter);
            if let Some((ch, _)) = MORSE.iter().find(|(_, m)| m.as_bytes() == code) {
                self.word[self.wlen] = *ch;
                self.wlen += 1;
            }
        } else if input.right {
            self.word[self.wlen] = 0;
            self.wlen = 0;
            let h = fnv1a_lower(cstr(&self.word));
            hit = CHEAT_WORDS.iter().find(|(_, wh)| *wh == h).map(|(w, _)| *w);
            self.letter[self.mlen] = 0;
            self.mlen = 0;
        }
        self.wlen &= 0x1f;
        self.mlen %= 6;
        hit
    }

    fn push(&mut self, sym: u8) {
        self.letter[self.mlen] = sym;
        self.mlen += 1;
    }

    /// (letter symbols so far, word so far) — for tests and debug overlays.
    pub fn pending(&self) -> (String, String) {
        let l = String::from_utf8_lossy(&self.letter[..self.mlen]).into_owned();
        let w = String::from_utf8_lossy(&self.word[..self.wlen]).into_owned();
        (l, w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> MenuModel {
        MenuModel {
            cheats: vec![
                ("invincible", "Invincibility", false),
                ("ammo", "Infinite ammo", true),
            ],
            assist_intensity: 1.0,
            ai_alive_cap: 16,
            fov_scale: 1.0,
            fov_deg: 37.7,
            music_volume: 1.0,
            cursor: 0,
            outcome_line: None,
            in_shell: None,
        }
    }

    #[test]
    fn navigation_wraps_toggles_and_adjusts() {
        let mut m = model();
        // Down twice → second cheat; confirm toggles it.
        let (c, a) = navigate(
            &m,
            MenuInput {
                down: true,
                ..Default::default()
            },
        );
        m.cursor = c;
        let (c, a2) = navigate(
            &m,
            MenuInput {
                down: true,
                confirm: true,
                ..Default::default()
            },
        );
        assert!(a.is_empty());
        assert_eq!((c, a2), (2, vec![MenuAction::ToggleCheat(1)]));
        // Up from the top wraps to Quit.
        m.cursor = 0;
        let (c, _) = navigate(
            &m,
            MenuInput {
                up: true,
                ..Default::default()
            },
        );
        assert_eq!(rows(&m)[c], Row::Quit);
        // Slider rows adjust with left/right and clamp.
        m.cursor = 3; // Assist
        let (_, a) = navigate(
            &m,
            MenuInput {
                right: true,
                ..Default::default()
            },
        );
        assert_eq!(a, vec![MenuAction::SetAssist(1.1)]);
        m.cursor = 4; // AiCap
        m.ai_alive_cap = 2;
        let (_, a) = navigate(
            &m,
            MenuInput {
                left: true,
                ..Default::default()
            },
        );
        assert_eq!(a, vec![MenuAction::SetAiCap(2)]);
        m.cursor = 5; // Fov
        assert_eq!(rows(&m)[5], Row::Fov);
        let (_, a) = navigate(
            &m,
            MenuInput {
                right: true,
                ..Default::default()
            },
        );
        assert_eq!(a, vec![MenuAction::SetFov(1.05)]);
        // The music volume: tenths, held in 0..1.
        m.cursor = 6;
        assert_eq!(rows(&m)[6], Row::Music);
        let step = |m: &MenuModel, right: bool| {
            let input = MenuInput {
                left: !right,
                right,
                ..Default::default()
            };
            navigate(m, input).1
        };
        assert_eq!(step(&m, true), vec![MenuAction::SetMusic(1.0)]);
        assert_eq!(step(&m, false), vec![MenuAction::SetMusic(0.9)]);
        m.music_volume = 0.1;
        assert_eq!(step(&m, false), vec![MenuAction::SetMusic(0.0)]);
        m.music_volume = 0.0;
        assert_eq!(step(&m, false), vec![MenuAction::SetMusic(0.0)]);
        // Back always resumes.
        let (_, a) = navigate(
            &m,
            MenuInput {
                back: true,
                ..Default::default()
            },
        );
        assert_eq!(a, vec![MenuAction::Resume]);
        // Draws headless.
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw_menu(ctx, &m);
        });
    }
}

#[cfg(test)]
mod morse_tests {
    use super::*;

    fn edge(up: bool, down: bool, left: bool, right: bool) -> MenuInput {
        MenuInput {
            up,
            down,
            left,
            right,
            confirm: false,
            back: false,
        }
    }

    /// Type a word the PS2 way: symbols, Left after each letter, Right at the end.
    fn type_word(m: &mut MorseCheats, word: &str) -> Option<&'static str> {
        let mut hit = None;
        for ch in word.bytes() {
            let code = MORSE.iter().find(|(c, _)| *c == ch).unwrap().1;
            for sym in code.bytes() {
                assert!(m
                    .feed(edge(sym == b'.', sym == b'-', false, false))
                    .is_none());
            }
            assert!(m.feed(edge(false, false, true, false)).is_none());
        }
        hit = hit.or(m.feed(edge(false, false, false, true)));
        hit
    }

    #[test]
    fn all_seven_words_recognised() {
        let mut m = MorseCheats::new();
        for (w, h) in CHEAT_WORDS {
            assert_eq!(fnv1a_lower(w.as_bytes()), h, "{w}");
            assert_eq!(type_word(&mut m, w), Some(w));
            assert_eq!(m.pending(), (String::new(), String::new()));
        }
    }

    #[test]
    fn unknown_word_resets_silently_then_next_word_works() {
        let mut m = MorseCheats::new();
        assert_eq!(type_word(&mut m, "sso"), None);
        assert_eq!(type_word(&mut m, "sos"), Some("sos"));
    }

    #[test]
    fn unrecognised_letter_is_dropped_but_word_continues() {
        let mut m = MorseCheats::new();
        // "..-.-" is no Morse letter: dropped, the word buffer is untouched.
        for sym in "..-.-".bytes() {
            m.feed(edge(sym == b'.', sym == b'-', false, false));
        }
        m.feed(edge(false, false, true, false));
        assert_eq!(m.pending().1, "");
        assert_eq!(type_word(&mut m, "at"), Some("at"));
    }

    #[test]
    fn letter_buffer_wraps_at_six_symbols() {
        // Six dots wrap mlen to 0; a seventh symbol overwrites slot 0 and the
        // commit sees a one-symbol string: '-' -> 't'.
        let mut m = MorseCheats::new();
        for _ in 0..6 {
            m.feed(edge(true, false, false, false));
        }
        assert_eq!(m.pending().0, "");
        m.feed(edge(false, true, false, false));
        m.feed(edge(false, false, true, false));
        assert_eq!(m.pending().1, "t");
    }

    #[test]
    fn priority_up_over_down_and_state_persists_across_idle_frames() {
        let mut m = MorseCheats::new();
        m.feed(edge(true, true, false, false));
        assert_eq!(m.pending().0, ".");
        for _ in 0..1000 {
            m.feed(edge(false, false, false, false));
        }
        assert_eq!(m.pending().0, ".");
        m.feed(edge(true, false, false, false));
        m.feed(edge(true, false, false, false));
        m.feed(edge(false, false, true, false));
        assert_eq!(m.pending(), (String::new(), "s".into()));
    }
}
