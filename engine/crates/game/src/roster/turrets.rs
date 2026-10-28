use super::{explosion_class, weapon_from_db, ClassDb, ClassSrc};
use anyhow::{Context, Result};
use core::autoturret::TurretClass;
use core::weapon::{Dispenser, OrdnanceClass, PropSource, WeaponClass};

pub(super) fn turret_weapon(
    db: &ClassDb,
    weapon: &str,
    ord: &str,
    wsrc: &dyn PropSource,
) -> Result<WeaponClass> {
    let name = db
        .resolve(ord)
        .with_context(|| format!("{weapon}: turret class {ord} not in class db"))?
        .to_owned();
    let src = ClassSrc { db, class: &name };
    let gun_name = src
        .prop("WeaponName")
        .and_then(|g| db.resolve(g.trim()))
        .with_context(|| format!("{name}: no WeaponName"))?;
    let mut item = TurretClass::new(&src, weapon_from_db(db, gun_name)?);
    item.explosion = src
        .prop("ExplosionName")
        .and_then(|n| explosion_class(db, n.trim()));
    let mut w = WeaponClass::from_props(weapon, wsrc, OrdnanceClass::default());
    w.ordnance.geometry = item.geometry.clone();
    w.turret = Some(Box::new(Dispenser::new(&mut w, wsrc, db.is_bf2(), item)));
    Ok(w)
}
