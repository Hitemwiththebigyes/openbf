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
    controls: Option<Controls>,
    sounds: Option<game::sounds::SoundLib>,
    pub(super) music: Option<audio::MusicVoice>,
}

struct Controls {
    cursor: usize,
    capture: Option<usize>,
    ready: bool,
    controller: Option<gilrs::GamepadId>,
    message: Option<String>,
}

const CONTROL_COMMAND_ROWS: usize = 2;

impl Controls {
    /// Start always cancels, including keyboard rows and simultaneous button presses.
    /// A held accept button must be released before it can become a new binding.
    fn capture_button(
        &mut self,
        mut down: impl FnMut(gilrs::Button) -> bool,
    ) -> Option<gilrs::Button> {
        if down(gilrs::Button::Start) {
            self.capture = None;
            self.message = Some("Binding change cancelled".into());
            return None;
        }
        let pressed = PAD_BUTTONS.iter().copied().find(|b| down(*b));
        if !self.ready {
            self.ready = pressed.is_none();
            None
        } else {
            pressed
        }
    }
}

fn action_name(bits: u32, walk: (i8, i8)) -> String {
    use core::sim::buttons as b;
    let mut names = Vec::new();
    match walk {
        (0, 1) => names.push("Move forward"),
        (0, -1) => names.push("Move backward"),
        (-1, 0) => names.push("Move left"),
        (1, 0) => names.push("Move right"),
        _ => {}
    }
    for (bit, name) in [
        (b::FIRE, "Fire"),
        (b::SECONDARY_FIRE, "Secondary fire"),
        (b::ZOOM, "Zoom"),
        (b::JUMP, "Jump"),
        (b::ROLL_STANCE, "Roll / stance"),
        (b::SPRINT, "Sprint"),
        (b::RELOAD, "Reload"),
        (b::ACTION, "Action"),
        (b::SWITCH_WEAPON, "Next weapon"),
        (b::SWITCH_SECONDARY, "Secondary weapon"),
        (b::SQUAD_MOVE_OUT, "Squad move"),
        (b::SQUAD_HOLD, "Squad hold"),
        (b::SQUAD_AT_EASE, "Squad at ease"),
        (b::SQUAD_FOLLOW, "Squad follow"),
        (b::SPAWN_ACCEPT, "Spawn accept"),
        (b::SPAWN_BACK, "Spawn back"),
        (b::SPAWN_CLASS_NEXT, "Next class"),
        (b::SPAWN_CLASS_PREV, "Previous class"),
        (b::SPAWN_POST_UP, "Post up"),
        (b::SPAWN_POST_DOWN, "Post down"),
        (b::SPAWN_POST_LEFT, "Post left"),
        (b::SPAWN_POST_RIGHT, "Post right"),
    ] {
        if bits & bit != 0 {
            names.push(name);
        }
    }
    names.join(" / ")
}

const PAD_BUTTONS: [gilrs::Button; 17] = [
    gilrs::Button::South,
    gilrs::Button::East,
    gilrs::Button::West,
    gilrs::Button::North,
    gilrs::Button::LeftTrigger,
    gilrs::Button::LeftTrigger2,
    gilrs::Button::RightTrigger,
    gilrs::Button::RightTrigger2,
    gilrs::Button::Select,
    gilrs::Button::Start,
    gilrs::Button::Mode,
    gilrs::Button::LeftThumb,
    gilrs::Button::RightThumb,
    gilrs::Button::DPadUp,
    gilrs::Button::DPadDown,
    gilrs::Button::DPadLeft,
    gilrs::Button::DPadRight,
];

impl FrontShell {
    pub(super) fn reset_control_device(&mut self) {
        if let Some(controls) = self.controls.as_mut() {
            controls.ready = false;
            controls.controller = None;
        }
    }

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
            controls: None,
            sounds: game::sounds::SoundLib::load(&game::shell_movie::shell_dir(assets_dir)),
            music: None,
        }
    }
}

impl App {
    fn control_labels(&self) -> Vec<(String, String)> {
        let mut rows = vec![
            ("Back".into(), String::new()),
            ("Restore defaults".into(), String::new()),
        ];
        for (trigger, bind) in &self.profile.settings.keyboard_bindings.table {
            rows.push((
                format!("Keyboard / mouse: {}", action_name(bind.bits, bind.walk)),
                format!("{trigger:?}"),
            ));
        }
        for (button, bits) in &self.profile.settings.controller_bindings.table {
            rows.push((
                format!("Controller: {}", action_name(*bits, (0, 0))),
                format!("{button:?}"),
            ));
        }
        rows
    }

    fn save_controls(&mut self) {
        self.kbm.follow(&self.profile.settings);
        self.kbm.release_keys();
        if let Some(g) = self.gilrs.as_ref() {
            self.pad_warning = input::log_pads_with(g, &self.profile.settings.controller_bindings);
        }
        if let Err(e) = self.profile.save() {
            eprintln!("profile autosave failed: {e:#}");
        }
    }

    fn activate_control_row(&mut self, front: &mut FrontShell, row: usize) {
        if let Some(controls) = front.controls.as_mut() {
            controls.cursor = row;
        }
        match row {
            0 => front.controls = None,
            1 => {
                self.profile.settings.keyboard_bindings = game::kbm::Bindings::default();
                self.profile.settings.controller_bindings = input::Mapping::default();
                self.sprint_latch = input::SprintLatch::default();
                if let Some(controls) = front.controls.as_mut() {
                    controls.message = Some("Default bindings restored".into());
                }
                self.save_controls();
            }
            _ => {
                if let Some(controls) = front.controls.as_mut() {
                    controls.message = None;
                    controls.capture = Some(row);
                    controls.ready = false;
                    controls.controller = None;
                }
            }
        }
    }

    pub(super) fn capture_keyboard(&mut self, event: &winit::event::WindowEvent) -> bool {
        use game::kbm::Trigger;
        use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
        use winit::keyboard::{KeyCode, PhysicalKey};
        let Some(controls) = self.front.as_mut().and_then(|f| f.controls.as_mut()) else {
            return false;
        };
        let Some(row) = controls.capture else {
            return false;
        };
        if matches!(event, WindowEvent::Focused(false)) {
            controls.capture = None;
            controls.message = Some("Binding change cancelled on focus loss".into());
            return false;
        }
        let count = self.profile.settings.keyboard_bindings.table.len();
        if row < CONTROL_COMMAND_ROWS || row >= CONTROL_COMMAND_ROWS + count {
            if matches!(event, WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && event.physical_key == PhysicalKey::Code(KeyCode::Escape))
            {
                controls.capture = None;
                controls.message = Some("Binding change cancelled".into());
                return true;
            }
            return false;
        }
        let trigger = match event {
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        controls.capture = None;
                        controls.message = Some("Binding change cancelled".into());
                        return true;
                    }
                    PhysicalKey::Code(code) => Some(Trigger::Key(code)),
                    _ => None,
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => Some(Trigger::Mouse(*button)),
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                if y > 0.0 {
                    Some(Trigger::WheelUp)
                } else if y < 0.0 {
                    Some(Trigger::WheelDown)
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(trigger) = trigger {
            let index = row - CONTROL_COMMAND_ROWS;
            let bindings = &mut self.profile.settings.keyboard_bindings;
            let swapped = bindings
                .table
                .iter()
                .enumerate()
                .find(|(i, (t, _))| *i != index && *t == trigger)
                .map(|(_, (_, bind))| action_name(bind.bits, bind.walk));
            if !bindings.remap(index, trigger) {
                controls.message =
                    Some("Mouse wheel cannot hold movement; choose a key or mouse button".into());
                return true;
            }
            controls.message = Some(swapped.map_or_else(
                || "Binding saved".into(),
                |name| format!("Binding saved; swapped with {name}"),
            ));
            controls.capture = None;
            self.save_controls();
        }
        matches!(event, WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed)
            || matches!(
                event,
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                } | WindowEvent::MouseWheel { .. }
            )
    }

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
        let mut controls_model = None;
        if front.setup {
            if input.back {
                front.setup = false;
            } else {
                actions = ui::shell::navigate(&mut self.shell, input);
                self.seed_shell_from_map();
            }
        } else if let Some(controls) = front.controls.as_mut() {
            let count = CONTROL_COMMAND_ROWS
                + self.profile.settings.keyboard_bindings.table.len()
                + self.profile.settings.controller_bindings.table.len();
            let mut activate = None;
            if let Some(row) = controls.capture {
                let keyboard_count = self.profile.settings.keyboard_bindings.table.len();
                let controller_row = row >= CONTROL_COMMAND_ROWS + keyboard_count;
                let pad = self.gilrs.as_ref().and_then(|g| g.gamepads().next());
                if let Some((id, pad)) = pad {
                    if controls.controller != Some(id) {
                        controls.controller = Some(id);
                        controls.ready = false;
                    }
                    let button = controls.capture_button(|b| input::is_down(&pad, b));
                    if controller_row {
                        if let Some(button) = button {
                            let index = row - CONTROL_COMMAND_ROWS - keyboard_count;
                            let bindings = &mut self.profile.settings.controller_bindings;
                            let swapped = bindings
                                .table
                                .iter()
                                .enumerate()
                                .find(|(i, (b, _))| *i != index && *b == button)
                                .map(|(_, (_, bits))| action_name(*bits, (0, 0)));
                            bindings.remap(index, button);
                            controls.capture = None;
                            controls.message = Some(swapped.map_or_else(
                                || "Binding saved".into(),
                                |name| format!("Binding saved; swapped with {name}"),
                            ));
                            self.sprint_latch = input::SprintLatch::default();
                            self.save_controls();
                        }
                    }
                } else if controller_row {
                    controls.capture = None;
                    controls.message = Some("Controller disconnected; binding unchanged".into());
                }
            } else {
                if input.up {
                    controls.cursor = (controls.cursor + count - 1) % count;
                }
                if input.down {
                    controls.cursor = (controls.cursor + 1) % count;
                }
                if input.back {
                    controls.cursor = 0;
                }
                if input.confirm || input.back {
                    activate = Some(controls.cursor);
                }
            }
            if let Some(row) = activate {
                self.activate_control_row(&mut front, row);
            }
            controls_model = front.controls.as_ref().map(|c| {
                (
                    self.control_labels(),
                    c.cursor,
                    c.capture.is_some(),
                    c.message.clone(),
                )
            });
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
                Some(ui::shell_ps2::ShellCommand::Controls) => {
                    front.controls = Some(Controls {
                        cursor: 0,
                        capture: None,
                        ready: false,
                        controller: None,
                        message: None,
                    });
                }
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
        front.menus.covered = front.setup || front.settings.is_some() || front.controls.is_some();
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
        let mut control_clicked = None;
        let mut setting_clicked = Vec::new();
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
                    setting_clicked = ui::menu::draw_menu(ctx, m);
                }
                if let Some((labels, cursor, capture, message)) = &controls_model {
                    control_clicked =
                        ui::menu::draw_controls(ctx, labels, *cursor, *capture, message.as_deref());
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
        if let (Some(row), Some(controls)) = (control_clicked, front.controls.as_mut()) {
            if controls.capture.is_none() {
                self.activate_control_row(&mut front, row);
            }
        }
        for action in setting_clicked {
            self.apply_shell_setting(&mut front, action);
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
            MenuAction::RequestRespawn | MenuAction::CancelRespawn | MenuAction::Respawn => return,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn controls(row: usize) -> Controls {
        Controls {
            cursor: row,
            capture: Some(row),
            ready: false,
            controller: None,
            message: None,
        }
    }

    #[test]
    fn start_cancels_either_device_before_accept_or_release_gate() {
        for row in [
            CONTROL_COMMAND_ROWS,
            CONTROL_COMMAND_ROWS + game::kbm::Bindings::default().table.len(),
        ] {
            for ready in [false, true] {
                let mut c = controls(row);
                c.ready = ready;
                assert_eq!(
                    c.capture_button(|b| matches!(b, gilrs::Button::Start | gilrs::Button::South)),
                    None
                );
                assert_eq!(c.capture, None);
                assert_eq!(c.message.as_deref(), Some("Binding change cancelled"));
            }
        }
    }

    #[test]
    fn held_accept_must_release_before_controller_binding() {
        let mut c = controls(CONTROL_COMMAND_ROWS + game::kbm::Bindings::default().table.len());
        assert_eq!(c.capture_button(|b| b == gilrs::Button::South), None);
        assert!(!c.ready);
        assert_eq!(c.capture_button(|_| false), None);
        assert!(c.ready);
        assert_eq!(
            c.capture_button(|b| b == gilrs::Button::West),
            Some(gilrs::Button::West)
        );
    }

    #[test]
    fn rejected_wheel_binding_explains_rejection_and_focus_loss_cancels() {
        let setup = crate::BattleSetup {
            seed: 42,
            difficulty: Default::default(),
            spawn: (0.0, 0.0),
            spawn_set: false,
            targets: false,
            ai: None,
            ai_cap: None,
            tickets: 250,
            tickets_set: false,
            ticket_pct: 100,
            time_limit: 0,
            cheats: Default::default(),
            teams: None,
            classes_dir: Default::default(),
            mission: None,
            mission_file: None,
            player_team: 0,
        };
        let mut app = App::new(
            Default::default(),
            Default::default(),
            setup,
            None,
            None,
            None,
            Default::default(),
        );
        let mut front = FrontShell::new(std::path::Path::new("__no_assets_for_capture_test__"));
        front.controls = Some(controls(CONTROL_COMMAND_ROWS));
        app.front = Some(front);
        let before = app.profile.clone();
        assert!(
            app.capture_keyboard(&winit::event::WindowEvent::MouseWheel {
                device_id: winit::event::DeviceId::dummy(),
                delta: winit::event::MouseScrollDelta::LineDelta(0.0, 1.0),
                phase: winit::event::TouchPhase::Moved,
            })
        );
        assert_eq!(app.profile, before);
        let c = app.front.as_ref().unwrap().controls.as_ref().unwrap();
        assert!(c.capture.is_some());
        assert!(c
            .message
            .as_deref()
            .unwrap()
            .contains("wheel cannot hold movement"));
        assert!(!app.capture_keyboard(&winit::event::WindowEvent::Focused(false)));
        assert!(app
            .front
            .as_ref()
            .unwrap()
            .controls
            .as_ref()
            .unwrap()
            .capture
            .is_none());
        assert_eq!(app.profile, before);
    }
}
