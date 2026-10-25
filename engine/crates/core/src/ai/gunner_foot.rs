//! The GUNNER of a bot on foot: [`run`] turns the tick's one [`AimRequest`]
//! into the look sticks and the trigger — the aim point ([`seen_aim`]: BF1's
//! lead point offset by the shooter's tracked error, [`track_error`]; BF2's
//! offset by the TARGET's own error block
//! through [`radius`], along the shooter's tumbling direction, scaled by
//! target, range and the weapon class's AIBubble keys in `error_vector`;
//! cards CLSE, CLSE2), the D9 aim tracker and its
//! slew, the D11 fire discipline ([`fire_discipline`]: bursts, pauses, the
//! charge), card STTAP's tap of a single-shot weapon (`tap`), card FFLN's
//! "no shot with a friend in the line of fire" (`friend_in_line`; card
//! HVYW: a missile's line is wider by what is walked in its wind-up) and card
//! GRAI's weapon of the think ([`super::arsenal`]: the switch press, the
//! grenade's low-arc aim and its tap on SECONDARY_FIRE) — with
//! [`yaw_toward`]. Card SPL11, `analysis/bot-design.md` 3 and 4.8: this is
//! the ONLY writer of a foot soldier's `pad.rx` / `pad.ry` and of FIRE (a
//! seated bot's gun is [`super::gunner`], which shares the aim point and the
//! bursts and charge release). The aim and fire tests live here.

use super::arsenal::{self, Armed};
use super::behave::Tick;
use super::draw::{unit_dir, unit_unit, SALT_BURST, SALT_CHARGE, SALT_ERR_DIR, SALT_TAP};
use super::state::{AiState, NavTick};
use super::{
    AIM_TOLERANCE, CHARGE_MIN, ERR_GROWTH, ERR_MAX, ERR_NO_TARGET, ERR_SPEED_TERM, LINE_CLEAR,
    LINE_PAST, LINE_SPREAD, SLEW_COS, SLEW_SIN, THINK_PERIOD,
};
use crate::collision::lead_point;
use crate::fmath;
use crate::perception::{self, Contact};
use crate::sim::{buttons, PadState, Sim};
use crate::soldier::Stance;
use crate::unit::{Unit, UnitHandle};
use crate::weapon::WeaponClass;
use glam::Vec3;

/// What the running behaviour asks of the gunner this tick — exactly one a
/// tick (`behave::think`). The fire permission is the variant today: only a
/// seen enemy is fired on.
#[derive(Clone, Copy)]
pub(super) enum AimRequest<'a> {
    /// A SEEN enemy and the contact it is known by: tracked at its lead
    /// point under the D10 error, and fired on once the contact is fireable.
    Enemy(&'a Unit, Contact),
    /// A remembered contact's last known position: tracked, never fired on.
    Memory(Vec3),
    /// Nobody known: turn to this point with the gun levelled (the
    /// travelling look).
    Travel(Vec3),
}

/// What the gunner answers, for the behaviour's position rules.
#[derive(Clone, Copy)]
pub(super) struct GunReport {
    /// Card FFLN: a friend stands in the line of fire at the seen enemy —
    /// the side to step to, away from him (+1 = the unit's right).
    pub line: Option<i8>,
    /// The seen enemy's range from the muzzle (m); `None` with no seen enemy.
    pub range: Option<f32>,
    /// Card HVYW: the think's pick is a grenade the bot cannot throw from
    /// here (the arc does not reach, the target is outside its band or too
    /// steep). A throw under way is not one.
    pub no_throw: bool,
}

/// Yaw (engine convention: 0 = −Z, positive left) that faces `dir`.
pub fn yaw_toward(dir: Vec3) -> f32 {
    fmath::atan2(-dir.x, -dir.z)
}

fn wrap(a: f32) -> f32 {
    let mut a = a % std::f32::consts::TAU;
    if a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    } else if a < -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a
}

/// A BF1 round (D10): the SHOOTER's tracked-position error, once per think. It decays toward
/// the skill's floor while the target is tracked and grows with how
/// far that target moved since the last think, over the target's own
/// top speed; with NO target it is held at the PS2's constant 1.5 m
/// no-target floor (LEAD ADDENDUM 4a) instead of being reset.
pub(super) fn track_error(sim: &Sim, st: &mut AiState, skill: &crate::skill::Skill) {
    let tracked = st
        .target
        .filter(|_| st.contacts.iter().flatten().any(|c| c.seen_now))
        .and_then(|h| sim.unit(h));
    match tracked {
        Some(o) => {
            let p = o.capsule().center();
            let think_dt = THINK_PERIOD as f32 / crate::TICK_HZ as f32;
            st.err -= think_dt * skill.err_decay;
            if st.err_pos != Vec3::ZERO {
                st.err += ERR_GROWTH * (p - st.err_pos).length() / o.consts.max_speed.max(0.1);
            }
            st.err = st.err.clamp(skill.err_floor, ERR_MAX);
            st.err_pos = p;
        }
        None => st.err = st.err.clamp(ERR_NO_TARGET, ERR_MAX),
    }
}

/// The AI sniper distance table's spread for this tick
/// ([`AiState::dispersion`]), written on every tick of a bot on foot.
pub(super) fn range_table(tk: &Tick, st: &mut AiState, enemy: Option<&Unit>) {
    let Tick { eye, wc, skill, .. } = *tk;
    // D9 (LEAD ADDENDUM 4b): the AI SNIPER DISTANCE TABLE. `think` runs for
    // AI units only, so the PS2's "fire point owner index < 0" is given; the
    // class gates are `ai_distance_table`; the remaining runtime gate is "has
    // a current target", and it is SEEN-only here — a D7 memory target never
    // presses FIRE, so a remembered one could not spend the value anyway.
    // Written every tick, not every think: the PS2 asks its getter at the
    // shot, from the range at that moment. `re-ai-sniper.md` §2.
    st.dispersion = enemy.and_then(|e| {
        wc.ai_distance_table().then(|| {
            crate::weapon::ai_sniper_yaw((e.capsule().center() - eye).length_squared())
                * skill.dispersion_mult
        })
    });
}

/// One tick of the gunner: the look sticks toward `aim` and, at a seen enemy,
/// the trigger. `None` when there is nothing to turn to (the aim point is the
/// muzzle itself): the caller's tick ends there, the pad untouched.
pub(super) fn run(
    sim: &Sim,
    tk: &Tick,
    st: &mut AiState,
    aim: &AimRequest,
    pad: &mut PadState,
    nt: &mut NavTick,
) -> Option<GunReport> {
    let Tick {
        me,
        u,
        eye,
        wc,
        tier,
        skill,
    } = *tk;
    let dt = crate::TICK_DT;
    let (enemy, held, memory) = match *aim {
        AimRequest::Enemy(e, c) => (Some(e), Some(c), None),
        AimRequest::Memory(p) => (None, None, Some(p)),
        AimRequest::Travel(_) => (None, None, None),
    };
    // Card GRAI: what the think's weapon choice means this tick.
    let armed = match enemy {
        Some(_) => arsenal::armed(sim, u, st),
        None => Armed::Hand,
    };
    let mut reaches = false;
    let look_at = match (*aim, &armed) {
        // A grenade: the low-arc solution to the target's feet, where it is
        // to land. The tracked error moves that point over the ground only
        // (OURS: on so flat an arc a metre of height is five of range).
        (AimRequest::Enemy(e, _), Armed::Thrown(gc, _)) => {
            let c = e.capsule().center();
            let err = seen_aim(sim, me, st, eye, e, 1e6, wc) - c;
            let feet = e.soldier.pos + Vec3::new(err.x, 0.0, err.z);
            let (p, ok) = arsenal::lob_aim(sim, u, eye, feet, e.soldier.vel, gc);
            reaches = ok;
            p
        }
        (AimRequest::Enemy(e, _), _) => seen_aim(sim, me, st, eye, e, wc.ordnance.velocity, wc),
        (AimRequest::Memory(p) | AimRequest::Travel(p), _) => p,
    };
    let to = look_at - eye;
    let dist = to.length();
    if dist < 1e-3 {
        return None;
    }
    let dir = to / dist;
    // Aiming at a KNOWN enemy — seen or remembered — as against levelling the
    // gun on the walk.
    let aiming = enemy.is_some() || memory.is_some();
    // D9: the AIM TRACKER. With a target the pad follows a steered direction,
    // not the wanted one: a second-order chase under the skill's acceleration
    // clamp and damping, with a 30° slew when the target is far off the
    // current aim (RE §4.2 steps 3-4). Without a target it is idle and the
    // travel facing is exactly what it was before Phase C.
    let (want_yaw, want_pitch) = if aiming {
        let (accel, damping) = skill.tracker_for(true);
        let mut ad = st.aim_dir;
        let mut av = st.aim_vel;
        if ad == Vec3::ZERO {
            // Not tracking yet (a new target, or the first tick with one):
            // start where the unit is actually looking.
            ad = u.aim_frame().0;
            av = Vec3::ZERO;
        }
        if ad.dot(dir) < SLEW_COS {
            ad = slew(ad, dir);
            av = Vec3::ZERO;
        } else {
            let mut a = (dir - ad) / dt - av * damping;
            let mag = a.length();
            if mag > accel {
                a *= accel / mag;
            }
            av += a * dt;
            let next = ad + av * dt;
            ad = if next.length() < 1e-6 {
                dir
            } else {
                next.normalize()
            };
        }
        st.aim_dir = ad;
        st.aim_vel = av;
        // Card GRAI (OURS): a throw's elevation is the solution's itself,
        // not the tracker's — on the grenade's flat arc the tracker's lag of
        // a few degrees is ten metres of range.
        let pitch = match armed {
            Armed::Thrown(..) => dir.y,
            _ => ad.y,
        };
        (yaw_toward(ad), fmath::asin(pitch.clamp(-1.0, 1.0)))
    } else {
        st.aim_dir = Vec3::ZERO;
        st.aim_vel = Vec3::ZERO;
        (yaw_toward(dir), fmath::asin(dir.y.clamp(-1.0, 1.0)))
    };
    let e_yaw = wrap(want_yaw - u.soldier.yaw);
    let e_pitch = if aiming {
        want_pitch - u.soldier.pitch
    } else {
        -u.soldier.pitch // level the gun while travelling
    };
    // Soldier: yaw -= rx·MaxTurnSpeed·dt ; pitch += ry·MaxPitchSpeed·dt.
    let rx = (-e_yaw / (u.consts.max_turn_speed * dt)).clamp(-1.0, 1.0);
    let ry = (e_pitch / (u.consts.max_pitch_speed * dt)).clamp(-1.0, 1.0);
    pad.rx = PadState::quantize(rx);
    pad.ry = PadState::quantize(ry);

    // Firing is unchanged. What changed (design decision 4) is that this is no
    // longer an early return: an engaged unit keeps WALKING ITS ROUTE and only
    // holds inside HOLD_RANGE, so the stuck window and the side-step run in a
    // firefight too — the UTA1 deadlock was a unit frozen against a wall with
    // its stuck timer pinned because `think` returned here.
    //
    // Phase C adds the two gates of decision 2: the shot goes at a contact
    // this unit PERCEIVES (`enemy` is the seen contact's live unit), and only
    // after the tier's reaction time has passed since it first knew of it. A
    // MEMORY target never presses FIRE, and never HOLDS either: the design's
    // "the unit may move toward it" wants it walking, not frozen.
    //
    // D11 adds FIRE DISCIPLINE on top: a burst of `st.burst` shots, then the
    // weapon class's pause, then a freshly drawn length ([`fire_discipline`];
    // card BRSTU: an unlimited weapon's burst runs on its heat, not a count).
    // The counter is decremented when the PREVIOUS tick's press actually
    // produced a shot — `Unit.last_shot_tick` is written in `Sim::step`
    // phase 4, after `think` has already run for this tick — so a weapon
    // that is reloading, overheated or rate-limited costs the bot nothing.
    let meter = Meter {
        heat: u.weapon.heat,
        unlimited: u.weapon.rounds(wc).is_none(),
    };
    fire_discipline(sim, me, u, st, wc, meter, &skill);
    // Card FFLN: a friend in the line of fire. The shot waits (below); the
    // behaviour steps a holder with no Tactician's position aside.
    let line = enemy.and_then(|e| {
        let range = (e.capsule().center() - eye).length();
        // The cone a shot can leave in: the weapon's spread as it is now
        // (a sniper's AI table, the posture, the recoil) round the aim.
        let spread = u.weapon.yaw_spread(wc).max(u.weapon.pitch_spread(wc));
        friend_in_line(
            sim,
            me,
            u,
            eye,
            range,
            LINE_SPREAD + spread,
            Some(shot_late(wc)),
        )
        .or_else(|| friend_in_blast(sim, me, u, e.capsule().center(), wc))
    });
    let mut no_throw = false;
    let range = match (enemy, memory) {
        (Some(e), _) => {
            let range = (e.capsule().center() - eye).length();
            let ready =
                held.is_some_and(|c| perception::fireable(&c, sim.tick, tier.reaction_ticks));
            let in_reach = match armed {
                // Card CLSE4 (`analysis/re-soldier-ai.md` 20.2, both games):
                // the trigger fires a tracked target only strictly inside
                // the weapon in hand's `MaxRange`, eye to target in 3-D (a
                // pistol 24 m, a rifle 64). The bolt's own reach stays a
                // bound for a class whose bolt is the shorter. Not built:
                // the x 2 on the range at a snipe stand.
                // Card HVYW: no shot at a vehicle no weapon of the bot's
                // hurts (`re-soldier-ai.md` 22: it is not attacked at all),
                // nor with a heavy weapon at a soldier it is not for.
                Armed::Hand => {
                    range <= wc.ordnance.max_range() && range < wc.ai_range[2] && !st.arm.spare
                }
                Armed::Draw(_) => false,
                Armed::Thrown(gc, live) => {
                    live && reaches
                        && e_pitch.abs() < arsenal::LOB_PITCH
                        && gc.ai_range[0] < range
                        && range < gc.ai_range[2]
                }
            };
            no_throw = matches!(armed, Armed::Thrown(_, true)) && !in_reach;
            let on_target =
                ready && e_yaw.abs() < AIM_TOLERANCE && e_pitch.abs() < AIM_TOLERANCE && in_reach;
            // Card FFLN: on target, and a friend is in the way — no shot.
            nt.line_held = u32::from(on_target && line.is_some());
            let on_target = on_target && line.is_none();
            let want_fire = on_target && st.burst > 0 && sim.tick >= st.pause_until;
            if let Armed::Draw(button) = armed {
                // The switch is an edge, and dead while a trigger is down.
                if sim.tick.is_multiple_of(2) {
                    pad.buttons |= button;
                }
                st.charge_until = 0;
            } else if let Armed::Thrown(gc, _) = armed {
                // The firing channel is the grenade's: its trigger is tapped
                // (the disc's grenades are TriggerSingle), the other is up.
                if tap(sim, me, st, gc, &skill, on_target) {
                    pad.buttons |= buttons::SECONDARY_FIRE;
                    // The pick is spent: the throw runs on its own fire
                    // state, and the next think picks again.
                    st.arm.pick = None;
                }
                st.charge_until = 0;
            } else if wc.max_pressed_time > 0.0 {
                charge(sim, me, st, wc, &skill, want_fire, pad);
            } else if wc.trigger_single {
                // Card STTAP: a single-shot weapon fires on a fresh press
                // only, so the bot TAPS it, by the PS2's own rule and not by
                // D11's bursts (the PS2 sends it past the burst function).
                if tap(sim, me, st, wc, &skill, on_target) {
                    pad.buttons |= buttons::FIRE;
                }
            } else if want_fire {
                pad.buttons |= buttons::FIRE;
            }
            Some(range)
        }
        (None, _) => {
            if wc.trigger_single {
                tap(sim, me, st, wc, &skill, false);
            }
            // A charge abandoned with the target: the bot does not keep a
            // held trigger for an enemy it can no longer see.
            st.charge_until = 0;
            None
        }
    };
    Some(GunReport {
        line,
        range,
        no_throw,
    })
}

/// Shared charge-release policy (STCHG, re-ai-difficulty §4.4): draw once
/// at the press, hold until the deadline, then release for one tick.
/// Callers clear the deadline when no target is seen, as on foot.
pub(super) fn charge(
    sim: &Sim,
    me: UnitHandle,
    st: &mut AiState,
    wc: &WeaponClass,
    skill: &crate::skill::Skill,
    want_fire: bool,
    pad: &mut PadState,
) {
    if want_fire && st.charge_until == 0 {
        let draw = unit_unit(me.index, sim.tick, me.gen, SALT_CHARGE);
        let frac = CHARGE_MIN + (skill.charge_hi - CHARGE_MIN).max(0.0) * draw;
        let ticks = (frac * wc.max_pressed_time * crate::TICK_HZ as f32).round();
        st.charge_until = sim.tick + (ticks.max(1.0) as u64);
    }
    if st.charge_until > 0 {
        if sim.tick < st.charge_until {
            pad.buttons |= buttons::FIRE;
        } else {
            // The release edge: the trigger comes up THIS tick.
            st.charge_until = 0;
        }
    }
}

/// How late a shot of class `wc` leaves and how fast it flies (s, m/s), for
/// [`friend_in_line`]. Card HVYW (OURS): a missile leaves a wind-up late and
/// flies slowly. Card TEAM (OURS): any other shot leaves after this tick's
/// walk - the sim moves the units, then fires - so a friend crossing a metre
/// in front of the muzzle met it (a mortar's shell, one tick old: 40 of the
/// BF2 maps' 53 mortar team kills in 8 seeds, the friend 1-2 m out).
fn shot_late(wc: &crate::weapon::WeaponClass) -> (f32, f32) {
    match wc.ordnance.kind {
        crate::weapon::OrdKind::Missile => (wc.initial_salvo_delay, wc.ordnance.velocity),
        _ => (1.0 / crate::TICK_HZ as f32, wc.ordnance.velocity),
    }
}

/// Card FFLN: is a unit of `u`'s own team in its line of fire — the unit's
/// own aim from `eye`, out to `range` (its target) and [`LINE_PAST`] beyond,
/// where a miss flies on? A friend is in the line when its capsule's axis is
/// within [`LINE_CLEAR`] of it, widening by `spread` per metre ([`LINE_SPREAD`]
/// and the weapon's own spread). A friend beside or behind the muzzle counts
/// when it is that close to the muzzle itself. The answer is the side to step
/// to, away from the nearest such friend (+1 = the unit's right). OURS: the PS2's bots shoot through their friends
/// (`analysis/re-blaster-damage.md` section 6.3). Riders are behind their
/// vehicle's hull and are not counted.
///
/// Card HVYW (OURS): `late` is a MISSILE's wind-up (s) and speed (m/s). The
/// rocket leaves the tube a quarter of a second after the trigger and takes
/// a third of a second more to a tank 30 m off, and it kills the friend it
/// meets outright where a rifle's bolt wounds him: its line is cleared by
/// what the shooter walks in the wind-up and what the friend walks until
/// the rocket is where he is, on top of [`LINE_CLEAR`].
/// Card TEAM: the fastest a soldier walks or a shooter is carried, for the
/// grid's bound on [`friend_in_line`] (m/s; a sprint is under 10).
const LINE_WALK_BOUND: f32 = 20.0;

fn friend_in_line(
    sim: &Sim,
    me: UnitHandle,
    u: &Unit,
    eye: Vec3,
    range: f32,
    spread: f32,
    late: Option<(f32, f32)>,
) -> Option<i8> {
    let (dir, right, _) = u.aim_frame();
    let reach = range + LINE_PAST;
    let own = u.soldier.vel.length();
    let mut best: Option<(f32, i8)> = None;
    // Card TEAM: a friend in the line stands within the line's reach and
    // its widest clearance of the eye, plus what a unit at
    // [`LINE_WALK_BOUND`] walks before the shot is there; a missile's
    // walked term is longer, so it asks every unit.
    let wide = reach + LINE_CLEAR + spread.max(0.0) * reach + 1.0;
    let bound = match late {
        None => wide,
        Some((wind, speed)) if wind < 0.1 && speed > 20.0 => {
            wide + 2.0 * LINE_WALK_BOUND * (wind + reach / speed)
        }
        Some(_) => f32::INFINITY,
    };
    let mut near: Vec<u32> = Vec::new();
    sim.units_within(eye, bound, &mut near);
    for &i in &near {
        let (h, o) = (sim.handle(i as usize), &sim.units[i as usize]);
        if h == me || o.team != u.team || o.mount.is_some() {
            continue;
        }
        let c = o.capsule();
        let (lo, hi) = (
            c.base.y + c.radius,
            c.base.y + (c.height - c.radius).max(c.radius),
        );
        // The nearest points of the ray and the (vertical) axis: the ray's
        // parameter for the axis' middle, the axis point at that height, and
        // the parameter once more for that point.
        let mut q = Vec3::new(c.base.x, 0.5 * (lo + hi), c.base.z);
        let mut t = 0.0;
        for _ in 0..2 {
            t = (q - eye).dot(dir).clamp(0.0, reach);
            q.y = (eye.y + dir.y * t).clamp(lo, hi);
        }
        if best.is_some_and(|(bt, _)| bt <= t) {
            continue;
        }
        let off = q - (eye + dir * t);
        let walked = late.map_or(0.0, |(wind, speed)| {
            own * wind + o.soldier.vel.length() * (wind + t / speed.max(1.0))
        });
        if off.length() < LINE_CLEAR + spread * t + walked {
            best = Some((t, if off.dot(right) > 0.0 { -1 } else { 1 }));
        }
    }
    best.map(|(_, side)| side)
}

/// Card RKEX (OURS, FFLN's rule carried to the blast): a weapon whose
/// ordnance detonates an impact explosion (the rocket launchers) holds its
/// shot while a unit of `u`'s team on foot stands within the explosion's
/// `DamageRadiusOuter` + [`BLAST_CLEAR`] of the target `at` - the blast that
/// kills the target kills him too. The PS2's bots fire regardless
/// (re-soldier-ai 6: no awareness of an AI friend); the side is the one
/// away from the nearest such friend, as [`friend_in_line`]'s.
fn friend_in_blast(
    sim: &Sim,
    me: UnitHandle,
    u: &Unit,
    at: Vec3,
    wc: &crate::weapon::WeaponClass,
) -> Option<i8> {
    let r = wc.ordnance.explosion.as_ref()?.damage_outer;
    if r <= 0.0 {
        return None;
    }
    let (_, right, _) = u.aim_frame();
    let mut near: Vec<u32> = Vec::new();
    sim.units_within(at, r + BLAST_CLEAR, &mut near);
    let mut best: Option<(f32, i8)> = None;
    for &i in &near {
        let (h, o) = (sim.handle(i as usize), &sim.units[i as usize]);
        if h == me || o.team != u.team || o.mount.is_some() || !o.alive {
            continue;
        }
        let off = o.capsule().center() - at;
        let d = off.length();
        if d < r + BLAST_CLEAR && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, if off.dot(right) > 0.0 { -1 } else { 1 }));
        }
    }
    best.map(|(_, side)| side)
}

/// Card RKSLF (OURS): does the exploding shot bot `i`'s weapon holds land
/// its blast on its own side? Asked where the shot leaves, not at the
/// press: the launcher's cycle releases a salvo after the trigger
/// (InitialSalvoDelay, a second salvo; `weapon::fire_cycle`), by when the
/// muzzle may face the cover beside it. The bolt's own spawn ray along the
/// aim (spread left out); true when the blast point,
/// [`crate::weapon::EXPLOSION_OFFSET`] short of the hit, is within
/// `DamageRadiusOuter` + [`BLAST_CLEAR`] of the bot or a team-mate on foot. The PS2's cycle cancels a salvo only when the weapon
/// is put away (weapon+0xc4), and its bots fire regardless (re-soldier-ai 6).
pub(crate) fn release_blasts_own_side(sim: &Sim, i: usize) -> bool {
    let u = &sim.units[i];
    let wc = &sim.weapon_classes[u.weapon_class];
    let Some(r) = wc.ordnance.explosion.as_ref().map(|e| e.damage_outer) else {
        return false;
    };
    let speed = wc.ordnance.velocity;
    if r <= 0.0 || speed <= 0.0 {
        return false;
    }
    let me = sim.handle(i);
    let (fwd, _, _) = u.aim_frame();
    let who = crate::projectile::Shooter {
        owner: me,
        team: u.team,
        weapon_class: u.weapon_class,
        is_player: u.is_player,
    };
    let b = crate::projectile::Bolt::spawn(
        u.muzzle(),
        fwd,
        speed,
        wc.ordnance.life_span,
        who,
        sim.heightfield.as_ref(),
        &sim.collision,
        crate::collision::HullFilter::All,
        sim.alive_units()
            .filter(|(h, _)| *h != me)
            .map(|(h, o)| (h, o.capsule())),
    );
    if matches!(b.hit, crate::projectile::Hit::None) {
        return false;
    }
    let at = b.end - fwd * crate::weapon::EXPLOSION_OFFSET;
    let mut near: Vec<u32> = Vec::new();
    sim.units_within(at, r + BLAST_CLEAR, &mut near);
    near.iter().any(|&j| {
        let o = &sim.units[j as usize];
        o.alive
            && o.team == u.team
            && (j as usize == i || o.mount.is_none())
            && (o.capsule().center() - at).length() < r + BLAST_CLEAR
    })
}

/// [`friend_in_blast`]'s margin past the explosion's outer radius (m): a
/// standing soldier's blast sphere reaches 0.73 m nearer than his centre
/// (`sim::blast::soldier_blast_sphere`) and a step more. PROVISIONAL (ours).
const BLAST_CLEAR: f32 = 1.0;

/// The aim point at a SEEN enemy `e` from `eye`, for a bolt of `speed` out of
/// a weapon of class `wc` (on foot and in a seat, VTAI). BF2: the lead point
/// offset by the TARGET's own [`radius`] along the SHOOTER's direction
/// (`unit::AimBlock`), through `error_vector` (card CLSE2). BF1: the lead
/// point offset by the shooter's own tracked error `st.err`, whole, in a direction
/// drawn anew each think - D10's model, PROVISIONAL and known wrong: the
/// PS2's BF1 bot aims at the lead point with NO error vector
/// (`analysis/re-soldier-ai.md` 18.5: B1 `FUN_00233200` -> `FUN_00232a70`; a
/// still PS2 bot's view ray passes its target at a flat 0.06 m at every
/// range). That rule is built and measured and held back (card CLSE2B): with
/// it the port's BF1 bots kill from farther off (an enemy within 20 m 10.9
/// -> 8.1 % of unit time on the 16 BF1 maps), away from the PS2, until the
/// far fire is the PS2's (card CLSE4).
pub(super) fn seen_aim(
    sim: &Sim,
    me: UnitHandle,
    st: &AiState,
    eye: Vec3,
    e: &Unit,
    speed: f32,
    wc: &WeaponClass,
) -> Vec3 {
    let c = e.capsule().center();
    let aim = lead_point(c - eye, e.soldier.vel, speed).map_or(c, |rel| eye + rel);
    let Some(u) = sim.unit(me) else {
        return aim;
    };
    // The cap is the shooter's skill's, under the tier's override (Elite).
    let cap = sim.difficulty_of(u.team).skill_at(u.skill).err_cap;
    let moving = e.soldier.vel.length() / e.consts.max_speed.max(0.1);
    if sim.ground_rules != crate::soldier::GroundRules::Bf2 {
        let vis = crate::perception::VIS;
        let r = radius(1.0, 1.0, st.err, moving, vis, cap);
        return aim + unit_dir(me.index, sim.tick / THINK_PERIOD, me.gen, SALT_ERR_DIR) * r;
    }
    let r = radius(
        e.consts.aim_size,
        stance_factor(e.soldier.stance),
        e.aim.err,
        moving,
        crate::perception::VIS,
        cap,
    );
    aim + error_vector(sim, me, e, u.aim.dir * r, (aim - eye).length(), wc)
}

/// BF2's error radius at a target (B2 `FUN_002ae4d0`, 18.2 and 18.3; 80 rows
/// of PS2 RAM reproduce it, `tests/radius_rows.rs`): `size` x (3 - 2 x `vis`) x
/// min(`err` x `stance` x (1 + 0.6 x `moving`), `cap`) - `size` the target's
/// model term (`SoldierConstants::aim_size`), `stance` its
/// [`stance_factor`], `err` its block's error, `moving` its speed over its
/// MaxSpeed, `cap` the skill's.
pub(super) fn radius(size: f32, stance: f32, err: f32, moving: f32, vis: f32, cap: f32) -> f32 {
    size * (3.0 - 2.0 * vis) * (err * stance * (1.0 + ERR_SPEED_TERM * moving)).min(cap)
}

/// What a target's stance does to its error, inside the cap (B2
/// `FUN_0024ea00`, 18.2: the stance word's 1 crouch, 2 / 3 / 4 prone, sprint
/// and jump, 5 roll): a crouched target is aimed at half again as badly, a
/// sprinting or jumping one twice, a rolling one three times. `Fall` is not
/// one of the PS2's five and keeps 1.0 (OURS).
pub(super) fn stance_factor(stance: Stance) -> f32 {
    match stance {
        Stance::Crouch => 1.5,
        Stance::Prone | Stance::Sprint | Stance::Jump => 2.0,
        Stance::Roll => 3.0,
        // Not one of the five either (OURS): a thrown or fallen body.
        Stance::Stand
        | Stance::Fall
        | Stance::Thrown
        | Stance::Tumble
        | Stance::GetUp
        | Stance::Dead => 1.0,
    }
}

/// BF2 scales a bot's error by 0.3 against a target that carries no human
/// (`analysis/re-soldier-ai.md` 17.2, B2 `FUN_002a4cc8`).
pub(super) const ERR_BOT_SCALE: f32 = 0.3;

/// What BF2 does to the error vector `off` between the block and the aim
/// point `d` metres away, for a shot of weapon class `wc` (cards CLSE and
/// CLSE2, `analysis/re-soldier-ai.md` 17.2 and 18.4: B2 `FUN_002a4cc8`, then
/// `FUN_002a50c0`'s factor): times the class's `AIBubbleSizeMultiplier`; it
/// never points below the aim point; it is nothing when the shooter rides
/// with a human, 0.3 of itself against a target that carries no human and
/// whole against one that does; and it grows with the range, x (d /
/// `AIBubbleScaleDistDivider` + 0.5), at most `AIBubbleScaleClamp`. So a bot
/// hits a bot at arm's length and misses it far off, and a player is three
/// times as hard to hit as a bot. PROVISIONAL (left out): the U(2, 3) m
/// upper clamp of y (above the radius's cap), the perpendicular draw of a
/// shooter at full speed, and the two EntityFlyer terms (x 0.1 against a
/// flyer when the shooter's state is 1; a flyer in state 2 exempt from the
/// clamp - 17.2 steps 4 and 6: the port has no flyer).
fn error_vector(
    sim: &Sim,
    me: UnitHandle,
    e: &Unit,
    mut off: Vec3,
    d: f32,
    wc: &WeaponClass,
) -> Vec3 {
    off *= wc.ai_bubble_size;
    off.y = off.y.max(0.0);
    if sim.unit(me).is_some_and(|u| carries_human(sim, u)) {
        return Vec3::ZERO;
    }
    if !carries_human(sim, e) {
        off *= ERR_BOT_SCALE;
    }
    off * (d / wc.ai_bubble_dist + 0.5).min(wc.ai_bubble_clamp)
}

/// Is `u` a human, or riding a vehicle a human rides (B2 `FUN_002a7fd8`:
/// the humans among the occupants of the unit's entity)?
fn carries_human(sim: &Sim, u: &Unit) -> bool {
    u.is_player
        || u.mount
            .and_then(|m| sim.vehicle(m.vehicle))
            .is_some_and(|v| {
                v.seats
                    .iter()
                    .filter_map(|s| s.occupant.and_then(|h| sim.unit(h)))
                    .any(|o| o.is_player)
            })
}

/// [`AiState::burst`] of a weapon with unlimited rounds: a burst that no
/// shot count ends (card BRSTU).
pub(super) const HEAT_BURST: u8 = u8::MAX;

/// The weapon a burst is fired with, as [`fire_discipline`] reads it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Meter {
    /// The weapon's heat now, 0..1 (`WeaponState::heat`).
    pub heat: f32,
    /// Unlimited rounds: `RoundsPerClip` under 1 or an endless reserve.
    pub unlimited: bool,
}

/// D11's burst counter, once per tick before the fire decision (on foot and
/// in a seat, VTAI); FIRE goes down only while a burst runs (`st.burst > 0`)
/// and the pause has passed. `wc` is the weapon fired: the bounds and the
/// pause are its class's Barrage keys (card CLSE4).
///
/// Card BRSTU (`analysis/re-soldier-ai.md` 23.2, B2 `FUN_002bac98`, B1
/// `FUN_0023c9c8`): the PS2's n counts ROUNDS spent, so a weapon with
/// unlimited rounds (every seat gun, the repeater, the chaingun) is never
/// held to n ([`HEAT_BURST`]). The meter is the heat: a burst starts only
/// when n shots' heat fits in what is left ([`heat_fits`]) and ends when
/// one more salvo would not ([`heat_spent`]), then the class's pause from
/// that tick. With `HeatPerShot` 0 an unlimited burst lasts as long as the
/// bot fires. A counted weapon's burst: the previous tick's shot spends one
/// and an empty burst starts the pause, counted from that last shot.
/// Not built: a bot that stops wanting to fire owes no pause on the PS2
/// (23.3), and its next burst draws a new n. So the n a waiting burst tests
/// is redrawn each tick here: an n kept until the want drops would hold a
/// gun whose n shots cost exactly a full meter (not over 1, so not capped:
/// BF1 Hoth's turret cannon, 4 x 0.25) off for as long as it has a target.
pub(super) fn fire_discipline(
    sim: &Sim,
    me: UnitHandle,
    u: &Unit,
    st: &mut AiState,
    wc: &crate::weapon::WeaponClass,
    m: Meter,
    skill: &crate::skill::Skill,
) {
    // A weapon switch across the two kinds ends the burst, owing nothing.
    if (st.burst == HEAT_BURST) != m.unlimited && st.burst != 0 {
        st.burst = 0;
    }
    if st.burst > 0 && st.burst != HEAT_BURST && sim.tick > 0 && u.last_shot_tick == sim.tick - 1 {
        st.burst -= 1;
        if st.burst == 0 {
            st.pause_until = u.last_shot_tick + pause_ticks(sim.ground_rules, wc, skill);
        }
    }
    if st.burst > 0 && heat_spent(wc, m.heat) {
        st.burst = 0;
        st.pause_until = sim.tick + pause_ticks(sim.ground_rules, wc, skill);
    }
    if st.burst == 0 && sim.tick >= st.pause_until {
        let n = burst_len(sim.ground_rules, wc, me, sim.tick, skill);
        if heat_fits(wc, n, m.heat) {
            st.burst = if m.unlimited { HEAT_BURST } else { n };
        }
    }
}

/// One salvo's heat (`HeatPerShot` x `ShotsPerSalvo`), and the salvo's shot
/// count (at least one).
fn salvo_heat(wc: &WeaponClass) -> (f32, f32) {
    let s = wc.shots_per_salvo.max(1) as f32;
    (wc.heat_per_shot * s, s)
}

/// The PS2's start test (state 2): n shots' heat, whole salvos, capped at
/// 0.99, under what is left of the meter.
fn heat_fits(wc: &WeaponClass, n: u8, heat: f32) -> bool {
    let (h, s) = salvo_heat(wc);
    let mut cost = (f32::from(n) / s).ceil() * h;
    if cost > 1.0 {
        cost = 0.99;
    }
    cost < 1.0 - heat
}

/// The PS2's end test (state 3): the heat is within one salvo (capped at
/// 0.99) and 0.0001 of full.
fn heat_spent(wc: &WeaponClass, heat: f32) -> bool {
    let (mut h, _) = salvo_heat(wc);
    if h >= 1.0 {
        h = 0.99;
    }
    1.0 - h - 0.0001 <= heat
}

/// Card STTAP: one tick of a bot's trigger on a `TriggerSingle` weapon
/// (`analysis/re-round-stalls.md` §2 C4) — is FIRE down this tick? `want` is
/// the fire decision before any cadence. A press that lands in the weapon's
/// ShotDelay, reload or overheat fires nothing, as on the PS2.
///
/// - **BF2** (`FUN_002baf70`): a press lasts one tick, sets the gate and draws
///   the re-arm `U(0.2, 2.0) × tap_x` s; every other wanting tick releases,
///   and clears the gate once the re-arm has passed. An idle tick sets the
///   gate with nothing to wait for, so the first press comes on the second
///   wanting tick. A sniper (TriggerSingle and ZoomMax ≥ 6, `FUN_002ace10`)
///   goes to `FUN_002baa38` instead: one press, then `U(2, 3)` s.
/// - **BF1** (`FUN_0023cc20` inside `FUN_0023cd40`): the trigger byte runs
///   1 → 2 → −1 → −2 → 1 while the bot wants to fire, a fresh press every
///   fourth tick; a sniper first passes `FUN_0023c698`'s `U(2, 3)` s cooldown.
///   PROVISIONAL: that function's `weapon+0xcc == 0` test is taken as true.
pub(super) fn tap(
    sim: &Sim,
    me: UnitHandle,
    st: &mut AiState,
    wc: &crate::weapon::WeaponClass,
    skill: &crate::skill::Skill,
    want: bool,
) -> bool {
    let hz = crate::TICK_HZ as f32;
    let draw = unit_unit(me.index, sim.tick, me.gen, SALT_TAP);
    let sniper = wc.zoom_max >= 6.0;
    match sim.ground_rules {
        crate::soldier::GroundRules::Bf2 if sniper => {
            if !want || sim.tick < st.snipe_until {
                return false;
            }
            st.snipe_until = sim.tick + ((2.0 + draw) * hz).ceil() as u64;
            true
        }
        crate::soldier::GroundRules::Bf2 => {
            if !want {
                st.tap_until = 0;
                st.tap_gate = true;
                false
            } else if !st.tap_gate {
                st.tap_gate = true;
                let secs = skill.tap_x * (0.2 + 1.8 * draw);
                st.tap_until = sim.tick + (secs * hz) as u64;
                true
            } else {
                if sim.tick > st.tap_until {
                    st.tap_gate = false;
                }
                false
            }
        }
        crate::soldier::GroundRules::Bf1 => {
            let mut want = want;
            if sniper {
                if sim.tick <= st.snipe_until {
                    want = false;
                } else if want {
                    st.snipe_until = sim.tick + ((2.0 + draw) * hz) as u64;
                }
            }
            let b = st.tap_byte;
            let press = want && (b == 0 || b == 1 || b <= -2);
            st.tap_byte = match (press, b) {
                (true, 0) => 2,
                (true, b) if b < 0 => 1,
                (true, b) => (b + 1).min(100),
                (false, 0) => -2,
                (false, b) if b < 0 => (b - 1).max(-100),
                (false, _) => -1,
            };
            // A byte that was never set goes straight to 2, which the
            // weapon's predicate (byte == 1) does not fire on.
            press && b != 0
        }
    }
}

/// D11: the next burst length with `wc` (card CLSE4,
/// `analysis/re-soldier-ai.md` 20.3). BF2 (`FUN_002bac98`): `max(2,
/// trunc(min + U[0, max − min)·t))`, t the skill's - 3 at the lowest level
/// and 3..5 at the highest for the class defaults 3 / 6. BF1
/// (`FUN_0023c9c8`): a uniform integer of min..=max, no skill term.
fn burst_len(
    rules: crate::soldier::GroundRules,
    wc: &crate::weapon::WeaponClass,
    me: UnitHandle,
    tick: u64,
    skill: &crate::skill::Skill,
) -> u8 {
    let [min, max] = wc.barrage.map(f32::from);
    let draw = unit_unit(me.index, tick, me.gen, SALT_BURST);
    match rules {
        crate::soldier::GroundRules::Bf2 => {
            ((min + draw * (max - min).max(0.0) * skill.burst_t) as u8).max(2)
        }
        // (At least one: a burst of none is the port's "between bursts".)
        crate::soldier::GroundRules::Bf1 => {
            ((min + (draw * (max - min + 1.0).max(1.0)).floor()).min(max.max(min)) as u8).max(1)
        }
    }
}

/// D11: ticks of the inter-burst pause with `wc`: BF2 `(1.5 − 0.8·t) ×
/// BarrageDelay` (`FUN_002bac98`, state 2), BF1 the delay flat.
fn pause_ticks(
    rules: crate::soldier::GroundRules,
    wc: &crate::weapon::WeaponClass,
    skill: &crate::skill::Skill,
) -> u64 {
    let factor = match rules {
        crate::soldier::GroundRules::Bf2 => skill.pause_mult,
        crate::soldier::GroundRules::Bf1 => 1.0,
    };
    (factor * wc.barrage_delay * crate::TICK_HZ as f32)
        .round()
        .max(0.0) as u64
}

/// Rotate `from` 30° toward `to` about their common perpendicular (D9's slew,
/// RE §4.2 step 3). Falls back to `to` when the two are (anti)parallel, where
/// the axis is undefined.
fn slew(from: Vec3, to: Vec3) -> Vec3 {
    let axis = from.cross(to);
    let len = axis.length();
    if len < 1e-6 {
        return to;
    }
    let k = axis / len;
    // Rodrigues at 30°, in `fmath`-free constants (no libm in core).
    (from * SLEW_COS + k.cross(from) * SLEW_SIN + k * k.dot(from) * (1.0 - SLEW_COS)).normalize()
}

#[cfg(test)]
mod tests;
