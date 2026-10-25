//! The windowed shell's front screens (card BSHLA, `ui::shell_ps2`): the
//! PS2's Press START, Main Menu, Singleplayer and Options over the shell
//! movie, with the shell music; under them the Instant Action setup
//! (`ui::shell`, until card BSHLB) and the settings page (`ui::menu` in its
//! shell form). A child of `windowed` so it reaches the `App` state.

use super::{App, Camera};

/// The BF2 PS2 shell's front screens (card BSHLA, `ui::shell_ps2`): Press
/// START, the Main Menu, Singleplayer and Options over the background
/// movie, with the shell music; Instant Action opens the egui setup
/// (`ui::shell`, until card BSHLB) and Game / Audio Options the settings
/// page (`ui::menu` in its shell form).
pub(super) struct FrontShell {
    menus: ui::shell_ps2::ShellMenus,
    movie: Option<game::shell_movie::ShellMovie>,
    /// Built at the first frame with a renderer.
    view: Option<game::hud_feed::HudView>,
    /// Instant Action's setup is up.
    setup: bool,
    /// The settings page is up, at this cursor.
    settings: Option<usize>,
    sounds: Option<game::sounds::SoundLib>,
    pub(super) music: Option<audio::MusicVoice>,
}

impl FrontShell {
    pub(super) fn new(assets_dir: &std::path::Path) -> Self {
        let texts = game::event_feed::Texts::load(assets_dir, "bf2");
        let text = |key: &str, english: &str| {
            texts
                .bf2_text(key)
                .map_or_else(|| english.to_owned(), |t| t.trim_end().to_owned())
        };
        let movie = game::shell_movie::ShellMovie::load(assets_dir);
        if movie.is_none() {
            eprintln!("shell: no movie baked (bake common); the menus draw over black");
        }
        FrontShell {
            menus: ui::shell_ps2::ShellMenus::new(&text),
            movie,
            view: None,
            setup: false,
            settings: None,
            sounds: game::sounds::SoundLib::load(&game::shell_movie::shell_dir(assets_dir)),
            music: None,
        }
    }
}

impl App {
    /// Shell frame: the PS2 front screens (card BSHLA) or, under them, the
    /// Instant Action setup or the settings page; navigate, maybe start,
    /// draw.
    pub(super) fn redraw_shell(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        dt: f32,
    ) {
        let edges = self.menu_edges();
        let input = ui::menu::MenuInput {
            up: edges & 1 != 0,
            down: edges & 2 != 0,
            left: edges & 4 != 0,
            right: edges & 8 != 0,
            confirm: edges & 16 != 0,
            back: edges & 32 != 0,
        };
        let start = edges & 64 != 0;
        let mut front = self
            .front
            .take()
            .unwrap_or_else(|| FrontShell::new(&self.assets_dir));
        if front.view.is_none() {
            if let Some(r) = &self.renderer {
                front.view = game::hud_feed::HudView::for_renderer(&self.assets_dir, r)
                    .map_err(|e| {
                        eprintln!("shell: no BF2 HUD bake ({e:#}); the menus are not drawn")
                    })
                    .ok();
            }
        }
        let mut actions = Vec::new();
        let mut quit = false;
        let mut settings_model = None;
        if front.setup {
            if input.back {
                front.setup = false;
            } else {
                actions = ui::shell::navigate(&mut self.shell, input);
                self.seed_shell_from_map();
            }
        } else if let Some(cursor) = front.settings {
            let settings = &self.profile.settings;
            let k = settings.fov_scale;
            let model = ui::menu::MenuModel {
                assist_intensity: settings.assist_intensity,
                ai_alive_cap: settings.ai_alive_cap.unwrap_or(16),
                fov_scale: k,
                fov_deg: (game::walk::ps2_fov_y("bf2") * k).to_degrees(),
                music_volume: settings.music_volume,
                mouse_sensitivity: settings.mouse_sensitivity,
                invert_y: settings.invert_y,
                cursor,
                in_shell: Some(
                    front.menus.options.rows[front.menus.options.focus]
                        .label
                        .clone(),
                ),
                ..Default::default()
            };
            let (cursor, acts) = ui::menu::navigate(&model, input);
            front.settings = Some(cursor);
            for a in acts {
                self.apply_shell_setting(&mut front, a);
            }
            settings_model = front
                .settings
                .map(|cursor| ui::menu::MenuModel { cursor, ..model });
        } else {
            match front.menus.step(input, start, dt) {
                Some(ui::shell_ps2::ShellCommand::InstantAction) => front.setup = true,
                Some(ui::shell_ps2::ShellCommand::Settings) => front.settings = Some(0),
                Some(ui::shell_ps2::ShellCommand::Quit) => quit = true,
                None => {}
            }
        }
        // The music (`ifs_start` sets `shell_soundtrack`, §2) at the
        // profile's level; `[H]` its first take, looped.
        if front.music.is_none() {
            if let (Some(audio), Some(sl)) = (self.audio.as_mut(), front.sounds.as_ref()) {
                let level = self.profile.settings.music_volume;
                if let Some((gain, takes)) = sl
                    .stream_takes(ui::shell_ps2::MUSIC)
                    .filter(|_| level > 0.0)
                {
                    front.music = audio
                        .play_stream(takes[0].clone(), game::sounds::level_db(gain * level), true)
                        .map_err(|e| eprintln!("shell music: {e:#}"))
                        .ok();
                }
            }
        }
        front.menus.covered = front.setup || front.settings.is_some();
        let movie_name = front.menus.movie();
        let mut has_movie = false;
        if let Some(m) = front.movie.as_mut() {
            m.play(movie_name);
            if let Some((rgba, w, h)) = m.frame(dt) {
                if let Some(v) = front.view.as_mut() {
                    v.set_movie_frame(rgba, w, h);
                }
            }
            has_movie = m.shown;
        }
        let model = self.shell.clone();
        let mut clicked = Vec::new();
        if let (Some(r), Some(overlay), Some(window)) =
            (&mut self.renderer, &mut self.overlay, &self.window)
        {
            let (w, h) = r.size();
            if let Some(v) = front.view.as_mut() {
                v.update_shell(&front.menus, has_movie, [w, h]);
            }
            let warning = self.pad_warning.as_deref();
            let setup = front.setup;
            let paint = overlay.begin(window, |ctx| {
                if setup {
                    clicked = ui::shell::draw_shell(ctx, &model);
                }
                if let Some(m) = &settings_model {
                    let _ = ui::menu::draw_menu(ctx, m);
                }
                if let Some(w) = warning {
                    ui::hud::draw_pad_warning(ctx, w);
                }
            });
            let view = &mut front.view;
            if let Err(e) = r.render_with_overlay(
                &Camera::default(),
                &[],
                [0.0, 0.0, 0.0, 1.0],
                Some(&mut |dev, q, enc, target, size| {
                    if let Some(v) = view.as_mut() {
                        v.paint(dev, q, enc, target, size);
                    }
                    overlay.paint(&paint, dev, q, enc, target, size)
                }),
            ) {
                eprintln!("render: {e}");
                event_loop.exit();
            }
        }
        self.front = Some(front);
        actions.extend(clicked);
        if quit {
            event_loop.exit();
        }
        for a in actions {
            match a {
                ui::shell::ShellAction::Start => self.begin_loading(),
                ui::shell::ShellAction::Quit => event_loop.exit(),
            }
        }
    }

    /// One settings page action in the shell: the profile's value, saved;
    /// the music's level follows its slider.
    fn apply_shell_setting(&mut self, front: &mut FrontShell, a: ui::menu::MenuAction) {
        use ui::menu::MenuAction;
        let s = &mut self.profile.settings;
        match a {
            MenuAction::Resume | MenuAction::Quit => {
                front.settings = None;
                return;
            }
            MenuAction::ToggleCheat(_) => return,
            MenuAction::SetAssist(v) => s.assist_intensity = v.clamp(0.0, 1.5),
            MenuAction::SetAiCap(c) => s.ai_alive_cap = Some(c),
            MenuAction::SetFov(k) => {
                s.fov_scale = k.clamp(game::walk::FOV_SCALE_RANGE.0, game::walk::FOV_SCALE_RANGE.1)
            }
            MenuAction::SetMusic(v) => {
                s.music_volume = v.clamp(game::music::VOLUME_RANGE.0, game::music::VOLUME_RANGE.1);
                // Restarted at the new level by the next frame (none at 0).
                front.music = None;
            }
            MenuAction::SetMouseSensitivity(v) => {
                s.mouse_sensitivity = v.clamp(
                    game::kbm::SENSITIVITY_RANGE.0,
                    game::kbm::SENSITIVITY_RANGE.1,
                )
            }
            MenuAction::ToggleInvertY => s.invert_y = !s.invert_y,
        }
        self.kbm.follow(&self.profile.settings);
        if let Err(e) = self.profile.save() {
            eprintln!("profile autosave failed: {e:#}");
        }
    }
}
