use super::{explosion_class, fired_ordnance, weapon_from_db, ClassDb, ClassSrc};
use anyhow::{Context, Result};
use core::remote::{Destruct, DroidClass, RemoteWeapon, Salvo};
use core::weapon::{OrdnanceClass, PropSource, WeaponClass};

const SALVO_ORDNANCE: u32 = 0x6fd9_2be8;
const BURST_SOUND: u32 = 0x76f3_beea;

pub(super) fn remote_weapon(
    db: &ClassDb,
    weapon: &str,
    wsrc: &dyn PropSource,
) -> Result<WeaponClass> {
    let ord =
        WeaponClass::ordnance_name(wsrc).with_context(|| format!("{weapon}: no OrdnanceName"))?;
    let droid_name = db
        .resolve(ord.trim())
        .with_context(|| format!("{weapon}: droid class {ord} not in class db"))?
        .to_owned();
    let mut droid = DroidClass::from_props(
        &ClassSrc {
            db,
            class: &droid_name,
        },
        db.is_bf2(),
    );
    droid.explosion = db
        .prop(&droid_name, "ExplosionName")
        .and_then(|n| explosion_class(db, n.trim()));
    let slots = db.weapon_slots(&droid_name);
    let blaster_name = slots
        .iter()
        .find(|s| s.channel == 0)
        .and_then(|s| db.resolve(&s.weapon))
        .with_context(|| format!("{droid_name}: no blaster"))?;
    let blaster = weapon_from_db(db, blaster_name)?;
    let mut w = WeaponClass::from_props(weapon, wsrc, OrdnanceClass::default());
    core::weapon::Dispenser::new(&mut w, wsrc, db.is_bf2(), ());
    w.ordnance.geometry = droid.geometry.clone();
    let mut remote = RemoteWeapon::new(wsrc, droid, blaster);
    remote.destruct = slots
        .iter()
        .find(|s| s.channel == 1)
        .and_then(|s| db.resolve(&s.weapon))
        .map(|name| destruct(db, name));
    w.remote = Some(Box::new(remote));
    Ok(w)
}

fn destruct(db: &ClassDb, name: &str) -> Destruct {
    let src = ClassSrc { db, class: name };
    let mut d = Destruct::from_props(&src);
    let ord = src
        .prop("OrdnanceName")
        .or_else(|| src.prop_hash(SALVO_ORDNANCE))
        .and_then(|o| db.resolve(o.trim()));
    let Some(ord) = ord else {
        return d;
    };
    if db.engine_base(ord) == Some("emitterordnance") {
        let osrc = ClassSrc { db, class: ord };
        let radius = db.prop_f32(ord, "Radius").unwrap_or(0.0);
        let mut e = core::weapon::ExplosionClass {
            damage: db
                .prop_f32(ord, "Damage")
                .or_else(|| db.prop_f32(ord, "MaxDamage"))
                .unwrap_or(0.0),
            damage_inner: radius,
            damage_outer: radius,
            spares_living: true,
            ..Default::default()
        };
        e.scales = OrdnanceClass::from_props(&osrc).scales;
        if let Some(look) = db
            .prop(ord, "Explosion")
            .and_then(|x| explosion_class(db, x.trim()))
        {
            e.look = look.look;
        }
        d.burst = Some(e);
        d.burst_sound = osrc.prop_hash(BURST_SOUND).map(|s| s.trim().to_owned());
    } else {
        let shell = WeaponClass {
            name: format!("{name}_salvo"),
            ordnance: fired_ordnance(db, ord),
            ..WeaponClass::default()
        };
        d.salvo = Some(Salvo::from_props(&src, shell));
    }
    d
}
