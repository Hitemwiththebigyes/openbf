//! [`Sim::step`] itself — one fixed tick, in the order `sim.rs`'s module
//! header lists — and `separate_units`, its step 3b. The helpers the other
//! stages call live with the feature they serve (`spawn`, `input`, `combat`,
//! `round`). The movement (unit separation, the A.4 step, the wall pass),
//! stamina-cheat and AI-fire (D9's spread multiplier, the fairness rule)
//! tests live here, and a human's shot (card AIMA, `re-aim-assist.md` §10.2:
//! from the fire point at what the aim ray hits, `Sim::aim_hit`) with its
//! tests.

use super::input::{buttons, PadState};
use super::state::{Call, DeathCause, Event, Sim, LETHAL_LANDING_SPEED};
use crate::ai::{self, AiState};
use crate::collision::{Capsule, AIR_STEP, STEP_HEIGHT};
use crate::projectile::{Bolt, Shooter, TERRAIN_RAY_STEP};
use crate::soldier::{GroundRules, Stance};
use crate::unit::{Controller, UnitHandle};
use crate::weapon::{shot_direction, FireInput};
use crate::{TICK_DT, TICK_HZ};
use glam::Vec3;

impl Sim {
    /// Push overlapping alive units apart in XZ (see step 3b). Deterministic:
    /// pairs visited in index order, a coincident pair separates along +X.
    fn separate_units(&mut self) {
        let n = self.units.len();
        for i in 0..n {
            if !self.units[i].alive || self.units[i].mount.is_some() {
                continue;
            }
            for j in i + 1..n {
                if !self.units[j].alive || self.units[j].mount.is_some() {
                    continue;
                }
                let (pi, pj) = (self.units[i].soldier.pos, self.units[j].soldier.pos);
                let min_d = self.units[i].capsule().radius + self.units[j].capsule().radius;
                let d = Vec3::new(pj.x - pi.x, 0.0, pj.z - pi.z);
                let len = d.length();
                if len >= min_d {
                    continue;
                }
                let axis = if len > 1e-4 { d / len } else { Vec3::X };
                let push = axis * ((min_d - len) * 0.5);
                let ni = pi - push;
                let nj = pj + push;
                // A grounded unit follows the ground under its new footprint
                // only within a STEP of where it stands; a push over a
                // platform edge leaves its y alone so it FALLS next tick
                // (`Soldier::resolve_ground` finds no floor) instead of being
                // teleported straight down — measured on BF2 COR1: sixteen
                // AI spawned on one node pushed each other off the platform
                // and every one went 15 -> -300 in a single tick, grounded,
                // never landing, so no fall could ever kill it. Airborne
                // units keep their y as before.
                for (k, n) in [(i, ni), (j, nj)] {
                    let mut y = n.y;
                    if self.units[k].soldier.grounded {
                        let g = self.ground_under(n, false);
                        if (g - n.y).abs() <= STEP_HEIGHT {
                            y = g;
                        }
                    }
                    self.units[k].soldier.pos = Vec3::new(n.x, y, n.z);
                }
            }
        }
    }

    /// BF2: think each living unit's [`crate::unit::AimBlock`] - a bot's
    /// every 5th tick, phased by its index, a human's every tick (card
    /// CLSE2, `analysis/re-soldier-ai.md` 18.1). The decay and the floor are
    /// the skill of who shoots at the unit: the tier's one base skill here
    /// (OURS: the PS2 keeps a skill per team and moves it by auto-balance);
    /// under a per-team tier (card ARVS) the other side's.
    fn aim_blocks(&mut self) {
        if self.ground_rules != crate::soldier::GroundRules::Bf2 {
            return;
        }
        let skills = [0u8, 1, 2].map(|t| self.enemy_difficulty_of(t).skill());
        let tick = self.tick;
        for (i, u) in self.units.iter_mut().enumerate() {
            let period = if u.is_player {
                1
            } else {
                crate::unit::AimBlock::PERIOD
            };
            if !u.alive || !(tick + i as u64).is_multiple_of(period) {
                continue;
            }
            let draw = |k: u32| {
                let salt = ai::SALT_ERR_AXIS.wrapping_add(k);
                2.0 * ai::unit_unit(i as u32, tick, u.gen, salt) - 1.0
            };
            let jolt = Vec3::new(draw(0), draw(0x51ED_2701), draw(0xA3DA_4E03));
            let skill = &skills[usize::from(u.team.min(2))];
            u.aim.think(
                period as f32 * TICK_DT,
                u.soldier.vel,
                u.consts.max_speed,
                skill.err_decay,
                skill.err_floor,
                jolt,
            );
        }
    }

    /// Advance one fixed tick. `pads` is indexed by PLAYER SLOT: the unit in
    /// slot `s` is driven by `pads[s]`. A slot past the end of the slice gets
    /// `PadState::default()` — a slot without a pad this tick simply idles,
    /// which is not an error (a splitscreen player who has not joined, a net
    /// peer whose packet has not arrived).
    pub fn step(&mut self, pads: &[PadState]) {
        let dt = TICK_DT;
        self.events.clear();
        // The hulls where their vehicles are (card VHULL): a caller may have
        // moved one, or installed a new world, since the last tick.
        self.sync_hulls();

        // 0a. The spawn screen's CLASS choice, before the respawn loop below
        //     can act on it.
        self.spawn_select(pads);

        // 0. Respawns (respawn timer expired, then the team's spawn wave for
        //    a bot - a human skips it, card SSELA; AI additionally capped by the alive-AI cap: BF2 FUN_0034cea8
        //    waits 5 s when NumBots is reached).
        for w in &mut self.waves {
            w.tick(dt);
        }
        for i in 0..self.units.len() {
            let u = &self.units[i];
            if u.alive || u.respawn_at.is_none_or(|t| t > self.tick) {
                continue;
            }
            // A HUMAN waits for no spawn wave (BF2 `FUN_0034cea8` skips the
            // wave tests for a human, `B2:344186-344192`, card SSELA). Behind
            // the spawn screens its timer running out opens the spawn map
            // (when its team still has tickets, `B2:392963`), and the class
            // accept is what spawns it (`Sim::spawn_screen_input`).
            if u.is_player {
                match u.spec.player_slot.map(usize::from) {
                    Some(s) if self.spawn_screens => {
                        let team = u.team;
                        if self.spawn_stage.get(s) == Some(&super::spawn::SpawnStage::Accepted)
                            && self.battle.teams[usize::from(team.min(1))].tickets() > 0
                        {
                            self.open_spawn_map(s, team);
                        }
                    }
                    _ => self.respawn_slot(i),
                }
                continue;
            }
            // A bot asks for its post when the timer runs out and keeps the
            // answer through the waits below (card BSPN).
            self.keep_or_draw_spawn_post(i);
            let u = &self.units[i];
            if let Some(slot) = u.spawn_slot {
                if !self.waves[usize::from(u.team).min(1)].ready(slot) {
                    continue;
                }
            }
            {
                let team = u.team;
                let alive_ai = self
                    .units
                    .iter()
                    .filter(|o| o.alive && !o.is_player && o.team == team)
                    .count() as u32;
                if alive_ai >= self.ai_alive_cap {
                    self.units[i].respawn_at = Some(self.tick + 5 * u64::from(TICK_HZ));
                    continue;
                }
            }
            self.respawn_slot(i);
        }

        // 0b. SQUAD ORDERS (`squad`): one edge per player slot in slot order,
        //     then each unit whose think is due takes the order it was
        //     given. Before the commander, so a bot that just left the pool
        //     is not given an objective this same tick.
        self.squad_pass(pads);

        // Card HITCH: the AI's wall time (the frame recorder's; no sim input).
        let ai_t0 = std::time::Instant::now();
        // 1. The COMMANDER (Phase B), before anyone thinks, so the think loop
        //    below reads the plan made this same tick. One team per tick at
        //    most: team t plans when the tick reaches its `next_plan_tick`,
        //    which starts at t and advances by `PLAN_PERIOD`.
        for t in 0..2u8 {
            if self.tick >= self.commander[usize::from(t)].next_plan_tick {
                crate::commander::plan(self, t);
            }
        }
        // 1a'. Card TURR: the turret offers (`ai::man`), after the plan and
        //      before the thinks, so a bot handed one walks to it this tick.
        if !self.vehicles.is_empty() {
            crate::ai::man::offer_pass(self);
        }

        // 1b. Controllers → per-unit pads (AI think reads the sim, writes back
        //    its scratch state afterwards).
        let mut unit_pads: Vec<PadState> = self
            .units
            .iter()
            .map(|u| match u.controller {
                Controller::Player { slot } => {
                    pads.get(usize::from(slot)).copied().unwrap_or_default()
                }
                _ => PadState::default(),
            })
            .collect();
        // 1a. Every BF2 unit's own aim-error block (card CLSE2), before the
        //     think that reads it.
        self.aim_blocks();
        // 1a''. Card TEAM: the team board's roles, before the thinks.
        crate::ai::team::board(self);
        let mut ai_updates: Vec<(usize, AiState, PadState, ai::NavTick)> = Vec::new();
        // Card TEAM: the unit grid answers "who is near" for the thinks, which
        // read a sim no one moves until the write-back.
        self.unit_grid.build(&self.units);
        for (i, u) in self.units.iter().enumerate() {
            if u.alive && u.controller == Controller::Ai {
                let mut st = u.ai;
                let (p, nt) = ai::think(self, self.handle(i), &mut st);
                ai_updates.push((i, st, p, nt));
            }
        }
        self.unit_grid.clear();
        self.ai_wall_ms = ai_t0.elapsed().as_secs_f32() * 1000.0;
        let mut recycled: Vec<(usize, DeathCause)> = Vec::new();
        // Card TPOS: positions taken this tick, granted in unit-index order.
        let mut granted: Vec<glam::Vec3> = Vec::new();
        for (i, mut st, p, nt) in ai_updates {
            ai::tactic::grant(&self.positions, self.tick, &mut st, &mut granted);
            self.nav_stats.cover_ticks += u64::from(st.role.post.is_some());
            self.nav_stats.flank_ticks += u64::from(st.flank.route.is_some());
            self.nav_stats.overwatch_ticks += u64::from(st.behaviour == ai::Behaviour::Overwatch);
            self.units[i].ai = st;
            // Card BNDS: a bot off the level with no way back is given up
            // here, in the write-back's fixed order — and (card RCYC) one
            // on it that made no net progress.
            if nt.recycle != 0 {
                recycled.push((i, DeathCause::Lost));
            } else if nt.stalled != 0 {
                recycled.push((i, DeathCause::Stalled));
                self.nav_stats.flank_stalled += u64::from(self.units[i].ai.flank.route.is_some());
            }
            if self.units[i].mount.is_some() {
                self.nav_stats.ai_drive_ticks += 1;
            }
            unit_pads[i] = p;
            self.nav_stats.bfs_fallbacks += u64::from(nt.bfs);
            self.nav_stats.straight_fallbacks += u64::from(nt.straight);
            self.nav_stats.flank_legs += u64::from(nt.flank);
            self.nav_stats.stuck_max = self.nav_stats.stuck_max.max(nt.stuck_run);
            self.nav_stats.detoured_legs += u64::from(nt.detoured);
            self.nav_stats.blocked_ticks += u64::from(nt.blocked);
            self.nav_stats.level_rejected += u64::from(nt.level_rejected);
            self.nav_stats.level_unrouted += u64::from(nt.level_unrouted);
            self.nav_stats.sweep_hits += u64::from(nt.sweep_hits);
            self.nav_stats.unroutable_objectives += u64::from(nt.unroutable);
            self.nav_stats.unreachable_dwell_max =
                self.nav_stats.unreachable_dwell_max.max(nt.dwell);
            self.nav_stats.contacts_max = self.nav_stats.contacts_max.max(nt.contacts);
            self.nav_stats.heard_contacts += u64::from(nt.heard);
            self.nav_stats.told_contacts += u64::from(nt.told);
            self.nav_stats.line_held += u64::from(nt.line_held);
            self.nav_stats.tpos_queries += u64::from(nt.tpos_query != 0);
            self.nav_stats.tpos_ceiling += u64::from(nt.tpos_query == 2);
            for (b, n) in self.nav_stats.tpos_ev.iter_mut().take(16).enumerate() {
                *n += u64::from(nt.tpos_ev >> b & 1);
            }
            for (n, f) in self.nav_stats.tpos_funnel.iter_mut().zip(nt.tpos_funnel) {
                *n += u64::from(f);
            }
            self.nav_stats.tpos_ev[16] += u64::from(
                nt.tpos_query == 2 && nt.tpos_ev & crate::ai::tactic::EV_PICK_COVERED == 0,
            );
            // Card UCALL: the think's combat calls; on BF2 only the
            // tracker's take, BF2's AcquiredTarget line (card UVO2,
            // `FUN_002cde30` B2:255580).
            if nt.acquired != 0 {
                self.events.push(Event::Call {
                    unit: self.handle(i),
                    call: Call::Acquired,
                });
            }
            if self.ground_rules != crate::soldier::GroundRules::Bf2 {
                let unit = self.handle(i);
                if nt.heard_turn != 0 {
                    self.events.push(Event::Call {
                        unit,
                        call: Call::Heard,
                    });
                }
                // Card UTALK: the trooper's "follow me" (issued with chance
                // 0.3) as it sets out, "move out" as it arrives.
                let issued = match nt.trooper {
                    crate::ai::NavTick::SET_OUT => {
                        let draw =
                            crate::ai::unit_unit(unit.index, self.tick, 0, crate::ai::SALT_FOLLOW);
                        (draw < super::state::FOLLOW_CHANCE).then_some(Call::Follow)
                    }
                    crate::ai::NavTick::ARRIVED => Some(Call::MoveOut),
                    _ => None,
                };
                if let Some(call) = issued.filter(|_| self.units[i].mount.is_none()) {
                    self.events.push(Event::Call { unit, call });
                }
            }
            if let Some(n) = self.nav_stats.man_codes.get_mut(usize::from(nt.man)) {
                *n += u64::from(nt.man != 0);
            }
            match nt.claim {
                crate::ai::NavTick::CLAIM_NEW => self.nav_stats.ai_claims += 1,
                crate::ai::NavTick::CLAIM_GIVEUP => self.nav_stats.ai_claim_giveups += 1,
                crate::ai::NavTick::CLAIM_LOST => self.nav_stats.ai_claim_lost += 1,
                _ => {}
            }
            // `think` takes `&Sim`, so a unit whose order is over — a follower
            // or a holder whose leader is gone, or has been beyond LEAVE_DIST
            // for LEAVE_TICKS of thinks — reports it and the ORDER IS ENDED HERE, inside
            // `Sim::step`'s fixed order, like every other write-back, so a
            // release is replay-exact.
            if nt.released != 0 {
                self.units[i].command = None;
                self.units[i].ai.cmd = None;
                self.nav_stats.releases += 1;
            }
        }
        for (i, cause) in recycled {
            let h = self.handle(i);
            self.kill_environment(h, cause);
        }

        // 1c. The use input (VSEAT, `seats`): an ACTION edge enters what the
        //     soldier looks at, or exits. No bot presses it: a bot's enter /
        //     exit is the request its think left (card VAI), served after.
        if !self.vehicles.is_empty() {
            self.use_pass(&unit_pads);
            self.switch_pass(&unit_pads);
            self.ai_use_pass();
        }

        // 2. BF1 aim assist on each player slot's sticks (FUN_0025fea8 via
        //    core::aim_assist) toward LAST tick's magnet, then this tick's
        //    acquisition (FUN_0025e098), in the PS2's order
        //    (`analysis/re-aim-assist.md` §3.1). A slot whose player is dead
        //    keeps its last magnet (nothing re-acquires until it respawns).
        self.sync_slot_tables();
        for slot in 0..self.players.len() {
            let Some(ph) = self.players[slot] else {
                continue;
            };
            if self.unit(ph).is_none_or(|u| u.mount.is_some()) {
                continue;
            }
            let p = unit_pads[ph.index as usize];
            if let Some(m) = self.magnet[slot] {
                unit_pads[ph.index as usize] = self.apply_assist(slot, ph, m, &p, dt);
            }
            self.acquire_magnet(slot, ph, p.pressed(buttons::FIRE));
        }

        // 2b. DOORS (DR2): open or close by who stands in their triggers
        //     now, before anyone moves against their boxes (`crate::door`;
        //     the placement in the tick is PROVISIONAL).
        self.tick_doors(dt);

        // 3. Movement.
        for (i, pad_i) in unit_pads.iter_mut().enumerate() {
            // The contact accumulator is per TICK, so it is cleared for every
            // unit — dead ones included — before anything can set it, and the
            // wall pass below is the only writer.
            self.units[i].wall_push = Vec3::ZERO;
            if !self.units[i].alive || self.units[i].mount.is_some() {
                // Death and a seat end the zoom (`re-zoom.md` §4).
                self.units[i].zoom = None;
            }
            // Card KNOCK (re-knockback §3): a dead unit's body still moves -
            // the death block first, then the soldier's own update with no
            // stick, the wall pass and the floor.
            let corpse = !self.units[i].alive && self.units[i].body;
            if !self.units[i].alive && !corpse {
                continue;
            }
            if corpse {
                self.death_block(i);
            }
            if self.units[i].mount.is_some() {
                self.ride(i, pad_i);
                continue;
            }
            // Where the tick's move starts: the wall pass tests the move, not
            // only its end (card WPSH).
            let from = self.units[i].capsule();
            // Some(was_grounded) when the soldier stepped: his floor contact
            // resolves AFTER the wall pass (card STHOP).
            let mut stepped: Option<bool> = None;
            match self.units[i].controller {
                _ if corpse => {
                    stepped = Some(self.units[i].soldier.grounded);
                    let (consts, rules) = (self.units[i].consts, self.ground_rules);
                    self.units[i]
                        .soldier
                        .step(dt, &PadState::default(), &consts, rules);
                }
                Controller::Pacer { vel, period } => {
                    let next = self.units[i].soldier.pos + vel * dt;
                    let g = self.ground_under(next, false);
                    let reverse =
                        period > 0 && self.tick % u64::from(period) == u64::from(period) - 1;
                    let u = &mut self.units[i];
                    u.soldier.vel = vel;
                    u.soldier.pos = Vec3::new(next.x, g, next.z);
                    if reverse {
                        u.controller = Controller::Pacer { vel: -vel, period };
                    }
                }
                _ => {
                    // PROVISIONAL (BF2 feel): firing interrupts a sprint —
                    // mask SPRINT while FIRE is held so the stance machine
                    // drops to stand and stays there.
                    if pad_i.pressed(buttons::FIRE) {
                        pad_i.buttons &= !buttons::SPRINT;
                    }
                    let stamina_cheat = self.cheats.infinite_stamina;
                    let mut consts = self.units[i].consts;
                    // The zoom press and its cancels, then the turn: zoomed,
                    // the yaw and pitch rates are divided by the weapon's
                    // turn divisor (`re-zoom.md` §4, §6).
                    self.tick_zoom(i, pad_i);
                    if let Some(z) = self.units[i].zoom {
                        consts.max_turn_speed /= z.divisor;
                        consts.max_pitch_speed /= z.divisor;
                    }
                    stepped = Some(self.units[i].soldier.grounded);
                    let rules = self.ground_rules;
                    self.units[i].soldier.step(dt, pad_i, &consts, rules);
                    let u = &mut self.units[i];
                    if stamina_cheat && u.is_player {
                        u.soldier.energy = consts.energy_bar;
                        u.soldier.energy_depleted = false;
                    }
                }
            }
            // 3a. Wall pass (PROVISIONAL, ours — core::collision): push the
            // capsule out of anything it walked into. The push is horizontal,
            // so it never fights the floor contact. EVERY alive unit, player or
            // AI (Phase A design decision 5): the AI follows the authored
            // corridors now, so it no longer needs to walk through buildings
            // to get anywhere.
            //
            // The capsule asked is the STEPPED one (`Capsule::stepped`, A.4) —
            // the very shape the AI's forward probe uses. A kerb lower than
            // STEP_HEIGHT is something a soldier walks up, so the pass must
            // not push him off it: he walks his feet into the riser and the
            // ground query (3a', below) puts him on top, which is what climbing a
            // step IS. A wall whose top is above STEP_HEIGHT is untouched.
            // A LEDGE (the stepped capsule's floor-like contact with something
            // taller than the step) is a wall too: `push_out_soldier`, card JWAL.
            // The pass runs along the tick's move from `from`, in sub-steps of
            // half a radius, and never pushes a soldier to the far side of a
            // mesh face he came at from the near one (`push_out_soldier_from`,
            // card WPSH): a BF2 sprint-jump no longer tunnels a sheet.
            //
            // The push then takes the velocity's INTO-wall part off and keeps
            // the along-wall part. The PS2 does exactly that on a wall contact
            // (`analysis/re-soldier-walls.md` §5.1, PCSX2 BF1 KAS2: −8.50 → 0.00
            // m/s in one frame, and a slide along a curved wall). So a jump
            // pressed against a wall goes straight up, as on the PS2, instead
            // of carrying the run's speed into it.
            //
            // In the AIR (the jump and fall stances, off the floor) this
            // pass's step is `AIR_STEP` (card JARC): the capsule is asked
            // that much lower, so the stepped one's bottom is `AIR_STEP` over
            // the feet. With the whole step a jump mantled a ledge 0.6 m over
            // its apex.
            let s0 = &self.units[i].soldier;
            let in_air = !s0.grounded
                && matches!(
                    s0.stance,
                    crate::soldier::Stance::Jump
                        | crate::soldier::Stance::Fall
                        | crate::soldier::Stance::Thrown
                );
            if (self.units[i].alive || corpse) && !self.collision.is_empty() {
                let drop = if in_air { STEP_HEIGHT - AIR_STEP } else { 0.0 };
                let low = |c: Capsule| Capsule {
                    base: c.base - Vec3::Y * drop,
                    height: c.height + drop,
                    ..c
                };
                let push = self
                    .collision
                    .push_out_soldier_from(&low(from), &low(self.units[i].capsule()));
                self.units[i].soldier.pos += push;
                let len = (push.x * push.x + push.z * push.z).sqrt();
                if len > 0.0 {
                    let (nx, nz) = (push.x / len, push.z / len);
                    let v = &mut self.units[i].soldier.vel;
                    let into = v.x * nx + v.z * nz;
                    if into < 0.0 {
                        v.x -= into * nx;
                        v.z -= into * nz;
                    }
                }
                // RECORDED, not re-applied: the AI reads it next tick as "I am
                // in contact on this side" (`Unit::wall_push`, PLAN "PHASE A.2
                // DESIGN" decision 2). Motion is exactly what it was.
                self.units[i].wall_push = push;
            }
            // 3a'. The floor under the NEW feet, where the wall pass left them
            // (card STHOP, analysis/re-soldier-ground.md §3: the PS2 finds the
            // contact after the move; resolving before the wall pass judged a
            // soldier pressed on an edge railing at his pre-push position,
            // over the pit, and he sank through the edge). The impact speed
            // is read before it resolves. `rising` = a JUMP going up:
            // plane-following gives a walking soldier vel.y > 0 uphill, and
            // probing from the feet then misses the ramp above them.
            if let Some(was_grounded) = stepped {
                if self.units[i].alive || corpse {
                    let s1 = &self.units[i].soldier;
                    let rising = s1.stance == crate::soldier::Stance::Jump && s1.vel.y > 0.0;
                    // The ground query keeps the whole step in the air (a
                    // floor the feet dipped under within the tick's move is
                    // still found), except for a face too steep to stand on
                    // over the air's step: the wall pass has just called it
                    // a wall, and lifting him onto it hangs him there.
                    let (mut fy, mut fnrm) = self.ground_under_n(s1.pos, rising, from.base.y);
                    if in_air
                        && !rising
                        && fnrm.y <= crate::soldier::FLOOR_MIN_NY
                        && fy > s1.pos.y + AIR_STEP
                    {
                        (fy, fnrm) = self.ground_under_falling(s1.pos, from.base.y);
                    }
                    let vy_before = s1.vel.y;
                    let rules = self.ground_rules;
                    self.units[i]
                        .soldier
                        .resolve_ground(dt, fy, fnrm, from.base.y, rules);
                    let landed =
                        self.units[i].alive && !was_grounded && self.units[i].soldier.grounded;
                    // A hard landing UNDER THE MAP kills, cheat or not (card
                    // KFLR, `sim/bounds.rs`); any other one at the lethal
                    // speed is the fall it was.
                    if landed && self.lands_under_the_map(self.units[i].soldier.pos, vy_before) {
                        let h = self.handle(i);
                        self.kill_environment(h, DeathCause::UnderMap);
                    } else if landed && vy_before <= -LETHAL_LANDING_SPEED {
                        let h = self.handle(i);
                        self.kill_environment(h, DeathCause::Fall);
                    }
                }
            }
            if corpse {
                // BF2's get-up ends straight in the dead state (no standing
                // update, §4); a corpse leaves the world after its 4 s.
                if self.ground_rules == GroundRules::Bf2 {
                    self.death_block(i);
                }
                let s = &self.units[i].soldier;
                if s.stance == Stance::Dead && s.time_in_stance > crate::soldier::CORPSE_SECS {
                    self.units[i].body = false;
                }
            }
        }

        // 3a, after the soldiers' wall pass. Hovers (HOVR): drive, springs,
        // contacts, one vehicle at a time in slot order; then the walkers
        // (WALK): input and locomotion, probes, legs, contacts.
        self.clear_driverless();
        self.tick_hovers(dt);
        self.tick_walkers(dt);
        self.sync_hulls();
        self.carry_riders();

        // 3b. Unit-vs-unit separation (PROVISIONAL, no RE: soldiers must not
        // share a footprint). One pass over index-ordered pairs, equal push
        // along the XZ offset, capsule radius each; re-grounded afterwards.
        self.separate_units();

        // 3c. Death regions: the mission's `AddDeathRegion` volumes, tested on
        // the entity ORIGIN — the same point `update_cps` uses
        // (analysis/re-cp-capture.md §4). Units in index order, regions in the
        // mission's call order, first region wins; every region kills the same
        // way, so nothing is compared.
        // The kill floor first (card KFLR): below it nothing is playable.
        if let Some(floor) = self.kill_floor {
            for i in 0..self.units.len() {
                if self.units[i].alive && self.units[i].soldier.pos.y < floor {
                    let h = self.handle(i);
                    self.kill_environment(h, DeathCause::KillFloor);
                }
            }
        }
        if !self.death_regions.is_empty() {
            for i in 0..self.units.len() {
                if !self.units[i].alive {
                    continue;
                }
                let p = self.units[i].soldier.pos;
                if self.death_regions.iter().any(|r| r.contains(p)) {
                    let h = self.handle(i);
                    self.kill_environment(h, DeathCause::DeathRegion);
                }
            }
        }

        // 4. Weapons → bolts.
        let mut shots = Vec::new();
        for (i, &p) in unit_pads.iter().enumerate() {
            if !self.units[i].alive || matches!(self.units[i].controller, Controller::Pacer { .. })
            {
                continue;
            }
            // A rider fires its seat's guns, not its own weapon (card VTUR).
            if self.units[i].mount.is_some() {
                self.seat_gun(i, &p);
                continue;
            }
            let move_mag = PadState::axis(p.ly).abs().max(PadState::axis(p.lx).abs());
            let stance_idx = match self.units[i].soldier.stance {
                Stance::Stand => 0,
                Stance::Crouch => 1,
                Stance::Prone => 2,
                _ => 0,
            };
            // BF1 skips fire processing in the roll states (re-stance-roll
            // §3.4); BF2 rolls believed to block too. A throw, its tumble and
            // get-up and the lock after it hold the trigger off too (card
            // KNOCK, re-knockback §2: the PS2 zeroes the trigger bytes;
            // PROVISIONAL: the port skips the fire processing, so a held
            // charge is neither fired nor lost).
            let blocked =
                self.units[i].soldier.stance == Stance::Roll || self.units[i].soldier.no_fire();
            // A soldier with more slots than the one in its hands (GREN):
            // the channels decide whether FIRE reaches the primary.
            let mut fire_held = if self.units[i].held.is_empty() {
                p.pressed(buttons::FIRE)
            } else {
                self.held_weapons(i, &p, blocked, dt)
            };
            // Card RKSLF (OURS): a bot's exploding shot that this update
            // could release - a salvo latched in the cycle, a held trigger,
            // or a charge weapon's trigger coming up (it fires on the
            // release, `fire_predicate`) - whose blast would come back on its
            // own side is not let go: the salvo is cancelled as the PS2's
            // put-away cancels one, and the trigger stays as it was before
            // the shot (up, or a charge still held).
            if self.units[i].controller == Controller::Ai {
                let w = &self.units[i].weapon;
                let charge = self.weapon_classes[self.units[i].weapon_class].max_pressed_time > 0.0;
                let releasing = w.salvo_counter >= 1
                    || if charge {
                        !fire_held && w.fire_cmd > 0
                    } else {
                        fire_held
                    };
                if releasing && crate::ai::release_blasts_own_side(self, i) {
                    fire_held = charge && self.units[i].weapon.fire_cmd > 0;
                    self.units[i].weapon.salvo_counter = 0;
                    self.nav_stats.blast_holds += 1;
                }
            }
            let input = FireInput {
                held: fire_held,
                blocked,
                infinite_ammo: self.cheats.infinite_ammo && self.units[i].is_player,
                // Phase C D9: the AI sniper distance table's yaw term, chosen
                // by this unit's own `think` this tick from its range to its
                // target (`analysis/re-ai-sniper.md` §2). A player's is
                // always `None` — nothing writes `AiState` for one.
                ai_dispersion: self.units[i].ai.dispersion,
            };
            let wc_idx = self.units[i].weapon_class;
            shots.clear();
            // Phase C D9: the skill's SPREAD multiplier is the PS2's
            // `weapon+0x134`, written by the AI every aim tick (RE §4.2 step
            // 2). Ours is written once per fire tick, which has the same
            // effect — `spread_mult` is only read when a shot adds spread
            // (`weapon.rs`, `spread_per_shot`). A player keeps 1.0.
            let ai_spread_mult = (self.units[i].controller == Controller::Ai).then(|| {
                self.difficulty_of(self.units[i].team)
                    .skill_at(self.units[i].skill)
                    .spread_mult
            });
            let fire_empty;
            let reload_started = {
                let wc = &self.weapon_classes[wc_idx];
                let u = &mut self.units[i];
                if let Some(m) = ai_spread_mult {
                    u.weapon.spread_mult = m;
                }
                u.weapon
                    .set_posture(wc, stance_idx, move_mag, u.zoom.is_some());
                let was_reloading = u.weapon.reloading();
                // What refuses a shot before this update's input.
                let dry = was_reloading || u.weapon.overheated || u.weapon.out_of_ammo(wc);
                if p.pressed(buttons::RELOAD) {
                    u.weapon.start_reload(wc);
                }
                u.weapon.update(dt, wc, input, &mut self.lcg, &mut shots);
                // The empty click: a human's trigger press (the held counter
                // at 1) on a weapon that cannot fire (PS2: the owner's
                // player index >= 0, re-weapon-sounds.md §7.3).
                fire_empty = dry && u.is_player && u.weapon.fire_cmd == 1;
                !was_reloading && u.weapon.reloading()
            };
            if fire_empty {
                let h = self.handle(i);
                self.events.push(Event::FireEmpty { unit: h });
            }
            if reload_started {
                let h = self.handle(i);
                self.events.push(Event::Reload { unit: h });
            }
            if shots.is_empty() {
                continue;
            }
            // Perception's hearing reads this (`perception::scan`): anyone
            // within HEAR_RADIUS of a shot knows a shooter is there, with no
            // line of sight.
            self.units[i].last_shot_tick = self.tick;
            // Design decision 2's fairness rule, asserted where the shot is
            // made: an AI unit may only fire at a contact its own perception
            // holds. The think of this same tick has already written `ai`
            // back, so the record is this tick's.
            let ai_shooter = self.units[i].controller == Controller::Ai;
            let perceived = !ai_shooter || {
                let st = &self.units[i].ai;
                st.target
                    .is_some_and(|t| st.contacts.iter().flatten().any(|c| c.handle == t))
            };
            let shooter = self.handle(i);
            // A human's shot leaves the fire point toward what the reticle's
            // ray hits (card AIMA); a bot's leaves the chest along its aim
            // (card FPNT has the fire point for bots).
            let human = matches!(self.units[i].controller, Controller::Player { .. });
            let (origin, (fwd, right, up)) = if human {
                self.human_shot(i)
            } else {
                (self.units[i].muzzle(), self.units[i].aim_frame())
            };
            let who = Shooter {
                owner: shooter,
                team: self.units[i].team,
                weapon_class: wc_idx,
                is_player: self.units[i].is_player,
            };
            let (speed, life) = {
                let o = &self.weapon_classes[wc_idx].ordnance;
                (o.velocity, o.life_span)
            };
            let targets: Vec<(UnitHandle, Capsule)> = self
                .alive_units()
                .filter(|(h, _)| *h != shooter)
                .map(|(h, u)| (h, u.capsule()))
                .collect();
            for spec in &shots {
                let dir = shot_direction(
                    &mut self.lcg,
                    fwd,
                    right,
                    up,
                    spec.pitch_spread,
                    spec.yaw_spread,
                    (0.0, 0.0),
                );
                let bolt = Bolt::spawn(
                    origin,
                    dir,
                    speed,
                    life,
                    who,
                    self.heightfield.as_ref(),
                    &self.collision,
                    crate::collision::HullFilter::All,
                    targets.iter().copied(),
                );
                if ai_shooter {
                    self.nav_stats.ai_shots += 1;
                    // The rule is about the AI's DECISION, so it is checked
                    // where the decision is: the tick's own FIRE press. A
                    // weapon whose cycle is already running emits its last
                    // bolt after the trigger comes up (`WeaponState::update`
                    // stays in `FireState::Firing` until `cycle_finished`),
                    // and MEASURED over the 32-map sweep that is the only way
                    // a shot ever leaves an AI with no target: 1-7 per map,
                    // every one of them `fire_pressed false, target None,
                    // contacts 0, state Firing` (the probe is in the phase-c2
                    // report). Those are counted apart, not asserted on.
                    if p.pressed(buttons::FIRE) {
                        debug_assert!(
                            perceived,
                            "AI unit {i} fired at an unperceived target at tick {}",
                            self.tick
                        );
                        if !perceived {
                            self.nav_stats.unperceived_shots += 1;
                        }
                    } else {
                        self.nav_stats.carry_shots += 1;
                    }
                }
                self.events.push(Event::Shot {
                    unit: shooter,
                    weapon_class: wc_idx as u32,
                    origin,
                    dir,
                });
                self.bolts.push(bolt);
            }
        }

        // 5. Bolt flight + impacts (in spawn order).
        let mut done = Vec::new();
        for (i, b) in self.bolts.iter_mut().enumerate() {
            if b.advance(dt) {
                done.push(i);
            }
        }
        for &i in &done {
            let b = self.bolts[i].clone();
            self.resolve_bolt(&b);
        }
        self.bolts.retain(|b| b.remaining > 0.0);

        // 5b. The thrown grenades (GREN).
        self.tick_stickies(dt);

        // 5c. The pickups (PWUP): flight, life, the touch.
        self.tick_pickups(dt);

        // 5c'. The health / ammo droids (PWST): each station's clock and pass.
        self.tick_stations(dt);

        // 6. Command posts → bleed → domination → outcome.
        self.update_cps(dt);

        // 7. Vehicle pads: follow the posts just updated, decay, spawn.
        self.tick_vehicle_pads(dt);

        // The grenades that have gone off are gone (card HOLD).
        if !self.hazards.is_empty() {
            let now = self.tick;
            self.hazards.retain(|z| z.until > now);
        }
        // ...and the ones still live warn the bots near them (card UCALL;
        // on BF2 the same rule asks the Grenade line, card UVO2:
        // `FUN_002a89b8` B2:229141 - its delay PROVISIONAL, BF1's).
        self.grenade_calls();
        self.tick += 1;
    }
}

impl Sim {
    /// Card UCALL, BF1's "grenade!" (B1 `FUN_00236be8` 150870-150939,
    /// re-unit-chatter.md 2.3): a live grenade, after the reaction delay
    /// ([`super::state::GRENADE_WARN_DELAY`], drawn per grenade) and then
    /// once a second, lists the units within [`super::HAZARD_RADIUS`]; the
    /// FIRST of them that is an AI soldier says its `FleeSound`. A human's
    /// soldier never does; a rider is its vehicle, not a soldier; the
    /// thrower is not warned of its own (card GRAI's reading of the PS2).
    /// PROVISIONAL [H]: "first" is the lowest unit slot - the PS2's order is
    /// its collision query's.
    fn grenade_calls(&mut self) {
        let (lo, hi) = super::state::GRENADE_WARN_DELAY;
        let hz = crate::TICK_HZ as f32;
        let mut calls = Vec::new();
        for z in &self.hazards {
            let draw =
                crate::ai::unit_unit(z.since as u32, z.since, 0, crate::ai::SALT_GRENADE ^ 2);
            let first = z.since + ((lo + (hi - lo) * draw) * hz).round() as u64;
            if self.tick < first || !(self.tick - first).is_multiple_of(u64::from(crate::TICK_HZ)) {
                continue;
            }
            let speaker = self.units.iter().enumerate().find(|(i, u)| {
                u.alive
                    && u.controller == crate::unit::Controller::Ai
                    && u.mount.is_none()
                    && z.owner != Some(self.handle(*i))
                    && (u.soldier.pos - z.pos).length() < super::HAZARD_RADIUS
            });
            if let Some((i, _)) = speaker {
                calls.push(self.handle(i));
            }
        }
        for unit in calls {
            self.events.push(Event::Call {
                unit,
                call: Call::Flee,
            });
        }
    }
}

/// The aim ray starts this far past the camera's distance behind the eye
/// (`re-aim-assist.md` §10.2 step 5: `lui 0x4000`), and is this long (`lui
/// 0x4270`): what is nearer than the start is not seen at all (the PS2's
/// blind spot, FEEL_NOTES AIMA).
pub const AIM_RAY_START: f32 = 2.0;
pub const AIM_RAY_LENGTH: f32 = 60.0;
/// In the scope view the aim's far point is this far along it, with no ray.
pub const SCOPE_AIM_LENGTH: f32 = 200.0;

impl Sim {
    /// The views drawn: one per player slot (split screen tilts the camera
    /// less, `unit::SPLIT_TILT`).
    pub fn views(&self) -> usize {
        self.players.len().max(1)
    }

    /// H of §10.2 step 5 for unit `i`: the first ordnance-solid thing on its
    /// aim ray between [`AIM_RAY_START`] past the track offset and
    /// [`AIM_RAY_LENGTH`] further - another unit's capsule, a vehicle's hull,
    /// the map's ordnance layer, the terrain - else the ray's far end.
    pub fn aim_hit(&self, i: usize) -> Vec3 {
        let u = &self.units[i];
        let (a, d) = u.aim_ray(self.ground_rules, self.views());
        // The scope view casts no ray (§10.2 step 7): the point 200 m along
        // the aim.
        if u.scoped() {
            return a + d * SCOPE_AIM_LENGTH;
        }
        let sec = u.section(self.ground_rules);
        let start = a + d * (sec.offset.z + AIM_RAY_START);
        let mut best = AIM_RAY_LENGTH;
        // A terrain ray that starts under the ground reports no hit (§10.5
        // item 4 [M]); the port's own ray would find the way back out.
        let under = self.heightfield.as_ref().is_some_and(|h| {
            start.y < h.height_at(start.x, start.z)
                && !crate::collision::is_cut_by(&self.collision.cutters, start)
        });
        if !under {
            if let Some(t) = self.terrain_ray(start, d, best, TERRAIN_RAY_STEP) {
                best = best.min(t);
            }
        }
        let own = crate::collision::HullFilter::except(u.mount.map(|m| m.vehicle));
        let layer = crate::collision::Layers::ORDNANCE;
        if let Some(h) = self.collision.ray_with(start, d, best, layer, own) {
            best = best.min(h.t);
        }
        let me = self.handle(i);
        for (h, o) in self.alive_units() {
            if h != me {
                if let Some(t) = o.capsule().ray_hit(start, d, best) {
                    best = best.min(t);
                }
            }
        }
        start + d * best
    }

    /// The soldier's zoom routine (B1 `FUN_00204578`, B2 `FUN_0023ead8`,
    /// `analysis/re-zoom.md` §4), run before unit `i` moves, so it sees the
    /// stance and the weapon as the last tick left them. Only a human's
    /// soldier zooms. The zoom ends in a roll or a sprint (BF2's rule; BF1
    /// has no sprint and the port's takes it, FEEL_NOTES), on a weapon that
    /// cannot zoom - the secondary's grenade once it is the weapon in hand -
    /// and while the primary reloads; otherwise a press of ZOOM steps it
    /// (`WeaponClass::zoom_press`). Death, a seat and the primary's switch
    /// end it where they happen. Not ported: the continuous zoom of a class
    /// with `ZoomRate` (no hand weapon), BF2's melee test (no disc lightsaber
    /// has a `ZoomMax` over 1) and the thrown state (the port has none).
    fn tick_zoom(&mut self, i: usize, pad: &PadState) {
        let u = &self.units[i];
        let human = matches!(u.controller, Controller::Player { .. });
        let press = pad.pressed(buttons::ZOOM) && u.soldier.prev_buttons() & buttons::ZOOM == 0;
        let moving = matches!(u.soldier.stance, Stance::Roll | Stance::Sprint);
        let (class, reloading) = match u.cur1.filter(|_| u.active_channel == 1) {
            Some(k) => (u.held[usize::from(k)].class, false),
            None => (u.weapon_class, u.weapon.reloading()),
        };
        let wc = &self.weapon_classes[class];
        let step = u.zoom.map(|z| z.step);
        let step = if !human || moving || !wc.zooms() || reloading {
            None
        } else if press {
            wc.zoom_press(step)
        } else {
            step
        };
        let zoom = step.map(|step| {
            let (value, divisor) = wc.zoom_at(step);
            crate::unit::Zoom {
                step,
                value,
                divisor,
                scope: wc.zoom_first_person,
            }
        });
        self.units[i].zoom = zoom;
    }

    /// A human's shot (§10.2 step 6): from the fire point F along
    /// normalize(H - F), with the spread's frame built on that line.
    fn human_shot(&self, i: usize) -> (Vec3, (Vec3, Vec3, Vec3)) {
        let u = &self.units[i];
        // The scope view: from the scope point along the aim itself.
        if u.scoped() {
            return (u.scope_point(self.ground_rules), u.aim_frame());
        }
        let f = u.fire_point();
        let (aim, aim_right, _) = u.aim_frame();
        let fwd = (self.aim_hit(i) - f).normalize_or(aim);
        let right = fwd.cross(Vec3::Y).normalize_or(aim_right);
        (f, (fwd, right, right.cross(fwd).normalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::CollisionWorld;
    use crate::sim::tests::{ai, fire, flat_ground};
    use crate::sim::{Cheats, InputLog};
    use crate::soldier::GroundRules;
    use crate::unit::UnitSpec;
    use crate::weapon::WeaponClass;

    /// The combat calls `unit` asked over `ticks` steps: (tick, call).
    fn calls_of(s: &mut Sim, ticks: u32) -> Vec<(u64, UnitHandle, Call)> {
        let mut out = Vec::new();
        for _ in 0..ticks {
            let tick = s.tick;
            s.step(&[PadState::default()]);
            for e in &s.events {
                if let Event::Call { unit, call } = *e {
                    out.push((tick, unit, call));
                }
            }
        }
        out
    }

    /// Card UCALL, BF1's "grenade!" (re-unit-chatter.md 2.3): a live
    /// grenade warns the units within 7.5 m after its reaction delay and
    /// then once a second, and only the FIRST AI soldier among them says
    /// the line - never the human beside it, never a bot outside the
    /// radius. A BF2-rule sim asks the same (its Grenade line, card UVO2).
    #[test]
    fn a_live_grenade_makes_one_flee_call_a_second_from_the_first_bot() {
        let run = |rules: GroundRules| {
            let mut s = Sim::empty(9);
            s.heightfield = Some(flat_ground());
            s.ground_rules = rules;
            let human = s
                .spawn(&UnitSpec::player(0, Vec3::new(0.5, 0.0, 0.0)))
                .unwrap();
            let first = s.spawn(&ai(0, Vec3::new(3.0, 0.0, 0.0), 0.0)).unwrap();
            let second = s.spawn(&ai(0, Vec3::new(2.0, 0.0, 1.0), 0.0)).unwrap();
            let outside = s.spawn(&ai(0, Vec3::new(9.0, 0.0, 0.0), 0.0)).unwrap();
            s.add_hazard(Vec3::ZERO, 150);
            let calls = calls_of(&mut s, 150);
            let gone = s.unit(first).map(|u| u.soldier.pos.length());
            (calls, [human, first, second, outside], gone)
        };
        let (calls, [human, first, second, outside], gone) = run(GroundRules::Bf1);
        let flee: Vec<_> = calls.iter().filter(|c| c.2 == Call::Flee).collect();
        assert!(flee.len() >= 3, "a call a second for 5 s: {flee:?}");
        // The lowest slot speaks while it is inside; once it has run clear
        // (it does: card HOLD) the next bot inside is the first.
        assert_eq!(flee[0].1, first, "{flee:?}");
        assert!(
            flee.iter().all(|c| c.1 == first || c.1 == second),
            "{flee:?}"
        );
        assert!(flee.iter().all(|c| c.1 != human), "a human never: {flee:?}");
        assert!(
            flee.iter().all(|c| c.1 != outside),
            "outside the radius: {flee:?}"
        );
        if flee.iter().any(|c| c.1 == second) {
            assert!(
                gone.is_some_and(|d| d > crate::sim::HAZARD_RADIUS),
                "{gone:?}"
            );
        }
        let t0 = flee[0].0;
        assert!((18..=39).contains(&t0), "after 0.6-1.3 s: tick {t0}");
        for w in flee.windows(2) {
            assert_eq!(
                w[1].0 - w[0].0,
                u64::from(TICK_HZ),
                "once a second: {flee:?}"
            );
        }
        // BF2: the same rule asks the Grenade line (card UVO2).
        let (calls, [_, first, ..], _) = run(GroundRules::Bf2);
        let flee: Vec<_> = calls.iter().filter(|c| c.2 == Call::Flee).collect();
        assert!(!flee.is_empty() && flee[0].1 == first, "BF2: {calls:?}");
    }

    /// Card UCALL, BF1's "there he is" (re-unit-chatter.md 2.2): a bot whose
    /// tracker takes the enemy in front of it asks `Acquired` at that think,
    /// and not again while it keeps the target. A BF2-rule sim asks the take
    /// alone (card UVO2).
    #[test]
    fn a_bot_that_takes_a_target_asks_its_acquired_call() {
        let run = |rules: GroundRules| {
            let mut s = Sim::empty(3);
            s.heightfield = Some(flat_ground());
            s.ground_rules = rules;
            let bot = s.spawn(&ai(0, Vec3::ZERO, 0.0)).unwrap();
            // Straight ahead (yaw 0 faces -Z), in plain sight.
            let enemy = s
                .spawn(&UnitSpec::player(1, Vec3::new(0.0, 0.0, -15.0)))
                .unwrap();
            let calls = calls_of(&mut s, 60);
            let target = s.unit(bot).and_then(|u| u.ai.target);
            (
                calls.into_iter().filter(|c| c.1 == bot).collect::<Vec<_>>(),
                target,
                enemy,
            )
        };
        let (calls, target, enemy) = run(GroundRules::Bf1);
        assert_eq!(target, Some(enemy), "it took the enemy");
        let acquired: Vec<_> = calls.iter().filter(|c| c.2 == Call::Acquired).collect();
        assert_eq!(acquired.len(), 1, "one take, one call: {calls:?}");
        // BF2: the take alone (its AcquiredTarget line, card UVO2); no
        // other call of BF1's.
        let (calls, ..) = run(GroundRules::Bf2);
        assert!(
            calls.len() == 1 && calls[0].2 == Call::Acquired,
            "BF2: {calls:?}"
        );
    }

    /// Card CLSE2 (`analysis/re-soldier-ai.md` 18.1): a BF2 bot's aim-error
    /// block thinks every 5th tick, phased by its index, and a human's every
    /// tick - both pay the same decay a second.
    #[test]
    fn a_bots_aim_block_thinks_every_fifth_tick_and_a_humans_every_tick() {
        use crate::unit::AimBlock;
        let mut s = Sim::empty(5);
        s.heightfield = Some(flat_ground());
        let p = s
            .spawn(&UnitSpec::player(0, Vec3::new(0.0, 0.0, 0.0)))
            .unwrap();
        let b = s.spawn(&ai(0, Vec3::new(4.0, 0.0, 0.0), 0.0)).unwrap();
        let c = s.spawn(&ai(0, Vec3::new(8.0, 0.0, 0.0), 0.0)).unwrap();
        let mut thinks = [0u32; 3];
        let mut ticks_of = [Vec::new(), Vec::new(), Vec::new()];
        for u in [p, b, c] {
            s.units[u.index as usize].aim.err = 2.0;
        }
        for _ in 0..30 {
            let before: Vec<AimBlock> = [p, b, c].iter().map(|h| s.unit(*h).unwrap().aim).collect();
            let tick = s.tick;
            s.step(&[PadState::default()]);
            for (k, h) in [p, b, c].iter().enumerate() {
                if s.unit(*h).unwrap().aim.dir != before[k].dir {
                    thinks[k] += 1;
                    ticks_of[k].push(tick);
                }
            }
        }
        assert_eq!(thinks, [30, 6, 6], "30 ticks: the human 30 thinks, a bot 6");
        assert!(ticks_of[1]
            .iter()
            .all(|t| (t + u64::from(b.index)) % 5 == 0));
        assert_ne!(ticks_of[1][0], ticks_of[2][0], "phased by the unit's index");
        let decay = s.difficulty.skill().err_decay;
        for u in [p, b, c] {
            let err = s.unit(u).unwrap().aim.err;
            assert!(
                (err - (2.0 - decay)).abs() < 1e-3,
                "a second's decay: {err}"
            );
        }
    }

    /// Flat ground at y 0 with ONE box ahead whose top is `top` metres up:
    /// the kerb / wall the A.4 step tests walk at. It spans z -5..-1, so a
    /// unit at the origin walking -Z reaches it after about a metre.
    fn step_sim(top: f32) -> Sim {
        let mut s = Sim::empty(4);
        s.heightfield = Some(flat_ground());
        s.collision = CollisionWorld::new(
            vec![crate::collision::Body::new(
                glam::Mat4::from_translation(Vec3::new(0.0, top - 2.0, -3.0)),
                crate::collision::Shape::Box {
                    half: Vec3::new(10.0, 2.0, 2.0),
                },
            )],
            Vec::new(),
        );
        s
    }

    #[test]
    fn overlapping_units_are_pushed_apart_deterministically() {
        let mut s = Sim::empty(1);
        s.heightfield = Some(flat_ground());
        let a = s.spawn(&UnitSpec::trooper(0, Vec3::ZERO)).unwrap();
        let b = s
            .spawn(&UnitSpec::trooper(1, Vec3::new(0.3, 0.0, 0.0)))
            .unwrap();
        let far = s
            .spawn(&UnitSpec::trooper(1, Vec3::new(5.0, 0.0, 0.0)))
            .unwrap();
        s.separate_units();
        let (pa, pb) = (
            s.units[a.index as usize].soldier.pos,
            s.units[b.index as usize].soldier.pos,
        );
        let min_d = 2.0 * crate::unit::SOLDIER_RADIUS;
        assert!((pb.x - pa.x - min_d).abs() < 1e-5, "{pa} {pb}");
        assert!(((pa.x + pb.x) * 0.5 - 0.15).abs() < 1e-5, "midpoint kept");
        assert_eq!(
            s.units[far.index as usize].soldier.pos,
            Vec3::new(5.0, 0.0, 0.0)
        );
        // Coincident pair: separates along +X, still deterministic.
        let mut s2 = Sim::empty(1);
        s2.heightfield = Some(flat_ground());
        let c = s2.spawn(&UnitSpec::trooper(0, Vec3::ZERO)).unwrap();
        let d = s2.spawn(&UnitSpec::trooper(1, Vec3::ZERO)).unwrap();
        s2.separate_units();
        assert!(
            s2.units[d.index as usize].soldier.pos.x > s2.units[c.index as usize].soldier.pos.x
        );
    }

    /// A step LOWER than `STEP_HEIGHT` is climbed, not walled off (A.4): the
    /// wall pass asks the stepped capsule, so the riser never touches it, the
    /// unit walks its feet in and the ground query stands it on top. This is
    /// the shape BF2 POL1's ramp pair was stuck on — `steer fwd`, `push
    /// 0.301`, 33,840 ticks, one 0.3 m push in and one 0.3 m push back out
    /// every tick.
    #[test]
    fn a_kerb_below_step_height_is_climbed_not_a_wall() {
        let mut s = step_sim(0.4);
        let h = s
            .spawn(&UnitSpec::player_in_slot(0, 0, Vec3::ZERO))
            .unwrap();
        let fwd = PadState {
            ly: 32767,
            ..PadState::default()
        };
        let mut max_push = 0.0f32;
        let mut stood_on_it = false;
        for _ in 0..60 {
            s.step(&[fwd]);
            let u = s.unit(h).unwrap();
            max_push = max_push.max(u.wall_push.length());
            // Over the box (z -5..-1) and standing on its top.
            if u.soldier.pos.z < -1.0 && u.soldier.pos.z > -5.0 {
                stood_on_it |= (u.soldier.pos.y - 0.4).abs() < 1e-3;
            }
        }
        assert!(stood_on_it, "never stood on the 0.4 m step");
        assert_eq!(max_push, 0.0, "and was never pushed out of it");
        let u = s.unit(h).unwrap();
        assert!(
            u.soldier.pos.z < -5.0,
            "walked clean over it: {:?}",
            u.soldier.pos
        );
    }

    /// ...and a step TALLER than `STEP_HEIGHT` is still a wall: the stepped
    /// capsule hits it, the pass pushes the unit out and records the push.
    #[test]
    fn a_wall_above_step_height_still_pushes() {
        let mut s = step_sim(1.0);
        let h = s
            .spawn(&UnitSpec::player_in_slot(0, 0, Vec3::ZERO))
            .unwrap();
        let fwd = PadState {
            ly: 32767,
            ..PadState::default()
        };
        let mut max_push = 0.0f32;
        for _ in 0..60 {
            s.step(&[fwd]);
            max_push = max_push.max(s.unit(h).unwrap().wall_push.length());
        }
        let u = s.unit(h).unwrap();
        assert!(max_push > 0.0, "pushed out of the 1.0 m wall");
        assert!(
            u.soldier.pos.y.abs() < 1e-3,
            "still on the floor, not on top: {:?}",
            u.soldier.pos
        );
        assert!(
            u.soldier.pos.z > -1.6,
            "stopped in front of it: {:?}",
            u.soldier.pos
        );
    }

    /// Drive a fresh player at `step_sim(top)` from (0, 0, `z0`), facing −Z:
    /// stick forward every tick (+ SPRINT when `sprint`), JUMP pressed at tick
    /// `jump` when given. Returns every tick's (feet, velocity).
    fn drive(top: f32, z0: f32, sprint: bool, jump: Option<u64>, ticks: u64) -> Vec<(Vec3, Vec3)> {
        drive_by(GroundRules::Bf2, top, z0, sprint, jump, ticks)
    }

    fn drive_by(
        rules: GroundRules,
        top: f32,
        z0: f32,
        sprint: bool,
        jump: Option<u64>,
        ticks: u64,
    ) -> Vec<(Vec3, Vec3)> {
        let mut s = step_sim(top);
        s.ground_rules = rules;
        let h = s
            .spawn(&UnitSpec::player_in_slot(0, 0, Vec3::new(0.0, 0.0, z0)))
            .unwrap();
        let mut out = Vec::new();
        for t in 0..ticks {
            let mut b = if sprint { buttons::SPRINT } else { 0 };
            if jump == Some(t) {
                b |= buttons::JUMP;
            }
            s.step(&[PadState {
                ly: 32767,
                buttons: b,
                ..PadState::default()
            }]);
            let u = s.unit(h).unwrap();
            out.push((u.soldier.pos, u.soldier.vel));
        }
        out
    }

    /// Card JWAL (`analysis/re-soldier-walls.md` §3.2): a box 0.75 m tall is
    /// above the step, so a walk and a sprint stop in front of it. The bare
    /// wall pass let both walk INTO it: the stepped capsule's rim contact
    /// tilted past the floor limit and was skipped.
    #[test]
    fn a_walk_or_a_sprint_stops_at_a_ledge_above_the_step() {
        for sprint in [false, true] {
            let run = drive(0.75, 0.0, sprint, None, 60);
            let (end, _) = run[run.len() - 1];
            assert!(
                end.z > -1.0 && end.y.abs() < 1e-3,
                "sprint {sprint}: stopped in front, on the floor: {end:?}"
            );
        }
    }

    /// ...and a ledge taller than a jump can reach (2.25 m: the apex, 1.86 m
    /// in BF2 and 1.91 m in BF1, + the air's step) stops every jump, in both
    /// games, from every take-off point: the feet never get past the face
    /// (the bare pass let a jump in, §2.2; the ground's whole step in the air
    /// let the PS2's apex in, card JARC).
    #[test]
    fn no_jump_passes_a_ledge_taller_than_its_reach() {
        for rules in [GroundRules::Bf2, GroundRules::Bf1] {
            for sprint in [false, true] {
                for jump in 0..40 {
                    let run = drive_by(rules, 2.25, 3.0, sprint, Some(jump), 90);
                    for (t, (p, _)) in run.iter().enumerate() {
                        assert!(
                            p.z > -1.0,
                            "{rules:?} sprint {sprint} jump {jump}: tick {t} past the face at {p:?}"
                        );
                    }
                }
            }
        }
    }

    /// A jump pressed against a wall goes straight up: the wall took the run's
    /// into-wall velocity off (the PS2's own response, re-soldier-walls §5.1),
    /// and it takes the air's steering off again every tick he is under its top.
    #[test]
    fn a_jump_pressed_against_a_wall_goes_straight_up() {
        let run = drive(3.0, 0.0, true, Some(20), 50);
        let (before, _) = run[19];
        for (p, v) in &run[21..] {
            let vxz = (v.x * v.x + v.z * v.z).sqrt();
            assert!(vxz < 0.3, "|v_xz| {vxz} after the pressed jump");
            assert!((p.z - before.z).abs() < 0.05, "stayed at the wall: {p:?}");
        }
    }

    /// A box within a jump's reach (1.2 m) is still jumped ONTO: the feet
    /// clear its top before the capsule meets its face, and the ground
    /// query stands him on it.
    #[test]
    fn a_jump_lands_on_a_box_within_its_reach() {
        let run = drive(1.2, 5.0, false, Some(4), 60);
        let on_top = run
            .iter()
            .any(|(p, _)| p.z < -1.0 && p.z > -5.0 && (p.y - 1.2).abs() < 1e-3);
        assert!(on_top, "never stood on the 1.2 m box");
    }

    /// Card AIRC: from rest against a 0.9 m box (over the step, so a walk
    /// stops at it), a jump with the stick held forward lands ON it in both
    /// games: once the feet are over its top the stick carries him in (BF2 at
    /// 2.1 m/s^2, BF1 at 1.3 m/s). With no stick he comes down where he stood.
    #[test]
    fn a_standing_jump_with_the_stick_held_gets_onto_a_box() {
        use crate::soldier::GroundRules;
        for rules in [GroundRules::Bf2, GroundRules::Bf1] {
            for stick in [32767i16, 0] {
                let mut s = step_sim(0.9);
                s.ground_rules = rules;
                let h = s
                    .spawn(&UnitSpec::player_in_slot(0, 0, Vec3::ZERO))
                    .unwrap();
                let fwd = PadState {
                    ly: 32767,
                    ..PadState::default()
                };
                // Walk to the face (z -1 less his radius) and come to rest.
                for t in 0..60 {
                    s.step(&[if t < 40 { fwd } else { PadState::default() }]);
                }
                let at = &s.unit(h).unwrap().soldier;
                assert!((at.pos.z + 0.5).abs() < 0.01 && at.vel.length() < 1e-3);
                let mut on_top = false;
                for t in 0..40 {
                    s.step(&[PadState {
                        ly: stick,
                        buttons: if t == 0 { buttons::JUMP } else { 0 },
                        ..PadState::default()
                    }]);
                    let so = &s.unit(h).unwrap().soldier;
                    on_top |= so.grounded && (so.pos.y - 0.9).abs() < 1e-3 && so.pos.z < -1.0;
                }
                assert_eq!(on_top, stick != 0, "{rules:?} stick {stick}");
            }
        }
    }

    /// A 100 m mesh floor at y = 0 with a wall slab across −Z at
    /// z ∈ (−3.5, −2.5).
    fn wall_world() -> crate::collision::CollisionWorld {
        use crate::collision::{Body, CollisionWorld, Shape};
        let floor = crate::collision::trimesh(
            &[
                [-50.0, 0.0, -50.0],
                [50.0, 0.0, -50.0],
                [50.0, 0.0, 50.0],
                [-50.0, 0.0, 50.0],
            ],
            &[0, 1, 2, 0, 2, 3],
        )
        .expect("floor");
        CollisionWorld::new(
            vec![
                Body::new(glam::Mat4::IDENTITY, Shape::Mesh(floor)),
                // A wall across the player's path (they start facing −Z).
                Body::new(
                    glam::Mat4::from_translation(Vec3::new(0.0, 0.0, -3.0)),
                    Shape::Box {
                        half: Vec3::new(20.0, 4.0, 0.5),
                    },
                ),
            ],
            Vec::new(),
        )
    }

    /// The wall pass applies to EVERY alive unit, not just the player (Phase A
    /// design decision 5 — `walls_for_ai` is retired now that the AI follows
    /// the authored corridors). A non-player unit walking at the slab stops at
    /// its face, one capsule radius out.
    #[test]
    fn ai_units_are_pushed_out_of_walls() {
        fn run(with_wall: bool) -> f32 {
            let mut sim = Sim::empty(3);
            sim.heightfield = Some(flat_ground());
            if with_wall {
                sim.collision = wall_world();
            }
            let h = sim
                .spawn(&UnitSpec {
                    controller: Controller::Pacer {
                        vel: Vec3::new(0.0, 0.0, -2.0),
                        period: 0,
                    },
                    ..UnitSpec::trooper(1, Vec3::ZERO)
                })
                .expect("spawned");
            for _ in 0..60 {
                sim.step(&[PadState::default()]);
            }
            sim.units[h.index as usize].soldier.pos.z
        }
        // 60 ticks at 2 m/s = 4 m with nothing in the way ...
        let free = run(false);
        assert!(free < -3.9, "walked 4 m: {free}");
        // ... and stopped at the slab's face with the wall there.
        let stopped = run(true);
        assert!(
            (stopped - -2.0).abs() < 0.1,
            "pushed out of the wall: {stopped}"
        );
    }

    /// Card RKSLF (OURS): a bot whose launcher's salvo is latched in the
    /// cycle (pressed, InitialSalvoDelay still running, FIRE up since)
    /// fires no rocket into the wall 2.5 m before its muzzle: the salvo is
    /// dropped where it would leave and the bot is unhurt. 22.5 m from the
    /// same wall the rocket leaves - unless a team-mate stands by the spot
    /// it lands on (an enemy there does not hold it). A charge launcher
    /// (MaxPressedTime, TriggerSingle: the shot leaves when the trigger comes
    /// up) whose bot lets go facing the near wall keeps its charge held.
    #[test]
    fn a_bot_fires_no_rocket_into_the_wall_before_it() {
        use crate::weapon::{ExplosionClass, FireState};
        // (shots, the shooter's health lost, blast holds)
        let run = |z: f32, by_wall: Option<u8>, charge: bool| {
            let mut sim = Sim::empty(3);
            sim.heightfield = Some(flat_ground());
            sim.collision = wall_world();
            let mut w = sim.weapon_classes[0].clone();
            w.ordnance.velocity = 100.0;
            w.ordnance.life_span = 3.0;
            w.ordnance.explosion = Some(ExplosionClass {
                damage: 400.0,
                damage_inner: 2.0,
                damage_outer: 4.0,
                ..ExplosionClass::default()
            });
            if charge {
                w.max_pressed_time = 0.75;
                w.trigger_single = true;
            } else {
                w.initial_salvo_delay = 0.25;
            }
            sim.weapon_classes.push(w);
            let wc = sim.weapon_classes.len() - 1;
            let h = sim.spawn(&ai(0, Vec3::new(0.0, 0.0, z), 0.0)).unwrap();
            if let Some(team) = by_wall {
                // An idle trooper on this side of the slab, 2.3 m from
                // where the rocket strikes it.
                sim.spawn(&UnitSpec::trooper(team, Vec3::new(2.0, 0.0, -1.5)))
                    .unwrap();
            }
            {
                let u = &mut sim.units[h.index as usize];
                u.weapon_class = wc;
                u.weapon.active = true;
                u.weapon.clip = 10;
                if charge {
                    // Charging for 5 ticks; the bot's pad lets go now.
                    u.weapon.fire_cmd = 5;
                } else {
                    // The press of a tick ago: the cycle holds one salvo,
                    // due 0.2 s from now.
                    u.weapon.state = FireState::Firing;
                    u.weapon.salvo_counter = 1;
                    u.weapon.salvo_timer = 0.2;
                }
            }
            let hp0 = sim.units[h.index as usize].health.cur;
            let mut shots = 0;
            for _ in 0..12 {
                sim.step(&[PadState::default()]);
                shots += sim
                    .events
                    .iter()
                    .filter(|e| matches!(e, Event::Shot { unit, .. } if *unit == h))
                    .count();
            }
            let hp = sim.unit(h).map_or(0.0, |u| u.health.cur);
            (shots, hp0 - hp, sim.nav_stats.blast_holds)
        };
        for charge in [false, true] {
            let (shots, lost, holds) = run(0.0, None, charge);
            assert_eq!(
                (shots, lost),
                (0, 0.0),
                "no rocket into the wall ({charge})"
            );
            assert!(holds >= 1);
            assert_eq!(
                run(20.0, None, charge),
                (1, 0.0, 0),
                "the far wall: it leaves"
            );
            let (shots, _, holds) = run(20.0, Some(0), charge);
            assert_eq!(shots, 0, "a team-mate by the far wall ({charge})");
            assert!(holds >= 1);
            assert_eq!(run(20.0, Some(1), charge).0, 1, "an enemy by the far wall");
        }
    }

    /// Determinism holds with a collision world, and the world CHANGES the
    /// replay (the player walks into a wall instead of through it).
    #[test]
    fn same_log_same_hash_with_a_collision_world() {
        fn run(log: &InputLog, with_world: bool) -> u64 {
            // The player is spawned AFTER the ground exists: `Sim::new`
            // spawns onto an empty sim, and since TERRAIN_BURY a unit that
            // lands on NO_GROUND stays there (300 m under an uncut
            // heightfield is underground, not a bank to climb).
            let mut sim = Sim::empty(log.seed);
            sim.heightfield = Some(flat_ground());
            if with_world {
                sim.collision = wall_world();
            }
            sim.spawn(&UnitSpec::player(0, Vec3::ZERO));
            for pads in log.frames() {
                sim.step(pads);
            }
            sim.hash()
        }
        // Walk straight into the wall (the player faces −Z), firing now and
        // then so the weapon/RNG streams are exercised too.
        let log = InputLog {
            seed: 0xC0FF_EE00,
            slots: 1,
            ticks: (0..300)
                .map(|i| PadState {
                    ly: 32767,
                    buttons: if i % 7 < 3 { buttons::FIRE } else { 0 },
                    ..Default::default()
                })
                .collect(),
        };
        let a = run(&log, true);
        assert_eq!(a, run(&log, true), "same log, same hash");
        assert_ne!(a, run(&log, false), "the wall changes the replay");
    }

    /// D9: the skill's SPREAD multiplier is written onto an AI unit's weapon
    /// every fire tick and left at 1.0 for a player, so an Easy bot's spread
    /// grows 2.26x faster per shot than a human's.
    #[test]
    fn an_ai_units_weapon_carries_the_skills_spread_multiplier() {
        use crate::skill::Difficulty;
        let mut s = Sim::empty(5);
        s.difficulty = Difficulty::Easy;
        s.heightfield = Some(flat_ground());
        s.cheats.infinite_ammo = true;
        let bot = s.spawn(&ai(0, Vec3::ZERO, 0.0)).unwrap();
        let player = s
            .spawn(&UnitSpec::player(1, Vec3::new(0.0, 0.0, -20.0)))
            .unwrap();
        s.step(&[fire()]);
        let want = Difficulty::Easy
            .skill_at(s.unit(bot).unwrap().skill)
            .spread_mult;
        assert!((want - 2.263).abs() < 1e-3, "Easy spread mult {want}");
        assert_eq!(s.unit(bot).unwrap().weapon.spread_mult, want);
        assert_eq!(s.unit(player).unwrap().weapon.spread_mult, 1.0);
        // ... and that multiplier is what scales the per-shot spread growth
        // (weapon.rs `spread_per_shot`): same class, same shot, 2.26x apart.
        let wc = &s.weapon_classes[0];
        let grow = |mult: f32| {
            let mut w = crate::weapon::WeaponState::new(wc, false, 0);
            w.spread_mult = mult;
            // From the PS2's floor, clear of the port's tight hold (card
            // SPRD), so the shot's growth is not clamped away.
            w.spread = 0.0;
            let mut lcg = crate::lcg::GameLcg::new(1);
            let mut shots = Vec::new();
            w.update(
                crate::TICK_DT,
                wc,
                crate::weapon::FireInput {
                    held: true,
                    blocked: false,
                    infinite_ammo: true,
                    ai_dispersion: None,
                },
                &mut lcg,
                &mut shots,
            );
            assert_eq!(shots.len(), 1);
            w.spread
        };
        let (a, b) = (grow(1.0), grow(want));
        // `update` also pays the per-tick recovery, which is the same on both
        // sides; add it back to compare the GROWTH the multiplier scales.
        let rec = crate::TICK_DT * wc.spread_recover_rate;
        assert!(
            ((b + rec) / (a + rec) - want).abs() < 1e-3,
            "spread growth {a} vs {b} (recovery {rec})"
        );
    }

    /// Phase C decision 2's fairness rule, at the shot: an AI unit only ever
    /// fires at a contact its own perception holds. A wall body between two
    /// bots means NO shot at all (the memory never gets a record); take the
    /// wall away and the shots come with `unperceived_shots` still 0.
    #[test]
    fn an_ai_never_fires_at_an_unperceived_target() {
        let run = |wall: bool| {
            let mut s = Sim::empty(31);
            s.heightfield = Some(flat_ground());
            if wall {
                // The wall_world slab sits at z = -3; ours has to stand
                // between two bots 25 m apart.
                s.collision.push_body(crate::collision::Body::new(
                    glam::Mat4::from_translation(Vec3::new(0.0, 5.0, -12.5)),
                    crate::collision::Shape::Box {
                        half: Vec3::new(20.0, 5.0, 0.5),
                    },
                ));
            }
            s.spawn(&ai(0, Vec3::ZERO, 0.0)).unwrap();
            s.spawn(&ai(1, Vec3::new(0.0, 0.0, -25.0), std::f32::consts::PI))
                .unwrap();
            let mut shots = 0usize;
            for _ in 0..300 {
                s.step(&[PadState::default()]);
                shots += s
                    .events
                    .iter()
                    .filter(|e| matches!(e, Event::Shot { .. }))
                    .count();
            }
            (shots, s.nav_stats.ai_shots, s.nav_stats.unperceived_shots)
        };
        let (shots, ai_shots, unperceived) = run(true);
        assert_eq!((shots, ai_shots), (0, 0), "blind bots do not shoot");
        assert_eq!(unperceived, 0);
        let (shots, ai_shots, unperceived) = run(false);
        assert!(shots > 0, "with sight they fight");
        assert_eq!(ai_shots as usize, shots, "every shot counted");
        assert_eq!(unperceived, 0, "and every one at a held contact");
    }

    /// Infinite stamina keeps a sprinting player's energy pinned at the bar.
    #[test]
    fn infinite_stamina_cheat_pins_energy() {
        let mut s = Sim::new(3);
        s.cheats.infinite_stamina = true;
        let sprint = PadState {
            ly: 32767,
            buttons: buttons::SPRINT,
            ..Default::default()
        };
        for _ in 0..300 {
            s.step(&[sprint]);
        }
        let p = s.unit(s.players[0].unwrap()).unwrap();
        assert_eq!(p.soldier.energy, p.consts.energy_bar);
        assert_eq!(p.soldier.stance, crate::soldier::Stance::Sprint);
        let mut c = Cheats::default();
        c.toggle("stamina");
        assert!(c.infinite_stamina && c.entries()[2].2);
    }

    /// A human at the origin on flat ground, facing -Z, the spread off, with
    /// a wall whose near face is `wall` metres ahead (`thick` deep).
    fn aim_sim(rules: GroundRules, pitch: f32, wall: Option<(f32, f32)>) -> Sim {
        let mut s = Sim::empty(4);
        s.ground_rules = rules;
        s.heightfield = Some(flat_ground());
        let w = &mut s.weapon_classes[0];
        w.pitch_spread = 0.0;
        w.yaw_spread = 0.0;
        w.spread_per_shot = 0.0;
        w.posture_spread = [[0.0; 3]; 3];
        if let Some((z, thick)) = wall {
            s.collision = CollisionWorld::new(
                vec![crate::collision::Body::new(
                    glam::Mat4::from_translation(Vec3::new(0.0, 0.0, -z - thick * 0.5)),
                    crate::collision::Shape::Box {
                        half: Vec3::new(50.0, 50.0, thick * 0.5),
                    },
                )],
                Vec::new(),
            );
        }
        let p = s.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
        s.units[p.index as usize].soldier.pitch = pitch;
        s
    }

    /// One shot: (the aim ray, H, the bolt's origin and direction).
    fn one_shot(s: &mut Sim) -> ((Vec3, Vec3), Vec3, Vec3, Vec3) {
        let ray = s.units[0].aim_ray(s.ground_rules, 1);
        let h = s.aim_hit(0);
        s.step(&[fire()]);
        let (o, d) = s
            .events
            .iter()
            .find_map(|e| match e {
                Event::Shot { origin, dir, .. } => Some((*origin, *dir)),
                _ => None,
            })
            .expect("the player fired");
        (ray, h, o, d)
    }

    /// Card AIMA, the PS2's numbers (`re-aim-assist.md` §10.2-10.3, [V-RAM]):
    /// a soldier spawns at pitch -0.07; standing, the aim origin A is 2.1043 m
    /// (BF1) / 2.3543 m (BF2) above the feet, the ideal camera looks 5 / 10
    /// degrees below the aim, and the fire point F is 1.5828 m up, 0.060 m to
    /// the right and 0.2424 m ahead.
    #[test]
    fn the_aim_origin_camera_and_fire_point_are_the_ps2s() {
        for (rules, a_y, cam_deg) in [
            (GroundRules::Bf1, 2.1043, -9.011),
            (GroundRules::Bf2, 2.3543, -14.011),
        ] {
            let s = aim_sim(rules, crate::unit::SPAWN_PITCH, None);
            let u = &s.units[0];
            assert_eq!(u.soldier.pitch, -0.07, "a human's spawn pitch");
            let (a, d) = u.aim_ray(rules, 1);
            let cam = u.ideal_camera(rules, 1);
            let pitch = cam.forward.y.asin().to_degrees();
            let f = u.fire_point();
            println!(
                "{rules:?}: A {:.4} m up, camera {pitch:.3} deg, F up {:.4} right {:.4} ahead {:.4}",
                a.y, f.y, f.x, -f.z
            );
            assert!((a.y - a_y).abs() < 0.002, "A {} != {a_y}", a.y);
            assert!(
                (pitch - cam_deg).abs() < 0.01,
                "camera {pitch} != {cam_deg}"
            );
            assert!((f.y - 1.5828).abs() < 0.002, "F up {}", f.y);
            assert!((f.x - 0.060).abs() < 0.002, "F right {}", f.x);
            assert!((-f.z - 0.2424).abs() < 0.002, "F ahead {}", -f.z);
            // The ray leaves the camera's own position along the aim.
            assert!((a - cam.position).cross(d).length() < 1e-4);
        }
        // Split screen: 0.4 of the tilt (BF1 2 deg).
        let s = aim_sim(GroundRules::Bf1, 0.0, None);
        let cam = s.units[0].ideal_camera(GroundRules::Bf1, 2);
        assert!((cam.forward.y.asin().to_degrees() + 2.0).abs() < 0.01);
    }

    /// Card AIMA, the wall test (BF1's table, the spread off). Within the
    /// ray's reach the bolt goes to the reticle's point; past it the bolt is
    /// aimed at the ray's far end; a wall nearer than the ray's start is not
    /// seen (the PS2's blind spot) and the bolt lands under the reticle.
    #[test]
    fn a_humans_bolt_goes_where_the_reticles_ray_hits() {
        let rules = GroundRules::Bf1;
        let pitch = crate::unit::SPAWN_PITCH;
        // Where a line from `o` along `d` meets the plane z = -wall.
        let at = |o: Vec3, d: Vec3, wall: f32| o + d * ((-wall - o.z) / d.z);

        // The wall 20 m out. On flat ground the default pitch's ray meets
        // the floor first, so the ray is raised to meet the wall.
        let mut s = aim_sim(rules, 0.05, Some((20.0, 1.0)));
        let ((a, d), h, f, dir) = one_shot(&mut s);
        let miss = (at(f, dir, 20.0) - at(a, d, 20.0)).length();
        println!("wall 20 m: H {h:?}, the bolt passes {miss:.4} m from the ray's point");
        assert!((h.z + 20.0).abs() < 1e-3, "H is on the wall: {h:?}");
        assert!(miss < 0.05);

        // 80 m out: H is the ray's end, 65.5 m from A.
        let mut s = aim_sim(rules, 0.05, Some((80.0, 1.0)));
        let ((a, d), h, f, dir) = one_shot(&mut s);
        let over = at(f, dir, 80.0).y - at(a, d, 80.0).y;
        println!(
            "wall 80 m: |H - A| {:.3}, the bolt {over:.4} m over the ray's point",
            (h - a).length()
        );
        assert!(((h - a).length() - 65.5).abs() < 1e-3);
        assert!((0.0..0.15).contains(&over), "{over}");

        // 5 m from the feet: inside the ray's 5.5 m start.
        let mut s = aim_sim(rules, 0.05, Some((5.0, 0.2)));
        let ((a, d), h, f, dir) = one_shot(&mut s);
        let under = at(a, d, 5.0).y - at(f, dir, 5.0).y;
        println!(
            "wall 5 m: |H - A| {:.3}, the bolt {under:.4} m under the ray's point",
            (h - a).length()
        );
        assert!(
            ((h - a).length() - 65.5).abs() < 1e-3,
            "the wall is not seen"
        );
        assert!((0.45..0.55).contains(&under), "{under}");

        // The default pitch: the bolt goes to the floor under the reticle,
        // not into the floor near the feet.
        let mut s = aim_sim(rules, pitch, None);
        let ((a, d), h, f, dir) = one_shot(&mut s);
        let land = f + dir * (-f.y / dir.y);
        let ray_floor = a + d * (-a.y / d.y);
        println!(
            "default pitch: H {h:?}, the bolt lands {:.2} m ahead, the ray {:.2} m",
            -land.z, -ray_floor.z
        );
        assert!(-land.z > 5.0);
        assert!(
            (land - ray_floor).length() < 0.6,
            "on the floor under the reticle"
        );

        // The steepest aim: 1.53 m ahead of the feet (BF1), 1.54 m (BF2);
        // never on them.
        for (rules, want) in [(GroundRules::Bf1, 1.53), (GroundRules::Bf2, 1.54)] {
            let mut s = aim_sim(rules, -0.8, None);
            let (_, _, f, dir) = one_shot(&mut s);
            let land = f + dir * (-f.y / dir.y);
            println!(
                "{rules:?} pitch -0.8: the bolt lands {:.3} m ahead",
                -land.z
            );
            assert!((-land.z - want).abs() < 0.02, "{}", -land.z);
        }
    }

    /// A unit on the aim ray is what the bolt is aimed at; a bot's shot is
    /// untouched (the chest, along its aim).
    #[test]
    fn the_aim_ray_sees_units_and_bots_keep_their_muzzle() {
        let mut s = aim_sim(GroundRules::Bf2, -0.05, None);
        let (a, d) = s.units[0].aim_ray(GroundRules::Bf2, 1);
        // A trooper standing where the ray is 1 m above the floor.
        let on_ray = a + d * ((1.0 - a.y) / d.y);
        s.spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, on_ray.z)))
            .unwrap();
        let h = s.aim_hit(0);
        println!("the ray is at 1 m {:.2} m out; H {h:?}", -on_ray.z);
        assert!(
            (h.z - (on_ray.z + 0.5)).abs() < 0.05,
            "the capsule's near side: {h:?}"
        );
        let (_, _, f, dir) = one_shot(&mut s);
        assert!(
            (f + dir * (h - f).length() - h).length() < 1e-3,
            "the bolt flies at H"
        );
        let b = s.spawn(&ai(1, Vec3::new(40.0, 0.0, 0.0), 0.0)).unwrap();
        let u = s.unit(b).unwrap();
        assert_eq!(u.soldier.pitch, 0.0, "a bot's spawn pitch is FPNT's");
        assert_eq!(u.muzzle().y, 1.425);
    }

    /// Card ZOOM's fixtures: a rifle (2.5 / 2.5), a sniper rifle (2 / 8,
    /// `ZoomFirstPerson`) in channel 0's second slot and a grenade (no zoom
    /// key) in channel 1, on a human.
    fn zoom_sim(rules: GroundRules) -> Sim {
        use crate::unit::SlotSpec;
        let mut s = Sim::empty(3);
        s.heightfield = Some(flat_ground());
        s.ground_rules = rules;
        let mut sniper = WeaponClass::bf1_rifle();
        sniper.name = "sniper".into();
        (sniper.zoom_min, sniper.zoom_max) = (2.0, 8.0);
        sniper.zoom_turn_div = [2.0, 8.0];
        sniper.zoom_first_person = true;
        let sniper = s.add_weapon_class(sniper);
        let mut grenade = WeaponClass::bf1_rifle();
        grenade.name = "grenade".into();
        (grenade.zoom_min, grenade.zoom_max) = (1.0, 1.0);
        grenade.zoom_turn_div = [1.0, 1.0];
        let grenade = s.add_weapon_class(grenade);
        let slot = |slot, channel, weapon_class| SlotSpec {
            slot,
            channel,
            weapon_class,
            clips: 4,
        };
        s.spawn(&UnitSpec {
            slots: std::sync::Arc::new([slot(1, 0, sniper), slot(2, 1, grenade)]),
            ..UnitSpec::player(0, Vec3::ZERO)
        })
        .unwrap();
        s
    }

    fn pad(buttons: u32) -> PadState {
        PadState {
            buttons,
            ..Default::default()
        }
    }

    /// A press of `bit` (one tick down, one up); the zoom value after it.
    fn tap(s: &mut Sim, bit: u32) -> Option<f32> {
        s.step(&[pad(bit)]);
        s.step(&[pad(0)]);
        s.units[0].zoom.map(|z| z.value)
    }

    /// Card ZOOM acceptance 1 (`analysis/re-zoom.md` §3, §9): R3 is a press
    /// toggle - the rifle on, off; the sniper rifle 2, 8, off; a held button
    /// is one press; a grenade in hand and a bot never.
    #[test]
    fn the_zoom_press_steps_the_weapon_in_hand() {
        let mut s = zoom_sim(GroundRules::Bf1);
        let seq: Vec<_> = (0..4).map(|_| tap(&mut s, buttons::ZOOM)).collect();
        println!("rifle: {seq:?}");
        assert_eq!(seq, [Some(2.5), None, Some(2.5), None]);
        // Held 1.5 s: one toggle on the press, nothing at the release.
        for _ in 0..45 {
            s.step(&[pad(buttons::ZOOM)]);
            assert_eq!(s.units[0].zoom.map(|z| z.step), Some(0));
        }
        s.step(&[pad(0)]);
        assert!(s.units[0].zoom.is_some());
        tap(&mut s, buttons::ZOOM);
        // The sniper rifle: 2, 8, off, 2 with the divisors 2, 8.
        tap(&mut s, buttons::SWITCH_WEAPON);
        assert_eq!(s.weapon_classes[s.units[0].weapon_class].name, "sniper");
        let seq: Vec<_> = (0..4)
            .map(|_| {
                tap(&mut s, buttons::ZOOM);
                s.units[0].zoom.map(|z| (z.value, z.divisor, z.scope))
            })
            .collect();
        println!("sniper: {seq:?}");
        assert_eq!(
            seq,
            [
                Some((2.0, 2.0, true)),
                Some((8.0, 8.0, true)),
                None,
                Some((2.0, 2.0, true))
            ]
        );
        // A weapon with no zoom key in the hands: nothing.
        let mut s = zoom_sim(GroundRules::Bf1);
        s.units[0].weapon_class = 2;
        assert_eq!(tap(&mut s, buttons::ZOOM), None);
        // A bot with the ZOOM bit forced: nothing.
        let mut s = zoom_sim(GroundRules::Bf1);
        s.units[0].controller = Controller::Ai;
        s.tick_zoom(0, &pad(buttons::ZOOM));
        assert_eq!(s.units[0].zoom, None);
        s.units[0].controller = Controller::Player { slot: 0 };
        s.tick_zoom(0, &pad(buttons::ZOOM));
        assert!(s.units[0].zoom.is_some(), "the same press on a human");
    }

    /// Card ZOOM acceptance 2 (§4, §9): what ends the zoom and what keeps it.
    #[test]
    fn the_zoom_ends_on_its_cancels_and_no_others() {
        let fwd = |buttons| PadState {
            ly: 32767,
            buttons,
            ..Default::default()
        };
        // (name, rules, the pad held after zooming, ticks, still zoomed?)
        let cases: [(&str, GroundRules, PadState, u32, bool); 9] = [
            ("reload", GroundRules::Bf1, pad(buttons::RELOAD), 3, false),
            (
                "primary switch",
                GroundRules::Bf1,
                pad(buttons::SWITCH_WEAPON),
                1,
                false,
            ),
            (
                "secondary throw",
                GroundRules::Bf1,
                pad(buttons::SECONDARY_FIRE),
                3,
                false,
            ),
            (
                "roll",
                GroundRules::Bf1,
                fwd(buttons::ROLL_STANCE),
                3,
                false,
            ),
            ("sprint", GroundRules::Bf2, fwd(buttons::SPRINT), 60, false),
            ("walk", GroundRules::Bf2, fwd(0), 60, true),
            ("jump", GroundRules::Bf1, pad(buttons::JUMP), 20, true),
            (
                "crouch",
                GroundRules::Bf1,
                pad(buttons::ROLL_STANCE),
                20,
                true,
            ),
            ("fire", GroundRules::Bf1, pad(buttons::FIRE), 20, true),
        ];
        for (name, rules, held, ticks, kept) in cases {
            let mut s = zoom_sim(rules);
            // A clip some rounds short (a full one does not reload), then
            // past the stance cycle's half second.
            for _ in 0..6 {
                s.step(&[pad(buttons::FIRE)]);
            }
            for _ in 0..20 {
                s.step(&[pad(0)]);
            }
            // The roll and the sprint need a soldier already moving.
            let moving = held.ly != 0;
            for _ in 0..if moving { 30 } else { 0 } {
                s.step(&[fwd(0)]);
            }
            s.step(&[if moving {
                fwd(buttons::ZOOM)
            } else {
                pad(buttons::ZOOM)
            }]);
            assert!(s.units[0].zoom.is_some(), "{name}: zoomed");
            let stance0 = s.units[0].soldier.stance;
            let mut ended = None;
            let mut stances = vec![];
            for t in 1..=ticks {
                s.step(&[held]);
                let st = s.units[0].soldier.stance;
                if !stances.contains(&st) {
                    stances.push(st);
                }
                if ended.is_none() && s.units[0].zoom.is_none() {
                    ended = Some(t);
                }
            }
            println!("{name}: from {stance0:?} through {stances:?}, zoom ended on tick {ended:?}");
            assert_eq!(ended.is_none(), kept, "{name}");
            match name {
                "roll" => assert!(stances.contains(&Stance::Roll)),
                "sprint" => assert!(stances.contains(&Stance::Sprint)),
                "jump" => assert!(stances.contains(&Stance::Jump)),
                "crouch" => assert!(stances.contains(&Stance::Crouch)),
                _ => {}
            }
            // Nothing brings it back: the next press starts at ZoomMin.
            if !kept {
                s.step(&[pad(0)]);
                assert_eq!(s.units[0].zoom, None, "{name}: stays off");
            }
        }
        // Death.
        let mut s = zoom_sim(GroundRules::Bf1);
        tap(&mut s, buttons::ZOOM);
        let h = s.handle(0);
        s.kill_environment(h, DeathCause::Fall);
        s.step(&[pad(0)]);
        assert_eq!(s.units[0].zoom, None, "death");
    }

    /// Card ZOOM acceptance 3 (§6): zoomed, the yaw and pitch rates are the
    /// plain ones over the weapon's turn divisor (PS2: 1 / 2.497 to 1 / 2.499
    /// for 2.5 on every update), and the spread's posture takes the flag.
    #[test]
    fn the_zoomed_turn_rates_are_divided_by_the_divisor() {
        let turn = PadState {
            rx: 32767,
            ry: 16000,
            ..Default::default()
        };
        // (yaw, pitch) turned in one tick, per zoom step reached by `taps`.
        let rate = |taps: u32, sniper: bool| {
            let mut s = zoom_sim(GroundRules::Bf1);
            s.assist_intensity = 0.0;
            s.units[0].soldier.pitch = 0.0;
            if sniper {
                tap(&mut s, buttons::SWITCH_WEAPON);
            }
            for _ in 0..taps {
                tap(&mut s, buttons::ZOOM);
            }
            let before = s.units[0].soldier.clone();
            s.step(&[turn]);
            let after = &s.units[0].soldier;
            (after.yaw - before.yaw, after.pitch - before.pitch)
        };
        let plain = rate(0, false);
        assert!(plain.0 != 0.0 && plain.1 != 0.0);
        for (taps, sniper, div) in [(1, false, 2.5), (1, true, 2.0), (2, true, 8.0)] {
            let z = rate(taps, sniper);
            let (ry, rp) = (plain.0 / z.0, plain.1 / z.1);
            println!("divisor {div}: yaw plain / zoomed {ry:.6}, pitch {rp:.6}");
            assert!((ry - div).abs() < 1e-4 * div, "yaw {ry}");
            assert!((rp - div).abs() < 1e-4 * div, "pitch {rp}");
            assert!((z.0 - plain.0 / div).abs() < 1e-5);
            assert!((z.1 - plain.1 / div).abs() < 1e-5);
        }
    }

    /// Card ZOOM acceptance 4, the sim's half (§5): the zoomed camera is the
    /// stance's ZOOM section on the first zoomed tick - the PS2's eye (right
    /// 0.400, up 2.216, back 2.769) 3.50 deg under the aim at pitch -4.01
    /// deg, both games - and the marksman's scope eye is (0.060, 1.604,
    /// 0.057) with no tilt (BF2: 0.6 m back, no sideways terms).
    #[test]
    fn the_zoomed_camera_is_the_ps2s() {
        let rel = |s: &Sim| {
            let u = &s.units[0];
            let cam = u.ideal_camera(s.ground_rules, 1);
            let (sy, cy) = crate::fmath::sin_cos(u.soldier.yaw);
            let (ahead, right) = (Vec3::new(-sy, 0.0, -cy), Vec3::new(cy, 0.0, -sy));
            let p = cam.position - u.soldier.pos;
            let pitch = cam.forward.y.atan2(cam.forward.with_y(0.0).length());
            (
                Vec3::new(p.dot(right), p.y, -p.dot(ahead)),
                (u.soldier.pitch - pitch).to_degrees(),
            )
        };
        let near = |a: Vec3, b: Vec3| (a - b).abs().max_element() < 0.0015;
        for rules in [GroundRules::Bf1, GroundRules::Bf2] {
            let mut s = zoom_sim(rules);
            s.units[0].soldier.yaw = 0.7;
            s.units[0].soldier.pitch = f32::to_radians(-4.01);
            let plain = rel(&s);
            s.step(&[pad(buttons::ZOOM)]);
            let (eye, tilt) = rel(&s);
            println!("{rules:?}: unzoomed {plain:?}, first zoomed tick eye {eye}, tilt {tilt:.3}");
            let want = match rules {
                GroundRules::Bf1 => (Vec3::new(0.0, 2.348, 3.457), 5.0),
                GroundRules::Bf2 => (Vec3::new(0.0, 2.575, 3.105), 10.0),
            };
            assert!(near(plain.0, want.0) && (plain.1 - want.1).abs() < 0.01);
            assert!(near(eye, Vec3::new(0.400, 2.216, 2.769)), "{eye}");
            assert!((tilt - 3.5).abs() < 0.01);
            // The aim ray leaves abreast of the eye point, 2.8 m ahead of
            // the camera, and the reticle's ray is still the aim.
            let (a, d) = s.units[0].aim_ray(rules, 1);
            assert_eq!(d, s.units[0].aim_frame().0);
            assert!(a.distance(s.units[0].ideal_camera(rules, 1).position) < 2.9);
            // The scope.
            s.step(&[pad(0)]);
            s.step(&[pad(buttons::ZOOM)]);
            tap(&mut s, buttons::SWITCH_WEAPON);
            s.step(&[pad(buttons::ZOOM)]);
            assert!(s.units[0].scoped());
            let (eye, tilt) = rel(&s);
            println!("{rules:?}: scope eye {eye}, tilt {tilt:.3}");
            let want = match rules {
                GroundRules::Bf1 => Vec3::new(0.060, 1.604, 0.057),
                GroundRules::Bf2 => Vec3::new(-0.060, 1.724, 0.349),
            };
            assert!(near(eye, want), "{eye}");
            assert!(tilt.abs() < 1e-3);
            // The scoped shot leaves the scope point along the aim itself.
            let u = &s.units[0];
            let (o, (fwd, ..)) = s.human_shot(0);
            assert_eq!((o, fwd), (u.scope_point(rules), u.aim_frame().0));
            assert!((s.aim_hit(0) - (o + fwd * SCOPE_AIM_LENGTH)).length() < 1e-3);
        }
    }

    /// A round nobody zooms in hashes as it did: the zoom enters the hash
    /// only while it is on.
    #[test]
    fn the_zoom_is_hashed_only_while_on() {
        let mut a = zoom_sim(GroundRules::Bf1);
        let b = zoom_sim(GroundRules::Bf1);
        assert_eq!(a.hash(), b.hash());
        a.units[0].zoom = Some(crate::unit::Zoom {
            step: 0,
            value: 2.5,
            divisor: 2.5,
            scope: false,
        });
        assert_ne!(a.hash(), b.hash());
    }
}
