//! Card GREN in the tick (`analysis/re-grenades.md` [O]): a soldier's two
//! weapon channels — which trigger has it, the two switch inputs, the held
//! weapons' cycles (§2.3, §2.4) — the throw (§3.3) and the thrown grenades'
//! flight, sweep, contact, fuse and blast (§4, [`crate::sticky`]).

use super::input::{buttons, PadState};
use super::state::{Event, Sim};
use crate::collision::{ray_terrain_ps2, HullFilter, Layers};
use crate::sticky::{self, Anchor, Contact, Sticky, Surface};
use crate::unit::Controller;
use crate::vehicle::VehicleKind;
use crate::weapon::{FireInput, FireState};
use glam::Vec3;

/// How long a grenade that has touched down is expected to keep bouncing
/// before its fuse starts (s) — the bots' warning needs an end time at the
/// first contact, the fuse only starts at the rest. PROVISIONAL (ours): the
/// PS2 rows rested 1.3 s after their first contact (§8); corrected at the
/// rest.
const HAZARD_BOUNCE_TIME: f32 = 1.5;

impl Sim {
    /// Phase 4 for a soldier that holds more than one weapon: pick the
    /// active channel, step the two switch inputs, run every held weapon's
    /// cycle and throw what channel 1 released. Returns whether the PRIMARY
    /// trigger reaches channel 0's weapon this tick.
    ///
    /// §2.3 (B1 `FUN_00205cc8` 117099-117161): channel 1 has the soldier
    /// while its weapon's fire state runs, or while its trigger is down on a
    /// weapon that is not out of ammo; the other trigger is swallowed. §2.4:
    /// a switch input steps its channel to the next slot of that channel
    /// (wrapping, at once) and only while neither trigger is down.
    pub(super) fn held_weapons(&mut self, i: usize, p: &PadState, blocked: bool, dt: f32) -> bool {
        let pri = p.pressed(buttons::FIRE);
        let sec = p.pressed(buttons::SECONDARY_FIRE);
        let infinite = self.cheats.infinite_ammo && self.units[i].is_player;

        // The active channel.
        let ch1_has_it = self.units[i].cur1.is_some_and(|k| {
            let w = &self.units[i].held[k as usize];
            let c = &self.weapon_classes[w.class];
            w.state.state == FireState::Firing
                || ((sec || w.state.fire_cmd != 0) && (infinite || !w.state.out_of_ammo(c)))
        });
        self.units[i].active_channel = u8::from(ch1_has_it);
        let held0 = pri && !ch1_has_it;
        let held1 = sec && ch1_has_it;

        // The switches: edges, and dead while a channel is active.
        let now = u8::from(p.pressed(buttons::SWITCH_WEAPON))
            | u8::from(p.pressed(buttons::SWITCH_SECONDARY)) << 1;
        let edges = now & !self.units[i].switch_held;
        self.units[i].switch_held = now;
        // A trigger let go last tick still counts (the PS2's release byte).
        let released = self.units[i].weapon.fire_cmd > 0;
        if !pri && !sec && !ch1_has_it && !released {
            if edges & 1 != 0 {
                self.switch_channel0(i);
            }
            if edges & 2 != 0 {
                self.switch_channel1(i);
            }
        }

        // Every held weapon's cycle; only channel 1's current one sees a
        // trigger. An idle cycle draws nothing from the LCG.
        let cur1 = self.units[i].cur1.map(usize::from);
        let mut shots = Vec::new();
        for k in 0..self.units[i].held.len() {
            let class = self.units[i].held[k].class;
            if self.tick_repair_weapon(i, Some(k), Some(k) == cur1 && held1, blocked, dt) {
                continue;
            }
            let input = FireInput {
                held: Some(k) == cur1 && held1,
                blocked,
                infinite_ammo: infinite,
                ai_dispersion: None,
            };
            shots.clear();
            let wc = &self.weapon_classes[class];
            self.units[i].held[k]
                .state
                .update(dt, wc, input, &mut self.lcg, &mut shots);
            for _ in 0..shots.len() {
                self.launch(i, class);
            }
        }
        held0
    }

    /// Step channel 0 to its next slot: the weapon in the hands trades
    /// places with the held one (§2.4; the same slot again does nothing).
    fn switch_channel0(&mut self, i: usize) {
        let u = &mut self.units[i];
        let cur = u.weapon_slot;
        let next = u
            .held
            .iter()
            .enumerate()
            .filter(|(_, w)| w.channel == 0)
            .min_by_key(|(_, w)| (w.slot < cur, w.slot))
            .map(|(k, _)| k);
        let Some(k) = next else {
            return;
        };
        let w = &mut u.held[k];
        std::mem::swap(&mut u.weapon, &mut w.state);
        std::mem::swap(&mut u.weapon_class, &mut w.class);
        std::mem::swap(&mut u.weapon_slot, &mut w.slot);
        w.state.active = false;
        u.weapon.active = true;
        // A switch of the weapon in hand ends the zoom (`re-zoom.md` §4).
        u.zoom = None;
        let weapon_class = u.weapon_class as u32;
        let unit = self.handle(i);
        self.events.push(Event::WeaponSelect { unit, weapon_class });
    }

    /// Step channel 1 to its next slot (§2.4).
    fn switch_channel1(&mut self, i: usize) {
        let u = &mut self.units[i];
        let Some(cur) = u.cur1.map(usize::from) else {
            return;
        };
        let slot = u.held[cur].slot;
        let next = u
            .held
            .iter()
            .enumerate()
            .filter(|(k, w)| w.channel == 1 && *k != cur)
            .min_by_key(|(_, w)| (w.slot < slot, w.slot))
            .map(|(k, _)| k);
        if let Some(k) = next {
            u.held[cur].state.active = false;
            u.held[k].state.active = true;
            u.cur1 = Some(k as u8);
            let weapon_class = u.held[k].class as u32;
            let unit = self.handle(i);
            self.events.push(Event::WeaponSelect { unit, weapon_class });
        }
    }

    /// One shot of weapon class `wc` from unit `i`, when that weapon throws:
    /// spawn the grenade (§3.3) and say so. `false` for a weapon that fires
    /// bolts — the caller's business.
    pub(super) fn launch(&mut self, i: usize, wc: usize) -> bool {
        let class = &self.weapon_classes[wc];
        let Some(sc) = class.ordnance.sticky.as_ref() else {
            return false;
        };
        let u = &self.units[i];
        let (aim, _, _) = u.aim_frame();
        let thrown = class.thrown;
        // A human's throw is lifted by ShotElevate; a bot's aim is used as it
        // is (B1 215847-215862, the owner's player index).
        let dir = match thrown {
            Some(t) if matches!(u.controller, Controller::Player { .. }) => {
                sticky::throw_dir(aim, t.shot_elevate)
            }
            _ => aim,
        };
        // BF1: the weapon's fire point; BF2: the hand, in the soldier's frame.
        let origin = match thrown.and_then(|t| t.hand) {
            Some((up, fwd)) => {
                let (sy, cy) = crate::fmath::sin_cos(u.soldier.yaw);
                u.soldier.pos + Vec3::Y * up + Vec3::new(-sy, 0.0, -cy) * fwd
            }
            None => u.muzzle(),
        };
        let owner = self.handle(i);
        let g = Sticky::new(
            origin,
            dir * class.ordnance.velocity,
            sc,
            class.ordnance.life_span,
            owner,
            u.team,
            wc,
        );
        self.stickies.push(g);
        self.events.push(Event::Throw {
            unit: owner,
            weapon_class: wc as u32,
            origin,
        });
        true
    }

    /// Phase 5b: every thrown grenade's tick, in throw order — the fuse, the
    /// flight, then the sweep and its contact (§4.1, §4.2: the PS2 updates
    /// the object and its collision manager sweeps it afterwards).
    pub(super) fn tick_stickies(&mut self, dt: f32) {
        if self.stickies.is_empty() {
            return;
        }
        let mut list = std::mem::take(&mut self.stickies);
        list.retain_mut(|g| self.tick_sticky(g, dt));
        // Nothing can throw during the pass; keep the order regardless.
        list.append(&mut self.stickies);
        self.stickies = list;
    }

    /// One grenade's tick; `false` when it is gone.
    fn tick_sticky(&mut self, g: &mut Sticky, dt: f32) -> bool {
        if g.burn(dt) {
            if !g.lost() {
                self.detonate(g);
            }
            return false;
        }
        match g.anchor {
            Some(Anchor::Vehicle { handle, local }) => {
                if let Some(v) = self.vehicle(handle) {
                    g.pos = v.pos + v.rot * local;
                }
            }
            Some(Anchor::Unit { handle, local }) => {
                if let Some(u) = self.unit(handle) {
                    g.pos = u.soldier.pos + local;
                }
            }
            _ => {}
        }
        if !g.moving {
            return true;
        }
        let prev = g.pos;
        let level = self.water_under(prev);
        g.fly(dt, level.is_some_and(|l| prev.y < l));
        let step = g.pos - prev;
        let len = step.length();
        if len <= 0.0 {
            return true;
        }
        let dir = step / len;
        let Some((point, n, what)) = self.sweep_sticky(g, prev, dir, len) else {
            return true;
        };
        let wc = g.weapon_class;
        let Some(class) = self.weapon_classes[wc].ordnance.sticky.clone() else {
            return false;
        };
        let life_span = self.weapon_classes[wc].ordnance.life_span;
        let first = g.touched.is_none();
        if first {
            g.touched = Some(self.tick);
        }
        let room = self.push_off_room(point, n);
        let out = g.contact(&class, point, n, what, room);
        if out == Contact::Stuck {
            g.anchor = Some(match what {
                Surface::Vehicle { handle, .. } => match self.vehicle(handle) {
                    Some(v) => Anchor::Vehicle {
                        handle,
                        local: v.rot.inverse() * (g.pos - v.pos),
                    },
                    None => Anchor::World,
                },
                Surface::Unit { handle, .. } => match self.unit(handle) {
                    Some(u) => Anchor::Unit {
                        handle,
                        local: g.pos - u.soldier.pos,
                    },
                    None => Anchor::World,
                },
                _ => Anchor::World,
            });
        }
        self.events.push(Event::StickyContact {
            pos: g.pos,
            weapon_class: wc as u32,
            water: out == Contact::Water,
        });
        // The bots' warning (card HOLD, §7.4): from the first contact, and
        // put right — the place and the real fuse — once it lies still.
        let ticks = |s: f32| (s * crate::TICK_HZ as f32).ceil() as u64;
        if first {
            let id = self
                .last_warning_id
                .checked_add(1)
                .expect("grenade warning identity exhausted");
            self.add_hazard_by(g.pos, ticks(g.life + HAZARD_BOUNCE_TIME), Some(g.owner));
            self.last_warning_id = id;
            g.warning_id = Some(id);
            self.hazards
                .last_mut()
                .expect("warning just inserted")
                .warning_id = Some(id);
        }
        if matches!(out, Contact::Rest | Contact::Stuck) {
            let now = self.tick;
            let fuse = ticks(g.life.min(life_span));
            if let Some(z) = self
                .hazards
                .iter_mut()
                .find(|z| g.warning_id.is_some() && z.warning_id == g.warning_id)
            {
                z.pos = g.pos;
                z.until = now + fuse;
            }
        }
        true
    }

    /// The sweep ray (§4.2, §13.2; B1 `FUN_0027eff8`, B2 `FUN_00320de8`):
    /// from [`sticky::SWEEP_BACK`] behind the previous position over the
    /// step plus [`sticky::SWEEP_EXTRA`] — so 0.1 m past the new position —
    /// the owner ignored. The nearest of the water surface, the terrain by
    /// BF2's own segment test ([`ray_terrain_ps2`]: not the crossing), the
    /// world's bodies and hulls (PROVISIONAL: the ordnance subset, as a bolt
    /// — the PS2's mask is 0x9de / 0x13ba) and the soldiers.
    ///
    /// DEVIATION (card GRFL, PROVISIONAL): the bodies' ray is NOT pulled
    /// back. A bounce leaves the grenade [`sticky::PUSH_OFF`] off the face
    /// and a steep one leaves along the normal, so a ray from
    /// [`sticky::SWEEP_BACK`] behind starts ON the face; the port's mesh ray
    /// meets a face from either side, took it from behind and bounced the
    /// grenade under the floor (8.2 % of the census's grenades, the maintainer's
    /// Kamino decks). The PS2's grenades rest on those decks; what its mesh
    /// test does behind the origin is unread (re-grenades §13.2), and the
    /// baked meshes' windings are mixed, so a one-sided ray is no answer.
    /// How far a bounce may push the grenade off along `n`
    /// ([`sticky::PUSH_OFF`]), or half the way to a surface it would meet
    /// first. Ours (issue #37): the underside of a step's lip, 2.5 cm over
    /// the floor, pushed it 4.5 cm down through that floor.
    fn push_off_room(&self, point: Vec3, n: Vec3) -> f32 {
        let start = point + n * sticky::PUSH_ROOM_START;
        let len = sticky::PUSH_OFF - sticky::PUSH_ROOM_START;
        let mut t = len;
        if let Some(h) = self
            .collision
            .ray_with(start, n, len, Layers::ORDNANCE, HullFilter::All)
        {
            t = t.min(h.t);
        }
        if let Some(hf) = self.heightfield.as_ref() {
            if let Some(f) = ray_terrain_ps2(hf, &self.collision.cutters, start, start + n * len) {
                t = t.min(len * f);
            }
        }
        if t < len {
            sticky::PUSH_ROOM_START + t * 0.5
        } else {
            sticky::PUSH_OFF
        }
    }

    fn sweep_sticky(
        &self,
        g: &Sticky,
        prev: Vec3,
        dir: Vec3,
        len: f32,
    ) -> Option<(Vec3, Vec3, Surface)> {
        let origin = prev - dir * sticky::SWEEP_BACK;
        let max = len + sticky::SWEEP_EXTRA;
        let mut best: Option<(f32, Vec3, Surface)> = None;
        if let Some(hf) = self.heightfield.as_ref() {
            let end = origin + dir * max;
            if let Some(f) = ray_terrain_ps2(hf, &self.collision.cutters, origin, end) {
                let t = max * f;
                let p = origin + dir * t;
                let n = Vec3::from(hf.normal_at(p.x, p.z));
                best = Some((t, n, Surface::Terrain));
            }
        }
        let mut nearer = |t: f32, n: Vec3, s: Surface| {
            if best.is_none_or(|b| t < b.0) {
                best = Some((t, n, s));
            }
        };
        if let Some(t) = self
            .water_plane()
            .filter(|w| prev.y >= w.level)
            .and_then(|w| w.crossing(origin, dir))
        {
            if t <= max {
                nearer(t, Vec3::Y, Surface::Water);
            }
        }
        // The bodies' ray starts AT the previous position and ends where the
        // others do (card GRFL, a named deviation: see the header above).
        let back = sticky::SWEEP_BACK;
        if let Some(mut h) =
            self.collision
                .ray_with(prev, dir, max - back, Layers::ORDNANCE, HullFilter::All)
        {
            h.t += back;
            let s = match h.owner {
                Some(v) => Surface::Vehicle {
                    handle: v,
                    building: self
                        .vehicle(v)
                        .is_some_and(|v| v.kind == VehicleKind::Turret),
                },
                None => Surface::World,
            };
            nearer(h.t, h.normal, s);
        }
        for (h, u) in self.alive_units() {
            if h == g.owner || u.mount.is_some() {
                continue;
            }
            let cap = u.capsule();
            if let Some(t) = cap.ray_hit(origin, dir, max) {
                // The capsule's side: the normal is level, away from its axis.
                let p = origin + dir * t;
                let away = Vec3::new(p.x - cap.base.x, 0.0, p.z - cap.base.z);
                let n = away.try_normalize().unwrap_or(Vec3::Y);
                let kind = u.kind();
                nearer(t, n, Surface::Unit { handle: h, kind });
            }
        }
        best.map(|(t, n, s)| (origin + dir * t, n, s))
    }

    /// The fuse ran out: the `ExplosionExpire` class goes off where the
    /// grenade lies, the thrower credited (§4.5).
    fn detonate(&mut self, g: &Sticky) {
        let wc = g.weapon_class;
        self.events.push(Event::StickyBlast {
            pos: g.pos,
            weapon_class: wc as u32,
        });
        if g.warning_id.is_some() {
            let now = self.tick;
            if let Some(z) = self
                .hazards
                .iter_mut()
                .find(|z| g.warning_id.is_some() && z.warning_id == g.warning_id)
            {
                z.until = now;
            }
        }
        let explosion = self.weapon_classes[wc]
            .ordnance
            .sticky
            .as_ref()
            .and_then(|s| s.explosion.clone());
        if let Some(e) = explosion {
            self.explode(&e, g.pos, Some(g.owner), Some(g.team));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::flat_ground;
    use crate::sticky::StickyClass;
    use crate::unit::{SlotSpec, UnitHandle, UnitSpec};
    use crate::weapon::{ExplosionClass, OrdnanceClass, Thrown, WeaponClass};
    use crate::TICK_DT;

    /// Same-owner warnings are linked to instances, even when contacts share
    /// a tick and unrelated warnings are compacted out of the vector.
    #[test]
    fn same_tick_grenades_keep_their_own_warnings() {
        let mut s = Sim::empty(7);
        s.heightfield = Some(flat_ground());
        let owner = s
            .spawn(&UnitSpec::trooper(0, Vec3::new(0.0, 0.0, 40.0)))
            .unwrap();
        let mut wc = thermal(true);
        let class = wc.ordnance.sticky.as_mut().unwrap();
        class.stick_mask = 0;
        class.explosion = None;
        let class = class.clone();
        let first_class = s.add_weapon_class(wc.clone());
        wc.ordnance.life_span = 1.5;
        let second_class = s.add_weapon_class(wc);
        s.add_hazard(Vec3::ZERO, 1);
        let mut a = Sticky::new(
            Vec3::new(-5.0, 0.1, 0.0),
            Vec3::new(2.0, -5.0, 0.0),
            &class,
            0.6,
            owner,
            0,
            first_class,
        );
        let mut b = Sticky::new(
            Vec3::new(5.0, 0.1, 0.0),
            Vec3::new(-2.0, -9.0, 0.0),
            &class,
            1.5,
            owner,
            0,
            second_class,
        );
        assert!(s.tick_sticky(&mut a, TICK_DT));
        assert!(s.tick_sticky(&mut b, TICK_DT));
        assert_eq!(a.touched, Some(0));
        assert_eq!(a.touched, b.touched);
        assert!(a.moving && b.moving, "first contacts bounce");
        assert_ne!(a.warning_id, b.warning_id);
        let (mut alive_a, mut alive_b) = (true, true);
        let (mut rested_a, mut rested_b, mut first_expired_alone) = (false, false, false);
        for tick in 1..600 {
            s.tick = tick;
            // Compact the unrelated warning. Keep grenade warnings through
            // their burn tick so expiry matching can be inspected even if a
            // rounded deadline becomes due one tick before the float fuse.
            s.hazards.retain(|z| z.warning_id.is_some());
            let (was_moving_a, was_moving_b) = (a.moving, b.moving);
            if alive_a {
                alive_a = s.tick_sticky(&mut a, TICK_DT);
            }
            if alive_b {
                alive_b = s.tick_sticky(&mut b, TICK_DT);
            }
            for (g, alive, was_moving, rested) in [
                (&a, alive_a, was_moving_a, &mut rested_a),
                (&b, alive_b, was_moving_b, &mut rested_b),
            ] {
                // A's warning is deliberately compacted after detonation.
                if !alive && *rested {
                    continue;
                }
                let z = s
                    .hazards
                    .iter()
                    .find(|z| z.warning_id == g.warning_id)
                    .expect("own warning before detonation compaction");
                assert_eq!(z.since, 0, "reaction clock stays at first contact");
                assert_eq!(z.owner, Some(owner));
                if alive && was_moving && !g.moving {
                    *rested = true;
                    assert_eq!(z.pos, g.pos, "rest corrects only its own warning");
                    let span = s.weapon_classes[g.weapon_class].ordnance.life_span;
                    assert_eq!(
                        z.until,
                        tick + (g.life.min(span) * crate::TICK_HZ as f32).ceil() as u64
                    );
                }
            }
            if !alive_a && alive_b && !first_expired_alone {
                let a_warning = s
                    .hazards
                    .iter()
                    .find(|z| z.warning_id == a.warning_id)
                    .unwrap();
                assert_eq!(a_warning.until, tick, "A expires its own warning");
                let b_warning = s
                    .hazards
                    .iter()
                    .find(|z| z.warning_id == b.warning_id)
                    .unwrap();
                assert!(
                    b_warning.until > tick,
                    "B retains its separate later warning"
                );
                s.hazards.retain(|z| z.warning_id != a.warning_id);
                first_expired_alone = true;
            }
            if !alive_a && !alive_b {
                let b_warning = s
                    .hazards
                    .iter()
                    .find(|z| z.warning_id == b.warning_id)
                    .unwrap();
                assert_eq!(
                    b_warning.until, tick,
                    "B expires its own warning after compaction"
                );
                break;
            }
        }
        assert!(rested_a && rested_b && first_expired_alone);
        assert!(!alive_a && !alive_b);
    }

    #[test]
    fn warning_links_and_allocator_are_observed_by_the_hash() {
        let mut s = Sim::empty(7);
        let baseline = s.hash();
        s.add_hazard(Vec3::ZERO, 10);
        let unlinked = s.hash();
        s.hazards[0].warning_id = Some(1);
        assert_ne!(s.hash(), unlinked);
        let linked = s.hash();
        s.hazards[0].warning_id = Some(2);
        assert_ne!(s.hash(), linked);
        s.hazards.clear();
        assert_eq!(s.hash(), baseline, "external warnings issue no identity");
        s.last_warning_id = 1;
        assert_ne!(
            s.hash(),
            baseline,
            "allocator history survives empty vectors"
        );
        let c = StickyClass::new(true);
        let mut g = Sticky::new(
            Vec3::ZERO,
            Vec3::Y,
            &c,
            1.0,
            UnitHandle { index: 0, gen: 1 },
            0,
            0,
        );
        let bytes = |g: &Sticky| {
            let mut bytes = Vec::new();
            g.hash_into(&mut |part| bytes.extend_from_slice(part));
            bytes
        };
        let no_link = bytes(&g);
        g.warning_id = Some(1);
        let first_link = bytes(&g);
        assert_ne!(first_link, no_link);
        g.warning_id = Some(2);
        assert_ne!(bytes(&g), first_link);
    }

    /// The thermal detonator as the discs author it (`re-grenades.md` §9):
    /// BF1 ShotDelay 1.4, wind-up 0.3, fuse 0.9; BF2 1.3, 0.51, 0.6.
    fn thermal(bf2: bool) -> WeaponClass {
        WeaponClass {
            name: "thermal".into(),
            rounds_per_clip: 1,
            reload_time: 0.01,
            trigger_single: true,
            shot_delay: if bf2 { [1.3; 2] } else { [1.4; 2] },
            initial_salvo_delay: if bf2 { 0.51 } else { 0.3 },
            thrown: Some(Thrown {
                shot_elevate: 0.21,
                hide_on_fire: true,
                hand: bf2.then_some(Thrown::BF2_HAND),
            }),
            ordnance: OrdnanceClass {
                velocity: 25.0,
                life_span: if bf2 { 0.6 } else { 0.9 },
                sticky: Some(StickyClass {
                    gravity: -12.25,
                    stick_mask: if bf2 { 0x7fff_ffd8 } else { 0x7fff_ff80 },
                    explosion: Some(ExplosionClass {
                        damage: 320.0,
                        damage_inner: 4.5,
                        damage_outer: 6.5,
                        ..ExplosionClass::default()
                    }),
                    ..StickyClass::new(bf2)
                }),
                ..OrdnanceClass::default()
            },
            ..WeaponClass::default()
        }
    }

    /// A player at the origin facing −Z holding the rifle, a pistol (slot 1,
    /// channel 0), the thermal (slot 2, channel 1, 4 clips) and a second
    /// grenade (slot 3, channel 1, 2 clips).
    fn grenadier(bf2: bool) -> (Sim, UnitHandle) {
        let mut s = Sim::empty(7);
        s.heightfield = Some(flat_ground());
        let mut pistol = WeaponClass::bf1_rifle();
        pistol.name = "pistol".into();
        let pistol = s.add_weapon_class(pistol);
        let thermal = s.add_weapon_class(thermal(bf2));
        let slot = |slot, channel, weapon_class, clips| SlotSpec {
            slot,
            channel,
            weapon_class,
            clips,
        };
        let p = s
            .spawn(&UnitSpec {
                slots: std::sync::Arc::new([
                    slot(1, 0, pistol, 0),
                    slot(2, 1, thermal, 4),
                    slot(3, 1, thermal, 2),
                ]),
                ..UnitSpec::player(0, Vec3::ZERO)
            })
            .unwrap();
        // A level aim (the tests' launch directions are from it).
        s.units[p.index as usize].soldier.pitch = 0.0;
        (s, p)
    }

    fn pad(buttons: u32) -> PadState {
        PadState {
            buttons,
            ..Default::default()
        }
    }

    fn throws(s: &Sim) -> usize {
        s.events
            .iter()
            .filter(|e| matches!(e, Event::Throw { .. }))
            .count()
    }

    /// Step with `buttons` until a throw; the number of steps taken.
    fn steps_to_throw(s: &mut Sim, buttons: u32, limit: u32) -> Option<u32> {
        for n in 1..=limit {
            s.events.clear();
            s.step(&[pad(buttons)]);
            if throws(s) > 0 {
                return Some(n);
            }
        }
        None
    }

    /// The cycle (§3.2 [M]): the grenade leaves the hand at the first update
    /// at or past the wind-up (BF1 0.30 s, BF2 0.51 s -> 0.533), a second
    /// press is ignored until 1.70 / 1.81 s after the first, the count goes
    /// 4 -> 3, and a held trigger throws once.
    #[test]
    fn the_grenade_cycle_matches_the_ps2_timings() {
        for (bf2, windup, cycle) in [(false, 0.30f32, 1.70f32), (true, 0.51, 1.81)] {
            let (mut s, p) = grenadier(bf2);
            let count = |s: &Sim| {
                let u = s.unit(p).unwrap();
                let w = &u.held[u.cur1.unwrap() as usize];
                w.state.rounds(&s.weapon_classes[w.class]).unwrap()
            };
            assert_eq!(count(&s), 4, "WeaponAmmo 4 = one in the hand + three");
            // Held all the way through: one throw.
            let n = steps_to_throw(&mut s, buttons::SECONDARY_FIRE, 60).expect("a throw");
            let t = n as f32 * TICK_DT;
            assert!(
                t >= windup - 1e-3 && t < windup + 2.0 * TICK_DT,
                "bf2 {bf2}: thrown {t} s after the press, wind-up {windup}"
            );
            assert_eq!(count(&s), 3);
            assert_eq!(s.unit(p).unwrap().active_channel, 1);
            let mut ticks = n;
            while (ticks as f32) * TICK_DT < cycle + 1.0 {
                s.events.clear();
                s.step(&[pad(buttons::SECONDARY_FIRE)]);
                ticks += 1;
                assert_eq!(throws(&s), 0, "a held trigger throws once");
            }
            assert_eq!(count(&s), 3);

            // Tapped again and again from the first press: the second throw
            // winds up only once the cycle (wind-up + ShotDelay) has run out.
            let (mut s, _) = grenadier(bf2);
            let first = steps_to_throw(&mut s, buttons::SECONDARY_FIRE, 60).unwrap();
            let mut n = first;
            let second = loop {
                // A fresh press every other tick.
                let b = if n.is_multiple_of(2) {
                    buttons::SECONDARY_FIRE
                } else {
                    0
                };
                s.events.clear();
                s.step(&[pad(b)]);
                n += 1;
                if throws(&s) > 0 {
                    break n;
                }
                assert!(n < 200, "no second throw");
            };
            let pressed = (second - first) as f32 * TICK_DT - windup;
            // The second wind-up started `pressed` s after the first throw:
            // not before the cycle's ShotDelay, and within a few ticks of it
            // (the reload's tick and the tap's phase).
            let shot_delay = cycle - windup;
            assert!(
                pressed >= shot_delay - 2.0 * TICK_DT && pressed <= shot_delay + 5.0 * TICK_DT,
                "bf2 {bf2}: second press took {pressed} s after the throw, ShotDelay {shot_delay}"
            );
            assert_eq!(count(&s), 2);
        }
    }

    /// Channel 1 wins (§2.3): while the grenade's fire state runs the
    /// primary trigger is swallowed, and it fires again afterwards.
    #[test]
    fn the_secondary_swallows_the_primary_trigger() {
        let (mut s, p) = grenadier(false);
        let both = buttons::FIRE | buttons::SECONDARY_FIRE;
        let shots = |s: &Sim| {
            s.events
                .iter()
                .filter(|e| matches!(e, Event::Shot { .. }))
                .count()
        };
        let mut fired = 0;
        for _ in 0..30 {
            s.events.clear();
            s.step(&[pad(both)]);
            fired += shots(&s);
        }
        assert_eq!(
            fired, 0,
            "the rifle is silent while the grenade has the soldier"
        );
        assert_eq!(s.unit(p).unwrap().active_channel, 1);
        // Let the cycle and the reload run out with FIRE alone held.
        for _ in 0..90 {
            s.events.clear();
            s.step(&[pad(buttons::FIRE)]);
            fired += shots(&s);
        }
        assert!(fired > 0, "the rifle fires once channel 1 lets go");
        assert_eq!(s.unit(p).unwrap().active_channel, 0);
    }

    /// The empty click (card WSND, re-weapon-sounds.md §7.3): a human's
    /// trigger PRESS on a weapon that is reloading - once per press, not
    /// while held, never for a bot.
    #[test]
    fn a_trigger_press_on_a_reloading_weapon_clicks_for_a_human() {
        let clicks = |s: &Sim| {
            s.events
                .iter()
                .filter(|e| matches!(e, Event::FireEmpty { .. }))
                .count()
        };
        for human in [true, false] {
            let (mut s, p) = grenadier(false);
            let i = p.index as usize;
            s.units[i].is_player = human;
            // A shot, then a reload of the part-used clip.
            s.step(&[pad(buttons::FIRE)]);
            assert_eq!(clicks(&s), 0, "a weapon that fires does not click");
            s.step(&[pad(0)]);
            s.step(&[pad(buttons::RELOAD)]);
            assert!(s.units[i].weapon.reloading());
            s.step(&[pad(0)]);
            let mut n = 0;
            for _ in 0..3 {
                s.step(&[pad(buttons::FIRE)]);
                n += clicks(&s);
            }
            assert!(s.units[i].weapon.reloading());
            assert_eq!(n, usize::from(human), "human {human}");
        }
    }

    /// The two switch inputs (§2.4): each steps its own channel, wraps, and
    /// does nothing while a trigger is down.
    #[test]
    fn each_switch_steps_its_own_channel() {
        let (mut s, p) = grenadier(false);
        let slots = |s: &Sim| {
            let u = s.unit(p).unwrap();
            (u.weapon_slot, u.held[u.cur1.unwrap() as usize].slot)
        };
        let tap = |s: &mut Sim, b: u32| {
            s.step(&[pad(b)]);
            s.step(&[pad(0)]);
        };
        assert_eq!(slots(&s), (0, 2));
        // Each switch says which weapon was selected (its
        // WeaponChangeSound, card WSND), on the tick of the press.
        let selected = |s: &mut Sim, b: u32| -> Vec<u32> {
            s.step(&[pad(b)]);
            let got = s
                .events
                .iter()
                .filter_map(|e| match e {
                    Event::WeaponSelect { unit, weapon_class } if *unit == p => Some(*weapon_class),
                    _ => None,
                })
                .collect();
            s.step(&[pad(0)]);
            assert!(!s
                .events
                .iter()
                .any(|e| matches!(e, Event::WeaponSelect { .. })));
            got
        };
        assert_eq!(selected(&mut s, buttons::SWITCH_WEAPON), [1]);
        assert_eq!(slots(&s), (1, 2), "channel 0 -> the pistol");
        assert_eq!(s.unit(p).unwrap().weapon_class, 1);
        assert_eq!(selected(&mut s, buttons::SWITCH_SECONDARY), [2]);
        assert_eq!(slots(&s), (1, 3), "channel 1 -> the second grenade");
        tap(&mut s, buttons::SWITCH_WEAPON);
        tap(&mut s, buttons::SWITCH_SECONDARY);
        assert_eq!(slots(&s), (0, 2), "both wrap");
        // Held is one step, not one a tick.
        s.step(&[pad(buttons::SWITCH_WEAPON)]);
        s.step(&[pad(buttons::SWITCH_WEAPON)]);
        assert_eq!(slots(&s), (1, 2));
        s.step(&[pad(0)]);
        // Not while a trigger is down.
        s.step(&[pad(buttons::FIRE)]);
        s.step(&[pad(buttons::FIRE | buttons::SWITCH_WEAPON)]);
        assert_eq!(slots(&s), (1, 2));
        // The grenade thrown is the selected one's: 2 clips -> 1.
        tap(&mut s, 0);
        tap(&mut s, buttons::SWITCH_SECONDARY);
        assert!(steps_to_throw(&mut s, buttons::SECONDARY_FIRE, 60).is_some());
        let u = s.unit(p).unwrap();
        let rounds: Vec<u32> = u
            .held
            .iter()
            .filter(|w| w.channel == 1)
            .map(|w| w.state.rounds(&s.weapon_classes[w.class]).unwrap())
            .collect();
        assert_eq!(rounds, [4, 1]);
    }

    /// A throw on flat ground, through the sim: launched from the fire point
    /// (BF1) or the hand (BF2) at Velocity along the elevated aim, it
    /// bounces, rests, then burns its fuse and its blast hurts an enemy
    /// standing where it lies — and the bots' warning sits there meanwhile.
    #[test]
    fn a_thrown_grenade_rests_then_blasts() {
        for bf2 in [false, true] {
            let (mut s, p) = grenadier(bf2);
            assert!(steps_to_throw(&mut s, buttons::SECONDARY_FIRE, 60).is_some());
            let Some(Event::Throw { origin, .. }) = s
                .events
                .iter()
                .find(|e| matches!(e, Event::Throw { .. }))
                .cloned()
            else {
                unreachable!()
            };
            let want = if bf2 {
                Vec3::new(0.0, Thrown::BF2_HAND.0, -Thrown::BF2_HAND.1)
            } else {
                s.unit(p).unwrap().muzzle()
            };
            assert!((origin - want).length() < 1e-4, "bf2 {bf2}: from {origin}");
            let g = &s.stickies[0];
            let launch = sticky::throw_dir(Vec3::NEG_Z, 0.21) * 25.0;
            // One flight step has already run: only gravity has touched it.
            assert!((g.vel - launch - Vec3::Y * (-12.25 * TICK_DT)).length() < 1e-3);

            let mut rest: Option<(u64, Vec3)> = None;
            let mut blast: Option<(u64, Vec3)> = None;
            let mut victim = None;
            for _ in 0..400 {
                s.events.clear();
                s.step(&[pad(0)]);
                if rest.is_none() {
                    if let Some(g) = s.stickies.first().filter(|g| !g.moving) {
                        rest = Some((s.tick, g.pos));
                        assert!(g.armed && g.vel == Vec3::ZERO);
                        assert_eq!(s.hazards.len(), 1);
                        assert!((s.hazards[0].pos - g.pos).length() < 1e-6);
                        // An enemy walks up to it.
                        victim = s.spawn(&UnitSpec::trooper(1, g.pos + Vec3::X));
                    }
                }
                if let Some(Event::StickyBlast { pos, .. }) = s
                    .events
                    .iter()
                    .find(|e| matches!(e, Event::StickyBlast { .. }))
                {
                    blast = Some((s.tick, *pos));
                    break;
                }
            }
            let (rest_tick, rest_pos) = rest.expect("it came to rest");
            let (blast_tick, blast_pos) = blast.expect("it went off");
            assert_eq!(blast_pos, rest_pos);
            assert!(rest_pos.z < -15.0, "bf2 {bf2}: it flew, {rest_pos}");
            let fuse = (blast_tick - rest_tick) as f32 * TICK_DT;
            let life = if bf2 { 0.6 } else { 0.9 };
            assert!(
                (fuse - life).abs() <= TICK_DT + 1e-4,
                "bf2 {bf2}: {fuse} s from rest to blast, LifeSpan {life}"
            );
            assert!(s.stickies.is_empty());
            // 320 at 1 m (inside the inner radius) kills a 300-health trooper,
            // and the thrower has the kill.
            assert!(s.unit(victim.unwrap()).is_none(), "the blast killed it");
            assert_eq!(s.unit(p).unwrap().kills, 1);
            s.step(&[pad(0)]);
            assert!(s.hazards.is_empty(), "the warning ends with the blast");
        }
    }

    /// The PS2's own throws (card GRNP, re-grenades §13 [M]): seven BF2
    /// thermal detonators on Mos Eisley's level street, the grenade's RAM row
    /// after each update. From each throw's first row the port's grenade is
    /// the PS2's row for row — every bounce, the rest update and the blast.
    #[test]
    fn sticky_ps2_bf2_throws_match_row_for_row() {
        let mut worst = 0.0f32;
        let mut throws = 0;
        let text = include_str!("../../tests/data/grenade_ps2_bf2.txt");
        let mut lines = text.lines().filter(|l| !l.starts_with('#')).peekable();
        while let Some(head) = lines.next() {
            let mut h = head.split_whitespace();
            assert_eq!(h.next(), Some("throw"), "{head}");
            let name = h.next().unwrap();
            let life: f32 = h.next().unwrap().parse().unwrap();
            let mut rows = Vec::new();
            while let Some(l) = lines.next_if(|l| !l.starts_with("throw")) {
                let v: Vec<f32> = l.split_whitespace().map(|t| t.parse().unwrap()).collect();
                rows.push(v);
            }
            let (mut s, p) = grenadier(true);
            let wc = s.units[p.index as usize].held[1].class;
            let sc = s.weapon_classes[wc].ordnance.sticky.clone().unwrap();
            let v3 = |r: &[f32], k: usize| Vec3::new(r[k], r[k + 1], r[k + 2]);
            let first = &rows[0];
            s.stickies
                .push(Sticky::new(v3(first, 1), v3(first, 4), &sc, life, p, 0, wc));
            let mut update = first[0] as u32;
            let mut rest = None;
            for r in &rows[1..] {
                while update < r[0] as u32 {
                    s.tick_stickies(TICK_DT);
                    update += 1;
                }
                let g = s.stickies.first().unwrap_or_else(|| {
                    panic!("{name}: gone at update {update}, the PS2's is there")
                });
                let e = (g.pos - v3(r, 1)).length();
                assert!(
                    e <= 0.002,
                    "{name} update {update}: {} against the PS2's {} ({e} m)",
                    g.pos,
                    v3(r, 1)
                );
                worst = worst.max(e);
                assert!(
                    (g.vel - v3(r, 4)).length() <= 0.01,
                    "{name} update {update}"
                );
                assert_eq!(
                    (g.bounces, g.armed),
                    (r[7] as u8, r[8] != 0.0),
                    "{name} update {update}"
                );
                if g.armed && rest.is_none() {
                    rest = Some(update);
                }
            }
            // The PS2's last row is the update before its blast: the fuse is
            // 18 updates from the rest.
            assert_eq!(update, rest.expect("it rested") + 17, "{name}");
            s.tick_stickies(TICK_DT);
            assert!(s.stickies.is_empty(), "{name}: the blast");
            throws += 1;
        }
        assert_eq!(throws, 7);
        println!("7 throws, worst row {worst} m");
    }

    /// Card GRNP: a BF1 thermal detonator thrown as BF2 throws
    /// ([`Thrown::throw_as_bf2`]) leaves the hand on the same tick as BF2's,
    /// flies, bounces and rests tick for tick where BF2's does and goes off
    /// on BF2's tick — with BF1's own blast.
    #[test]
    fn a_bf1_grenade_thrown_as_bf2_is_bf2s_throw() {
        let run = |class: WeaponClass| {
            let mut s = Sim::empty(7);
            s.heightfield = Some(flat_ground());
            let wc = s.add_weapon_class(class);
            let p = s
                .spawn(&UnitSpec {
                    slots: std::sync::Arc::new([SlotSpec {
                        slot: 2,
                        channel: 1,
                        weapon_class: wc,
                        clips: 4,
                    }]),
                    ..UnitSpec::player(0, Vec3::ZERO)
                })
                .unwrap();
            s.units[p.index as usize].soldier.pitch = 0.0;
            let mut rows = Vec::new();
            let (mut thrown, mut blast) = (None, None);
            for tick in 1..=200u32 {
                s.events.clear();
                s.step(&[pad(if tick <= 4 {
                    buttons::SECONDARY_FIRE
                } else {
                    0
                })]);
                for e in &s.events {
                    match e {
                        Event::Throw { .. } => thrown = Some(tick),
                        Event::StickyBlast { .. } => blast = Some(tick),
                        _ => {}
                    }
                }
                rows.push(s.stickies.first().map(|g| (g.pos, g.moving)));
            }
            (rows, thrown.unwrap(), blast.unwrap())
        };
        let mut bf1 = thermal(false);
        Thrown::throw_as_bf2(&mut bf1);
        assert_eq!(
            bf1.ordnance.sticky.as_ref().unwrap().stick_mask,
            0x7fff_ffd8
        );
        // BF1's own blast and cycle stay.
        assert_eq!(bf1.shot_delay, [1.4; 2]);
        let (a, a_thrown, a_blast) = run(bf1);
        let (b, b_thrown, b_blast) = run(thermal(true));
        // The PS2's BF2 release: the 16th update from the press (§13.1).
        assert_eq!((a_thrown, a_blast), (b_thrown, b_blast));
        assert_eq!(a_thrown, 16);
        assert_eq!(a, b, "row for row");
        // The plain BF1 throw is another: its 0.3 s wind-up, the fire point.
        let (c, c_thrown, _) = run(thermal(false));
        assert_eq!(c_thrown, 10);
        assert_ne!(c, a);
        // Level ground, a level aim: at rest about where the PS2's BF2
        // throw at aim +0.5 deg lies (35.49 m, 74 updates after the release
        // — the half degree moves the bounces a few updates; the fixture
        // test above is the exact one), the blast 18 updates after the rest.
        let rest = a.iter().position(|r| r.is_some_and(|r| !r.1)).unwrap();
        let at = a[rest].unwrap().0;
        let rest = rest as u32 + 1;
        assert!((at.length() - 35.5).abs() < 0.6, "rest at {at}");
        assert!(rest.abs_diff(a_thrown + 74) <= 4, "rest tick {rest}");
        assert_eq!(a_blast, rest + 18);
    }

    /// Card GRFL: a grenade thrown down at 48 deg onto a level MESH floor (a
    /// two-triangle sheet, either winding) bounces and rests on it. The steep
    /// impact leaves along the normal (f = 1), so the next sweep starts
    /// [`sticky::PUSH_OFF`] over the face: a ray pulled back by
    /// [`sticky::SWEEP_BACK`] met the two-sided face from behind and the
    /// bounce put the grenade under the floor (the maintainer's Kamino decks).
    #[test]
    fn a_grenade_thrown_steeply_at_a_mesh_floor_rests_on_it() {
        use crate::collision::{trimesh, Body, CollisionWorld, Shape};
        const FLOOR: f32 = 10.0;
        let corners = [
            [-30.0, FLOOR, -30.0],
            [30.0, FLOOR, -30.0],
            [30.0, FLOOR, 30.0],
            [-30.0, FLOOR, 30.0],
        ];
        for (winding, bf2) in [(0, true), (1, true), (0, false), (1, false)] {
            let indices: [u32; 6] = if winding == 0 {
                [0, 1, 2, 0, 2, 3]
            } else {
                [0, 2, 1, 0, 3, 2]
            };
            let sheet = Body::new(
                glam::Mat4::IDENTITY,
                Shape::Mesh(trimesh(&corners, &indices).expect("mesh")),
            );
            let mut s = Sim::empty(7);
            s.collision = CollisionWorld::new(vec![sheet], Vec::new());
            let wc = s.add_weapon_class(thermal(bf2));
            let owner = s
                .spawn(&UnitSpec::player(0, Vec3::new(20.0, FLOOR, 20.0)))
                .unwrap();
            let class = s.weapon_classes[wc].ordnance.sticky.clone().unwrap();
            let a = 48.0f32.to_radians();
            let vel = Vec3::new(a.cos(), -a.sin(), 0.0) * 12.0;
            s.stickies.push(Sticky::new(
                Vec3::new(0.0, FLOOR + 2.0, 0.0),
                vel,
                &class,
                1.0e6,
                owner,
                0,
                wc,
            ));
            let mut low = f32::MAX;
            for _ in 0..600 {
                s.step(&[pad(0)]);
                // Under the floor it falls to the world's floor and is gone.
                let Some(g) = s.stickies.first() else { break };
                low = low.min(g.pos.y);
                if !g.moving {
                    break;
                }
            }
            let case = format!("winding {winding} bf2 {bf2}");
            assert!(
                low > FLOOR,
                "{case}: it went under the floor, lowest y {low}"
            );
            let g = &s.stickies[0];
            assert!(!g.moving && g.bounces > 0, "{case}: it came to rest, {g:?}");
            assert!(
                (g.pos.y - FLOOR).abs() <= sticky::PUSH_OFF + 1e-3,
                "{case}: it lies on the floor, y {}",
                g.pos.y
            );
        }
    }

    /// Issue #37 (BF2 Jabba's Palace, the cell corridor's step): a grenade
    /// that meets the underside of a step's lip, 2.5 cm over the floor, is
    /// pushed off along the lip's down-facing normal. The PS2's full 0.1 m
    /// put it under the floor; the push stops short of the floor now.
    #[test]
    fn a_grenade_under_a_steps_lip_stays_over_the_floor() {
        use crate::collision::{trimesh, Body, CollisionWorld, Shape};
        const FLOOR: f32 = 10.0;
        let floor = [
            [-30.0, FLOOR, -30.0],
            [30.0, FLOOR, -30.0],
            [30.0, FLOOR, 30.0],
            [-30.0, FLOOR, 30.0],
        ];
        // The lip: its bottom edge 2.5 cm over the floor at z 2, rising
        // toward -z, facing down and toward the thrower (the traced normal).
        let n = Vec3::new(0.0, -0.45, -0.89).normalize();
        let up = n.cross(Vec3::X).normalize() * -1.0;
        let edge = Vec3::new(0.0, FLOOR + 0.025, 2.0);
        let lip = [
            edge - Vec3::X * 5.0,
            edge + Vec3::X * 5.0,
            edge + Vec3::X * 5.0 + up * 0.5,
            edge - Vec3::X * 5.0 + up * 0.5,
        ]
        .map(|v| v.to_array());
        for (winding, bf2) in [(0, true), (1, true), (0, false), (1, false)] {
            let quad: [u32; 6] = if winding == 0 {
                [0, 1, 2, 0, 2, 3]
            } else {
                [0, 2, 1, 0, 3, 2]
            };
            for k in 0..9 {
                let mut s = Sim::empty(7);
                let sheet = |c: &[[f32; 3]; 4]| {
                    Body::new(
                        glam::Mat4::IDENTITY,
                        Shape::Mesh(trimesh(c, &quad).expect("mesh")),
                    )
                };
                s.collision = CollisionWorld::new(vec![sheet(&floor), sheet(&lip)], Vec::new());
                let wc = s.add_weapon_class(thermal(bf2));
                let owner = s
                    .spawn(&UnitSpec::player(0, Vec3::new(20.0, FLOOR, 20.0)))
                    .unwrap();
                let class = s.weapon_classes[wc].ordnance.sticky.clone().unwrap();
                // Level throws 0.5 m short of the lip, low enough to meet it
                // within the PS2 push-off's reach of the floor.
                let start = Vec3::new(0.0, FLOOR + 0.04 + 0.01 * k as f32, 1.5);
                let g = Sticky::new(start, Vec3::new(0.0, 0.0, 6.0), &class, 1.0e6, owner, 0, wc);
                s.stickies.push(g);
                let mut low = f32::MAX;
                for _ in 0..300 {
                    s.step(&[pad(0)]);
                    let Some(g) = s.stickies.first() else { break };
                    low = low.min(g.pos.y);
                    if !g.moving {
                        break;
                    }
                }
                assert!(
                    low > FLOOR,
                    "winding {winding} bf2 {bf2} from y {}: under the floor, lowest {low}",
                    start.y
                );
            }
        }
    }

    /// A unit with one weapon retains its channels, held slots and ordnance.
    #[test]
    fn a_one_weapon_unit_has_no_channels() {
        let mut s = Sim::empty(7);
        s.heightfield = Some(flat_ground());
        let p = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
        for _ in 0..10 {
            s.step(&[pad(buttons::SECONDARY_FIRE | buttons::SWITCH_WEAPON)]);
        }
        let u = s.unit(p).unwrap();
        assert!(u.held.is_empty() && u.cur1.is_none() && u.active_channel == 0);
        assert!(s.stickies.is_empty());
    }
}
