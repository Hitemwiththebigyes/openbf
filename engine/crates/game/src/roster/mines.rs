use super::{explosion_class, ClassDb, ClassSrc};
use core::mine::MineClass;

pub(super) fn mine_class(db: &ClassDb, name: &str) -> MineClass {
    let mut class = MineClass::from_props(&ClassSrc { db, class: name }, db.is_bf2());
    class.explosions = [
        db.prop(name, "ExplosionTrigger")
            .or_else(|| db.prop(name, "ExplosionName"))
            .or_else(|| db.prop_hash(name, 0x02bb1fe0)),
        db.prop(name, "ExplosionExpire"),
        db.prop_hash(name, 0xa2723442),
    ]
    .map(|n| n.and_then(|n| explosion_class(db, n.trim())));
    class
}
