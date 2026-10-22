//! The hidden dev mode (card SQMK): it turns on what normal play hides —
//! today the in-world squad markers (`squad_vis::markers_on`) and the
//! playtest black box (card BLKB), later the debug overlays. It is in no menu.
//!
//! The maintainer, 2026-10-02: dev mode is ON BY DEFAULT while the game is unfinished;
//! `OPENBF_DEV=0` turns it off. When the game is nearly done, flip
//! [`DEFAULT_ON`] to `false` (then `OPENBF_DEV=1` turns it on again).
//! The maintainer, 2026-10-21: a packed build (card PACK: the player's game) is in dev
//! mode too while the game is unfinished, so its bug reports carry a black box.

/// What dev mode is when `OPENBF_DEV` is unset or empty.
pub const DEFAULT_ON: bool = true;

/// Whether `OPENBF_DEV`'s value asks for dev mode: `1` does, `0` does not,
/// unset or empty or anything else is [`DEFAULT_ON`].
pub fn flag(value: Option<&str>) -> bool {
    match value {
        Some("1") => true,
        Some("0") => false,
        _ => DEFAULT_ON,
    }
}

/// Dev mode, from the environment.
pub fn on() -> bool {
    flag(std::env::var("OPENBF_DEV").ok().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_mode_follows_the_default_and_the_env() {
        assert_eq!(flag(None), DEFAULT_ON);
        assert_eq!(flag(Some("")), DEFAULT_ON);
        assert!(!flag(Some("0")));
        assert!(flag(Some("1")));
    }
}
