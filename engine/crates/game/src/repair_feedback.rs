use core::sim::{Event, Sim};
use core::unit::UnitHandle;
use core::weapon::{FireState, WeaponClass};
use glam::Vec3;

pub(crate) fn active(sim: &Sim, h: UnitHandle) -> Option<(usize, &WeaponClass)> {
    let u = sim.unit(h)?;
    if !u.alive || u.mount.is_some() {
        return None;
    }
    let (class, _, state) = u.active_weapon();
    let wc = sim.weapon_classes.get(class)?;
    if wc.repair.is_none() || state.state != FireState::Firing {
        return None;
    }
    let paid = sim.events.iter().any(|e| {
        matches!(e, Event::Repair { unit, weapon_class, .. }
            if *unit == h && *weapon_class as usize == class)
    });
    (paid
        || (u.is_player && sim.cheats.infinite_stamina)
        || u.soldier.energy >= wc.heat_per_shot * u.consts.energy_bar)
        .then_some((class, wc))
}

pub(crate) fn loops(sim: &Sim) -> Vec<(UnitHandle, usize, &str, Vec3)> {
    sim.alive_units()
        .filter_map(|(h, u)| {
            let (class, wc) = active(sim, h)?;
            Some((h, class, wc.fire_loop_sound.as_deref()?, u.muzzle()))
        })
        .collect()
}

#[cfg(test)]
mod tests;
