use crate::sim::{buttons, Sim};
use crate::unit::{Controller, UnitHandle};
use crate::weapon::FireState;

const SUPPORT_THREAT_RANGE: f32 = 30.0;

pub(crate) fn input(sim: &Sim, owner: UnitHandle) -> Option<u32> {
    let u = sim.unit(owner)?;
    if u.controller != Controller::Ai || u.mount.is_some() {
        return None;
    }
    if let Some(k) = u.cur1 {
        let w = &u.held[usize::from(k)];
        if w.state.state == FireState::Firing {
            let wc = &sim.weapon_classes[w.class];
            return (wc.ability.is_some() || wc.remote.is_some()).then_some(0);
        }
    }
    let selected = u.held.iter().enumerate().find_map(|(k, w)| {
        let wc = &sim.weapon_classes[w.class];
        let ability = wc.ability.as_ref()?;
        if w.channel != 1 || w.state.reloading() || w.state.clip < wc.ammo_per_shot {
            return None;
        }
        // Who would gain from it first (cheap), then the rays: the caster's
        // sight of that soldier, and an enemy in sight of a soldier who would
        // gain damage or defense.
        let caster = owner.index as usize;
        let useful = sim
            .ability_candidates(caster, ability)
            .filter(|&target| {
                let other = &sim.units[target];
                (ability.poison.0 > 0.0 && other.buffs.poison.0 <= 0.0)
                    || (ability.regeneration.0 > 0.0
                        && other.health.cur < other.health.max
                        && other.buffs.regeneration.0 <= 0.0)
                    || ((ability.offense.0 > 0.0 && other.buffs.offense.0 <= 0.0
                        || ability.defense.0 > 0.0 && other.buffs.defense.0 <= 0.0)
                        && sim.alive_units().any(|(_, enemy)| {
                            enemy.team != u.team
                                && enemy.soldier.pos.distance_squared(other.soldier.pos)
                                    <= SUPPORT_THREAT_RANGE * SUPPORT_THREAT_RANGE
                                && crate::perception::los(sim, other.muzzle(), enemy.muzzle())
                        }))
            })
            .any(|target| sim.ability_sees(caster, target));
        useful.then_some(k)
    });
    let selected = match selected {
        Some(k) => k,
        None => remote_slot(sim, owner)?,
    };
    Some(if u.cur1 == Some(selected as u8) {
        if u.held[selected].state.fire_cmd == 0 {
            buttons::SECONDARY_FIRE
        } else {
            0
        }
    } else if u.switch_held & 2 == 0 {
        buttons::SWITCH_SECONDARY
    } else {
        0
    })
}

fn remote_slot(sim: &Sim, owner: UnitHandle) -> Option<usize> {
    let u = sim.unit(owner)?;
    let target = u.ai.target.and_then(|t| sim.unit(t))?;
    if target.soldier.pos.distance_squared(u.soldier.pos)
        > crate::remote::BOT_RANGE * crate::remote::BOT_RANGE
    {
        return None;
    }
    u.held.iter().enumerate().find_map(|(k, w)| {
        let wc = &sim.weapon_classes[w.class];
        (wc.remote.is_some()
            && w.channel == 1
            && !w.state.reloading()
            && w.state.clip >= wc.ammo_per_shot
            && sim.droid_of(owner, w.class).is_none())
        .then_some(k)
    })
}
