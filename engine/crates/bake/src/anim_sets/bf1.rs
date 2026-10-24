//! BF1's soldier animation set, `FUN_0022d828` (see `super`): the six tables
//! and their fallbacks, a port of `lead_tmp/anim/sets_bf1.py`.

use super::{AnimBank, ClipRef, Inventory};
pub use manifest::anim_sets::bf1::{weapon_set, ACTIONS, SPECIAL, STANCES, UPPER, WEAPONS};

/// Each weapon set's fallback set (`DAT_00535598`): bazooka -> rifle,
/// pistol -> tool, grenade -> tool, tool -> pistol, sabre -> tool,
/// sbdroid -> tool, bdroid -> rifle, lascannon -> rifle.
pub const FALLBACK: [usize; 9] = [0, 0, 4, 4, 2, 4, 4, 0, 0];
/// The six banks, in the order a name is looked up.
pub const BANKS: [&str; 6] = [
    "human_animation_0",
    "human_animation_1",
    "human_animation_2",
    "human_animation_3",
    "human_animation_4",
    "human_animation_alert",
];

type Slot = Option<ClipRef>;

/// The six tables, every fallback applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tables {
    /// `[stance][action]`: `human_rifle_<stance>_<action>` — the LOWER
    /// body's clips, whatever the weapon.
    pub base: Vec<Vec<Slot>>,
    /// `[special]`: `human_rifle_<special>`.
    pub special: Vec<Slot>,
    /// `[stance][weapon][action]`: the upper body while no action runs.
    pub w: Vec<Vec<Vec<Slot>>>,
    /// `[weapon][action]`: stand, alert.
    pub alert: Vec<Vec<Slot>>,
    /// `[stance][weapon][upper action]`.
    pub u: Vec<Vec<Vec<Slot>>>,
    /// `[weapon][special]`.
    pub s: Vec<Vec<Slot>>,
}

/// BF1's inventory: the six `human_animation` banks of `COMMON.LVL`, in
/// lookup order.
pub fn inventory(banks: &[AnimBank<'_>]) -> Inventory {
    Inventory::new(
        BANKS
            .iter()
            .filter_map(|n| banks.iter().find(|b| b.name.eq_ignore_ascii_case(n))),
    )
}

pub fn build(inv: &Inventory) -> Tables {
    build_with(inv, &FALLBACK)
}

/// [`build`] with another fallback table (a test breaks the rule with it).
// The tables are walked by index in the builder's own order, several at once.
#[allow(clippy::needless_range_loop)]
pub fn build_with(inv: &Inventory, fallback: &[usize; 9]) -> Tables {
    let mut base: Vec<Vec<Slot>> = STANCES
        .iter()
        .map(|s| {
            ACTIONS
                .iter()
                .map(|a| inv.find(&format!("human_rifle_{s}_{a}")))
                .collect()
        })
        .collect();
    let special: Vec<Slot> = SPECIAL
        .iter()
        .map(|n| inv.find(&format!("human_rifle_{n}")))
        .collect();
    // B1 144405-144441: a stance's idle_emote falls back to stand's, its
    // idle variants and turns to that stance's idle_emote.
    for s in 0..3 {
        if base[s][0].is_none() {
            base[s][0] = base[0][0].clone();
        }
        for a in 1..=4 {
            if base[s][a].is_none() {
                base[s][a] = base[s][0].clone();
            }
        }
    }
    let mut w = vec![vec![vec![None; 13]; 9]; 3];
    let mut alert = vec![vec![None; 13]; 9];
    let mut u = vec![vec![vec![None; 6]; 9]; 3];
    let mut sp = vec![vec![None; 31]; 9];
    for (wi, wn) in WEAPONS.iter().enumerate() {
        for (s, sn) in STANCES.iter().enumerate() {
            for (a, an) in ACTIONS.iter().enumerate() {
                let name = format!("human_{wn}_{sn}_{an}");
                w[s][wi][a] = inv.find(&name);
                if s == 0 {
                    // The bazooka's alert row is its plain row (144484).
                    alert[wi][a] = if wi == 1 {
                        w[0][1][a].clone()
                    } else {
                        inv.find(&format!("{name}_alert"))
                    };
                }
            }
            for (k, un) in UPPER.iter().enumerate() {
                u[s][wi][k] = inv.find(&format!("human_{wn}_{sn}_{un}"));
            }
        }
        for (i, n) in SPECIAL.iter().enumerate() {
            sp[wi][i] = inv.find(&format!("human_{wn}_{n}"));
        }
    }
    // 144543-144602: every empty slot takes the fallback set's, as that
    // stands when this set's turn comes (w = 0..8).
    for wi in 0..9 {
        let f = fallback[wi];
        if f == wi {
            continue;
        }
        for s in 0..3 {
            fill_missing(&mut w[s], wi, f);
            fill_missing(&mut u[s], wi, f);
        }
        fill_missing(&mut alert, wi, f);
        fill_missing(&mut sp, wi, f);
    }
    // 144603-144620: crouch grenade shoot / shoot2 / shoot3 <- stand's.
    for k in 1..=3 {
        if u[1][3][k].is_none() {
            u[1][3][k] = u[0][3][k].clone();
        }
    }
    // 144621-144686: the three shoots share the first that exists; what
    // is still empty takes the rifle's, an upper action then the one
    // before it.
    for wi in 0..9 {
        for s in 0..3 {
            let first = (1..=3).find_map(|k| u[s][wi][k].clone());
            for k in 1..=3 {
                if u[s][wi][k].is_none() {
                    u[s][wi][k] = first.clone();
                }
            }
            fill_missing(&mut w[s], wi, 0);
            for k in 0..6 {
                if u[s][wi][k].is_none() {
                    u[s][wi][k] = u[s][0][k].clone();
                }
                if k > 0 && u[s][wi][k].is_none() {
                    u[s][wi][k] = u[s][wi][k - 1].clone();
                }
            }
        }
        fill_missing(&mut alert, wi, 0);
        fill_missing(&mut sp, wi, 0);
    }
    Tables {
        base,
        special,
        w,
        alert,
        u,
        s: sp,
    }
}

fn fill_missing(rows: &mut [Vec<Slot>], destination: usize, source: usize) {
    for k in 0..rows[destination].len() {
        if rows[destination][k].is_none() {
            rows[destination][k] = rows[source][k].clone();
        }
    }
}

/// A movement record's blend seconds: 0.15, prone 0.4 (§2.1).
pub fn move_blend(stance: usize) -> f32 {
    if stance == 2 {
        0.4
    } else {
        0.15
    }
}

/// A movement record loops, but for turnleft / turnright.
pub fn move_loops(action: usize) -> bool {
    !(3..=4).contains(&action)
}

/// An upper action's blend seconds as the code has them: the three shoots
/// 0.02 (kept as the byte 1, 0.01 s), idle / charge / reload 0.5. None
/// loops.
pub fn upper_blend(upper: usize) -> f32 {
    if (1..=3).contains(&upper) {
        0.02
    } else {
        0.5
    }
}

/// A special animation's (loop, blend seconds) — the pair table
/// `DAT_005354a0` as §2.1 lists it.
pub fn special_blend(i: usize) -> (bool, f32) {
    match SPECIAL[i] {
        "jump" => (false, 0.4),
        "landsoft" | "landhard" => (false, 0.05),
        "fall" => (false, 0.8),
        "thrown_flail" => (true, 0.5),
        "thrown_tumblefront" | "thrown_tumbleback" => (true, 0.1),
        "thrown_bouncefrontsoft" | "thrown_bouncebacksoft" => (false, 0.1),
        "thrown_landfrontsoft" | "thrown_landbacksoft" => (false, 0.4),
        "jetpack_hover" => (true, 0.15),
        _ => (false, 0.15),
    }
}
