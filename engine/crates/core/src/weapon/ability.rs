use super::{pf, PropSource};

#[derive(Clone, Debug, PartialEq)]
pub struct Ability {
    pub radius: f32,
    pub friends: bool,
    pub enemies: bool,
    pub self_on_hit: bool,
    pub offense: (f32, f32),
    pub defense: (f32, f32),
    pub regeneration: (f32, f32),
    pub poison: (f32, f32),
    pub activation_effect: Option<String>,
}

impl Ability {
    pub fn from_props(src: &dyn PropSource) -> Option<Self> {
        let pair = |timer, value, default| {
            (
                pf(src, timer).unwrap_or(0.0).max(0.0),
                pf(src, value).unwrap_or(default),
            )
        };
        let ability = Self {
            radius: pf(src, "Radius").unwrap_or(0.0).max(0.0),
            friends: pf(src, "AffectFriends").unwrap_or(1.0) != 0.0,
            enemies: pf(src, "AffectEnemies").unwrap_or(1.0) != 0.0,
            self_on_hit: src
                .prop_hash(0x39b3_648c)
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.0)
                != 0.0,
            offense: pair("BuffOffenseTimer", "BuffOffenseMult", 1.0),
            defense: pair("BuffDefenseTimer", "BuffDefenseMult", 1.0),
            regeneration: pair("BuffHealthTimer", "BuffHealthRate", 0.0),
            poison: pair("DebuffDamageTimer", "DebuffDamageRate", 0.0),
            activation_effect: src.prop_hash(0x76f3_beea).map(str::to_owned),
        };
        (ability.radius > 0.0
            && pf(src, "ConeLength").unwrap_or(10.0) == 0.0
            && [
                ability.offense.0,
                ability.defense.0,
                ability.regeneration.0,
                ability.poison.0,
            ]
            .into_iter()
            .any(|t| t > 0.0))
        .then_some(ability)
    }
}
