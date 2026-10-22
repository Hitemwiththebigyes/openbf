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
//! bursts). The aim and fire tests live here.

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
                // A CHARGE weapon fires on the trigger RELEASE (`weapon.rs`
                // `fire_cmd == -1`): the bot holds for `U(0.15, charge_hi) ×
                // MaxPressedTime` seconds, drawn ONCE at the press (RE §4.4),
                // and then lets go. No roster soldier carries one today.
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
mod tests {
    use super::*;
    use crate::ai::think;

    mod radius_rows;

    /// Card CLSE2: the radius formula against the PS2's own RAM - 80 rows
    /// at rest (69 standing, 11 crouched), each within 1e-4 m.
    #[test]
    fn the_radius_reproduces_the_ps2s_ram_rows() {
        let (mut crouched, mut capped) = (0, 0);
        for &(size, word, err, vis, cap, ps2) in &radius_rows::ROWS {
            let stance = match word {
                0 => Stance::Stand,
                1 => Stance::Crouch,
                w => panic!("stance word {w}"),
            };
            crouched += usize::from(word == 1);
            capped += usize::from(err * stance_factor(stance) > cap);
            let r = radius(2.0 * size, stance_factor(stance), err, 0.0, vis, cap);
            assert!(
                (r - ps2).abs() < 1e-4,
                "size {size} stance {word} err {err} vis {vis}: {r} against the PS2's {ps2}"
            );
        }
        assert_eq!((radius_rows::ROWS.len(), crouched), (80, 11));
        assert!(capped > 0, "the cap is exercised");
        // The arms no row at rest shows (the ELF's jump table, 18.2), and the
        // speed term on top.
        assert_eq!(stance_factor(Stance::Sprint), 2.0);
        assert_eq!(stance_factor(Stance::Jump), 2.0);
        assert_eq!(stance_factor(Stance::Prone), 2.0);
        assert_eq!(stance_factor(Stance::Roll), 3.0);
        assert!((radius(1.0, 1.0, 0.5, 1.0, 1.0, 9.0) - 0.8).abs() < 1e-6);
    }

    /// Two still bots 20 m apart in a round of `rules`, the shooter's
    /// direction set to +X and the target's error to 0.8 m.
    fn aim_pair(rules: crate::soldier::GroundRules) -> (Sim, UnitHandle, UnitHandle) {
        use crate::unit::{Controller, UnitSpec};
        let mut s = Sim::empty(5);
        s.ground_rules = rules;
        let mut bot = |team: u8, z: f32| {
            s.spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(team, Vec3::new(0.0, 0.0, z))
            })
            .unwrap()
        };
        let (a, t) = (bot(0, 0.0), bot(1, -20.0));
        s.units[a.index as usize].aim.dir = Vec3::X;
        s.units[t.index as usize].aim.err = 0.8;
        (s, a, t)
    }

    /// The aim point's offset from the target's centre, for `aim_pair`, over
    /// the bot-against-bot and range factors (0.3 x (d / 20 + 0.5)): the
    /// radius along the shooter's direction.
    fn aim_offset(s: &Sim, a: UnitHandle, t: UnitHandle) -> Vec3 {
        let e = s.unit(t).unwrap();
        let eye = s.unit(a).unwrap().muzzle();
        let c = e.capsule().center();
        let st = AiState::default();
        let off = seen_aim(s, a, &st, eye, e, 1e6, &WeaponClass::default()) - c;
        off / (ERR_BOT_SCALE * ((c - eye).length() / 20.0 + 0.5))
    }

    /// Card CLSE2: a BF2 bot's aim point is off by the TARGET's own error,
    /// along the SHOOTER's own direction, times the target's model size and
    /// stance - and the shooter's own block's error is nothing to it.
    #[test]
    fn a_bf2_bot_aims_off_by_its_targets_own_error() {
        let (mut s, a, t) = aim_pair(crate::soldier::GroundRules::Bf2);
        let (ai, ti) = (a.index as usize, t.index as usize);
        let base = aim_offset(&s, a, t);
        assert!((base - Vec3::X * 0.8).length() < 1e-5, "{base}");
        // The shooter's own error is not asked.
        s.units[ai].aim.err = 2.0;
        assert_eq!(aim_offset(&s, a, t), base);
        // The target's is.
        s.units[ti].aim.err = 0.4;
        assert!((aim_offset(&s, a, t) - base * 0.5).length() < 1e-5);
        s.units[ti].aim.err = 0.8;
        // Its model: the super battle droid's 0.48.
        s.units[ti].consts.aim_size = 0.48;
        assert!((aim_offset(&s, a, t) - base * 0.48).length() < 1e-5);
        s.units[ti].consts.aim_size = 1.0;
        // Its stance, inside the cap (1.36 m at skill 17): crouched 1.5.
        s.units[ti].soldier.stance = Stance::Crouch;
        assert!((aim_offset(&s, a, t) - base * 1.5).length() < 1e-5);
        s.units[ti].soldier.stance = Stance::Roll;
        let cap = crate::skill::Skill::from_level(17).err_cap;
        assert!((aim_offset(&s, a, t) - Vec3::X * cap).length() < 1e-5);
        s.units[ti].soldier.stance = Stance::Stand;
        // The shooter's direction: turned, the offset turns with it; and it
        // never points under the aim point.
        s.units[ai].aim.dir = Vec3::new(0.0, -0.6, 0.8);
        let turned = aim_offset(&s, a, t);
        assert!(
            (turned - Vec3::new(0.0, 0.0, 0.8) * 0.8).length() < 1e-5,
            "{turned}"
        );
    }

    /// A BF1 round keeps D10's model (PROVISIONAL, card CLSE2B): the offset
    /// is the SHOOTER's tracked error, whole - no factor by target or range,
    /// nothing of the blocks - and the sim thinks no block.
    #[test]
    fn a_bf1_bot_keeps_its_own_tracked_error() {
        let (mut s, a, t) = aim_pair(crate::soldier::GroundRules::Bf1);
        let skill = crate::skill::Skill::from_level(17);
        let off = |s: &Sim, err: f32| {
            let st = AiState {
                err,
                ..AiState::default()
            };
            let e = s.unit(t).unwrap();
            let eye = s.unit(a).unwrap().muzzle();
            let wc = WeaponClass::default();
            seen_aim(s, a, &st, eye, e, 1e6, &wc) - e.capsule().center()
        };
        let dir = unit_dir(a.index, s.tick / THINK_PERIOD, a.gen, SALT_ERR_DIR);
        assert!((off(&s, 0.7) - dir * 0.7).length() < 1e-4);
        assert!(
            (off(&s, 9.0) - dir * skill.err_cap).length() < 1e-4,
            "capped"
        );
        s.units[t.index as usize].aim.err = 2.0;
        s.units[t.index as usize].consts.aim_size = 0.48;
        s.units[t.index as usize].is_player = true;
        assert!(
            (off(&s, 0.7) - dir * 0.7).length() < 1e-4,
            "nothing of BF2's"
        );
        let before = s.unit(a).unwrap().aim;
        for _ in 0..10 {
            s.step(&[]);
        }
        assert_eq!(s.unit(a).unwrap().aim, before);
    }

    /// A BF1 round (D10): the shooter's tracked error DECAYS toward the skill's floor on a
    /// still target and GROWS when the target moves, and it is clamped into
    /// `[floor, 2.0]` with a target and `[1.5, 2.0]` without one (LEAD
    /// ADDENDUM 4a).
    #[test]
    fn the_tracked_error_decays_to_the_floor_and_grows_with_movement() {
        use crate::skill::{Difficulty, Skill};
        use crate::unit::{Controller, UnitSpec};
        // A bot's skill is the TIER's (`core::skill_for`), so the level is
        // chosen by the difficulty: Easy 8, Normal 17.
        let build = |d: Difficulty| {
            let mut s = crate::sim::Sim::empty(5);
            s.ground_rules = crate::soldier::GroundRules::Bf1;
            s.difficulty = d;
            s.heightfield = Some(crate::heightfield::Heightfield {
                size: 2,
                unit: 4000.0,
                offset_x: 2000.0,
                offset_z: 2000.0,
                heights: vec![0.0; 4],
            });
            let a = s
                .spawn(&UnitSpec {
                    controller: Controller::Ai,
                    ..UnitSpec::trooper(0, Vec3::ZERO)
                })
                .unwrap();
            let t = s
                .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -20.0)))
                .unwrap();
            let level = s.unit(a).unwrap().skill;
            assert_eq!(level, d.base_skill());
            (s, a, t, level)
        };
        // A still target, five seconds: the error decays to the floor.
        for d in [Difficulty::Easy, Difficulty::Normal] {
            let (s, a, _, level) = build(d);
            let mut st = AiState::default();
            assert_eq!(st.err, ERR_NO_TARGET, "starts at the no-target floor");
            let mut sim = s;
            for t in 0..150u64 {
                sim.tick = t;
                think(&sim, a, &mut st);
            }
            let floor = Skill::from_level(level).err_floor;
            assert!(
                (st.err - floor).abs() < 1e-3,
                "skill {level} decayed to {} not {floor}",
                st.err
            );
        }
        // The floors themselves, as ADDENDUM 4a rules them: 1.158 m at
        // skill 8, 0.589 m at 17 (the brief's "1 m / 0" text is superseded).
        assert!((Skill::from_level(8).err_floor - 1.157_894_7).abs() < 1e-6);
        assert!((Skill::from_level(17).err_floor - 0.589_473_6).abs() < 1e-6);

        // A target moving at full speed GROWS the error above the floor.
        let (mut sim, a, t, _) = build(Difficulty::Normal);
        let mut st = AiState::default();
        let speed = sim.unit(t).unwrap().consts.max_speed;
        for k in 0..30u64 {
            sim.tick = k * THINK_PERIOD;
            // Walk the target sideways at its own top speed between thinks.
            let step = speed * THINK_PERIOD as f32 / crate::TICK_HZ as f32;
            let u = sim.unit_mut(t).unwrap();
            u.soldier.pos.x += step;
            think(&sim, a, &mut st);
        }
        let floor = Skill::from_level(17).err_floor;
        assert!(
            st.err > floor + 0.05,
            "moving target grew the error: {}",
            st.err
        );
        assert!(st.err <= ERR_MAX);

        // With no target at all the error is held at the PS2's 1.5 m no-target
        // floor, never reset and never below it.
        let (mut sim, a, t, _) = build(Difficulty::Normal);
        sim.units[t.index as usize].alive = false;
        let mut st = AiState {
            err: 0.2,
            ..Default::default()
        };
        think(&sim, a, &mut st);
        assert_eq!(st.err, ERR_NO_TARGET);
    }

    /// A BF1 round (D10): the aim point is offset by at most the effective radius, in a
    /// direction that is constant within a think and different across thinks.
    #[test]
    fn the_error_offset_is_bounded_and_re_drawn_once_per_think() {
        let r = |tick: u64| unit_dir(3, tick / THINK_PERIOD, 1, SALT_ERR_DIR);
        assert_eq!(r(0), r(THINK_PERIOD - 1), "constant within a think");
        assert_ne!(r(0), r(THINK_PERIOD), "re-drawn next think");
        assert_ne!(r(0), unit_dir(4, 0, 1, SALT_ERR_DIR), "per unit");
        for t in 0..40u64 {
            assert!((r(t).length() - 1.0).abs() < 1e-5, "unit vector");
        }
        // The Elite tier's cap overrides the level's, so the offset a skill-20
        // Elite bot can carry is 0.5 m and a Hard one 1.2 m.
        use crate::skill::Difficulty;
        assert_eq!(Difficulty::Elite.skill_at(20).err_cap, 0.5);
        assert!((Difficulty::Hard.skill_at(20).err_cap - 1.2).abs() < 1e-6);
    }

    /// D9: the aim tracker steers the aim direction instead of snapping it,
    /// slews when the target is more than 30° off, and is idle with no target.
    #[test]
    fn the_aim_tracker_steers_and_slews() {
        use crate::unit::{Controller, UnitSpec};
        let build = |enemy_at: Option<Vec3>| {
            let mut s = crate::sim::Sim::empty(5);
            s.heightfield = Some(crate::heightfield::Heightfield {
                size: 2,
                unit: 4000.0,
                offset_x: 2000.0,
                offset_z: 2000.0,
                heights: vec![0.0; 4],
            });
            let a = s
                .spawn(&UnitSpec {
                    controller: Controller::Ai,
                    ..UnitSpec::trooper(0, Vec3::ZERO)
                })
                .unwrap();
            if let Some(p) = enemy_at {
                s.spawn(&UnitSpec::trooper(1, p)).unwrap();
            }
            (s, a)
        };
        // A target straight ahead: the tracker starts at the unit's facing and
        // stays a unit vector while it closes on the aim point.
        let (sim, a) = build(Some(Vec3::new(0.0, 0.0, -20.0)));
        let mut st = AiState::default();
        think(&sim, a, &mut st);
        assert!((st.aim_dir.length() - 1.0).abs() < 1e-5, "unit vector");
        let first = st.aim_dir;
        let mut sim = sim;
        for t in 1..20u64 {
            sim.tick = t;
            think(&sim, a, &mut st);
        }
        assert!((st.aim_dir.length() - 1.0).abs() < 1e-5);
        assert_ne!(st.aim_dir, first, "the tracker moved");

        // A target 90° off the facing: the first tick SLEWS 30° toward it and
        // zeroes the rate, rather than accelerating across.
        // 60° off the facing — inside the 77.5° cone, so it IS perceived,
        // and more than the tracker's 30° slew step.
        let (sim, a) = build(Some(Vec3::new(-17.32, 0.0, -10.0)));
        let mut st = AiState::default();
        assert!(sim.unit(a).is_some());
        let facing = sim.unit(a).unwrap().aim_frame().0;
        think(&sim, a, &mut st);
        assert!(st.target.is_some(), "60 deg off is inside the cone");
        let cos = st.aim_dir.dot(facing).clamp(-1.0, 1.0);
        assert!(
            (cos - SLEW_COS).abs() < 1e-3,
            "slewed exactly 30 deg: cos {cos}"
        );
        assert_eq!(st.aim_vel, Vec3::ZERO, "a slew zeroes the rate");

        // With NO target the tracker is idle and the travel facing is
        // untouched.
        let (sim, a) = build(None);
        let mut st = AiState::default();
        think(&sim, a, &mut st);
        assert_eq!(st.aim_dir, Vec3::ZERO);
        assert_eq!(st.aim_vel, Vec3::ZERO);
    }

    /// Accuracy against range under the Phase C aim model (D9/D10): the
    /// tracked-position error is a distance at the target and the weapon's
    /// dispersion is an angle, so the hit rate falls with range and never
    /// grows.
    #[test]
    fn aim_error_scales_misses_with_range() {
        // Card FGHT: a shooter that keeps its place.
        let _style = crate::ai::tests::all_stand();
        use crate::sim::{Event, Sim};
        use crate::unit::{Controller, UnitSpec};
        let rate_of = |dist: f32, human: bool| {
            let mut s = Sim::empty(5);
            s.cheats.infinite_ammo = true;
            let shooter = s
                .spawn(&UnitSpec {
                    controller: Controller::Ai,
                    ..UnitSpec::trooper(0, Vec3::ZERO)
                })
                .unwrap();
            // Immortal dummy target (health topped up every tick).
            let t = s
                .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -dist)))
                .unwrap();
            // Card CLSE: the error is whole against a human's unit.
            s.units[t.index as usize].is_player = human;
            let (mut shots, mut hits) = (0, 0);
            // (1800 ticks: card CLSE4's pause is 1.7 s of every burst.)
            for _ in 0..1800 {
                s.step(&[PadState::default()]);
                s.unit_mut(t).unwrap().health.cur = 300.0;
                shots += s
                    .events
                    .iter()
                    .filter(|e| matches!(e, Event::Shot { unit, .. } if *unit == shooter))
                    .count();
                hits += s
                    .events
                    .iter()
                    .filter(|e| matches!(e, Event::Hit { .. }))
                    .count();
            }
            assert!(shots > 20, "AI fired {shots} shots at {dist} m");
            hits as f32 / shots as f32
        };
        // MEASURED at seed 5, a Normal bot (skill 17) against a still
        // dummy, 300 ticks each: 0.545 at 4 m, 0.528 at 8, 0.444 at 16,
        // 0.417 at 24, 0.250 at 40. The shape (misses grow with range) is
        // the weapon's own angular dispersion; the LEVEL up close fell from
        // the P4 placeholder's >0.8 because the AI's error is now a DISTANCE
        // at the tracked position (D10) — 1.4 m of error is 10° at 8 m and
        // 2° at 40 — so a near shot is no longer nearly free.
        //
        // D11 (bursts) then RAISED the far end and lowered the near one
        // (before it: 0.614 / 0.571 / 0.408 / 0.224 / 0.143): the
        // inter-burst pause lets `WeaponState.spread` recover, so the bot no
        // longer sprays its later shots from a fully bloomed cone. The
        // sample is small (the same 300 ticks now carry ~25 shots, not
        // ~90), so the bands below are deliberately wide.
        let rate = |dist: f32| rate_of(dist, true);
        let rates: Vec<f32> = [4.0, 8.0, 16.0, 24.0, 40.0]
            .iter()
            .map(|&d| rate(d))
            .collect();
        let bots: Vec<f32> = [4.0, 8.0, 16.0, 24.0, 40.0]
            .iter()
            .map(|&d| rate_of(d, false))
            .collect();
        //
        // Card HOLD: the shooter now holds a position and strafes on some of
        // its bursts, which moves a shot or two of ~30 (0.531 / 0.556 /
        // 0.472 / 0.444 / 0.250), so a step is allowed one shot's worth.
        for w in rates.windows(2) {
            assert!(
                w[1] <= w[0] + 0.04,
                "hit rate must not grow with range: {rates:?}"
            );
        }
        assert!(rates[4] < rates[0], "and it falls over the whole span");
        // Card SPRD (OURS): a bot's burst stays tight (`TIGHT_HOLD_SECS_AI`),
        // so the far end rose - 0.606 / 0.611 / 0.639 / 0.611 / 0.472; what
        // was left of the fall with range was the aim error alone.
        //
        // Card CLSE: BF2 grows the error vector with the range (d / 20 +
        // 0.5), so against a HUMAN's unit the rate is 0.771 / 0.722 / 0.556 /
        // 0.333 / 0.167, and against a bot (0.3 of the vector) 0.914 / 0.917
        // / 0.861 / 0.861 / 0.750 - a bot hits a bot at arm's length
        // (`analysis/re-soldier-ai.md` 17: 0.64 of a PS2 rifle's shots
        // inside 10 m take health off its target, the port's 0.20 before).
        // Card TPOS: with no strafe on a burst the shooter stands still:
        // 0.892 at 8 m.
        assert!(rates[1] > 0.6 && rates[1] < 0.95, "8 m rate {}", rates[1]);
        assert!(rates[4] < 0.35, "40 m rate {}", rates[4]);
        for (b, h) in bots.iter().zip(&rates) {
            assert!(
                *b + 0.04 >= *h,
                "a bot is hit no less than a human: {bots:?} {rates:?}"
            );
        }
        assert!(bots[1] > 0.85, "8 m rate at a bot {}", bots[1]);
        assert!(bots[4] <= bots[0] + 0.04, "and no better far off: {bots:?}");
    }

    /// Card CLSE4 (`analysis/re-soldier-ai.md` 20.3): the burst and the pause
    /// are the weapon class's Barrage keys under each game's own rule. With
    /// the class defaults 3 / 6 / 2.0 s a BF2 bot fires 3 at the lowest
    /// level and 3..5 at the highest and waits (1.5 − 0.8·t) × 2.0 s; a BF1
    /// bot fires 3..6 at any level and waits 2.0 s. A class that writes the
    /// keys (BF2's flyer chaingun: 10 / 20 / 0.5) is fired by its own.
    #[test]
    fn the_burst_and_pause_are_the_weapon_classes_under_each_games_rule() {
        use crate::skill::Skill;
        use crate::soldier::GroundRules::{Bf1, Bf2};
        let me = UnitHandle { index: 3, gen: 1 };
        let rifle = crate::weapon::WeaponClass::default();
        assert_eq!((rifle.barrage, rifle.barrage_delay), ([3, 6], 2.0));
        let lens = |rules, wc: &crate::weapon::WeaponClass, skill: &Skill| {
            let mut seen = std::collections::BTreeSet::new();
            for t in 0..2048 {
                seen.insert(burst_len(rules, wc, me, t, skill));
            }
            seen.into_iter().collect::<Vec<u8>>()
        };
        let (s1, s20) = (Skill::from_level(1), Skill::from_level(20));
        assert_eq!(lens(Bf2, &rifle, &s1), [3], "BF2, the lowest level");
        assert_eq!(lens(Bf2, &rifle, &s20), [3, 4, 5], "BF2, the highest");
        assert_eq!(
            lens(Bf1, &rifle, &s1),
            [3, 4, 5, 6],
            "BF1 has no skill term"
        );
        assert_eq!(lens(Bf1, &rifle, &s20), [3, 4, 5, 6]);
        // The pause: 1.5 and 0.7 x 2.0 s x 30 Hz, and BF1's flat 2.0 s.
        assert_eq!(pause_ticks(Bf2, &rifle, &s1), 90);
        assert_eq!(pause_ticks(Bf2, &rifle, &s20), 42);
        assert_eq!(pause_ticks(Bf1, &rifle, &s1), 60);
        assert_eq!(pause_ticks(Bf1, &rifle, &s20), 60);
        // A class's own keys.
        let chaingun = crate::weapon::WeaponClass {
            barrage: [10, 20],
            barrage_delay: 0.5,
            ..crate::weapon::WeaponClass::default()
        };
        assert_eq!(lens(Bf2, &chaingun, &s1), [10]);
        assert_eq!(
            lens(Bf2, &chaingun, &s20),
            (10..20).collect::<Vec<u8>>(),
            "the maximum is never drawn"
        );
        assert_eq!(lens(Bf1, &chaingun, &s1), (10..=20).collect::<Vec<u8>>());
        assert_eq!(pause_ticks(Bf2, &chaingun, &s20), 11); // 0.7 × 0.5 × 30 = 10.5
        assert_eq!(pause_ticks(Bf1, &chaingun, &s20), 15);
        // BF2's floor of 2, whatever a class writes.
        let one = crate::weapon::WeaponClass {
            barrage: [1, 1],
            ..crate::weapon::WeaponClass::default()
        };
        assert_eq!(lens(Bf2, &one, &s20), [2]);
        assert_eq!(lens(Bf1, &one, &s20), [1]);
    }

    /// Card CLSE4 (`analysis/re-soldier-ai.md` 20.2): the shots of a standing
    /// bot at a standing enemy `d` metres off over 20 s, its weapon's
    /// `MaxRange` set to `max`; and the least and the most the two were
    /// apart (the shooter shuffles on its spot).
    fn shots_at(rules: crate::soldier::GroundRules, d: f32, max: f32) -> (usize, f32, f32) {
        let _style = crate::ai::tests::all_stand();
        use crate::sim::{Event, Sim};
        use crate::unit::{Controller, UnitSpec};
        let mut s = Sim::empty(5);
        s.ground_rules = rules;
        s.cheats.infinite_ammo = true;
        s.weapon_classes[0].ai_range = [0.0, max * 0.5, max];
        let shooter = s
            .spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(0, Vec3::ZERO)
            })
            .unwrap();
        let t = s
            .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -d)))
            .unwrap();
        let (mut shots, mut near, mut far) = (0, f32::MAX, 0.0f32);
        for _ in 0..600 {
            s.step(&[PadState::default()]);
            s.unit_mut(t).unwrap().health.cur = 300.0;
            let apart = s.unit(shooter).unwrap().soldier.pos - s.unit(t).unwrap().soldier.pos;
            near = near.min(apart.length());
            far = far.max(apart.length());
            shots += s
                .events
                .iter()
                .filter(|e| matches!(e, Event::Shot { unit, .. } if *unit == shooter))
                .count();
        }
        (shots, near, far)
    }

    /// Card CLSE4: a bot fires a target only strictly inside its weapon's
    /// `MaxRange` - a pistol's 24 m: at 23 m it fires, at 25 m it does not,
    /// in both games; the same 25 m under a rifle's 64 m is fired at (the
    /// bolt reaches either way).
    #[test]
    fn a_bot_fires_only_inside_its_weapons_max_range() {
        for rules in [
            crate::soldier::GroundRules::Bf2,
            crate::soldier::GroundRules::Bf1,
        ] {
            let (shots, _, far) = shots_at(rules, 23.0, 24.0);
            assert!(
                shots > 10 && far < 23.5,
                "{rules:?}: 23 m of 24: {shots} {far}"
            );
            let (shots, near, _) = shots_at(rules, 25.0, 24.0);
            assert!(near > 24.5, "{rules:?}: it stays beyond the range: {near}");
            assert_eq!(shots, 0, "{rules:?}: 25 m of 24");
            let (shots, _, _) = shots_at(rules, 25.0, 64.0);
            assert!(shots > 10, "{rules:?}: 25 m of 64: {shots}");
        }
    }

    /// Card STTAP's fixture: a bot with a single-shot weapon 8 m from a
    /// standing target for `ticks`. Returns the ticks it fired on and every
    /// value its weapon's trigger byte took.
    fn tap_run(
        rules: crate::soldier::GroundRules,
        skill: u8,
        ticks: u32,
        class: fn(&mut crate::weapon::WeaponClass),
    ) -> (Vec<u64>, Vec<i32>) {
        use crate::sim::{Event, Sim};
        use crate::unit::{Controller, UnitSpec};
        let mut s = Sim::empty(5);
        s.ground_rules = rules;
        s.cheats.infinite_ammo = true;
        s.weapon_classes[0].trigger_single = true;
        class(&mut s.weapon_classes[0]);
        let shooter = s
            .spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(0, Vec3::ZERO)
            })
            .unwrap();
        s.unit_mut(shooter).unwrap().skill = skill;
        let t = s
            .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -8.0)))
            .unwrap();
        let (mut shots, mut cmds) = (Vec::new(), Vec::new());
        for _ in 0..ticks {
            s.step(&[PadState::default()]);
            s.unit_mut(t).unwrap().health.cur = 300.0;
            cmds.push(s.unit(shooter).unwrap().weapon.fire_cmd);
            if s.events
                .iter()
                .any(|e| matches!(e, Event::Shot { unit, .. } if *unit == shooter))
            {
                shots.push(s.tick);
            }
        }
        (shots, cmds)
    }

    /// Card STTAP: a BF2 bot TAPS a single-shot weapon (`FUN_002baf70`) — the
    /// trigger is down one tick at a time and comes back after `U(0.2, 2.0) ×
    /// tap_x` s, so the weapon keeps firing where a held trigger fired once.
    #[test]
    fn a_bf2_bot_taps_a_single_shot_weapon() {
        // Card FGHT: a shooter that keeps its place.
        let _style = crate::ai::tests::all_stand();
        use crate::soldier::GroundRules::Bf2;
        // A rocket launcher's pace (one shot a second), the worst skill: the
        // re-arm is 0.18–1.8 s.
        let (shots, cmds) = tap_run(Bf2, 1, 300, |c| c.shot_delay = [1.0, 1.0]);
        assert!(shots.len() >= 5, "skill 1 fired {} in 10 s", shots.len());
        assert!(cmds.iter().all(|&c| c <= 1), "a press lasts one tick");
        // The best skill re-arms in 0.02–0.2 s: a press every 2–8 ticks, and
        // a weapon that is ready every 6 fires on most of them.
        let (shots, cmds) = tap_run(Bf2, 20, 300, |_| {});
        assert!(cmds.iter().all(|&c| c <= 1), "a press lasts one tick");
        let presses: Vec<usize> = (0..cmds.len()).filter(|&i| cmds[i] == 1).collect();
        let gaps: Vec<usize> = presses.windows(2).map(|w| w[1] - w[0]).collect();
        let inner = &gaps[1..];
        assert!(
            inner.iter().all(|g| (2..=8).contains(g)),
            "press gaps {gaps:?}"
        );
        assert!(shots.len() >= 25, "skill 20 fired {}", shots.len());
    }

    /// Card STTAP: a BF2 sniper (TriggerSingle and ZoomMax ≥ 6) presses once
    /// per `U(2, 3)` s (`FUN_002baa38`).
    #[test]
    fn a_bf2_sniper_fires_once_per_two_to_three_seconds() {
        // Card FGHT: a shooter that keeps its place.
        let _style = crate::ai::tests::all_stand();
        use crate::soldier::GroundRules::Bf2;
        let (shots, _) = tap_run(Bf2, 20, 900, |c| c.zoom_max = 8.0);
        let gaps: Vec<u64> = shots.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(shots.len() >= 9, "fired {}", shots.len());
        assert!(gaps.iter().all(|g| (60..=90).contains(g)), "gaps {gaps:?}");
        assert!(gaps.iter().any(|&g| g > 62) && gaps.iter().any(|&g| g < 88));
        // ZoomMax 5.99 is not a sniper: it taps.
        let (shots, _) = tap_run(Bf2, 20, 900, |c| c.zoom_max = 5.99);
        assert!(shots.len() > 60, "fired {}", shots.len());
    }

    /// Card STTAP: a BF1 bot's trigger byte runs 1 → 2 → −1 → −2 → 1
    /// (`FUN_0023cc20`), one fresh press every fourth tick; a BF1 sniper also
    /// waits `FUN_0023c698`'s `U(2, 3)` s.
    #[test]
    fn a_bf1_bot_presses_a_single_shot_weapon_every_fourth_tick() {
        // Card FGHT: a shooter that keeps its place.
        let _style = crate::ai::tests::all_stand();
        use crate::soldier::GroundRules::Bf1;
        let (shots, cmds) = tap_run(Bf1, 20, 300, |c| c.shot_delay = [0.1, 0.1]);
        // The weapon's own byte reads the pad: down, down, up, up.
        let first = cmds.iter().position(|&c| c == 1).expect("a press");
        for (i, w) in cmds[first..].chunks_exact(4).take(40).enumerate() {
            assert_eq!(w, [1, 2, -1, 0], "cycle {i}");
        }
        // Every shot is a fresh press, so none comes sooner than the fourth
        // tick; a tick off the aim tolerance may stretch a gap.
        let gaps: Vec<u64> = shots.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.iter().all(|&g| g >= 4), "gaps {gaps:?}");
        let on_beat = gaps.iter().filter(|&&g| g == 4).count();
        assert!(on_beat * 10 >= gaps.len() * 9, "gaps {gaps:?}");
        assert!(shots.len() >= 60, "fired {}", shots.len());

        let (shots, cmds) = tap_run(Bf1, 20, 900, |c| c.zoom_max = 6.0);
        let gaps: Vec<u64> = shots.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(shots.len() >= 9, "fired {}", shots.len());
        assert!(gaps.iter().all(|g| (60..=92).contains(g)), "gaps {gaps:?}");
        assert!(cmds.iter().all(|&c| c <= 1), "a sniper's press is one tick");
    }

    /// D11: a bot fires in BURSTS separated by the skill's pause, where
    /// before it held the trigger down for as long as it was aligned.
    /// Card FFLN: a bot does not fire through a friend. One bot, its enemy
    /// 14 m ahead, and a friend standing on the line between them or 4 m off
    /// it. On the line: no shot leaves while the friend is in the cone, no
    /// bolt of the bot's lands on the friend, the waiting is counted, and the
    /// bot does not stay mute — it steps out of the line and fires (a bot
    /// with no Tactician's position, as here: card TPOS keeps the step for
    /// them). Off the line: nothing is withheld.
    #[test]
    fn a_bot_does_not_fire_through_a_friend() {
        // Card FGHT: a shooter that keeps its place.
        let _style = crate::ai::tests::all_stand();
        use crate::sim::{Event, Sim};
        use crate::unit::{Controller, UnitSpec};
        let run = |friend_x: f32| {
            let mut s = Sim::empty(5);
            s.cheats.infinite_ammo = true;
            let shooter = s
                .spawn(&UnitSpec {
                    controller: Controller::Ai,
                    ..UnitSpec::trooper(0, Vec3::ZERO)
                })
                .unwrap();
            let enemy = s
                .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -14.0)))
                .unwrap();
            let friend = s
                .spawn(&UnitSpec::trooper(0, Vec3::new(friend_x, 0.0, -5.0)))
                .unwrap();
            let (mut shots, mut friend_hits, mut through) = (0u32, 0u32, 0u32);
            for _ in 0..900 {
                // What the bot's think is about to see.
                let u = s.unit(shooter).unwrap();
                let wc = &s.weapon_classes[u.weapon_class];
                // A salvo the refire branch has already started leaves at
                // the next update whatever the trigger does (weapon.rs: only
                // holstering cancels it, as on the PS2) - not a shot the
                // think let go, so it is not counted (card SPRD: the bot's
                // sweeping aim met the friend on such a tick).
                let queued = u.weapon.salvo_counter > 0;
                let blocked = !queued
                    && friend_in_line(
                        &s,
                        shooter,
                        u,
                        u.muzzle(),
                        14.0,
                        LINE_SPREAD + u.weapon.yaw_spread(wc).max(u.weapon.pitch_spread(wc)),
                        None,
                    )
                    .is_some();
                s.step(&[PadState::default()]);
                s.unit_mut(enemy).unwrap().health.cur = 300.0;
                for e in &s.events {
                    match *e {
                        Event::Shot { unit, .. } if unit == shooter => {
                            shots += 1;
                            through += u32::from(blocked);
                        }
                        Event::Hit {
                            shooter: a, victim, ..
                        } if a == shooter && victim == friend => friend_hits += 1,
                        _ => {}
                    }
                }
            }
            (shots, friend_hits, through, s.nav_stats.line_held)
        };
        let (shots, friend_hits, through, held) = run(0.0);
        assert_eq!(friend_hits, 0, "bolts into the friend");
        assert_eq!(through, 0, "shots that left with the friend in the cone");
        assert!(held > 0, "the wait is counted");
        assert!(shots > 20, "stepped aside and fired: {shots} shots");
        let (shots, friend_hits, _, held) = run(4.0);
        assert_eq!((friend_hits, held), (0, 0), "a friend off the line");
        assert!(shots > 20, "{shots} shots");
    }

    /// Card HVYW (OURS): a missile's line of fire is cleared by what the
    /// shooter walks in the wind-up and what the friend walks until the
    /// rocket is where he is. A friend 2 m off the line 5 m ahead: standing,
    /// he is in nobody's line; walking at 6 m/s he is in a rocket's (a
    /// quarter-second wind-up, 100 m/s: 0.75 + 6 x 0.3 = 2.55 m) and in no
    /// rifle's; and a shooter who walks at 6 m/s has him in the rocket's
    /// (0.75 + 6 x 0.25 = 2.25 m). 30 m out the flight counts: 3.5 m off
    /// and walking he is in it (0.75 + 6 x 0.55 = 4.05 m).
    #[test]
    fn a_missiles_line_is_cleared_by_what_is_walked_before_it_gets_there() {
        use crate::sim::Sim;
        use crate::unit::UnitSpec;
        let rocket = Some((0.25, 100.0));
        let ask = |at: Vec3, friend_v: f32, own_v: f32, late| {
            let mut s = Sim::empty(5);
            let shooter = s.spawn(&UnitSpec::trooper(0, Vec3::ZERO)).unwrap();
            let friend = s.spawn(&UnitSpec::trooper(0, at)).unwrap();
            s.units[friend.index as usize].soldier.vel = Vec3::X * friend_v;
            s.units[shooter.index as usize].soldier.vel = Vec3::X * own_v;
            let u = s.unit(shooter).unwrap();
            friend_in_line(&s, shooter, u, u.muzzle(), 40.0, 0.0, late)
        };
        let near = Vec3::new(2.0, 0.0, -5.0);
        assert_eq!(ask(near, 0.0, 0.0, None), None);
        assert_eq!(ask(near, 0.0, 0.0, rocket), None, "a standing friend");
        assert_eq!(ask(near, 6.0, 0.0, None), None, "a rifle's line");
        assert_eq!(ask(near, 6.0, 0.0, rocket), Some(-1), "he walks");
        assert_eq!(ask(near, 0.0, 6.0, rocket), Some(-1), "the shooter walks");
        // Just outside each: 2.6 m off for the friend's walk (2.55), 2.3 m
        // for the shooter's (2.25).
        assert_eq!(ask(Vec3::new(2.6, 0.0, -5.0), 6.0, 0.0, rocket), None);
        assert_eq!(ask(Vec3::new(2.3, 0.0, -5.0), 0.0, 6.0, rocket), None);
        // The flight: 30 m out, 3.5 m off (0.75 + 6 x 0.55 = 4.05), and not
        // with the wind-up alone (0.75 + 1.5 = 2.25).
        let far = Vec3::new(3.5, 0.0, -30.0);
        assert_eq!(ask(far, 6.0, 0.0, rocket), Some(-1));
        assert_eq!(ask(far, 6.0, 0.0, Some((0.25, 1e6))), None);
        assert_eq!(ask(far, 6.0, 0.0, None), None);
    }

    /// Card RKEX (OURS): a weapon whose ordnance explodes (Damage radius 4)
    /// holds its shot while a friend on foot stands within 4 + 1 m of the
    /// target - 3 m off it he is, 5.5 m off he is not, nor is an enemy
    /// there; a rifle (no explosion) never asks.
    #[test]
    fn an_exploding_shot_waits_for_a_friend_beside_its_target() {
        use crate::sim::Sim;
        use crate::unit::UnitSpec;
        use crate::weapon::ExplosionClass;
        let target = Vec3::new(0.0, 1.0, -20.0);
        let ask = |at: Vec3, team: u8, explodes: bool| {
            let mut s = Sim::empty(5);
            let shooter = s.spawn(&UnitSpec::trooper(0, Vec3::ZERO)).unwrap();
            let f = s.spawn(&UnitSpec::trooper(team, at)).unwrap();
            // No ground in `Sim::empty`: stand him at y 0.
            s.units[f.index as usize].soldier.pos = at;
            let mut wc = s.weapon_classes[0].clone();
            if explodes {
                wc.ordnance.explosion = Some(ExplosionClass {
                    damage: 400.0,
                    damage_inner: 2.0,
                    damage_outer: 4.0,
                    ..ExplosionClass::default()
                });
            }
            let u = s.unit(shooter).unwrap();
            friend_in_blast(&s, shooter, u, target, &wc)
        };
        let beside = Vec3::new(3.0, 0.0, -20.0);
        assert_eq!(ask(beside, 0, true), Some(-1), "a friend beside it");
        assert_eq!(
            ask(Vec3::new(-3.0, 0.0, -20.0), 0, true),
            Some(1),
            "the other side"
        );
        assert_eq!(ask(Vec3::new(5.5, 0.0, -20.0), 0, true), None, "outside");
        assert_eq!(ask(beside, 1, true), None, "an enemy");
        assert_eq!(ask(beside, 0, false), None, "a rifle");
    }

    /// Card TEAM (OURS): any shot's line is cleared by one tick of walking
    /// (the sim fires after it moves the units) and by its flight. A friend
    /// 1.5 m ahead and 0.9 m off the line is in no standing rifleman's line
    /// (0.75 m); walking at 6 m/s he is (0.75 + 6 x (1/30 + 1.5/v)), and so
    /// he is to a shooter walking at 6 m/s (0.75 + 6/30); 1.2 m off he is in
    /// neither.
    #[test]
    fn a_shots_line_is_cleared_by_a_ticks_walk_and_its_flight() {
        use crate::sim::Sim;
        use crate::unit::UnitSpec;
        let ask = |at: Vec3, friend_v: f32, own_v: f32| {
            let mut s = Sim::empty(5);
            let shooter = s.spawn(&UnitSpec::trooper(0, Vec3::ZERO)).unwrap();
            let friend = s.spawn(&UnitSpec::trooper(0, at)).unwrap();
            s.units[friend.index as usize].soldier.vel = Vec3::X * friend_v;
            s.units[shooter.index as usize].soldier.vel = Vec3::X * own_v;
            let u = s.unit(shooter).unwrap();
            let wc = &s.weapon_classes[u.weapon_class];
            assert_ne!(wc.ordnance.kind, crate::weapon::OrdKind::Missile);
            assert!(wc.ordnance.velocity > 20.0, "a bolt");
            let late = shot_late(wc);
            assert_eq!(late.0, 1.0 / crate::TICK_HZ as f32);
            friend_in_line(&s, shooter, u, u.muzzle(), 40.0, 0.0, Some(late))
        };
        let near = Vec3::new(0.9, 0.0, -1.5);
        assert_eq!(ask(near, 0.0, 0.0), None, "a standing friend");
        assert_eq!(ask(near, 6.0, 0.0), Some(-1), "he walks");
        assert_eq!(ask(near, 0.0, 6.0), Some(-1), "the shooter walks");
        assert_eq!(ask(Vec3::new(1.2, 0.0, -1.5), 6.0, 0.0), None);
        assert_eq!(ask(Vec3::new(1.2, 0.0, -1.5), 0.0, 6.0), None);
    }

    /// The whole trigger, both games: a standing bot's shots at a standing
    /// enemy 8 m off come in bursts of the class's bounds with the class's
    /// pause between them, counted from a burst's last shot (card CLSE4).
    #[test]
    fn the_ai_fires_in_bursts_with_a_pause_between_them() {
        for rules in [
            crate::soldier::GroundRules::Bf2,
            crate::soldier::GroundRules::Bf1,
        ] {
            bursts_with_a_pause(rules);
        }
    }

    /// Card BRSTU (`analysis/re-soldier-ai.md` 23.2): a hand weapon with
    /// unlimited rounds (the repeater's kind: `RoundsPerClip` 0, heat a
    /// shot) is not held to the class's 3-6 shots - a bot on foot fires it
    /// until its heat is a shot short of full, and it never overheats.
    #[test]
    fn an_unlimited_hand_weapon_bursts_on_its_heat() {
        let _style = crate::ai::tests::all_stand();
        use crate::sim::{Event, Sim};
        use crate::unit::{Controller, UnitSpec};
        for rules in [
            crate::soldier::GroundRules::Bf1,
            crate::soldier::GroundRules::Bf2,
        ] {
            let mut s = Sim::empty(5);
            s.ground_rules = rules;
            s.weapon_classes[0] = WeaponClass {
                rounds_per_clip: 0,
                heat_per_shot: 0.1,
                heat_recover_rate: 0.4,
                heat_threshold: 0.5,
                ..WeaponClass::bf1_rifle()
            };
            let shooter = s
                .spawn(&UnitSpec {
                    controller: Controller::Ai,
                    ..UnitSpec::trooper(0, Vec3::ZERO)
                })
                .unwrap();
            let t = s
                .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -8.0)))
                .unwrap();
            let (mut shots, mut peak) = (Vec::new(), 0.0f32);
            for _ in 0..1800 {
                s.step(&[PadState::default()]);
                s.unit_mut(t).unwrap().health.cur = 300.0;
                let u = s.unit(shooter).unwrap();
                assert!(!u.weapon.overheated, "{rules:?}: overheated at {}", s.tick);
                peak = peak.max(u.weapon.heat);
                if s.events
                    .iter()
                    .any(|e| matches!(e, Event::Shot { unit, .. } if *unit == shooter))
                {
                    shots.push(s.tick);
                }
            }
            let mut runs = vec![1u32];
            for w in shots.windows(2) {
                if w[1] - w[0] > 7 {
                    runs.push(1);
                } else {
                    *runs.last_mut().unwrap() += 1;
                }
            }
            assert!(runs.len() >= 4, "{rules:?}: {runs:?}");
            assert!(
                runs[..runs.len() - 1].iter().all(|&n| n > 6),
                "{rules:?}: {runs:?}"
            );
            assert!(peak > 0.85, "{rules:?}: peak heat {peak}");
        }
    }

    fn bursts_with_a_pause(rules: crate::soldier::GroundRules) {
        // Card FGHT: a shooter that keeps its place.
        let _style = crate::ai::tests::all_stand();
        use crate::sim::{Event, Sim};
        use crate::unit::{Controller, UnitSpec};
        let mut s = Sim::empty(5);
        s.ground_rules = rules;
        s.cheats.infinite_ammo = true;
        let shooter = s
            .spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(0, Vec3::ZERO)
            })
            .unwrap();
        let t = s
            .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -8.0)))
            .unwrap();
        let mut shot_ticks = Vec::new();
        for _ in 0..2700 {
            s.step(&[PadState::default()]);
            s.unit_mut(t).unwrap().health.cur = 300.0;
            if s.events
                .iter()
                .any(|e| matches!(e, Event::Shot { unit, .. } if *unit == shooter))
            {
                shot_ticks.push(s.tick);
            }
        }
        assert!(shot_ticks.len() > 20, "fired {} shots", shot_ticks.len());
        // Split the shots into runs at every gap longer than the weapon's own
        // refire period (ShotDelay 0.2 s = 6 ticks).
        let wc = &s.weapon_classes[0];
        let refire = (wc.shot_delay[0] * crate::TICK_HZ as f32).round() as u64;
        let mut runs = vec![1u32];
        for w in shot_ticks.windows(2) {
            if w[1] - w[0] > refire + 1 {
                runs.push(1);
            } else {
                *runs.last_mut().unwrap() += 1;
            }
        }
        // Drop the first and last runs: they are truncated by the window.
        let inner = &runs[1..runs.len() - 1];
        assert!(
            inner.len() >= 10,
            "{rules:?}: only {} whole bursts: {runs:?}",
            inner.len()
        );
        // The bounds are the class's 3 / 6: BF2 never draws the maximum,
        // BF1 does.
        let [min, max] = wc.barrage.map(u32::from);
        let top = match rules {
            crate::soldier::GroundRules::Bf2 => max - 1,
            crate::soldier::GroundRules::Bf1 => max,
        };
        for &n in inner {
            assert!(n <= top, "{rules:?}: burst of {n}: {runs:?}");
        }
        // A run SHORTER than the minimum is legitimate and rare: the counter
        // only counts shots that were actually emitted, so a bot that loses
        // its alignment mid-burst stops early and its remaining shots come
        // after the next gap.
        let short = inner.iter().filter(|&&n| n < min).count();
        assert!(
            short * 10 <= inner.len(),
            "{rules:?}: {short} of {} runs are short: {runs:?}",
            inner.len()
        );
        // The gaps between bursts are the class's pause from the last shot:
        // none shorter, and the usual one no longer than the next tick the
        // weapon and the aim allow.
        let skill = s.difficulty.skill_at(s.unit(shooter).unwrap().skill);
        let pause = pause_ticks(rules, wc, &skill);
        assert!(pause >= 42, "{rules:?}: 2.0 s by the skill: {pause}");
        let gaps: Vec<u64> = shot_ticks.windows(2).map(|w| w[1] - w[0]).collect();
        let mut long: Vec<u64> = gaps.iter().copied().filter(|g| *g > refire + 1).collect();
        long.sort_unstable();
        // (A reload inside a burst is a gap too, and is not a pause.)
        let under = long.iter().filter(|g| **g < pause).count();
        assert!(
            under * 10 <= long.len(),
            "{rules:?}: pauses {long:?} of {pause}"
        );
        let median = long[long.len() / 2];
        assert!(
            median <= pause + refire,
            "{rules:?}: pauses {long:?} of {pause}"
        );
    }

    /// D11's charge weapons: the bot HOLDS the trigger for
    /// `U(0.15, charge_hi) × MaxPressedTime` and the weapon fires on the
    /// RELEASE (`fire_predicate`: `fire_cmd == -1`), so the hold length is
    /// the drawn window and a high-skill bot (charge_hi = t = 1.0) may hold
    /// far longer than a low one (t = 0.158 at skill 4). No roster soldier
    /// carries a charge weapon today — this class is synthetic.
    #[test]
    fn a_charge_weapon_is_held_then_released_inside_the_skill_window() {
        use crate::sim::Sim;
        use crate::unit::{Controller, UnitSpec};
        const MAX_PRESSED: f32 = 1.0;
        // Ticks the pad had held FIRE when the weapon saw the release edge.
        let holds = |skill: u8| {
            let mut s = Sim::empty(5);
            s.cheats.infinite_ammo = true;
            s.weapon_classes[0].max_pressed_time = MAX_PRESSED;
            let shooter = s
                .spawn(&UnitSpec {
                    controller: Controller::Ai,
                    ..UnitSpec::trooper(0, Vec3::ZERO)
                })
                .unwrap();
            s.unit_mut(shooter).unwrap().skill = skill;
            let t = s
                .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -8.0)))
                .unwrap();
            let (mut out, mut held) = (Vec::new(), 0i32);
            for _ in 0..1200 {
                s.step(&[PadState::default()]);
                s.unit_mut(t).unwrap().health.cur = 300.0;
                let cmd = s.unit(shooter).unwrap().weapon.fire_cmd;
                if cmd == -1 {
                    out.push(held);
                }
                held = cmd.max(0);
            }
            assert!(out.len() > 4, "skill {skill}: {} releases", out.len());
            out
        };
        let lo = holds(4);
        let hi = holds(20);
        let ticks = |f: f32| (f * MAX_PRESSED * crate::TICK_HZ as f32).round() as i32;
        let (floor, ceil4) = (ticks(CHARGE_MIN), ticks(0.15789474));
        let ceil20 = ticks(1.0);
        // Skill 4's window is [0.15, 0.158] × 1 s — 5 ticks, near-constant.
        assert!(
            lo.iter().all(|&h| (floor..=ceil4).contains(&h)),
            "skill 4 holds outside [{floor}, {ceil4}]: {lo:?}"
        );
        // Skill 20's is the whole [0.15, 1.0] × 1 s.
        assert!(
            hi.iter().all(|&h| (floor..=ceil20).contains(&h)),
            "skill 20 holds outside [{floor}, {ceil20}]: {hi:?}"
        );
        let mean = |v: &[i32]| v.iter().sum::<i32>() as f32 / v.len() as f32;
        assert!(
            mean(&hi) > mean(&lo) * 2.0,
            "skill 20 must charge far longer: {} vs {}",
            mean(&hi),
            mean(&lo)
        );
        // ... and the spread of the window is what separates them.
        assert!(
            hi.iter().max() > lo.iter().max(),
            "skill 20 never used the top of its window: {hi:?}"
        );
    }

    #[test]
    fn yaw_toward_matches_the_aim_frame() {
        // Facing -Z is yaw 0; facing -X (left) is +90°.
        assert!((yaw_toward(Vec3::NEG_Z)).abs() < 1e-6);
        assert!((yaw_toward(Vec3::NEG_X) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((yaw_toward(Vec3::X) + std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((wrap(4.0) - (4.0 - std::f32::consts::TAU)).abs() < 1e-6);
    }

    /// Card CLSE (`analysis/re-soldier-ai.md` 17.2): BF2's error vector is
    /// 0.3 of itself against a bot and whole against a human, times
    /// d / 20 + 0.5 (at most 10), never below the aim point, and nothing for
    /// a bot that rides with a human (any seat; the target's vehicle too).
    #[test]
    fn bf2_scales_the_error_vector_by_target_and_range() {
        use crate::unit::{Controller, UnitSpec};
        use crate::vehicle::{SeatKind, VehicleKind, VehicleSpec};
        use crate::weapon::HealthType;
        let mut s = crate::sim::Sim::empty(5);
        let bot = |s: &mut crate::sim::Sim, team: u8, z: f32| {
            s.spawn(&UnitSpec {
                controller: Controller::Ai,
                ..UnitSpec::trooper(team, Vec3::new(0.0, 0.0, z))
            })
            .unwrap()
        };
        let (a, t, mate) = (
            bot(&mut s, 0, 0.0),
            bot(&mut s, 1, -20.0),
            bot(&mut s, 0, 4.0),
        );
        let off = Vec3::new(0.6, 0.4, -0.3);
        let close = |a: Vec3, b: Vec3| (a - b).length() < 1e-6;
        let vec = |s: &crate::sim::Sim, off: Vec3, d: f32| {
            error_vector(s, a, s.unit(t).unwrap(), off, d, &WeaponClass::default())
        };
        // Bot against bot: 0.3, and the range factor 0.75 / 1.5 / 2.5 / 10.
        assert!(close(vec(&s, off, 5.0), off * 0.3 * 0.75));
        assert!(close(vec(&s, off, 20.0), off * 0.3 * 1.5));
        assert!(close(vec(&s, off, 40.0), off * 0.3 * 2.5));
        assert!(close(vec(&s, off, 400.0), off * 0.3 * 10.0));
        // Never below the aim point.
        let down = Vec3::new(0.6, -0.4, -0.3);
        assert!(close(
            vec(&s, down, 20.0),
            Vec3::new(0.6, 0.0, -0.3) * 0.3 * 1.5
        ));
        // A human target keeps the vector whole (the range factor stays),
        // and it too is never aimed under.
        s.units[t.index as usize].is_player = true;
        assert!(close(vec(&s, off, 20.0), off * 1.5));
        assert!(close(vec(&s, off, 5.0), off * 0.75));
        assert!(close(vec(&s, down, 20.0), Vec3::new(0.6, 0.0, -0.3) * 1.5));
        s.units[t.index as usize].is_player = false;
        // Card CLSE2: the weapon class's three AIBubble keys - the vector's
        // factor, the range divider and the range factor's cap.
        let keyed = |size: f32, dist: f32, clamp: f32, d: f32| {
            let wc = WeaponClass {
                ai_bubble_size: size,
                ai_bubble_dist: dist,
                ai_bubble_clamp: clamp,
                ..WeaponClass::default()
            };
            error_vector(&s, a, s.unit(t).unwrap(), off, d, &wc)
        };
        assert!(close(keyed(1.25, 20.0, 10.0, 20.0), off * 1.25 * 0.3 * 1.5));
        assert!(close(keyed(0.6, 20.0, 10.0, 20.0), off * 0.6 * 0.3 * 1.5));
        assert!(close(keyed(1.0, 100.0, 10.0, 50.0), off * 0.3 * 1.0));
        assert!(close(keyed(1.0, 100.0, 3.0, 400.0), off * 0.3 * 3.0));
        // A bot in a vehicle a human rides aims with no error; alone in it,
        // with its own.
        let v = s.spawn_vehicle(&VehicleSpec {
            kind: VehicleKind::Hover,
            class: 0,
            home_team: None,
            neutral_when_empty: true,
            spawn_pad: None,
            pos: Vec3::new(12.0, 2.0, 0.0),
            yaw: 0.0,
            max_health: 1000.0,
            health_type: HealthType::Vehicle,
            seats: vec![SeatKind::Pilot, SeatKind::Gunner(0)],
        });
        assert!(s.enter_vehicle(a, v, 1));
        assert!(close(vec(&s, off, 20.0), off * 0.3 * 1.5), "alone in it");
        assert!(s.enter_vehicle(mate, v, 0));
        assert!(close(vec(&s, off, 20.0), off * 0.3 * 1.5), "a bot drives");
        s.units[mate.index as usize].is_player = true;
        assert_eq!(vec(&s, off, 20.0), Vec3::ZERO, "a human drives");
        // Every seat is asked, not the first: the bot drives, the human is
        // its gunner.
        s.exit_vehicle(a);
        s.exit_vehicle(mate);
        assert!(s.enter_vehicle(a, v, 0));
        assert!(s.enter_vehicle(mate, v, 1));
        assert_eq!(vec(&s, off, 20.0), Vec3::ZERO, "a human is the gunner");
        s.exit_vehicle(a);
        // The TARGET's vehicle is asked too: a bot that rides with a human
        // is aimed at with the whole vector.
        assert!(close(vec(&s, off, 20.0), off * 0.3 * 1.5));
        let seated = |s: &crate::sim::Sim, d: f32| {
            error_vector(s, a, s.unit(mate).unwrap(), off, d, &WeaponClass::default())
        };
        assert!(close(seated(&s, 20.0), off * 1.5), "mate is the human");
        s.exit_vehicle(mate);
        s.units[mate.index as usize].is_player = false;
        s.units[t.index as usize].is_player = true;
        s.units[t.index as usize].team = 0;
        assert!(s.enter_vehicle(t, v, 0));
        assert!(s.enter_vehicle(mate, v, 1));
        assert!(close(seated(&s, 20.0), off * 1.5), "a bot beside a human");
    }
}
