//! What a bolt does when it lands (`resolve_bolt`, step 5: damage through the
//! id guard, D12's human damage layer, the kill and its ticket, then its
//! ordnance's impact explosion - card RKEX, the rocket launchers) and the one
//! environmental death path, [`Sim::kill_environment`] (a mission death
//! region, step 3c, or a lethal landing, step 3). The bolt, cheat, damage and
//! environmental-death tests live here; the bolts themselves are fired inline
//! in `step` (step 4).

use super::state::{DeathCause, Event, Sim};
use crate::collision::{HullFilter, Layers};
use crate::impact::Surface;
use crate::projectile::{Bolt, Hit, Target};
use crate::soldier::Stance;
use crate::unit::{Controller, UnitHandle};
use crate::vehicle::VehicleHandle;
use crate::TICK_HZ;
use glam::Vec3;

// Only for `kill_environment`'s docs, which link `LETHAL_LANDING_SPEED`.
#[cfg(doc)]
use super::state::LETHAL_LANDING_SPEED;

/// The head prim's HitLocation factor (see `Sim::hit_scale`).
pub const HEADSHOT_SCALE: f32 = 3.0;

impl Sim {
    /// The PS2's per-frame bolt sweep (B1 FUN_00273128, BF2 FUN_0030fad0;
    /// `re-ordnance-kinds.md` §2.4 [V][M]; issue #41: the port had kept the
    /// spawn ray's target): what each live bolt passes through on this
    /// tick's flight - the soldiers but its shooter (their
    /// bolt prims, `Unit::bolt_hit`) and the hulls its `Bolt::hulls` admits. The nearest such
    /// hit short of where the flight ends becomes the bolt's hit, so it lands
    /// this tick. The map and the terrain were asked at the press: they do
    /// not move. A rider of a vehicle whose hull the bolt passes is passed
    /// too (a gun's own crew). Cost: one prim list a tick, a box test per bolt and
    /// soldier, and a hull-only ray per bolt.
    pub(super) fn sweep_bolts(&mut self, dt: f32) {
        if self.bolts.is_empty() {
            return;
        }
        type Body = (
            UnitHandle,
            Option<VehicleHandle>,
            crate::unit::BoltPrims,
            Vec3,
            Vec3,
        );
        let units: Vec<Body> = self
            .alive_units()
            .map(|(h, u)| {
                let p = u.bolt_prims();
                let (min, max) = p.bounds();
                (h, u.mount.map(|m| m.vehicle), p, min, max)
            })
            .collect();
        let (world, bolts) = (&self.collision, &mut self.bolts);
        for b in bolts.iter_mut() {
            let step = dt.min(b.remaining.max(0.0));
            let speed = b.vel.length();
            if step <= 0.0 || speed <= 0.0 {
                continue;
            }
            let dir = b.vel / speed;
            let mut reach = speed * step;
            let (from, to) = (b.pos, b.pos + dir * reach);
            let (lo, hi) = (from.min(to), from.max(to));
            let mut hit = None;
            let crew = match b.hulls {
                HullFilter::Except(s) | HullFilter::HullsExcept(s) => s,
                _ => [None; 2],
            };
            let mut mult = 1.0;
            for (h, rides, prims, min, max) in &units {
                if *h == b.owner
                    || (rides.is_some() && crew.contains(rides))
                    || max.cmplt(lo).any()
                    || min.cmpgt(hi).any()
                {
                    continue;
                }
                if let Some((t, zone)) = prims.hit(from, dir, reach) {
                    reach = t;
                    hit = Some(Target::Unit(*h));
                    mult = if zone == crate::unit::HitZone::Head {
                        HEADSHOT_SCALE
                    } else {
                        1.0
                    };
                }
            }
            for m in self.mines.iter().filter(|m| m.health > 0.0) {
                if let Some(t) =
                    crate::unit::ray_sphere(from, dir, m.flight.pos, crate::mine::RADIUS)
                        .filter(|&t| t <= reach)
                {
                    reach = t;
                    hit = Some(Target::Mine(m.id));
                    mult = 1.0;
                }
            }
            if let Some(filter) = b.hulls.hulls_only() {
                if let Some(r) = world.ray_with(from, dir, reach, Layers::ORDNANCE, filter) {
                    if let Some(v) = r.owner.filter(|_| r.t < reach) {
                        reach = r.t;
                        hit = Some(Target::Vehicle(v));
                        mult = r.hit_mult;
                    }
                }
            }
            if let Some(target) = hit {
                b.hit = Hit::Target(target);
                b.hit_mult = mult;
                b.end = from + dir * reach;
                b.remaining = reach / speed;
            }
        }
    }

    /// The struck prim's damage factor (issue #41): a soldier's head prim
    /// takes the PS2's default HitLocation x3 (BF2 FUN_002517e8
    /// B2:170444-170450, BF1 FUN_0020e2e0 B1:122725; analysis/re-hitzones.md),
    /// a vehicle prim its class's HitLocation (the x4 critical prims; BF1's
    /// two x0). A factor over 1 is a bot's only on the PS2's top level (BF2
    /// FUN_001e0580 B2:91567-91580: dropped for an AI attacker under level
    /// 3; the port's Hard and Elite). BF1 has no such gate; one rule on both
    /// games' maps.
    fn hit_scale(&self, b: &Bolt) -> f32 {
        let bot_gate =
            !b.shooter_is_player && self.difficulty_of(b.team) < crate::skill::Difficulty::Hard;
        if b.hit_mult > 1.0 && bot_gate {
            1.0
        } else {
            b.hit_mult
        }
    }

    /// Impact handler (BF1 FUN_00276810 → TakeDamage FUN_001bc190; BF2
    /// FUN_00313c40 → FUN_001dfb38): damage only if the remembered target is
    /// still the same live entity (id guard).
    pub(super) fn resolve_bolt(&mut self, b: &Bolt) {
        match b.hit {
            Hit::Target(Target::Unit(h)) => {
                let Some(victim) = self.unit(h) else {
                    return; // died in flight → bolt dies, no impact
                };
                let victim_team = victim.team;
                let victim_is_player = victim.is_player;
                let desc = self.weapon_classes[b.weapon_class]
                    .ordnance
                    .damage_desc_charged(b.age, false, b.charge);
                let friendly = victim_team == b.team;
                // A bolt of a unit that is not a human player does nothing to
                // a human player of its own team (B1 FUN_001bc190, B2
                // FUN_001dfb38: TakeDamage returns before the scale;
                // re-blaster-damage.md section 6.3) - as a blast, `blast_unit`.
                let invincible = victim_is_player
                    && (self.cheats.invincible || (friendly && !b.shooter_is_player));
                let ff = self.friendly_fire_scale;
                // D12, the human damage layer: a BOT's bolt into a HUMAN
                // enemy carries the tier's multiplier. OURS, PROVISIONAL —
                // the PS2's only level-3 damage lever is `HitLocation`, the
                // ×4 vehicle critical prims an Elite attacker may score
                // (`analysis/re-ai-difficulty.md` §9.2), which is not a buff
                // against humans and waits for P7 vehicles.
                let mult = if self.human_damage_on
                    && victim_is_player
                    && !b.shooter_is_player
                    && !friendly
                {
                    self.difficulty_of(b.team).tier().human_damage
                } else {
                    1.0
                } * self.buff_scale(Some(b.owner), h)
                    * self.hit_scale(b);
                let v = self.unit_mut(h).unwrap();
                let damage = if invincible {
                    0.0
                } else {
                    v.health.take_damage(&desc, mult, friendly, ff)
                };
                if self
                    .units
                    .get(b.owner.index as usize)
                    .filter(|s| s.gen == b.owner.gen)
                    .is_some_and(|s| s.controller == Controller::Ai)
                {
                    self.nav_stats.ai_hits += 1;
                }
                self.nav_stats.bolt_hits += 1;
                if friendly && b.owner != h {
                    self.nav_stats.friend_hits += 1;
                }
                self.push_impact(b, Surface::Soldier, None);
                if self.finish_unit_damage(
                    h,
                    Some(b.owner),
                    damage,
                    b.age,
                    b.vel.normalize_or_zero(),
                ) {
                    // The direct hit's death kick along the shot (B1
                    // FUN_00200c30 -> FUN_00200f90, re-knockback §3).
                    let u = &mut self.units[h.index as usize];
                    if u.body {
                        u.soldier.kick(b.vel);
                    }
                }
                self.bolt_explodes(b);
            }
            // The hull pool (re-vehicles §6): the ordnance's damage at this
            // age, scaled by the hull's HealthType (`VehicleScale`). The id
            // guard as for a soldier: a vehicle gone in flight kills the bolt
            // with no impact. The hit strikes (the puff), and it lands before
            // the damage so a killing bolt's puff precedes the blast. The
            // spawn ray picks a hull from the ORDNANCE layer
            // (`Bolt::spawn`: `world.ray_with`).
            Hit::Target(Target::Vehicle(vh)) => {
                if self.vehicle(vh).is_none() {
                    return;
                }
                let desc = self.weapon_classes[b.weapon_class]
                    .ordnance
                    .damage_desc_charged(b.age, false, b.charge);
                self.events.push(Event::BoltExpired {
                    pos: b.end,
                    struck: true,
                });
                self.push_impact(b, Surface::Vehicle, Some(vh));
                let scale = self.hit_scale(b);
                self.damage_vehicle_by(
                    vh,
                    &desc,
                    scale,
                    Some(b.owner),
                    Some(b.team),
                    Some(b.vel.normalize_or_zero()),
                );
                self.bolt_explodes(b);
            }
            Hit::Target(Target::Mine(id)) => {
                let desc = self.weapon_classes[b.weapon_class]
                    .ordnance
                    .damage_desc_charged(b.age, false, b.charge);
                if let Some(k) = self.mines.iter().position(|m| m.id == id) {
                    let damage =
                        self.mine_damage(&self.mines[k], desc.damage, &desc.scales, Some(b.team));
                    self.mines[k].health -= damage;
                }
                self.events.push(Event::BoltExpired {
                    pos: b.end,
                    struck: true,
                });
                self.push_impact(b, Surface::Vehicle, None);
                self.bolt_explodes(b);
            }
            // A model wall ends the flight exactly as the terrain sheet does:
            // the same event, so the same puff and the same impact sound.
            Hit::Target(Target::Station(s)) => {
                if self
                    .stations
                    .get(usize::from(s))
                    .is_none_or(|s| s.destroyed)
                {
                    return;
                }
                let desc = self.weapon_classes[b.weapon_class]
                    .ordnance
                    .damage_desc_charged(b.age, false, b.charge);
                self.events.push(Event::BoltExpired {
                    pos: b.end,
                    struck: true,
                });
                self.push_impact(b, Surface::Building, None);
                self.damage_station(s, &desc, 1.0, Some(b.owner), Some(b.team));
                self.bolt_explodes(b);
            }
            Hit::Terrain | Hit::World | Hit::Water => {
                self.events.push(Event::BoltExpired {
                    pos: b.end,
                    struck: true,
                });
                let surface = match b.hit {
                    Hit::Terrain => Surface::Terrain,
                    Hit::Water => Surface::Water,
                    _ => Surface::Building,
                };
                self.push_impact(b, surface, None);
                self.bolt_explodes(b);
            }
            Hit::None => {
                // A shell out of life bursts in the air with its
                // ExplosionExpire (re-ordnance-kinds §6).
                if b.gravity > 0.0 {
                    if let Some(e) = self.weapon_classes[b.weapon_class].ordnance.expire.clone() {
                        self.explode(&e, b.pos, Some(b.owner), Some(b.team));
                    }
                }
                self.events.push(Event::BoltExpired {
                    pos: b.end,
                    struck: false,
                })
            }
        }
    }

    /// A struck bolt's impact explosion (card RKEX): the ordnance base's
    /// collision (B1 FUN_00272d70, B2 FUN_0030f550) detonates the class's
    /// `Explosion` / `ExplosionImpact` / `ExplosionName` class (B1 class
    /// +0x38, B2 +0x4c) 0.2 m off the hit with the shooter's {owner, team}
    /// whenever the class names one - on every surface (re-blaster-damage
    /// 7.3; the rocket launchers: MaxDamage 500 on the one it strikes, then
    /// Damage 400 over DamageRadius 4). After the direct hit [D]: the bolt
    /// handler (B1 FUN_00276810, B2 FUN_00313c40) builds its damage at
    /// B1:197920 and calls the base last (B1:198103), so a victim the direct
    /// hit killed takes no more damage; its corpse still consumes a gather
    /// slot and can receive the blast's kick/push. A bolt whose target died in flight
    /// dies with no impact and no blast, as before. The drawn explosion and
    /// its sound ride [`Event::Impact`].
    fn bolt_explodes(&mut self, b: &Bolt) {
        let Some(e) = self.weapon_classes[b.weapon_class]
            .ordnance
            .explosion
            .clone()
        else {
            return;
        };
        let at = b.end - b.vel.normalize_or_zero() * crate::weapon::EXPLOSION_OFFSET;
        self.explode(&e, at, Some(b.owner), Some(b.team));
    }

    /// BF2's buffs on one damage (card PWUP, `re-powerups.md` §5.5): the
    /// attacker's offense multiplier x the victim's defense multiplier, 1.0
    /// while neither timer runs.
    pub(super) fn buff_scale(&self, attacker: Option<UnitHandle>, victim: UnitHandle) -> f32 {
        let dealt = attacker
            .and_then(|a| self.units.get(a.index as usize).filter(|u| u.gen == a.gen))
            .map_or(1.0, |u| u.buffs.dealt());
        let taken = self
            .units
            .get(victim.index as usize)
            .map_or(1.0, |u| u.buffs.taken());
        dealt * taken
    }

    /// [`Event::Impact`] for `b` striking `surface` at its end (card FXP3).
    fn push_impact(
        &mut self,
        b: &Bolt,
        surface: Surface,
        vehicle: Option<crate::vehicle::VehicleHandle>,
    ) {
        self.events.push(Event::Impact {
            pos: b.end,
            dir: b.vel.normalize_or_zero(),
            weapon_class: b.weapon_class as u32,
            surface,
            vehicle,
        });
    }

    pub(super) fn finish_unit_damage(
        &mut self,
        victim: UnitHandle,
        attacker: Option<UnitHandle>,
        damage: f32,
        age: f32,
        dir: Vec3,
    ) -> bool {
        let u = &mut self.units[victim.index as usize];
        let killed = !u.health.alive();
        if let Some(shooter) = attacker {
            u.last_hit = Some((shooter, self.tick));
            self.events.push(Event::Hit {
                shooter,
                victim,
                damage,
                age,
                health_left: u.health.cur,
                dir,
            });
        }
        if killed {
            self.unit_killed(victim, attacker);
        }
        killed
    }

    /// The bookkeeping of a unit that damage killed (its health is already
    /// ≤ 0): respawn timer, wave slot, the battle's alive count and ticket,
    /// the squad rules, the seat, and the kill credit — [`Event::Kill`] for
    /// `killer` (scored on its life even when it is dead), or a killer-less blast's
    /// [`Event::EnvDeath`] ([`DeathCause::Blast`]).
    pub(super) fn unit_killed(&mut self, h: UnitHandle, killer: Option<UnitHandle>) {
        let v = &self.units[h.index as usize];
        let (victim_team, victim_is_player) = (v.team, v.is_player);
        let respawn_at = self.tick + (self.respawn_delay * TICK_HZ as f32) as u64;
        let slot = self.waves[usize::from(victim_team).min(1)].assign();
        let v = &mut self.units[h.index as usize];
        v.alive = false;
        v.deaths += 1;
        v.respawn_at = Some(respawn_at);
        v.spawn_slot = Some(slot);
        // Card KNOCK (re-knockback §3): a soldier killed on foot keeps his
        // body; its item drop waits for the dead state (`Sim::death_block`).
        // A rider's is made now, as before (the seat's x 9 throw is not built).
        v.body = v.mount.is_none();
        v.drop_due = v.body;
        if !v.body {
            self.drop_item(h, None);
        }
        self.battle.on_death(victim_team as usize, victim_is_player);
        self.class_change_life_ended(h);
        // Phase D: the squad rules for a death, both of them.
        crate::squad::on_unit_died(self, h);
        self.unseat_unit(h);
        match killer {
            Some(k) => {
                let killer_team = self
                    .units
                    .get(k.index as usize)
                    .filter(|s| s.gen == k.gen)
                    .map(|s| s.team);
                self.nav_stats.unit_kills += 1;
                if k == h {
                    self.nav_stats.self_kills += 1;
                } else if killer_team == Some(victim_team) {
                    self.nav_stats.team_kills += 1;
                }
                // The PS2 credits the member, alive or dead (B1 FUN_00297440:
                // a destroyed vehicle's riders die after its destroyer may
                // have, re-vehicle-death 10.5): this life's slot by generation.
                if let Some(s) = self
                    .units
                    .get_mut(k.index as usize)
                    .filter(|s| s.gen == k.gen)
                {
                    s.kills += 1;
                }
                self.events.push(Event::Kill {
                    shooter: k,
                    victim: h,
                });
            }
            None => self.events.push(Event::EnvDeath {
                unit: h,
                cause: DeathCause::Blast,
            }),
        }
    }

    /// The death block of a dead unit's body (card KNOCK, re-knockback §3: B1
    /// B1:116889-116968, B2 B2:159605-159669): skipped while it is thrown,
    /// tumbling or getting up - a soldier a blast kills dies when he has
    /// landed - else the body enters [`Stance::Dead`] and the kill's item
    /// drop comes now. (The droid's chunk death is card DCHK's.)
    pub(super) fn death_block(&mut self, i: usize) {
        let u = &self.units[i];
        if u.alive || !u.body || u.soldier.stance.knocked() || u.soldier.stance == Stance::Dead {
            return;
        }
        let drop = u.drop_due;
        let u = &mut self.units[i];
        u.drop_due = false;
        u.soldier.stance = Stance::Dead;
        u.soldier.time_in_stance = 0.0;
        if drop {
            let h = self.handle(i);
            self.drop_item(h, None);
        }
    }

    /// The ONE environmental death path: a mission death region, or a lethal
    /// landing. It does to the unit, the battle and the spawn waves exactly
    /// what a bolt kill does — respawn timer, wave slot, alive count, one
    /// ticket — and emits [`Event::EnvDeath`] so the HUD and audio can tell
    /// the two apart. A dead or stale handle is a no-op. The `invincible`
    /// cheat spares the player from the causes that are ordinary damage on
    /// the PS2 ([`DeathCause::cheat_spares`]: a landing, a blast) and from
    /// nothing else: a death region, the kill floor, a landing under the map
    /// and a destroyed vehicle kill him too.
    ///
    /// `AddDeathRegion`'s consumer (`analysis/re-kill-floor.md`, 2026-10-01): a
    /// per-frame test of every soldier and vehicle against the 32 region
    /// slots, an instant kill with no killer and no cheat check (BF1
    /// `FUN_002af1c0` / `FUN_002af0d0`, BF2 `FUN_00371148` / `FUN_00370fd8`).
    /// BF1 tests the feet, as here; BF2 tests the collision sphere (not
    /// built). The lethal-landing rule is [`LETHAL_LANDING_SPEED`]
    /// (PROVISIONAL, card SLAND).
    pub fn kill_environment(&mut self, h: UnitHandle, cause: DeathCause) {
        let Some(u) = self.unit(h) else {
            return;
        };
        if u.is_player && self.cheats.invincible && cause.cheat_spares() {
            return;
        }
        let (team, is_player) = (u.team, u.is_player);
        let respawn_at = self.tick + (self.respawn_delay * TICK_HZ as f32) as u64;
        let slot = self.waves[usize::from(team).min(1)].assign();
        let v = &mut self.units[h.index as usize];
        v.alive = false;
        v.deaths += 1;
        v.health.cur = 0.0;
        v.respawn_at = Some(respawn_at);
        v.spawn_slot = Some(slot);
        self.drop_item(h, Some(cause));
        self.battle.on_death(team as usize, is_player);
        self.class_change_life_ended(h);
        crate::squad::on_unit_died(self, h);
        self.unseat_unit(h);
        self.events.push(Event::EnvDeath { unit: h, cause });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightfield::Heightfield;
    use crate::region::Region;
    use crate::sim::tests::{ai, duel, fire, flat_ground};
    use crate::sim::PadState;
    use crate::skill::Difficulty;
    use crate::unit::UnitSpec;
    use glam::Vec3;

    /// Late projectiles cannot attribute the replacement occupant's identity
    /// to a prior life; matching-generation dead shooters still count.
    #[test]
    fn diagnostic_counters_check_the_shooters_generation() {
        let mut s = Sim::empty(3);
        let old = s
            .spawn(&UnitSpec {
                controller: Controller::Idle,
                ..UnitSpec::trooper(1, Vec3::X * 30.0)
            })
            .unwrap();
        let victim = s.spawn(&UnitSpec::trooper(0, Vec3::ZERO)).unwrap();
        s.kill_environment(old, DeathCause::KillFloor);
        let replacement = s
            .spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(0, Vec3::X * 30.0)
            })
            .unwrap();
        assert_eq!(replacement.index, old.index);
        assert_ne!(replacement.gen, old.gen);
        let bolt = |owner| Bolt {
            pos: Vec3::X,
            vel: -Vec3::X,
            remaining: 0.0,
            age: 0.0,
            owner,
            team: 1,
            weapon_class: 0,
            hit: Hit::Target(Target::Unit(victim)),
            end: Vec3::ZERO,
            shooter_is_player: false,
            hulls: crate::collision::HullFilter::All,
            hit_mult: 1.0,
            gravity: 0.0,
            charge: 1.0,
        };
        s.resolve_bolt(&bolt(old));
        assert_eq!(s.nav_stats.ai_hits, 0, "new AI is not the old idle shooter");
        s.kill_environment(replacement, DeathCause::KillFloor);
        s.resolve_bolt(&bolt(replacement));
        assert_eq!(s.nav_stats.ai_hits, 1, "matching dead AI still recognized");
        s.unit_killed(victim, Some(old));
        assert_eq!(
            s.nav_stats.team_kills, 0,
            "replacement team is not old owner team"
        );
        assert_eq!(s.units[replacement.index as usize].kills, 0);
        let friend = s.spawn(&UnitSpec::trooper(0, Vec3::ZERO)).unwrap();
        // Spawn may reuse the dead shooter's slot; keep this separate control
        // on a freshly killed attacker whose generation still occupies it.
        let control = s.spawn(&UnitSpec::trooper(0, Vec3::X * 30.0)).unwrap();
        s.kill_environment(control, DeathCause::KillFloor);
        s.unit_killed(friend, Some(control));
        assert_eq!(s.nav_stats.team_kills, 1);
        assert_eq!(s.units[control.index as usize].kills, 1);
    }

    /// A mission death region kills like anything else: the unit dies, the
    /// team pays a ticket, the respawn timer and wave slot are set, and the
    /// event says what did it.
    #[test]
    fn unit_inside_death_region_dies_and_consumes_a_ticket() {
        let mut s = Sim::empty(11);
        s.heightfield = Some(flat_ground());
        s.death_regions = vec![Region::sphere(Vec3::new(0.0, 0.0, -10.0), 3.0)];
        let h = s
            .spawn(&UnitSpec {
                controller: Controller::Pacer {
                    vel: Vec3::new(0.0, 0.0, -2.0),
                    period: 0,
                },
                ..UnitSpec::trooper(1, Vec3::ZERO)
            })
            .expect("spawned");
        assert_eq!(s.battle.teams[1].tickets(), 250);
        assert_eq!(s.battle.teams[1].alive(), 1);
        let mut cause = None;
        let mut died_at = None;
        for t in 0..150u64 {
            s.step(&[PadState::default()]);
            for e in &s.events {
                if let Event::EnvDeath { unit, cause: c } = e {
                    assert_eq!(*unit, h);
                    cause = Some(*c);
                    died_at = Some(t);
                }
            }
        }
        assert_eq!(cause, Some(DeathCause::DeathRegion));
        // It walks 2 m/s and the sphere's near face is 7 m away.
        assert!(
            died_at.is_some_and(|t| (100..=115).contains(&t)),
            "{died_at:?}"
        );
        assert!(s.unit(h).is_none(), "dead");
        assert_eq!(s.battle.teams[1].tickets(), 249, "one ticket paid");
        assert_eq!(s.battle.teams[1].alive(), 0);
        let u = &s.units[h.index as usize];
        assert!(u.respawn_at.is_some() && u.spawn_slot.is_some());
        assert_eq!(u.deaths, 1);
    }

    /// Card FFLN (re-blaster-damage.md section 6.3): a bolt into a unit of
    /// the shooter's own team takes `friendly_fire_scale` (1.0 on the PS2's
    /// instant action) — except a bot's bolt into a human, which does
    /// nothing. The counters name what happened.
    #[test]
    fn a_bots_bolt_spares_a_human_of_its_team_and_not_a_bot() {
        use crate::projectile::{Bolt, Hit, Target};
        let mut s = Sim::empty(3);
        let human = s
            .spawn(&UnitSpec {
                player_slot: Some(0),
                controller: Controller::Player { slot: 0 },
                ..UnitSpec::trooper(0, Vec3::ZERO)
            })
            .unwrap();
        let bot = s.spawn(&UnitSpec::trooper(0, Vec3::X * 3.0)).unwrap();
        let friend = s.spawn(&UnitSpec::trooper(0, Vec3::X * 6.0)).unwrap();
        let bolt = |owner: UnitHandle, victim: UnitHandle, shooter_is_player: bool| Bolt {
            pos: Vec3::ZERO,
            vel: Vec3::X,
            remaining: 0.0,
            age: 0.0,
            owner,
            team: 0,
            weapon_class: 0,
            hit: Hit::Target(Target::Unit(victim)),
            end: Vec3::ZERO,
            shooter_is_player,
            hulls: crate::collision::HullFilter::All,
            hit_mult: 1.0,
            gravity: 0.0,
            charge: 1.0,
        };
        let hp = |s: &Sim, h: UnitHandle| s.unit(h).unwrap().health.cur;
        s.resolve_bolt(&bolt(bot, human, false));
        assert_eq!(hp(&s, human), 300.0, "a bot's bolt, a human friend");
        s.resolve_bolt(&bolt(bot, friend, false));
        let full = 300.0 - hp(&s, friend);
        assert!(full > 0.0, "a bot's bolt, a bot friend: full damage");
        s.resolve_bolt(&bolt(human, bot, true));
        assert_eq!(300.0 - hp(&s, bot), full, "a human's bolt, a friend");
        assert_eq!((s.nav_stats.bolt_hits, s.nav_stats.friend_hits), (3, 3));
        // The friend dies to its own team: a team kill, not a self kill.
        for _ in 0..20 {
            if s.unit(friend).is_some() {
                s.resolve_bolt(&bolt(bot, friend, false));
            }
        }
        let n = s.nav_stats;
        assert_eq!((n.unit_kills, n.team_kills, n.self_kills), (1, 1, 0));
    }

    /// Card KNOCK (re-knockback §3, B1 FUN_00200c30): a bolt that kills
    /// kicks the body 5 m/s along the shot; the body then brakes to rest in
    /// the dead state.
    #[test]
    fn a_killing_bolt_kicks_the_body() {
        use crate::projectile::{Bolt, Hit, Target};
        use crate::soldier::Stance;
        let mut s = Sim::empty(3);
        s.heightfield = Some(crate::sim::tests::flat_ground());
        let shooter = s.spawn(&UnitSpec::trooper(0, Vec3::ZERO)).unwrap();
        let v = s.spawn(&UnitSpec::trooper(1, Vec3::X * 6.0)).unwrap();
        s.units[v.index as usize].health.cur = 1.0;
        let dir = Vec3::new(3.0, 0.0, 4.0);
        s.resolve_bolt(&Bolt {
            pos: Vec3::ZERO,
            vel: dir * 40.0,
            remaining: 0.0,
            age: 0.0,
            owner: shooter,
            team: 0,
            weapon_class: 0,
            hit: Hit::Target(Target::Unit(v)),
            end: Vec3::ZERO,
            shooter_is_player: false,
            hulls: crate::collision::HullFilter::All,
            hit_mult: 1.0,
            gravity: 0.0,
            charge: 1.0,
        });
        let u = &s.units[v.index as usize];
        assert!(!u.alive && u.body);
        assert!((u.soldier.vel - Vec3::new(3.0, 0.0, 4.0)).length() < 1e-5);
        for _ in 0..60 {
            s.step(&[]);
        }
        let u = &s.units[v.index as usize];
        assert_eq!(u.soldier.stance, Stance::Dead);
        assert_eq!(u.soldier.vel, Vec3::ZERO, "at rest");
        let moved = u.soldier.pos - Vec3::X * 6.0;
        assert!(
            moved.length() > 1.0 && moved.x > 0.0 && moved.z > 0.0,
            "{moved:?}"
        );
    }

    /// Card RKEX: a struck bolt whose ordnance names an impact explosion
    /// (the rocket launchers' `ExplosionName`: Damage 400, DamageRadiusInner
    /// 2, Outer 4, VehicleScale 0) detonates it 0.2 m back along the flight:
    /// a soldier beside the impact takes the blast's falloff, one outside
    /// the radius nothing, a wall hit as a ground hit, the struck soldier
    /// the direct hit (then the blast round it); a bolt with no explosion
    /// (the rifle's) hurts nobody beside its hit.
    #[test]
    fn a_rocket_explodes_where_it_lands() {
        use super::super::blast::soldier_blast_sphere;
        use crate::projectile::{Bolt, Hit, Target};
        use crate::weapon::{blast_falloff, ExplosionClass, HealthType};
        let rocket = |s: &mut Sim| {
            let mut w = s.weapon_classes[0].clone();
            w.ordnance.max_damage = 500.0;
            let mut e = ExplosionClass {
                damage: 400.0,
                damage_inner: 2.0,
                damage_outer: 4.0,
                ..ExplosionClass::default()
            };
            e.scales[HealthType::Vehicle as usize] = 0.0;
            w.ordnance.explosion = Some(e);
            s.weapon_classes.push(w);
            s.weapon_classes.len() - 1
        };
        let bolt = |owner: UnitHandle, wc: usize, hit: Hit, end: Vec3| Bolt {
            pos: end,
            vel: Vec3::new(0.0, -100.0, 0.0),
            remaining: 0.0,
            age: 0.0,
            owner,
            team: 0,
            weapon_class: wc,
            hit,
            end,
            shooter_is_player: false,
            hulls: crate::collision::HullFilter::All,
            hit_mult: 1.0,
            gravity: 0.0,
            charge: 1.0,
        };
        let world = || {
            let mut s = Sim::empty(3);
            s.heightfield = Some(flat_ground());
            let shooter = s
                .spawn(&UnitSpec::trooper(0, Vec3::new(-30.0, 0.0, 0.0)))
                .unwrap();
            let near = s
                .spawn(&UnitSpec::trooper(1, Vec3::new(3.5, 0.0, 0.0)))
                .unwrap();
            let far = s
                .spawn(&UnitSpec::trooper(1, Vec3::new(7.0, 0.0, 0.0)))
                .unwrap();
            (s, shooter, near, far)
        };
        let hp = |s: &Sim, h: UnitHandle| s.unit(h).map_or(0.0, |u| u.health.cur);
        // At the ground between them: `near`'s sphere surface is inside the
        // ramp, `far`'s outside the 4 m.
        let (mut s, shooter, near, far) = world();
        let wc = rocket(&mut s);
        s.resolve_bolt(&bolt(shooter, wc, Hit::Terrain, Vec3::ZERO));
        let at = Vec3::Y * 0.2;
        let (lift, w) = soldier_blast_sphere(s.unit(near).unwrap().soldier.stance);
        let d = (Vec3::new(3.5, lift, 0.0) - at).length();
        let f = blast_falloff(d, w, 2.0, 4.0);
        assert!(f > 0.0 && f < 1.0, "the near soldier is on the ramp: {f}");
        assert!(
            (300.0 - hp(&s, near) - 400.0 * f).abs() < 1e-3,
            "{}",
            hp(&s, near)
        );
        assert_eq!(hp(&s, far), 300.0, "outside the radius");
        // The rifle's bolt (no explosion) at the same spot: nobody.
        let (mut s, shooter, near, _) = world();
        s.resolve_bolt(&bolt(shooter, 0, Hit::Terrain, Vec3::ZERO));
        assert_eq!(hp(&s, near), 300.0, "a bolt with no explosion");
        // A wall model is struck the same way.
        let (mut s, shooter, near, _) = world();
        let wc = rocket(&mut s);
        s.resolve_bolt(&bolt(shooter, wc, Hit::World, Vec3::ZERO));
        assert!(hp(&s, near) < 300.0, "a wall hit explodes too");
        // A direct hit kills: 500 on a 300 trooper, the blast beside it.
        let (mut s, shooter, near, far) = world();
        let wc = rocket(&mut s);
        let chest = Vec3::new(3.5, 1.2, 0.0);
        s.resolve_bolt(&bolt(shooter, wc, Hit::Target(Target::Unit(near)), chest));
        assert!(s.unit(near).is_none(), "the struck soldier dies");
        let (lift, w) = soldier_blast_sphere(s.unit(far).unwrap().soldier.stance);
        let d = (Vec3::new(7.0, lift, 0.0) - (chest + Vec3::Y * 0.2)).length();
        let want = 400.0 * blast_falloff(d, w, 2.0, 4.0);
        assert!(want > 0.0, "{d}");
        assert!((300.0 - hp(&s, far) - want).abs() < 1e-3, "{}", hp(&s, far));
        assert_eq!(s.nav_stats.unit_kills, 1);
    }

    /// The 3c pass runs units in index order over regions in the mission's
    /// order, so a whole squad walking into two regions replays identically.
    #[test]
    fn death_region_check_is_index_ordered_and_deterministic() {
        fn run() -> (u64, usize) {
            let mut s = Sim::empty(5);
            s.heightfield = Some(flat_ground());
            s.death_regions = vec![
                Region::sphere(Vec3::new(0.0, 0.0, -10.0), 4.0),
                Region::sphere(Vec3::new(0.0, 0.0, -14.0), 4.0),
            ];
            for k in 0..6 {
                s.spawn(&UnitSpec {
                    controller: Controller::Pacer {
                        vel: Vec3::new(0.0, 0.0, -2.0 - k as f32 * 0.1),
                        period: 0,
                    },
                    ..UnitSpec::trooper(1, Vec3::new(k as f32 * 0.01, 0.0, 0.0))
                })
                .expect("spawned");
            }
            let mut deaths = 0;
            for _ in 0..150 {
                s.step(&[PadState::default()]);
                deaths += s
                    .events
                    .iter()
                    .filter(|e| matches!(e, Event::EnvDeath { .. }))
                    .count();
            }
            (s.hash(), deaths)
        }
        let a = run();
        assert_eq!(a.1, 6, "every pacer walked into a region");
        assert_eq!(a, run(), "same world, same hash");
    }

    /// A death region kills an invincible player too: the PS2's region kill
    /// never asks the cheat (`analysis/re-kill-floor.md` §1). A lethal landing
    /// is ordinary damage there, and the cheat still spares him from it.
    #[test]
    fn a_death_region_kills_through_the_invincibility_cheat() {
        use crate::sim::DeathCause;
        let mut s = Sim::empty(4);
        s.heightfield = Some(flat_ground());
        s.cheats.invincible = true;
        let p = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
        s.kill_environment(p, DeathCause::Fall);
        assert!(s.unit(p).is_some(), "the cheat spares him from a landing");
        s.kill_environment(p, DeathCause::Blast);
        assert!(s.unit(p).is_some(), "...and from a blast");
        s.death_regions = vec![Region::sphere(Vec3::ZERO, 3.0)];
        s.step(&[PadState::default()]);
        assert!(s.unit(p).is_none(), "the death region killed him");
    }

    /// PROVISIONAL `LETHAL_LANDING_SPEED`: a 30 m drop lands at ~33 m/s and
    /// kills; a 2 m drop lands at ~8.5 m/s and does not.
    #[test]
    fn a_long_fall_kills_and_a_short_one_does_not() {
        fn drop_from(height: f32) -> (bool, u32) {
            let mut s = Sim::empty(9);
            s.heightfield = Some(flat_ground());
            let h = s.spawn(&UnitSpec::trooper(1, Vec3::ZERO)).expect("spawned");
            {
                let u = &mut s.units[h.index as usize];
                u.soldier.pos.y = height;
                u.soldier.grounded = false;
            }
            for _ in 0..120 {
                s.step(&[PadState::default()]);
            }
            (s.unit(h).is_none(), s.battle.teams[1].tickets())
        }
        assert_eq!(drop_from(30.0), (true, 249), "a 30 m fall is lethal");
        assert_eq!(drop_from(2.0), (false, 250), "a 2 m fall is not");
    }

    /// §9.5 bolt flight: BF1 rifle bolt reaches a 20 m target at t = 0.25 s
    /// (impact processed on tick 8), no drop, ray length 120 m.
    #[test]
    fn v5_bolt_flight_time_to_20m_target() {
        let (mut s, p, t) = duel();
        s.step(&[fire()]);
        assert_eq!(s.bolts.len(), 1, "trigger shot spawned");
        // Issue #41: the target is found along the flight, not at the press
        // (the press finds only the ground far behind it).
        assert!(!matches!(s.bolts[0].hit, Hit::Target(_)));
        let way = s.bolts[0].vel.normalize();
        assert_eq!(s.weapon_classes[0].ordnance.max_range(), 120.0);
        let mut hit_tick = None;
        for _ in 0..12 {
            s.step(&[PadState::default()]);
            if s.events.iter().any(|e| matches!(e, Event::Hit { .. })) {
                hit_tick = Some(s.tick);
                break;
            }
        }
        // 19.5/80 = 0.244 s → 8th tick (ticks 1..8 cover 0.033..0.267 s).
        assert_eq!(hit_tick, Some(8), "impact tick");
        assert!(s.unit(t).unwrap().health.cur < 300.0);
        assert_eq!(s.unit(p).unwrap().weapon.clip, 49);
        // The hit's age counts the updates the bolt came through before the
        // one it lands in (card SGDMG): 7 of them, the flight's 0.244 s
        // rounded down to the tick.
        let end = s.events.iter().find_map(|e| match e {
            Event::Hit { age, .. } => Some(*age),
            _ => None,
        });
        assert!((end.unwrap() - 7.0 / 30.0).abs() < 1e-5, "{end:?}");
        // The hit carries the way the bolt flew (card HATK's chevrons):
        // from the shooter toward the target.
        let dir = s.events.iter().find_map(|e| match e {
            Event::Hit { dir, .. } => Some(*dir),
            _ => None,
        });
        let dir = dir.unwrap();
        assert!((dir - way).length() < 1e-6, "{dir} {way}");
        let at = (s.unit(t).unwrap().soldier.pos - s.unit(p).unwrap().soldier.pos).normalize();
        assert!(dir.dot(at) > 0.99, "{dir} {at}");
    }

    /// §9.6 TTK: five BF1 rifle hits kill a 300 HP trooper; the kill pays
    /// the victim team's ticket and alive count (core::battle wiring).
    #[test]
    fn v6_five_shots_kill_and_pay_a_ticket() {
        let (mut s, p, t) = duel();
        assert_eq!(s.battle.teams[1].tickets(), 250);
        assert_eq!(s.battle.teams[1].alive(), 1);
        let mut kill_tick = None;
        // Each hit reports the health it left (card UCRY's hurt test): the
        // surviving ones what the unit holds after it, the killing one 0 or
        // less.
        let mut left = Vec::new();
        for _ in 0..90 {
            s.step(&[fire()]);
            for e in &s.events {
                if let Event::Hit { health_left, .. } = e {
                    left.push((*health_left, s.unit(t).map(|u| u.health.cur)));
                }
            }
            if s.events.iter().any(|e| matches!(e, Event::Kill { .. })) {
                kill_tick = Some(s.tick);
                break;
            }
        }
        let k = kill_tick.expect("target killed within 3 s");
        let (last, alive) = left.pop().expect("hits");
        assert!(last <= 0.0 && alive.is_none(), "{last}");
        assert_eq!(left.len(), 4);
        assert!(
            left.iter().all(|(l, now)| *now == Some(*l) && *l > 0.0),
            "{left:?}"
        );
        // Shots at 0, .2, .4, .6, .8 s (+0.244 flight) → kill ≈ 1.04 s = tick 32.
        assert!((31..=33).contains(&k), "kill tick {k}");
        assert!(s.unit(t).is_none(), "dead unit handle invalid");
        assert_eq!(s.units[t.index as usize].deaths, 1);
        assert_eq!(s.unit(p).unwrap().kills, 1);
        assert_eq!(s.battle.teams[1].tickets(), 249);
        assert_eq!(s.battle.teams[1].alive(), 0);
        // Further bolts at the dead unit's slot do nothing.
        assert_eq!(s.unit(p).unwrap().weapon.shots_fired, 6);
    }

    /// Issue #41: a bolt that meets a soldier's head prim does the PS2's
    /// HitLocation x3 - a human's always, a bot's only on Hard and Elite
    /// (the PS2's level 3). The same bolt at his chest is the x1.
    #[test]
    fn a_headshot_takes_three_times_the_damage() {
        // A bolt from 20 m at a still soldier's head prim (1.65 m up, 0.16 m
        // ahead of his feet) or his chest (the torso, 1.15 m), by a human or
        // a bot of team 1: the damage it does.
        fn shot(human: bool, d: Difficulty, y: f32) -> f32 {
            let (mut s, p, t) = duel();
            s.difficulty = d;
            s.unit_mut(t).unwrap().health.max = 3000.0;
            s.unit_mut(t).unwrap().health.cur = 3000.0;
            // The target faces the shooter (+Z), so his head is 0.16 m nearer.
            s.unit_mut(t).unwrap().soldier.yaw = std::f32::consts::PI;
            let z = if y > 1.5 { -20.0 + 0.16 } else { -20.0 };
            let from = Vec3::new(0.0, y, 0.0);
            let b = Bolt::fire(
                from,
                Vec3::new(0.0, y, z) - from,
                80.0,
                1.5,
                crate::projectile::Shooter {
                    owner: p,
                    team: 0,
                    weapon_class: 0,
                    is_player: human,
                },
                s.heightfield.as_ref(),
                s.water_plane(),
                &s.collision,
                crate::collision::HullFilter::All,
            );
            s.bolts.push(b);
            for _ in 0..30 {
                s.step(&[PadState::default()]);
                if let Some(d) = s.events.iter().find_map(|ev| match ev {
                    Event::Hit { damage, .. } => Some(*damage),
                    _ => None,
                }) {
                    return d;
                }
            }
            panic!("no hit (human {human}, {d:?}, y {y})");
        }
        let (head, chest) = (1.65, 1.15);
        let body = shot(true, Difficulty::Normal, chest);
        assert!(body > 0.0);
        // The PS2's x3, not the constant (a test of the rule, not of itself).
        assert_eq!(shot(true, Difficulty::Normal, head), body * 3.0);
        assert_eq!(shot(false, Difficulty::Normal, head), body, "a bot: no x3");
        assert_eq!(
            shot(false, Difficulty::Hard, head),
            shot(false, Difficulty::Hard, chest) * 3.0,
            "a bot on the PS2's top level: x3"
        );
    }

    /// Issue #41 (OURS; the PS2's one ray at the press, report §5.1, hit a
    /// target that stepped out of the path after the shot and never one
    /// that stepped in): a bolt strikes what it passes through. A target
    /// that steps aside mid-flight is missed; a unit that steps into the
    /// path is hit.
    #[test]
    fn a_bolt_hits_what_it_passes_through() {
        let (mut s, _p, t) = duel();
        s.step(&[fire()]);
        // Teleport the target sideways mid-flight.
        s.unit_mut(t).unwrap().soldier.pos.x += 10.0;
        for _ in 0..10 {
            s.step(&[PadState::default()]);
            assert!(!s.events.iter().any(|e| matches!(e, Event::Hit { .. })));
        }
        assert_eq!(s.unit(t).unwrap().health.cur, 300.0, "the dodger is missed");

        // Late entrant: fire at nothing, then move a unit into the path.
        let mut s2 = Sim::empty(3);
        s2.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
        let late = s2
            .spawn(&UnitSpec::trooper(1, Vec3::new(30.0, 0.0, -20.0)))
            .unwrap();
        s2.step(&[fire()]);
        assert_eq!(s2.bolts[0].hit, Hit::None);
        s2.unit_mut(late).unwrap().soldier.pos.x = 0.0;
        let mut hit = false;
        for _ in 0..60 {
            s2.step(&[PadState::default()]);
            hit |= s2.events.iter().any(|e| matches!(e, Event::Hit { .. }));
        }
        assert!(hit, "the unit that stepped in is hit");
        assert!(s2.unit(late).unwrap().health.cur < 300.0);
    }

    /// Terrain in the way: the bolt ends on the slope, nobody is hurt.
    #[test]
    fn terrain_blocks_bolts() {
        let mut s = Sim::empty(5);
        // Wall: height jumps to 50 m for z < -10.
        s.heightfield = Some(Heightfield {
            size: 3,
            unit: 10.0,
            offset_x: 10.0,
            offset_z: 10.0,
            heights: vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        });
        // Terrain heights are per grid vertex; make row z=-10 tall.
        let hf = s.heightfield.as_mut().unwrap();
        hf.heights[0] = 50.0;
        hf.heights[1] = 50.0;
        hf.heights[2] = 50.0;
        s.spawn(&UnitSpec::player(0, Vec3::new(0.0, 0.0, 8.0)))
            .unwrap();
        let t = s
            .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -9.0)))
            .unwrap();
        s.step(&[fire()]);
        assert_eq!(s.bolts[0].hit, Hit::Terrain, "{:?}", s.bolts[0]);
        for _ in 0..60 {
            s.step(&[PadState::default()]);
        }
        let _ = t;
        assert!(s.units.iter().all(|u| u.health.cur == 300.0));
    }

    #[test]
    fn water_stops_bolts_and_names_the_surface() {
        let mut s = Sim::empty(5);
        s.water_level = Some(-2.0);
        let owner = s
            .spawn(&UnitSpec::trooper(0, Vec3::new(0.0, 0.0, 30.0)))
            .unwrap();
        let shooter = crate::projectile::Shooter {
            owner,
            team: 0,
            weapon_class: 0,
            is_player: false,
        };
        let down = Vec3::new(0.0, -1.0, -1.0);
        let spawn = |s: &Sim, dir: Vec3| {
            Bolt::spawn(
                Vec3::new(0.0, 2.0, 0.0),
                dir,
                80.0,
                2.0,
                shooter,
                s.heightfield.as_ref(),
                s.water_plane(),
                &s.collision,
                crate::collision::HullFilter::All,
                std::iter::empty(),
            )
        };
        let b = spawn(&s, down);
        assert_eq!(b.hit, Hit::Water);
        assert!(
            (b.end - Vec3::new(0.0, -2.0, -4.0)).length() < 1e-4,
            "{b:?}"
        );
        assert_eq!(spawn(&s, Vec3::new(0.0, 0.2, -1.0)).hit, Hit::None);
        let mut nibbles = vec![0u8; 4];
        nibbles[0] = 1;
        s.water_mask = crate::water::WetMask::new(2, 10.0, [10.0, 10.0], &nibbles);
        assert_eq!(
            spawn(&s, down).hit,
            Hit::None,
            "the crossing at z -4 is a dry cell"
        );
        assert_eq!(spawn(&s, Vec3::new(-1.0, -1.0, -1.0)).hit, Hit::Water);
        assert_eq!(s.water_under(Vec3::new(5.0, -3.0, -5.0)), None);
        assert_eq!(s.water_under(Vec3::new(-5.0, -3.0, -5.0)), Some(-2.0));
        s.water_mask = None;
        s.heightfield = Some(flat_ground());
        assert_eq!(spawn(&s, down).hit, Hit::Terrain);
        s.heightfield = None;
        s.water_level = None;
        assert_eq!(spawn(&s, down).hit, Hit::None);
        s.bolts.push(b);
        for _ in 0..10 {
            s.step(&[PadState::default()]);
            if !s.events.is_empty() {
                break;
            }
        }
        assert!(s.bolts.is_empty());
        assert!(s.events.iter().any(|e| matches!(
            e,
            Event::Impact {
                surface: Surface::Water,
                pos,
                ..
            } if (*pos - Vec3::new(0.0, -2.0, -4.0)).length() < 1e-4
        )));
    }

    /// A WORLD model in the way: the bolt stops on the slab's near face and
    /// the trooper standing behind it is never touched. Before the world ray
    /// a bolt flew through every `coll` mesh and `prim` on the map.
    #[test]
    fn world_model_blocks_bolts() {
        use crate::collision::{Body, Shape};
        use glam::Mat4;

        // A 1 m slab across the path, y in (0, 10), near face at z = −9.5 —
        // between the shooter at z = 0 and the trooper at z = −20.
        let wall = || {
            Body::new(
                Mat4::from_translation(Vec3::new(0.0, 5.0, -10.0)),
                Shape::Box {
                    half: Vec3::new(5.0, 5.0, 0.5),
                },
            )
        };

        let (mut s, p, t) = duel();
        s.collision.push_body(wall());
        // A human's bolt goes to the reticle's point on the face (card AIMA).
        let aim = s.aim_hit(p.index as usize);
        s.step(&[fire()]);
        let b = s.bolts[0].clone();
        assert_eq!(b.hit, Hit::World, "{b:?}");
        // ON the face, not merely on the plane it lies in: the near plane in
        // z, and inside the slab's own x/y extent.
        assert!((b.end.z + 9.5).abs() < 1e-3, "{}", b.end);
        assert!(b.end.x.abs() <= 5.0, "{}", b.end);
        assert!((0.0..=10.0).contains(&b.end.y), "{}", b.end);
        // What is left over is the weapon's own spread across 9.5 m (~2 cm).
        assert!((b.end - aim).length() < 0.05, "{} vs {aim}", b.end);
        // The capsule at 19.5 m is BEHIND the wall and is not the hit — and
        // stays unhurt for the rest of the bolt's would-be flight.
        for _ in 0..60 {
            s.step(&[PadState::default()]);
        }
        assert!(
            !s.events.iter().any(|e| matches!(e, Event::Hit { .. })),
            "nothing behind the wall is hit"
        );
        assert_eq!(s.unit(t).unwrap().health.cur, 300.0);

        // A world hit expires like a terrain hit: the same struck-something
        // event, so the same impact puff and impact sound.
        let (mut s, _, _) = duel();
        s.collision.push_body(wall());
        s.step(&[fire()]);
        let mut expired = None;
        let mut impact = None;
        for _ in 0..60 {
            s.step(&[PadState::default()]);
            impact = s
                .events
                .iter()
                .find_map(|e| matches!(e, Event::Impact { .. }).then_some(*e));
            if let Some(e) = s
                .events
                .iter()
                .find_map(|e| matches!(e, Event::BoltExpired { .. }).then_some(*e))
            {
                expired = Some(e);
                break;
            }
        }
        // FXP3: the wall is a building to the impact effect.
        match impact {
            Some(Event::Impact {
                pos,
                surface,
                vehicle,
                weapon_class,
                dir,
            }) => {
                assert_eq!(surface, crate::impact::Surface::Building);
                assert_eq!((vehicle, weapon_class), (None, 0));
                assert!((pos.z + 9.5).abs() < 1e-3, "{pos}");
                assert!(dir.z < -0.99, "{dir}");
            }
            other => panic!("no Impact: {other:?}"),
        }
        match expired {
            Some(Event::BoltExpired { pos, struck }) => {
                assert!(struck, "a wall is something struck");
                assert!((pos.z + 9.5).abs() < 1e-3, "{pos}");
            }
            other => panic!("no BoltExpired: {other:?}"),
        }

        // The control: without the wall the identical shot reaches the
        // trooper, so it is the body and nothing else that stopped it.
        let (mut s, _, _) = duel();
        s.step(&[fire()]);
        // … and a soldier is soft to the impact effect (FXP3), beside the
        // hit and with no expiry.
        for _ in 0..60 {
            s.step(&[PadState::default()]);
            if s.events.iter().any(|e| matches!(e, Event::Hit { .. })) {
                break;
            }
        }
        assert!(s.events.iter().any(|e| matches!(
            e,
            Event::Impact {
                surface: crate::impact::Surface::Soldier,
                vehicle: None,
                ..
            }
        )));
        assert!(!s
            .events
            .iter()
            .any(|e| matches!(e, Event::BoltExpired { .. })));
    }

    /// Cheats: invincible player takes no damage; infinite ammo never reloads.
    #[test]
    fn cheats_invincible_and_infinite_ammo() {
        let mut s = Sim::empty(9);
        // The protected player stands idle (Idle controller, is_player set);
        // the pad drives an enemy trooper facing them.
        let p = s
            .spawn(&UnitSpec {
                controller: Controller::Idle,
                ..UnitSpec::player(0, Vec3::ZERO)
            })
            .unwrap();
        let e = s
            .spawn(&UnitSpec {
                controller: Controller::Player { slot: 0 },
                yaw: std::f32::consts::PI,
                ..UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -20.0))
            })
            .unwrap();
        s.players[0] = Some(e);
        s.cheats.invincible = true;
        s.cheats.infinite_ammo = true;
        for _ in 0..60 {
            s.step(&[fire()]);
        }
        assert!(s.units[e.index as usize].weapon.shots_fired >= 9);
        assert_eq!(
            s.units[p.index as usize].health.cur, 300.0,
            "invincible player untouched"
        );
        // The shooter here is not flagged is_player, so its clip drains.
        assert!(s.units[e.index as usize].weapon.clip < 50);
        assert!(s.events.iter().all(|ev| !matches!(ev, Event::Kill { .. })));
        assert!(s.unit(p).is_some());
    }

    /// Two AI troopers facing each other at 25 m: one dies within 15 s (card
    /// CLSE4: bursts of 3-5 with 1.7 s between them), and the whole thing is
    /// seed-deterministic.
    #[test]
    fn ai_duel_resolves_and_is_deterministic() {
        let run = |seed: u32| {
            let mut s = Sim::empty(seed);
            s.spawn(&ai(0, Vec3::ZERO, 0.0)).unwrap();
            s.spawn(&ai(1, Vec3::new(0.0, 0.0, -25.0), std::f32::consts::PI))
                .unwrap();
            let mut kill_tick = None;
            for _ in 0..450 {
                s.step(&[PadState::default()]);
                if kill_tick.is_none() && s.events.iter().any(|e| matches!(e, Event::Kill { .. })) {
                    kill_tick = Some(s.tick);
                }
            }
            (kill_tick, s.hash())
        };
        let (k1, h1) = run(21);
        let (k2, h2) = run(21);
        assert!(k1.is_some(), "somebody died within 15 s");
        assert_eq!((k1, h1), (k2, h2), "deterministic");
        let (_, h3) = run(22);
        assert_ne!(h1, h3, "seed changes the spread rolls");
    }

    /// D12 (a): the tier's `human_damage` multiplies a BOT's bolt into a
    /// HUMAN and nothing else — not a human's own bolt, not a bot-on-bot
    /// bolt, and not at all with the host option off.
    #[test]
    fn the_human_damage_layer_is_bot_on_human_only() {
        // The victim stands idle; the shooter is a BOT (`UnitSpec::trooper`,
        // `is_player` false) driven by the pad, so the shots are the same
        // shots in every arm and only the multiplier can differ.
        fn shoot(d: Difficulty, on: bool, victim_is_human: bool) -> f32 {
            let mut s = Sim::empty(9);
            s.difficulty = d;
            s.human_damage_on = on;
            let victim = if victim_is_human {
                UnitSpec {
                    controller: Controller::Idle,
                    ..UnitSpec::player(0, Vec3::ZERO)
                }
            } else {
                UnitSpec {
                    controller: Controller::Idle,
                    ..UnitSpec::trooper(0, Vec3::ZERO)
                }
            };
            let v = s.spawn(&victim).unwrap();
            // Enough health that every arm's hits are counted whole.
            s.unit_mut(v).unwrap().health.max = 3000.0;
            s.unit_mut(v).unwrap().health.cur = 3000.0;
            let full = s.unit(v).unwrap().health.cur;
            let e = s
                .spawn(&UnitSpec {
                    controller: Controller::Player { slot: 0 },
                    yaw: std::f32::consts::PI,
                    ..UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -20.0))
                })
                .unwrap();
            // Slot 0 drives the shooter (a bot victim registers no slot of
            // its own, so the table may still be empty here).
            s.players.resize(1, None);
            s.players[0] = Some(e);
            // At his chest: a level shot meets the head prim, and the
            // headshot's x3 (issue #41) is not what this compares.
            s.units[e.index as usize].soldier.pitch = -0.08;
            // Short enough that the victim SURVIVES in every arm: past the
            // kill the damage would clip at the killing bolt and the arms
            // would no longer be comparable.
            for _ in 0..15 {
                s.step(&[fire()]);
            }
            assert!(s.unit(v).is_some(), "victim survives the sample window");
            full - s.units[v.index as usize].health.cur
        }

        let normal = shoot(Difficulty::Normal, true, true);
        assert!(normal > 0.0, "the bot landed shots: {normal}");
        let elite = shoot(Difficulty::Elite, true, true);
        let m = Difficulty::Elite.tier().human_damage;
        assert!(
            (elite - normal * m).abs() < 1e-3,
            "elite {elite} vs normal {normal} x {m}"
        );
        // The three controls: the option off, a bot victim, and Easy/Normal
        // (whose multiplier is 1.0) all take the unmultiplied damage.
        assert!((shoot(Difficulty::Elite, false, true) - normal).abs() < 1e-3);
        assert!(
            (shoot(Difficulty::Elite, true, false) - shoot(Difficulty::Normal, true, false)).abs()
                < 1e-3
        );
        assert_eq!(Difficulty::Easy.tier().human_damage, 1.0);
    }

    /// The BF2 shotgun pellet (`com_weap_inf_shotgun`: 2000 m/s, 0.1 s,
    /// MaxDamage 65 falling to 15 over 0.1 s) as PCSX2 measured it at 3 m
    /// (`re-ordnance-kinds.md` §7): a pellet that lands in its first update
    /// does the full 65 to the body and 195 to the head, so one point-blank
    /// body shot is 8 x 65 = 520. One that needs a second update (past
    /// 66.7 m) is a tick old: 48.3.
    #[test]
    fn a_pellet_landing_in_its_first_update_does_its_full_damage() {
        use crate::projectile::{Bolt, Hit};
        fn hit(dist: f32, height: f32) -> f32 {
            let mut s = Sim::empty(3);
            s.heightfield = Some(flat_ground());
            let o = &mut s.weapon_classes[0].ordnance;
            o.velocity = 2000.0;
            o.life_span = 0.1;
            o.max_damage = 65.0;
            o.falloff = Some(crate::weapon::Falloff {
                start_time: 0.0,
                duration: 0.1,
                end_damage: 15.0,
            });
            let shooter = s
                .spawn(&UnitSpec {
                    controller: Controller::Idle,
                    ..UnitSpec::trooper(0, Vec3::new(0.0, 0.0, 500.0))
                })
                .unwrap();
            let v = s
                .spawn(&UnitSpec {
                    controller: Controller::Idle,
                    ..UnitSpec::trooper(1, Vec3::ZERO)
                })
                .unwrap();
            s.unit_mut(v).unwrap().health.max = 3000.0;
            s.unit_mut(v).unwrap().health.cur = 3000.0;
            let feet = s.unit(v).unwrap().soldier.pos;
            let pos = feet + Vec3::new(-dist, height, 0.0);
            let vel = Vec3::X * 2000.0;
            s.bolts.push(Bolt {
                pos,
                vel,
                remaining: 0.1,
                age: 0.0,
                owner: shooter,
                team: 0,
                weapon_class: 0,
                hit: Hit::None,
                end: pos + vel * 0.1,
                shooter_is_player: true,
                hulls: crate::collision::HullFilter::All,
                hit_mult: 1.0,
                gravity: 0.0,
                charge: 1.0,
            });
            for _ in 0..3 {
                s.step(&[]);
            }
            3000.0 - s.unit(v).unwrap().health.cur
        }
        assert!((hit(3.0, 1.0) - 65.0).abs() < 1e-3, "{}", hit(3.0, 1.0));
        assert!((hit(3.0, 1.65) - 195.0).abs() < 1e-3, "{}", hit(3.0, 1.65));
        let far = 65.0 - 50.0 / 3.0;
        assert!((hit(100.0, 1.0) - far).abs() < 1e-3, "{}", hit(100.0, 1.0));
    }
}
