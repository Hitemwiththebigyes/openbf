//! Controller input: gilrs -> deterministic `core::sim::PadState`.
//!
//! The sim consumes quantized i16 axes + logical button bits only (replay
//! logs are bit-exact); this crate owns the physical mapping. Default layout
//! mirrors the PS2 originals per the locked design (PLAN §Platform & meta:
//! controller-only, PS2 default mapping, full remap later):
//!   left stick move / right stick aim, R2(RightTrigger2) fire,
//!   L2(LeftTrigger2) secondary, Cross(South) jump, Circle(East)
//!   roll/stance (contextual), L3(LeftThumb) sprint (one click, see
//!   [`SprintLatch`]), Square(West) reload, Triangle(North) action,
//!   R1(RightTrigger) switch weapon, L1(LeftTrigger) switch secondary,
//!   R3(RightThumb) zoom (the maintainer, 2026-10-04: the PS2's shoulder layout with
//!   fire on the triggers; a press steps the zoom of the weapon in hand, card
//!   ZOOM).
//!
//! The spawn screen's class select is PROVISIONAL (lead placeholder, no RE):
//! R1 (`RightTrigger`) next class and Select previous. The PS2's own
//! spawn-screen buttons are UNTRACED.
//!
//! The spawn screens' accept and back (card SSELA, `SPAWN_ACCEPT` /
//! `SPAWN_BACK`) are the PS2's X and Triangle (re-spawn-flow §1): Cross and
//! Triangle carry them beside JUMP and ACTION; the sim reads them only from a
//! slot that is not on the field.
//!
//! The D-PAD is the squad commands' (Phase D, card SQACK) and, while the
//! player is dead, the spawn map's post push (card SPSEL, `SPAWN_POST_*`): the
//! sim reads the squad bits only from a live player and the post bits only
//! from a dead one. The pause menu still reads it UI-side; a paused player's
//! pad reaches the sim as idle.
//!
//! Pads (card PADM): every `Gilrs` comes from [`open_gilrs`], which adds the
//! project's own SDL mappings (`pads.txt`, pads the SDL_GameControllerDB
//! lacks — the maintainer's GameSir G7 Pro first) to gilrs's bundled DB, and
//! [`log_pads`] prints how each connected pad resolved and returns an
//! on-screen warning when the camera stick or FIRE maps to nothing.

use core::sim::{buttons, PadState};
use gilrs::Button;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Mapping {
    pub table: Vec<(Button, u32)>,
}

impl Mapping {
    pub fn remap(&mut self, index: usize, button: Button) {
        if index >= self.table.len() {
            return;
        }
        if let Some(other) = self.table.iter().position(|(b, _)| *b == button) {
            self.table[other].0 = self.table[index].0;
        }
        self.table[index].0 = button;
    }

    /// The default layout the module header describes.
    pub fn ps2_default() -> Mapping {
        Mapping {
            table: vec![
                (Button::RightTrigger2, buttons::FIRE),
                (Button::LeftTrigger2, buttons::SECONDARY_FIRE),
                // X / Triangle are also the spawn screens' accept / back
                // (card SSELA), read by the sim only off the field.
                (Button::South, buttons::JUMP | buttons::SPAWN_ACCEPT),
                (Button::East, buttons::ROLL_STANCE),
                (Button::LeftThumb, buttons::SPRINT),
                (Button::LeftTrigger, buttons::SWITCH_SECONDARY),
                (Button::West, buttons::RELOAD),
                (Button::North, buttons::ACTION | buttons::SPAWN_BACK),
                (Button::RightThumb, buttons::ZOOM),
                // The four squad orders, on the PS2's D-pad arms (card
                // SQACK, re-squad-orders.md section 1). The same arms move
                // the spawn map's post while dead (card SPSEL, as the PS2's
                // spawn map reads its D-pad): the sim reads squad bits only
                // alive and post bits only dead.
                (
                    Button::DPadUp,
                    buttons::SQUAD_MOVE_OUT | buttons::SPAWN_POST_UP,
                ),
                (
                    Button::DPadDown,
                    buttons::SQUAD_HOLD | buttons::SPAWN_POST_DOWN,
                ),
                (
                    Button::DPadLeft,
                    buttons::SQUAD_AT_EASE | buttons::SPAWN_POST_LEFT,
                ),
                (
                    Button::DPadRight,
                    buttons::SQUAD_FOLLOW | buttons::SPAWN_POST_RIGHT,
                ),
                // The spawn-class bits are PROVISIONAL (lead placeholder, no
                // RE) and read by the sim only while the player is dead, so
                // R1 carries both.
                (
                    Button::RightTrigger,
                    buttons::SPAWN_CLASS_NEXT | buttons::SWITCH_WEAPON,
                ),
                (Button::Select, buttons::SPAWN_CLASS_PREV),
            ],
        }
    }
}

impl Default for Mapping {
    fn default() -> Self {
        Mapping::ps2_default()
    }
}

/// Per-axis deadzone applied before quantization. PROVISIONAL and ours: no
/// PS2 value has been traced (the DualShock 2 deadzone lives in the game's
/// pad code, untouched by the RE so far); 0.15 is a common modern-pad
/// default. Settled by an `analysis/` citation or the maintainer's feel check.
pub const DEADZONE: f32 = 0.15;

/// Left-stick travel that reads as a full push (card SPUP, ours, FEEL_NOTES):
/// the stick is scaled by 1 / this and clamped. The maintainer's pad peaks at 0.984
/// (32255 in his snapshots of 2026-10-04/05), which ran his soldier at 6.89
/// of 7.0 m/s and, up Kamino's 14.8 deg ramp, at 0.890 x MaxSpeed where the
/// PS2's full stick gives 0.904 - under the sprint's 0.9 floor. The look
/// stick keeps its own scale ([`LOOK_SCALE`]).
pub const STICK_FULL: f32 = 0.95;

/// An analog trigger (`RightTrigger2` / `LeftTrigger2` mapped from an axis)
/// counts as pressed at this travel. PROVISIONAL and ours (FEEL_NOTES): the
/// PS2 reads L2/R2 as digital buttons [H — the pad-read code is not RE'd],
/// so any threshold is a modern-pad choice; a digital trigger reads 1.0.
pub const TRIGGER_PRESS: f32 = 0.5;

/// The right stick's scale before quantization: the player's aim turns at
/// this share of the class's turn rate at full stick. Ours (the maintainer, 2026-10-04,
/// FEEL_NOTES: the camera was too sensitive); bots do not come through here.
pub const LOOK_SCALE: f32 = 0.80;

/// Calls of [`SprintLatch::apply`] a click waits for the sim to start the
/// sprint (it needs 0.9 x MaxSpeed first) before it is dropped: 1.5 s at 60.
pub const SPRINT_WAIT: u32 = 90;

/// Forward stick (quantized `ly`) under which a latched sprint is dropped.
pub const SPRINT_KEEP_LY: i16 = 16384;

/// One click of L3 sprints (the maintainer, 2026-10-04, ours: the sim's sprint is
/// hold-to-sprint, and holding a stick click while pushing the stick is
/// awkward). The latch holds the SPRINT bit after a click until the sim's
/// sprint ends (energy, the stick's cone), the stick leaves forward, FIRE is
/// pressed, a second click, or [`SPRINT_WAIT`] passes with no sprint started.
/// Holding L3 still sprints as before. On foot only: in a seat SPRINT is the
/// boost and passes through.
#[derive(Clone, Copy, Debug, Default)]
pub struct SprintLatch {
    on: bool,
    prev_down: bool,
    seen: bool,
    age: u32,
}

impl SprintLatch {
    /// `pad` as sampled; `on_foot` and `sprinting` are the player's soldier
    /// before this tick. Call once per sampled pad.
    pub fn apply(&mut self, mut pad: PadState, on_foot: bool, sprinting: bool) -> PadState {
        let down = pad.buttons & buttons::SPRINT != 0;
        let edge = down && !self.prev_down;
        self.prev_down = down;
        if !on_foot {
            self.on = false;
            return pad;
        }
        if edge {
            self.on = !self.on;
            self.seen = false;
            self.age = 0;
        }
        if self.on {
            self.age += 1;
            self.seen |= sprinting;
            if (self.seen && !sprinting && !edge)
                || (!self.seen && self.age > SPRINT_WAIT)
                || pad.ly < SPRINT_KEEP_LY
                || pad.buttons & buttons::FIRE != 0
            {
                self.on = false;
            }
        }
        if self.on {
            pad.buttons |= buttons::SPRINT;
        }
        pad
    }
}

/// The project's SDL mappings, one line per pad (see the file's header).
pub const PADS_TXT: &str = include_str!("../pads.txt");

/// The one way the game opens gilrs. Mapping precedence (gilrs 0.11
/// `GilrsBuilder::build` inserts in this order, later wins per GUID):
/// `pads.txt` < gilrs's bundled SDL_GameControllerDB < `SDL_GAMECONTROLLERCONFIG`.
/// So `pads.txt` fills gaps in the DB; the env var still overrides anything.
/// The error is gilrs's, as text (its `Error` carries a whole dummy `Gilrs`).
pub fn open_gilrs() -> Result<gilrs::Gilrs, String> {
    gilrs::GilrsBuilder::new()
        .add_mappings(PADS_TXT)
        .build()
        .map_err(|e| e.to_string())
}

/// The SDL GUID string of a pad (the key of a `pads.txt` / DB line).
pub fn guid(pad: &gilrs::Gamepad<'_>) -> String {
    pad.uuid().iter().map(|b| format!("{b:02x}")).collect()
}

/// The logical controls a pad leaves unmapped: the sticks by axis name and
/// every button of `map` by button name.
pub fn unresolved(pad: &gilrs::Gamepad<'_>, map: &Mapping) -> Vec<String> {
    use gilrs::Axis;
    let mut out = Vec::new();
    for a in [
        Axis::LeftStickX,
        Axis::LeftStickY,
        Axis::RightStickX,
        Axis::RightStickY,
    ] {
        if pad.axis_code(a).is_none() {
            out.push(format!("{a:?}"));
        }
    }
    for &(b, _) in &map.table {
        if pad.button_code(b).is_none() {
            out.push(format!("{b:?}"));
        }
    }
    out
}

/// The warning for a pad whose camera stick or FIRE resolves to nothing
/// (from [`unresolved`]'s list), or `None` when both work.
pub fn pad_warning(name: &str, guid: &str, missing: &[String]) -> Option<String> {
    let dead: Vec<&str> = missing
        .iter()
        .map(String::as_str)
        .filter(|m| matches!(*m, "RightStickX" | "RightStickY" | "RightTrigger2"))
        .collect();
    (!dead.is_empty()).then(|| {
        format!(
            "controller '{name}' has no mapping for {} — camera/fire will not work; \
             add an SDL line for GUID {guid} to crates/input/pads.txt or SDL_GAMECONTROLLERCONFIG",
            dead.join(", ")
        )
    })
}

/// Print one line per connected pad (name, GUID, mapping source, what did
/// not resolve) — "0 pads" when none — and return the first pad's warning
/// ([`pad_warning`]), which the caller also shows on screen.
pub fn log_pads(g: &gilrs::Gilrs) -> Option<String> {
    let map = Mapping::ps2_default();
    let mut n = 0;
    let mut warning = None;
    for (_, pad) in g.gamepads() {
        n += 1;
        let (name, id) = (pad.name(), guid(&pad));
        let missing = unresolved(&pad, &map);
        println!(
            "pad {n}: '{name}' guid {id} mapping {:?} ({}); unresolved: {}",
            pad.mapping_source(),
            pad.map_name().unwrap_or("no SDL entry"),
            if missing.is_empty() {
                "none".to_string()
            } else {
                missing.join(" ")
            }
        );
        if let Some(w) = pad_warning(name, &id, &missing) {
            eprintln!("WARNING: {w}");
            warning.get_or_insert(w);
        }
    }
    if n == 0 {
        println!("0 pads connected — controls need one (gilrs)");
    }
    warning
}

/// Drain gilrs's event queue (keeping the cached state fresh); true when a
/// pad connected or disconnected, so the caller re-runs [`log_pads`].
pub fn pump(g: &mut gilrs::Gilrs) -> bool {
    let mut changed = false;
    while let Some(ev) = g.next_event() {
        changed |= matches!(
            ev.event,
            gilrs::EventType::Connected | gilrs::EventType::Disconnected
        );
    }
    changed
}

/// A mapped button is down: pressed, or (an analog trigger) past
/// [`TRIGGER_PRESS`] — gilrs's own axis-to-button press is at 0.75.
pub fn is_down(pad: &gilrs::Gamepad<'_>, b: Button) -> bool {
    pad.is_pressed(b)
        || (matches!(b, Button::RightTrigger2 | Button::LeftTrigger2)
            && pad
                .button_data(b)
                .is_some_and(|d| d.value() >= TRIGGER_PRESS))
}

fn dz(v: f32) -> f32 {
    if v.abs() < DEADZONE {
        0.0
    } else {
        v
    }
}

/// A left-stick axis: the deadzone, then [`STICK_FULL`] reads as 1.
fn move_axis(v: f32) -> f32 {
    (dz(v) / STICK_FULL).clamp(-1.0, 1.0)
}

/// Sample one gamepad into a PadState through the DEFAULT mapping. Pump
/// `gilrs.next_event()` before calling so cached axis state is fresh.
pub fn pad_from_gamepad(pad: &gilrs::Gamepad<'_>) -> PadState {
    pad_from_gamepad_with(pad, &Mapping::ps2_default())
}

/// Card (the maintainer, 2026-10-20): the left stick works the menus and the spawn map
/// like the D-pad. A push past this reads as an arm; one already held stays
/// until the stick falls under [`STICK_ARM_OFF`], so a wobble near the edge
/// does not click twice.
pub const STICK_ARM_ON: f32 = 0.6;
pub const STICK_ARM_OFF: f32 = 0.35;
/// The D-pad arm bits a stick position stands for (1 up, 2 down, 4 left,
/// 8 right: the menu's edge bits): the dominant axis only, so a diagonal
/// never presses two arms. `held` is the arms read last frame (hysteresis);
/// +y is up, as gilrs reports it.
pub fn stick_arms(x: f32, y: f32, held: u32) -> u32 {
    let (ax, ay) = (x.abs(), y.abs());
    let (v, along_x) = if ax >= ay { (ax, true) } else { (ay, false) };
    let arm = match (along_x, if along_x { x } else { y } > 0.0) {
        (true, true) => 8,
        (true, false) => 4,
        (false, true) => 1,
        (false, false) => 2,
    };
    if v >= STICK_ARM_ON || (held & arm != 0 && v >= STICK_ARM_OFF) {
        arm
    } else {
        0
    }
}

/// [`pad_from_gamepad`] through a given [`Mapping`].
pub fn pad_from_gamepad_with(pad: &gilrs::Gamepad<'_>, map: &Mapping) -> PadState {
    pad_from_gamepad_with_arms(pad, map, 0).0
}

/// Sample with the previous gameplay stick arms, returning the new latch.
/// The caller owns controller selection and resets `held` on disconnect/change.
/// Stateless samplers above remain available to one-shot consumers.
pub fn pad_from_gamepad_with_arms(
    pad: &gilrs::Gamepad<'_>,
    map: &Mapping,
    held: u32,
) -> (PadState, u32) {
    use gilrs::Axis;
    let mut b = 0u32;
    for &(gb, bit) in &map.table {
        if is_down(pad, gb) {
            b |= bit;
        }
    }

    // The left stick also moves the spawn map's post while dead (the sim
    // reads post bits only dead; alive the stick walks).
    let arms = stick_arms(
        pad.value(Axis::LeftStickX),
        pad.value(Axis::LeftStickY),
        held,
    );
    for (arm, bit) in [
        (1, buttons::SPAWN_POST_UP),
        (2, buttons::SPAWN_POST_DOWN),
        (4, buttons::SPAWN_POST_LEFT),
        (8, buttons::SPAWN_POST_RIGHT),
    ] {
        if arms & arm != 0 {
            b |= bit;
        }
    }

    (
        PadState {
            lx: PadState::quantize(move_axis(pad.value(Axis::LeftStickX))),
            ly: PadState::quantize(move_axis(pad.value(Axis::LeftStickY))),
            rx: PadState::quantize(dz(pad.value(Axis::RightStickX)) * LOOK_SCALE),
            ry: PadState::quantize(dz(pad.value(Axis::RightStickY)) * LOOK_SCALE),
            buttons: b,
            ..PadState::default()
        },
        arms,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remapping_swaps_conflicting_buttons_without_duplicate_actions() {
        let mut mapping = Mapping::default();
        let fire = mapping.table[0];
        let reload = mapping
            .table
            .iter()
            .position(|(button, _)| *button == Button::West)
            .unwrap();
        let old_reload = mapping.table[reload];
        mapping.remap(0, Button::West);
        assert_eq!(mapping.table[0], (Button::West, fire.1));
        assert_eq!(mapping.table[reload], (fire.0, old_reload.1));
        assert_eq!(
            mapping
                .table
                .iter()
                .filter(|(button, _)| *button == Button::West)
                .count(),
            1
        );
    }

    #[test]
    fn the_left_stick_reads_as_the_dpad_arms() {
        assert_eq!(stick_arms(0.0, 1.0, 0), 1, "up");
        assert_eq!(stick_arms(0.0, -1.0, 0), 2, "down");
        assert_eq!(stick_arms(-1.0, 0.0, 0), 4, "left");
        assert_eq!(stick_arms(1.0, 0.0, 0), 8, "right");
        assert_eq!(stick_arms(0.0, 0.0, 0), 0, "centred");
        assert_eq!(stick_arms(0.5, 0.0, 0), 0, "a half push is not a click");
        // A diagonal presses its stronger axis only, never two arms.
        assert_eq!(stick_arms(0.7, 0.9, 0), 1);
        assert_eq!(stick_arms(0.9, 0.7, 0), 8);
        // Hysteresis: a held arm stays down to 0.35 and a new one needs 0.6.
        assert_eq!(stick_arms(0.0, 0.45, 1), 1);
        assert_eq!(stick_arms(0.0, 0.45, 0), 0);
        assert_eq!(stick_arms(0.0, 0.3, 1), 0);
    }

    fn fwd(buttons: u32) -> PadState {
        PadState {
            ly: 32767,
            buttons,
            ..PadState::default()
        }
    }

    /// One click sprints until the sim's sprint ends; the stick, FIRE, a
    /// second click and a sprint that never starts each drop it; a seat's
    /// boost is not latched.
    #[test]
    fn a_sprint_click_latches_until_the_sprint_ends() {
        use buttons::{FIRE, SPRINT};
        let on = |p: PadState| p.buttons & SPRINT != 0;
        let mut l = SprintLatch::default();
        assert!(on(l.apply(fwd(SPRINT), true, false)));
        assert!(
            on(l.apply(fwd(0), true, false)),
            "released, not yet sprinting"
        );
        assert!(on(l.apply(fwd(0), true, true)));
        assert!(!on(l.apply(fwd(0), true, false)), "the sim's sprint ended");

        let mut l = SprintLatch::default();
        l.apply(fwd(SPRINT), true, false);
        assert!(
            !on(l.apply(PadState::default(), true, true)),
            "stick let go"
        );

        let mut l = SprintLatch::default();
        l.apply(fwd(SPRINT), true, false);
        assert!(!on(l.apply(fwd(FIRE), true, true)), "FIRE");

        let mut l = SprintLatch::default();
        l.apply(fwd(SPRINT), true, false);
        l.apply(fwd(0), true, true);
        assert!(
            on(l.apply(fwd(SPRINT), true, true)),
            "held: still the button's"
        );
        assert!(
            !on(l.apply(fwd(0), true, true)),
            "the second click took it off"
        );

        let mut l = SprintLatch::default();
        l.apply(fwd(SPRINT), true, false);
        for _ in 0..SPRINT_WAIT {
            l.apply(fwd(0), true, false);
        }
        assert!(
            !on(l.apply(fwd(0), true, false)),
            "no sprint started in time"
        );

        let mut l = SprintLatch::default();
        assert!(
            on(l.apply(fwd(SPRINT), false, false)),
            "a seat's boost passes"
        );
        assert!(!on(l.apply(fwd(0), false, false)), "and is not latched");
    }

    /// PADM: the G7 Pro line in `pads.txt`, resolved the way SDL resolves a
    /// generic Linux HID pad (`bN` = the N-th key code the device reports,
    /// `aN` = the N-th abs axis) against the device Claude probed on the maintainer's
    /// pad: keys 304..=318, abs X Y Z RZ GAS BRAKE (+ hat 0). The camera is
    /// on ABS_Z/ABS_RZ, SDL's x/y (West/North) are 307/308, both triggers are
    /// analog, and every logical control the game reads has a source.
    #[test]
    fn the_g7_pro_line_resolves_camera_triggers_and_face_buttons() {
        let line = PADS_TXT
            .lines()
            .find(|l| l.starts_with("03000000373500002210000010010000,"))
            .expect("G7 Pro line in pads.txt");
        let field = |k: &str| {
            line.split(',')
                .find_map(|f| f.strip_prefix(k).and_then(|v| v.strip_prefix(':')))
                .unwrap_or_else(|| panic!("{k} missing"))
        };
        let keys: Vec<u16> = (304..=318).collect();
        let abs: [u16; 6] = [0x00, 0x01, 0x02, 0x05, 0x09, 0x0a]; // X Y Z RZ GAS BRAKE
        let key = |k: &str| {
            keys[field(k)
                .strip_prefix('b')
                .unwrap()
                .parse::<usize>()
                .unwrap()]
        };
        let axis = |k: &str| {
            abs[field(k)
                .strip_prefix('a')
                .unwrap()
                .parse::<usize>()
                .unwrap()]
        };
        assert_eq!((axis("rightx"), axis("righty")), (0x02, 0x05)); // ABS_Z, ABS_RZ
        assert_eq!((axis("leftx"), axis("lefty")), (0x00, 0x01));
        assert_eq!((key("x"), key("y")), (307, 308));
        assert_eq!((key("a"), key("b")), (304, 305));
        assert_eq!((axis("righttrigger"), axis("lefttrigger")), (0x09, 0x0a));
        for k in [
            "leftshoulder",
            "rightshoulder",
            "leftstick",
            "rightstick",
            "back",
            "start",
            "dpup",
            "dpdown",
            "dpleft",
            "dpright",
        ] {
            field(k);
        }
        assert!(line.ends_with("platform:Linux,"));
    }

    #[test]
    fn a_dead_camera_or_fire_warns_and_nothing_else_does() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(pad_warning("p", "g", &[]).is_none());
        assert!(pad_warning("p", "g", &s(&["Select", "LeftThumb"])).is_none());
        let w = pad_warning("G7", "0300", &s(&["RightStickX", "RightStickY"])).unwrap();
        assert!(
            w.contains("RightStickX, RightStickY") && w.contains("0300"),
            "{w}"
        );
        assert!(pad_warning("p", "g", &s(&["RightTrigger2"])).is_some());
    }

    #[test]
    fn deadzone_and_quantization_round_trip() {
        assert_eq!(dz(0.1), 0.0);
        assert_eq!(dz(-0.14), 0.0);
        assert_eq!(dz(0.2), 0.2);
        // A pad that tops out short of 1.0 still gives a full push (SPUP).
        assert_eq!(PadState::quantize(move_axis(0.984)), 32767);
        assert_eq!(PadState::quantize(move_axis(-0.96)), -32767);
        assert_eq!(move_axis(0.1), 0.0);
        assert!((move_axis(0.475) - 0.5).abs() < 1e-6);
        // Quantize/axis round-trip stays within one quantum.
        for v in [-1.0f32, -0.5, 0.0, 0.33, 0.999, 1.0] {
            let q = PadState::quantize(v);
            assert!((PadState::axis(q) - v).abs() < 1.0 / 32000.0, "{v}");
        }
        // Out-of-range hardware values clamp instead of wrapping.
        assert_eq!(PadState::quantize(1.7), 32767);
        assert_eq!(PadState::quantize(-1.7), -32767);
    }

    #[test]
    fn button_bits_are_distinct() {
        use core::sim::buttons::*;
        let all = [
            FIRE,
            JUMP,
            ROLL_STANCE,
            SPRINT,
            ZOOM,
            RELOAD,
            ACTION,
            SWITCH_WEAPON,
            SECONDARY_FIRE,
            SQUAD_FOLLOW,
            SQUAD_HOLD,
            SQUAD_AT_EASE,
            SQUAD_MOVE_OUT,
            SPAWN_CLASS_NEXT,
            SPAWN_CLASS_PREV,
            SWITCH_SECONDARY,
            SPAWN_POST_UP,
            SPAWN_POST_DOWN,
            SPAWN_POST_LEFT,
            SPAWN_POST_RIGHT,
            SPAWN_ACCEPT,
            SPAWN_BACK,
        ];
        let mut seen = 0u32;
        for b in all {
            assert_eq!(seen & b, 0, "overlapping bit {b:#x}");
            seen |= b;
        }
    }

    fn of_bits(m: &Mapping, gb: Button) -> Option<u32> {
        m.table.iter().find(|(b, _)| *b == gb).map(|(_, x)| *x)
    }

    /// D10: the default table binds every bit the sim reads, binds each
    /// PHYSICAL button once, and gives the d-pad to the four squad commands
    /// with `SWITCH_WEAPON` moved off it.
    #[test]
    fn the_default_mapping_binds_each_button_and_each_bit_once() {
        use core::sim::buttons::*;
        let m = Mapping::ps2_default();
        let mut bits = 0u32;
        let mut seen: Vec<Button> = Vec::new();
        for &(gb, bit) in &m.table {
            assert_eq!(bits & bit, 0, "bit {bit:#x} bound twice");
            assert!(!seen.contains(&gb), "{gb:?} bound twice");
            bits |= bit;
            seen.push(gb);
        }
        for bit in [
            FIRE,
            SECONDARY_FIRE,
            JUMP,
            ROLL_STANCE,
            SPRINT,
            ZOOM,
            RELOAD,
            ACTION,
            SWITCH_WEAPON,
            SQUAD_FOLLOW,
            SQUAD_HOLD,
            SQUAD_AT_EASE,
            SQUAD_MOVE_OUT,
            SPAWN_CLASS_NEXT,
            SPAWN_CLASS_PREV,
            SPAWN_POST_UP,
            SPAWN_POST_DOWN,
            SPAWN_POST_LEFT,
            SPAWN_POST_RIGHT,
            SPAWN_ACCEPT,
            SPAWN_BACK,
        ] {
            assert_ne!(bits & bit, 0, "nothing presses {bit:#x}");
        }
        assert_eq!(of_bits(&m, Button::South), Some(JUMP | SPAWN_ACCEPT));
        assert_eq!(of_bits(&m, Button::North), Some(ACTION | SPAWN_BACK));
        let of = |gb: Button| m.table.iter().find(|(b, _)| *b == gb).map(|(_, x)| *x);
        assert_eq!(of(Button::DPadUp), Some(SQUAD_MOVE_OUT | SPAWN_POST_UP));
        assert_eq!(of(Button::DPadDown), Some(SQUAD_HOLD | SPAWN_POST_DOWN));
        assert_eq!(of(Button::DPadLeft), Some(SQUAD_AT_EASE | SPAWN_POST_LEFT));
        assert_eq!(of(Button::DPadRight), Some(SQUAD_FOLLOW | SPAWN_POST_RIGHT));
        assert_eq!(of(Button::RightThumb), Some(ZOOM));
        assert_eq!(of(Button::LeftTrigger), Some(SWITCH_SECONDARY));
        assert_eq!(
            of(Button::RightTrigger),
            Some(SPAWN_CLASS_NEXT | SWITCH_WEAPON)
        );
    }
}
