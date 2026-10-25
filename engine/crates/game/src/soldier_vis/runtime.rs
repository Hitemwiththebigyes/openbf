//! Soldier asset loading, animation updates and draw submission.

use super::{
    fire_point, held_or_primary, model_of, part_groups, part_spec, rider_now, seat_now,
    weapon_in_hand, with_spec, ClassDraw, DrawPlan, FlashModel, HoverClock, SeatClock,
    SoldierAsset, SoldierVisuals, UnitVisual, WeaponAsset, CORPSE_SECS,
};
use crate::animator::{AnimInput, Animator, ClassAnim, Rig};
use crate::assets::{load_skinned_glb, SkinnedAsset};
use crate::viewer::turret_pose as tp;
use crate::walk::{TeamTemplate, WalkSim};
use glam::{Mat4, Vec3};
use render::{ParticleDraw, Renderer, SkinnedDrawCmd, TextureHandle};
use std::collections::HashMap;
use std::path::Path;

impl SoldierVisuals {
    /// Load the battle's soldier models and install its turret and walker rigs.
    pub fn for_walk(
        r: &mut Renderer,
        assets_dir: &Path,
        scene: &crate::assets::Scene,
        walk: &WalkSim,
    ) -> Self {
        let mut vis = Self::load(r, assets_dir, &scene.manifest.source_game, &walk.teams);
        vis.set_turrets(tp::rigged_turrets(scene));
        vis.set_walker_rigs(crate::viewer::walker_rigs(
            &scene.manifest,
            assets_dir,
            &walk.vehicle_geometry,
        ));
        vis
    }

    /// Load every class model both teams name, from
    /// `assets_dir/side_<game>/soldiers/`. Classes whose GLB is missing take
    /// their team's first class's asset; a team with no model at all keeps
    /// the marker box.
    ///
    /// `game` is the MAP's source game — the same disc `game::roster` took
    /// the classes off, so the `GeometryName` a class carries is looked up in
    /// the side bake that holds it. Both discs bake all four sides, and the
    /// faction's OWN game is the wrong answer for a BF2 GCW map or a BF1 CW
    /// one: over the 322 class slots on such (map, faction) pairs the model
    /// resolves 309 times in the map's own side dir and 225 times in the
    /// faction's native one (the 13 that miss either way are the droideka's
    /// `cis_walk_droideka`, and the droideka is not a fieldable class).
    pub fn load(
        r: &mut Renderer,
        assets_dir: &Path,
        game: &str,
        teams: &[TeamTemplate; 2],
    ) -> Self {
        let plan = DrawPlan::of(teams);
        let mut cache: HashMap<String, Option<TextureHandle>> = HashMap::new();
        let bodies = plan
            .bodies
            .iter()
            .map(|geometry| Self::load_body(Some(&mut *r), assets_dir, game, geometry, &mut cache))
            .collect();
        let weapons: Vec<WeaponAsset> = plan
            .weapons
            .iter()
            .map(|wg| Self::load_weapon(Some(&mut *r), assets_dir, game, wg, &mut cache))
            .collect();
        let flash = FlashModel::load(r, assets_dir, game);
        let mut s = Self::assemble(plan, teams, bodies, weapons, flash);
        s.bf2 = game.eq_ignore_ascii_case("bf2");
        // Each class's skins over its body's parts (card USKN).
        let side = assets_dir.join(format!("side_{game}"));
        for (team, t) in teams.iter().enumerate() {
            let mut rows = Vec::new();
            for (k, c) in t.classes.iter().enumerate() {
                let body = s.per_class[team].get(k).and_then(|d| d.body);
                let groups = body.map_or(&[][..], |b| s.assets[b].groups.as_slice());
                let row: Vec<Option<TextureHandle>> = groups
                    .iter()
                    .map(|g| {
                        let tex = c.skins.of_group(g.as_deref()?)?;
                        Self::texture(r, &side, Some(tex), &mut cache)
                    })
                    .collect();
                rows.push(if row.iter().any(Option::is_some) {
                    row
                } else {
                    Vec::new()
                });
            }
            s.skins[team] = rows;
        }
        s
    }

    /// The map's rigged armed buildings (`turret_pose::rigged_turrets`):
    /// a gunner seated in one is drawn at his seat (card VTURG).
    pub fn set_turrets(&mut self, turrets: Vec<tp::RiggedTurret>) {
        self.turrets = turrets;
    }

    /// Each vehicle class's walker rig (`viewer::walker_rigs`): a rider on
    /// an animated node rides it as it walks (card VRIDE).
    pub fn set_walker_rigs(
        &mut self,
        rigs: Vec<Option<std::sync::Arc<crate::viewer::walker_pose::WalkerRig>>>,
    ) {
        self.walker_rigs = rigs;
    }

    /// Bring every hover's rider numbers to this tick: one drive update of
    /// the tick's length per tick while it has a pilot, with his input
    /// (`tp::HoverNine::step`); a fresh vehicle starts at the PS2's -1.
    fn step_hover_nines(&mut self, walk: &WalkSim) {
        let tick = walk.sim.tick;
        let vs = &walk.sim.vehicles;
        self.hover_nine.resize(vs.len(), None);
        for (slot, v) in vs.iter().enumerate() {
            if !v.alive || v.kind != core::vehicle::VehicleKind::Hover {
                self.hover_nine[slot] = None;
                continue;
            }
            let fresh = HoverClock {
                gen: v.gen,
                tick,
                nine: tp::HoverNine::START,
            };
            let c = self.hover_nine[slot].get_or_insert(fresh);
            if c.gen != v.gen {
                *c = fresh;
            }
            let piloted = v.pilot_seat().is_some_and(|s| s.occupant.is_some());
            if piloted {
                let no_strafe = walk
                    .sim
                    .hover_params
                    .get(v.class as usize)
                    .and_then(Option::as_ref)
                    .is_some_and(|p| p.strafe_speed < 1e-4);
                let i = v.input;
                for _ in 0..tick.saturating_sub(c.tick).min(30) {
                    c.nine = c.nine.step(
                        [i.throttle, i.strafe, i.turn],
                        no_strafe,
                        self.bf2,
                        core::TICK_DT,
                    );
                }
            }
            c.tick = tick;
        }
    }

    /// The animators without a renderer (a census, a test): every body's
    /// skeleton, clips and sets, every hand weapon's nodes; nothing to draw.
    pub fn load_cpu(assets_dir: &Path, game: &str, teams: &[TeamTemplate; 2]) -> Self {
        let plan = DrawPlan::of(teams);
        let mut cache = HashMap::new();
        let bodies = plan
            .bodies
            .iter()
            .map(|geometry| Self::load_body(None, assets_dir, game, geometry, &mut cache))
            .collect();
        let weapons: Vec<WeaponAsset> = plan
            .weapons
            .iter()
            .map(|wg| Self::load_weapon(None, assets_dir, game, wg, &mut cache))
            .collect();
        let mut s = Self::assemble(plan, teams, bodies, weapons, None);
        s.bf2 = game.eq_ignore_ascii_case("bf2");
        s
    }

    /// `bodies` per `plan.bodies` entry, `None` for one that is not baked.
    fn assemble(
        plan: DrawPlan,
        teams: &[TeamTemplate; 2],
        bodies: Vec<Option<SoldierAsset>>,
        weapons: Vec<WeaponAsset>,
        flash: Option<FlashModel>,
    ) -> Self {
        // A body that is not baked leaves no asset: plan index -> `assets`.
        let mut assets: Vec<SoldierAsset> = Vec::new();
        let bodies: Vec<Option<usize>> = bodies
            .into_iter()
            .map(|a| {
                assets.push(a?);
                Some(assets.len() - 1)
            })
            .collect();
        let mut per_class = plan.per_class;
        for (i, t) in teams.iter().enumerate() {
            for c in per_class[i].iter_mut() {
                c.body = c.body.and_then(|b| bodies[b]);
            }
            // A class with no model of its own borrows the team's FIRST
            // class's; only a team whose first class has none keeps the box.
            let fallback = per_class[i].first().and_then(|c| c.body);
            for (k, c) in per_class[i].iter_mut().enumerate() {
                if c.body.is_none() && fallback.is_some() {
                    println!(
                        "soldier {}: no baked model — drawing {} instead",
                        t.classes[k].name, t.classes[0].name
                    );
                    c.body = fallback;
                }
            }
        }
        Self {
            assets,
            weapons,
            per_class,
            held: plan.held,
            anim: teams
                .each_ref()
                .map(|t| t.classes.iter().map(|c| c.anim.clone()).collect()),
            skins: [Vec::new(), Vec::new()],
            units: Vec::new(),
            flash,
            turrets: Vec::new(),
            bf2: false,
            walker_rigs: Vec::new(),
            hover_nine: Vec::new(),
        }
    }

    /// A part's texture from `side/textures/`, uploaded once per name.
    fn texture(
        r: &mut Renderer,
        side: &Path,
        material: Option<&str>,
        cache: &mut HashMap<String, Option<TextureHandle>>,
    ) -> Option<TextureHandle> {
        let key = material?.to_ascii_lowercase();
        if !cache.contains_key(&key) {
            let h = crate::viewer::upload_texture_file(r, &side.join("textures"), &key)
                .unwrap_or_else(|e| {
                    eprintln!("{e:#}");
                    None
                });
            cache.insert(key.clone(), h);
        }
        cache[&key]
    }

    /// One skinned soldier model, or `None` if it is not baked. Without a
    /// renderer it has no meshes.
    fn load_body(
        r: Option<&mut Renderer>,
        assets_dir: &Path,
        game: &str,
        geometry: &str,
        cache: &mut HashMap<String, Option<TextureHandle>>,
    ) -> Option<SoldierAsset> {
        // The side bake knows no map, so a BF1 part's sun-coloured
        // specular pass resolves against the lighting the map's
        // `upload_scene` already put on the renderer: every caller loads
        // the soldiers after the scene.
        let side = assets_dir.join(format!("side_{game}"));
        let glb = side.join("soldiers").join(format!("{geometry}.glb"));
        let asset = match load_skinned_glb(&glb) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("soldier {geometry}: {e:#}");
                return None;
            }
        };
        let mut meshes = Vec::new();
        let mut groups = Vec::new();
        if let Some(r) = r {
            match crate::viewer::upload_skinned(r, &asset.mesh) {
                Ok(parts) => {
                    // The model's override groups; a model baked before
                    // card USKN has none.
                    let path = side.join("soldiers").join(format!("{geometry}.skins.ron"));
                    let skins: manifest::SoldierSkins = pak::read_to_string(&path)
                        .ok()
                        .and_then(|t| ron::from_str(&t).ok())
                        .unwrap_or_default();
                    let mats: Vec<Option<&str>> = parts.iter().map(|p| p.1).collect();
                    groups = part_groups(&mats, &skins.groups);
                    for (k, (mesh, mat, layer)) in parts.into_iter().enumerate() {
                        let tex = Self::texture(r, &side, mat, cache);
                        meshes.push((mesh, tex, layer, part_spec(r, asset.mesh.spec_pass(k))));
                    }
                }
                Err(e) => {
                    eprintln!("soldier {geometry}: {e:#}");
                    return None;
                }
            }
        }
        let hp_weapons = asset.skeleton.index_of("hp_weapons");
        let rig = Self::load_rig(&side, geometry, &asset);
        println!(
            "soldier {geometry}: {} bones, {} clips, {} parts",
            asset.skeleton.len(),
            asset.clips.len(),
            meshes.len()
        );
        Some(SoldierAsset {
            asset,
            meshes,
            rig,
            hp_weapons,
            groups,
        })
    }

    /// A baked soldier's animation sets over its skeleton and clips.
    pub fn load_rig(side: &Path, geometry: &str, asset: &SkinnedAsset) -> Option<Rig> {
        let path = manifest::anim_sets::path(side, geometry);
        let sets = pak::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| ron::from_str(&t).map_err(|e| e.to_string()));
        match sets {
            Ok(sets) => Some(Rig::new(sets, &asset.skeleton, &asset.clips)),
            Err(e) => {
                eprintln!("soldier {geometry}: {}: {e} (not animated)", path.display());
                None
            }
        }
    }

    /// One hand weapon: a static GLB under weapons/, textures beside, and its
    /// nodes. A model that is not baked gives no parts (no weapon drawn);
    /// without a renderer only the nodes are read.
    fn load_weapon(
        r: Option<&mut Renderer>,
        assets_dir: &Path,
        game: &str,
        wg: &str,
        cache: &mut HashMap<String, Option<TextureHandle>>,
    ) -> WeaponAsset {
        let side = assets_dir.join(format!("side_{game}"));
        let mut parts = Vec::new();
        let wpath = side.join("weapons").join(format!("{wg}.glb"));
        if let Some(r) = r {
            match crate::assets::load_glb(&wpath) {
                Ok(cpu) => match crate::viewer::upload_submeshes(r, &cpu) {
                    Ok(up) => {
                        for (k, (mesh, mat, layer, _)) in up.into_iter().enumerate() {
                            let tex = Self::texture(r, &side, mat, cache);
                            parts.push((mesh, tex, layer, part_spec(r, cpu.spec_pass(k))));
                        }
                    }
                    Err(e) => eprintln!("weapon {wg}: {e:#}"),
                },
                Err(e) => eprintln!("weapon {wg}: {e:#} (no weapon drawn)"),
            }
        }
        let nodes = pak::read_to_string(manifest::weapon_nodes_path(&side, wg))
            .ok()
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_default();
        println!("weapon {wg}: {} parts", parts.len());
        WeaponAsset { parts, nodes }
    }

    /// What a unit's class draws: its own pair, or the team's first-class
    /// body fallback that `load` resolved for it.
    fn class_draw(&self, team: u8, class: u8) -> ClassDraw {
        let row = &self.per_class[usize::from(team.min(1))];
        let i = usize::from(class).min(row.len().saturating_sub(1));
        row.get(i).copied().unwrap_or_default()
    }

    /// Index into `assets` for a unit's class.
    fn asset_index(&self, team: u8, class: u8) -> Option<usize> {
        self.class_draw(team, class).body
    }

    /// The asset a unit draws with.
    fn asset_for(&self, team: u8, class: u8) -> Option<&SoldierAsset> {
        self.assets.get(self.asset_index(team, class)?)
    }

    /// The hand weapon a unit draws while its class slot `slot` is in hand,
    /// if that slot has a model ([`DrawPlan::weapon_in_slot`]).
    pub(super) fn weapon_for(&self, team: u8, class: u8, slot: u8) -> Option<&WeaponAsset> {
        let row = &self.held[usize::from(team.min(1))];
        let i = usize::from(class).min(row.len().saturating_sub(1));
        let held = row.get(i).map_or(&[][..], Vec::as_slice);
        self.weapons.get(held_or_primary(
            held,
            self.class_draw(team, class).weapon,
            slot,
        )?)
    }

    pub fn has_model(&self, team: u8) -> bool {
        self.per_class[usize::from(team.min(1))]
            .iter()
            .any(|c| c.body.is_some())
    }

    /// The tick the sim just stepped: who fired (the muzzle flash's clock).
    /// [`SoldierVisuals::update`] reads the last tick's shots itself; a frame
    /// that steps several ticks calls this after each
    /// (`WalkSim::advance_with`) so an earlier tick's shot still flashes.
    pub fn note_shots(&mut self, walk: &WalkSim) {
        for e in &walk.sim.events {
            let core::sim::Event::Shot { unit, .. } = e else {
                continue;
            };
            let (Some(v), Some(u)) = (
                self.units.get_mut(unit.index as usize),
                walk.sim.units.get(unit.index as usize),
            ) else {
                continue;
            };
            if v.gen == u.gen {
                v.shot_tick = Some(walk.sim.tick);
            }
        }
    }

    /// Advance every unit's animator from the sim state (`dt` = real seconds).
    pub fn update(&mut self, walk: &WalkSim, dt: f32) {
        self.update_with(walk, dt, None);
    }

    /// [`SoldierVisuals::update`], counting what each animator holds.
    pub fn update_with(
        &mut self,
        walk: &WalkSim,
        dt: f32,
        mut census: Option<&mut crate::animator::census::Census>,
    ) {
        let n = walk.sim.units.len();
        if self.units.len() < n {
            self.units.resize_with(n, UnitVisual::default);
        }
        self.step_hover_nines(walk);
        let fired: Vec<u32> = walk
            .sim
            .events
            .iter()
            .filter_map(|e| match e {
                core::sim::Event::Shot { unit, .. } => Some(unit.index),
                _ => None,
            })
            .collect();
        for (i, u) in walk.sim.units.iter().enumerate() {
            // Split the borrows by FIELD: the asset lives in `self.assets`,
            // the animator in `self.units`.
            let Some(ai) = self.asset_index(u.team, u.spec.class) else {
                continue;
            };
            let sa = &self.assets[ai];
            let v = &mut self.units[i];
            if v.gen != u.gen {
                *v = UnitVisual {
                    gen: u.gen,
                    ..Default::default()
                };
            }
            if !u.alive {
                v.dead_for += dt;
            }
            if fired.contains(&(i as u32)) {
                v.shot_tick = Some(walk.sim.tick);
            }
            // Seated in a turret that shows him (card VTURG) or on a
            // vehicle that does (card VRIDE): hung at the seat's hardpoint,
            // posed by the seat's clip.
            v.seat_model = None;
            let tick = walk.sim.tick;
            // A full-circle seat's yaw number: BF2 the yaw rate over the
            // turn speed (B2:189542-189546), BF1 the raw stick
            // (B1:137715-137719, T+0x78), which turns the yaw against its
            // sign on the PS2 as the port's `rx` does: -rate / speed.
            let bf2 = self.bf2;
            let turn = |c: SeatClock| if bf2 { c.stick } else { -c.stick };
            let rider = u
                .mount
                .filter(|_| u.alive)
                .and_then(|m| rider_now(&self.walker_rigs, &self.hover_nine, walk, m, self.bf2));
            if let Some(r) = rider {
                let c = SeatClock::advance(
                    &mut v.seat_clock,
                    tick,
                    r.look.0,
                    r.yaw_rate,
                    Mat4::IDENTITY,
                );
                v.seat_model = Some(r.model);
                let Some(pose) = &r.seat.pilot_pose else {
                    continue;
                };
                if let Some(clip) = crate::animator::find_clip(&sa.asset.clips, &pose.clip()) {
                    let nine = pose.nine.then(|| match r.hover {
                        Some(h) => h.pose(),
                        None => tp::nine_pose(
                            -r.look.1,
                            r.look.0,
                            turn(c),
                            r.seat.pitch_limits,
                            r.seat.yaw_limits,
                        ),
                    });
                    v.animator
                        .seat(&sa.asset.clips, &sa.asset.skeleton, clip, nine, dt);
                }
                continue;
            }
            let seated = u
                .mount
                .filter(|_| u.alive)
                .and_then(|m| seat_now(&self.turrets, walk, m, self.bf2));
            let Some(s) = seated else {
                v.seat_clock = None;
                let Some(rig) = &sa.rig else { continue };
                Self::step_on_foot(
                    walk,
                    &self.anim,
                    i,
                    u,
                    sa,
                    rig,
                    v,
                    &fired,
                    dt,
                    census.as_deref_mut(),
                );
                continue;
            };
            let held = s.hang.parent.map_or(Mat4::IDENTITY, |p| s.now[p]);
            let c = SeatClock::advance(&mut v.seat_clock, tick, s.look.0, s.yaw_rate, held);
            let mut now = s.now;
            if let (true, Some(p)) = (s.lags, s.hang.parent) {
                now[p] = c.prev;
            }
            v.seat_model = Some(s.hang.matrix(s.model, &now));
            let Some(pose) = &s.seat.pilot_pose else {
                continue;
            };
            if let Some(clip) = crate::animator::find_clip(&sa.asset.clips, &pose.clip()) {
                let nine = pose.nine.then(|| {
                    tp::nine_pose(
                        -s.look.1,
                        s.look.0,
                        turn(c),
                        s.seat.pitch_limits,
                        s.seat.yaw_limits,
                    )
                });
                v.animator
                    .seat(&sa.asset.clips, &sa.asset.skeleton, clip, nine, dt);
            }
        }
    }

    /// One unit's animator on foot (not in a seat that shows him).
    #[allow(clippy::too_many_arguments)]
    fn step_on_foot(
        walk: &WalkSim,
        anim: &[Vec<ClassAnim>; 2],
        i: usize,
        u: &core::unit::Unit,
        sa: &SoldierAsset,
        rig: &Rig,
        v: &mut UnitVisual,
        fired: &[u32],
        dt: f32,
        census: Option<&mut crate::animator::census::Census>,
    ) {
        // The weapon in hand (channel 1's while it has the soldier) picks
        // the set.
        let held = (u.active_channel == 1)
            .then(|| u.held.get(usize::from(u.cur1?)))
            .flatten();
        let (wc, weapon) = held.map_or((u.weapon_class, &u.weapon), |h| (h.class, &h.state));
        let class = walk.sim.weapon_classes.get(wc);
        let team = &anim[usize::from(u.team.min(1))];
        let anim = team.get(usize::from(u.spec.class).min(team.len().saturating_sub(1)));
        let set = match v.set {
            Some((c, set)) if c == wc => set,
            _ => {
                let bank = anim.zip(class).map_or("rifle", |(a, w)| a.bank_of(&w.name));
                let set = rig.weapon_set(anim.map_or("human", |a| &a.family), bank);
                v.set = Some((wc, set));
                set
            }
        };
        let s = &u.soldier;
        let input = AnimInput {
            stance: s.stance,
            vel: crate::animator::body_vel(s.vel, s.yaw),
            yaw: s.yaw,
            max_speed: u.consts.max_speed,
            root_scale: anim.map_or(1.0, |a| a.root_scale),
            dead: !u.alive,
            set,
            fired: u.mount.is_none() && fired.contains(&(i as u32)),
            reloading: weapon.reloading(),
            reload_time: class.map_or(0.0, |c| c.reload_time),
            // PROVISIONAL and ours (FEEL_NOTES; the PS2's two soldier
            // flags are unread): he fires, holds the trigger (a charge
            // too), or has a target (a bot's; the player's aim magnet).
            alert: u.mount.is_none()
                && u.alive
                && (fired.contains(&(i as u32))
                    || weapon.fire_cmd > 0
                    || u.ai.target.is_some()
                    || u.spec.player_slot.is_some_and(|p| {
                        walk.sim
                            .magnet
                            .get(usize::from(p))
                            .is_some_and(Option::is_some)
                    })),
            // The body faces the aim's yaw; the animator takes the legs'
            // twist off it.
            aim: (0.0, s.pitch),
        };
        v.animator
            .step(rig, &sa.asset.clips, &sa.asset.skeleton, &input, dt);
        if let Some(c) = census {
            if u.alive && u.mount.is_none() {
                c.note(rig, &sa.asset.clips, &v.animator, &input);
                c.note_aim(rig, &sa.asset.skeleton, sa.hp_weapons, &v.animator, &input);
            }
        }
    }

    /// Units to draw this frame: (unit index, asset, model matrix, palette).
    pub(super) fn posed(&self, walk: &WalkSim) -> Vec<(usize, &SoldierAsset, Mat4, Vec<Mat4>)> {
        let mut out = Vec::new();
        for (i, u) in walk.sim.units.iter().enumerate() {
            let Some(sa) = self.asset_for(u.team, u.spec.class) else {
                continue;
            };
            let Some(v) = self.units.get(i) else { continue };
            if v.gen != u.gen || (!u.alive && v.dead_for > CORPSE_SECS) {
                continue;
            }
            if !u.alive && !v.animator.posed() {
                continue;
            }
            // A rider is hidden but in a seat that shows him (cards VTURG,
            // VRIDE), who is drawn in every view, his own too.
            let model = match (u.mount, v.seat_model) {
                (Some(_), Some(m)) => m,
                (Some(_), None) => continue,
                // In the scope view the player's own body is not drawn.
                (None, _) if i == walk.player.index as usize && walk.player_body_hidden() => {
                    continue
                }
                (None, _) => model_of(&u.soldier),
            };
            let pose = v.animator.pose(&sa.asset.skeleton);
            let palette = sa.asset.skeleton.palette(&pose.locals());
            out.push((i, sa, model, palette));
        }
        out
    }

    /// Every unit drawn this frame and its model matrix (a test's and a
    /// census's read of who is drawn where).
    pub fn drawn(&self, walk: &WalkSim) -> Vec<(usize, Mat4)> {
        self.posed(walk)
            .into_iter()
            .map(|(i, _, m, _)| (i, m))
            .collect()
    }

    /// The hand weapon of every drawn unit, at the posed `hp_weapons` bone:
    /// `unit × palette[hp] × bind_global[hp]` (palette = global × inverse bind).
    pub fn weapon_draws(&self, walk: &WalkSim) -> Vec<render::DrawCmd> {
        let mut out = Vec::new();
        for (i, sa, model, palette) in self.posed(walk) {
            let Some(hp) = sa.hp_weapons else { continue };
            let u = &walk.sim.units[i];
            // A seated gunner holds no hand weapon (B1:114261-114266,
            // B2:155344-155345).
            if u.mount.is_some() {
                continue;
            }
            let hand = weapon_in_hand(u, &walk.sim.weapon_classes);
            if hand.hidden {
                continue;
            }
            let Some(weapon) = self.weapon_for(u.team, u.spec.class, hand.slot) else {
                continue;
            };
            let m = model * palette[hp] * sa.asset.skeleton.global_bind[hp];
            weapon.part_draws(m, &mut out);
        }
        out
    }

    /// Every drawn unit's weapon fire point in the world, by unit index:
    /// `hp_weapons` × the weapon in hand's `FirePointName` node (the `hp_weapons`
    /// origin when the weapon model has no such node, §4.1). `None` for a
    /// unit drawn without a model or a hand node. Where the muzzle flash
    /// sits and the drawn bolt starts (`crate::bolt_vis`).
    pub fn fire_points(&self, walk: &WalkSim) -> Vec<Option<Vec3>> {
        let mut out = vec![None; walk.sim.units.len()];
        for (i, sa, model, palette) in self.posed(walk) {
            if let (Some(hp), None) = (sa.hp_weapons, walk.sim.units[i].mount) {
                let u = &walk.sim.units[i];
                let in_hand = weapon_in_hand(u, &walk.sim.weapon_classes);
                let name = walk
                    .sim
                    .weapon_classes
                    .get(in_hand.class)
                    .and_then(|w| w.flash.as_ref())
                    .map_or(core::flash::DEFAULT_FIRE_POINT, |f| f.fire_point.as_str());
                let hand = model * palette[hp] * sa.asset.skeleton.global_bind[hp];
                out[i] = Some(match self.weapon_for(u.team, u.spec.class, in_hand.slot) {
                    Some(w) => fire_point(hand, &w.nodes, name),
                    None => hand.transform_point3(Vec3::ZERO),
                });
            }
        }
        out
    }

    /// The muzzle flashes to draw this frame (see the module doc): one per
    /// drawn unit whose weapon flashes and whose last shot's t is still > 0
    /// on the sim clock.
    pub fn flash_draws(&self, walk: &WalkSim) -> Vec<ParticleDraw> {
        let Some(model) = &self.flash else {
            return Vec::new();
        };
        let now = walk.sim.tick as f64 * f64::from(core::TICK_DT);
        let points = self.fire_points(walk);
        let mut out = Vec::new();
        for (i, u) in walk.sim.units.iter().enumerate() {
            let (Some(v), Some(Some(p))) = (self.units.get(i), points.get(i)) else {
                continue;
            };
            let Some(shot) = v.shot_tick else { continue };
            let Some(look) = walk
                .sim
                .weapon_classes
                .get(weapon_in_hand(u, &walk.sim.weapon_classes).class)
                .and_then(|w| w.flash.as_ref())
                .filter(|f| f.draws() && f.model.as_deref() == Some(model.name.as_str()))
            else {
                continue;
            };
            let t = look.t(shot as f64 * f64::from(core::TICK_DT), now);
            let Some(color) = look.color_at(t) else {
                continue;
            };
            let m = core::flash::matrix(u.aim_frame().0, look.radius, *p);
            out.extend(model.draws(m, color, *p));
        }
        out
    }

    /// Skinned draws for every unit with a model (alive, or a fresh corpse),
    /// each part's specular second pass beside its base.
    /// Soldier models face +Z; sim yaw 0 faces −Z, hence the extra π.
    pub fn draws(&self, walk: &WalkSim) -> Vec<SkinnedDrawCmd> {
        let mut out = Vec::new();
        for (i, sa, model, palette) in self.posed(walk) {
            let u = &walk.sim.units[i];
            self.body_draws(sa, u.team, u.spec.class, model, &palette, &mut out);
        }
        out
    }

    /// The parts of `team`'s class `class` (its skins over body `sa`) at
    /// `model` posed by `palette`, each with its specular second pass.
    fn body_draws(
        &self,
        sa: &SoldierAsset,
        team: u8,
        class: u8,
        model: Mat4,
        palette: &[Mat4],
        out: &mut Vec<SkinnedDrawCmd>,
    ) {
        let row = &self.skins[usize::from(team.min(1))];
        let skin = row
            .get(usize::from(class).min(row.len().saturating_sub(1)))
            .map_or(&[][..], Vec::as_slice);
        for (k, (mesh, tex, layer, spec)) in sa.meshes.iter().enumerate() {
            let base = SkinnedDrawCmd {
                mesh: *mesh,
                texture: skin.get(k).copied().flatten().or(*tex),
                color: [1.0; 4],
                model,
                palette: palette.to_vec(),
                layer: *layer,
                light: render::LightPath::Character,
                // BF2's local lights reach the soldier (card BPNT).
                lit: Some(crate::point_light::soldier_sphere(
                    model.transform_point3(glam::Vec3::ZERO),
                )),
            };
            out.extend(with_spec(base, *spec));
        }
    }

    /// A soldier of `team`'s class `class` standing at `model` with his
    /// primary weapon in hand, outside the sim (the spawn screens' preview
    /// and line-up, card SSELB): `animator` is his own, stepped `dt` on the
    /// standing clip of the primary's set. `None` when the class has no
    /// model. Returns his body's draws and the weapon's.
    pub fn still_draws(
        &self,
        animator: &mut Animator,
        team: u8,
        class: u8,
        model: Mat4,
        dt: f32,
    ) -> Option<(Vec<SkinnedDrawCmd>, Vec<render::DrawCmd>)> {
        let sa = self.asset_for(team, class)?;
        if let Some(rig) = &sa.rig {
            let row = &self.anim[usize::from(team.min(1))];
            let anim = row.get(usize::from(class).min(row.len().saturating_sub(1)));
            let bank = anim
                .and_then(|a| a.banks.first())
                .map_or("rifle", |(_, b)| b.as_str());
            let input = AnimInput {
                stance: core::soldier::Stance::Stand,
                vel: glam::Vec2::ZERO,
                yaw: 0.0,
                max_speed: 1.0,
                root_scale: anim.map_or(1.0, |a| a.root_scale),
                dead: false,
                set: rig.weapon_set(anim.map_or("human", |a| &a.family), bank),
                fired: false,
                reloading: false,
                reload_time: 0.0,
                alert: false,
                aim: (0.0, 0.0),
            };
            animator.step(rig, &sa.asset.clips, &sa.asset.skeleton, &input, dt);
        }
        let palette = sa
            .asset
            .skeleton
            .palette(&animator.pose(&sa.asset.skeleton).locals());
        let mut body = Vec::new();
        self.body_draws(sa, team, class, model, &palette, &mut body);
        let mut gun = Vec::new();
        let weapon = self
            .class_draw(team, class)
            .weapon
            .and_then(|w| self.weapons.get(w));
        if let (Some(hp), Some(w)) = (sa.hp_weapons, weapon) {
            w.part_draws(
                model * palette[hp] * sa.asset.skeleton.global_bind[hp],
                &mut gun,
            );
        }
        Some((body, gun))
    }
}
